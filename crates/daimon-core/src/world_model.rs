//! The world model — the agent's *beliefs* about its surroundings.
//!
//! Perception is partial and fleeting; cognition needs persistence. The world
//! model is the agent's best current guess about what exists and where, updated
//! from each percept and decaying in confidence when out of sight (object
//! permanence with doubt). The mind plans against *this*, not against ground
//! truth — which is why a Daimon can be wrong, surprised, and mistaken, all of
//! which are prerequisites for looking like it is actually thinking.

use crate::percept::Percept;
use crate::types::{Entity, EntityId, EntityKind, Pos, SelfState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A belief about one entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Belief {
    pub entity: Entity,
    /// Tick this belief was last confirmed by perception.
    pub last_seen: u64,
    /// Confidence the entity is still where we think, in `[0, 1]`.
    pub confidence: f32,
    /// True while currently in view.
    pub visible: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorldModel {
    beliefs: BTreeMap<EntityId, Belief>,
    me: Option<SelfState>,
    tick: u64,
}

impl WorldModel {
    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn me(&self) -> Option<SelfState> {
        self.me
    }

    /// Fold a fresh percept into beliefs. Returns the ids of entities seen for
    /// the *first* time, which the caller turns into "Discovered" novelty.
    pub fn integrate(&mut self, p: &Percept) -> Vec<EntityId> {
        self.tick = p.tick;
        self.me = Some(p.me);

        // mark everything not-visible; visible ones get re-marked below.
        for b in self.beliefs.values_mut() {
            b.visible = false;
        }

        let mut newly = Vec::new();
        for e in &p.visible {
            let is_new = !self.beliefs.contains_key(&e.id);
            if is_new {
                newly.push(e.id);
            }
            self.beliefs.insert(
                e.id,
                Belief {
                    entity: e.clone(),
                    last_seen: p.tick,
                    confidence: 1.0,
                    visible: true,
                },
            );
        }

        // Out-of-sight beliefs lose confidence; forget the truly stale.
        let now = p.tick;
        self.beliefs.retain(|_, b| {
            if !b.visible {
                let age = now.saturating_sub(b.last_seen) as f32;
                b.confidence = 0.98_f32.powf(age);
            }
            b.confidence > 0.05
        });

        newly
    }

    pub fn belief(&self, id: EntityId) -> Option<&Belief> {
        self.beliefs.get(&id)
    }

    pub fn beliefs(&self) -> impl Iterator<Item = &Belief> {
        self.beliefs.values()
    }

    /// Currently-visible entities of a given kind.
    pub fn visible_of(&self, kind: EntityKind) -> Vec<&Entity> {
        self.beliefs
            .values()
            .filter(|b| b.visible && b.entity.kind == kind)
            .map(|b| &b.entity)
            .collect()
    }

    /// Nearest believed entity of a kind (visible or remembered), by distance
    /// from `from`, weighted by confidence so the agent prefers a sure bet.
    pub fn nearest_of(&self, kind: EntityKind, from: Pos) -> Option<&Belief> {
        self.beliefs
            .values()
            .filter(|b| b.entity.kind == kind)
            .min_by(|a, b| {
                let da = a.entity.pos.manhattan(from) as f32 / a.confidence.max(0.05);
                let db = b.entity.pos.manhattan(from) as f32 / b.confidence.max(0.05);
                da.total_cmp(&db)
            })
    }

    /// The closest visible predator, if any — the safety system's input.
    pub fn nearest_threat(&self, from: Pos) -> Option<&Belief> {
        self.beliefs
            .values()
            .filter(|b| b.visible && b.entity.kind == EntityKind::Predator)
            .min_by_key(|b| b.entity.pos.manhattan(from))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(id: u32, kind: EntityKind, x: i32, y: i32) -> Entity {
        Entity {
            id: EntityId(id),
            kind,
            pos: Pos::new(x, y),
            label: String::new(),
        }
    }

    fn percept(tick: u64, visible: Vec<Entity>) -> Percept {
        Percept {
            tick,
            me: SelfState::new(Pos::new(0, 0)),
            visible,
            events: vec![],
        }
    }

    #[test]
    fn first_sight_is_reported_as_novel() {
        let mut wm = WorldModel::default();
        let newly = wm.integrate(&percept(1, vec![entity(1, EntityKind::Food, 2, 0)]));
        assert_eq!(newly, vec![EntityId(1)]);
        // seeing it again is not novel
        let again = wm.integrate(&percept(2, vec![entity(1, EntityKind::Food, 2, 0)]));
        assert!(again.is_empty());
    }

    #[test]
    fn out_of_sight_decays_then_is_forgotten() {
        let mut wm = WorldModel::default();
        wm.integrate(&percept(1, vec![entity(1, EntityKind::Curio, 5, 5)]));
        // never seen again for a long time
        for t in 2..400 {
            wm.integrate(&percept(t, vec![]));
        }
        assert!(wm.belief(EntityId(1)).is_none());
    }

    #[test]
    fn nearest_threat_picks_closest_visible_predator() {
        let mut wm = WorldModel::default();
        wm.integrate(&percept(
            1,
            vec![
                entity(1, EntityKind::Predator, 10, 0),
                entity(2, EntityKind::Predator, 3, 0),
            ],
        ));
        let t = wm.nearest_threat(Pos::new(0, 0)).unwrap();
        assert_eq!(t.entity.id, EntityId(2));
    }
}
