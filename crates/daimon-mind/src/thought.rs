//! Expression — the agent's externalised inner monologue.
//!
//! The single biggest contributor to an agent *feeling* intelligent is not the
//! quality of its decisions but the **legibility** of them. A Daimon narrates
//! itself: every cognitive cycle emits a [`Thought`] that says which need is
//! driving it, what it decided, and — crucially — *which mode of thinking it
//! used*. A player reading "the predator startled me — I'm not sticking around
//! to find out what it wants" is looking straight into the architecture.

use daimon_core::{Action, Drive, GoalKind};
use serde::{Deserialize, Serialize};

/// Which cognitive pathway produced this cycle's decision. This is the
/// dual-process distinction made observable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Process {
    /// System 1, hard-wired: a reflex that pre-empts all deliberation
    /// (e.g. a predator at arm's reach).
    Reflex,
    /// System 1, learned: fast utility arbitration over the obvious choice.
    Routine,
    /// System 2: the slow, expensive deliberator was consulted.
    Deliberate,
}

impl Process {
    pub fn tag(self) -> &'static str {
        match self {
            Process::Reflex => "REFLEX",
            Process::Routine => "fast",
            Process::Deliberate => "SLOW",
        }
    }
}

/// One externalised moment of cognition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thought {
    pub tick: u64,
    pub process: Process,
    pub dominant_drive: Drive,
    pub goal: GoalKind,
    /// The chosen action this tick.
    pub action: Action,
}
