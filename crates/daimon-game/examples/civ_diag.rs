//! Trace the CIVILIZATION CAPSTONE (Civilization Sprint 3) headlessly: build the live
//! showcase world exactly as `Game::new` does (lifecycle + materials + wildlife + society
//! + eras + war + CIV on), fast-forward the villages to the SPACE AGE via the live-only
//! `advance_research_to_era` capture seam, then run on and report:
//!   • POLITICS — each village's named LEADER (its eldest member), refreshed as elders die;
//!   • DIPLOMACY — formalized named TREATIES that crystallize from sustained alliances;
//!   • WONDERS — the monumental landmark each advanced village raises;
//!   • SPACE AGE — rocket LAUNCHES (count) once a village reaches Era::Space.
//! Verifies the layered world systems actually fire. Throwaway tool.
//!
//! Env: T=ticks after advance (default 4000), ADV=era rank to fast-forward to (default 4=Space).

use daimon_game::sim::GameWorld;
use daimon_mind::Genome;

fn main() {
    let mut genome = Genome::showcase();
    for i in [21, 22, 23, 24, 29, 30, 31, 32, 33] {
        genome.g[i] = 1.0;
    }
    let mut world = GameWorld::with_genome_sized(0x61, 64, &genome, 124, 84, 7);
    world.soften_stalker();
    world.set_materials_world(true);
    world.set_wildlife(true);
    world.set_lifecycle(true, 90);
    world.set_society(true, 4);
    world.set_eras(true);
    world.set_war(true);
    world.set_civ(true);

    // warm a little so villages are peopled + have built before we fast-forward tech.
    for _ in 0..1500 {
        world.step();
    }

    let adv: usize = std::env::var("ADV").ok().and_then(|s| s.parse().ok()).unwrap_or(4);
    world.advance_research_to_era(adv);
    println!("== fast-forwarded all villages to era rank {adv} ==");

    let ticks: u32 = std::env::var("T").ok().and_then(|s| s.parse().ok()).unwrap_or(4000);
    for _ in 0..ticks {
        world.step();
    }

    println!("\n--- FINAL CIVILIZATION STATE (tick {}) ---", world.tick);
    println!("living: {}", world.living_count());
    println!(
        "tallies: wonders_raised={} treaties_signed={} rockets_launched={} (in-flight now {})",
        world.wonders_raised,
        world.treaties_signed,
        world.rockets_launched,
        world.rockets.len()
    );
    println!("\nVILLAGES:");
    for v in &world.villages {
        let leader = if v.leader_name.is_empty() { "—" } else { v.leader_name.as_str() };
        let wonder = v.wonder.as_ref().map(|w| w.name.as_str()).unwrap_or("—");
        println!(
            "  [{}] {:<12} {:<7} pop {:>2}  research {:>6.0}",
            v.id, v.name, v.era.name(), v.population, v.research
        );
        println!("        Leader: {leader}");
        println!("        Wonder: {wonder}");
    }
    println!("\nTREATIES:");
    if world.treaties.is_empty() {
        println!("  (none yet)");
    }
    for t in &world.treaties {
        let (na, nb) = (&world.villages[t.a as usize].name, &world.villages[t.b as usize].name);
        println!("  {} — {na} & {nb} (signed tick {})", t.name, t.signed);
    }

    // a forced launch demo so we can confirm a rocket can be put aloft on demand.
    world.force_launch();
    world.put_rockets_mid_flight();
    println!("\nforced launch → rockets in flight now: {}", world.rockets.len());
    for r in &world.rockets {
        println!(
            "  rocket from V{} at pad ({},{}) launched tick {}",
            r.village, r.pad.x, r.pad.y, r.launched
        );
    }
}
