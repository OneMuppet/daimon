# POET — Paired Open-Ended Trailblazer (research prototype) — design

**Reference:** Wang, Lehman, Clune & Stanley (2019), *"Paired Open-Ended
Trailblazer (POET): Endlessly Generating Increasingly Complex and Diverse Learning
Environments and Their Solutions."*

**Why, grounded in our own runs (not re-litigated):** a *static* fitness objective
saturated our EA — ≈30k harsh searches, 0% target reached, 0 "dual-high" genomes,
faculties stuck ≈50% (no gradient). But where the world posed a real survival
problem (seasons → winter), evolution genuinely worked: the foresight gene climbed
0.55 → 0.95. The lesson is the POET thesis: **open-ended *environments* drive
open-ended *minds*.** So we do not hand the minds a curriculum — we co-evolve one.

## What it is

A population of **(environment, agent) pairs**, maintained by an outer loop.

- **Environment** = a bounded parameter vector ([`EnvParams`], 5 knobs in `[0,1]`)
  decoded onto the *existing* open-world knobs:
  - `cold`  → `GameWorld::open_world_cold_scale` ∈ [0.4, 3.0] (winter severity)
  - `metab` → `metabolism_scale` ∈ [0.35, 0.9] (energy/water drain)
  - `food_scarce`  → food patches/mind ∈ [1.2, 0.3]
  - `water_scarce` → water patches/mind ∈ [1.0, 0.25] (water is tightest)
  - `stalker` → predator bite ∈ [0.4, 1.3] + move period (3→1)
  A world is built by `EnvParams::build_world`, which goes through the merged
  surface only — `with_genomes_sized_harsh` + `set_open_world` + the public fields
  / `set_stalker` / `set_resource_counts`. Mutating an env perturbs the vector
  (reflection keeps it in `[0,1]`).
- **Agent** = the 28-gene [`Genome`] (the live showcase policy with `can_die`,
  `can_grieve`, `can_provision` on). **No neural nets.**
- **Inner optimisation** = a `(1+λ)` evolution strategy that reuses
  `Genome::mutate` verbatim against each env's **seasonal-survival fitness**
  (`survival_fitness`) — survival is the gradient the seasons experiment proved
  works. We do *not* fork the believability EA.

## The outer loop (`Poet::step`)

1. Start from a few EASY pairs (`EnvParams::easy` + tiny perturbations).
2. **Inner-optimise** every active agent on its native env for λ mutants/iteration.
3. Every `repro_every` iters, **generate** child envs by mutating *eligible*
   parents (eligible iff the parent's agent score ≥ `repro_threshold`). A child is
   admitted ONLY if it passes the **Minimal Criterion**: the population's best
   *transferred* agent scores inside `[mc_low, mc_high]` on it — i.e. not trivially
   solved and not impossibly hard. This is the heart of POET: it keeps new
   environments at the frontier of current capability. Children too close (knob-space
   L2 < `novelty_min`) to an existing env are rejected for novelty. The active set is
   capped (`max_active`); the oldest is retired when over cap.
4. Every `transfer_every` iters, **transfer**: evaluate every active agent on every
   active env; if a non-native agent beats the native incumbent, it takes the env
   over. This is the stepping-stone mechanism — progress on one world unlocks
   another.

## Budget accounting (the crux of fairness)

One **evaluation** = one genome simulated on one environment for `EVAL_TICKS`
(7000) ticks — 1.4 open-world years, so every eval spans a full winter *and* the
spring after it. A single shared counter (`Poet::evals`) is incremented on **every**
world-run: inner-ES candidates, the MC probe, and transfer probes all count. The
direct-EA control counts the identical unit. Both arms stop at the same total
budget `B`, so the comparison is fair. The final "best agent on the hard target"
probe is applied identically to both arms and reported separately from the loop
budget.

## The honest experiment (`examples/poet.rs`)

- **Hard target:** `EnvParams::hard_target` — severe winter + heavy metabolism +
  scarce food/water + lethal stalker.
- **Control:** a direct `(1+λ)` hill-climb (same fitness, same eval unit, NO
  curriculum, NO transfer) straight on the hard target, stopped at budget `B`.
- **POET:** the loop run to the same `B`; then its best agent scored on the target.
- **Report:** the two scores at equal budget, the curriculum trace (how envs
  escalated), and an honest verdict. A clean negative result is a valid outcome and
  is reported as such, with hypotheses (encoding too coarse / MC band wrong /
  transfer too rare / budget too small).

## Constraints honoured

- **Additive & deterministic.** New module `src/poet.rs` + `examples/poet.rs`.
  Everything seeded off one `Rng`; same seed → same run (a unit test pins this). No
  `rand`, no `Date`, no neural nets.
- **Harness untouched.** The only sim change is one new field
  `open_world_cold_scale` (default `1.0`, read *only* inside the already
  `open_world`-gated winter cold) plus two POET-only setters (`set_stalker`,
  `set_resource_counts`) that no harness path calls. `Genome::baseline()` /
  `showcase()` and the believability / proofs paths are unchanged, so the four gates
  stay green.
