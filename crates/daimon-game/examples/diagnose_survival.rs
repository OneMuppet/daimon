//! Structural diagnosis of the survival frontier: is the ~23.5% critical-need
//! floor a *policy* gap (fixable by mechanism) or a *structural* limit of the
//! world's decay/relief/travel math (fixable only by changing the world)?
//!
//! Run: `cargo run -p daimon-game --example diagnose_survival --release`

use daimon_core::Drive;
use daimon_game::sim::GameWorld;
use daimon_mind::evolve::Genome;

fn critical_frac(n_agents: usize, foresight: f32, social: bool, seed: u64) -> (f32, f32, f32) {
    let mut g = Genome::baseline();
    g.g[13] = foresight; // anticipation
    g.g[15] = if social { 1.0 } else { 0.0 };
    let mut world = GameWorld::with_genome(seed, n_agents, &g);
    let (mut crit, mut hung, mut thir, mut total) = (0u64, 0u64, 0u64, 0u64);
    for _ in 0..1500 {
        world.step();
        for a in &world.agents {
            let dr = a.mind.drives();
            let h = dr.level(Drive::Hunger) > 0.92;
            let t = dr.level(Drive::Thirst) > 0.92;
            if h || t {
                crit += 1;
            }
            if h {
                hung += 1;
            }
            if t {
                thir += 1;
            }
            total += 1;
        }
    }
    let d = total.max(1) as f32;
    (crit as f32 / d, hung as f32 / d, thir as f32 / d)
}

fn avg(n: usize, foresight: f32, social: bool) -> (f32, f32, f32) {
    let seeds = [0xF0E1u64, 0xF0E2, 0xF0E3];
    let mut a = (0.0, 0.0, 0.0);
    for &s in &seeds {
        let (c, h, t) = critical_frac(n, foresight, social, s);
        a.0 += c;
        a.1 += h;
        a.2 += t;
    }
    let k = seeds.len() as f32;
    (a.0 / k, a.1 / k, a.2 / k)
}

fn main() {
    println!("\nStructural survival diagnosis (critical = need-level > 0.92; 1500 ticks, 3 seeds)\n");
    println!("  {:<42} crit%   hunger%  thirst%", "configuration");
    let show = |label: &str, (c, h, t): (f32, f32, f32)| {
        println!("  {label:<42} {:>5.1}   {:>6.1}   {:>6.1}", c * 100.0, h * 100.0, t * 100.0);
    };
    show("6 agents, REACTIVE (earned-ness check)", avg(6, 0.0, false));
    show("6 agents, anticipatory (the real setting)", avg(6, 0.55, false));
    show("6 agents, anticipatory + commons", avg(6, 0.55, true));
    show("1 agent, anticipatory (reference)", avg(1, 0.55, false));
    println!("\n  --- SCALE: does collective intelligence earn its keep when crowded? ---");
    show("12 agents, anticipatory (no commons)", avg(12, 0.55, false));
    show("12 agents, anticipatory + commons", avg(12, 0.55, true));
    show("18 agents, anticipatory (no commons)", avg(18, 0.55, false));
    show("18 agents, anticipatory + commons", avg(18, 0.55, true));
    println!("\n  Read: if even 1 agent alone stays critical ~20%, the world's decay/relief/");
    println!("  travel math is the wall — coordination cannot fix what is structural.\n");
}
