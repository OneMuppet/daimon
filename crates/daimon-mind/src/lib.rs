//! # daimon-mind
//!
//! The cognitive engine of **Daimon**. Where [`daimon-core`](../daimon_core)
//! defines the *nouns* of cognition, this crate supplies the *verbs*: the
//! seven-step cognitive cycle ([`mind::Mind::cycle`]) that turns a stream of
//! percepts into a stream of actions and legible thoughts.
//!
//! The design in one paragraph: a Daimon is a **BDI agent** (beliefs from
//! perception, desires from a homeostatic + intrinsic **drive system**,
//! intentions committed as plans) running a **dual-process** controller. Cheap
//! System-1 machinery — reflexes and utility arbitration — handles routine
//! life. An explicit, rate-limited **escalation policy** hands the hard, novel,
//! or high-stakes moments to a pluggable System-2 **[`Deliberator`]** (a
//! heuristic here; a language model in production). **Memory** persists across
//! the whole life — episodic, semantic, procedural — and a periodic
//! **reflection** pass consolidates experience into knowledge. A lightweight
//! **theory of mind** models the other agents it meets. Everything it does is
//! narrated, so the architecture is visible from the outside.
//!
//! ## Modules
//!
//! | Module | Role |
//! |---|---|
//! | [`mind`] | The cognitive cycle and escalation policy — the spine. |
//! | [`deliberate`] | The System-2 seam: the [`Deliberator`] trait + offline default. |
//! | [`planner`] | Goal → short, re-plannable action sequence (GOAP/HTN-style). |
//! | [`theory_of_mind`] | Models of other agents; relationships with history. |
//! | [`persona`] | Stable trait biases — one engine, many characters. |
//! | [`thought`] | Externalised inner monologue; the dual process made visible. |

pub mod affect;
pub mod anticipation;
pub mod crit;
pub mod deliberate;
pub mod entangle;
pub mod evolve;
pub mod learn;
pub mod stigmergy;
pub mod dialogue;
pub mod imagine;
pub mod language;
pub mod llm;
pub mod mind;
pub mod overlay;
pub mod persona;
pub mod planner;
pub mod praxis;
pub mod project;
pub mod qcog;
pub mod reciprocity;
pub mod theory_of_mind;
pub mod thought;

pub use affect::Affect;
pub use anticipation::Anticipation;
pub use crit::{dynamic_range, CriticalNet};
pub use entangle::Entangled;
pub use evolve::{Evolution, Fitness, Genome, Verdict};
pub use learn::LearningProgress;
pub use stigmergy::DoubleBridge;
pub use reciprocity::Strategy;
pub use deliberate::{Deliberation, DeliberationContext, Deliberator, HeuristicDeliberator, Lesson};
pub use imagine::ForwardModel;
pub use llm::{LlmDeliberator, Transport};
pub use mind::{Metrics, Mind, MindConfig};
pub use persona::Persona;
pub use praxis::{Concept, Praxis};
pub use qcog::{QMind, C};
pub use project::{Project, ProjectKind};
pub use theory_of_mind::{AgentModel, TheoryOfMind};
pub use thought::{Process, Thought};
