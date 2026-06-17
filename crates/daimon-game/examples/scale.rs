//! How does the cognitive engine scale with population? We run the *real* step()
//! loop at growing agent counts and measure wall-clock cost per tick, so the
//! "can we run 1,000 minds?" question is answered with numbers, not vibes.
//!
//!   cargo run -p daimon-game --example scale --release           # constant-density island
//!   cargo run -p daimon-game --example scale --release -- dense   # fixed 40×26 (crowded) for contrast
//!
//! The island grows with the population (grid ∝ √N) so DENSITY is held constant
//! at the 6-agent baseline — each mind sees only a few local neighbours. This
//! isolates *population* scaling from *crowding*: if cost is still super-linear
//! here, it's the all-pairs neighbour SCAN, not the island being full.

use std::time::Instant;

use daimon_game::sim::GameWorld;
use daimon_mind::evolve::Genome;

fn main() {
    let dense = std::env::args().any(|a| a == "dense");
    // baseline density: 6 agents on a 40×26 = 1040-cell island.
    let base_cells_per_agent = (40.0 * 26.0) / 6.0;
    println!(
        "\n  DAIMON — population scaling (real cognitive loop, --release) — {}\n",
        if dense { "DENSE fixed 40×26 grid" } else { "constant-density island (grid ∝ √N)" }
    );
    println!(
        "  {:>5}  {:>9}  {:>10}  {:>14}  {:>12}  {:>12}",
        "N", "grid", "ticks/s", "agent-ticks/s", "ms/tick", "µs/agent·tk"
    );
    println!("  {}", "-".repeat(74));

    let genome = Genome::showcase();
    let mut prev: Option<(usize, f64)> = None; // (N, ms/tick) for the exponent
    for &n in &[6usize, 25, 50, 100, 250, 500, 1000] {
        // size the island to hold density constant (unless --dense).
        let (gw, gh) = if dense {
            (40, 26)
        } else {
            let side = ((n as f64 * base_cells_per_agent).sqrt()).round() as i32;
            // keep the 40:26 aspect roughly; square is fine for a benchmark.
            (side.max(40), ((side as f64 * 26.0 / 40.0).round() as i32).max(26))
        };
        let mut world = GameWorld::with_genome_sized(0xDA13, n, &genome, gw, gh, 7);
        let _ = &mut world;

        // warm up so caches/branch predictors settle, then time a bounded run
        for _ in 0..3 {
            world.step();
        }
        let budget_ticks = (2_000_000 / n).clamp(8, 400); // keep each row ~bounded
        let t = Instant::now();
        let mut done = 0u32;
        while done < budget_ticks as u32 {
            world.step();
            done += 1;
            if t.elapsed().as_secs_f64() > 4.0 {
                break; // cap wall-clock per row
            }
        }
        let secs = t.elapsed().as_secs_f64();
        let tps = done as f64 / secs;
        let atps = tps * n as f64;
        let ms_tick = 1e3 / tps;
        let us_agent = 1e6 / atps;

        println!(
            "  {:>5}  {:>9}  {:>10.0}  {:>14.0}  {:>12.3}  {:>12.2}",
            n, format!("{gw}×{gh}"), tps, atps, ms_tick, us_agent
        );

        if let Some((pn, pms)) = prev {
            // local scaling exponent: ms/tick ∝ N^k  ⇒  k = log(ratio)/log(N-ratio)
            let k = (ms_tick / pms).ln() / (n as f64 / pn as f64).ln();
            eprintln!("        ↳ from {pn}→{n}: cost ×{:.1}, exponent k≈{:.2} (1.0=linear, 2.0=quadratic)", ms_tick / pms, k);
        }
        prev = Some((n, ms_tick));
    }

    println!("\n  Real-time feasibility (one core): an agent that 'thinks' a few times a");
    println!("  second needs ~3–5 cognitive ticks/agent/s. 1,000 minds at 4 tk/s = 4,000");
    println!("  agent-ticks/s required — compare the 1,000-row agent-ticks/s above.\n");
}
