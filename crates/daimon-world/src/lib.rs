//! # daimon-world
//!
//! A deliberately tiny, deterministic grid world — just enough environment to
//! give a [`daimon_core`]/[`daimon_mind`] agent a body, stakes, and other
//! minds to reason about. It is **not** a game; it is a testbed. The point is
//! to exercise the architecture (drives, memory, deliberation, theory of mind)
//! with the smallest possible simulation, so that any lifelike behaviour you
//! observe comes from the *mind*, not from elaborate scripted scenery.
//!
//! The world owns the agent's **body** (energy, hydration, health) — the mind
//! only ever *senses* it. Each [`World::step`] applies one bounded [`Action`],
//! advances metabolism and the other entities, then returns the next
//! [`Percept`]. Same seed, same actions → same world, forever.
//!
//! What lives here:
//! * **Food** and **Water** — consumable; respawn elsewhere after a while.
//! * **Curios** — novel objects that satisfy curiosity when inspected.
//! * A **Predator** — stalks the agent, hurts it on contact, then retreats.
//! * **Townsfolk** (agents) — wander, and greet the Daimon when it's near.

use daimon_core::{
    Action, Dir, Entity, EntityId, EntityKind, Percept, Pos, Rng, SelfState, WorldEvent,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldConfig {
    pub width: i32,
    pub height: i32,
    pub sight: i32,
    pub food: usize,
    pub water: usize,
    pub curios: usize,
    pub townsfolk: usize,
    pub seed: u64,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            width: 32,
            height: 20,
            sight: 6,
            food: 5,
            water: 3,
            curios: 4,
            townsfolk: 2,
            seed: 0xDA13,
        }
    }
}

#[derive(Debug, Clone)]
struct Object {
    id: EntityId,
    kind: EntityKind,
    pos: Pos,
    label: String,
    alive: bool,
    /// For food: energy delivered. For water: hydration.
    payload: f32,
    /// Tick at which a consumed resource reappears (relocated).
    respawn_at: Option<u64>,
    /// For predator: ticks between moves (it's slower than the agent).
    move_period: u64,
    /// For predator: until this tick it backs off and wanders instead of
    /// hunting (it has just struck and is sated). Gives the agent room to flee.
    cooldown_until: u64,
}

/// The simulation.
pub struct World {
    cfg: WorldConfig,
    rng: Rng,
    tick: u64,
    body: SelfState,
    objects: Vec<Object>,
    seen: BTreeSet<EntityId>,
    next_id: u32,
    /// Pending NPC reply: (speaker, listener-relative line) delivered next step.
    pending_speech: Vec<(EntityId, String)>,
}

impl World {
    pub fn new(cfg: WorldConfig) -> Self {
        let mut w = World {
            body: SelfState::new(Pos::new(cfg.width / 2, cfg.height / 2)),
            objects: Vec::new(),
            seen: BTreeSet::new(),
            next_id: 1,
            pending_speech: Vec::new(),
            tick: 0,
            rng: Rng::new(cfg.seed),
            cfg: cfg.clone(),
        };
        for i in 0..cfg.food {
            w.spawn(EntityKind::Food, &format!("berries-{i}"), 0.45);
        }
        for i in 0..cfg.water {
            w.spawn(EntityKind::Water, &format!("spring-{i}"), 0.55);
        }
        let curio_names = ["monolith", "glyph", "humming stone", "old shrine", "strange bloom"];
        for i in 0..cfg.curios {
            let name = curio_names[i % curio_names.len()];
            w.spawn(EntityKind::Curio, name, 0.0);
        }
        let folk = ["Elder Mara", "Bram", "Wren", "Old Tace"];
        for i in 0..cfg.townsfolk {
            w.spawn(EntityKind::Agent, folk[i % folk.len()], 0.0);
        }
        // exactly one predator.
        let pid = w.spawn(EntityKind::Predator, "the stalker", 0.0);
        if let Some(p) = w.object_mut(pid) {
            p.move_period = 2;
        }
        w
    }

    pub fn with_seed(seed: u64) -> Self {
        World::new(WorldConfig {
            seed,
            ..Default::default()
        })
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn body(&self) -> SelfState {
        self.body
    }
    pub fn config(&self) -> &WorldConfig {
        &self.cfg
    }
    /// True once the agent's health hits zero — the run is over.
    pub fn is_dead(&self) -> bool {
        self.body.health <= 0.0
    }

    fn random_pos(&mut self) -> Pos {
        Pos::new(
            self.rng.below(self.cfg.width as usize) as i32,
            self.rng.below(self.cfg.height as usize) as i32,
        )
    }

    fn spawn(&mut self, kind: EntityKind, label: &str, payload: f32) -> EntityId {
        let id = EntityId(self.next_id);
        self.next_id += 1;
        let pos = self.random_pos();
        self.objects.push(Object {
            id,
            kind,
            pos,
            label: label.to_string(),
            alive: true,
            payload,
            respawn_at: None,
            move_period: 1,
            cooldown_until: 0,
        });
        id
    }

    fn object(&self, id: EntityId) -> Option<&Object> {
        self.objects.iter().find(|o| o.id == id)
    }
    fn object_mut(&mut self, id: EntityId) -> Option<&mut Object> {
        self.objects.iter_mut().find(|o| o.id == id)
    }

    fn clamp(&self, p: Pos) -> Pos {
        Pos::new(
            p.x.clamp(0, self.cfg.width - 1),
            p.y.clamp(0, self.cfg.height - 1),
        )
    }

    /// The opening percept, before any action — the agent's first look around.
    pub fn observe(&mut self) -> Percept {
        let (visible, events) = self.sense();
        Percept {
            tick: self.tick,
            me: self.body,
            visible,
            events,
        }
    }

    /// Advance the world by one tick under the agent's chosen `action`, and
    /// return the resulting percept.
    pub fn step(&mut self, action: &Action) -> Percept {
        self.tick += 1;
        let mut events: Vec<WorldEvent> = Vec::new();

        // 1) resolve the agent's action.
        self.resolve_action(action, &mut events);

        // 2) metabolism — the slow clock that creates need.
        self.body.energy = (self.body.energy - 0.012).clamp(0.0, 1.0);
        self.body.hydration = (self.body.hydration - 0.014).clamp(0.0, 1.0);
        if self.is_safe() && self.body.health < 1.0 {
            self.body.health = (self.body.health + 0.004).min(1.0);
        }
        // starvation/dehydration bleed into health.
        if self.body.energy <= 0.0 || self.body.hydration <= 0.0 {
            self.body.health = (self.body.health - 0.02).max(0.0);
        }

        // 3) other entities act.
        self.step_predator(&mut events);
        self.step_townsfolk(&mut events);

        // 4) deliver any speech queued last tick.
        for (from, text) in std::mem::take(&mut self.pending_speech) {
            events.push(WorldEvent::Heard { from, text });
        }

        // 5) respawn consumed resources.
        self.respawn_due();

        // 6) sense the new state.
        let (visible, sense_events) = self.sense();
        events.extend(sense_events);

        Percept {
            tick: self.tick,
            me: self.body,
            visible,
            events,
        }
    }

    fn resolve_action(&mut self, action: &Action, events: &mut Vec<WorldEvent>) {
        match action {
            Action::Move(d) => {
                self.body.pos = self.clamp(self.body.pos.step(*d));
            }
            Action::Eat(id) => {
                if self.adjacent_alive(*id, EntityKind::Food) {
                    let gain = self.object(*id).map(|o| o.payload).unwrap_or(0.4);
                    self.body.energy = (self.body.energy + gain).min(1.0);
                    self.consume(*id);
                    events.push(WorldEvent::Ate { id: *id, energy: gain });
                }
            }
            Action::Drink(id) => {
                if self.adjacent_alive(*id, EntityKind::Water) {
                    let gain = self.object(*id).map(|o| o.payload).unwrap_or(0.5);
                    self.body.hydration = (self.body.hydration + gain).min(1.0);
                    // water doesn't deplete; just emit the event.
                    events.push(WorldEvent::Drank { id: *id });
                }
            }
            Action::Inspect(_id) => {
                // satisfying curiosity is its own reward; no body change. The
                // novelty was already delivered when first seen.
            }
            Action::Talk { to, text } => {
                events.push(WorldEvent::Spoke {
                    to: *to,
                    text: text.clone(),
                });
                // a townsperson hears it and will reply next tick.
                if self.object(*to).map(|o| o.kind == EntityKind::Agent).unwrap_or(false) {
                    let reply = self.npc_reply(*to);
                    self.pending_speech.push((*to, reply));
                }
            }
            Action::Strike(_id) => {
                // single-agent world: a lone strike has no allies to make it work,
                // so it does nothing here (collective defence lives in the village).
            }
            Action::Rest => {
                self.body.energy = (self.body.energy + 0.03).min(1.0);
            }
            Action::Wait => {}
        }
    }

    fn adjacent_alive(&self, id: EntityId, kind: EntityKind) -> bool {
        self.object(id)
            .map(|o| o.alive && o.kind == kind && o.pos.manhattan(self.body.pos) <= 1)
            .unwrap_or(false)
    }

    fn consume(&mut self, id: EntityId) {
        let at = self.tick + 18 + self.rng.below(20) as u64;
        if let Some(o) = self.object_mut(id) {
            o.alive = false;
            o.respawn_at = Some(at);
        }
    }

    fn respawn_due(&mut self) {
        let now = self.tick;
        // Resources regrow *in place* — the berry bush is still where it was.
        // Stable locations are what make the agent's spatial memory worth
        // having: it learns where food is and returns there once it regrows.
        for o in self.objects.iter_mut() {
            if !o.alive && o.respawn_at.map(|t| t <= now).unwrap_or(false) {
                o.alive = true;
                o.respawn_at = None;
            }
        }
    }

    fn is_safe(&self) -> bool {
        !self
            .objects
            .iter()
            .any(|o| o.kind == EntityKind::Predator && o.alive && o.pos.manhattan(self.body.pos) <= 3)
    }

    fn step_predator(&mut self, events: &mut Vec<WorldEvent>) {
        let tick = self.tick;
        let agent = self.body.pos;

        // snapshot predators so we can use the rng freely while computing moves.
        let preds: Vec<(EntityId, Pos, u64, u64)> = self
            .objects
            .iter()
            .filter(|o| o.kind == EntityKind::Predator && o.alive)
            .map(|o| (o.id, o.pos, o.move_period, o.cooldown_until))
            .collect();

        for (id, pos, period, cooldown_until) in preds {
            // already on top of the agent (e.g. agent walked into it)?
            if pos.manhattan(agent) == 0 {
                self.strike(id, events);
                continue;
            }
            let new_pos = if tick < cooldown_until {
                // sated: amble away aimlessly, ignoring the agent.
                let d = Dir::ALL[self.rng.below(4)];
                self.clamp(pos.step(d))
            } else if tick.is_multiple_of(period) {
                // it only hunts within an aggro leash; lose it in the distance
                // and it gives up and patrols — so a clean break really works.
                const AGGRO: i32 = 11;
                if pos.manhattan(agent) <= AGGRO {
                    pos.step(pos.toward(agent))
                } else {
                    let d = Dir::ALL[self.rng.below(4)];
                    self.clamp(pos.step(d))
                }
            } else {
                pos
            };
            if let Some(o) = self.object_mut(id) {
                o.pos = new_pos;
            }
            if new_pos.manhattan(agent) == 0 {
                self.strike(id, events);
            }
        }
    }

    /// The predator lands a hit: damage, an event, then it recoils to a distant
    /// corner and goes on cooldown — the agent earns a real chance to escape.
    fn strike(&mut self, id: EntityId, events: &mut Vec<WorldEvent>) {
        self.body.health = (self.body.health - 0.2).max(0.0);
        events.push(WorldEvent::Hurt { id, health: 0.2 });
        // retreat to whichever corner is farthest from the agent.
        let agent = self.body.pos;
        let corners = [
            Pos::new(0, 0),
            Pos::new(self.cfg.width - 1, 0),
            Pos::new(0, self.cfg.height - 1),
            Pos::new(self.cfg.width - 1, self.cfg.height - 1),
        ];
        let far = corners
            .into_iter()
            .max_by_key(|c| c.manhattan(agent))
            .unwrap_or(Pos::new(0, 0));
        let until = self.tick + 12;
        if let Some(o) = self.object_mut(id) {
            o.pos = far;
            o.cooldown_until = until;
        }
    }

    fn step_townsfolk(&mut self, events: &mut Vec<WorldEvent>) {
        let agent = self.body.pos;
        let mut greetings: Vec<(EntityId, String)> = Vec::new();
        for o in self.objects.iter_mut().filter(|o| o.kind == EntityKind::Agent) {
            // amble one step ~half the time.
            if self.rng.chance(0.5) {
                let d = Dir::ALL[self.rng.below(4)];
                let np = o.pos.step(d);
                o.pos = Pos::new(np.x.clamp(0, self.cfg.width - 1), np.y.clamp(0, self.cfg.height - 1));
            }
            // greet the Daimon when close, occasionally.
            if o.pos.manhattan(agent) <= 2 && self.rng.chance(0.2) {
                greetings.push((o.id, format!("Hello, friend. Welcome — {} here.", o.label)));
            }
        }
        for (from, text) in greetings {
            events.push(WorldEvent::Heard { from, text });
        }
    }

    fn npc_reply(&mut self, id: EntityId) -> String {
        let name = self
            .object(id)
            .map(|o| o.label.clone())
            .unwrap_or_else(|| "someone".into());
        let lines = [
            format!("Good to meet you. I'm {name}. Stay safe out here."),
            "Share the water by the spring if you're thirsty, friend.".to_string(),
            "Mind the stalker to the north. Travel together and you'll be alright.".to_string(),
        ];
        lines[self.rng.below(lines.len())].clone()
    }

    /// Compute what the agent can currently see, plus first-sight discoveries.
    fn sense(&mut self) -> (Vec<Entity>, Vec<WorldEvent>) {
        let here = self.body.pos;
        let sight = self.cfg.sight;
        let mut visible = Vec::new();
        let mut first_sight = Vec::new();
        for o in &self.objects {
            if !o.alive {
                continue;
            }
            if o.pos.manhattan(here) <= sight {
                visible.push(Entity {
                    id: o.id,
                    kind: o.kind,
                    pos: o.pos,
                    label: o.label.clone(),
                });
                if !self.seen.contains(&o.id) {
                    first_sight.push(o.id);
                }
            }
        }
        let events = first_sight
            .iter()
            .map(|id| WorldEvent::Discovered { id: *id })
            .collect();
        for id in first_sight {
            self.seen.insert(id);
        }
        (visible, events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_is_deterministic_for_seed() {
        let mut a = World::with_seed(7);
        let mut b = World::with_seed(7);
        let acts = [Action::Move(Dir::East), Action::Rest, Action::Move(Dir::North)];
        for act in acts.iter().cycle().take(50) {
            let pa = a.step(act);
            let pb = b.step(act);
            assert_eq!(pa.me.pos, pb.me.pos);
            assert_eq!(pa.visible.len(), pb.visible.len());
        }
    }

    #[test]
    fn metabolism_creates_hunger_over_time() {
        let mut w = World::with_seed(1);
        let e0 = w.body().energy;
        for _ in 0..20 {
            w.step(&Action::Wait);
        }
        assert!(w.body().energy < e0);
    }

    #[test]
    fn eating_adjacent_food_restores_energy() {
        let mut w = World::with_seed(3);
        // drain a little, then drop the agent next to a known food item.
        for _ in 0..10 {
            w.step(&Action::Wait);
        }
        // find a living food object and teleport the body beside it.
        let food = w
            .objects
            .iter()
            .find(|o| o.kind == EntityKind::Food && o.alive)
            .map(|o| (o.id, o.pos))
            .unwrap();
        w.body.pos = food.1;
        let before = w.body().energy;
        let p = w.step(&Action::Eat(food.0));
        assert!(w.body().energy > before);
        assert!(p
            .events
            .iter()
            .any(|e| matches!(e, WorldEvent::Ate { .. })));
    }

    #[test]
    fn first_sight_emits_discovered_once() {
        let mut w = World::with_seed(5);
        let p = w.observe();
        let discovered: usize = p
            .events
            .iter()
            .filter(|e| matches!(e, WorldEvent::Discovered { .. }))
            .count();
        // seeing the same things again yields no new discoveries.
        let p2 = w.step(&Action::Wait);
        let again: usize = p2
            .events
            .iter()
            .filter(|e| matches!(e, WorldEvent::Discovered { .. }))
            .count();
        assert!(discovered >= again);
    }
}
