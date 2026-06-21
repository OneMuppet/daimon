//! The shared world that drives several real Daimon minds at once.
//!
//! Each agent here owns a genuine [`daimon_mind::Mind`]. The world does only
//! what `daimon-world` does for one agent, generalised to many sharing a space:
//! every cognitive tick it synthesises a [`Percept`] for each mind from a
//! start-of-tick snapshot (simultaneous update — nobody sees the future),
//! cycles the mind, and applies the returned [`Action`]. Cognition is entirely
//! the published crates; only the embodiment is new — which is exactly the
//! "give it a body / give it a society" step from the white paper.

use daimon_core::{
    Action, Dir, Drive, Entity, EntityId, EntityKind, Info, Percept, Pos, Rng, SelfState, WorldEvent,
};
use daimon_mind::{dialogue, Genome, Mind, Persona, Process, Thought};

// ─── LIFE-CYCLE tuning (Sprint 3, live-only) ────────────────────────────────
// All times are in logical ticks. These are balanced (with the diag) so births ≈
// deaths over the long run: the village turns over across generations without
// exploding to the cap or dying out.
/// Mean natural lifespan, and its ± spread (so a generation does not pass at once).
/// A long life gives each couple time to raise enough children to replace the pair
/// (≈2 survivors), which is what keeps the lineage from dwindling.
const LIFESPAN_MEAN: u64 = 9000;
const LIFESPAN_SPREAD: u64 = 3000;
/// How fast a child matures per tick (newborn → adult). 1/700 ≈ a ~600-tick
/// childhood from `NEWBORN_MATURITY` to adulthood — short relative to a lifespan so
/// a child becomes a breeding adult with most of its life ahead.
const MATURE_RATE: f32 = 1.0 / 700.0;
/// A newborn's maturity (rendered ~this fraction of adult size) and the maturity
/// at which a mind counts as a grown adult (eligible to pair-bond / reproduce).
const NEWBORN_MATURITY: f32 = 0.18;
const ADULT: f32 = 0.92;
/// Per-tick probability a courting single forms a pair-bond. Manhattan reach to a
/// candidate mate, and the minimum *mutual* warmth (theory-of-mind disposition)
/// required to pair. Tuned so most adults find a partner over a few hundred ticks
/// (a village where almost everyone pairs is what sustains the birth rate).
const PAIR_CHANCE: f32 = 0.08;
const MATE_RADIUS: i32 = 16;
const MATE_BOND_MIN: f32 = 0.02;
/// Per-tick probability a settled, eligible couple has a child, and the cooldown
/// before they may have another (so children come *occasionally*, spaced out).
/// Tuned (with lifespan + the settled gate) so each couple raises ≈2 surviving
/// children across its life — replacement, so the village turns over and holds.
const BIRTH_CHANCE: f32 = 0.05;
const BREED_COOLDOWN: u64 = 700;
/// Inheritance mutation step (smaller than search mutation — a lineage drifts, it
/// does not scatter).
const INHERIT_SIGMA: f32 = 0.04;
/// Family bond strength a child and parent hold for each other from birth — well
/// above the grief threshold, so losing family is genuinely mourned.
const FAMILY_BOND: f32 = 0.85;
/// Partner proximity: tug a partner together when they drift past this many cells
/// (but not when far apart — they have independent lives), with this per-tick
/// chance, up to a leash beyond which we leave them be.
const PARTNER_PULL: f32 = 0.30;
const PARTNER_LEASH: i32 = 18;

// --- society (Sprint 4) tunables. Society is a SLOW social process, so relations
// are re-evaluated only every SOCIETY_PERIOD ticks and each interaction nudges
// `affinity` by a small amount — alliances/rivalries build over minutes, not ticks. ---
/// Ticks between society re-evaluations (relations drift gradually, not per-tick).
const SOCIETY_PERIOD: u64 = 60;
/// Manhattan radius within which two minds count as "in contact" (peaceful mingling,
/// or a death-across-the-line if a death sits this close to another village's member).
const SOCIETY_CONTACT_R: i32 = 6;
/// A member further than this from its village centre is "away from home" and may be
/// tugged back, so villages hold distinct, tighter territory (the basis for contested
/// borders). Kept below the territory radius so neighbouring villages share an edge.
const VILLAGE_HOME_R: i32 = 12;
/// Per-evaluation chance a far-flung member is stepped one cell back toward home.
const VILLAGE_PULL: f32 = 0.55;
/// Centroid distance under which two villages are close enough to contest ground.
const SOCIETY_TERRITORY_R: i32 = 22;
/// A member counts as "on a contested border" with a rival village when it is within
/// this many extra cells of being as close to the rival's centre as to its own.
const CONTESTED_SLACK: i32 = 4;
/// Affinity gained per standing cross-village marriage, per evaluation (alliance pull).
/// Kept modest so a single marriage warms a pair but does not instantly cement an
/// alliance — relations have to be *earned* and can still be overwhelmed by conflict.
const MARRIAGE_PULL: f32 = 0.022;
/// Affinity gained per peaceful cross-village contact, per evaluation (good neighbours).
const CONTACT_PULL: f32 = 0.004;
/// Affinity lost per death across an inter-village line, per evaluation (sharp — a
/// loss near the border is the strongest single souring force, so a hard season can
/// flip warming neighbours into rivals).
const BORDER_DEATH_PUSH: f32 = 0.300;
/// Affinity lost per evaluation per CONTESTED-GROUND member: when two close, balanced
/// villages both crowd the strip of land between them, they compete for it — and the
/// heavier the overlap, the harder it sours, so a genuinely contested border drives a
/// pair toward open rivalry even as the odd marriage pulls the other way.
const CONTENTION_PUSH: f32 = 0.022;
/// Cap on |affinity| from the per-evaluation drift, so no relation saturates at the
/// rail — it always has room to SHIFT back (allies can cool, enemies can thaw).
const AFFINITY_SOFT_CAP: f32 = 0.85;
/// Fraction by which every relation relaxes toward neutral per evaluation, so allies
/// can fall out and enemies can reconcile (nothing is permanent).
const RELATION_DECAY: f32 = 0.020;

/// Minimal HSV→RGB (each channel 0..255) for spreading village identity hues evenly
/// around the colour wheel. `h,s,v` in `[0,1]`.
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match (i as i32).rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

/// A small pool of child given-names, picked deterministically off `life_rng`.
fn child_first_name(rng: &mut Rng) -> &'static str {
    const NAMES: [&str; 16] = [
        "Wren", "Sol", "Ember", "Fern", "Bram", "Iris", "Cassius", "Maren", "Hollis", "Juniper",
        "Oren", "Lark", "Tamsin", "Rook", "Vesper", "Linden",
    ];
    NAMES[rng.below(NAMES.len())]
}

/// The family (sur)name carried down a lineage: the last whitespace-token of a
/// parent's name, or the whole name if it has none. Keeps a bloodline legible.
fn family_name(parent_name: &str) -> String {
    parent_name.rsplit(' ').next().unwrap_or(parent_name).to_string()
}

/// One inhabitant: a mind, a body, and a little render state for smoothness.
pub struct Agent {
    pub id: EntityId,
    pub name: String,
    pub mind: Mind,
    pub body: SelfState,
    pub accent: u32, // 0xRRGGBB persona accent
    /// Interpolated grid position for smooth drawing.
    pub rx: f32,
    pub ry: f32,
    pub last: Option<Thought>,
    /// This agent's latest narration line, copied off the mind's reused buffer at
    /// think time (the buffer itself is overwritten each tick, so the renderer
    /// keeps its own owned copy for the inspector panel).
    pub inner: String,
    /// A short history of render positions for a glowing motion trail.
    pub trail: Vec<(f32, f32)>,
    /// Decays each frame; spikes when a reflex or deliberation fires.
    pub flash: f32,
    pub flash_kind: Process,
    /// Floating speech timer + text.
    pub say: Option<(String, f32)>,
    inbox: Vec<WorldEvent>,
    /// PERMADEATH: false once this mind has died. A dead mind is skipped in
    /// think/act, is not a valid forage/social/predator target, is not pickable,
    /// is not rendered (a small grave marks the spot), and does not count as
    /// living. It is NOT removed from `agents` — indices are load-bearing across
    /// the step loop. With mortality off, no agent ever dies, so every count and
    /// `agents.len()` is unchanged and seeded worlds stay bit-identical.
    pub alive: bool,
    /// The tick this mind died (for the fading grave marker), if it has.
    pub death_tick: Option<u64>,
    /// What killed it ("the stalker", "hunger"), for the death event + narration.
    pub death_cause: &'static str,

    // ---- life-cycle (Sprint 3) — only ever advanced behind the world's
    // `lifecycle` flag; on a non-lifecycle world these stay at their spawn
    // defaults and are read by nothing, so seeded worlds are byte-identical. ----
    /// The tick this mind was born. Founders are born at tick 0; children at the
    /// tick the world spawns them. `age = world.tick - born_tick`.
    pub born_tick: u64,
    /// Logical lifespan in ticks: past this age the mind dies a NATURAL death.
    /// Each mind draws a slightly different span off the lifecycle side-RNG so a
    /// generation does not pass all at once.
    pub lifespan: u64,
    /// Romantic PAIR-BOND: the `EntityId` of this mind's chosen partner, once a
    /// mutual lasting bond has formed. `None` while single. Distinct from (and
    /// stronger than) the theory-of-mind friendships.
    pub partner: Option<EntityId>,
    /// The two parents this mind was born to (empty for the founding generation).
    pub parents: Vec<EntityId>,
    /// Children born to this mind (and its partner). Grows as the pair reproduces.
    pub children: Vec<EntityId>,
    /// Maturity in `[0,1]`: 0 = newborn (rendered small), 1 = grown adult. Founders
    /// start at 1.0; children grow from ~0.18 to 1.0 over a childhood, and only a
    /// matured (≈ adult) mind may itself form a pair-bond and reproduce.
    pub maturity: f32,
    /// Cooldown tick before this pair may have another child (so a settled pair
    /// has children *occasionally*, not every tick).
    pub breed_ready_at: u64,
    /// This agent's cognitive GENOME and base persona — kept so a child can inherit
    /// a deterministic blend of both parents (genome crossover + small mutation,
    /// like the evolution `mutate`). Founders all carry the live showcase genome.
    /// Only read on a lifecycle world; never touched otherwise.
    pub genome: Genome,
    pub base_persona: Persona,

    // ---- society (Sprint 4) — only ever set/read behind the world's `society`
    // flag; on a non-society world this stays `None` and is read by nothing, so
    // seeded worlds are byte-identical. ----
    /// Which VILLAGE (settlement) this mind belongs to, by village index. Founders
    /// are clustered into villages by `set_society`; a child inherits its (first)
    /// parent's village so lineages stay together (kinship keeps a village coherent).
    /// `None` until the live world assigns it.
    pub village: Option<u8>,
}

pub struct Resource {
    pub id: EntityId,
    pub kind: EntityKind,
    pub pos: Pos,
    pub label: String,
    pub alive: bool,
    pub payload: f32,
    respawn_at: Option<u64>,
    pub pulse: f32,
}

pub struct Predator {
    pub id: EntityId,
    pub pos: Pos,
    pub rx: f32,
    pub ry: f32,
    cooldown_until: u64,
    move_period: u64,
    /// Per-bite damage scale (1.0 = the default lethal stalker). The live game
    /// softens this so death is occasional, not constant.
    bite: f32,
    /// The predator's **hunting-strategy genome**. `None` ⇒ the incumbent stalker
    /// behaviour, reproduced BYTE-IDENTICALLY (same code path, same RNG draws, same
    /// order) — so every existing test / AC / proof is unchanged. `Some(strategy)`
    /// is read only by the Red-Queen co-evolution experiment, which evolves a
    /// predator against the minds. See [`PredatorStrategy`].
    strategy: Option<PredatorStrategy>,
}

/// How the predator targets and chases — the evolvable side of the Red-Queen
/// co-evolution experiment. Five normalised genes in `[0,1]`, decoded into hunting
/// knobs. The **[`PredatorStrategy::default`]** decodes to the *exact* incumbent
/// stalker policy (aggro range 12, target the nearest agent, random-walk both when
/// idle and during cooldown), and [`GameWorld::step_predator`] keeps a single fused
/// code path so that when the strategy is the default — or absent — the RNG draws
/// and resulting trajectory are byte-identical to the original stalker.
///
/// Genes (index → knob):
/// * `0` **aggro range** — how far the predator detects/locks onto prey, decoded to
///   `[4, 28]` Manhattan cells (default 12 = the incumbent `AGGRO`).
/// * `1` **target mode** — `nearest` (<1/3), `weakest` (lowest health, 1/3..2/3),
///   `isolated` (most distant from its own nearest neighbour, ≥2/3). Default
///   `nearest`.
/// * `2` **persistence** — when `< 0.5` the predator random-walks during its
///   post-strike cooldown (the incumbent); when `≥ 0.5` it keeps pressing toward
///   its target through cooldown (a relentless stalker). Default off (incumbent).
/// * `3` **patrol vs random** — when out of aggro range and `≥ 0.5`, the predator
///   patrols deterministically toward the village hearth (an ambush at the commons)
///   instead of random-walking. Default off (random walk — the incumbent).
/// * `4` **speed** — biases the move cadence: `< 0.5` keeps the world's built
///   `move_period` (the incumbent); `≥ 0.5` makes the predator move every tick
///   (faster). Default off (incumbent cadence).
#[derive(Clone, Copy, Debug)]
pub struct PredatorStrategy {
    pub g: [f32; 5],
}

/// How the predator selects which agent to hunt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TargetMode {
    Nearest,
    Weakest,
    Isolated,
}

impl Default for PredatorStrategy {
    /// The incumbent stalker, in gene space: aggro 12 (`(12-4)/24 ≈ 0.333`), target
    /// nearest, no cooldown persistence, random-walk (no patrol), world cadence.
    fn default() -> Self {
        PredatorStrategy { g: [(12.0 - 4.0) / 24.0, 0.0, 0.0, 0.0, 0.0] }
    }
}

impl PredatorStrategy {
    /// Decoded aggro range ∈ `[4, 28]` (default 12).
    fn aggro(&self) -> i32 {
        (4.0 + self.g[0].clamp(0.0, 1.0) * 24.0).round() as i32
    }
    fn target_mode(&self) -> TargetMode {
        if self.g[1] < 1.0 / 3.0 {
            TargetMode::Nearest
        } else if self.g[1] < 2.0 / 3.0 {
            TargetMode::Weakest
        } else {
            TargetMode::Isolated
        }
    }
    fn persistent(&self) -> bool {
        self.g[2] >= 0.5
    }
    fn patrols(&self) -> bool {
        self.g[3] >= 0.5
    }
    fn fast(&self) -> bool {
        self.g[4] >= 0.5
    }

    /// Is this the incumbent (default) policy on the behaviour-affecting genes?
    /// When true, `step_predator` takes the original code path verbatim.
    fn is_incumbent(&self) -> bool {
        self.aggro() == 12
            && self.target_mode() == TargetMode::Nearest
            && !self.persistent()
            && !self.patrols()
            && !self.fast()
    }

    /// A uniformly random hunting genome (weak/random gen-0 init for the experiment).
    pub fn random(rng: &mut Rng) -> Self {
        PredatorStrategy { g: std::array::from_fn(|_| rng.next_f32()) }
    }

    /// Mutate each gene by a Gaussian step (reflection at the bounds), reusing the
    /// shared seeded RNG. Analogous to [`daimon_mind::Genome::mutate`] for minds.
    pub fn mutate(&self, sigma: f32, rng: &mut Rng) -> Self {
        let mut g = self.g;
        for x in &mut g {
            let step = sigma * pred_gaussian(rng);
            *x = pred_reflect01(*x + step);
        }
        PredatorStrategy { g }
    }

    /// Gene-frequency accessors used by the experiment's telemetry.
    pub fn pursues_relentlessly(&self) -> bool {
        self.persistent()
    }
    pub fn ambushes(&self) -> bool {
        self.patrols()
    }
    pub fn is_fast(&self) -> bool {
        self.fast()
    }
    pub fn aggro_range(&self) -> i32 {
        self.aggro()
    }
    pub fn targets_weakest(&self) -> bool {
        self.target_mode() == TargetMode::Weakest
    }
    pub fn targets_isolated(&self) -> bool {
        self.target_mode() == TargetMode::Isolated
    }
}

/// Standard-normal sample (Box–Muller) on the seeded RNG — mirrors the mind
/// genome's mutator so the predator side is deterministic too.
fn pred_gaussian(rng: &mut Rng) -> f32 {
    let u1 = rng.next_f32().max(1e-6);
    let u2 = rng.next_f32();
    (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
}

/// Reflect a value back into `[0,1]`.
fn pred_reflect01(mut x: f32) -> f32 {
    for _ in 0..4 {
        if x < 0.0 {
            x = -x;
        } else if x > 1.0 {
            x = 2.0 - x;
        } else {
            break;
        }
    }
    x.clamp(0.0, 1.0)
}

/// A VILLAGE (settlement): a named, coloured cluster of minds (Sprint 4 society).
/// A village is a spatial cluster of minds reinforced by KINSHIP — children inherit
/// their parent's village, so families/lineages stay together and the cluster
/// persists across generations even as individuals come and go. Its `center` is the
/// live centroid of its living members (it drifts as the village moves/grows), and
/// its identity (`name` + `hue`) is fixed at founding. Empty unless `society` is on.
#[derive(Clone, Debug)]
pub struct Village {
    /// Stable village index (its position in `world.villages`).
    pub id: u8,
    /// A warm display name for the settlement (e.g. "Thornhollow").
    pub name: String,
    /// 0xRRGGBB banner/identity colour — minds, buildings, and territory tint by it.
    pub hue: u32,
    /// Live centroid of the village's living members (recomputed each society tick).
    pub center: Pos,
    /// Living-member count at the last society tick (diag / render).
    pub population: usize,
}

/// The standing RELATION between two villages (Sprint 4 society). A single signed
/// `affinity` in `[-1, +1]` that EMERGES from interactions over time and shifts:
/// cross-village pair-bonds (intermarriage) and peaceful shared edges push it up;
/// crowded contested territory and a death across the line push it down; and it
/// decays slowly toward neutral so allies can fall out and enemies reconcile. The
/// signed value is bucketed into [`RelationKind`] for behaviour + render. There is
/// one `Relation` per unordered village pair `(a < b)`.
#[derive(Clone, Copy, Debug)]
pub struct Relation {
    pub a: u8,
    pub b: u8,
    /// Signed standing in `[-1, +1]`: positive = warm (toward alliance), negative =
    /// hostile (toward enmity). Starts at 0 (strangers) and drifts with interactions.
    pub affinity: f32,
}

/// The bucketed quality of an inter-village [`Relation`], derived from its signed
/// affinity. Used by render (link/marker colour) and the wariness nudge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationKind {
    /// Strongly positive — the villages intermarry / share territory peacefully.
    Allied,
    /// Mildly positive — on friendly terms.
    Friendly,
    /// Near zero — strangers / indifferent.
    Neutral,
    /// Mildly negative — competitive, wary.
    Rival,
    /// Strongly negative — hostile; their minds avoid each other.
    Enemy,
}

impl Relation {
    /// Bucket the signed affinity into a relation quality. Thresholds chosen so most
    /// pairs sit Neutral and only sustained interaction tips a pair to Allied/Enemy.
    pub fn kind(&self) -> RelationKind {
        match self.affinity {
            x if x >= 0.55 => RelationKind::Allied,
            x if x >= 0.18 => RelationKind::Friendly,
            x if x <= -0.55 => RelationKind::Enemy,
            x if x <= -0.18 => RelationKind::Rival,
            _ => RelationKind::Neutral,
        }
    }
}

pub struct GameWorld {
    pub w: i32,
    pub h: i32,
    pub sight: i32,
    pub tick: u64,
    pub agents: Vec<Agent>,
    pub resources: Vec<Resource>,
    pub predator: Predator,
    /// 0..1 wrap; drives the day/night mood.
    pub day: f32,
    /// Rolling log of inter-agent utterances (act-tag, text) for inspection/tests.
    pub spoken: Vec<(&'static str, String)>,
    /// Stigmergic pheromone field (w·h), deposited on productive routes and
    /// evaporating each tick — emergent worn paths. Zero unless agents are
    /// stigmergic, so non-stigmergic worlds stay bit-identical.
    pub pheromone: Vec<f32>,
    /// Times the stalker was driven off by ≥2 agents confronting it together, and
    /// times a lone striker was bitten — for observing whether collective defence
    /// *emerges* (never scripted).
    pub repels: u32,
    pub lone_strikes: u32,
    /// Wall blocks placed by agents — emergent shelters. Sparse; stays empty
    /// unless an agent has the build affordance (`can_build`), so non-building
    /// worlds are bit-identical.
    pub walls: std::collections::HashSet<Pos>,
    /// Set when `walls` changes so the renderer can rebuild its block buffer lazily.
    pub structures_dirty: bool,
    /// LIVE-ONLY harshness switch. Default `false` so every AC/proof/fitness run is
    /// byte-identical: starvation then floors at a low ebb (survivable privation —
    /// the death/grief design). When `true` the floor is removed so a mortal mind
    /// that cannot reach food/water starves all the way to 0 and dies. This is the
    /// point of the evolution mode: a mind that walls itself in fails to forage and
    /// is selected against — self-enclosure is never a safe stable state.
    pub lethal_starvation: bool,
    /// LIVE-ONLY metabolism scale. Default `1.0` so every AC/proof/fitness/harsh
    /// run is byte-identical. The generational evolution mode lowers this so the
    /// per-tick energy/hydration drain is gentler — with `lethal_starvation` still
    /// on, the weak still starve and die, but a meaningful elite (≈10-20%) survives
    /// a generation, giving selection real signal instead of a near-total wipe.
    pub metabolism_scale: f32,
    /// LIVE-ONLY starvation health-drain per tick once energy/hydration hit 0.
    /// Default `0.02` (unchanged for all harness paths). Evolution mode softens
    /// this so famine kills, but a touch slower, widening the survivor band so the
    /// best minds are visible and breedable.
    pub starve_health_drain: f32,
    /// OPEN WORLD switch. Default `false` so every AC/proof/fitness run is
    /// byte-identical: no seasons, no winter cold, normal respawn, an inert granary,
    /// and no new RNG draws. When `true` the year turns through four seasons —
    /// food stops spawning in winter and a cold energy drain bites (eased near the
    /// hearth) — and the granary + gather/store affordances come alive, so a mind
    /// that has provisioned can draw down its stores to survive the cold. The live
    /// game and `--evolve` mode turn it on.
    pub open_world: bool,
    /// The village granary / hearth: the shared food cache (a position + a level in
    /// "food units"). Provisioning minds Store surplus here in the good months; in
    /// winter a hungry mind adjacent to it auto-draws. The hearth position also eases
    /// the winter cold for anyone near it. Inert unless `open_world`.
    pub granary: Pos,
    /// How much food the village granary holds (drawn down through winter). Starts
    /// empty — it only fills if minds actually provision.
    pub granary_food: f32,
    /// OPEN-WORLD winter-cold multiplier. Default `1.0` (every harness/AC/proof path
    /// keeps it, so they are byte-identical). Only the `open_world`-gated winter cold
    /// drain reads it, so a closed world is unaffected regardless of its value. The
    /// POET prototype tunes it per environment to make winter milder or more brutal —
    /// the central "winter severity" curriculum knob — without forking the sim.
    pub open_world_cold_scale: f32,
    /// Harvestable trees (a position + a wood level that depletes when gathered and
    /// regrows in spring). Empty unless `open_world`, so closed worlds carry none.
    pub trees: Vec<Tree>,
    /// LIVE-ONLY MATERIALS ECONOMY switch. Default `false` so every AC/proof/fitness
    /// run is byte-identical: no material stocks, no quarrying, building is free (the
    /// original physics), and no new RNG draws. When `true` the village GATHERS wood
    /// (from trees) and stone (from quarry rocks) into a shared stockpile, and every
    /// wall the minds build CONSUMES from that stockpile — no materials, no building.
    /// Only the live showcase (`Game::new`) and `village_diag` flip it on. The build
    /// *cognition* in daimon-mind is untouched (the harness AC42 + walls_block_predator
    /// stay byte-identical); this gates only the *physics* of placement + harvest.
    pub materials_econ: bool,
    /// Village wood stockpile (units). Rises as minds gather from trees; drawn down
    /// as walls go up. Inert (always 0) unless `materials_econ`.
    pub wood_stock: f32,
    /// Village stone stockpile (units). Rises as minds quarry rocks; drawn down as
    /// walls go up. Inert unless `materials_econ`.
    pub stone_stock: f32,
    /// Quarry rocks — a position + a stone level that depletes when quarried and
    /// slowly replenishes. Empty unless `materials_econ`, so other worlds carry none.
    /// Seeded from a side-RNG so enabling the economy never perturbs the main stream.
    pub rocks: Vec<Rock>,
    /// LIVE-ONLY NATURAL ECOSYSTEM switch. Default `false` so every AC/proof/fitness
    /// run is byte-identical: no wolves, bears, or deer, no new RNG draws, the snapshot
    /// carries only the original single stalker. When `true` the island comes alive with
    /// a natural ecosystem — wolves roam in loose packs and hunt, a solitary bear roams
    /// slowly, and deer graze and flee. Wolves and bears are perceived by the minds as
    /// [`EntityKind::Predator`] (so the EXISTING flee cognition handles them — no new mind
    /// behaviour), deer are ambient. All wildlife is stepped off a dedicated side-RNG
    /// ([`wild_rng`]) so the main deterministic stream is never perturbed. Only the live
    /// showcase (`Game::new`) and `village_diag` flip it on.
    pub wildlife: bool,
    /// The wolf pack(s): grey pack hunters. Empty unless `wildlife`.
    pub wolves: Vec<Wolf>,
    /// The solitary bears. Empty unless `wildlife`.
    pub bears: Vec<Bear>,
    /// The deer herd — ambient grazing prey that flee predators. Empty unless `wildlife`.
    pub deer: Vec<Deer>,
    /// Dedicated wildlife RNG. ALL stochastic wildlife choices draw from here, never from
    /// the main `rng`, so enabling the ecosystem leaves every seeded mind trajectory
    /// byte-identical. Seeded from the world dims when `wildlife` is turned on.
    wild_rng: Rng,
    /// LIVE-ONLY LIFE-CYCLE switch (Sprint 3). Default `false` so every AC/proof/
    /// fitness run is byte-identical: minds never age, never pair-bond, never
    /// reproduce, never die of old age — `agents.len()` and every count are exactly
    /// the incumbent. When `true` the village becomes a living lineage: adults meet
    /// mates and form lasting pair-bonds, settled fed pairs occasionally have an
    /// INHERITED child (a new mind spawned into the world), children grow up, and
    /// elders pass of old age — births ≈ deaths so the village turns over across
    /// generations. ALL stochastic life-cycle choices draw from [`life_rng`], never
    /// the main `rng`. Only the live showcase (`Game::new`) / `village_diag` flip it.
    pub lifecycle: bool,
    /// Hard cap on the living population so the live world stays performant and
    /// deterministic — reproduction is suppressed once the village reaches it.
    pub pop_cap: usize,
    /// Dedicated life-cycle RNG. ALL mating / inheritance-mutation / lifespan /
    /// child-placement draws come from here so the main stream is never perturbed.
    /// Seeded from the world dims when `lifecycle` is turned on.
    life_rng: Rng,
    /// Running tallies for the diag (live-only): children ever born, natural
    /// (old-age) deaths, and pair-bonds ever formed.
    pub births: u32,
    pub natural_deaths: u32,
    pub pairings: u32,
    /// LIVE-ONLY SOCIETY switch (Sprint 4). Default `false` so every AC/proof/fitness
    /// run is byte-identical: minds carry no village, no inter-village relations form,
    /// no society RNG is drawn, and the `village_affinity` movement nudge never fires.
    /// When `true` the founding minds are clustered into distinct VILLAGES (kinship
    /// keeps each coherent across generations), and the villages drift to ALLIANCES
    /// (cross-village marriage, peaceful shared edges) and RIVALRIES/ENMITIES
    /// (contested territory, a death across the line) that EMERGE and SHIFT over time.
    /// ALL stochastic society choices draw from [`soc_rng`], never the main `rng`.
    /// Only the live showcase (`Game::new`) / society diag flip it on.
    pub society: bool,
    /// The villages (settlements). Empty unless `society`. Index == `Village::id`.
    pub villages: Vec<Village>,
    /// The inter-village relation matrix, one [`Relation`] per unordered pair
    /// `(a < b)`. Empty unless `society`. Drifts each society tick.
    pub relations: Vec<Relation>,
    /// Running tallies for the society diag (live-only): cross-village marriages
    /// formed, and deaths that occurred across an enemy/rival line.
    pub cross_marriages: u32,
    pub border_deaths: u32,
    /// Dedicated society RNG. ALL clustering / naming / society-jitter draws come from
    /// here so the main stream is never perturbed. Seeded when `society` is turned on.
    soc_rng: Rng,
    rng: Rng,
    next_id: u32,
}

/// A wolf: a lean grey pack hunter. Wolves move as a loose pack (cohere toward the
/// pack centroid + roam), lock onto the nearest prey (a mind or a deer) within an
/// aggro radius and press the attack, but back off when alone or hurt — so a wolf is
/// a real but survivable threat. Perceived by the minds as [`EntityKind::Predator`].
pub struct Wolf {
    pub id: EntityId,
    pub pos: Pos,
    /// Which loose pack this wolf belongs to (for cohesion). 0-based.
    pub pack: u8,
    /// Smoothed render position (lerps toward `pos` each frame).
    pub rx: f32,
    pub ry: f32,
    /// Facing heading in radians, for orienting the mesh as it moves.
    pub heading: f32,
    /// Spikes to 1.0 on a strike / scare, decays — the renderer flashes it.
    pub flash: f32,
    /// No bite until this tick (post-strike recovery).
    cooldown_until: u64,
}

/// A bear: big, brown, powerful, and solitary. Bears roam slowly, have a short aggro
/// radius (you have to be close to be in danger), but hit hard — a rare, serious
/// encounter. Perceived by the minds as [`EntityKind::Predator`].
pub struct Bear {
    pub id: EntityId,
    pub pos: Pos,
    pub rx: f32,
    pub ry: f32,
    pub heading: f32,
    pub flash: f32,
    cooldown_until: u64,
}

/// A deer: tan, antlered, ambient prey. Deer graze (wander near grass), flee any
/// predator (wolf, bear, or the stalker) within sight, and can be caught by wolves
/// and bears — when caught they despawn and a fresh deer respawns elsewhere (renewable
/// prey), so the herd is a standing part of the living world. Deer are NOT perceived by
/// the minds (ambient life, not a threat), so they add no new mind cognition.
pub struct Deer {
    pub id: EntityId,
    pub pos: Pos,
    pub rx: f32,
    pub ry: f32,
    pub heading: f32,
    /// `true` while actively fleeing (the renderer can pose it alert / bounding).
    pub fleeing: bool,
    pub flash: f32,
    /// When caught: hidden until this tick, then it respawns far from predators.
    respawn_at: Option<u64>,
}

/// A quarry rock / stone outcrop: a position + a stone level that depletes when
/// quarried and slowly replenishes (a renewable outcrop, not a finite mine). The
/// stone half of the materials economy; the renderer draws these as warm boulders.
pub struct Rock {
    pub pos: Pos,
    pub stone: f32,
    pub pulse: f32,
}

/// A harvestable tree: wood depletes when gathered and regrows in spring. Visual +
/// a light material source; the provisioning *loop* under test in v1 is food
/// (gather food surplus → granary → draw in winter). Wood-gated crafting is v2.
pub struct Tree {
    pub pos: Pos,
    pub wood: f32,
    pub pulse: f32,
}

/// The four seasons of the open-world year. Derived deterministically from the tick
/// clock — never from a wall clock or RNG.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

/// Ticks in one day (the `day` field wraps every `1/0.0016` ≈ 625 ticks).
pub const DAY_TICKS: u64 = 625;
/// Days per season — 2 by default would give an 8-day year, but a 2-day (1250-tick)
/// winter is longer than a believable autumn store can bridge, so the year is tuned
/// to **1 day per season** (a 4-day, 2500-tick year). A `~625-tick` winter is a real,
/// lethal squeeze the unprovisioned die in, yet short enough that a stocked granary
/// carries the village through — and an `--evolve` generation (6250 ticks) now spans
/// 2.5 winters, an even stronger selection pressure for provisioning.
pub const DAYS_PER_SEASON: u64 = 1;
/// Ticks in one season and one full year.
pub const SEASON_TICKS: u64 = DAY_TICKS * DAYS_PER_SEASON; // 1250
pub const YEAR_TICKS: u64 = SEASON_TICKS * 4; // 5000

fn personas() -> Vec<(Persona, u32)> {
    vec![
        (
            Persona::new("Kael").with_boldness(0.5).with_sociability(0.5).with_curiosity(0.5)
                .with_creed("I want to understand this place — and last long enough to."),
            0x6ea8ff,
        ),
        (
            Persona::new("Vell").with_boldness(0.5).with_sociability(0.3).with_curiosity(0.95)
                .with_creed("Everything here is a question I haven't answered yet."),
            0xb98cff,
        ),
        (
            Persona::new("Mira").with_boldness(0.4).with_sociability(0.95).with_curiosity(0.5)
                .with_creed("No one should have to face the stalker alone."),
            0xffb14e,
        ),
        (
            Persona::new("Sela").with_boldness(0.12).with_sociability(0.6).with_curiosity(0.4)
                .with_creed("Careful keeps you breathing. I watch before I step."),
            0x5fd6a0,
        ),
        (
            Persona::new("Roin").with_boldness(0.92).with_sociability(0.35).with_curiosity(0.6)
                .with_creed("Fear is a leash. I'd rather see what's out there."),
            0xff6a6a,
        ),
        (
            Persona::new("Bex").with_boldness(0.6).with_sociability(0.7).with_curiosity(0.7)
                .with_creed("Find the others, find the food, stay clever."),
            0xf0d24e,
        ),
    ]
}

/// Personas for a village of `n` — the six hand-written characters, then as many
/// procedurally-varied villagers as needed (deterministic from index), so the
/// society can scale past six for stress/scale tests without losing diversity.
fn gen_personas(n: usize) -> Vec<(Persona, u32)> {
    let mut v = personas();
    let names = ["Aria", "Doru", "Lio", "Nyx", "Pell", "Ravi", "Suri", "Tovi", "Wren", "Yara", "Zane", "Emi"];
    let hash = |s: usize| ((s.wrapping_mul(2654435761)) % 1000) as f32 / 1000.0;
    while v.len() < n {
        let k = v.len() - 6; // procedural index past the hand-written cast
        let name = if k < names.len() { names[k].to_string() } else { format!("V{k}") };
        let bold = 0.15 + 0.7 * hash(k * 3 + 1);
        let soc = 0.2 + 0.7 * hash(k * 3 + 2);
        let cur = 0.2 + 0.7 * hash(k * 3 + 3);
        // a distinct accent hue per procedural villager.
        let accent = {
            let r = (90.0 + 150.0 * hash(k * 7 + 1)) as u32;
            let g = (90.0 + 150.0 * hash(k * 7 + 2)) as u32;
            let b = (90.0 + 150.0 * hash(k * 7 + 3)) as u32;
            (r << 16) | (g << 8) | b
        };
        v.push((
            Persona::new(&name)
                .with_boldness(bold)
                .with_sociability(soc)
                .with_curiosity(cur)
                .with_creed("I make my own way here."),
            accent,
        ));
    }
    v.truncate(n);
    v
}

impl GameWorld {
    pub fn new(seed: u64, n_agents: usize) -> Self {
        // The genome-less default path: agents express `Mind::new`. The carried
        // genome is a baseline placeholder — never read, since this path is not a
        // lifecycle world. (Inheritance only ever runs behind `set_lifecycle`.)
        Self::build(seed, n_agents, (40, 26, 7), false, |persona, s| {
            (Mind::new(persona.clone(), s), Genome::baseline(), persona)
        })
    }

    /// Build a world whose agents all express the given cognitive [`Genome`] —
    /// the genome's escalation config and faculty switches apply to every agent,
    /// while persona deltas ride on top of each base character (so the cast stays
    /// diverse and the *architecture* is what varies). This is the seam the
    /// self-improvement pipeline optimises through.
    pub fn with_genome(seed: u64, n_agents: usize, genome: &Genome) -> Self {
        Self::build(seed, n_agents, (40, 26, 7), false, |persona, s| {
            (genome.express(&persona, s), genome.clone(), persona)
        })
    }

    /// Like [`with_genome`], but on a custom grid — so a village's *density* can
    /// be held constant as the population grows (a bigger island for more minds).
    /// `sight` is the perception radius in cells.
    pub fn with_genome_sized(seed: u64, n_agents: usize, genome: &Genome, w: i32, h: i32, sight: i32) -> Self {
        Self::build(seed, n_agents, (w, h, sight), false, |persona, s| {
            (genome.express(&persona, s), genome.clone(), persona)
        })
    }

    /// A **harsh** world: scarce resources (water especially) and an aggressive,
    /// fast stalker, so survival genuinely *costs* good policy. The fair world is
    /// too easy to drive evolution (everything passes); this gives the
    /// self-improvement search a real gradient to climb.
    pub fn with_genome_harsh(seed: u64, n_agents: usize, genome: &Genome) -> Self {
        Self::build(seed, n_agents, (40, 26, 7), true, |persona, s| {
            (genome.express(&persona, s), genome.clone(), persona)
        })
    }

    /// Harsh world on a custom grid — for the big-island live evolution mode, so a
    /// thousand minds get a density-matched island that is still deliberately
    /// scarce. Live-only; the default/harness constructors are untouched.
    pub fn with_genome_sized_harsh(
        seed: u64,
        n_agents: usize,
        genome: &Genome,
        w: i32,
        h: i32,
        sight: i32,
    ) -> Self {
        Self::build(seed, n_agents, (w, h, sight), true, |persona, s| {
            (genome.express(&persona, s), genome.clone(), persona)
        })
    }

    /// Harsh, sized world where **each agent expresses its own genome** (in agent
    /// order) — the substrate for the generational evolution mode, where every mind
    /// is a distinct (possibly mutated) genome. Live-only. The persona deltas still
    /// ride on top of each base character, so the cast stays diverse while the
    /// *architecture* is what varies per individual.
    pub fn with_genomes_sized_harsh(
        seed: u64,
        genomes: &[Genome],
        w: i32,
        h: i32,
        sight: i32,
    ) -> Self {
        let n = genomes.len();
        // a deterministic per-call index counter, since `express` is `Fn`.
        let idx = std::cell::Cell::new(0usize);
        Self::build(seed, n, (w, h, sight), true, move |persona, s| {
            let i = idx.get();
            idx.set(i + 1);
            let g = genomes[i.min(n.saturating_sub(1))].clone();
            (g.express(&persona, s), g, persona)
        })
    }

    fn build(
        seed: u64,
        n_agents: usize,
        dims: (i32, i32, i32),
        harsh: bool,
        express: impl Fn(Persona, u64) -> (Mind, Genome, Persona),
    ) -> Self {
        let (w, h, sight) = dims;
        let mut rng = Rng::new(seed);
        let mut next_id = 1u32;
        let new_id = |next: &mut u32| {
            let id = EntityId(*next);
            *next += 1;
            id
        };
        let rpos = |rng: &mut Rng| Pos::new(rng.below(w as usize) as i32, rng.below(h as usize) as i32);

        // Resource supply scales with the population so a *village has enough
        // wells for its people* — a fair world is a survivable one. With a ~24-tick
        // respawn, n+1 springs supply ≈ (n+1)/24 drinks/tick, comfortably above a
        // 6-agent thirst demand of ≈0.18/tick; 4 springs (the old fixed count) sat
        // *below* it, making survival structurally impossible for the village no
        // matter the policy. Anticipation/foraging still have to *earn* survival —
        // this only makes the test fair, not trivial.
        let pop = n_agents.max(1);
        // Fair world: supply > demand, so survival is earned but reachable. Harsh
        // world: deliberate scarcity (water tightest) so only anticipatory,
        // commons-sharing, risk-aware policies survive — a real fitness gradient.
        let (n_food, n_water) = if harsh {
            ((pop / 2).max(2), ((pop + 1) / 3).max(2))
        } else {
            (pop + 3, pop + 3)
        };
        let mut resources = Vec::new();
        for i in 0..n_food {
            resources.push(Resource {
                id: new_id(&mut next_id),
                kind: EntityKind::Food,
                pos: rpos(&mut rng),
                label: format!("berries-{i}"),
                alive: true,
                payload: 0.45,
                respawn_at: None,
                pulse: rng.next_f32(),
            });
        }
        for i in 0..n_water {
            resources.push(Resource {
                id: new_id(&mut next_id),
                kind: EntityKind::Water,
                pos: rpos(&mut rng),
                label: format!("spring-{i}"),
                alive: true,
                payload: 0.55,
                respawn_at: None,
                pulse: rng.next_f32(),
            });
        }
        let curio_names = ["monolith", "glyph", "humming stone", "old shrine", "strange bloom"];
        for i in 0..5 {
            resources.push(Resource {
                id: new_id(&mut next_id),
                kind: EntityKind::Curio,
                pos: rpos(&mut rng),
                label: curio_names[i % curio_names.len()].to_string(),
                alive: true,
                payload: 0.0,
                respawn_at: None,
                pulse: rng.next_f32(),
            });
        }

        let mut agents = Vec::new();
        for (i, (persona, accent)) in gen_personas(n_agents).into_iter().enumerate() {
            let pos = rpos(&mut rng);
            let name = persona.name.clone();
            let (mind, genome, base_persona) = express(persona, seed ^ (0x9e37 + i as u64 * 0x1111));
            agents.push(Agent {
                id: new_id(&mut next_id),
                name,
                mind,
                body: SelfState::new(pos),
                accent,
                rx: pos.x as f32,
                ry: pos.y as f32,
                last: None,
                inner: String::new(),
                trail: Vec::new(),
                flash: 0.0,
                flash_kind: Process::Routine,
                say: None,
                inbox: Vec::new(),
                alive: true,
                death_tick: None,
                death_cause: "",
                // Founders are grown adults born at tick 0 with no family yet; the
                // lifespan is a placeholder (overwritten with a varied draw when the
                // live world calls `set_lifecycle`). On a non-lifecycle world these
                // are never read — the age clock never advances.
                born_tick: 0,
                lifespan: u64::MAX,
                partner: None,
                parents: Vec::new(),
                children: Vec::new(),
                maturity: 1.0,
                breed_ready_at: 0,
                genome,
                base_persona,
                // unassigned until the live world calls `set_society` (clusters founders).
                village: None,
            });
        }

        let pid = new_id(&mut next_id);
        let ppos = rpos(&mut rng);
        let predator = Predator {
            id: pid,
            pos: ppos,
            rx: ppos.x as f32,
            ry: ppos.y as f32,
            cooldown_until: 0,
            // harsh world: the stalker moves every tick (twice as relentless).
            move_period: if harsh { 1 } else { 2 },
            bite: 1.0,
            // None ⇒ the incumbent stalker policy; the Red-Queen experiment sets it.
            strategy: None,
        };

        GameWorld {
            w,
            h,
            sight,
            tick: 0,
            agents,
            resources,
            predator,
            day: 0.15,
            spoken: Vec::new(),
            pheromone: vec![0.0; (w * h) as usize],
            repels: 0,
            lone_strikes: 0,
            walls: std::collections::HashSet::new(),
            structures_dirty: false,
            lethal_starvation: false,
            metabolism_scale: 1.0,
            starve_health_drain: 0.02,
            open_world: false,
            // the hearth sits at the village heart; inert until open_world.
            granary: Pos::new(w / 2, h / 2),
            granary_food: 0.0,
            open_world_cold_scale: 1.0,
            trees: Vec::new(),
            materials_econ: false,
            wood_stock: 0.0,
            stone_stock: 0.0,
            rocks: Vec::new(),
            wildlife: false,
            wolves: Vec::new(),
            bears: Vec::new(),
            deer: Vec::new(),
            // a placeholder seed; reseeded deterministically the moment wildlife is
            // turned on. Never drawn from while `wildlife` is false.
            wild_rng: Rng::new(0),
            // life-cycle: off by default (the incumbent). The flag, side-RNG, and
            // tallies are inert until `set_lifecycle(true)` reseeds + arms them.
            lifecycle: false,
            pop_cap: 0,
            life_rng: Rng::new(0),
            births: 0,
            natural_deaths: 0,
            pairings: 0,
            // society: off by default (the incumbent). The flag, side-RNG, villages,
            // and relation matrix are inert until `set_society(true)` reseeds + arms.
            society: false,
            villages: Vec::new(),
            relations: Vec::new(),
            cross_marriages: 0,
            border_deaths: 0,
            soc_rng: Rng::new(0),
            rng,
            next_id,
        }
    }

    /// Turn this into an OPEN WORLD: the year now turns through seasons (food stops
    /// in winter + a cold drain), and the granary + harvestable trees come alive.
    /// Called by the live game and `--evolve` mode only; the seeded harness paths
    /// never call it, so they stay byte-identical. Trees are seeded from a
    /// **separate** RNG derived from the world seed, so enabling the open world does
    /// not perturb the main simulation RNG stream (the determinism discipline).
    pub fn set_open_world(&mut self, on: bool) {
        self.open_world = on;
        if on && self.trees.is_empty() {
            // a grove of trees, scattered deterministically off a side RNG so the
            // main stream — and therefore every seeded trajectory — is untouched.
            let mut tr = Rng::new(0x0072_2EE5 ^ self.w as u64 ^ ((self.h as u64) << 16));
            let n_trees = ((self.w * self.h) / 90).clamp(6, 60) as usize;
            for _ in 0..n_trees {
                let pos = Pos::new(tr.below(self.w as usize) as i32, tr.below(self.h as usize) as i32);
                self.trees.push(Tree { pos, wood: 1.0, pulse: tr.next_f32() });
            }
        }
    }

    /// Turn on the LIVE-ONLY MATERIALS ECONOMY: minds gather wood (trees) + stone
    /// (quarry rocks) into a shared village stockpile, and every wall built consumes
    /// from it. Called by the live game and `village_diag` only; the seeded harness
    /// paths never call it, so they stay byte-identical. Trees AND rocks are seeded
    /// from a **separate** RNG derived from the world seed, so enabling the economy
    /// does not perturb the main simulation RNG stream (the determinism discipline).
    /// Seeds a starter stockpile so the village can begin raising buildings at once.
    pub fn set_materials_world(&mut self, on: bool) {
        self.materials_econ = on;
        if !on {
            return;
        }
        // Wood grove: the live game does not enable `open_world`, so trees are seeded
        // here too (idempotent — skipped if a grove already exists). Same side-RNG and
        // count as `set_open_world` so a world that turns on both is identical.
        if self.trees.is_empty() {
            let mut tr = Rng::new(0x0072_2EE5 ^ self.w as u64 ^ ((self.h as u64) << 16));
            let n_trees = ((self.w * self.h) / 90).clamp(6, 60) as usize;
            for _ in 0..n_trees {
                let pos = Pos::new(tr.below(self.w as usize) as i32, tr.below(self.h as usize) as i32);
                self.trees.push(Tree { pos, wood: 1.0, pulse: tr.next_f32() });
            }
        }
        // Quarry outcrops: a scatter of stone rocks, fewer than trees (stone is the
        // scarcer material), off their own side-RNG so neither stream is perturbed.
        if self.rocks.is_empty() {
            let mut rr = Rng::new(0x0057_0CE5 ^ self.w as u64 ^ ((self.h as u64) << 16));
            let n_rocks = ((self.w * self.h) / 220).clamp(4, 30) as usize;
            for _ in 0..n_rocks {
                let pos = Pos::new(rr.below(self.w as usize) as i32, rr.below(self.h as usize) as i32);
                self.rocks.push(Rock { pos, stone: 1.0, pulse: rr.next_f32() });
            }
        }
        // a starter stockpile so the first buildings can rise before the haul cycle
        // gets going (a freshly founded village arrives with some materials in hand).
        self.wood_stock = 24.0;
        self.stone_stock = 18.0;
    }

    /// Allocate a fresh entity id off the world counter (the same scheme the
    /// constructor uses), for live-only entities added after construction (wildlife).
    fn alloc_id(&mut self) -> EntityId {
        let id = EntityId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Turn on the LIVE-ONLY NATURAL ECOSYSTEM: wolves in loose packs, a solitary
    /// bear, and a deer herd. Called by the live game and `village_diag` only; the
    /// seeded harness paths never call it, so they stay byte-identical (no wildlife in
    /// the snapshot, no wildlife step, no new RNG draws on the main stream). All
    /// wildlife is seeded + stepped off a **dedicated side-RNG** ([`wild_rng`]) so the
    /// main simulation stream — and therefore every seeded mind trajectory — is
    /// untouched (the determinism discipline, exactly like the materials economy).
    ///
    /// Counts are tuned for a 124×84 island of 64 minds: small enough that the village
    /// thrives with the occasional grievable loss, not a bloodbath.
    pub fn set_wildlife(&mut self, on: bool) {
        self.wildlife = on;
        if !on {
            return;
        }
        // dedicated, dimension-derived seed so two same-sized worlds get the same
        // ecosystem, independent of how many main-stream draws have happened.
        self.wild_rng = Rng::new(0x0017_1DE5u64 ^ self.w as u64 ^ ((self.h as u64) << 20));
        // --- wolves: a couple of loose packs, scaled gently with map area ---
        if self.wolves.is_empty() {
            let n_packs: u8 = if (self.w * self.h) >= 6000 { 2 } else { 1 };
            // pack size 2-3 each (loose packs, a real but survivable threat).
            for pack in 0..n_packs {
                // a pack rallies around a seed cell, so its members start together.
                let seed = Pos::new(
                    self.wild_rng.below(self.w as usize) as i32,
                    self.wild_rng.below(self.h as usize) as i32,
                );
                let size = 2 + self.wild_rng.below(2); // 2 or 3
                for _ in 0..size {
                    let jx = self.wild_rng.below(7) as i32 - 3;
                    let jy = self.wild_rng.below(7) as i32 - 3;
                    let pos = self.clamp(Pos::new(seed.x + jx, seed.y + jy));
                    let id = self.alloc_id();
                    self.wolves.push(Wolf {
                        id,
                        pos,
                        pack,
                        rx: pos.x as f32,
                        ry: pos.y as f32,
                        heading: self.wild_rng.next_f32() * std::f32::consts::TAU,
                        flash: 0.0,
                        cooldown_until: 0,
                    });
                }
            }
        }
        // --- bears: solitary, RARE. One bear unless the island is enormous (a second
        // only past ~16k cells), so the 124×84 showcase carries a single bear — a rare,
        // serious encounter rather than a recurring threat. ---
        if self.bears.is_empty() {
            let n_bears = if (self.w * self.h) >= 16000 { 2 } else { 1 };
            for _ in 0..n_bears {
                let pos = Pos::new(
                    self.wild_rng.below(self.w as usize) as i32,
                    self.wild_rng.below(self.h as usize) as i32,
                );
                let id = self.alloc_id();
                self.bears.push(Bear {
                    id,
                    pos,
                    rx: pos.x as f32,
                    ry: pos.y as f32,
                    heading: self.wild_rng.next_f32() * std::f32::consts::TAU,
                    flash: 0.0,
                    cooldown_until: 0,
                });
            }
        }
        // --- deer: an ambient herd, scaled with area (the world should feel alive) ---
        if self.deer.is_empty() {
            let n_deer = ((self.w * self.h) / 700).clamp(6, 16) as usize;
            for _ in 0..n_deer {
                let pos = Pos::new(
                    self.wild_rng.below(self.w as usize) as i32,
                    self.wild_rng.below(self.h as usize) as i32,
                );
                let id = self.alloc_id();
                self.deer.push(Deer {
                    id,
                    pos,
                    rx: pos.x as f32,
                    ry: pos.y as f32,
                    heading: self.wild_rng.next_f32() * std::f32::consts::TAU,
                    fleeing: false,
                    flash: 0.0,
                    respawn_at: None,
                });
            }
        }
    }

    /// Turn on the LIVE-ONLY LIFE-CYCLE (Sprint 3): aging, romantic pair-bonds,
    /// inherited children, and natural death of old age. Called by the live game and
    /// `village_diag` only; the seeded harness paths never call it, so they stay
    /// byte-identical (no age clock, no pairing, no births, no natural deaths, no new
    /// RNG draws on the main stream). Everything stochastic draws from a dedicated
    /// side-RNG ([`life_rng`]), exactly the determinism discipline of the wildlife /
    /// materials systems.
    ///
    /// `pop_cap` bounds the living population so the world stays performant and
    /// deterministic — reproduction is suppressed once the village reaches it.
    pub fn set_lifecycle(&mut self, on: bool, pop_cap: usize) {
        self.lifecycle = on;
        if !on {
            return;
        }
        self.pop_cap = pop_cap.max(1);
        // dedicated, dimension-derived seed so two same-sized worlds get the same
        // lineage, independent of how many main-stream draws have happened.
        self.life_rng = Rng::new(0x0011_FE5Du64 ^ self.w as u64 ^ ((self.h as u64) << 24));
        // Give the founding generation varied lifespans so they do not all pass at
        // once — a spread around a mean so the turnover is gradual. Founders are
        // already adults (maturity 1.0, born at tick 0).
        for a in &mut self.agents {
            a.lifespan = Self::draw_lifespan(&mut self.life_rng);
            // stagger the founders' apparent age across most of a lifespan so the
            // founding cohort passes GRADUALLY (a continuous trickle of elders), not
            // as a synchronized die-off — that trickle is what the new generation has
            // time to replace. Pre-age up to ~80% of their span (so none is born
            // already dead, but some are near the end).
            let pre_age = self.life_rng.below((a.lifespan * 4 / 5).max(1) as usize) as u64;
            a.born_tick = 0u64.wrapping_sub(pre_age); // age = tick - born_tick = tick + pre_age
            // founders are ready to start a family soon (not all at once).
            a.breed_ready_at = self.life_rng.below(BREED_COOLDOWN as usize) as u64;
        }
    }

    /// Turn on the LIVE-ONLY EMERGENT SOCIETY (Sprint 4): the founding minds are
    /// clustered into `n_villages` distinct SETTLEMENTS, each with an identity (id +
    /// colour + name) and a live territory centroid. Thereafter inter-village
    /// ALLIANCES and RIVALRIES/ENMITIES EMERGE and SHIFT from how the villages
    /// interact (intermarriage + peaceful shared edges → allied; contested territory
    /// + a death across the line → rival/enemy), stepped each tick in [`step_society`].
    ///
    /// Called by the live game and the society diag only; the seeded harness paths
    /// never call it, so they stay byte-identical (no villages, no relations, no
    /// society step, no new RNG draws on the main stream). ALL stochastic society
    /// choices draw from a dedicated, dimension-derived side-RNG ([`soc_rng`]) so the
    /// main simulation stream — and every seeded mind trajectory — is untouched
    /// (exactly the determinism discipline of the wildlife / life-cycle systems).
    ///
    /// Villages are formed by spatial clustering of the founders: pick `n_villages`
    /// seed centroids off the side-RNG, assign each founder to its nearest seed, then
    /// settle the centroids with a couple of Lloyd (k-means) passes. KINSHIP then
    /// keeps each village coherent across generations because a child inherits its
    /// parent's village (see `birth_child`).
    pub fn set_society(&mut self, on: bool, n_villages: usize) {
        self.society = on;
        if !on {
            return;
        }
        // dedicated, dimension-derived seed so two same-sized worlds get the same
        // society, independent of how many main-stream draws have happened.
        self.soc_rng = Rng::new(0x0042_50C5u64 ^ self.w as u64 ^ ((self.h as u64) << 12));
        let k = n_villages.clamp(1, 8);

        // --- positions of the founding minds (society is established at founding) ---
        let pts: Vec<Pos> = self.agents.iter().filter(|a| a.alive).map(|a| a.body.pos).collect();
        if pts.is_empty() {
            return;
        }
        // --- k seed centroids, scattered off the side-RNG across the island ---
        let mut centers: Vec<Pos> = (0..k)
            .map(|_| {
                Pos::new(
                    self.soc_rng.below(self.w as usize) as i32,
                    self.soc_rng.below(self.h as usize) as i32,
                )
            })
            .collect();
        // --- Lloyd's algorithm: assign-to-nearest, then recompute centroids. A few
        // passes settle the clusters into the actual spatial groups of minds. ---
        let mut assign = vec![0u8; pts.len()];
        for _pass in 0..4 {
            for (idx, p) in pts.iter().enumerate() {
                let mut best = (0u8, i32::MAX);
                for (ci, c) in centers.iter().enumerate() {
                    let d = p.manhattan(*c);
                    if d < best.1 {
                        best = (ci as u8, d);
                    }
                }
                assign[idx] = best.0;
            }
            for (ci, c) in centers.iter_mut().enumerate() {
                let (mut sx, mut sy, mut n) = (0i64, 0i64, 0i64);
                for (idx, p) in pts.iter().enumerate() {
                    if assign[idx] as usize == ci {
                        sx += p.x as i64;
                        sy += p.y as i64;
                        n += 1;
                    }
                }
                if n > 0 {
                    *c = Pos::new((sx / n) as i32, (sy / n) as i32);
                }
            }
        }

        // --- build the villages: a fixed identity (warm name + a spread-out hue) ---
        self.villages = (0..k)
            .map(|ci| Village {
                id: ci as u8,
                name: Self::village_name(&mut self.soc_rng, ci),
                hue: Self::village_hue(ci, k),
                center: centers[ci],
                population: 0,
            })
            .collect();

        // --- stamp each living founder with its village (children inherit it) ---
        let mut ai = 0usize;
        for a in self.agents.iter_mut().filter(|a| a.alive) {
            a.village = Some(assign[ai]);
            ai += 1;
        }

        // --- the relation matrix: one neutral (affinity 0) Relation per village pair ---
        self.relations.clear();
        for x in 0..k as u8 {
            for y in (x + 1)..k as u8 {
                self.relations.push(Relation { a: x, b: y, affinity: 0.0 });
            }
        }
        self.recompute_village_centers();
    }

    /// A warm, deterministic settlement name (drawn off the society side-RNG so the
    /// world stays reproducible). A small curated word-bank, indexed by a side-RNG
    /// draw, with the village ordinal kept distinct so two villages never collide.
    fn village_name(rng: &mut Rng, ordinal: usize) -> String {
        const ROOTS: [&str; 12] = [
            "Thorn", "Ash", "Ember", "Fern", "Stone", "Wynd", "Bramble", "Hollow", "Frost",
            "Lark", "Mire", "Oak",
        ];
        const TAILS: [&str; 8] =
            ["hollow", "reach", "fell", "wick", "stead", "vale", "barrow", "ford"];
        let r = ROOTS[(rng.below(ROOTS.len()) + ordinal) % ROOTS.len()];
        let t = TAILS[(rng.below(TAILS.len()) + ordinal) % TAILS.len()];
        format!("{r}{t}")
    }

    /// A distinct identity HUE per village, spread evenly around the colour wheel so
    /// villages read apart at a glance. 0xRRGGBB.
    fn village_hue(ordinal: usize, k: usize) -> u32 {
        // even hue spacing; high saturation, mid-high value so banners pop in the iso.
        let h = ordinal as f32 / k.max(1) as f32; // [0,1)
        let (r, g, b) = hsv_to_rgb(h, 0.62, 0.92);
        ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
    }

    /// Recompute each village's live centroid + population from its living members.
    /// Called when society is established and every society tick, so a village's
    /// territory centre tracks where its people actually are (it drifts as they move).
    fn recompute_village_centers(&mut self) {
        let k = self.villages.len();
        if k == 0 {
            return;
        }
        let mut sums = vec![(0i64, 0i64, 0usize); k];
        for a in self.agents.iter().filter(|a| a.alive) {
            if let Some(v) = a.village {
                let e = &mut sums[v as usize];
                e.0 += a.body.pos.x as i64;
                e.1 += a.body.pos.y as i64;
                e.2 += 1;
            }
        }
        for (vi, v) in self.villages.iter_mut().enumerate() {
            let (sx, sy, n) = sums[vi];
            v.population = n;
            if n > 0 {
                v.center = Pos::new((sx / n as i64) as i32, (sy / n as i64) as i32);
            }
        }
    }

    /// Tug village members who have strayed FAR from their own settlement a step back
    /// toward its centre, so each village stays a distinct place (a real territory).
    /// Only adult, far-flung members are tugged, and only a fraction each evaluation,
    /// so it reads as "home is over there" rather than a leash — foraging, courtship,
    /// and fleeing all still range freely. Live-only (off the society side-RNG), so it
    /// never perturbs a seeded trajectory; a no-op without villages.
    fn draw_to_village(&mut self) {
        let centers: Vec<Pos> = self.villages.iter().map(|v| v.center).collect();
        if centers.is_empty() {
            return;
        }
        for idx in 0..self.agents.len() {
            if !self.agents[idx].alive {
                continue;
            }
            let Some(v) = self.agents[idx].village else { continue };
            // newborns/children drift with parents; only grown members hold territory.
            if self.agents[idx].maturity < ADULT {
                continue;
            }
            let home = centers[v as usize];
            let pos = self.agents[idx].body.pos;
            // only when well outside the home territory, and only sometimes.
            if pos.manhattan(home) <= VILLAGE_HOME_R {
                continue;
            }
            if !self.soc_rng.chance(VILLAGE_PULL) {
                continue;
            }
            let step = Pos::new((home.x - pos.x).signum(), (home.y - pos.y).signum());
            let np = self.clamp(Pos::new(pos.x + step.x, pos.y + step.y));
            if !self.walls.contains(&np) {
                self.agents[idx].body.pos = np;
            }
        }
    }

    /// The society update (live-only): drift the inter-village relations from this
    /// tick's interactions, so ALLIANCES and RIVALRIES emerge and shift over time.
    /// A no-op (zero new RNG draws) when `society` is off, so every seeded harness
    /// trajectory is byte-identical.
    ///
    /// Each tick (cheaply, throttled), for every village pair we nudge `affinity`:
    /// * **+ intermarriage** — a standing pair-bond that crosses the two villages is
    ///   a strong, lasting tie (the classic alliance-by-marriage). Counted live.
    /// * **+ peaceful contact** — members of the two villages mingling near each other
    ///   without a death drifts them gently warmer (good neighbours).
    /// * **− crowding/contention** — when the two village centroids sit close AND both
    ///   are populous, they compete for the same ground/resources → cooler.
    /// * **− a death across the line** — a mind that dies with an other-village member
    ///   as its nearest neighbour is a loss across the border → sharply cooler.
    /// * **decay** — every pair relaxes a touch toward 0 each tick, so nothing is
    ///   permanent: allies can fall out and enemies can reconcile.
    fn step_society(&mut self) {
        if self.villages.len() < 2 {
            // a single village still tracks its centre, but has no relations to drift.
            if self.tick % SOCIETY_PERIOD == 0 {
                self.recompute_village_centers();
            }
            return;
        }
        // society is a SLOW social process — only re-evaluate every SOCIETY_PERIOD
        // ticks (cheap, and relations should drift over minutes, not flicker).
        if self.tick % SOCIETY_PERIOD != 0 {
            return;
        }
        self.recompute_village_centers();
        // keep each village spatially COHERENT: gently tug stray members back toward
        // their own settlement, so villages stay distinct places on the map (rather
        // than dissolving into one blob). This is what makes territory — and therefore
        // contested borders and genuine rivalries — possible. Live-only nudge (off the
        // side-RNG), so it perturbs no seeded trajectory.
        self.draw_to_village();
        let k = self.villages.len();

        // --- count cross-village pair-bonds (intermarriages), once per pair ---
        let mut marriages = vec![0u32; k * k];
        for a in self.agents.iter().filter(|a| a.alive) {
            let (Some(va), Some(pid)) = (a.village, a.partner) else { continue };
            if let Some(p) = self.agents.iter().find(|b| b.id == pid && b.alive) {
                if let Some(vb) = p.village {
                    if va != vb && a.id.0 < pid.0 {
                        marriages[va as usize * k + vb as usize] += 1;
                    }
                }
            }
        }

        // --- count peaceful cross-village contacts (members within a few cells) ---
        let mut contacts = vec![0u32; k * k];
        let living: Vec<(u8, Pos)> = self
            .agents
            .iter()
            .filter(|a| a.alive)
            .filter_map(|a| a.village.map(|v| (v, a.body.pos)))
            .collect();
        for i in 0..living.len() {
            for j in (i + 1)..living.len() {
                let (vi, pi) = living[i];
                let (vj, pj) = living[j];
                if vi != vj && pi.manhattan(pj) <= SOCIETY_CONTACT_R {
                    let (lo, hi) = (vi.min(vj), vi.max(vj));
                    contacts[lo as usize * k + hi as usize] += 1;
                }
            }
        }

        // --- border deaths since the last society tick: a mind that died recently
        // whose nearest LIVING neighbour is of another village = a loss across a line ---
        let mut deaths = vec![0u32; k * k];
        let recent = self.tick.saturating_sub(SOCIETY_PERIOD);
        for d in self.agents.iter().filter(|a| !a.alive) {
            let Some(dt) = d.death_tick else { continue };
            if dt < recent {
                continue;
            }
            let Some(vd) = d.village else { continue };
            // nearest living neighbour to the death site.
            let mut nearest: Option<(u8, i32)> = None;
            for n in self.agents.iter().filter(|a| a.alive) {
                if let Some(vn) = n.village {
                    let dist = n.body.pos.manhattan(d.body.pos);
                    if nearest.map(|(_, b)| dist < b).unwrap_or(true) {
                        nearest = Some((vn, dist));
                    }
                }
            }
            if let Some((vn, dist)) = nearest {
                if vn != vd && dist <= SOCIETY_CONTACT_R {
                    let (lo, hi) = (vd.min(vn), vd.max(vn));
                    deaths[lo as usize * k + hi as usize] += 1;
                    self.border_deaths += 1;
                }
            }
        }

        // --- territorial CONTENTION: for each close, balanced pair, count members who
        // sit in the CONTESTED MIDZONE (roughly between the two centres — nearly as
        // close to the rival's centre as to their own). Two villages whose people crowd
        // the same strip of ground are competing for it; the more contested members,
        // the harder the pair sours. This is independent of marriage, so a genuinely
        // contested border can drive a pair into open rivalry. ---
        let centers: Vec<Pos> = self.villages.iter().map(|v| v.center).collect();
        let pops: Vec<usize> = self.villages.iter().map(|v| v.population).collect();
        let mut contested = vec![0u32; k * k];
        for &(v, p) in &living {
            let own = centers[v as usize];
            let d_own = p.manhattan(own);
            for (o, oc) in centers.iter().enumerate() {
                if o == v as usize {
                    continue;
                }
                let dist_centers = own.manhattan(*oc);
                if dist_centers > SOCIETY_TERRITORY_R {
                    continue; // that village is too far to contest
                }
                // this member is "in the contested zone" if it sits about as near the
                // rival's centre as its own (it has wandered onto the border).
                if p.manhattan(*oc) <= d_own + CONTESTED_SLACK {
                    let (lo, hi) = (v.min(o as u8), v.max(o as u8));
                    contested[lo as usize * k + hi as usize] += 1;
                }
            }
        }

        // --- drift each pair's affinity from the tallies, then decay toward 0 ---
        for rel in &mut self.relations {
            let a = rel.a as usize;
            let b = rel.b as usize;
            let m = marriages[a * k + b] + marriages[b * k + a];
            let c = contacts[a.min(b) * k + a.max(b)];
            let d = deaths[a.min(b) * k + a.max(b)];
            // contested-ground members on this border, gated to a balanced standoff
            // (neither village markedly larger — otherwise it is absorption, not rivalry).
            let (pa, pb) = (pops[a], pops[b]);
            let balanced = pa > 0 && pb > 0 && pa.min(pb) * 5 >= pa.max(pb) * 2; // within ~2.5×
            let cz = if balanced { contested[a.min(b) * k + a.max(b)] } else { 0 };

            let mut delta = 0.0f32;
            delta += (m as f32) * MARRIAGE_PULL; // intermarriage → allied
            delta += (c.min(8) as f32) * CONTACT_PULL; // good neighbours → warmer
            delta -= (d as f32) * BORDER_DEATH_PUSH; // a loss across the line → hostile
            delta -= (cz.min(12) as f32) * CONTENTION_PUSH; // contested ground → rivalry
            rel.affinity = (rel.affinity + delta).clamp(-1.0, 1.0);
            // slow relax toward neutral so relations are never permanent (allies cool,
            // enemies thaw) — the larger the standing, the stronger the pull back.
            rel.affinity -= rel.affinity * RELATION_DECAY;
            // soft-cap so a relation never welds to the rail: it always keeps room to
            // SHIFT in either direction as interactions change.
            rel.affinity = rel.affinity.clamp(-AFFINITY_SOFT_CAP, AFFINITY_SOFT_CAP);
        }
    }

    /// The standing relation between two villages (in either order). `None` if either
    /// id is unknown or it is the same village. Live-only inspector / render / diag.
    pub fn relation_between(&self, a: u8, b: u8) -> Option<&Relation> {
        if a == b {
            return None;
        }
        let (lo, hi) = (a.min(b), a.max(b));
        self.relations.iter().find(|r| r.a == lo && r.b == hi)
    }

    /// A living mind's village, by agent index (live-only inspector / render).
    pub fn village_of(&self, i: usize) -> Option<&Village> {
        let v = self.agents.get(i)?.village?;
        self.villages.get(v as usize)
    }

    /// Whether the mind at index `i` should feel WARY of the mind at index `j`:
    /// they belong to villages whose relation is hostile (Rival/Enemy) AND the mind
    /// has the `village_affinity` gene on. Drives the avoidance nudge in `step`.
    /// Always false off a society world / gene off, so non-society worlds are inert.
    fn feels_wary_of(&self, i: usize, j: usize) -> bool {
        if !self.society || !self.agents[i].mind.village_affinity() {
            return false;
        }
        let (Some(vi), Some(vj)) = (self.agents[i].village, self.agents[j].village) else {
            return false;
        };
        if vi == vj {
            return false;
        }
        matches!(
            self.relation_between(vi, vj).map(|r| r.kind()),
            Some(RelationKind::Rival) | Some(RelationKind::Enemy)
        )
    }

    /// The position of the nearest mind that the mind at index `i` feels WARY of (an
    /// enemy/rival village's member) within [`SOCIETY_CONTACT_R`], or `None`. Drives
    /// the avoidance nudge in `step`. Always `None` off a society world / gene off, so
    /// non-society worlds draw nothing here and stay byte-identical.
    fn nearest_wary(&self, i: usize) -> Option<Pos> {
        if !self.society || !self.agents[i].mind.village_affinity() {
            return None;
        }
        let here = self.agents[i].body.pos;
        let mut best: Option<(Pos, i32)> = None;
        for j in 0..self.agents.len() {
            if j == i || !self.agents[j].alive {
                continue;
            }
            let d = self.agents[j].body.pos.manhattan(here);
            if d > SOCIETY_CONTACT_R {
                continue;
            }
            if self.feels_wary_of(i, j) && best.map(|(_, b)| d < b).unwrap_or(true) {
                best = Some((self.agents[j].body.pos, d));
            }
        }
        best.map(|(p, _)| p)
    }

    /// Draw a lifespan in ticks: a mean with a ± spread, so a generation does not
    /// pass all at once. Tuned (with the breeding rate + cap) so births ≈ deaths and
    /// the village turns over without exploding or dying out.
    fn draw_lifespan(rng: &mut Rng) -> u64 {
        // mean ≈ 4200 ticks, spread ±1400 → [2800, 5600].
        LIFESPAN_MEAN - LIFESPAN_SPREAD + rng.below((2 * LIFESPAN_SPREAD) as usize) as u64
    }

    /// This agent's current age in ticks (`tick - born_tick`). Founders pre-aged so
    /// the cast spans young adults to elders. Only meaningful on a lifecycle world.
    pub fn age_of(&self, a: &Agent) -> u64 {
        self.tick.wrapping_sub(a.born_tick)
    }

    /// The number of standing pair-bonds where BOTH partners are alive (each counted
    /// once). Live-only diag / inspector.
    pub fn pairbond_count(&self) -> usize {
        let mut n = 0usize;
        for a in self.agents.iter().filter(|a| a.alive) {
            if let Some(pid) = a.partner {
                // count once, from the lower id side, and only if the partner lives.
                if a.id.0 < pid.0 && self.agents.iter().any(|b| b.id == pid && b.alive) {
                    n += 1;
                }
            }
        }
        n
    }

    /// An agent's felt **happiness** for display, in `[0,1]`: the mind's own
    /// well-being readout (met needs / health / good feeling, dimmed by grief) lifted
    /// by the things a life-cycle gives it that the body alone does not — a living
    /// partner nearby, children, and safety (being sheltered / away from a predator).
    /// The mind computes the intrinsic well-being; the world layers on the social /
    /// family / safety context it owns. Falls back to the bare well-being when the
    /// `feel_happiness` gene is off (a flat neutral) or off a lifecycle world.
    pub fn happiness_of(&self, a: &Agent) -> f32 {
        let base = a.mind.happiness();
        if !self.lifecycle || !a.mind.feel_happiness() {
            return base;
        }
        let mut lift = 0.0f32;
        // a living partner within reach — companionship.
        if let Some(pid) = a.partner {
            if let Some(p) = self.agents.iter().find(|b| b.id == pid && b.alive) {
                lift += 0.10;
                if p.body.pos.manhattan(a.body.pos) <= 4 {
                    lift += 0.06; // and they are close right now
                }
            }
        }
        // living children — family.
        let kids_alive =
            a.children.iter().filter(|&&c| self.agents.iter().any(|b| b.id == c && b.alive)).count();
        lift += (kids_alive as f32 * 0.04).min(0.12);
        // safety: sheltered, or simply far from the stalker.
        if self.enclosure(a.body.pos) > 0.25 || a.body.pos.manhattan(self.predator.pos) > 8 {
            lift += 0.05;
        }
        (base + lift).clamp(0.0, 1.0)
    }

    /// Mean happiness across the living village, in `[0,1]`. Live-only diag /
    /// inspector. Reads each mind's display happiness ([`happiness_of`]).
    pub fn avg_happiness(&self) -> f32 {
        let n = self.living_count();
        if n == 0 {
            return 0.0;
        }
        self.agents.iter().filter(|a| a.alive).map(|a| self.happiness_of(a)).sum::<f32>() / n as f32
    }

    /// Mean age (in ticks) across the living village. Live-only diag.
    pub fn avg_age(&self) -> f32 {
        let n = self.living_count();
        if n == 0 {
            return 0.0;
        }
        let t = self.tick;
        self.agents
            .iter()
            .filter(|a| a.alive)
            .map(|a| t.wrapping_sub(a.born_tick) as f32)
            .sum::<f32>()
            / n as f32
    }

    /// The whole live-only life-cycle pass for one tick (see [`set_lifecycle`]). Runs
    /// only when `lifecycle` is on; all randomness is off `life_rng`.
    fn step_lifecycle(&mut self) {
        self.widow_check();
        self.grow_children();
        self.form_pairbonds();
        self.draw_partners_together();
        self.try_reproduce();
        self.age_and_pass();
    }

    /// Release a survivor whose partner has died — the continuing bond (and the grief)
    /// lives on in the mind's theory-of-mind, but the pair-bond *slot* is freed so the
    /// widow(er) may, in time, find a new partner and the village keeps turning over.
    /// (Without this, every loss permanently removes a breeder and the lineage stalls.)
    fn widow_check(&mut self) {
        let dead: std::collections::HashSet<EntityId> =
            self.agents.iter().filter(|a| !a.alive).map(|a| a.id).collect();
        if dead.is_empty() {
            return;
        }
        for a in &mut self.agents {
            if a.alive {
                if let Some(p) = a.partner {
                    if dead.contains(&p) {
                        a.partner = None;
                    }
                }
            }
        }
    }

    /// Children grow from newborn (`maturity ≈ 0.18`) to adult (`1.0`) over a
    /// childhood, then are full members of the village (eligible to pair-bond).
    fn grow_children(&mut self) {
        for a in &mut self.agents {
            if a.alive && a.maturity < 1.0 {
                a.maturity = (a.maturity + MATURE_RATE).min(1.0);
            }
        }
    }

    /// Single mature adults occasionally form a romantic PAIR-BOND with a nearby
    /// mind they already feel warmly toward (the strongest standing theory-of-mind
    /// friendship within reach). The bond is mutual and lasting: both `partner`
    /// fields are set and never casually dropped. A mind whose `can_mate` gene is off
    /// never seeks one, so a non-mating cast forms no pairs.
    fn form_pairbonds(&mut self) {
        // gather eligible singles (alive, mature, mating-capable, unpartnered).
        let eligible: Vec<usize> = (0..self.agents.len())
            .filter(|&i| {
                let a = &self.agents[i];
                a.alive && a.partner.is_none() && a.maturity >= ADULT && a.mind.can_mate()
            })
            .collect();
        if eligible.len() < 2 {
            return;
        }
        for &i in &eligible {
            // already paired this pass? (a partner set earlier in the loop)
            if self.agents[i].partner.is_some() {
                continue;
            }
            // only a few couples form per tick — courtship is occasional, not a stampede.
            if !self.life_rng.chance(PAIR_CHANCE) {
                continue;
            }
            let ipos = self.agents[i].body.pos;
            let iid = self.agents[i].id;
            // among reachable, still-single eligibles, pick the one toward whom THIS
            // mind feels warmest AND who is not kin (no pairing with parents/children/
            // siblings) — a mutual warm bond over a stranger.
            let mut best: Option<(usize, f32)> = None;
            for &j in &eligible {
                if j == i || self.agents[j].partner.is_some() {
                    continue;
                }
                let aj = &self.agents[j];
                if aj.body.pos.manhattan(ipos) > MATE_RADIUS {
                    continue;
                }
                if self.is_kin(i, j) {
                    continue;
                }
                // mutual warmth: the lower of the two dispositions must clear the bar.
                let d_ij = self.agents[i].mind.social().bond(aj.id);
                let d_ji = aj.mind.social().bond(iid);
                let mutual = d_ij.min(d_ji);
                if mutual < MATE_BOND_MIN {
                    continue;
                }
                if best.map(|(_, b)| mutual > b).unwrap_or(true) {
                    best = Some((j, mutual));
                }
            }
            if let Some((j, _)) = best {
                let jid = self.agents[j].id;
                self.agents[i].partner = Some(jid);
                self.agents[j].partner = Some(iid);
                self.pairings += 1;
                // an INTERMARRIAGE — a pair-bond that crosses two villages — is the
                // classic alliance-forming tie; tally it for the society diag. (Both
                // `village`s are `None` off a society world, so this never fires there.)
                if let (Some(vi), Some(vj)) = (self.agents[i].village, self.agents[j].village) {
                    if vi != vj {
                        self.cross_marriages += 1;
                    }
                }
            }
        }
    }

    /// Whether agents `i` and `j` are close kin (share a parent, or one is the
    /// other's parent/child) — so the village does not pair siblings or
    /// parent-with-child.
    fn is_kin(&self, i: usize, j: usize) -> bool {
        let (a, b) = (&self.agents[i], &self.agents[j]);
        if a.parents.contains(&b.id) || b.parents.contains(&a.id) {
            return true;
        }
        // shared parent ⇒ siblings.
        a.parents.iter().any(|p| b.parents.contains(p))
    }

    /// Partners prefer proximity: when a bonded pair drifts apart, gently step one
    /// partner a cell toward the other (off `life_rng`, occasionally) so couples read
    /// as *together* on screen. Live-only, so nudging a body here never perturbs any
    /// seeded harness trajectory.
    fn draw_partners_together(&mut self) {
        let n = self.agents.len();
        for i in 0..n {
            if !self.agents[i].alive {
                continue;
            }
            let Some(pid) = self.agents[i].partner else { continue };
            let Some(j) = self.agents.iter().position(|a| a.id == pid && a.alive) else { continue };
            if j < i {
                continue; // handle each pair once (the lower index drives)
            }
            let pa = self.agents[i].body.pos;
            let pb = self.agents[j].body.pos;
            let d = pa.manhattan(pb);
            if d <= 2 || d > PARTNER_LEASH {
                continue; // close enough, or too far (independent lives) to tug
            }
            if !self.life_rng.chance(PARTNER_PULL) {
                continue;
            }
            // step the partner (j) one cell toward i.
            let step = Pos::new((pa.x - pb.x).signum(), (pa.y - pb.y).signum());
            let np = self.clamp(Pos::new(pb.x + step.x, pb.y + step.y));
            if !self.walls.contains(&np) {
                self.agents[j].body.pos = np;
            }
        }
    }

    /// A settled, fed, sheltered, matured pair occasionally has a CHILD — a NEW mind
    /// spawned into the world whose genome + persona are inherited from both parents
    /// (uniform crossover + a small mutation, like the evolution `mutate`). The child
    /// is young (small, grows up), bonded to its parents (family), and counts toward
    /// the population cap. Suppressed at the cap so the village stays bounded.
    fn try_reproduce(&mut self) {
        if self.living_count() >= self.pop_cap {
            return;
        }
        // collect ready couples by lower index, so each pair is considered once.
        let mut couples: Vec<(usize, usize)> = Vec::new();
        for i in 0..self.agents.len() {
            let a = &self.agents[i];
            if !a.alive || !a.mind.can_reproduce() || a.maturity < ADULT {
                continue;
            }
            let Some(pid) = a.partner else { continue };
            let Some(j) = self.agents.iter().position(|x| x.id == pid) else { continue };
            if j <= i {
                continue;
            }
            couples.push((i, j));
        }
        for (i, j) in couples {
            if self.living_count() >= self.pop_cap {
                break;
            }
            if !self.couple_settled(i, j) {
                continue;
            }
            // both partners must be off cooldown.
            if self.tick < self.agents[i].breed_ready_at || self.tick < self.agents[j].breed_ready_at {
                continue;
            }
            if !self.life_rng.chance(BIRTH_CHANCE) {
                continue;
            }
            self.birth_child(i, j);
            self.agents[i].breed_ready_at = self.tick + BREED_COOLDOWN;
            self.agents[j].breed_ready_at = self.tick + BREED_COOLDOWN;
        }
    }

    /// Whether a couple is *settled enough* to raise a child: both alive, both in
    /// non-critical health, and neither in immediate danger. The felt preconditions
    /// of starting a family, kept achievable in this lean world (see below).
    fn couple_settled(&self, i: usize, j: usize) -> bool {
        // In this lean island the minds continuously forage and sit near a privation
        // health floor, so "well fed and hale" is rarely true for long — gating
        // births on high health stalls the lineage. Instead a couple is settled
        // enough to raise a child when both are simply alive and not in immediate
        // danger (not right next to a predator). The POPULATION CAP + the per-couple
        // breeding cooldown are what actually throttle the birth rate, so growth
        // tracks carrying capacity rather than over-running it.
        let ok = |a: &Agent| {
            a.alive
                && a.body.health > 0.10
                && a.body.pos.manhattan(self.predator.pos) > 4
        };
        ok(&self.agents[i]) && ok(&self.agents[j])
    }

    /// Spawn an inherited child mind for parents at indices `i`, `j`.
    fn birth_child(&mut self, i: usize, j: usize) {
        // --- inherit the genome: crossover both parents + small mutation, then force
        // the live life-cycle + mortality gene set on (so the child ages, mates,
        // reproduces, grieves, and can die just like its parents — a lineage that
        // keeps turning over). ---
        let mut child_genome =
            Genome::inherit(&self.agents[i].genome, &self.agents[j].genome, INHERIT_SIGMA, &mut self.life_rng);
        Self::arm_live_genes(&mut child_genome);

        // --- inherit the persona: blend the two parents' base traits + a small
        // jitter, and give the child a fresh family name so the lineage reads. ---
        let pa = &self.agents[i].base_persona;
        let pb = &self.agents[j].base_persona;
        let jit = |rng: &mut Rng| (rng.next_f32() - 0.5) * 0.12;
        let blend = |x: f32, y: f32, rng: &mut Rng| ((x + y) * 0.5 + jit(rng)).clamp(0.0, 1.0);
        let bold = blend(pa.boldness, pb.boldness, &mut self.life_rng);
        let soc = blend(pa.sociability, pb.sociability, &mut self.life_rng);
        let cur = blend(pa.curiosity, pb.curiosity, &mut self.life_rng);
        let child_name = format!("{} {}", child_first_name(&mut self.life_rng), family_name(&pa.name));
        let child_persona = Persona::new(&child_name)
            .with_boldness(bold)
            .with_sociability(soc)
            .with_curiosity(cur)
            .with_creed(pa.creed.clone().as_str());

        // express the child mind off a deterministic seed mixed from the parents +
        // the running birth count, so each child is a distinct individual.
        let seed = 0x0C111Du64
            ^ (self.agents[i].id.0 as u64)
            ^ ((self.agents[j].id.0 as u64) << 17)
            ^ ((self.births as u64) << 33);
        let mut child_mind = child_genome.express(&child_persona, seed);

        // the child already knows and is warmly bonded to both parents (family is the
        // first relationship): seed strong dispositions so a parent's later death is
        // grieved, and the child is not a stranger in the village.
        child_mind.bond_with(self.agents[i].id, &self.agents[i].name, FAMILY_BOND, self.tick);
        child_mind.bond_with(self.agents[j].id, &self.agents[j].name, FAMILY_BOND, self.tick);

        // place the child next to a parent.
        let near = self.agents[i].body.pos;
        let cpos = self.clamp(Pos::new(near.x + 1, near.y));
        let accent = self.agents[i].accent; // takes after the first parent's hue
        // a child belongs to a PARENT'S VILLAGE, so lineages stay in their settlement
        // across generations — kinship is what keeps a village coherent. For a
        // cross-village (intermarried) couple we alternate which parent's village the
        // child takes (by birth parity), so neither village is systematically drained
        // into the other — both factions persist. (`None` on a non-society world, so
        // this is inert there.)
        let village = {
            let (vi, vj) = (self.agents[i].village, self.agents[j].village);
            match (vi, vj) {
                (Some(a), Some(b)) if a != b => {
                    if self.births % 2 == 0 { vi } else { vj }
                }
                _ => vi.or(vj),
            }
        };
        let cid = self.alloc_id();

        // the parents (and the child) record the family tie.
        let (pid_i, pid_j) = (self.agents[i].id, self.agents[j].id);
        self.agents[i].children.push(cid);
        self.agents[j].children.push(cid);
        // parents are bonded to the newborn too (so a child's death would be grieved).
        self.agents[i].mind.bond_with(cid, &child_name, FAMILY_BOND, self.tick);
        self.agents[j].mind.bond_with(cid, &child_name, FAMILY_BOND, self.tick);

        self.agents.push(Agent {
            id: cid,
            name: child_name,
            mind: child_mind,
            body: SelfState::new(cpos),
            accent,
            rx: cpos.x as f32,
            ry: cpos.y as f32,
            last: None,
            inner: String::new(),
            trail: Vec::new(),
            flash: 0.0,
            flash_kind: Process::Routine,
            say: None,
            inbox: Vec::new(),
            alive: true,
            death_tick: None,
            death_cause: "",
            born_tick: self.tick,
            lifespan: Self::draw_lifespan(&mut self.life_rng),
            partner: None,
            parents: vec![pid_i, pid_j],
            children: Vec::new(),
            maturity: NEWBORN_MATURITY,
            breed_ready_at: 0, // set once grown
            genome: child_genome,
            base_persona: child_persona,
            village,
        });
        self.births += 1;
    }

    /// Advance every mind's age; when a mind that *ages* (its `can_age` gene on)
    /// passes its lifespan, it dies a PEACEFUL NATURAL death — health set to 0 with a
    /// distinct cause so `reap_dead` grieves it like any loss (family feels it most,
    /// via the strong family bond) and the renderer can give it a warm farewell.
    fn age_and_pass(&mut self) {
        let tick = self.tick;
        for a in &mut self.agents {
            if !a.alive || !a.mind.can_age() {
                continue;
            }
            let age = tick.wrapping_sub(a.born_tick);
            // a mind that has reached its lifespan dies a peaceful NATURAL death this
            // tick. We zero the body and stamp the cause "old age" (overwriting any
            // stale pending cause from a survived privation/scare — old age is the
            // true cause now). `reap_dead` (called right after) turns it into a
            // tombstone and broadcasts the loss so family grieves.
            if age >= a.lifespan && a.body.health > 0.0 {
                a.body.health = 0.0;
                a.death_cause = "old age";
                self.natural_deaths += 1;
            }
        }
    }

    /// Force the live game's full gene set ON for a genome (the cloned-and-flipped
    /// set from `Game::new`): building, mortality, grief, provisioning, and the four
    /// life-cycle genes. Used so inherited children keep the same faculties as the
    /// founders despite crossover/mutation drift. NEVER touches a baseline/showcase
    /// preset — only a child's own cloned genome on a live world.
    fn arm_live_genes(g: &mut Genome) {
        for idx in [21, 22, 23, 24, 29, 30, 31, 32, 33] {
            g.g[idx] = 1.0;
        }
    }

    /// The current [`Season`], derived deterministically from the tick clock. Outside
    /// an open world the year does not turn — it is always [`Season::Spring`] — so
    /// nothing seasonal ever fires and the world stays bit-identical.
    pub fn season(&self) -> Season {
        if !self.open_world {
            return Season::Spring;
        }
        match (self.tick / SEASON_TICKS) % 4 {
            0 => Season::Spring,
            1 => Season::Summer,
            2 => Season::Autumn,
            _ => Season::Winter,
        }
    }

    /// Ticks until winter next begins (for the foresight/anticipation faculty). Huge
    /// outside an open world (winter never comes). During winter itself it reports
    /// the ticks until the *following* winter, which is correct: the mind in winter
    /// is past anticipating it.
    pub fn winter_in(&self) -> f32 {
        if !self.open_world {
            return f32::MAX;
        }
        // winter is the 4th season (index 3): its start ticks are 3·SEASON_TICKS
        // within each year. Ticks from now to the next such boundary.
        let into_year = self.tick % YEAR_TICKS;
        let winter_start = 3 * SEASON_TICKS;
        let delta = if into_year <= winter_start {
            winter_start - into_year
        } else {
            YEAR_TICKS - into_year + winter_start
        };
        delta as f32
    }

    /// A `0..1` season phase for the renderer to tint by — keyed to the *real* sim
    /// season when open, so the snow falls exactly when food stops.
    pub fn season_phase(&self) -> f32 {
        if !self.open_world {
            return 0.0;
        }
        (self.tick % YEAR_TICKS) as f32 / YEAR_TICKS as f32
    }

    fn clamp(&self, p: Pos) -> Pos {
        Pos::new(p.x.clamp(0, self.w - 1), p.y.clamp(0, self.h - 1))
    }

    /// The living inhabitants — the only ones that think, act, are perceived as
    /// agents, and count as "minds alive". With mortality off this is every agent.
    pub fn living(&self) -> impl Iterator<Item = &Agent> {
        self.agents.iter().filter(|a| a.alive)
    }

    /// How many minds are still alive (the village's true population).
    pub fn living_count(&self) -> usize {
        self.agents.iter().filter(|a| a.alive).count()
    }

    /// How sheltered a cell is: the fraction of its 4 cardinal neighbours that are
    /// walls or the map edge (∈ [0,1]). This is the felt basis of safety/home —
    /// fully ringed = enclosed. The shelter need reads this; the planner climbs it.
    pub fn enclosure(&self, p: Pos) -> f32 {
        let mut walled = 0;
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let n = Pos::new(p.x + dx, p.y + dy);
            let edge = n.x < 0 || n.x >= self.w || n.y < 0 || n.y >= self.h;
            if edge || self.walls.contains(&n) {
                walled += 1;
            }
        }
        walled as f32 / 4.0
    }

    /// Index into the pheromone field for a (clamped) cell.
    pub fn pidx(&self, p: Pos) -> usize {
        let p = self.clamp(p);
        (p.y * self.w + p.x) as usize
    }

    /// During aimless exploration, the worn-path direction: the neighbour with the
    /// most pheromone above the current cell (stigmergic following). `None` if no
    /// neighbour is meaningfully stronger.
    fn worn_path_dir(&self, from: Pos) -> Option<Dir> {
        let here = self.pheromone[self.pidx(from)];
        let mut best: Option<(Dir, f32)> = None;
        for d in Dir::ALL {
            let v = self.pheromone[self.pidx(from.step(d))];
            if v > here + 1e-4 && best.is_none_or(|(_, b)| v > b) {
                best = Some((d, v));
            }
        }
        best.map(|(d, _)| d)
    }

    /// Ease the stalker for the live game so death is a meaningful, occasional
    /// event rather than constant churn: it moves a little less relentlessly and
    /// bites a little less deep, so a body has to be cornered repeatedly to fall.
    /// The village then persists long enough to bond — and then, now and then, to
    /// lose someone the others grieve. Tuning, not a behaviour change to cognition.
    pub fn soften_stalker(&mut self) {
        self.predator.move_period = 2; // the fair world's pace
        self.predator.bite = 0.95; // very nearly the fair world's full bite
    }

    /// POET-only stalker tuning: set the per-bite damage scale and movement period
    /// directly, so an environment's predator lethality is a curriculum knob. Pure
    /// setter on existing fields — no behaviour change for any harness path (they
    /// never call it, keeping `bite=1.0`/`move_period` as built).
    pub fn set_stalker(&mut self, bite: f32, move_period: u64) {
        self.predator.bite = bite;
        self.predator.move_period = move_period.max(1);
    }

    /// Install an evolvable **hunting strategy** on the predator (the Red-Queen
    /// co-evolution experiment). Setting the *default* strategy is behaviourally a
    /// no-op: `step_predator` detects the incumbent policy and takes the original
    /// code path verbatim, so the world stays byte-identical. Only a *non-default*
    /// strategy changes the hunt. The harness never calls this, so all ACs/proofs/
    /// tests keep `strategy: None` and are unchanged.
    pub fn set_predator_strategy(&mut self, strategy: PredatorStrategy) {
        self.predator.strategy = Some(strategy);
    }

    /// LIVE-ONLY tuning for the generational evolution mode. The bare harsh island
    /// wipes ~99% of 1000 minds in a generation — too thin an elite (≈11 survivors)
    /// to breed a gradient. This loosens the world just enough that a generation
    /// typically ends with ≈10-20% alive: still a fast, visible die-off that culls
    /// the weak hard, but with an elite big enough for selection to have signal.
    ///
    /// Three coordinated knobs (all live-only; the seeded harness/proofs/AC worlds
    /// never call this and keep the defaults, so they stay byte-identical):
    /// 1. **scarcity**: top up food/water so supply is tight but not famine-by-
    ///    construction (harsh alone gives `pop/2` food, `pop/3` water).
    /// 2. **metabolism**: slow the energy/hydration drain so a competent forager can
    ///    keep ahead of it, while a non-forager (e.g. a self-walling mind) still
    ///    empties and starves under `lethal_starvation`.
    /// 3. **stalker**: ease the predator a touch (it stays lethal, just less
    ///    relentless) so death is selective, not a coin-flip on spawn position.
    ///
    /// `lethal_starvation` stays ON (set by the caller): self-enclosure remains
    /// fatal — this only widens the survivor band, it does not save the weak.
    pub fn tune_for_evolution(&mut self) {
        // 1. scarcity: lift supply from the harsh ≈(pop/2, pop/3) up toward demand
        //    without reaching the fair-world surplus. Roughly one food per mind and
        //    ~0.7 water per mind: tight (water still the binding constraint) but
        //    foragable. We add the shortfall rather than rebuild, so positions stay
        //    deterministic from the same RNG stream.
        let pop = self.agents.len().max(1) as i32;
        let want_food = pop; // ≈1.0 food / mind
        let want_water = (pop * 7) / 10; // ≈0.7 water / mind (water stays tightest)
        let have_food = self.resources.iter().filter(|r| r.kind == EntityKind::Food).count() as i32;
        let have_water = self.resources.iter().filter(|r| r.kind == EntityKind::Water).count() as i32;
        for i in 0..(want_food - have_food).max(0) {
            let pos = Pos::new(self.rng.below(self.w as usize) as i32, self.rng.below(self.h as usize) as i32);
            let id = EntityId(self.next_id);
            self.next_id += 1;
            self.resources.push(Resource {
                id,
                kind: EntityKind::Food,
                pos,
                label: format!("berries+{i}"),
                alive: true,
                payload: 0.45,
                respawn_at: None,
                pulse: self.rng.next_f32(),
            });
        }
        for i in 0..(want_water - have_water).max(0) {
            let pos = Pos::new(self.rng.below(self.w as usize) as i32, self.rng.below(self.h as usize) as i32);
            let id = EntityId(self.next_id);
            self.next_id += 1;
            self.resources.push(Resource {
                id,
                kind: EntityKind::Water,
                pos,
                label: format!("spring+{i}"),
                alive: true,
                payload: 0.55,
                respawn_at: None,
                pulse: self.rng.next_f32(),
            });
        }
        // 2. metabolism: gentler drain so foraging can keep pace (the weak still
        //    starve — lethal_starvation has no floor).
        self.metabolism_scale = 0.55;
        self.starve_health_drain = 0.010;
        // 3. stalker: ease it (still lethal, less of a spawn-position lottery).
        self.soften_stalker();
    }

    /// POET-only scarcity tuning: force the world to hold exactly `food`/`water`
    /// resource patches. Added patches are placed deterministically off the world
    /// RNG (same discipline as `tune_for_evolution`); the tail is culled when over.
    /// Curios are untouched. No harness path calls this, so all default/AC/proof
    /// worlds keep their as-built resource counts.
    pub fn set_resource_counts(&mut self, food: usize, water: usize) {
        for (kind, want, label) in [
            (EntityKind::Food, food, "berries#"),
            (EntityKind::Water, water, "spring#"),
        ] {
            let have = self.resources.iter().filter(|r| r.kind == kind).count();
            if have < want {
                let payload = if kind == EntityKind::Food { 0.45 } else { 0.55 };
                for i in 0..(want - have) {
                    let pos = Pos::new(
                        self.rng.below(self.w as usize) as i32,
                        self.rng.below(self.h as usize) as i32,
                    );
                    let id = EntityId(self.next_id);
                    self.next_id += 1;
                    self.resources.push(Resource {
                        id,
                        kind,
                        pos,
                        label: format!("{label}{i}"),
                        alive: true,
                        payload,
                        respawn_at: None,
                        pulse: self.rng.next_f32(),
                    });
                }
            } else if have > want {
                // cull the surplus from the tail (keep the earliest-placed for
                // determinism), removing exactly `have - want` of this kind.
                let mut to_remove = have - want;
                let mut i = self.resources.len();
                while i > 0 && to_remove > 0 {
                    i -= 1;
                    if self.resources[i].kind == kind {
                        self.resources.remove(i);
                        to_remove -= 1;
                    }
                }
            }
        }
    }

    /// Player action: drop a fresh patch of food at a world cell.
    pub fn feed(&mut self, wx: f32, wy: f32) {
        let pos = self.clamp(Pos::new(wx.round() as i32, wy.round() as i32));
        let id = EntityId(self.next_id);
        self.next_id += 1;
        self.resources.push(Resource {
            id,
            kind: EntityKind::Food,
            pos,
            label: "gift".into(),
            alive: true,
            payload: 0.5,
            respawn_at: None,
            pulse: 0.0,
        });
    }

    /// The best still-open adjacent side to wall next: the cardinal direction whose
    /// neighbouring cell is in-bounds, empty (no wall / resource / agent), and so not
    /// yet counted toward enclosure — i.e. an open side that, once walled, raises
    /// [`enclosure`]. Returns `None` when fully enclosed or no buildable side exists.
    /// Deterministic order (N, S, E, W) so seeded worlds stay reproducible.
    pub fn shelter_gap(&self, p: Pos) -> Option<Dir> {
        for d in Dir::ALL {
            let n = p.step(d);
            let in_bounds = n.x >= 0 && n.x < self.w && n.y >= 0 && n.y < self.h;
            if !in_bounds {
                continue; // an edge already shelters this side — nothing to build
            }
            let occupied = self.walls.contains(&n)
                || self.resources.iter().any(|r| r.alive && r.pos == n)
                || self.agents.iter().any(|a| a.body.pos == n);
            if !occupied {
                return Some(d); // an open side the agent can wall to enclose itself
            }
        }
        None
    }

    /// Whether the village granary still has room for more stores (a soft cap so a
    /// well-stocked village stops hoarding). Scaled by population: more mouths to
    /// feed through winter ⇒ a bigger cache is worth filling.
    pub fn granary_capacity(&self) -> f32 {
        (self.agents.len() as f32 * 12.0).max(24.0)
    }

    /// The step a provisioning agent at `here` should take to *gather* more — toward
    /// the nearest living food (the surplus it will carry to the cache) — or `None`
    /// when the cache is already full (nothing worth gathering) or no food is in
    /// reach. Open-world only; returns `None` otherwise so closed worlds sense nothing.
    pub fn gather_dir(&self, here: Pos) -> Option<Dir> {
        // MATERIALS ECONOMY (live-only): when the village stockpile is below its target,
        // gathering means hauling building materials — steer toward the nearest tree (for
        // wood) or quarry rock (for stone), preferring whichever stock is shorter. Steers
        // to materials independently of `open_world` (the live showcase has no seasons).
        // Inert when the economy is off, so closed/harness worlds sense nothing here.
        if self.materials_econ {
            let want_wood = self.wood_stock < self.materials_target_wood();
            let want_stone = self.stone_stock < self.materials_target_stone();
            // when both are wanted, fetch the scarcer one first (relative to its target).
            let stone_first = want_stone
                && (!want_wood
                    || self.stone_stock / self.materials_target_stone()
                        <= self.wood_stock / self.materials_target_wood());
            let target = if stone_first {
                self.rocks
                    .iter()
                    .filter(|r| r.stone > 0.1)
                    .min_by_key(|r| r.pos.manhattan(here))
                    .map(|r| r.pos)
            } else if want_wood {
                self.trees
                    .iter()
                    .filter(|t| t.wood > 0.1)
                    .min_by_key(|t| t.pos.manhattan(here))
                    .map(|t| t.pos)
            } else {
                None
            };
            if let Some(target) = target {
                return if target == here { None } else { Some(here.toward(target)) };
            }
            // stockpile full (or no source left): nothing worth gathering.
            return None;
        }
        if !self.open_world || self.granary_food >= self.granary_capacity() {
            return None;
        }
        // gather from living food within a generous range (the grove of berry bushes).
        let target = self
            .resources
            .iter()
            .filter(|r| r.alive && r.kind == EntityKind::Food)
            .min_by_key(|r| r.pos.manhattan(here))
            .map(|r| r.pos)?;
        if target == here {
            None // standing on it — the planner's Gather resolves here
        } else {
            Some(here.toward(target))
        }
    }

    /// Target wood stock the village gathers toward (a soft cap, scaled by population so
    /// a bigger village stockpiles more to raise more buildings). Live-only.
    fn materials_target_wood(&self) -> f32 {
        (self.agents.len() as f32 * 2.0).max(40.0)
    }
    /// Target stone stock (stone is the scarcer material, so a lower target). Live-only.
    fn materials_target_stone(&self) -> f32 {
        (self.agents.len() as f32 * 1.4).max(28.0)
    }

    /// The step toward the granary — for an agent carrying a surplus to store, and
    /// (in winter) for a cold mind heading home to the hearth's warmth + stores.
    /// `None` when already in the hearth's feeding/warmth radius (no need to move) or
    /// outside an open world.
    pub fn store_dir(&self, here: Pos) -> Option<Dir> {
        if !self.open_world {
            return None;
        }
        // Within the hearth radius the mind is already warm + fed; only step home if
        // it has wandered outside it (in winter) or is not yet adjacent (to store).
        let winter = matches!(self.season(), Season::Winter);
        // As winter nears (the last stretch of autumn) and through winter, draw the
        // village home to the hearth (≤4) so it is sheltered and fed when the cold
        // lands — not caught out and frozen. Outside that window, store_dir only
        // homes a carried load (get adjacent, ≤1) so gathering ranges freely.
        let winter_near = winter || self.winter_in() <= 220.0;
        let home_radius = if winter_near { 4 } else { 1 };
        if here.manhattan(self.granary) <= home_radius {
            None
        } else {
            Some(here.toward(self.granary))
        }
    }

    /// Index of the agent nearest to a world coordinate within `radius` cells.
    pub fn pick_agent(&self, wx: f32, wy: f32, radius: f32) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (i, a) in self.agents.iter().enumerate() {
            if !a.alive {
                continue; // a grave is not pickable
            }
            let d = ((a.rx - wx).powi(2) + (a.ry - wy).powi(2)).sqrt();
            if d <= radius && best.map(|(_, bd)| d < bd).unwrap_or(true) {
                best = Some((i, d));
            }
        }
        best.map(|(i, _)| i)
    }

    /// All currently-perceivable entities (resources, agents, predator).
    fn snapshot(&self) -> Vec<Entity> {
        let mut v = Vec::with_capacity(self.resources.len() + self.agents.len() + 1);
        for r in &self.resources {
            if r.alive {
                v.push(Entity { id: r.id, kind: r.kind, pos: r.pos, label: r.label.clone() });
            }
        }
        for a in &self.agents {
            // the dead leave the perceivable world — not seen, not a social/forage
            // target. (With mortality off, every agent is alive, so unchanged.)
            if a.alive {
                v.push(Entity { id: a.id, kind: EntityKind::Agent, pos: a.body.pos, label: a.name.clone() });
            }
        }
        v.push(Entity {
            id: self.predator.id,
            kind: EntityKind::Predator,
            pos: self.predator.pos,
            label: "the stalker".into(),
        });
        // NATURAL ECOSYSTEM (live-only): wolves and bears are perceived as predators, so
        // the minds flee them through the EXISTING predator cognition — no new mind
        // behaviour. Deer are NOT added (ambient life, not a threat). Empty unless
        // `wildlife`, so the harness snapshot carries only the single stalker above and
        // stays byte-identical.
        if self.wildlife {
            for w in &self.wolves {
                v.push(Entity { id: w.id, kind: EntityKind::Predator, pos: w.pos, label: "a wolf".into() });
            }
            for b in &self.bears {
                v.push(Entity { id: b.id, kind: EntityKind::Predator, pos: b.pos, label: "a bear".into() });
            }
        }
        v
    }

    /// One cognitive tick across all minds (simultaneous update).
    pub fn step(&mut self) {
        self.tick += 1;
        self.day = (self.day + 0.0016).fract();
        let snap = self.snapshot();

        // collected cross-agent effects, applied after the loop.
        let mut consume: Vec<EntityId> = Vec::new();
        let mut tell: Vec<(EntityId, EntityId, Info)> = Vec::new(); // (listener, from, info)
        let mut strikers: Vec<usize> = Vec::new(); // agents who struck the stalker this tick

        // commons: each agent's current foraging claim (resource pos + urgency),
        // read from last tick's commitment so the update stays simultaneous.
        let claims: Vec<(EntityId, daimon_core::Pos, f32)> = self
            .agents
            .iter()
            .filter(|a| a.alive)
            .filter_map(|a| a.mind.forage_claim().map(|(p, u)| (a.id, p, u)))
            .collect();

        // culture: each agent's teachable affordance + its *prestige* (how well it
        // is doing — successful agents are worth learning from). Prestige-biased
        // transmission is the engine of cumulative culture.
        let teachers: Vec<(EntityId, daimon_core::Pos, f32, daimon_mind::Concept)> = self
            .agents
            .iter()
            .filter(|a| a.alive)
            .filter_map(|a| {
                a.mind.teachable_concept().map(|c| {
                    let b = a.body;
                    (a.id, b.pos, (b.health + b.energy + b.hydration) / 3.0, c)
                })
            })
            .collect();

        for i in 0..self.agents.len() {
            // the dead do not think, sense, or act — skip them entirely. (With
            // mortality off, every agent is alive, so this guard never fires and the
            // loop is bit-identical to before.)
            if !self.agents[i].alive {
                continue;
            }
            // SPATIAL SENSE OF SAFETY: tell the body how sheltered it is and where
            // the nearest open side to wall is, so the mind can feel exposed and act
            // on it. In a world without walls enclosure is 0 and the gap is just the
            // first open neighbour — but nothing reads these unless `can_build` is on,
            // so non-building worlds stay bit-identical.
            let here = self.agents[i].body.pos;
            self.agents[i].body.enclosure = self.enclosure(here);
            self.agents[i].body.shelter_gap = self.shelter_gap(here);
            // OPEN-WORLD interoception: hand the body the season, the countdown to
            // winter (for the foresight faculty), and where to gather / store. All
            // inert defaults when `open_world` is off (season 0, winter_in MAX, no
            // dirs), so nothing reads them and closed worlds stay bit-identical.
            self.agents[i].body.season = match self.season() {
                Season::Spring => 0,
                Season::Summer => 1,
                Season::Autumn => 2,
                Season::Winter => 3,
            };
            self.agents[i].body.winter_in = self.winter_in();
            self.agents[i].body.gather_dir = self.gather_dir(here);
            self.agents[i].body.store_dir = self.store_dir(here);
            let me = self.agents[i].body;
            let my_id = self.agents[i].id;
            // hand this agent everyone *else's* claims so it can yield/disperse.
            let others: Vec<(daimon_core::Pos, f32)> =
                claims.iter().filter(|(id, _, _)| *id != my_id).map(|(_, p, u)| (*p, *u)).collect();
            self.agents[i].mind.set_contention(others);
            // maybe learn a form's meaning from the most successful visible peer.
            // Guarded by is_cultural so non-cultural worlds draw no RNG here and
            // stay bit-identical (the harness depends on determinism).
            if self.agents[i].mind.is_cultural() {
                if let Some((_, _, _, c)) = teachers
                    .iter()
                    .filter(|(id, p, _, _)| *id != my_id && p.manhattan(me.pos) <= self.sight)
                    .max_by(|a, b| a.2.total_cmp(&b.2))
                {
                    if self.rng.chance(0.06) {
                        self.agents[i].mind.adopt_concept(c);
                    }
                }
            }
            let visible: Vec<Entity> = snap
                .iter()
                .filter(|e| e.id != my_id && e.pos.manhattan(me.pos) <= self.sight)
                .cloned()
                .collect();
            let events = std::mem::take(&mut self.agents[i].inbox);
            let percept = Percept { tick: self.tick, me, visible, events };

            let thought = self.agents[i].mind.cycle(&percept);
            // keep an owned copy of this tick's narration off the reused buffer.
            {
                let a = &mut self.agents[i];
                a.inner.clear();
                a.inner.push_str(a.mind.inner());
            }

            // flash on the expensive / instinctive paths
            match thought.process {
                Process::Reflex => {
                    self.agents[i].flash = 1.0;
                    self.agents[i].flash_kind = Process::Reflex;
                }
                Process::Deliberate => {
                    self.agents[i].flash = self.agents[i].flash.max(0.7);
                    self.agents[i].flash_kind = Process::Deliberate;
                }
                Process::Routine => {}
            }

            // resolve the action on my own body + collect external effects
            let stig = self.agents[i].mind.is_stigmergic();
            match &thought.action {
                Action::Move(d) => {
                    let mut dir = *d;
                    // stigmergy: while *aimlessly exploring* (curiosity leads), follow
                    // worn paths. Guarded by gene + curiosity + RNG so goal-directed
                    // movement and non-stigmergic worlds are untouched/bit-identical.
                    if stig
                        && self.agents[i].mind.drives().dominant().0 == Drive::Curiosity
                        && self.rng.chance(0.5)
                    {
                        if let Some(b) = self.worn_path_dir(me.pos) {
                            dir = b;
                        }
                    }
                    // SOCIETY WARINESS (live-only, gated by the village_affinity gene +
                    // a hostile inter-village relation): a mind keeps its distance from
                    // an ENEMY/RIVAL village's members. If the chosen step would carry
                    // it CLOSER to a nearby wary mind, deflect to whichever neighbour
                    // step keeps it farthest from that mind — a low-grade avoidance, not
                    // a war. Deterministic (no RNG) and inert off a society world, so
                    // every seeded harness trajectory is byte-identical.
                    if let Some(wp) = self.nearest_wary(i) {
                        let here = me.pos;
                        if self.clamp(here.step(dir)).manhattan(wp) < here.manhattan(wp) {
                            // pick the in-bounds, non-wall neighbour maximizing distance.
                            let mut best: Option<(Dir, i32)> = None;
                            for cand in Dir::ALL {
                                let cp = self.clamp(here.step(cand));
                                if self.walls.contains(&cp) {
                                    continue;
                                }
                                let score = cp.manhattan(wp);
                                if best.map(|(_, s)| score > s).unwrap_or(true) {
                                    best = Some((cand, score));
                                }
                            }
                            if let Some((bd, _)) = best {
                                dir = bd;
                            }
                        }
                    }
                    let np = self.clamp(me.pos.step(dir));
                    // Walls are solid: an agent cannot step into a wall cell (so a
                    // fully-walled agent has shut itself in). `walls` is empty unless
                    // someone has built, so non-building worlds are unaffected.
                    if !self.walls.contains(&np) {
                        self.agents[i].body.pos = np;
                        if stig {
                            let idx = self.pidx(np);
                            self.pheromone[idx] += 0.05; // a faint trail where feet fall
                        }
                    }
                }
                Action::Eat(id) => {
                    if let Some(r) = self.resources.iter().find(|r| r.id == *id) {
                        if r.alive && r.kind == EntityKind::Food && r.pos.manhattan(me.pos) <= 1 {
                            self.agents[i].body.energy = (me.energy + r.payload).min(1.0);
                            consume.push(*id);
                            self.agents[i].inbox.push(WorldEvent::Ate { id: *id, energy: r.payload });
                            if stig {
                                let idx = self.pidx(me.pos);
                                self.pheromone[idx] += 1.0; // a strong mark: food was here
                            }
                        }
                    }
                }
                Action::Drink(id) => {
                    if let Some(r) = self.resources.iter().find(|r| r.id == *id) {
                        if r.alive && r.kind == EntityKind::Water && r.pos.manhattan(me.pos) <= 1 {
                            self.agents[i].body.hydration = (me.hydration + r.payload).min(1.0);
                            self.agents[i].inbox.push(WorldEvent::Drank { id: *id });
                            if stig {
                                let idx = self.pidx(me.pos);
                                self.pheromone[idx] += 1.0; // water was here
                            }
                        }
                    }
                }
                Action::Talk { to, text } => {
                    // Say something with *content*: share a resource we know of,
                    // so the listener can act on it. This is how knowledge spreads.
                    let places: Vec<(EntityId, EntityKind, Pos, String)> = self.agents[i]
                        .mind
                        .memory()
                        .places()
                        .filter(|(_, p)| matches!(p.kind, EntityKind::Food | EntityKind::Water))
                        .map(|(id, p)| (id, p.kind, p.pos, p.label.clone()))
                        .collect();
                    // compose a varied utterance — a real speech act keyed to who
                    // we're addressing and what we know, not one canned line.
                    let from_id = self.agents[i].id;
                    let known_place = if places.is_empty() {
                        None
                    } else {
                        Some(places[self.rng.below(places.len())].clone())
                    };
                    let known_danger = self.agents[i].mind.worst_danger();
                    let listener_name = self
                        .agents
                        .iter()
                        .find(|a| a.id == *to)
                        .map(|a| a.name.clone())
                        .unwrap_or_else(|| "friend".into());
                    let ctx = dialogue::SpeakCtx {
                        listener: &listener_name,
                        times_met: self.agents[i].mind.social().model(*to).map(|m| m.interactions).unwrap_or(0),
                        disposition: self.agents[i].mind.social().disposition(*to),
                        known_place,
                        known_danger,
                    };
                    let utt = dialogue::compose(&mut self.rng, &ctx);
                    self.agents[i].say = Some((short_say(&utt.text), 2.2));
                    self.agents[i].inbox.push(WorldEvent::Spoke { to: *to, text: text.clone() });
                    if self.spoken.len() < 8000 {
                        self.spoken.push((utt.act.tag(), utt.text.clone()));
                    }
                    tell.push((*to, from_id, utt.info));
                    // we mention a couple more places in passing — knowledge
                    // travels in bunches, so it actually spreads through the group.
                    let mut extra = places.clone();
                    for k in (1..extra.len()).rev() {
                        extra.swap(k, self.rng.below(k + 1));
                    }
                    for (id, kind, pos, label) in extra.into_iter().take(4) {
                        tell.push((*to, from_id, Info::ResourceAt { id, kind, pos, label }));
                    }
                }
                Action::Inspect(_) => {
                    self.agents[i].say = Some(("…fascinating.".into(), 1.6));
                }
                Action::Strike(id) => {
                    // record a strike at the stalker if adjacent; the *outcome*
                    // (repelled, or bitten) is resolved collectively below.
                    if *id == self.predator.id && me.pos.manhattan(self.predator.pos) <= 1 {
                        strikers.push(i);
                        self.agents[i].flash = 1.0;
                        self.agents[i].flash_kind = Process::Reflex;
                    }
                }
                Action::Build(at) => {
                    // Place a wall on an adjacent, in-bounds, empty cell. Costs
                    // energy (build vs rest/forage trade-off). The *decision* to
                    // build is the mind's; here we only resolve the physics.
                    let adj = at.manhattan(me.pos) == 1;
                    let in_bounds = at.x >= 0 && at.x < self.w && at.y >= 0 && at.y < self.h;
                    let occupied = self.walls.contains(at)
                        || self.resources.iter().any(|r| r.alive && r.pos == *at)
                        || self.agents.iter().any(|a| a.body.pos == *at);
                    // MATERIALS GATE (live-only). When the economy is on, each wall
                    // consumes wood + stone from the village stockpile; with neither in
                    // store the wall cannot rise — no materials, no building. When the
                    // economy is OFF (every harness path) this is true unconditionally,
                    // so building is free exactly as before and the run is byte-identical.
                    const WALL_WOOD: f32 = 1.0;
                    const WALL_STONE: f32 = 0.7;
                    let have_materials = !self.materials_econ
                        || (self.wood_stock >= WALL_WOOD && self.stone_stock >= WALL_STONE);
                    if adj && in_bounds && !occupied && me.energy > 0.2 && have_materials {
                        if self.materials_econ {
                            self.wood_stock -= WALL_WOOD;
                            self.stone_stock -= WALL_STONE;
                        }
                        self.walls.insert(*at);
                        self.structures_dirty = true;
                        self.agents[i].body.energy = (me.energy - 0.06).max(0.0);
                        self.agents[i].flash = 0.6;
                        self.agents[i].flash_kind = Process::Routine;
                    }
                }
                Action::Gather => {
                    // Harvest a surplus into the body, IF in an open world and a
                    // living food source is adjacent/co-located. Carried provisions
                    // accumulate up to a load; gathering depletes the source (it will
                    // respawn). A no-op outside an open world (the affordance is inert
                    // there), so closed worlds are bit-identical. Also nibbles a tree
                    // for wood where one is adjacent — the v1 wood gather (crafting v2).
                    if self.open_world {
                        let near_food = self
                            .resources
                            .iter()
                            .position(|r| r.alive && r.kind == EntityKind::Food && r.pos.manhattan(me.pos) <= 1);
                        if let Some(ri) = near_food {
                            // take a portion as carried provisions; deplete the bush.
                            self.agents[i].body.carrying = (me.carrying + 0.3).min(1.0);
                            let id = self.resources[ri].id;
                            consume.push(id);
                            self.agents[i].flash = 0.5;
                            self.agents[i].flash_kind = Process::Routine;
                        }
                        // wood: harvest from an adjacent tree if any (depletes; regrows
                        // in spring). Light-touch in v1 — the granary is the heart and
                        // the loop under test is food provisioning.
                        if let Some(t) = self
                            .trees
                            .iter_mut()
                            .find(|t| t.wood > 0.1 && t.pos.manhattan(me.pos) <= 1)
                        {
                            t.wood = (t.wood - 0.34).max(0.0);
                            self.structures_dirty = true;
                        }
                    }
                    // MATERIALS ECONOMY (live-only): a Gather near a tree or quarry rock
                    // feeds the shared village stockpile that buildings draw from. Runs
                    // independently of `open_world` (the live showcase enables materials,
                    // not seasons); inert when the economy is off, so harness paths are
                    // byte-identical. One material unit roughly == one wall block.
                    if self.materials_econ {
                        if let Some(t) = self
                            .trees
                            .iter_mut()
                            .find(|t| t.wood > 0.1 && t.pos.manhattan(me.pos) <= 1)
                        {
                            // a swing's worth of wood depletes the tree and stocks the village.
                            let taken = t.wood.min(0.34);
                            t.wood -= taken;
                            self.wood_stock += taken * 3.0;
                            self.structures_dirty = true;
                            self.agents[i].flash = 0.5;
                            self.agents[i].flash_kind = Process::Routine;
                        } else if let Some(r) = self
                            .rocks
                            .iter_mut()
                            .find(|r| r.stone > 0.1 && r.pos.manhattan(me.pos) <= 1)
                        {
                            // quarrying a rock: harder yield than wood, the scarcer stock.
                            let taken = r.stone.min(0.30);
                            r.stone -= taken;
                            self.stone_stock += taken * 2.4;
                            self.structures_dirty = true;
                            self.agents[i].flash = 0.5;
                            self.agents[i].flash_kind = Process::Routine;
                        }
                    }
                }
                Action::Store => {
                    // Deposit carried provisions into the village granary when near it.
                    // The shared cache rises; the body's load empties. Open-world only.
                    if self.open_world
                        && me.carrying > 0.01
                        && me.pos.manhattan(self.granary) <= 1
                        && self.granary_food < self.granary_capacity()
                    {
                        self.granary_food = (self.granary_food + me.carrying).min(self.granary_capacity());
                        self.agents[i].body.carrying = 0.0;
                        self.agents[i].flash = 0.6;
                        self.agents[i].flash_kind = Process::Routine;
                        self.structures_dirty = true;
                    }
                }
                Action::Rest => {
                    self.agents[i].body.energy = (me.energy + 0.03).min(1.0);
                }
                Action::Wait => {}
            }

            self.agents[i].last = Some(thought);
        }

        // deliver utterances to listeners (heard next tick)
        for (listener, from, info) in tell {
            if let Some(a) = self.agents.iter_mut().find(|a| a.id == listener) {
                a.inbox.push(WorldEvent::Told { from, info });
            }
        }

        // consume eaten resources (respawn in place after a while). In an OPEN WORLD
        // the good seasons are a *season of plenty*: food regrows faster in
        // summer/autumn (abundance), so a competent forager easily keeps fed and the
        // pre-winter village is full — making WINTER (food stops + cold) the real, sole
        // killer, exactly the loop under test. Spring is normal; winter's frozen
        // bushes are handled in `respawn_due` (they stay pending). The RNG draw is the
        // SAME `self.rng.below(16)` either way, so the seeded stream is untouched and
        // closed worlds are bit-identical (the season offset is added afterward).
        let plentiful = self.open_world && matches!(self.season(), Season::Summer | Season::Autumn);
        for id in consume {
            let jitter = self.rng.below(16) as u64;
            // good seasons: a tighter base so resources come back quickly (plenty).
            let base = if plentiful { 8 } else { 16 };
            let at = self.tick + base + jitter;
            if let Some(r) = self.resources.iter_mut().find(|r| r.id == id) {
                r.alive = false;
                r.respawn_at = Some(at);
            }
        }

        // ── COLLECTIVE DEFENCE (world physics, not behaviour) ────────────────
        // The stalker yields to numbers: when two or more confront it together it
        // is driven off (and every confronter learns it worked); a lone striker is
        // simply bitten. Nothing here tells the agents to gather — this is only how
        // the world *responds* to being faced. Whether they rally is up to them.
        let near: Vec<usize> = strikers
            .iter()
            .copied()
            .filter(|&i| self.agents[i].body.pos.manhattan(self.predator.pos) <= 1)
            .collect();
        if near.len() >= 2 {
            let ap = self.predator.pos;
            let corners = [
                Pos::new(0, 0),
                Pos::new(self.w - 1, 0),
                Pos::new(0, self.h - 1),
                Pos::new(self.w - 1, self.h - 1),
            ];
            let far = corners.into_iter().max_by_key(|c| c.manhattan(ap)).unwrap_or(ap);
            self.predator.pos = far;
            self.predator.cooldown_until = self.tick + 24;
            self.repels += 1;
            let pid = self.predator.id;
            for &i in &near {
                self.agents[i].inbox.push(WorldEvent::Repelled { id: pid });
            }
        } else if near.len() == 1 {
            let i = near[0];
            let floor = if self.agents[i].mind.can_die() { 0.0 } else { 0.05 };
            self.agents[i].body.health = (self.agents[i].body.health - 0.15).max(floor);
            self.agents[i].death_cause = "the stalker";
            self.lone_strikes += 1;
            let pid = self.predator.id;
            self.agents[i].inbox.push(WorldEvent::Hurt { id: pid, health: 0.15 });
        }

        self.metabolism();
        self.step_predator();
        // reap any mortal agent whose health hit 0 this tick (from starvation or the
        // stalker), turn it into a tombstone, and tell the village so survivors can
        // grieve. A no-op when no agent is mortal.
        self.reap_dead();
        self.respawn_due();
        // NATURAL ECOSYSTEM (live-only): step wolves, bears, and deer off the dedicated
        // side-RNG. A no-op (and zero new RNG draws) when `wildlife` is off, so every
        // seeded harness trajectory is byte-identical. Wolf/bear bites route through the
        // SAME health/grief path as the stalker (so a kill is grieved); a second
        // `reap_dead` collects any mind a predator killed this tick.
        if self.wildlife {
            self.step_wildlife();
            self.reap_dead();
        }
        // LIFE-CYCLE (live-only): advance ages, grow children, form pair-bonds,
        // birth inherited children, and pass elders of old age — all off the
        // dedicated `life_rng`. A no-op (zero new RNG draws) when `lifecycle` is off,
        // so every seeded harness trajectory is byte-identical. A natural-death pass
        // routes through the SAME grief path as any other death (so an elder's
        // passing is grieved by family), then `reap_dead` collects them.
        if self.lifecycle {
            self.step_lifecycle();
            self.reap_dead();
        }
        // SOCIETY (live-only): drift the inter-village relations from this tick's
        // interactions (intermarriage / peaceful contact / contested ground / a death
        // across a line) so ALLIANCES and RIVALRIES emerge and shift. Throttled
        // internally (every SOCIETY_PERIOD ticks) and drawing zero RNG, it is a no-op
        // when `society` is off, so every seeded harness trajectory is byte-identical.
        if self.society {
            self.step_society();
        }
        // pheromone evaporates so worn paths reflect *recent* success and fade as
        // routes go stale (no RNG; a no-op when the field is all zero).
        for v in &mut self.pheromone {
            *v *= 0.97;
            if *v < 0.002 {
                *v = 0.0;
            }
        }
    }

    fn metabolism(&mut self) {
        let safe_radius = 3;
        let pred = self.predator.pos;
        let lethal = self.lethal_starvation;
        let met_scale = self.metabolism_scale;
        let starve_drain = self.starve_health_drain;
        // WINTER (open world only): a cold energy drain bites every tick, eased near
        // the hearth (the village heart / granary). This is the pressure that kills
        // the unprepared — a mind with no stores empties and, under lethal_starvation,
        // dies; a mind that provisioned can draw the cache back up below. Zero outside
        // an open world and outside winter, so closed/other-season worlds are
        // bit-identical. A side accumulator collects granary draws so the shared cache
        // (borrowed mutably below) is updated once after the per-agent loop.
        let winter = matches!(self.season(), Season::Winter);
        let hearth = self.granary;
        // POET winter-severity multiplier (1.0 for every harness path, so they are
        // byte-identical). Only scales the open-world winter cold below.
        let cold_scale = self.open_world_cold_scale;
        let cache_avail = self.granary_food;
        let mut cache_drawn = 0.0f32;
        for a in &mut self.agents {
            if !a.alive {
                continue; // the dead don't metabolise
            }
            // In winter a mind huddled at the hearth conserves: its ordinary
            // metabolism slows (it is resting, not ranging). This — together with the
            // hearth removing the cold — is what lets a modest autumn store bridge the
            // whole winter, so provisioning pays off. Away from the hearth, full
            // metabolism (you burn fuel out in the cold). Inert outside winter / a
            // closed world, so nothing changes there.
            // At the hearth in winter a mind all but hibernates — huddled, conserving
            // — so a modest autumn store reliably bridges the cold. Away from it,
            // ordinary metabolism (you burn fuel ranging in the open).
            let at_hearth = winter && a.body.pos.manhattan(hearth) <= 6;
            let met = if at_hearth { met_scale * 0.1 } else { met_scale };
            a.body.energy = (a.body.energy - 0.012 * met).clamp(0.0, 1.0);
            a.body.hydration = (a.body.hydration - 0.014 * met).clamp(0.0, 1.0);
            if winter {
                // cold bites the body. Being near the hearth (≤6 cells) or sheltered
                // (enclosed by walls) all but removes it — the warmth of the village
                // heart / a roof. Out in the open the cold is harsh. So a mind that
                // comes home pays almost nothing (the hearth's whole point); one caught
                // out in the cold bleeds energy fast. This is what makes "be at the
                // hearth in winter" — i.e. have provisioned a place to shelter — the
                // survival move, while the cache covers the shortfall.
                let near_hearth = a.body.pos.manhattan(hearth) <= 6;
                let warm = near_hearth || a.body.enclosure >= 0.5;
                // out in the open the cold is real but not instantly fatal — a mind
                // has time to path home to the hearth; at the hearth it is near-nil.
                let cold = (if warm { 0.002 } else { 0.008 }) * cold_scale;
                a.body.energy = (a.body.energy - cold).clamp(0.0, 1.0);
                // AUTO-DRAW from the granary: a hungry mind close enough to the cache
                // eats from the village stores. This is the payoff of provisioning —
                // the surplus laid by in autumn feeds the village through the cold.
                // Shared/Commons: first-come this tick draws from what was available.
                // The hearth feeds the village *around* it (the same ≤6 radius as the
                // warmth), so a mind that comes home to shelter is fed — it does not
                // have to stand on the exact cell. Without this radius the draw never
                // fires (foragers scatter) and even a full granary saves no one.
                // Draw a *small ration* — just enough to offset the winter drain and
                // hold the body steady — rather than a big gulp. A slow sip makes a
                // modest autumn store last the whole winter (a hoarded cache feeding a
                // resting village), so provisioning that actually happened is rewarded
                // with survival. The cache is the village's stored PROVISIONS (food and
                // water both), so the ration tops up whichever the body is short on.
                let needs_supply = a.body.energy < 0.7 || a.body.hydration < 0.7;
                if needs_supply && a.body.pos.manhattan(hearth) <= 6 {
                    let ration = 0.05f32; // ≈ one mind's per-tick winter upkeep
                    let take = ration.min((cache_avail - cache_drawn).max(0.0));
                    if take > 0.0 {
                        if a.body.energy < 0.7 {
                            a.body.energy = (a.body.energy + take).min(1.0);
                        }
                        if a.body.hydration < 0.7 {
                            a.body.hydration = (a.body.hydration + take).min(1.0);
                        }
                        cache_drawn += take;
                    }
                }
            }
            let safe = a.body.pos.manhattan(pred) > safe_radius;
            if safe && a.body.health < 1.0 {
                a.body.health = (a.body.health + 0.004).min(1.0);
            }
            if a.body.energy <= 0.0 || a.body.hydration <= 0.0 {
                // STARVATION is survivable privation, not a quick wipe: a mortal body
                // floors at a low ebb on hunger/thirst alone (it weakens but clings
                // on), so famine doesn't depopulate the village by itself. DEATH is
                // the stalker's doing — a *weakened* body caught in the open is what
                // falls (the predator strike is unfloored). This makes loss an
                // occasional, dramatic predator event (David's vision), not a slow
                // famine. (Immortal bodies floor at 0.05 below, unchanged.)
                if a.mind.can_die() {
                    // LETHAL mode (evolution): no floor — starvation runs to 0 and
                    // the body dies, so failing to forage (e.g. walling oneself in)
                    // is fatal and selected against. Default mode keeps the 0.12 ebb
                    // so the believability village isn't depopulated by famine.
                    //
                    // OPEN-WORLD WINTER is *also* lethal, independent of the global
                    // lethal switch: a mortal body that runs empty IN WINTER dies (no
                    // floor) — this is the cold killing the unprepared, the whole point
                    // of provisioning. In the good seasons the survivable 0.12 ebb
                    // holds, so the village isn't culled by ordinary privation and
                    // winter is the clean differentiator (a provisioned mind draws the
                    // hearth's stores and never runs empty; an unprovisioned one does).
                    let winter_kill = winter && a.body.pos.manhattan(hearth) > 6;
                    let floor = if lethal || winter_kill { 0.0 } else { 0.12 };
                    a.body.health = (a.body.health - starve_drain).max(floor);
                } else {
                    a.body.health = (a.body.health - starve_drain).max(0.0);
                }
                a.death_cause = "hunger and thirst";
            }
            // IMMORTAL incumbent: health floors at 0.05 unconditionally, exactly as
            // before — so non-mortal worlds are byte-identical. When the agent CAN
            // die (the gene), this floor is gone, so a body mauled to 0 by the
            // stalker actually dies (reaped after the predator step).
            if !a.mind.can_die() {
                a.body.health = a.body.health.max(0.05);
            }
        }
        // apply the winter draws to the shared cache (after the borrow above ends).
        if cache_drawn > 0.0 {
            self.granary_food = (self.granary_food - cache_drawn).max(0.0);
            self.structures_dirty = true;
        }
    }

    /// Turn any living-but-zero-health mortal agent into a tombstone, and broadcast
    /// a [`WorldEvent::Died`] to every *other* living agent so the village perceives
    /// the loss (grief is the survivors' response, modelled in cognition). Does
    /// nothing when no agent is mortal — so non-mortal worlds emit no events and
    /// stay bit-identical. The dead are NOT removed from `agents` (indices are
    /// load-bearing); they become tombstones via `alive = false`.
    fn reap_dead(&mut self) {
        let tick = self.tick;
        let mut fallen: Vec<(EntityId, Pos, &'static str)> = Vec::new();
        for a in &mut self.agents {
            if a.alive && a.mind.can_die() && a.body.health <= 0.0 {
                a.alive = false;
                a.death_tick = Some(tick);
                if a.death_cause.is_empty() {
                    a.death_cause = "the stalker";
                }
                a.say = None;
                fallen.push((a.id, a.body.pos, a.death_cause));
            }
        }
        for (id, pos, cause) in fallen {
            for a in &mut self.agents {
                if a.alive && a.id != id {
                    a.inbox.push(WorldEvent::Died { id, pos, cause: cause.to_string() });
                }
            }
        }
    }

    fn step_predator(&mut self) {
        // INCUMBENT FAST-PATH. With no strategy (the harness) or the default strategy
        // (the Red-Queen control), run the ORIGINAL stalker verbatim — same target
        // rule, same RNG draws, same order — so every test/AC/proof is byte-identical.
        let incumbent = match self.predator.strategy {
            None => true,
            Some(s) => s.is_incumbent(),
        };
        if incumbent {
            self.step_predator_incumbent();
            return;
        }
        // EVOLVED PATH — only reached by a non-default Red-Queen predator strategy.
        self.step_predator_evolved();
    }

    /// The original stalker policy, untouched. Kept as its own method so the fast-path
    /// above is provably identical to the pre-experiment code.
    fn step_predator_incumbent(&mut self) {
        // target the nearest *living* agent (the dead are not prey)
        let target = self
            .agents
            .iter()
            .filter(|a| a.alive)
            .min_by_key(|a| a.body.pos.manhattan(self.predator.pos))
            .map(|a| (a.id, a.body.pos));
        let Some((tid, tpos)) = target else { return };
        let p = self.predator.pos;

        if p.manhattan(tpos) == 0 {
            self.strike(tid);
            return;
        }
        let new = if self.tick < self.predator.cooldown_until {
            let d = Dir::ALL[self.rng.below(4)];
            self.clamp(p.step(d))
        } else if self.tick.is_multiple_of(self.predator.move_period) {
            const AGGRO: i32 = 12;
            if p.manhattan(tpos) <= AGGRO {
                p.step(p.toward(tpos))
            } else {
                let d = Dir::ALL[self.rng.below(4)];
                self.clamp(p.step(d))
            }
        } else {
            p
        };
        // Walls block the stalker too: it cannot move into (or through) a wall cell,
        // so an agent that has fully walled itself in is genuinely unreachable —
        // survival-need → wall-self-in → survive is a real closed loop. `walls` is
        // empty unless someone built, so non-building worlds are bit-identical (the
        // RNG draws above are unchanged; only this guard, always-true there, is new).
        let new = if self.walls.contains(&new) { p } else { new };
        self.predator.pos = new;
        if new == tpos {
            self.strike(tid);
        }
    }

    /// The EVOLVED hunting policy — reached only when a non-default
    /// [`PredatorStrategy`] is installed (the Red-Queen experiment). Mirrors the
    /// incumbent's structure (target → on-top strike → move → wall-guard → reach
    /// strike) but reads the strategy's genes for target selection, aggro range,
    /// cooldown persistence, patrol vs random-walk, and speed. Never runs on a
    /// harness world (those keep `strategy: None`).
    fn step_predator_evolved(&mut self) {
        let s = self.predator.strategy.expect("evolved path needs a strategy");
        let p = self.predator.pos;

        // --- target selection by strategy gene ---
        let living: Vec<(EntityId, Pos, f32)> = self
            .agents
            .iter()
            .filter(|a| a.alive)
            .map(|a| (a.id, a.body.pos, a.body.health))
            .collect();
        if living.is_empty() {
            return;
        }
        let target = match s.target_mode() {
            TargetMode::Nearest => living
                .iter()
                .min_by_key(|(_, pos, _)| pos.manhattan(p))
                .copied(),
            TargetMode::Weakest => living
                .iter()
                // lowest health; tie-break by proximity for determinism
                .min_by(|a, b| {
                    a.2.total_cmp(&b.2)
                        .then_with(|| a.1.manhattan(p).cmp(&b.1.manhattan(p)))
                })
                .copied(),
            TargetMode::Isolated => living
                .iter()
                // the agent whose nearest neighbour is farthest (most isolated);
                // tie-break by proximity to the predator.
                .max_by(|a, b| {
                    let iso = |me: &(EntityId, Pos, f32)| {
                        living
                            .iter()
                            .filter(|o| o.0 != me.0)
                            .map(|o| o.1.manhattan(me.1))
                            .min()
                            .unwrap_or(i32::MAX)
                    };
                    iso(a)
                        .cmp(&iso(b))
                        .then_with(|| b.1.manhattan(p).cmp(&a.1.manhattan(p)))
                })
                .copied(),
        };
        let Some((tid, tpos, _)) = target else { return };

        if p.manhattan(tpos) == 0 {
            self.strike(tid);
            return;
        }

        // movement cadence: fast genes move every tick, else the world's period.
        let period = if s.fast() { 1 } else { self.predator.move_period };
        let in_cooldown = self.tick < self.predator.cooldown_until;

        let new = if in_cooldown && !s.persistent() {
            // incumbent-style scatter during cooldown
            let d = Dir::ALL[self.rng.below(4)];
            self.clamp(p.step(d))
        } else if s.persistent() || self.tick.is_multiple_of(period) {
            if p.manhattan(tpos) <= s.aggro() {
                // chase
                p.step(p.toward(tpos))
            } else if s.patrols() {
                // AMBUSH: deterministically march toward the village hearth and
                // lie in wait at the commons, rather than random-walking.
                p.step(p.toward(self.granary))
            } else {
                let d = Dir::ALL[self.rng.below(4)];
                self.clamp(p.step(d))
            }
        } else {
            p
        };
        let new = if self.walls.contains(&new) { p } else { new };
        self.predator.pos = new;
        if new == tpos {
            self.strike(tid);
        }
    }

    fn strike(&mut self, agent_id: EntityId) {
        if let Some(a) = self.agents.iter_mut().find(|a| a.id == agent_id) {
            // a mortal body takes the full bite to 0 (and dies in reap_dead); an
            // immortal one floors at 0.05 exactly as before.
            let floor = if a.mind.can_die() { 0.0 } else { 0.05 };
            let dmg = 0.2 * self.predator.bite;
            a.body.health = (a.body.health - dmg).max(floor);
            a.death_cause = "the stalker";
            a.inbox.push(WorldEvent::Hurt { id: self.predator.id, health: dmg });
            a.flash = 1.0;
        }
        // retreat to the farthest corner + cooldown
        let ap = self.agents.iter().find(|a| a.id == agent_id).map(|a| a.body.pos).unwrap_or(self.predator.pos);
        let corners = [
            Pos::new(0, 0),
            Pos::new(self.w - 1, 0),
            Pos::new(0, self.h - 1),
            Pos::new(self.w - 1, self.h - 1),
        ];
        let far = corners.into_iter().max_by_key(|c| c.manhattan(ap)).unwrap_or(Pos::new(0, 0));
        self.predator.pos = far;
        self.predator.cooldown_until = self.tick + 12;
    }

    fn respawn_due(&mut self) {
        let now = self.tick;
        // SEASONAL FOOD (open world only): food respawns freely in spring/summer/
        // autumn, but in WINTER it does NOT come back — the bushes stay bare until the
        // thaw. This is the scarcity that makes a winter store the difference between
        // life and death. Water still flows (a spring doesn't freeze solid here).
        // Outside an open world this guard is always true (Spring), so the respawn
        // logic — and the seeded trajectory — is byte-identical to before.
        let food_grows = !matches!(self.season(), Season::Winter);
        for r in &mut self.resources {
            if !r.alive && r.respawn_at.map(|t| t <= now).unwrap_or(false) {
                let frozen = self.open_world && r.kind == EntityKind::Food && !food_grows;
                if !frozen {
                    r.alive = true;
                    r.respawn_at = None;
                }
                // if frozen, leave it pending: it reappears the tick the thaw comes.
            }
        }
        // TREES regrow their wood in spring (open world only; trees is empty
        // otherwise so this is a no-op for closed worlds).
        if self.open_world && matches!(self.season(), Season::Spring) {
            for t in &mut self.trees {
                if t.wood < 1.0 {
                    t.wood = (t.wood + 0.01).min(1.0);
                    self.structures_dirty = true;
                }
            }
        }
        // MATERIALS ECONOMY (live-only): trees and quarry rocks slowly replenish every
        // tick so the resource base is renewable — a well-tended village keeps building.
        // The live showcase runs no seasons, so this regrow is season-independent. Inert
        // (no trees/rocks) when the economy is off, so harness paths are byte-identical.
        if self.materials_econ {
            for t in &mut self.trees {
                if t.wood < 1.0 {
                    t.wood = (t.wood + 0.004).min(1.0);
                    self.structures_dirty = true;
                }
            }
            for r in &mut self.rocks {
                if r.stone < 1.0 {
                    r.stone = (r.stone + 0.0025).min(1.0);
                    self.structures_dirty = true;
                }
            }
        }
    }

    /// Smoothly advance render positions toward grid positions, and decay timers.
    pub fn animate(&mut self, dt: f32) {
        let k = (dt * 9.0).min(1.0);
        for a in &mut self.agents {
            if !a.alive {
                // a grave stays where it fell; let any lingering speech bubble fade.
                if let Some((_, ref mut t)) = a.say {
                    *t -= dt;
                }
                continue;
            }
            let (ox, oy) = (a.rx, a.ry);
            a.rx += (a.body.pos.x as f32 - a.rx) * k;
            a.ry += (a.body.pos.y as f32 - a.ry) * k;
            // lay down a motion-trail breadcrumb when the agent has moved enough.
            let moved = (a.rx - ox).hypot(a.ry - oy);
            let far_enough =
                a.trail.last().map(|&(lx, ly)| (a.rx - lx).hypot(a.ry - ly) > 0.18).unwrap_or(true);
            if moved > 0.02 && far_enough {
                a.trail.push((a.rx, a.ry));
                if a.trail.len() > 14 {
                    a.trail.remove(0);
                }
            }
            a.flash = (a.flash - dt * 1.6).max(0.0);
            if let Some((_, ref mut t)) = a.say {
                *t -= dt;
            }
            if a.say.as_ref().map(|(_, t)| *t <= 0.0).unwrap_or(false) {
                a.say = None;
            }
        }
        self.predator.rx += (self.predator.pos.x as f32 - self.predator.rx) * k;
        self.predator.ry += (self.predator.pos.y as f32 - self.predator.ry) * k;
        for r in &mut self.resources {
            r.pulse = (r.pulse + dt * 0.6).fract();
        }
        // NATURAL ECOSYSTEM render smoothing (live-only; the lists are empty otherwise).
        // Each animal lerps its render position toward its grid cell and turns to face
        // the direction it is moving, so the herd/pack reads as moving believably.
        for wl in &mut self.wolves {
            let (ox, oy) = (wl.rx, wl.ry);
            wl.rx += (wl.pos.x as f32 - wl.rx) * k;
            wl.ry += (wl.pos.y as f32 - wl.ry) * k;
            let (mx, my) = (wl.rx - ox, wl.ry - oy);
            if mx.hypot(my) > 0.004 {
                wl.heading = my.atan2(mx);
            }
            wl.flash = (wl.flash - dt * 1.6).max(0.0);
        }
        for b in &mut self.bears {
            let (ox, oy) = (b.rx, b.ry);
            b.rx += (b.pos.x as f32 - b.rx) * k;
            b.ry += (b.pos.y as f32 - b.ry) * k;
            let (mx, my) = (b.rx - ox, b.ry - oy);
            if mx.hypot(my) > 0.004 {
                b.heading = my.atan2(mx);
            }
            b.flash = (b.flash - dt * 1.6).max(0.0);
        }
        for d in &mut self.deer {
            // a despawned (caught) deer is hidden until it respawns; don't drag its
            // render position across the map in the meantime.
            if d.respawn_at.is_some() {
                continue;
            }
            let (ox, oy) = (d.rx, d.ry);
            d.rx += (d.pos.x as f32 - d.rx) * k;
            d.ry += (d.pos.y as f32 - d.ry) * k;
            let (mx, my) = (d.rx - ox, d.ry - oy);
            if mx.hypot(my) > 0.004 {
                d.heading = my.atan2(mx);
            }
            d.flash = (d.flash - dt * 1.6).max(0.0);
        }
    }

    /// `true` if a deer is currently caught (despawned, awaiting respawn) — the
    /// renderer skips drawing it.
    pub fn deer_hidden(&self, d: &Deer) -> bool {
        d.respawn_at.is_some()
    }

    // ===================================================================
    // NATURAL ECOSYSTEM (live-only). All RNG draws here use `wild_rng`, never the
    // main `rng`, so enabling wildlife leaves every seeded mind trajectory
    // byte-identical. Only reached when `self.wildlife` is true.
    // ===================================================================

    /// One ecosystem tick: deer flee/graze, wolves pack-hunt, bears roam-and-maul,
    /// and caught deer respawn. Mind kills route through the same health/grief path
    /// as the stalker, so a wolf-kill is grieved exactly like a stalker-kill.
    fn step_wildlife(&mut self) {
        self.step_deer();
        self.step_wolves();
        self.step_bears();
        self.respawn_deer();
    }

    /// The Manhattan distance from `p` to the nearest *living* mind, and that mind's
    /// id+pos — wolves and bears hunt minds, deer flee them.
    fn nearest_mind(&self, p: Pos) -> Option<(EntityId, Pos, i32)> {
        self.agents
            .iter()
            .filter(|a| a.alive)
            .map(|a| (a.id, a.body.pos, a.body.pos.manhattan(p)))
            .min_by_key(|(_, _, d)| *d)
    }

    /// Nearest *live* (not caught) deer to `p`: index, pos, distance. For wolf/bear
    /// hunting. Deer are the preferred, abundant prey, so a healthy ecosystem mostly
    /// thins the herd, not the village.
    fn nearest_deer(&self, p: Pos) -> Option<(usize, Pos, i32)> {
        self.deer
            .iter()
            .enumerate()
            .filter(|(_, d)| d.respawn_at.is_none())
            .map(|(i, d)| (i, d.pos, d.pos.manhattan(p)))
            .min_by_key(|(_, _, dist)| *dist)
    }

    /// Is any predator (wolf, bear, or the stalker) within `r` cells of `p`?
    /// Returns the nearest threat's position if so — deer flee directly away from it.
    fn nearest_threat(&self, p: Pos, r: i32) -> Option<Pos> {
        let mut best: Option<(Pos, i32)> = None;
        let mut consider = |q: Pos| {
            let d = q.manhattan(p);
            if d <= r && best.map(|(_, bd)| d < bd).unwrap_or(true) {
                best = Some((q, d));
            }
        };
        for w in &self.wolves {
            consider(w.pos);
        }
        for b in &self.bears {
            consider(b.pos);
        }
        consider(self.predator.pos);
        best.map(|(q, _)| q)
    }

    /// A roaming step biased by a heading direction with some random jitter — gives
    /// animals a believable wandering gait instead of pure noise. `bias` toward a
    /// target (if any) blends with the wander.
    fn wild_wander(&mut self, p: Pos) -> Pos {
        let d = Dir::ALL[self.wild_rng.below(4)];
        self.clamp(p.step(d))
    }

    /// Move a deer: graze (wander, lightly drawn toward grass/trees) most of the time,
    /// but flee directly away from the nearest predator within sight. Deer are quick —
    /// they take a step almost every tick, faster than the wolves' base roam — so a
    /// healthy deer usually escapes and the herd persists as standing ambient life.
    fn step_deer(&mut self) {
        let sight = self.sight + 4; // deer are watchful — they spot danger early
        let n = self.deer.len();
        for i in 0..n {
            if self.deer[i].respawn_at.is_some() {
                continue;
            }
            let p = self.deer[i].pos;
            let threat = self.nearest_threat(p, sight);
            if let Some(tp) = threat {
                // FLEE: step in the cardinal direction that most increases distance
                // from the threat. Deer bound away fast (a step every tick).
                self.deer[i].fleeing = true;
                let mut best = p;
                let mut best_d = p.manhattan(tp);
                for dir in Dir::ALL {
                    let q = self.clamp(p.step(dir));
                    let dd = q.manhattan(tp);
                    if dd > best_d {
                        best_d = dd;
                        best = q;
                    }
                }
                self.deer[i].pos = best;
            } else {
                self.deer[i].fleeing = false;
                // GRAZE: wander slowly (move ~half the ticks), gently drawn toward the
                // nearest tree/grove if there is one (a believable browsing herd).
                if self.wild_rng.chance(0.55) {
                    let toward_grove = self
                        .trees
                        .iter()
                        .map(|t| t.pos)
                        .min_by_key(|tp| tp.manhattan(p));
                    let q = match toward_grove {
                        Some(tp) if tp.manhattan(p) > 2 && self.wild_rng.chance(0.5) => {
                            self.clamp(p.step(p.toward(tp)))
                        }
                        _ => self.wild_wander(p),
                    };
                    self.deer[i].pos = q;
                }
            }
        }
    }

    /// Move the wolves: each pack coheres toward its own centroid and roams; a wolf
    /// within aggro of prey (deer preferred, else a mind) presses the attack, but a
    /// wolf that is ALONE (no packmate nearby) or recently bitten backs off — so a
    /// lone wolf is not a reliable killer and the village mostly thrives. Adjacency to
    /// a mind triggers a small, cooled-down bite routed through the grief path.
    fn step_wolves(&mut self) {
        const AGGRO: i32 = 9;
        const PACK_NEAR: i32 = 7; // a packmate this close counts as "with the pack"
        let n = self.wolves.len();
        // pack centroids (for cohesion), computed up front from this tick's start.
        let mut pack_sum: [(i32, i32, i32); 4] = [(0, 0, 0); 4];
        for w in &self.wolves {
            let pk = (w.pack as usize).min(3);
            pack_sum[pk].0 += w.pos.x;
            pack_sum[pk].1 += w.pos.y;
            pack_sum[pk].2 += 1;
        }
        for i in 0..n {
            let p = self.wolves[i].pos;
            let pack = self.wolves[i].pack;
            let in_cooldown = self.tick < self.wolves[i].cooldown_until;
            // how many packmates are hunting alongside? (a lone or paired-off wolf is
            // timid; it takes a real gathered pack to press a mind)
            let mut packmates_near = 0;
            for (j, o) in self.wolves.iter().enumerate() {
                if j != i && o.pack == pack && o.pos.manhattan(p) <= PACK_NEAR {
                    packmates_near += 1;
                }
            }
            // choose prey: nearest deer, else nearest mind, within aggro.
            let deer_target = self.nearest_deer(p).filter(|(_, _, d)| *d <= AGGRO);
            let mind_target = self.nearest_mind(p).filter(|(_, _, d)| *d <= AGGRO);
            // a lone, paired-off, or cooled-down wolf won't commit to hunting MINDS (it
            // backs off), but will still chase a deer — deer are the safe staple prey.
            // Requiring a GATHERED pack (≥2 packmates near) to press a mind is the key
            // lethality knob: minds are only seriously threatened by a coordinated pack,
            // so the village mostly thrives and a kill is a real pack event.
            let hunt_mind = mind_target.is_some() && packmates_near >= 2 && !in_cooldown;

            // wolves move every tick when hunting, else every other tick (a loping roam).
            let roaming_tick = self.tick.is_multiple_of(2);

            let new = if let Some((di, dp, dist)) = deer_target {
                // chase the deer; on contact, take it.
                if dist == 0 {
                    self.catch_deer(di);
                    p
                } else {
                    p.step(p.toward(dp))
                }
            } else if hunt_mind {
                let (_, mp, dist) = mind_target.unwrap();
                if dist <= 1 {
                    // adjacent: bite (cooled down), then hold position pressing in.
                    self.wolf_bite(i);
                    p
                } else {
                    p.step(p.toward(mp))
                }
            } else if roaming_tick {
                // ROAM with pack cohesion: drift toward the pack centroid plus jitter,
                // so the pack stays loosely together rather than dispersing.
                let pk = (pack as usize).min(3);
                let (sx, sy, cnt) = pack_sum[pk];
                if cnt > 1 && self.wild_rng.chance(0.5) {
                    let centroid = Pos::new(sx / cnt, sy / cnt);
                    if centroid.manhattan(p) > 3 {
                        p.step(p.toward(centroid))
                    } else {
                        self.wild_wander(p)
                    }
                } else {
                    self.wild_wander(p)
                }
            } else {
                p
            };
            let new = self.clamp(new);
            // walls block wolves too (the village's shelters are real protection).
            let new = if self.walls.contains(&new) { p } else { new };
            self.wolves[i].pos = new;
        }
    }

    /// A wolf bites the nearest living mind (must be adjacent): a SMALL bite (wolves
    /// are survivable), on a per-wolf cooldown, routed through the same Hurt/grief path
    /// as the stalker so a kill is grieved. The wolf then recoils a step and goes on
    /// cooldown — it does not pin a mind and chew it down in one stand.
    fn wolf_bite(&mut self, wolf_idx: usize) {
        let wp = self.wolves[wolf_idx].pos;
        let wid = self.wolves[wolf_idx].id;
        let target = self
            .agents
            .iter_mut()
            .filter(|a| a.alive && a.body.pos.manhattan(wp) <= 1)
            .min_by_key(|a| a.body.pos.manhattan(wp));
        if let Some(a) = target {
            let floor = if a.mind.can_die() { 0.0 } else { 0.05 };
            // wolves bite light: a single wolf can't fell a healthy mind quickly; a
            // pack worrying a weakened straggler is what occasionally kills.
            let dmg = 0.07;
            a.body.health = (a.body.health - dmg).max(floor);
            a.death_cause = "a wolf";
            a.inbox.push(WorldEvent::Hurt { id: wid, health: dmg });
            a.flash = 1.0;
        }
        self.wolves[wolf_idx].flash = 1.0;
        self.wolves[wolf_idx].cooldown_until = self.tick + 18;
        // recoil: the wolf darts back a step (random cardinal) after biting.
        let d = Dir::ALL[self.wild_rng.below(4)];
        self.wolves[wolf_idx].pos = self.clamp(wp.step(d));
    }

    /// Move the bears: solitary slow roam (a step roughly every 3rd tick), a SHORT
    /// aggro radius (you must be close to be in danger), but a heavy bite. Bears prefer
    /// deer; a mind that strays right up to a bear takes a serious hit. Rare by design.
    fn step_bears(&mut self) {
        const AGGRO_DEER: i32 = 6; // bears will chase a deer from a little way off
        const AGGRO_MIND: i32 = 3; // but a mind is only in danger right up close
        let hearth = self.granary;
        let n = self.bears.len();
        for i in 0..n {
            let p = self.bears[i].pos;
            let in_cooldown = self.tick < self.bears[i].cooldown_until;
            let deer_target = self.nearest_deer(p).filter(|(_, _, d)| *d <= AGGRO_DEER);
            let mind_target = self.nearest_mind(p).filter(|(_, _, d)| *d <= AGGRO_MIND);
            // bears are slow: move ~every 3rd tick unless actively closing on prey.
            let slow_tick = self.tick.is_multiple_of(3);

            let new = if let Some((di, dp, dist)) = deer_target {
                // deer are the bear's staple prey — chase them preferentially.
                if dist == 0 {
                    self.catch_deer(di);
                    p
                } else {
                    p.step(p.toward(dp))
                }
            } else if let Some((_, mp, dist)) = mind_target {
                // a mind that wanders right up to the bear is in danger; otherwise the
                // bear does NOT path into the village after it (it is not a stalker).
                if dist <= 1 && !in_cooldown {
                    self.bear_maul(i);
                    p
                } else if dist <= 1 {
                    // adjacent but on cooldown: shove off rather than loiter on the mind.
                    self.wild_wander(p)
                } else {
                    p.step(p.toward(mp))
                }
            } else if slow_tick {
                // PERIPHERY ROAMER: a bear drifts away from the crowded village heart
                // (so it isn't a fixture among the minds), wandering the wild margins.
                if p.manhattan(hearth) < 14 && self.wild_rng.chance(0.6) {
                    let away = match p.toward(hearth) {
                        Dir::North => Dir::South,
                        Dir::South => Dir::North,
                        Dir::East => Dir::West,
                        Dir::West => Dir::East,
                    };
                    self.clamp(p.step(away))
                } else {
                    self.wild_wander(p)
                }
            } else {
                p
            };
            let new = self.clamp(new);
            let new = if self.walls.contains(&new) { p } else { new };
            self.bears[i].pos = new;
        }
    }

    /// A bear mauls an adjacent mind: a HEAVY bite (bears are powerful), on a long
    /// cooldown, through the grief path. A rare but serious danger.
    fn bear_maul(&mut self, bear_idx: usize) {
        let bp = self.bears[bear_idx].pos;
        let bid = self.bears[bear_idx].id;
        let target = self
            .agents
            .iter_mut()
            .filter(|a| a.alive && a.body.pos.manhattan(bp) <= 1)
            .min_by_key(|a| a.body.pos.manhattan(bp));
        if let Some(a) = target {
            let floor = if a.mind.can_die() { 0.0 } else { 0.05 };
            let dmg = 0.16; // a real, heavy hit up close — but rarely a clean kill
            a.body.health = (a.body.health - dmg).max(floor);
            a.death_cause = "a bear";
            a.inbox.push(WorldEvent::Hurt { id: bid, health: dmg });
            a.flash = 1.0;
        }
        self.bears[bear_idx].flash = 1.0;
        self.bears[bear_idx].cooldown_until = self.tick + 80;
    }

    /// A predator catches a deer: the deer despawns (hidden) and is scheduled to
    /// respawn as a fresh deer elsewhere — renewable prey, so the herd persists.
    fn catch_deer(&mut self, deer_idx: usize) {
        let d = &mut self.deer[deer_idx];
        if d.respawn_at.is_some() {
            return;
        }
        d.respawn_at = Some(self.tick + 120); // a short while later a new deer wanders in
        d.flash = 1.0;
    }

    /// Bring caught deer back as fresh deer, well away from any predator, so the herd
    /// stays roughly constant (ambient life, not a one-way cull).
    fn respawn_deer(&mut self) {
        let now = self.tick;
        let n = self.deer.len();
        for i in 0..n {
            let due = matches!(self.deer[i].respawn_at, Some(t) if now >= t);
            if !due {
                continue;
            }
            // find a cell comfortably clear of every predator (a few tries; fall back
            // to wherever if the island is crowded).
            let mut spot = self.deer[i].pos;
            for _ in 0..8 {
                let cand = Pos::new(
                    self.wild_rng.below(self.w as usize) as i32,
                    self.wild_rng.below(self.h as usize) as i32,
                );
                if self.nearest_threat(cand, 8).is_none() {
                    spot = cand;
                    break;
                }
            }
            let d = &mut self.deer[i];
            d.pos = spot;
            d.rx = spot.x as f32;
            d.ry = spot.y as f32;
            d.respawn_at = None;
            d.fleeing = false;
        }
    }
}

fn short_say(text: &str) -> String {
    let t = text.trim();
    if t.chars().count() <= 28 {
        t.to_string()
    } else {
        let s: String = t.chars().take(27).collect();
        format!("{s}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn village_runs_long_without_panicking() {
        // Hammer the whole multi-mind embodiment: percept synthesis, real
        // cognition, action resolution, predator, speech, respawn.
        let mut w = GameWorld::new(0xDA13, 6);
        for _ in 0..1200 {
            w.step();
            w.animate(0.016);
        }
        assert_eq!(w.agents.len(), 6);
        // every mind should have lived, formed beliefs, and stayed bounded.
        for a in &w.agents {
            assert!(a.mind.metrics().ticks >= 1000);
            assert!((0.0..=1.0).contains(&a.body.health));
            assert!(a.mind.memory().episode_count() > 0);
        }
        // determinism: same seed → same agent positions.
        let mut a = GameWorld::new(7, 4);
        let mut b = GameWorld::new(7, 4);
        for _ in 0..300 {
            a.step();
            b.step();
        }
        for (x, y) in a.agents.iter().zip(b.agents.iter()) {
            assert_eq!(x.body.pos, y.body.pos);
        }
    }

    #[test]
    fn walls_block_predator() {
        // The crux of the emergent-shelter feature: walls are *solid* to the
        // stalker. Ring an agent's four cardinal neighbours with walls (a full
        // enclosure) and the predator can never reach it, however long it hunts.
        // This is world physics — independent of whether any mind chooses to build
        // — so we insert the walls by hand and assert the protection holds.
        let mut w = GameWorld::new(0x5A1E, 3);
        // Park the chosen agent somewhere interior so all four neighbours are
        // in-bounds (real wall cells, not map edge), then wall it in completely.
        let agent_idx = 0;
        let home = Pos::new(20, 13);
        w.agents[agent_idx].body.pos = home;
        let ring = [
            Pos::new(home.x + 1, home.y),
            Pos::new(home.x - 1, home.y),
            Pos::new(home.x, home.y + 1),
            Pos::new(home.x, home.y - 1),
        ];
        for p in ring {
            w.walls.insert(p);
        }

        for _ in 0..800 {
            w.step();
            // The enclosed agent stays put (every neighbour is a wall it cannot
            // step into), so its cell is fixed for the whole run.
            assert_eq!(
                w.agents[agent_idx].body.pos, home,
                "the walled-in agent should be unable to move out of its enclosure"
            );
            let pp = w.predator.pos;
            // The predator must never occupy a wall cell …
            assert!(
                !w.walls.contains(&pp),
                "predator entered a wall cell at {pp:?} — walls are not solid"
            );
            // … and must never land on the sheltered agent (manhattan ≥ 1).
            assert!(
                pp.manhattan(home) >= 1,
                "predator reached the walled-in agent at {home:?} — shelter failed"
            );
        }
    }

    #[test]
    fn death_removes_mind_from_living() {
        use daimon_mind::Genome;
        // With mortality ON, drive a chosen mind's health to 0 and assert it dies:
        // becomes not-alive, stops being a forage/social/predator target, is not
        // pickable, and the living count drops — while the agents vec is unchanged
        // (tombstone, not removal). With mortality OFF, the identical scenario keeps
        // every mind alive: the determinism/incumbent guard.
        let run = |can_die: bool| -> (usize, usize, bool, bool) {
            let mut g = Genome::baseline();
            g.g[22] = if can_die { 1.0 } else { 0.0 };
            let mut w = GameWorld::with_genome(0xDEAD, 6, &g);
            // The lethal path is the (unfloored) predator strike. Pin the stalker on
            // top of a low-health agent 0 each tick: it bites, and with mortality ON
            // the bite is not floored, so agent 0 falls to 0 and is reaped. With
            // mortality OFF the same bite floors at 0.05 and the agent survives —
            // the determinism/incumbent guard.
            for _ in 0..40 {
                w.agents[0].body.health = w.agents[0].body.health.min(0.18);
                w.predator.pos = w.agents[0].body.pos; // colocated → it strikes
                w.step();
                if !w.agents[0].alive {
                    break;
                }
            }
            let alive0 = w.agents[0].alive;
            let living = w.living_count();
            let total = w.agents.len();
            // a dead agent must not appear among perceivable entities (snapshot).
            let snap = w.snapshot();
            let dead_id = w.agents[0].id;
            let in_snapshot = snap.iter().any(|e| e.id == dead_id);
            // a dead agent must not be pickable at its own position.
            let (rx, ry) = (w.agents[0].rx, w.agents[0].ry);
            let pickable = w.pick_agent(rx, ry, 1.5) == Some(0);
            assert_eq!(total, 6, "agents vec length is load-bearing — never shrinks");
            (if alive0 { 1 } else { 0 }, living, in_snapshot, pickable)
        };

        // mortality ON: agent 0 dies; living count drops below 6; gone from world.
        let (on_alive0, on_living, on_snap, on_pick) = run(true);
        assert_eq!(on_alive0, 0, "the starved mind should be dead with can_die on");
        assert!(on_living < 6, "living count must drop when a mind dies (was {on_living})");
        assert!(!on_snap, "a dead mind must not be a perceivable target");
        assert!(!on_pick, "a dead mind must not be pickable");

        // mortality OFF (determinism guard): the same scenario kills no one.
        let (off_alive0, off_living, _off_snap, _off_pick) = run(false);
        assert_eq!(off_alive0, 1, "with can_die off, the floored mind stays alive");
        assert_eq!(off_living, 6, "no deaths with mortality off — all six live");
    }

    #[test]
    fn open_world_is_off_by_default_and_inert() {
        // The open_world flag defaults false on every standard constructor, the season
        // is eternal Spring, winter never comes, and there are no trees/granary stores
        // — so the seasonal machinery is wholly inert and seeded worlds are unchanged.
        let w = GameWorld::new(0x61, 6);
        assert!(!w.open_world);
        assert_eq!(w.season(), Season::Spring);
        assert_eq!(w.winter_in(), f32::MAX);
        assert!(w.trees.is_empty());
        assert_eq!(w.granary_food, 0.0);
        // and the per-agent open-world senses are at their inert defaults.
        for a in &w.agents {
            assert_eq!(a.body.season, 0);
            assert!(a.body.gather_dir.is_none() && a.body.store_dir.is_none());
            assert_eq!(a.body.carrying, 0.0);
        }
    }

    #[test]
    fn season_clock_is_deterministic_and_winter_stops_food() {
        let mut w = GameWorld::new(0x5EA50, 4);
        w.set_open_world(true);
        // season is a pure function of the tick clock: sample the four quarters.
        let at = |t: u64| match (t / SEASON_TICKS) % 4 {
            0 => Season::Spring,
            1 => Season::Summer,
            2 => Season::Autumn,
            _ => Season::Winter,
        };
        for t in [0u64, SEASON_TICKS, 2 * SEASON_TICKS, 3 * SEASON_TICKS, YEAR_TICKS, YEAR_TICKS + 3 * SEASON_TICKS] {
            // step the world to tick t and confirm the reported season matches the clock.
            while w.tick < t {
                w.step();
            }
            assert_eq!(w.season(), at(w.tick), "season clock at tick {}", w.tick);
        }
        // WINTER zeroes food respawn: kill all food, advance into winter, and confirm
        // no Food comes back while a Water resource still can.
        let mut w2 = GameWorld::new(0x5EA51, 4);
        w2.set_open_world(true);
        // jump to the start of winter.
        while !matches!(w2.season(), Season::Winter) {
            w2.step();
        }
        for r in &mut w2.resources {
            if r.kind == EntityKind::Food {
                r.alive = false;
                r.respawn_at = Some(w2.tick + 1); // due immediately
            }
        }
        // step a chunk of winter; food must NOT come back.
        for _ in 0..200 {
            w2.step();
            if !matches!(w2.season(), Season::Winter) {
                break;
            }
        }
        let winter_food = w2.resources.iter().filter(|r| r.kind == EntityKind::Food && r.alive).count();
        assert_eq!(winter_food, 0, "food must not respawn in winter");
    }

    #[test]
    fn granary_deposit_and_winter_draw() {
        // Deposit into the cache via Store physics, then confirm a hungry mind near the
        // hearth in winter draws it down. We drive the world state directly so this
        // tests the WORLD mechanics, independent of whether any mind chooses to provision.
        let mut w = GameWorld::new(0x6A0, 4);
        w.set_open_world(true);
        // park agent 0 on the granary, give it a carried load, and Store it.
        w.agents[0].body.pos = w.granary;
        w.agents[0].body.carrying = 0.8;
        let before = w.granary_food;
        // resolve a Store by hand (mirror the step() resolution).
        if w.agents[0].body.carrying > 0.01 && w.agents[0].body.pos.manhattan(w.granary) <= 1 {
            w.granary_food = (w.granary_food + w.agents[0].body.carrying).min(w.granary_capacity());
            w.agents[0].body.carrying = 0.0;
        }
        assert!(w.granary_food > before, "Store must raise the cache");
        assert_eq!(w.agents[0].body.carrying, 0.0, "storing empties the load");

        // now advance to winter with a stocked cache and a hungry mind at the hearth;
        // the auto-draw (in metabolism) must reduce the cache.
        w.granary_food = 10.0;
        while !matches!(w.season(), Season::Winter) {
            w.step();
            w.granary_food = w.granary_food.max(10.0); // keep it stocked up to winter
        }
        w.granary_food = 10.0;
        // drag a mind to the hearth and make it hungry so it draws.
        w.agents[0].body.pos = w.granary;
        w.agents[0].body.energy = 0.1;
        let cache_before = w.granary_food;
        for _ in 0..30 {
            w.agents[0].body.pos = w.granary; // hold it at the hearth
            w.step();
            if !matches!(w.season(), Season::Winter) {
                break;
            }
        }
        assert!(w.granary_food < cache_before, "a hungry mind at the hearth must draw the winter stores");
        assert!(w.agents[0].body.energy > 0.1, "the draw must feed the mind");
    }

    #[test]
    fn feed_and_pick_work() {
        let mut w = GameWorld::new(1, 3);
        let before = w.resources.len();
        w.feed(5.0, 5.0);
        assert_eq!(w.resources.len(), before + 1);
        // an agent sits on its own render position, so picking there finds it.
        let a0 = (w.agents[0].rx, w.agents[0].ry);
        assert_eq!(w.pick_agent(a0.0, a0.1, 0.7), Some(0));
    }

    /// Build the live showcase gene set the way `Game::new` does.
    fn live_lifecycle_world(cap: usize) -> GameWorld {
        let mut g = daimon_mind::Genome::showcase();
        for i in [21, 22, 23, 24, 29, 30, 31, 32] {
            g.g[i] = 1.0;
        }
        let mut w = GameWorld::with_genome_sized(0x61, 64, &g, 124, 84, 7);
        w.soften_stalker();
        w.set_materials_world(true);
        w.set_wildlife(true);
        w.set_lifecycle(true, cap);
        w
    }

    #[test]
    fn lifecycle_off_world_never_reproduces_or_ages() {
        // A world that never calls `set_lifecycle` must be byte-identical to the
        // incumbent: fixed population, no births, no natural deaths, no pairing.
        let mut w = GameWorld::new(0xDA13, 6);
        let n0 = w.agents.len();
        for _ in 0..1500 {
            w.step();
        }
        assert_eq!(w.agents.len(), n0, "no minds spawned on a non-lifecycle world");
        assert_eq!(w.births, 0);
        assert_eq!(w.natural_deaths, 0);
        assert!(w.agents.iter().all(|a| a.partner.is_none()), "no pair-bonds form");
        // determinism: two non-lifecycle worlds stay identical.
        let mut a = GameWorld::new(7, 4);
        let mut b = GameWorld::new(7, 4);
        for _ in 0..400 {
            a.step();
            b.step();
        }
        for (x, y) in a.agents.iter().zip(b.agents.iter()) {
            assert_eq!(x.body.pos, y.body.pos);
        }
    }

    #[test]
    fn lifecycle_births_deaths_and_population_bound() {
        // The live life-cycle: over a long run a turning-over village forms
        // pair-bonds, has inherited children, loses elders to old age, and stays
        // bounded by the cap — births and natural deaths both happen.
        let cap = 90;
        let mut w = live_lifecycle_world(cap);
        for _ in 0..12000 {
            w.step();
        }
        assert!(w.births > 0, "the village had children");
        assert!(w.natural_deaths > 0, "elders passed of old age");
        assert!(w.pairings > 0, "romantic pair-bonds formed");
        // population stays within the cap (a couple in flight, so allow a small slack).
        assert!(w.living_count() <= cap + 2, "population bounded by the cap");
        // and it did not die out — the lineage is turning over, not collapsing.
        assert!(w.living_count() > cap / 3, "the village did not collapse");
        // an inherited child carries the live gene set (it ages, mates, reproduces).
        let a_child = w.agents.iter().find(|a| !a.parents.is_empty());
        assert!(a_child.is_some(), "at least one child was born into the world");
        let c = a_child.unwrap();
        assert!(c.mind.can_age() && c.mind.can_mate() && c.mind.can_reproduce());
    }

    #[test]
    fn lifecycle_is_deterministic() {
        // Same seed + same flags ⇒ identical lineage (population, births, deaths).
        let mut a = live_lifecycle_world(90);
        let mut b = live_lifecycle_world(90);
        for _ in 0..4000 {
            a.step();
            b.step();
        }
        assert_eq!(a.living_count(), b.living_count());
        assert_eq!(a.births, b.births);
        assert_eq!(a.natural_deaths, b.natural_deaths);
        assert_eq!(a.pairings, b.pairings);
        assert_eq!(a.agents.len(), b.agents.len());
        for (x, y) in a.agents.iter().zip(b.agents.iter()) {
            assert_eq!(x.body.pos, y.body.pos);
            assert_eq!(x.partner, y.partner);
            assert_eq!(x.born_tick, y.born_tick);
        }
    }

    /// Build the full live showcase world (lifecycle + materials + wildlife + SOCIETY),
    /// exactly as `Game::new` does, for the society tests.
    fn live_society_world(n_villages: usize) -> GameWorld {
        let mut g = daimon_mind::Genome::showcase();
        for i in [21, 22, 23, 24, 29, 30, 31, 32, 33] {
            g.g[i] = 1.0;
        }
        let mut w = GameWorld::with_genome_sized(0x61, 64, &g, 124, 84, 7);
        w.soften_stalker();
        w.set_materials_world(true);
        w.set_wildlife(true);
        w.set_lifecycle(true, 90);
        w.set_society(true, n_villages);
        w
    }

    #[test]
    fn society_off_world_is_byte_identical() {
        // A world that never calls `set_society` must be byte-identical to the
        // incumbent: no villages, no relations, no society RNG, no movement nudge.
        // (The village_affinity gene is off in both presets, so even the live-flag
        // arming on a NON-society world leaves trajectories untouched.)
        let mut w = GameWorld::new(0xDA13, 6);
        for _ in 0..1500 {
            w.step();
        }
        assert!(!w.society);
        assert!(w.villages.is_empty());
        assert!(w.relations.is_empty());
        assert!(w.agents.iter().all(|a| a.village.is_none()));
        // determinism: two non-society worlds stay identical.
        let mut a = GameWorld::new(7, 4);
        let mut b = GameWorld::new(7, 4);
        for _ in 0..400 {
            a.step();
            b.step();
        }
        for (x, y) in a.agents.iter().zip(b.agents.iter()) {
            assert_eq!(x.body.pos, y.body.pos);
        }
    }

    #[test]
    fn society_forms_distinct_villages_with_identity() {
        // Founders are partitioned into the requested number of distinct villages,
        // each with a unique id + a colour + a name, and every founder is assigned.
        let w = live_society_world(4);
        assert_eq!(w.villages.len(), 4);
        // unique ids 0..4, distinct hues, distinct names.
        let ids: std::collections::HashSet<u8> = w.villages.iter().map(|v| v.id).collect();
        assert_eq!(ids.len(), 4);
        let hues: std::collections::HashSet<u32> = w.villages.iter().map(|v| v.hue).collect();
        assert_eq!(hues.len(), 4, "each village has a distinct banner hue");
        let names: std::collections::HashSet<&str> =
            w.villages.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names.len(), 4, "each village has a distinct name");
        // every living founder belongs to a village, and more than one is populated.
        assert!(w.agents.iter().filter(|a| a.alive).all(|a| a.village.is_some()));
        let populated = w.villages.iter().filter(|v| v.population > 0).count();
        assert!(populated >= 2, "the founders spread across multiple villages");
        // one relation per unordered pair: C(4,2) = 6, all starting neutral.
        assert_eq!(w.relations.len(), 6);
        assert!(w.relations.iter().all(|r| r.affinity == 0.0));
    }

    #[test]
    fn society_relations_emerge_and_shift() {
        // Over a long run the inter-village relations must MOVE off neutral — real
        // alliances and/or rivalries emerge from interaction (not hard-coded), and the
        // world stays alive (the lineage keeps turning over, villages persist).
        let mut w = live_society_world(4);
        // sample EVERY pair's affinity over time. Emergence + shifting means: at least
        // one relation reaches strongly non-neutral (an alliance or rivalry forms), and
        // at least one pair both RISES and FALLS across the run (relations shift, not a
        // monotone ramp to a rail). We don't fix on one pair — spatially-isolated pairs
        // never interact, so we require the dynamics to appear in SOME pair.
        let np = w.relations.len();
        let mut series: Vec<Vec<f32>> = vec![Vec::new(); np];
        let mut max_abs = 0.0f32;
        for t in 0..30000u32 {
            w.step();
            if t % 500 == 0 {
                for (k, r) in w.relations.iter().enumerate() {
                    series[k].push(r.affinity);
                    max_abs = max_abs.max(r.affinity.abs());
                }
            }
        }
        // at least one relation became strongly non-neutral (allied or hostile).
        assert!(max_abs > 0.30, "relations emerged off neutral (max |aff| {max_abs:.2})");
        // SOME pair both rose and fell across the run — a genuine, shifting relation.
        let shifted = series.iter().any(|s| {
            let mut rose = false;
            let mut fell = false;
            for win in s.windows(2) {
                if win[1] > win[0] + 0.02 {
                    rose = true;
                }
                if win[1] < win[0] - 0.02 {
                    fell = true;
                }
            }
            rose && fell
        });
        assert!(shifted, "at least one relation shifted both up and down over the run");
        // the world stayed alive: a turning-over population, not a collapse or war.
        assert!(w.living_count() > 30, "the society did not die out ({} alive)", w.living_count());
        assert!(w.villages.iter().filter(|v| v.population > 0).count() >= 2, "multiple villages persist");
        assert!(w.cross_marriages > 0, "some pair-bonds crossed village lines (intermarriage)");
    }

    #[test]
    fn society_is_deterministic() {
        // Same seed + same flags ⇒ identical society (villages, relations, population).
        let mut a = live_society_world(4);
        let mut b = live_society_world(4);
        for _ in 0..6000 {
            a.step();
            b.step();
        }
        assert_eq!(a.living_count(), b.living_count());
        assert_eq!(a.births, b.births);
        assert_eq!(a.cross_marriages, b.cross_marriages);
        assert_eq!(a.border_deaths, b.border_deaths);
        assert_eq!(a.relations.len(), b.relations.len());
        for (x, y) in a.relations.iter().zip(b.relations.iter()) {
            assert_eq!(x.a, y.a);
            assert_eq!(x.b, y.b);
            assert_eq!(x.affinity.to_bits(), y.affinity.to_bits(), "relation affinity identical");
        }
        for (x, y) in a.agents.iter().zip(b.agents.iter()) {
            assert_eq!(x.body.pos, y.body.pos);
            assert_eq!(x.village, y.village);
        }
    }
}
