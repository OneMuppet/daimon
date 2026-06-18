# Daimon in Frost-Oak — integration guide

Frost-Oak is **Rust**, so Daimon embeds **directly via the `daimon-sdk` crate** — no
FFI, no JSON-over-the-wire. A Frost-Oak game (`impl Game`) holds a
`daimon_sdk::Agent` per AI-driven entity and drives it from inside the room's
authoritative simulation. Both systems are deterministic, which is exactly what
Frost-Oak's authoritative-server + R0-snapshot model wants.

But two of Frost-Oak's load-bearing invariants are in tension with Daimon, and
this guide exists to resolve them honestly rather than pretend they don't apply.

## The two tensions (and how the pattern resolves them)

**1. `Game::tick` must be allocation-free + deterministic. `Agent::think` allocates.**
Daimon's cognitive cycle builds `Vec`s (visible entities, plans) and `String`s
(narration) — it is *not* allocation-free, so you must **never call `think`
inside `tick`**. Resolution: run cognition **off the hot path** at a throttled
cadence, and have it write a small **cached intent** (a chosen `Action`) per bot;
`tick` then *executes* that cached intent allocation-free, exactly the way it
already executes a human's `apply_input`. This mirrors Frost-Oak's own rule —
"record intent in `apply_input`, do movement in `tick`." Daimon is just a *bot
brain* that fills the intent slot.

**2. Frost-Oak's engine core is zero-external-crates. Daimon pulls in `serde`.**
Daimon depends on `serde`/`serde_json` (for persistence). Resolution: gate the
whole integration behind a cargo **feature** (e.g. `daimon`) so the dependency is
pulled **only when a Daimon-enabled game is built** — the engine core and every
non-Daimon game still build offline, crate-free. (Whether to accept even a
feature-gated external crate in the Frost-Oak tree is a project-owner decision;
this guide assumes yes for a game that wants believable NPCs.)

## Where each piece hooks into the `Game` trait

| `Game` hook | allocation? | what Daimon does here |
|---|---|---|
| `add_player` | **may allocate** (control-plane) | spawn an `Agent` for an AI entity; store it in a side table keyed by `PlayerId` |
| `on_player_timeout` | control-plane | the designed seam: a dropped player's entity is handed to a bot → **attach a Daimon `Agent` and let it drive that civ/NPC** |
| *cognition step* (your own, throttled) | allocation OK | build a `Percept` from current room state, `agent.think(...)`, store the returned `Action` as that bot's cached intent |
| `tick` | **allocation-free + deterministic** | for each bot, **execute its cached intent** (move/eat/build/…) the same way you apply a human input — no `think` here |
| `save_state` / `load_state` | — | include each mind's `Agent::save()` JSON in your migration blob; `Agent::load()` to restore |
| `remove_player` | control-plane | drop the bot's `Agent` |

### Throttling cognition

NPC cognition does not need to run every 30 Hz tick. Run it every N ticks (e.g.
every 6 → 5 Hz), round-robin across bots, on the **control-plane between ticks**
(where allocation is allowed) — not inside `tick`. Spreading bots across frames
also bounds the per-tick cognition cost.

## Sketch (the shape, not a drop-in)

```rust
// behind:  #[cfg(feature = "daimon")]
use daimon_sdk::prelude::*;
use std::collections::HashMap;

struct Bots {
    minds: HashMap<PlayerId, Agent>,
    intent: HashMap<PlayerId, Action>,   // the cached, allocation-free-to-read decision
    cursor: usize,                        // round-robin pointer
}

impl Bots {
    // CONTROL-PLANE (between ticks): allocation is fine here.
    fn attach(&mut self, id: PlayerId, seed: u64) {
        // distinct seed per bot — shared seeds lock-step
        let persona = Persona::new("Civ").with_boldness(0.5);
        self.minds.insert(id, Agent::new(EntityId(id.0), persona, seed));
    }

    // CONTROL-PLANE, THROTTLED: think for a slice of bots, cache their intents.
    fn cognize(&mut self, world: &MyGameState, budget: usize) {
        let ids: Vec<PlayerId> = self.minds.keys().copied().collect();
        for k in 0..budget.min(ids.len()) {
            let id = ids[(self.cursor + k) % ids.len()];
            let body: SelfState = world.body_of(id);          // mapping #1a
            let visible: Vec<Entity> = world.visible_to(id);  // mapping #1b
            let thought = self.minds.get_mut(&id).unwrap().think(body, visible);
            self.intent.insert(id, thought.action);
        }
        self.cursor = self.cursor.wrapping_add(budget);
    }
}

impl Game for MyGame {
    fn tick(&mut self, dt: f32) {
        // HOT PATH — allocation-free, deterministic. Only EXECUTE cached intents.
        for (id, action) in self.bots.intent.iter() {
            match action {
                Action::Move(d) => self.apply_move(*id, *d, dt), // your authoritative move
                Action::Build(p) => self.try_build(*id, *p),
                _ => {}
            }
        }
        // … your world rules …
    }
    // add_player / on_player_timeout call self.bots.attach(...);
    // a host-side schedule calls self.bots.cognize(world, budget) every N ticks.
}
```

## Determinism & migration

- Daimon is deterministic (same seed + same percepts → same decisions), which
  composes with Frost-Oak's deterministic sim. Keep cognition order fixed
  (round-robin by id) so a replay reproduces.
- NPC cognition runs **server-side only** — clients replicate NPC *positions*,
  they don't predict NPC *minds* — so Daimon does not affect client prediction
  parity (that constraint is about player movement).
- For live room migration (`save_state`/`load_state` byte-exact), include each
  `Agent::save()` JSON. **Caveat:** byte-identical state across two *different*
  nodes additionally requires identical floating-point behaviour across those
  machines, which Daimon does **not** guarantee cross-platform. Same-build /
  same-arch migration is fine; a mixed-arch cluster needs validation. Don't claim
  cross-arch byte-exactness until it's measured.

## Mapping Daimon ↔ Frost-Oak coordinates

Daimon reasons over a **discrete grid** (`Pos { x: i32, y: i32 }`); Frost-Oak uses
continuous `V2`. Quantise the world to a grid for perception (round/bin
positions) and dequantise the chosen `Move(Dir)` back into your authoritative
continuous movement. Pick a grid scale that matches your gameplay's tactical
resolution.

## Honest scope

Same as everywhere: Daimon is not an LLM (templated narration), does no
vision/raycasting (you curate `visible`), and is grid-shaped. It fits Frost-Oak
games where believable autonomous agents matter — **Dominion** (AI civs via
`on_player_timeout`), the ambient open world, the civ sim — not twitch shooter
aim. See `crates/daimon-sdk` (the embed API) and root `AGENTS.md` (full vocab).
```
