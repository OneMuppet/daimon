//! The honest arbiter: **let evolution choose whether to enable the overlay.**
//!
//! The System-2 learned overlay can be ablated for free — gene g25 off ⇒ the mind
//! is byte-identical to pure instinct, zero cost. So if the overlay helped, the
//! evolutionary search would *keep it on*; if it doesn't, the search turns it off.
//! We run many independent searches over the FULL 28-gene genome (the nn genes are
//! in the search space) and report the selection rate of `nn_enabled` among the
//! evolved champions — calibrated against `quantum` (a faculty we already know
//! evolution rejects) and a couple of roughly-neutral faculties measured the same
//! way. Evolution, not us, delivers the verdict.
//!
//!   cargo run -p daimon-game --example overlay_evolve --release

use daimon_game::fitness::evaluate_harsh;
use daimon_mind::evolve::Evolution;

fn main() {
    // the harsh world's training seeds + budget, matching the overnight search.
    let train: Vec<u64> = vec![0xA1, 0xB2, 0xC3, 0xD4];
    let ticks = 600;
    let eval = |g: &daimon_mind::Genome| evaluate_harsh(g, &train, ticks);

    let n_searches = 40u64;
    let pop = 10usize;
    let max_gens = 24u32;

    println!(
        "Let evolution choose — {n_searches} independent searches (pop {pop}, ≤{max_gens} gens, harsh world)\n"
    );

    // champion tallies
    let mut nn_on = 0u32; // g25 >= 0.5
    let mut quantum_on = 0u32; // g12 — known-rejected calibration
    let mut empower_on = 0u32; // g8 — roughly neutral calibration
    let mut imagine_on = 0u32; // g10 — roughly neutral calibration
    let mut sum_scalar = 0.0f32;
    let mut sum_mod_on = 0.0f32; // mean modulation among nn-ON champions
    let mut sum_gens = 0u32;

    for i in 0..n_searches {
        let seed = 0xE0_0000u64 ^ i.wrapping_mul(0x9E3779B1);
        let mut evo = Evolution::new(seed, pop, &eval);
        let _verdict = evo.run(max_gens, &eval);
        let g = &evo.best;
        if g.nn_enabled() {
            nn_on += 1;
            sum_mod_on += g.nn_modulation();
        }
        if g.quantum() {
            quantum_on += 1;
        }
        if g.empowerment() {
            empower_on += 1;
        }
        if g.imagination() {
            imagine_on += 1;
        }
        sum_scalar += evo.best_fit.scalar();
        sum_gens += evo.history.len() as u32;
    }

    let n = n_searches as f32;
    let pct = |c: u32| 100.0 * c as f32 / n;
    println!("Champion selection rates (of {n_searches}):");
    println!("  nn_enabled (overlay, prior OFF)    {:.0}%", pct(nn_on));
    println!("  quantum    (known-rejected, OFF)   {:.0}%   ← apples-to-apples: same OFF prior as the overlay", pct(quantum_on));
    println!("  empowerment (incumbent-ON ref)     {:.0}%   (soft upper-reference, not a 50% null)", pct(empower_on));
    println!("  imagination (incumbent-ON ref)     {:.0}%   (soft upper-reference, not a 50% null)", pct(imagine_on));
    println!("\n  mean champion scalar {:.3} · mean gens {:.1}", sum_scalar / n, sum_gens as f32 / n);
    if nn_on > 0 {
        println!("  mean modulation among nn-ON champions {:.2}", sum_mod_on / nn_on as f32);
    }

    // the verdict — relative to the 50% random null and the quantum calibration.
    let r = pct(nn_on);
    let q = pct(quantum_on);
    println!("\nVERDICT:");
    if r <= q + 5.0 {
        println!("  Evolution REJECTS the overlay — it sits at/with quantum ({r:.0}% vs {q:.0}%),");
        println!("  the faculty we already know is selected against. Learning does not pay here.");
    } else if r < 45.0 {
        println!("  Evolution leans AGAINST the overlay ({r:.0}% < 50% null) — mild negative pressure.");
    } else if r <= 55.0 {
        println!("  NEUTRAL ({r:.0}% ≈ 50% null): selection too weak to express a preference —");
        println!("  the overlay neither helps nor clearly hurts in this regime.");
    } else {
        println!("  Evolution FAVOURS the overlay ({r:.0}% > 50% null) — learning earns its keep here.");
    }
    println!("\n  (Honest: this is the harsh world, where instinct is already well-tuned — so a");
    println!("   rejection here is expected and consistent with the A/B. The open question is");
    println!("   whether a regime instinct CAN'T pre-solve would flip the vote.)");
}
