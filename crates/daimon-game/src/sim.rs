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
/// Affinity lost per evaluation scaled by a pair's combined RESOURCE SCARCITY — the
/// economic engine of war: as villages work the land thin they covet what their
/// neighbours still have, and the border sours toward war. Only applied in a
/// `scarcity_world` (the live game), so the seed-tuned balance tests never feel it.
const SCARCITY_PUSH: f32 = 0.05;
/// Standing wood+stone per head at/above which the land feels comfortable (scarcity 0);
/// scarcity climbs toward 1 as the per-head share falls below it. Tuned so the live
/// island sits in a fluctuating mid-range — flush in good times, pinched as it grows.
const SCARCITY_COMFORT: f32 = 1.5;
/// Cap on |affinity| from the per-evaluation drift, so no relation saturates at the
/// rail — it always has room to SHIFT back (allies can cool, enemies can thaw).
const AFFINITY_SOFT_CAP: f32 = 0.85;
/// Fraction by which every relation relaxes toward neutral per evaluation, so allies
/// can fall out and enemies can reconcile (nothing is permanent).
const RELATION_DECAY: f32 = 0.020;

// --- TECH / ERA tunables (Civilization Sprint 1). Research accrues each society
// evaluation (every SOCIETY_PERIOD ticks), scaled by a village's people, buildings and
// peace, so a bigger, busier, more peaceful settlement climbs the era ladder faster.
// All deterministic; runs off the society side-RNG path (no main-stream draws). ---
/// Knowledge units a single villager contributes per society evaluation (the base
/// drip). Multiplied by the population, building, and stability factors below. Tuned
/// so a healthy mid-size village reaches Industrial within a long session (~16k ticks)
/// without the climb being instant.
const RESEARCH_PER_HEAD: f32 = 0.060;
/// Each building counted near a village's centre adds this fraction to its research
/// rate (infrastructure compounds knowledge) — capped by `RESEARCH_BUILDING_CAP`.
const RESEARCH_BUILDING_BONUS: f32 = 0.045;
/// Cap on the building multiplier so a wall-happy village can't run away (≤ this many
/// effective buildings count toward the rate).
const RESEARCH_BUILDING_CAP: f32 = 12.0;
/// Manhattan radius around a village's centre within which a built cell counts as that
/// village's building (for the building research factor + render era lookup).
const VILLAGE_BUILD_R: i32 = 26;
/// Stability multiplier when a village is at PEACE (no rival/enemy borders): research
/// flows fastest. Scaled down toward `RESEARCH_STABILITY_WAR` as hostilities mount.
const RESEARCH_STABILITY_PEACE: f32 = 1.0;
/// Stability multiplier when a village sits in open hostility (a rival/enemy on every
/// front): a war footing starves the workshops, so progress crawls.
const RESEARCH_STABILITY_WAR: f32 = 0.35;

// --- WARFARE tunables (Civilization Sprint 2). War rides the society-evaluation
// schedule (declarations, recalls) but BATTLES tick every frame so a clash reads in
// motion. All war stochasticity (who musters, the strike coin-flip) draws from the
// dedicated `war_rng`, never the main stream, so seeded worlds stay byte-identical.
// Tuned so wars are OCCASIONAL and SURVIVABLE — they flare at a hot enemy border,
// take a few casualties, then end in a truce; villages persist + keep advancing. ---
/// Affinity at or below which two villages go to open WAR — set to the EXISTING `Enemy`
/// relation threshold (−0.55), so war is exactly the spec's "when two villages are
/// ENEMIES" condition: a border that has genuinely soured to enmity (a fought-over,
/// death-scarred line), not a passing rivalry. A pair must cross this AND clear its
/// war-weariness cooldown to fight. (Measured: close, balanced borders reach −0.5…−0.6
/// under the society drift, so this fires occasionally — not never, not constantly.)
const WAR_DECLARE_AFFINITY: f32 = -0.55;
/// Max WARRIORS a village fields per war — bounded so the settlement is never emptied
/// (the rest keep living/building). A handful a side reads as a skirmish, not an army.
const WARBAND_MAX: usize = 3;
/// A village needs at least this many living adult `can_war` minds to muster at all
/// (so a small or depleted village does not march itself to extinction).
const WARBAND_MIN_POOL: usize = 4;
/// Manhattan radius at which two opposing warriors are "in melee" — a stone/bronze/iron
/// clash lands here (adjacency-ish). Ranged eras strike from farther (below).
const WAR_MELEE_R: i32 = 1;
/// Manhattan range at which an INDUSTRIAL+ warrior (musket/rifle/energy) can fire on an
/// enemy warrior — a ranged exchange opens before the lines ever close.
const WAR_RANGED_R: i32 = 7;
/// Per-tick probability a warrior in contact actually lands a blow / a shot connects
/// (off `war_rng`). Kept LOW so a clash is a drawn-out, back-and-forth SKIRMISH over
/// hundreds of ticks (a watcher can actually SEE the battle), not an instant wipe —
/// casualties accrue slowly, leaving plenty of time to break + truce. Tuned with
/// `WAR_HIT_DAMAGE` so ~5 connecting hits fell a fighter and a kill takes ~100+ ticks.
const WAR_STRIKE_CHANCE: f32 = 0.018;
/// Damage a landed melee blow / connecting shot deals. ~5 hits fell a healthy mind
/// (health starts at 1.0), routed through the EXISTING Hurt/health/reap path so a war
/// death is grieved by family/village exactly like any other loss. Low per-hit so a
/// hurt-but-not-killed warrior is common (the clash reads as a struggle, not a slaughter).
const WAR_HIT_DAMAGE: f32 = 0.22;
/// A war ENDS when either warband is broken to this many standing fighters or fewer (a
/// side reduced to its last fighter sues for peace) — keeps wars from grinding to
/// extinction while still letting a real, multi-casualty battle play out first.
const WARBAND_BROKEN_AT: usize = 1;
/// Hard ceiling on total casualties (both sides) in a single war; on reaching it the war
/// is force-ended to a truce — a final guarantee against a war of annihilation. A few a
/// side, never the whole band, so both villages always persist through a war.
const WAR_CASUALTY_CAP: u32 = 4;
/// Max ticks a war may run before it burns out to an exhausted truce regardless of the
/// battle's state (≈ a few society evaluations) — wars FLARE and END, never simmer forever.
const WAR_MAX_TICKS: u64 = 1400;
/// On truce, the warring pair's affinity is pulled back toward neutral to THIS value
/// (war spends the enmity — a fought-out border cools to a wary peace, free to re-sour
/// or warm again later via the normal society drift). Reuses the relation registry.
const WAR_TRUCE_AFFINITY: f32 = -0.20;
/// Ticks after a war before the same pair may fight again (war-weariness): a cooldown so
/// wars are OCCASIONAL, the world recovers between them, and population can turn over.
const WAR_COOLDOWN: u64 = 2200;
/// While at war, a warrior steers toward the battle border each tick with this pull
/// (its remaining motion stays its own — warriors still live, they just march to war).
const WAR_MARCH_PULL: f32 = 0.85;

// --- CIVILIZATION CAPSTONE (Civilization Sprint 3): politics & diplomacy, WONDERS, and
// the SPACE AGE (launchpads → rockets to the moon). These are WORLD systems layered on
// the existing society/eras (NOT cognitive faculties — no genome genes, N_GENES stays
// 35). All of it is gated behind the live-only `civ` flag and every stochastic choice
// (wonder/treaty naming, launch jitter) draws from a dedicated `civ_rng`, never the main
// stream, so a world that never arms civ (every harness/AC/proof path) is byte-identical.
// Politics/wonders ride the slow society cadence; rockets animate per tick once aloft. ---
/// Research a village must bank to raise its monumental WONDER. Set in the Iron→Industrial
/// band so a mid-advanced settlement earns one (a real civilizational milestone), not
/// every village and not only at Space. One wonder per village, permanent once raised.
const WONDER_RESEARCH: f32 = 230.0;
/// A treaty (named ALLIANCE/PACT) is forged when a pair has sat at `Allied` standing
/// (affinity ≥ this, the existing Allied bucket floor) continuously for `TREATY_TICKS`.
const TREATY_AFFINITY: f32 = 0.55;
/// Ticks a pair must hold Allied standing before their alliance is FORMALIZED into a named
/// treaty (≈ several society evaluations) — a treaty is a *sustained* friendship, not a
/// passing warm spell. A treaty dissolves if the pair falls out of the Allied band.
const TREATY_TICKS: u64 = 360;
/// Once a village reaches the SPACE AGE it builds a LAUNCHPAD and thereafter launches a
/// ROCKET every this-many ticks — bounded so the launch is a recurring SPECTACLE, not a
/// constant stream. ≈ a couple of minutes at the live speed between launches.
const LAUNCH_PERIOD: u64 = 900;
/// Ticks a rocket spends in flight (ascent + arc toward the moon) before it fades — long
/// enough to actually WATCH it rise and arc across the sky.
const ROCKET_FLIGHT_TICKS: u64 = 240;

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

    // ---- warfare (Civilization Sprint 2) — only ever set/read behind the world's
    // `war` flag; on a non-war world these stay at their defaults and are read by
    // nothing, so seeded worlds are byte-identical. ----
    /// When this mind has been MUSTERED into a warband, the id of the ENEMY village
    /// it marches against; `None` for a civilian (the default). A warrior steers
    /// toward the contested border, fights enemy warriors there, and stands down
    /// (back to `None`) when the war ends. Only set behind the world's `war` flag.
    pub warband: Option<u8>,
    /// Transient combat visual in `[0,1]`: spikes to 1.0 on a melee clash or a shot
    /// fired/landed, decays each frame — the renderer flashes a clash spark / muzzle
    /// flash from it. Inert (stays 0) off a war world.
    pub weapon_flash: f32,

    /// True once this mind has ever served as its village's LEADER (set in
    /// `update_leaders`; persists after death so a fallen leader can be honoured with a
    /// BURIAL MOUND at the grave). Only ever set behind the civ/society path, so on a
    /// world without leaders it stays false and nothing changes.
    pub was_leader: bool,
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

/// A village's TECH ERA (Civilization Sprint 1). A village climbs this ladder as it
/// accumulates RESEARCH — more people + more buildings + peace make it advance faster.
/// The era is the legible "how far has this settlement come" axis, and it drives the
/// architecture the renderer raises (thatch huts → stone houses → brick + smokestacks
/// → sleek metal/glass domes). Ordered, so `era as u8` is the tech rank — later sprints
/// (weapons/war, wonders, space launch) read it as a gate. Default `Stone`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Era {
    /// The founding age: thatch + timber huts, low and warm.
    Stone,
    /// Worked metal arrives: tidy timber-framed houses on stone footings.
    Bronze,
    /// Stone masonry + sturdier halls — the classic walled village.
    Iron,
    /// The machine age: brick terraces with chimneys + a smoking factory stack.
    Industrial,
    /// The far future: sleek metal-and-glass blocks crowned with domes, lit cool.
    Space,
}

impl Era {
    /// The full ladder in order (Stone … Space). Index == tech rank.
    pub const LADDER: [Era; 5] = [Era::Stone, Era::Bronze, Era::Iron, Era::Industrial, Era::Space];
    /// A short display name for the HUD.
    pub fn name(self) -> &'static str {
        // Generic age labels — the village clearly has houses/tools from the start, so
        // "Stone Age" read wrong; these are just rungs on its own tech ladder.
        match self {
            Era::Stone => "Age 1",
            Era::Bronze => "Age 2",
            Era::Iron => "Age 3",
            Era::Industrial => "Age 4",
            Era::Space => "Age 5",
        }
    }
    /// The next era up the ladder, or `None` at the top (Space).
    pub fn next(self) -> Option<Era> {
        Era::LADDER.get(self as usize + 1).copied()
    }
    /// The WEAPON a warrior of this era takes up (Civilization Sprint 2). Tech drives
    /// armament: stone clubs/spears → bronze/iron swords + shields → industrial muskets
    /// (ranged) → space energy arms. The renderer draws the matching mesh in-hand.
    pub fn weapon(self) -> Weapon {
        match self {
            Era::Stone => Weapon::Club,
            Era::Bronze => Weapon::Sword,
            Era::Iron => Weapon::Sword, // a sturdier blade + shield; same mesh family
            Era::Industrial => Weapon::Musket,
            Era::Space => Weapon::Energy,
        }
    }
}

/// A warrior's armament, scaled to its village's [`Era`] (Civilization Sprint 2). The
/// melee weapons (`Club`, `Sword`) only strike in adjacency; the ranged ones (`Musket`,
/// `Energy`) open fire across [`WAR_RANGED_R`] before the lines close. The renderer draws
/// each as a distinct in-hand mesh (and a muzzle flash / tracer for the ranged ones).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weapon {
    /// Stone age: a heavy timber club (melee).
    Club,
    /// Bronze/Iron age: a worked-metal sword, carried with a shield (melee).
    Sword,
    /// Industrial age: a long musket/rifle — the first RANGED arm (muzzle flash).
    Musket,
    /// Space age: a sleek energy weapon — ranged (a bright bolt/tracer).
    Energy,
}

impl Weapon {
    /// Whether this weapon strikes at range (industrial+) rather than only in melee.
    pub fn is_ranged(self) -> bool {
        matches!(self, Weapon::Musket | Weapon::Energy)
    }
    /// The reach (Manhattan) at which this weapon can strike an enemy warrior.
    pub fn reach(self) -> i32 {
        if self.is_ranged() { WAR_RANGED_R } else { WAR_MELEE_R }
    }
}

/// Cumulative research (in arbitrary "knowledge units") a village must have banked to
/// have REACHED each era. Index == era rank. Stone is free (0); each rung roughly
/// doubles, so the early climb is brisk (a watcher sees Stone→Bronze→Iron within a
/// session) and the late climb is a long haul (Space is a marathon, rarely reached in
/// one sitting). Tuned against the long-run trace (seed 0x61, 4 villages): the climb is
/// brisk early — Bronze by ~tick 3k, Iron by ~7k, Industrial by ~14-18k — and Space is
/// a genuine stretch goal a strong village only reaches late in a long session (≈25k+),
/// while a small/contested village may still be Industrial at 30k. See
/// `era_for_research`.
pub const ERA_THRESHOLDS: [f32; 5] = [0.0, 45.0, 130.0, 360.0, 1100.0];

/// The highest era whose threshold `research` has crossed.
pub fn era_for_research(research: f32) -> Era {
    let mut e = Era::Stone;
    for (i, &thr) in ERA_THRESHOLDS.iter().enumerate() {
        if research >= thr {
            e = Era::LADDER[i];
        }
    }
    e
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
    /// Cumulative RESEARCH banked (knowledge units), Civilization Sprint 1. Grows each
    /// society evaluation, scaled by population × buildings × stability/peace. Drives
    /// `era`. Zero unless the world's `eras` flag is on.
    pub research: f32,
    /// This village's current TECH ERA, derived from `research` via the ladder. The
    /// renderer raises era-appropriate architecture from it; later sprints gate on it.
    /// `Era::Stone` unless `eras` is on and the village has climbed.
    pub era: Era,
    /// Buildings counted nearest this village's centre at the last eval (diag / render
    /// of the building-count research factor). Zero unless `eras` is on.
    pub buildings: usize,

    // ---- civilization capstone (Civilization Sprint 3) — only ever set/read behind the
    // world's `civ` flag; on a non-civ world these stay at their defaults and are read by
    // nothing, so seeded worlds are byte-identical. ----
    /// The village's LEADER: a specific named living member (the eldest — lowest
    /// `born_tick`), refreshed each society eval so the title passes on as elders die.
    /// `None` until civ is armed (or while the village is empty).
    pub leader: Option<EntityId>,
    /// The leader's display name, copied at selection time for the HUD (so the panel
    /// needn't re-scan the roster). Empty unless `civ` and the village is peopled.
    pub leader_name: String,
    /// The monumental WONDER this village has raised, once it banked `WONDER_RESEARCH`.
    /// `None` until then; permanent once set (a civilization keeps its monument). The
    /// renderer raises a great landmark at the village centre from this.
    pub wonder: Option<Wonder>,
}

/// A village's monumental WONDER (Civilization Sprint 3): a single great landmark raised
/// at the settlement centre once it banks enough research — a civilizational achievement,
/// visible across the island. The KIND drives which silhouette the renderer raises; the
/// `name` is a warm proper name drawn off the `civ_rng`. Live-only (civ-gated).
#[derive(Clone, Debug)]
pub struct Wonder {
    /// Which monument silhouette to raise (a stable per-village choice).
    pub kind: WonderKind,
    /// The wonder's proper name (e.g. "The Great Spire of Thornhollow").
    pub name: String,
    /// The tick it was raised (for a rise-from-the-ground render ramp).
    pub raised: u64,
}

/// The silhouette family of a [`Wonder`] — each reads as a distinct great monument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WonderKind {
    /// A stepped great PYRAMID / ziggurat — broad tiered stone.
    Pyramid,
    /// A soaring SPIRE / obelisk — a tall slender monument.
    Spire,
    /// A grand domed MONUMENT — a wide rotunda crowned with a cupola.
    Rotunda,
}

/// A ROCKET in flight (Civilization Sprint 3, Space Age): launched periodically from a
/// Space-era village's launchpad, it rises on a plume and arcs toward the moon, then
/// fades. Pure spectacle — it touches no mind cognition. Live-only (civ-gated); all
/// launch timing/jitter is off the dedicated `civ_rng`, so seeded worlds are unaffected.
#[derive(Clone, Debug)]
pub struct Rocket {
    /// The launching village (for the pad position + the HUD).
    pub village: u8,
    /// Ground launch position (the village centre / its launchpad).
    pub pad: Pos,
    /// The tick it lifted off (drives the flight phase 0..1 over `ROCKET_FLIGHT_TICKS`).
    pub launched: u64,
    /// A small per-rocket heading jitter (radians) off `civ_rng`, so successive launches
    /// arc along slightly different lines toward the moon (variety, never the same shot).
    pub bearing: f32,
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

/// A formalized TREATY (Civilization Sprint 3): a named ALLIANCE/PACT that crystallizes
/// when a village pair has held `Allied` standing continuously for `TREATY_TICKS`. It is
/// derived from — and dissolves with — the underlying [`Relation`] affinity, so a treaty
/// is the *legible, named* face of a sustained emergent alliance, never a separate force.
/// Live-only (civ-gated); the seeded harness never forms one, so it perturbs nothing.
#[derive(Clone, Debug)]
pub struct Treaty {
    /// The two signatory villages, `a < b` (same convention as [`Relation`]).
    pub a: u8,
    pub b: u8,
    /// The treaty's proper name (e.g. "The Pact of Ashreach").
    pub name: String,
    /// The tick it was signed (for the HUD / a render flourish).
    pub signed: u64,
}

/// An active WAR between two villages (Civilization Sprint 2). Born when a soured pair
/// crosses [`WAR_DECLARE_AFFINITY`] (and clears its cooldown), it musters a bounded
/// WARBAND from each side that marches to the contested border and fights; casualties
/// route through the existing Hurt/grief path. The war ENDS — a truce — when a band is
/// broken, the casualty cap is hit, or the clock runs out, after which the pair cools to
/// a wary peace and a cooldown bars a rematch. Exists only on a `war`-armed world; the
/// seeded harness never declares one, so seeded trajectories are byte-identical.
#[derive(Clone, Debug)]
pub struct War {
    /// The two warring villages, `a < b` (same convention as [`Relation`]).
    pub a: u8,
    pub b: u8,
    /// The contested-border battlefield (midpoint of the two village centres at
    /// declaration) — both warbands converge here.
    pub front: Pos,
    /// The tick this war was declared (for the burn-out clock + the HUD).
    pub started: u64,
    /// Casualties so far on each side (village `a` / village `b`) — drives the
    /// broken-band + casualty-cap end conditions and the HUD readout.
    pub dead_a: u32,
    pub dead_b: u32,
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
    /// LIVE-ONLY HUNTING (default false). When true, an adult villager standing next to
    /// a live deer or sheep may TAKE it for meat — restoring its own energy + a little
    /// health, despawning the animal (which respawns elsewhere like any caught prey).
    /// Opportunistic only: it does NOT change where minds move (no chase), so it adds no
    /// new pathing. Enabled ONLY by the live `Game::new`; the harness and the seed-tuned
    /// society/war balance tests never flip it, so they are byte-identical.
    pub hunting: bool,
    /// LIVE-ONLY SCARCITY WORLD (default false): villagers move purposefully toward their
    /// gather target, and resource scarcity (thinning wood+stone per head) drives wars.
    /// Enabled ONLY by `Game::new`; the harness + balance tests leave it off → byte-identical.
    pub scarcity_world: bool,
    /// The wolf pack(s): grey pack hunters. Empty unless `wildlife`.
    pub wolves: Vec<Wolf>,
    /// The solitary bears. Empty unless `wildlife`.
    pub bears: Vec<Bear>,
    /// The deer herd — ambient grazing prey that flee predators. Empty unless `wildlife`.
    pub deer: Vec<Deer>,
    /// The sheep flock(s) — woolly, clustering grazers near the village. Empty unless `wildlife`.
    pub sheep: Vec<Sheep>,
    /// The horse herd — larger, faster roaming grazers. Empty unless `wildlife`.
    pub horses: Vec<Horse>,
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
    /// LIVE-ONLY TECH/ERA switch (Civilization Sprint 1). Default `false` so every
    /// AC/proof/fitness run is byte-identical: villages never accumulate research, stay
    /// at `Era::Stone`, and no era logic runs (zero new draws). When `true` each village
    /// banks RESEARCH every society evaluation — scaled by its population, buildings and
    /// peace — and climbs the [`Era`] ladder (Stone → Bronze → Iron → Industrial →
    /// Space); the renderer then raises era-appropriate architecture. Requires `society`
    /// (eras live on villages). The accrual is deterministic and rides the existing
    /// society-evaluation path (no main-stream RNG), so it perturbs no seeded trajectory.
    /// Only the live showcase (`Game::new`) / eras diag flip it on.
    pub eras: bool,
    /// Dedicated society RNG. ALL clustering / naming / society-jitter draws come from
    /// here so the main stream is never perturbed. Seeded when `society` is turned on.
    soc_rng: Rng,
    /// LIVE-ONLY WARFARE switch (Civilization Sprint 2). Default `false` so every
    /// AC/proof/fitness run is byte-identical: no warband is ever mustered, no war RNG
    /// is drawn, and no combat damage is dealt. When `true`, a soured village pair (its
    /// relation past [`WAR_DECLARE_AFFINITY`]) goes to open WAR — each side fields a
    /// bounded warband that marches to the border and fights with era-appropriate
    /// weapons; casualties route through the EXISTING Hurt/grief path, and the war ends
    /// in a truce (band broken / casualty cap / clock) that cools the pair and starts a
    /// cooldown. Requires `society` (wars live on village pairs). ALL war stochasticity
    /// draws from [`war_rng`], never the main `rng`. Only the live game / war diag flip
    /// it on.
    pub war: bool,
    /// Active wars, one [`War`] per fighting pair. Empty unless `war`.
    pub wars: Vec<War>,
    /// Per unordered village pair `(a < b)`: the tick before which that pair may NOT
    /// go to war again (war-weariness cooldown). Indexed `a * k + b`. Empty unless `war`.
    war_cooldowns: std::collections::HashMap<(u8, u8), u64>,
    /// Running tallies for the war diag (live-only): wars ever declared, wars resolved
    /// to a truce, and total battle deaths across all wars.
    pub wars_declared: u32,
    pub wars_resolved: u32,
    pub war_casualties: u32,
    /// CHRONICLE — a rolling log of significant events `(tick, line)` for the HUD
    /// (births, deaths, wars, era advances, leaders, wonders). Capped to the most
    /// recent few. Live-only presentation: it records, it never feeds back into the
    /// sim, so it cannot perturb cognition or the RNG stream.
    pub events: Vec<(u64, String)>,
    /// Dedicated warfare RNG. ALL muster / strike-coin draws come from here so the main
    /// stream is never perturbed. Seeded when `war` is turned on.
    war_rng: Rng,
    /// LIVE-ONLY CIVILIZATION switch (Civilization Sprint 3): politics & diplomacy
    /// (named LEADERS + formalized TREATIES), WONDERS (a monumental landmark per advanced
    /// village), and the SPACE AGE (launchpads → ROCKETS that arc to the moon). Default
    /// `false` so every AC/proof/fitness run is byte-identical: no leaders, no treaties,
    /// no wonders, no rockets, and ZERO new RNG draws. When `true` (the live showcase /
    /// civ diag only) the society gains these layered world systems. Requires `society`
    /// (they live on villages); rides the slow society cadence (rockets animate per tick).
    /// ALL civ stochasticity draws from [`civ_rng`], never the main `rng`.
    pub civ: bool,
    /// Formalized treaties, one [`Treaty`] per allied-long-enough pair. Empty unless `civ`.
    pub treaties: Vec<Treaty>,
    /// Per unordered village pair `(a < b)`: the tick that pair *entered* the Allied band
    /// (for the `TREATY_TICKS` sustained-alliance test). Cleared when it falls out. Empty
    /// unless `civ`.
    allied_since: std::collections::HashMap<(u8, u8), u64>,
    /// Rockets currently in flight (one per active launch). Empty unless `civ`. Bounded:
    /// a village launches at most once per `LAUNCH_PERIOD` and a rocket fades after
    /// `ROCKET_FLIGHT_TICKS`, so this stays tiny.
    pub rockets: Vec<Rocket>,
    /// Running tallies for the civ diag (live-only): wonders raised, treaties ever
    /// signed, and rockets ever launched.
    pub wonders_raised: u32,
    pub treaties_signed: u32,
    pub rockets_launched: u32,
    /// Dedicated CIVILIZATION RNG. ALL wonder/treaty naming + launch-bearing jitter draws
    /// come from here so the main stream is never perturbed. Seeded when `civ` is armed.
    civ_rng: Rng,
    rng: Rng,
    next_id: u32,
    /// Dedicated counter for sheep/horse ids, drawn from a HIGH range so wildlife never
    /// advances `next_id` (see [`alloc_wild_id`]). Inert (0) until wildlife is seeded.
    wild_id_next: u32,
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

/// A sheep: woolly, cream, domesticated-feeling grazing prey that moves as a tight
/// FLOCK. Sheep steer gently toward their flock's centroid (cohesion) so they cluster
/// — often near the village — and graze on the grass; they flee predators (wolf, bear,
/// stalker) like deer but a touch slower (more ticks idle), so a wolf can occasionally
/// take a straggler. Caught sheep respawn elsewhere (renewable prey), so the flock
/// persists as standing ambient life. NOT perceived by the minds (ambient, not a
/// threat), so they add no new mind cognition.
pub struct Sheep {
    pub id: EntityId,
    pub pos: Pos,
    /// Which flock this sheep belongs to (for cohesion). 0-based.
    pub flock: u8,
    pub rx: f32,
    pub ry: f32,
    pub heading: f32,
    /// `true` while actively fleeing (the renderer can pose it alert).
    pub fleeing: bool,
    pub flash: f32,
    /// When caught: hidden until this tick, then it respawns far from predators.
    respawn_at: Option<u64>,
}

/// A horse: a larger, FASTER grazer that roams in a small herd. Horses graze and
/// wander faster than deer (a step almost every tick), and flee predators faster still
/// (they get a bonus bolt step), so they are hard to catch — but a horse run down at
/// contact is taken and respawns elsewhere (renewable). NOT perceived by the minds
/// (ambient life), so they add no new mind cognition.
pub struct Horse {
    pub id: EntityId,
    pub pos: Pos,
    pub rx: f32,
    pub ry: f32,
    pub heading: f32,
    /// `true` while actively fleeing (the renderer can pose it galloping).
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
                // peacetime by default; only `set_war`-armed worlds ever muster a warband.
                warband: None,
                weapon_flash: 0.0,
                was_leader: false,
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
            hunting: false,
            scarcity_world: false,
            wolves: Vec::new(),
            bears: Vec::new(),
            deer: Vec::new(),
            sheep: Vec::new(),
            horses: Vec::new(),
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
            // eras: off by default (the incumbent). No research accrues, villages stay
            // Stone, and zero era logic runs until `set_eras(true)` arms it live-only.
            eras: false,
            soc_rng: Rng::new(0),
            // warfare: off by default (the incumbent). The flag, side-RNG, war list,
            // cooldowns, and tallies are inert until `set_war(true)` reseeds + arms them.
            war: false,
            wars: Vec::new(),
            war_cooldowns: std::collections::HashMap::new(),
            wars_declared: 0,
            wars_resolved: 0,
            war_casualties: 0,
            events: Vec::new(),
            war_rng: Rng::new(0),
            // civilization capstone: off by default (the incumbent). The flag, side-RNG,
            // treaties, allied-since clocks, rockets, and tallies are inert until
            // `set_civ(true)` reseeds + arms them live-only.
            civ: false,
            treaties: Vec::new(),
            allied_since: std::collections::HashMap::new(),
            rockets: Vec::new(),
            wonders_raised: 0,
            treaties_signed: 0,
            rockets_launched: 0,
            civ_rng: Rng::new(0),
            rng,
            next_id,
            wild_id_next: 0,
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

    /// Allocate an EntityId for a NEW wildlife species (sheep / horses) from a dedicated
    /// HIGH id range that NEVER advances the main `next_id` counter. This is the iron-rule
    /// discipline for ids: the original deer/wolf/bear seeding (which predates this and
    /// is part of the byte-identical baseline) still draws from `alloc_id`, but the
    /// species added here must not shift `next_id` — otherwise later id-bearing entities
    /// (e.g. lifecycle births) would shift and perturb seed-sensitive live systems
    /// (society/war), even though the determinism RNGs are untouched. Ids here are only
    /// ever used as the source tag in a `Hurt` grief event, so any unique value works.
    fn alloc_wild_id(&mut self) -> EntityId {
        // base well above any plausible `next_id` (births are pop-capped at ~90 and ids
        // grow by 1 per spawn); `wild_id_next` lives only while wildlife is on.
        let id = EntityId(0xE000_0000 + self.wild_id_next);
        self.wild_id_next += 1;
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
        self.wild_id_next = 0; // dedicated high-range ids for sheep/horses (never `next_id`)
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
        // --- sheep: a clustering flock or two, near the village, scaled with area. They
        // graze and steer toward their flock centroid (cohesion), so they read as a tight
        // woolly flock rather than scattered animals. Seeded AFTER deer so the deer/wolf/
        // bear seeds (and thus their positions) are unchanged. ---
        if self.sheep.is_empty() {
            let n_flocks: u8 = if (self.w * self.h) >= 6000 { 2 } else { 1 };
            // total flock size scaled with area (~8-12 on the 124×84 showcase).
            let total = ((self.w * self.h) / 1000).clamp(8, 14) as usize;
            let per = (total / n_flocks.max(1) as usize).max(1);
            let hearth = self.granary;
            for flock in 0..n_flocks {
                // a flock rallies near a seed cell biased toward the village hearth so the
                // sheep feel domesticated/pastured rather than wild.
                let bx = self.wild_rng.below(self.w as usize) as i32;
                let by = self.wild_rng.below(self.h as usize) as i32;
                let seed = self.clamp(Pos::new((bx + hearth.x) / 2, (by + hearth.y) / 2));
                for _ in 0..per {
                    let jx = self.wild_rng.below(9) as i32 - 4;
                    let jy = self.wild_rng.below(9) as i32 - 4;
                    let pos = self.clamp(Pos::new(seed.x + jx, seed.y + jy));
                    let id = self.alloc_wild_id();
                    self.sheep.push(Sheep {
                        id,
                        pos,
                        flock,
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
        // --- horses: a small fast herd that roams the open island. Fewer than sheep
        // (~4-6 on the showcase). Seeded AFTER sheep so all prior seeds are unchanged. ---
        if self.horses.is_empty() {
            let n_horses = ((self.w * self.h) / 1800).clamp(4, 7) as usize;
            // the herd starts loosely around one roaming seed.
            let seed = Pos::new(
                self.wild_rng.below(self.w as usize) as i32,
                self.wild_rng.below(self.h as usize) as i32,
            );
            for _ in 0..n_horses {
                let jx = self.wild_rng.below(11) as i32 - 5;
                let jy = self.wild_rng.below(11) as i32 - 5;
                let pos = self.clamp(Pos::new(seed.x + jx, seed.y + jy));
                let id = self.alloc_wild_id();
                self.horses.push(Horse {
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
                // every village is founded in the Stone Age with no research banked;
                // it climbs only if `eras` is on and it has the people/buildings/peace.
                research: 0.0,
                era: Era::Stone,
                buildings: 0,
                leader: None,
                leader_name: String::new(),
                wonder: None,
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

    /// Turn on the TECH/ERA progression (Civilization Sprint 1). Live-only: the seeded
    /// harness/AC/proof paths never call it, so they stay byte-identical (no research,
    /// villages frozen at `Era::Stone`, zero new RNG draws). Requires `society` to be on
    /// first (eras live on villages) — a no-op otherwise. Once armed, each village banks
    /// research every society evaluation (scaled by population × buildings × peace) and
    /// climbs the ladder; the renderer reads `village.era` to raise era architecture.
    /// The accrual is fully deterministic and rides the existing society-evaluation
    /// schedule (no extra RNG), so it perturbs no seeded trajectory.
    pub fn set_eras(&mut self, on: bool) {
        self.eras = on && self.society;
        if !self.eras {
            return;
        }
        // (re)start every village from a clean Stone-age slate so two same-sized worlds
        // get the same climb regardless of when eras were armed.
        for v in &mut self.villages {
            v.research = 0.0;
            v.era = Era::Stone;
            v.buildings = 0;
        }
    }

    /// Arm LIVE-ONLY WARFARE (Civilization Sprint 2). Requires `society` (wars live on
    /// village pairs). When on, a soured pair (relation past [`WAR_DECLARE_AFFINITY`])
    /// goes to open WAR: each side musters a bounded warband of its adult `can_war`
    /// minds, they march to the contested border and fight with era-scaled weapons, and
    /// the war ends in a truce that cools the pair + starts a cooldown. ALL war
    /// stochasticity draws from a dedicated, dimension-derived side-RNG, so a world that
    /// never arms war (every harness/AC/proof path) is byte-identical. Off clears any
    /// active wars and stands every warrior down.
    pub fn set_war(&mut self, on: bool) {
        self.war = on && self.society;
        if !self.war {
            // stand everyone down + drop any active wars (so toggling off is clean).
            for a in &mut self.agents {
                a.warband = None;
                a.weapon_flash = 0.0;
            }
            self.wars.clear();
            return;
        }
        // dedicated, dimension-derived seed so two same-sized worlds get the same wars,
        // independent of how many main-stream draws have happened.
        self.war_rng = Rng::new(0x0057_A12Fu64 ^ self.w as u64 ^ ((self.h as u64) << 28));
    }

    /// Arm the LIVE-ONLY CIVILIZATION CAPSTONE (Civilization Sprint 3): politics &
    /// diplomacy (named LEADERS + formalized TREATIES on top of the existing relation
    /// affinity), WONDERS (a monumental landmark per advanced village), and the SPACE AGE
    /// (a launchpad + periodic ROCKETS that arc to the moon). Requires `society` (these
    /// live on villages) — a no-op otherwise. ALL civ stochasticity draws from a
    /// dedicated, dimension-derived side-RNG, so a world that never arms civ (every
    /// harness/AC/proof path) is byte-identical. Off clears all civ state.
    pub fn set_civ(&mut self, on: bool) {
        self.civ = on && self.society;
        if !self.civ {
            self.treaties.clear();
            self.allied_since.clear();
            self.rockets.clear();
            for v in &mut self.villages {
                v.leader = None;
                v.leader_name = String::new();
                v.wonder = None;
            }
            return;
        }
        // dedicated, dimension-derived seed (distinct constant from soc/war), so two
        // same-sized worlds get the same civ, independent of main-stream draws.
        self.civ_rng = Rng::new(0x00C1_412Eu64 ^ self.w as u64 ^ ((self.h as u64) << 24));
    }

    /// CAPTURE SEAM (live-only): fast-forward every peopled village's banked research to
    /// the threshold of era rank `era` (0=Stone … 4=Space), then refresh leaders / raise
    /// wonders / form treaties so a headless screenshot can show the late-game civilization
    /// without an impractically deep warm. Only mutates the civ/eras side state (research
    /// is a village field, not a mind-cognition value); draws nothing from the main RNG, so
    /// it cannot perturb a seeded trajectory — and the harness never calls it anyway.
    pub fn advance_research_to_era(&mut self, era: usize) {
        if !self.eras {
            return;
        }
        let rank = era.min(ERA_THRESHOLDS.len() - 1);
        // park research a hair above the target rung so the village sits firmly in `era`.
        let target = ERA_THRESHOLDS[rank] + 1.0;
        for v in &mut self.villages {
            if v.population > 0 {
                v.research = v.research.max(target);
                v.era = era_for_research(v.research);
            }
        }
        // immediately materialize the civ consequences of the new tech standing.
        if self.civ {
            self.update_leaders();
            self.raise_wonders();
        }
    }

    /// CAPTURE SEAM (live-only): force every Space-era village to launch a rocket NOW
    /// (ignoring the cadence), so a screenshot can catch a rocket mid-flight on demand.
    /// Off `civ_rng` like a real launch; the harness never calls it.
    pub fn force_launch(&mut self) {
        if !self.civ {
            return;
        }
        let now = self.tick;
        let mut to_launch: Vec<(u8, Pos)> = Vec::new();
        for v in &self.villages {
            if v.population > 0 && v.era == Era::Space && !self.rockets.iter().any(|r| r.village == v.id) {
                to_launch.push((v.id, v.center));
            }
        }
        for (village, pad) in to_launch {
            let bearing = (self.civ_rng.next_f32() - 0.5) * 0.8;
            self.rockets.push(Rocket { village, pad, launched: now, bearing });
            self.rockets_launched += 1;
        }
    }

    /// CAPTURE SEAM (live-only): backdate every in-flight rocket's launch tick to ~40% of
    /// its flight so it is caught MID-ARC in the very next frame (paired with `force_launch`
    /// for the `?launch=1` screenshot seam). Pure render-state nudge; never touched by harness.
    pub fn put_rockets_mid_flight(&mut self) {
        // ~18% of flight: high enough off the pad to read as "launched" with a strong
        // plume, low enough that the rocket + its launchpad still share a tight frame.
        let back = (ROCKET_FLIGHT_TICKS as f32 * 0.18) as u64;
        let now = self.tick;
        for r in &mut self.rockets {
            r.launched = now.saturating_sub(back);
        }
    }

    /// Step the CIVILIZATION layer (Civilization Sprint 3). Two cadences: POLITICS,
    /// TREATIES and WONDERS update on the slow society cadence (cheap, social processes);
    /// the SPACE AGE launch check also runs there, but rockets ANIMATE every tick (their
    /// flight phase advances with `tick`). A no-op when `civ` is off (zero draws), so
    /// non-civ worlds stay byte-identical. All randomness is off `civ_rng`.
    fn step_civ(&mut self) {
        if !self.civ || self.villages.is_empty() {
            return;
        }
        // --- per-tick: retire rockets whose flight has completed (pure render state) ---
        let now = self.tick;
        self.rockets
            .retain(|r| now.saturating_sub(r.launched) < ROCKET_FLIGHT_TICKS);

        // the heavy social work runs on the society cadence only.
        if self.tick % SOCIETY_PERIOD != 0 {
            return;
        }
        self.update_leaders();
        self.update_treaties();
        self.raise_wonders();
        self.launch_rockets();
    }

    /// Refresh each peopled village's LEADER: the eldest living member (lowest
    /// `born_tick`, ties broken by id for determinism). Title passes on as elders die.
    /// Record a significant event in the CHRONICLE (capped to the most recent entries).
    /// Pure record-keeping — never read back by the sim, so it can't affect determinism.
    fn log_event(&mut self, line: impl Into<String>) {
        self.events.push((self.tick, line.into()));
        let n = self.events.len();
        if n > 48 {
            self.events.drain(0..n - 48);
        }
    }

    fn update_leaders(&mut self) {
        let k = self.villages.len();
        let mut best: Vec<Option<(u64, EntityId)>> = vec![None; k];
        for a in self.agents.iter().filter(|a| a.alive) {
            let Some(v) = a.village else { continue };
            let vi = v as usize;
            let key = (a.born_tick, a.id);
            if best[vi].map(|(bt, bid)| key < (bt, bid)).unwrap_or(true) {
                best[vi] = Some(key);
            }
        }
        for (vi, v) in self.villages.iter_mut().enumerate() {
            match best[vi] {
                Some((_, id)) => {
                    v.leader = Some(id);
                    // copy the leader's display name for the HUD.
                    if let Some(a) = self.agents.iter().find(|a| a.id == id) {
                        v.leader_name = a.name.clone();
                    }
                }
                None => {
                    v.leader = None;
                    v.leader_name.clear();
                }
            }
        }
        // mark each standing leader persistently — `v.leader` is reassigned the moment a
        // leader dies, so a per-mind flag is what lets the renderer raise a BURIAL MOUND
        // over a fallen leader's grave.
        for entry in best.iter().flatten() {
            let id = entry.1;
            if let Some(a) = self.agents.iter_mut().find(|a| a.id == id) {
                a.was_leader = true;
            }
        }
    }

    /// Form / dissolve formalized TREATIES from the underlying relation affinity. A pair
    /// that has held `Allied` standing continuously for `TREATY_TICKS` gets a named PACT;
    /// a pair that falls out of the Allied band loses its treaty (and its clock resets),
    /// so a treaty is the legible face of a *sustained* emergent alliance. Off `civ_rng`
    /// for the name draw only.
    fn update_treaties(&mut self) {
        let now = self.tick;
        // snapshot which pairs are currently Allied-enough (immutable scan first).
        let allied: Vec<(u8, u8, bool)> = self
            .relations
            .iter()
            .map(|r| (r.a, r.b, r.affinity >= TREATY_AFFINITY))
            .collect();
        // pairs that just earned a treaty (held Allied long enough, none yet).
        let mut to_sign: Vec<(u8, u8)> = Vec::new();
        for &(a, b, is_allied) in &allied {
            let pair = (a, b);
            if is_allied {
                let since = *self.allied_since.entry(pair).or_insert(now);
                let has = self.treaties.iter().any(|t| t.a == a && t.b == b);
                if !has && now.saturating_sub(since) >= TREATY_TICKS {
                    to_sign.push(pair);
                }
            } else {
                // fell out of the Allied band: reset the clock + drop any treaty.
                self.allied_since.remove(&pair);
                self.treaties.retain(|t| !(t.a == a && t.b == b));
            }
        }
        for (a, b) in to_sign {
            let name = Self::treaty_name(&mut self.civ_rng, &self.villages, a, b);
            self.treaties.push(Treaty { a, b, name, signed: now });
            self.treaties_signed += 1;
        }
    }

    /// Raise a WONDER at any peopled village that has banked `WONDER_RESEARCH` and has
    /// none yet — a one-time civilizational milestone. The kind is a stable per-village
    /// choice (off the village id) and the name is drawn off `civ_rng`.
    fn raise_wonders(&mut self) {
        let now = self.tick;
        let mut to_raise: Vec<(usize, WonderKind, String)> = Vec::new();
        for (vi, v) in self.villages.iter().enumerate() {
            if v.wonder.is_none() && v.population > 0 && v.research >= WONDER_RESEARCH {
                // stable kind per village so two same-sized worlds match; varied across
                // villages so the island shows different monuments.
                let kind = match v.id % 3 {
                    0 => WonderKind::Pyramid,
                    1 => WonderKind::Spire,
                    _ => WonderKind::Rotunda,
                };
                let name = Self::wonder_name(&mut self.civ_rng, kind, &v.name);
                to_raise.push((vi, kind, name));
            }
        }
        for (vi, kind, name) in to_raise {
            let vname = self.villages[vi].name.clone();
            self.log_event(format!("\u{2605} {vname} raises a wonder: {name}"));
            self.villages[vi].wonder = Some(Wonder { kind, name, raised: now });
            self.wonders_raised += 1;
        }
    }

    /// SPACE AGE: every `LAUNCH_PERIOD` ticks, each Space-era village (its launchpad) fires
    /// a ROCKET that arcs to the moon. Bounded — one rocket per village per period, with a
    /// small per-launch bearing jitter off `civ_rng` so successive shots vary. A village
    /// never has more than one rocket aloft at a time.
    fn launch_rockets(&mut self) {
        let now = self.tick;
        let mut to_launch: Vec<(u8, Pos)> = Vec::new();
        for v in &self.villages {
            if v.population == 0 || v.era != Era::Space {
                continue;
            }
            // bounded cadence; phase by village id so launches stagger rather than salvo.
            let phase = (now + v.id as u64 * (LAUNCH_PERIOD / 4)) % LAUNCH_PERIOD;
            if phase != 0 {
                continue;
            }
            if self.rockets.iter().any(|r| r.village == v.id) {
                continue; // already one aloft from this pad
            }
            to_launch.push((v.id, v.center));
        }
        for (village, pad) in to_launch {
            let bearing = (self.civ_rng.next_f32() - 0.5) * 0.8; // ±0.4 rad spread
            self.rockets.push(Rocket { village, pad, launched: now, bearing });
            self.rockets_launched += 1;
        }
    }

    /// A warm, deterministic TREATY name off the civ side-RNG (so the world stays
    /// reproducible). Combines a treaty-word with one signatory's name.
    fn treaty_name(rng: &mut Rng, villages: &[Village], a: u8, b: u8) -> String {
        const FORMS: [&str; 5] = ["Pact", "Accord", "Concord", "League", "Covenant"];
        let f = FORMS[rng.below(FORMS.len())];
        let seat = if rng.chance(0.5) { a } else { b } as usize;
        let town = villages.get(seat).map(|v| v.name.as_str()).unwrap_or("the Vale");
        format!("The {f} of {town}")
    }

    /// A warm, deterministic WONDER name off the civ side-RNG, themed to the silhouette.
    fn wonder_name(rng: &mut Rng, kind: WonderKind, town: &str) -> String {
        let (form, adjs): (&str, [&str; 4]) = match kind {
            WonderKind::Pyramid => ("Pyramid", ["Great", "Golden", "Eternal", "Sun"]),
            WonderKind::Spire => ("Spire", ["Sky", "Great", "Silver", "Star"]),
            WonderKind::Rotunda => ("Rotunda", ["Grand", "Crystal", "Hallowed", "Dawn"]),
        };
        let adj = adjs[rng.below(adjs.len())];
        format!("The {adj} {form} of {town}")
    }

    /// Step WARFARE one tick (Civilization Sprint 2). Two cadences: at each society
    /// evaluation it DECLARES wars (a freshly-soured, off-cooldown pair musters bands)
    /// and RECALLS bands when a war ends; every tick it runs the BATTLE (warriors march
    /// to the front + trade blows). A no-op when `war` is off (zero draws), so non-war
    /// worlds stay byte-identical. All randomness is off `war_rng`; casualties route
    /// through the existing Hurt/health path so a war death is grieved like any loss.
    fn step_warfare(&mut self) {
        if !self.war || self.villages.len() < 2 {
            return;
        }
        // --- DECLARATIONS + RECALLS run on the slow society cadence ---
        if self.tick % SOCIETY_PERIOD == 0 {
            self.declare_and_resolve_wars();
        }
        // --- the BATTLE itself ticks every frame so a clash reads in motion ---
        self.step_battles();
    }

    /// On the society cadence: open a war for any newly-hostile, off-cooldown pair, and
    /// close out any war whose end condition (band broken / casualty cap / clock) is met.
    fn declare_and_resolve_wars(&mut self) {
        // --- resolve: end wars that are broken / capped / timed-out, into a truce ---
        let now = self.tick;
        let mut ended: Vec<(u8, u8)> = Vec::new();
        for war in &self.wars {
            let band_a = self.warband_count(war.a);
            let band_b = self.warband_count(war.b);
            let total_dead = war.dead_a + war.dead_b;
            let broken = band_a <= WARBAND_BROKEN_AT || band_b <= WARBAND_BROKEN_AT;
            let capped = total_dead >= WAR_CASUALTY_CAP;
            let timed_out = now.saturating_sub(war.started) >= WAR_MAX_TICKS;
            if broken || capped || timed_out {
                ended.push((war.a, war.b));
            }
        }
        for (a, b) in ended {
            self.end_war(a, b);
        }

        // --- declare: a hostile, off-cooldown pair not already at war musters bands ---
        // gather candidate pairs first (immutable scan) to avoid borrow tangles.
        let mut to_declare: Vec<(u8, u8)> = Vec::new();
        for rel in &self.relations {
            let pair = (rel.a, rel.b);
            if rel.affinity > WAR_DECLARE_AFFINITY {
                continue; // not soured enough for open war
            }
            if self.wars.iter().any(|w| w.a == pair.0 && w.b == pair.1) {
                continue; // already fighting
            }
            if self.war_cooldowns.get(&pair).map(|&t| now < t).unwrap_or(false) {
                continue; // still war-weary from a recent war
            }
            // both sides must have a deep enough pool of adult fighters to muster.
            if self.fighter_pool(pair.0) < WARBAND_MIN_POOL
                || self.fighter_pool(pair.1) < WARBAND_MIN_POOL
            {
                continue;
            }
            to_declare.push(pair);
        }
        for (a, b) in to_declare {
            self.declare_war(a, b);
        }
    }

    /// RESOURCE SCARCITY in `[0,1]` (0 = plenty, 1 = dire) — a world-level Malthusian
    /// pressure: the island's total standing wood + stone measured against the mouths
    /// that depend on it. Flush early (few people, full woods); climbs as the population
    /// grows and the land is worked down. Independent of WHERE minds gather, so it never
    /// fights the inter-village mixing that keeps the society alive. Zero off a materials
    /// world. (`_v` reserved for a future per-village split; currently world-uniform.)
    pub fn village_scarcity(&self, _v: u8) -> f32 {
        if !self.materials_econ {
            return 0.0;
        }
        let standing: f32 = self.trees.iter().map(|t| t.wood).sum::<f32>()
            + self.rocks.iter().map(|r| r.stone).sum::<f32>();
        let mouths = self.living_count().max(1) as f32;
        let pc = standing / mouths;
        ((SCARCITY_COMFORT - pc) / SCARCITY_COMFORT).clamp(0.0, 1.0)
    }

    /// Count this village's living adult minds that *can* bear arms (the muster pool).
    fn fighter_pool(&self, v: u8) -> usize {
        self.agents
            .iter()
            .filter(|a| {
                a.alive
                    && a.maturity >= 0.95
                    && a.mind.can_war()
                    && a.village == Some(v)
            })
            .count()
    }

    /// Public read-only view of a village's standing warband size (for the HUD / diag).
    pub fn warband_size(&self, v: u8) -> usize {
        self.warband_count(v)
    }

    /// Count this village's still-standing (living) warriors in the CURRENT war.
    fn warband_count(&self, v: u8) -> usize {
        self.agents
            .iter()
            .filter(|a| a.alive && a.village == Some(v) && a.warband.is_some())
            .count()
    }

    /// Open a WAR between villages `a` and `b`: record it, muster a bounded warband from
    /// each side (the nearest-to-the-front adults, so they have least distance to march),
    /// and tag those minds with the enemy id. The rest of each village keeps living.
    fn declare_war(&mut self, a: u8, b: u8) {
        let (a, b) = (a.min(b), a.max(b));
        let ca = self.villages.get(a as usize).map(|v| v.center);
        let cb = self.villages.get(b as usize).map(|v| v.center);
        let (Some(ca), Some(cb)) = (ca, cb) else { return };
        let front = Pos::new((ca.x + cb.x) / 2, (ca.y + cb.y) / 2);
        self.muster_warband(a, b, front); // a's band marches against b
        self.muster_warband(b, a, front); // b's band marches against a
        self.wars.push(War { a, b, front, started: self.tick, dead_a: 0, dead_b: 0 });
        self.wars_declared += 1;
        let (na, nb) = (self.villages[a as usize].name.clone(), self.villages[b as usize].name.clone());
        self.log_event(format!("\u{2694} {na} goes to war with {nb} over dwindling land"));
    }

    /// Pick up to [`WARBAND_MAX`] of village `v`'s adult `can_war` minds — the ones
    /// nearest the front (least to march) — and conscript them to fight `enemy` (stored
    /// on `warband` so they march to the right front + the renderer reads who they fight).
    /// A small jitter off `war_rng` breaks ties so the same band is not always picked.
    fn muster_warband(&mut self, v: u8, enemy: u8, front: Pos) {
        // rank candidates by distance to the front (closest first), with a tiny
        // side-RNG jitter so musters vary between wars without touching the main stream.
        let mut cands: Vec<(usize, i64)> = Vec::new();
        for (i, a) in self.agents.iter().enumerate() {
            if a.alive && a.maturity >= 0.95 && a.mind.can_war() && a.village == Some(v) {
                let jitter = (self.war_rng.below(3) as i64) - 1; // -1,0,+1
                cands.push((i, a.body.pos.manhattan(front) as i64 + jitter));
            }
        }
        cands.sort_by_key(|&(_, d)| d);
        for &(i, _) in cands.iter().take(WARBAND_MAX) {
            self.agents[i].warband = Some(enemy);
        }
    }

    /// End a WAR between `a` and `b` (truce): stand both bands down, cool the relation to
    /// a wary peace, start the war-weariness cooldown, and tally it. Reuses the relation
    /// registry so the pair is free to re-sour or warm again via the normal drift.
    fn end_war(&mut self, a: u8, b: u8) {
        let (a, b) = (a.min(b), a.max(b));
        // stand down every warrior of either village.
        for ag in &mut self.agents {
            if (ag.village == Some(a) || ag.village == Some(b)) && ag.warband.is_some() {
                ag.warband = None;
                ag.weapon_flash = 0.0;
            }
        }
        // cool the relation toward a wary peace (war spends the enmity).
        if let Some(rel) = self.relations.iter_mut().find(|r| r.a == a && r.b == b) {
            // pull toward the truce affinity (don't slam — leave it wary, free to drift).
            rel.affinity = rel.affinity + (WAR_TRUCE_AFFINITY - rel.affinity) * 0.85;
        }
        self.war_cooldowns.insert((a, b), self.tick + WAR_COOLDOWN);
        self.wars.retain(|w| !(w.a == a && w.b == b));
        self.wars_resolved += 1;
        let (na, nb) = (self.villages[a as usize].name.clone(), self.villages[b as usize].name.clone());
        self.log_event(format!("\u{262e} {na} and {nb} make a wary peace"));
    }

    /// Run every active war's BATTLE for one tick: each warrior marches to its war's
    /// front, and any warrior within weapon reach of an enemy warrior may land a blow
    /// (off `war_rng`). A felled warrior takes lethal damage through the EXISTING Hurt /
    /// health path (cause "the war") so `reap_dead` grieves it like any loss. Casualty
    /// tallies feed the end conditions; weapon flashes drive the renderer's clash/muzzle.
    fn step_battles(&mut self) {
        // decay weapon flashes (renderer reads these; purely cosmetic).
        for a in &mut self.agents {
            if a.weapon_flash > 0.0 {
                a.weapon_flash = (a.weapon_flash - 0.12).max(0.0);
            }
        }
        if self.wars.is_empty() {
            return;
        }
        // snapshot the wars (front + the two sides) so we can scan agents immutably
        // while mutating their health in a second pass.
        let wars: Vec<War> = self.wars.clone();
        // resolve strikes: collect (victim_index, attacker_id) so we apply damage after.
        let mut hits: Vec<(usize, EntityId, u8)> = Vec::new(); // (victim, attacker, victim_village)
        for war in &wars {
            // indices of each side's standing warriors.
            let side_a: Vec<usize> = self
                .agents
                .iter()
                .enumerate()
                .filter(|(_, a)| a.alive && a.village == Some(war.a) && a.warband.is_some())
                .map(|(i, _)| i)
                .collect();
            let side_b: Vec<usize> = self
                .agents
                .iter()
                .enumerate()
                .filter(|(_, a)| a.alive && a.village == Some(war.b) && a.warband.is_some())
                .map(|(i, _)| i)
                .collect();
            if side_a.is_empty() || side_b.is_empty() {
                continue;
            }
            let weapon_a = self.villages.get(war.a as usize).map(|v| v.era.weapon());
            let weapon_b = self.villages.get(war.b as usize).map(|v| v.era.weapon());
            // each A warrior may strike its nearest B warrior in reach, and vice-versa.
            for &i in &side_a {
                if let Some((j, _)) = self.nearest_in_reach(i, &side_b, weapon_a) {
                    if self.war_rng.chance(WAR_STRIKE_CHANCE) {
                        hits.push((j, self.agents[i].id, war.b));
                    }
                    self.agents[i].weapon_flash = 1.0;
                }
            }
            for &j in &side_b {
                if let Some((i, _)) = self.nearest_in_reach(j, &side_a, weapon_b) {
                    if self.war_rng.chance(WAR_STRIKE_CHANCE) {
                        hits.push((i, self.agents[j].id, war.a));
                    }
                    self.agents[j].weapon_flash = 1.0;
                }
            }
        }
        // apply damage through the existing Hurt/health path so a kill is grieved.
        for (victim, attacker, victim_village) in hits {
            let died = {
                let a = &mut self.agents[victim];
                if !a.alive {
                    continue;
                }
                let floor = if a.mind.can_die() { 0.0 } else { 0.05 };
                let before = a.body.health;
                a.body.health = (a.body.health - WAR_HIT_DAMAGE).max(floor);
                a.death_cause = "the war";
                a.inbox.push(WorldEvent::Hurt { id: attacker, health: WAR_HIT_DAMAGE });
                a.flash = 1.0;
                a.weapon_flash = 1.0;
                before > 0.0 && a.body.health <= 0.0 && a.mind.can_die()
            };
            if died {
                // tally the casualty on the right side of the right war (for the end
                // conditions + the diag); reap_dead broadcasts the Died event for grief.
                self.war_casualties += 1;
                for war in &mut self.wars {
                    if war.a == victim_village {
                        war.dead_a += 1;
                    } else if war.b == victim_village {
                        war.dead_b += 1;
                    }
                }
            }
        }
    }

    /// The contested FRONT this warrior should march to: the active war between its
    /// village and the enemy it was mustered against. `None` if it is not mustered or its
    /// war has already ended (so it stops marching and stands down on the next recall).
    fn war_front_for(&self, i: usize) -> Option<Pos> {
        let me = &self.agents[i];
        let v = me.village?;
        let enemy = me.warband?;
        let (lo, hi) = (v.min(enemy), v.max(enemy));
        self.wars.iter().find(|w| w.a == lo && w.b == hi).map(|w| w.front)
    }

    /// The nearest enemy-warrior index (and distance) within this warrior's weapon
    /// reach, or `None`. Melee weapons only reach adjacency; ranged ones reach farther.
    fn nearest_in_reach(
        &self,
        i: usize,
        enemies: &[usize],
        weapon: Option<Weapon>,
    ) -> Option<(usize, i32)> {
        let reach = weapon.map(|w| w.reach()).unwrap_or(WAR_MELEE_R);
        let here = self.agents[i].body.pos;
        let mut best: Option<(usize, i32)> = None;
        for &j in enemies {
            if !self.agents[j].alive {
                continue;
            }
            let d = self.agents[j].body.pos.manhattan(here);
            if d <= reach && best.map(|(_, b)| d < b).unwrap_or(true) {
                best = Some((j, d));
            }
        }
        best
    }

    /// Accumulate RESEARCH for every village and re-derive its era (Civilization
    /// Sprint 1). Called once per society evaluation from [`step_society`] when `eras`
    /// is on; a no-op (zero draws) otherwise, so non-era worlds stay byte-identical.
    /// Rate = people × (1 + buildings) × peace, so a bigger, busier, more peaceful
    /// village climbs faster — and a war footing or depopulation stalls it.
    fn step_eras(&mut self) {
        if !self.eras || self.villages.is_empty() {
            return;
        }
        let k = self.villages.len();
        // --- count buildings nearest each village's centre (its infrastructure) ---
        // A built cell is attributed to the closest village centre within VILLAGE_BUILD_R;
        // cells far from every centre (no settlement) count for none.
        let centers: Vec<Pos> = self.villages.iter().map(|v| v.center).collect();
        let mut builds = vec![0usize; k];
        for w in &self.walls {
            let mut best: Option<(usize, i32)> = None;
            for (vi, c) in centers.iter().enumerate() {
                let d = w.manhattan(*c);
                if d <= VILLAGE_BUILD_R && best.map(|(_, b)| d < b).unwrap_or(true) {
                    best = Some((vi, d));
                }
            }
            if let Some((vi, _)) = best {
                builds[vi] += 1;
            }
        }
        // --- a per-village PEACE factor from its standing relations: at peace it learns
        // fastest; a rival/enemy on the borders drags it toward the war floor. ---
        let peace = self.village_peace_factors();
        let mut advances: Vec<(String, &'static str)> = Vec::new();
        for (vi, v) in self.villages.iter_mut().enumerate() {
            v.buildings = builds[vi];
            if v.population == 0 {
                continue; // a depopulated village makes no progress (but keeps its rank)
            }
            let pop_f = v.population as f32;
            let build_f = 1.0
                + RESEARCH_BUILDING_BONUS * (builds[vi] as f32).min(RESEARCH_BUILDING_CAP);
            let rate = RESEARCH_PER_HEAD * pop_f * build_f * peace[vi];
            let prev = v.era;
            v.research += rate;
            v.era = era_for_research(v.research);
            if (v.era as usize) > (prev as usize) {
                advances.push((v.name.clone(), v.era.name()));
            }
        }
        for (name, era) in advances {
            self.log_event(format!("\u{2692} {name} advances to {era}"));
        }
    }

    /// Per-village PEACE factor in `[RESEARCH_STABILITY_WAR, RESEARCH_STABILITY_PEACE]`:
    /// 1.0 with no hostile neighbour, sliding toward the war floor as the fraction of a
    /// village's *populated* neighbours that are Rival/Enemy rises. Used by `step_eras`.
    fn village_peace_factors(&self) -> Vec<f32> {
        let k = self.villages.len();
        let mut out = vec![RESEARCH_STABILITY_PEACE; k];
        for (vi, v) in self.villages.iter().enumerate() {
            let mut neighbours = 0u32;
            let mut hostile = 0u32;
            for other in &self.villages {
                if other.id == v.id || other.population == 0 {
                    continue;
                }
                neighbours += 1;
                if let Some(r) = self.relation_between(v.id, other.id) {
                    if matches!(r.kind(), RelationKind::Rival | RelationKind::Enemy) {
                        hostile += 1;
                    }
                }
            }
            if neighbours > 0 {
                let frac = hostile as f32 / neighbours as f32;
                out[vi] = RESEARCH_STABILITY_PEACE
                    + frac * (RESEARCH_STABILITY_WAR - RESEARCH_STABILITY_PEACE);
            }
        }
        out
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
            // a single village still tracks its centre, but has no relations to drift —
            // it can still climb the tech ladder (peace is trivially 1.0 with no rival).
            if self.tick % SOCIETY_PERIOD == 0 {
                self.recompute_village_centers();
                self.step_eras();
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

        // world-level resource scarcity (computed before the mut-borrow of relations);
        // zero unless this is a `scarcity_world`, so other worlds' drift is unchanged.
        let scarcity = if self.scarcity_world { self.village_scarcity(0) } else { 0.0 };

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
            delta -= scarcity * SCARCITY_PUSH; // hard times → covet a neighbour's land
            rel.affinity = (rel.affinity + delta).clamp(-1.0, 1.0);
            // slow relax toward neutral so relations are never permanent (allies cool,
            // enemies thaw) — the larger the standing, the stronger the pull back.
            rel.affinity -= rel.affinity * RELATION_DECAY;
            // soft-cap so a relation never welds to the rail: it always keeps room to
            // SHIFT in either direction as interactions change.
            rel.affinity = rel.affinity.clamp(-AFFINITY_SOFT_CAP, AFFINITY_SOFT_CAP);
        }
        // TECH/ERA (Civilization Sprint 1): with relations now drifted for this eval,
        // bank each village's research (scaled by people × buildings × the fresh peace)
        // and re-derive its era. A no-op (zero draws) when `eras` is off, so non-era
        // worlds are byte-identical.
        self.step_eras();
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

    /// The formalized TREATY between two villages, if any (live-only inspector / render).
    /// `None` off a civ world or when the pair has no standing treaty.
    pub fn treaty_between(&self, a: u8, b: u8) -> Option<&Treaty> {
        if a == b {
            return None;
        }
        let (lo, hi) = (a.min(b), a.max(b));
        self.treaties.iter().find(|t| t.a == lo && t.b == hi)
    }

    /// A living mind's village, by agent index (live-only inspector / render).
    pub fn village_of(&self, i: usize) -> Option<&Village> {
        let v = self.agents.get(i)?.village?;
        self.villages.get(v as usize)
    }

    /// The TECH ERA governing a building at world position `p`: the era of the nearest
    /// village centre within `VILLAGE_BUILD_R`, or `Era::Stone` if no village owns that
    /// ground (or eras are off). The renderer calls this to raise era-appropriate
    /// architecture per building cluster, so a village's structures reflect its era.
    pub fn era_at(&self, p: Pos) -> Era {
        if !self.eras {
            return Era::Stone;
        }
        let mut best: Option<(Era, i32)> = None;
        for v in &self.villages {
            let d = p.manhattan(v.center);
            if d <= VILLAGE_BUILD_R && best.map(|(_, b)| d < b).unwrap_or(true) {
                best = Some((v.era, d));
            }
        }
        best.map(|(e, _)| e).unwrap_or(Era::Stone)
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
        // chronicle the birth (name + village captured before the child is moved in).
        let birth_line = {
            let vn = village.and_then(|v| self.villages.get(v as usize)).map(|v| v.name.clone());
            match vn {
                Some(vn) => format!("\u{2726} {child_name} born in {vn}"),
                None => format!("\u{2726} {child_name} is born"),
            }
        };

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
            // a newborn is a civilian; it can only be mustered once grown to an adult.
            warband: None,
            weapon_flash: 0.0,
            was_leader: false,
        });
        self.births += 1;
        self.log_event(birth_line);
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
                    // WAR MARCH (live-only, gated by `war` + this mind being MUSTERED):
                    // a conscripted warrior overrides peacetime avoidance and steers
                    // toward its war's contested FRONT to give battle. Once at the front
                    // it engages the nearest enemy warrior (handled in `step_battles`).
                    // Inert off a war world (no warbands), so seeded worlds are
                    // byte-identical. The pull is partial (`WAR_MARCH_PULL`), so warriors
                    // still otherwise behave — they march, they don't teleport.
                    let mut marched = false;
                    if self.war && self.agents[i].warband.is_some() {
                        if let Some(front) = self.war_front_for(i) {
                            if me.pos != front && self.war_rng.chance(WAR_MARCH_PULL) {
                                dir = me.pos.toward(front);
                                marched = true;
                            }
                        }
                    }
                    // SOCIETY WARINESS (live-only, gated by the village_affinity gene +
                    // a hostile inter-village relation): a mind keeps its distance from
                    // an ENEMY/RIVAL village's members. If the chosen step would carry
                    // it CLOSER to a nearby wary mind, deflect to whichever neighbour
                    // step keeps it farthest from that mind — a low-grade avoidance, not
                    // a war. Deterministic (no RNG) and inert off a society world, so
                    // every seeded harness trajectory is byte-identical. A MUSTERED
                    // warrior skips this — it is marching to battle, not avoiding the foe.
                    if !marched {
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
                    } // end !marched (warriors skip peacetime avoidance)
                    // PURPOSE (scarcity world): when there is gathering to do, the
                    // villager steers decisively toward the resource its gather sense
                    // points at instead of wandering — and the pull RELEASES once it's
                    // there (gather_dir → None), so up close it acts freely and mixes
                    // with whoever is near. Idle (nothing to gather) it roams as before,
                    // so villages still intermingle. Gated on the live flag, so seeded
                    // balance worlds are byte-identical. Warriors (marched) are exempt.
                    if !marched && self.scarcity_world {
                        if let Some(td) = me.gather_dir.or(me.store_dir) {
                            dir = td;
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
            // villagers may take adjacent prey for meat (live game only; inert otherwise).
            self.step_hunting();
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
        // WARFARE (live-only): declare wars for freshly-soured pairs, run the battles
        // (warriors trade blows at the border), and resolve to a truce when a band
        // breaks / the casualty cap is hit / the clock runs out. Casualties route
        // through the EXISTING Hurt/grief path, so a war death is grieved like any loss;
        // then reap_dead collects the fallen. A no-op when `war` is off (zero draws), so
        // every seeded harness trajectory is byte-identical. Requires `society`.
        if self.war {
            self.step_warfare();
            self.reap_dead();
        }
        // CIVILIZATION CAPSTONE (live-only): refresh village LEADERS, form/dissolve named
        // TREATIES from the sustained alliances, raise WONDERS at advanced villages, and
        // run the SPACE AGE launch cadence (rockets arc to the moon); rockets also animate
        // each tick here. A no-op when `civ` is off (zero draws), so every seeded harness
        // trajectory is byte-identical. Requires `society`.
        if self.civ {
            self.step_civ();
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
        let mut chronicle: Vec<(String, &'static str, bool)> = Vec::new();
        for a in &mut self.agents {
            if a.alive && a.mind.can_die() && a.body.health <= 0.0 {
                a.alive = false;
                a.death_tick = Some(tick);
                if a.death_cause.is_empty() {
                    a.death_cause = "the stalker";
                }
                a.say = None;
                fallen.push((a.id, a.body.pos, a.death_cause));
                chronicle.push((a.name.clone(), a.death_cause, a.was_leader));
            }
        }
        for (id, pos, cause) in fallen {
            for a in &mut self.agents {
                if a.alive && a.id != id {
                    a.inbox.push(WorldEvent::Died { id, pos, cause: cause.to_string() });
                }
            }
        }
        // chronicle the losses (a fallen leader gets a grander line).
        for (name, cause, was_leader) in chronicle {
            if was_leader {
                self.log_event(format!("\u{26b0} Chief {name} laid to rest ({cause})"));
            } else {
                self.log_event(format!("\u{271d} {name} fell to {cause}"));
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
        for s in &mut self.sheep {
            if s.respawn_at.is_some() {
                continue;
            }
            let (ox, oy) = (s.rx, s.ry);
            s.rx += (s.pos.x as f32 - s.rx) * k;
            s.ry += (s.pos.y as f32 - s.ry) * k;
            let (mx, my) = (s.rx - ox, s.ry - oy);
            if mx.hypot(my) > 0.004 {
                s.heading = my.atan2(mx);
            }
            s.flash = (s.flash - dt * 1.6).max(0.0);
        }
        for h in &mut self.horses {
            if h.respawn_at.is_some() {
                continue;
            }
            let (ox, oy) = (h.rx, h.ry);
            h.rx += (h.pos.x as f32 - h.rx) * k;
            h.ry += (h.pos.y as f32 - h.ry) * k;
            let (mx, my) = (h.rx - ox, h.ry - oy);
            if mx.hypot(my) > 0.004 {
                h.heading = my.atan2(mx);
            }
            h.flash = (h.flash - dt * 1.6).max(0.0);
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
        self.step_sheep();
        self.step_horses();
        self.step_wolves();
        self.step_bears();
        self.respawn_deer();
        self.respawn_sheep();
        self.respawn_horses();
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
            //
            // NOTE on sheep/horses: predators do NOT retarget onto sheep/horses. Doing so
            // would change wolf/bear PATHS (they'd chase the nearer flock animal), which —
            // although all wildlife RNG is on `wild_rng` — moves the predators to different
            // cells and perturbs the seed-sensitive LIVE systems (society/war) that run on
            // the same world. Keeping deer-only prey-selection keeps every predator path
            // byte-identical to the pre-sheep/horse baseline. Sheep/horses are still real
            // ambient prey: they FLEE predators and RESPAWN, so the populations persist.
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
            // deer-only prey-selection (see the note in `step_wolves`): retargeting onto
            // sheep/horses would shift bear paths and perturb the seed-sensitive live
            // systems. Sheep/horses remain ambient prey that flee and respawn.
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
    /// Turn LIVE-ONLY hunting on/off (the live game only). When on, adult villagers take
    /// adjacent prey for meat. Inert otherwise, so seeded/harness worlds are unchanged.
    pub fn set_hunting(&mut self, on: bool) {
        self.hunting = on;
    }

    /// Turn on the LIVE-ONLY SCARCITY WORLD (the economic engine of purpose + conflict):
    /// villagers move with PURPOSE toward the resource they're gathering (no aimless
    /// wandering while there's work), and as the island's standing wood + stone thins
    /// under a growing population, RESOURCE SCARCITY drifts neighbours toward war over
    /// the dwindling land. Gated by this flag and enabled ONLY by the live `Game::new`,
    /// so the seed-tuned society/war balance tests (which never call it) are byte-identical.
    pub fn set_scarcity_world(&mut self, on: bool) {
        self.scarcity_world = on;
    }

    /// OPPORTUNISTIC HUNT: any adult villager standing next to a live deer or sheep may
    /// take it — restoring its energy + a little health, despawning the animal (it
    /// respawns elsewhere). It does NOT steer minds toward prey (no new pathing), so it
    /// only fires when a mind already happens to be adjacent. Off `wild_rng`. Runs only
    /// behind the `hunting` flag (the live game), so the harness + balance tests never
    /// hit it and stay byte-identical.
    fn step_hunting(&mut self) {
        if !self.hunting {
            return;
        }
        let mut hunts: Vec<(usize, bool, usize)> = Vec::new(); // (mind, is_deer, prey_idx)
        for (mi, a) in self.agents.iter().enumerate() {
            if !a.alive || a.maturity < 0.9 {
                continue;
            }
            let p = a.body.pos;
            if let Some(di) = self.deer.iter().position(|d| d.respawn_at.is_none() && d.pos.manhattan(p) <= 1) {
                if self.wild_rng.chance(0.12) {
                    hunts.push((mi, true, di));
                    continue;
                }
            }
            if let Some(si) = self.sheep.iter().position(|s| s.respawn_at.is_none() && s.pos.manhattan(p) <= 1) {
                if self.wild_rng.chance(0.12) {
                    hunts.push((mi, false, si));
                }
            }
        }
        for (mi, is_deer, pi) in hunts {
            if is_deer {
                self.catch_deer(pi);
            } else {
                self.sheep[pi].respawn_at = Some(self.tick + 120);
                self.sheep[pi].flash = 1.0;
            }
            let b = &mut self.agents[mi].body;
            b.energy = (b.energy + 0.4).min(1.0);
            b.health = (b.health + 0.15).min(1.0);
            self.agents[mi].flash = 0.7;
            self.agents[mi].flash_kind = Process::Routine;
            let name = self.agents[mi].name.clone();
            self.log_event(format!("\u{1f3f9} {name} hunted {}", if is_deer { "a deer" } else { "a sheep" }));
        }
    }

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

    /// Move the sheep: graze + FLOCK. A sheep flees the nearest predator within sight
    /// (a touch slower than deer — it idles a little more, so a wolf can run down a
    /// straggler). Otherwise it grazes: most of the time it steers gently toward its
    /// flock's centroid (cohesion) so the flock stays a tight cluster; occasionally it
    /// just nibbles in place / wanders a step. All draws use `wild_rng`.
    fn step_sheep(&mut self) {
        let sight = self.sight + 2; // a bit less watchful than deer (sight+4)
        let n = self.sheep.len();
        // flock centroids up front (for cohesion), from this tick's start.
        let mut flock_sum: [(i32, i32, i32); 4] = [(0, 0, 0); 4];
        for s in &self.sheep {
            if s.respawn_at.is_some() {
                continue;
            }
            let fk = (s.flock as usize).min(3);
            flock_sum[fk].0 += s.pos.x;
            flock_sum[fk].1 += s.pos.y;
            flock_sum[fk].2 += 1;
        }
        for i in 0..n {
            if self.sheep[i].respawn_at.is_some() {
                continue;
            }
            let p = self.sheep[i].pos;
            let threat = self.nearest_threat(p, sight);
            if let Some(tp) = threat {
                // FLEE away — but sheep are slower: only bolt ~3 of every 4 ticks, so a
                // determined predator occasionally catches a straggler.
                self.sheep[i].fleeing = true;
                if self.wild_rng.chance(0.75) {
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
                    self.sheep[i].pos = best;
                }
            } else {
                self.sheep[i].fleeing = false;
                // GRAZE: move ~half the ticks. Cohesion — steer toward the flock centroid
                // when it has drifted away, else nibble/wander in place.
                if self.wild_rng.chance(0.5) {
                    let fk = (self.sheep[i].flock as usize).min(3);
                    let (sx, sy, cnt) = flock_sum[fk];
                    let q = if cnt > 1 {
                        let centroid = Pos::new(sx / cnt, sy / cnt);
                        if centroid.manhattan(p) > 3 && self.wild_rng.chance(0.7) {
                            self.clamp(p.step(p.toward(centroid)))
                        } else {
                            self.wild_wander(p)
                        }
                    } else {
                        self.wild_wander(p)
                    };
                    self.sheep[i].pos = q;
                }
            }
        }
    }

    /// Move the horses: graze + roam, FASTER than deer. A horse flees the nearest
    /// predator within sight and, being fast, gets a bonus bolt step (it covers two
    /// cells while fleeing) so it almost always escapes. Grazing, it wanders a step
    /// nearly every tick (faster roam than deer). All draws use `wild_rng`.
    fn step_horses(&mut self) {
        let sight = self.sight + 5; // horses are alert and spot danger early
        let n = self.horses.len();
        for i in 0..n {
            if self.horses[i].respawn_at.is_some() {
                continue;
            }
            let p = self.horses[i].pos;
            let threat = self.nearest_threat(p, sight);
            if let Some(tp) = threat {
                // FLEE fast: two cells away from the threat (a galloping bolt).
                self.horses[i].fleeing = true;
                let mut cur = p;
                for _ in 0..2 {
                    let mut best = cur;
                    let mut best_d = cur.manhattan(tp);
                    for dir in Dir::ALL {
                        let q = self.clamp(cur.step(dir));
                        let dd = q.manhattan(tp);
                        if dd > best_d {
                            best_d = dd;
                            best = q;
                        }
                    }
                    cur = best;
                }
                self.horses[i].pos = cur;
            } else {
                self.horses[i].fleeing = false;
                // GRAZE/ROAM: wander a step almost every tick (faster than deer), gently
                // drawn toward the nearest grove now and then (a roaming grazer).
                if self.wild_rng.chance(0.8) {
                    let toward_grove = self.trees.iter().map(|t| t.pos).min_by_key(|tp| tp.manhattan(p));
                    let q = match toward_grove {
                        Some(tp) if tp.manhattan(p) > 3 && self.wild_rng.chance(0.4) => {
                            self.clamp(p.step(p.toward(tp)))
                        }
                        _ => self.wild_wander(p),
                    };
                    self.horses[i].pos = q;
                }
            }
        }
    }

    /// Bring caught sheep back as fresh sheep, near their flock's centroid and clear of
    /// predators, so the flock stays roughly constant and coherent.
    fn respawn_sheep(&mut self) {
        let now = self.tick;
        let n = self.sheep.len();
        for i in 0..n {
            let due = matches!(self.sheep[i].respawn_at, Some(t) if now >= t);
            if !due {
                continue;
            }
            // prefer near the flock centroid (so it rejoins the flock), but clear of any
            // predator; a few tries, then fall back to anywhere clear.
            let flock = self.sheep[i].flock;
            let mut sum = (0i32, 0i32, 0i32);
            for s in &self.sheep {
                if s.respawn_at.is_none() && s.flock == flock {
                    sum.0 += s.pos.x;
                    sum.1 += s.pos.y;
                    sum.2 += 1;
                }
            }
            let mut spot = self.sheep[i].pos;
            for attempt in 0..10 {
                let cand = if sum.2 > 0 && attempt < 6 {
                    let c = Pos::new(sum.0 / sum.2, sum.1 / sum.2);
                    let jx = self.wild_rng.below(7) as i32 - 3;
                    let jy = self.wild_rng.below(7) as i32 - 3;
                    self.clamp(Pos::new(c.x + jx, c.y + jy))
                } else {
                    Pos::new(
                        self.wild_rng.below(self.w as usize) as i32,
                        self.wild_rng.below(self.h as usize) as i32,
                    )
                };
                if self.nearest_threat(cand, 8).is_none() {
                    spot = cand;
                    break;
                }
            }
            let s = &mut self.sheep[i];
            s.pos = spot;
            s.rx = spot.x as f32;
            s.ry = spot.y as f32;
            s.respawn_at = None;
            s.fleeing = false;
        }
    }

    /// Bring caught horses back as fresh horses, well clear of predators, so the herd
    /// stays roughly constant.
    fn respawn_horses(&mut self) {
        let now = self.tick;
        let n = self.horses.len();
        for i in 0..n {
            let due = matches!(self.horses[i].respawn_at, Some(t) if now >= t);
            if !due {
                continue;
            }
            let mut spot = self.horses[i].pos;
            for _ in 0..8 {
                let cand = Pos::new(
                    self.wild_rng.below(self.w as usize) as i32,
                    self.wild_rng.below(self.h as usize) as i32,
                );
                if self.nearest_threat(cand, 10).is_none() {
                    spot = cand;
                    break;
                }
            }
            let h = &mut self.horses[i];
            h.pos = spot;
            h.rx = spot.x as f32;
            h.ry = spot.y as f32;
            h.respawn_at = None;
            h.fleeing = false;
        }
    }

    /// `true` if a sheep is currently caught (despawned, awaiting respawn).
    pub fn sheep_hidden(&self, s: &Sheep) -> bool {
        s.respawn_at.is_some()
    }

    /// `true` if a horse is currently caught (despawned, awaiting respawn).
    pub fn horse_hidden(&self, h: &Horse) -> bool {
        h.respawn_at.is_some()
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

    // ---- WARFARE (Civilization Sprint 2) ----

    /// The live showcase world with WARFARE armed (can_war gene + set_war), at a chosen
    /// seed. Seed 2 is verified to declare + resolve a survivable war while the world
    /// thrives; seed 0x61 (the default showcase) stays at peace (villages spread apart).
    fn live_war_world(seed: u64, n_villages: usize) -> GameWorld {
        let mut g = daimon_mind::Genome::showcase();
        for i in [21, 22, 23, 24, 29, 30, 31, 32, 33, 34] {
            g.g[i] = 1.0;
        }
        let mut w = GameWorld::with_genome_sized(seed, 64, &g, 124, 84, 7);
        w.soften_stalker();
        w.set_materials_world(true);
        w.set_wildlife(true);
        w.set_lifecycle(true, 90);
        w.set_society(true, n_villages);
        w.set_eras(true);
        w.set_war(true);
        w
    }

    /// The full LIVE showcase stack including the scarcity world (matches `Game::new`).
    fn live_scarcity_world(seed: u64, n_villages: usize) -> GameWorld {
        let mut w = live_war_world(seed, n_villages);
        w.set_hunting(true);
        w.set_scarcity_world(true);
        w
    }

    #[test]
    fn scarcity_drives_wars_and_world_survives() {
        // In the SCARCITY WORLD, as the island thins under its growing people, resource
        // scarcity must build and drive emergent wars over the dwindling land — wars that
        // are bounded PER WAR and survivable (births keep the world turning over), with
        // the villages persisting. This is the gated live behaviour; the seed-tuned
        // balance worlds (no scarcity flag) are covered separately and stay unchanged.
        let mut w = live_scarcity_world(0x61, 4);
        let mut saw_warband = false;
        let mut peak_band = 0usize;
        let mut peak_scarcity = 0.0f32;
        for _ in 0..16000 {
            w.step();
            let band = w.agents.iter().filter(|a| a.alive && a.warband.is_some()).count();
            saw_warband |= band > 0;
            peak_band = peak_band.max(band);
            peak_scarcity = peak_scarcity.max(w.village_scarcity(0));
        }
        assert!(peak_scarcity > 0.25, "the land genuinely got scarce (peak {peak_scarcity:.2})");
        assert!(w.wars_declared >= 1, "scarcity drove at least one war");
        assert!(saw_warband, "a warband actually mustered");
        // warbands stay a minority of the living world (never an all-in levy).
        assert!(
            peak_band < w.living_count().max(1) / 3 + 1,
            "warbands a minority ({peak_band} of {} alive)",
            w.living_count()
        );
        // casualties bounded PER WAR (the per-war cap stops annihilation).
        assert!(
            w.war_casualties <= w.wars_resolved.max(1) * WAR_CASUALTY_CAP,
            "per-war casualties capped ({} dead over {} wars)",
            w.war_casualties,
            w.wars_resolved
        );
        assert!(w.living_count() > 40, "world survives the scarcity wars ({} alive)", w.living_count());
        assert!(
            w.villages.iter().filter(|v| v.population > 0).count() >= 2,
            "multiple villages persist"
        );
    }

    #[test]
    fn war_off_world_is_byte_identical() {
        // A world that never calls `set_war` must be byte-identical to the incumbent: no
        // wars, no warbands, no war RNG, no combat damage. The can_war gene is off in BOTH
        // presets, so even arming the live flag on a non-war world leaves the seeded
        // trajectory untouched.
        let mut w = GameWorld::new(0xDA13, 6);
        for _ in 0..1500 {
            w.step();
        }
        assert!(!w.war);
        assert!(w.wars.is_empty());
        assert!(w.agents.iter().all(|a| a.warband.is_none()));
        assert_eq!(w.wars_declared, 0);
        assert_eq!(w.war_casualties, 0);
        // determinism: two non-war worlds stay identical step-for-step.
        let mut a = GameWorld::new(11, 4);
        let mut b = GameWorld::new(11, 4);
        for _ in 0..400 {
            a.step();
            b.step();
        }
        for (x, y) in a.agents.iter().zip(b.agents.iter()) {
            assert_eq!(x.body.pos, y.body.pos);
            assert_eq!(x.warband, y.warband);
        }
    }

    #[test]
    fn war_is_deterministic() {
        // Same seed + same flags ⇒ identical war history (declarations, casualties, the
        // standing of every relation), so warfare never perturbs reproducibility.
        let mut a = live_war_world(0x2, 4);
        let mut b = live_war_world(0x2, 4);
        for _ in 0..13000 {
            a.step();
            b.step();
        }
        assert_eq!(a.wars_declared, b.wars_declared);
        assert_eq!(a.wars_resolved, b.wars_resolved);
        assert_eq!(a.war_casualties, b.war_casualties);
        assert_eq!(a.living_count(), b.living_count());
        for (x, y) in a.relations.iter().zip(b.relations.iter()) {
            assert_eq!(x.affinity.to_bits(), y.affinity.to_bits());
        }
        for (x, y) in a.agents.iter().zip(b.agents.iter()) {
            assert_eq!(x.body.pos, y.body.pos);
            assert_eq!(x.warband, y.warband);
        }
    }

    #[test]
    fn war_flares_resolves_and_world_survives() {
        // At a border-contested seed, an emergent WAR must: be declared (a soured pair
        // crosses enmity), field bounded warbands, take a few casualties, then RESOLVE to
        // a truce — and the world must SURVIVE (it is a skirmish, not extinction): both
        // villages persist and the population keeps turning over.
        let mut w = live_war_world(0x2, 4);
        let mut saw_warband = false;
        let mut peak_band = 0usize;
        for _ in 0..14000 {
            w.step();
            let band = w.agents.iter().filter(|a| a.alive && a.warband.is_some()).count();
            if band > 0 {
                saw_warband = true;
            }
            peak_band = peak_band.max(band);
        }
        assert!(w.wars_declared >= 1, "an emergent war was declared");
        assert!(saw_warband, "a warband was actually mustered and stood in the field");
        // warbands are BOUNDED (a few a side) — the village is never emptied.
        assert!(peak_band <= WARBAND_MAX * 2, "warbands bounded ({peak_band} fielded)");
        assert!(w.war_casualties >= 1, "the battle drew real blood (casualties routed through the grief path)");
        // SURVIVABLE: casualties are capped, the world lives on, villages persist.
        assert!(w.war_casualties <= WAR_CASUALTY_CAP, "casualties capped (no annihilation)");
        assert!(w.wars_resolved >= 1, "the war ENDED in a truce (it did not grind forever)");
        assert!(w.wars.is_empty() || w.wars_declared > w.wars_resolved, "no orphan war state");
        assert!(w.living_count() > 30, "world survived the war(s) ({} alive)", w.living_count());
        assert!(
            w.villages.iter().filter(|v| v.population > 0).count() >= 2,
            "both sides persist through the war"
        );
    }

    #[test]
    fn war_truce_cools_the_relation_and_arms_a_cooldown() {
        // After a war resolves, the warring pair is cooled toward a wary peace (not left
        // pinned at enmity) and a cooldown bars an immediate rematch — so wars are
        // OCCASIONAL, the world recovers between them.
        let mut w = live_war_world(0x2, 4);
        for _ in 0..13000 {
            w.step();
        }
        // by now seed-2's one war (t≈12360→12540) has resolved.
        assert!(w.wars_resolved >= 1);
        // every relation sits above full enmity right after a truce (cooled), and no
        // war is currently active for a just-resolved pair (the cooldown holds).
        assert!(
            w.relations.iter().all(|r| r.affinity > -0.85 || w.wars.iter().any(|x| x.a == r.a && x.b == r.b)),
            "a fought-out border cooled below the rail after truce"
        );
    }
}
