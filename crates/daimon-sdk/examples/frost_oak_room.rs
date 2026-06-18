//! Proof that Daimon fits **Frost-Oak**'s authoritative-room model — specifically
//! its hardest invariant: `Game::tick` must be **allocation-free + deterministic**.
//!
//! Daimon's `think()` allocates (it builds plans and narration), so the pattern is:
//!   • COGNITION runs off the hot path (control-plane, throttled) and caches each
//!     bot's chosen `Action` as an *intent* — allocation is fine here.
//!   • `tick()` only EXECUTES the cached intents — no `think`, no allocation —
//!     exactly how Frost-Oak executes a human's `apply_input`.
//!
//! This file mirrors Frost-Oak's real `Game` trait shape (`add_player`,
//! `on_player_timeout`, `tick`) and a Dominion-style civ room, then *measures*:
//!   1. ZERO heap allocations across a batch of hot-path ticks (a counting
//!      global allocator proves it, rather than us asserting it), and
//!   2. byte-identical results across two runs (determinism).
//!
//! Run: `cargo run -p daimon-sdk --example frost_oak_room --release`

use daimon_sdk::prelude::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

// ── a global allocator that counts allocations, so we can PROVE tick() is clean ──
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
struct Counting;
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        System.alloc(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        System.dealloc(p, l)
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;

// ── the slice of Frost-Oak's `Game` trait this proof exercises (same shape) ──────
trait Game {
    fn add_player(&mut self, id: u32); // control-plane — MAY allocate
    fn on_player_timeout(&mut self, id: u32); // a seat goes to the AI — MAY allocate
    fn tick(&mut self, dt: f32); // HOT PATH — allocation-free + deterministic
}

const W: i32 = 24;
const H: i32 = 16;
const SIGHT: i32 = 5;

/// A Dominion-style room: civs on a grid, some driven by Daimon minds.
struct Room {
    // world state the host replicates (the engine owns positions/vitals)
    bodies: HashMap<u32, SelfState>,
    food: Vec<Entity>,
    // the Daimon side — minds live OFF the hot path
    minds: HashMap<u32, Agent>,
    intent: HashMap<u32, Action>, // cached decision; reading it in tick() never allocates
    order: Vec<u32>,              // fixed iteration order (built control-plane; sorted)
    cursor: usize,                // round-robin cognition pointer
}

impl Room {
    fn new() -> Self {
        let food = (0..12)
            .map(|i| Entity {
                id: EntityId(1000 + i),
                kind: EntityKind::Food,
                pos: Pos::new((i as i32 * 7 + 2) % W, (i as i32 * 5 + 1) % H),
                label: String::new(),
            })
            .collect();
        Room { bodies: HashMap::new(), food, minds: HashMap::new(), intent: HashMap::new(), order: Vec::new(), cursor: 0 }
    }

    /// CONTROL-PLANE, THROTTLED: think for a slice of bots and cache their intents.
    /// Allocation is fine here — this is NOT the hot path.
    fn cognize(&mut self, budget: usize) {
        if self.order.is_empty() {
            return;
        }
        for k in 0..budget.min(self.order.len()) {
            let id = self.order[(self.cursor + k) % self.order.len()];
            let me = self.bodies[&id];
            // mapping #1: world → perception (allocating a Vec here is fine)
            let visible: Vec<Entity> = self
                .food
                .iter()
                .filter(|e| e.pos.manhattan(me.pos) <= SIGHT)
                .cloned()
                .collect();
            let thought = self.minds.get_mut(&id).unwrap().think(me, visible);
            self.intent.insert(id, thought.action);
        }
        self.cursor = self.cursor.wrapping_add(budget);
    }
}

impl Game for Room {
    fn add_player(&mut self, id: u32) {
        // control-plane: spawn a mind with a DISTINCT seed (shared seeds lock-step)
        let persona = Persona::new("Civ").with_boldness(0.5).with_curiosity(0.6);
        self.minds.insert(id, Agent::new(EntityId(id), persona, 0x5EED ^ id as u64));
        self.bodies.insert(id, SelfState::new(Pos::new((id as i32 * 3) % W, (id as i32 * 2) % H)));
        self.intent.insert(id, Action::Wait);
        self.order.push(id);
        self.order.sort_unstable(); // fixed deterministic order, built off the hot path
    }

    fn on_player_timeout(&mut self, id: u32) {
        // Frost-Oak's designed seam: a dropped player's civ is now AI-driven.
        if !self.minds.contains_key(&id) {
            self.add_player(id);
        }
    }

    fn tick(&mut self, dt: f32) {
        // HOT PATH. Only execute cached intents. No think(), no allocation.
        // Iterate the pre-built `order` (no sort/collect here) for determinism.
        for idx in 0..self.order.len() {
            let id = self.order[idx];
            let action = match self.intent.get(&id) {
                Some(a) => a,
                None => continue,
            };
            match action {
                Action::Move(d) => {
                    let b = self.bodies.get_mut(&id).unwrap();
                    let np = b.pos.step(*d);
                    if np.x >= 0 && np.x < W && np.y >= 0 && np.y < H {
                        b.pos = np;
                    }
                }
                Action::Eat(fid) => {
                    let bpos = self.bodies[&id].pos;
                    // linear scan + in-place mutate — no allocation
                    for f in self.food.iter_mut() {
                        if f.id == *fid && f.pos.manhattan(bpos) <= 1 {
                            self.bodies.get_mut(&id).unwrap().energy = 1.0;
                            f.pos = Pos::new(-1, -1); // "consumed": move out of reach in place
                        }
                    }
                }
                _ => {}
            }
            // metabolism (in-place Copy mutation, no allocation)
            let b = self.bodies.get_mut(&id).unwrap();
            b.energy = (b.energy - 0.01 * dt).max(0.0);
        }
    }
}

fn run() -> Vec<(u32, i32, i32)> {
    let mut room = Room::new();
    for id in 1..=6u32 {
        room.add_player(id);
    }
    room.on_player_timeout(7); // a 7th civ's player dropped → AI takes over

    let dt = 1.0 / 30.0;
    let mut hot_path_allocs = 0usize;
    for t in 0..300u64 {
        // cognition every 6 ticks (5 Hz), round-robin — OFF the hot path
        if t % 6 == 0 {
            room.cognize(3);
        }
        // measure allocations strictly across the hot-path tick
        let before = ALLOCS.load(Ordering::Relaxed);
        room.tick(dt);
        hot_path_allocs += ALLOCS.load(Ordering::Relaxed) - before;
    }

    // report once (the print itself allocates — outside the measured region)
    println!("hot-path allocations across 300 ticks: {hot_path_allocs}");

    let mut snap: Vec<(u32, i32, i32)> =
        room.order.iter().map(|id| (*id, room.bodies[id].pos.x, room.bodies[id].pos.y)).collect();
    snap.sort_unstable();
    snap
}

fn main() {
    println!("== Daimon in a Frost-Oak-style room (allocation-free hot path, proven) ==\n");
    let a = run();
    let b = run();

    println!("\nfinal civ positions: {a:?}");
    println!(
        "\ndeterministic across two runs: {}",
        if a == b { "YES — byte-identical" } else { "NO — bug" }
    );
    println!(
        "Pattern: cognition off the hot path (caches an Action intent); tick() only\n\
         executes intents — zero heap traffic, so it slots into Frost-Oak's\n\
         allocation-free + deterministic `tick` without violating it."
    );
}
