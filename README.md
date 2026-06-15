<div align="center">

<img src="assets/banner.png" alt="Daimon: Smallworld — an island of autonomous minds, rendered in real time" width="100%" />

<h1>◍&nbsp; Daimon</h1>

<b>Game agents that feel like minds.</b><br/>
<i>A self-authoring cognitive architecture for autonomous NPCs — agents that invent their own<br/>concepts, goals, world-model, and even their values from lived experience.</i>

<br/><br/>

![Rust](https://img.shields.io/badge/Rust-CPU--only%20·%20no%20neural%20nets-CE4B0E?logo=rust&logoColor=white)
![WebGPU](https://img.shields.io/badge/render-wgpu%2029%20·%20WebGPU-5B3DF0)
![Proven](https://img.shields.io/badge/theorems-9%20machine--checked-2E9E5B)
![Tested](https://img.shields.io/badge/harness-39%20criteria%20·%2066%20tests-EF6A3D)
![License](https://img.shields.io/badge/license-MIT-3A86C8)

**[Read the paper →](RESEARCH.md)** &nbsp;·&nbsp; [Theorems](PROOFS.md) &nbsp;·&nbsp; [PDF](Daimon-RESEARCH.pdf) &nbsp;·&nbsp; [Whitepaper (narrative)](WHITEPAPER.md)

</div>

---

Daimon is a cognitive architecture for
autonomous NPCs: an agent that *wants* things (a homeostatic + intrinsic drive
system), *remembers* (episodic, semantic, and procedural memory), *learns*
(reflection and a growing skill library), *models the people it meets* (theory
of mind), and — crucially — *narrates itself*, so the intelligence is legible
rather than merely asserted. It thinks **fast** almost always and **slow** only
when it matters, which is what makes a language-model brain affordable at NPC
scale.

It is not AGI. It is an **illusion engine** for the *felt experience* of a
general mind — autonomy, continuity, social awareness, adaptation, and coherence
over a long life — engineered from well-understood parts, arranged correctly.

```
                          ┌──────────── the body (world-owned) ───────────┐
                          │  position · health · energy · hydration · view │
                          └───────────────────────┬───────────────────────┘
                                                   │ percept
                                                   ▼
   ┌──────────────────── one cognitive cycle (per tick) ─────────────────────┐
   │ 1 perceive → 2 appraise → 3 reflex? → 4 decide → 5 plan → 6 act          │
   │  beliefs +    drives +     predator    System 1 fast  re-plan   bounded  │
   │  memory +     surprise     at arm's     OR System 2   if stale   action  │
   │  social map   (novelty)    reach?       (LLM) — only                     │
   │                                         when surprised/risky/torn        │
   │            7 reflect (every N ticks): consolidate experience → knowledge │
   └─────────────────────────────────────────────────────────────────────────┘
```

## Why

Thirty years of game AI has been state machines and behaviour trees: they encode
the *designer's* intentions, never the *agent's*. The NPC has no standpoint, no
memory of you, no inner life, and cannot surprise its own creator. Players notice
— and the industry is racing to fix it (Stanford's Generative Agents, DeepMind's
SIMA, NVIDIA ACE, Inworld). What's missing is an architecture that is **at once**
believable, lifelong, socially aware, *and* cheap enough to run for a thousand
NPCs in real time. That's Daimon.

The load-bearing idea is a **dual-process controller with a rate-limited
escalation policy**: cheap reflexive cognition every tick; expensive
deliberation (an LLM in production) only when the agent is *surprised*, in
*danger*, or genuinely *uncertain*. In our runs that's ~10% of ticks — the
difference between "one model call per NPC per frame" (impossible) and "one per
*interesting moment*" (affordable).

Read **[`RESEARCH.md`](./RESEARCH.md)** — the full technical report (v1.0): the
mechanism formalism, the 39-criterion falsifiable evaluation, the **nine
machine-checked theorems** ([`PROOFS.md`](./PROOFS.md)), benchmarks, and a complete
reference list grounded in the literature (Generative Agents, Voyager, ReAct,
Reflexion, BDI, Kahneman's two systems, Schmidhuber/Pathak curiosity, ToMnet,
GOAP/HTN, quantum cognition, neural criticality). [`WHITEPAPER.md`](./WHITEPAPER.md)
is the original vision narrative.

## Quick start

```bash
cargo run --bin daimon                       # Kael, a balanced wanderer, 200 ticks
cargo run --bin daimon -- --persona social   # Mira, who courts risk to be near others
cargo run --bin daimon -- --persona curious --ticks 400
cargo run --bin daimon -- --persona bold --quiet   # just the end-of-life review
cargo test                                   # 66 tests across the workspace
```

You'll watch the agent live — switching goals as its needs shift, fleeing on
instinct, stopping to *think* (`SLOW`) at hard moments, befriending townsfolk,
learning where water is — and then read its **life in review**: the facts it
came to believe, the skills it grew into, the people it met and how it felt about
them, and its most vivid memories.

## One engine, distinct minds

Five personas differ only in three trait scalars (boldness, sociability,
curiosity) over identical code. Run in the same world for 400 ticks, they live
measurably different lives — none of it scripted:

| Persona | Outcome | Conversations | Reads as… |
|---|---|---|---|
| Kael (balanced) | thrives, 100% health | 15 | a steady survivor |
| Vell (curious) | thrives, 100% health | 0 | a focused loner, obsessed with the shrine |
| Sela (timid) | thrives, 96% health | 27 | cautious, sociable, careful |
| Mira (social) | survives, 79% health | 64 | a connector who courts risk for company |
| Roin (bold) | survives, 33% health | 0 | reckless; runs hot, near the edge |

## What's enforced by the architecture

1. **Autonomy** — behaviour springs from internal drives, not player triggers.
2. **Continuity** — a persistent persona and an accreting autobiographical
   memory; forgetting evicts the *least salient*, so vivid moments persist.
3. **Generality** — sensible action in unscripted situations, via utility
   arbitration + deliberation rather than enumerated cases.
4. **Social intelligence** — per-agent models with relationship history.
5. **Adaptation** — reflection distils experience into facts and trusted skills.
6. **Legibility** — every decision is narrated, tagged by which mind made it
   (`REFLEX` / fast / `SLOW`).

## Components

| Crate | Role |
|---|---|
| `daimon-core` | Cognitive type system: percepts, beliefs, drives, memory, goals, actions. `serde`-only — a whole mind is a serialisable value. |
| `daimon-mind` | The cognitive cycle, escalation policy, commitment, planner, theory of mind, and the `Deliberator` seam (where an LLM plugs in). |
| `daimon-world`| A tiny deterministic grid world (food, water, curios, a stalking predator, townsfolk) — a testbed, not a game. |
| `daimon-demo` | The `daimon` binary: a narrated life + end-of-life review. |
| `daimon-game`| **Daimon: Smallworld** — a wgpu 29 game; a village of real minds you can watch and tend (native + web). See `crates/daimon-game/README.md`. |

## The game — Daimon: Smallworld

`daimon-game` puts the architecture on screen: six real Daimon minds share one
world, rendered with **wgpu 29** as a **real 3-D isometric island** — procedural
terrain with a day/night cycle, seasons, and weather, the minds glowing with
their moods — drawn low-res and upscaled for a painterly pixel look, with a crisp
glyphon HUD. Click any agent to open a **live mind inspector** — its current
thought, drive bars, skills, relationships, and vivid memory, updating as it
thinks. (The banner above is a real frame from this renderer.)

```bash
cargo run -p daimon-game --release                       # native (3-D village)
cargo run -p daimon-game --example proofs    --release   # the 9 machine-checked theorems
cargo run -p daimon-game --example believability --release  # all 39 ablation criteria
cargo run -p daimon-game --example autogenesis --release # the self-improvement loop
cargo run -p daimon-game --example study     --release   # render-free behavioural field study
cargo run -p daimon-game --example banner    --release   # render the README hero (assets/banner.png)
./scripts/build-web.sh                                    # WebGPU build (see game README)
```

## The LLM seam

System 2 is one trait:

```rust
pub trait Deliberator {
    fn deliberate(&mut self, ctx: &DeliberationContext) -> Deliberation;
}
```

`DeliberationContext` is exactly what you'd render into a prompt (drives, salient
memories, beliefs, social models, surprise). The repo ships an offline
`HeuristicDeliberator` so everything is deterministic and network-free; an
`LlmDeliberator` backed by **Claude** implements the same trait, and ReAct /
Reflexion / Tree-of-Thoughts slot in behind it unchanged. Because the escalation
policy rate-limits the slow path, a production Daimon makes only a handful of
model calls per agent per minute.

## Determinism

Every stochastic choice flows from one seeded PRNG. Same seed → the same life,
byte for byte (`life_is_reproducible_from_seed` is a passing test). Emergent
behaviour is testable, not just anecdotal.

## Status

A faithful, runnable **concept** — not a shippable runtime. The full cognitive
cycle, memory, theory of mind, persona diversity, and the LLM seam are built and
tested; the LLM-backed deliberator, a learned world model, real-engine
embodiment, pixel perception, and multi-agent scale are the documented next
steps. See `WHITEPAPER.md` §7 and `ROADMAP.md`.

## Governance

A fleet of autonomous, acting NPCs is a fleet of non-human actors. Daimon
constrains each by construction (a bounded action surface, a reflexive override).
For production, formalise it with the sister project **Reins** — a governance
mesh that holds the reins of agentic systems with identity, budgets,
human-in-the-loop thresholds, a kill-switch, and a hash-chained audit trail.

## License

MIT. © 2026 David Borgenvik. See [`LICENSE`](./LICENSE) and `PROVENANCE.md` for clean-room provenance.
