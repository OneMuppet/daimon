//! Anticipation — a small *learned* model of what the agent expects to see, so
//! that **surprise is genuine prediction error**, not a hand-set constant.
//!
//! The mind predicts the salient shape of the next percept from what it has
//! learned: which places it has visited, which entities it has seen before, and
//! how often a predator is around. Surprise is how badly that prediction missed.
//! Crucially it *learns down*: the first time you see the humming stone it is
//! astonishing; the fiftieth time it is wallpaper. That decay is what makes
//! curiosity point at the genuinely novel instead of flickering at everything.

use daimon_core::{Percept, Pos, WorldEvent};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Coarsen a position into a region so "familiarity of place" generalises a bit
/// rather than being per-cell.
fn region(p: Pos) -> (i32, i32) {
    (p.x / 3, p.y / 3)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Anticipation {
    /// Times each entity has been seen (entity familiarity).
    seen: BTreeMap<u32, u32>,
    /// Times each region has been observed from (place familiarity).
    #[serde(with = "daimon_core::serdeutil::vecmap")]
    visits: BTreeMap<(i32, i32), u32>,
    /// Exponentially-weighted estimate that a predator is in view on any tick.
    predator_rate: f32,
    ticks: u64,
    last: f32,
    /// Running mean/variance of surprise (Welford) for "spike above mean+σ".
    mean: f32,
    m2: f32,
}

impl Anticipation {
    pub fn last(&self) -> f32 {
        self.last
    }
    pub fn mean(&self) -> f32 {
        self.mean
    }
    pub fn std(&self) -> f32 {
        if self.ticks < 2 {
            0.0
        } else {
            (self.m2 / (self.ticks as f32 - 1.0)).max(0.0).sqrt()
        }
    }
    /// How novel entity `id` is right now, in (0, 1]: 1 when never seen.
    pub fn entity_novelty(&self, id: u32) -> f32 {
        1.0 / (1.0 + *self.seen.get(&id).unwrap_or(&0) as f32)
    }

    /// Predict, score the miss as surprise, then learn. Returns surprise in [0,1].
    pub fn observe(&mut self, p: &Percept) -> f32 {
        // --- predict & score (before learning from this percept) ---
        let place = self.visits.get(&region(p.me.pos)).copied().unwrap_or(0);
        let place_novelty = (-(place as f32) * 0.25).exp(); // 1 when never here

        // average novelty of what's in view (drives the learn-down behaviour)
        let entity_term = if p.visible.is_empty() {
            0.0
        } else {
            let sum: f32 = p.visible.iter().map(|e| self.entity_novelty(e.id.0)).sum();
            sum / p.visible.len() as f32
        };

        // a predator in view when we rarely see one is alarming
        let predator_here = p
            .visible
            .iter()
            .any(|e| matches!(e.kind, daimon_core::EntityKind::Predator));
        let predator_term = if predator_here {
            1.0 - self.predator_rate
        } else {
            0.0
        };

        // events carry their own surprise
        let mut event_term = 0.0_f32;
        for ev in &p.events {
            event_term += match ev {
                WorldEvent::Hurt { .. } => 0.85,
                WorldEvent::Heard { .. } => 0.2,
                WorldEvent::Vanished { .. } => 0.25,
                _ => 0.0,
            };
        }

        let surprise = (0.22 * place_novelty
            + 0.42 * entity_term
            + 0.5 * predator_term
            + event_term)
            .clamp(0.0, 1.0);

        // --- learn ---
        *self.visits.entry(region(p.me.pos)).or_insert(0) += 1;
        for e in &p.visible {
            *self.seen.entry(e.id.0).or_insert(0) += 1;
        }
        let target = if predator_here { 1.0 } else { 0.0 };
        self.predator_rate += (target - self.predator_rate) * 0.05;

        // --- stats (Welford) ---
        self.ticks += 1;
        let d = surprise - self.mean;
        self.mean += d / self.ticks as f32;
        self.m2 += d * (surprise - self.mean);
        self.last = surprise;

        surprise
    }

    /// Is `s` a spike (above the running mean + one standard deviation)?
    pub fn is_spike(&self, s: f32) -> bool {
        s > self.mean + self.std()
    }

    /// Entities the agent currently considers familiar (seen ≥ `n`).
    pub fn familiar(&self, n: u32) -> BTreeSet<u32> {
        self.seen
            .iter()
            .filter(|(_, c)| **c >= n)
            .map(|(id, _)| *id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daimon_core::{Entity, EntityId, EntityKind, SelfState};

    fn percept(tick: u64, pos: Pos, vis: Vec<Entity>, events: Vec<WorldEvent>) -> Percept {
        Percept { tick, me: SelfState::new(pos), visible: vis, events }
    }
    fn ent(id: u32, kind: EntityKind, x: i32, y: i32) -> Entity {
        Entity { id: EntityId(id), kind, pos: Pos::new(x, y), label: "x".into() }
    }

    #[test]
    fn novelty_learns_down() {
        let mut a = Anticipation::default();
        // first sighting of a curio at a fresh spot
        let first = a.observe(&percept(1, Pos::new(20, 20), vec![ent(1, EntityKind::Curio, 20, 21)], vec![]));
        // see the same curio many more times
        let mut later = first;
        for t in 2..=8 {
            later = a.observe(&percept(t, Pos::new(20, 20), vec![ent(1, EntityKind::Curio, 20, 21)], vec![]));
        }
        assert!(first >= 3.0 * later, "first {first} should be >= 3x later {later}");
    }

    #[test]
    fn unexpected_predator_spikes() {
        let mut a = Anticipation::default();
        // long peaceful stretch wandering, no predator
        for t in 1..40 {
            let x = 5 + (t % 7) as i32;
            a.observe(&percept(t, Pos::new(x, 5), vec![ent(2, EntityKind::Food, x + 1, 5)], vec![]));
        }
        let mean_before = a.mean();
        let s = a.observe(&percept(40, Pos::new(9, 5), vec![ent(9, EntityKind::Predator, 10, 5)], vec![]));
        assert!(s > mean_before, "predator surprise {s} should exceed prior mean {mean_before}");
    }
}
