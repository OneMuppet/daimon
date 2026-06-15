# The End Goal — and the machine that pursues it

## 1. What we are actually trying to build

One sentence, held constant across every milestone:

> **A game agent that is autonomously, *measurably* believable — it carves its
> own concepts, sets its own ends, models its own dynamics, revises its own
> values, and chooses in regimes a scripted NPC cannot — and whose improvement is
> driven by evidence, not by a human's hand.**

"Believable" is not a vibe here. It is operationalised as five facets of a lived
life, each scored in `[0,1]` from a real run of the real world, each with genuine
headroom and genuine trade-offs against the others:

| Facet | Meaning | End-goal bar |
|---|---|---|
| **Survival** | keeps its needs met; little time starving/parched | ≥ 0.85 |
| **Safety** | avoids harm; rarely within the stalker's reach | ≥ 0.80 |
| **Balance** | no fixation — high entropy over which drive leads | ≥ 0.55 |
| **Expression** | varied, non-repetitive, grounded dialogue | ≥ 0.55 |
| **Exploration** | covers ground, discovers, invents | ≥ 0.45 |
| **Aggregate** | weighted scalar of all five | ≥ 0.72 |

The **ultimate end goal** is an agent that clears *every* bar *at once*, robustly
across seeds — a life that is simultaneously durable, safe, open, expressive, and
balanced.

## 2. The shift this milestone makes

Until now **a human was the optimiser**: add a mechanism → run the harness → read
the verdict → decide the next move. That loop produced everything from BDI
cognition through Praxis, empowerment, imagination, quantum cognition, and neural
criticality. But the human is the bottleneck and the source of bias.

This milestone closes the loop. The believability harness — already the project's
*arbiter of truth* — becomes the **fitness function** of a search that runs
itself:

```
            ┌─────────────────────────────────────────────┐
            │                                               │
   genome ──┤  express → live N real lives → measure 5 facets ─┐
            │                                               │   │
            └───────────────── select / mutate ◄────────────┘
                                   ▲                         │
                       learn which genes matter ◄────────────┘
```

The **genome** is a point in the architecture's tunable space (13 genes:
escalation policy, persona deltas, and which cognitive faculties are switched on).
Each genome is *expressed* into a full village of minds and graded by living
several 600-tick lives in the same world the manual harness judges. There is one
physics, one arbiter — now grading a system that is improving itself.

## 3. Why this is *self-learning*, not blind search

Three mechanisms (in `daimon-mind/src/evolve.rs`):

1. **Self-adapting mutation** — Rechenberg's 1/5th-success rule: the step size
   `σ` grows while variation pays off and shrinks as the search homes in.
   Annealing *emerges*; it is not scheduled. (Observed: σ 0.22 → 0.02.)
2. **Per-gene sensitivity** — each generation the loop correlates every gene with
   fitness across the population and mutates high-impact genes harder. It *learns
   which levers move believability* and leans on them. (Observed: once the
   anticipatory-homeostasis gene exists, the loop ranks **foresight** the single
   most fitness-sensitive gene — it finds the lever that matters.)
3. **Self-evaluation & honest halting** — the loop grades its own champion against
   the end-goal target *and* a plateau detector, and stops with a `Verdict` that
   says which: `ReachedTarget`, `Converged` (plateau), or `Budget`. It never
   reports "done" on a fixed loop count.

## 3a. The outer loop: the search writes the next mechanism

The inner loop tunes a fixed genome. The **outer loop** is what makes this a
*research* engine: when the search plateaus, its per-facet scores and learned
sensitivities name the missing capability, a human (or, later, a generator) adds
*new mechanism*, the genome grows, and the same search re-runs. We have now closed
one full turn of it, on the record:

1. First search → `Converged`; every facet cleared its bar **except survival**
   (0.48 → 0.60). The loop localised the frontier.
2. Diagnosis: physiological needs (thirst +0.016/tick) outpace a purely *reactive*
   forager, especially while `Survival` (weight 2.5) suppresses foraging during a
   predator chase. So we added **anticipatory homeostasis** (`DriveSystem`
   foresight): a need is weighed as if it had crept forward *N* ticks, so the
   agent forages *ahead* of crisis — a computable step toward active inference
   (minimising *expected* future need). It is exposed as a new gene (default 0,
   preserving every prior result) and ablation-tested (AC29).
3. Second search → **survival 0.48 → 0.70**, scalar **0.701 → 0.787** (+0.086),
   and the loop **independently ranked `foresight` as the single most
   fitness-sensitive gene** — it confirmed the mechanism we added in answer to its
   own prior finding is exactly the biggest lever.

That is the cycle the goal asks for, running: *search → localise frontier → add
mechanism → search confirms it and advances.*

**A second turn — and an honest negative result.** Still short on survival, we
mined the literature (arXiv) for the next lever and implemented the strongest
candidate: **drive-reduction-rate foraging under survival risk** (route to the
resource maximising relief × trip-survival ÷ time; Keramati–Gutkin / Charnov /
Mangel–Clark). Ablation-tested — and it **did not transfer**: critical-need time
23.5% → 24.7%, survival no better. We keep the negative result. The diagnosis is
the payoff: the incumbent planner already weighs travel and danger, so the
bottleneck is *not* which resource an agent picks — it is the **commons** (6
agents, 4 water tiles). The next mechanism is therefore *social* (need-priority
yielding + contention dispersion), not more single-agent foraging. The loop didn't
just fail to advance; it *redirected* the search.

## 4. Where we actually stand — END GOAL REACHED (held-out validated)

Run `cargo run -p daimon-game --example autogenesis --release`. The loop now
returns verdict **`END-GOAL TARGET REACHED — every facet cleared the bar`**:

| facet | bar | champion (train) | champion (held-out, 5 unseen seeds) |
|---|---|---|---|
| survival | ≥0.85 | 0.92 | **0.88** |
| safety | ≥0.80 | 0.94 | 0.94 |
| balance | ≥0.55 | 0.67 | 0.61 |
| expression | ≥0.55 | 0.65 | 0.65 |
| exploration | ≥0.45 | 0.96 | 0.88 |
| scalar | ≥0.72 | 0.85 | **0.81** |

It **generalises** — the target is met on seeds the search never saw, so this is
not seed-overfit. And it is **earned**: the hand-tuned baseline (anticipation off)
sits at survival 0.74, and a purely reactive policy at 0.65 — both below the bar.
The loop reached the goal in 4 generations, by its own search, discovering the
combination that works.

### How we got here — and the honesty that made it real

The road was not a straight climb; it was the pipeline doing science, including
telling us hard truths:

1. **Anticipatory homeostasis** (positive): survival 0.48 → 0.70. The loop asked
   for it; it delivered.
2. **DRR foraging** (negative, kept): a literature-grounded forager that *did not
   transfer* — the bottleneck was not which resource you pick.
3. **Commons coordination** (negative *then* positive): dispersion/yielding first
   made things *worse* — which forced the key diagnostic.
4. **The diagnosis that cracked it.** A single agent with anticipation hits ~8.5%
   critical-need time (≈0.84 survival) in the *original* world, but six agents
   exploded to ~25%. The cause was not policy: water supply (4 springs ≈
   0.167 drinks/tick) sat **below** a six-agent demand of ≈0.176/tick. **The
   testbed was structurally unsurvivable for a village** — and an unsurvivable
   world is itself unbelievable.
5. **The fair-world correction.** We scaled resources to the population (a village
   has enough wells for its people: `pop+3` of each). This is testbed correction,
   not metric-gaming, and we guard it: with the fair world, a *reactive* policy
   still fails (survival 0.65) and *anticipation alone* only reaches 0.80 —
   survival must still be **earned**. With adequate supply the commons mechanism
   flipped from harmful to helpful (critical-need time 10.8% → 6.0%): coordination
   only pays when there is enough to coordinate over.

The end goal is reached by a **good autonomous policy in a fair world**
(anticipation + commons-aware foraging + a tuned escalation config), found by the
self-improving loop and validated on unseen seeds — not by trivialising the world
or lowering the bar. The negative results (DRR; commons-under-scarcity) are kept
on the record, because they are what *located* the true cause.

## 5. What "iterate until we reach the end goal" means here

The pipeline is the iterator. Each new mechanism we add (the next being a
foraging/active-inference planner aimed squarely at the survival frontier the loop
identified) enlarges the genome or the architecture, and the *same* self-improving
loop re-searches the enlarged space and re-reports whether the bars are cleared.
The loop is the permanent engine; the milestones are its fuel. We reach the end
goal when the loop returns `ReachedTarget` robustly across seeds — and not before,
and we will not pretend otherwise.

---

*Reproduce:* `cargo run -p daimon-game --example autogenesis --release`
(full search) · `cargo run -p daimon-game --example believability --release`
(AC27/AC28 gate the loop) · `cargo test` (engine unit tests, incl. a synthetic
fitness that proves the search learns).
