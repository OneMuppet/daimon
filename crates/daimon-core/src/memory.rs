//! Memory: episodic, semantic, and procedural.
//!
//! Believability is mostly memory. An agent that remembers that you helped it
//! yesterday, that berries grow by the river, and that *the last time it tried
//! to fight the predator it nearly died*, reads as a mind. One that resets each
//! frame reads as a puppet.
//!
//! Daimon keeps three stores, following the standard tripartite division and
//! the "memory stream + reflection" design of Park et al.'s Generative Agents:
//!
//! * **Episodic** — a time-ordered stream of remembered events, each scored by
//!   *salience* so that important moments are recalled and trivia is forgotten.
//! * **Semantic** — durable facts distilled from experience ("river is east",
//!   "elder is friendly"). Reflection writes here.
//! * **Procedural** — a growing library of *skills*: named, reusable plans the
//!   agent has found to work, à la Voyager's skill library.

use crate::assoc::AssocMemory;
use crate::types::{EntityId, EntityKind, Pos};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::VecDeque;

/// One remembered moment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Episode {
    pub tick: u64,
    /// One-line natural-language description, the way the agent would recount it.
    pub what: String,
    /// How important this felt, in `[0, 1]`. Drives recall and forgetting.
    pub salience: f32,
    /// Emotional valence in `[-1, 1]`.
    pub valence: f32,
    /// Entity this episode is about, if any (for associative recall).
    pub subject: Option<EntityId>,
}

/// A distilled, durable fact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fact {
    pub statement: String,
    /// Confidence in `[0, 1]`; reflection can raise it as evidence accrues.
    pub confidence: f32,
    pub learned_tick: u64,
}

/// A reusable, named procedure the agent has learned to trust.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub about: String,
    /// Times invoked / times succeeded — a crude competence estimate.
    pub uses: u32,
    pub successes: u32,
}

impl Skill {
    pub fn competence(&self) -> f32 {
        if self.uses == 0 {
            0.0
        } else {
            self.successes as f32 / self.uses as f32
        }
    }
}

/// The full memory system. Capacity-bounded so a long-lived agent forgets the
/// unimportant — forgetting is a feature, not a leak.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    episodic: VecDeque<Episode>,
    episodic_cap: usize,
    semantic: BTreeMap<String, Fact>,
    procedural: BTreeMap<String, Skill>,
    /// Spatial memory: last known location (and kind) of notable entities. The
    /// agent can navigate back to remembered resources it can no longer see —
    /// the difference between an animal that forages from a mental map and one
    /// that only reacts to what is in front of it.
    places: BTreeMap<EntityId, Place>,
    /// The associative layer: Hebbian links + activation-based recall.
    assoc: AssocMemory,
}

/// A remembered location.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Place {
    pub pos: Pos,
    pub label: String,
    pub kind: EntityKind,
}

impl Default for Memory {
    fn default() -> Self {
        Self {
            episodic: VecDeque::new(),
            episodic_cap: 256,
            semantic: BTreeMap::new(),
            procedural: BTreeMap::new(),
            places: BTreeMap::new(),
            assoc: AssocMemory::default(),
        }
    }
}

impl Memory {
    pub fn new(episodic_cap: usize) -> Self {
        Self {
            episodic_cap,
            ..Default::default()
        }
    }

    // --- episodic ---------------------------------------------------------

    /// Record an episode. When over capacity, evict the *least salient* old
    /// memory rather than simply the oldest — vivid moments persist, the dull
    /// commute to the river fades.
    pub fn remember(&mut self, ep: Episode) {
        self.episodic.push_back(ep);
        while self.episodic.len() > self.episodic_cap {
            let weakest = self
                .episodic
                .iter()
                .enumerate()
                .min_by(|a, b| a.1.salience.total_cmp(&b.1.salience))
                .map(|(i, _)| i);
            if let Some(i) = weakest {
                self.episodic.remove(i);
            } else {
                break;
            }
        }
    }

    pub fn episodes(&self) -> impl Iterator<Item = &Episode> {
        self.episodic.iter()
    }

    pub fn episode_count(&self) -> usize {
        self.episodic.len()
    }

    /// Recall the `k` episodes most relevant to a query, scored the way
    /// Generative Agents scores its memory stream: a blend of **recency**,
    /// **salience**, and **relevance** (here, sharing the query subject).
    pub fn recall(&self, now: u64, subject: Option<EntityId>, k: usize) -> Vec<&Episode> {
        let mut scored: Vec<(f32, &Episode)> = self
            .episodic
            .iter()
            .map(|ep| {
                let age = now.saturating_sub(ep.tick) as f32;
                let recency = 0.99_f32.powf(age * 0.05); // exponential decay
                let relevance = match (subject, ep.subject) {
                    (Some(q), Some(s)) if q == s => 1.0,
                    (Some(_), _) => 0.0,
                    (None, _) => 0.3,
                };
                let score = 0.4 * recency + 0.4 * ep.salience + 0.2 * relevance;
                (score, ep)
            })
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        scored.into_iter().take(k).map(|(_, ep)| ep).collect()
    }

    // --- semantic ---------------------------------------------------------

    /// Assert or reinforce a fact. Re-asserting an existing fact raises its
    /// confidence (evidence accumulation) rather than duplicating it.
    pub fn learn(&mut self, key: &str, statement: &str, confidence: f32, tick: u64) {
        self.semantic
            .entry(key.to_string())
            .and_modify(|f| {
                f.confidence = (f.confidence + confidence * 0.5).min(1.0);
            })
            .or_insert(Fact {
                statement: statement.to_string(),
                confidence: confidence.clamp(0.0, 1.0),
                learned_tick: tick,
            });
    }

    pub fn fact(&self, key: &str) -> Option<&Fact> {
        self.semantic.get(key)
    }

    pub fn facts(&self) -> impl Iterator<Item = (&String, &Fact)> {
        self.semantic.iter()
    }

    // --- procedural -------------------------------------------------------

    /// Register or update a skill outcome. Skills that keep working become
    /// trusted; ones that keep failing lose competence and stop being chosen.
    pub fn record_skill(&mut self, name: &str, about: &str, success: bool) {
        let s = self
            .procedural
            .entry(name.to_string())
            .or_insert_with(|| Skill {
                name: name.to_string(),
                about: about.to_string(),
                uses: 0,
                successes: 0,
            });
        s.uses += 1;
        if success {
            s.successes += 1;
        }
    }

    pub fn skill(&self, name: &str) -> Option<&Skill> {
        self.procedural.get(name)
    }

    pub fn skills(&self) -> impl Iterator<Item = &Skill> {
        self.procedural.values()
    }

    // --- spatial ----------------------------------------------------------

    pub fn note_place(&mut self, id: EntityId, pos: Pos, label: &str, kind: EntityKind) {
        self.places.insert(
            id,
            Place {
                pos,
                label: label.to_string(),
                kind,
            },
        );
    }

    pub fn known_place(&self, id: EntityId) -> Option<(Pos, &str)> {
        self.places.get(&id).map(|p| (p.pos, p.label.as_str()))
    }

    /// The nearest remembered place of a given kind — "I'll head back to where I
    /// last saw water," even if it is long out of sight.
    pub fn nearest_place_of(&self, kind: EntityKind, from: Pos) -> Option<(EntityId, Pos)> {
        self.places
            .iter()
            .filter(|(_, p)| p.kind == kind)
            .min_by_key(|(_, p)| p.pos.manhattan(from))
            .map(|(id, p)| (*id, p.pos))
    }

    pub fn knows_place_of(&self, kind: EntityKind) -> bool {
        self.places.values().any(|p| p.kind == kind)
    }

    /// All remembered places, for reflection and introspection.
    pub fn places(&self) -> impl Iterator<Item = (EntityId, &Place)> {
        self.places.iter().map(|(id, p)| (*id, p))
    }

    // --- associative layer ------------------------------------------------

    /// Co-experience a set of concepts (entity ids / kind tokens) — builds and
    /// strengthens Hebbian links and raises their base-level activation.
    pub fn associate(&mut self, concepts: &[u32], now: u64) {
        self.assoc.present(concepts, now);
    }

    /// Association strength between two concepts.
    pub fn association(&self, a: u32, b: u32) -> f32 {
        self.assoc.association(a, b)
    }

    /// Activation of a concept given current cues (base level + spreading).
    pub fn activation(&self, id: u32, cues: &[u32], now: u64) -> f32 {
        self.assoc.activation(id, cues, now)
    }

    /// Recall the `k` concepts most brought to mind by `cues`.
    pub fn recall_assoc(&self, cues: &[u32], now: u64, k: usize) -> Vec<(u32, f32)> {
        self.assoc.recall(cues, now, k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(tick: u64, sal: f32, subj: Option<EntityId>) -> Episode {
        Episode {
            tick,
            what: format!("event at {tick}"),
            salience: sal,
            valence: 0.0,
            subject: subj,
        }
    }

    #[test]
    fn forgetting_evicts_least_salient() {
        let mut m = Memory::new(3);
        m.remember(ep(1, 0.1, None)); // dull
        m.remember(ep(2, 0.9, None)); // vivid
        m.remember(ep(3, 0.2, None));
        m.remember(ep(4, 0.5, None)); // pushes over cap -> evict the 0.1
        assert_eq!(m.episode_count(), 3);
        assert!(m.episodes().all(|e| e.salience > 0.1));
        // the vivid memory survives
        assert!(m.episodes().any(|e| (e.salience - 0.9).abs() < 1e-6));
    }

    #[test]
    fn recall_prefers_same_subject() {
        let mut m = Memory::new(10);
        let elder = EntityId(7);
        m.remember(ep(1, 0.3, Some(EntityId(1))));
        m.remember(ep(2, 0.3, Some(elder)));
        let got = m.recall(3, Some(elder), 1);
        assert_eq!(got[0].subject, Some(elder));
    }

    #[test]
    fn learning_same_fact_raises_confidence() {
        let mut m = Memory::default();
        m.learn("river", "the river is east", 0.5, 1);
        let c1 = m.fact("river").unwrap().confidence;
        m.learn("river", "the river is east", 0.5, 2);
        let c2 = m.fact("river").unwrap().confidence;
        assert!(c2 > c1);
    }

    #[test]
    fn skill_competence_tracks_success_rate() {
        let mut m = Memory::default();
        m.record_skill("forage", "find and eat food", true);
        m.record_skill("forage", "find and eat food", true);
        m.record_skill("forage", "find and eat food", false);
        let s = m.skill("forage").unwrap();
        assert!((s.competence() - 2.0 / 3.0).abs() < 1e-6);
    }
}
