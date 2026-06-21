//! Trace the EMERGENT SOCIETY (Sprint 4) headlessly: build the live showcase world
//! exactly as `Game::new` does (lifecycle + materials + wildlife + SOCIETY on) and
//! watch, over a long run, whether distinct VILLAGES form and persist, and whether
//! their ALLIANCES and RIVALRIES genuinely EMERGE and SHIFT — printing #villages,
//! their sizes, the inter-village relation matrix, cross-village marriages, and
//! border deaths. Verifies factions actually appear, relations move, and the world
//! stays alive (the lineage keeps turning over). Throwaway tool.

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

    println!("founded {} villages:", world.villages.len());
    for v in &world.villages {
        println!(
            "  [{}] {:<12} hue #{:06X}  center ({:>3},{:>2})  pop {}",
            v.id, v.name, v.hue, v.center.x, v.center.y, v.population
        );
    }
    println!();

    let ticks: u32 = std::env::var("T").ok().and_then(|s| s.parse().ok()).unwrap_or(30000);
    let every: u32 = std::env::var("E").ok().and_then(|s| s.parse().ok()).unwrap_or(2500);

    // header for the relation matrix (one column per unordered village pair).
    let pairs: Vec<(u8, u8)> = world.relations.iter().map(|r| (r.a, r.b)).collect();
    print!("{:>6}  {:>5}  {:>4}  ", "tick", "alive", "vill");
    for (a, b) in &pairs {
        print!(" {a}-{b}        ");
    }
    println!(" marr  bdth");

    for t in 0..=ticks {
        if t % every == 0 {
            let alive = world.living_count();
            let live_villages = world.villages.iter().filter(|v| v.population > 0).count();
            print!("{t:>6}  {alive:>5}  {live_villages:>4}  ");
            for (a, b) in &pairs {
                if let Some(r) = world.relation_between(*a, *b) {
                    print!(" {:>+.2}/{} ", r.affinity, kind_glyph(r.kind()));
                } else {
                    print!("   --     ");
                }
            }
            println!(" {:>4}  {:>4}", world.cross_marriages, world.border_deaths);
        }
        world.step();
    }

    println!("\n--- final village standings ---");
    for v in &world.villages {
        println!("  [{}] {:<12} pop {:>2}  center ({:>3},{:>2})", v.id, v.name, v.population, v.center.x, v.center.y);
    }
    println!("\n--- final relations ---");
    for r in &world.relations {
        let (na, nb) = (&world.villages[r.a as usize].name, &world.villages[r.b as usize].name);
        println!("  {na:<12} <-> {nb:<12}  {:>+.2}  {}", r.affinity, kind_glyph(r.kind()));
    }
    println!(
        "\nbirths {}  natural_deaths {}  pairings {}  border_deaths {}",
        world.births, world.natural_deaths, world.pairings, world.border_deaths
    );
}
