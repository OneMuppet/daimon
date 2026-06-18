//! **POET — the honest experiment.** Does an open-ended co-evolved curriculum +
//! transfer reach a capability on a HARD target environment that DIRECT optimisation
//! on that same target — at the *same evaluation budget* — cannot?
//!
//! Both arms are stopped at the identical total budget `B`, where one evaluation =
//! one genome simulated on one environment for `EVAL_TICKS` ticks (the unit both
//! arms count). We then score each arm's best agent on the hard target with a
//! final, equal-for-both probe and report the numbers + the curriculum trace.
//!
//! A clean negative result is a valid outcome — if POET does not beat direct search
//! we say so and hypothesise why.
//!
//!   cargo run -p daimon-game --example poet --release

use daimon_game::poet::{
    seed_agent, survival_fitness, EnvParams, Poet, PoetConfig, EVAL_TICKS,
};
use daimon_mind::Genome;
use daimon_core::Rng;

/// Total evaluation budget shared by BOTH arms (genome×env runs of EVAL_TICKS each).
/// ≈13ms/genome × EVAL_TICKS/600 ≈ ~150ms/eval → 1200 evals ≈ a few minutes/arm.
const BUDGET: u64 = 1200;

fn main() {
    let seed = 0xDA13_00E7_F00D_1234u64;
    let target = EnvParams::hard_target();
    println!("\n=== POET — honest experiment ===");
    println!("eval = one genome on one env for {EVAL_TICKS} ticks");
    println!("shared budget B = {BUDGET} evaluations per arm (fair comparison)");
    println!(
        "hard target knobs {:?} = {:?}  (difficulty {:.2})\n",
        EnvParams::KNOBS,
        target.k.map(|v| (v * 100.0).round() / 100.0),
        target.difficulty()
    );

    // ---- CONTROL: direct (1+λ)/μ search straight on the hard target ----
    let (ctrl_best_score, ctrl_evals) = direct_control(seed, &target, BUDGET);
    println!("--- CONTROL (direct search on the hard target) ---");
    println!("budget spent: {ctrl_evals} evals");
    println!("best survival-fitness on hard target: {ctrl_best_score:.4}\n");

    // ---- POET: co-evolve the curriculum to the same budget, then probe target ----
    let cfg = PoetConfig::default();
    let mut poet = Poet::new(seed, cfg, 2);
    while poet.evals < BUDGET {
        let before = poet.evals;
        let row = poet.step();
        // stop as soon as we'd exceed B (the last step may overshoot slightly; we
        // report the actual count so the comparison stays honest).
        if before >= BUDGET {
            break;
        }
        if row.iter.is_multiple_of(2) || poet.evals >= BUDGET {
            print_trace_row(&row);
        }
    }
    let poet_loop_evals = poet.evals;
    // final probe: POET's best agent on the hard target. These probes ALSO cost
    // budget; we report them separately so the headline B is the loop budget and
    // the probe is an equal, post-hoc measurement applied to BOTH arms identically
    // (the control's best was likewise its single best-on-target score).
    let (_poet_agent, poet_target_score) = poet.best_on(&target);
    let probe_evals = poet.evals - poet_loop_evals;

    println!("\n--- POET (open-ended co-evolution) ---");
    println!("loop budget spent: {poet_loop_evals} evals  (+{probe_evals} final target-probe evals)");
    println!("active envs at end: {}", poet.active.len());
    println!("hardest active env difficulty:  {:.3}", poet.trace.last().map(|r| r.hardest_active).unwrap_or(0.0));
    println!("hardest SOLVED env difficulty:  {:.3}", poet.trace.last().map(|r| r.hardest_solved).unwrap_or(0.0));
    println!("POET best agent survival-fitness on hard target: {poet_target_score:.4}\n");

    // ---- curriculum summary ----
    println!("--- curriculum trace (final active environments) ---");
    let mut envs: Vec<_> = poet.active.iter().collect();
    envs.sort_by(|a, b| a.env.difficulty().total_cmp(&b.env.difficulty()));
    for p in &envs {
        println!(
            "  diff {:.2}  score {:.3}  knobs {:?}",
            p.env.difficulty(),
            p.score,
            p.env.k.map(|v| (v * 100.0).round() / 100.0)
        );
    }

    // ---- VERDICT ----
    println!("\n=== VERDICT (equal budget B = {BUDGET}) ===");
    println!("direct control on hard target : {ctrl_best_score:.4}");
    println!("POET best on hard target      : {poet_target_score:.4}");
    let delta = poet_target_score - ctrl_best_score;
    if delta > 0.01 {
        println!(
            "POET BEATS direct optimisation by {:+.4} ({:.1}% relative). Open-ended\n\
             co-evolution reached a hard-target capability direct search did not, at\n\
             equal budget.",
            delta,
            100.0 * delta / ctrl_best_score.max(1e-6)
        );
    } else if delta.abs() <= 0.01 {
        println!(
            "TIE ({delta:+.4}). No clear advantage either way at this budget — see the\n\
             hypotheses below."
        );
    } else {
        println!(
            "POET DID NOT BEAT direct optimisation ({delta:+.4}). A clean negative result.\n\
             Likely causes to investigate: (1) the env encoding may be too coarse, so the\n\
             curriculum's stepping stones don't transfer to the *specific* hard target;\n\
             (2) the MC band [{:.2},{:.2}] may admit envs off the path to the target;\n\
             (3) transfer every {} iters may be too rare to move stepping stones; (4) B may\n\
             be too small for the curriculum to reach the target's difficulty (hardest\n\
             solved {:.2} vs target {:.2}).",
            cfg.mc_low,
            cfg.mc_high,
            cfg.transfer_every,
            poet.trace.last().map(|r| r.hardest_solved).unwrap_or(0.0),
            target.difficulty(),
        );
    }
}

/// Direct control: a (1+λ) hill-climb (μ=1 elite, λ children/round) straight on the
/// hard target, counting every world-run against the same budget B. This is the
/// fairest direct analogue of POET's inner ES — same fitness, same eval unit — just
/// with NO curriculum and NO transfer. Returns (best score on target, evals spent).
fn direct_control(seed: u64, target: &EnvParams, budget: u64) -> (f32, u64) {
    let mut rng = Rng::new(seed ^ 0xC02_7401);
    let gain = [1.0f32; daimon_mind::evolve::N_GENES];
    let lambda = 8usize;
    let sigma = 0.10f32;
    let mut evals: u64 = 0;
    let probe = |g: &Genome, e: &mut u64| {
        *e += 1;
        // a fixed target world seed so the control optimises a stationary landscape
        // (exactly the discipline evolve_mode uses across generations).
        survival_fitness(g, target, seed ^ 0x7A_4267)
    };
    let mut champ = seed_agent();
    let mut champ_score = probe(&champ, &mut evals);
    while evals < budget {
        let mut best = champ.clone();
        let mut best_score = champ_score;
        for _ in 0..lambda {
            if evals >= budget {
                break;
            }
            let child = champ.mutate(sigma, &gain, &mut rng);
            let s = probe(&child, &mut evals);
            if s > best_score {
                best_score = s;
                best = child;
            }
        }
        champ = best;
        champ_score = best_score;
    }
    (champ_score, evals)
}

fn print_trace_row(r: &daimon_game::poet::TraceRow) {
    println!(
        "iter {:>3}  evals {:>5}  active {:>2}  hardest_solved {:.2}  hardest {:.2}  \
         cap {:.3}  +{} -{}mc  xfer {}",
        r.iter,
        r.evals,
        r.n_active,
        r.hardest_solved,
        r.hardest_active,
        r.mean_capability,
        r.admitted,
        r.rejected_mc,
        r.transfers,
    );
}
