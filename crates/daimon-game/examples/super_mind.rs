//! "Evolving a Super Mind" — does diverse selection produce a generalist mind
//! that BEATS the hand-designed `Genome::showcase()` (the best human-tuned mind)
//! on a HELD-OUT battery of diverse hard worlds?
//!
//! Design (see the task brief):
//!   * Weak/random initial minds — capability genes (fight/build/die/provision)
//!     forced ON so the faculties are *available* to evolve; adaptive genes random.
//!   * Fitness = MEAN graded survival across 5 *qualitatively different* challenge
//!     KINDS, each averaged over K fixed seeds (low noise). A generalist is
//!     required — no single faculty wins all five.
//!   * Truncation + elitism + `Genome::mutate` (frontier recipe).
//!   * THE ARBITER: at the end, evaluate evolved champion vs `Genome::showcase()`
//!     (capability genes enabled) vs gen-0 champion on a HELD-OUT battery — same
//!     challenge KINDS, DISJOINT seeds, never trained on.
//!
//! ONE clean deterministic run. Fixed seed, params fixed up front. Survival is the
//! sole metric (graded fraction of EVAL_TICKS lived, averaged over minds & seeds),
//! used identically for fitness and for the arbiter — no confounds.

use daimon_core::Rng;
use daimon_game::poet::EnvParams;
use daimon_game::redqueen::duel;
use daimon_game::sim::PredatorStrategy;
use daimon_mind::evolve::N_GENES;
use daimon_mind::Genome;

// ----------------------------- fixed hyperparameters -----------------------------
const RUN_SEED: u64 = 0x50DE_C0DE_5EED_0FFE_u64;
const POP: usize = 60;
const GENERATIONS: usize = 50;
const SELECT_FRAC: f32 = 0.22;
const ELITES: usize = 4;
const MUT_SIGMA: f32 = 0.06;

const MINDS_PER_WORLD: usize = 6; // clones per env world
const EVAL_TICKS: u64 = 2400; // long enough to cross a winter & feel scarcity
const K_TRAIN_SEEDS: usize = 3; // fixed seeds per challenge per generation
const K_HELD_SEEDS: usize = 6; // held-out seeds for the arbiter (DISJOINT)

// Predator challenge: brutal hand-picked stalkers (fast + persistent + aggressive).
const PRED_EVAL_TICKS: u64 = 1800;

// ----------------------------- challenge battery ---------------------------------
// Five qualitatively different challenge KINDS. Each is stressed on ONE axis hard
// while the others stay moderate, so a mind that only solves one axis cannot win
// the aggregate — a generalist is required.

#[derive(Clone)]
enum Challenge {
    /// Env-knob world (cold, metabolism, food, water, stalker) via EnvParams.
    Env { name: &'static str, env: EnvParams },
    /// Predator-heavy arena: survive a sample of brutal evolvable predators.
    Predators { name: &'static str, preds: Vec<PredatorStrategy> },
}

impl Challenge {
    fn name(&self) -> &'static str {
        match self {
            Challenge::Env { name, .. } => name,
            Challenge::Predators { name, .. } => name,
        }
    }
}

/// EnvParams knobs: [cold, metabolism, food_scarcity, water_scarcity, stalker].
fn battery() -> Vec<Challenge> {
    vec![
        // (a) brutal cold + metabolism — the frontier "high-D" winter world.
        Challenge::Env {
            name: "cold_metabolism",
            env: EnvParams { k: [0.97, 0.88, 0.25, 0.25, 0.10] },
        },
        // (b) food + water scarcity — famine/drought; foraging discipline matters.
        Challenge::Env {
            name: "food_water_scarcity",
            env: EnvParams { k: [0.20, 0.45, 0.92, 0.92, 0.10] },
        },
        // (c) seasonal winter requiring provisioning — strong cold, tight food.
        Challenge::Env {
            name: "seasonal_provision",
            env: EnvParams { k: [0.90, 0.55, 0.70, 0.35, 0.10] },
        },
        // (d) mixed / hardest — everything cranked (capped stalker via env knob).
        Challenge::Env { name: "mixed_hardest", env: EnvParams::hard_target() },
        // (e) predator-heavy — strong evolvable stalkers (handled via redqueen::duel).
        Challenge::Predators {
            name: "predator_swarm",
            preds: vec![
                // fast + persistent + nearest, max aggro (relentless chaser)
                PredatorStrategy { g: [1.0, 0.0, 1.0, 0.0, 1.0] },
                // fast + patrol/ambush + isolated-target (picks off stragglers)
                PredatorStrategy { g: [0.9, 0.9, 1.0, 1.0, 1.0] },
            ],
        },
    ]
}

// ----------------------------- scoring (survival) --------------------------------
// Graded survival on ONE env world (mean fraction of EVAL_TICKS lived across the
// MINDS_PER_WORLD clones). Pure survival — no nourishment/payoff terms — so the
// metric is identical for fitness and for the held-out arbiter.
fn env_survival_on(genome: &Genome, env: &EnvParams, seed: u64) -> f32 {
    let genomes: Vec<Genome> = (0..MINDS_PER_WORLD).map(|_| genome.clone()).collect();
    let mut world = env.build_world(seed, &genomes);
    let n = world.agents.len().max(1);
    let mut alive_ticks = vec![0u64; n];
    for _ in 0..EVAL_TICKS {
        world.step();
        for (i, a) in world.agents.iter().enumerate() {
            if a.alive {
                alive_ticks[i] += 1;
            }
        }
    }
    let t = EVAL_TICKS as f64;
    let acc: f64 = alive_ticks.iter().map(|&x| x as f64 / t).sum();
    (acc / n as f64) as f32
}

/// Mean graded survival of a genome on ONE challenge over a seed-set.
fn challenge_survival(genome: &Genome, ch: &Challenge, seeds: &[u64]) -> f32 {
    match ch {
        Challenge::Env { env, .. } => {
            let s: f32 = seeds.iter().map(|&sd| env_survival_on(genome, env, sd)).sum();
            s / seeds.len() as f32
        }
        Challenge::Predators { preds, .. } => {
            // mean survival across (predator, seed) pairs — duel returns graded survival.
            let mut acc = 0.0f32;
            let mut n = 0u32;
            for p in preds {
                for &sd in seeds {
                    acc += duel(genome, p, PRED_EVAL_TICKS, sd).0;
                    n += 1;
                }
            }
            acc / n.max(1) as f32
        }
    }
}

/// Aggregate fitness = MEAN survival across all challenge KINDS (generalist metric).
/// Returns (aggregate, per_challenge_vec).
fn aggregate_fitness(
    genome: &Genome,
    battery: &[Challenge],
    seeds_per_challenge: &[Vec<u64>],
) -> (f32, Vec<f32>) {
    let per: Vec<f32> = battery
        .iter()
        .zip(seeds_per_challenge)
        .map(|(ch, seeds)| challenge_survival(genome, ch, seeds))
        .collect();
    let agg = per.iter().sum::<f32>() / per.len() as f32;
    (agg, per)
}

// ----------------------------- population helpers --------------------------------
/// Weak/random mind: random adaptive genes; open-world capability genes ON so the
/// faculties are *available* to evolve. (Mirrors evolve_frontier::weak_random.)
fn weak_random(rng: &mut Rng) -> Genome {
    let mut g = [0.0f32; N_GENES];
    for x in g.iter_mut() {
        *x = rng.next_f32();
    }
    g[20] = 1.0; // can_fight
    g[21] = 1.0; // can_build
    g[22] = 1.0; // can_die (mortality)
    g[24] = 1.0; // can_provision
    g[25] = 0.0; // nn_enabled OFF (no NN — additive symbolic faculties only)
    g[26] = 0.0;
    g[27] = 0.0;
    Genome { g }
}

/// `showcase()` with the same open-world capability genes enabled — the strongest
/// HUMAN-DESIGNED mind, set up on identical footing to the evolved minds.
fn showcase_with_capabilities() -> Genome {
    let mut g = Genome::showcase();
    g.g[20] = 1.0; // can_fight (showcase already sets this, kept explicit)
    g.g[21] = 1.0; // can_build
    g.g[22] = 1.0; // can_die
    g.g[24] = 1.0; // can_provision
    g.g[25] = 0.0; // NN overlay OFF — match the evolved minds (no learned NN)
    g.g[26] = 0.0;
    g.g[27] = 0.0;
    g
}

// Per-generation, per-challenge TRAIN seed-set (fixed within a generation; varies
// across generations so we don't overfit a single instance — but DISJOINT from the
// held-out band by construction).
fn train_seeds(gen: usize, ch_idx: usize) -> Vec<u64> {
    let base = RUN_SEED
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((gen as u64) << 16)
        .wrapping_add((ch_idx as u64) << 4);
    (0..K_TRAIN_SEEDS as u64).map(|i| base ^ (i.wrapping_mul(0xD1A3_0001))).collect()
}

// Held-out seed-set: a DISJOINT high band the training loop never touches.
const HELDOUT_TAG: u64 = 0xCE1D_0FF0_BACE_DEAD_u64;
fn heldout_seeds(ch_idx: usize) -> Vec<u64> {
    let base = HELDOUT_TAG
        .wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        .wrapping_add((ch_idx as u64) << 8);
    (0..K_HELD_SEEDS as u64).map(|i| base ^ (i.wrapping_mul(0x27D4_EB2F_1657_4DA1))).collect()
}

// ----------------------------- the run -------------------------------------------
fn main() {
    let mut rng = Rng::new(RUN_SEED);
    let battery = battery();
    let n_ch = battery.len();

    println!("=== EVOLVING A SUPER MIND ===");
    println!("run_seed={RUN_SEED:#x}  pop={POP}  gens={GENERATIONS}  minds/world={MINDS_PER_WORLD}");
    println!("eval_ticks={EVAL_TICKS}  k_train_seeds={K_TRAIN_SEEDS}  k_held_seeds={K_HELD_SEEDS}");
    println!("battery ({n_ch} KINDS):");
    for ch in &battery {
        println!("  - {}", ch.name());
    }

    // ---- population init: weak/random minds ----
    let mut pop: Vec<Genome> = (0..POP).map(|_| weak_random(&mut rng)).collect();

    // ---- score gen-0 (WEAK-BASELINE CHECK) ----
    let gen0_seeds: Vec<Vec<u64>> = (0..n_ch).map(|c| train_seeds(0, c)).collect();
    let mut gen0_scored: Vec<(f32, Vec<f32>, usize)> = pop
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let (agg, per) = aggregate_fitness(g, &battery, &gen0_seeds);
            (agg, per, i)
        })
        .collect();
    gen0_scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    let gen0_champ = pop[gen0_scored[0].2].clone();
    let gen0_best_agg = gen0_scored[0].0;
    let gen0_mean_agg: f32 =
        gen0_scored.iter().map(|s| s.0).sum::<f32>() / gen0_scored.len() as f32;

    println!("\n--- WEAK-BASELINE CHECK (gen-0, training seeds) ---");
    println!("gen-0 BEST aggregate survival = {:.1}%", gen0_best_agg * 100.0);
    println!("gen-0 MEAN aggregate survival = {:.1}%", gen0_mean_agg * 100.0);
    print!("gen-0 best per-challenge:");
    for (i, ch) in battery.iter().enumerate() {
        print!("  {}={:.0}%", ch.name(), gen0_scored[0].1[i] * 100.0);
    }
    println!();
    if gen0_best_agg < 0.40 {
        println!("WEAK BASELINE OK (best < 40%) — there is headroom to measure improvement.");
    } else {
        println!(
            "WARNING: gen-0 best >= 40% ({:.1}%) — battery may be too easy to show improvement.",
            gen0_best_agg * 100.0
        );
    }

    // ---- evolution loop ----
    let n_parents = ((POP as f32 * SELECT_FRAC) as usize).max(2);
    let gain = [1.0f32; N_GENES];
    let mut champion = gen0_champ.clone();

    println!("\n--- EVOLUTION ({GENERATIONS} generations) ---");
    for gen in 0..GENERATIONS {
        let seeds: Vec<Vec<u64>> = (0..n_ch).map(|c| train_seeds(gen, c)).collect();
        let mut scored: Vec<(f32, Vec<f32>, usize)> = pop
            .iter()
            .enumerate()
            .map(|(i, g)| {
                let (agg, per) = aggregate_fitness(g, &battery, &seeds);
                (agg, per, i)
            })
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));

        let best_agg = scored[0].0;
        champion = pop[scored[0].2].clone();

        if gen % 5 == 0 || gen == GENERATIONS - 1 {
            print!("gen {gen:2}: best_agg={:.1}%  per:", best_agg * 100.0);
            for (i, ch) in battery.iter().enumerate() {
                print!(" {}={:.0}%", ch.name(), scored[0].1[i] * 100.0);
            }
            println!();
        }

        // truncation + elitism + mutation
        let parents: Vec<Genome> =
            scored[..n_parents].iter().map(|s| pop[s.2].clone()).collect();
        let mut next: Vec<Genome> = Vec::with_capacity(POP);
        for s in scored.iter().take(ELITES) {
            next.push(pop[s.2].clone());
        }
        while next.len() < POP {
            let p = &parents[rng.below(parents.len())];
            next.push(p.mutate(MUT_SIGMA, &gain, &mut rng));
        }
        pop = next;
    }

    // ---- THE ARBITER: held-out battery (disjoint seeds, never trained on) ----
    let held: Vec<Vec<u64>> = (0..n_ch).map(heldout_seeds).collect();
    let showcase = showcase_with_capabilities();

    let (champ_agg, champ_per) = aggregate_fitness(&champion, &battery, &held);
    let (show_agg, show_per) = aggregate_fitness(&showcase, &battery, &held);
    let (g0_agg, g0_per) = aggregate_fitness(&gen0_champ, &battery, &held);

    println!("\n=== THE ARBITER — HELD-OUT BATTERY (disjoint seeds) ===");
    println!(
        "{:<22} {:>10} {:>10} {:>10}",
        "challenge", "EVOLVED", "SHOWCASE", "GEN-0"
    );
    for (i, ch) in battery.iter().enumerate() {
        println!(
            "{:<22} {:>9.1}% {:>9.1}% {:>9.1}%",
            ch.name(),
            champ_per[i] * 100.0,
            show_per[i] * 100.0,
            g0_per[i] * 100.0
        );
    }
    println!("{:-<54}", "");
    println!(
        "{:<22} {:>9.1}% {:>9.1}% {:>9.1}%",
        "AGGREGATE",
        champ_agg * 100.0,
        show_agg * 100.0,
        g0_agg * 100.0
    );

    // per-challenge wins for the champion vs showcase
    let mut champ_wins = 0usize;
    for i in 0..n_ch {
        if champ_per[i] > show_per[i] {
            champ_wins += 1;
        }
    }

    // ---- champion gene profile ----
    println!("\n=== CHAMPION GENE PROFILE (what a 'super mind' looks like) ===");
    print_profile(&champion);
    println!("\n--- (for reference) SHOWCASE gene profile ---");
    print_profile(&showcase);

    // ---- VERDICT ----
    println!("\n=== VERDICT ===");
    println!(
        "evolved held-out aggregate = {:.1}%   showcase = {:.1}%   gen-0 = {:.1}%",
        champ_agg * 100.0,
        show_agg * 100.0,
        g0_agg * 100.0
    );
    println!("champion beat showcase on {champ_wins}/{n_ch} individual challenges.");
    let agg_better = champ_agg > show_agg;
    let majority_wins = champ_wins * 2 > n_ch;
    if agg_better && majority_wins {
        println!(
            "SUPER MIND — evolution beat the human design (champion {:.1}% vs showcase {:.1}% held-out, winning {champ_wins}/{n_ch} challenges).",
            champ_agg * 100.0,
            show_agg * 100.0
        );
    } else if (champ_agg - show_agg).abs() <= 0.02 {
        println!(
            "MATCHES — evolution equalled but did not exceed showcase (champion {:.1}% vs showcase {:.1}% held-out).",
            champ_agg * 100.0,
            show_agg * 100.0
        );
    } else if agg_better && !majority_wins {
        println!(
            "PARTIAL — champion wins the aggregate ({:.1}% vs {:.1}%) but only {champ_wins}/{n_ch} individual challenges; not a clean super-mind.",
            champ_agg * 100.0,
            show_agg * 100.0
        );
    } else {
        println!(
            "BELOW — human design still wins (champion {:.1}% vs showcase {:.1}% held-out).",
            champ_agg * 100.0,
            show_agg * 100.0
        );
    }
}

fn onoff(v: f32) -> &'static str {
    if v >= 0.5 {
        "ON "
    } else {
        "off"
    }
}

fn print_profile(g: &Genome) {
    let x = &g.g;
    println!(
        "  scalars: surprise={:.2} delib_cd={:.2} tie={:.2} reflect={:.2} plan_stale={:.2} foresight={:.2}",
        x[0], x[1], x[2], x[3], x[4], x[13]
    );
    println!(
        "  persona: bold={:+.2} social={:+.2} curious={:+.2}",
        x[5] - 0.5,
        x[6] - 0.5,
        x[7] - 0.5
    );
    println!(
        "  faculties: empower={} consolid={} imagine={} meta_mot={} quantum={} foresight_on(g13>0)={}",
        onoff(x[8]),
        onoff(x[9]),
        onoff(x[10]),
        onoff(x[11]),
        onoff(x[12]),
        if x[13] > 0.5 { "ON " } else { "off" }
    );
    println!(
        "  open-world: forage_drr={} social_forage={} cultural={} lp_curiosity={} stigmergy={} affect_mod={}",
        onoff(x[14]),
        onoff(x[15]),
        onoff(x[16]),
        onoff(x[17]),
        onoff(x[18]),
        onoff(x[19])
    );
    println!(
        "  capability: fight={} build={} die={} grieve={} provision={} | nn_enabled={}",
        onoff(x[20]),
        onoff(x[21]),
        onoff(x[22]),
        onoff(x[23]),
        onoff(x[24]),
        onoff(x[25])
    );
}
