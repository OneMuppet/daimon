//! The deliberator — Daimon's System 2.
//!
//! This is the architecture's most important seam. Routine life runs on cheap,
//! reflexive System-1 machinery (the rest of this crate). But when the agent is
//! *surprised*, in *danger*, or facing a genuinely *ambiguous* choice, it stops
//! and thinks — and "thinking" is an arbitrary, expensive reasoner behind the
//! [`Deliberator`] trait.
//!
//! The demo ships [`HeuristicDeliberator`]: a transparent utility calculation
//! that is fully offline and deterministic, so a Daimon's life is reproducible
//! and this repository builds with zero network. But the trait is the point.
//! A production Daimon implements `Deliberator` with a call to a large language
//! model — the same interface, the same context in, a goal and a *rationale*
//! out:
//!
//! ```ignore
//! struct LlmDeliberator { client: Anthropic, model: &'static str }
//!
//! impl Deliberator for LlmDeliberator {
//!     fn deliberate(&mut self, ctx: &DeliberationContext) -> Deliberation {
//!         // Render ctx (drives, salient memories, beliefs, social models)
//!         // into a prompt; ask for a goal + chain-of-thought rationale;
//!         // parse the structured reply. ReAct / Reflexion / Tree-of-Thoughts
//!         // all slot in here unchanged.
//!     }
//! }
//! ```
//!
//! Because the slow path is rate-limited by the escalation policy (see
//! [`crate::mind`]), a real deployment makes only a handful of model calls per
//! agent per minute — the difference between a tech demo and something you can
//! afford to run for a thousand NPCs.

use crate::persona::Persona;
use crate::theory_of_mind::TheoryOfMind;
use daimon_core::{Drive, DriveSystem, EntityKind, GoalKind, Memory, WorldModel};

/// Everything the deliberator is allowed to look at. A faithful, serialisable
/// snapshot of the agent's situation — this is exactly what you would render
/// into a language-model prompt.
pub struct DeliberationContext<'a> {
    pub tick: u64,
    pub persona: &'a Persona,
    pub drives: &'a DriveSystem,
    pub world: &'a WorldModel,
    pub memory: &'a Memory,
    pub social: &'a TheoryOfMind,
    /// How much this situation violated expectations, in `[0, 1]`.
    pub surprise: f32,
}

/// A lesson the deliberator wants written to semantic memory.
pub struct Lesson {
    pub key: String,
    pub statement: String,
    pub confidence: f32,
}

/// The deliberator's verdict: a goal to adopt, why, and anything learned.
pub struct Deliberation {
    pub goal: GoalKind,
    /// First-person justification, surfaced in narration. With an LLM this is
    /// the chain of thought; here it is templated from the winning utility.
    pub rationale: String,
    pub lessons: Vec<Lesson>,
}

/// The System-2 interface. Implement this with an LLM for a "real" Daimon.
pub trait Deliberator {
    fn deliberate(&mut self, ctx: &DeliberationContext) -> Deliberation;
    fn name(&self) -> &'static str;
}

/// How attainable a resource goal is right now: high if we can see it, still
/// solidly worth pursuing if we *remember* where it is, low (but non-zero —
/// we can always go looking) if we know of none.
fn resource_feasibility(ctx: &DeliberationContext, kind: EntityKind, pos: daimon_core::Pos) -> f32 {
    if let Some(b) = ctx.world.nearest_of(kind, pos) {
        0.4 + b.confidence * 0.6
    } else if ctx.memory.knows_place_of(kind) {
        0.7 // out of sight, but on the mental map
    } else {
        0.2 // go explore and find some
    }
}

/// One candidate goal under consideration, with a computed utility and a human
/// reason — the unit the deliberator argmaxes over.
struct Candidate {
    goal: GoalKind,
    utility: f32,
    reason: String,
}

/// An offline, deterministic stand-in for a reasoning LLM.
///
/// It scores every plausible goal by `drive pressure × feasibility`, folds in
/// lessons from memory (a Reflexion-style "I remember this going badly"), and
/// picks the best — then explains itself. The explanation is what makes even
/// this toy reasoner read as deliberate rather than reflexive.
#[derive(Debug, Default, Clone)]
pub struct HeuristicDeliberator;

impl Deliberator for HeuristicDeliberator {
    fn name(&self) -> &'static str {
        "heuristic"
    }

    fn deliberate(&mut self, ctx: &DeliberationContext) -> Deliberation {
        let me = match ctx.world.me() {
            Some(m) => m,
            None => {
                return Deliberation {
                    goal: GoalKind::Explore,
                    rationale: "I can't sense myself — I'll move and reorient.".into(),
                    lessons: vec![],
                }
            }
        };
        let pos = me.pos;
        let mut cands: Vec<Candidate> = Vec::new();

        // --- survival / flee ------------------------------------------------
        if let Some(threat) = ctx.world.nearest_threat(pos) {
            let dist = threat.entity.pos.manhattan(pos).max(1) as f32;
            // closer threat → higher utility, amplified by the survival drive
            // and a Reflexion-style memory that predators are dangerous.
            let lesson_gain = if ctx.memory.fact("predator").is_some() {
                0.3
            } else {
                0.0
            };
            let u = (ctx.drives.level(Drive::Survival) * 2.0 + 1.5 / dist + lesson_gain)
                * Drive::Survival.salience_weight();
            cands.push(Candidate {
                goal: GoalKind::Flee(threat.entity.id),
                utility: u,
                reason: format!(
                    "a predator is {dist:.0} steps away; I've learned not to gamble with that"
                ),
            });
        }

        // --- forage ---------------------------------------------------------
        {
            let hunger = ctx.drives.level(Drive::Hunger);
            let feas = resource_feasibility(ctx, EntityKind::Food, pos);
            cands.push(Candidate {
                goal: GoalKind::Forage,
                utility: hunger * Drive::Hunger.salience_weight() * feas * ctx.drives.bias(Drive::Hunger),
                reason: format!("hunger is at {:.0}%", hunger * 100.0),
            });
        }

        // --- hydrate --------------------------------------------------------
        {
            let thirst = ctx.drives.level(Drive::Thirst);
            let feas = resource_feasibility(ctx, EntityKind::Water, pos);
            cands.push(Candidate {
                goal: GoalKind::Hydrate,
                utility: thirst * Drive::Thirst.salience_weight() * feas * ctx.drives.bias(Drive::Thirst),
                reason: format!("thirst is at {:.0}%", thirst * 100.0),
            });
        }

        // --- investigate a curio (intrinsic curiosity) ----------------------
        if let Some(curio) = ctx.world.nearest_of(EntityKind::Curio, pos) {
            let curiosity = ctx.drives.level(Drive::Curiosity) * ctx.persona.curiosity * 2.0;
            // surprise sharpens curiosity — novelty is the reward signal.
            let u = (curiosity + ctx.surprise * 0.5)
                * Drive::Curiosity.salience_weight()
                * ctx.drives.bias(Drive::Curiosity);
            cands.push(Candidate {
                goal: GoalKind::Investigate(curio.entity.id),
                utility: u,
                reason: format!(
                    "that {} is unlike anything I've catalogued — I want to know what it is",
                    curio.entity.label
                ),
            });
        }

        // --- socialize ------------------------------------------------------
        if let Some(other) = ctx.world.visible_of(EntityKind::Agent).into_iter().next() {
            let disp = ctx.social.disposition(other.id);
            let social = ctx.drives.level(Drive::Social) * ctx.persona.sociability;
            // we approach those we like; dislike suppresses the urge.
            let u = (social + disp * 0.5).max(0.0)
                * Drive::Social.salience_weight()
                * ctx.drives.bias(Drive::Social);
            let who = ctx
                .social
                .model(other.id)
                .map(|m| m.name.clone())
                .unwrap_or_else(|| other.label.clone());
            cands.push(Candidate {
                goal: GoalKind::Socialize(other.id),
                utility: u,
                reason: if disp >= 0.0 {
                    format!("{who} is nearby and I think well of them")
                } else {
                    format!("{who} is nearby, though I'm wary of them")
                },
            });
        }

        // --- explore (the always-available fallback) ------------------------
        {
            let curiosity = ctx.drives.level(Drive::Curiosity) * ctx.persona.curiosity;
            cands.push(Candidate {
                goal: GoalKind::Explore,
                utility: (curiosity * 0.8 + ctx.surprise * 0.3)
                    * Drive::Curiosity.salience_weight()
                    * ctx.drives.bias(Drive::Curiosity),
                reason: "there's still ground I haven't seen".into(),
            });
        }

        // --- recover --------------------------------------------------------
        // Rest only makes sense when safe AND not acutely hungry or thirsty —
        // otherwise resting is just a slower way to die.
        let safe = ctx.world.nearest_threat(pos).is_none();
        let needs_pressing =
            ctx.drives.level(Drive::Thirst) > 0.5 || ctx.drives.level(Drive::Hunger) > 0.5;
        if me.energy < 0.25 && safe && !needs_pressing {
            cands.push(Candidate {
                goal: GoalKind::Recover,
                utility: (1.0 - me.energy) * 1.2,
                reason: "I'm exhausted and, for now, safe enough to rest".into(),
            });
        }

        // argmax — deterministic tie-break by insertion order.
        let best = cands
            .into_iter()
            .max_by(|a, b| a.utility.total_cmp(&b.utility))
            .expect("explore is always a candidate");

        let mut lessons = Vec::new();
        if matches!(best.goal, GoalKind::Flee(_)) {
            lessons.push(Lesson {
                key: "predator".into(),
                statement: "predators are dangerous; keep distance".into(),
                confidence: 0.6,
            });
        }

        let label = best.goal.label();
        Deliberation {
            goal: best.goal,
            rationale: format!("I weighed my options: {}. I'll {}.", best.reason, label),
            lessons,
        }
    }
}
