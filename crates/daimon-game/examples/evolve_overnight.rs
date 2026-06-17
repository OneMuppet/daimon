//! Overnight evolution runner — churns through INDEPENDENT evolutionary searches
//! over the cognitive genome, each to convergence, appending one JSON record per
//! completed search to a log. A companion analysis loop harvests the log to learn
//! which mechanisms evolution robustly selects for.
//!
//!   cargo run -p daimon-game --example evolve_overnight --release
//!   -> appends to /tmp/daimon_evolution.jsonl  (one line per completed search)
//!
//! Each search: seed a population, run the elitist EA (1/5th-rule self-adaptive
//! mutation, learned per-gene sensitivity) against the real believability fitness
//! to its honest Verdict, then validate the champion on held-out seeds. CPU-only.

use std::fs::OpenOptions;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use daimon_game::fitness::evaluate_harsh as evaluate;
use daimon_mind::evolve::{Evolution, Genome};

fn main() {
    let path = "/tmp/daimon_evolution.jsonl";
    let pop = 10;
    let max_gens = 24;
    let ticks = 600u64;
    let train = [0xA1u64, 0xB2, 0xC3, 0xD4];
    let holdout = [0xE5u64, 0xF6, 0x17];

    let base = evaluate(&Genome::baseline(), &train, ticks).scalar();
    eprintln!("[evolve_overnight] HARSH world · baseline scalar {base:.4}; writing {path}");

    let mut iter = 0u64;
    loop {
        iter += 1;
        let seed = 0x5EED_0000_0000u64 ^ iter.wrapping_mul(0x9E37_79B9);
        let eval = |g: &Genome| evaluate(g, &train, ticks);
        let mut evo = Evolution::new(seed, pop, &eval);
        let verdict = evo.run(max_gens, &eval);
        let champ = evo.best_fit.scalar();
        let hv = evaluate(&evo.best, &holdout, ticks).scalar();
        let f = &evo.best_fit;
        let g = &evo.best;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);

        let line = format!(
            concat!(
                "{{\"iter\":{},\"t\":{},\"seed\":{},\"verdict\":\"{:?}\",\"gens\":{},",
                "\"base\":{:.4},\"champ\":{:.4},\"heldout\":{:.4},",
                "\"facets\":{{\"survival\":{:.3},\"safety\":{:.3},\"balance\":{:.3},",
                "\"expression\":{:.3},\"exploration\":{:.3},\"emotion\":{:.3},\"knowledge\":{:.3}}},",
                "\"fac\":{{\"empowerment\":{},\"consolidation\":{},\"imagination\":{},",
                "\"metamotivation\":{},\"quantum\":{},\"forage_drr\":{},\"social_forage\":{},",
                "\"cultural\":{},\"lp_curiosity\":{},\"stigmergy\":{},\"affect_mod\":{},",
                "\"can_fight\":{},\"foresight\":{:.1}}}}}"
            ),
            iter, now, seed, verdict, evo.history.len(),
            base, champ, hv,
            f.survival, f.safety, f.balance, f.expression, f.exploration, f.emotion, f.knowledge,
            g.empowerment(), g.consolidation(), g.imagination(), g.metamotivation(), g.quantum(),
            g.forage_drr(), g.social_forage(), g.cultural(), g.lp_curiosity(), g.stigmergy(),
            g.affect_mod(), g.can_fight(), g.foresight(),
        );
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{line}");
            let _ = file.flush();
        }
        eprintln!(
            "[evolve_overnight] search {iter}: {:?} in {} gens · champ {:.4} (base {:.4}) · held-out {:.4}",
            verdict, evo.history.len(), champ, base, hv
        );
    }
}
