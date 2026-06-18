//! # Daimon SDK — autonomous minds for any game
//!
//! Daimon gives a game character a *mind*: it forms its own goals from felt
//! drives (hunger, thirst, safety, social, curiosity), learns what objects are
//! for by touching them, remembers, fears death, grieves a bonded peer, and can
//! wall itself in or provision for winter — none of it scripted. It is
//! **deterministic** (same seed → same life: replays, lockstep multiplayer, and
//! reproducible debugging come free), **CPU-only**, and **pure Rust** — no model
//! weights, no GPU, no network, no per-frame inference cost.
//!
//! This crate is the thin, stable layer you embed. The research engine lives in
//! [`daimon_core`] and [`daimon_mind`]; you should rarely need them directly.
//!
//! ## The whole contract, in one call
//!
//! Each tick you give an [`Agent`] what it senses and it hands back a
//! [`Thought`] — the [`Action`] it chose, plus its current goal, dominant drive,
//! and a first-person line you can show the player.
//!
//! ```text
//!            your game world                         daimon
//!   ┌──────────────────────────────┐        ┌────────────────────────┐
//!   │  where is this NPC, what is   │  (1)   │                        │
//!   │  around it, what just         ├───────▶│   Agent::think(...)    │
//!   │  happened to it?              │        │                        │
//!   │                               │  (2)   │   → Thought { action,  │
//!   │  carry out thought.action     │◀───────┤       goal, inner, … } │
//!   └──────────────────────────────┘        └────────────────────────┘
//! ```
//!
//! You write exactly **two mappings**: your world → what the agent senses, and
//! the chosen [`Action`] → effects in your world. Everything else is done.
//!
//! ## Minimal loop (low-level)
//!
//! ```
//! use daimon_sdk::prelude::*;
//!
//! // Spawn an agent: an id (your handle), a personality, and a seed.
//! let mut agent = Agent::new(EntityId(1), Persona::new("Mara").with_curiosity(0.9), 42);
//!
//! // Each tick, describe the agent's body and what it can see, then think.
//! let body = SelfState::new(Pos::new(5, 5));            // pos + full vitals
//! let visible = vec![Entity {                           // what's in sight
//!     id: EntityId(2), kind: EntityKind::Food,
//!     pos: Pos::new(6, 5), label: "berry".into(),
//! }];
//! let thought = agent.think(body, visible);
//!
//! // Carry out what it decided, in your own world.
//! match thought.action {
//!     Action::Move(dir) => { /* move the NPC one cell */ let _ = dir; }
//!     Action::Eat(id)   => { /* consume entity `id` if adjacent */ let _ = id; }
//!     _ => {}
//! }
//!
//! // Optional, but cheap and lovely: surface the inner monologue.
//! println!("{}: {}", agent.name(), thought.inner);
//! ```
//!
//! ## Driver loop (let the SDK orchestrate)
//!
//! Implement [`Senses`] + [`Actuator`] on your world and call [`step`]; it builds
//! the percept, advances the agent, applies the action, and feeds the resulting
//! [`WorldEvent`]s back so the mind can learn from them. See `examples/minimal.rs`
//! for a complete, runnable world built only on this crate.
//!
//! ## What you get to tune
//!
//! - [`Persona`] — per-character trait biases (boldness, sociability, curiosity,
//!   a creed). One engine, a whole cast — not a clone army.
//! - [`Genome`] — which faculties exist and how strongly. [`Agent::from_genome`]
//!   lets you ship a genome you bred offline with the evolution tools. Most
//!   advanced faculties are **off by default**; opt in deliberately.
//!
//! ## Honest scope
//!
//! Daimon is built for *simulation-shaped* games — colony / survival / immersive
//! / social sims and roguelikes — where believable autonomous agents are the
//! point. It is **not** an LLM (dialogue is templated, not free-form language),
//! it does no raycasting or vision (you hand it a curated list of what's
//! visible), and the world it reasons over is a discrete grid of cells. It is not
//! a drop-in for, say, a shooter's combat AI.

#![forbid(unsafe_code)]

pub use daimon_core::{
    Action, Dir, Drive, Entity, EntityId, EntityKind, Goal, GoalKind, Info, Percept, Plan, Pos,
    SelfState, WorldEvent,
};
pub use daimon_mind::{Genome, Mind, Persona, Process, Thought};

/// A game character with a Daimon mind.
///
/// Wraps the cognitive engine and owns the small amount of per-tick bookkeeping
/// you would otherwise do by hand — the percept tick counter and the queue of
/// world events the mind has not yet been told about. Spawn one per NPC.
pub struct Agent {
    /// Your stable handle for this character. Daimon never invents ids; it only
    /// ever refers back to ones you gave it (in [`SelfState`]/[`Entity`]).
    pub id: EntityId,
    name: String,
    mind: Mind,
    tick: u64,
    inbox: Vec<WorldEvent>,
}

impl Agent {
    /// Spawn an agent from a [`Persona`] and a deterministic `seed`.
    ///
    /// The same `(persona, seed)` always yields the same life given the same
    /// percepts — that determinism is the point, so pick seeds intentionally.
    pub fn new(id: EntityId, persona: Persona, seed: u64) -> Self {
        let name = persona.name.clone();
        Self { id, name, mind: Mind::new(persona, seed), tick: 0, inbox: Vec::new() }
    }

    /// Spawn from an evolved [`Genome`] — e.g. a champion you bred offline and
    /// want to ship. The genome decides which faculties this mind has; the
    /// persona colours its temperament on top.
    pub fn from_genome(id: EntityId, genome: &Genome, persona: Persona, seed: u64) -> Self {
        let name = persona.name.clone();
        Self { id, name, mind: genome.express(&persona, seed), tick: 0, inbox: Vec::new() }
    }

    /// The character's name (from its persona).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Tell the mind something that happened to it this tick that did **not**
    /// come from its own action — it was hurt by a predator, another agent spoke
    /// to it, a bonded peer died, and so on. Queued and delivered on the next
    /// [`think`](Agent::think). (Events produced by the agent's *own* action are
    /// fed back for you automatically when you use [`step`].)
    pub fn observe(&mut self, event: WorldEvent) {
        self.inbox.push(event);
    }

    /// Advance the mind one tick.
    ///
    /// `body` is the agent's own state (position + vitals — see [`SelfState`]);
    /// `visible` is every [`Entity`] within its sight this tick (you decide the
    /// radius and what counts). Returns the [`Thought`] it produced — most
    /// importantly [`Thought::action`], which you carry out in your world.
    ///
    /// Any pending [`observe`](Agent::observe)d events are delivered as part of
    /// this call and then cleared.
    pub fn think(&mut self, body: SelfState, visible: Vec<Entity>) -> Thought {
        let percept = Percept {
            tick: self.tick,
            me: body,
            visible,
            events: std::mem::take(&mut self.inbox),
        };
        self.tick = self.tick.wrapping_add(1);
        self.mind.cycle(&percept)
    }

    /// Serialise the mind to JSON for save games. Drain any in-flight events
    /// (i.e. call this right after a [`think`](Agent::think), not mid-tick) so
    /// nothing is lost; the cosmetic percept counter resets to 0 on
    /// [`load`](Agent::load), but the mind's own episodic clock is preserved.
    pub fn save(&self) -> String {
        self.mind.to_json()
    }

    /// Restore an agent previously [`save`](Agent::save)d. You re-supply the `id`
    /// and `name` (those are your concern, not the mind's). Returns `None` if the
    /// JSON is not a valid saved mind.
    pub fn load(id: EntityId, name: &str, json: &str) -> Option<Self> {
        Some(Self {
            id,
            name: name.to_string(),
            mind: Mind::from_json(json)?,
            tick: 0,
            inbox: Vec::new(),
        })
    }

    /// Escape hatch: borrow the underlying [`Mind`] for advanced faculties not
    /// surfaced here (cultural learning, stigmergy, contention hints, …). You
    /// rarely need this; reach for it only when the ergonomic surface above is
    /// not enough.
    pub fn mind(&self) -> &Mind {
        &self.mind
    }

    /// Mutable [`mind`](Agent::mind) escape hatch.
    pub fn mind_mut(&mut self) -> &mut Mind {
        &mut self.mind
    }
}

/// Mapping #1: your world → what an agent senses.
///
/// Implement this on whatever owns your game state. Daimon asks, per agent and
/// per tick, "where/how is this character, and what can it see?"
pub trait Senses {
    /// The agent's own body this tick: position and vitals. Build it with
    /// [`SelfState::new`] and set the fields you model (health/energy/hydration,
    /// and — only if you use them — enclosure, season, provisions).
    fn body(&self, agent: EntityId) -> SelfState;

    /// Every [`Entity`] the agent can perceive this tick. You choose the sight
    /// radius and what is occluded; exclude the agent itself.
    fn visible(&self, agent: EntityId) -> Vec<Entity>;
}

/// Mapping #2: a chosen [`Action`] → effects in your world.
///
/// Implement this on whatever owns your game state. Return any [`WorldEvent`]s
/// the action produced (e.g. [`WorldEvent::Ate`] when the agent successfully
/// ate) so the mind hears the outcome and learns from it; return an empty `Vec`
/// when the action had no perceivable result. World-driven events not caused by
/// this action (a predator's strike, a peer's death) go through
/// [`Agent::observe`] instead.
pub trait Actuator {
    /// Carry out `action` for `agent`, returning the events it produced.
    fn apply(&mut self, agent: EntityId, action: &Action) -> Vec<WorldEvent>;
}

/// Advance one agent by one tick against a world that can both [`Senses::body`]
/// it and [`Actuator::apply`] its action — the whole perceive → think → act →
/// learn loop in one call. Returns the [`Thought`] for UI/debug.
///
/// Drive a population by calling this for each agent each tick (you own the
/// iteration order; with a fixed order it stays deterministic).
pub fn step<W>(agent: &mut Agent, world: &mut W) -> Thought
where
    W: Senses + Actuator + ?Sized,
{
    let body = world.body(agent.id);
    let visible = world.visible(agent.id);
    let thought = agent.think(body, visible);
    for ev in world.apply(agent.id, &thought.action) {
        agent.observe(ev);
    }
    thought
}

/// The handful of types you actually touch when embedding Daimon.
///
/// ```
/// use daimon_sdk::prelude::*;
/// let agent = Agent::new(EntityId(1), Persona::new("Kael"), 7);
/// assert_eq!(agent.name(), "Kael");
/// ```
pub mod prelude {
    pub use crate::{step, Actuator, Agent, Senses};
    pub use daimon_core::{
        Action, Dir, Drive, Entity, EntityId, EntityKind, Goal, GoalKind, Info, Percept, Plan, Pos,
        SelfState, WorldEvent,
    };
    pub use daimon_mind::{Genome, Persona, Process, Thought};
}

#[cfg(test)]
mod tests {
    use super::prelude::*;

    #[test]
    fn agent_thinks_and_is_deterministic() {
        // Same (persona, seed) + same percepts → identical action stream.
        let run = || {
            let mut a = Agent::new(EntityId(1), Persona::new("Echo").with_curiosity(0.8), 99);
            let mut verbs = Vec::new();
            for _ in 0..20 {
                let t = a.think(SelfState::new(Pos::new(5, 5)), Vec::new());
                verbs.push(t.action.verb());
            }
            verbs
        };
        assert_eq!(run(), run(), "same seed + same percepts must reproduce");
    }

    #[test]
    fn save_load_round_trips() {
        let mut a = Agent::new(EntityId(3), Persona::new("Vell"), 5);
        a.think(SelfState::new(Pos::new(2, 2)), Vec::new());
        let json = a.save();
        let restored = Agent::load(EntityId(3), "Vell", &json);
        assert!(restored.is_some(), "a saved mind must reload");
        assert_eq!(restored.unwrap().name(), "Vell");
    }

    #[test]
    fn observed_events_are_delivered_then_cleared() {
        // An observed event is folded in on the next think and not replayed after.
        let mut a = Agent::new(EntityId(4), Persona::new("Sela"), 1);
        a.observe(WorldEvent::Hurt { id: EntityId(9), health: 0.2 });
        // Should not panic and should consume the inbox.
        let _ = a.think(SelfState::new(Pos::new(0, 0)), Vec::new());
        let _ = a.think(SelfState::new(Pos::new(0, 0)), Vec::new());
    }

    struct TinyWorld {
        pos: Pos,
    }
    impl Senses for TinyWorld {
        fn body(&self, _a: EntityId) -> SelfState {
            SelfState::new(self.pos)
        }
        fn visible(&self, _a: EntityId) -> Vec<Entity> {
            Vec::new()
        }
    }
    impl Actuator for TinyWorld {
        fn apply(&mut self, _a: EntityId, action: &Action) -> Vec<WorldEvent> {
            if let Action::Move(d) = action {
                self.pos = self.pos.step(*d);
            }
            Vec::new()
        }
    }

    #[test]
    fn step_drives_a_world() {
        let mut world = TinyWorld { pos: Pos::new(5, 5) };
        let mut a = Agent::new(EntityId(1), Persona::new("Roin").with_curiosity(0.95), 3);
        for _ in 0..30 {
            step(&mut a, &mut world);
        }
        // A curious agent with nothing to do should have wandered off its start.
        assert_ne!(world.pos, Pos::new(5, 5), "the agent should have moved");
    }
}
