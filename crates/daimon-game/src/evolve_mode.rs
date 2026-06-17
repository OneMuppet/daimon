//! Live generational evolution mode — **live-only**, additive, and completely
//! separate from the seeded harness paths (default `Game::new`, the believability
//! example, the proofs, and the fitness sweep all go through other constructors
//! and never touch this module's RNG).
//!
//! A big island is seeded with `pop` minds (default 1000), built **harsh** then
//! tuned for evolution ([`GameWorld::tune_for_evolution`]): tight-but-foragable
//! resources, gentler metabolism, an eased (still lethal) stalker, and a packed
//! island so resources fall within sight. **Lethal starvation** stays on, so a
//! mind that cannot forage — including one that walls itself in — actually starves
//! and dies. The net effect is a fast, visible die-off that still leaves an elite
//! of ≈5-20% (not the ~1% the bare harsh world left), giving selection real
//! signal. We run at max sim speed and, every **generation** (10 day/night cycles =
//! `GEN_TICKS` ticks), rank the population by fitness, keep the elite, breed
//! mutated offspring from them to refill back to `pop`, respawn the whole island
//! with fresh bodies, bump the generation counter, and repeat — watchable natural
//! selection.

use daimon_mind::Genome;

use crate::sim::GameWorld;
use daimon_core::{Drive, Rng};

/// One day/night cycle is a full `day` wrap. `day` advances 0.0016/tick in
/// [`GameWorld::step`], so a cycle is 625 ticks; a generation is 10 cycles.
pub const CYCLE_TICKS: u64 = 625;
pub const CYCLES_PER_GEN: u64 = 10;
pub const GEN_TICKS: u64 = CYCLE_TICKS * CYCLES_PER_GEN; // 6250

/// Fraction of the population kept as breeding elite each generation.
const ELITE_FRAC: f32 = 0.20;

/// In this mode only, cap the social/cultural interaction radius so the latent
/// O(n²) neighbour scan stays cheap at a thousand minds. The default game does not
/// use this (its `sight` is small anyway); the big island uses a large `sight` for
/// perception but we clamp the *interaction* fan-out via the world's `sight` field
/// being set by [`island_dims`] to a modest radius.
///
/// Packed island for `pop` minds (≈55 cells/mind, side ∝ √pop). Tighter than the
/// believability village's ≈173 cells/mind on purpose: with a bounded sight the
/// scattered springs must fall within perception or survival becomes a spawn
/// lottery (no GA gradient). Sight stays small so the per-tick neighbour scan does
/// not blow up; density does the findability work instead.
pub fn island_dims(pop: usize) -> (i32, i32, i32) {
    let pop = pop.max(1) as f32;
    // Tighter than the believability village's ≈173 cells/mind: a 1000-mind island
    // at that density is ~515 wide, and with a bounded sight (≤14, kept low so the
    // O(n²) visible-scan stays cheap) the scattered springs fall *outside*
    // perception — survival then collapses to a spawn-position lottery and the GA
    // gets no gradient. We pack the island to ≈40 cells/mind so resources land
    // within sight and foraging *competence* (the heritable thing) decides who
    // lives, while density stays well below 1-mind-per-cell so movement/contention
    // still work. Same ~1.54 aspect as the 40×26 village.
    let area = pop * 55.0;
    let aspect = 40.0 / 26.0;
    let h = (area / aspect).sqrt().round().max(26.0);
    let w = (h * aspect).round().max(40.0);
    // sight ∝ √pop but bounded — the village uses 7; cap at 14 so 1000 minds don't
    // each scan a huge neighbourhood every tick.
    let sight = (7.0 + (pop.sqrt() * 0.18)).round().clamp(7.0, 14.0);
    (w as i32, h as i32, sight as i32)
}

/// Per-mind fitness accumulated over a generation. Survival-dominant: `ticks_alive`
/// is the headline term; the rest are tie-breakers that reward staying *fed and
/// safe*, not merely twitching. A mind that walls itself in and starves dies early
/// → tiny `ticks_alive` → near-zero fitness, exactly as intended.
#[derive(Clone, Copy, Default)]
struct MindScore {
    ticks_alive: u64,
    /// Σ (energy+hydration)/2 over living ticks — being well-fed, not just alive.
    nourishment: f64,
    /// Σ enclosure over living ticks while *also* nourished — sheltered AND fed
    /// (sheltering yourself to death earns nothing, since it requires being alive
    /// and fed to accrue).
    shelter: f64,
    /// ticks spent out of the predator's immediate reach (peaceful longevity).
    peaceful: u64,
}

impl MindScore {
    /// Collapse to a single comparable scalar.
    ///
    /// Survival still matters most — staying alive a tick is worth far more than any
    /// single tick of comfort — but the quality terms are weighted so they do **not**
    /// saturate among full-generation survivors. The earlier ×1000 survival weight
    /// dwarfed everything (a full-gen survivor pinned at ≈6.25M regardless of how it
    /// lived), so best/elite-mean had no headroom to climb and selection showed no
    /// gradient even when it was working. Here each living tick is worth 100, and a
    /// tick lived *well-fed* (`nourish`∈[0,1]) adds up to ~+300 more, a tick lived
    /// *safe* up to +100, *sheltered-while-fed* up to +50 — so two minds that both
    /// survive the whole generation separate by **how well** they lived (a heritable,
    /// climbable signal), while a mind that dies early still scores far lower (its
    /// terms stop accruing the moment it dies).
    fn scalar(&self) -> f64 {
        self.ticks_alive as f64 * 100.0
            + self.nourishment * 300.0
            + self.shelter * 50.0
            + self.peaceful as f64 * 100.0
    }
}

/// A snapshot of how the last generation went, for the HUD.
#[derive(Clone, Copy, Default)]
pub struct GenStats {
    /// Mean fitness over the whole population (dead included) — dominated by the
    /// dead's near-zero scores, so it tracks the survival *rate*.
    pub mean_fitness: f64,
    /// Mean fitness over just the survivors (the breeding pool). The cleaner signal
    /// for "is the elite getting better?".
    pub elite_mean: f64,
    pub best_fitness: f64,
    pub survivors_end: usize,
    /// How many genomes were carried forward as breeding elite this generation.
    pub elite_n: usize,
    /// The drive that led most often among the *elite* at generation end — a quick
    /// read on what kind of mind is winning.
    pub elite_dominant: Option<Drive>,
    /// Mean of the three survival-critical foraging genes among the elite
    /// `[foresight (g13), goal-directed forage (g14), commons-aware forage (g15)]`,
    /// each in `[0,1]`. The clearest read on *what the population is selecting for*:
    /// if the loop works these rise toward 1.0 (especially from a degraded start).
    pub elite_forage_genes: [f32; 3],
}

/// The live generational evolution driver.
pub struct Evolution {
    pub world: GameWorld,
    pub pop: usize,
    pub generation: u32,
    /// Current population genomes, parallel to `world.agents` (same order — the
    /// world expresses genomes in agent order).
    genomes: Vec<Genome>,
    scores: Vec<MindScore>,
    /// Ticks elapsed within the current generation (0..GEN_TICKS).
    gen_tick: u64,
    rng: Rng,
    seed: u64,
    /// Stats from the most recently completed generation (None before gen 1 ends).
    pub last: Option<GenStats>,
    pub dims: (i32, i32, i32),
}

impl Evolution {
    /// Build the initial island: `pop` minds, each a mutated variant of the live
    /// genome (showcase + build/die/grieve genes on), so generation 0 already has
    /// genetic variance for selection to act on.
    pub fn new(seed: u64, pop: usize) -> Self {
        let pop = pop.max(2);
        let dims = island_dims(pop);
        let mut rng = Rng::new(seed);

        // base live genome: the trained showcase policy with building, mortality
        // and grief switched on (exactly the live game's genes).
        let mut base = Genome::showcase();
        base.g[21] = 1.0; // can_build
        base.g[22] = 1.0; // can_die
        base.g[23] = 1.0; // can_grieve
        base.g[24] = 1.0; // can_provision — winters select for stocking the granary

        // DIAGNOSTIC (env-gated, live-only): start from a *crippled* policy with the
        // survival-critical foraging genes knocked out (no anticipatory foresight, no
        // goal-directed/ commons-aware foraging). If the GA is working, selection
        // should rediscover these (genes 13/14/15 climb toward 1.0) and fitness
        // should rise generation over generation — the cleanest proof the loop can
        // actually improve the minds, separate from the question of whether the
        // already-trained showcase policy has any headroom left. Off unless
        // `DAIMON_EVOLVE_DEGRADE=1`, so the normal `--evolve` run is unaffected.
        if std::env::var("DAIMON_EVOLVE_DEGRADE").as_deref() == Ok("1") {
            base.g[13] = 0.0; // foresight off (no anticipatory homeostasis)
            base.g[14] = 0.0; // goal-directed foraging off (greedy-nearest only)
            base.g[15] = 0.0; // commons-aware foraging off
        }

        let gain = mutation_gain();
        let genomes: Vec<Genome> = (0..pop)
            .map(|i| {
                if i == 0 {
                    base.clone() // keep one pristine reference individual
                } else {
                    base.mutate(0.08, &gain, &mut rng)
                }
            })
            .collect();

        let world = build_world(seed, &genomes, dims);
        let n = world.agents.len();
        Evolution {
            world,
            pop,
            generation: 1,
            genomes,
            scores: vec![MindScore::default(); n],
            gen_tick: 0,
            rng,
            seed,
            last: None,
            dims,
        }
    }

    /// Advance one cognitive tick, accumulate fitness, and roll the generation over
    /// when the generation's tick budget is spent. Returns `true` on the tick that
    /// crossed a generation boundary (for HUD flashes / tests).
    pub fn tick(&mut self) -> bool {
        self.world.step();
        self.gen_tick += 1;

        // accumulate per-mind fitness from observable body state this tick.
        let pred = self.world.predator.pos;
        for (i, a) in self.world.agents.iter().enumerate() {
            if !a.alive {
                continue;
            }
            let s = &mut self.scores[i];
            s.ticks_alive += 1;
            let nourish = ((a.body.energy + a.body.hydration) * 0.5) as f64;
            s.nourishment += nourish;
            // sheltered AND fed: scale enclosure by nourishment so walling in while
            // starving (nourish→0) earns ~nothing.
            s.shelter += a.body.enclosure as f64 * nourish;
            if a.body.pos.manhattan(pred) > 3 {
                s.peaceful += 1;
            }
        }

        if self.gen_tick >= GEN_TICKS {
            self.next_generation();
            true
        } else {
            false
        }
    }

    /// Living minds right now (the watched die-off).
    pub fn alive(&self) -> usize {
        self.world.living_count()
    }

    /// Which day/night cycle of the current generation we are in (1..=10).
    pub fn cycle(&self) -> u64 {
        (self.gen_tick / CYCLE_TICKS + 1).min(CYCLES_PER_GEN)
    }

    /// Rank → keep elite → breed offspring → respawn → bump counter.
    fn next_generation(&mut self) {
        // 1. rank all minds by fitness, *preferring those still alive* (alive minds
        //    sort above the dead at equal score — they proved out the generation).
        let mut order: Vec<usize> = (0..self.genomes.len()).collect();
        let alive: Vec<bool> = self.world.agents.iter().map(|a| a.alive).collect();
        order.sort_by(|&a, &b| {
            let ka = (alive[a], self.scores[a].scalar());
            let kb = (alive[b], self.scores[b].scalar());
            kb.1.total_cmp(&ka.1).then(kb.0.cmp(&ka.0))
        });

        // stats for the HUD (computed before we mutate the population).
        let n = self.genomes.len().max(1);
        let total: f64 = self.scores.iter().map(|s| s.scalar()).sum();
        let best = order.first().map(|&i| self.scores[i].scalar()).unwrap_or(0.0);
        let survivors_end = self.world.living_count();

        // Elite = the actual survivors when there are enough of them to breed a
        // gradient (don't pad with best-of-the-dead); otherwise fall back to the
        // top ELITE_FRAC so we always have *someone* to breed from. The survivors
        // are exactly the prefix of `order` that is still alive (alive sorts above
        // the dead at equal score), so we take the count of survivors, capped at the
        // ELITE_FRAC ceiling so the elite never balloons to most of the population.
        let elite_cap = ((self.pop as f32 * ELITE_FRAC).round() as usize).clamp(1, self.pop);
        let elite_floor = ((self.pop as f32 * 0.02).round() as usize).clamp(1, self.pop);
        let elite_n = survivors_end.clamp(elite_floor, elite_cap).min(order.len());
        // dominant drive among the elite (read from the live minds).
        let elite_dominant = self.dominant_drive(&order[..elite_n]);
        // mean fitness over the *survivors* (the breeding pool) — a cleaner signal
        // of whether the elite is improving than the all-1000 mean, which is
        // dominated by the dead's near-zero scores. We report both.
        let surv_total: f64 = order[..survivors_end.min(order.len())]
            .iter()
            .map(|&i| self.scores[i].scalar())
            .sum();
        let elite_mean = if survivors_end > 0 {
            surv_total / survivors_end as f64
        } else {
            0.0
        };
        // mean foraging genes among the elite genomes — the "what is being selected
        // for" read. Computed from the genomes (not the expressed minds) so it's the
        // heritable material itself.
        let mut gsum = [0.0f64; 3];
        for &i in &order[..elite_n] {
            let g = &self.genomes[i].g;
            gsum[0] += g[13] as f64;
            gsum[1] += g[14] as f64;
            gsum[2] += g[15] as f64;
        }
        let en = elite_n.max(1) as f64;
        let elite_forage_genes = [
            (gsum[0] / en) as f32,
            (gsum[1] / en) as f32,
            (gsum[2] / en) as f32,
        ];
        self.last = Some(GenStats {
            mean_fitness: total / n as f64,
            elite_mean,
            best_fitness: best,
            survivors_end,
            elite_n,
            elite_dominant,
            elite_forage_genes,
        });

        // 2. keep the elite genomes.
        let elite: Vec<Genome> = order[..elite_n]
            .iter()
            .map(|&i| self.genomes[i].clone())
            .collect();

        // 3. refill to `pop` by breeding mutated offspring from the elite. Elite are
        //    carried forward unchanged (elitism); the rest are asexual mutants of a
        //    randomly chosen elite parent (uses the existing Genome::mutate — no
        //    hand-rolled mutation).
        let gain = mutation_gain();
        let mut next: Vec<Genome> = Vec::with_capacity(self.pop);
        next.extend(elite.iter().cloned());
        while next.len() < self.pop {
            let parent = &elite[self.rng.below(elite.len())];
            next.push(parent.mutate(0.06, &gain, &mut self.rng));
        }
        next.truncate(self.pop);

        // 4. respawn the island for the next generation. CRUCIAL: reuse the **same**
        //    world seed every generation so the resource layout / stalker start /
        //    spawn positions are *identical*. A re-seeded world each generation makes
        //    fitness non-comparable across generations (the same elite genome faces a
        //    different island and scores differently), which turns elitism into noise
        //    and hides any real climb — exactly the flat result the first tuning
        //    produced. With a stationary environment, a genuinely fitter genome
        //    scores higher *here*, the carried-forward elite ratchets, and
        //    improvement becomes visible. (Live-only; nothing else re-uses this.)
        self.generation += 1;
        let world_seed = self.seed; // stationary fitness landscape across generations
        self.world = build_world(world_seed, &next, self.dims);
        self.genomes = next;
        self.scores = vec![MindScore::default(); self.world.agents.len()];
        self.gen_tick = 0;
    }

    /// Most common dominant drive among a set of agent indices (the elite).
    fn dominant_drive(&self, idxs: &[usize]) -> Option<Drive> {
        let mut counts = [0u32; 6];
        for &i in idxs {
            if let Some(a) = self.world.agents.get(i) {
                if a.alive {
                    let d = a.mind.drives().dominant().0;
                    counts[drive_idx(d)] += 1;
                }
            }
        }
        let (mut bi, mut bc) = (0usize, 0u32);
        for (i, &c) in counts.iter().enumerate() {
            if c > bc {
                bc = c;
                bi = i;
            }
        }
        if bc == 0 {
            None
        } else {
            Some(Drive::ALL[bi])
        }
    }
}

fn drive_idx(d: Drive) -> usize {
    Drive::ALL.iter().position(|x| *x == d).unwrap_or(0)
}

/// Build the harsh, density-matched island for a set of per-mind genomes, with
/// **lethal starvation** on so the population dies off quickly.
fn build_world(seed: u64, genomes: &[Genome], dims: (i32, i32, i32)) -> GameWorld {
    let (w, h, sight) = dims;
    let mut world = GameWorld::with_genomes_sized_harsh(seed, genomes, w, h, sight);
    world.lethal_starvation = true;
    // OPEN WORLD: the year turns on the island, so winter (food stops + a cold drain)
    // is a real, recurring selection pressure. A generation is 10 days = 1.25 years,
    // so every cohort meets at least one full winter — minds that provision (stock the
    // granary in autumn, draw it down in winter) out-survive those that don't, and the
    // GA breeds provisioning. Lethal starvation stays on, so the unprovisioned die.
    world.set_open_world(true);
    // Loosen the harsh island just enough that ≈10-20% survive a generation —
    // a real breeding elite — instead of a near-total (~99%) wipe. Self-enclosure
    // stays fatal (lethal_starvation remains on). Live-only; the seeded harness
    // worlds never take this path.
    world.tune_for_evolution();
    world
}

/// Per-gene mutation gain — mutate every gene roughly equally. (Uniform gain keeps
/// this independent of the fitness sweep's tuned gains; we just want variation.)
fn mutation_gain() -> [f32; daimon_mind::evolve::N_GENES] {
    [1.0; daimon_mind::evolve::N_GENES]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn island_density_is_packed_for_findability() {
        // The evolution island is deliberately packed (≈40 cells/mind) so the
        // scattered springs fall within a bounded sight and foraging skill — not
        // spawn luck — decides survival. Density is held constant as pop grows, and
        // stays well below 1 mind/cell so movement still works. (This is tighter
        // than the believability village's ≈173 cells/mind, on purpose; the small
        // floor sizes (40×26) make tiny pops looser, which is fine.)
        let (w, h, _) = island_dims(1000);
        let per_mind = (w * h) as f32 / 1000.0;
        assert!((20.0..80.0).contains(&per_mind), "1000-mind density {per_mind}");
        // density well below saturation (room to move).
        assert!(per_mind > 4.0, "not over-packed: {per_mind}");
        // bigger pop ⇒ bigger island.
        let (w2, h2, _) = island_dims(2000);
        assert!(w2 * h2 > w * h);
        // minimum island floor respected for tiny pops.
        let (w3, h3, _) = island_dims(6);
        assert!(w3 >= 40 && h3 >= 26, "floor size respected: {w3}x{h3}");
    }

    #[test]
    fn generations_advance_and_population_refills() {
        // Small, fast: 24 minds, run through two generation boundaries headlessly
        // and assert the counter advances and the population refills to pop.
        let pop = 24;
        let mut ev = Evolution::new(0xABCD, pop);
        assert_eq!(ev.generation, 1);
        assert_eq!(ev.alive(), pop, "starts full");
        assert!(ev.world.lethal_starvation, "evolution world is lethal");

        let mut crossed = 0;
        let mut ticks = 0u64;
        while crossed < 2 && ticks < GEN_TICKS * 3 {
            if ev.tick() {
                crossed += 1;
                // refilled to full population right after the boundary.
                assert_eq!(ev.alive(), pop, "refilled to pop after gen boundary");
            }
            ticks += 1;
        }
        assert_eq!(crossed, 2, "crossed two generation boundaries");
        assert_eq!(ev.generation, 3, "counter advanced 1 -> 3");
        assert!(ev.last.is_some(), "last-generation stats recorded");
    }

    #[test]
    fn default_world_unaffected_by_lethal_field() {
        // The new field defaults false on every standard constructor → harness paths
        // are unchanged. (Determinism of the seeded worlds is covered by the proofs
        // and off-control ACs; this just pins the default.)
        let w = GameWorld::new(0x61, 6);
        assert!(!w.lethal_starvation);
        let g = Genome::showcase();
        let w2 = GameWorld::with_genome(0x61, 6, &g);
        assert!(!w2.lethal_starvation);
    }
}
