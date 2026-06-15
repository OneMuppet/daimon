//! Theory of mind — modelling *other* minds.
//!
//! A character that treats other agents as moving furniture never feels social.
//! Daimon keeps a lightweight model of every other agent it meets: where it
//! last saw them, how it has come to feel about them (disposition), and its
//! best guess at what they are trying to do. This is the small, tractable
//! cousin of Rabinowitz et al.'s *Machine Theory of Mind* (ToMnet): infer
//! another agent's mental state from observed behaviour, and act on the
//! inference. Disposition updates from interactions, so relationships have
//! history — the agent who shared food with you is greeted differently from the
//! one who ignored you.

use daimon_core::{Drive, Entity, EntityId, Pos};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentModel {
    pub id: EntityId,
    pub name: String,
    /// How we feel about them, in `[-1, 1]`. Starts mildly positive.
    pub disposition: f32,
    /// Our best guess at what they're up to (a short label).
    pub believed_goal: Option<String>,
    /// Inferred from observed behaviour: which drive seems to be steering them.
    pub believed_drive: Option<Drive>,
    /// Tick at which `believed_drive` was last (re)inferred — for freshness.
    pub believed_tick: u64,
    /// A tentative read awaiting confirmation (we don't trust a single glance).
    pub prelim_drive: Option<Drive>,
    /// Where we last saw them — lets us read their *movement*, not just position.
    pub last_pos: Option<Pos>,
    pub last_seen: u64,
    pub interactions: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TheoryOfMind {
    models: BTreeMap<EntityId, AgentModel>,
}

impl TheoryOfMind {
    /// Register or refresh a sighting of another agent.
    pub fn observe(&mut self, e: &Entity, tick: u64) {
        let label = e.label.clone();
        self.models
            .entry(e.id)
            .and_modify(|m| {
                m.last_seen = tick;
                m.last_pos = Some(e.pos);
            })
            .or_insert(AgentModel {
                id: e.id,
                name: if label.is_empty() {
                    format!("stranger-{}", e.id.0)
                } else {
                    label
                },
                disposition: 0.15,
                believed_goal: None,
                believed_drive: None,
                believed_tick: 0,
                prelim_drive: None,
                last_pos: Some(e.pos),
                last_seen: tick,
                interactions: 0,
            });
    }

    pub fn last_pos(&self, id: EntityId) -> Option<Pos> {
        self.models.get(&id).and_then(|m| m.last_pos)
    }

    /// Infer, from what an agent is doing/near, which drive seems to steer them —
    /// the tractable cousin of reading another mind from behaviour alone.
    pub fn infer_drive(&mut self, id: EntityId, drive: Drive, tick: u64) {
        if let Some(m) = self.models.get_mut(&id) {
            m.believed_drive = Some(drive);
            m.believed_goal = Some(drive.name().to_string());
            m.believed_tick = tick;
            m.last_seen = tick;
        }
    }

    /// Consider a behavioural read of `id`'s drive. We only *commit* to a belief
    /// when two consecutive reads agree — a single glance toward the river while
    /// wandering shouldn't convince us they're thirsty.
    pub fn consider_drive(&mut self, id: EntityId, drive: Drive, tick: u64) {
        if let Some(m) = self.models.get_mut(&id) {
            if m.prelim_drive == Some(drive) {
                m.believed_drive = Some(drive);
                m.believed_goal = Some(drive.name().to_string());
                m.believed_tick = tick;
            } else {
                m.prelim_drive = Some(drive);
            }
            m.last_seen = tick;
        }
    }

    pub fn believed_drive(&self, id: EntityId) -> Option<Drive> {
        self.models.get(&id).and_then(|m| m.believed_drive)
    }

    /// The believed drive only if it was (re)inferred on `tick` — i.e. a *fresh*
    /// read, not a stale leftover. Used to measure inference quality fairly.
    pub fn believed_fresh(&self, id: EntityId, tick: u64) -> Option<Drive> {
        self.models
            .get(&id)
            .filter(|m| m.believed_tick == tick)
            .and_then(|m| m.believed_drive)
    }

    /// Update from being spoken to. Warm words warm the relationship.
    pub fn heard(&mut self, from: EntityId, text: &str, tick: u64) {
        if let Some(m) = self.models.get_mut(&from) {
            m.last_seen = tick;
            m.interactions += 1;
            let warmth = sentiment(text);
            m.disposition = (m.disposition + warmth * 0.3).clamp(-1.0, 1.0);
            m.believed_goal = Some("being social".to_string());
        }
    }

    /// Record that we initiated an interaction (builds rapport slightly).
    pub fn spoke_to(&mut self, to: EntityId) {
        if let Some(m) = self.models.get_mut(&to) {
            m.interactions += 1;
            m.disposition = (m.disposition + 0.05).clamp(-1.0, 1.0);
        }
    }

    pub fn model(&self, id: EntityId) -> Option<&AgentModel> {
        self.models.get(&id)
    }

    pub fn disposition(&self, id: EntityId) -> f32 {
        self.models.get(&id).map(|m| m.disposition).unwrap_or(0.0)
    }

    pub fn known(&self) -> impl Iterator<Item = &AgentModel> {
        self.models.values()
    }

    /// The agent we feel warmest toward, for "I'd like to find a friend" logic.
    pub fn friendliest(&self) -> Option<&AgentModel> {
        self.models
            .values()
            .max_by(|a, b| a.disposition.total_cmp(&b.disposition))
    }
}

/// A deliberately crude bag-of-words sentiment, in `[-1, 1]`. In a real Daimon
/// this is exactly the kind of judgement the System-2 language model makes far
/// better; here it keeps the demo self-contained and offline.
fn sentiment(text: &str) -> f32 {
    let t = text.to_lowercase();
    let warm = ["hello", "friend", "share", "help", "thanks", "welcome", "together"];
    let cold = ["go away", "mine", "leave", "back off", "enemy", "no"];
    let mut s: f32 = 0.0;
    for w in warm {
        if t.contains(w) {
            s += 0.5;
        }
    }
    for w in cold {
        if t.contains(w) {
            s -= 0.6;
        }
    }
    s.clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use daimon_core::{EntityKind, Pos};

    fn agent(id: u32, name: &str) -> Entity {
        Entity {
            id: EntityId(id),
            kind: EntityKind::Agent,
            pos: Pos::new(0, 0),
            label: name.to_string(),
        }
    }

    #[test]
    fn warm_words_improve_disposition() {
        let mut tom = TheoryOfMind::default();
        tom.observe(&agent(1, "elder"), 1);
        let before = tom.disposition(EntityId(1));
        tom.heard(EntityId(1), "hello friend, let's share", 2);
        assert!(tom.disposition(EntityId(1)) > before);
    }

    #[test]
    fn cold_words_sour_disposition() {
        let mut tom = TheoryOfMind::default();
        tom.observe(&agent(1, "rival"), 1);
        tom.heard(EntityId(1), "go away, this is mine", 2);
        assert!(tom.disposition(EntityId(1)) < 0.15);
    }
}
