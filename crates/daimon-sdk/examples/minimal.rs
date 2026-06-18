//! A complete, runnable game world built on **nothing but `daimon-sdk`**.
//!
//! It is deliberately tiny — a grid with food, water, and one wandering predator
//! — so the whole embedding pattern fits on one screen:
//!
//!   1. your world implements [`Senses`] (what an agent perceives) and
//!      [`Actuator`] (how a chosen action takes effect), and
//!   2. each tick you call [`step`] for every agent.
//!
//! Run it:  `cargo run -p daimon-sdk --example minimal`
//!
//! Watch three personalities — a curious wanderer, a timid homebody, and a
//! balanced one — diverge from identical code, purely from their personas and
//! what they each live through. Same seed → same run, every time.

use daimon_sdk::prelude::*;
use std::collections::HashMap;

const W: i32 = 20;
const H: i32 = 12;
const SIGHT: i32 = 4;
const TICKS: u64 = 120;

/// Our entire game state. Daimon never touches this directly — it only ever sees
/// what `Senses` reports and only ever changes it through `Actuator`.
struct Field {
    bodies: HashMap<u32, SelfState>, // each agent's position + vitals (the world owns these)
    food: Vec<Entity>,
    water: Vec<Entity>,
    curios: Vec<Entity>, // novel objects — fuel for the curious
    predator: Entity,
    next_id: u32,
}

impl Field {
    fn clamp(p: Pos) -> Pos {
        Pos::new(p.x.clamp(0, W - 1), p.y.clamp(0, H - 1))
    }
}

// ── Mapping #1: world → perception ───────────────────────────────────────────
impl Senses for Field {
    fn body(&self, agent: EntityId) -> SelfState {
        self.bodies[&agent.0]
    }
    fn visible(&self, agent: EntityId) -> Vec<Entity> {
        let me = self.bodies[&agent.0].pos;
        let in_sight = |p: Pos| p.manhattan(me) <= SIGHT;
        let mut seen: Vec<Entity> = Vec::new();
        seen.extend(self.food.iter().filter(|e| in_sight(e.pos)).cloned());
        seen.extend(self.water.iter().filter(|e| in_sight(e.pos)).cloned());
        seen.extend(self.curios.iter().filter(|e| in_sight(e.pos)).cloned());
        if in_sight(self.predator.pos) {
            seen.push(self.predator.clone());
        }
        // other agents are perceivable too — subjects of theory-of-mind
        for (id, st) in &self.bodies {
            if *id != agent.0 && in_sight(st.pos) {
                seen.push(Entity {
                    id: EntityId(*id),
                    kind: EntityKind::Agent,
                    pos: st.pos,
                    label: "neighbour".into(),
                });
            }
        }
        seen
    }
}

// ── Mapping #2: chosen action → effect (and the events it produces) ──────────
impl Actuator for Field {
    fn apply(&mut self, agent: EntityId, action: &Action) -> Vec<WorldEvent> {
        let mut events = Vec::new();
        let me = self.bodies[&agent.0].pos;
        match action {
            Action::Move(dir) => {
                self.bodies.get_mut(&agent.0).unwrap().pos = Field::clamp(me.step(*dir));
            }
            Action::Eat(id) => {
                if let Some(idx) = self.food.iter().position(|f| f.id == *id && f.pos.manhattan(me) <= 1) {
                    let eaten = self.food.swap_remove(idx);
                    let b = self.bodies.get_mut(&agent.0).unwrap();
                    b.energy = 1.0;
                    events.push(WorldEvent::Ate { id: eaten.id, energy: 0.6 });
                    // respawn a berry elsewhere so the world stays lively (deterministic spot)
                    self.next_id += 1;
                    let p = Pos::new((self.next_id as i32 * 7) % W, (self.next_id as i32 * 5) % H);
                    self.food.push(Entity { id: EntityId(self.next_id), kind: EntityKind::Food, pos: p, label: "berry".into() });
                }
            }
            Action::Drink(id) if self.water.iter().any(|w| w.id == *id && w.pos.manhattan(me) <= 1) => {
                self.bodies.get_mut(&agent.0).unwrap().hydration = 1.0;
                events.push(WorldEvent::Drank { id: *id });
            }
            Action::Rest => {
                let b = self.bodies.get_mut(&agent.0).unwrap();
                b.energy = (b.energy + 0.02).min(1.0);
            }
            _ => {} // Inspect / Talk / Strike / Build / Gather / Store / Wait: unused in this demo
        }
        events
    }
}

fn main() {
    // One field, shared by all agents.
    let mut field = Field {
        bodies: HashMap::new(),
        food: (0..9).map(|i| Entity { id: EntityId(100 + i), kind: EntityKind::Food, pos: Pos::new((i as i32 * 7 + 1) % W, (i as i32 * 5 + 2) % H), label: "berry".into() }).collect(),
        water: (0..4).map(|i| Entity { id: EntityId(200 + i), kind: EntityKind::Water, pos: Pos::new((i as i32 * 9 + 4) % W, (i as i32 * 7 + 3) % H), label: "spring".into() }).collect(),
        curios: (0..2).map(|i| Entity { id: EntityId(300 + i), kind: EntityKind::Curio, pos: Pos::new((i as i32 * 8 + 6) % W, (i as i32 * 5 + 6) % H), label: "strange totem".into() }).collect(),
        predator: Entity { id: EntityId(999), kind: EntityKind::Predator, pos: Pos::new(W - 1, H - 1), label: "the stalker".into() },
        next_id: 400,
    };

    // Three characters — identical code, different souls. Each gets its OWN seed
    // (a per-NPC seed is the rule: same seed + same percepts = same behaviour, so
    // two agents sharing a seed would lock-step the moment they meet).
    let mut agents = vec![
        Agent::new(EntityId(1), Persona::new("Vell").with_curiosity(0.95).with_boldness(0.6), 7),
        Agent::new(EntityId(2), Persona::new("Sela").with_boldness(0.12).with_sociability(0.7), 23),
        Agent::new(EntityId(3), Persona::new("Kael"), 41),
    ];
    // start them in three different regions so their lives can genuinely diverge
    let starts = [Pos::new(2, 2), Pos::new(10, 10), Pos::new(17, 4)];
    for (a, p) in agents.iter().zip(starts) {
        field.bodies.insert(a.id.0, SelfState::new(p));
    }

    println!("== Daimon SDK — minimal world ({} agents, {} ticks, seed 7) ==\n", agents.len(), TICKS);
    let mut last_inner: HashMap<u32, String> = HashMap::new();

    for tick in 0..TICKS {
        // A slow, roaming predator: it only closes in when it actually spots a
        // living agent nearby, and steps every other tick — a real but survivable
        // threat (enough to make the timid one flee and the bold one hold).
        if tick % 2 == 0 {
            let prey = agents.iter()
                .map(|a| field.bodies[&a.id.0])
                .filter(|b| b.health > 0.0)
                .map(|b| b.pos)
                .min_by_key(|p| p.manhattan(field.predator.pos));
            field.predator.pos = match prey {
                Some(p) if p.manhattan(field.predator.pos) <= SIGHT => field.predator.pos.step(field.predator.pos.toward(p)),
                _ => Field::clamp(field.predator.pos.step(Dir::ALL[(tick / 2 % 4) as usize])), // else wander
            };
        }

        // Advance each mind, then apply metabolism and predator contact.
        for agent in agents.iter_mut() {
            let id = agent.id;
            if field.bodies[&id.0].health <= 0.0 {
                continue; // this one has died
            }
            let thought = step(agent, &mut field);
            last_inner.insert(id.0, thought.inner.clone());

            let pos = field.bodies[&id.0].pos;
            // a predator on top of you hurts — a world-driven event the mind must hear
            if pos.manhattan(field.predator.pos) == 0 {
                let b = field.bodies.get_mut(&id.0).unwrap();
                b.health = (b.health - 0.15).max(0.0);
                agent.observe(WorldEvent::Hurt { id: field.predator.id, health: 0.15 });
            }
            // metabolism: living costs energy and water; running out costs health
            let b = field.bodies.get_mut(&id.0).unwrap();
            b.energy = (b.energy - 0.012).max(0.0);
            b.hydration = (b.hydration - 0.009).max(0.0);
            if b.energy <= 0.0 || b.hydration <= 0.0 {
                b.health = (b.health - 0.02).max(0.0);
            }
        }

        if tick % 30 == 29 {
            println!("--- tick {} ---", tick + 1);
            for a in &agents {
                let b = &field.bodies[&a.id.0];
                println!("  {:>5}  @({:>2},{:>2})  hp {:.2} en {:.2} hy {:.2}  | {}", a.name(), b.pos.x, b.pos.y, b.health, b.energy, b.hydration, last_inner.get(&a.id.0).cloned().unwrap_or_default());
            }
            println!();
        }
    }

    println!("== final ==");
    for a in &agents {
        let b = &field.bodies[&a.id.0];
        let state = if b.health > 0.0 { "alive" } else { "died" };
        println!("  {:>5}: {:<5}  hp {:.2}  energy {:.2}  hydration {:.2}", a.name(), state, b.health, b.energy, b.hydration);
    }
    println!("\nThree personas, one codebase — they diverge from temperament alone.");
    println!("Same seed → identical run. Re-run to confirm: the numbers don't move.");
}
