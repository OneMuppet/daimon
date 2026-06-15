# Architecture — a code map

This is the developer's companion to `WHITEPAPER.md`: where each idea lives in
the code, and how a percept becomes an action. Read the white paper for *why*;
read this for *where*.

## The one-tick data flow

```
World::step(action) ─▶ Percept ─▶ Mind::cycle(&percept) ─▶ Thought{ action, inner } ─▶ World::step(...)
```

`Mind::cycle` (in `crates/daimon-mind/src/mind.rs`) is the spine. Its seven
steps, and the code each one touches:

| Step | Method | Reads | Writes |
|---|---|---|---|
| 1 perceive | `world.integrate` + `observe_surroundings` | percept | `WorldModel`, spatial `Memory`, `TheoryOfMind` |
| 2 appraise | `appraise` + `record_events` | percept, beliefs | `DriveSystem`, episodic `Memory`, lessons; returns *surprise* |
| 3 reflex | `reflex_check` | nearest threat, persona | short-circuits to a flee `Plan` if a predator is within `persona.reflex_distance()` |
| 4 decide | `decide` → `should_escalate` → (`fast_goal` \| `Deliberator`) → `apply_commitment` | drives, beliefs, memory, social | a `Goal`, a `Process` tag, a rationale; the `committed` intention |
| 5 plan | `ensure_plan` → `planner::plan_for` | goal, beliefs, memory | the current `Plan` |
| 6 act | `next_action` | the plan | pops the next `Action` |
| 7 reflect | `reflect` (every `reflect_interval`) | skills, places, social | semantic `Memory` facts |

## Where each concept lives

| Concept (white paper §) | Type / function | File |
|---|---|---|
| Determinism (seeded PRNG) | `Rng` (SplitMix64) | `daimon-core/src/rng.rs` |
| Embodiment / space | `Pos`, `Dir`, `Entity`, `SelfState` | `daimon-core/src/types.rs` |
| Perception (§3.1) | `Percept`, `WorldEvent` | `daimon-core/src/percept.rs` |
| Belief / world model (§3.1) | `WorldModel`, `Belief` (confidence decay) | `daimon-core/src/world_model.rs` |
| Drives (§3.2) | `Drive`, `DriveSystem` (dominant = urgency × weight) | `daimon-core/src/drive.rs` |
| Memory (§3.3) | `Memory` (episodic/semantic/procedural/spatial), `Episode`, `Fact`, `Skill`, `Place` | `daimon-core/src/memory.rs` |
| Goals & plans (§3.4) | `Goal`, `GoalKind`, `Plan` | `daimon-core/src/goal.rs` |
| Bounded action surface (§3.10/§8) | `Action` (`is_mutating`, `verb`) | `daimon-core/src/action.rs` |
| Cognitive cycle + escalation (§3.5) | `Mind`, `MindConfig`, `should_escalate` | `daimon-mind/src/mind.rs` |
| Commitment / hysteresis (§3.4) | `apply_commitment`, `COMMIT_MARGIN`, `CRITICAL` | `daimon-mind/src/mind.rs` |
| The LLM seam (§3.6) | `Deliberator` trait, `DeliberationContext`, `HeuristicDeliberator` | `daimon-mind/src/deliberate.rs` |
| Planning (§3.9) | `plan_for`, `path_to`, `wander` | `daimon-mind/src/planner.rs` |
| Theory of mind (§3.7) | `TheoryOfMind`, `AgentModel` | `daimon-mind/src/theory_of_mind.rs` |
| Persona (§5) | `Persona` (boldness/sociability/curiosity/creed) | `daimon-mind/src/persona.rs` |
| Expression / legibility (§3.10) | `Thought`, `Process` (`REFLEX`/`Routine`/`Deliberate`) | `daimon-mind/src/thought.rs` |
| World testbed (§5) | `World`, `WorldConfig` | `daimon-world/src/lib.rs` |
| Demo + life review (§5) | `main`, `life_in_review` | `daimon-demo/src/main.rs` |

## The escalation policy in one place

`Mind::should_escalate` (the §3.5 core) returns `true` — i.e. invoke System 2 —
when **any** of:

- `surprise >= surprise_threshold` (prediction error; *overrides the cooldown*),
- the two strongest drive pressures are within `tie_margin` (ambiguity), or
- the dominant pressure exceeds the high-stakes threshold,

**and** the `deliberation_cooldown` since the last slow call has elapsed (unless
surprise forced it). Everything else this tick is handled by `fast_goal` (System
1) or a held commitment — which is why deliberation lands on ~10% of ticks.

## Swapping in a real LLM

Implement `Deliberator` and hand it to `Mind::with(persona, seed, Box::new(my), cfg)`:

```rust
struct LlmDeliberator { /* http client, model id */ }

impl Deliberator for LlmDeliberator {
    fn name(&self) -> &'static str { "claude" }
    fn deliberate(&mut self, ctx: &DeliberationContext) -> Deliberation {
        // 1. render ctx (drives, recalled memories, beliefs, social models,
        //    surprise) into a prompt
        // 2. ask the model for { goal, rationale, lessons } (ReAct/Reflexion/ToT)
        // 3. parse and return — the rationale becomes the agent's monologue
    }
}
```

Nothing else in the architecture changes. The offline `HeuristicDeliberator`
exists so the repository stays deterministic and network-free; it computes the
same `Deliberation` from a transparent utility calculation.
