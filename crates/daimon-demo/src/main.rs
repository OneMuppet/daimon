//! `daimon` — watch a mind live a life.
//!
//! Drops a [`Mind`] into a [`World`] and runs the perception→cognition→action
//! loop for a while, printing the agent's narrated inner life as it goes. At
//! the end it prints a "life in review": what the agent learned, the skills it
//! grew confident in, the relationships it formed, and how often it actually
//! had to stop and *think* versus run on cheap reflexes.
//!
//! Usage:
//! ```text
//!   daimon                 # default: Kael, a balanced wanderer, 200 ticks
//!   daimon --ticks 400     # live longer
//!   daimon --seed 7        # a different but reproducible life
//!   daimon --persona bold  # bold | timid | curious | social | balanced
//!   daimon --quiet         # only the final life-in-review
//! ```

use daimon_core::Pos;
use daimon_mind::{Mind, Persona, Process};
use daimon_world::{World, WorldConfig};

struct Args {
    ticks: u64,
    seed: u64,
    persona: String,
    quiet: bool,
}

fn parse_args() -> Args {
    let mut a = Args {
        ticks: 200,
        seed: 0xDA13,
        persona: "balanced".to_string(),
        quiet: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--ticks" => {
                if let Some(v) = it.next() {
                    a.ticks = v.parse().unwrap_or(a.ticks);
                }
            }
            "--seed" => {
                if let Some(v) = it.next() {
                    a.seed = v.parse().unwrap_or(a.seed);
                }
            }
            "--persona" => {
                if let Some(v) = it.next() {
                    a.persona = v;
                }
            }
            "--quiet" => a.quiet = true,
            "-h" | "--help" => {
                println!("daimon [--ticks N] [--seed N] [--persona bold|timid|curious|social|balanced] [--quiet]");
                std::process::exit(0);
            }
            _ => {}
        }
    }
    a
}

fn persona(kind: &str) -> Persona {
    match kind {
        "bold" => Persona::new("Roin")
            .with_boldness(0.9)
            .with_sociability(0.4)
            .with_curiosity(0.6)
            .with_creed("Fear is a leash. I'd rather see what's out there."),
        "timid" => Persona::new("Sela")
            .with_boldness(0.1)
            .with_sociability(0.6)
            .with_curiosity(0.4)
            .with_creed("Careful keeps you breathing. I watch before I step."),
        "curious" => Persona::new("Vell")
            .with_boldness(0.5)
            .with_sociability(0.3)
            .with_curiosity(0.95)
            .with_creed("Everything here is a question I haven't answered yet."),
        "social" => Persona::new("Mira")
            .with_boldness(0.4)
            .with_sociability(0.95)
            .with_curiosity(0.5)
            .with_creed("No one should have to face the stalker alone."),
        _ => Persona::new("Kael")
            .with_boldness(0.5)
            .with_sociability(0.5)
            .with_curiosity(0.5)
            .with_creed("I want to understand this place — and last long enough to."),
    }
}

fn bar(v: f32, width: usize) -> String {
    let filled = ((v.clamp(0.0, 1.0)) * width as f32).round() as usize;
    let mut s = String::with_capacity(width);
    for i in 0..width {
        s.push(if i < filled { '█' } else { '·' });
    }
    s
}

fn main() {
    let args = parse_args();
    let p = persona(&args.persona);
    let name = p.name.clone();
    let creed = p.creed.clone();

    let mut world = World::new(WorldConfig {
        seed: args.seed,
        ..Default::default()
    });
    let mut mind = Mind::new(p, args.seed ^ 0xA11CE);

    println!("╔══════════════════════════════════════════════════════════════════════╗");
    println!("║  DAIMON — an autonomous mind, living a life                            ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");
    println!("  Agent : {name}");
    println!("  Creed : \"{creed}\"");
    println!(
        "  World : {}×{} grid · sight {} · seed 0x{:X}",
        world.config().width,
        world.config().height,
        world.config().sight,
        args.seed
    );
    println!("  Life  : {} ticks", args.ticks);
    println!("  Legend: [SLOW]=deliberated  [REFLEX]=instinct  (plain)=fast/routine\n");

    // first look around
    let mut percept = world.observe();

    // Collapse consecutive identical thoughts (e.g. a long flee) into one line
    // with a "(×N)" suffix, so the inner monologue reads as a life, not a log.
    let mut pending: Option<(String, String, u32)> = None; // (line, dedup-key, count)
    let flush = |pending: &mut Option<(String, String, u32)>| {
        if let Some((line, _, n)) = pending.take() {
            if n > 1 {
                println!("{line}   (×{n})");
            } else {
                println!("{line}");
            }
        }
    };

    for _ in 0..args.ticks {
        let thought = mind.cycle(&percept);

        if !args.quiet {
            let tag = match thought.process {
                Process::Deliberate => "SLOW  ",
                Process::Reflex => "REFLEX",
                Process::Routine => "      ",
            };
            // surface deliberations and reflexes always; sample routine beats.
            let interesting = thought.process != Process::Routine || thought.tick.is_multiple_of(7);
            if interesting {
                let line = format!("t{:>4} {} {}", thought.tick, tag, thought.inner);
                let key = format!("{tag} {}", thought.inner);
                match pending.as_mut() {
                    Some((_, k, n)) if *k == key => *n += 1,
                    _ => {
                        flush(&mut pending);
                        pending = Some((line, key, 1));
                    }
                }
            }
        }

        if world.is_dead() {
            flush(&mut pending);
            println!("\n  ✖ {name} did not survive (t{}).", world.tick());
            break;
        }
        percept = world.step(&thought.action);
    }
    flush(&mut pending);

    life_in_review(&name, &mind, world.body().pos);
}

fn life_in_review(name: &str, mind: &Mind, last_pos: Pos) {
    let m = mind.metrics();
    let body = mind.world().me().unwrap_or_else(|| daimon_core::SelfState::new(last_pos));

    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║  {name}'s life in review");
    println!("╚══════════════════════════════════════════════════════════════════════╝");

    println!("\n  Body at the end");
    println!("    health    {} {:.0}%", bar(body.health, 16), body.health * 100.0);
    println!("    energy    {} {:.0}%", bar(body.energy, 16), body.energy * 100.0);
    println!("    hydration {} {:.0}%", bar(body.hydration, 16), body.hydration * 100.0);

    println!("\n  How the mind spent its cycles");
    println!("    ticks lived ............ {}", m.ticks);
    let think_pct = if m.ticks > 0 {
        m.deliberations as f32 / m.ticks as f32 * 100.0
    } else {
        0.0
    };
    println!(
        "    deliberations (System 2)  {}  ({:.1}% of ticks — the rest was cheap)",
        m.deliberations, think_pct
    );
    println!("    reflexes (instinct) ...... {}", m.reflexes);
    println!("    reflections .............. {}", m.reflections);

    println!("\n  What it did with its life");
    println!("    things discovered ........ {}", m.discoveries);
    println!("    meals eaten .............. {}", m.meals);
    println!("    conversations ............ {}", m.conversations);
    println!("    predator strikes survived  {}", m.near_death_escapes);

    println!("\n  Final drives (what it wants right now)");
    for (d, v) in mind.drives().iter() {
        println!("    {:<10}{} {:.0}%", d.name(), bar(v, 14), v * 100.0);
    }
    let (dom, _) = mind.drives().dominant();
    println!("    → strongest: {}", dom.name());

    println!("\n  What it came to believe (semantic memory)");
    let mut facts: Vec<_> = mind.memory().facts().collect();
    facts.sort_by(|a, b| b.1.confidence.total_cmp(&a.1.confidence));
    for (_, f) in facts.iter().take(8) {
        println!("    • {}  ({:.0}% sure)", f.statement, f.confidence * 100.0);
    }

    println!("\n  Skills it grew into (procedural memory)");
    let mut skills: Vec<_> = mind.memory().skills().collect();
    skills.sort_by(|a, b| b.competence().total_cmp(&a.competence()));
    if skills.is_empty() {
        println!("    (still a novice at everything)");
    }
    for s in skills.iter().take(6) {
        println!(
            "    • {:<16}{} {:.0}% over {} tries",
            s.name,
            bar(s.competence(), 12),
            s.competence() * 100.0,
            s.uses
        );
    }

    println!("\n  People it met (theory of mind)");
    let mut folk: Vec<_> = mind.social().known().collect();
    folk.sort_by(|a, b| b.disposition.total_cmp(&a.disposition));
    if folk.is_empty() {
        println!("    (kept to itself)");
    }
    for a in folk {
        let feeling = if a.disposition > 0.4 {
            "friend"
        } else if a.disposition > 0.0 {
            "amiable"
        } else {
            "wary"
        };
        println!(
            "    • {:<14} {:<8} (disposition {:+.2}, {} interactions)",
            a.name, feeling, a.disposition, a.interactions
        );
    }

    println!("\n  A few vivid memories (most salient episodes)");
    let mut eps: Vec<_> = mind.memory().episodes().collect();
    eps.sort_by(|a, b| b.salience.total_cmp(&a.salience));
    for e in eps.iter().take(5) {
        println!("    t{:<4} {}", e.tick, e.what);
    }
    println!();
}
