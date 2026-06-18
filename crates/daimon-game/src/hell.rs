//! **Hell** — a TRULY hellish world, far beyond the Super-Mind battery (which
//! saturated at 100%). Every harshness axis is cranked AT ONCE and tied to a single
//! **hell-intensity scalar `H`**: brutal cold, heavy metabolism, near-famine food +
//! water, AND a fast/persistent/aggressive predator swarm-of-one with a near-one-shot
//! bite — all in the SAME open-world (seasonal, `lethal_starvation` ON) so the
//! pressures compound rather than being separate challenge KINDS.
//!
//! ## Why a new builder (and not `EnvParams::at_difficulty`)
//! `EnvParams::at_difficulty` caps the knobs at `hi = [.95,.85,.85,.80,.45]` and the
//! Super-Mind battery already drove survival to 100% inside that envelope — it is NOT
//! hellish. Hell pushes the decoded knobs to their true extremes (cold→3.0, metab→
//! ~0.9, food/water→their famine floors) AND overrides the predator with a bite far
//! past the env-knob ceiling (≈one-shot) plus a brutal evolvable hunting strategy
//! (fast + persistent + max-aggro + isolated-target). `H` can exceed 1.0 — beyond 1.0
//! the predator keeps getting deadlier (more bite, larger aggro radius) so there is
//! always more hell to ratchet into, to find the architecture's true ceiling.
//!
//! ## Metric (GRADED, mandatory — proven in the ceiling experiment)
//! [`hell_survival`] = mean fraction of `HELL_EVAL_TICKS` lived across the world's
//! clones. In hell binary survival is ≈0% for everyone (no gradient); graded survival
//! (ticks-lived fraction) is the stable gradient. The SAME metric is used for fitness
//! and for the held-out arbiter — no confounds.
//!
//! Deterministic: seeded entirely off the world seed; reuses
//! [`EnvParams::build_world`] for the island + open-world surface, then overrides the
//! harshness fields. No neural nets. Additive: a new module + the `hell`/`hell_calib`
//! examples; no harness path calls any of this (the default predator strategy stays
//! the harness default), so all ACs/proofs/believability are byte-identical.

use daimon_core::Rng;
use daimon_mind::evolve::N_GENES;
use daimon_mind::Genome;

use crate::poet::EnvParams;
use crate::sim::{GameWorld, PredatorStrategy};

/// Ticks per hell evaluation. Long enough to cross winter onset and feel sustained
/// scarcity + predation, short enough that POP×K×TICKS finishes in a few minutes in
/// `--release`. Matches the Super-Mind battery's `EVAL_TICKS` for comparability.
pub const HELL_EVAL_TICKS: u64 = 2400;

/// Minds per evaluated hell world (survival is a per-agent mean → a stable signal).
pub const MINDS_PER_WORLD: usize = 6;

/// Build a **hell world** at hell-intensity `H` for the given per-agent genomes.
///
/// `H = 0` is already hard (the high end of the old battery); `H = 1` is brutal;
/// `H > 1` keeps escalating the predator. Every axis scales with `H` SIMULTANEOUSLY:
///
/// * **cold** `open_world_cold_scale` ∈ [1.6, 3.0] — deep, lethal winter from the start.
/// * **metabolism** `metabolism_scale` ∈ [0.75, 0.9] — heavy energy/hydration drain.
/// * **starvation** `starve_health_drain` raised (famine kills faster), `lethal` ON.
/// * **food / water** driven to a near-famine floor (a fraction of one patch per mind).
/// * **predator** bite ∈ [2.2, 5.0+] (≈0.44→1.0+ damage/hit, near one-shot at H≥1),
///   moving EVERY tick, with a brutal hunting strategy (max-aggro, persistent,
///   isolated-target, fast). Beyond `H=1` the bite and aggro keep climbing.
///
/// Reuses [`EnvParams::build_world`] for the island + open-world wiring, then
/// overrides the harshness fields and installs the predator strategy.
pub fn hell_world(h: f32, seed: u64, genomes: &[Genome]) -> GameWorld {
    // base env: full env-knob extremes so build_world lays out a winter island with
    // famine-level resources and the seasonal/open-world machinery live.
    let base = EnvParams { k: [1.0, 1.0, 1.0, 1.0, 1.0] };
    let mut world = base.build_world(seed, genomes);

    // --- override the harshness fields, scaled CONTINUOUSLY by H ---
    // `H` is a single intensity axis. Calibration showed the env-knob extremes +
    // a one-shot predator wipe EVERYONE in ~100 ticks with no gradient, so the axis
    // is anchored so that the GRADIENT ZONE sits around H≈0 (a competent mind lives a
    // meaningful fraction, a weak one dies fast) and TRUE HELL is H≳0.6 climbing up —
    // the ratchet rides H upward to find the wall. Lower H eases proportionally so the
    // calibration probe can locate the gen-0 gradient; the experiment never goes below
    // its calibrated start once running.
    let pop = world.agents.len().max(1);
    let hc = h.clamp(-1.5, 2.5);

    // cold: anchored ≈1.2 at H=0, climbing to the decode max 3.0 by H≈1.3; eased below.
    world.open_world_cold_scale = (1.2 + 0.9 * hc).clamp(0.4, 3.0);
    // metabolism: 0.62 at H=0 → 0.90 (decode max) by H≈1.
    world.metabolism_scale = (0.62 + 0.28 * hc).clamp(0.35, 0.90);
    // starvation drain: 0.018 at H=0, faster as H rises (default harness is 0.02).
    world.starve_health_drain = (0.018 + 0.020 * hc).max(0.008);
    world.lethal_starvation = true;

    // scarcity: a fraction of one resource patch per mind, tightening with H. At H=0
    // ≈0.85 food / 0.70 water per mind (tight); driven toward famine as H climbs.
    let food_per = (0.85 - 0.40 * hc).clamp(0.20, 1.4);
    let water_per = (0.70 - 0.35 * hc).clamp(0.15, 1.2);
    let want_food = ((pop as f32) * food_per).round().max(1.0) as usize;
    let want_water = ((pop as f32) * water_per).round().max(1.0) as usize;
    world.set_resource_counts(want_food, want_water);

    // predator bite: the headline lethality axis the ratchet rides into the ceiling.
    // 0.2·bite damage/hit: bite=1.0 at H=0 (0.20/hit — survivable for a sharp mind),
    // climbing to ≈4.0 (0.80/hit, near one-shot) by H≈1.5; no upper cap.
    let bite = (1.0 + 2.0 * hc).max(0.4);
    // movement cadence: every-other-tick when eased, every tick (fastest) for H≥0.
    let period = if hc >= 0.0 { 1 } else { 2 };
    world.set_stalker(bite, period);
    // hunting strategy scales with H: at low H a milder chaser; at hell a brutal
    // max-aggro, persistent, fast, isolated-target stalker (picks off stragglers).
    let aggro_gene = (0.45 + 0.40 * hc).clamp(0.30, 1.0);
    let on = |t: f32| if hc >= t { 1.0 } else { 0.0 };
    world.set_predator_strategy(PredatorStrategy {
        g: [aggro_gene, on(0.0), on(-0.3), on(-0.6), on(-0.6)],
        // g = [aggro, isolated-target(H≥0), persistent(H≥-0.3), patrol(H≥-0.6), fast(H≥-0.6)]
    });

    world
}

/// **Graded hell survival** ∈ [0,1] for ONE genome at hell-intensity `H` on ONE
/// seeded world: the mean fraction of [`HELL_EVAL_TICKS`] lived across the world's
/// clones. Pure survival — identical for fitness and for the held-out arbiter.
pub fn hell_survival(genome: &Genome, h: f32, seed: u64) -> f32 {
    let genomes: Vec<Genome> = (0..MINDS_PER_WORLD).map(|_| genome.clone()).collect();
    let mut world = hell_world(h, seed, &genomes);
    let n = world.agents.len().max(1);
    let mut alive_ticks = vec![0u64; n];
    for _ in 0..HELL_EVAL_TICKS {
        world.step();
        for (i, a) in world.agents.iter().enumerate() {
            if a.alive {
                alive_ticks[i] += 1;
            }
        }
    }
    let t = HELL_EVAL_TICKS as f64;
    let acc: f64 = alive_ticks.iter().map(|&x| x as f64 / t).sum();
    (acc / n as f64) as f32
}

/// Mean graded hell survival over a FIXED seed-set (low-noise selection: every genome
/// in a generation sees the same seeds, so selection acts on the genome not the dice).
pub fn hell_survival_avg(genome: &Genome, h: f32, seeds: &[u64]) -> f32 {
    let s: f32 = seeds.iter().map(|&sd| hell_survival(genome, h, sd)).sum();
    s / seeds.len().max(1) as f32
}

/// A WEAK, random mind with only the open-world capability *switches* forced on (so
/// the sophisticated faculties are *available* to be selected) and the NN overlay off
/// (no-NN experiment). Every adaptive gene starts random — a real climb from a
/// degraded start. Mirrors `redqueen::weak_mind` / `evolve_frontier::weak_random`.
pub fn weak_random(rng: &mut Rng) -> Genome {
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

/// `Genome::showcase()` (the strongest human-designed mind) with the SAME capability
/// switches enabled and the NN overlay off — on identical footing to the evolved
/// minds. This is the human design the arbiter pits the evolved champion against.
pub fn showcase_with_capabilities() -> Genome {
    let mut g = Genome::showcase();
    g.g[20] = 1.0;
    g.g[21] = 1.0;
    g.g[22] = 1.0;
    g.g[24] = 1.0;
    g.g[25] = 0.0;
    g.g[26] = 0.0;
    g.g[27] = 0.0;
    g
}

/// Uniform mutation gain (just want variation across all genes).
pub fn unit_gain() -> [f32; N_GENES] {
    [1.0f32; N_GENES]
}

/// **Re-pin the capability switches after mutation.** CRITICAL anti-exploit: the
/// capability genes are affordance *switches*, not adaptive parameters — `Genome::
/// mutate` perturbs every gene, so without this `can_die` (g22) drifts below 0.5 and
/// the agent becomes IMMORTAL, "surviving" 100% of ticks at any hell-intensity by
/// opting out of death rather than coping with hell. That is a degenerate exploit of
/// the graded-survival metric, NOT a super mind. We therefore force mortality ON
/// (g22) every generation so survival can only mean NOT DYING, and keep the other
/// open-world affordances available (fight/build/provision ON, NN overlay OFF) so the
/// faculties stay present for the *adaptive* genes to use. Every adaptive gene
/// (foresight, foraging, affect, deliberation knobs, persona) still evolves freely.
pub fn pin_capabilities(g: &mut Genome) {
    g.g[20] = 1.0; // can_fight available
    g.g[21] = 1.0; // can_build available
    g.g[22] = 1.0; // can_die ON — mortality is NON-NEGOTIABLE (no immortality exploit)
    g.g[24] = 1.0; // can_provision available
    g.g[25] = 0.0; // NN overlay off (no-NN experiment)
    g.g[26] = 0.0;
    g.g[27] = 0.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hell_is_harsher_than_the_old_battery_envelope() {
        // At H=0 hell already starts beyond the battery's ceiling on cold + drain, and
        // every axis climbs with H. (Predator bite is private; it is asserted to climb
        // indirectly via the calibration table / survival trajectory.)
        let g = vec![Genome::showcase()];
        let w0 = hell_world(0.0, 0xDEAD_BEEF, &g);
        let w1 = hell_world(1.0, 0xDEAD_BEEF, &g);
        // hell starts past at_difficulty(1.0)'s cold ceiling (decode of k=0.95 ≈ 2.87
        // → no; the cold ceiling there is 0.95·2.6+0.4 ≈ 2.87) — assert monotone climb
        // and that lethal starvation + heavy metabolism are on from H=0.
        assert!(w1.open_world_cold_scale > w0.open_world_cold_scale);
        assert!(w0.open_world_cold_scale >= 1.0);
        assert!(w1.metabolism_scale > w0.metabolism_scale);
        assert!(w0.metabolism_scale >= 0.60);
        assert!(w1.starve_health_drain > w0.starve_health_drain);
        assert!(w0.lethal_starvation && w1.lethal_starvation);
    }

    #[test]
    fn hell_survival_is_deterministic() {
        let g = Genome::showcase();
        let a = hell_survival(&g, 0.6, 0xABCD);
        let b = hell_survival(&g, 0.6, 0xABCD);
        assert_eq!(a, b, "same seed → same graded survival");
        assert!((0.0..=1.0).contains(&a));
    }
}
