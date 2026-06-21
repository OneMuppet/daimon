//! Trace EMERGENT WARFARE (Civilization Sprint 2) headlessly: build the live showcase
//! world exactly as `Game::new` does (lifecycle + materials + wildlife + society + eras
//! + WAR on) and watch, over a long run, whether soured village pairs go to WAR, field
//! warbands, take casualties at the border, and — crucially — whether those wars END
//! (truce → relations recover) while the world STAYS ALIVE (lineage keeps turning over,
//! villages persist + advance eras). Prints, on a cadence: living pop, #villages, the
//! relation matrix, active wars (with warband sizes + casualties), and the running
//! war tallies. Verifies wars FLARE and END and are SURVIVABLE (not extinction).
//! Throwaway tool. Env: T=ticks E=every V=villages SEED=hex.

use daimon_game::sim::{GameWorld, RelationKind};
use daimon_mind::Genome;

fn kind_glyph(k: RelationKind) -> &'static str {
    match k {
        RelationKind::Allied => "ALLY ",
        RelationKind::Friendly => "frnd ",
        RelationKind::Neutral => "  .  ",
        RelationKind::Rival => "rival",
        RelationKind::Enemy => "ENEMY",
    }
}

fn main() {
    // the exact live showcase gene set (clone showcase + arm the live genes incl. can_war=34).
    let mut genome = Genome::showcase();
    for i in [21, 22, 23, 24, 29, 30, 31, 32, 33, 34] {
        genome.g[i] = 1.0;
    }
    let seed = std::env::var("SEED")
        .ok()
        .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
        .unwrap_or(0x61);
    let mut world = GameWorld::with_genome_sized(seed, 64, &genome, 124, 84, 7);
    world.soften_stalker();
    world.set_materials_world(true);
    world.set_wildlife(true);
    world.set_lifecycle(true, 90);
    let n_villages: usize = std::env::var("V").ok().and_then(|s| s.parse().ok()).unwrap_or(4);
    world.set_society(true, n_villages);
    world.set_eras(true);
    world.set_war(true);

    println!("founded {} villages (seed {seed:#x}):", world.villages.len());
    for v in &world.villages {
        println!(
            "  [{}] {:<12} hue #{:06X}  center ({:>3},{:>2})",
            v.id, v.name, v.hue, v.center.x, v.center.y
        );
    }
    println!();

    let ticks: u32 = std::env::var("T").ok().and_then(|s| s.parse().ok()).unwrap_or(40000);
    let every: u32 = std::env::var("E").ok().and_then(|s| s.parse().ok()).unwrap_or(2000);

    let pairs: Vec<(u8, u8)> = world.relations.iter().map(|r| (r.a, r.b)).collect();

    // also detect war-start/war-end events between sample points so we never miss a
    // flare that started AND ended inside a sampling window.
    let mut prev_declared = 0u32;
    let mut prev_resolved = 0u32;
    let mut peak_concurrent_wars = 0usize;
    let mut min_alive = usize::MAX;
    let mut peak_hostility = 0.0f32; // most-negative affinity ever observed
    // minimum pairwise village-center distance (whether ANY pair can ever contest).
    let mut min_center_dist = i32::MAX;

    print!("{:>6}  {:>5}  {:>4}  {:>4}  ", "tick", "alive", "vill", "wars");
    for (a, b) in &pairs {
        print!(" {a}-{b}      ");
    }
    println!("  decl  resv  dead");

    for t in 0..=ticks {
        if t % every == 0 {
            let alive = world.living_count();
            let live_villages = world.villages.iter().filter(|v| v.population > 0).count();
            let n_wars = world.wars.len();
            peak_concurrent_wars = peak_concurrent_wars.max(n_wars);
            print!("{t:>6}  {alive:>5}  {live_villages:>4}  {n_wars:>4}  ");
            for (a, b) in &pairs {
                if let Some(r) = world.relation_between(*a, *b) {
                    let at_war = world.wars.iter().any(|w| w.a == *a && w.b == *b);
                    let mark = if at_war { "*" } else { " " };
                    print!(" {:>+.2}{}{} ", r.affinity, mark, kind_glyph(r.kind()).chars().next().unwrap());
                } else {
                    print!("   --    ");
                }
            }
            println!(
                "  {:>4}  {:>4}  {:>4}",
                world.wars_declared, world.wars_resolved, world.war_casualties
            );
        }
        world.step();
        min_alive = min_alive.min(world.living_count());
        for r in &world.relations {
            if r.affinity < peak_hostility {
                peak_hostility = r.affinity;
            }
        }
        for i in 0..world.villages.len() {
            for j in (i + 1)..world.villages.len() {
                let d = world.villages[i].center.manhattan(world.villages[j].center);
                if d < min_center_dist {
                    min_center_dist = d;
                }
            }
        }
        // report each NEW war start / resolution as it happens (between samples).
        if world.wars_declared > prev_declared {
            for w in &world.wars {
                // only the freshly added ones started at ~this tick.
                if w.started == world.tick {
                    let na = &world.villages[w.a as usize].name;
                    let nb = &world.villages[w.b as usize].name;
                    let ba = world.warband_size(w.a);
                    let bb = world.warband_size(w.b);
                    println!(
                        "  t{:<6} WAR DECLARED  {na} (band {ba}, {}) vs {nb} (band {bb}, {})  front ({},{})",
                        world.tick,
                        world.villages[w.a as usize].era.name(),
                        world.villages[w.b as usize].era.name(),
                        w.front.x, w.front.y
                    );
                }
            }
            prev_declared = world.wars_declared;
        }
        if world.wars_resolved > prev_resolved {
            println!(
                "  t{:<6} WAR RESOLVED (truce)  total casualties so far {}",
                world.tick, world.war_casualties
            );
            prev_resolved = world.wars_resolved;
        }
    }

    println!("\n--- final village standings ---");
    for v in &world.villages {
        println!(
            "  [{}] {:<12} pop {:>2}  {:<14} center ({:>3},{:>2})",
            v.id, v.name, v.population, v.era.name(), v.center.x, v.center.y
        );
    }
    println!("\n--- final relations ---");
    for r in &world.relations {
        let (na, nb) = (&world.villages[r.a as usize].name, &world.villages[r.b as usize].name);
        println!("  {na:<12} <-> {nb:<12}  {:>+.2}  {}", r.affinity, kind_glyph(r.kind()));
    }
    println!(
        "\nwars_declared {}  wars_resolved {}  war_casualties {}  peak_concurrent_wars {}",
        world.wars_declared, world.wars_resolved, world.war_casualties, peak_concurrent_wars
    );
    println!(
        "peak_hostility {:+.2} (war bar {:+.2})  min_center_dist {} (contest radius {})",
        peak_hostility, -0.55, min_center_dist, 22
    );
    println!(
        "births {}  natural_deaths {}  border_deaths {}  final_alive {}  min_alive {}",
        world.births, world.natural_deaths, world.border_deaths, world.living_count(), min_alive
    );
    // honest verdict
    let world_survived = world.living_count() >= 10;
    let wars_ended = world.wars_resolved > 0 && world.wars_resolved >= world.wars_declared.saturating_sub(world.wars.len() as u32);
    println!(
        "\nVERDICT: wars flared={} resolved={} | world_survived={} | wars_end_cleanly={}",
        world.wars_declared, world.wars_resolved, world_survived, wars_ended
    );
}
