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
    rng: Rng,
    next_id: u32,
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
        Self::build(seed, n_agents, (40, 26, 7), false, Mind::new)
    }

    /// Build a world whose agents all express the given cognitive [`Genome`] —
    /// the genome's escalation config and faculty switches apply to every agent,
    /// while persona deltas ride on top of each base character (so the cast stays
    /// diverse and the *architecture* is what varies). This is the seam the
    /// self-improvement pipeline optimises through.
    pub fn with_genome(seed: u64, n_agents: usize, genome: &Genome) -> Self {
        Self::build(seed, n_agents, (40, 26, 7), false, |persona, s| genome.express(&persona, s))
    }

    /// Like [`with_genome`], but on a custom grid — so a village's *density* can
    /// be held constant as the population grows (a bigger island for more minds).
    /// `sight` is the perception radius in cells.
    pub fn with_genome_sized(seed: u64, n_agents: usize, genome: &Genome, w: i32, h: i32, sight: i32) -> Self {
        Self::build(seed, n_agents, (w, h, sight), false, |persona, s| genome.express(&persona, s))
    }

    /// A **harsh** world: scarce resources (water especially) and an aggressive,
    /// fast stalker, so survival genuinely *costs* good policy. The fair world is
    /// too easy to drive evolution (everything passes); this gives the
    /// self-improvement search a real gradient to climb.
    pub fn with_genome_harsh(seed: u64, n_agents: usize, genome: &Genome) -> Self {
        Self::build(seed, n_agents, (40, 26, 7), true, |persona, s| genome.express(&persona, s))
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
        Self::build(seed, n_agents, (w, h, sight), true, |persona, s| genome.express(&persona, s))
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
            genomes[i.min(n.saturating_sub(1))].express(&persona, s)
        })
    }

    fn build(
        seed: u64,
        n_agents: usize,
        dims: (i32, i32, i32),
        harsh: bool,
        express: impl Fn(Persona, u64) -> Mind,
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
            agents.push(Agent {
                id: new_id(&mut next_id),
                name,
                mind: express(persona, seed ^ (0x9e37 + i as u64 * 0x1111)),
                body: SelfState::new(pos),
                accent,
                rx: pos.x as f32,
                ry: pos.y as f32,
                last: None,
                trail: Vec::new(),
                flash: 0.0,
                flash_kind: Process::Routine,
                say: None,
                inbox: Vec::new(),
                alive: true,
                death_tick: None,
                death_cause: "",
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
                    if adj && in_bounds && !occupied && me.energy > 0.2 {
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
}
