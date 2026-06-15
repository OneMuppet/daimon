//! Associative memory — the brain-like layer.
//!
//! Two ideas from cognitive science, kept small and cheap:
//!
//! * **Hebbian association** ("cells that fire together wire together"): things
//!   experienced *together* become linked, and the link strengthens with every
//!   co-occurrence. Later, encountering one thing brings the others to mind.
//! * **Base-level activation** (ACT-R; Anderson): how available a memory is to
//!   recall depends on how *often* and how *recently* it has been used, decaying
//!   as a power-of-time. Retrieval ranks by activation = base level + the
//!   **spreading activation** that flows in from whatever is currently cueing
//!   us.
//!
//! Concepts are identified by a `u32` (entity ids, plus a few reserved tokens
//! for kinds). This is deliberately a thin, testable core — not a neural net —
//! but it gives the agent genuine *association* and *cue-driven retrieval*
//! instead of a flat list scan.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Reserved concept ids for entity *kinds* (so "predator" can be associated with
/// a specific berry bush). Kept well above any real entity id.
pub mod concept {
    pub const PREDATOR: u32 = 1_000_000;
    pub const FOOD: u32 = 1_000_001;
    pub const WATER: u32 = 1_000_002;
    pub const CURIO: u32 = 1_000_003;
    pub const AGENT: u32 = 1_000_004;
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Node {
    /// Recency/frequency-decayed strength; base level = ln(strength).
    strength: f32,
    last: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssocMemory {
    nodes: BTreeMap<u32, Node>,
    #[serde(with = "crate::serdeutil::vecmap")]
    edges: BTreeMap<(u32, u32), f32>,
    /// Per-tick base-level retention (recency decay).
    decay: f32,
    /// Hebbian increment per co-occurrence.
    learn: f32,
    /// Weight on spreading activation from cues during recall.
    spread: f32,
}

impl Default for AssocMemory {
    fn default() -> Self {
        Self {
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
            decay: 0.985,
            learn: 0.5,
            spread: 1.0,
        }
    }
}

fn key(a: u32, b: u32) -> (u32, u32) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

impl AssocMemory {
    /// Co-experience: bump each concept's base level and strengthen every pairwise
    /// link between them. This is the only way associations form.
    pub fn present(&mut self, concepts: &[u32], now: u64) {
        for &c in concepts {
            let n = self.nodes.entry(c).or_default();
            let dt = now.saturating_sub(n.last) as f32;
            n.strength = n.strength * self.decay.powf(dt) + 1.0;
            n.last = now;
        }
        for i in 0..concepts.len() {
            for j in (i + 1)..concepts.len() {
                if concepts[i] == concepts[j] {
                    continue;
                }
                *self.edges.entry(key(concepts[i], concepts[j])).or_insert(0.0) += self.learn;
            }
        }
    }

    /// Base-level activation of a concept = ln(decayed strength).
    pub fn base_level(&self, id: u32, now: u64) -> f32 {
        match self.nodes.get(&id) {
            Some(n) => {
                let dt = now.saturating_sub(n.last) as f32;
                let s = n.strength * self.decay.powf(dt);
                if s > 1e-4 {
                    s.ln()
                } else {
                    f32::NEG_INFINITY
                }
            }
            None => f32::NEG_INFINITY,
        }
    }

    /// Raw association strength between two concepts.
    pub fn association(&self, a: u32, b: u32) -> f32 {
        self.edges.get(&key(a, b)).copied().unwrap_or(0.0)
    }

    /// Activation of `id` given a set of current cues: base level + spreading.
    pub fn activation(&self, id: u32, cues: &[u32], now: u64) -> f32 {
        let mut a = self.base_level(id, now);
        if a == f32::NEG_INFINITY {
            a = -10.0; // allow pure-spread recall of never-directly-stored links
        }
        for &c in cues {
            a += self.spread * self.association(c, id);
        }
        a
    }

    /// Recall the `k` concepts most activated by `cues` (excluding the cues).
    pub fn recall(&self, cues: &[u32], now: u64, k: usize) -> Vec<(u32, f32)> {
        let mut scored: Vec<(u32, f32)> = self
            .nodes
            .keys()
            .filter(|id| !cues.contains(id))
            .map(|&id| (id, self.activation(id, cues, now)))
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored.truncate(k);
        scored
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cooccurrence_builds_association_and_recall() {
        let mut m = AssocMemory::default();
        let (pred, x, y) = (concept::PREDATOR, 5u32, 6u32);
        // predator co-occurs with X repeatedly; Y is only ever seen alone.
        for t in 1..=10 {
            m.present(&[pred, x], t * 2);
            m.present(&[y], t * 2 + 1);
        }
        assert!(m.association(pred, x) > m.association(pred, y));
        // cueing the predator should bring X to mind over Y.
        let recalled = m.recall(&[pred], 30, 3);
        let rx = recalled.iter().find(|(id, _)| *id == x).map(|(_, a)| *a).unwrap();
        let ry = recalled.iter().find(|(id, _)| *id == y).map(|(_, a)| *a).unwrap();
        assert!(rx > ry, "X activation {rx} should exceed Y {ry} when cued by predator");
    }

    #[test]
    fn base_level_rewards_frequency_recency_and_decays() {
        let mut m = AssocMemory::default();
        // A: seen often and recently. B: seen once, long ago.
        m.present(&[2], 1);
        for t in 90..=100 {
            m.present(&[1], t);
        }
        assert!(m.base_level(1, 100) > m.base_level(2, 100));
        // after a long unseen gap, A's activation falls toward a fresh single hit.
        let a_old = m.base_level(1, 100);
        let a_decayed = m.base_level(1, 100 + 400);
        assert!(a_decayed < a_old);
    }
}
