//! Measure steady-state heap allocations per `Agent::think()` tick.
//!
//! This is the baseline + regression probe for making Daimon's cognitive cycle
//! allocation-free (so it can live in a realtime engine's hot path, e.g.
//! Frost-Oak). A counting global allocator tallies every `alloc`; we warm up
//! (let one-time setup + bounded-structure growth settle), then measure
//! allocations across a long run of `think()` with REUSED inputs — so what's left
//! is the cycle's intrinsic per-tick allocation.
//!
//! Run: `cargo run -p daimon-sdk --example alloc_probe --release`

use daimon_sdk::prelude::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

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
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        System.realloc(p, l, n)
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;

fn measure(label: &str, visible: &[Entity], warmup: u64, ticks: u64) {
    let mut agent = Agent::new(EntityId(1), Persona::new("Probe").with_curiosity(0.7), 42);
    let body = SelfState::new(Pos::new(8, 8));
    // warm up: one-time setup + any bounded-structure growth converges
    for _ in 0..warmup {
        agent.think(body, visible.to_vec());
    }
    // measure: reuse the SAME owned `visible` each tick so the only allocations
    // counted are the cycle's own (we pre-clone outside the measured region).
    let inputs: Vec<Vec<Entity>> = (0..ticks).map(|_| visible.to_vec()).collect();
    let before = ALLOCS.load(Ordering::Relaxed);
    for v in inputs {
        // NOTE: `v` is moved in (think takes Vec<Entity>); its allocation happened
        // above, outside the measured window. Cycle-internal allocs are what move.
        let _ = agent.think(body, v);
    }
    let allocs = ALLOCS.load(Ordering::Relaxed) - before;
    println!(
        "{label:<28} {ticks} ticks → {allocs} allocs total → {:.1} allocs/tick",
        allocs as f64 / ticks as f64
    );
}

fn main() {
    println!("== Daimon cognition allocation probe (steady state) ==\n");

    // Empty world (idle wandering): the floor of per-tick cost.
    measure("idle (nothing visible)", &[], 200, 2000);

    // A typical scene: food, water, a peer, a predator in view.
    let scene = vec![
        Entity { id: EntityId(2), kind: EntityKind::Food, pos: Pos::new(10, 8), label: "berry".into() },
        Entity { id: EntityId(3), kind: EntityKind::Water, pos: Pos::new(8, 11), label: "spring".into() },
        Entity { id: EntityId(4), kind: EntityKind::Agent, pos: Pos::new(7, 8), label: "kin".into() },
        Entity { id: EntityId(5), kind: EntityKind::Predator, pos: Pos::new(13, 8), label: "stalker".into() },
    ];
    measure("typical scene (4 visible)", &scene, 200, 2000);

    println!("\nGoal: steady-state allocs/tick → 0 (then `think` fits a realtime hot path).");
    println!("This probe is the regression guard while we get there.");
}
