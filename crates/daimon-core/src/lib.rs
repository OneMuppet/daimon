//! # daimon-core
//!
//! The cognitive type system for **Daimon** — a concept architecture for game
//! agents that feel less like NPCs and more like minds.
//!
//! This crate is deliberately *mechanism-free*: it defines the nouns of
//! cognition (percepts, beliefs, drives, memories, goals, plans, actions) but
//! not the verbs. The thinking happens in [`daimon-mind`](../daimon_mind);
//! the world the agent is thrown into lives in
//! [`daimon-world`](../daimon_world). Keeping the data model in its own crate,
//! with only `serde` as a dependency, means a Daimon's entire mental state is a
//! plain serialisable value you can snapshot, diff, replay, and inspect.
//!
//! ## The pieces
//!
//! | Module | Cognitive role |
//! |---|---|
//! | [`rng`] | Determinism: one seed → one reproducible life. |
//! | [`types`] | Embodiment: space, entities, the body's own state. |
//! | [`percept`] | Sensation: one frame of input, plus events. |
//! | [`world_model`] | Belief: a persistent, fallible model of the world. |
//! | [`drive`] | Motivation: homeostatic + intrinsic needs that *want*. |
//! | [`memory`] | Episodic, semantic, and procedural memory. |
//! | [`goal`] | Intention: goals and the plans that pursue them. |
//! | [`action`] | The bounded surface through which the mind touches the world. |

pub mod action;
pub mod assoc;
pub mod drive;
pub mod goal;
pub mod memory;
pub mod percept;
pub mod rng;
pub mod serdeutil;
pub mod types;
pub mod world_model;

pub use action::Action;
pub use drive::{Drive, DriveSystem};
pub use goal::{Goal, GoalKind, Plan};
pub use memory::{Episode, Fact, Memory, Skill};
pub use percept::{Info, Percept, WorldEvent};
pub use rng::Rng;
pub use types::{Dir, Entity, EntityId, EntityKind, Pos, SelfState};
pub use world_model::{Belief, WorldModel};
