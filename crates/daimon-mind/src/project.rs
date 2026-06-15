//! Projects — goals that outlast a single need.
//!
//! Homeostasis explains why an agent eats; it does not explain why it would
//! catalogue every strange object in the valley, or keep circling back to a
//! friend. Believable minds carry *projects*: long-horizon intentions pursued in
//! the gaps between urgent needs, with progress that accumulates over many ticks
//! and a real sense of completion. A persona picks a project that fits it, and
//! it colours the agent's free time for the rest of its life.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectKind {
    /// Inspect every distinct curio in the world.
    ExploreEverything,
    /// Eat well, repeatedly — build a habit of provisioning.
    Provision,
    /// Spend time close to a friend.
    Companionship,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub kind: ProjectKind,
    pub progress: u32,
    pub target: u32,
    pub started: u64,
    pub completed: Option<u64>,
    /// distinct curios inspected (for ExploreEverything).
    inspected: BTreeSet<u32>,
}

impl Project {
    pub fn new(kind: ProjectKind, target: u32, now: u64) -> Self {
        Self {
            kind,
            progress: 0,
            target: target.max(1),
            started: now,
            completed: None,
            inspected: BTreeSet::new(),
        }
    }

    pub fn fraction(&self) -> f32 {
        (self.progress as f32 / self.target as f32).clamp(0.0, 1.0)
    }

    pub fn is_done(&self) -> bool {
        self.completed.is_some()
    }

    /// Record a unit of progress (for ExploreEverything, dedup by curio id).
    pub fn advance(&mut self, curio: Option<u32>, now: u64) {
        match (self.kind, curio) {
            (ProjectKind::ExploreEverything, Some(id)) => {
                self.inspected.insert(id);
                self.progress = self.inspected.len() as u32;
            }
            (ProjectKind::ExploreEverything, None) => {}
            _ => self.progress += 1,
        }
        if self.completed.is_none() && self.progress >= self.target {
            self.completed = Some(now);
        }
    }

    pub fn label(&self) -> &'static str {
        match self.kind {
            ProjectKind::ExploreEverything => "catalogue every curio",
            ProjectKind::Provision => "provision well",
            ProjectKind::Companionship => "stay close to a friend",
        }
    }
}
