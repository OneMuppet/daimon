# Daimon — a cognitive architecture for game agents that feel like minds

> **⚠️ Superseded.** This is the original vision white paper (v0.1, 2026-06-13).
> The canonical, peer-review-ready technical report — with the full mechanism
> formalism, the 39-criterion evaluation, the nine machine-checked theorems
> (§4.5), benchmarks, and a complete reference list — is **`RESEARCH.md`
> (Version 1.0)**. Read that for citable results; this file is kept for the
> motivating narrative only.

**A white paper on building autonomous, believable, AGI-*feeling* game AI — and a working Rust concept that demonstrates it.**

Version 0.1.0 · 2026-06-13

---

## Abstract

Most game "AI" is a puppet: a state machine or behaviour tree that reacts to the
player and resets when unobserved. It can be tuned to look competent, but it
never reads as a *mind* — it has no goals of its own, no memory of you, no
inner life, and no capacity to surprise its own designers. **Daimon** is an
architecture for the opposite: an agent that wants things, remembers, learns,
models the people it meets, and — crucially — *narrates itself*, so the
intelligence is legible rather than merely asserted.

Daimon is not a claim to artificial general intelligence. It is a claim that the
*felt experience* of generality — autonomy, continuity, social awareness,
adaptation, and coherence over a long life — can be engineered today from
well-understood parts, arranged correctly, with a large language model used
surgically rather than constantly. The arrangement is the contribution. The
load-bearing idea is a **dual-process controller with a rate-limited escalation
policy**: cheap reflexive cognition runs every tick; expensive deliberation (an
LLM in production) is invoked only when the agent is *surprised*, in *danger*,
or genuinely *uncertain* — a few times per agent per minute. That is the
difference between a tech demo and something a studio can afford to run for a
thousand NPCs.

This paper specifies the architecture, grounds each part in the published
literature, and describes a complete, deterministic, offline Rust
implementation (in this repository) in which five agents sharing one engine
live measurably different lives.

---

## 1. Why — the puppet problem

For thirty years the game-AI toolbox has been finite-state machines, behaviour
trees, and planners. They are excellent engineering — F.E.A.R.'s soldiers
flanking via Goal-Oriented Action Planning (Orkin, 2006) still hold up — but
they encode the *designer's* intentions, not the *agent's*. The agent has no
standpoint. Turn the player's back and it freezes; it cannot tell you why it did
anything; it never learns; meet it tomorrow and it has forgotten you.

Players notice. And the market has decided this is the next frontier:

- Surveyed developers are overwhelmingly bullish: **~75% of game developers are
  excited about AI NPCs**, and roughly half expect that within five years more
  than 40% of studios will be building with them (Inworld × Censuswide, 2024 —
  vendor-sponsored).¹
- Player appetite is near-universal in the same vendor's study: **99% of gamers
  said advanced AI NPCs would improve some aspect of gameplay**.²
- The money is following: the *generative-AI-in-gaming* segment is projected to
  grow from **~$1.1B (2023) to ~$11.1B by 2033** (Market.us; market-sizing
  firms disagree widely, so treat any single figure as directional).³

And the lab results have arrived. Stanford's **Generative Agents** populated a
sandbox town with 25 LLM-driven characters that remembered, reflected, planned,
and spread information believably (Park et al., 2023).⁴ **Voyager** played
Minecraft for hours, accumulating a reusable skill library and setting its own
curriculum (Wang et al., 2023).⁵ DeepMind's **SIMA** followed natural-language
instructions across many commercial 3D games from pixels alone (2024–2025).⁶
Altera's **Project Sid** ran 1,000+ agents that formed professions, trade, and
culture in Minecraft (2024).⁷ Production stacks — NVIDIA ACE, Inworld — are
already shipping agentic NPCs in real titles.⁸

What is *missing* is an architecture that ties these threads into something a
studio can ship: autonomous and believable like Generative Agents, lifelong and
self-improving like Voyager, socially aware, **and** cheap enough to run at NPC
scale in real time. That is the gap Daimon targets.

---

## 2. What "feels like AGI" actually decomposes into

"AGI" is a contested term and Daimon does not attempt it. But the *impression*
of a general mind, in a game, is concrete and decomposable. We treat these six
properties as the design requirements:

| # | Property | What it means in a game | Failure mode it cures |
|---|---|---|---|
| 1 | **Autonomy** | The agent pursues its *own* goals from internal motivation, unprompted. | Puppets that only react to the player. |
| 2 | **Continuity** | A persistent identity and autobiographical memory across a long life. | Amnesiac NPCs that reset off-screen. |
| 3 | **Generality** | Sensible behaviour in situations the designer never scripted. | Brittle scripts that break off the happy path. |
| 4 | **Social intelligence** | Models other agents' goals and feelings; relationships have history. | Other characters treated as furniture. |
| 5 | **Adaptation** | Learns from experience within a life — forms skills, revises beliefs. | Static difficulty; never gets wiser. |
| 6 | **Legibility** | Behaviour reads as *intentional*; the agent can explain itself. | Competent-but-opaque AI that feels random. |

The sixth is the one practitioners underrate. An agent making good decisions
*opaquely* feels mechanical; an agent making merely-adequate decisions while
**telling you why** feels alive. Legibility is a force multiplier on every other
property — so in Daimon it is a first-class output, not an afterthought.

---

## 3. The architecture

A Daimon runs a fixed **cognitive cycle** once per game tick. The world hands it
a percept; it hands back one bounded action and one legible thought.

```
                          ┌──────────────────── the body (world-owned) ───────────────────┐
                          │   position · health · energy · hydration · what's in view       │
                          └───────────────────────────────┬──────────────────────────────┘
                                                           │ percept
                                                           ▼
   ┌───────────────────────────────── one cognitive cycle (per tick) ─────────────────────────────────┐
   │                                                                                                    │
   │  1 PERCEIVE        2 APPRAISE             3 REFLEX?        4 DECIDE              5 PLAN     6 ACT     │
   │  integrate    ─▶   drives +        ─▶    predator    ─▶   System 1: arbitrate ─▶ (re)plan ─▶ bounded │
   │  into beliefs      surprise              at arm's        │  or                  if stale    action  │
   │  + memory          (interoception        reach?         System 2: deliberate              ──────────┤
   │  + spatial map     + novelty +           ── yes ──▶ flee   (LLM in prod) — only            │ to world│
   │  + social model    bad events)           ── no ──▶ ↓     when surprised/risky/torn         │         │
   │                                                                                            ▼         │
   │                                            7 REFLECT (every N ticks): consolidate episodes ──────────┤
   │                                            into facts, skills, relationships, self-concept           │
   └────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

This is the **Belief–Desire–Intention** loop (Bratman, 1987; Rao & Georgeff,
1995)⁹ wearing a **dual-process** coat (Kahneman, 2011; Booch et al., *Thinking
Fast and Slow in AI*, 2021).¹⁰ Beliefs come from perception; desires from the
drive system; intentions are committed plans. What makes it run at scale is the
explicit, budgeted escalation from System 1 to System 2 in step 4.

The subsystems, each with its grounding:

### 3.1 Perception → belief (a fallible world model)

Perception is partial and fleeting; cognition needs persistence. Each percept is
folded into a **world model** — beliefs about what exists and where — that
*decays in confidence* when things leave view (object permanence with doubt) and
is *forgotten* when stale. The agent plans against this model, not against
ground truth, which is precisely why it can be wrong, mistaken, and surprised —
all prerequisites for looking like it is thinking rather than reading the
simulation's memory.

### 3.2 Drives — the motivational core (autonomy)

A scripted NPC does what it is told; a Daimon does what it *wants*. Wanting comes
from a small **homeostatic drive system** — survival, hunger, thirst, curiosity,
social, mastery — each a scalar urgency that drifts over time and reacts to the
percept. The dominant drive shapes the goal. Nobody scripts "go eat"; hunger
simply wins arbitration. This is classic drive-reduction motivation (Hull) with
Maslow-style prioritisation.

The **curiosity** drive is the important one. It is an *intrinsic* reward for
novelty and surprise, in the spirit of Schmidhuber's formal theory of curiosity
(reward for improvements to the world model)¹¹ and Pathak et al.'s Intrinsic
Curiosity Module (reward as forward-model prediction error).¹² Intrinsic
motivation is what stops a well-fed agent from standing still — the engine of
open-ended, self-directed behaviour. Framed in active-inference terms, the agent
acts to reduce its expected surprise while seeking the *learnable* surprise that
improves its model (Friston, 2010).¹³

### 3.3 Memory — episodic, semantic, procedural (continuity)

Believability is mostly memory. Daimon keeps the standard tripartite store, in
the "memory stream + reflection" shape of Generative Agents⁴ and the
declarative/procedural division of SOAR and ACT-R:¹⁴

- **Episodic** — a time-ordered stream of events, each scored by *salience*;
  recall blends recency, salience, and relevance (the Generative-Agents
  retrieval function), and forgetting evicts the *least salient*, not merely the
  oldest. Vivid moments — the predator strike — persist; the dull commute fades.
- **Semantic** — durable facts distilled by reflection ("there's water at
  (27,15)", "predators are dangerous"), with confidence that rises as evidence
  accrues.
- **Procedural** — a **skill library** of named, reusable procedures whose
  competence is tracked by success rate, directly after Voyager.⁵ Skills that
  keep working become trusted; ones that keep failing are abandoned.

A **spatial map** lets the agent navigate back to resources long out of sight —
the difference between foraging from a mental map and only reacting to what is
in front of you.

### 3.4 Goals, intentions, and commitment (generality without dithering)

A drive is a pressure; a **goal** is a concrete intention adopted to relieve it;
a **plan** is the ordered actions pursuing it. The subtle, essential piece is
**commitment**: an intention must persist (Bratman's central thesis⁹) or a mind
with two near-equal needs *dithers* between them and achieves neither — it takes
one step toward water, one toward food, and starves between them. Daimon commits
to a goal with **hysteresis**: it switches only when a rival drive *clearly*
outweighs the current one, or when the current need is satisfied, or when some
other need becomes *critical*. (We observed the dithering failure directly in
development and added commitment to fix it — see §5.)

### 3.5 Dual-process cognition and the escalation policy (the load-bearing idea)

Routine life runs on **System 1**: hard-wired reflexes (a predator at arm's
reach triggers flight with no deliberation) and cheap utility arbitration over
the obvious choice. **System 2** — slow, expensive, general — is consulted only
when an explicit **escalation policy** fires:

> Escalate to deliberation when **surprise** is high (the world violated
> prediction), **or** stakes are high (a strong survival need), **or** the choice
> is genuinely **ambiguous** (the two strongest drives are near-tied) — subject
> to a **cooldown** that rate-limits the slow path, which only a true emergency
> may override.

This is the practical core of "thinking fast and slow in AI" (Booch et al.,
2021;¹⁰ DeepMind's Talker–Reasoner architecture, 2024¹⁵). It is also what makes
the design *economic*. In our runs, deliberation fires on **~10% of ticks**; the
other 90% is nearly free. A production Daimon backing System 2 with a language
model therefore makes only a handful of model calls per agent per minute —
turning "one LLM call per NPC per frame" (impossible at scale) into "one LLM
call per NPC per *interesting moment*" (affordable for a fleet).

### 3.6 The deliberator seam — where the LLM plugs in

System 2 sits behind a single trait:

```rust
pub trait Deliberator {
    fn deliberate(&mut self, ctx: &DeliberationContext) -> Deliberation;
}
```

`DeliberationContext` is a serialisable snapshot of the situation — drives,
salient memories, beliefs, social models, surprise — i.e. *exactly what you
render into a prompt*. The deliberator returns a goal **and a rationale** (the
chain of thought, surfaced verbatim as the agent's inner monologue) and any
**lessons** to write to memory.

The concept ships an offline `HeuristicDeliberator` (a transparent utility
calculation, so the repo builds and runs deterministically with zero network).
But the seam is the point: an `LlmDeliberator` implements the same trait with a
call to a large language model, and the major agentic-reasoning methods slot in
unchanged behind it —

- **ReAct** (Yao et al., 2023): interleave reasoning and acting — the natural
  shape of a single deliberation.¹⁶
- **Reflexion** (Shinn et al., 2023): verbal self-critique after failure, stored
  in episodic memory — Daimon already records lessons ("predators are
  dangerous") on bad outcomes; an LLM writes far richer ones.¹⁷
- **Tree of Thoughts** (Yao et al., 2023): branch-and-evaluate search for the
  genuinely hard, high-stakes deliberations the escalation policy reserves for
  System 2.¹⁸

### 3.7 Theory of mind (social intelligence)

For every other agent it meets, a Daimon keeps a lightweight model: where it was
last seen, a **disposition** that updates from interactions (warm words warm the
relationship; cold ones sour it), and a guess at its goal. This is the tractable
cousin of Rabinowitz et al.'s *Machine Theory of Mind* — infer another agent's
mental state from observed behaviour and act on it.¹⁹ Because disposition has
history, relationships do too: the character who shared food with you yesterday
is greeted differently from the one who told you to leave.

### 3.8 Reflection (adaptation)

Off the critical path, a periodic **reflection** pass consolidates recent
experience into durable knowledge — promoting competent skills into
self-knowledge, turning remembered resource sightings into stable facts,
recording standing relationships. This is exactly the "reflection" step of
Generative Agents,⁴ and in production it is another (infrequent) LLM pass that
writes higher-level beliefs back into memory.

### 3.9 Planning (the action layer)

Goals become actions through a humble, frequently-re-planned planner in the
tradition of game-AI planning — GOAP (Orkin, 2006),²⁰ HTN (Erol, Hendler & Nau,
1994),²¹ and behaviour trees (Colledanchise & Ögren, 2018):²² decompose a goal
into ordered primitives, but stay ready to throw the plan away when the world
shifts. In a world where the river can dry up and the predator keeps moving,
cheap frequent re-planning beats expensive optimal planning.

### 3.10 Expression (legibility)

Every cycle emits a **thought**: a first-person line stating the dominant need,
the decision, and — vital — *which mode of thinking produced it* (`REFLEX`,
fast `routine`, or deliberate `SLOW`). The dual process is made visible. A
player reading *"the predator startled me — no time to think, I run"* is looking
straight into the architecture. This is the believability multiplier of §2.

---

## 4. The escalation economics

The single number that decides whether agentic NPCs ship is **LLM calls per
agent per second**. Naively giving every NPC a language-model brain on every
frame is financially and latency-wise impossible at scale. Daimon's escalation
policy converts the problem:

| Approach | LLM calls / agent / minute (rough) | Feasible fleet size |
|---|---|---|
| LLM every frame (60 fps) | ~3,600 | ~1 |
| LLM every second | ~60 | tens |
| **Daimon: LLM per *interesting moment*** | **~3–10** (≈10% of ticks, debounced) | **hundreds–thousands** |

Everything System 1 handles — locomotion, routine foraging, reflexive flight,
holding a commitment — costs nothing but local compute. The model is spent only
where it changes the outcome: novel situations, hard trade-offs, social nuance,
recovering from surprise. This is the same philosophy as Reins' inline policy
gate (a sister project): do the cheap, certain thing in-process, and escalate
only the genuinely hard decision.

---

## 5. The concept — what we built, and what emerged

This repository is a complete, deterministic, **offline** Rust implementation of
the architecture: a Cargo workspace of four crates, ~3,400 lines (with unit
tests throughout) and a narrated demo binary. No network, no model weights — the
`HeuristicDeliberator` stands in for the LLM so the *architecture* is what you
observe, not a model's eloquence.

| Crate | Role |
|---|---|
| `daimon-core` | The cognitive type system: percepts, beliefs, drives, memory, goals, actions. `serde`-only; a whole mind is a serialisable value you can snapshot and replay. |
| `daimon-mind` | The cognitive cycle, escalation policy, commitment, planner, theory of mind, the `Deliberator` seam. |
| `daimon-world` | A tiny deterministic grid world (food, water, curios, a stalking predator, townsfolk) — a testbed, not a game. |
| `daimon-demo` | `daimon` — drops a mind into the world and prints its narrated life and an end-of-life review. |

**Determinism.** Every stochastic choice flows from one seeded PRNG. Same seed →
the same life, byte for byte. Emergent behaviour is therefore *testable*, not
merely anecdotal (`life_is_reproducible_from_seed` is a passing test).

**The headline result: one engine, distinct minds.** Five personas — differing
only in three trait scalars (boldness, sociability, curiosity) layered over
identical code — were each run for 400 ticks in the same world (seed `0xDA13`):

| Persona | Outcome | Meals | Conversations | Predator strikes | Read as… |
|---|---|---|---|---|---|
| **Kael** (balanced) | thrives, 100% health | 9 | 15 | 1 | a steady survivor |
| **Vell** (curious) | thrives, 100% health | 9 | **0** | 1 | a focused loner, obsessed with the shrine |
| **Sela** (timid) | thrives, 96% health | 9 | 27 | 1 | cautious, sociable, careful |
| **Mira** (social) | survives, 79% health | 10 | **64** | 2 | a connector who courts risk to be near others |
| **Roin** (bold) | survives, 33% health | 8 | 0 | 0 | reckless; runs hot, near the edge |

These differences were not scripted. They *emerge* from the same arbitration
acting on different trait biases — bold agents let the predator get closer and
pay for it in health; curious agents pour cycles into investigating the monolith
and neglect company; social agents repeatedly cross the map to talk and take
more hits doing it. This is the felt-generality of §2 falling out of the
mechanism.

A representative slice of inner life (Vell, the curious one):

```
t   7        [Vell] curiosity pulls hardest; I'll investigate → inspect   (×2)
t  16 SLOW   [Vell] (thinking) that old shrine is unlike anything I've catalogued
                     — I want to know what it is. I'll investigate. → inspect
t  28        [Vell] staying with what I started — investigate → inspect
t  40 SLOW   [Vell] (thinking) thirst is at 56%. I'll find water. → move
t  86 SLOW   [Vell] (thinking) a predator is 5 steps away; I've learned not to
                     gamble with that. I'll flee. → move
t  98 REFLEX [Vell] The predator is right there — no time to think, I run.
```

…and its end-of-life review reconstructs an autobiography: the facts it came to
believe (where each spring and berry patch is; *"predators are dangerous"*), the
skills it grew confident in (forage 100%, hydrate 100%), the people it met and
how it felt about them, and its most vivid (highest-salience) memories — which
are, correctly, the moments it was hurt.

**What development taught us (honestly).** The interesting bugs were
*cognitive*, not mechanical, and each fix is a thesis from §3:

1. The agent starved beside known food because consumed resources respawned
   *elsewhere*, invalidating its spatial memory → resources must be stable for a
   mental map to be worth having (§3.3).
2. It then starved while *resting*, because low health made the survival drive
   dominate and survival-without-a-predator wrongly meant "rest" → a depleted
   agent must address its actual deficit, not sit still (§3.2/3.4).
3. It then starved by *dithering* between equally-urgent hunger and thirst →
   **commitment with hysteresis** (§3.4). This is Bratman's argument for the
   persistence of intentions, rediscovered empirically.

That the failure modes of a believable agent are *psychological* — distraction,
indecision, neglect of needs — is itself evidence the architecture is operating
at the right level.

---

## 6. Why it feels like AGI (and why it isn't)

Daimon is an **illusion engine**, and we think that is the honest and useful
framing. It is not general intelligence. It has no language understanding of its
own (the offline build), no transfer across domains, no genuine grounding. What
it has is the *observable signature* of a mind:

- it acts from its own motives (autonomy),
- it remembers and is changed by what happens to it (continuity, adaptation),
- it treats other characters as agents with histories (social intelligence),
- it behaves sensibly in unscripted situations (generality), and
- it shows its reasoning, including *how hard* it is thinking (legibility).

When those five co-occur, persist over a long life, and stay *coherent* —
anchored to a stable persona and an accreting autobiography — a human observer
attributes a mind. That attribution is the product. With the `Deliberator` seam
filled by a frontier model (Claude), the deliberations stop being templated
utilities and become genuine open-vocabulary reasoning, conversation, and
planning — and the illusion narrows the gap to the real thing considerably,
*without* changing the architecture or the cost envelope.

---

## 7. Honest status — concept vs. production

This is a faithful, runnable concept, not a shippable runtime. What is real, and
what is deliberately stubbed:

- [x] Full cognitive cycle: perception, appraisal, drives, dual-process control,
      escalation policy, planning, commitment, action — built and tested.
- [x] Tripartite memory with salience-weighted recall, forgetting, skill
      competence, spatial map, reflection — built and tested.
- [x] Theory of mind with relationship history — built and tested.
- [x] Persona system yielding emergent behavioural diversity — demonstrated.
- [x] Deterministic, reproducible lives; 13 passing tests.
- [x] Pluggable `Deliberator` trait (the LLM seam) with an offline default.
- [ ] **LLM-backed deliberator** — the trait is defined and documented; wiring it
      to Claude (prompt rendering, structured parse, ReAct/Reflexion/ToT) is the
      first production step. Not built here (keeps the repo offline & deterministic).
- [ ] **Learned world/forward model** for principled surprise and planning in
      unknown dynamics (MuZero-style, Schrittwieser et al., 2020;²³ MCTS,
      Browne et al., 2012²⁴). Surprise here is a hand-tuned proxy.
- [ ] **Embodiment in a real engine** (Unity/Unreal/Bevy) — the action surface is
      designed for it, but the world is an abstract grid.
- [ ] **Perception from pixels** (SIMA-style⁶) rather than a structured percept.
- [ ] **Multi-agent culture at scale** (Project Sid / PIANO⁷) — single-agent here.
- [ ] **Persistence/serialisation of a live mind to disk** and authoring tools.

---

## 8. A note on governance

An autonomous, learning, LLM-driven NPC that can *act* in a shared world is a
non-human actor with a blast radius. Daimon's design already constrains it by
construction: a **bounded action surface** (the agent can only affect the world
through a small, validated `Action` enum) and a **reflexive override** that can
clamp behaviour (the same shape as a read-only circuit breaker). For production
— especially anything touching real economies, user-generated content, or other
players — that instinct should be formalised. The sister project **Reins** does
exactly this for agentic systems: a declarative per-agent contract enforced
out-of-process, with identity, scopes, budgets, human-in-the-loop thresholds,
kill-switch, and a hash-chained audit trail. A fleet of Daimons is precisely the
kind of fleet Reins is built to hold the reins of.

---

## 9. Provenance & related work

Daimon is an original, independently developed architecture and implementation,
created 2026-06-13. It composes widely published, public concepts —
belief–desire–intention agents, dual-process cognition, homeostatic and
intrinsic motivation, the memory-stream/reflection pattern, skill libraries,
theory-of-mind modelling, and game-AI planning — into one coherent design and a
clean-room Rust codebase. No source, documentation, or non-public material from
any third-party product was used; all systems referenced were known only through
their public papers and marketing. See `PROVENANCE.md`.

---

## References

1. Inworld AI / Censuswide, *The Future of Game Development with AI NPCs* (2024) — vendor-sponsored. https://inworld.ai/blog/future-of-game-development-with-ai-npcs-report
2. Inworld AI, *The Future of NPCs* (2024) — vendor-sponsored. https://inworld.ai/blog/future-of-npcs-report
3. Market.us, *Generative AI in Gaming Market* (2024). https://market.us/report/generative-ai-in-gaming-market/
4. Park, J. S., O'Brien, J. C., Cai, C. J., Morris, M. R., Liang, P., & Bernstein, M. S. (2023). *Generative Agents: Interactive Simulacra of Human Behavior.* UIST '23. https://arxiv.org/abs/2304.03442
5. Wang, G., Xie, Y., Jiang, Y., Mandlekar, A., Xiao, C., Zhu, Y., Fan, L., & Anandkumar, A. (2023). *Voyager: An Open-Ended Embodied Agent with Large Language Models.* TMLR. https://arxiv.org/abs/2305.16291
6. SIMA Team, DeepMind (2024). *Scaling Instructable Agents Across Many Simulated Worlds.* https://arxiv.org/abs/2404.10179 · SIMA 2 (2025): https://deepmind.google/blog/sima-2-an-agent-that-plays-reasons-and-learns-with-you-in-virtual-3d-worlds/
7. Altera.AL (2024). *Project Sid: Many-agent Simulations toward AI Civilization.* https://arxiv.org/abs/2411.00114
8. NVIDIA ACE for Games: https://developer.nvidia.com/ace-for-games · Inworld AI: https://inworld.ai/
9. Bratman, M. E. (1987). *Intention, Plans, and Practical Reason.* Harvard Univ. Press. · Rao, A. S., & Georgeff, M. P. (1995). *BDI Agents: From Theory to Practice.* ICMAS-95.
10. Kahneman, D. (2011). *Thinking, Fast and Slow.* FSG. · Booch, G., et al. (2021). *Thinking Fast and Slow in AI.* AAAI. https://arxiv.org/abs/2010.06002
11. Schmidhuber, J. (2010). *Formal Theory of Creativity, Fun, and Intrinsic Motivation (1990–2010).* IEEE TAMD 2(3). https://people.idsia.ch/~juergen/ieeecreative.pdf
12. Pathak, D., Agrawal, P., Efros, A. A., & Darrell, T. (2017). *Curiosity-driven Exploration by Self-supervised Prediction.* ICML. https://arxiv.org/abs/1705.05363
13. Friston, K. (2010). *The Free-Energy Principle: A Unified Brain Theory?* Nature Reviews Neuroscience 11(2). https://doi.org/10.1038/nrn2787
14. Laird, J. E., Newell, A., & Rosenbloom, P. S. (1987). *Soar: An Architecture for General Intelligence.* Artificial Intelligence 33(1). · Anderson, J. R., et al. (2004). *An Integrated Theory of the Mind.* Psychological Review 111(4).
15. Christakopoulou, K., et al. (2024). *Agents Thinking Fast and Slow: A Talker–Reasoner Architecture.* DeepMind. https://arxiv.org/abs/2410.08328
16. Yao, S., et al. (2023). *ReAct: Synergizing Reasoning and Acting in Language Models.* ICLR. https://arxiv.org/abs/2210.03629
17. Shinn, N., Cassano, F., Berman, E., Gopinath, A., Narasimhan, K., & Yao, S. (2023). *Reflexion: Language Agents with Verbal Reinforcement Learning.* NeurIPS. https://arxiv.org/abs/2303.11366
18. Yao, S., Yu, D., Zhao, J., Shafran, I., Griffiths, T. L., Cao, Y., & Narasimhan, K. (2023). *Tree of Thoughts: Deliberate Problem Solving with LLMs.* NeurIPS. https://arxiv.org/abs/2305.10601
19. Rabinowitz, N. C., Perbet, F., Song, H. F., Zhang, C., Eslami, S. M. A., & Botvinick, M. (2018). *Machine Theory of Mind.* ICML. https://arxiv.org/abs/1802.07740
20. Orkin, J. (2006). *Three States and a Plan: The A.I. of F.E.A.R.* GDC. https://alumni.media.mit.edu/~jorkin/gdc2006_orkin_jeff_fear.pdf
21. Erol, K., Hendler, J., & Nau, D. S. (1994). *HTN Planning: Complexity and Expressivity.* AAAI-94.
22. Colledanchise, M., & Ögren, P. (2018). *Behavior Trees in Robotics and AI: An Introduction.* CRC Press. https://arxiv.org/abs/1709.00084
23. Schrittwieser, J., et al. (2020). *Mastering Atari, Go, Chess and Shogi by Planning with a Learned Model (MuZero).* Nature 588. https://arxiv.org/abs/1911.08265
24. Browne, C. B., et al. (2012). *A Survey of Monte Carlo Tree Search Methods.* IEEE TCIAIG 4(1). https://doi.org/10.1109/TCIAIG.2012.2186810

---

*Daimon is a research concept. © 2026 David Borgenvik. Licensed MIT (see `LICENSE`).*
