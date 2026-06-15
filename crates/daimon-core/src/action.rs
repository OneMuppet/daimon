//! The agent's bounded action interface.
//!
//! A Daimon can only affect the world through this enum. This is the cognitive
//! analogue of a robot's actuators — and, like Reins' bounded action surface,
//! it is the place where an autonomous mind's blast radius is *defined by
//! construction*. The world validates and resolves each action; the mind never
//! touches world state directly.

use crate::types::{Dir, EntityId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    /// Move one cell in a cardinal direction.
    Move(Dir),
    /// Eat an adjacent (or co-located) Food entity.
    Eat(EntityId),
    /// Drink from an adjacent Water entity.
    Drink(EntityId),
    /// Speak to another agent (theory-of-mind interaction).
    Talk { to: EntityId, text: String },
    /// Inspect a Curio up close — the curiosity-satisfying action.
    Inspect(EntityId),
    /// Strike at an adjacent threat. Alone it does little (and invites harm); its
    /// power is *collective* — the agents are given this tool, not told to use it.
    Strike(EntityId),
    /// Stand still and recover a little energy.
    Rest,
    /// Do nothing this tick (used when deliberation is pending).
    Wait,
}

impl Action {
    /// A short human-readable verb for narration logs.
    pub fn verb(&self) -> &'static str {
        match self {
            Action::Move(_) => "move",
            Action::Eat(_) => "eat",
            Action::Drink(_) => "drink",
            Action::Talk { .. } => "talk",
            Action::Inspect(_) => "inspect",
            Action::Strike(_) => "strike",
            Action::Rest => "rest",
            Action::Wait => "wait",
        }
    }

    /// Whether this action *mutates* the world (consumes food, harms, etc.) as
    /// opposed to being purely locomotive/observational. A read-only circuit
    /// breaker — borrowed from Reins — would clamp a misbehaving Daimon to the
    /// non-mutating subset.
    pub fn is_mutating(&self) -> bool {
        matches!(
            self,
            Action::Eat(_)
                | Action::Drink(_)
                | Action::Talk { .. }
                | Action::Inspect(_)
                | Action::Strike(_)
        )
    }
}
