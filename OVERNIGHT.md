# Overnight evolution study

A background runner (`examples/evolve_overnight`, pid recorded below) churns through
**independent evolutionary searches** over the cognitive genome — each: seed a
population, run the elitist EA against the real believability fitness to its honest
Verdict, validate the champion on held-out seeds — appending one JSON record per
search to `/tmp/daimon_evolution.jsonl`. This log harvests that file every ~10 min
to learn **which mechanisms evolution robustly selects for**, and flags anything
surprising. CPU-only; runs while the session is open.

Runner pid (iter 1): **46946**  ·  log: `/tmp/daimon_evolution.jsonl`

Headline question: *does sustained evolution discover anything, and which faculties
does selection favour?*

---

## Iteration 1 — 74 searches

- baseline scalar **0.755**; champion mean **0.805** (median 0.807, max 0.828); held-out mean **0.811**.
- **82% reach the target (`ReachedTarget`) in ~2.2 generations.** The believability
  target is *easy to reach from a random genome* — the EA converges fast rather than
  climbing for long. Held-out ≥ champion (0.811 vs 0.805): no seed-overfitting.
- **Faculty selection rates across champions** (50% ≈ neutral / weak selection):

  | faculty | selected | reading |
  |---|---|---|
  | quantum cognition | **1%** | strongly selected **against** by believability fitness |
  | can_fight | 65% | mildly favoured |
  | empowerment · affect_mod | 61% | mildly favoured |
  | cultural · lp_curiosity · stigmergy · imagination · forage_drr · social_forage | 53–55% | ~neutral |
  | consolidation · metamotivation | 43–46% | ~neutral / mildly disfavoured |
  | foresight (mean) | 26 ticks | anticipation kept on, moderate lead-time |

**What we learned (iter 1).** Two honest findings: (1) the current believability
objective is *loose* — most faculty combinations pass, so selection is weak; and
(2) quantum-cognition is the one faculty the fitness actively avoids (it adds
non-classical decision noise the believability proxies don't reward). For *sustained*
open-ended evolution we'd need a harder objective (e.g. a scarcer/larger world where
the champion isn't already near-optimal) — noted as a candidate change for a later
iteration.

(subsequent iterations appended below)

---

## Iteration 2 — 447 searches

- baseline **0.755**; champion mean **0.807** (median 0.808, **max 0.839** ↑ from 0.828 — a marginally better genome surfaced among 447); held-out **0.812**. Reached-target **83%** in **2.7** gens. All stable vs iter 1.
- **Faculty selection rates regressed toward ~50% as the sample grew** — the iter-1 "favourites" were small-sample noise:

  | faculty | iter1 (n=74) | iter2 (n=447) | reading |
  |---|---|---|---|
  | quantum | 1% | **0%** | **robustly selected AGAINST** (the one real signal) |
  | social_forage | 53% | 62% | mild, not yet robust |
  | forage_drr | 54% | 58% | mild |
  | affect_mod · cultural · can_fight | 55–65% | 53–55% | regressed to neutral |
  | empowerment · consolidation · lp_curiosity | 46–61% | 47–48% | ~neutral |
  | imagination · metamotivation · stigmergy | 43–54% | 50–52% | ~neutral |
  | foresight (mean) | 26.4 | 26.0 | stable |

**What we learned (iter 2).** The 6× larger sample confirms and sharpens iter 1:
the believability objective exerts **almost no selection pressure on the faculties**
(all converging to ~50% = free to vary) **except quantum-cognition, which it firmly
rejects (0%)**. The apparent iter-1 preferences washed out as noise. Conclusion
strengthening: this objective is too loose to *drive* faculty evolution — the EA
just samples a passing genome (83%, ~2.7 gens) and stops. Best-found fitness barely
moves (0.828→0.839 over +373 searches). For genuine open-ended evolution we need a
**harder world** where the champion isn't already near-optimal — the clearest
candidate change, and worth doing rather than running thousands more flat searches.

---

## Iteration 3 — 834 searches

- champion mean **0.807** (median 0.808), **max 0.839 — UNCHANGED** vs iter 2 over +387 searches → the search has **saturated**; the believability objective's ceiling is ~0.84. Held-out 0.811, reached 84%, gens 2.6. All flat.
- With n=834 the robust faculty signals separate from the neutral mass:

  | signal | rate | reading |
  |---|---|---|
  | quantum | **0%** | strongly against (unchanged) |
  | social_forage | **63%** | now robustly mild-FOR (held >50% across 834) |
  | forage_drr · affect_mod | **57%** | robustly mild-FOR |
  | lp_curiosity | **46%** | robustly mild-AGAINST |
  | empowerment·consolidation·imagination·metamotivation·cultural·stigmergy·can_fight | 48–54% | neutral |
  | foresight | 26.2 | stable |

**What we learned (iter 3).** Convergence reached — three iterations agree, max
fitness has plateaued, and the faculty selection is now well-estimated. Final read
of *this* objective: it rewards **social foraging, DRR-risk-aware foraging, and
affect-modulation** mildly; **rejects quantum** firmly; mildly *disfavours*
lp-curiosity (it spends exploration the believability proxies don't fully repay);
everything else is free to vary. The loop has extracted essentially all the signal
this flat objective contains — **continuing to run it will keep confirming 0.84 / 0%
quantum**, so the honest next move is a harder world (the documented lever), not more
flat searches. Will keep logging for stability but expect little new.

---

## Iteration 4 — 1,821 searches

Converged and stable; nothing new. Max **0.839** (flat over +987 searches), champ
mean 0.808, held 0.811, reached 83%, gens 2.7. Faculty rates unchanged: quantum
~0%, social_forage 63% / forage_drr 58% / affect_mod 56% mild-FOR, lp_curiosity
45% mild-AGAINST, rest neutral. **No new max-fitness outlier.** The objective is
exhausted. Extending cadence to hourly — pure liveness/outlier watch now.

---

## Iteration 5 — 3,740 searches

No change. Max **0.839** (no new outlier), champ mean 0.808, held 0.812, reached 84%, gens 2.7; all faculty rates within ±2 pts of iter 4 (quantum 1%, social_forage 63%, forage_drr 58%, affect_mod 57%, lp_curiosity 46%). Runner alive. Stable.

---

## Iteration 6 — 5,656 searches

No change. Max **0.839**, champ mean 0.807, reached 84%; faculty rates within ~1 pt of iter 5 (quantum 1%, social_forage 63%, forage_drr/affect_mod 57%, lp_curiosity 45%). Runner alive. Stable.

---

## Iteration 7 — 7,610 searches

No change. Max **0.839**, champ mean 0.807, reached 84%; faculty rates stable (quantum 1%, social_forage 63%, forage_drr/affect_mod 56-57%, lp_curiosity 46%). Runner alive.

---

## Iteration 8 — 9,493 searches — a flag (overfit outlier)

A new TRAINING-max appeared: **champ 0.849** (vs the long-standing 0.839). But it's
**seed-overfit, not progress**: its **held-out is only 0.803** — below the previous
champion's held-out (0.835) and below the population mean (~0.811). It needed **12
generations** (the most of any) and runs with **foresight≈1** (anticipation off) and
only empowerment/forage_drr/cultural/affect_mod on — a genome tuned to its 4
training worlds, not a better agent.

**What we learned (iter 8).** The honest reading: *more search overfits*. The
champions that generalise best (held-out ~0.835) sit at the ordinary 0.839 training
band with foresight≈26 + imagination/social-foraging/lp-curiosity — not the 0.849
outlier. This is a textbook train-vs-held-out gap and a good argument for **selecting
on held-out, not training fitness**, and for the harder/varied world (more
train-world diversity would close the overfit gap). Faculty selection rates
unchanged (quantum 1%, social_forage 63%, forage_drr/affect_mod 56-57%). Runner alive.

---

## Iteration 9 — 16,564 searches — held-out outlier (also variance, not progress)

The held-out-max climbed to **0.859** (clears the 0.835 flag) — but it's the **mirror
of iter 8's overfit**: that genome has **train only 0.80 and gens=0** (an *initial
random* genome, never selected/evolved). The EA selects on *training*, so it never
pursues these lucky-held-out points. Over 16.5k draws the extremes of BOTH axes
(train-max 0.849, held-max 0.859) are **order-statistic noise** — train↔held-out are
weakly correlated, so a record's score on one axis barely predicts the other.

**What we learned (iter 9).** Central tendency unchanged (champ mean 0.807, held
mean 0.812); faculty rates unchanged (quantum 1%, social_forage 64%, forage_drr 57%,
affect_mod 56%, lp_curiosity 45%). Combined with iter 8, the firm conclusion:
**training fitness ≠ generalisation here**, and 16k flat searches only sharpen the
noise. The two real levers (documented) stand: *select on held-out*, and *harder/
more diverse worlds* to couple the two. Runner alive. Will keep watching.

---

## PIVOT → HARSH WORLD (after 17,150 fair searches, archived)

The fair-world experiment is concluded (saturated at 0.84, quantum rejected, weak
selection; raw log archived to `/tmp/daimon_evolution_fair.jsonl`). Per the
documented lever, the runner now evolves against a **harsh world**: scarce
resources (water tightest: ~pop/3 springs, ~pop/2 food) and a stalker that moves
**every tick**. New `GameWorld::with_genome_harsh` + `fitness::evaluate_harsh`; the
fair world (and the believability harness) is untouched.

**First harsh results (8 searches): the gradient is real.**

| metric | fair | harsh |
|---|---|---|
| baseline scalar | 0.755 | **0.646** |
| generations to converge | ~2.7 | **7–15** (real climbing) |
| reach-target rate | 84% | **0%** (nothing passes trivially) |
| champ max | 0.839 | 0.695 (early) |
| binding facet | — | **survival ≈ 0.42** (the wall) |

Now the EA must *earn* fitness over many generations, and survival is the honest
frontier — exactly the substrate needed to see whether evolution discovers real
survival strategy (anticipation, commons, DRR). The overnight loop now tracks the
harsh log. (Subsequent harsh iterations below.)

---

## Iteration 10 — 876 harsh searches (first full harsh aggregate)

Establishing the harsh-world baseline (iters 7-9 were fair-world; the pivot's "8
searches" note is now superseded). Across 876 searches: train mean **0.687** (max
0.726), held-out mean **0.657** (max **0.737**), gens mean 8.7 (max 24) — the EA
genuinely climbs, but the **harsh ceiling sits ~0.74**, well below the 0.84 target,
so verdicts are 873 Converged / 3 Budget, **0% ReachedTarget** (as designed —
nothing passes trivially). **0 dual-high genomes** (none clear both train>0.84 and
held>0.84): the target is simply unreachable in this regime, so dual-high is the
wrong bar here — held-out max 0.737 is the honest frontier.

Faculty selection among champions (harsh baseline): affect_mod 66%, can_fight 60%,
forage_drr 58%, cultural 58%, metamotivation 53%, imagination 51% — a mild tilt
above the ~50% fair-world noise floor toward emotion-modulation, defence, and
commons-aware foraging (plausible under scarcity + a tick-fast stalker). quantum
**1%** (firmly rejected, consistent across both regimes). mean foresight **25.6**
(anticipation strongly favoured). Treat these tilts as the baseline to watch, not
as progress — they're selection rates among converged champs, still weak signal.
Runner alive (pid 78074). Nothing genuinely new; will keep watching.

---

## Iteration 11 — 1,202 searches (runner crashed on full disk; rebuilt)

**Event:** the harsh runner died (exit 1) when the machine's disk filled to ENOSPC
(only ~2 GB free of 926 GB). Freed space by deleting `target/` (~5 GB); David
freed more (now ~8.7 GB). Rebuilding `daimon-game` + relaunching the runner.

**Data (unchanged — converged):** the 326 searches added since iter 10 moved
nothing >1 pt. train 0.687 (max 0.726), held-out 0.657 (max **0.737** — still the
ceiling), gens 8.6, verdicts 1199 Converged / 3 Budget, **0% ReachedTarget**,
**0 dual-high**. Faculty rates flat vs iter 10 (affect_mod 65%, can_fight 60%,
forage_drr/cultural 58%; quantum 1%; foresight 25.6). The harsh experiment has
plateaued; further searches only sharpen the same conclusion.

---

## Iteration 12 — 2,946 searches

Nothing new. Disk recovered (131 GB free), runner healthy (pid 56743). Converged
exactly as before: train 0.684 (max 0.729), held-out 0.657 (max **0.737** ceiling),
gens 8.9, 2939 Converged / 7 Budget, 0% ReachedTarget, **0 dual-high**. Faculty
rates within noise of the iter-10/11 baseline; the only mover is quantum 1%→6%,
still firmly rejected (low single digits = sampling noise as the pool turns over,
not selection). Harsh experiment remains plateaued. (Note: the live runner binary
predates the death/grief gene additions — it evolves the original 12-faculty
harsh objective, so records stay comparable across all iterations.)

---

## Iteration 13 — 6,929 searches

Nothing new. Runner healthy (pid 56743), disk fine (130 GB). Converged as before:
train 0.683 (max 0.734), held-out 0.657 (max 0.756), gens 9.1, 0% ReachedTarget,
**0 dual-high**. Faculty rates within noise of the iter-10/11 baseline (quantum
7%, still rejected; foresight 22.8). Harsh experiment remains plateaued.

---

## Iteration 14 — 7,558 searches

Nothing new. Runner healthy (pid 56743), disk fine. Unchanged from iter 13: train
0.683 (max 0.734), held-out 0.657 (max 0.756), gens 9.1, 0% ReachedTarget,
**0 dual-high**; faculty rates and quantum (7%) flat. Still plateaued.

---

## Iteration 15 — 8,625 searches

Nothing new. Runner healthy (pid 56743), disk fine. Flat vs iters 13-14: train
0.683 (max 0.734), held-out 0.657 (max 0.756), gens 9.1, 0% ReachedTarget,
**0 dual-high**; faculties + quantum (7%) unchanged. Still plateaued.

---

## Iteration 16 — 9,737 searches

Nothing new. Runner healthy (pid 56743), disk fine. Flat vs iters 13-15: train
0.683 (max 0.734), held-out 0.657 (max 0.756), gens 9.1, 0% ReachedTarget,
**0 dual-high**; quantum 8% (still rejected), faculties unchanged. Plateaued.

---

## Iteration 17 — 10,857 searches

Nothing new. Runner healthy (pid 56743), disk fine. Flat vs iters 13-16: train
0.683 (max 0.734), held-out 0.657 (max 0.756), gens 9.1, 0% ReachedTarget,
**0 dual-high**; quantum 8%, faculties unchanged. Past 10k searches, plateaued.

---

## Iteration 18 — 11,981 searches

Nothing new. Runner healthy (pid 56743), disk fine. Flat vs iters 13-17: train
0.683 (max 0.734), held-out 0.657 (max 0.756), gens 9.1, 0% ReachedTarget,
**0 dual-high**; quantum 8%, faculties unchanged. Plateaued (~12k searches).

---

## Iteration 19 — 13,130 searches

Nothing new. Runner healthy (pid 56743), disk fine. Flat vs iters 13-18: train
0.683 (max 0.734), held-out 0.657 (max 0.756), gens 9.1, 0% ReachedTarget,
**0 dual-high**; quantum 8%, faculties unchanged. Plateaued (~13k searches).

---

## Iteration 20 — 14,263 searches

Nothing new. Runner healthy (pid 56743), disk fine. Flat vs iters 13-19: train
0.683 (max 0.734), held-out 0.657 (max 0.756), gens 9.1, 0% ReachedTarget,
**0 dual-high**; quantum 8%, faculties unchanged. Plateaued (~14k searches).

---

## Iteration 21 — 15,405 searches

Nothing new. Runner healthy (pid 56743). Flat: train 0.683 (max 0.734), held-out
0.657 (max 0.756), 0% ReachedTarget, **0 dual-high**, quantum 8%. Plateaued (~15k).

---

## Iteration 22 — 16,529 searches

Nothing new. Runner healthy (pid 56743). Flat: train 0.683 (max 0.734), held-out
0.657 (max 0.756), 0% ReachedTarget, **0 dual-high**, quantum 8%. Plateaued (~16k).

---

## Iteration 23 — 17,651 searches

Nothing new. Runner healthy (pid 56743). Flat: train 0.683 (max 0.734), held-out
0.657 (max 0.756), 0% ReachedTarget, **0 dual-high**, quantum 8%. Plateaued (~18k).

---

## Iteration 24 — 18,797 searches

Nothing new. Runner healthy (pid 56743). Flat: train 0.683 (max 0.734), held-out
0.657 (max 0.756), 0% ReachedTarget, **0 dual-high**, quantum 8%. Plateaued (~19k).

---

## Iteration 25 — 19,950 searches

Nothing new. Runner healthy (pid 56743). Flat: train 0.683 (max 0.734), held-out
0.657 (max 0.756), 0% ReachedTarget, **0 dual-high**, quantum 8%. Plateaued (~20k).

---

## Iteration 26 — 21,103 searches

Nothing new. Runner healthy (pid 56743). Flat: train 0.683 (max 0.736), held-out
0.657 (max 0.756), 0% ReachedTarget, **0 dual-high**, quantum 8%. Plateaued (~21k).

---

## Iteration 27 — 22,165 searches

Nothing new. Runner healthy (pid 56743). Flat: train 0.683 (max 0.736), held-out
0.657 (max 0.756), 0% ReachedTarget, **0 dual-high**, quantum 8%. Plateaued (~22k).

---

## Iteration 28 — 23,306 searches

Nothing new. Runner healthy (pid 56743). Flat: train 0.683 (max 0.736), held-out
0.657 (max 0.756), 0% ReachedTarget, **0 dual-high**, quantum 8%. Plateaued (~23k).

---

## Iteration 29 — 24,375 searches

Nothing new. Runner healthy (pid 56743). Flat: train 0.683 (max 0.736), held-out
0.657 (max 0.756), 0% ReachedTarget, **0 dual-high**, quantum 8%. Plateaued (~24k).

---

## Iteration 30 — 25,474 searches

Nothing new. Runner healthy (pid 56743). Flat: train 0.683 (max 0.736), held-out
0.657 (max 0.756), 0% ReachedTarget, **0 dual-high**, quantum 8%. Plateaued (~25k).

---

## Iteration 31 — 26,597 searches

Nothing new. Runner healthy (pid 56743). Flat: train 0.683 (max 0.736), held-out
0.657 (max 0.756), 0% ReachedTarget, **0 dual-high**, quantum 8%. Plateaued (~27k).

---

## Iteration 32 — 27,642 searches

Nothing new. Runner healthy (pid 56743). Flat: train 0.683 (max 0.736), held-out
0.657 (max 0.756), 0% ReachedTarget, **0 dual-high**, quantum 8%. Plateaued (~28k).

---

## Iteration 33 — 28,745 searches

Nothing new. Runner healthy (pid 56743). Flat: train 0.683 (max 0.736), held-out
0.657 (max 0.756), 0% ReachedTarget, **0 dual-high**, quantum 8%. Plateaued (~29k).

---

## Iteration 34 — 29,655 searches

Nothing new. Runner healthy (pid 56743). Flat: train 0.683 (max 0.736), held-out
0.657 (max 0.756), 0% ReachedTarget, **0 dual-high**, quantum 8%. Plateaued (~30k).

---

## LOOP RETIRED —    29727 searches (concluded by David, 2026-06-17)

The harsh-world evolutionary experiment is **concluded**. It plateaued from
iteration ~12 onward and stayed flat for ~22 hourly checks: train ≈0.683,
held-out ≈0.657, **0% ReachedTarget, 0 dual-high genomes**, quantum firmly
rejected (~8%). The harsh-world objective is saturated — further searches only
re-confirm the same null. Runner stopped; analysis loop retired. Raw log:
/tmp/daimon_evolution.jsonl.
