# daimon-sdk

**Drop a deterministic, drive-driven mind into any game.**

Daimon gives a character a mind that forms its own goals from felt drives —
hunger, thirst, safety, social, curiosity — learns what things are for, remembers,
fears death, and grieves. Nothing is scripted. It's **deterministic** (same seed →
same life), **CPU-only**, and **pure Rust** — no model weights, no GPU, no network,
no per-frame inference cost.

```rust
use daimon_sdk::prelude::*;

let mut npc = Agent::new(EntityId(1), Persona::new("Mara").with_curiosity(0.9), 42);

// each tick: tell it what it senses, get back what it decided
let thought = npc.think(SelfState::new(Pos::new(5, 5)), visible_entities);
match thought.action {
    Action::Move(dir) => { /* move the NPC */ }
    Action::Eat(id)   => { /* eat entity `id` if adjacent */ }
    _ => {}
}
println!("{}: {}", npc.name(), thought.inner);   // a first-person line, free
```

You write just **two mappings**: your world → what the agent senses, and the
chosen `Action` → effects in your world. That's the whole integration.

## Run the example

```
cargo run -p daimon-sdk --example minimal
```

A tiny world — food, water, a roaming predator — with three personalities that
diverge from identical code. Run it twice: the numbers don't move.

## Two ways to drive it

- **Low-level:** call `agent.think(body, visible)` and apply `thought.action`
  yourself.
- **Driver:** implement `Senses` + `Actuator` on your world and call
  `step(&mut agent, &mut world)` — it perceives, thinks, acts, and feeds outcomes
  back so the mind learns.

## Tune your cast

- `Persona` — per-character temperament (`boldness`, `sociability`, `curiosity`,
  a `creed`). One engine, a whole cast.
- `Genome` — which faculties exist (building, mortality, grief, provisioning, …),
  most **off by default**. Ship a champion you bred with the evolution tools via
  `Agent::from_genome`.

## Good fit / not

Built for simulation-shaped games — colony / survival / immersive / social sims,
roguelikes. It is **not** an LLM (templated dialogue, not free-form language), does
no raycasting (you curate what's visible), and reasons over a discrete grid.

## More

- **`AGENTS.md`** (repo root) — the precise integration guide, written for coding
  agents; the complete `Action`/`WorldEvent`/`SelfState` vocabulary and the
  determinism rules.
- **`RESEARCH.md`** — the science: drives, dual-process deliberation, the
  believability harness, and the evolution results.
