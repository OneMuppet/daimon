//! The cognitive cycle — where a Daimon actually lives.
//!
//! Every tick the world hands the mind a [`Percept`] and the mind hands back
//! one [`Action`] and one [`Thought`]. In between runs a fixed seven-step
//! cycle, the spine of the whole architecture:
//!
//! ```text
//!   percept ─▶ 1 perceive ─▶ 2 appraise ─▶ 3 reflex? ─▶ 4 decide ─▶ 5 plan ─▶ 6 act ─▶ action
//!                  │             │             │            │           │         │
//!              world model   drives +      System-1     System-1/2   re-plan   bounded
//!              + memory      surprise      override     arbitration  if stale   action
//!                                                                                  │
//!                                              7 reflect (every N ticks) ◀─────────┘
//! ```
//!
//! The cycle is the BDI loop (Bratman; Rao & Georgeff) wearing a dual-process
//! coat (Kahneman; Booch et al., *Thinking Fast and Slow in AI*): beliefs from
//! perception, desires from drives, intentions committed as plans — but with an
//! explicit, rate-limited escalation from cheap reflexive choice to expensive
//! deliberation. That escalation policy is the load-bearing idea. It is what
//! lets a Daimon think hard *when it matters* and stay cheap the other 95% of
//! the time.

use crate::affect::Affect;
use crate::anticipation::Anticipation;
use crate::learn::LearningProgress;
use crate::deliberate::{Deliberator, DeliberationContext, HeuristicDeliberator};
use crate::persona::Persona;
use crate::planner::{plan_for_with, region_of, Danger, Herd};
use crate::imagine::ForwardModel;
use crate::overlay::{Overlay, N_IN};
use crate::praxis::Praxis;
use crate::project::{Project, ProjectKind};
use daimon_core::Dir;
use crate::theory_of_mind::TheoryOfMind;
use crate::thought::{Process, Thought};
use daimon_core::{
    Action, Drive, DriveSystem, Episode, EntityId, EntityKind, Goal, GoalKind, Memory, Percept,
    Plan, Pos, Rng, WorldEvent, WorldModel,
};
use serde::{Deserialize, Serialize};

/// Tunables for the escalation policy and housekeeping cadence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MindConfig {
    /// Surprise above this forces System-2 deliberation, cooldown or not.
    pub surprise_threshold: f32,
    /// Minimum ticks between *routine* deliberations (budget for the slow path).
    pub deliberation_cooldown: u64,
    /// Two drives whose pressures are within this margin count as an ambiguous
    /// choice and warrant deliberation.
    pub tie_margin: f32,
    /// How often the reflection pass runs.
    pub reflect_interval: u64,
    /// Re-plan if the current plan is older than this many ticks.
    pub plan_staleness: u64,
    /// Whether the agent has the build affordance: it *may* wall itself in for
    /// shelter when exposed under threat. Off by default — so worlds without
    /// building draw no shelter logic and stay bit-identical. Nothing here tells
    /// it to build a hut; the structure emerges from utility planning.
    #[serde(default)]
    pub can_build: bool,
    /// Whether the agent is mortal (health no longer floored; it can die for good)
    /// and feels a fear of death from its health trajectory. Off by default.
    #[serde(default)]
    pub can_die: bool,
    /// Whether the agent grieves the death of a bonded peer. Off by default.
    #[serde(default)]
    pub can_grieve: bool,
    /// Whether the agent provisions for winter in an open world (gather surplus →
    /// store in the granary → draw it down through the cold). Off by default — so a
    /// world without provisioning adopts no Provision goal and stays bit-identical.
    #[serde(default)]
    pub can_provision: bool,
}

impl Default for MindConfig {
    fn default() -> Self {
        Self {
            surprise_threshold: 0.55,
            deliberation_cooldown: 8,
            tie_margin: 0.25,
            reflect_interval: 25,
            plan_staleness: 6,
            can_build: false,
            can_die: false,
            can_grieve: false,
            can_provision: false,
        }
    }
}

/// Running tallies, surfaced as a "life in numbers" at the end of a run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metrics {
    pub ticks: u64,
    pub deliberations: u64,
    pub reflexes: u64,
    pub reflections: u64,
    pub discoveries: u64,
    pub meals: u64,
    pub conversations: u64,
    pub near_death_escapes: u64,
    /// Facts learned from *others* (information received via dialogue).
    pub facts_learned: u64,
    /// Whether the agent's long-horizon project has been completed.
    pub project_completed: bool,
    /// Ticks spent acting toward the project (persistence).
    pub project_ticks: u64,
    /// Times the agent acted on a *self-invented* goal (learned affordance).
    pub praxis_invented: u64,
}

/// A complete Daimon mind. Serialisable end-to-end (a life is portable data);
/// the System-2 deliberator is stateless and reattached on load.
#[derive(Serialize, Deserialize)]
pub struct Mind {
    pub persona: Persona,
    rng: Rng,
    world: WorldModel,
    memory: Memory,
    drives: DriveSystem,
    social: TheoryOfMind,
    plan: Option<Plan>,
    /// The goal the agent is currently *committed* to. An intention persists
    /// (Bratman) until it is satisfied or another goal clearly outweighs it —
    /// without this, a mind with two near-equal needs dithers between them and
    /// achieves neither. This field is that commitment.
    committed: Option<Goal>,
    /// True when this tick's decision was a *held* commitment (for narration).
    held: bool,
    /// True when this tick's decision came from a self-invented (praxis) goal.
    invented_now: bool,
    /// Learned map of regions where things have gone badly — the agent avoids
    /// them (built from remembered harm, used by the planner).
    #[serde(with = "daimon_core::serdeutil::vecmap")]
    danger: Danger,
    /// A long-horizon project pursued in the gaps between urgent needs.
    project: Option<Project>,
    /// The autonomy frontier: self-invented concepts + learned affordances, from
    /// which the agent can invent goals nobody coded.
    praxis: Praxis,
    /// Learned forward model of the agent's own dynamics + empowerment source.
    forward: ForwardModel,
    /// Last (position, move) taken, so next tick we can learn the transition.
    last_step: Option<(Pos, Dir)>,
    /// Bounds learned from experience (max position seen), for empowerment.
    seen_bounds: (i32, i32),
    /// Intrinsic empowerment drive on (seek open ground) — ablatable for tests.
    empowered: bool,
    /// Memory consolidation ("sleep" replay) on — ablatable for tests.
    consolidate: bool,
    /// Imagination: plan around known obstacles via the forward model — ablatable.
    imagine: bool,
    /// While detouring around an obstacle, the target we're routing to (so we
    /// commit to the planned path instead of snapping back to the greedy step).
    detour_target: Option<Pos>,
    /// Meta-motivation: revise own drive weights from outcomes — ablatable.
    metamotivation: bool,
    /// Quantum-cognitive deliberation: decide by superposition + Born-rule
    /// collapse over drives (order/interference effects). Off by default.
    quantum: bool,
    /// Goal-directed foraging by drive-reduction-rate under survival risk (DRR):
    /// pick the resource that buys the most need-relief per risk-adjusted travel
    /// time, not the nearest one. Off by default (preserves greedy behaviour).
    #[serde(default)]
    forage_drr: bool,
    /// Commons-aware foraging: avoid resources other agents have already claimed,
    /// yielding to the more-urgent and dispersing across tiles (decentralised,
    /// no central control). Off by default.
    #[serde(default)]
    social_forage: bool,
    /// Cumulative cultural transmission: learn forms' affordances from successful
    /// peers (not only from direct contact), so knowledge accumulates across the
    /// population. Own experience refines/corrects what is copied. Off by default.
    #[serde(default)]
    cultural: bool,
    /// Curiosity driven by *learning progress* (competence gain) rather than raw
    /// novelty — drawn to the learnable, not lured by unlearnable noise
    /// (Oudeyer–Kaplan IAC). Off by default.
    #[serde(default)]
    lp_curiosity: bool,
    /// Stigmergy: deposit pheromone on productive routes and follow worn trails
    /// during exploration (Grassé/Dorigo) — emergent shared paths, no central
    /// control. The world reads this flag to deposit/route. Off by default.
    #[serde(default)]
    stigmergy: bool,
    /// Affect modulation: let the felt emotion bias action readiness (Frijda) —
    /// fear sharpens caution, contentment loosens curiosity. Off by default
    /// (affect is otherwise tracked read-only).
    #[serde(default)]
    affect_mod: bool,
    /// Can-fight: the agent is *given the tool* to confront a threat instead of
    /// only fleeing. Off by default. Nothing tells it to rally — it learns whether
    /// confronting works (`confront_value`) and may choose it; any collective
    /// defence must emerge.
    #[serde(default)]
    can_fight: bool,
    /// Learned value of confronting a threat (EMA of outcomes: driven off → up,
    /// hurt while confronting → down). Starts at 0 — no belief either way.
    #[serde(default)]
    confront_value: f32,
    /// Whether last tick's action was a strike (for crediting the outcome).
    #[serde(default)]
    last_struck: bool,
    /// PREDATOR-AWARE COORDINATION (selfish-herd / dispersal-evasion). When on, a
    /// threatened mind's evasive step composes fleeing the predator with an
    /// anti-isolation pull toward its local prey group's centroid / nearest ally
    /// (Hamilton 1971; risk dilution), so it is not the lone straggler an
    /// isolated-target predator targets. Off by default — the flee path then computes
    /// exactly the straight-away step it always did, keeping seeded worlds with it off
    /// byte-identical. No new RNG is drawn either way.
    #[serde(default)]
    herd_evasion: bool,
    /// How heavily the herd-cohesion term weighs against fleeing, in `[0,1]`. Only
    /// consulted when `herd_evasion` is on.
    #[serde(default)]
    herd_cohesion: f32,
    /// Other agents' current foraging claims `(resource_pos, their_urgency)`,
    /// supplied by the world each tick. Transient — never serialised.
    #[serde(skip, default)]
    contention: Vec<(Pos, f32)>,
    /// `Option` so the slow path can be `take()`n out, sidestepping the borrow
    /// checker while it reads the rest of the mind as context. Stateless, so it
    /// is skipped by serialisation and reattached (offline) on load.
    #[serde(skip, default = "default_deliberator")]
    deliberator: Option<Box<dyn Deliberator>>,
    anticipation: Anticipation,
    /// Learning progress over forward-model prediction error — competence gain as
    /// an intrinsic signal, and the basis of culture's adoption gate.
    #[serde(default)]
    lprog: LearningProgress,
    /// Felt emotional state (valence/arousal), appraised each tick — how the agent
    /// *feels* about its situation, distinct from what it needs.
    #[serde(default)]
    affect: Affect,
    /// MORTALITY SALIENCE (TMT): the felt dread of one's own decline. Driven by the
    /// health *trajectory* (a slow-bleeding body dreads more than a stable one),
    /// recent harm, and witnessed deaths — not just present injury. Decays when the
    /// body recovers. In `[0,1]`. Zero and inert unless `can_die` is on.
    #[serde(default)]
    mortality: f32,
    /// Health last tick, so the appraisal can read the *trajectory* (declining vs
    /// recovering), which is what mortality salience keys on.
    #[serde(default)]
    prev_health: f32,
    /// GRIEF: the open wound of a lost bond. Set when a bonded peer dies, scaled by
    /// the bond strength at that moment; lowers valence and drives Dual-Process
    /// oscillation (Mourn vs restoration). Decays over ticks — faster when other
    /// bonded living friends are near (social support). In `[0,1]`. Inert unless
    /// `can_grieve` is on. A stranger's death adds ~nothing here — the asymmetry.
    #[serde(default)]
    grief: f32,
    /// The peer whose death this grief is for (the named dead friend the mind
    /// reminisces about). Kept alongside the continuing bond in theory-of-mind.
    #[serde(default)]
    grieving_for: Option<EntityId>,
    /// System-2: the learned, evolved-plastic neural overlay. Inert (zero bias,
    /// no learning) when disabled, so the instinct stays byte-identical.
    #[serde(default)]
    overlay: Overlay,
    /// The mind's well-being last tick — the baseline for the overlay's intrinsic
    /// reward signal (Δ well-being). Transient learning bookkeeping.
    #[serde(default)]
    prev_wellbeing: f32,
    cfg: MindConfig,
    last_deliberation: Option<u64>,
    metrics: Metrics,
}

fn default_deliberator() -> Option<Box<dyn Deliberator>> {
    Some(Box::new(HeuristicDeliberator))
}

/// How much stronger a rival drive must be, in pressure units, before the agent
/// abandons its current intention for it. Hysteresis = follow-through.
const COMMIT_MARGIN: f32 = 0.3;
/// Below this urgency, the drive behind a commitment is considered satisfied and
/// the intention is released.
const SATISFIED_BELOW: f32 = 0.28;
/// A drive this urgent interrupts *any* standing intention — you don't finish
/// foraging while you're dying of thirst, however committed you were.
const CRITICAL: f32 = 0.8;
/// DRR risk sensitivity: how sharply learned hazard along a route discounts a
/// destination's value. exp(-κ·exposure) — κ≈0.7 ⇒ one unit of exposure halves it
/// (Mangel & Clark 1986: value scales by survival probability, never subtracted).
const DRR_KAPPA: f32 = 0.7;
/// The bond strength (disposition) above which a peer counts as a *bonded friend*
/// whose death triggers real grief. New acquaintances start at 0.15 and a standing
/// friendship is recorded at 0.4 — so 0.3 marks "more than a passing stranger,
/// genuinely close", which is exactly the line grief science draws (attachment, not
/// mere acquaintance). Below it, a death is noted but not mourned — the asymmetry.
const BOND_THRESHOLD: f32 = 0.3;
/// Below this grief intensity, the wound is considered healed and mourning ends.
const GRIEF_RESOLVED_BELOW: f32 = 0.08;

impl Mind {
    /// A Daimon with the default offline deliberator and config.
    pub fn new(persona: Persona, seed: u64) -> Self {
        Self::with(persona, seed, Box::new(HeuristicDeliberator), MindConfig::default())
    }

    /// A Daimon with a custom System-2 (e.g. an LLM-backed deliberator) and
    /// config.
    pub fn with(
        persona: Persona,
        seed: u64,
        deliberator: Box<dyn Deliberator>,
        cfg: MindConfig,
    ) -> Self {
        let mut memory = Memory::default();
        // seed the self-concept into semantic memory: the agent "knows itself".
        memory.learn("creed", &persona.creed, 1.0, 0);
        // adopt a life project that suits the personality.
        let project = Some(if persona.curiosity >= 0.8 {
            Project::new(ProjectKind::ExploreEverything, 4, 0)
        } else if persona.sociability >= 0.8 {
            Project::new(ProjectKind::Companionship, 40, 0)
        } else {
            Project::new(ProjectKind::Provision, 6, 0)
        });
        Self {
            persona,
            rng: Rng::new(seed),
            world: WorldModel::default(),
            memory,
            drives: DriveSystem::default(),
            social: TheoryOfMind::default(),
            plan: None,
            committed: None,
            held: false,
            invented_now: false,
            danger: Danger::new(),
            project,
            praxis: Praxis::default(),
            forward: ForwardModel::default(),
            last_step: None,
            seen_bounds: (8, 8),
            empowered: true,
            consolidate: true,
            imagine: true,
            detour_target: None,
            metamotivation: true,
            quantum: false,
            forage_drr: false,
            social_forage: false,
            cultural: false,
            lp_curiosity: false,
            stigmergy: false,
            affect_mod: false,
            can_fight: false,
            confront_value: 0.0,
            last_struck: false,
            herd_evasion: false,
            herd_cohesion: 0.0,
            contention: Vec::new(),
            deliberator: Some(deliberator),
            anticipation: Anticipation::default(),
            lprog: LearningProgress::default(),
            affect: Affect::default(),
            mortality: 0.0,
            prev_health: 1.0,
            grief: 0.0,
            grieving_for: None,
            overlay: Overlay::disabled(),
            prev_wellbeing: 0.0,
            cfg,
            last_deliberation: None,
            metrics: Metrics::default(),
        }
    }

    // -- introspection accessors (used by the demo / tests) -----------------
    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }
    pub fn memory(&self) -> &Memory {
        &self.memory
    }
    /// Mutable memory — used to *seed* knowledge (e.g. a starting belief) and by
    /// the embodiment to inject learned places.
    pub fn memory_mut(&mut self) -> &mut Memory {
        &mut self.memory
    }
    pub fn drives(&self) -> &DriveSystem {
        &self.drives
    }
    pub fn world(&self) -> &WorldModel {
        &self.world
    }
    pub fn social(&self) -> &TheoryOfMind {
        &self.social
    }
    pub fn anticipation(&self) -> &Anticipation {
        &self.anticipation
    }
    pub fn surprise(&self) -> f32 {
        self.anticipation.last()
    }
    /// The agent's felt emotional state (valence/arousal + a mood name).
    pub fn affect(&self) -> Affect {
        self.affect
    }
    /// Whether the agent is *this tick* pursuing a goal it invented for itself
    /// from a learned affordance (Praxis goal genesis) — acting on the unforeseen.
    pub fn acting_on_invented(&self) -> bool {
        self.invented_now
    }
    /// Which cognitive faculties are currently active — for surfacing the
    /// architecture in an inspector. (name, on).
    pub fn faculty_flags(&self) -> [(&'static str, bool); 9] {
        [
            ("anticipation", self.drives.foresight() > 0.0),
            ("empowerment", self.empowered),
            ("imagination", self.imagine),
            ("consolidation", self.consolidate),
            ("meta-motivation", self.metamotivation),
            ("commons", self.social_forage),
            ("culture", self.cultural),
            ("stigmergy", self.stigmergy),
            ("quantum", self.quantum),
        ]
    }
    /// The learned forward model (dynamics + empowerment).
    pub fn forward(&self) -> &ForwardModel {
        &self.forward
    }
    /// Learning progress: the rate at which the agent's predictions are improving
    /// (Oudeyer–Kaplan competence gain). Positive while learning the world,
    /// decaying toward 0 as the dynamics are mastered.
    pub fn learning_progress(&self) -> f32 {
        self.lprog.progress()
    }
    /// Mean forward-model prediction error over the recent window (lower = more
    /// competent at predicting the world).
    pub fn prediction_error(&self) -> f32 {
        self.lprog.mean_error()
    }
    /// Toggle the intrinsic empowerment drive (for ablation experiments).
    pub fn set_empowerment(&mut self, on: bool) {
        self.empowered = on;
    }
    /// Toggle memory consolidation / replay (for ablation experiments).
    pub fn set_consolidation(&mut self, on: bool) {
        self.consolidate = on;
    }
    /// Toggle imagination / forward-model path planning (for ablation).
    pub fn set_imagination(&mut self, on: bool) {
        self.imagine = on;
    }
    /// Toggle meta-motivation / self-revised drive weights (for ablation).
    pub fn set_metamotivation(&mut self, on: bool) {
        self.metamotivation = on;
    }
    /// Toggle quantum-cognitive deliberation (Born-rule choice over drives).
    pub fn set_quantum(&mut self, on: bool) {
        self.quantum = on;
    }
    /// Set anticipatory-homeostasis lead time (ticks). With `> 0` the agent weighs
    /// physiological needs as if they had crept forward this long, foraging ahead
    /// of crisis — a computable step toward active inference. `0` = reactive.
    pub fn set_foresight(&mut self, ticks: f32) {
        self.drives.set_foresight(ticks);
    }
    /// Toggle goal-directed foraging by drive-reduction-rate under survival risk.
    pub fn set_forage_drr(&mut self, on: bool) {
        self.forage_drr = on;
    }
    /// Toggle commons-aware (contention-yielding, dispersing) foraging.
    pub fn set_social_forage(&mut self, on: bool) {
        self.social_forage = on;
    }
    /// Toggle cumulative cultural transmission (learning affordances from peers).
    pub fn set_cultural(&mut self, on: bool) {
        self.cultural = on;
    }
    /// Toggle learning-progress curiosity (drawn to the learnable, not raw novelty).
    pub fn set_lp_curiosity(&mut self, on: bool) {
        self.lp_curiosity = on;
    }
    /// Toggle stigmergy (deposit/follow environmental pheromone trails).
    pub fn set_stigmergy(&mut self, on: bool) {
        self.stigmergy = on;
    }
    /// Toggle affect modulation (emotion biases caution/curiosity).
    pub fn set_affect_mod(&mut self, on: bool) {
        self.affect_mod = on;
    }
    /// Give the agent the *option* to confront threats (not the instruction to).
    pub fn set_can_fight(&mut self, on: bool) {
        self.can_fight = on;
    }
    /// Enable predator-aware coordination (selfish-herd / dispersal-evasion) with the
    /// given cohesion strength. Off by default — when off the flee path is byte-
    /// identical to the incumbent straight-away flee. Nothing here tells the mind to
    /// flock; the anti-isolation bias only ever fires while a predator is perceived.
    pub fn set_herd_evasion(&mut self, on: bool, cohesion: f32) {
        self.herd_evasion = on;
        self.herd_cohesion = cohesion.clamp(0.0, 1.0);
    }
    /// Whether predator-aware coordination is active (for inspection / the AC).
    pub fn herd_evasion(&self) -> bool {
        self.herd_evasion
    }
    /// Give the agent the *option* to build shelter (not the instruction to). With
    /// it on, an exposed-and-threatened agent may adopt a Shelter goal and wall
    /// itself in; whether and what it builds emerges from its own utility planning.
    pub fn set_can_build(&mut self, on: bool) {
        self.cfg.can_build = on;
    }
    /// Make the agent mortal (health no longer floored; it can die for good) and
    /// give it a fear of death from its health trajectory. Off by default.
    pub fn set_can_die(&mut self, on: bool) {
        self.cfg.can_die = on;
    }
    /// Whether this mind is mortal — the world reads this to know whether to remove
    /// the health floor and let the body actually die.
    pub fn can_die(&self) -> bool {
        self.cfg.can_die
    }
    /// Let the agent grieve the death of a bonded peer. Off by default.
    pub fn set_can_grieve(&mut self, on: bool) {
        self.cfg.can_grieve = on;
    }
    /// Whether this mind grieves (for inspection).
    pub fn can_grieve(&self) -> bool {
        self.cfg.can_grieve
    }
    /// Give the agent the *option* to provision for winter (not the instruction to).
    /// With it on (and the world an open world), a mind whose needs are met may adopt
    /// a Provision goal and stock the granary; whether and when emerges from its
    /// Mastery + foresight appraisal. Off by default.
    pub fn set_can_provision(&mut self, on: bool) {
        self.cfg.can_provision = on;
    }
    /// Whether this mind provisions (for inspection / the harness).
    pub fn can_provision(&self) -> bool {
        self.cfg.can_provision
    }
    /// Install the System-2 learned overlay (called from `Genome::express`). When
    /// `enabled` is false the overlay is inert (zero bias, no learning), so the
    /// instinct — and any seeded run with the gene off — is byte-identical.
    pub fn install_overlay(&mut self, enabled: bool, seed: u64, lr: f32, modulation: f32) {
        self.overlay = if enabled {
            // per-agent init from the mind seed → diversity across a population;
            // the genome evolves the *learning machinery* (lr, modulation), not
            // the weights (Baldwin). Deterministic given the seed.
            Overlay::seeded(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xA11CE, lr, modulation)
        } else {
            Overlay::disabled()
        };
    }
    /// Whether the learned overlay is active (for inspection / the harness).
    pub fn overlay_enabled(&self) -> bool {
        self.overlay.enabled()
    }
    /// Sum of |overlay weights| — lets the harness confirm in-life learning moved
    /// the network (and that it stays bounded).
    pub fn overlay_weight_magnitude(&self) -> f32 {
        self.overlay.weight_magnitude()
    }
    /// The mind's current **well-being** in `[0,1]`: bodily satisfaction (low
    /// drives), health, and a felt-good (valence) term, dimmed by grief. This is
    /// the intrinsic signal the overlay's reward is the *change* of — the mind
    /// learns to bias decisions toward what improves its own life.
    fn wellbeing(&self) -> f32 {
        // mean satisfaction across the 6 drives (1 - level), so a sated mind scores high.
        let mut sat = 0.0f32;
        for d in Drive::ALL {
            sat += 1.0 - self.drives.level(d);
        }
        sat /= Drive::ALL.len() as f32;
        let health = self.world.me().map(|m| m.health).unwrap_or(1.0);
        let valence01 = (self.affect.valence + 1.0) * 0.5; // [-1,1] → [0,1]
        let wb = 0.5 * sat + 0.35 * health + 0.15 * valence01;
        (wb - 0.4 * self.grief).clamp(0.0, 1.0)
    }
    /// Assemble the overlay's input feature vector from values the appraisal has
    /// already computed this tick. Order is fixed and mirrored in `overlay.rs`.
    fn overlay_features(&self) -> [f32; N_IN] {
        let me = self.world.me();
        let (health, enclosure, winter, carrying) = me
            .map(|m| (m.health, m.enclosure, m.winter_in, m.carrying.min(1.0)))
            .unwrap_or((1.0, 0.0, 0.0, 0.0));
        let threat = me
            .and_then(|m| self.world.nearest_threat(m.pos).map(|t| (m.pos, t)))
            .map(|(p, t)| {
                let d = t.entity.pos.manhattan(p) as f32;
                ((8.0 - d) / 8.0).clamp(0.0, 1.0) // 1 = right on top of me, 0 = far/none
            })
            .unwrap_or(0.0);
        [
            self.drives.level(Drive::Hunger),
            self.drives.level(Drive::Thirst),
            self.drives.level(Drive::Survival),
            self.drives.level(Drive::Curiosity),
            self.drives.level(Drive::Social),
            self.drives.level(Drive::Mastery),
            self.affect.valence,
            self.affect.arousal,
            health,
            threat,
            enclosure,
            self.mortality,
            self.grief,
            winter,
            carrying,
            1.0, // bias unit
        ]
    }
    /// Pick the dominant drive, with the learned overlay's bounded bias added to
    /// each drive's pressure before the arg-max. The chosen drive's *true*
    /// pressure is returned (the overlay steers selection, not the urgency the
    /// rest of the cascade reasons about). Disabled overlay ⇒ `drives.dominant()`.
    fn dominant_biased(&mut self) -> (Drive, f32) {
        if !self.overlay.enabled() {
            return self.drives.dominant();
        }
        let feats = self.overlay_features();
        let bias = self.overlay.bias(&feats);
        let mut best = Drive::ALL[0];
        let mut best_score = f32::MIN;
        for (i, d) in Drive::ALL.into_iter().enumerate() {
            let score = self.drives.pressure(d) + bias[i];
            if score > best_score {
                best_score = score;
                best = d;
            }
        }
        (best, self.drives.pressure(best))
    }
    /// The felt dread of one's own mortality (mortality salience, TMT), in `[0,1]`.
    /// Rises with a declining health trajectory and witnessed death; ~0 for a
    /// thriving, immortal, or stable agent. For inspection and the ACs.
    pub fn mortality_salience(&self) -> f32 {
        self.mortality
    }
    /// The current intensity of grief over a lost friend, in `[0,1]`. ~0 unless a
    /// *bonded* peer has died (and decaying toward 0 as the mind heals).
    pub fn grief(&self) -> f32 {
        self.grief
    }
    /// The agent's learned value of confronting (for inspection/metrics).
    pub fn confront_value(&self) -> f32 {
        self.confront_value
    }
    /// Whether stigmergy is on — the world checks this to deposit/route, and to
    /// skip all stigmergy work (and RNG) when off, keeping non-stigmergic runs
    /// bit-identical.
    pub fn is_stigmergic(&self) -> bool {
        self.stigmergy
    }
    /// Whether this mind learns culturally (so the world can skip the transmission
    /// work — and crucially the RNG draw — entirely when it is off, keeping
    /// non-cultural runs bit-identical).
    pub fn is_cultural(&self) -> bool {
        self.cultural
    }
    /// The affordance this mind is most worth teaching a peer (if any).
    pub fn teachable_concept(&self) -> Option<crate::praxis::Concept> {
        self.praxis.teachable().cloned()
    }
    /// Learn a form's affordance from a peer — only if culturally enabled. Returns
    /// true if the knowledge was newly adopted (for narration/metrics).
    pub fn adopt_concept(&mut self, c: &crate::praxis::Concept) -> bool {
        self.cultural && self.praxis.adopt(c)
    }
    /// Receive other agents' foraging claims `(resource_pos, their_urgency)` for
    /// this tick (the world supplies these; transient).
    pub fn set_contention(&mut self, claims: Vec<(Pos, f32)>) {
        self.contention = claims;
    }
    /// This agent's current foraging claim — the resource it is committed to and
    /// how urgently it needs that resource — so the world can let peers see it.
    pub fn forage_claim(&self) -> Option<(Pos, f32)> {
        let c = self.committed.as_ref()?;
        let urgency = match c.kind {
            GoalKind::Forage => self.drives.level(Drive::Hunger),
            GoalKind::Hydrate => self.drives.level(Drive::Thirst),
            _ => return None,
        };
        self.goal_target_pos(&c.kind).map(|p| (p, urgency))
    }

    /// Decide a drive by quantum cognition: prepare a superposition over drives
    /// weighted by current pressure, apply the given sequence of *considerations*
    /// (non-commuting unitary rotations — order matters), then collapse via a
    /// Born-rule measurement. Exposed for experiments; also used in-cycle when
    /// quantum mode is on. Advances the agent's RNG (the measurement).
    pub fn quantum_choice(&mut self, order: &[Drive]) -> Drive {
        let weights: Vec<f64> = Drive::ALL
            .iter()
            .map(|&d| self.drives.pressure(d) as f64 + 0.05)
            .collect();
        // distinct, affect-tinged phases so considerations actually interfere.
        let s = self.anticipation.last() as f64;
        let phases: Vec<f64> = (0..Drive::ALL.len()).map(|i| i as f64 * 0.7 + s).collect();
        let mut q = crate::qcog::QMind::prepare(&weights, &phases);
        for w in order.windows(2) {
            q.consider(drive_index(w[0]), drive_index(w[1]), 0.8);
        }
        let u = self.rng.next_f32() as f64;
        let idx = q.measure(u);
        Drive::ALL[idx.min(Drive::ALL.len() - 1)]
    }
    /// Serialise this whole mind to JSON — a life as portable data.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
    /// Reload a mind from JSON (the offline deliberator is reattached).
    pub fn from_json(s: &str) -> Option<Self> {
        serde_json::from_str(s).ok()
    }
    /// The drive *implied by what the agent is visibly doing* (its committed
    /// goal kind) — the fair ground truth an onlooker could hope to read. Note
    /// this is the goal's observable meaning, not its inner origin (a project
    /// forage is motivated by Mastery but *looks* like Hunger).
    /// Where the agent currently intends to go — the world position its committed
    /// goal resolves to (a resource, a peer, a remembered place). For drawing the
    /// line of intent: planning and goal-direction made visible.
    pub fn intent_target(&self) -> Option<Pos> {
        let g = self.committed.as_ref()?;
        self.goal_target_pos(&g.kind)
    }

    pub fn intent_drive(&self) -> Option<Drive> {
        self.committed.as_ref().map(|g| goal_drive(&g.kind))
    }

    /// Run one full cognitive cycle.
    pub fn cycle(&mut self, p: &Percept) -> Thought {
        self.metrics.ticks += 1;

        // 1) PERCEIVE — fold the percept into beliefs; note who/what is around;
        //    and let praxis cluster what's seen + learn affordances from contact.
        let newly = self.world.integrate(p);
        self.observe_surroundings(p);
        self.praxis.observe(&p.visible, p.me.pos, p.me);

        // forward model: the result of last tick's move is now visible — score the
        // prediction (learning progress), then learn it.
        if let Some((from, dir)) = self.last_step.take() {
            // error = 0 if the model already predicted this outcome, 1 otherwise
            // (incl. not-yet-known). Falls toward 0 as the dynamics are learned.
            let err = match self.forward.predict(from, dir) {
                Some(pred) if pred == p.me.pos => 0.0,
                _ => 1.0,
            };
            self.lprog.record(err);
            self.forward.learn(from, dir, p.me.pos);
        }
        // grow the learned world bounds from experience (for empowerment).
        self.seen_bounds.0 = self.seen_bounds.0.max(p.me.pos.x + 1);
        self.seen_bounds.1 = self.seen_bounds.1.max(p.me.pos.y + 1);
        for e in &p.visible {
            self.seen_bounds.0 = self.seen_bounds.0.max(e.pos.x + 1);
            self.seen_bounds.1 = self.seen_bounds.1.max(e.pos.y + 1);
        }

        // 2) APPRAISE — update drives from body + world; measure surprise.
        let surprise = self.appraise(p, newly.len());
        self.record_events(&p.events);
        // GRIEF DECAYS over time — and faster when bonded living friends are near
        // (social support speeds bereavement's resolution). A no-op when not
        // grieving, and entirely skipped (no work) when grief is off.
        self.decay_grief(p.me.pos);

        // SYSTEM 2 — reward the learned overlay for the OUTCOME of last tick's
        // biased decision: the change in the mind's own well-being (an intrinsic,
        // deterministic signal — no external supervision). A no-op when the
        // overlay is disabled, and on the first tick (nothing biased yet).
        if self.overlay.enabled() {
            let wb = self.wellbeing();
            self.overlay.learn(wb - self.prev_wellbeing);
            self.prev_wellbeing = wb;
        }

        // 3) REFLEX — a close predator pre-empts everything (System 1, wired).
        if let Some(t) = self.reflex_check(p) {
            return t;
        }

        // 4) DECIDE — fast arbitration, or escalate to the slow deliberator.
        let (goal, process, rationale) = self.decide(surprise);

        // 5) PLAN — (re)build a short plan if the current one is unfit.
        self.ensure_plan(&goal);

        // 6) ACT — take the next bounded action.
        let action = self.next_action();
        // remember this move so we can learn its effect next tick.
        if let Action::Move(d) = action {
            self.last_step = Some((p.me.pos, d));
        }
        self.last_struck = matches!(action, Action::Strike(_));
        self.update_project(&action, &p.events);

        // 7) REFLECT — periodically distil experience into durable knowledge.
        if self.metrics.ticks.is_multiple_of(self.cfg.reflect_interval) {
            self.reflect();
        }

        let _ = rationale;
        let inner = self.narrate(&goal, process, surprise);
        Thought {
            tick: p.tick,
            process,
            dominant_drive: self.drives.dominant().0,
            goal: goal.kind,
            action,
            inner,
        }
    }

    // ----- step 1/2 helpers ------------------------------------------------

    /// Remember where notable things are and model any agents in view.
    fn observe_surroundings(&mut self, p: &Percept) {
        for e in &p.visible {
            match e.kind {
                EntityKind::Food | EntityKind::Water | EntityKind::Curio => {
                    self.memory.note_place(e.id, e.pos, &e.label, e.kind);
                }
                EntityKind::Agent => {
                    self.memory.note_place(e.id, e.pos, &e.label, e.kind);
                    // theory of mind: read where they *moved*, then infer intent.
                    let prev = self.social.last_pos(e.id);
                    self.social.observe(e, p.tick);
                    // only trust the read when they're close enough that we share
                    // their view of the surroundings.
                    // read intent from the step they took, confirmed over two
                    // glances (a single step toward the river while wandering
                    // shouldn't convince us they're thirsty).
                    if e.pos.manhattan(p.me.pos) <= 4 {
                        if let Some(drive) = infer_drive_from_movement(e, prev, &p.visible) {
                            self.social.consider_drive(e.id, drive, p.tick);
                        }
                    }
                }
                EntityKind::Predator => {}
            }
        }
        // associative memory: everything seen together this tick gets linked, so
        // the agent later associates (say) the stalker with the place it haunts.
        let mut concepts: Vec<u32> = Vec::with_capacity(p.visible.len() * 2);
        for e in &p.visible {
            concepts.push(e.id.0);
            concepts.push(kind_concept(e.kind));
        }
        if concepts.len() >= 2 {
            self.memory.associate(&concepts, p.tick);
        }
    }

    /// Update the drive system from interoception and the world, and return a
    /// normalised surprise signal — the trigger for curiosity and for thinking.
    fn appraise(&mut self, p: &Percept, novelty: usize) -> f32 {
        let me = p.me;

        // physiological drives are read straight off the body.
        self.drives.set(Drive::Hunger, 1.0 - me.energy);
        self.drives.set(Drive::Thirst, 1.0 - me.hydration);

        // survival blends injury with predator proximity, tempered by boldness.
        let mut survival = 1.0 - me.health;
        if let Some(t) = self.world.nearest_threat(me.pos) {
            let d = t.entity.pos.manhattan(me.pos) as f32;
            let prox = ((6.0 - d) / 6.0).clamp(0.0, 1.0);
            // ENCLOSURE folds into the appraisal: being walled-in dampens the felt
            // threat from a nearby predator (a sheltered agent is calmer, and — since
            // walls actually block the stalker — genuinely safer), so survival-need →
            // wall-in → calm is a real loop. Gated by `can_build`: only builders sense
            // shelter relief, so non-building worlds (where map edges give nonzero
            // enclosure) stay byte-identical to the incumbent appraisal.
            let shelter_relief = if self.cfg.can_build {
                1.0 - 0.8 * me.enclosure.clamp(0.0, 1.0)
            } else {
                1.0
            };
            survival = survival.max(prox * (1.0 - 0.3 * self.persona.boldness) * shelter_relief);
        }
        // affect modulation (uses *last* tick's mood — your current feeling shapes
        // how you appraise now): fear (negative valence × arousal) sharpens caution,
        // so a frightened agent treats threats as more pressing and flees sooner.
        if self.affect_mod {
            let fear = (-self.affect.valence).max(0.0) * self.affect.arousal;
            survival = (survival * (1.0 + 0.6 * fear)).min(1.0);
        }

        // FEAR OF DEATH (mortality salience, Terror Management Theory). A mind that
        // can die and senses its own decline feels a dread that present injury alone
        // doesn't capture: it is the *trajectory* that frightens — a body bleeding
        // out at 0.5-and-falling dreads more than one steady at 0.5. Driven by
        // (a) declining health (Δ over the last tick), (b) low absolute health as a
        // standing reminder, and (c) recent harm / witnessed death already lifted
        // `mortality` elsewhere. It decays when the body recovers. Gated by
        // `can_die`, so immortal worlds compute nothing here and stay bit-identical.
        if self.cfg.can_die {
            let decline = (self.prev_health - me.health).max(0.0); // how fast I'm fading
            let frailty = (1.0 - me.health).clamp(0.0, 1.0); // how close to the edge
            if decline > 1e-4 {
                // a DECLINING body's dread ACCUMULATES — the fear of a downward
                // trajectory builds the longer it persists (a bleeding wound that
                // won't stop is terrifying out of proportion to any one tick). Each
                // declining tick adds, scaled up sharply (per-tick health loss is
                // small) and amplified by how frail one already is. This is what
                // makes the dread *preventive*: it crosses the affiliation/shelter
                // thresholds well before health is critical.
                let add = (30.0 * decline) * (0.5 + frailty);
                self.mortality = (self.mortality + add).min(1.0);
            } else {
                // a stable or recovering body sheds dread gradually (relief).
                self.mortality = (self.mortality - 0.03).max(0.0);
            }
            // mortality salience raises the felt survival pressure preventively, so
            // the mind turns to shelter and to its friends *before* a crisis — the
            // TMT prediction (worldview + affiliation defence). Bounded so it sharpens
            // priorities without erasing hunger/thirst.
            survival = (survival + 0.5 * self.mortality).min(1.0);
        }
        self.prev_health = me.health;
        self.drives.set(Drive::Survival, survival);

        // intrinsic curiosity: novelty is rewarding (Schmidhuber; Pathak ICM).
        if novelty > 0 {
            self.drives
                .bump(Drive::Curiosity, 0.18 * novelty as f32 * self.persona.curiosity);
            self.metrics.discoveries += novelty as u64;
        }
        // learning-progress curiosity (Oudeyer–Kaplan IAC): competence *gain* is
        // its own reward, so the agent is pulled toward the learnable frontier and
        // — unlike novelty — not held by unlearnable noise (LP≈0 there).
        if self.lp_curiosity {
            let lp = self.lprog.progress().max(0.0);
            self.drives.bump(Drive::Curiosity, 0.9 * lp * self.persona.curiosity);
        }
        // gentle social/mastery drift so a sated agent is never inert.
        self.drives.bump(Drive::Social, 0.004 * self.persona.sociability);
        self.drives.bump(Drive::Mastery, 0.003);
        // curiosity relaxes toward a restless baseline.
        let c = self.drives.level(Drive::Curiosity);
        self.drives.set(Drive::Curiosity, c + (0.25 - c) * 0.04);
        // contentment (positive valence, low arousal) loosens curiosity — a calm,
        // satisfied agent is freer to explore.
        if self.affect_mod {
            let content = self.affect.valence.max(0.0) * (1.0 - self.affect.arousal);
            self.drives.bump(Drive::Curiosity, 0.02 * content);
        }

        // surprise = genuine prediction error from the learned anticipation
        // model (it learns down as the world becomes familiar).
        let surprise = self.anticipation.observe(p);

        // appraise the whole situation into a felt emotional state (read-only).
        // Mortality dread and grief both colour the feeling: dread (TMT) pushes
        // valence down + arousal up (afraid), while grief drags valence down with
        // less arousal (the weary, withdrawn pole of bereavement). Both are zero
        // unless their gene is on, so the baseline appraisal is unchanged.
        let mut condition = (me.health + me.energy + me.hydration) / 3.0;
        let mut threat = self.drives.level(Drive::Survival); // blends predator + injury
        condition = (condition - 0.4 * self.grief).clamp(0.0, 1.0); // loss dims wellbeing
        threat = (threat + 0.5 * self.mortality).clamp(0.0, 1.0); // dread reads as threat
        let urgency = Drive::ALL.iter().map(|&d| self.drives.level(d)).fold(0.0, f32::max);
        self.affect.update(condition, threat, surprise, urgency);

        surprise
    }

    /// Turn this tick's events into episodic memories, social updates, learned
    /// lessons, and metric tallies.
    fn record_events(&mut self, events: &[WorldEvent]) {
        let now = self.world.tick();
        for ev in events {
            let ep = match ev {
                WorldEvent::Ate { id, energy } => {
                    self.metrics.meals += 1;
                    self.memory.record_skill("forage", "find and eat food", true);
                    Episode {
                        tick: now,
                        what: format!("ate something nourishing (+{:.0}% energy)", energy * 100.0),
                        salience: 0.45,
                        valence: 0.6,
                        subject: Some(*id),
                    }
                }
                WorldEvent::Drank { id } => {
                    self.memory.record_skill("hydrate", "find and drink water", true);
                    Episode {
                        tick: now,
                        what: "drank from the water".into(),
                        salience: 0.4,
                        valence: 0.5,
                        subject: Some(*id),
                    }
                }
                WorldEvent::Repelled { id } => {
                    // confronting *worked* — the threat was driven off. Learn that
                    // facing it (especially together) is worthwhile.
                    self.confront_value = (self.confront_value + 0.5).min(1.5);
                    self.memory.record_skill("confront-predator", "face the stalker", true);
                    Episode {
                        tick: now,
                        what: "we drove the stalker off — standing together, it works".into(),
                        salience: 0.95,
                        valence: 0.8,
                        subject: Some(*id),
                    }
                }
                WorldEvent::Hurt { id, health } => {
                    self.metrics.near_death_escapes += 1;
                    self.memory.record_skill("evade-predator", "avoid predators", false);
                    // if I was striking when this happened, confronting cost me —
                    // value it less (a lone or failed stand is punished).
                    if self.last_struck {
                        self.confront_value = (self.confront_value - 0.3).max(-1.0);
                    }
                    self.memory.learn(
                        "predator",
                        "predators are dangerous; keep distance",
                        0.7,
                        now,
                    );
                    // mark *this place* as dangerous so the agent learns to avoid it.
                    let here = self.world.me().map(|m| m.pos);
                    if let Some(p) = here {
                        *self.danger.entry(region_of(p)).or_insert(0.0) += 1.0;
                    }
                    // META-MOTIVATION: if the very thing I was pursuing is what hurt
                    // me, value that pursuit less. The agent rewrites its own
                    // priorities from experience (narrow attribution: only the
                    // sought target, never ambient predator harm).
                    if self.metamotivation {
                        if let Some(g) = self.committed.clone() {
                            if goal_target_id(&g.kind) == Some(*id) {
                                self.drives.nudge_bias(g.origin, 0.82);
                            }
                        }
                    }
                    Episode {
                        tick: now,
                        what: format!("a predator hurt me (-{:.0}% health) — I won't forget that", health * 100.0),
                        salience: 0.97,
                        valence: -0.9,
                        subject: Some(*id),
                    }
                }
                WorldEvent::Heard { from, text } => {
                    self.social.heard(*from, text, now);
                    Episode {
                        tick: now,
                        what: format!("heard someone say: \"{text}\""),
                        salience: 0.6,
                        valence: self.social.disposition(*from).clamp(-1.0, 1.0),
                        subject: Some(*from),
                    }
                }
                WorldEvent::Spoke { to, text } => {
                    self.metrics.conversations += 1;
                    self.social.spoke_to(*to);
                    self.memory.record_skill("socialize", "talk to others", true);
                    Episode {
                        tick: now,
                        what: format!("I said: \"{text}\""),
                        salience: 0.4,
                        valence: 0.25,
                        subject: Some(*to),
                    }
                }
                WorldEvent::Told { from, info } => {
                    // dialogue with content: act on what we were told.
                    let what = match info {
                        daimon_core::Info::Greeting => {
                            self.social.heard(*from, "hello friend", now);
                            "someone greeted me".to_string()
                        }
                        daimon_core::Info::ResourceAt { id, kind, pos, label } => {
                            // learn the location — the planner will route here later.
                            self.memory.note_place(*id, *pos, label, *kind);
                            self.metrics.facts_learned += 1;
                            format!("{from:?} told me there's {label} at ({}, {})", pos.x, pos.y)
                        }
                        daimon_core::Info::DangerAt { pos } => {
                            *self.danger.entry(region_of(*pos)).or_insert(0.0) += 1.2;
                            format!("warned about danger near ({}, {})", pos.x, pos.y)
                        }
                    };
                    Episode { tick: now, what, salience: 0.5, valence: 0.2, subject: Some(*from) }
                }
                WorldEvent::Discovered { id } => Episode {
                    tick: now,
                    what: "found something new".into(),
                    salience: 0.45,
                    valence: 0.35,
                    subject: Some(*id),
                },
                WorldEvent::Vanished { id } => Episode {
                    tick: now,
                    what: "something I was tracking slipped out of view".into(),
                    salience: 0.2,
                    valence: -0.1,
                    subject: Some(*id),
                },
                WorldEvent::Died { id, pos: _, cause } => {
                    // A peer has died, for good. The CONTINUING BOND: we do not purge
                    // their model — we re-tag it "gone" (mark_dead), keeping the
                    // relationship so we can still reminisce. GRIEF is triggered only
                    // by rupture of an *attachment bond*: its intensity scales with
                    // how close we were (the bond = disposition at time of death). A
                    // stranger (low/no bond) produces ~no grief — that asymmetry is
                    // the point. Witnessing a death also lifts mortality salience
                    // (a stark reminder of one's own end).
                    let bond = self.social.mark_dead(*id, now).unwrap_or(0.0);
                    let name = self.social.model(*id).map(|m| m.name.clone());
                    if self.cfg.can_die {
                        // seeing another fall is a memento mori, bonded or not.
                        self.mortality = (self.mortality + 0.25).min(1.0);
                    }
                    if self.cfg.can_grieve && bond > BOND_THRESHOLD {
                        // grief ∝ closeness, accumulating if more than one is lost.
                        let intensity = (bond.clamp(0.0, 1.0)).min(1.0);
                        self.grief = (self.grief + intensity).min(1.0);
                        // grieve for the closest loss we carry.
                        let switch = self
                            .grieving_for
                            .map(|g| self.social.bond(g) < bond)
                            .unwrap_or(true);
                        if switch {
                            self.grieving_for = Some(*id);
                        }
                        let who = name.clone().unwrap_or_else(|| "a friend".into());
                        Episode {
                            tick: now,
                            what: format!("{who} is gone — taken by {cause}. I can't take it in."),
                            salience: 1.0,
                            valence: -1.0,
                            subject: Some(*id),
                        }
                    } else {
                        // a stranger's death: noted, but it does not wound us.
                        let who = name.unwrap_or_else(|| "someone".into());
                        Episode {
                            tick: now,
                            what: format!("{who} died — {cause} took them. We barely knew each other."),
                            salience: 0.45,
                            valence: -0.2,
                            subject: Some(*id),
                        }
                    }
                }
            };
            self.memory.remember(ep);
        }
    }

    /// Grief heals with time, and faster amid living friends (social support is the
    /// best-evidenced accelerant of bereavement recovery). The decay is geometric;
    /// a nearby bonded friend roughly triples the per-tick healing. When grief falls
    /// below the resolved threshold the wound closes and mourning ends — though the
    /// continuing bond to the dead is kept forever in theory-of-mind.
    fn decay_grief(&mut self, pos: Pos) {
        if self.grief <= 0.0 {
            return;
        }
        let supported =
            self.social.living_friend_near(pos, 5, BOND_THRESHOLD);
        // base half-life ≈ 350 ticks alone; ≈ 120 with a friend close by.
        let decay = if supported { 0.0058 } else { 0.0020 };
        self.grief = (self.grief - decay).max(0.0);
        if self.grief < GRIEF_RESOLVED_BELOW {
            self.grief = 0.0;
            self.grieving_for = None;
        }
    }

    // ----- step 3: reflex --------------------------------------------------

    fn reflex_check(&mut self, p: &Percept) -> Option<Thought> {
        let me = p.me;
        let threat = self.world.nearest_threat(me.pos)?;
        if threat.entity.pos.manhattan(me.pos) > self.persona.reflex_distance() {
            return None;
        }
        let tid = threat.entity.id;
        self.metrics.reflexes += 1;
        self.drives.set(Drive::Survival, 1.0);
        // The agent has two faces of survival: flee, or — if it has the tool —
        // *confront*. Which it picks is its own call, from what it has learned
        // about confronting (confront_value) plus innate boldness, with a little
        // exploration so the option gets *tried* and thus becomes learnable.
        // Nothing here looks at allies or commands a rally; any togetherness must
        // emerge from many agents independently facing the same threat.
        let confront = self.can_fight && {
            let inclination = self.confront_value + 0.3 * self.persona.boldness;
            inclination > 0.6 || self.rng.chance(0.04 + 0.10 * self.persona.boldness)
        };
        let kind = if confront { GoalKind::Confront(tid) } else { GoalKind::Flee(tid) };
        let goal = Goal { kind: kind.clone(), origin: Drive::Survival, priority: 1.0 };
        // PREDATOR-AWARE COORDINATION: when the faculty is on, hand the flee planner
        // this mind's local prey group so its evasive step composes flee + herd
        // (selfish-herd anti-isolation). Off ⇒ `None`, byte-identical straight flee.
        let allies = if self.herd_evasion { self.herd_positions() } else { Vec::new() };
        let herd = if self.herd_evasion {
            Some(Herd { cohesion: self.herd_cohesion, allies: &allies })
        } else {
            None
        };
        // a reflex re-plans immediately and unconditionally.
        self.plan = Some(plan_for_with(
            &goal,
            &self.world,
            &self.memory,
            &self.danger,
            &mut self.rng,
            p.tick,
            None,
            &[],
            0.0,
            herd,
        ));
        let action = self.next_action();
        self.last_struck = matches!(action, Action::Strike(_));
        let inner = if confront {
            format!("[{}] I'm done running — I turn and face it.", self.persona.name)
        } else {
            format!("[{}] The predator is right there — no time to think, I run.", self.persona.name)
        };
        Some(Thought {
            tick: p.tick,
            process: Process::Reflex,
            dominant_drive: Drive::Survival,
            goal: kind,
            action,
            inner,
        })
    }

    // ----- step 4: decide --------------------------------------------------

    fn decide(&mut self, surprise: f32) -> (Goal, Process, String) {
        self.invented_now = false;
        // FRONTIER: a self-invented goal from a learned affordance overrides the
        // built-in drives. Nothing coded this — the agent learned the thing helps
        // and decided, on its own, to use it.
        if let Some(goal) = self.invented_goal() {
            self.invented_now = true;
            self.metrics.praxis_invented += 1;
            self.committed = Some(goal.clone());
            self.held = false;
            return (goal, Process::Routine, String::new());
        }

        // QUANTUM COGNITION: when enabled, the agent decides by collapsing a
        // superposition over its drives (Born rule) rather than argmax — so its
        // choices carry interference and order effects no classical NPC can.
        if self.quantum {
            let mut order: Vec<Drive> = Drive::ALL.to_vec();
            order.sort_by(|a, b| self.drives.pressure(*b).total_cmp(&self.drives.pressure(*a)));
            let d = self.quantum_choice(&order[..order.len().min(3)]);
            let kind = self.fast_goal(d);
            let reason = self.fast_reason(d, &kind);
            let pressure = self.drives.pressure(d);
            return self.apply_commitment(
                Goal { kind, origin: d, priority: pressure },
                Process::Routine,
                reason,
                pressure,
            );
        }

        // SYSTEM 2 — the learned overlay nudges which drive dominates. With the
        // overlay disabled, `dominant_biased` returns exactly `drives.dominant()`,
        // so the whole cascade below is byte-identical to the pure instinct.
        let (dom, dom_pressure) = self.dominant_biased();

        // CONFRONT: when survival leads and a threat is in view, the agent may
        // choose to face it instead of fleeing — its own call from what it has
        // learned (confront_value) + innate boldness + a little exploration. It
        // never looks at how many allies are near; any rally must emerge from many
        // agents each, independently, facing the same threat.
        if self.can_fight && dom == Drive::Survival {
            let mypos = self.world.me().map(|m| m.pos).unwrap_or(Pos::new(0, 0));
            if let Some(t) = self.world.nearest_threat(mypos) {
                let tid = t.entity.id;
                let inclination = self.confront_value + 0.3 * self.persona.boldness;
                if inclination > 0.6 || self.rng.chance(0.06 + 0.12 * self.persona.boldness) {
                    let kind = GoalKind::Confront(tid);
                    let reason = self.fast_reason(Drive::Survival, &kind);
                    let pr = dom_pressure.max(0.9);
                    return self.apply_commitment(
                        Goal { kind, origin: Drive::Survival, priority: pr },
                        Process::Routine,
                        reason,
                        pr,
                    );
                }
            }
        }

        // SHELTER: a felt-safety move, not a scripted hut. When the agent has the
        // build affordance and feels EXPOSED (low enclosure) with a buildable gap,
        // and a threat is perceived (a predator belief within ~5 cells) OR it was
        // recently hurt — while hunger/thirst are not critical (those still win) —
        // it adopts a Shelter goal and walls in the open side. Repeating this,
        // side by side, surrounds the agent and a shelter *emerges*. Deterministic:
        // no RNG draw, so seeded worlds with building off are byte-identical.
        if self.cfg.can_build {
            if let Some(me) = self.world.me() {
                let hunger = self.drives.level(Drive::Hunger);
                let thirst = self.drives.level(Drive::Thirst);
                let needs_ok = hunger < CRITICAL && thirst < CRITICAL;
                let exposed = me.enclosure < 0.75 && me.shelter_gap.is_some();
                let threat_near = self
                    .world
                    .nearest_threat(me.pos)
                    .map(|t| t.entity.pos.manhattan(me.pos) <= 5)
                    .unwrap_or(false);
                let recently_hurt = me.health < 0.85;
                // FEAR OF DEATH biases toward shelter PREVENTIVELY: a mortal mind that
                // dreads its own decline (high mortality salience) seeks walls before
                // any wound or predator — the TMT worldview/shelter defence. Zero for
                // immortal agents, so this only adds behaviour where mortality is on.
                let dread = self.cfg.can_die && self.mortality > 0.5;
                if needs_ok && exposed && (threat_near || recently_hurt || dread) {
                    let kind = GoalKind::Shelter;
                    let reason =
                        "I'm out in the open and it's not safe — I'll wall myself in".to_string();
                    // urgent enough to hold against routine pulls, below a true crisis.
                    let pr = dom_pressure.clamp(0.6, 0.95);
                    return self.apply_commitment(
                        Goal { kind, origin: Drive::Survival, priority: pr },
                        Process::Routine,
                        reason,
                        pr,
                    );
                }
            }
        }

        // FEAR-OF-DEATH AFFILIATION (TMT's affiliation defence). A mind that feels
        // its mortality keenly turns toward its own — being near others is a balm
        // against the dread of the end. When mortality salience is high and no
        // bodily crisis presses, the agent seeks out a *living* friend, preventively.
        // Gated by can_die; the RNG draw is inside the gate so off-worlds are
        // bit-identical. Shelter (above) is the other TMT defence; affiliation is this.
        if self.cfg.can_die && self.mortality > 0.4 {
            let hunger = self.drives.level(Drive::Hunger);
            let thirst = self.drives.level(Drive::Thirst);
            let crisis = hunger > CRITICAL || thirst > CRITICAL;
            if !crisis {
                if let Some(friend) = self.social.friendliest() {
                    if friend.disposition > 0.0 && self.rng.chance(0.35 + 0.4 * self.mortality) {
                        let fid = friend.id;
                        let kind = GoalKind::Socialize(fid);
                        let reason =
                            "I can feel my own end out here — I don't want to face it alone".to_string();
                        let pr = (0.55 + 0.3 * self.mortality).clamp(0.55, 0.9);
                        return self.apply_commitment(
                            Goal { kind, origin: Drive::Social, priority: pr },
                            Process::Routine,
                            reason,
                            pr,
                        );
                    }
                }
            }
        }

        // MOURN: the loss-oriented pole of the Dual Process Model of grief (Stroebe
        // & Schut 1999). A grieving mind does not mourn *constantly* — it OSCILLATES
        // between loss-orientation (withdraw, idle, reminisce) and restoration
        // (re-engage ordinary goals). We model the oscillation as a grief-weighted
        // alternation: the fraction of ticks spent mourning rises with grief
        // intensity and falls as it heals, so the mind swings between the two and,
        // as the wound closes, returns fully to life. A genuine survival crisis
        // (handled above) always pre-empts mourning — you flee the stalker even in
        // grief. Gated by can_grieve; inert (and drawing no RNG) otherwise.
        if self.cfg.can_grieve && self.grief > GRIEF_RESOLVED_BELOW {
            // Even in grief the BODY comes first — a mind that withdrew while hungry
            // would starve, turning mourning into a death spiral. So any meaningful
            // hunger/thirst (not just a crisis) pulls the mind out of mourning and
            // back to foraging: bereavement bends ordinary life, it does not abolish
            // it. (Restoration-orientation in the Dual Process Model is exactly this
            // pull of daily necessity.)
            let hunger = self.drives.level(Drive::Hunger);
            let thirst = self.drives.level(Drive::Thirst);
            let needs_ok = hunger < 0.5 && thirst < 0.5;
            // oscillate: mourn on a grief-weighted share of ticks. A small RNG draw
            // gives the swing an organic, non-periodic rhythm (this is the only new
            // RNG, and it is gated behind can_grieve — off-worlds are bit-identical).
            let mourn_share = (0.2 + 0.5 * self.grief).clamp(0.0, 0.7);
            if needs_ok && self.rng.chance(mourn_share) {
                let kind = GoalKind::Mourn;
                let reason = "the grief pulls me inward — I can't move on just yet".to_string();
                let pr = (0.5 + 0.4 * self.grief).clamp(0.5, 0.9);
                return self.apply_commitment(
                    Goal { kind, origin: Drive::Social, priority: pr },
                    Process::Routine,
                    reason,
                    pr,
                );
            }
        }

        // PROVISION: stock up against winter. An open-world, Mastery+foresight move,
        // never a scripted "prepare for winter". When the mind has the provisioning
        // affordance and the world is an open world (signalled by the body sensing a
        // real season / an approaching winter), and its IMMEDIATE needs are met
        // (hunger/thirst not pressing — the body always wins), it adopts a Provision
        // goal IF it is harvest season (summer/autumn) OR winter is *anticipated*
        // within its foresight horizon, and there is still gathering/storing to do.
        // The foresight gene is what makes this PREVENTIVE: a foresighted mind reads
        // `winter_in` and begins stocking before the cold, exactly the survival edge.
        // Gated by can_provision; in a closed world `season`/`winter_in` are inert
        // defaults (Spring, winter never) so this never fires and draws no RNG.
        if self.cfg.can_provision {
            if let Some(me) = self.world.me() {
                let hunger = self.drives.level(Drive::Hunger);
                let thirst = self.drives.level(Drive::Thirst);
                // a comfortable margin below CRITICAL: bodily needs always pre-empt
                // provisioning (you don't stockpile while you're starving).
                let needs_ok = hunger < 0.6 && thirst < 0.6;
                let harvest = me.season == 1 || me.season == 2; // summer / autumn
                let winter = me.season == 3;
                // anticipation: winter is within the foresight lead-time the mind has
                // (the same faculty that forages ahead of hunger now stocks ahead of
                // cold). Foresight 0 ⇒ purely reactive: only stocks once it IS harvest.
                let lead = self.drives.foresight().max(1.0);
                let winter_soon = me.winter_in <= lead;
                // is there anything to do? In the good seasons: gather more, or carry a
                // load home. In WINTER: there is nothing to gather (food has stopped),
                // but a mind should come HOME to the hearth — store_dir homes toward it
                // in winter — to draw on the village stores and stay warm. So winter
                // work = "the hearth is somewhere to go".
                let stocking = me.gather_dir.is_some() || (me.carrying > 0.05 && me.store_dir.is_some());
                // in winter — and the late-autumn run-up to it — being at the hearth IS
                // the work: walk home (store_dir homes toward it then) or rest there in
                // the warmth, drawing the stores. The store_dir homing window (set by
                // the world from `winter_in`) is what turns the abstract "winter soon"
                // into a concrete pull toward the cache before the cold lands.
                // The world hands a `store_dir` during the late-autumn → winter homing
                // window (and to carry a load home in the good seasons). When it points
                // home, coming to the hearth IS the provisioning act — we trust that
                // world signal, which already encodes the right season window, so the
                // foresight horizon and the homing window stay consistent.
                let coming_home = me.store_dir.is_some();
                let work = stocking || coming_home;
                if needs_ok && (harvest || winter_soon || winter || coming_home) && work {
                    let kind = GoalKind::Provision;
                    let reason = if winter {
                        "the cold is here — back to the hearth and the stores we laid by".to_string()
                    } else if winter_soon {
                        "winter is coming — I should put stores by while I can".to_string()
                    } else {
                        "it's the season of plenty — time to stock up for the lean months".to_string()
                    };
                    // a Mastery-strength pull: firm enough to hold against routine
                    // exploration, well below any survival/forage crisis.
                    let pr = (0.45 + 0.25 * self.persona.curiosity).clamp(0.45, 0.7);
                    return self.apply_commitment(
                        Goal { kind, origin: Drive::Mastery, priority: pr },
                        Process::Routine,
                        reason,
                        pr,
                    );
                }
            }
        }

        // form a *proposal* — the goal this tick's appraisal favours, via the
        // fast path or, when warranted, the slow deliberator.
        let (proposal, process, rationale) = if self.should_escalate(surprise, dom_pressure) {
            self.metrics.deliberations += 1;
            self.last_deliberation = Some(self.world.tick());

            // take the slow path out so it can borrow the rest of `self`.
            let mut delib = self.deliberator.take().expect("deliberator present");
            let d = {
                let ctx = DeliberationContext {
                    tick: self.world.tick(),
                    persona: &self.persona,
                    drives: &self.drives,
                    world: &self.world,
                    memory: &self.memory,
                    social: &self.social,
                    surprise,
                };
                delib.deliberate(&ctx)
            };
            self.deliberator = Some(delib);

            for l in d.lessons {
                self.memory.learn(&l.key, &l.statement, l.confidence, self.world.tick());
            }
            let goal = Goal {
                kind: d.goal,
                origin: dom,
                priority: dom_pressure,
            };
            (goal, Process::Deliberate, d.rationale)
        } else {
            let kind = self.fast_goal(dom);
            let reason = self.fast_reason(dom, &kind);
            let goal = Goal {
                kind,
                origin: dom,
                priority: dom_pressure,
            };
            (goal, Process::Routine, reason)
        };

        self.apply_commitment(proposal, process, rationale, dom_pressure)
    }

    /// Decide whether to act on the new proposal or stay the course. Switching
    /// only when the proposal *clearly* beats the standing commitment (by
    /// [`COMMIT_MARGIN`]) is what turns a twitchy optimiser into something that
    /// looks like it has a plan.
    fn apply_commitment(
        &mut self,
        proposal: Goal,
        process: Process,
        rationale: String,
        proposed_pressure: f32,
    ) -> (Goal, Process, String) {
        // A deliberated decision is, by definition, a considered re-commitment.
        let deliberated = process == Process::Deliberate;

        if let Some(cur) = self.committed.clone() {
            let cur_pressure = self.drives.level(cur.origin) * cur.origin.salience_weight();
            let satisfied = self.drives.level(cur.origin) < SATISFIED_BELOW;
            let same = std::mem::discriminant(&cur.kind) == std::mem::discriminant(&proposal.kind);
            let clearly_better = proposed_pressure > cur_pressure + COMMIT_MARGIN;
            // a critical need always breaks the current commitment.
            let urgent = self.drives.level(proposal.origin) > CRITICAL;

            if !same && !satisfied && !clearly_better && !deliberated && !urgent {
                // hold the line: keep pursuing the current intention.
                let label = cur.kind.label();
                self.held = true;
                return (
                    cur,
                    Process::Routine,
                    format!("staying with what I started — {label}"),
                );
            }
        }
        // adopt the proposal as the new commitment.
        self.held = false;
        self.committed = Some(proposal.clone());
        (proposal, process, rationale)
    }

    /// The escalation policy: think hard on surprise, on high stakes, or on a
    /// genuinely close call — but no more often than the cooldown allows,
    /// unless surprise overrides the budget entirely.
    fn should_escalate(&self, surprise: f32, dom_pressure: f32) -> bool {
        let high_surprise = surprise >= self.cfg.surprise_threshold;

        // ambiguity: are the two strongest pressures near-tied?
        let mut pressures: Vec<f32> = Drive::ALL
            .into_iter()
            .map(|d| self.drives.level(d) * d.salience_weight())
            .collect();
        pressures.sort_by(|a, b| b.total_cmp(a));
        let ambiguous = pressures.len() >= 2 && (pressures[0] - pressures[1]).abs() < self.cfg.tie_margin;

        let high_stakes = dom_pressure > 1.6; // survival/strong need territory

        let triggered = high_surprise || ambiguous || high_stakes;
        if !triggered {
            return false;
        }
        if high_surprise {
            return true; // emergencies ignore the budget
        }
        // otherwise respect the cooldown.
        match self.last_deliberation {
            Some(t) => self.world.tick().saturating_sub(t) >= self.cfg.deliberation_cooldown,
            None => true,
        }
    }

    /// Map the dominant drive to its default goal using current beliefs.
    fn fast_goal(&self, dom: Drive) -> GoalKind {
        let pos = self.world.me().map(|m| m.pos).unwrap_or(Pos::new(0, 0));
        match dom {
            Drive::Hunger => GoalKind::Forage,
            Drive::Thirst => GoalKind::Hydrate,
            // Survival has two faces: a predator means flee; otherwise the
            // agent is depleted/injured, and the cure is to address the biggest
            // physiological deficit — never to sit and rest while dying of
            // thirst. (Resting only helps once the real needs are met.)
            Drive::Survival => {
                if let Some(t) = self.world.nearest_threat(pos) {
                    GoalKind::Flee(t.entity.id)
                } else {
                    let thirst = self.drives.level(Drive::Thirst);
                    let hunger = self.drives.level(Drive::Hunger);
                    if thirst >= 0.5 && thirst >= hunger {
                        GoalKind::Hydrate
                    } else if hunger >= 0.5 {
                        GoalKind::Forage
                    } else {
                        GoalKind::Recover
                    }
                }
            }
            Drive::Curiosity => self
                .world
                .nearest_of(EntityKind::Curio, pos)
                .map(|c| GoalKind::Investigate(c.entity.id))
                .unwrap_or(GoalKind::Explore),
            Drive::Social => self
                .world
                .visible_of(EntityKind::Agent)
                .first()
                .map(|a| GoalKind::Socialize(a.id))
                .unwrap_or(GoalKind::Explore),
            // "free time" — steer it toward the standing life project.
            Drive::Mastery => match self.project.as_ref().map(|p| p.kind) {
                Some(ProjectKind::Provision) => GoalKind::Forage,
                Some(ProjectKind::ExploreEverything) => self
                    .world
                    .nearest_of(EntityKind::Curio, pos)
                    .map(|c| GoalKind::Investigate(c.entity.id))
                    .unwrap_or(GoalKind::Explore),
                Some(ProjectKind::Companionship) => self
                    .world
                    .visible_of(EntityKind::Agent)
                    .first()
                    .map(|a| GoalKind::Socialize(a.id))
                    .unwrap_or(GoalKind::Explore),
                None => GoalKind::Explore,
            },
        }
    }

    fn fast_reason(&self, dom: Drive, kind: &GoalKind) -> String {
        format!("{} pulls hardest; I'll {}", dom.name(), kind.label())
    }

    // ----- step 5/6: plan + act -------------------------------------------

    fn ensure_plan(&mut self, goal: &Goal) {
        let needs_replan = match &self.plan {
            None => true,
            Some(p) => {
                p.is_done()
                    || std::mem::discriminant(&p.goal.kind) != std::mem::discriminant(&goal.kind)
                    || self.world.tick().saturating_sub(p.formed) > self.cfg.plan_staleness
            }
        };
        if needs_replan {
            // EMPOWERMENT: free-time exploration is steered toward the move that
            // leads to the most-reachable future under the learned model — the
            // agent seeks open ground and shuns dead-ends, on its own.
            let comfortable = self
                .world
                .me()
                .map(|m| m.energy > 0.6 && m.hydration > 0.6 && m.health > 0.6)
                .unwrap_or(false);
            if self.empowered && comfortable && matches!(goal.kind, GoalKind::Explore) {
                if let Some(me) = self.world.me() {
                    let (w, h) = self.seen_bounds;
                    let tb = self.rng.below(4);
                    let mut cands: Vec<(Dir, usize)> = Vec::new();
                    for i in 0..4 {
                        let d = Dir::ALL[(i + tb) % 4];
                        let np = self.forward.predict(me.pos, d).unwrap_or_else(|| {
                            let s = me.pos.step(d);
                            Pos::new(s.x.clamp(0, w - 1), s.y.clamp(0, h - 1))
                        });
                        // a move that does nothing, or leads into danger, is no good.
                        if np == me.pos || self.danger_level(np) > 0.5 {
                            continue;
                        }
                        cands.push((d, self.forward.empowerment(np, 4, w, h)));
                    }
                    // Only let empowerment steer when there is a *real gradient* —
                    // a dead-end to flee. In open terrain (all moves ~equal) it
                    // stays out of the way and the agent wanders normally, so it
                    // never strands itself far from food.
                    let hi = cands.iter().map(|(_, e)| *e).max().unwrap_or(0);
                    let lo = cands.iter().map(|(_, e)| *e).min().unwrap_or(0);
                    if hi.saturating_sub(lo) >= 3 {
                        if let Some((d, _)) = cands.iter().max_by_key(|(_, e)| *e) {
                            self.plan = Some(Plan::new(goal.clone(), vec![Action::Move(*d)], self.world.tick()));
                            return;
                        }
                    }
                }
            }
            // IMAGINATION: if the straight-line step toward the target is a wall
            // the agent has *learned about*, plan a route around it through the
            // learned model instead of bumping the wall forever.
            if self.imagine {
                if let (Some(me), Some(tgt)) = (self.world.me(), self.goal_target_pos(&goal.kind)) {
                    if me.pos.manhattan(tgt) <= 1 {
                        self.detour_target = None; // arrived
                    } else {
                        let greedy = me.pos.toward(tgt);
                        // engage (and *stay engaged*) on a detour once the direct
                        // route is known blocked — committing to the plan instead
                        // of snapping back to greedy and oscillating.
                        let detouring = self.detour_target == Some(tgt)
                            || self.forward.known_blocked(me.pos, greedy);
                        if detouring {
                            if let Some(d) = self.forward.plan_to(
                                me.pos,
                                tgt,
                                self.seen_bounds.0,
                                self.seen_bounds.1,
                            ) {
                                self.detour_target = Some(tgt);
                                self.plan = Some(Plan::new(
                                    goal.clone(),
                                    vec![Action::Move(d)],
                                    self.world.tick(),
                                ));
                                return;
                            }
                        }
                    }
                }
            }
            // goal-directed foraging by drive-reduction-rate, when enabled: choose
            // the resource the planner should route to, instead of nearest.
            let forage_override = if self.forage_drr {
                match goal.kind {
                    GoalKind::Forage => self.drr_target(EntityKind::Food),
                    GoalKind::Hydrate => self.drr_target(EntityKind::Water),
                    _ => None,
                }
            } else {
                None
            };
            // commons-aware foraging: hand the planner peers' claims so it can
            // yield contested tiles to the more-urgent and disperse.
            let (contention, my_urgency): (&[(Pos, f32)], f32) = if self.social_forage {
                let u = match goal.kind {
                    GoalKind::Forage => self.drives.level(Drive::Hunger),
                    GoalKind::Hydrate => self.drives.level(Drive::Thirst),
                    _ => 0.0,
                };
                (&self.contention, u)
            } else {
                (&[], 0.0)
            };
            // PREDATOR-AWARE COORDINATION: supply the local prey group for the flee
            // path (only matters for GoalKind::Flee; ignored for every other goal).
            // Off ⇒ None ⇒ incumbent straight-away flee, byte-identical.
            let allies = if self.herd_evasion && matches!(goal.kind, GoalKind::Flee(_)) {
                self.herd_positions()
            } else {
                Vec::new()
            };
            let herd = if self.herd_evasion && matches!(goal.kind, GoalKind::Flee(_)) {
                Some(Herd { cohesion: self.herd_cohesion, allies: &allies })
            } else {
                None
            };
            self.plan = Some(plan_for_with(
                goal,
                &self.world,
                &self.memory,
                &self.danger,
                &mut self.rng,
                self.world.tick(),
                forage_override,
                contention,
                my_urgency,
                herd,
            ));
        }
    }

    fn next_action(&mut self) -> Action {
        match self.plan.as_mut().and_then(|p| p.advance()) {
            Some(a) => a,
            None => Action::Wait,
        }
    }

    // ----- step 7: reflect -------------------------------------------------

    /// Distil recent experience into durable semantic knowledge — the slow,
    /// off-the-critical-path consolidation that Generative Agents calls
    /// reflection. Cheap here; in a real Daimon this is another LLM pass that
    /// writes higher-level beliefs ("the elder shares food", "the north woods
    /// are dangerous") back into memory.
    fn reflect(&mut self) {
        self.metrics.reflections += 1;
        let now = self.world.tick();

        // 0) CONSOLIDATION ("sleep" replay): re-process the most salient recent
        //    episodes, re-presenting their subjects to associative memory so the
        //    moments that mattered become more retrievable later — offline
        //    learning from the agent's own logged experience (hippocampal replay).
        if self.consolidate {
            let mut eps: Vec<(f32, u32)> = self
                .memory
                .episodes()
                .filter_map(|e| e.subject.map(|s| (e.salience, s.0)))
                .collect();
            eps.sort_by(|a, b| b.0.total_cmp(&a.0));
            for (_, subj) in eps.into_iter().take(4) {
                self.memory.associate(&[subj], now);
            }
        }

        // 1) consolidate competent skills into self-knowledge.
        let best = self
            .memory
            .skills()
            .filter(|s| s.uses >= 2)
            .max_by(|a, b| a.competence().total_cmp(&b.competence()))
            .map(|s| (s.name.clone(), s.competence()));
        if let Some((name, comp)) = best {
            if comp > 0.5 {
                self.memory.learn(
                    &format!("skill:{name}"),
                    &format!("I've gotten reliable at {name} ({:.0}% success)", comp * 100.0),
                    comp,
                    now,
                );
            }
        }

        // 2) turn remembered resource locations into stable facts.
        let places: Vec<(String, Pos)> = self
            .memory
            .places()
            .filter(|(_, p)| matches!(p.kind, EntityKind::Food | EntityKind::Water))
            .map(|(_, p)| (p.label.clone(), p.pos))
            .collect();
        for (label, pos) in places {
            self.memory.learn(
                &format!("place:{label}"),
                &format!("there's {label} around ({}, {})", pos.x, pos.y),
                0.7,
                now,
            );
        }

        // 3) note a standing relationship, if one has formed.
        if let Some(friend) = self.social.friendliest() {
            if friend.disposition > 0.4 {
                self.memory.learn(
                    &format!("friend:{}", friend.name),
                    &format!("{} is a friend", friend.name),
                    friend.disposition,
                    now,
                );
            }
        }

        // 4) DANGER ZONES — decay the map, then turn places where harm clustered
        //    into durable beliefs the agent acts on (the planner avoids them).
        for v in self.danger.values_mut() {
            *v *= 0.92;
        }
        self.danger.retain(|_, v| *v > 0.05);
        for (&(rx, ry), &d) in self.danger.iter() {
            if d >= 1.5 {
                let (cx, cy) = (rx * crate::planner::REGION + 1, ry * crate::planner::REGION + 1);
                self.memory.learn(
                    &format!("danger:{rx},{ry}"),
                    &format!("the ground around ({cx},{cy}) is dangerous — I keep clear"),
                    (d / 3.0).min(1.0),
                    now,
                );
            }
        }

        // 5) DERIVED SPATIAL INSIGHT — where do resources tend to be? Average the
        //    remembered resource positions into a felt sense of direction.
        let res: Vec<Pos> = self
            .memory
            .places()
            .filter(|(_, p)| matches!(p.kind, EntityKind::Food | EntityKind::Water))
            .map(|(_, p)| p.pos)
            .collect();
        if res.len() >= 3 {
            let me = self.world.me().map(|m| m.pos).unwrap_or(Pos::new(0, 0));
            let ax = res.iter().map(|p| p.x).sum::<i32>() / res.len() as i32;
            let ay = res.iter().map(|p| p.y).sum::<i32>() / res.len() as i32;
            let dir = compass(ax - me.x, ay - me.y);
            self.memory.learn(
                "insight:resources",
                &format!("most of what sustains me lies to the {dir}"),
                0.6,
                now,
            );
        }

        // 6) DERIVED SOCIAL INSIGHT — who do I cross paths with most?
        if let Some(m) = self.social.known().max_by_key(|m| m.interactions) {
            if m.interactions >= 3 {
                self.memory.learn(
                    "insight:social",
                    &format!("I run into {} more than anyone", m.name),
                    0.6,
                    now,
                );
            }
        }
    }

    /// The herd-evasion parameters for the planner's flee path: `Some(Herd)` with
    /// this mind's cohesion and the positions of its *visible* allies (the local prey
    /// group it can actually coordinate with) when the faculty is on AND there is a
    /// group to herd toward; `None` otherwise — which the planner reads as the
    /// incumbent straight-away flee. No RNG, so off-worlds stay byte-identical.
    fn herd_positions(&self) -> Vec<Pos> {
        self.world
            .visible_of(EntityKind::Agent)
            .iter()
            .map(|e| e.pos)
            .collect()
    }

    /// How dangerous the agent believes a position is (0 = safe).
    pub fn danger_level(&self, p: Pos) -> f32 {
        *self.danger.get(&region_of(p)).unwrap_or(&0.0)
    }

    /// The agent's standing long-horizon project, if any.
    pub fn project(&self) -> Option<&Project> {
        self.project.as_ref()
    }

    /// The emergent concept/affordance layer (self-invented categories).
    pub fn praxis(&self) -> &Praxis {
        &self.praxis
    }

    /// A goal nobody coded: when hurt, if the agent has *learned* that some form
    /// mends it and can locate one, go there. This is goal genesis from a learned
    /// affordance — the override that lets the agent use the unforeseen.
    fn invented_goal(&self) -> Option<Goal> {
        let me = self.world.me()?;
        if me.health >= 0.55 {
            return None;
        }
        // if low health is *caused* by hunger/thirst, eat or drink — don't go
        // chasing a "mending" form while starving. The invented heal-seek is for
        // being hurt (e.g. by the predator) while otherwise fed and watered.
        if me.energy < 0.45 || me.hydration < 0.45 {
            return None;
        }
        let mc = self.praxis.mending_concept()?;
        let pos = me.pos;
        let target = self
            .world
            .beliefs()
            .filter(|b| self.praxis.classify(&b.entity) == Some(mc))
            .min_by_key(|b| b.entity.pos.manhattan(pos))?;
        Some(Goal {
            kind: GoalKind::Investigate(target.entity.id),
            origin: Drive::Survival,
            priority: 1.0,
        })
    }

    /// The centre of the most dangerous region the agent knows of (for warning
    /// others), if any region is clearly dangerous.
    pub fn worst_danger(&self) -> Option<Pos> {
        self.danger
            .iter()
            .filter(|(_, d)| **d >= 1.0)
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|((rx, ry), _)| {
                Pos::new(rx * crate::planner::REGION + 1, ry * crate::planner::REGION + 1)
            })
    }

    /// Advance the long-horizon project from this tick's action and events.
    fn update_project(&mut self, action: &Action, events: &[WorldEvent]) {
        let now = self.world.tick();
        let kind = match &self.project {
            Some(p) if !p.is_done() => p.kind,
            _ => return,
        };
        // figure out progress *before* taking the &mut borrow on the project.
        let (curio, units) = match kind {
            ProjectKind::ExploreEverything => (
                if let Action::Inspect(id) = action { Some(id.0) } else { None },
                0,
            ),
            ProjectKind::Provision => (
                None,
                events.iter().filter(|e| matches!(e, WorldEvent::Ate { .. })).count() as u32,
            ),
            ProjectKind::Companionship => {
                let near_friend = self.world.me().map(|m| m.pos).is_some_and(|mp| {
                    self.world.beliefs().any(|b| {
                        b.visible
                            && b.entity.kind == EntityKind::Agent
                            && b.entity.pos.manhattan(mp) <= 3
                            && self.social.disposition(b.entity.id) > 0.3
                    })
                });
                (None, if near_friend { 1 } else { 0 })
            }
        };
        let aligned = curio.is_some() || units > 0;
        if let Some(proj) = self.project.as_mut() {
            if curio.is_some() {
                proj.advance(curio, now);
            }
            for _ in 0..units {
                proj.advance(None, now);
            }
            if proj.completed == Some(now) {
                self.metrics.project_completed = true;
            }
        }
        if aligned {
            self.metrics.project_ticks += 1;
        }
    }

    // ----- expression ------------------------------------------------------

    /// Compose this tick's inner-monologue line from concrete current state,
    /// via the procedural narrator (varied + situational, not templated).
    fn narrate(&mut self, goal: &Goal, process: Process, surprise: f32) -> String {
        let name = self.persona.name.clone();
        // a self-invented goal gets its own voice — the agent naming a thing it
        // figured out for itself.
        if self.invented_now {
            if let Some(mc) = self.praxis.mending_concept() {
                let c = &self.praxis.concepts[mc];
                return format!("[{name}] I worked it out — {} ({}). I'm going to it.", c.name, c.epithet());
            }
        }
        let (target, coord, other) = self.phrase_target(&goal.kind);
        // a short recalled fact to lean on, if any
        let memory = self
            .memory
            .facts()
            .filter(|(k, _)| k.starts_with("place:") || k.starts_with("danger:") || k.starts_with("insight:"))
            .map(|(_, f)| f.statement.clone())
            .next();
        let ph = crate::language::Phrasing {
            name: &name,
            goal: &goal.kind,
            process,
            drive: goal.origin,
            surprise,
            target: target.as_deref(),
            coord,
            memory: memory.as_deref(),
            other: other.as_deref(),
            holding: self.held,
        };
        crate::language::decision_line(&mut self.rng, &ph)
    }

    /// Where a goal's target sits (for imagination/path-planning). Flee is
    /// excluded — fleeing is movement *away*, not toward a point.
    /// Drive-Reduction Rate under survival risk (DRR). Among all *known* resources
    /// of `want`, choose the one that maximises expected aggregate drive reduction
    /// × trip-survival ÷ travel time — i.e. the most need-relief per risk-adjusted
    /// tick, rather than the merely nearest tile. Synthesises homeostatic RL
    /// (Keramati & Gutkin 2014: ΔD = D(H_before) − D(H_after), convex drive norm),
    /// optimal-foraging rate currency (Charnov 1976), and survival-weighted value
    /// (Mangel & Clark 1986). The convex drive `h_h² + h_t²` makes the worst need
    /// dominate for free; needs are crept forward over the trip (anticipation),
    /// so the score already accounts for arriving later than now.
    fn drr_target(&self, want: EntityKind) -> Option<(EntityId, Pos)> {
        let pos = self.world.me()?.pos;
        let (hunger, thirst) = (self.drives.level(Drive::Hunger), self.drives.level(Drive::Thirst));
        let (creep_h, creep_t) = (Drive::Hunger.creep(), Drive::Thirst.creep());
        // intake fully relieves the targeted need (clamped at the satisfied point).
        let relief = 0.6f32;
        // the stalker *now*: a route or destination near its believed position is
        // a place a trip gets interrupted and the deficit deepens — so it must
        // scale value down (Mangel & Clark), not merely add to learned scars.
        let predator = self.world.nearest_of(EntityKind::Predator, pos).map(|b| b.entity.pos);
        let predator_near = |c: Pos| -> f32 {
            predator.map_or(0.0, |pp| 1.0 / (1.0 + pp.manhattan(c) as f32 * 0.5))
        };
        let mut best: Option<(EntityId, Pos, f32)> = None;
        for (id, place) in self.memory.places().filter(|(_, p)| p.kind == want) {
            let d = place.pos;
            let t = pos.manhattan(d) as f32 + 1.0; // travel + one tick of handling
            let ha_h = (hunger + creep_h * t).min(1.0);
            let ha_t = (thirst + creep_t * t).min(1.0);
            let (af_h, af_t) = match want {
                EntityKind::Food => ((ha_h - relief).max(0.0), ha_t),
                EntityKind::Water => (ha_h, (ha_t - relief).max(0.0)),
                _ => (ha_h, ha_t),
            };
            // convex aggregate drive D(H) = h_h² + h_t² (n=2, m=1): worst need leads.
            let delta_d = ((ha_h * ha_h + ha_t * ha_t) - (af_h * af_h + af_t * af_t)).max(0.0);
            // trip survival from the learned hazard field along the route (cheap
            // two-point sample: midpoint + destination). exp form, multiplicative.
            let mid = Pos::new((pos.x + d.x) / 2, (pos.y + d.y) / 2);
            let exposure = self.danger_level(mid)
                + self.danger_level(d)
                + 2.0 * (predator_near(d) + predator_near(mid));
            let survive = (-DRR_KAPPA * exposure).exp();
            let score = delta_d * survive / t;
            if best.is_none_or(|(_, _, b)| score > b) {
                best = Some((id, d, score));
            }
        }
        best.map(|(id, p, _)| (id, p))
    }

    fn goal_target_pos(&self, kind: &GoalKind) -> Option<Pos> {
        let pos = self.world.me()?.pos;
        match kind {
            GoalKind::Forage => self
                .forage_drr
                .then(|| self.drr_target(EntityKind::Food))
                .flatten()
                .map(|(_, p)| p)
                .or_else(|| self.world.nearest_of(EntityKind::Food, pos).map(|b| b.entity.pos))
                .or_else(|| self.memory.nearest_place_of(EntityKind::Food, pos).map(|(_, p)| p)),
            GoalKind::Hydrate => self
                .forage_drr
                .then(|| self.drr_target(EntityKind::Water))
                .flatten()
                .map(|(_, p)| p)
                .or_else(|| self.world.nearest_of(EntityKind::Water, pos).map(|b| b.entity.pos))
                .or_else(|| self.memory.nearest_place_of(EntityKind::Water, pos).map(|(_, p)| p)),
            GoalKind::Investigate(id) | GoalKind::Socialize(id) => {
                self.world.belief(*id).map(|b| b.entity.pos)
            }
            _ => None,
        }
    }

    /// Resolve the concrete thing a goal is about: its label, where it is, and a
    /// related agent name — so narration can name real entities and places.
    fn phrase_target(&self, kind: &GoalKind) -> (Option<String>, Option<(i32, i32)>, Option<String>) {
        let pos = self.world.me().map(|m| m.pos).unwrap_or(Pos::new(0, 0));
        let from_belief = |id: daimon_core::EntityId| {
            self.world
                .belief(id)
                .map(|b| (b.entity.label.clone(), (b.entity.pos.x, b.entity.pos.y)))
        };
        match kind {
            GoalKind::Forage => self
                .world
                .nearest_of(EntityKind::Food, pos)
                .map(|b| (Some(b.entity.label.clone()), Some((b.entity.pos.x, b.entity.pos.y)), None))
                .unwrap_or((None, Some((pos.x, pos.y)), None)),
            GoalKind::Hydrate => self
                .world
                .nearest_of(EntityKind::Water, pos)
                .map(|b| (Some(b.entity.label.clone()), Some((b.entity.pos.x, b.entity.pos.y)), None))
                .unwrap_or((None, Some((pos.x, pos.y)), None)),
            GoalKind::Investigate(id) | GoalKind::Flee(id) | GoalKind::Confront(id) => {
                let (l, c) = from_belief(*id).unzip();
                (l, c, None)
            }
            GoalKind::Socialize(id) => {
                let name = self
                    .social
                    .model(*id)
                    .map(|m| m.name.clone())
                    .or_else(|| self.world.belief(*id).map(|b| b.entity.label.clone()));
                let coord = self.world.belief(*id).map(|b| (b.entity.pos.x, b.entity.pos.y));
                (name.clone(), coord, name)
            }
            GoalKind::Explore | GoalKind::Recover | GoalKind::Shelter | GoalKind::Provision => {
                (None, Some((pos.x, pos.y)), None)
            }
            // mourning names the dead friend (the continuing bond), so the narration
            // can reminisce about them by name.
            GoalKind::Mourn => {
                let who = self.grieving_for.and_then(|id| self.social.model(id)).map(|m| m.name.clone());
                (who.clone(), Some((pos.x, pos.y)), who)
            }
        }
    }
}

/// Infer another agent's likely drive from the nearest thing to them — a
/// behaviour-only theory of mind (we can't read their drives, only watch).
/// Infer another agent's intent from the *step it just took* — genuine
/// behaviour reading: they move toward what they want and away from what they
/// fear. Needs their previous position (so the first sighting tells us nothing).
fn infer_drive_from_movement(
    agent: &daimon_core::Entity,
    prev: Option<Pos>,
    visible: &[daimon_core::Entity],
) -> Option<Drive> {
    let prev = prev?;
    let now = agent.pos;
    if now == prev {
        return None; // standing still says little
    }
    // fleeing: a predator is near and they stepped *away* from it.
    if let Some(pred) = visible
        .iter()
        .find(|e| e.kind == EntityKind::Predator && e.pos.manhattan(now) <= 6)
    {
        if pred.pos.manhattan(now) > pred.pos.manhattan(prev) {
            return Some(Drive::Survival);
        }
    }
    // otherwise: the nearest thing they stepped *toward* reveals what they want.
    // resources (and curios) are clean evidence of intent; "stepped toward
    // another agent" is too noisy (agents cluster for many reasons), so we don't
    // read Social from movement.
    let toward = visible
        .iter()
        .filter(|e| {
            e.id != agent.id
                && matches!(e.kind, EntityKind::Food | EntityKind::Water | EntityKind::Curio)
        })
        .filter(|e| e.pos.manhattan(now) <= 4) // plausibly their actual target
        .filter(|e| e.pos.manhattan(now) < e.pos.manhattan(prev))
        .min_by_key(|e| e.pos.manhattan(now))?;
    Some(match toward.kind {
        EntityKind::Food => Drive::Hunger,
        EntityKind::Water => Drive::Thirst,
        EntityKind::Curio => Drive::Curiosity,
        _ => return None,
    })
}

/// Index of a drive in the canonical ordering (its basis index for qcog).
fn drive_index(d: Drive) -> usize {
    Drive::ALL.iter().position(|&x| x == d).unwrap_or(0)
}

/// The target entity of a goal, if it has one (for meta-motivation attribution).
fn goal_target_id(kind: &GoalKind) -> Option<daimon_core::EntityId> {
    match kind {
        GoalKind::Investigate(id) | GoalKind::Socialize(id) | GoalKind::Flee(id) => Some(*id),
        _ => None,
    }
}

/// Map an entity kind to its reserved associative-concept token.
fn kind_concept(kind: EntityKind) -> u32 {
    use daimon_core::assoc::concept;
    match kind {
        EntityKind::Predator => concept::PREDATOR,
        EntityKind::Food => concept::FOOD,
        EntityKind::Water => concept::WATER,
        EntityKind::Curio => concept::CURIO,
        EntityKind::Agent => concept::AGENT,
    }
}

/// The drive a goal *looks like* from the outside — used both to read other
/// agents' intent and to score theory-of-mind fairly.
fn goal_drive(kind: &GoalKind) -> Drive {
    match kind {
        GoalKind::Forage => Drive::Hunger,
        GoalKind::Hydrate => Drive::Thirst,
        GoalKind::Flee(_) | GoalKind::Confront(_) => Drive::Survival,
        GoalKind::Investigate(_) | GoalKind::Explore => Drive::Curiosity,
        GoalKind::Socialize(_) => Drive::Social,
        GoalKind::Recover | GoalKind::Shelter => Drive::Survival,
        // mourning is a social act (a severed bond), even as it looks like withdrawal.
        GoalKind::Mourn => Drive::Social,
        // provisioning is competence/foresight — building up a store of the future.
        GoalKind::Provision => Drive::Mastery,
    }
}

/// A rough cardinal/intercardinal direction for a delta — for spoken/insight text.
fn compass(dx: i32, dy: i32) -> &'static str {
    match (dx.signum(), dy.signum()) {
        (0, 0) => "here",
        (1, 0) => "east",
        (-1, 0) => "west",
        (0, 1) => "south",
        (0, -1) => "north",
        (1, 1) => "south-east",
        (1, -1) => "north-east",
        (-1, 1) => "south-west",
        (-1, -1) => "north-west",
        _ => "nearby",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daimon_core::{Entity, EntityId, SelfState};

    fn persona() -> Persona {
        Persona::new("Test")
    }

    fn percept(tick: u64, me: SelfState, visible: Vec<Entity>, events: Vec<WorldEvent>) -> Percept {
        Percept {
            tick,
            me,
            visible,
            events,
        }
    }

    fn ent(id: u32, kind: EntityKind, x: i32, y: i32) -> Entity {
        Entity {
            id: EntityId(id),
            kind,
            pos: Pos::new(x, y),
            label: "x".into(),
        }
    }

    #[test]
    fn close_predator_triggers_flee_reflex() {
        let mut mind = Mind::new(persona().with_boldness(0.0), 1);
        let me = SelfState::new(Pos::new(0, 0));
        let p = percept(1, me, vec![ent(9, EntityKind::Predator, 1, 0)], vec![]);
        let t = mind.cycle(&p);
        assert_eq!(t.process, Process::Reflex);
        assert!(matches!(t.goal, GoalKind::Flee(_)));
        assert!(matches!(t.action, Action::Move(_)));
    }

    #[test]
    fn hunger_drives_foraging_without_a_predator() {
        let mut mind = Mind::new(persona(), 2);
        let mut me = SelfState::new(Pos::new(0, 0));
        me.energy = 0.2; // very hungry
        let p = percept(1, me, vec![ent(1, EntityKind::Food, 3, 0)], vec![]);
        let t = mind.cycle(&p);
        assert!(matches!(t.goal, GoalKind::Forage | GoalKind::Recover));
        assert_ne!(t.process, Process::Reflex);
    }

    #[test]
    fn getting_hurt_is_remembered_and_learned() {
        let mut mind = Mind::new(persona(), 3);
        let me = SelfState::new(Pos::new(0, 0));
        let p = percept(
            1,
            me,
            vec![ent(9, EntityKind::Predator, 8, 0)],
            vec![WorldEvent::Hurt {
                id: EntityId(9),
                health: 0.2,
            }],
        );
        mind.cycle(&p);
        assert!(mind.memory().fact("predator").is_some());
        // the painful episode is highly salient and retained.
        assert!(mind.memory().episodes().any(|e| e.valence < -0.5));
    }

    #[test]
    fn life_is_reproducible_from_seed() {
        // two minds, same seed, same percept stream → identical action stream.
        let run = |seed: u64| {
            let mut mind = Mind::new(persona(), seed);
            let mut acts = Vec::new();
            for t in 1..30 {
                let me = SelfState::new(Pos::new(0, 0));
                let p = percept(t, me, vec![ent(5, EntityKind::Curio, 4, 2)], vec![]);
                acts.push(mind.cycle(&p).action);
            }
            acts
        };
        assert_eq!(run(99), run(99));
    }
}
