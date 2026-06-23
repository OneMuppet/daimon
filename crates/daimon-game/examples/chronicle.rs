//! LIVE CHRONICLE — run the showcase world EXACTLY as `Game::new` does (the full live
//! stack incl. the scarcity world) and print, as text, "how they're doing": periodic
//! society snapshots (population, each village's era/pop/diplomacy/leader, world resource
//! scarcity, war tallies) and, at the end, the rolling event CHRONICLE the HUD shows
//! (births, deaths/burials, wars, era advances, wonders). A live readable log of the
//! same world you watch on :8080. Throwaway diagnostic.
//!
//!   T=ticks E=every cargo run -p daimon-game --example chronicle --release

use daimon_game::sim::{GameWorld, RelationKind};
use daimon_mind::Genome;

fn main() {
    // mirror Game::new's live world (same genes + same live flags).
    let mut g = Genome::showcase();
    for i in [21, 22, 23, 24, 29, 30, 31, 32, 33, 34] {
        g.g[i] = 1.0;
    }
    let mut w = GameWorld::with_genome_sized(0x61, 64, &g, 124, 84, 7);
    w.soften_stalker();
    w.set_materials_world(true);
    w.set_wildlife(true);
    w.set_hunting(true);
    w.set_lifecycle(true, 90);
    w.set_society(true, 4);
    w.set_eras(true);
    w.set_war(true);
    w.set_civ(true);
    w.set_scarcity_world(true);

    let ticks: u32 = std::env::var("T").ok().and_then(|s| s.parse().ok()).unwrap_or(30000);
    let every: u32 = std::env::var("E").ok().and_then(|s| s.parse().ok()).unwrap_or(5000);

    let count = |w: &GameWorld, v: u8, want_ally: bool| -> usize {
        w.relations
            .iter()
            .filter(|r| r.a == v || r.b == v)
            .filter(|r| {
                let o = if r.a == v { r.b } else { r.a };
                w.villages[o as usize].population > 0
            })
            .filter(|r| {
                let ally = matches!(r.kind(), RelationKind::Allied | RelationKind::Friendly);
                if want_ally { ally } else { matches!(r.kind(), RelationKind::Enemy | RelationKind::Rival) }
            })
            .count()
    };

    for t in 0..=ticks {
        if t % every == 0 {
            println!(
                "\n── day {:>3} (tick {t}) ── {} of {} living · scarcity {:.2} · wars {}↑ {}✓ {} dead",
                t / 240,
                w.living_count(),
                w.agents.len(),
                w.village_scarcity(0),
                w.wars_declared,
                w.wars_resolved,
                w.war_casualties,
            );
            for v in &w.villages {
                if v.population == 0 {
                    continue;
                }
                println!(
                    "   {:<12} {:<6} pop {:>2} · {} bld · {} ally / {} rival · leader {}{}",
                    v.name,
                    v.era.name(),
                    v.population,
                    v.buildings,
                    count(&w, v.id, true),
                    count(&w, v.id, false),
                    if v.leader_name.is_empty() { "—".into() } else { v.leader_name.clone() },
                    if v.wonder.is_some() { " · ★wonder" } else { "" },
                );
            }
        }
        w.step();
    }

    println!("\n=== CHRONICLE (last {} events) ===", w.events.len().min(30));
    let start = w.events.len().saturating_sub(30);
    for (tick, line) in &w.events[start..] {
        println!("  d{:<3} {}", tick / 240, line);
    }
}
