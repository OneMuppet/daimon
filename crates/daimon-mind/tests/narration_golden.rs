//! Golden test: pins the EXACT cognitive output (action + goal + drive + the
//! first-person narration line) over a scripted run, so any refactor that claims
//! to be "byte-identical" is actually held to it. The determinism proof only
//! guarantees same-seed→same-run; it would happily pass a refactor that changed
//! the narration text. This test would not.
//!
//! If a CHANGE TO BEHAVIOUR is intended, re-capture the hash (run with the const
//! wrong, read the printed `actual`, paste it back). Otherwise a mismatch means
//! the change was not byte-identical — investigate before updating the constant.

use daimon_core::{Entity, EntityId, EntityKind, Percept, Pos, SelfState, WorldEvent};
use daimon_mind::Mind;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Drive one mind through a fixed, varied script and fold every decision into a
/// stable digest of (action verb, goal label, dominant drive, inner line).
fn run_digest(seed: u64) -> u64 {
    let mut mind = Mind::new(daimon_mind::Persona::new("Probe").with_curiosity(0.7), seed);
    let mut h = DefaultHasher::new();

    let scene = |tick: u64, predator: bool| -> Percept {
        let mut visible = vec![
            Entity { id: EntityId(2), kind: EntityKind::Food, pos: Pos::new(10, 8), label: "berry".into() },
            Entity { id: EntityId(3), kind: EntityKind::Water, pos: Pos::new(8, 11), label: "spring".into() },
            Entity { id: EntityId(4), kind: EntityKind::Agent, pos: Pos::new(7, 8), label: "kin".into() },
            Entity { id: EntityId(6), kind: EntityKind::Curio, pos: Pos::new(9, 9), label: "totem".into() },
        ];
        if predator {
            visible.push(Entity { id: EntityId(5), kind: EntityKind::Predator, pos: Pos::new(11, 8), label: "stalker".into() });
        }
        let mut me = SelfState::new(Pos::new(8, 8));
        // vary the body so different drives dominate across the run
        me.energy = 1.0 - (tick % 7) as f32 / 7.0;
        me.hydration = 1.0 - (tick % 5) as f32 / 5.0;
        me.health = 1.0 - (tick % 11) as f32 / 22.0;
        Percept { tick, me, visible, events: vec![WorldEvent::Discovered { id: EntityId(6) }] }
    };

    for tick in 0..400u64 {
        let t = mind.cycle(&scene(tick, tick % 3 == 0));
        t.action.verb().hash(&mut h);
        t.goal.label().hash(&mut h);
        t.dominant_drive.name().hash(&mut h);
        mind.inner().hash(&mut h);
    }
    h.finish()
}

#[test]
fn cognitive_output_is_byte_identical() {
    // Captured baseline (seed 42). A mismatch means behaviour changed — if that
    // was NOT intended, the change is not byte-identical. Recapture only on a
    // deliberate behaviour change.
    const GOLDEN_SEED_42: u64 = 8091831107678400469;
    let actual = run_digest(42);
    assert_eq!(actual, GOLDEN_SEED_42, "cognitive output changed (not byte-identical)");
}

#[test]
fn distinct_seeds_diverge() {
    // sanity: the digest actually depends on the run (not a constant)
    assert_ne!(run_digest(1), run_digest(2));
}
