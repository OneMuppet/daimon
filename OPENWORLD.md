# Open-ended world v1 — "Seasons & Provision" — design

**Goal (verbatim intent):** "take the next big leap in the evolution of our minds."
Direction: an **open-ended world**. *"seasons turn → store for winter … a real
day/season rhythm with consequences. The world poses open-ended problems; the mind
must compose a life to meet them."* v1 is the **SEASONS + PROVISIONING core**.
Crafting tools / farming a plot are a deliberate **v2** (called out as deferred
below) — v1 proves the one load-bearing loop: *a present-only mind starves its first
winter; a mind that provisions lives.*

## The principle: script the *pressure*, not the *plan*

This is the same move as Praxis, emergent shelter (`can_build`), and grief
(`can_grieve`): give the mind a **pressure** + an **affordance**, and let the
existing utility/Praxis/planning layers discover the high-value behaviour. Nothing
ever says "prepare for winter."

- **A real year turns.** A deterministic season clock derived from the existing day
  clock (1 day = 625 ticks; `world.day` already wraps). **Year = 8 days = 2 days per
  season**, so an `--evolve` generation (6250 ticks = 10 days) always spans a full
  winter — winter is a *selection pressure*, not a coin flip.
- **The seasonal pressure.** When `open_world` is on:
  - **Summer/autumn → food abundant** (fast respawn); **winter → food stops spawning
    (≈0)**; spring → food returns (regrowth).
  - **Winter applies a COLD energy drain** every tick — reduced near the hearth (the
    village heart) / when sheltered (ties to building + the heart). The cold is what
    *kills the unprepared*.
- **The affordances.** Two generic actions, nothing structure-specific:
  - **`Action::Gather`** — harvest a surplus of food while it is abundant; it goes
    into the body as **carried provisions** (`SelfState.carrying`).
  - **`Action::Store`** — deposit carried provisions into the **shared village
    granary** (a cache built up over the good months).
  - In winter a hungry mind **adjacent to the granary auto-draws** from it (a Commons
    draw) — no Draw action needed; it composes with the existing Commons theme.
- **Emergence.** Gated by `can_provision` **and** `open_world`: when needs are met
  (hunger/thirst not critical — the body always wins) **and** it is harvest season
  *or* winter is anticipated by the foresight faculty **and** the granary isn't full,
  the mind adopts **`GoalKind::Provision`** (origin **Mastery**). The planner turns
  it into Gather→Store toward food and the granary. A **foresighted** mind starts
  provisioning *before* winter (the anticipation appraisal). When winter hits, the
  stored cache is drawn down to survive the cold; an unprovisioned mind's energy
  bleeds out and — under `lethal_starvation` (evolve mode) — it dies. So:
  - *winter culls the unprepared* (lethal starvation + cold);
  - *the foresight gene now predicts winter and triggers provisioning*;
  - *`--evolve` breeds provisioning* (winters select for `can_provision` + foresight);
  - *grief composes* — survivors mourn the minds that starved.

## HARD architecture constraints (the discipline that kept prior features safe)

1. **`open_world: bool` on `GameWorld`, default FALSE.** Everything seasonal /
   provisioning is gated by it. Off ⇒ the world behaves EXACTLY as today: the season
   is always Spring with no cold and the normal respawn, no granary, no provisioning
   logic, **no new RNG draws** — so the 73 tests, 43 believability ACs and 9 proofs
   stay byte-identical. The live game and `--evolve` mode turn it ON.
2. **No 7th Drive.** The 6-drive system is load-bearing (`fitness.rs` uses `[u32;6]`
   + entropy over `ln(6)`). Provisioning is motivated by the **existing** drives —
   primarily **Mastery** (competence / stocking up) with **Hunger** + the
   **foresight/anticipation** faculty. Modelled via a new `GoalKind`, two new
   `Action`s, a gene, and the appraisal — never a drive.
3. **Gene-gated, default off.** New gene **`can_provision` at index 24**
   (`N_GENES` 24 → 25). `baseline()` and `showcase()` both set `g[24] = 0.0`
   (asserted in tests). Accessor + `MindConfig.can_provision` + `express()` wiring,
   exactly mirroring `can_build`/`can_grieve`. The gene only *does* anything when
   `open_world` is also on.
4. **No unguarded RNG.** Every new RNG draw sits behind the `open_world` flag and/or
   the `can_provision` gene, so seeded default worlds are byte-identical. Verified by
   proofs **T1** (800-tick world fingerprint) + the off-control ACs.
5. **Determinism everywhere.** The season advances deterministically from the tick
   clock; no `Date`/`rand`. Carried provisions / granary level are plain integers /
   floats updated in lockstep with the sim.

## Design choice: NO new `EntityKind`

`EntityKind` is matched exhaustively in ~10 places (innate valence, praxis channels,
concept mapping, render). Adding `Tree`/`Granary` variants would perturb all of them
and risk changing default-world behaviour. Instead — **mirroring the `shelter_gap`
pattern** — the sim computes, per agent each tick, two directional hints and a carry
count and hands them to the mind through `SelfState`:

- `season: u8` (0 Spring · 1 Summer · 2 Autumn · 3 Winter; default 0 = Spring),
- `winter_in: f32` (ticks until winter, for the anticipation appraisal; default large),
- `carrying: f32` (provisions on the body; default 0),
- `gather_dir: Option<Dir>` (step toward the nearest harvestable food when stocking),
- `store_dir: Option<Dir>` (step toward the granary when carrying a surplus).

The planner consumes these directly (just like `Shelter` consumes `shelter_gap`), so
no new world-model belief plumbing is needed and the off-world stays inert. The
granary itself is **world state on `GameWorld`** (a position + a food level), built
up from `Action::Store`. Wood: trees become a depletable count that regrows in
spring; building the granary costs gathered wood (kept light — the granary exists
from tick 0 as the *village heart* so the loop is testable; wood-gating its
construction is the soft part of v1, see "Honest scope").

## Roadmap (files)

1. **Core** (`daimon-core`): `Action::Gather` / `Action::Store`; `GoalKind::Provision`;
   `SelfState` season/carry/dir fields (all `#[serde(default)]`, defaults inert).
2. **Sim** (`daimon-game/sim.rs`): `open_world` flag; the season clock; seasonal food
   respawn modulation + winter food≈0; winter cold drain (hearth-reduced); the
   granary (pos + level) + `Gather`/`Store` resolution + winter auto-draw; per-agent
   `gather_dir`/`store_dir`/`carrying`/`season` fed into `SelfState`; trees deplete /
   regrow. All gated by `open_world`.
3. **Cognition** (`daimon-mind`): `MindConfig.can_provision` + setter/getter; gene 24
   in `evolve.rs` (baseline/showcase 0, accessor, `express`); the **Provision
   adoption** block in `decide` (after Shelter/Mourn, gated, needs-first); the
   **planner** arm (Gather/Store toward the dirs); **language** narration.
4. **Live + evolve wiring**: live game (`lib.rs`) and `--evolve` (`evolve_mode.rs`)
   set `g[24]=1` + `world.open_world=true`. Harness paths keep both OFF.
5. **Render** (`view.rs`, `geo.rs`): real-season tint (winter snow strengthened) keyed
   to the *sim* season when `open_world`; a wooden **granary** structure; depleted
   trees if cheap.
6. **Verify** (gates): a sim unit test (season cycle deterministic; winter zeroes
   spawn + applies cold; cache deposit/withdraw; off-world unchanged); a believability
   **AC46 winter provisioning** (ablation: provisioning population survives winter far
   better than a gene-off control + the cache rises then falls); re-run the full
   suite + proofs + clippy + headless; a determinism fingerprint; and a headless
   open-world winter-survival smoke (provision vs control numbers).

## Verified results (as built)

All gates green, determinism intact:

- **Tests:** `cargo test --workspace` → **77 passed, 0 failed** (73 prior + 1 gene-off
  test + 3 sim tests: season clock / winter-stops-food, granary deposit+winter-draw,
  open-world-off-is-inert).
- **Believability:** **44 [PASS] · ALL CRITERIA GREEN** (43 prior + AC46 winter
  provisioning). AC46 (3 seeds, one full winter, gene ablation): gene ON adopts
  ~24k Provision goals, performs ~15k gather/store actions, fills the cache to a peak
  of ~69, and **7 minds survive winter; the gene-OFF control adopts 0 Provision goals,
  performs 0 gather/store, the cache stays 0/0/0, and only 2 survive** — a clean
  ablation with a real survival edge (7 > 2).
- **Proofs:** **9 [PROVEN]** (T1 determinism still holds — the default seeded world is a
  pure function of its seed).
- **Clippy:** **0 warnings**, workspace + all targets.
- **Headless:** renders all frames, no panic.
- **Determinism fingerprint:** a default seeded world (open_world off, showcase) is
  byte-identical across runs and the open-world machinery is fully inert when off.
- **Winter smoke** (`examples/winter_smoke.rs`, 8 minds, one full winter, stalker
  softened to isolate the cold): **provision 7/24 survived vs control 2/24** across 3
  seeds; the cache fills only with the gene on.

### Tuning that made the loop bite (honest record)

The loop did NOT work on the first pass — the design discipline meant tuning against
real run-traces, not assertion:
- **Year = 4 days (1 day/season, 625-tick winter)**, not the suggested 8 — a 1250-tick
  winter is longer than a believable autumn store can bridge.
- **Auto-draw radius = 6** (the hearth feeds the village around it), with a **small
  ration** so a modest store lasts the whole winter.
- **At the hearth in winter a mind nearly hibernates** (metabolism ×0.1, cold ≈ nil),
  so a small cache bridges the cold; out in the open the cold is real and lethal.
- **Late-autumn homing window** (`winter_in ≤ 220`) pulls the village home *before* the
  cold lands, so they aren't caught out and frozen at the transition.
- **Open-world winter is lethal on its own** (a mortal body that runs empty out in the
  cold dies, no floor) — independent of the global `lethal_starvation` — so winter is a
  clean differentiator while the good seasons stay survivable.

### What is weak / honest caveats

- The control still has **2 survivors** (not 0): the hearth's warmth is gentler to
  *anyone* near it, so a control mind that happens to idle near the village heart can
  scrape through. The **cache ablation is perfectly clean** (0 stored without the
  gene); the *survival* edge is real but modest (7 vs 2), and seed-sensitive.
- Some provisioning minds still die at the winter onset if caught far from the hearth.
- The asymmetry is strongest with the stalker softened (predator deaths otherwise add
  noise to both arms); the live game and the AC soften it to isolate the cold.

## Honest scope (v1 vs v2)

- **In v1:** seasons with consequences; cold drain; gather food surplus; a shared
  granary store/draw; provisioning emerges from Mastery + foresight; winter culls;
  evolve breeds it; render shows winter + granary.
- **Wood/crafting:** trees deplete & regrow and wood is *gathered*, but **crafting
  tools and farming a plot are v2.** v1 keeps the granary present from the start (the
  village heart) rather than requiring it be built from wood first — so the core
  provisioning loop is the thing under test, not a construction prerequisite. If the
  winter-survival asymmetry is weak, that is reported as a finding, not hidden.
