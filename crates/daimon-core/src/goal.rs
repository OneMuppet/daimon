//! Goals and plans — the bridge from *wanting* to *doing*.
//!
//! A drive is a pressure; a [`Goal`] is a concrete intention adopted to relieve
//! it; a [`Plan`] is the ordered actions that pursue the goal. This is the
//! Belief–Desire–Intention loop (Bratman; Rao & Georgeff): drives are desires,
//! the world model supplies beliefs, and an adopted goal is an intention the
//! agent commits to until it succeeds, fails, or is pre-empted by something
//! more urgent.

use crate::action::Action;
use crate::drive::Drive;
use crate::types::EntityId;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GoalKind {
    /// Find and eat food.
    Forage,
    /// Find and drink water.
    Hydrate,
    /// Get away from a specific threat.
    Flee(EntityId),
    /// Stand and confront a specific threat (the other face of Survival — chosen,
    /// not scripted; effective only together).
    Confront(EntityId),
    /// Explore toward the unknown.
    Explore,
    /// Approach and talk to another agent.
    Socialize(EntityId),
    /// Inspect a curio to satisfy curiosity.
    Investigate(EntityId),
    /// Recover (rest) somewhere safe.
    Recover,
    /// Make oneself safe by enclosing: wall the open sides until sheltered. A
    /// survival goal the agent *adopts* when exposed under threat — the structure
    /// (a hut) is never scripted; it emerges from repeating this.
    Shelter,
    /// Withdraw and grieve a lost friend — the loss-oriented pole of the Dual
    /// Process Model of bereavement (Stroebe & Schut). The mind pulls back from
    /// foraging/social initiative, idles, and reminisces about the dead; it
    /// oscillates with restoration (ordinary) goals as grief decays. Adopted only
    /// when a *bonded* peer has died (the `can_grieve` gene); a stranger's death
    /// produces no Mourn goal — that asymmetry is the whole point.
    Mourn,
    /// Stock up against winter: gather a surplus of provisions while food is
    /// abundant and store it in the shared village granary, so the cache can be
    /// drawn down through the lean season. An open-world goal the agent *adopts*
    /// from Mastery + foresight when its immediate needs are met and harvest season
    /// is on (or winter is anticipated) — never scripted; provisioning emerges as
    /// the high-value choice exactly as walling-in does for Shelter. Gated by the
    /// `can_provision` gene + the world's `open_world` flag; absent both, no
    /// Provision goal is ever adopted and the world stays bit-identical.
    Provision,
}

impl GoalKind {
    pub fn label(&self) -> String {
        match self {
            GoalKind::Forage => "forage".into(),
            GoalKind::Hydrate => "find water".into(),
            GoalKind::Flee(_) => "flee".into(),
            GoalKind::Confront(_) => "confront".into(),
            GoalKind::Explore => "explore".into(),
            GoalKind::Socialize(_) => "socialize".into(),
            GoalKind::Investigate(_) => "investigate".into(),
            GoalKind::Recover => "recover".into(),
            GoalKind::Shelter => "shelter".into(),
            GoalKind::Mourn => "mourn".into(),
            GoalKind::Provision => "provision".into(),
        }
    }
}

/// An adopted intention.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Goal {
    pub kind: GoalKind,
    /// The drive this goal serves (for arbitration and narration).
    pub origin: Drive,
    /// Priority at adoption time, in `[0, 1]`.
    pub priority: f32,
}

/// A committed sequence of actions pursuing a goal. Kept short and re-planned
/// often: long rigid plans are brittle in a changing world (the river dries up,
/// the predator moves). Re-planning cheaply every few ticks beats planning once
/// perfectly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub goal: Goal,
    pub steps: VecDeque<Action>,
    /// Tick at which this plan was formed (for staleness checks).
    pub formed: u64,
}

impl Plan {
    pub fn new(goal: Goal, steps: impl IntoIterator<Item = Action>, formed: u64) -> Self {
        Self {
            goal,
            steps: steps.into_iter().collect(),
            formed,
        }
    }

    /// Re-aim an existing plan at `goal` with `steps`, reusing the already-allocated
    /// step deque (clear + extend) so a hot-path re-plan touches the heap only if the
    /// deque must grow. Behaviour-equivalent to building a fresh [`Plan`].
    pub fn refill(&mut self, goal: Goal, steps: impl IntoIterator<Item = Action>, formed: u64) {
        self.goal = goal;
        self.formed = formed;
        self.steps.clear();
        self.steps.extend(steps);
    }

    pub fn is_done(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn advance(&mut self) -> Option<Action> {
        self.steps.pop_front()
    }

    pub fn peek(&self) -> Option<&Action> {
        self.steps.front()
    }
}
