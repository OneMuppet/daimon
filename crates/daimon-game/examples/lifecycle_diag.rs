//! Trace the LIFE-CYCLE (Sprint 3) headlessly — far faster than screenshots for
//! verifying the loop works and is balanced. Builds the live showcase world
//! EXACTLY as `Game::new` does (same seed, same flags, same gene set) and traces,
//! over a long run, the population, pair-bonds, cumulative births / natural deaths,
//! average age, and average happiness.
//!
//! The bar for "a stable turning-over village": births ≈ deaths over the long run,
//! so the population neither explodes to the cap nor dwindles to zero — elders pass,
//! children are born, and the lineage keeps going across generations.
//!
//! Run: `cargo run -p daimon-game --example lifecycle_diag --release`
//!   TICKS=40000  how long to run (default 30000 — generations take time)
//!   CAP=220      population cap (default 220, as in Game::new)

use daimon_game::sim::GameWorld;
use daimon_mind::Genome;

fn main() {
    // The live showcase gene set, verbatim from `Game::new`.
    let mut genome = Genome::showcase();
    genome.g[21] = 1.0; // can_build
    genome.g[22] = 1.0; // can_die
    genome.g[23] = 1.0; // can_grieve
    genome.g[24] = 1.0; // can_provision
    genome.g[29] = 1.0; // can_mate
    genome.g[30] = 1.0; // can_reproduce
    genome.g[31] = 1.0; // can_age
    genome.g[32] = 1.0; // feel_happiness

    let cap: usize = std::env::var("CAP").ok().and_then(|s| s.parse().ok()).unwrap_or(90);
    let ticks: u32 = std::env::var("TICKS").ok().and_then(|s| s.parse().ok()).unwrap_or(30000);

    // same big village as Game::new (seed, dims, softened stalker, materials, wildlife).
    let mut world = GameWorld::with_genome_sized(0x61, 64, &genome, 124, 84, 7);
    world.soften_stalker();
    world.set_materials_world(true);
    world.set_wildlife(true);
    world.set_lifecycle(true, cap);

    eprintln!("(lifecycle diag — cap {cap}, {ticks} ticks; founders 64)");
    println!(
        "{:>6}  {:>5}  {:>5}  {:>6}  {:>6}  {:>7}  {:>7}  {:>5}",
        "tick", "alive", "pairs", "births", "natDth", "avgAge", "avgHap", "kids"
    );

    for t in 0..=ticks {
        if t % 2000 == 0 {
            // count current children (immature minds) for a quick "young blood" read.
            let kids = world.living().filter(|a| a.maturity < 0.92).count();
            println!(
                "{:>6}  {:>5}  {:>5}  {:>6}  {:>6}  {:>7.0}  {:>7.2}  {:>5}",
                t,
                world.living_count(),
                world.pairbond_count(),
                world.births,
                world.natural_deaths,
                world.avg_age(),
                world.avg_happiness(),
                kids,
            );
        }
        if world.living_count() == 0 {
            println!("ALL DEAD by tick {t}");
            break;
        }
        world.step();
    }

    // window-rate report: are births and deaths roughly balanced over the run?
    let births = world.births;
    let natural = world.natural_deaths;
    println!(
        "\nFINAL @ {ticks}: alive {} / cap {cap}  | births {births}  natural-deaths {natural}  pairs {}  avgAge {:.0}  avgHap {:.2}",
        world.living_count(),
        world.pairbond_count(),
        world.avg_age(),
        world.avg_happiness(),
    );

    // total deaths of all causes (population accounting): founders + births - alive.
    let total_spawned = 64 + births as usize;
    let total_dead = total_spawned.saturating_sub(world.living_count());
    println!(
        "accounting: spawned {total_spawned} (64 founders + {births} born), dead {total_dead} (of which {natural} of old age), alive {}",
        world.living_count()
    );
}
