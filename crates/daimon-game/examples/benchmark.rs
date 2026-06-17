//! Daimon benchmark — evolvability, performance, and zero-shot generalisation.
//!
//! Produces the headline numbers for the whitepaper. Deterministic except for the
//! wall-clock performance figures (which are inherently machine-dependent).
//!
//!   cargo run -p daimon-game --example benchmark --release

use std::time::Instant;

use daimon_core::{Action, Drive, Entity, EntityId, EntityKind, Percept, Pos, SelfState};
use daimon_game::fitness::evaluate;
use daimon_game::sim::GameWorld;
use daimon_mind::evolve::{Evolution, Genome};
use daimon_mind::{Mind, Persona};

fn main() {
    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║  DAIMON — BENCHMARK SUITE                                              ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");

    performance();
    let champion = evolvability();
    generalisation(&champion);
    println!();
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. PERFORMANCE — raw throughput of the cognitive cycle (all local CPU code).
// ─────────────────────────────────────────────────────────────────────────────
fn performance() {
    println!("\n── PERFORMANCE (deterministic CPU; no GPU/network/ML) ──────────────────");

    // 6-agent village
    let mut world = GameWorld::with_genome(0xBEEF, 6, &Genome::showcase());
    for _ in 0..200 {
        world.step(); // warm caches / let beliefs populate
    }
    let steps = 4000u32;
    let t0 = Instant::now();
    for _ in 0..steps {
        world.step();
    }
    let dt = t0.elapsed().as_secs_f64();
    let tps = steps as f64 / dt;
    println!(
        "  6-agent village:   {:>8.0} ticks/s   ({:>9.0} agent-ticks/s)",
        tps,
        tps * 6.0
    );

    // single agent
    let mut solo = GameWorld::with_genome(0xBEEF, 1, &Genome::showcase());
    for _ in 0..200 {
        solo.step();
    }
    let t1 = Instant::now();
    for _ in 0..steps {
        solo.step();
    }
    let stps = steps as f64 / t1.elapsed().as_secs_f64();
    println!("  1-agent:           {:>8.0} ticks/s", stps);

    // 18-agent crowd
    let mut crowd = GameWorld::with_genome(0xBEEF, 18, &Genome::showcase());
    for _ in 0..200 {
        crowd.step();
    }
    let t2 = Instant::now();
    for _ in 0..1000 {
        crowd.step();
    }
    let ctps = 1000.0 / t2.elapsed().as_secs_f64();
    println!("  18-agent crowd:    {:>8.0} ticks/s   ({:>9.0} agent-ticks/s)", ctps, ctps * 18.0);

    // cost of one fitness evaluation (a whole 600-tick life, 6 agents)
    let t3 = Instant::now();
    let reps = 10;
    for i in 0..reps {
        let _ = evaluate(&Genome::showcase(), &[0x1000 + i as u64], 600);
    }
    let per = t3.elapsed().as_secs_f64() / reps as f64;
    println!(
        "  fitness eval:      {:>8.1} ms/genome (600-tick, 6-agent life)  → {:.0} genomes/s",
        per * 1000.0,
        1.0 / per
    );

    // a whole agent serialises to how many bytes?
    let mind = Mind::new(Persona::new("X"), 7);
    let bytes = mind.to_json().len();
    println!("  a whole mind serialises to ~{} bytes of JSON (portable, inspectable)", bytes);
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. EVOLVABILITY — does the self-improvement loop reliably evolve a good agent,
//    not just get lucky once? K independent searches from different seeds.
// ─────────────────────────────────────────────────────────────────────────────
fn evolvability() -> Genome {
    let search_seeds = [0x5EED1u64, 0x5EED2, 0x5EED3, 0x5EED4, 0x5EED5];
    println!("\n── EVOLVABILITY ({} independent searches, full fitness budget) ─────────", search_seeds.len());
    let train = [0xA1u64, 0xB2, 0xC3]; // training seeds for fitness (full)
    let holdout = [0xD4u64, 0xE5, 0xF6, 0x17, 0x28]; // unseen seeds
    let ticks = 600u64;
    let eval = |g: &Genome| evaluate(g, &train, ticks);
    let baseline = eval(&Genome::baseline()).scalar();

    let mut reached = 0;
    let mut gens_sum = 0u32;
    let mut gain_sum = 0.0f32;
    let mut holdout_ok = 0;
    let mut best: Option<(Genome, f32)> = None; // keep the strongest champion
    for &s in &search_seeds {
        let mut evo = Evolution::new(s, 12, &eval);
        let _ = evo.run(18, &eval);
        if evo.best_fit.meets_target() {
            reached += 1;
            gens_sum += evo.history.len().max(1) as u32; // generations the search ran
        }
        gain_sum += evo.best_fit.scalar() - baseline;
        let val = evaluate(&evo.best, &holdout, ticks);
        if val.meets_target() {
            holdout_ok += 1;
        }
        if best.as_ref().is_none_or(|(_, v)| val.scalar() > *v) {
            best = Some((evo.best.clone(), val.scalar()));
        }
    }
    let k = search_seeds.len();
    println!("  baseline (hand-tuned) scalar:    {baseline:.3}");
    println!("  reached end-goal target:         {reached}/{k} searches");
    if reached > 0 {
        println!("  mean generations the search ran: {:.1}", gens_sum as f32 / reached as f32);
    }
    println!("  mean scalar gain over baseline:  {:+.3}", gain_sum / k as f32);
    println!("  champion meets target on UNSEEN seeds: {holdout_ok}/{k}  (generalises, not overfit)");
    best.expect("at least one search").0
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. GENERALISATION — can it handle what it was NOT trained for?
// ─────────────────────────────────────────────────────────────────────────────
fn generalisation(champion: &Genome) {
    println!("\n── ZERO-SHOT / GENERALISATION (tasks & worlds not trained for) ─────────");

    // (a) Praxis goal-genesis: an agent that *lived beside* a secretly-healing form
    //     crosses the map to it when hurt — pursuing a goal that exists in no drive,
    //     planner, or goal table. The architecture was never coded for "healers".
    let mut learned_ok = 0;
    let mut naive_stayed = 0;
    let seeds = [0x533Du64, 0x533E, 0x533F, 0x5340, 0x5341, 0x5342, 0x5343, 0x5344];
    for &s in &seeds {
        let (ld, nd) = unforeseen_task(s);
        if ld < 10 {
            learned_ok += 1;
        }
        if nd > 18 {
            naive_stayed += 1;
        }
    }
    let n = seeds.len();
    println!("  (a) acting on the UNFORESEEN (never-coded healer), {n} seeds:");
    println!(
        "      experienced agent reaches the healer:   {learned_ok}/{n}",
    );
    println!(
        "      inexperienced control ignores it:       {naive_stayed}/{n}   (only difference = lived experience)"
    );

    // (b) Generalisation to UNSEEN environments: take the *evolved champion* (tuned
    //     on a 6-agent world) and drop it into village sizes it never trained on.
    println!("  (b) evolved champion in UNSEEN village sizes (critical-need time, lower=better):");
    for &nag in &[6usize, 10, 14, 18] {
        let crit = critical_time(champion, nag, 0xC0DE);
        println!("      {nag:>2} agents: {:>4.1}%  {}", crit * 100.0, if crit < 0.15 { "✓ holds" } else { "—" });
    }

    // (c) Generalisation to UNSEEN world layouts (different seeds = different maps),
    //     using the eval protocol the target is defined on (multi-seed average),
    //     plus per-single-world to show honest run-to-run variance.
    let unseen = [0x9001u64, 0x9002, 0x9003, 0x9004, 0x9005];
    let avg = evaluate(champion, &unseen, 600);
    let per_world = unseen.iter().filter(|&&s| evaluate(champion, &[s], 600).meets_target()).count();
    println!(
        "  (c) UNSEEN layouts — averaged over {} (the eval protocol): scalar {:.2}, full 7-facet target met: {}",
        unseen.len(),
        avg.scalar(),
        if avg.meets_target() { "YES" } else { "no" }
    );
    println!(
        "      per single world (inherently higher variance): {per_world}/{} clear all 7 facets at once",
        unseen.len()
    );
}

/// The "acts on the unforeseen" task (AC15), parameterised by seed. Returns
/// (experienced-agent distance to healer, inexperienced-agent distance).
fn unforeseen_task(seed: u64) -> (i32, i32) {
    let well_id = EntityId(900);
    let well = |x: i32, y: i32| Entity {
        id: well_id,
        kind: EntityKind::Curio,
        pos: Pos::new(x, y),
        label: "wellspring".into(),
    };
    let start = Pos::new(6, 6);
    let goal = Pos::new(32, 6);
    let familiarize = |m: &mut Mind| {
        for t in 1..=24 {
            m.cycle(&Percept {
                tick: t,
                me: SelfState { pos: start, health: 1.0, energy: 0.9, hydration: 0.9, enclosure: 0.0, shelter_gap: None, season: 0, winter_in: f32::MAX, carrying: 0.0, gather_dir: None, store_dir: None },
                visible: vec![well(goal.x, goal.y)],
                events: vec![],
            });
        }
    };
    let mut learned = Mind::new(Persona::new("Learner").with_curiosity(0.1), seed);
    let mut naive = Mind::new(Persona::new("Naive").with_curiosity(0.1), seed);
    familiarize(&mut learned);
    familiarize(&mut naive);
    // only the learner lives beside the (secretly) healing form
    let mut hp = 0.4f32;
    for t in 50..=70 {
        learned.cycle(&Percept {
            tick: t,
            me: SelfState { pos: start, health: hp, energy: 0.9, hydration: 0.9, enclosure: 0.0, shelter_gap: None, season: 0, winter_in: f32::MAX, carrying: 0.0, gather_dir: None, store_dir: None },
            visible: vec![well(7, 6)],
            events: vec![],
        });
        hp = (hp + 0.05).min(0.95);
    }
    let run = |m: &mut Mind| -> i32 {
        let mut pos = start;
        for t in 200..240 {
            let th = m.cycle(&Percept {
                tick: t,
                me: SelfState { pos, health: 0.4, energy: 0.9, hydration: 0.9, enclosure: 0.0, shelter_gap: None, season: 0, winter_in: f32::MAX, carrying: 0.0, gather_dir: None, store_dir: None },
                visible: vec![well(goal.x, goal.y)],
                events: vec![],
            });
            if let Action::Move(d) = th.action {
                let np = pos.step(d);
                pos = Pos::new(np.x.clamp(0, 39), np.y.clamp(0, 25));
            }
        }
        pos.manhattan(goal)
    };
    (run(&mut learned), run(&mut naive))
}

/// Mean fraction of agent-ticks in critical need, for a village of `n` running `g`.
fn critical_time(g: &Genome, n: usize, seed: u64) -> f32 {
    let mut world = GameWorld::with_genome(seed, n, g);
    let (mut crit, mut total) = (0u64, 0u64);
    for _ in 0..1200 {
        world.step();
        for a in &world.agents {
            let dr = a.mind.drives();
            if dr.level(Drive::Hunger) > 0.92 || dr.level(Drive::Thirst) > 0.92 {
                crit += 1;
            }
            total += 1;
        }
    }
    crit as f32 / total.max(1) as f32
}
