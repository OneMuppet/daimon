# Daimon — Formal Properties (machine-checked theorems)

A *mathematically-proved* game AI: the architecture's core mechanisms are stated
as theorems, **proved**, and each paired with a check that **verifies the claim
against the real implementation** (not a model of it). A theorem counts as proven
only when its written proof here is matched by a green check in
`cargo run -p daimon-game --example proofs --release`.

This is the honest bar: proofs *about the code*, verified *on the code*. The
verification run caught one overclaim during authoring (T8 — see its note), which
is exactly the point of pairing proof with machine-check.

Notation: drives, fitness facets ∈ [0,1]; α, λ are update rates; T>R>P>S are the
prisoner's-dilemma payoffs.

---

## T1 — Determinism (reproducibility)

**Statement.** For a fixed seed `s` and genome `G`, the world trajectory
`{W_t}` of `GameWorld::with_genome(s, n, G)` is uniquely determined: the map
`(s, G, t) ↦ W_t` is a function.

**Proof.** The only source of nondeterminism in `step()` is the RNG. `Rng` is
SplitMix64: `state_{k+1} = state_k + φ (mod 2⁶⁴)` and output
`o_k = mix(state_{k+1})`, where `mix` is a fixed composition of xor-shifts and
odd multiplications. `Rng::new(s)` fixes `state_0 = s`, so the sequence
`(state_k, o_k)` is identical across runs. `step()` consumes draws in a fixed
program order (single-threaded; IEEE-754 arithmetic is deterministic). Induction
on `t`: `W_0` is a pure function of `(s, G)`; if `W_t` and the RNG state are
determined, then `W_{t+1} = step(W_t)` reads only `W_t` and the deterministic RNG,
so `W_{t+1}` and the next RNG state are determined. ∎

**Verified.** 100 000 RNG draws from two same-seed generators identical; two
same-seed/genome worlds bit-identical (FNV digest of positions + health) over 800
ticks.

---

## T2 — Homeostatic boundedness (safety invariant)

**Statement.** For every drive `d` and tick `t`: `level_d(t) ∈ [0,1]` and
`bias_d(t) ∈ [0.35, 2.5]`.

**Proof.** `[0,1]` is invariant under every public mutator. `set(d,v)` stores
`v.clamp(0,1)`; `bump = set(level+δ)` clamps; `decay()` applies, per drive, either
a creep `x ↦ clamp(x+c, 0, 1)`, or the curiosity relaxation
`x ↦ x + (0.25−x)α = (1−α)x + α·0.25` with `α ∈ [0,1]`, a convex combination of
`x ∈ [0,1]` and `0.25 ∈ [0,1]`, hence in `[0,1]`. Default levels
`∈ {0.1,0.15,0.3} ⊂ [0,1]`. By induction the level invariant holds. For bias:
the default is `1.0 ∈ [0.35,2.5]`, and the only mutator is
`nudge_bias: bias ↦ clamp(bias·factor, 0.35, 2.5) ∈ [0.35,2.5]`. ∎

**Verified.** 200 000 adversarial perturbations (out-of-range `set`s, ± `bump`s,
× `nudge_bias`, `decay`) → **0** invariant breaches.

---

## T3 — Homeostatic stability (Lyapunov contraction)

**Statement.** Absent novelty/learning-progress bumps, curiosity `c_t` converges
geometrically to the setpoint `c* = 0.25`. `V(c) = (c − c*)²` is a Lyapunov
function with `V_{t+1} = (1−α)² V_t`, `α = 0.05`, contraction factor
`0.9025 < 1`; `c*` is globally asymptotically stable on `[0,1]`.

**Proof.** The curiosity map is `c ↦ c + (0.25−c)α`. Subtracting `c*`:
`c_{t+1} − c* = (1−α)(c_t − c*)`, so `V_{t+1} = (1−α)² V_t`. As
`0 < (1−α)² = 0.9025 < 1`, `V_t = 0.9025^t V_0 → 0`, hence `c_t → c*` from any
`c_0`; and `ΔV = ((1−α)² − 1)V ≤ 0`, with equality iff `c = c*`. ∎

**Verified.** From `c_0 ∈ {0, 0.1, 0.5, 0.9, 1.0}`, `V` is non-increasing, the
measured per-step ratio matches `0.9025` to `< 1e-4`, and `c → 0.25` within
`1e-3`.

---

## T4 — Evolutionary elitism (monotone improvement)

**Statement.** Let `B_g` be the maximum fitness scalar in generation `g`. Then
`B_{g+1} ≥ B_g` (best-so-far is monotone non-decreasing).

**Proof.** `step()` ranks the population by scalar and copies the top quarter —
which contains an arg-max individual of fitness `B_g` — *unmutated* into
generation `g+1`. So generation `g+1` contains an individual of fitness `≥ B_g`,
whence its maximum `B_{g+1} ≥ B_g`. (The engine's tracked `best/best_fit` is the
running arg-max, likewise non-decreasing.) ∎

**Verified.** 60 generations on a smooth synthetic landscape: `best_scalar`
non-decreasing at every step; climbed `0.953 → 0.997`.

---

## T5 — Evolutionary convergence (Rudolph 1994)

**Statement.** The elitist EA, with a mutation kernel of std-dev `σ ≥ σ_min =
0.02 > 0` (a full-support Gaussian reflected into `[0,1]`), converges almost
surely to the global optimum: `P(B_g → f*) = 1` as `g → ∞`.

**Proof.** The best-so-far chain is monotone (T4) and bounded above by `f*`,
hence convergent. Rudolph (1994, *Convergence analysis of canonical genetic
algorithms*, IEEE TNN 5(1)) proves that any EA which is (i) elitist and (ii) has a
mutation operator assigning positive probability to reaching every point of the
search space in one step converges to the global optimum a.s. Hypothesis (i) is
T4. Hypothesis (ii): the step is `g_i' = reflect01(g_i + σ·(0.3+0.7·gainᵢ)·N(0,1))`;
the Gaussian has full support on ℝ, `reflect01` maps onto `[0,1]`, `σ ∈ [0.02,0.5]`
so `σ ≥ 0.02 > 0`, and `0.3+0.7·gainᵢ ≥ 0.3 > 0`; hence every gene neighbourhood
has positive transition probability. Both hypotheses hold ⇒ a.s. convergence. ∎

*Scope (honest).* The conclusion is Rudolph's theorem applied to **verified
hypotheses**, not reproven from first principles; the check verifies the
hypotheses (elitism, `σ ≥ 0.02`) on the real engine.

**Verified.** Over 60 generations: elitism holds and `σ ∈ [0.02,0.5]` at every
generation; best reaches `0.994` (→ the `q = 1` optimum).

---

## T6 — Bell–CHSH / Tsirelson bounds (quantum-cognition layer)

**Statement.** For the implemented observables `σ(θ) = cosθ·Z + sinθ·X` and
correlation `E(a,b) = ⟨ψ| σ(a) ⊗ σ(b) |ψ⟩`:
(i) every normalized two-qubit state, at all angles, satisfies `|S| ≤ 2√2`
(Tsirelson); (ii) every separable (product) state satisfies `|S| ≤ 2` (the
classical CHSH bound); (iii) the Bell state `|Φ⁺⟩` at angles `(0, π/2, π/4, −π/4)`
attains `S = 2√2`. Here `S = E(a,b) + E(a,b') + E(a',b) − E(a',b')`.

**Proof.** `σ(θ)² = cos²θ·Z² + sin²θ·X² + cosθsinθ(ZX+XZ) = I`, since
`Z² = X² = I` and `ZX + XZ = 0`; so each `σ(θ)` is a `±1`-valued observable.
(i) The CHSH operator `B = A⊗B + A⊗B' + A'⊗B − A'⊗B'` satisfies
`B² = 4I + [A,A']⊗[B,B']`, and `‖[A,A']‖ ≤ 2`, `‖[B,B']‖ ≤ 2`, so `‖B²‖ ≤ 8`,
giving `‖B‖ ≤ 2√2` and `|⟨ψ|B|ψ⟩| ≤ 2√2`. (ii) For `|ψ⟩ = |α⟩⊗|β⟩`,
`E(a,b) = u(a)v(b)` with `u,v ∈ [−1,1]`; then
`S = u(a)(v(b)+v(b')) + u(a')(v(b)−v(b'))`, so
`|S| ≤ |v(b)+v(b')| + |v(b)−v(b')| = 2·max(|v(b)|,|v(b')|) ≤ 2` (equivalently, a
product state admits a local hidden-variable model). (iii) Evaluating `|Φ⁺⟩` at
the canonical angles gives each `|E| = 1/√2` with signs summing to `2√2`. ∎

**Verified.** Bell `chsh_optimal() = 2.8284 = 2√2` (±1e-6); 40 000 random states ×
random angles: `max|S| = 2.7173 ≤ 2√2`; 40 000 separable states × random angles:
`max|S| = 1.9993 ≤ 2`.

---

## T7 — Self-organised criticality (attracting fixed point)

**Statement.** The SOC tuning map `w ↦ w·(1 + λ(1 − σ))`, with branching ratio
`σ = k·w`, has a fixed point at `σ* = 1` (`w* = 1/k`) that is locally attracting
for `λ ∈ (0,2)`; the network self-organises to criticality from both sub- and
super-critical starts.

**Proof.** At `σ = 1` the factor `1 + λ·0 = 1` leaves `w` unchanged — a fixed
point. In `σ`-coordinates `σ_{t+1} = k·w_{t+1} = σ_t(1 + λ(1 − σ_t))`. Writing
`σ = 1 + ε`: `σ_{t+1} = (1+ε)(1−λε) = 1 + (1−λ)ε − λε²`, so to first order
`ε_{t+1} = (1−λ)ε`; `|1−λ| < 1 ⇔ λ ∈ (0,2)`, giving local linear convergence
`ε → 0`. ∎

**Verified.** `self_organise(λ = 0.4)` from `σ₀ = 1.8 → σ = 1.001` and from
`σ₀ = 0.4 → σ = 0.997` — both pulled to `1`.

---

## T8 — Reciprocity non-exploitation (iterated PD)

**Statement.** With payoffs `T > R > P > S` (`5,3,1,0`) over `r` rounds:
(i) tit-for-tat's score is at most `(T − S) = 5` below any single opponent's
(it is exploited at most once); (ii) TFT is **tied-optimal** in the field
`{AllC, AllD, TFT, Grim}` — no strategy strictly outscores it.

**Proof.** (i) TFT opens with C and thereafter copies the opponent's last move.
The only round where the opponent earns `T` while TFT earns `S` is the first round
the opponent defects against a just-cooperating TFT; from the next round TFT
mirrors the defection, so the pair stays in `{(D,D),(D,C),(C,C)}`, where no further
`(T vs S)` gap accrues. Hence `opp − tft ≤ T − S`. (ii) Against AllC both
TFT/Grim earn `rR`; against AllD both earn `S + (r−1)P`; against each other and
themselves both cooperate throughout (`rR`). AllD is dragged into mutual
defection and AllC is suckered by AllD; summing, no strategy exceeds TFT's total.
Grim *ties* TFT because no opponent here defects-then-cooperates, so TFT's
forgiveness is never tested. ∎

*Note (the overclaim the checker caught).* An earlier statement said "TFT wins the
field." The machine-check reported the tournament winner as **Grim** — because
Grim and TFT score identically here. The honest, provable claim is *tied-optimal*,
which is what is now stated and verified.

**Verified.** Exploitation gap `≤ 5` against each of `{AllC:0, AllD:5, TFT:0,
Grim:0}`; TFT's tournament score equals the top score (tied-optimal).

---

## T9 — Autonomous evolution (the full-autonomy / evolves-over-time leg)

**Statement.** The autogenesis loop improves the *real* AI's fitness over the
baseline genome with no human intervention, and halts on a principled `Verdict`.

**Proof (empirical property).** The loop's only inputs are a seed and the fitness
oracle (living real lives in `GameWorld`). By T4 the champion's fitness is
monotone and by T5 it converges; whether it strictly improves over baseline on a
given world is an empirical claim, established by running it. This is the
*evolves-over-time / full-autonomy* leg — stated as a property and checked, not a
closed-form theorem. ∎

**Verified.** With no human input, from baseline scalar `0.753` the loop reaches
champion `0.825` (`Δ +0.072`) in a few generations, halting with
`Verdict::Converged`. The fuller `autogenesis` example additionally validates the
champion on **held-out** seeds (generalisation, not memorisation).

---

## Running the checks

```
cargo run -p daimon-game --example proofs --release   # all theorems → [PROVEN]
```

The harness exits non-zero if any theorem fails, so it doubles as a regression
gate: a code change that breaks a proved property turns the proof red.
