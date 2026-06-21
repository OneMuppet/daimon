//! Autogenesis — the self-learning, self-iterating improvement pipeline.
//!
//! Run with:  `cargo run -p daimon-game --example autogenesis --release`
//!
//! The loop makes the believability harness its own fitness function: it varies
//! the cognitive genome, evaluates each variant by living real lives in the real
//! world, keeps what wins, *learns which genes matter*, and halts on its own
//! evaluation — reaching a measurable target or honestly reporting a plateau.

use daimon_game::fitness::evaluate;
use daimon_mind::evolve::{Evolution, Genome, Verdict, N_GENES, WEIGHTS};
use daimon_mind::Fitness;

const GENE_NAMES: [&str; N_GENES] = [
    "surprise_thresh",
    "delib_cooldown",
    "tie_margin",
    "reflect_interval",
    "plan_staleness",
    "Δboldness",
    "Δsociability",
    "Δcuriosity",
    "empowerment",
    "consolidation",
    "imagination",
    "metamotivation",
    "quantum",
    "foresight",
    "forage_drr",
    "social_forage",
    "cultural",
    "lp_curiosity",
    "stigmergy",
    "affect_mod",
    "can_fight",
    "can_build",
    "can_die",
    "can_grieve",
    "can_provision",
    "nn_enabled",
    "nn_learn_rate",
    "nn_modulation",
    "herd_evasion",
    "can_mate",
    "can_reproduce",
    "can_age",
    "feel_happiness",
    "village_affinity",
];

fn show(label: &str, f: &Fitness) {
    println!(
        "  {label:<10}  scalar {:.3}  │ surv {:.2} safe {:.2} bal {:.2} expr {:.2} expl {:.2} emo {:.2} know {:.2}",
        f.scalar(),
        f.survival,
        f.safety,
        f.balance,
        f.expression,
        f.exploration,
        f.emotion,
        f.knowledge
    );
}

fn main() {
    // A real generation lives several 600-tick lives per genome.
    let seeds = [0xA1, 0xB2, 0xC3];
    let ticks = 600u64;
    let pop = 14usize;
    let max_gens = 24u32;

    let eval = |g: &Genome| evaluate(g, &seeds, ticks);

    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║  Daimon Autogenesis — self-improving believability                    ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝\n");
    println!("  end-goal target (every facet at once):");
    println!("    survival≥0.85 · safety≥0.80 · balance≥0.55 · expression≥0.55 · exploration≥0.45 · scalar≥0.72");
    println!(
        "  objective weights: surv {:.2} safe {:.2} bal {:.2} expr {:.2} expl {:.2}\n",
        WEIGHTS.survival, WEIGHTS.safety, WEIGHTS.balance, WEIGHTS.expression, WEIGHTS.exploration
    );

    let baseline = Genome::baseline();
    let base_fit = eval(&baseline);
    show("baseline", &base_fit);
    println!();

    // The engine's own self-evaluating, self-halting loop.
    let mut evo = Evolution::new(0x6E0_E515, pop, &eval);
    let verdict = evo.run(max_gens, &eval);

    // replay the trajectory the loop produced.
    for r in &evo.history {
        println!(
            "  gen {:>2}  best {:.3}  mean {:.3}  σ {:.3}",
            r.generation, r.best_scalar, r.mean_scalar, r.sigma
        );
    }

    println!("\n  ── result ──────────────────────────────────────────────────────────");
    show("champion", &evo.best_fit);
    show("baseline", &base_fit);
    let gain = evo.best_fit.scalar() - base_fit.scalar();
    println!("  improvement over hand-tuned baseline: {:+.3} scalar", gain);

    // HELD-OUT VALIDATION: the loop optimised on `seeds`; re-score the champion on
    // fresh, unseen seeds to prove the result generalises and isn't seed-overfit.
    let holdout = [0xD4u64, 0xE5, 0xF6, 0x17, 0x28];
    let val = evaluate(&evo.best, &holdout, ticks);
    show("champion@holdout", &val);
    println!(
        "  held-out target met: {} (survival {:.2}, scalar {:.2} on {} unseen seeds)",
        if val.meets_target() { "YES — generalises" } else { "no" },
        val.survival,
        val.scalar(),
        holdout.len()
    );
    println!(
        "  verdict: {}",
        match verdict {
            Verdict::ReachedTarget => "END-GOAL TARGET REACHED — every facet cleared the bar.",
            Verdict::Converged => "CONVERGED below target — this design's honest ceiling (plateau).",
            Verdict::Budget => "BUDGET SPENT while still improving — more generations would help.",
        }
    );

    // what the loop LEARNED: which genes move believability, and where it landed.
    println!("\n  learned gene sensitivities (what moves believability) & champion value:");
    let mut idx: Vec<usize> = (0..N_GENES).collect();
    idx.sort_by(|&a, &b| evo.gain[b].total_cmp(&evo.gain[a]));
    for &i in &idx {
        let bar = "█".repeat((evo.gain[i] * 20.0).round() as usize);
        println!("    {:<16} {:>5.2}  {:<20}  gene={:.2}", GENE_NAMES[i], evo.gain[i], bar, evo.best.g[i]);
    }
    println!();
}
