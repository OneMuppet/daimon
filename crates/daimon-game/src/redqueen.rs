//! **Red Queen co-evolution** — co-evolve a *predator* against the *minds* and ask
//! two questions the metabolic frontier could not answer:
//!
//! 1. **Open-endedness** — does the arms race keep improving with NO permanent
//!    winner (the two sides co-climb / oscillate), or does one side dominate and the
//!    dynamic saturate (as the static metabolic frontier did, where mastery hit a
//!    ceiling by ~gen 8)?
//! 2. **Sharper minds** — when the predator gets smarter, do the *sophisticated*
//!    mind faculties (foresight, fighting, shelter/building, social foraging,
//!    provisioning) get **selected up**, instead of being purged as they were when
//!    the only pressure was metabolic privation?
//!
//! "Red Queen": *"it takes all the running you can do, to keep in the same place."*
//! Each side's fitness is defined against the CURRENT opposing population, so a gain
//! by one side degrades the other's effective fitness — the landscape never stops
//! moving, which is exactly the condition the frontier lacked.
//!
//! ## Design (reuses the proven machinery)
//! * **Minds** are the 28-gene [`daimon_mind::Genome`] (same as `evolve_frontier`),
//!   weak/random init with only the open-world capability *switches* forced on so
//!   the sophisticated faculties are *available* to be selected but every adaptive
//!   gene starts random — a real climb from a degraded start.
//! * **Predators** are the 5-gene [`PredatorStrategy`] (this experiment's new,
//!   additive, gene-gated hunting genome), weak/random init too.
//! * **Coupled fitness, averaged over a FIXED seed-set** (the noise-control lesson):
//!   a mind is scored by survival against a *sample of the current predator
//!   population*; a predator is scored by catch-rate against a *sample of the
//!   current mind population* — the SAME seeds for every individual in a generation,
//!   so selection acts on the genome not the dice.
//! * **Truncation + elitism on BOTH**, `Genome::mutate` for minds and
//!   [`PredatorStrategy::mutate`] for predators.
//!
//! Deterministic: one seeded [`Rng`] drives all init/selection/reproduction; the
//! per-pairing worlds are seeded off the run seed + (generation, slot). No neural
//! nets. Additive: the only sim change is the optional [`PredatorStrategy`], whose
//! default is byte-identical to the incumbent stalker.
//!
//! The outer generational loop + the report + the held-out head-to-head ladders live
//! in `examples/redqueen.rs`; this module holds the *coupled evaluation* (the part
//! worth unit-testing in isolation) so the example stays a thin driver.

use daimon_core::Rng;
use daimon_mind::Genome;

use crate::sim::{GameWorld, PredatorStrategy};

/// Minds per evaluated world. Survival is a per-agent mean → a stable signal.
pub const MINDS_PER_WORLD: usize = 6;

/// Build the **predator arena** — the fixed environment the arms race runs in.
///
/// THE KEY DESIGN DECISION (and why we do NOT reuse `EnvParams::build_world`): the
/// metabolic open world kills *everyone by winter/starvation* long before the
/// predator matters (probed: a weak mind vs a nasty predator dies 6/6 to hunger,
/// 0/6 to the stalker, at every difficulty). That gives the predator ZERO selection
/// signal and reduces the minds' pressure to the very metabolic axis the frontier
/// already exhausted. So the arena here is a **harsh CLOSED world** (the relentless
/// stalker is live; no seasons, no winter cold) with mortality ON but
/// `lethal_starvation` OFF and ample resources — so a mind that forages competently
/// does NOT starve, and the **predator becomes the dominant cause of death**. Now
/// catch-rate is a real predator-fitness signal and the minds are selected on
/// *anti-predator* faculties (fight, build/shelter, foresight, dispersal) — exactly
/// the sophistication the experiment asks about.
///
/// The environment is held FIXED across all generations, so any change in survival
/// is attributable to the EVOLVING PREDATOR, not a difficulty ratchet — the Red
/// Queen itself is the moving difficulty. Seeded entirely off `seed`; deterministic.
pub fn build_arena(seed: u64, mind: &Genome) -> GameWorld {
    let genomes: Vec<Genome> = (0..MINDS_PER_WORLD).map(|_| mind.clone()).collect();
    // a compact island sized like the frontier/POET arenas (≈55 cells/mind).
    let pop = genomes.len().max(1);
    let area = (pop as f32) * 55.0;
    let aspect = 40.0 / 26.0;
    let h = ((area / aspect).sqrt().round()).max(26.0) as i32;
    let w = ((h as f32) * aspect).round().max(40.0) as i32;
    let sight = 9;
    let mut world = GameWorld::with_genomes_sized_harsh(seed, &genomes, w, h, sight);
    // mortality is in the genome (can_die on); keep starvation NON-lethal so the
    // predator — not hunger — is what kills. Ample resources reinforce this.
    world.lethal_starvation = false;
    world.metabolism_scale = 0.6; // gentle drain — competent foraging keeps fed
    world.set_resource_counts(pop * 2, pop * 2); // ample food + water
    // PREDATOR-DOMINATED lethality. Calibration (probe): at bite≈3.5, move every
    // tick, a NASTY hunting strategy kills weak minds (~1/6) while a SHARP mind
    // (fight/build/foresight) survives untouched, and the INCUMBENT strategy at the
    // same bite kills nobody — so there is genuine headroom on BOTH sides for the
    // arms race to climb into. (The default-equivalence guarantee is unaffected:
    // set_stalker only scales the existing bite/period fields; the default strategy
    // still takes the byte-identical incumbent code path.)
    world.set_stalker(ARENA_BITE, 1);
    world
}

/// Per-bite damage scale for the arena (see [`build_arena`] calibration).
pub const ARENA_BITE: f32 = 3.5;

/// Evaluate ONE mind genome against ONE predator strategy on ONE seeded world.
/// Returns `(mind_survival, predator_catch_rate)` for that single pairing:
/// * `mind_survival` ∈ [0,1] — mean graded survival (fraction of ticks lived) over
///   the world's minds. This is the mind's payoff.
/// * `predator_catch_rate` ∈ [0,1] — fraction of the world's minds the predator
///   caught (died to "the stalker") by the end. This is the predator's payoff.
///
/// One coupled simulation produces BOTH payoffs — no double work.
pub fn duel(
    mind: &Genome,
    predator: &PredatorStrategy,
    eval_ticks: u64,
    seed: u64,
) -> (f32, f32) {
    let mut world = build_arena(seed, mind);
    world.set_predator_strategy(*predator);

    let n = world.agents.len().max(1);
    let mut alive_ticks = vec![0u64; n];
    for _ in 0..eval_ticks {
        world.step();
        for (i, a) in world.agents.iter().enumerate() {
            if a.alive {
                alive_ticks[i] += 1;
            }
        }
    }
    let t = eval_ticks as f64;
    let mut graded = 0.0f64;
    let mut caught = 0u64;
    for (i, a) in world.agents.iter().enumerate() {
        graded += alive_ticks[i] as f64 / t;
        // a mind that died specifically to the predator counts as a "catch"; this is
        // the predator's payoff (the rare non-predator death is not its credit).
        if !a.alive && a.death_cause == "the stalker" {
            caught += 1;
        }
    }
    let mind_survival = (graded / n as f64) as f32;
    let catch_rate = caught as f32 / n as f32;
    (mind_survival, catch_rate)
}

/// Score a mind against a *sample* of the current predator population over a FIXED
/// seed-set: returns its **mean survival** (higher = better mind). Each (predator,
/// seed) pairing is one duel; the same pairings are used for every mind in the
/// generation, so the comparison is fair and low-noise.
pub fn mind_fitness(
    mind: &Genome,
    predators: &[PredatorStrategy],
    eval_ticks: u64,
    seeds: &[u64],
) -> f32 {
    let mut acc = 0.0f32;
    let mut n = 0u32;
    for pred in predators {
        for &sd in seeds {
            acc += duel(mind, pred, eval_ticks, sd).0;
            n += 1;
        }
    }
    acc / n.max(1) as f32
}

/// Score a predator against a *sample* of the current mind population over a FIXED
/// seed-set: returns its **mean catch-rate** (higher = better predator).
pub fn predator_fitness(
    predator: &PredatorStrategy,
    minds: &[Genome],
    eval_ticks: u64,
    seeds: &[u64],
) -> f32 {
    let mut acc = 0.0f32;
    let mut n = 0u32;
    for mind in minds {
        for &sd in seeds {
            acc += duel(mind, predator, eval_ticks, sd).1;
            n += 1;
        }
    }
    acc / n.max(1) as f32
}

/// A WEAK, random mind genome with only the open-world capability *switches* forced
/// on (so the sophisticated faculties are *available* to be selected) and the NN
/// overlay off (this is a NO-NN experiment). Every adaptive gene — foresight,
/// fighting, building, social/DRR foraging, provisioning, affect, cultural — starts
/// RANDOM, so a sweep of any of them is genuine selection, not a fixed prior. This
/// mirrors `evolve_frontier::weak_random`.
pub fn weak_mind(rng: &mut Rng) -> Genome {
    let mut g = Genome::random(rng);
    g.g[20] = 1.0; // can_fight available (option to confront the predator)
    g.g[21] = 1.0; // can_build available (shelter affordance)
    g.g[22] = 1.0; // can_die on — predation/winter can actually kill (survival gradient)
    g.g[24] = 1.0; // can_provision available (winter stepping stone)
    g.g[25] = 0.0; // nn overlay off
    g.g[26] = 0.0;
    g.g[27] = 0.0;
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The DEFAULT predator strategy must reproduce the incumbent stalker EXACTLY —
    /// duelling a mind against a `None`-strategy world (the harness path) and against
    /// an explicitly-default-strategy world must give bit-identical payoffs.
    #[test]
    fn default_strategy_matches_incumbent() {
        let mind = Genome::showcase();
        let seed = 0x5EED_1234u64;

        // incumbent: build the arena, do NOT set a strategy.
        let mut w_inc = build_arena(seed, &mind);
        let mut w_def = build_arena(seed, &mind);
        w_def.set_predator_strategy(PredatorStrategy::default());

        // step both in lockstep; the predator positions must stay identical tick by
        // tick (byte-identical RNG consumption + movement).
        for _ in 0..1500 {
            w_inc.step();
            w_def.step();
            assert_eq!(
                w_inc.predator.pos, w_def.predator.pos,
                "default strategy diverged from the incumbent stalker"
            );
        }
        // and the surviving population matches exactly.
        let inc_alive: Vec<bool> = w_inc.agents.iter().map(|a| a.alive).collect();
        let def_alive: Vec<bool> = w_def.agents.iter().map(|a| a.alive).collect();
        assert_eq!(inc_alive, def_alive, "default strategy changed who survived");
    }

    /// Predator mutation/random are deterministic on a seeded RNG.
    #[test]
    fn predator_mutation_is_deterministic() {
        let mut r1 = Rng::new(7);
        let mut r2 = Rng::new(7);
        let a = PredatorStrategy::random(&mut r1).mutate(0.1, &mut r1);
        let b = PredatorStrategy::random(&mut r2).mutate(0.1, &mut r2);
        assert_eq!(a.g, b.g);
    }
}
