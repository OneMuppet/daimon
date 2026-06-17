//! Headless smoke test for the live generational evolution mode (the windowed
//! `--evolve` path needs a display; this runs the *same* `Evolution` driver with
//! no GPU). Reports: per-tick speed at the requested population, wall-clock to
//! cross one generation, survivors at the boundary, and confirms the generation
//! counter advanced and the population refilled to `pop`.
//!
//! Usage: `cargo run -p daimon-game --example evolve_headless --release [-- POP GENS]`

use std::time::Instant;

use daimon_game::evolve_mode::{Evolution, GEN_TICKS};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let pop: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1000);
    let gens: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(2);

    println!("evolve_headless: pop={pop}, target generations={gens}, GEN_TICKS={GEN_TICKS}");
    let mut ev = Evolution::new(0xE001, pop);
    println!(
        "island dims {:?}  ·  initial alive {} / {}",
        ev.dims,
        ev.alive(),
        ev.pop
    );

    // warm-up speed sample: 200 ticks.
    let warm = Instant::now();
    for _ in 0..200 {
        ev.tick();
    }
    let per_tick_ms = warm.elapsed().as_secs_f64() * 1000.0 / 200.0;
    println!(
        "speed: {:.3} ms/tick  ·  {:.0} ticks/s  ·  alive after 200 ticks: {}",
        per_tick_ms,
        1000.0 / per_tick_ms,
        ev.alive()
    );

    let start_gen = ev.generation;
    let mut crossed = 0u32;
    let mut last_gen_wall = None;
    let mut t_gen = Instant::now();

    // Per-generation table: the survivor count and best/mean fitness tell the
    // whole story — if the elite is breeding a gradient, best & elite-mean climb.
    println!(
        "\n gen | survivors |  elite |        best |   elite-mean |    pop-mean | elite forage genes [g13 g14 g15] | drive"
    );
    println!(
        "-----+-----------+--------+-------------+--------------+-------------+----------------------------------+------"
    );

    // collect for a trend verdict at the end.
    let mut best_series: Vec<f64> = Vec::new();
    let mut elite_mean_series: Vec<f64> = Vec::new();

    // run until we've crossed `gens` generation boundaries.
    let overall = Instant::now();
    let mut ticks = 0u64;
    while crossed < gens {
        let boundary = ev.tick();
        ticks += 1;
        if boundary {
            crossed += 1;
            let wall = t_gen.elapsed().as_secs_f64();
            last_gen_wall = Some(wall);
            let st = ev.last.expect("gen stats after boundary");
            // The generation that just *finished* is `ev.generation - 1` (the
            // counter has already advanced for the new one).
            let finished = ev.generation - 1;
            best_series.push(st.best_fitness);
            elite_mean_series.push(st.elite_mean);
            let fg = st.elite_forage_genes;
            println!(
                " {:>3} | {:>9} | {:>6} | {:>11.0} | {:>12.0} | {:>11.0} |        [{:.2} {:.2} {:.2}]          | {:?}  ({:.1}s)",
                finished,
                st.survivors_end,
                st.elite_n,
                st.best_fitness,
                st.elite_mean,
                st.mean_fitness,
                fg[0], fg[1], fg[2],
                st.elite_dominant,
                wall,
            );
            // assert refill happened.
            assert_eq!(ev.alive(), ev.pop, "population should refill to pop after a generation");
            t_gen = Instant::now();
        }
    }

    println!(
        "\nDONE: {} generation(s) crossed (counter {} -> {}), {} ticks in {:.2}s total",
        crossed,
        start_gen,
        ev.generation,
        ticks,
        overall.elapsed().as_secs_f64(),
    );
    println!(
        "last-generation wall-clock: {:.2}s  ·  population refilled to {}",
        last_gen_wall.unwrap_or(0.0),
        ev.alive()
    );

    // Trend verdict: compare the mean of the first third of generations to the last
    // third for both best and elite-mean fitness. A clear rise = the minds evolved.
    if best_series.len() >= 3 {
        let k = (best_series.len() / 3).max(1);
        let avg = |s: &[f64]| s.iter().sum::<f64>() / s.len() as f64;
        let best_early = avg(&best_series[..k]);
        let best_late = avg(&best_series[best_series.len() - k..]);
        let em_early = avg(&elite_mean_series[..k]);
        let em_late = avg(&elite_mean_series[elite_mean_series.len() - k..]);
        let pct = |a: f64, b: f64| if a.abs() > 1e-9 { (b - a) / a * 100.0 } else { 0.0 };
        println!(
            "\nTREND (first {k} gens vs last {k} gens):\n  best fitness : {:.0} -> {:.0}  ({:+.1}%)\n  elite-mean   : {:.0} -> {:.0}  ({:+.1}%)",
            best_early,
            best_late,
            pct(best_early, best_late),
            em_early,
            em_late,
            pct(em_early, em_late),
        );
        let climbs = best_late > best_early * 1.02 || em_late > em_early * 1.02;
        if climbs {
            println!("VERDICT: fitness CLIMBS — the minds are evolving (best and/or elite-mean rose).");
        } else {
            println!("VERDICT: fitness is FLAT — no clear improvement over the run.");
        }
    }

    assert!(ev.generation >= start_gen + gens, "generation counter must advance");
    assert_eq!(ev.alive(), ev.pop, "population refilled to pop");
    println!("[PASS] generations advanced and population refilled");
}
