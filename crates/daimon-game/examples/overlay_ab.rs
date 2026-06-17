//! Thesis A/B: does the System-2 **learned overlay** beat pure instinct?
//!
//! Identical showcase genome in both arms; the only difference is the overlay
//! genes (g25 enabled, g26 learn-rate, g27 modulation). We live many lives in the
//! harsh world — where survival is the binding constraint — and compare. This is
//! the honest test of "neural net + lifetime plasticity": if the overlay does not
//! help, the numbers say so.
//!
//!   cargo run -p daimon-game --example overlay_ab --release

use daimon_game::fitness::evaluate_harsh;
use daimon_mind::Genome;

fn main() {
    let seeds: Vec<u64> = (0..24u64).map(|i| 0xA1_0000 ^ (i.wrapping_mul(0x9E3779B1))).collect();
    let ticks = 800;

    let instinct = Genome::showcase();
    let mut overlay = Genome::showcase();
    overlay.g[25] = 1.0; // nn_enabled
    overlay.g[26] = 0.6; // learn-rate ≈ 0.09
    overlay.g[27] = 0.6; // modulation ≈ 0.36

    println!("Overlay A/B — instinct vs learned overlay · {} seeds × {} ticks (harsh world)\n", seeds.len(), ticks);

    let fi = evaluate_harsh(&instinct, &seeds, ticks);
    let fo = evaluate_harsh(&overlay, &seeds, ticks);

    let row = |name: &str, f: &daimon_mind::Fitness| {
        println!(
            "  {name:9} scalar {:.3} · survival {:.3} · safety {:.3} · balance {:.3} · explore {:.3} · knowledge {:.3}",
            f.scalar(), f.survival, f.safety, f.balance, f.exploration, f.knowledge
        );
    };
    row("instinct", &fi);
    row("overlay", &fo);

    let d_scalar = fo.scalar() - fi.scalar();
    let d_surv = fo.survival - fi.survival;
    println!("\n  Δscalar {d_scalar:+.3} · Δsurvival {d_surv:+.3}");
    let verdict = if d_scalar > 0.01 {
        "OVERLAY HELPS"
    } else if d_scalar < -0.01 {
        "OVERLAY HURTS"
    } else {
        "NO SIGNIFICANT DIFFERENCE (within noise)"
    };
    println!("  → {verdict}");
    println!("\n  (Honest read: the mechanism is real and harness-safe; whether it *wins* at this");
    println!("   scale/horizon is exactly what these numbers report — a null result is a finding.)");
}
