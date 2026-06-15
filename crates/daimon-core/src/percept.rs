//! What the agent senses each tick, and what it can do back.

use crate::types::{Entity, EntityId, EntityKind, Pos, SelfState};
use serde::{Deserialize, Serialize};

/// The *content* of an utterance — what one agent actually tells another.
/// Dialogue carries information, not just sentiment, so hearing it can change
/// what the listener does (the difference between an NPC reacting to words and
/// one merely emoting at you).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Info {
    /// A friendly opener.
    Greeting,
    /// "There's water/food at (x,y)" — a sharable resource location.
    ResourceAt {
        id: EntityId,
        kind: EntityKind,
        pos: Pos,
        label: String,
    },
    /// "Mind the ground around (x,y)" — a warning about a dangerous place.
    DangerAt { pos: Pos },
}

/// Something that happened in the world this tick, delivered to the agent's
/// senses. Events are the raw material of episodic memory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WorldEvent {
    /// The agent ate `id`, gaining `energy`.
    Ate { id: EntityId, energy: f32 },
    /// The agent drank from `id`.
    Drank { id: EntityId },
    /// The agent was hurt by `id` (a predator), losing `health`.
    Hurt { id: EntityId, health: f32 },
    /// A threat the agent struck at was driven off this tick — the outcome signal
    /// from which an agent *learns* whether confronting works.
    Repelled { id: EntityId },
    /// Another agent said something to us (sentiment only).
    Heard { from: EntityId, text: String },
    /// Another agent told us something with *content* we can act on.
    Told { from: EntityId, info: Info },
    /// We greeted / spoke to another agent.
    Spoke { to: EntityId, text: String },
    /// A new entity entered the agent's field of view for the first time.
    Discovered { id: EntityId },
    /// An entity the agent believed was here is now gone.
    Vanished { id: EntityId },
}

/// One frame of sensory input. The world produces it; perception consumes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Percept {
    pub tick: u64,
    pub me: SelfState,
    /// Entities currently within sight radius.
    pub visible: Vec<Entity>,
    /// Events resolved this tick.
    pub events: Vec<WorldEvent>,
}
