//! The believability **fitness function** for the self-improvement pipeline.
//!
//! This is the bridge that turns the believability harness into an optimisation
//! objective. It expresses a cognitive [`Genome`] into a full [`GameWorld`], runs
//! the world for a budget of ticks, and measures five facets of a believable life
//! — survival, safety, decision balance, expressive variety, and exploration —
//! each in `[0,1]` with real headroom and real trade-offs between them. Averaging
//! over several seeds makes the score robust, not a lucky run.
//!
//! Crucially, the *same physics* that the manual harness judges is the physics
//! the search optimises against: there is one arbiter of truth, and now it grades
//! a machine that is improving itself.

use std::collections::{HashMap, HashSet};

use daimon_core::Drive;
use daimon_mind::{Fitness, Genome};

use crate::sim::GameWorld;

/// Evaluate a genome by living several lives in the real world and measuring
/// them. `ticks` controls depth (≈600 for a real generation, less for fast
/// gating); `seeds` controls robustness (more seeds = less variance).
pub fn evaluate(genome: &Genome, seeds: &[u64], ticks: u64) -> Fitness {
    evaluate_in(genome, seeds, ticks, false)
}

/// As [`evaluate`], but in the **harsh** world (scarce water, relentless stalker)
/// — the regime that gives the self-improvement search a real gradient to climb.
pub fn evaluate_harsh(genome: &Genome, seeds: &[u64], ticks: u64) -> Fitness {
    evaluate_in(genome, seeds, ticks, true)
}

fn evaluate_in(genome: &Genome, seeds: &[u64], ticks: u64, harsh: bool) -> Fitness {
    let mut acc = Fitness::default();
    for &seed in seeds {
        let f = evaluate_once(genome, seed, ticks, harsh);
        acc.survival += f.survival;
        acc.safety += f.safety;
        acc.balance += f.balance;
        acc.expression += f.expression;
        acc.exploration += f.exploration;
        acc.emotion += f.emotion;
        acc.knowledge += f.knowledge;
    }
    let n = seeds.len().max(1) as f32;
    Fitness {
        survival: acc.survival / n,
        safety: acc.safety / n,
        balance: acc.balance / n,
        expression: acc.expression / n,
        exploration: acc.exploration / n,
        emotion: acc.emotion / n,
        knowledge: acc.knowledge / n,
    }
}

fn evaluate_once(genome: &Genome, seed: u64, ticks: u64, harsh: bool) -> Fitness {
    let mut world = if harsh {
        GameWorld::with_genome_harsh(seed, 6, genome)
    } else {
        GameWorld::with_genome(seed, 6, genome)
    };
    let n = world.agents.len().max(1);

    // per-agent accumulators
    let mut crit_ticks = vec![0u32; n]; // ticks in critical starvation/thirst
    let mut danger_ticks = vec![0u32; n]; // ticks within the stalker's reach
    let mut drive_counts = vec![[0u32; 6]; n]; // dominant-drive histogram
    let mut visited: Vec<HashSet<(i32, i32)>> = vec![HashSet::new(); n];
    // emotional range — does the agent *feel* a varied life, or stay flat?
    let mut v_min = vec![f32::MAX; n];
    let mut v_max = vec![f32::MIN; n];
    let mut a_min = vec![f32::MAX; n];
    let mut a_max = vec![f32::MIN; n];

    let di = |d: Drive| Drive::ALL.iter().position(|&x| x == d).unwrap();

    for _ in 0..ticks {
        world.step();
        let pred = world.predator.pos;
        for (i, a) in world.agents.iter().enumerate() {
            let dr = a.mind.drives();
            if dr.level(Drive::Hunger) > 0.92 || dr.level(Drive::Thirst) > 0.92 {
                crit_ticks[i] += 1;
            }
            if a.body.pos.manhattan(pred) <= 2 {
                danger_ticks[i] += 1;
            }
            drive_counts[i][di(dr.dominant().0)] += 1;
            visited[i].insert((a.body.pos.x, a.body.pos.y));
            let af = a.mind.affect();
            v_min[i] = v_min[i].min(af.valence);
            v_max[i] = v_max[i].max(af.valence);
            a_min[i] = a_min[i].min(af.arousal);
            a_max[i] = a_max[i].max(af.arousal);
        }
    }

    let t = ticks.max(1) as f32;
    // SURVIVAL: little time starving. (squared so chronic starvation is punished.)
    let survival = mean(&crit_ticks, n, |c| {
        let frac = c as f32 / t;
        (1.0 - frac).clamp(0.0, 1.0).powi(2)
    });
    // SAFETY: rarely within the stalker's reach.
    let safety = mean(&danger_ticks, n, |c| (1.0 - (c as f32 / t) * 4.0).clamp(0.0, 1.0));
    // BALANCE: high normalised entropy over which drive leads — no fixation.
    let balance = (0..n).map(|i| norm_entropy(&drive_counts[i])).sum::<f32>() / n as f32;
    // EXPLORATION: ground covered (≈80 distinct tiles is a strong explorer) plus
    // discoveries made.
    let coverage = (0..n).map(|i| (visited[i].len() as f32 / 80.0).min(1.0)).sum::<f32>() / n as f32;
    let discov = world.agents.iter().map(|a| (a.mind.metrics().discoveries as f32 / 8.0).min(1.0)).sum::<f32>()
        / n as f32;
    let exploration = 0.7 * coverage + 0.3 * discov;

    // EXPRESSION: society-wide dialogue variety (distinctness + speech-act range
    // + that it actually speaks).
    let spoken = &world.spoken;
    let total = spoken.len().max(1) as f32;
    let distinct = spoken.iter().map(|(_, txt)| txt).collect::<HashSet<_>>().len() as f32;
    let acts = spoken.iter().map(|(a, _)| *a).collect::<HashSet<_>>().len() as f32;
    let mut freq: HashMap<&String, u32> = HashMap::new();
    for (_, txt) in spoken {
        *freq.entry(txt).or_insert(0) += 1;
    }
    let top_share = freq.values().copied().max().unwrap_or(0) as f32 / total;
    let diversity = (distinct / total).clamp(0.0, 1.0);
    let act_range = (acts / 5.0).min(1.0);
    let volume = (spoken.len() as f32 / (t * 0.04)).min(1.0); // some talk, not silence
    let expression =
        (0.45 * diversity + 0.30 * act_range + 0.15 * volume + 0.10 * (1.0 - top_share)).clamp(0.0, 1.0);

    // EMOTION: a responsive, varied emotional life (valence range over [-1,1]
    // and arousal range over [0,1]) — a flat agent scores ~0, one that feels
    // content, afraid, weary at the right times scores high.
    let emotion = (0..n)
        .map(|i| {
            let vr = (v_max[i] - v_min[i]).max(0.0) / 2.0;
            let ar = (a_max[i] - a_min[i]).max(0.0);
            (0.5 * vr + 0.5 * ar).clamp(0.0, 1.0)
        })
        .sum::<f32>()
        / n as f32;

    // KNOWLEDGE: learned competence (forward-model accuracy — lp-curiosity serves
    // this) + breadth of understood affordances (concepts learned, incl. from
    // peers — cultural transmission serves this).
    let knowledge = world
        .agents
        .iter()
        .map(|a| {
            let competence = 1.0 - a.mind.prediction_error();
            let breadth = a.mind.praxis().concepts.iter().filter(|c| c.engagements >= 3).count();
            0.6 * competence + 0.4 * (breadth as f32 / 4.0).min(1.0)
        })
        .sum::<f32>()
        / n as f32;

    Fitness { survival, safety, balance, expression, exploration, emotion, knowledge }
}

fn mean(counts: &[u32], n: usize, f: impl Fn(u32) -> f32) -> f32 {
    counts.iter().take(n).map(|&c| f(c)).sum::<f32>() / n as f32
}

/// Shannon entropy of a 6-bin histogram, normalised to `[0,1]` by `ln(6)`.
fn norm_entropy(counts: &[u32; 6]) -> f32 {
    let total: u32 = counts.iter().sum();
    if total == 0 {
        return 0.0;
    }
    let total = total as f32;
    let h: f32 = counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f32 / total;
            -p * p.ln()
        })
        .sum();
    h / 6f32.ln()
}
