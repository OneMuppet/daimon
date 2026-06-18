//! Calibration probe for frontier evolution: where is the survivable frontier?
//! Measures survival-rate (fraction of minds alive at end) for a COMPETENT genome
//! (showcase + open-world genes) vs a WEAK random one, across difficulty D and a few
//! eval lengths. Tells us the D band + ratchet thresholds that leave real headroom.
//!
//!   cargo run -p daimon-game --example frontier_calib --release

use daimon_core::Rng;
use daimon_game::poet::EnvParams;
use daimon_game::sim::GameWorld;
use daimon_mind::Genome;

const MINDS: usize = 6;
const K: usize = 5;

fn comp() -> Genome {
    let mut g = Genome::showcase();
    g.g[20] = 1.0;
    g.g[21] = 1.0;
    g.g[22] = 1.0; // mortal
    g.g[24] = 1.0; // provision
    g
}

fn weak(rng: &mut Rng) -> Genome {
    let mut g = Genome::random(rng);
    g.g[20] = 1.0;
    g.g[21] = 1.0;
    g.g[22] = 1.0;
    g.g[24] = 1.0;
    g.g[25] = 0.0;
    g
}

/// Returns (end-alive fraction, mean graded survival = mean alive_ticks/ticks).
fn surv(g: &Genome, d: f32, ticks: u64) -> (f32, f32) {
    let env = EnvParams::at_difficulty(d);
    let mut end_acc = 0.0f64;
    let mut graded_acc = 0.0f64;
    for s in 0..K {
        let genomes: Vec<Genome> = (0..MINDS).map(|_| g.clone()).collect();
        let mut w = env.build_world(0xABC0 + s as u64 * 7, &genomes);
        let n = w.agents.len().max(1);
        let mut at = vec![0u64; n];
        for _ in 0..ticks {
            w.step();
            for (i, a) in w.agents.iter().enumerate() {
                if a.alive {
                    at[i] += 1;
                }
            }
        }
        end_acc += w.living_count() as f64 / MINDS as f64;
        graded_acc += at.iter().map(|&t| t as f64 / ticks as f64).sum::<f64>() / n as f64;
    }
    ((end_acc / K as f64) as f32, (graded_acc / K as f64) as f32)
}

/// A directly-built GENTLE open world to test whether a benign regime separates
/// competent from weak. Cold/metabolism scaled by `d`; stalker softened; resources
/// ample. This mirrors what at_difficulty SHOULD produce at the easy end.
fn gentle_build(d: f32, seed: u64, genomes: &[Genome]) -> GameWorld {
    let pop = genomes.len().max(1);
    let area = (pop as f32) * 55.0;
    let aspect = 40.0 / 26.0;
    let h = ((area / aspect).sqrt().round()).max(26.0) as i32;
    let w = ((h as f32) * aspect).round().max(40.0) as i32;
    let mut world = GameWorld::with_genomes_sized_harsh(seed, genomes, w, h, 9);
    world.lethal_starvation = true;
    world.set_open_world(true);
    // gentle base, hardened by d:
    world.metabolism_scale = 0.45 + d * 0.40; // 0.45 .. 0.85
    world.open_world_cold_scale = 0.25 + d * 1.5; // 0.25 .. 1.75
    world.set_stalker(0.4 + d * 0.6, if d > 0.5 { 1 } else { 2 });
    let want_food = ((pop as f32) * (1.3 - d * 0.9)).round().max(1.0) as usize;
    let want_water = ((pop as f32) * (1.1 - d * 0.8)).round().max(1.0) as usize;
    world.set_resource_counts(want_food, want_water);
    world
}

fn surv_gentle(g: &Genome, d: f32, ticks: u64) -> (f32, f32) {
    let mut end_acc = 0.0f64;
    let mut graded_acc = 0.0f64;
    for s in 0..K {
        let genomes: Vec<Genome> = (0..MINDS).map(|_| g.clone()).collect();
        let mut w = gentle_build(d, 0xABC0 + s as u64 * 7, &genomes);
        let n = w.agents.len().max(1);
        let mut at = vec![0u64; n];
        for _ in 0..ticks {
            w.step();
            for (i, a) in w.agents.iter().enumerate() {
                if a.alive {
                    at[i] += 1;
                }
            }
        }
        end_acc += w.living_count() as f64 / MINDS as f64;
        graded_acc += at.iter().map(|&t| t as f64 / ticks as f64).sum::<f64>() / n as f64;
    }
    ((end_acc / K as f64) as f32, (graded_acc / K as f64) as f32)
}

fn main() {
    let mut rng = Rng::new(0x99);
    let c = comp();
    let weaks: Vec<Genome> = (0..8).map(|_| weak(&mut rng)).collect();

    println!("\n@@@@@@@@@@ SHORT-HORIZON poet at_difficulty (foraging-decided, winter starts ~3750) @@@@@@@@@@");
    for &ticks in &[1200u64, 2000, 3000] {
        println!("=== eval_ticks = {ticks} ===");
        println!("  D    comp_end  comp_graded   weak_end  weak_graded");
        for &d in &[0.0f32, 0.1, 0.2, 0.3, 0.5, 0.7] {
            let (ce, cg) = surv(&c, d, ticks);
            let mut we = 0.0;
            let mut wg = 0.0;
            for g in &weaks {
                let (e, gr) = surv(g, d, ticks);
                we += e;
                wg += gr;
            }
            we /= 8.0;
            wg /= 8.0;
            println!(
                "  {d:>4.2}  {:>6.0}%   {:>7.0}%    {:>6.0}%   {:>7.0}%",
                ce * 100.0,
                cg * 100.0,
                we * 100.0,
                wg * 100.0
            );
        }
    }

    println!("\n########## GENTLE custom build (cold 0.25.., metab 0.45.., soft stalker, ample food) ##########");
    for &ticks in &[5000u64] {
        println!("=== eval_ticks = {ticks} ===");
        println!("  D    comp_end  comp_graded   weak_end  weak_graded");
        for &d in &[0.0f32, 0.1, 0.2, 0.3, 0.5, 0.7, 1.0] {
            let (ce, cg) = surv_gentle(&c, d, ticks);
            let mut we = 0.0;
            let mut wg = 0.0;
            for g in &weaks {
                let (e, gr) = surv_gentle(g, d, ticks);
                we += e;
                wg += gr;
            }
            we /= 8.0;
            wg /= 8.0;
            println!(
                "  {d:>4.2}  {:>6.0}%   {:>7.0}%    {:>6.0}%   {:>7.0}%",
                ce * 100.0,
                cg * 100.0,
                we * 100.0,
                wg * 100.0
            );
        }
    }
    println!("\n########## poet at_difficulty build (for contrast) ##########");

    for &ticks in &[5000u64, 6000] {
        println!("\n=== eval_ticks = {ticks} (year=5000) ===");
        println!("  D    comp_end  comp_graded   weak_end  weak_graded");
        for &d in &[0.0f32, 0.05, 0.1, 0.15, 0.2, 0.3, 0.4, 0.6] {
            let (ce, cg) = surv(&c, d, ticks);
            let mut we = 0.0;
            let mut wg = 0.0;
            for g in &weaks {
                let (e, gr) = surv(g, d, ticks);
                we += e;
                wg += gr;
            }
            we /= 8.0;
            wg /= 8.0;
            println!(
                "  {d:>4.2}  {:>6.0}%   {:>7.0}%    {:>6.0}%   {:>7.0}%",
                ce * 100.0,
                cg * 100.0,
                we * 100.0,
                wg * 100.0
            );
        }
    }
}
