//! Self-organising defence — observe what happens when agents are merely *given
//! the tool* to confront the stalker (never told to rally).
//!
//!   cargo run -p daimon-game --example defense --release
//!
//! Nothing in the agents counts allies or commands a group response. Each agent
//! independently learns whether confronting works and may choose it. The world
//! physics only make confrontation effective *in numbers*. We watch whether
//! collective defence emerges, against a flee-only control.

use daimon_game::sim::GameWorld;
use daimon_mind::evolve::Genome;

fn run(can_fight: bool, n: usize, seed: u64, ticks: u64) -> (u32, u32, f32, f32, f32) {
    // showcase policy, with the fight *option* toggled (gene 20).
    let mut g = Genome::showcase().g;
    g[20] = if can_fight { 1.0 } else { 0.0 };
    let genome = Genome { g };
    let mut world = GameWorld::with_genome(seed, n, &genome);
    let (mut near, mut total) = (0u64, 0u64);
    let mut harm = 0.0f32;
    let mut prev_health: Vec<f32> = world.agents.iter().map(|a| a.body.health).collect();
    for _ in 0..ticks {
        world.step();
        let pp = world.predator.pos;
        for (k, a) in world.agents.iter().enumerate() {
            if a.body.pos.manhattan(pp) <= 2 {
                near += 1;
            }
            let drop = (prev_health[k] - a.body.health).max(0.0);
            harm += drop;
            prev_health[k] = a.body.health;
            total += 1;
        }
    }
    // mean learned confront-value across the village at the end (did they learn?).
    let cv = world.agents.iter().map(|a| a.mind.confront_value()).sum::<f32>()
        / world.agents.len() as f32;
    let near_frac = near as f32 / total.max(1) as f32;
    let harm_per_agent = harm / world.agents.len() as f32;
    (world.repels, world.lone_strikes, cv, near_frac, harm_per_agent)
}

fn main() {
    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║  SELF-ORGANISING DEFENCE — given the tool, not the instruction          ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝\n");
    let seeds = [0xDEFu64, 0xDE0, 0xDE1, 0xDE2, 0xDE3];
    let ticks = 4000u64;
    // Density is the world *condition* (not a behaviour script): a sparse village
    // is never together when the stalker strikes; a dense one is. We vary it and
    // watch whether collective defence emerges on its own.
    for &n in &[6usize, 12, 18] {
        let (mut repels, mut lone, mut cv) = (0u32, 0u32, 0.0f32);
        let (mut harm_on, mut harm_off) = (0.0f32, 0.0f32);
        for &s in &seeds {
            let (r, l, c, _, h) = run(true, n, s, ticks);
            repels += r;
            lone += l;
            cv += c;
            harm_on += h;
            let (_, _, _, _, h0) = run(false, n, s, ticks);
            harm_off += h0;
        }
        let k = seeds.len() as f32;
        println!("  {n:>2}-agent village ({ticks} ticks × {} seeds):", seeds.len());
        println!(
            "     collective repels (≥2 face it together): {repels:>3}   lone strikes: {lone:>3}   learned confront-value: {:+.2}",
            cv / k
        );
        println!(
            "     harm/agent — with fight option: {:.2}   flee-only control: {:.2}   {}",
            harm_on / k,
            harm_off / k,
            if repels > 0 { "← DEFENCE EMERGED" } else { "(no rally)" }
        );
        println!();
    }
    println!("  Nothing tells the agents to gather or to fight together. Any collective");
    println!("  defence is emergent: shared threat + the tool + each agent's own learning.");
    println!();
}
