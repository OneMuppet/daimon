//! Run the autogenesis loop and export a full training trace to JSON, for the
//! training visualisation in `viz/`.
//!
//! Run: `cargo run -p daimon-game --example autogenesis_trace --release`
//! Writes: `viz/training_data.json`

use std::fs;

use daimon_game::fitness::evaluate;
use daimon_mind::evolve::{Evolution, Fitness, Genome, Verdict, WEIGHTS, N_GENES};

const GENE_NAMES: [&str; N_GENES] = [
    "surprise_thresh", "delib_cooldown", "tie_margin", "reflect_interval", "plan_staleness",
    "Δboldness", "Δsociability", "Δcuriosity", "empowerment", "consolidation", "imagination",
    "metamotivation", "quantum", "foresight", "forage_drr", "social_forage", "cultural", "lp_curiosity", "stigmergy", "affect_mod", "can_fight", "can_build", "can_die", "can_grieve", "can_provision", "nn_enabled", "nn_learn_rate", "nn_modulation", "herd_evasion", "can_mate", "can_reproduce", "can_age", "feel_happiness", "village_affinity", "can_war",
];

fn facets(f: &Fitness) -> String {
    format!(
        r#"{{"survival":{:.4},"safety":{:.4},"balance":{:.4},"expression":{:.4},"exploration":{:.4},"emotion":{:.4},"knowledge":{:.4},"scalar":{:.4}}}"#,
        f.survival, f.safety, f.balance, f.expression, f.exploration, f.emotion, f.knowledge, f.scalar()
    )
}

fn farr(v: &[f32]) -> String {
    let parts: Vec<String> = v.iter().map(|x| format!("{x:.4}")).collect();
    format!("[{}]", parts.join(","))
}

fn main() {
    let seeds = [0xA1u64, 0xB2, 0xC3];
    let ticks = 600u64;
    let eval = |g: &Genome| evaluate(g, &seeds, ticks);
    let baseline = eval(&Genome::baseline());

    let mut evo = Evolution::new(0x6E0_E515, 14, &eval);
    let max_gens = 24u32;

    let mut gens: Vec<String> = Vec::new();
    let mut verdict = Verdict::Budget;
    // record gen 0 (initial population, before any step)
    let record = |evo: &Evolution, gen: u32| -> String {
        let pop: Vec<String> =
            evo.fitnesses().iter().map(|f| format!("{:.4}", f.scalar())).collect();
        let mean = evo.fitnesses().iter().map(|f| f.scalar()).sum::<f32>()
            / evo.fitnesses().len().max(1) as f32;
        format!(
            r#"{{"gen":{gen},"best":{},"mean":{:.4},"pop":[{}],"gain":{},"genes":{}}}"#,
            facets(&evo.best_fit),
            mean,
            pop.join(","),
            farr(&evo.gain),
            farr(&evo.best.g),
        )
    };
    gens.push(record(&evo, 0));
    for gen in 1..=max_gens {
        if evo.best_fit.meets_target() {
            verdict = Verdict::ReachedTarget;
            break;
        }
        evo.step(&eval);
        gens.push(record(&evo, gen));
        // mirror the engine's plateau halting for the trace (patience 4).
        if let Some(last) = evo.history.last() {
            if last.generation >= 5 {
                let h = &evo.history;
                let prior = h[h.len().saturating_sub(5)].best_scalar;
                if last.best_scalar <= prior + 1e-4 {
                    verdict = Verdict::Converged;
                    break;
                }
            }
        }
    }
    if evo.best_fit.meets_target() {
        verdict = Verdict::ReachedTarget;
    }

    // held-out validation on unseen seeds.
    let holdout = evaluate(&evo.best, &[0xD4u64, 0xE5, 0xF6, 0x17, 0x28], ticks);
    let vname = match verdict {
        Verdict::ReachedTarget => "ReachedTarget",
        Verdict::Converged => "Converged",
        Verdict::Budget => "Budget",
    };

    // the cross-disciplinary mechanism stack (legend) and the outer-loop journey.
    let mechanisms = r#"[
      {"name":"Praxis","field":"cognitive science","note":"invents its own concepts, affordances, goals"},
      {"name":"Empowerment","field":"information theory","note":"intrinsic drive toward future control"},
      {"name":"Imagination","field":"planning / RL","note":"plans over a learned forward model"},
      {"name":"Associative memory","field":"neuroscience (ACT-R, Hebb)","note":"links, activation, replay"},
      {"name":"Quantum cognition","field":"quantum probability","note":"order effects + interference"},
      {"name":"Neural criticality","field":"statistical physics","note":"self-tunes to the edge of chaos"},
      {"name":"Conceptual entanglement","field":"quantum foundations (Bell/CHSH)","note":"non-separable concept pairs, S=2√2"},
      {"name":"Learning progress","field":"developmental robotics (Oudeyer)","note":"competence gain as intrinsic reward"},
      {"name":"Cultural transmission","field":"cultural evolution (Cook 2024)","note":"learn affordances from peers; false memes filtered"},
      {"name":"Stigmergy","field":"swarm intelligence (Dorigo ACO)","note":"self-organised routes via environmental traces"},
      {"name":"Autogenesis","field":"evolutionary computation","note":"the loop that trains all of the above"}
    ]"#;
    let journey = r#"[
      {"turn":1,"mechanism":"Anticipatory homeostasis","effect":"positive","note":"survival 0.48→0.70 — forage ahead of crisis (active-inference-lite)"},
      {"turn":2,"mechanism":"DRR foraging","effect":"negative","note":"literature-grounded forager did NOT transfer — bottleneck is not which resource you pick"},
      {"turn":3,"mechanism":"Commons coordination","effect":"negative→positive","note":"dispersion hurt under undersupply → forced the diagnosis"},
      {"turn":4,"mechanism":"Structural diagnosis + fair world","effect":"breakthrough","note":"6 agents starved on 4 wells (supply<demand); a village needs enough wells — then commons flips to helpful and the loop reaches the goal"}
    ]"#;

    let gene_names_json =
        format!("[{}]", GENE_NAMES.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(","));
    let json = format!(
        r#"{{
  "target": {{"survival":0.85,"safety":0.80,"balance":0.55,"expression":0.55,"exploration":0.45,"emotion":0.45,"knowledge":0.45,"scalar":0.72}},
  "weights": {{"survival":{:.2},"safety":{:.2},"balance":{:.2},"expression":{:.2},"exploration":{:.2},"emotion":{:.2},"knowledge":{:.2}}},
  "geneNames": {},
  "baseline": {},
  "generations": [
    {}
  ],
  "verdict": "{}",
  "champion": {{"facets":{},"genes":{}}},
  "holdout": {{"facets":{},"met":{}}},
  "mechanisms": {},
  "journey": {}
}}
"#,
        WEIGHTS.survival, WEIGHTS.safety, WEIGHTS.balance, WEIGHTS.expression, WEIGHTS.exploration, WEIGHTS.emotion, WEIGHTS.knowledge,
        gene_names_json,
        facets(&baseline),
        gens.join(",\n    "),
        vname,
        facets(&evo.best_fit),
        farr(&evo.best.g),
        facets(&holdout),
        holdout.meets_target(),
        mechanisms,
        journey,
    );

    fs::create_dir_all("viz").expect("create viz dir");
    fs::write("viz/training_data.json", &json).expect("write trace");
    println!(
        "wrote viz/training_data.json — {} generations, verdict {vname}, held-out met {}",
        gens.len(),
        holdout.meets_target()
    );
}
