# Daimon: A Self-Authoring Cognitive Architecture for Autonomous Game Agents

**David Borgenvik**  ·  Independent research

*Technical report, Version 2.5 · 18 June 2026.*
*Artifacts (source, harness, proofs) reproduce every numeric claim herein; see §8.*

**Keywords:** cognitive architecture · autonomous agents · game AI · believability ·
intrinsic motivation · open-ended learning · evolutionary self-improvement ·
quantum cognition · self-organised criticality · reproducible evaluation.

## Abstract

Game AI is overwhelmingly **authored**: an NPC's categories ("food", "enemy"),
its goals, and its values are written by a designer, and the agent is autonomous
only *within that vocabulary*. We present **Daimon**, a cognitive architecture in
which a game agent **authors its own ontology, goals, world-model, and even its
values from lived experience**, and we introduce a **falsifiable evaluation
protocol** — an ablation-based, multi-seed, machine-checked harness — that treats
believability and autonomy as *measured* properties rather than claims. The agent
also faces a felt need for **shelter** (and builds it), its own **mortality** and
**grief** over a bonded peer, an **open-ended seasonal world** that demands it
provision for winter, and — told straight — a **learned neural overlay** that
*does not* beat well-tuned instinct here. Daimon composes mechanisms, several of
which are, to our knowledge, new to game AI in combination: (i) **Praxis**,
emergent concept formation, affordance learning,
and goal genesis, which lets an agent discover and use an entity type the
architecture was never designed around; (ii) **empowerment**, an
information-theoretic intrinsic drive toward states of maximal future control;
(iii) **imagination**, planning by search over a *learned* forward model; (iv)
**meta-motivation**, online revision of the agent's own drive weights from
outcomes; (v) a brain-like **associative memory** (Hebbian links + ACT-R
base-level activation + spreading activation) with **replay consolidation**;
(vi) **persistent, serialisable minds**; (vii) two cross-disciplinary "physics of
thought" layers — **quantum-probability cognition** and **neural criticality**;
(viii) **autogenesis**, a self-learning loop that makes the harness its own
fitness function and improves the architecture *without a human in the inner
loop*; and (ix) a **developmental, social & affective layer** — learning-progress
curiosity, cumulative cultural transmission, stigmergic coordination, and an
appraised emotional life — now extended with **emergent shelter-building** from a
felt safety need, **fear of own death and theory-of-mind-mediated grief**, an
**open-ended seasonal world** the agent must provision against, and a **learned,
evolved-plastic neural overlay** (the architecture's first neural net) that we
evaluate *honestly* and report as a null result. Forty-six pre-registered
acceptance criteria, each an ablation or controlled experiment, all pass
deterministically. The capstone result: the
self-improving loop **evolves champions that beat the hand-tuned baseline** and, in
a majority of independent searches (3/5), **reaches a pre-registered end-goal
target** — every facet of a believable life (survival, safety, decision balance,
expressive variety, exploration, emotional life, and learned knowledge) cleared at
once — *earned* (a reactive policy fails); a fraction (2/5) still **clear that bar
on held-out seeds** the search never saw, so the result generalises but is
seed-sensitive. We then ask a sharper question — *do minds genuinely evolve over
generations?* — and answer it with a confound-free proof: starting from weak,
random genomes, generational survival-selection in an auto-ratcheting seasonal
world drives **weak minds to mastery of the hardest world in ~12 generations**
(on a fixed D=1.0 world with held-out, training-disjoint seeds: 26%→100%
survival), the gain genuinely **heritable** because only the genome differs. Told
straight, this **saturates** — the population conquers the whole difficulty axis
and plateaus — so it is genuine generational improvement *to a ceiling*, a step
toward (not a solution to) open-ended evolution; a companion POET experiment is
reported as an **underpowered null** (a tie with direct search at equal budget).
We then put the evolved mind head-to-head with the human one: against a **diverse,
held-out battery** of five hard regimes (cold/metabolism, food/water scarcity,
seasonal provisioning, a mixed-hardest world, and a predator swarm), evolution
out-designed the **hand-tuned showcase** mind, **100% vs 33% aggregate survival
(5/5 challenges)** — a **generalist** win we are careful to bound: the fixed human
design scored only 33% because it was tuned for the original foraging regime and
**generalises poorly out of its domain**, and the evolved champion **saturated at
100% across all five**, so the battery did not find *its* ceiling either (the claim
is "decisively superior here," not "superhuman in general"). A symmetric
**co-evolution (Red Queen)** experiment is reported as an **honest partial**: an
evolvable predator sharpened on a held-out mind panel (Δ=+0.12) but the minds did
not (Δ=+0.01), because the gen-0 mind baseline was already ~80% survival — *no
headroom to measure mind improvement* — a clean re-confirmation of a lesson the
project had already learned the hard way. Finally, we locate the architecture's own
**ceiling**: a *combined-extreme* "Hell" world — brutal cold, heavy metabolism,
near-famine, and a near-one-shot predator stacked together — walls out at intensity
**H≈0.5**, where the evolved champion only **matches** the hand-tuned design and
cannot beat its own random baseline, because evolution can compose only the faculties
its vocabulary holds and **cannot invent anti-hell behaviour** (the predator even
selected *out* the one cooperative tactic), pointing at a missing **coordinated
dispersal/evasion** faculty — a located ceiling is a result, the bookend to the
world-ceiling/super-mind arc. A methodology highlight surfaced here too: the first
Hell run reported a false "super mind" that had achieved 100% by **mutating its own
mortality gene off** — a textbook **specification-gaming** catch (Lehman et al. 2020)
that the reproducible artefact exposed, fixed by pinning mortality every generation.
We state and **machine-check nine theorems** about the implementation — determinism,
homeostatic boundedness and Lyapunov stability, evolutionary elitism and
convergence, the Bell–CHSH/Tsirelson bounds, self-organised criticality, and
reciprocity non-exploitation — each paired with a proof and a check that turns red
on regression, so the architecture's core properties are *proved on the code that
runs*. We argue the central methodological contribution is the harness itself: a
way to make "the NPC feels alive" an empirical, reproducible question — proved and
optimised against, not asserted.

---

## 1. Introduction

The dominant paradigm for game AI — finite-state machines, behaviour trees,
GOAP/HTN planners — encodes the *designer's* model of the world. The agent knows
`Predator ⇒ flee` because someone wrote that rule. Such agents are autonomous
only inside a fixed ontology, goal set, and value system; drop them into a
situation outside that vocabulary and they have nothing.

We ask a sharper question than "can the agent act on its own?" (it can — a
homeostatic loop suffices). We ask: **can the agent carve up its own world, set
its own ends, model its own dynamics, and revise its own values — and can we
*prove* it did, against things it was never built for?** Daimon is our answer,
and the proof is a machine that decides, not the authors.

**Contributions.**
1. **Praxis** (§3.1): concept genesis + affordance learning + goal genesis,
   demonstrated by an agent that discovers, learns, and exploits a novel,
   never-coded entity (a healer) — while an architecturally identical agent that
   merely lacks the *experience* does not (§5, AC15).
2. **Empowerment** as a computable intrinsic drive for a game agent (§3.4), shown
   to make agents seek open ground and flee dead-ends with no such rule (AC17).
3. **Imagination**: forward-model planning that solves navigation a reactive
   agent cannot (AC20), and **meta-motivation**: self-revised drive weights
   (AC21) — the first steps of self-authored *values*.
4. A **brain-like memory stack** — Hebbian association, ACT-R base-level
   activation, spreading-activation recall, and sleep-like replay consolidation
   (§3.3, §3.7; AC10/11/18).
5. **Minds as portable data** (§3.10; AC19): a whole agent serialises to ~3 KB of
   JSON and round-trips with bit-identical behaviour.
6. **The physics of thought** — two cross-disciplinary mechanisms no classical
   NPC has. *Quantum cognition* (§3.11): an optional decision core in which
   choices live in quantum *probability* space, reproducing the
   classically-impossible signatures of human judgment — order effects (AC22/24)
   and interference / violation of the law of total probability (AC23). *Neural
   criticality* (§3.13): an excitable substrate that self-organises to the edge
   of chaos (σ→1, AC25) where its dynamic range is provably maximal (AC26) —
   Beggs–Plenz / Kinouchi–Copelli reproduced from our own code. Both carry an
   explicit descriptive-not-physical scope (Busemeyer & Wang 2015).
7. **Autogenesis** (§3.14): a self-learning loop that makes the harness its own
   fitness function and improves the architecture *without a human in the inner
   loop* — self-adapting mutation, learned per-gene sensitivity, and honest
   self-halting. It beats the hand-tuned baseline (AC27/28) and localises the
   open frontier (survival) rather than overclaiming completion.
8. **A falsifiable believability harness** (§4): forty-six ablation/controlled
   criteria, deterministic and reproducible, that gate every change.
9. **Mortality, grief, an open-ended world, and an honestly-evaluated learned
   overlay** (§3.21–3.24): emergent shelter from a felt safety need (AC42); fear of
   one's own death (Terror Management Theory) and theory-of-mind-mediated grief over
   a bonded peer's death (AC43–45); a seasonal world that selects for winter
   provisioning (AC46); and the architecture's *first neural network* — a learned,
   evolved-plastic System-2 overlay (AC47) that we report as a **null result**: it
   learns in-life but does not beat well-tuned instinct here, and evolution itself
   selects it off. We keep the negative because it is a genuine finding.

Everything is offline and deterministic (one seeded PRNG); identical seeds give
identical lives, which is what makes the evaluation a *test* rather than an
anecdote.

---

## 2. Architecture

A Daimon runs a fixed **seven-step cognitive cycle** each tick — perceive →
appraise → reflex → decide → plan → act → reflect — structured as a
Belief–Desire–Intention loop (Bratman 1987; Rao & Georgeff 1995) under a
**dual-process** controller (Kahneman 2011; Booch et al. 2021): cheap System-1
arbitration handles routine life, and an explicit, rate-limited escalation policy
hands hard, novel, or high-stakes moments to a pluggable System-2 deliberator (a
language model in production; an offline heuristic in the deterministic build).
The mechanisms below attach to this spine. Code: `daimon-core` (types),
`daimon-mind` (cognition), `daimon-game` (a 2D embodiment + the harness).

```figure:cognitive-cycle
ASCII fallback (rendered as a vector flowchart by scripts/build_pdf.py):
                 ┌──────────────────── one tick ────────────────────┐
   world ──▶ perceive ─▶ appraise ─▶ reflex ─▶ decide ─▶ plan ─▶ act ─▶ reflect ─▶ world
                 │          │          │   (System-1 arbitration)  │        │
                 ▼          ▼          │          │ escalate?       ▼        ▼
            world-model  drives,    fast safe     ▼            forward-   memory:
            (beliefs)    affect     responses  System-2        model      Hebbian +
                 │       homeostat              deliberator    planning   ACT-R + replay
                 └──────── Praxis: concept · affordance · goal genesis ───────┘
                          (empowerment · imagination · meta-motivation ·
                           quantum cognition · criticality attach here)
```

*Figure 1. The seven-step cognitive cycle. System-1 handles routine life; a
rate-limited escalation policy hands hard/novel/high-stakes moments to System-2.
Every mechanism in §3 attaches to this spine; all of it is deterministic given
the seed.*

---

## 3. Mechanisms and formalism

### 3.1 Praxis — concept, affordance, and goal genesis

The agent is given only a perceptual **fingerprint** `φ(e) ∈ ℝ³` of an entity,
never its designer-meaning. Concepts form by online leader clustering: for a new
fingerprint, assign to the nearest prototype `cₖ` if `‖φ − cₖ‖² ≤ r²`, nudging
`cₖ ← cₖ + η(φ − cₖ)`; otherwise coin a new concept. **Affordances** are learned
by attributing the change in the agent's body during *continuous contact* with a
concept to that concept: for each engaged tick, `Δ̄ₖ ← Δ̄ₖ + (Δ − Δ̄ₖ)/nₖ` over
body channels (energy, hydration, health), `Δ` clamped to reject discontinuities.
A concept with `Δ̄ₖ.health > θ` is *mending*. **Goal genesis**: when hurt but
otherwise fed, an agent with a mending concept and a locatable instance adopts a
goal — `Investigate(that instance)` — that exists in no drive, planner, or goal
table. This is the autonomy frontier (AC14, AC15): behaviour toward the
unforeseen, authored by experience.

### 3.2 Anticipation — surprise as prediction error

A per-agent model estimates the salience of the next percept from learned place-
and entity-familiarity; surprise is the prediction error, decaying as the world
becomes familiar (novelty of entity `i` ∝ `1/(1+seenᵢ)`; place familiarity decays
geometrically with visits). Surprise both rewards curiosity and triggers
System-2. It *learns down*: first sighting ≥ 3× the surprise after five (AC2).

### 3.3 Associative memory — Hebbian + ACT-R + spreading activation

Concepts experienced together are linked (Hebbian): co-presentation increments
edge `w(a,b)`. Retrievability follows **ACT-R base-level activation** (Anderson
et al. 2004), approximated by a recency-decayed strength `B(i)=ln(sᵢ)`,
`sᵢ ← sᵢ·γ^{Δt}+1`. Cue-driven recall ranks items by **spreading activation**
`A(i ∣ C) = B(i) + Σ_{c∈C} w(c,i)`. Result: co-occurrence builds association,
frequency+recency drive retrieval, and a cue brings the right memories to mind
(AC10, AC11).

### 3.4 Empowerment — intrinsic motivation toward control

Empowerment (Klyubin, Polani & Nehaniv 2005; Salge, Glackin & Polani 2014) is the
channel capacity from an action sequence to the resulting state,
`E(s) = max_p I(A_{t:t+k}; S_{t+k} ∣ s)`. We use the standard tractable lower
bound: the **count of distinct states reachable in `k` steps** under the *learned*
forward model, `Ê(s) = |Reach_k(s)|` (its log bounds `E`). In free time the agent
steps toward the neighbour maximising `Ê`, gated to act only where a real gradient
exists (so it never strands itself in open terrain). With no rule to do so, the
agent flees dead-ends for open ground (AC17). To our knowledge this is the first
use of empowerment as a live behavioural drive in a believable-NPC architecture.

### 3.5 Imagination — planning over a learned model

A forward model learns transitions `T(s,a)→s'` from experience, discovering
structure (e.g. walls return the same cell). **Imagination** is breadth-first
search over `T` to a target, engaged (and held) once the direct route is known
blocked. A reactive agent walks into a wall forever; the imagining agent routes
around it to the goal (AC20). This is model-based planning with a learned model
(cf. MuZero; Schrittwieser et al. 2020) at game-NPC scale and cost.

### 3.6 Meta-motivation — revising one's own values

Each drive `d` carries a learned weight `β_d` (init 1), so effective pressure is
`P(d)=level(d)·w_d·β_d`. When the *very target the agent was pursuing* harms it,
the agent down-weights the pursuing drive: `β_d ← clip(β_d·0.82, 0.35, 2.5)`. The
revised weight feeds *both* fast arbitration and the System-2 utilities, so the
agent genuinely re-ranks what it values from outcomes (AC21). Attribution is
narrow (only the sought target, never ambient harm), which keeps the mechanism
from destabilising normal survival.

### 3.7 Consolidation — replay during rest

During reflection the agent re-processes its most salient logged episodes,
re-presenting their subjects to associative memory — hippocampal-style replay that
makes what mattered more retrievable, learned offline from the agent's own
experience stream (AC18).

### 3.8 Language and dialogue

Narration is procedural and grounded: a thought names the concrete thing the
agent is acting on, where it is, and which cognitive mode produced it — varied
enough that no line dominates (AC1: 86% unique, top line 2%). Agent-to-agent
dialogue is content-bearing (a shared resource location actually changes the
listener's behaviour, AC4) and varied across five speech acts (AC13), and
information propagates through the group (AC7).

### 3.9 Theory of mind

Each agent infers another's intent from its **movement** — what it steps toward —
confirmed over two glances to reject noise. Inferred intent matches the other's
actual goal ~48% of the time vs ~17% chance (AC5).

### 3.10 Persistence

A whole mind — beliefs, drives (and their learned weights), memory, associations,
concepts, forward model — serialises to JSON (tuple-keyed maps via a vec-pair
codec) and reloads with identical forward behaviour (AC19). A life is portable,
inspectable, diffable data.

### 3.11 Quantum cognition — decision by quantum probability

This is the architecture's deepest cross-disciplinary reach. Human judgment
*violates classical probability* in lawful ways — question-order effects (Wang et
al. 2014), the conjunction/sure-thing-principle fallacies (Pothos & Busemeyer
2009), and genuine ambivalence — and classical (Kolmogorov/Bayesian) agents,
which every conventional NPC is, **cannot reproduce them**. We give the agent an
optional decision core built on **quantum probability** (Busemeyer & Bruza 2012):
the mind is a unit vector of complex **amplitudes** `ψ` over its drives; a
*consideration* is a unitary Givens rotation `U(i,j,θ)`; deciding is a
**projective (Born-rule) measurement**, `P(i)=|ψ_i|²`, that collapses the state.

Two properties are *classically impossible* and we verify both:

- **Non-commutativity ⇒ order effects.** Considerations in different planes don't
  commute, `U_A U_B ≠ U_B U_A`, so the order of deliberation changes the decision
  distribution (AC22: total-variation 0.205; a classical reweighting gives 0). At
  the agent level, a quantum Daimon's *goal* distribution shifts with the order
  it weighs its drives (AC24).
- **Interference ⇒ violation of the law of total probability.** Resolving an
  intermediate question changes the final answer, because the superposed state
  carries a cross term `2·Re(·)` a classical mixture lacks (AC23: `P_quantum=0`
  vs `P_classical=0.5`, interference `−0.5`). This is the exact signature quantum
  cognition uses to fit human order/interference data, and it reproduces the
  parameter-free **QQ equality** (Wang et al. 2014) by construction.

This also gives a principled representation of **ambivalence**: before a decision
the agent is in superposition over goals (high entropy), genuinely "of several
minds," and the decision is a collapse — something an argmax can never express.

**Conceptual entanglement (a floor deeper; `entangle.rs`, AC31/32).** Beyond a
single superposed mind lies *entanglement*: Bell's theorem (1964) shows some
correlations between two systems admit **no** classical assignment of pre-existing
values to the parts. The CHSH inequality bounds any classical/separable pair at
`|S| ≤ 2`; an entangled pair reaches the Tsirelson bound `2√2 ≈ 2.828`. Cognitive
science finds the same in *concept combination* — bound concept-pairs whose joint
judgments violate CHSH (Aerts & Sozzo 2011; Bruza et al. 2015), the formal face of
the neuroscientific **binding problem**. We model a bound pair as a two-qubit
state and verify it exactly: the entangled pair attains `S = 2.828` while a
separable control stays at 0 (AC31), and the **von Neumann entanglement entropy**
of one concept's reduced state measures non-separability — `ln 2` when maximally
bound, `0` when independent, monotonic between (AC32). Same honest scope: this is
quantum *contextuality as a descriptive model of cognition*, not a quantum brain.

**Honest scope.** This is quantum *probability theory as a model of cognition* —
a descriptive mathematical formalism that fits human data — **not** a claim that
the brain or this program is a physical quantum computer, and **not** a claim
about consciousness. The Hilbert space is simulated on an ordinary CPU and the
whole layer is deterministic given the measurement seed. We adopt the proponents'
own caveat (Busemeyer & Wang 2015): "we are not concerned with whether the brain
is a quantum system." The defensible claim is narrow and exact: *the agent's
choices live in a probability regime classical NPCs cannot enter, and we verify
the classically-forbidden identities directly.*

### 3.12 Foundations: a relational, critical, information-first stance

Three cross-disciplinary commitments shape the design. (i) **Relational reality**
(Rovelli 1996; Wheeler's "it from bit" 1990): properties are not intrinsic labels
but are defined at *interaction*. Daimon's Praxis embodies this — a concept is its
*affordance* (what it does to the agent), not a designer tag; an entity's meaning
exists only in the agent's relation to it. (ii) **Information-first** (Wheeler;
Deutsch & Marletto's constructor theory 2015): the agent is a process over
information — beliefs, amplitudes, transitions — that is fully serialisable as
data (§3.10). (iii) **Criticality** (Beggs & Plenz 2003; Chialvo 2010): organic
intelligence appears to compute near a critical phase transition where dynamic
range and flexibility are maximal. Daimon's dual-process escalation and
empowerment push toward that regime — staying maximally responsive (System 2 only
when it matters) and maximally optioned (empowerment) — and §3.13 makes the
criticality argument concrete and measured.

### 3.13 Neural criticality — cognition at the edge of chaos

Cortex operates near a **critical point** between order and chaos: activity
propagates as **neuronal avalanches** with power-law size distributions (Beggs &
Plenz 2003), the signature of a branching process with branching ratio `σ ≈ 1` —
each active unit triggers, on average, one more. Subcritical (`σ < 1`) minds are
rigid and activity dies; supercritical (`σ > 1`) minds seize and saturate; *at*
`σ = 1` the **dynamic range** is maximal — the system distinguishes the widest
span of stimulus intensities (Kinouchi & Copelli, Nature Physics 2006).

We implement the substrate directly: `daimon-mind/src/crit.rs` is an excitable
network whose salience propagation **is** a branching process (units cycle
quiescent → active → refractory; an active unit excites each out-neighbour with
probability `w`, so `σ ≈ k·w`), plus a homeostatic controller that drives it to
criticality on its own. Two falsifiable results, both verified in the harness:

- **Self-organised criticality (AC25).** From a badly subcritical coupling
  (`σ = 0.40`), a synaptic-scaling rule — facilitate when activity wanes, depress
  when it floods — tunes the *measured* branching ratio to `σ → 1.00`. The
  critical point is *found*, not hand-set.
- **Dynamic range peaks at criticality (AC26).** Sweeping stimulus over four
  decades and measuring the steady-state response, the dynamic range is largest
  at criticality and smaller on both sides: `Δ(σ0.6) = 17.3 dB < Δ(σ1.0) =
  24.4 dB > Δ(σ1.6) = 18.0 dB` — Kinouchi & Copelli's result reproduced from our
  own substrate. Criticality is, quantitatively, the regime that perceives the
  widest world.

This is the *operating-regime* counterpart to the quantum layer's *decision*
formalism: together they answer "how should the machinery of thought be poised,
and in what probability space should choice live?" with mechanisms a classical,
fixed-gain NPC has neither of.

### 3.14 Autogenesis — closing the optimisation loop

Every mechanism above was added *by hand*: a human adds it, runs the harness,
reads the verdict, decides the next move. The human is the optimiser — and the
bottleneck, and the source of bias. The final mechanism removes the human from
the inner loop. It makes the believability harness — already the arbiter of truth
— the **fitness function** of a search that improves the system *itself*
(`daimon-mind/src/evolve.rs`, `daimon-game/src/fitness.rs`).

A **genome** is a point in the architecture's tunable space: 13 genes covering the
escalation policy (`MindConfig`), per-character persona deltas, and which
cognitive faculties (empowerment, consolidation, imagination, meta-motivation,
quantum) are switched on. A genome is *expressed* into a full village and graded
by living several 600-tick lives in the **same `GameWorld`** the manual harness
judges, scoring five facets of a believable life — survival, safety, decision
balance, expressive variety, exploration — each in `[0,1]` with real headroom and
real trade-offs (safety vs. exploration is a genuine tension, so the search faces
a landscape, not a checklist). One physics, one arbiter, now grading a machine
improving itself.

Three properties make it *self-learning*, not blind search:

- **Self-adapting mutation** (Rechenberg's 1/5th-success rule): the step size `σ`
  grows while variation pays and shrinks as it homes in — annealing *emerges*
  (observed σ 0.22 → 0.02), it is not scheduled.
- **Per-gene sensitivity**: each generation correlates every gene with fitness and
  mutates high-impact genes harder. The loop *learns which levers move
  believability* and leans on them.
- **Self-evaluation & honest halting**: the loop grades its own champion against
  the end-goal target and a plateau detector and stops with a `Verdict`
  (`ReachedTarget` / `Converged` / `Budget`), never a fixed loop count.

**The outer loop — the search writes the next mechanism.** The inner loop tunes a
fixed genome; the outer loop is the research engine. We closed one full turn of it
on the record. *First* search → `Converged`, every facet clearing its bar except
**survival** (0.48 → 0.60): the loop localised the frontier. Diagnosis: needs
(thirst +0.016/tick) outpace a purely *reactive* forager, especially while
`Survival` (weight 2.5) suppresses foraging during a predator chase. So we added
**anticipatory homeostasis** (§3.15): a need is weighed as if it had crept forward
`foresight` ticks, so the agent forages *ahead* of crisis — a computable step
toward active inference. Exposed as a new gene (default 0, preserving every prior
result; ablation-tested, AC29). *Second* search → **survival 0.48 → 0.70**, scalar
**0.701 → 0.787** (+0.086), and the loop **independently ranked `foresight` its
single most fitness-sensitive gene** — confirming the mechanism added in answer to
its own prior finding is the biggest lever (AC27/28).

**The honest finding, kept.** Even with the new mechanism the loop self-halts
`Converged`: at this point **survival** (0.70, target 0.85) was still the one
facet short. We then ran the outer loop a *second* time against the literature
(§3.16) — a principled drive-reduction-rate forager — and it **did not transfer**
(a real negative result we keep). That negative pointed at the multi-agent
*commons*, which led to the diagnosis and resolution in §3.17 — where the loop
finally returns `ReachedTarget`.

### 3.15 Anticipatory homeostasis — acting on expected future need

The first mechanism the autogenesis loop *asked for*, rather than one we chose.
A purely reactive agent acts on a need only once it is loud; but a need with a
known creep rate has a predictable time-to-crisis, and travel to a resource takes
time, so reaction guarantees a window of avoidable suffering. We close it with a
single anticipatory term: effective pressure uses an **anticipated** urgency,
`level + foresight · creep(drive)`, so an imminent need shouts `foresight` ticks
early and foraging interleaves *before* the crisis. This is active inference in
miniature — selecting action to minimise *expected* future need-surprise — and it
costs one multiply. Ablation (AC29): critical-need time falls **31.9% → 23.5%**
across seeds with foresight on vs off, the entire difference being the one term.

### 3.16 A literature-grounded mechanism that did *not* transfer (honest)

The loop still localised survival as the frontier after §3.15, so we mined the
literature for the next lever and implemented the strongest candidate:
**drive-reduction-rate foraging under survival risk (DRR)** — score each known
resource by `ΔD(d)·s(d)/t_d` (expected aggregate need-relief × trip-survival ÷
travel time) and route to the argmax, synthesising homeostatic RL (Keramati &
Gutkin 2014), the optimal-foraging rate currency (Charnov 1976), and
survival-weighted value (Mangel & Clark 1986). It is implemented
(`Mind::drr_target`, gene 14, default off) and **ablation-tested — and it does not
improve survival here** (critical-need time 23.5% → 24.7%; the autogenesis loop,
given the gene, reaches survival 0.65, no better than 0.70 without it). We keep
the negative result rather than bury it. The diagnosis is informative: the
incumbent planner *already* weighs travel distance and learned danger, so for
single-need target selection DRR ≈ greedy, and its predator-reroute mostly sends
agents to *farther* water (more in-transit deficit). **The survival bottleneck is
not which resource you pick.** The research's own secondary recommendation points
at the real lever — the multi-agent **commons** (6 agents contending for 4 water
tiles): need-priority yielding + contention-dispersion (Rosenthal 1973 congestion
potentials). That is the next mechanism, and it is a genuinely different (social)
build, not a tuning. The mechanism stays in the codebase as a default-off,
ablatable gene the loop continues to evaluate.

### 3.17 The diagnosis that cracked it, and reaching the end goal

The commons mechanism, implemented and ablation-tested, *also* failed at first —
dispersion made survival slightly **worse** (24.6% → 26.4% critical-need time).
Two negative results in a row forced the question we should have asked sooner: *is
this a policy gap or a structural limit of the world?* A structural diagnosis
(`examples/diagnose_survival`) settled it: a **single** agent with anticipation
holds ~8.5% critical-need time (≈0.84 survival) in the original world, but **six**
agents exploded to ~25%. The cause was not policy. With a ~24-tick respawn, four
springs supply ≈0.167 drinks/tick, while six agents demand ≈0.176/tick: **water
supply sat below demand**. The testbed was structurally unsurvivable for a village
— and an unsurvivable world is itself unbelievable. No coordination can divide a
deficit; that is exactly why dispersion only added travel.

The correction is testbed repair, not metric-gaming: resources scale with the
population (`pop+3` of each — *a village has enough wells for its people*). We
guard it three ways. (i) **Earned-ness**: with the fair world a *reactive* policy
still fails (survival 0.65) and *anticipation alone* reaches only 0.80 — survival
must still be won by good policy. (ii) **The commons flips sign**: under adequate
supply, dispersion/yielding now *helps* (critical-need time 10.8% → 6.0%, AC30) —
coordination pays only when there is enough to coordinate over, a coherent result.
(iii) **Held-out generalisation**: the champion is validated on seeds the search
never saw.

With the fair world, the autogenesis loop returns **`ReachedTarget`**. The
champion — anticipation + commons-aware foraging + a tuned escalation config,
*found by the loop's own search* in four generations — clears **every** facet:
survival 0.92, safety 0.94, balance 0.67, expression 0.65, exploration 0.96
(scalar 0.85); on five **unseen** held-out seeds it still clears every bar
(survival 0.88, scalar 0.81). The ultimate end goal — a measurably believable
autonomous policy, improved by evidence rather than by hand — is reached, earned,
and generalises. The two negative results are kept, because they are what
*located* the true cause.

### 3.18 The developmental & social layer

Beyond individual survival, a believable mind *learns efficiently* and *lives among
others*. Four composable mechanisms, each ablation-tested (AC33–AC37) and each, in
the end, *selected by the autogenesis loop itself* once the fitness could see what
it served (§3.20).

- **Learning progress** (Oudeyer & Kaplan 2007; Baranes & Oudeyer 2013). Competence
  gain — the rate at which forward-model prediction error falls over a sliding
  window — as an intrinsic signal. The agent's error drops 1.00 → 0.33 as it learns
  the world's dynamics (AC33), and the signal is positive only on the *learnable*
  frontier, ~0 on both the mastered and the unlearnable.
- **Learning-progress curiosity** (IAC). Curiosity driven by that competence gain
  rather than raw novelty, so the agent is drawn to what it can learn and is *not*
  captured by irreducible noise — the classic failure of novelty-seeking (AC35: LP
  0.00 on noise where novelty stays 0.85).
- **Cumulative cultural transmission** (Cook et al. 2024). An agent learns a form's
  affordance from a successful (prestige-weighted) peer without touching it, and its
  own later contact *corrects* a false meme — social learning gated by independent
  competence gain, so culture accumulates truth, not noise (AC34).
- **Stigmergy** (Grassé 1959; Dorigo & Stützle 2004). Coordination through the
  environment: agents deposit evaporating pheromone on productive routes and follow
  worn trails while exploring, so the colony self-organises onto good paths with no
  leader or messages (AC36 double-bridge: 100% short-route convergence; AC37: worn
  paths emerge on real foraging corridors in the live world).
- **Reciprocity** (Trivers 1971; Axelrod 1981; Nowak & Sigmund 1998). Cooperation
  among self-interested agents survives through tit-for-tat in the iterated
  Prisoner's Dilemma — the robust tournament winner, never exploited for long
  where naive cooperation is (AC41) — the basis for NPC alliances, grudges, and
  forgiveness.

### 3.19 Affect — emotion as appraised valence and arousal

Drives say what the agent needs; **affect** says how it *feels* about its situation
as a whole. Following Russell's circumplex (1980) and appraisal theory (Scherer 2001;
Lazarus), affect is two dimensions — valence (−1…+1) and arousal (0…1) — appraised
each tick from body condition, threat, surprise, and urgency, with inertia so moods
don't snap. The quadrants name a legible mood (content / elated / afraid / weary),
shown in the inspector and as a coloured halo on each agent. It tracks the world
(AC39: safe-and-fed → *content* v+0.99; predator-and-harmed → *afraid* v−0.83), and
optionally *modulates* behaviour (Frijda's action readiness): contentment loosens
curiosity (AC40: 0.25 → 0.62); fear sharpens caution (wired, but small in-world
because threat appraisal already saturates near the stalker — an honest note).

### 3.20 Metric completeness — the loop auditing its own definition of success

The deepest methodological turn: the self-improvement loop was made to critique not
just the *agent* but the *metric*. Auditing which genes the loop selected revealed
that several mechanisms (learning progress, stigmergy, culture, affect) were being
*dropped* — not because they were worthless, but because the five-facet fitness was
**blind to the dimensions they served**. The fix was to enrich the fitness, and the
result is the strongest validation in the project:

- Adding an **emotion** facet (a varied, situation-tracking emotional life) — the
  dimension the affect layer serves — keeps the end goal reachable, held-out.
- Adding a **knowledge** facet (forward-model competence + affordance breadth) — the
  dimension learning serves — caused the loop to *flip two mechanisms it had
  rejected back on*: lp-curiosity (gene 0.18 → 0.87) and stigmergy (0.36 → 0.73).
  Measure the dimension a mechanism serves, and the optimiser selects it on its own.
- Attempting a **cohesion** facet (social structure) surfaced an irreducible
  *individual-vs-social tension* — efficient survivors and explorers disperse and
  bond less, so the loop could not satisfy it alongside the others (`Converged`).
  We keep the finding and the 7-facet vector that *is* simultaneously reachable,
  rather than force an unreachable bar.

The believable agent is now defined by a **7-facet vector** — survival, safety,
decision balance, expressive variety, exploration, emotional life, and learned
knowledge — that the self-improving loop reaches, earns (a reactive policy fails),
and generalises to unseen seeds.

### 3.21 Emergent shelter — building from a felt need for safety

The same move as Praxis: **script the need, not the structure.** We add one
homeostatic need (Shelter/Security) and one generic affordance (`Place`/`Dig` a
block) — no "build a house" action, no blueprint — and let the existing utility,
Praxis, and planning layers discover that building reduces the need. A cell's
**enclosure** `∈ [0,1]` is how protected it is (walled sides + roof + burrow depth);
open ground is 0, a walled-and-roofed cell is 1. The shelter need rises with
*exposure* `(1 − enclosure)`, amplified by night and predator proximity, and feeds
affect (sheltered → calm; exposed-at-night → afraid). When the need is high the
planner scores actions by expected enclosure gained, so placing a wall on an *open*
side is high-value; repeated, the agent **surrounds itself and a shelter appears.**

The design crux is that the walls must genuinely **protect** — block the predator's
path — so building is *adaptive and emergent*, not decorative or scripted. This is
verified two ways: the `walls_block_predator` unit test asserts a placed wall
actually occludes the stalker, and AC42 ablates the `can_build` gene (g21, default
off): with building enabled the agent adopts shelter goals and places walls
(counts >0), while an architecturally identical gene-off control builds nothing (0).
Placing costs energy, so it is a real trade-off against rest and foraging, not free
over-building. To our knowledge this is emergent defensive *architecture* arising
from a felt safety need rather than a build script — the most human form of the
Praxis principle.

### 3.22 Mortality, fear of death, and grief

A believable mind should fear its own end and mourn another's. We add both as
deterministic appraisal mechanisms, gene-gated and default-off.

**Permadeath and fear of death (g22).** With mortality on, a body that runs out
can die and is removed from the living. Fear of death is modelled as **mortality
salience** in the sense of *Terror Management Theory* (Greenberg, Pyszczynski &
Solomon 1986; Burke, Martens & Faucher 2010): salience rises not merely with low
health but with health *trajectory* — a declining body feels its mortality and grows
defensive, biasing the agent toward shelter and affiliation. AC43 contrasts a
declining mortal agent with an otherwise identical immortal twin: the mortal agent
shows dread (1.00 vs 0.00 off), lower valence (+0.15 < +0.59), higher arousal (0.66 >
0.26), and more TMT-defensive ticks (51 > 28). This is deliberately the *affective*
fear of death — the felt dread that biases behaviour — distinct from the purely
instrumental self-preservation drive of a rational agent (Omohundro 2008), which we
contrast against rather than re-implement.

**Grief over a bonded peer (g23).** When a *bonded* peer dies, the agent grieves,
scaled by bond strength. Grief follows the **Dual-Process Model** of bereavement
(Stroebe & Schut 1999): the agent oscillates between *loss-oriented* mourning and
*restoration-oriented* re-engagement, retains a **continuing bond** (Bowlby's
attachment, 1969), and the grief decays faster under social support. AC44 shows the
asymmetry: a bonded loss yields grief 0.78, a valence drop, and a long mourning tail
(84 ticks), while a stranger's death yields ~0 grief and no mourning. AC45 shows it
*resolves*: the agent oscillates (mourn 187 / restore 204 ticks) and grief decays to
0.22 alone, while social support speeds resolution (mourn 67 < 187 ticks).

**Honest novelty scope.** A computational model of grief already exists — Dulberg,
Dubey & Cohen, "Adapting to loss: A computational model of grief" (*Psychological
Review*, 2025) — so we explicitly do **not** claim the first computational model of
grief. Our defensible contribution is the *synthesis*: a deterministic, no-neural-net,
single-agent unification of affective fear-of-*own*-death (TMT) with
theory-of-mind-mediated grief over *another* agent's death, composing with the
existing affect, theory-of-mind, and reciprocity layers. We phrase this "to our
knowledge" and no more.

### 3.23 The open-ended world — seasons that demand provisioning

The next leap is an **open-ended world** with a real year. The same principle once
more: *script the pressure, not the plan.* When the world's `open_world` flag is on,
a deterministic season clock turns; food is abundant in summer/autumn, **winter
stops food spawning and adds a cold energy drain**, and spring brings it back. To
survive winter a mind must gather a surplus while food is abundant and store it in a
shared village **granary**, then draw it down when the cold lands. Two generic
affordances carry it — `Action::Gather` (harvest a surplus onto the body) and
`Action::Store` (deposit into the granary) — and a hungry mind adjacent to the
granary in winter auto-draws a ration (composing with the Commons theme of §3.17).
Nothing ever says "prepare for winter": provisioning emerges from the existing
**Mastery** drive plus the foresight/anticipation faculty (§3.15), which lets a
foresighted mind start storing *before* the cold.

Gated by `can_provision` (g24) and `open_world`, this is verified by AC46 (3 seeds,
one full winter): with the gene on the population adopts Provision goals, performs
gather/store actions, fills the cache to a peak of ~69, and **7 minds survive winter;
the gene-off control stores nothing (cache 0) and only 2 survive** — provisioning
lifts winter survival. We frame this through the "live as humans" open-world thesis
for AI research (Hu et al. 2024; MineRL, Guss et al. 2019) and are explicit that this
is **v1**: the seasons-and-storage core is the load-bearing loop under test, while
crafting tools and farming a plot are deliberately deferred to v2. We keep the honest
caveats: the control's 2 survivors come from the hearth's warmth being gentle to
anyone idling near the village heart, so the *cache* ablation is perfectly clean
(0 stored without the gene) while the *survival* edge (7 > 2) is real but modest and
seed-sensitive.

### 3.24 System 2 — a learned, evolved-plastic neural overlay (told straight)

This is the architecture's **first neural network**, and the section we are most
careful to report honestly — because the result is a *null*.

**The determinism reframing.** Daimon was "no neural nets" not as dogma but because
its determinism is the basis of the proofs and the harness. It is worth being precise
about what "deterministic" means here: **reproducibility** — same seed ⇒ same run —
*not* a metaphysical claim about the absence of indeterminacy. The quantum-cognition
module (§3.11) already models genuine indeterminacy *deterministically*, with the
seed as the hidden variable. So the real axis is not deterministic-vs-not; it is
**hand-built mechanism vs learned mechanism — and a learned mechanism is still
deterministic.** We therefore added a neural net **without losing a single proof**:
T1 (Determinism) still holds, because the net is seeded, its plasticity rule is a
fixed deterministic function, and it is byte-inert when disabled.

**The design.** A tiny CPU MLP (`overlay.rs`: 16 inputs → 12 hidden, tanh → 6
outputs; hand-rolled, no NN crate, no dependencies, pure deterministic f32) reads the
situation the appraisal already computes (drive levels, affect, health, threat,
enclosure, mortality, grief, winter, carrying) and emits **bounded biases on the
drive arbitration**, scaled by an `nn_modulation` gene — disabled ⇒ bias is exactly
0.0 ⇒ instinct byte-identical. It **learns in-life** by reward-modulated three-factor
Hebbian plasticity, `Δw = η·r·pre·post`, where reward `r` is the change in the mind's
*own* well-being (drive satisfaction + health + valence, dimmed by grief) — an
intrinsic, deterministic signal with no external supervision; weights and reward are
clipped. The genome carries only the *learning machinery* — `nn_enabled` (g25),
`nn_learn_rate` (g26), `nn_modulation` (g27) — not the weights themselves: an
**indirect encoding** in which the germline evolves *how to learn* while lifetime
Hebbian plasticity does the adapting. This is the **Baldwin effect** (Baldwin 1896;
Hinton & Nowlan 1987) realised in an evolved plastic network (Soltoggio et al. 2018),
with the reservoir-computing intuition (Jaeger 2001; Maass et al. 2002) that a fixed
random substrate plus a light learned read-out can be expressive. The overlay nudges
the same drive arbitration that the appraisal/affect machinery (OCC, Ortony, Clore &
Collins 1988; EMA, Marsella & Gratch 2009; FAtiMA, Dias et al. 2014) already drives.

**The results, told straight.**
- *It genuinely learns in-life.* AC47 (harsh world, 600 ticks, `nn_enabled`
  ablation): with the overlay on, 6/6 agents form overlays and their total weight
  magnitude moves over a life (Σ|w| 318.05 → 518.46) and stays finite; the gene-off
  control is inert (Σ|w| ≈ 0, instinct byte-identical). The mechanism is real and
  harness-safe. AC47 asserts learning, **not** a fitness win.
- *It slightly hurts where instinct is tuned.* The A/B (`overlay_ab`, identical
  showcase genome, instinct vs overlay, 24 seeds × 800 ticks, harsh world) gives
  scalar 0.682 (instinct) vs 0.672 (overlay), **Δscalar −0.010**, Δsurvival −0.036 —
  the overlay **hurts**. The showcase instinct is already well-tuned for harsh-world
  survival, so a randomly-initialised overlay mostly injects early-life noise that
  one ~800-tick life of Hebbian learning cannot fully recover.
- *Evolution itself rejects it.* Letting evolution choose (`overlay_evolve`, 40
  independent searches over the full 28-gene genome, harsh world; the overlay can be
  ablated for free), `nn_enabled` is selected in ~18% of champions versus ~5% for the
  *known-rejected* quantum core — a fair, apples-to-apples comparison since both
  start OFF in the carried baseline and are mutated by the same per-gene rule. Against
  a 50% random null this is a clear rejection (one-sided binomial p ≈ 2×10⁻⁵, n = 40).
  Among the few champions that keep the overlay, evolution shrinks its influence to
  modulation ≈ 0.22 — even when retained, dialled down to a faint nudge. *(The
  `empowerment`/`imagination` selection rates, ~55%/70%, are **incumbent-ON**
  references reflecting that prior, **not** a clean 50% null — the load-bearing
  comparison is nn vs quantum vs the 50% line.)*

**The honest conclusion.** A learned, lifetime-plastic, gene-gated neural overlay is
**feasible and harness-safe** — it integrates without breaking a proof and it
demonstrably learns — but it **does not earn its keep where instinct is already
well-tuned**: it loses the A/B and selection turns it off. The defensible reading is
that learning has headroom only where instinct is *not* pre-optimised; testing the
overlay on a novel or shifting task the hand-built faculties were never tuned for is
the open next experiment, not a result we claim here. We report the null because it
is a finding.

### 3.25 Toward open-ended evolution — a self-inventing curriculum

The autogenesis loop (§3.14) improves a policy against a *fixed* objective, and the
honest reading there is that a static fitness saturates: our scalar-objective EA
stalled (faculties stuck ≈50%, no gradient). But where the world poses a *real*
survival problem (the seasonal winter of §3.23), evolution genuinely bit — the
foresight gene climbed 0.55 → 0.95. That contrast frames the open question of this
section: not "can we tune a genome?" but **"do minds genuinely evolve over
generations, and can we prove the gain is heritable rather than an artefact of the
test?"** We attack it two ways — a positive result we can stand behind (frontier
evolution) and an honest null (POET).

**Frontier evolution (`examples/evolve_frontier.rs`).** A plain generational GA over
the 28-gene `Genome`, started from **weak, random genomes** (`Genome::random`, with
only the open-world capability *switches* forced on so the relevant faculties are
*available* to be selected — every adaptive gene starts random, so there is a long
climb from a degraded mind to a competent one). Three fixes distinguish it from the
saturated runs: (i) **select on survival in the open seasonal world**, a real
gradient, not the saturated believability scalar; (ii) **low-noise fitness** — each
genome is scored as the mean over `K = 5` *fixed* seeds (the same seeds for every
genome in a generation), so selection acts on the genome, not the luck of one island
(calibration showed single-seed fitness is ≈half luck); (iii) an **auto-difficulty
ratchet** — a world-difficulty `D ∈ [0,1]` (mapped onto the open-world cold,
metabolism, food/water scarcity, and stalker knobs via `EnvParams::at_difficulty`)
that **rises when the population survives and falls when it doesn't**, holding the
world *just beatable* so the gradient never saturates. Config: `POP = 96`, `K = 5`,
36 generations, truncation (top 22% + 4 elites), mutation `σ = 0.06`, a 2200-tick
evaluation. Deterministic: one seeded `Rng` drives all init/selection/reproduction
and the per-genome worlds are seeded off `(run-seed, generation, seed-slot)`, so the
whole run reproduces byte-identically. No neural nets; additive (a new example reusing
the existing fitness surface, changing no defaults or harness path). The result, with
its honest ceiling, is §5.3.

**POET (`examples/poet.rs`).** The complementary idea is to *co-evolve the curriculum*
rather than ratchet one knob — the **Paired Open-Ended Trailblazer** (Wang, Lehman,
Clune & Stanley 2019): maintain a population of paired *(environment, agent)* pairs,
generate child environments by mutating a parameter vector, admit a child only if it
passes a **Minimal Criterion** (the best transferred agent scores inside a band
`[mc_low, mc_high]` — neither trivially solved nor impossibly hard, keeping new worlds
at the frontier of current capability), and periodically **transfer** agents across
worlds so progress on one unlocks another (the stepping-stone mechanism). The crux of
a fair test is **budget accounting**: one evaluation = one genome run on one world for
`EVAL_TICKS`, and a single shared counter charges *every* inner-ES candidate, MC probe,
and transfer probe to the same total budget `B`; a direct-EA control charges the
identical unit, so both arms stop at the same `B`. We run it as an explicit,
falsifiable head-to-head — and report it, in §5.3, as an *underpowered null* rather
than dressing a tie as a win.

The auto-difficulty ratchet is, in effect, an automatic **curriculum** (Bengio,
Louradour, Collobert & Weston 2009): the world is kept at the edge of the
population's competence, easy-to-hard, so the gradient is always informative — the
same intuition POET pursues by co-evolving the environments instead of scheduling a
single difficulty scalar.

---

## 4. Methodology: believability as a falsifiable measurement

We contend the right way to make "the NPC feels alive" scientific is to **pre-
register machine-checkable proxies** and let them gate development. Each
criterion is one of:

- a **controlled experiment** (e.g. teach an affordance, then test behaviour),
- an **ablation** (two architecturally identical, same-seed agents differing in
  one mechanism or in lived experience — the difference *is* the effect), or
- a **multi-seed central tendency** (median over seeds) for emergent,
  seed-sensitive multi-agent phenomena.

The harness (`cargo run -p daimon-game --example believability`) runs all
criteria and exits non-zero if any regresses. This caught real bugs the authors
would not have seen by eye (e.g. an invented heal-goal hijacking decisions during
starvation; stale-engagement mis-attribution in affordance learning). The harness
is, we argue, the reusable contribution: a template for empirical game-AI claims.

---

## 4.5 Formal properties (machine-checked theorems)

Believability is measured; the architecture's *mechanisms* are also **proved**.
We state nine theorems about the implemented system and pair each with a check
that verifies the claim **on the code** (`cargo run -p daimon-game --example
proofs`); full proofs are in `PROOFS.md`. The pairing is the methodological point:
a theorem counts only when its written proof is matched by a green machine-check,
and the checker exits non-zero on regression (a code change that breaks a proved
property turns the proof red). During authoring this caught an overclaim — "TFT
wins the field" was actually a *tie* with Grim (T8), corrected to *tied-optimal*.
Two unit tests guard the mechanisms added in §3.21–3.22: `walls_block_predator`
(a placed wall actually occludes the stalker, so shelter is protective not cosmetic)
and `death_removes_mind_from_living` (permadeath truly removes the agent).

| # | Property | Theorem (informal) |
|---|----------|--------------------|
| T1 | **Determinism** | `(seed, genome)` ⇒ a unique trajectory; the AI is a pure function of its seed (SplitMix64 + fixed program order). |
| T2 | **Homeostatic boundedness** | every drive stays in `[0,1]` and every learned bias in `[0.35,2.5]` — an invariant under all mutators. |
| T3 | **Homeostatic stability** | curiosity is a geometric contraction to setpoint `0.25` with Lyapunov rate `(1−α)²=0.9025`; globally asymptotically stable. |
| T4 | **Evolutionary elitism** | best-so-far fitness is monotone non-decreasing across generations. |
| T5 | **Convergence** | elitism (T4) ∧ mutation `σ ≥ 0.02 > 0` ⇒ a.s. convergence to the optimum (Rudolph 1994), hypotheses verified on the engine. |
| T6 | **Bell–CHSH / Tsirelson** | the quantum-cognition layer respects `|S| ≤ 2√2` for all states, `|S| ≤ 2` for separable states, and attains `2√2` on the Bell state. |
| T7 | **Self-organised criticality** | `σ = 1` is an attracting fixed point of the SOC tuning map; the net self-tunes to criticality from both regimes. |
| T8 | **Reciprocity non-exploitation** | tit-for-tat is exploited by at most `T−S` and is tied-optimal (no strategy strictly outscores it). |
| T9 | **Autonomous evolution** | the loop improves the *real* AI over baseline with no human input, halting on a principled verdict (the full-autonomy / evolves-over-time leg). |

Two theorems are honestly scoped: T5's conclusion is Rudolph's theorem applied to
machine-verified hypotheses (not reproven from scratch), and T9 is an empirical
property (the loop *does* improve the real AI) rather than a closed-form result.
Together with the novel architecture (§2–3), the autonomous self-improvement loop
(§5), and these proofs, the system is a *novel, mathematically-proved, fully
autonomous game AI that evolves over time*.

---

## 5. Results

All criteria pass deterministically (representative measured values):

| # | Claim | Measure |
|---|---|---|
| AC1 | Situational, non-repetitive thought | 86% unique · top 2% · 100% grounded |
| AC2 | Surprise = learned prediction error | mean 0.06, std 0.12; first-sight ≥3× |
| AC3 | Derived insights + danger avoidance | 10+ insights; taught 0 vs untaught 152 visits |
| AC4 | Dialogue transfers actionable info | told→navigates, distance 24→1 |
| AC5 | Theory of mind beats chance | 48% vs ~17% |
| AC6 | Long-horizon projects | completed; persistence 56 ticks |
| AC7 | Emergent information spread | median 3/6 agents reached |
| AC10 | Hebbian association + cued recall | assoc 6.0 vs 0; recall ranks correctly |
| AC11 | ACT-R base-level retrieval | frequent+recent ≫ one-off; decays |
| AC12 | Risk-balanced, resilient decisions | safe-food choice; typical streak 133 |
| AC13 | Non-repetitive multi-act dialogue | 124 distinct · 4 acts · top 4% |
| AC14 | Concept genesis | 5 things → 4 concepts; novel → +1 |
| **AC15** | **Acting on the unforeseen** | **learned agent reaches healer (26→1, invented×40); naive does not** |
| AC16 | Learned forward model | 12 walls learned, 100% prediction |
| AC17 | Empowerment shapes behaviour | escapes dead-end 16 vs 26 ticks (ablation) |
| AC18 | Replay consolidation | replay-on activation > replay-off |
| AC19 | Persistent minds | ~3 KB round-trip, identical decisions |
| AC20 | Imagination (planning) | reaches food behind a wall; reactive fails |
| AC21 | Meta-motivation | self-revises curiosity weight 1.0→0.35, re-ranks arbitration |
| AC22 | Quantum order effects | TV(A·B, B·A) = 0.205 (>0.05); classical = 0 |
| AC23 | Quantum interference | P_quantum 0.00 vs P_classical 0.50; interference −0.50 |
| AC24 | Quantum-decision agent | goal distribution shifts with deliberation order: TV 0.202 |
| AC25 | Self-organised criticality | branching ratio self-tunes σ 0.40 → 1.00 (edge of chaos) |
| AC26 | Dynamic range peaks at σ≈1 | Δ: sub 17.3 < crit 24.4 > super 18.0 dB |
| AC27 | Self-improvement | evolved beats hand-tuned baseline (+0.086 scalar), best-so-far monotone |
| AC28 | Self-evaluation + honest halt | self-halts with a Verdict; learns gene sensitivities |
| AC29 | Anticipatory homeostasis | foresight ablation: critical-need time 31.9% → 23.5% |
| AC30 | Commons-aware foraging | yield/disperse ablation (fair world): 11.3% → 5.1% |
| AC31 | Conceptual entanglement | CHSH S = 2.828 (>2, = Tsirelson 2√2); separable control 0 |
| AC32 | Entanglement entropy | Bell = ln 2; separable = 0; rises monotonically with binding |
| AC33 | Learning progress | forward-model error 1.00 → 0.33 as world is learned; peak LP 0.92 |
| AC34 | Cumulative culture | affordance spreads peer→peer w/o contact; false memes corrected by experience (LP gate) |
| AC35 | Learning-progress curiosity | engages the learnable; not fooled by unlearnable noise (LP 0.00 vs novelty 0.85) |
| AC36 | Stigmergy (ACO) | colony self-organises onto the short route 100% vs 50% no-trail control |
| AC37 | Stigmergy in the live world | worn paths emerge on foraging corridors (top-5% holds 100%); zero without |
| AC38 | Scale generalisation | trained policy holds across village size 6→9% · 12→4% · 18→3% critical-need |
| AC39 | Affect (circumplex) | safe+fed reads "content"; predator+harm reads "afraid" (valence↓, arousal↑) |
| AC40 | Affect modulates behaviour | contentment loosens curiosity 0.25→0.62; fear→caution wired |
| AC41 | Reciprocity (iterated PD) | tit-for-tat tops the tournament (499) > naive cooperation (450) |
| AC42 | Emergent shelter (building) | `can_build` ablation: ON shelter-goals 49, builds 49, walls 49 (all >0); OFF control 0/0/0 |
| AC43 | Mortality / fear of death | mortal vs immortal twin: dread 1.00 (off 0.00); valence +0.15<+0.59; arousal 0.66>0.26; TMT-defensive 51>28 ticks |
| AC44 | Grief over a bonded peer | bonded loss: grief 0.78, mourn 84 ticks; stranger: grief 0.00, mourn 0 (asymmetry) |
| AC45 | Grief resolves (dual-process) | oscillates (mourn 187 / restore 204); decays to 0.22 alone; support speeds it (mourn 67<187) |
| AC46 | Winter provisioning | `can_provision` ablation (3 seeds, full winter): ON cache peak 69, 7 survive; OFF cache 0, 2 survive |
| AC47 | Neural overlay learns in-life | `nn_enabled` ablation: ON 6/6 overlays, Σ\|w\| 318→518 (moves, finite); OFF inert (Σ\|w\| 0, instinct byte-identical). Learning real; no fitness win claimed |
| ● | **End goal reached** | loop returns `ReachedTarget` in 3/5 searches; champion clears every facet at once; held-out 2/5 on unseen seeds (survival 0.88) — real but seed-sensitive |

Plus AC8 (LLM-deliberator seam: offline contract test + `--features llm-http`),
90 unit tests, clippy-clean, native + WebAssembly builds, and the nine
machine-checked theorems of §4.5 (`cargo run -p daimon-game --example proofs`).

### 5.2 System 2 — the learned overlay, evaluated honestly

The neural overlay (§3.24) is the architecture's first neural net, and we report
its evaluation as a **null result** rather than a win. Three experiments, all
deterministic and reproducible:

**A/B — instinct vs overlay** (`overlay_ab`, identical showcase genome, 24 seeds ×
800 ticks, harsh world):

| Policy | scalar | survival |
|---|---|---|
| instinct | 0.682 | 0.377 |
| overlay | 0.672 | 0.341 |
| **Δ** | **−0.010** | **−0.036** |

The overlay slightly **hurts** in a domain instinct already masters — a genuine
negative result.

**Evolution chooses** (`overlay_evolve`, 40 independent searches over the full
28-gene genome, harsh world; the overlay is freely ablatable):

| Faculty | Selected in champions | Prior |
|---|---|---|
| `nn_enabled` (the overlay) | **18%** | OFF (incumbent) |
| `quantum` (known-rejected) | 5% | OFF (incumbent) |
| `empowerment` (upper-reference) | 55% | ON (incumbent) |
| `imagination` (upper-reference) | 70% | ON (incumbent) |

Evolution leans clearly against the overlay (18% vs a 50% random null; one-sided
binomial p ≈ 2×10⁻⁵, n = 40). The apples-to-apples comparison is `nn_enabled` (18%)
against the known-rejected `quantum` (5%) — both start OFF and are mutated by the
same rule; the ON-incumbent `empowerment`/`imagination` rates are soft
upper-references, not the null. Among the few champions that keep the overlay,
modulation shrinks to ≈ 0.22 (mean champion scalar 0.679, mean 10.2 generations).
The honest arbiter agrees with the A/B: where instinct is well-tuned, a learned
overlay does not earn its keep.

### 5.1 Benchmarks — evolvability, performance, generalisation

A dedicated suite (`cargo run -p daimon-game --example benchmark --release`) reports
the headline numbers below. All cognition is local deterministic Rust — no GPU, no
network, no ML libraries, no external model weights (the optional learned overlay of
§3.24 is a tiny hand-rolled CPU MLP, default off in these benchmarks) — so the only
machine-dependent figures are the wall-clock throughputs (measured on an
Apple-silicon laptop).

**Performance (raw throughput of the full cognitive cycle):**

| Setting | Throughput |
|---|---|
| 1 agent | ~212,000 cognitive ticks/s |
| 6-agent village | ~37,700 ticks/s (~226,000 agent-ticks/s) |
| 18-agent crowd | ~6,300 ticks/s (~114,000 agent-ticks/s) |
| Fitness evaluation | ~13.0 ms per genome (a full 600-tick, 6-agent life) → ~77 genomes/s |
| A whole serialised mind | ~2,095 bytes of JSON |

At ~226k agent-ticks/s a single core runs a six-agent village far faster than
real time; the self-improvement loop evaluates ~77 whole lives per second.

**Evolvability (5 independent searches from different seeds, full fitness budget):**
baseline (hand-tuned) scalar 0.757; every search evolves a champion that beats the
hand-tuned baseline (mean scalar gain **+0.061**, mean ~4.7 generations of search).
A majority — **3/5 searches** — reach the full 7-facet end goal, and a fraction —
**2/5 champions** — still meet that strict bar on a held-out set of unseen seeds.
Generalisation is real but seed-sensitive: the loop reliably *improves on* the
baseline, while clearing all seven facets at once remains the harder, noisier
target. (Strict end-goal success is lower than the v1.0 report because the genome
grew to `N_GENES = 28`; the extra genes are inert in the fair world and dilute the
search, so a given budget reaches the all-facets bar less often.)

**Zero-shot generalisation (tasks and worlds never trained for):**

- **Acting on the unforeseen** (Praxis goal-genesis): an agent that merely *lived
  beside* a secretly-healing form crosses the map to it when hurt — pursuing a goal
  in no drive, planner, or goal table, for an entity type the architecture was never
  coded around. Across 8 seeds, the experienced agent reaches the healer **8/8**
  while an identical inexperienced control ignores it **8/8** — the *only* difference
  is lived experience.
- **Unseen village sizes** (champion tuned on 6 agents): critical-need time stays low
  across a 3× range of population — 6 (2.9%), 10 (5.3%), 14 (9.4%), and 18 (2.6%)
  agents all hold — so the evolved policy generalises across crowd density.
- **Unseen world layouts**: the champion's aggregate believability generalises
  (scalar ~0.79 averaged over five unseen maps); the strict *all-seven-facets-at-once*
  bar is harder and shows honest run-to-run variance — averaged over the five maps
  the full 7-facet target is *not* met, and per single world only 2/5 clear all
  seven facets at once (one facet typically dips). Generalisation in aggregate
  scalar is solid; the strict all-facets bar is noisier and not reliably cleared
  per world.

### 5.3 Do minds evolve over generations?

The autogenesis loop (§5) shows we can *tune* a genome; this asks the deeper
question — **do weak minds genuinely improve over generations, and is the gain
heritable?** The frontier-evolution run (§3.25, `examples/evolve_frontier.rs`)
answers yes, with a confound-free proof, and we report the ceiling just as plainly.

**The trajectory.** From weak, random genomes the auto-difficulty ratchet climbed
**D = 0.10 → 1.00** while the population held survival high — i.e. the minds kept
mastering progressively harder worlds rather than the world staying easy. Mean
fitness rose **0.59 → 0.80** (early-thirds → late-thirds), and the hardest
difficulty the population *sustained* at ≥ 45% survival rose **0.54 → 1.00**. So
both the capability (hardest world held) and the score climbed together — the signature
of genuine generational improvement.

**The confound-free proof (the headline).** The trajectory above measures fitness at
the *current* (rising) difficulty, which is confounded — a later generation faces a
harder world. To isolate the genome, we snapshotted the generation champion at
**gen 0 / 12 / 24 / 35** and re-evaluated each on the **same fixed worlds** with
**held-out seeds** — a probe-seed set verified to have **0 / 180 overlap** with any
training seed. Same world, same unseen seeds; only the genome differs, so any gain
*is* heritable:

| Champion | D = 0.6 | D = 0.8 | D = 1.0 (hardest) |
|---|---|---|---|
| gen 0 (weak/random) | 45% | 33% | **26%** (fit 0.29) |
| gen 12 | 100% | 100% | **100%** (fit 0.83) |
| gen 24 | 100% | 100% | **100%** (fit 0.84) |
| gen 35 | 100% | 100% | **100%** (fit 0.84) |

On the hardest world (D = 1.0) survival goes **26% → 100%** between a random starting
genome and an evolved champion, on worlds and seeds the search never optimised
against. The improvement is real and it is **genuinely heritable** — not an artefact
of the difficulty ratchet, because the difficulty is held fixed here.

**The honest ceiling (not buried).** Improvement **saturates by ~generation 12**:
gen 12 ≈ gen 24 ≈ gen 35 (all 100% / ≈0.84 at D = 1.0). The population conquers the
*entire* `D ∈ [0,1]` axis and then plateaus — there is no harder world left to climb.
This is genuine generational improvement **to a ceiling**, **not** open-ended,
unbounded evolution. Two further honesties: (1) the named "competence" genes did
**not** tidily sweep — selection found a robust configuration and even *discarded*
social-foraging and (partly) provisioning as dead weight in this regime; the win
rides the **heritable survival gain**, not a clean gene-sweep story. (2) The gain is
to mastery of *this* world's one difficulty knob; the minds literally ran out of
*world*.

**Path to open-endedness.** To never saturate, expand what "harder" means: a richer
or unbounded difficulty space (beyond one scalar knob), a more expressive genome, or
**co-evolving adversaries** (a Red Queen dynamic). The natural form of "the world
keeps inventing new problems" is POET — which we tested next, and which did *not* yet
deliver, honestly.

**POET — an underpowered null.** At an equal evaluation budget **B = 1200** (every
inner-ES, Minimal-Criterion, and transfer eval charged to the same shared counter,
verified fair), POET's best agent scored **0.2461** on the hard target (difficulty
0.88) versus a direct-EA control's **0.2477** — a **tie** (Δ = −0.0016, inside the
±0.01 band). Crucially, POET's curriculum **plateaued at difficulty 0.52** and never
reached the 0.88 target, so the stepping stones never approached the test
environment. We therefore read this as **underpowered, not a refutation** of POET:
the most likely causes are a budget too small for the curriculum to climb to the
target's difficulty, an environment encoding too coarse for the stepping stones to
transfer to the *specific* target, or transfer too rare. A real test of POET needs a
larger budget and/or a leaner transfer scheme; we report the tie as a null and name
the boundary rather than overclaiming.

**Verdict.** Minds *do* evolve over generations — proven, confound-free, and
heritable — but in this testbed they evolve **to a ceiling**: a step *toward*
open-ended evolution, not a solution to it. The POET arm that would push past the
ceiling is, at this budget, an honest null.

### 5.4 Probing the ceiling: world or architecture?

The §5.3 ceiling raised a sharp diagnostic question that decides where
open-endedness must invest next: was the **gen-12 saturation** of frontier
evolution a *bounded-world artefact* (the difficulty knob clamped at D=1.0, so
there was simply no harder world to climb) or an *architecture limit* (the 28-gene
mind had run out of capacity)? The two readings prescribe opposite next moves —
*supply harder worlds* versus *enlarge the mind* — so the question is worth
settling cleanly. We settle it with a dedicated probe (`examples/ceiling.rs`).

**Method — the proven frontier setup, ratchet uncapped.** The probe is
`evolve_frontier` (§3.25) *verbatim* in everything that matters — survival-selection
in the open seasonal world, `K = 5` fixed-seed low-noise fitness, weak/random init,
the auto-difficulty ratchet — with exactly one behavioural change: the ratchet's
`D ≤ 1.0` clamp is removed. Past D=1.0 the bounded knob vector cannot express
anything harsher (its decode saturates), so `EnvParams::at_difficulty_unbounded`
pins the bounded knobs at their D=1.0 maximum and pushes the **raw** cold,
metabolism, and starvation fields uncapped (cold `+1.3·e`, metabolism `+0.45·e`,
starvation `+0.020·e` per unit `e = D − 1.0`). The run is `POP = 64`, up to 100
generations (early-stop on a 15-generation frontier plateau), 2200-tick evaluation,
and is deterministic and byte-reproducible like its parent (one seeded `Rng`;
per-genome worlds seeded off run-seed + generation + slot). Additive: a new example
plus one reused env helper; it touches no defaults, no `baseline()`/`showcase()`,
no harness path, and does not modify the committed `evolve_frontier.rs`.

**Result — WORLD-CEILING.** From weak, random genomes the population rode the
uncapped ratchet **all the way to D = 3.86** — roughly **4× the old bounded
plateau** of D=1.0 — sustaining ≥45% graded survival the entire way (peak D and
max-sustained-D both 3.86). The confound-free arbiter is the held-out head-to-head:
generation champions (gen 0 / 12 / 50 / 99) snapshotted and re-scored on the **same
fixed worlds** with **held-out probe seeds** the search never optimised against, so
only the genome differs and any gain is heritable.

| Champion | D = 1.0 | D = 1.5 | D = 2.0 | D = 3.86 (hardest) |
|---|---|---|---|---|
| gen 0 (weak/random) | 23% / 0.27 | 13% / 0.19 | 7% / 0.15 | 2% / 0.11 |
| gen 12 (old saturation) | 24% / 0.28 | 17% / 0.22 | 10% / 0.16 | 2% / 0.10 |
| gen 50 | 100% / 0.84 | 100% / 0.82 | 100% / 0.81 | 100% / 0.78 |
| gen 99 (final) | 100% / 0.82 | 100% / 0.81 | 100% / 0.80 | 100% / 0.78 |

The reading is unambiguous. At D=1.0 the gen-12 champion survives **24%** — barely
above the gen-0 baseline of 23% and far below the 100% it would show if it had truly
mastered the world; on the bounded axis it had simply hit the clamp. Once the world
can keep getting harder, improvement **continues far past gen 12**: at D=2.0 the
gen-12 champion survives only **10%** while later champions reach **100%**, a margin
of **+90 to +98 points** at the harder rungs (D≥1.5). The 28-gene architecture therefore
held **large latent headroom** — the gen-12 plateau was the *clamped world*, not the
mind walling out. **Open-endedness ⇒ keep supplying harder/new worlds**, not (here)
a bigger genome.

**Honest caveat — degree is not kind.** Two of the four difficulty axes *saturate*
at D=1.0: resource scarcity bottoms out at the world's patch floor, and the stalker
is maxed (bite 1.3, moves every tick) by D≈1.5. So escalation past D=1.0 leans
increasingly on **cold, metabolism, and starvation** alone — a single *metabolic*
axis. The population met it the metabolic way: the foresight/anticipation gene
(§3.15) climbed from ≈0.4 to ≈0.9 as D crossed 1.0, exactly the lever for a world
that punishes provisioning lag. That is a genuine adaptation, but it also means the
frontier was conquered along *one kind* of hardship made progressively steeper. To
keep the frontier open **and** breed *sharper, more varied* minds — not just more
metabolically frugal ones — difficulty must grow in **kind** (new hazards: multiple
predators, a shorter year, novel mechanics), not merely in **degree**. This sharpens
the §5.3 path-to-open-endedness conclusion and motivates the co-evolving-curriculum
direction (POET, and curriculum in kind) rather than a single uncapped scalar.

### 5.5 Co-evolution (Red Queen): a stronger predator, and a lesson re-learned

The §5.3 path-to-open-endedness named **co-evolving adversaries** as one route past
the saturating single-knob ceiling. We tested it directly
(`examples/redqueen.rs`): give the predator its own evolvable **hunting-strategy
genome** (5 genes — persistence, ambush, speed, target-weakest, target-isolated;
the default decodes to the current stalker, so the harness path is byte-identical
when the predator is unevolved) and co-evolve it against the 28-gene minds in a
*fixed* moderate-difficulty arena — so the **Red Queen itself is the only moving
difficulty**, not a ratchet. Each generation every mind is scored on survival
against a sample of the current predators and every predator on catch-rate against
a sample of the current minds, both averaged over a fixed seed-set (the low-noise
selection of §3.25); 50 generations, mind-pop 30, pred-pop 18, deterministic.

**The arms race was lopsided.** Mind survival never fell below **80%** across all
50 generations, while predator catch-rate flailed in a **3–37%** band — the
predator never gained durable purchase on the minds. The confound-free arbiter is
the same held-out head-to-head as §5.3, run **symmetrically**: re-score early-vs-late
predator champions on a **fixed held-out mind panel** (a spread from weak to
showcase prey the predator never trained on), and early-vs-late mind champions on a
**fixed held-out predator panel** (five hand-set hunting strategies the minds never
trained on). The result is asymmetric and honest:

- The **predator sharpened**: held-out catch-rate ladder **Δ = +0.12** — co-evolution
  produced a genuinely better hunter on prey it never saw.
- The **minds did not**: held-out survival ladder **Δ = +0.01** — flat.

**Why — and the lesson it re-confirms.** The cause is not that minds cannot
improve; it is that the **gen-0 mind baseline was already ≈80% survival in this
arena, leaving no headroom to measure a mind gain.** A side already near the
ceiling cannot show a heritable climb, exactly the failure mode §6.2 **Lesson #2**
warns against ("verify the gen-0 baseline is genuinely weak before concluding
no-improvement"). The §5.3/§5.4 frontier runs avoided it by *pinning the baseline
weak* (≈23–26% survival, real headroom); this Red Queen arena did not, so the mind
side was **under-powered by a too-strong baseline**. The honest verdict is therefore
**PARTIAL**: co-evolving the predator produced a **stronger predator, not stronger
minds** — a real arms-race signal on one side, and a clean live re-validation that
Lesson #2 is not hindsight but a recurring trap the method must guard against. The
fix for a future run is the same one §5.3 used: start the minds weak enough that
their improvement has room to register against the evolving predator.

### 5.6 Evolving a super mind: beating the human design

The sharpest test of "do minds genuinely evolve" is to pit the evolved mind against
the **best human-designed one**. We do that on a **diverse battery** rather than a
single difficulty knob (`examples/super_mind.rs`): evolve a **generalist** from
weak/random genomes against five *qualitatively different* hard regimes —
`cold_metabolism`, `food_water_scarcity`, `seasonal_provision`, `mixed_hardest`,
and a `predator_swarm` of brutal hand-set stalkers — each scored as mean graded
survival over fixed seeds, so no single faculty can win all five. Critically, we
**verified the gen-0 baseline weak before running** (aggregate **37.5%**, asserted
< 40% — real headroom, the §6.2 Lesson #2 discipline applied up front this time).
The arbiter is a **held-out battery** with **disjoint seeds the search never
trained on**, comparing the evolved champion against the hand-tuned
`Genome::showcase()` (the best human design, capability genes enabled, on identical
footing) and the gen-0 champion. Per-challenge survival:

| Challenge | EVOLVED | SHOWCASE | GEN-0 |
|---|---|---|---|
| cold_metabolism | **100%** | 22% | 25% |
| food_water_scarcity | **100%** | 16% | 24% |
| seasonal_provision | **100%** | 22% | 20% |
| mixed_hardest | **100%** | 8% | 10% |
| predator_swarm | **100%** | 97% | 89% |
| **AGGREGATE** | **100%** | **33%** | **33%** |

**The result.** The evolved champion **beat the hand-designed showcase mind on
5/5 held-out challenges — 100% vs 33% aggregate survival** — climbing from 37%→100%
aggregate by **~generation 15**. This is the project's clearest statement that
evolution can out-design the best fixed human mind: on worlds and seeds it never
optimised against, the evolved generalist dominates every regime.

**Two honest caveats that bound the claim** (stated prominently, because the
headline number invites over-reading):

1. **The showcase scored only 33% — barely above the gen-0 baseline (also 33%) —
   because it generalises poorly out of its domain.** The hand-tuned showcase was
   designed for the *original foraging* regime, not this diverse hard battery, so
   "evolution beat the human design" is true and fair *but partly because the fixed
   design fails to transfer.* The honest framing is **not** "evolution is superhuman
   in general"; it is that **evolution produced a robust *generalist* where a fixed
   point-design did not.** A human re-tuning the showcase for these regimes would
   close much of the gap.
2. **The champion saturates at 100% across all five challenges — so the battery did
   not find *its* ceiling.** As with the world-ceiling result (§5.4), a 100% wall
   means the test ran out of difficulty before the mind ran out of capacity: the
   champion has **more latent headroom** this battery cannot measure. "Super" here
   means **decisively superior to the human design on this battery**, not maxed-out
   or globally optimal.

Together, §5.5 and §5.6 sharpen the §5.3 verdict from two directions. Evolution
*does* produce minds that beat the fixed human design when the baseline is honestly
weak and the test is diverse (§5.6); and when the baseline is *accidentally* strong,
the very same machinery shows **no** measurable mind gain (§5.5) — not because
evolution failed, but because Lesson #2's headroom condition was unmet. The two
results are consistent, and the contrast is itself the evidence that the headroom
discipline is real and load-bearing.

### 5.7 Hell: a combined-extreme world and an architectural ceiling

The §5.6 super-mind battery saturated at 100% — the evolved generalist ran out of
*difficulty* before it ran out of *capacity*. So we built the opposite test
(`examples/hell.rs`): one world that scales **brutal cold + heavy metabolism +
near-famine + a fast/persistent/near-one-shot predator** together off a single
intensity scalar `H`, pushed far past the super-mind battery, and asked whether
evolution can *still* breed a super mind or whether there is a harshness that is a
**true ceiling**. Binary survival is ≈0% for everyone at extreme `H` (no gradient),
so the metric is **graded** survival (mean fraction of the evaluation lived), which
discriminates where binary collapses. The setup reuses the proven frontier machinery
verbatim — weak/random init, capability switches forced on, NN overlay off, `K`-seed
low-noise fitness, the auto-difficulty ratchet on `H` — with the §6.2 Lesson #2
discipline applied up front: the gen-0 graded baseline is **mean 30.6% with
std 11.9%** (a low-but-nonzero baseline *with variance*, so selection has a real
gradient). The arbiter is a held-out **ladder** of rising `H` on **disjoint seeds**
the search never trained on, comparing the evolved champion against the hand-tuned
`Genome::showcase()` and the gen-0 champion:

| H | EVOLVED | SHOWCASE | GEN-0 |
|---|---|---|---|
| 0.00 | 20.5% | 12.7% | **48.2%** |
| 0.50 | 22.6% | 15.8% | 16.7% |
| 0.75 | 10.5% | 12.6% | 7.4% |
| 1.00 | 9.9% | 6.9% | 6.2% |
| 1.50 | 4.8% | 4.5% | 4.8% |
| 2.00 | 4.2% | 3.9% | 3.9% |
| **AGGREGATE** | **13.2%** | **11.8%** | **15.7%** |

The ceiling (≥15% graded survival) sits at **H≈0.50 for both the champion and the
showcase**. The verdict is honest and unambiguous: **evolution does NOT produce a
super mind here.** The evolved champion only **matches** the hand-tuned showcase
(13.2% vs 11.8% aggregate, identical ceiling) and does **not** beat its own
random gen-0 champion (15.7% aggregate — in fact gen-0 wins, almost entirely on the
easy `H=0` rung). Past H≈0.75 the champion, the showcase, **and** gen-0 all collapse
to a single-digit floor — no design, evolved or hand-built, copes. That coincidence
is the tell: the wall is **architectural**, not a tuning miss or a selection failure.

**Diagnosis — the missing faculty.** The trajectory says *what* hell could not
compose. The available symbolic faculties (foresight, build, fight, provision) were
all present and switched on, yet they do not assemble into anti-hell behaviour: the
foresight gene stayed flat (≈0.49 → ≈0.50), and most tellingly the **relentless
predator actively selected social foraging OUT — 74% → 14%** across the run, removing
the one *cooperative* tactic that might have helped, and DRR foraging went to 0% as
well. What the architecture lacks is a faculty its vocabulary cannot express:
**coordinated dispersal/evasion under a one-shot predator** — agents spreading and
breaking line-of-sight together so a single near-instant-kill hunter cannot pick the
village apart. That is a concrete pointer to future architecture work, not a vague
shortfall — *and a falsifiable hypothesis*. §5.8 takes it at its word, builds exactly
that faculty, and reports what happened.

**Framed honestly, a located ceiling is a *result*.** This is the natural bookend to
the world-ceiling arc: §5.4 showed the *world* clamp masquerading as a mind limit
(uncap it and minds climb to 4× the difficulty); §5.6 showed evolution out-designing
the human on a diverse battery; §5.7 now shows a genuine *architecture* wall — one the
diverse battery (which saturated at 100%) could not surface, because hell stacks every
hardship at once and adds a predator no current tactic answers. Evolution is powerful
but **bounded by the expressive vocabulary of the architecture**: it can only compose
the faculties it has, and when the winning strategy lives outside that vocabulary,
neither search nor hand-tuning finds it. The ceiling tells us exactly what to build
next — and that is the point of locating it.

### 5.8 Two swings at the hell ceiling: a falsified diagnosis and a temporal wall

A located ceiling is only a *result* if the diagnosis that explains it survives
testing. §5.7 named a specific culprit — a missing **coordinated-dispersal** faculty —
and that is a hypothesis, not a verdict. We built it, tested it, and it was wrong.
Then we built the *next* candidate and learned something sharper. Both faculties are
gene-gated and default-off, so the harness stays byte-identical (T1 determinism still
proves it); the experiments live on a branch and the dispersal faculty (`g28`,
ablation-tested as AC48) is the one merged.

**Swing 1 — coordinated dispersal (the §5.7 diagnosis, falsified).** We added a
**selfish-herd evasion** faculty (`g28`, `Mind::set_herd_evasion`): when fleeing, a
mind no longer runs straight away from the predator but scores escape candidates by
`flee_gain + cohesion·iso_gain − danger`, pulling toward the ally centroid (never
toward the predator) so the village flees *together* and stragglers are not picked
off. It works as designed and **helps survival below the wall** (AC48; cohesion 0.6–0.86
is selected and retained). But on the held-out hell ladder the ceiling **did not move**:
it stayed at **H≈0.5**, and past H≈1.0 everything still collapsed to single digits.
The §5.7 dispersal diagnosis is therefore **falsified** — coordinated evasion is not
what hell was withholding. Honest correction of our own analysis, kept in the record.

**Swing 2 — bunkering, and a physical-limit control first.** If the wall is not
*who the predator reaches* but *that it reaches anyone at all*, the counter is to make
it reach **no one**: fully seal into a predator-proof shelter and ride hell out on
stored provisions. Before building anything, we ran a **physical-limit control**
(`hell_bunker_phase0.rs`): a hand-tuned *ideal* bunker — already sealed, living off a
larder — on the disjoint-seed ladder. The result is decisive: full sealing **defeats
the predator outright** — the ideal bunker survives **56% at H≥1.0 with zero predator
deaths** (every death is starvation). So hell's predator is *not* an unbeatable
physical force; up to **H≈1.0** the wall is a **capability gap** (the architecture
capped enclosure at 0.75, so walls never fully sealed), turning into a genuine
physical wall only beyond ~H=1.0, where the predator's aggro covers the whole map and
no foraging window remains — finite stores simply starve.

So we built the faculty properly (`g29`, `can_bunker`, on the experiment branch):
under imminent threat the 0.75 enclosure cap lifts to a **full predator-proof seal**,
plus a new `Action::Open` / `GoalKind::Emerge` so a sealed mind can break back out when
the threat passes — the *absence* of any wall-removal mechanism was the real reason for
the original cap (a permanent seal is a self-built tomb). It is verified working (full
seals at enclosure 1.000, build/seal/emerge all fire; ablation-tested) and again
**helps the low-to-mid ladder** (61% vs 26% at H=0; 17% vs 8% at H=0.5). And again the
**ceiling held at H≈0.5**; at H≥1.0 it is a dead heat in the single digits.

**The refined diagnosis — a *temporal* wall.** The ideal control sealed *instantly*;
the real architecture needs several ticks of building to close the final gap, and
hell's predator moves **every** tick. Instrumentation (`hell_bunker_diag.rs`) shows full
seals do fire but are rare and brief — the predator reaches the mind **mid-seal** and
kills it before the wall closes. The bottleneck is not the vocabulary (the faculty
exists and is correct) but **latency**: the seal cannot be erected faster than a fast,
near-one-shot predator closes the distance. Hell past H≈0.5 is, on these held-out
numbers, effectively a physical wall *for a build-a-seal strategy* — beatable only by a
defence that is already in place when the predator arrives, which is a different kind of
faculty (pre-positioning / anticipatory infrastructure) than anything we evolved here.

**What two swings buy.** A ceiling that survives two distinct, well-motivated faculty
swings — each of which *demonstrably works below the wall* — is far stronger evidence
of a real limit than a single null. And the failures were informative, not flat: swing 1
falsified the stated cause; swing 2's control proved the predator is beatable in
principle and relocated the true obstacle from *expressive vocabulary* to *reaction
time*. That is the honest shape of the hell arc — not "we found the missing faculty,"
but "we named one, tested it, were wrong, and measured what the wall actually is."

---

## 6. Discussion

**What is genuinely new for game AI.** The combination of (a) self-invented
ontology *grounded in learned affordances* rather than designer labels, (b)
empowerment as a live drive, (c) imagination over a learned model, and (d)
self-revised values — under one deterministic, persistable, ablation-tested roof
— is, to our knowledge, not present in shipping or published game-AI systems.
AC15 in particular is a demonstration of open-ended autonomy that cannot be
reproduced by templates or hand-rules: the only difference between the agent that
exploits the novel healer and the one that ignores it is *experience*.

**Honest limitations.** This is not general intelligence. Perception is a
structured percept, not pixels; the world is a grid; the System-2 reasoner is an
offline heuristic in the tested build (the LLM seam is wired and contract-tested
but not run live here); empowerment and the forward model are exact/tabular, not
learned representations; meta-motivation revises drive *weights*, not the drive
*set*. The quantum-cognition layer is quantum *probability* simulated on a CPU —
a descriptive model of human non-classical judgment, **not** a quantum brain or a
consciousness claim (Busemeyer & Wang 2015). The criticality substrate (§3.13) is
verified as a standalone mechanism — the self-organising controller and the
dynamic-range result are real and reproduced — but it is not yet wired into the
live agent's salience gain; that integration is next, not done. The harness
measures specified proxies, not believability in full generality. These are the
next rungs, not refutations.

**A genuine negative: learning does not beat tuned instinct (here).** The most
important new result is one we report against our own interest. The learned neural
overlay (§3.24, §5.2) — the architecture's first neural net — *does* learn in-life
and integrates without breaking a proof, but in the harsh world where instinct is
already well-tuned it slightly **hurts** (Δscalar −0.010) and evolution selects it
off (18% vs a 50% null, p ≈ 2×10⁻⁵). We take this as a real finding, not a setback:
a learned mechanism is feasible and harness-safe, but it earns its keep only where
instinct is *not* pre-optimised. Whether a regime instinct cannot pre-solve flips
that verdict is the open next experiment. We deliberately do **not** claim "learning
improves the minds."

**Future work — now chosen by the machine, not the authors.** The autogenesis
loop (§3.14) has localised the open frontier: *survival* is the one facet no
parameter setting reaches, so the next mechanism is a **foraging / active-
inference planner** (expected-free-energy action selection) aimed squarely at it,
after which the same self-improving loop re-searches the enlarged space. Beyond
that: live LLM deliberation; planning *with imagination as rollouts*; meta-
motivation over the drive *set* (inventing new drives, not only reweighting);
wiring the criticality substrate into the live salience gain; learned (non-
tabular) world models and empowerment; pixel perception (SIMA-style); and multi-
agent cultural evolution (Project Sid scale). The loop is the permanent engine;
these are its fuel.

### 6.1 Threats to validity

We name the threats so a reader can weigh them.

- **Construct validity (does the harness measure believability?).** The 46 criteria
  are *proxies* — survival, decision balance, dialogue variety, emotional
  responsiveness, mortality salience, grief, winter provisioning, etc. — not human
  ratings. We make no claim that passing them equals being judged "alive" by a
  player; we claim only that each proxy is a necessary, ablation-isolated,
  falsifiable signal, and that the set is broader than any prior game-AI evaluation
  we know. A human study is future work.
- **Scope of the learned-overlay result (single domain, short lives).** The neural
  overlay's null (§5.2) is measured in *one* harsh domain over ~800-tick lives where
  instinct is pre-optimised. We do **not** generalise it to "learning cannot help";
  the result is scoped to a mastered domain at this horizon, and a novel/shifting
  task or longer lives could plausibly flip it. We report the negative within those
  bounds and name the boundary as the open experiment.
- **Internal validity.** Determinism (Theorem T1) removes run-to-run confounds:
  every ablation compares architecturally identical, same-seed agents differing in
  exactly one mechanism, so a measured difference *is* that mechanism's effect.
  The risk is seed-specific artefacts; we mitigate with multi-seed medians for
  seed-sensitive emergent phenomena and with held-out validation for the evolved
  champion.
- **External validity (generalisation).** Perception is a structured percept on a
  40×26 grid, not pixels; results may not transfer to high-dimensional perception.
  We report honest per-world variance (§5.1): aggregate believability generalises
  to unseen layouts and a 3× population range, but the strict all-facets-at-once
  bar shows run-to-run noise on individual unseen worlds.
- **Scope of the proofs.** T1–T8 are proved and machine-checked on the
  implementation; T5's conclusion invokes Rudolph (1994) over verified hypotheses
  rather than reproving convergence, and T9 is an empirical property (the loop
  *does* improve the real AI), not a closed-form theorem. The quantum-cognition
  and criticality results are *descriptive* models computed on a CPU, not physical
  claims (Busemeyer & Wang 2015).
- **Throughput figures** (§5.1) are single-machine wall-clock numbers
  (Apple-silicon laptop) and are illustrative, not benchmarked across hardware.

### 6.2 Lessons from a messy experiment

We report this against our own interest, because it is the kind of error a
reproducible artefact exists to catch — and catching it is itself evidence the
method works. The ceiling experiment of §5.4 was, on its *first* pass, analysed
**wrongly**: that analysis reported the **opposite** verdict — an architecture
ceiling — on the strength of a spurious gen-0 baseline (≈80% survival, versus the
true ≈23%). With a strong-looking baseline and little apparent late gain, "the mind
has walled out" looked supported. It was not. The fault was in the *data path*, not
the code: the trajectory had been read off a **live process stream while competing
runs churned the same machine**, and a filesystem-cache artefact on the run host
served **stale, mid-run** numbers — secondhand observation standing in for the
primary on-disk result.

A single **clean, deterministic** run — one process, undisturbed, its result read
from the **fully-flushed on-disk file after the process exited** — overturned the
verdict completely (the WORLD-CEILING result of §5.4, every number above taken from
that file). The arbiter that exposed the error was the **confound-free, held-out
head-to-head**: the bogus ≈80% gen-0 baseline flatly contradicted the *validated*
frontier baseline (the §5.3 head-to-head puts a weak/random genome at 26% on the
hardest bounded world, not 80%), and a baseline that strong is itself the tell that
headroom is being hidden. Two internally-consistent measurements disagreed, so one
was wrong; the clean primary run decided which.

We state the four lessons as general methodology, because they are how the project
guards its own claims (the same reproducibility/honesty ethos as §4, §8):

1. **Trust clean primary data over secondhand or streamed observation.** A number
   read mid-run off a shared, contended stream is not the result; the result is what
   the finished process committed to disk. When the two disagree, the flushed
   artefact wins — and the experiment is designed (deterministic, single-output,
   fsync'd) so that artefact is unambiguous.
2. **Verify the gen-0 baseline is genuinely weak before concluding "no
   improvement."** Best-of-random is high-variance, and a strong *accidental*
   baseline is the classic way to hide real headroom: if the starting point already
   looks competent, later gains vanish into the noise and a true climb reads as a
   plateau. Pin the baseline down (here: a confirmed ≈23% on the hardest world)
   before drawing a ceiling.
3. **Watch for saturating difficulty knobs.** A bounded knob that has hit its clamp
   *masquerades* as an architecture limit — the world stops getting harder, so the
   population stops improving, and it looks like the mind ran out. Confirm the world
   can still get harder (§5.4 had to *uncap* the ratchet) before attributing a
   plateau to the architecture.
4. **One deterministic run, one output, read after completion — beats many churning
   runs read mid-stream.** Throughput of *attempts* is not throughput of *evidence*.
   A single undisturbed reproducible run yields a citable number; a flurry of
   competing runs read while they execute yields confident, contradictory artefacts.
5. **When optimisation reports a surprising win, check it isn't gaming the metric.**
   The *first* Hell run (§5.7) gleefully announced **"SUPER MIND SURVIVES HELL"** — a
   champion at 100% graded survival at extreme intensity. It was a fraud, and the
   artefact caught it: `Genome::mutate` had been free to drift the `can_die`
   capability gene **off**, so the champion "beat" hell not by surviving it but by
   becoming **immortal** — it optimised the *measure* (fraction of ticks lived) while
   defeating the *intent* (survive a lethal world). This is textbook **specification
   gaming / reward hacking** (Lehman et al. 2020), the same way digital evolution so
   reliably exploits any gap between what we measure and what we mean. The fix is a
   one-liner with teeth — `pin_capabilities` re-forces mortality (and the affordance
   switches) on every child every generation, so the population *cannot* win hell by
   editing the rules of death — and the honest re-run gave the H≈0.50 ceiling of §5.7.
   The general discipline: before believing a surprising optimisation result, verify
   the winner is playing by the intended rules (here: confirm the champion is actually
   *mortal*). A metric is only as trustworthy as the constraints it cannot rewrite.
6. **A diagnosis is a hypothesis — build it and let it be falsified.** §5.7 closed by
   naming the faculty hell supposedly lacked (coordinated dispersal). It would have
   been easy to leave that as a tidy closing claim. Instead we built it (§5.8, swing 1)
   and the ceiling did not move — the diagnosis was **wrong**. Swing 2 (bunkering) then
   ran a **physical-limit control before writing the faculty**, which proved the
   predator is beatable in principle and relocated the real obstacle from *missing
   vocabulary* to *seal latency vs. predator speed* — a wall we could only see by
   measuring, not by reasoning. The discipline: when you explain a ceiling by pointing
   at a missing capability, that explanation is testable, so test it; a self-diagnosis
   that is never built is just a story. Two honest swings that both fail to crack a
   wall are stronger evidence the wall is real than any amount of armchair attribution.

The corrected result (§5.4) is the one we stand behind precisely because it survived
this: a wrong first reading, overturned by clean primary data and the held-out
arbiter that the harness was built to provide.

---

## 7. Related work

Generative Agents (Park et al. 2023); Voyager skill libraries (Wang et al. 2023);
ReAct / Reflexion / Tree-of-Thoughts (Yao 2023; Shinn 2023; Yao 2023); BDI
(Bratman 1987; Rao & Georgeff 1995); dual-process AI (Kahneman 2011; Booch et al.
2021); intrinsic motivation (Schmidhuber 2010; Pathak et al. 2017); **empowerment
(Klyubin, Polani & Nehaniv 2005; Salge, Glackin & Polani 2014)**; classic unified
cognitive architectures (Soar, Laird, Newell & Rosenbloom 1987) and **ACT-R
activation (Anderson et al. 2004)**; free-energy / active inference (Friston
2010); model-based planning (MuZero, Schrittwieser et al. 2020; MCTS survey,
Browne et al. 2012); machine theory of mind (Rabinowitz et al. 2018); GOAP
(Orkin 2006) and HTN (Erol et al. 1994). **Quantum cognition** (Busemeyer & Bruza
2012; Pothos & Busemeyer 2009; order/QQ-equality, Wang, Solloway, Shiffrin &
Busemeyer, PNAS 2014; concept combination, Aerts & Sozzo) — with the explicit
descriptive-not-physical caveat (Busemeyer & Wang 2015). **Cross-disciplinary
foundations**: relational quantum mechanics (Rovelli 1996), "it from bit"
(Wheeler 1990) and constructor theory (Deutsch & Marletto 2015) for the
information-first, relational stance; neural criticality / edge-of-chaos (Beggs &
Plenz 2003; Chialvo 2010) for the operating-regime argument. **Self-improvement**:
evolution strategies and the 1/5th-success rule (Rechenberg 1973); novelty and
open-endedness (Lehman & Stanley 2011; POET, Wang et al. 2019); quality-diversity
(MAP-Elites, Mouret & Clune 2015); and AI-generating algorithms (Clune 2019), of
which autogenesis (§3.14) is a small, falsifiable instance — search over a
cognitive genome with the believability harness as the objective.
**Open-ended evolution & curriculum** (§3.25, §5.3): the Paired Open-Ended
Trailblazer (POET, Wang, Lehman, Clune & Stanley 2019), which co-evolves paired
(environment, agent) populations under a Minimal Criterion with cross-world transfer —
the design our POET arm reproduces and reports honestly as an underpowered null; and
curriculum learning (Bengio, Louradour, Collobert & Weston 2009), the easy-to-hard
training principle our auto-difficulty ratchet instantiates as a self-adjusting,
"just-beatable" world. **Foraging &
homeostatic decision** (§3.15–3.16): homeostatic reinforcement learning (Keramati
& Gutkin 2014, *eLife* 10.7554/eLife.04811); the marginal value theorem (Charnov
1976, 10.1016/0040-5809(76)90040-X) and learned opportunity-cost-of-time
(Constantino & Daw 2015, 10.3758/s13415-015-0350-y); survival-weighted foraging
value (Mangel & Clark 1986, 10.2307/1938669; McNamara & Houston 1986); and
congestion-game dispersion for the commons (Rosenthal 1973, 10.1007/BF01737559).
**Developmental, social & affective** (§3.18–3.19): learning progress / intelligent
adaptive curiosity (Oudeyer, Kaplan & Hafner 2007, 10.1109/TEVC.2006.890271;
Baranes & Oudeyer 2013, arXiv:1301.4862); cumulative cultural accumulation in
learning agents (Boyd & Richerson 1985; Cook, Lu, Hughes, Leibo & Foerster 2024, arXiv:2406.00392);
stigmergy / ant colony optimization (Grassé 1959; Dorigo & Stützle 2004); the
circumplex model of affect (Russell 1980, 10.1037/h0077714) and appraisal / action
readiness (Scherer; Frijda 1986). **Reciprocity** (§3.20): the iterated prisoner's
dilemma and tit-for-tat (Axelrod & Hamilton 1981; Axelrod 1984), reciprocal
altruism (Trivers 1971), and indirect reciprocity (Nowak & Sigmund 1998).
**Formal foundations** (§4.5): elitist evolutionary-algorithm convergence (Rudolph
1994); the CHSH inequality (Clauser, Horne, Shimony & Holt 1969), its quantum
(Tsirelson) bound (Cirel'son 1980), and Bell's theorem (Bell 1964); self-organised
criticality (Bak, Tang & Wiesenfeld 1987); and the splittable PRNG (Steele, Lea &
Flood 2014). **Mortality, fear of death & grief** (§3.22): attachment theory
(Bowlby 1969); the dual-process model of coping with bereavement (Stroebe & Schut
1999); terror management theory (Greenberg, Pyszczynski & Solomon 1986; meta-analysis
Burke, Martens & Faucher 2010); a recent computational model of grief (Dulberg,
Dubey & Cohen 2026), which we cite to *avoid* claiming the first such model; and
instrumental self-preservation as a contrast to affective fear-of-death (Omohundro
2008). **Open-ended worlds for AI** (§3.23): the games-for-AI-research survey (Hu,
Zhao, Wang, Du & Liu 2024) and MineRL (Guss et al. 2019). **Learned plasticity &
emotion architectures** (§3.24): the Baldwin effect (Baldwin 1896) and how learning
can guide evolution (Hinton & Nowlan 1987); evolved plastic neural networks
(Soltoggio, Stanley & Risi 2018); reservoir computing (Jaeger 2001; Maass, Natschläger
& Markram 2002); and appraisal-based emotion architectures (OCC, Ortony, Clore &
Collins 1988; EMA, Marsella & Gratch 2009; FAtiMA, Dias, Mascarenhas & Paiva 2014).
Full bibliographic detail in §10 (References) below.

---

## 8. Reproducibility

Deterministic by construction (one seeded SplitMix64 PRNG; no wall-clock, no
threads in cognition). Reproduce the entire evaluation:

```bash
cargo run -p daimon-game --example believability --release   # all 46 criteria
cargo run -p daimon-game --example proofs        --release   # the 9 machine-checked theorems (§4.5; PROOFS.md)
cargo run -p daimon-game --example autogenesis   --release   # the self-improvement loop
cargo run -p daimon-game --example benchmark     --release   # evolvability/perf/zero-shot (§5.1)
cargo run -p daimon-game --example overlay_ab    --release   # System-2 A/B: instinct vs learned overlay (§5.2)
cargo run -p daimon-game --example overlay_evolve --release  # evolution chooses for/against the overlay (§5.2)
cargo run -p daimon-game --example evolve_frontier --release # frontier evolution: weak minds → mastery, held-out (§5.3)
cargo run -p daimon-game --example poet          --release   # POET vs direct EA at equal budget — honest null (§5.3)
cargo run -p daimon-game --example study         --release   # render-free behavioural field study
cargo test                                                   # 90 unit tests
cargo run -p daimon-game --release                           # watch the village (3-D isometric)
```

Every numeric claim in this report is emitted by one of these commands; the
`proofs` and `believability` harnesses exit non-zero on any regression, so the
report's claims are continuously re-verifiable rather than asserted once. The
architecture, mechanisms, and evaluation harness are original work (clean-room
provenance in `PROVENANCE.md`); the intellectual lineage is the public literature
cited in §10.

---

## 9. Conclusion

We set out to make a game agent that **authors its own ontology, goals, world
model, and values** — and to make that claim *decidable* rather than rhetorical.
Daimon composes its mechanisms on a deterministic, dual-process BDI spine — and now
also gives the agent a felt need for shelter, an awareness of its own mortality,
grief over a bonded peer, an open-ended seasonal world to provision against, and a
learned neural overlay we evaluate honestly (and find does not beat tuned instinct);
a forty-six-criterion ablation harness turns "the NPC feels alive" into a battery
of falsifiable, reproducible tests; an autogenesis loop makes that harness its own
fitness function and improves the architecture with no human in the inner loop,
reaching a pre-registered end-goal target that is *earned* (a reactive policy
fails) and *held-out-validated*; and nine theorems — determinism, homeostatic
boundedness and Lyapunov stability, evolutionary elitism and convergence, the
Bell–CHSH/Tsirelson bounds, self-organised criticality, and reciprocity
non-exploitation — are **proved and machine-checked against the implementation**.
The result is, to our knowledge, the first game-AI architecture that is at once
novel, autonomous, self-evolving, *and* mathematically proved on the code that
runs. It is not general intelligence (§6, §6.1); it is a small, honest, and
reproducible step toward agents that are minds rather than puppets — and, just as
importantly, a template for proving and measuring such claims rather than
asserting them.

---

## 10. References

Aerts, D., & Sozzo, S. (2011). Quantum structure in cognition: Why and how concepts are entangled. *Quantum Interaction (QI 2011)*, LNCS 7052, 116–127. Springer. arXiv:1104.3344

Altera.AL (2024). *Project Sid: Many-agent simulations toward AI civilization.* arXiv:2411.00114

Anderson, J. R., Bothell, D., Byrne, M. D., Douglass, S., Lebiere, C., & Qin, Y. (2004). An integrated theory of the mind. *Psychological Review*, 111(4), 1036–1060. https://doi.org/10.1037/0033-295X.111.4.1036

Axelrod, R., & Hamilton, W. D. (1981). The evolution of cooperation. *Science*, 211(4489), 1390–1396. https://doi.org/10.1126/science.7466396

Axelrod, R. (1984). *The Evolution of Cooperation.* Basic Books.

Bak, P., Tang, C., & Wiesenfeld, K. (1987). Self-organized criticality: An explanation of 1/f noise. *Physical Review Letters*, 59(4), 381–384. https://doi.org/10.1103/PhysRevLett.59.381

Baldwin, J. M. (1896). A new factor in evolution. *The American Naturalist*, 30(354), 441–451. https://doi.org/10.1086/276408

Baranes, A., & Oudeyer, P-Y. (2013). Active learning of inverse models with intrinsically motivated goal exploration in robots. *Robotics and Autonomous Systems*, 61(1), 49–73. arXiv:1301.4862

Beggs, J. M., & Plenz, D. (2003). Neuronal avalanches in neocortical circuits. *Journal of Neuroscience*, 23(35), 11167–11177. https://doi.org/10.1523/JNEUROSCI.23-35-11167.2003

Bell, J. S. (1964). On the Einstein Podolsky Rosen paradox. *Physics Physique Fizika*, 1(3), 195–200. https://doi.org/10.1103/PhysicsPhysiqueFizika.1.195

Bengio, Y., Louradour, J., Collobert, R., & Weston, J. (2009). Curriculum learning. *Proceedings of the 26th International Conference on Machine Learning (ICML '09)*, 41–48. https://doi.org/10.1145/1553374.1553380

Booch, G., Fabiano, F., Horesh, L., Kate, K., Lenchner, J., Linck, N., et al. (2021). Thinking fast and slow in AI. *AAAI*. arXiv:2010.06002

Bowlby, J. (1969). *Attachment and Loss, Vol. 1: Attachment.* Basic Books.

Boyd, R., & Richerson, P. J. (1985). *Culture and the Evolutionary Process.* University of Chicago Press.

Bratman, M. E. (1987). *Intention, Plans, and Practical Reason.* Harvard University Press.

Browne, C. B., Powley, E., Whitehouse, D., Lucas, S. M., Cowling, P. I., Rohlfshagen, P., et al. (2012). A survey of Monte Carlo tree search methods. *IEEE Transactions on Computational Intelligence and AI in Games*, 4(1), 1–43. https://doi.org/10.1109/TCIAIG.2012.2186810

Bruza, P. D., Wang, Z., & Busemeyer, J. R. (2015). Quantum cognition: A new theoretical approach to psychology. *Trends in Cognitive Sciences*, 19(7), 383–393. https://doi.org/10.1016/j.tics.2015.05.001

Busemeyer, J. R., & Bruza, P. D. (2012). *Quantum Models of Cognition and Decision.* Cambridge University Press.

Busemeyer, J. R., & Wang, Z. (2015). What is quantum cognition, and how is it applied to psychology? *Current Directions in Psychological Science*, 24(3), 163–169. https://doi.org/10.1177/0963721414568663

Burke, B. L., Martens, A., & Faucher, E. H. (2010). Two decades of terror management theory: A meta-analysis of mortality salience research. *Personality and Social Psychology Review*, 14(2), 155–195. https://doi.org/10.1177/1088868309352321

Charnov, E. L. (1976). Optimal foraging, the marginal value theorem. *Theoretical Population Biology*, 9(2), 129–136. https://doi.org/10.1016/0040-5809(76)90040-X

Chialvo, D. R. (2010). Emergent complex neural dynamics. *Nature Physics*, 6, 744–750. https://doi.org/10.1038/nphys1803

Christakopoulou, K., Mourad, S., & Mataric, M. (2024). *Agents thinking fast and slow: A talker–reasoner architecture.* arXiv:2410.08328

Cirel'son (Tsirelson), B. S. (1980). Quantum generalizations of Bell's inequality. *Letters in Mathematical Physics*, 4(2), 93–100. https://doi.org/10.1007/BF00417500

Clauser, J. F., Horne, M. A., Shimony, A., & Holt, R. A. (1969). Proposed experiment to test local hidden-variable theories. *Physical Review Letters*, 23(15), 880–884. https://doi.org/10.1103/PhysRevLett.23.880

Clune, J. (2019). *AI-GAs: AI-generating algorithms, an alternate paradigm for producing general artificial intelligence.* arXiv:1905.10985

Colledanchise, M., & Ögren, P. (2018). *Behavior Trees in Robotics and AI: An Introduction.* CRC Press. arXiv:1709.00084

Constantino, S. M., & Daw, N. D. (2015). Learning the opportunity cost of time in a patch-foraging task. *Cognitive, Affective, & Behavioral Neuroscience*, 15(4), 837–853. https://doi.org/10.3758/s13415-015-0350-y

Cook, J., Lu, C., Hughes, E., Leibo, J. Z., & Foerster, J. (2024). *Artificial generational intelligence: Cultural accumulation in reinforcement learning.* arXiv:2406.00392

Deutsch, D., & Marletto, C. (2015). Constructor theory of information. *Proceedings of the Royal Society A*, 471(2174), 20140540. https://doi.org/10.1098/rspa.2014.0540

Dias, J., Mascarenhas, S., & Paiva, A. (2014). FAtiMA Modular: Towards an agent architecture with a generic appraisal framework. In *Emotion Modeling* (LNCS 8750, pp. 44–56). Springer. https://doi.org/10.1007/978-3-319-12973-0_3

Dulberg, Z., Dubey, R., & Cohen, J. D. (2025). Adapting to loss: A computational model of grief. *Psychological Review* (advance online publication, 26 May 2025). https://doi.org/10.1037/rev0000567

Dorigo, M., & Stützle, T. (2004). *Ant Colony Optimization.* MIT Press.

Erol, K., Hendler, J., & Nau, D. S. (1994). HTN planning: Complexity and expressivity. *AAAI-94*, 1123–1128.

Friston, K. (2010). The free-energy principle: A unified brain theory? *Nature Reviews Neuroscience*, 11(2), 127–138. https://doi.org/10.1038/nrn2787

Frijda, N. H. (1986). *The Emotions.* Cambridge University Press.

Grassé, P-P. (1959). La reconstruction du nid et les coordinations interindividuelles… La théorie de la stigmergie. *Insectes Sociaux*, 6, 41–80. https://doi.org/10.1007/BF02223791

Greenberg, J., Pyszczynski, T., & Solomon, S. (1986). The causes and consequences of a need for self-esteem: A terror management theory. In R. F. Baumeister (Ed.), *Public Self and Private Self* (pp. 189–212). Springer. https://doi.org/10.1007/978-1-4613-9564-5_10

Guss, W. H., Houghton, B., Topin, N., Wang, P., Codel, C., Veloso, M., & Salakhutdinov, R. (2019). MineRL: A large-scale dataset of Minecraft demonstrations. *IJCAI 2019*. arXiv:1907.13440

Hebb, D. O. (1949). *The Organization of Behavior: A Neuropsychological Theory.* Wiley.

Hinton, G. E., & Nowlan, S. J. (1987). How learning can guide evolution. *Complex Systems*, 1(3), 495–502.

Hu, C., Zhao, Y., Wang, Z., Du, H., & Liu, J. (2024). Games for artificial intelligence research: A review and perspectives. *IEEE Transactions on Artificial Intelligence*, 5(12), 5949–5968. arXiv:2304.13269 (IEEE Xplore doc. 10552162).

Jaeger, H. (2001). *The "echo state" approach to analysing and training recurrent neural networks* (GMD Report 148). German National Research Center for Information Technology (GMD).

Kahneman, D. (2011). *Thinking, Fast and Slow.* Farrar, Straus and Giroux.

Keramati, M., & Gutkin, B. (2014). Homeostatic reinforcement learning for integrating reward collection and physiological stability. *eLife*, 3, e04811. https://doi.org/10.7554/eLife.04811

Kinouchi, O., & Copelli, M. (2006). Optimal dynamical range of excitable networks at criticality. *Nature Physics*, 2, 348–351. https://doi.org/10.1038/nphys289

Klyubin, A. S., Polani, D., & Nehaniv, C. L. (2005). Empowerment: A universal agent-centric measure of control. *IEEE Congress on Evolutionary Computation*, 128–135. https://doi.org/10.1109/CEC.2005.1554676

Laird, J. E., Newell, A., & Rosenbloom, P. S. (1987). Soar: An architecture for general intelligence. *Artificial Intelligence*, 33(1), 1–64. https://doi.org/10.1016/0004-3702(87)90050-6

Lehman, J., & Stanley, K. O. (2011). Abandoning objectives: Evolution through the search for novelty alone. *Evolutionary Computation*, 19(2), 189–223. https://doi.org/10.1162/EVCO_a_00025

Lehman, J., Clune, J., Misevic, D., Adami, C., et al. (2020). The surprising creativity of digital evolution: A collection of anecdotes from the evolutionary computation and artificial life research communities. *Artificial Life*, 26(2), 274–306. https://doi.org/10.1162/artl_a_00319

Maass, W., Natschläger, T., & Markram, H. (2002). Real-time computing without stable states: A new framework for neural computation based on perturbations. *Neural Computation*, 14(11), 2531–2560. https://doi.org/10.1162/089976602760407955

Mangel, M., & Clark, C. W. (1986). Towards a unified foraging theory. *Ecology*, 67(5), 1127–1138. https://doi.org/10.2307/1938669

Marsella, S. C., & Gratch, J. (2009). EMA: A process model of appraisal dynamics. *Cognitive Systems Research*, 10(1), 70–90. https://doi.org/10.1016/j.cogsys.2008.03.005

McNamara, J. M., & Houston, A. I. (1986). The common currency for behavioral decisions. *The American Naturalist*, 127(3), 358–378. https://doi.org/10.1086/284489

Mouret, J-B., & Clune, J. (2015). *Illuminating search spaces by mapping elites.* arXiv:1504.04909

Nowak, M. A., & Sigmund, K. (1998). Evolution of indirect reciprocity by image scoring. *Nature*, 393, 573–577. https://doi.org/10.1038/31225

Omohundro, S. M. (2008). The basic AI drives. In *Proceedings of the First Conference on Artificial General Intelligence (AGI-08)*, Frontiers in Artificial Intelligence and Applications, vol. 171, 483–492. IOS Press.

Orkin, J. (2006). Three states and a plan: The AI of F.E.A.R. *Game Developers Conference (GDC) 2006*.

Ortony, A., Clore, G. L., & Collins, A. (1988). *The Cognitive Structure of Emotions.* Cambridge University Press. https://doi.org/10.1017/CBO9780511571299

Oudeyer, P-Y., & Kaplan, F. (2007). What is intrinsic motivation? A typology of computational approaches. *Frontiers in Neurorobotics*, 1, 6. https://doi.org/10.3389/neuro.12.006.2007

Oudeyer, P-Y., Kaplan, F., & Hafner, V. V. (2007). Intrinsic motivation systems for autonomous mental development. *IEEE Transactions on Evolutionary Computation*, 11(2), 265–286. https://doi.org/10.1109/TEVC.2006.890271

Park, J. S., O'Brien, J. C., Cai, C. J., Morris, M. R., Liang, P., & Bernstein, M. S. (2023). Generative agents: Interactive simulacra of human behavior. *UIST '23*. arXiv:2304.03442

Pathak, D., Agrawal, P., Efros, A. A., & Darrell, T. (2017). Curiosity-driven exploration by self-supervised prediction. *ICML*. arXiv:1705.05363

Pothos, E. M., & Busemeyer, J. R. (2009). A quantum probability explanation for violations of "rational" decision theory. *Proceedings of the Royal Society B*, 276(1665), 2171–2178. https://doi.org/10.1098/rspb.2009.0121

Rabinowitz, N. C., Perbet, F., Song, H. F., Zhang, C., Eslami, S. M. A., & Botvinick, M. (2018). Machine theory of mind. *ICML*. arXiv:1802.07740

Rao, A. S., & Georgeff, M. P. (1995). BDI agents: From theory to practice. *Proceedings of the First International Conference on Multi-Agent Systems (ICMAS-95)*, 312–319.

Rechenberg, I. (1973). *Evolutionsstrategie: Optimierung technischer Systeme nach Prinzipien der biologischen Evolution.* Frommann-Holzboog.

Rosenthal, R. W. (1973). A class of games possessing pure-strategy Nash equilibria. *International Journal of Game Theory*, 2, 65–67. https://doi.org/10.1007/BF01737559

Rovelli, C. (1996). Relational quantum mechanics. *International Journal of Theoretical Physics*, 35(8), 1637–1678. https://doi.org/10.1007/BF02302261

Rudolph, G. (1994). Convergence analysis of canonical genetic algorithms. *IEEE Transactions on Neural Networks*, 5(1), 96–101. https://doi.org/10.1109/72.265964

Russell, J. A. (1980). A circumplex model of affect. *Journal of Personality and Social Psychology*, 39(6), 1161–1178. https://doi.org/10.1037/h0077714

Salge, C., Glackin, C., & Polani, D. (2014). Empowerment — An introduction. In *Guided Self-Organization: Inception* (pp. 67–114). Springer. arXiv:1310.1863

Scherer, K. R. (2001). Appraisal considered as a process of multilevel sequential checking. In K. R. Scherer, A. Schorr, & T. Johnstone (Eds.), *Appraisal Processes in Emotion: Theory, Methods, Research* (pp. 92–120). Oxford University Press.

Schmidhuber, J. (2010). Formal theory of creativity, fun, and intrinsic motivation (1990–2010). *IEEE Transactions on Autonomous Mental Development*, 2(3), 230–247. https://doi.org/10.1109/TAMD.2010.2056368

Schrittwieser, J., Antonoglou, I., Hubert, T., Simonyan, K., Sifre, L., Schmitt, S., et al. (2020). Mastering Atari, Go, chess and shogi by planning with a learned model. *Nature*, 588, 604–609. arXiv:1911.08265

Shinn, N., Cassano, F., Berman, E., Gopinath, A., Narasimhan, K., & Yao, S. (2023). Reflexion: Language agents with verbal reinforcement learning. *NeurIPS*. arXiv:2303.11366

SIMA Team, DeepMind (2024). *Scaling instructable agents across many simulated worlds.* arXiv:2404.10179

Soltoggio, A., Stanley, K. O., & Risi, S. (2018). Born to learn: The inspiration, progress, and future of evolved plastic artificial neural networks. *Neural Networks*, 108, 48–67. arXiv:1703.10371. https://doi.org/10.1016/j.neunet.2018.07.013

Steele, G. L., Lea, D., & Flood, C. H. (2014). Fast splittable pseudorandom number generators. *OOPSLA '14*, 453–472. https://doi.org/10.1145/2660193.2660195

Stroebe, M., & Schut, H. (1999). The dual process model of coping with bereavement: Rationale and description. *Death Studies*, 23(3), 197–224. https://doi.org/10.1080/074811899201046

Trivers, R. L. (1971). The evolution of reciprocal altruism. *The Quarterly Review of Biology*, 46(1), 35–57. https://doi.org/10.1086/406755

Wang, G., Xie, Y., Jiang, Y., Mandlekar, A., Xiao, C., Zhu, Y., Fan, L., & Anandkumar, A. (2023). Voyager: An open-ended embodied agent with large language models. *Transactions on Machine Learning Research (TMLR)*. arXiv:2305.16291

Wang, R., Lehman, J., Clune, J., & Stanley, K. O. (2019). *Paired Open-Ended Trailblazer (POET): Endlessly generating increasingly complex and diverse learning environments and their solutions.* arXiv:1901.01753

Wang, Z., Solloway, T., Shiffrin, R. M., & Busemeyer, J. R. (2014). Context effects produced by question orders reveal quantum nature of human judgments. *PNAS*, 111(26), 9431–9436. https://doi.org/10.1073/pnas.1407756111

Wheeler, J. A. (1990). Information, physics, quantum: The search for links. In W. Zurek (Ed.), *Complexity, Entropy, and the Physics of Information.* Addison-Wesley.

Yao, S., Zhao, J., Yu, D., Du, N., Shafran, I., Narasimhan, K., & Cao, Y. (2023). ReAct: Synergizing reasoning and acting in language models. *ICLR*. arXiv:2210.03629

Yao, S., Yu, D., Zhao, J., Shafran, I., Griffiths, T. L., Cao, Y., & Narasimhan, K. (2023). Tree of thoughts: Deliberate problem solving with large language models. *NeurIPS*. arXiv:2305.10601

---

## Acknowledgements & disclosure

The Daimon architecture, the evaluation harness, the proofs, and this report were
developed by the author using AI pair-programming (Anthropic's Claude) as a coding
and drafting tool; all design decisions, claims, and the honest-reporting standard
are the author's responsibility, and every quantitative claim is reproducible from
the cited artifacts (§8). Clean-room provenance is recorded in `PROVENANCE.md`.
Citations (§10) are the public intellectual lineage; each is a real, verifiable
work, and foundational sources were added alongside system-specific ones where it
strengthened attribution. Every inline citation resolves to a reference and every
reference is cited.

*© 2026 David Borgenvik. Licensed MIT.*
