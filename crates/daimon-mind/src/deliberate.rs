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
use daimon_core::{Drive, DriveSystem, EntityId, EntityKind, GoalKind, Memory, WorldModel};

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

/// A candidate's justification, stored *unrendered* (no String) so only the
/// winning candidate's reason is ever formatted — the loser reasons cost nothing.
/// Each variant carries exactly the data its sentence interpolates; rendering one
/// reproduces, byte-for-byte, the text the eager `format!`s used to build.
#[derive(Debug, Clone, Copy)]
enum Reason {
    Flee { dist: f32 },
    Hunger { pct: f32 },
    Thirst { pct: f32 },
    /// `who` is resolved from `ctx` only when this is the winner (see `render`),
    /// so non-winning curio/agent reasons clone no label.
    Investigate { id: EntityId },
    SocializeLiked { id: EntityId },
    SocializeWary { id: EntityId },
    Explore,
    Recover,
}

impl Reason {
    /// Render the (winning) reason to the exact text the eager `format!`s produced.
    /// Label-bearing variants resolve their name from `ctx` here — the same source
    /// and fallback order as before — so the rationale is byte-identical.
    fn render(&self, ctx: &DeliberationContext) -> String {
        match self {
            Reason::Flee { dist } => {
                format!("a predator is {dist:.0} steps away; I've learned not to gamble with that")
            }
            Reason::Hunger { pct } => format!("hunger is at {:.0}%", pct * 100.0),
            Reason::Thirst { pct } => format!("thirst is at {:.0}%", pct * 100.0),
            Reason::Investigate { id } => {
                let label = ctx
                    .world
                    .belief(*id)
                    .map(|b| b.entity.label.as_str())
                    .unwrap_or("");
                format!("that {label} is unlike anything I've catalogued — I want to know what it is")
            }
            Reason::SocializeLiked { id } => {
                format!("{} is nearby and I think well of them", who(ctx, *id))
            }
            Reason::SocializeWary { id } => {
                format!("{} is nearby, though I'm wary of them", who(ctx, *id))
            }
            Reason::Explore => "there's still ground I haven't seen".to_string(),
            Reason::Recover => "I'm exhausted and, for now, safe enough to rest".to_string(),
        }
    }
}

/// The name we'd use for agent `id`: their modelled name, else their world label —
/// the same resolution (and fallback order) the socialize candidate used inline.
fn who(ctx: &DeliberationContext, id: EntityId) -> String {
    ctx.social
        .model(id)
        .map(|m| m.name.clone())
        .unwrap_or_else(|| {
            ctx.world
                .belief(id)
                .map(|b| b.entity.label.clone())
                .unwrap_or_default()
        })
}

/// One candidate goal under consideration, with a computed utility and an
/// unrendered reason — the unit the deliberator argmaxes over.
#[derive(Debug, Clone)]
struct Candidate {
    goal: GoalKind,
    utility: f32,
    reason: Reason,
}

/// An offline, deterministic stand-in for a reasoning LLM.
///
/// It scores every plausible goal by `drive pressure × feasibility`, folds in
/// lessons from memory (a Reflexion-style "I remember this going badly"), and
/// picks the best — then explains itself. The explanation is what makes even
/// this toy reasoner read as deliberate rather than reflexive.
#[derive(Debug, Default, Clone)]
pub struct HeuristicDeliberator {
    /// Reused candidate buffer: cleared and refilled each deliberation so the slow
    /// path reuses its backing allocation instead of allocating a fresh Vec per
    /// call. Not part of the verdict; never serialised.
    cands: Vec<Candidate>,
}

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
        // reuse the held candidate buffer (take/restore keeps `&mut self` happy and
        // preserves the backing allocation across deliberations).
        let mut cands = std::mem::take(&mut self.cands);
        cands.clear();

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
                reason: Reason::Flee { dist },
            });
        }

        // --- forage ---------------------------------------------------------
        {
            let hunger = ctx.drives.level(Drive::Hunger);
            let feas = resource_feasibility(ctx, EntityKind::Food, pos);
            cands.push(Candidate {
                goal: GoalKind::Forage,
                utility: hunger * Drive::Hunger.salience_weight() * feas * ctx.drives.bias(Drive::Hunger),
                reason: Reason::Hunger { pct: hunger },
            });
        }

        // --- hydrate --------------------------------------------------------
        {
            let thirst = ctx.drives.level(Drive::Thirst);
            let feas = resource_feasibility(ctx, EntityKind::Water, pos);
            cands.push(Candidate {
                goal: GoalKind::Hydrate,
                utility: thirst * Drive::Thirst.salience_weight() * feas * ctx.drives.bias(Drive::Thirst),
                reason: Reason::Thirst { pct: thirst },
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
                reason: Reason::Investigate { id: curio.entity.id },
            });
        }

        // --- socialize ------------------------------------------------------
        if let Some(other) = ctx.world.first_visible_of(EntityKind::Agent) {
            let disp = ctx.social.disposition(other.id);
            let social = ctx.drives.level(Drive::Social) * ctx.persona.sociability;
            // we approach those we like; dislike suppresses the urge.
            let u = (social + disp * 0.5).max(0.0)
                * Drive::Social.salience_weight()
                * ctx.drives.bias(Drive::Social);
            cands.push(Candidate {
                goal: GoalKind::Socialize(other.id),
                utility: u,
                reason: if disp >= 0.0 {
                    Reason::SocializeLiked { id: other.id }
                } else {
                    Reason::SocializeWary { id: other.id }
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
                reason: Reason::Explore,
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
                reason: Reason::Recover,
            });
        }

        // argmax — `max_by` over the indexed candidates returns the last of any
        // equal-utility run, identical to the previous `into_iter().max_by(..)`.
        let best_i = cands
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.utility.total_cmp(&b.utility))
            .map(|(i, _)| i)
            .expect("explore is always a candidate");
        let best_goal = cands[best_i].goal.clone();
        let best_reason = cands[best_i].reason; // Copy; only the winner is rendered

        let mut lessons = Vec::new();
        if matches!(best_goal, GoalKind::Flee(_)) {
            lessons.push(Lesson {
                key: "predator".into(),
                statement: "predators are dangerous; keep distance".into(),
                confidence: 0.6,
            });
        }

        let label = best_goal.label();
        let rationale = format!("I weighed my options: {}. I'll {label}.", best_reason.render(ctx));
        // return the (now-empty after restore) buffer to the deliberator for reuse.
        cands.clear();
        self.cands = cands;
        Deliberation {
            goal: best_goal,
            rationale,
            lessons,
        }
    }
}
