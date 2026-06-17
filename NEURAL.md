# System 2 — a learned, evolved-plastic neural overlay

Daimon's mind is hand-built **instinct** (System 1): drives → appraisal → a
priority cascade that picks a goal. The 9 machine-checked proofs cover that layer.
This adds the first neural network in the architecture — a **learned overlay**
(System 2) that rides on top of instinct, never replaces it.

## Why (the question that motivated it)
Is the deterministic, no-neural-net path the right bet? Philosophically,
"deterministic" here is **reproducibility** (same seed → same run), the basis of
the whole proofs/harness edifice — *not* a metaphysical claim. The quantum-cognition
module already models indeterminacy deterministically (the seed is the hidden
variable). So the real fork isn't determinism-vs-not; it's **hand-built mechanism
vs learned mechanism — and a learned mechanism is still deterministic.** This
overlay tests whether learning helps, while keeping reproducibility intact.

## Design
- **Tiny MLP** (`overlay.rs`): 16 inputs → 12 hidden (tanh) → 6 outputs, hand-rolled,
  no NN crate, no deps, pure deterministic f32. Microseconds per decision; fine at 1000 minds.
- **Inputs**: the situation the appraisal already computes — 6 drive levels, affect
  (valence/arousal), health, threat proximity, enclosure, mortality, grief, winter, carrying, bias.
- **Outputs = bounded biases on the drive arbitration.** The net nudges
  `drives.dominant()` (mind.rs `dominant_biased`), scaled by an `nn_modulation`
  gene. **Disabled ⇒ bias is exactly 0.0 ⇒ instinct byte-identical.**
- **Lifetime plasticity** = reward-modulated 3-factor Hebbian `Δw = η·r·pre·post`,
  where reward `r` = Δ(the mind's own well-being) (drive satisfaction + health +
  valence, dimmed by grief) — an intrinsic, deterministic signal, no external
  supervision. Weights clipped; reward clipped. Computed at the top of `cycle()`
  where last tick's outcome is visible.
- **Indirect encoding (Baldwin effect)**: the genome carries only the *learning
  machinery* — `nn_enabled` (g25), `nn_learn_rate` (g26), `nn_modulation` (g27).
  Initial weights are seeded per-agent from the mind seed (diversity). The germline
  EA evolves *how to learn*; in-life Hebbian does the adaptation. N_GENES 25→28.

## Discipline (all preserved)
- `nn_enabled` defaults **OFF** in `baseline()` and `showcase()` → 82 tests, 47
  believability ACs, 9 proofs (incl. T1 Determinism) all byte-identical.
- No 7th drive (biases the existing 6). No unguarded RNG (seeded init, deterministic
  Hebbian). No new deps. Bounded (clipped) — no NaN/blow-up/hijack.
- Honest line: the overlay is **empirically validated, never "proved."** The proofs
  cover the instinct layer + determinism.

## Verification
- Unit tests (`overlay.rs`): disabled is zero/inert; seeded is deterministic; output
  bounded by modulation; learning moves weights & stays bounded; zero-reward → no learning.
- **AC47 overlay-learns**: ON → 6/6 overlays, Σ|w| moves in-life (318→518) & finite;
  OFF control → 0/6, Σ|w| 0 (instinct byte-identical). Asserts the mechanism is real
  and safe — NOT that it improves fitness.

## The honest A/B result (`examples/overlay_ab.rs`)
Identical showcase genome, instinct vs overlay, 24 seeds × 800 ticks, harsh world:

| | scalar | survival |
|---|---|---|
| instinct | 0.682 | 0.377 |
| overlay | 0.672 | 0.341 |
| **Δ** | **−0.010** | **−0.036** |

**The overlay slightly HURTS in the harsh world — a genuine negative result.** The
showcase instinct is already well-tuned for harsh-world survival, so a randomly-
initialized overlay mostly injects early-life noise that in-life Hebbian learning
can't fully recover within one ~800-tick life. **A learned overlay has headroom only
where instinct is NOT pre-optimized.** Do NOT claim "learning improves the minds";
claim "a deterministic, gene-gated, lifetime-plastic overlay is feasible and
harness-safe, and — honestly — does not beat a well-tuned instinct in a mastered
domain at this scale."

## The evolution verdict (`examples/overlay_evolve.rs`)
We let **evolution choose** — 40 independent searches over the full 28-gene genome
(the nn genes in the search space), harsh world. The overlay can be ablated for
free, so evolution keeps it on only if it pays. Champion selection rates:

| faculty | selected in champions | prior |
|---|---|---|
| **nn_enabled (the overlay)** | **18%** | OFF (incumbent) |
| quantum (known-rejected) | 5% | OFF (incumbent) |
| empowerment (upper-reference) | 55% | ON (incumbent) |
| imagination (upper-reference) | 70% | ON (incumbent) |

**Evolution leans clearly AGAINST the overlay** (18% vs a 50% random null —
one-sided binomial p≈2×10⁻⁵, n=40). The load-bearing comparison is **apples-to-
apples**: `nn_enabled` and `quantum` both start OFF in the carried baseline and are
mutated by the same per-gene rule, so the overlay (18%) sitting just above the
known-rejected `quantum` (5%) is a fair read that evolution rejects it.
`empowerment`/`imagination` start **ON** in the incumbent baseline, so their
55%/70% is a *soft upper-reference reflecting that prior*, **not** a clean 50% null
— don't read them as the neutral line. And among the few champions that keep the
overlay, evolution shrank its influence to **modulation ≈ 0.22** — even when
retained, dialled down to a faint nudge. The honest arbiter agrees with the A/B: in
a domain instinct already masters, a learned overlay does not earn its keep, and
selection turns it off.

## Next experiments (where learning *should* help)
- Test the overlay on a **novel / shifting** task the hand-built faculties were not
  tuned for (instinct has no edge there → learning has headroom).
- Start the overlay near-neutral and let it *earn* influence (anneal modulation), to
  remove the early-life noise penalty.
- Richer reward / learning rule than vanilla reward-Hebbian; longer lives.
- Let the `--evolve` germline tune nn genes and see if it *chooses* to enable the
  overlay (it can ablate it for free) — evolution as the honest arbiter.
