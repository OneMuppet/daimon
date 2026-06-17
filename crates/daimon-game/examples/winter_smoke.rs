//! Open-world winter-survival smoke: does provisioning actually save minds?
//!
//! Runs two otherwise-identical open worlds through ≥1 full winter — one with the
//! `can_provision` gene ON, one OFF (the control) — under lethal starvation, and
//! reports survivors + the granary cache trajectory. The whole point of v1 is that
//! the asymmetry is REAL: provisioners draw down a stocked granary through the cold
//! and live; the control has no cache and the winter culls it.
//!
//!   cargo run -p daimon-game --example winter_smoke --release

use daimon_game::sim::{GameWorld, Season, SEASON_TICKS, YEAR_TICKS};
use daimon_mind::Genome;

fn run(can_provision: bool, seed: u64, ticks: u64) -> (usize, usize, f32, f32) {
    let mut g = Genome::showcase();
    g.g[22] = 1.0; // can_die — the winter must be able to kill
    g.g[24] = if can_provision { 1.0 } else { 0.0 }; // the ONLY difference
    let mut w = GameWorld::with_genome(seed, 8, &g);
    w.set_open_world(true);
    w.soften_stalker(); // isolate WINTER as the killer (not predator luck)
    // NOTE: global lethal_starvation stays OFF here, so the GOOD seasons are
    // survivable (ordinary hunger floors at a low ebb) — winter is then the clean
    // differentiator: the open-world winter is lethal on its own to a mind that runs
    // empty out in the cold (the unprovisioned), while a provisioned mind draws the
    // hearth's stores and lives.
    let start = w.living_count();
    let mut peak_cache = 0.0f32;
    let mut cache_at_winter = 0.0f32;
    let mut logged_winter = false;
    for _ in 0..ticks {
        w.step();
        peak_cache = peak_cache.max(w.granary_food);
        if matches!(w.season(), Season::Winter) && !logged_winter {
            cache_at_winter = w.granary_food;
            logged_winter = true;
        }
    }
    (start, w.living_count(), peak_cache, cache_at_winter)
}

fn main() {
    // run through the first full winter and just into the next spring (so we measure
    // who SURVIVED the winter, not who outlasts several years). Winter is the 4th
    // season of the first year, ending at YEAR_TICKS; +a little spring to confirm the
    // survivors made it out the other side.
    let ticks = YEAR_TICKS + SEASON_TICKS / 4; // end of winter + a touch of spring
    println!("\nOPEN-WORLD WINTER SMOKE  (8 minds, {ticks} ticks: through the first full winter)\n");
    println!("year = {YEAR_TICKS} ticks, season = {SEASON_TICKS} ticks; first winter ≈ [{}, {}]",
        3 * SEASON_TICKS, YEAR_TICKS);
    println!();
    let seeds = [0x5EED01u64, 0x5EED02, 0x5EED03];
    let (mut prov_start, mut prov_end, mut ctrl_end) = (0usize, 0usize, 0usize);
    for &s in &seeds {
        let (ps, pe, pcache, pcw) = run(true, s, ticks);
        let (_cs, ce, _ccache, ccw) = run(false, s, ticks);
        prov_start += ps;
        prov_end += pe;
        ctrl_end += ce;
        println!(
            "  seed {s:#x}: PROVISION {pe}/{ps} live (cache peak {pcache:.1}, at winter {pcw:.1})  ·  CONTROL {ce}/{ps} live (cache {ccw:.1})"
        );
    }
    println!();
    println!("  TOTAL across {} seeds: provision {prov_end}/{prov_start} survived  vs  control {ctrl_end}/{prov_start}", seeds.len());
    let asym = prov_end as i64 - ctrl_end as i64;
    if asym > 0 {
        println!("  → provisioning improved winter survival by {asym} minds (the loop bites).");
    } else {
        println!("  → NO survival advantage from provisioning (the loop is weak — a finding, not a pass).");
    }
}
