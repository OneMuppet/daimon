# Integrating Daimon — a guide for coding agents

You are wiring **Daimon** into a game. Daimon gives an NPC a *mind*: it forms its
own goals from felt drives (hunger, thirst, safety, social, curiosity), learns
what objects are for, remembers, fears death, grieves a bonded peer, and can wall
itself in or provision for winter — **none of it scripted**. It is
**deterministic** (same seed + same percepts → same behaviour: replays, lockstep
multiplayer, reproducible debugging), **CPU-only**, **pure Rust**, with **no model
weights, no GPU, no network, and no per-frame inference cost**.

Integrate through the **`daimon-sdk`** crate. You almost never touch
`daimon-core`/`daimon-mind` directly. This document is the whole contract — follow
it literally; the type names and variants below are exact.

## Which engine are you on?

- **Rust engine (Bevy, Frost-Oak, custom):** embed `daimon-sdk` directly — this
  document. For **Frost-Oak** specifically (allocation-free + deterministic hot
  path), see `integrations/frost-oak/AGENTS.md` and the proven pattern in
  `crates/daimon-sdk/examples/frost_oak_room.rs` (cognition off the hot path,
  `tick` only executes cached intents — measured at zero hot-path allocations).
- **Unity (C#):** use the native plugin + C# wrapper in `integrations/unity/`
  (see its `AGENTS.md`). Talks to the `daimon-ffi` C ABI.
- **Unreal (C++):** use the module in `integrations/unreal/` (see its `AGENTS.md`).
- **Any other C/C++/C# host:** link `daimon-ffi` directly — C ABI + flat-JSON,
  header at `crates/daimon-ffi/include/daimon.h`, contract in that crate's docs.

The cognitive contract (perception in → decision out, the two mappings,
determinism rules, the `Action`/`WorldEvent`/`SelfState` vocabulary) is identical
across all of them; only the transport differs (Rust types vs flat JSON).

---

## 1. The contract in one sentence

Each tick, hand every agent **what it senses** and it returns a **`Thought`**
containing the **`Action`** it chose (plus its goal, dominant drive, and a
first-person line). You do exactly **two mappings**:

1. **your world → perception** — where/how is this NPC and what can it see?
2. **chosen `Action` → effect** — carry the action out in your world.

Everything between those two is Daimon.

## 2. Add the dependency

`daimon-sdk` is a workspace crate in this repo. From another Rust project, depend
on it by path or git:

```toml
[dependencies]
daimon-sdk = { path = "…/crates/daimon-sdk" }   # or { git = "…", package = "daimon-sdk" }
```

Then: `use daimon_sdk::prelude::*;`

## 3. Spawn an agent — one per NPC

```rust
use daimon_sdk::prelude::*;

// id: YOUR stable handle for this character (Daimon never invents ids).
// persona: temperament. seed: pick a DISTINCT seed per NPC (see §7).
let mut agent = Agent::new(EntityId(1), Persona::new("Mara").with_curiosity(0.9), 42);
```

`Persona` builder knobs (all `f32` in `0..=1`, default `0.5`):
`.with_boldness(_)` (timid→fearless: scales flee distance & survival drive),
`.with_sociability(_)` (pull of the social drive),
`.with_curiosity(_)` (reward from novelty),
`.with_creed(&str)` (a one-line self-concept used in narration).

To ship a mind you bred offline with the evolution tools, use
`Agent::from_genome(id, &genome, persona, seed)` instead.

## 4. The per-tick loop — two ways

### A. Low-level (you own the loop) — recommended; most game loops already have the world handy

```rust
// 1. describe the body (position + vitals) and what's visible:
let body: SelfState = /* build from your world — see §5 */;
let visible: Vec<Entity> = /* entities in sight — see §5 */;

// 2. think:
let thought = agent.think(body, visible);

// 3. carry out the action (see §6 for every variant):
match thought.action {
    Action::Move(dir) => { /* move NPC one cell */ }
    Action::Eat(id)   => { /* if entity `id` is adjacent & food, consume it */ }
    // …
    _ => {}
}

// 4. (optional) surface the inner life:
println!("{}: {}", agent.name(), thought.inner);   // first-person line for UI/log
// thought.goal (GoalKind), thought.dominant_drive (Drive), thought.process — for debug
```

For events that happen to the NPC but are **not** its own action (a predator hit
it, a peer died, someone spoke to it), call `agent.observe(event)` before the next
`think`; they are delivered on that next tick. See §8 for the `WorldEvent` list.

### B. Driver (let the SDK orchestrate)

Implement two traits on whatever owns your game state, then call `step` per agent:

```rust
impl Senses for MyWorld {
    fn body(&self, a: EntityId) -> SelfState { /* mapping #1a */ }
    fn visible(&self, a: EntityId) -> Vec<Entity> { /* mapping #1b */ }
}
impl Actuator for MyWorld {
    // mapping #2: carry out the action, RETURN the events it produced
    // (e.g. WorldEvent::Ate) so the mind learns; empty Vec if none.
    fn apply(&mut self, a: EntityId, action: &Action) -> Vec<WorldEvent> { … }
}

let thought = step(&mut agent, &mut world);   // perceive → think → act → feed events back
```

`step` feeds the events returned by `apply` back into the agent automatically;
world-driven events still go through `agent.observe`.

A complete, runnable world built on **only this crate** lives at
`crates/daimon-sdk/examples/minimal.rs` — run `cargo run -p daimon-sdk --example
minimal`. Read it; it is the canonical reference.

## 5. `SelfState` — the body you supply each tick

`SelfState::new(pos)` fills sensible defaults (full vitals); set the fields you
model. Fields:

- `pos: Pos` — grid cell. `Pos::new(x, y)`, `i32`.
- `health`, `energy`, `hydration: f32` — `0.0..=1.0`. The core homeostatic drives.
- `enclosure: f32` (default `0.0`) — how walled-in the cell feels (`0` open, `1`
  fully ringed). Only set it if you model building/shelter.
- `shelter_gap: Option<Dir>` (default `None`) — best open side to wall next.
- `season: u8`, `winter_in: f32`, `carrying: f32`, `gather_dir`/`store_dir:
  Option<Dir>` — only for the open-world/provisioning faculty; leave defaults
  otherwise. (Defaults keep these faculties inert, so they cost nothing.)

## 6. `Action` — the complete vocabulary the mind can return

A Daimon can only affect the world through these. Map each onto your verbs;
ignoring one is fine (it just won't happen).

| Variant | Meaning | You should… |
|---|---|---|
| `Move(Dir)` | step one cell (`Dir::{North,South,East,West}`) | move the NPC; block on walls/edges |
| `Eat(EntityId)` | eat an adjacent/co-located Food | restore energy if `id` is adjacent food; emit `Ate` |
| `Drink(EntityId)` | drink adjacent Water | restore hydration; emit `Drank` |
| `Talk { to, text }` | speak to another agent | deliver to `to` as `Heard`/`Told` |
| `Inspect(EntityId)` | study a Curio (curiosity) | no world change needed; reward is internal |
| `Strike(EntityId)` | hit an adjacent threat | apply your combat; emit `Repelled` if driven off |
| `Build(Pos)` | place a wall on an adjacent empty cell | add a wall there (makes it solid) |
| `Gather` | harvest surplus provisions (open-world) | add to `carrying`; inert if unused |
| `Store` | deposit `carrying` into the granary | fill your cache; inert if unused |
| `Rest` | recover a little energy | small energy regen |
| `Wait` | do nothing this tick | nothing |

`action.verb()` gives a short string. `action.is_mutating()` is `true` for every
action except `Move`/`Rest`/`Wait` — it treats `Inspect` and `Talk` as deliberate
*interactions*, not pure locomotion, even though `Inspect` needs no world change on
your side. Use it as a read-only circuit breaker: clamp a misbehaving agent to the
non-mutating subset (`Move`/`Rest`/`Wait`).

## 7. Determinism — the rules that keep it reproducible

Determinism is Daimon's headline feature. To keep it:

- **Give each NPC a distinct seed.** Two agents sharing a seed will behave
  *identically* the moment they share a percept (e.g. stand on the same cell). Use
  e.g. `base_seed ^ (id as u64)`.
- **Advance agents in a fixed order each tick** (e.g. by id). Random iteration
  order ⇒ non-reproducible runs.
- **Feed percepts consistently.** Same `(seed, percept stream)` ⇒ same life.
- Do not introduce wall-clock time or `rand` into how you build percepts; seed
  any of your own world randomness too.

## 8. `WorldEvent` — what you tell the mind happened

Emit these (return them from `Actuator::apply`, or push via `agent.observe`):
`Ate{id,energy}`, `Drank{id}`, `Hurt{id,health}`, `Repelled{id}`,
`Heard{from,text}`, `Told{from,info}`, `Spoke{to,text}`, `Discovered{id}`,
`Vanished{id}`, `Died{id,pos,cause}`. The mind builds episodic memory from these;
`Died` is what triggers grief (only emit it if you model mortality).

`Entity { id, kind, pos, label }` where `kind: EntityKind` is one of `Food`,
`Water`, `Agent`, `Predator`, `Curio`. `label` is free text for narration/memory.

## 9. Save / load

`agent.save() -> String` (JSON of the mind); `Agent::load(id, name, &json) ->
Option<Agent>`. Save right after a `think` (don't strand queued events). The mind's
episodic clock is preserved in the JSON; the cosmetic percept counter resets.

## 10. Honest scope — set expectations correctly

- **Best fit:** simulation-shaped games — colony/survival/immersive/social sims,
  roguelikes — where believable autonomous agents are the point.
- **Not an LLM.** Dialogue (`Talk`/`thought.inner`) is templated text, not
  free-form language generation.
- **No vision/raycasting.** You curate `visible` yourself (pick the radius,
  occlusion). The world Daimon reasons over is a **discrete grid of cells**.
- **Advanced faculties are off by default** (building, mortality, grief,
  provisioning, neural overlay, …). They live behind `Genome` genes; opt in
  deliberately. The default agent is a believable forager/explorer with safety,
  social, and curiosity drives.
- Not a drop-in for, e.g., a shooter's combat AI.

## 11. Common mistakes (don't do these)

- ❌ Reusing one seed for every NPC → clones that lock-step. Give each its own.
- ❌ Letting Daimon mutate your world. It returns an `Action`; **you** apply it.
- ❌ Forgetting to feed back outcomes → the mind can't learn (use `apply`'s return
  value or `observe`).
- ❌ Inventing entity ids inside percepts → always use the ids your world owns.
- ❌ Expecting free-form chat or pixel-level perception → see §10.

---

For the science behind the behaviours (drives, dual-process deliberation, the
believability harness, the evolution results), see `RESEARCH.md`. For the API in
code, read `crates/daimon-sdk/src/lib.rs` (rustdoc) and the runnable
`examples/minimal.rs`.
