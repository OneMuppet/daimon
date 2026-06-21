//! Trace the TECH/ERA progression (Civilization Sprint 1) headlessly: build the live
//! showcase world exactly as `Game::new` does (lifecycle + materials + wildlife +
//! society + ERAS on) and watch, over a long run, whether each VILLAGE genuinely banks
//! RESEARCH and climbs the era ladder (Stone → Bronze → Iron → Industrial → Space) —
//! printing each village's population, building count, research, and era at intervals.
//! Verifies the climb is watchable (Industrial reachable in a session) and that
//! population/buildings/peace actually drive the rate. Throwaway tool.
//!
//! Env: T=ticks (default 30000), E=interval (default 2500), V=#villages (default 4).

use daimon_game::sim::{Era, GameWorld};
use daimon_mind::Genome;

fn era_glyph(e: Era) -> &'static str {
    match e {
        Era::Stone => "Stone",
        Era::Bronze => "Bronz",
        Era::Iron => "Iron ",
        Era::Industrial => "Indus",
        Era::Space => "SPACE",
    }
}

fn main() {
    // the exact live showcase gene set (clone showcase + arm the live genes).
    let mut genome = Genome::showcase();
    for i in [21, 22, 23, 24, 29, 30, 31, 32, 33] {
        genome.g[i] = 1.0;
    }
    let mut world = GameWorld::with_genome_sized(0x61, 64, &genome, 124, 84, 7);
    world.soften_stalker();
    world.set_materials_world(true);
    world.set_wildlife(true);
    world.set_lifecycle(true, 90);
    let n_villages: usize = std::env::var("V").ok().and_then(|s| s.parse().ok()).unwrap_or(4);
    world.set_society(true, n_villages);
    world.set_eras(true);

    println!("ERA thresholds (research banked to REACH each era):");
    for (i, e) in Era::LADDER.iter().enumerate() {
        println!("  {:<14} ≥ {:.0}", e.name(), daimon_game::sim::ERA_THRESHOLDS[i]);
    }
    println!("founded {} villages\n", world.villages.len());

    let ticks: u32 = std::env::var("T").ok().and_then(|s| s.parse().ok()).unwrap_or(30000);
    let every: u32 = std::env::var("E").ok().and_then(|s| s.parse().ok()).unwrap_or(2500);

    let k = world.villages.len();
    // header: per village, "pop/bld rsch era".
    print!("{:>6}  {:>5}  ", "tick", "alive");
    for v in 0..k {
        print!("| V{v} pop/bld rsch  era  ");
    }
    println!();

    let mut first_reach: Vec<Option<(Era, u32)>> = vec![None; k];

    for t in 0..=ticks {
        if t % every == 0 {
            print!("{t:>6}  {:>5}  ", world.living_count());
            for v in &world.villages {
                print!(
                    "| {:>2}/{:>2} {:>5.0} {} ",
                    v.population, v.buildings, v.research, era_glyph(v.era)
                );
            }
            println!();
        }
        // record the first time each village reaches each higher era.
        for v in &world.villages {
            let vi = v.id as usize;
            let reached = first_reach[vi].map(|(e, _)| e).unwrap_or(Era::Stone);
            if v.era > reached {
                first_reach[vi] = Some((v.era, t));
            }
        }
        if world.living_count() == 0 {
            println!("ALL DEAD by tick {t}");
            break;
        }
        world.step();
    }

    println!("\nFINAL eras + climb milestones:");
    for v in &world.villages {
        println!(
            "  [{}] {:<12} pop {:>2}  bld {:>2}  research {:>6.0}  → {} ",
            v.id,
            v.name,
            v.population,
            v.buildings,
            v.research,
            v.era.name(),
        );
    }
}
