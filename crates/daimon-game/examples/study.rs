//! Field study — a fast, render-free run that watches the village *behave* and
//! reports where the minds are strong and weak, so we can learn and improve the
//! AI (not just pass/fail it like `believability`). No GPU: the sim runs at tens
//! of thousands of ticks/sec, so a multi-thousand-tick life takes a blink.
//!
//!   cargo run -p daimon-game --example study --release [ticks]
//!
//! It prints: survival & wellbeing, the dual-process decision mix, whether the
//! world-model is actually *learning* (surprise/error trend), affect, social &
//! cultural life, predator pressure — then a list of ANOMALY FLAGS: heuristics
//! that point at the next thing worth fixing.

use daimon_core::Drive;
use daimon_game::sim::GameWorld;
use daimon_mind::{evolve::Genome, Process};

const NW: usize = 10; // trend windows (deciles of the run)

#[derive(Default, Clone, Copy)]
struct Win {
    n: f64,
    alive: f64,
    health: f64,
    crit_thirst: f64,
    crit_hunger: f64,
    surprise: f64,
    pred_err: f64,
    learn_prog: f64,
    valence: f64,
    arousal: f64,
    reflex: f64,
    deliberate: f64,
    routine: f64,
    has_plan: f64,
    invented: f64,
}

fn main() {
    let ticks: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(6000);
    let seed = 0xDA13u64;
    let n_agents = 6;
    let mut world = GameWorld::with_genome(seed, n_agents, &Genome::showcase());

    let mut win = [Win::default(); NW];
    let mut emotions: std::collections::HashMap<&'static str, u64> = std::collections::HashMap::new();
    // Health is floored at 0.05 in the sim (agents don't truly die), so we count
    // *near-death dips* to that floor rather than "deaths".
    let mut near_death = 0u64;
    let mut prev_floored = world.agents.iter().filter(|a| a.body.health <= 0.06).count();
    let mut harm_total = 0.0f64;
    let mut prev_health: Vec<f32> = world.agents.iter().map(|a| a.body.health).collect();
    // Movement must be read off the LOGICAL grid position (body.pos); rx/ry are
    // render-interpolation state and only move when animate() is called.
    let mut displacement = 0.0f64;
    let mut prev_pos: Vec<(f32, f32)> =
        world.agents.iter().map(|a| (a.body.pos.x as f32, a.body.pos.y as f32)).collect();

    for t in 0..ticks {
        world.step();
        let wi = ((t * NW as u64) / ticks) as usize;
        let w = &mut win[wi.min(NW - 1)];
        let mut alive = 0;
        for (k, a) in world.agents.iter().enumerate() {
            w.n += 1.0;
            let dr = a.mind.drives();
            if a.body.health > 0.05 {
                alive += 1;
            }
            w.alive += if a.body.health > 0.05 { 1.0 } else { 0.0 };
            w.health += a.body.health as f64;
            if dr.level(Drive::Thirst) > 0.92 {
                w.crit_thirst += 1.0;
            }
            if dr.level(Drive::Hunger) > 0.92 {
                w.crit_hunger += 1.0;
            }
            w.surprise += a.mind.surprise() as f64;
            w.pred_err += a.mind.prediction_error() as f64;
            w.learn_prog += a.mind.learning_progress() as f64;
            let af = a.mind.affect();
            w.valence += af.valence as f64;
            w.arousal += af.arousal as f64;
            *emotions.entry(af.emotion()).or_insert(0) += 1;
            if a.mind.intent_target().is_some() {
                w.has_plan += 1.0;
            }
            if a.mind.acting_on_invented() {
                w.invented += 1.0;
            }
            if let Some(th) = &a.last {
                match th.process {
                    Process::Reflex => w.reflex += 1.0,
                    Process::Deliberate => w.deliberate += 1.0,
                    Process::Routine => w.routine += 1.0,
                }
            }
            // harm + displacement
            let drop = (prev_health[k] - a.body.health).max(0.0);
            harm_total += drop as f64;
            prev_health[k] = a.body.health;
            let (px, py) = (a.body.pos.x as f32, a.body.pos.y as f32);
            displacement += ((px - prev_pos[k].0).powi(2) + (py - prev_pos[k].1).powi(2)).sqrt() as f64;
            prev_pos[k] = (px, py);
        }
        let _ = alive;
        let floored = world.agents.iter().filter(|a| a.body.health <= 0.06).count();
        if floored > prev_floored {
            near_death += (floored - prev_floored) as u64;
        }
        prev_floored = floored;
    }

    // ---- aggregate ----
    let tot: f64 = win.iter().map(|w| w.n).sum();
    let mean = |f: fn(&Win) -> f64| win.iter().map(f).sum::<f64>() / tot;
    let first = |f: fn(&Win) -> f64| f(&win[0]) / win[0].n.max(1.0);
    let last = |f: fn(&Win) -> f64| f(&win[NW - 1]) / win[NW - 1].n.max(1.0);

    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║  DAIMON FIELD STUDY — {n_agents} minds · {ticks} ticks · showcase policy        ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");

    let alive_frac = mean(|w| w.alive);
    println!("\n── SURVIVAL & WELLBEING ─────────────────────────────────────────────");
    println!("  mean alive:           {:.2}/{n_agents}  ({:.0}%)", alive_frac * n_agents as f64, alive_frac * 100.0);
    println!("  near-death dips:      {near_death}  (health touched the 0.05 floor; agents don't truly die)");
    println!("  mean health:          {:.2}", mean(|w| w.health));
    println!(
        "  critical thirst:      {:5.1}% of agent-ticks   (early {:.1}% → late {:.1}%)",
        mean(|w| w.crit_thirst) * 100.0,
        first(|w| w.crit_thirst) * 100.0,
        last(|w| w.crit_thirst) * 100.0
    );
    println!(
        "  critical hunger:      {:5.1}% of agent-ticks   (early {:.1}% → late {:.1}%)",
        mean(|w| w.crit_hunger) * 100.0,
        first(|w| w.crit_hunger) * 100.0,
        last(|w| w.crit_hunger) * 100.0
    );

    println!("\n── COGNITION (dual-process & world-model) ───────────────────────────");
    let (rf, de, ro) = (mean(|w| w.reflex), mean(|w| w.deliberate), mean(|w| w.routine));
    let dsum = (rf + de + ro).max(1e-9);
    println!(
        "  decision mix:         reflex {:.0}%  ·  deliberate {:.0}%  ·  routine {:.0}%",
        rf / dsum * 100.0,
        de / dsum * 100.0,
        ro / dsum * 100.0
    );
    println!(
        "  surprise:             {:.3}  (early {:.3} → late {:.3})   {}",
        mean(|w| w.surprise),
        first(|w| w.surprise),
        last(|w| w.surprise),
        trend(first(|w| w.surprise), last(|w| w.surprise), true)
    );
    println!(
        "  prediction error:     {:.3}  (early {:.3} → late {:.3})   {}",
        mean(|w| w.pred_err),
        first(|w| w.pred_err),
        last(|w| w.pred_err),
        trend(first(|w| w.pred_err), last(|w| w.pred_err), true)
    );
    println!(
        "  learning progress:    {:.3}  (early {:.3} → late {:.3})",
        mean(|w| w.learn_prog),
        first(|w| w.learn_prog),
        last(|w| w.learn_prog)
    );
    println!("  has a plan:           {:.0}% of agent-ticks", mean(|w| w.has_plan) * 100.0);
    println!("  acting on invented:   {:.1}% of agent-ticks (praxis goal-genesis)", mean(|w| w.invented) * 100.0);
    println!("  mean step / tick:     {:.3} cells", displacement / tot);

    println!("\n── AFFECT ───────────────────────────────────────────────────────────");
    println!("  mean valence: {:+.2}   mean arousal: {:.2}", mean(|w| w.valence), mean(|w| w.arousal));
    let mut ev: Vec<_> = emotions.iter().collect();
    ev.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
    let etot: u64 = emotions.values().sum();
    print!("  felt:  ");
    for (e, c) in ev.iter().take(5) {
        print!("{e} {:.0}%  ", **c as f64 / etot.max(1) as f64 * 100.0);
    }
    println!();

    println!("\n── SOCIAL & CULTURE (end of run) ────────────────────────────────────");
    let utterances = world.spoken.len();
    let concepts: usize = world.agents.iter().map(|a| a.mind.praxis().concepts.iter().filter(|c| c.seen >= 2).count()).sum();
    let (mut rel, mut tom_ok, mut tom_n) = (0usize, 0usize, 0usize);
    let actual_drive: std::collections::HashMap<String, Drive> = world
        .agents
        .iter()
        .map(|a| (a.name.clone(), a.mind.drives().dominant().0))
        .collect();
    for a in &world.agents {
        for m in a.mind.social().known() {
            rel += 1;
            if let Some(bd) = m.believed_drive {
                tom_n += 1;
                if actual_drive.get(&m.name) == Some(&bd) {
                    tom_ok += 1;
                }
            }
        }
    }
    let skills: usize = world.agents.iter().map(|a| a.mind.memory().skills().count()).sum();
    println!("  utterances logged:    {utterances}");
    println!("  relationships formed: {rel}  (mean {:.1}/agent)", rel as f64 / n_agents as f64);
    println!(
        "  theory-of-mind:       {}/{} beliefs about others' drive correct ({:.0}%, chance ≈17%)",
        tom_ok,
        tom_n,
        if tom_n > 0 { tom_ok as f64 / tom_n as f64 * 100.0 } else { 0.0 }
    );
    println!("  concepts coined:      {concepts}   ·   skills practised: {skills}");

    println!("\n── PREDATOR PRESSURE ────────────────────────────────────────────────");
    println!("  collective repels:    {}  (≥2 faced the stalker together)", world.repels);
    println!("  lone strikes taken:   {}", world.lone_strikes);
    println!("  total harm absorbed:  {:.1}", harm_total);

    // ---- anomaly flags: the actionable payload ----
    println!("\n── ANOMALY FLAGS (where to look next) ───────────────────────────────");
    let mut flags = Vec::new();
    if mean(|w| w.crit_thirst) > 0.18 {
        flags.push(format!("thirst critical {:.0}% of the time → anticipation/foraging or world relief gap", mean(|w| w.crit_thirst) * 100.0));
    }
    if mean(|w| w.crit_hunger) > 0.18 {
        flags.push(format!("hunger critical {:.0}% of the time → forage planning gap", mean(|w| w.crit_hunger) * 100.0));
    }
    if last(|w| w.surprise) >= first(|w| w.surprise) * 0.97 {
        flags.push("surprise not falling → the world-model isn't learning the environment".into());
    }
    if de / dsum < 0.02 {
        flags.push("almost no deliberation → dual-process collapsed to reflex/routine (System-2 dormant)".into());
    }
    if de / dsum > 0.6 {
        flags.push("almost always deliberating → no habit formation (System-1 never takes over; costly)".into());
    }
    if mean(|w| w.has_plan) < 0.4 {
        flags.push("plans rare → agents often act without a committed goal (drifting)".into());
    }
    if displacement / tot < 0.02 {
        flags.push("agents barely move on the grid → planning/navigation may be stuck".into());
    }
    if concepts == 0 {
        flags.push("no concepts coined → Praxis concept-genesis dormant under this policy".into());
    }
    if concepts > 0 && mean(|w| w.invented) < 0.001 {
        flags.push("concepts are coined but never ACTED on → Praxis goal-genesis dormant (concepts ≠ goals)".into());
    }
    if tom_n > 0 && (tom_ok as f64 / tom_n as f64) < 0.17 {
        flags.push("theory-of-mind at/below chance (1/6≈17%) → others'-drive inference miscalibrated".into());
    }
    if world.lone_strikes > 0 && world.repels == 0 {
        flags.push("predator harm taken but never repelled → collective defence never emerges here".into());
    }
    if alive_frac < 0.9 {
        flags.push(format!("village under-survives ({:.0}% alive) → survival policy weak", alive_frac * 100.0));
    }
    if flags.is_empty() {
        println!("  none tripped — behaviour is within healthy bands on every watched signal.");
    } else {
        for (i, f) in flags.iter().enumerate() {
            println!("  {}. {f}", i + 1);
        }
    }
    println!();
}

/// A tiny trend arrow; `lower_better` flips the reading (e.g. surprise).
fn trend(early: f64, late: f64, lower_better: bool) -> &'static str {
    let d = late - early;
    let improving = if lower_better { d < -1e-3 } else { d > 1e-3 };
    let worsening = if lower_better { d > 1e-3 } else { d < -1e-3 };
    if improving {
        "↓ learning"
    } else if worsening {
        "↑ rising"
    } else {
        "→ flat"
    }
}
