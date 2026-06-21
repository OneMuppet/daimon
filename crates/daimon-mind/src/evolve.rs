//! Autogenesis — a self-learning, self-iterating improvement loop.
//!
//! Every milestone so far was optimised *by hand*: a human adds a mechanism,
//! runs the believability harness, reads the verdict, decides the next move.
//! This module closes that loop. It makes the harness the **fitness function**
//! of an evolutionary search over an agent's cognitive **genome**, so the system
//! improves *itself*: it proposes variants, evaluates them against the real
//! arbiter, keeps what wins, **learns which genes matter**, and iterates until it
//! either reaches a measurable target or can no longer improve — reporting which.
//!
//! Three things make this *self-learning*, not blind random search:
//!
//! * **Self-adapting mutation** (Rechenberg's 1/5th-success rule): the step size
//!   grows when variation is paying off and shrinks when it isn't.
//! * **Per-gene sensitivity**: each generation the loop correlates every gene
//!   with fitness across the population and mutates high-impact genes harder —
//!   it learns *which levers move believability* and leans on them.
//! * **Self-evaluation & honest halting**: the loop judges its own champion
//!   against an explicit target and against a plateau detector, and stops with a
//!   [`Verdict`] saying which — never a fixed loop count pretending to be done.
//!
//! The engine here is pure and generic over a fitness closure (so it is tested
//! in isolation); the *real* fitness — running a whole `GameWorld` and measuring
//! it — lives one layer up, in `daimon-game`, where the world physics lives.

use serde::{Deserialize, Serialize};

use daimon_core::Rng;

use crate::mind::{Mind, MindConfig};
use crate::persona::Persona;

/// Number of genes in the cognitive genome.
pub const N_GENES: usize = 35;

/// A cognitive genome: a point in the architecture's tunable space, stored as
/// `N_GENES` normalised genes in `[0,1]` and decoded into real cognitive knobs.
///
/// Genes: `0` surprise threshold · `1` deliberation cooldown · `2` tie margin ·
/// `3` reflect interval · `4` plan staleness · `5..8` persona deltas
/// (boldness / sociability / curiosity) · `8..13` faculty switches
/// (empowerment, consolidation, imagination, meta-motivation, quantum) ·
/// `13` anticipatory-homeostasis foresight (lead ticks) · `14` DRR foraging
/// (drive-reduction-rate goal-directed foraging under survival risk) · `15`
/// commons-aware (contention-yielding/dispersing) foraging · `16..29` cultural /
/// stigmergy / affect / can-fight / can-build / can-die / can-grieve /
/// can-provision / nn-overlay (×3) / herd-evasion · `29..33` life-cycle
/// (can-mate / can-reproduce / can-age / feel-happiness) · `33` village-affinity
/// (Sprint 4 society: feel a settlement identity — drawn to same-village kin,
/// wary of enemy-village minds).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Genome {
    #[serde(with = "genes_serde")]
    pub g: [f32; N_GENES],
}

/// Serde for a fixed `[f32; N_GENES]` array. `serde`'s derive only covers arrays
/// up to length 32, and the genome now has 33 genes, so we (de)serialise it as a
/// length-checked sequence — semantically identical, just hand-rolled past the
/// derive's ceiling. (The on-the-wire form is a plain JSON array of numbers.)
mod genes_serde {
    use super::N_GENES;
    use serde::de::{Deserializer, Error as _, SeqAccess, Visitor};
    use serde::ser::{SerializeTuple, Serializer};
    use std::fmt;

    pub fn serialize<S: Serializer>(g: &[f32; N_GENES], s: S) -> Result<S::Ok, S::Error> {
        let mut t = s.serialize_tuple(N_GENES)?;
        for v in g {
            t.serialize_element(v)?;
        }
        t.end()
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[f32; N_GENES], D::Error> {
        struct GenesVisitor;
        impl<'de> Visitor<'de> for GenesVisitor {
            type Value = [f32; N_GENES];
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "an array of {N_GENES} f32 genes")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<[f32; N_GENES], A::Error> {
                let mut g = [0.0f32; N_GENES];
                for (i, slot) in g.iter_mut().enumerate() {
                    *slot = seq
                        .next_element()?
                        .ok_or_else(|| A::Error::invalid_length(i, &self))?;
                }
                Ok(g)
            }
        }
        d.deserialize_tuple(N_GENES, GenesVisitor)
    }
}

/// Linear decode of a normalised gene into `[lo, hi]`.
fn lerp(t: f32, lo: f32, hi: f32) -> f32 {
    lo + t.clamp(0.0, 1.0) * (hi - lo)
}

impl Genome {
    /// The hand-tuned architecture as a genome — the baseline every search must
    /// beat. Mirrors `MindConfig::default()` and the faculty defaults (all on
    /// except quantum), with zero persona delta.
    pub fn baseline() -> Genome {
        // gene values chosen so the decoders reproduce the current defaults.
        let mut g = [0.0f32; N_GENES];
        g[0] = inv_lerp(0.55, 0.2, 0.9); // surprise_threshold = 0.55
        g[1] = inv_lerp(8.0, 2.0, 20.0); // deliberation_cooldown = 8
        g[2] = inv_lerp(0.25, 0.05, 0.6); // tie_margin = 0.25
        g[3] = inv_lerp(25.0, 10.0, 60.0); // reflect_interval = 25
        g[4] = inv_lerp(6.0, 2.0, 15.0); // plan_staleness = 6
        g[5] = 0.5; // boldness delta 0
        g[6] = 0.5; // sociability delta 0
        g[7] = 0.5; // curiosity delta 0
        g[8] = 1.0; // empowerment on
        g[9] = 1.0; // consolidation on
        g[10] = 1.0; // imagination on
        g[11] = 1.0; // meta-motivation on
        g[12] = 0.0; // quantum off
        g[13] = 0.0; // foresight off (purely reactive — the incumbent behaviour)
        g[14] = 0.0; // DRR foraging off (greedy-nearest — the incumbent behaviour)
        g[15] = 0.0; // commons-aware foraging off (no contention yielding)
        g[16] = 0.0; // cultural transmission off (learn only from own contact)
        g[17] = 0.0; // learning-progress curiosity off (raw-novelty curiosity)
        g[18] = 0.0; // stigmergy off (no environmental pheromone)
        g[19] = 0.0; // affect modulation off (emotion tracked read-only)
        g[20] = 0.0; // can-fight off (flee-only — the incumbent behaviour)
        g[21] = 0.0; // can-build off (no shelter need / building — incumbent)
        g[22] = 0.0; // can-die off (immortal: health floors at 0.05 — incumbent)
        g[23] = 0.0; // can-grieve off (no bereavement modelling — incumbent)
        g[24] = 0.0; // can-provision off (no seasonal stockpiling — incumbent)
        g[25] = 0.0; // nn-overlay off (System-2 learned overlay disabled — incumbent)
        g[26] = 0.0; // nn learning-rate 0 (no in-life plasticity)
        g[27] = 0.0; // nn modulation 0 (overlay contributes zero bias)
        g[28] = 0.0; // herd-evasion off (flee-straight-away — the incumbent behaviour)
        // --- life-cycle genes (Sprint 3) — all default OFF in BOTH presets so every
        // existing AC/proof/fitness run stays byte-identical: a mind with these off
        // seeks no mate, draws no mating/aging RNG, and reports a flat happiness. The
        // live game flips them on by cloning showcase, and the *population* changes
        // (births/old-age deaths) live entirely behind the world's `lifecycle` flag.
        g[29] = 0.0; // can-mate off (no pair-bond seeking — incumbent solitary social)
        g[30] = 0.0; // can-reproduce off (a bonded pair never has children — incumbent)
        g[31] = 0.0; // can-age off (ageless: no senescence, no natural death — incumbent)
        g[32] = 0.0; // feel-happiness off (well-being not surfaced — incumbent)
        // --- society gene (Sprint 4) — default OFF in BOTH presets so every existing
        // AC/proof/fitness run stays byte-identical: a mind with it off feels no
        // settlement identity and never biases its movement toward/away from another
        // village. The whole inter-village SOCIETY (grouping, alliances, rivalries)
        // lives in the world behind its `society` flag, off the dedicated `soc_rng`;
        // the live game flips this gene on by cloning showcase.
        g[33] = 0.0; // village-affinity off (no settlement identity — incumbent solitary)
        // --- warfare gene (Civilization Sprint 2) — default OFF in BOTH presets so
        // every existing AC/proof/fitness run stays byte-identical: a mind with it off
        // is never mustered into a warband, draws no war RNG, and fields no warrior.
        // The whole WARFARE system (warbands, era weapons, battles, casualties,
        // truce) lives in the world behind its `war` flag, off the dedicated
        // `war_rng`; the live game flips this gene on for its village minds.
        g[34] = 0.0; // can-war off (never takes up arms — incumbent non-combatant)
        Genome { g }
    }

    /// A strong, believable preset for showcasing the architecture live: the
    /// baseline plus the two mechanisms the autogenesis loop proved load-bearing
    /// — anticipatory homeostasis (~25-tick foresight) and commons-aware foraging.
    /// This is the policy that reaches the end goal, so the village runs the
    /// *trained* behaviour rather than the untuned default.
    pub fn showcase() -> Genome {
        let mut g = Genome::baseline().g;
        g[13] = 0.55; // foresight ≈ 25 ticks (anticipatory homeostasis)
        g[15] = 1.0; // commons-aware foraging on
        g[16] = 1.0; // cumulative cultural transmission on
        g[17] = 1.0; // learning-progress curiosity on
        g[18] = 1.0; // stigmergy on (worn-path trails)
        g[19] = 1.0; // affect modulates behaviour (fear→caution, content→curiosity)
        g[20] = 1.0; // can-fight: the village has the *option* to confront the stalker
        Genome { g }
    }

    /// A uniformly random genome.
    pub fn random(rng: &mut Rng) -> Genome {
        let mut g = [0.0f32; N_GENES];
        for x in &mut g {
            *x = rng.next_f32();
        }
        Genome { g }
    }

    // ---- decoders: gene space -> cognitive knobs ----
    pub fn config(&self) -> MindConfig {
        MindConfig {
            surprise_threshold: lerp(self.g[0], 0.2, 0.9),
            deliberation_cooldown: lerp(self.g[1], 2.0, 20.0).round() as u64,
            tie_margin: lerp(self.g[2], 0.05, 0.6),
            reflect_interval: lerp(self.g[3], 10.0, 60.0).round() as u64,
            plan_staleness: lerp(self.g[4], 2.0, 15.0).round() as u64,
            can_build: self.can_build(),
            can_die: self.can_die(),
            can_grieve: self.can_grieve(),
            can_provision: self.can_provision(),
            can_mate: self.can_mate(),
            can_reproduce: self.can_reproduce(),
            can_age: self.can_age(),
            feel_happiness: self.feel_happiness(),
            village_affinity: self.village_affinity(),
            can_war: self.can_war(),
        }
    }
    /// Persona deltas in `[-0.3, 0.3]`, applied on top of a base character so the
    /// cast stays diverse while the *architecture* is what evolves.
    pub fn boldness_delta(&self) -> f32 {
        lerp(self.g[5], -0.3, 0.3)
    }
    pub fn sociability_delta(&self) -> f32 {
        lerp(self.g[6], -0.3, 0.3)
    }
    pub fn curiosity_delta(&self) -> f32 {
        lerp(self.g[7], -0.3, 0.3)
    }
    pub fn empowerment(&self) -> bool {
        self.g[8] >= 0.5
    }
    pub fn consolidation(&self) -> bool {
        self.g[9] >= 0.5
    }
    pub fn imagination(&self) -> bool {
        self.g[10] >= 0.5
    }
    pub fn metamotivation(&self) -> bool {
        self.g[11] >= 0.5
    }
    pub fn quantum(&self) -> bool {
        self.g[12] >= 0.5
    }
    /// Anticipatory-homeostasis lead time in ticks, decoded to `[0, 45]`.
    pub fn foresight(&self) -> f32 {
        lerp(self.g[13], 0.0, 45.0)
    }
    /// Whether goal-directed DRR foraging (vs greedy-nearest) is on.
    pub fn forage_drr(&self) -> bool {
        self.g[14] >= 0.5
    }
    /// Whether commons-aware (contention-yielding/dispersing) foraging is on.
    pub fn social_forage(&self) -> bool {
        self.g[15] >= 0.5
    }
    /// Whether cumulative cultural transmission (learning from peers) is on.
    pub fn cultural(&self) -> bool {
        self.g[16] >= 0.5
    }
    /// Whether curiosity is driven by learning progress (vs raw novelty).
    pub fn lp_curiosity(&self) -> bool {
        self.g[17] >= 0.5
    }
    /// Whether stigmergy (pheromone trails) is on.
    pub fn stigmergy(&self) -> bool {
        self.g[18] >= 0.5
    }
    /// Whether affect modulates behaviour (vs tracked read-only).
    pub fn affect_mod(&self) -> bool {
        self.g[19] >= 0.5
    }
    /// Whether the agent has the *option* to confront a threat (not flee-only).
    pub fn can_fight(&self) -> bool {
        self.g[20] >= 0.5
    }
    /// Whether the agent has the *option* to build (place walls to enclose itself
    /// for shelter). Off by default — nothing tells it to build a hut; given the
    /// affordance + a shelter need, walling-in must *emerge* from utility planning.
    pub fn can_build(&self) -> bool {
        self.g[21] >= 0.5
    }
    /// Whether the agent is *mortal*: health is no longer floored at 0.05, so a
    /// fully-depleted or fatally-mauled body dies for good (permadeath), and the
    /// mind feels a fear of death (mortality salience) as its health trajectory
    /// declines. Off by default — immortal worlds floor health and draw no death
    /// RNG, staying bit-identical to the incumbent.
    pub fn can_die(&self) -> bool {
        self.g[22] >= 0.5
    }
    /// Whether the agent *grieves*: the death of a bonded peer triggers mourning
    /// scaled by the bond, a continuing bond to the dead, and Dual-Process
    /// oscillation that decays over time (faster with friends near). Off by
    /// default — non-grieving worlds run no bereavement logic and stay identical.
    pub fn can_grieve(&self) -> bool {
        self.g[23] >= 0.5
    }
    /// Whether the agent *provisions*: in an open world it gathers a surplus of
    /// food while abundant and stores it in the village granary, drawing it down to
    /// survive winter. Off by default — a non-provisioning world adopts no Provision
    /// goal, resolves no Gather/Store, and stays bit-identical. Only does anything
    /// when the world's `open_world` flag is also on (no seasons otherwise).
    pub fn can_provision(&self) -> bool {
        self.g[24] >= 0.5
    }
    /// Whether the **System-2 learned overlay** is active. Off by default — a
    /// disabled overlay emits exactly zero bias and never learns, so the instinct
    /// (and the whole seeded harness) is byte-identical to the incumbent. When on,
    /// a tiny evolved-plastic net nudges the drive arbitration and adapts in-life.
    pub fn nn_enabled(&self) -> bool {
        self.g[25] >= 0.5
    }
    /// In-life Hebbian learning rate η, decoded to `[0, 0.15]`.
    pub fn nn_learn_rate(&self) -> f32 {
        lerp(self.g[26], 0.0, 0.15)
    }
    /// How strongly the overlay's outputs bias the drive pressures, in `[0, 0.6]`
    /// — a bounded *nudge*, never enough to fully hijack the instinct.
    pub fn nn_modulation(&self) -> f32 {
        lerp(self.g[27], 0.0, 0.6)
    }
    /// Whether **predator-aware coordination (selfish-herd / dispersal-evasion)** is
    /// on. Off by default — a disabled faculty leaves the flee path computing the
    /// exact straight-away-from-the-predator step it always did, so the instinct (and
    /// the whole seeded harness) is byte-identical to the incumbent. When on, a
    /// threatened mind composes fleeing the predator with an anti-isolation pull
    /// toward its local prey group (Hamilton 1971, "Geometry for the Selfish Herd";
    /// risk dilution), so it is not the lone straggler an isolated-target predator
    /// picks off.
    pub fn herd_evasion(&self) -> bool {
        self.g[28] >= 0.5
    }
    /// The cohesion strength of the herd-evasion pull, in `[0, 1]` — how heavily the
    /// "move toward the group" term weighs against the "move away from the predator"
    /// term when composing the evasive step. Evolvable: the search tunes how
    /// gregarious a threatened mind is. Decoded from the same gene as the switch, so
    /// a value just over the 0.5 threshold is a timid herder, 1.0 a tight one.
    pub fn herd_cohesion(&self) -> f32 {
        // map [0.5,1.0] → [0.2,1.0] so even a freshly-flipped gene gives a real pull.
        lerp((self.g[28] - 0.5).max(0.0) * 2.0, 0.2, 1.0)
    }
    /// Whether the mind *seeks a mate* — forms a lasting romantic PAIR-BOND with a
    /// chosen partner (distinct from, and stronger than, ordinary friendship) and
    /// prefers proximity to them. Off by default — a non-mating mind seeks no
    /// partner and the whole seeded harness stays byte-identical (the live
    /// pair-bond registry lives in the world behind its `lifecycle` flag).
    pub fn can_mate(&self) -> bool {
        self.g[29] >= 0.5
    }
    /// Whether the mind *reproduces* — a settled, fed, sheltered bonded pair may
    /// have a CHILD whose genome + persona are inherited from both parents. Off by
    /// default; population growth happens only when the world's `lifecycle` flag is
    /// also on (the harness has a fixed population and never reproduces).
    pub fn can_reproduce(&self) -> bool {
        self.g[30] >= 0.5
    }
    /// Whether the mind *ages* — it accrues years and, past a lifespan, dies a
    /// peaceful NATURAL death (distinct from predator/starvation), grievable by
    /// family. Off by default — an ageless mind never senesces and draws no aging
    /// RNG, so the harness stays byte-identical (the age clock + natural death live
    /// in the world behind its `lifecycle` flag).
    pub fn can_age(&self) -> bool {
        self.g[31] >= 0.5
    }
    /// Whether the mind *surfaces happiness* — exposes its felt contentment
    /// (well-being: bonds/family/safety/fullness up, hunger/threat/grief down) as a
    /// readable signal. Off by default — `happiness()` then returns a flat neutral
    /// value and nothing about the seeded cycle changes (well-being is still
    /// computed read-only for the System-2 overlay regardless).
    pub fn feel_happiness(&self) -> bool {
        self.g[32] >= 0.5
    }
    /// Whether the mind *feels a settlement identity* — it belongs to a VILLAGE and
    /// (Sprint 4 society) is drawn toward its own village's members and grows wary of
    /// minds from an ENEMY village. Off by default — a mind with no village-affinity
    /// senses no factions, biases its movement for none, and the whole seeded harness
    /// stays byte-identical (the inter-village relation registry lives in the world
    /// behind its `society` flag). The live game flips it on by cloning showcase.
    pub fn village_affinity(&self) -> bool {
        self.g[33] >= 0.5
    }

    /// Whether the mind will *bear arms for its village in war* (Sprint 2 weapons &
    /// war) — it can be mustered into a warband, march to the border and fight an
    /// enemy village's combatants. Off by default — a mind with it off is never
    /// conscripted, draws no war RNG, and the seeded harness fields no warriors and
    /// stays byte-identical (the warfare registry lives in the world behind its `war`
    /// flag). The live game flips it on for its village minds.
    pub fn can_war(&self) -> bool {
        self.g[34] >= 0.5
    }

    /// Express this genome as a live [`Mind`], applying the persona deltas on top
    /// of a base character (preserving its identity and diversity).
    pub fn express(&self, base: &Persona, seed: u64) -> Mind {
        let clamp01 = |v: f32| v.clamp(0.0, 1.0);
        let persona = base
            .clone()
            .with_boldness(clamp01(base.boldness + self.boldness_delta()))
            .with_sociability(clamp01(base.sociability + self.sociability_delta()))
            .with_curiosity(clamp01(base.curiosity + self.curiosity_delta()));
        let mut mind = Mind::with(
            persona,
            seed,
            Box::new(crate::deliberate::HeuristicDeliberator::default()),
            self.config(),
        );
        mind.set_empowerment(self.empowerment());
        mind.set_consolidation(self.consolidation());
        mind.set_imagination(self.imagination());
        mind.set_metamotivation(self.metamotivation());
        mind.set_quantum(self.quantum());
        mind.set_foresight(self.foresight());
        mind.set_forage_drr(self.forage_drr());
        mind.set_social_forage(self.social_forage());
        mind.set_cultural(self.cultural());
        mind.set_lp_curiosity(self.lp_curiosity());
        mind.set_stigmergy(self.stigmergy());
        mind.set_affect_mod(self.affect_mod());
        mind.set_can_fight(self.can_fight());
        mind.set_can_build(self.can_build());
        mind.set_can_die(self.can_die());
        mind.set_can_grieve(self.can_grieve());
        mind.set_can_provision(self.can_provision());
        mind.set_can_mate(self.can_mate());
        mind.set_can_reproduce(self.can_reproduce());
        mind.set_can_age(self.can_age());
        mind.set_feel_happiness(self.feel_happiness());
        mind.set_can_war(self.can_war());
        mind.install_overlay(
            self.nn_enabled(),
            seed,
            self.nn_learn_rate(),
            self.nn_modulation(),
        );
        mind.set_herd_evasion(self.herd_evasion(), self.herd_cohesion());
        mind
    }

    /// **Inheritance**: a child genome from two parents — uniform crossover (each
    /// gene taken from one parent or the other by a coin flip) plus a small Gaussian
    /// mutation per gene, exactly the spirit of the evolution `mutate` (reflected
    /// into `[0,1]`). Deterministic on the supplied seeded `rng`. This is how a
    /// village's lineage drifts: children resemble both parents, with novelty.
    pub fn inherit(a: &Genome, b: &Genome, sigma: f32, rng: &mut Rng) -> Genome {
        let mut g = [0.0f32; N_GENES];
        for (i, slot) in g.iter_mut().enumerate() {
            // uniform crossover: pick the gene from one parent or the other.
            let picked = if rng.next_f32() < 0.5 { a.g[i] } else { b.g[i] };
            // small inheritance mutation (smaller than search mutation — a lineage
            // drifts, it does not scatter).
            let step = sigma * gaussian(rng);
            *slot = reflect01(picked + step);
        }
        Genome { g }
    }

    /// Mutate, scaling each gene's Gaussian step by a per-gene `gain` (learned
    /// sensitivity) and a global `sigma`. Bounds are handled by reflection.
    pub fn mutate(&self, sigma: f32, gain: &[f32; N_GENES], rng: &mut Rng) -> Genome {
        let mut g = self.g;
        for (i, x) in g.iter_mut().enumerate() {
            let step = sigma * (0.3 + 0.7 * gain[i]) * gaussian(rng);
            *x = reflect01(*x + step);
        }
        Genome { g }
    }
}

/// Inverse of [`lerp`] — encode a real knob value back into gene space.
fn inv_lerp(v: f32, lo: f32, hi: f32) -> f32 {
    ((v - lo) / (hi - lo)).clamp(0.0, 1.0)
}

/// A standard-normal sample via Box–Muller (deterministic on the seeded `Rng`).
fn gaussian(rng: &mut Rng) -> f32 {
    let u1 = rng.next_f32().max(1e-6);
    let u2 = rng.next_f32();
    (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
}

/// Reflect a value back into `[0,1]` (so mutation never escapes gene space).
fn reflect01(mut x: f32) -> f32 {
    for _ in 0..4 {
        if x < 0.0 {
            x = -x;
        } else if x > 1.0 {
            x = 2.0 - x;
        } else {
            break;
        }
    }
    x.clamp(0.0, 1.0)
}

/// A multi-objective believability score. Every component is in `[0,1]`, higher
/// is better, and each has genuine headroom — there is no way to max one without
/// paying in another (safety vs. exploration is a real trade-off), so the search
/// faces a true landscape, not a checkbox.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Fitness {
    /// Keeps its needs met — little time in critical starvation/thirst.
    pub survival: f32,
    /// Avoids harm — low predator-damage exposure.
    pub safety: f32,
    /// Decides in a balanced way — high entropy over which drive leads, no
    /// fixation on one motive.
    pub balance: f32,
    /// Expresses itself — varied, non-repetitive dialogue and social contact.
    pub expression: f32,
    /// Explores — covers ground, discovers, invents (open-ended behaviour).
    pub exploration: f32,
    /// Feels — a responsive, varied emotional life (not flat), tracking the
    /// situation. The believability dimension the affect layer serves; added in
    /// iteration 12 after the mechanism audit found the metric was blind to it.
    #[serde(default)]
    pub emotion: f32,
    /// Knows — learned competence (forward-model accuracy) and breadth of
    /// understood affordances. The learning/social dimension that lp-curiosity,
    /// cultural transmission, and consolidation serve (added iteration 13).
    #[serde(default)]
    pub knowledge: f32,
}

/// Default objective weights — the operational definition of "a believable life":
/// survive and stay safe, but also explore, speak with variety, choose in a
/// balanced way, and *feel*. (Sums to 1.0.)
pub const WEIGHTS: Fitness = Fitness {
    survival: 0.26,
    safety: 0.16,
    balance: 0.12,
    expression: 0.12,
    exploration: 0.14,
    emotion: 0.10,
    knowledge: 0.10,
};

impl Fitness {
    /// Weighted scalar used for selection.
    pub fn scalar(&self) -> f32 {
        self.survival * WEIGHTS.survival
            + self.safety * WEIGHTS.safety
            + self.balance * WEIGHTS.balance
            + self.expression * WEIGHTS.expression
            + self.exploration * WEIGHTS.exploration
            + self.emotion * WEIGHTS.emotion
            + self.knowledge * WEIGHTS.knowledge
    }

    /// The articulated end-goal target: every facet of a believable life clears a
    /// demanding bar *at once*. Reaching this is the loop's success condition.
    pub fn meets_target(&self) -> bool {
        self.survival >= 0.85
            && self.safety >= 0.80
            && self.balance >= 0.55
            && self.expression >= 0.55
            && self.exploration >= 0.45
            && self.emotion >= 0.45
            && self.knowledge >= 0.45
            && self.scalar() >= 0.72
    }
}

/// How a self-improvement run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    /// The champion met the end-goal target on every facet. Done, honestly.
    ReachedTarget,
    /// No meaningful improvement for `patience` generations — converged below
    /// target. The honest "this is as far as this design reaches" outcome.
    Converged,
    /// Generation budget exhausted while still improving.
    Budget,
}

/// One generation's record, for the trajectory log.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct GenRecord {
    pub generation: u32,
    pub best_scalar: f32,
    pub mean_scalar: f32,
    pub sigma: f32,
}

/// The self-improvement engine. Generic over a fitness closure so it is tested in
/// isolation; the real fitness runs a `GameWorld`.
pub struct Evolution {
    pop: Vec<Genome>,
    fit: Vec<Fitness>,
    sigma: f32,
    rng: Rng,
    /// Learned per-gene sensitivity in `[0,1]` — how strongly each gene tracks
    /// fitness. Mutation leans on the high-impact genes.
    pub gain: [f32; N_GENES],
    pub best: Genome,
    pub best_fit: Fitness,
    pub history: Vec<GenRecord>,
    stall: u32,
    patience: u32,
}

impl Evolution {
    /// Seed a population (baseline + random variants) and score it.
    pub fn new(seed: u64, pop_size: usize, eval: &impl Fn(&Genome) -> Fitness) -> Evolution {
        let mut rng = Rng::new(seed);
        let mut pop = Vec::with_capacity(pop_size);
        pop.push(Genome::baseline()); // always carry the incumbent
        while pop.len() < pop_size.max(2) {
            pop.push(Genome::random(&mut rng));
        }
        let fit: Vec<Fitness> = pop.iter().map(eval).collect();
        let (bi, bf) = best_of(&fit);
        Evolution {
            best: pop[bi].clone(),
            best_fit: bf,
            pop,
            fit,
            sigma: 0.18,
            rng,
            gain: [0.5; N_GENES],
            history: Vec::new(),
            stall: 0,
            patience: 4,
        }
    }

    /// The current population's fitnesses (for tracing/visualisation).
    pub fn fitnesses(&self) -> &[Fitness] {
        &self.fit
    }

    /// Update the learned per-gene sensitivities from the current population:
    /// |correlation(gene, fitness)|, smoothed. This is the loop *learning* which
    /// knobs move believability.
    fn learn_sensitivities(&mut self) {
        let n = self.pop.len() as f32;
        let scalars: Vec<f32> = self.fit.iter().map(|f| f.scalar()).collect();
        let fmean = scalars.iter().sum::<f32>() / n;
        let fss: f32 = scalars.iter().map(|s| (s - fmean).powi(2)).sum();
        for i in 0..N_GENES {
            let gmean = self.pop.iter().map(|p| p.g[i]).sum::<f32>() / n;
            let mut cov = 0.0;
            let mut gss = 0.0;
            for (p, s) in self.pop.iter().zip(&scalars) {
                cov += (p.g[i] - gmean) * (s - fmean);
                gss += (p.g[i] - gmean).powi(2);
            }
            // Pearson |r| = |cov| / sqrt(Σdg² · Σdf²).
            let denom = (gss * fss).sqrt();
            let corr = if denom > 1e-6 { (cov / denom).abs() } else { 0.0 };
            // exponential smoothing so sensitivity is *learned* across generations.
            self.gain[i] = 0.6 * self.gain[i] + 0.4 * corr.clamp(0.0, 1.0);
        }
    }

    /// Advance one generation: learn sensitivities, breed (elitism + sensitivity-
    /// weighted mutation), self-adapt `sigma` by the 1/5th-success rule, and log.
    pub fn step(&mut self, eval: &impl Fn(&Genome) -> Fitness) {
        self.learn_sensitivities();

        // rank by scalar fitness.
        let mut order: Vec<usize> = (0..self.pop.len()).collect();
        order.sort_by(|&a, &b| self.fit[b].scalar().total_cmp(&self.fit[a].scalar()));
        let elite = (self.pop.len() / 4).max(1);
        // parents carry their already-known fitness — no re-evaluation.
        let parents: Vec<(Genome, f32)> = order[..(self.pop.len() / 2).max(1)]
            .iter()
            .map(|&i| (self.pop[i].clone(), self.fit[i].scalar()))
            .collect();

        // build the next population: keep elites (fitness known), fill with
        // mutated children (scored once, fitness carried forward — no recompute).
        let mut next: Vec<Genome> = Vec::with_capacity(self.pop.len());
        let mut next_fit: Vec<Fitness> = Vec::with_capacity(self.pop.len());
        for &i in &order[..elite] {
            next.push(self.pop[i].clone());
            next_fit.push(self.fit[i]);
        }
        let mut improved = 0u32;
        let mut attempts = 0u32;
        while next.len() < self.pop.len() {
            let pi = self.rng.below(parents.len());
            let (parent, parent_scalar) = &parents[pi];
            let child = parent.mutate(self.sigma, &self.gain, &mut self.rng);
            let cf = eval(&child);
            attempts += 1;
            if cf.scalar() > *parent_scalar {
                improved += 1;
            }
            next.push(child);
            next_fit.push(cf);
        }

        // 1/5th-success rule: expand when variation pays, contract when it doesn't.
        if attempts > 0 {
            let rate = improved as f32 / attempts as f32;
            self.sigma *= if rate > 0.2 { 1.22 } else { 0.82 };
            self.sigma = self.sigma.clamp(0.02, 0.5);
        }

        self.pop = next;
        self.fit = next_fit;
        let (bi, bf) = best_of(&self.fit);
        let gen_best = bf.scalar();
        let improved_best = gen_best > self.best_fit.scalar() + 1e-4;
        if improved_best {
            self.best = self.pop[bi].clone();
            self.best_fit = bf;
            self.stall = 0;
        } else {
            self.stall += 1;
        }
        let mean = self.fit.iter().map(|f| f.scalar()).sum::<f32>() / self.fit.len() as f32;
        self.history.push(GenRecord {
            generation: self.history.len() as u32,
            best_scalar: self.best_fit.scalar(),
            mean_scalar: mean,
            sigma: self.sigma,
        });
    }

    /// Run until the target is met, the search converges, or the budget is spent.
    /// This is the self-evaluating, self-halting outer loop.
    pub fn run(&mut self, max_gens: u32, eval: &impl Fn(&Genome) -> Fitness) -> Verdict {
        for _ in 0..max_gens {
            if self.best_fit.meets_target() {
                return Verdict::ReachedTarget;
            }
            self.step(eval);
            if self.stall >= self.patience {
                return Verdict::Converged;
            }
        }
        if self.best_fit.meets_target() {
            Verdict::ReachedTarget
        } else {
            Verdict::Budget
        }
    }
}

fn best_of(fit: &[Fitness]) -> (usize, Fitness) {
    let mut bi = 0;
    for i in 1..fit.len() {
        if fit[i].scalar() > fit[bi].scalar() {
            bi = i;
        }
    }
    (bi, fit[bi])
}

#[cfg(test)]
mod tests {
    use super::*;

    // A synthetic fitness: closeness of each gene to a hidden optimum. Lets us
    // verify the engine *as a learner* without a whole game world.
    fn synthetic(target: [f32; N_GENES]) -> impl Fn(&Genome) -> Fitness {
        move |gnm: &Genome| {
            let err: f32 =
                gnm.g.iter().zip(target.iter()).map(|(a, b)| (a - b).powi(2)).sum::<f32>()
                    / N_GENES as f32;
            let s = (1.0 - err.sqrt()).clamp(0.0, 1.0);
            // spread the single score across components so scalar() ≈ s.
            Fitness {
                survival: s,
                safety: s,
                balance: s,
                expression: s,
                exploration: s,
                emotion: s,
                knowledge: s,
            }
        }
    }

    #[test]
    fn engine_improves_and_learns() {
        let target = [
            0.9, 0.1, 0.8, 0.2, 0.7, 0.3, 0.6, 0.4, 1.0, 0.0, 1.0, 0.0, 1.0, 0.5, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
            1.0, 0.5, 0.5, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ];
        let eval = synthetic(target);
        let mut evo = Evolution::new(0xA11CE, 16, &eval);
        let start = evo.best_fit.scalar();
        let _ = evo.run(40, &eval);
        let end = evo.best_fit.scalar();
        assert!(end > start + 0.1, "search improved {start:.3} -> {end:.3}");
        // best-so-far is monotonic non-decreasing (elitism never regresses).
        for w in evo.history.windows(2) {
            assert!(w[1].best_scalar >= w[0].best_scalar - 1e-5);
        }
    }

    #[test]
    fn self_halts_with_a_verdict() {
        // an easy target so the loop reaches it and stops on its own evaluation.
        let eval = |_g: &Genome| Fitness {
            survival: 0.95,
            safety: 0.95,
            balance: 0.95,
            expression: 0.95,
            exploration: 0.95,
            emotion: 0.95,
            knowledge: 0.95,
        };
        let mut evo = Evolution::new(0xBEE, 8, &eval);
        let v = evo.run(20, &eval);
        assert_eq!(v, Verdict::ReachedTarget);
    }

    #[test]
    fn baseline_decodes_to_defaults() {
        let g = Genome::baseline();
        let c = g.config();
        assert_eq!(c.deliberation_cooldown, 8);
        assert_eq!(c.reflect_interval, 25);
        assert!((c.surprise_threshold - 0.55).abs() < 0.02);
        assert!(g.empowerment() && g.imagination() && !g.quantum());
    }

    #[test]
    fn building_is_off_in_baseline_and_showcase() {
        // can_build must default OFF in BOTH presets so every existing AC, proof,
        // and fitness run stays bit-identical (no walls, no shelter need, no RNG).
        assert!(!Genome::baseline().can_build());
        assert!(!Genome::showcase().can_build());
    }

    #[test]
    fn mortality_and_grief_are_off_in_baseline_and_showcase() {
        // can_die and can_grieve must default OFF in BOTH presets so every existing
        // AC, proof, and fitness run stays bit-identical (no deaths, no death RNG,
        // no bereavement logic). The live game flips them on by cloning showcase.
        assert!(!Genome::baseline().can_die());
        assert!(!Genome::baseline().can_grieve());
        assert!(!Genome::showcase().can_die());
        assert!(!Genome::showcase().can_grieve());
        // and the config decode agrees.
        assert!(!Genome::baseline().config().can_die);
        assert!(!Genome::baseline().config().can_grieve);
    }

    #[test]
    fn herd_evasion_is_off_in_baseline_and_showcase() {
        // gene 28 (predator-aware coordination / selfish-herd) must default OFF in BOTH
        // presets so every existing AC, proof, and fitness run stays byte-identical: the
        // flee path then computes the exact incumbent straight-away step and draws no
        // RNG. The hell_coord experiment flips it on by pinning the gene.
        assert!(!Genome::baseline().herd_evasion());
        assert!(!Genome::showcase().herd_evasion());
        // and an OFF gene yields zero express-time cohesion intent (cohesion is only
        // consulted when the switch is on).
        assert!(Genome::baseline().g[28] < 0.5);
        assert!(Genome::showcase().g[28] < 0.5);
    }

    #[test]
    fn herd_cohesion_decodes_in_range_when_on() {
        // a freshly-flipped gene (just over 0.5) still gives a real pull, and 1.0 maxes.
        let mut g = Genome::baseline();
        g.g[28] = 0.5;
        assert!((g.herd_cohesion() - 0.2).abs() < 1e-5, "min cohesion at the threshold");
        g.g[28] = 1.0;
        assert!((g.herd_cohesion() - 1.0).abs() < 1e-5, "max cohesion at 1.0");
        assert!(g.herd_evasion());
    }

    #[test]
    fn provisioning_is_off_in_baseline_and_showcase() {
        // can_provision must default OFF in BOTH presets so every existing AC, proof,
        // and fitness run stays bit-identical (no seasons, no granary, no Provision
        // goal, no Gather/Store, no new RNG). The live game / --evolve flip it on by
        // cloning showcase and also turning on the world's open_world flag.
        assert!(!Genome::baseline().can_provision());
        assert!(!Genome::showcase().can_provision());
        assert!(!Genome::baseline().config().can_provision);
    }

    #[test]
    fn lifecycle_is_off_in_baseline_and_showcase() {
        // The four life-cycle genes (29 can-mate, 30 can-reproduce, 31 can-age,
        // 32 feel-happiness) must default OFF in BOTH presets so every existing AC,
        // proof, and fitness run stays byte-identical: a mind with these off seeks no
        // mate, draws no mating/aging RNG, never reproduces, and reports a flat
        // happiness. The live game flips them on by cloning showcase, and the
        // population only grows behind the world's `lifecycle` flag.
        for g in [Genome::baseline(), Genome::showcase()] {
            assert!(!g.can_mate());
            assert!(!g.can_reproduce());
            assert!(!g.can_age());
            assert!(!g.feel_happiness());
            assert!(g.g[29] < 0.5 && g.g[30] < 0.5 && g.g[31] < 0.5 && g.g[32] < 0.5);
            // and the config decode agrees.
            let c = g.config();
            assert!(!c.can_mate && !c.can_reproduce && !c.can_age && !c.feel_happiness);
        }
    }

    #[test]
    fn village_affinity_is_off_in_baseline_and_showcase() {
        // The society gene (33 village-affinity) must default OFF in BOTH presets so
        // every existing AC, proof, and fitness run stays byte-identical: a mind with
        // it off feels no settlement identity, biases no movement toward/away from any
        // village, and draws no society RNG. The live game flips it on by cloning
        // showcase, and all inter-village relations live behind the world's `society`
        // flag (off the dedicated society side-RNG).
        for g in [Genome::baseline(), Genome::showcase()] {
            assert!(!g.village_affinity());
            assert!(g.g[33] < 0.5);
        }
    }

    #[test]
    fn happiness_is_flat_until_gene_on() {
        // With feel_happiness off the readout is a fixed neutral, so no seeded run
        // can depend on it; flipping the gene on makes it report real well-being.
        let off = Genome::baseline().express(&Persona::new("Test"), 0x1);
        assert!((off.happiness() - 0.5).abs() < 1e-6);
        let mut hg = Genome::baseline();
        hg.g[32] = 1.0;
        let on = hg.express(&Persona::new("Test"), 0x1);
        assert!(on.feel_happiness());
    }
}
