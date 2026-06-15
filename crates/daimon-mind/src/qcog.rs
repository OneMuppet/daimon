//! Quantum cognition — decision-making by quantum *probability*.
//!
//! Human judgment violates classical (Kolmogorov) probability in lawful ways:
//! the order of two questions changes the answers (Wang & Busemeyer, PNAS 2014),
//! conjunctions can be judged *more* likely than their parts (Pothos & Busemeyer
//! 2009), and a person can be genuinely "of two minds." Classical Bayesian
//! agents — and every NPC built on them — *cannot* produce these. **Quantum
//! cognition** (Busemeyer & Bruza 2012) models them with the mathematics of
//! quantum probability: a belief is a unit vector of complex **amplitudes**;
//! considering something is a **unitary rotation**; deciding is a **projective
//! measurement** with Born-rule probabilities `|ψ_i|²`, which collapses the
//! state.
//!
//! Two consequences are *classically impossible* and we test for both:
//!
//! * **Non-commutativity / order effects.** Considerations are rotations in
//!   different planes; `U_A U_B ≠ U_B U_A`, so the order of deliberation changes
//!   the decision distribution. A classical reweighting commutes; this does not.
//! * **Interference / violation of the law of total probability.** Resolving an
//!   intermediate question (a measurement) changes the final answer, because the
//!   superposed state carries cross terms `2·Re(·)` a classical mixture lacks.
//!
//! **Scope, stated plainly.** This is quantum *probability theory as a model of
//! cognition* — a descriptive formalism that fits human data. It is **not** a
//! claim that the brain (or this program) is a physical quantum computer, and
//! **not** a claim about consciousness. The Hilbert space is simulated on an
//! ordinary CPU and the whole thing is deterministic given the measurement seed.

use serde::{Deserialize, Serialize};

/// A complex number (f64 for the small interference terms).
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct C {
    pub re: f64,
    pub im: f64,
}

#[allow(clippy::should_implement_trait)] // tiny inherent complex arithmetic, by design
impl C {
    pub const fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }
    pub fn polar(r: f64, theta: f64) -> Self {
        Self { re: r * theta.cos(), im: r * theta.sin() }
    }
    pub fn add(self, o: C) -> C {
        C::new(self.re + o.re, self.im + o.im)
    }
    pub fn sub(self, o: C) -> C {
        C::new(self.re - o.re, self.im - o.im)
    }
    pub fn mul(self, o: C) -> C {
        C::new(self.re * o.re - self.im * o.im, self.re * o.im + self.im * o.re)
    }
    pub fn scale(self, s: f64) -> C {
        C::new(self.re * s, self.im * s)
    }
    pub fn conj(self) -> C {
        C::new(self.re, -self.im)
    }
    pub fn norm2(self) -> f64 {
        self.re * self.re + self.im * self.im
    }
}

/// A cognitive state: complex amplitudes over `n` basis "inclinations" (e.g. the
/// drives). The squared magnitudes are a probability distribution, but the
/// *phases* carry the interference that makes the mind non-classical.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QMind {
    pub psi: Vec<C>,
}

impl QMind {
    /// Prepare a superposition from non-negative weights and phases:
    /// `ψ_i = √wᵢ · e^{iφᵢ}`, normalised. With distinct phases, later rotations
    /// interfere.
    pub fn prepare(weights: &[f64], phases: &[f64]) -> QMind {
        let mut psi: Vec<C> = weights
            .iter()
            .zip(phases.iter())
            .map(|(&w, &p)| C::polar(w.max(0.0).sqrt(), p))
            .collect();
        let mut q = QMind { psi: std::mem::take(&mut psi) };
        q.normalize();
        q
    }

    pub fn n(&self) -> usize {
        self.psi.len()
    }

    pub fn normalize(&mut self) {
        let norm: f64 = self.psi.iter().map(|c| c.norm2()).sum::<f64>().sqrt();
        if norm > 1e-12 {
            for c in &mut self.psi {
                *c = c.scale(1.0 / norm);
            }
        }
    }

    /// A "consideration": a unitary Givens rotation by `theta` in the (i,j)
    /// plane — coupling two inclinations the way weighing one thought against
    /// another does. Rotations in different planes do **not** commute.
    pub fn consider(&mut self, i: usize, j: usize, theta: f64) {
        if i >= self.psi.len() || j >= self.psi.len() || i == j {
            return;
        }
        let (c, s) = (theta.cos(), theta.sin());
        let (a, b) = (self.psi[i], self.psi[j]);
        self.psi[i] = a.scale(c).sub(b.scale(s));
        self.psi[j] = a.scale(s).add(b.scale(c));
    }

    /// A relative phase shift on one inclination — colours how it will later
    /// interfere (mood/affect acting on a thought).
    pub fn phase(&mut self, i: usize, theta: f64) {
        if let Some(a) = self.psi.get_mut(i) {
            *a = a.mul(C::polar(1.0, theta));
        }
    }

    /// The Born-rule distribution `|ψ_i|²`.
    pub fn probs(&self) -> Vec<f64> {
        self.psi.iter().map(|c| c.norm2()).collect()
    }

    /// Shannon entropy of the decision distribution — how "of many minds" it is.
    pub fn entropy(&self) -> f64 {
        self.probs()
            .iter()
            .filter(|&&p| p > 1e-12)
            .map(|&p| -p * p.ln())
            .sum()
    }

    /// Projective measurement: sample an outcome with Born probabilities using a
    /// uniform `u ∈ [0,1)`, then **collapse** the state onto that basis vector.
    pub fn measure(&mut self, u: f64) -> usize {
        let probs = self.probs();
        let mut acc = 0.0;
        let mut out = probs.len() - 1;
        for (i, &p) in probs.iter().enumerate() {
            acc += p;
            if u < acc {
                out = i;
                break;
            }
        }
        for (i, c) in self.psi.iter_mut().enumerate() {
            *c = if i == out { C::new(1.0, 0.0) } else { C::new(0.0, 0.0) };
        }
        out
    }
}

/// Total-variation distance between two distributions (for order-effect tests).
pub fn tv_distance(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum::<f64>() * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_effects_are_non_classical() {
        // two non-commuting "considerations" in different planes.
        let base = || QMind::prepare(&[0.4, 0.3, 0.2, 0.1], &[0.0, 0.7, 1.3, 2.0]);
        let mut ab = base();
        ab.consider(0, 2, 0.9);
        ab.consider(2, 3, 1.1);
        let mut ba = base();
        ba.consider(2, 3, 1.1);
        ba.consider(0, 2, 0.9);
        // order changes the decision distribution — impossible classically.
        assert!(tv_distance(&ab.probs(), &ba.probs()) > 0.05);
    }

    #[test]
    fn interference_violates_law_of_total_probability() {
        // qubit in equal superposition; "decision" is a measurement in a basis
        // rotated by π/4 from the {0,1} question.
        let theta = std::f64::consts::FRAC_PI_4;
        // quantum: rotate then read P(0) — no intermediate question resolved.
        let mut q = QMind::prepare(&[0.5, 0.5], &[0.0, 0.0]);
        q.consider(0, 1, theta);
        let p_quantum = q.probs()[0];
        // classical: resolve the {0,1} question first (collapse), then decide.
        let pre = QMind::prepare(&[0.5, 0.5], &[0.0, 0.0]).probs();
        let mut p_classical = 0.0;
        for (k, &pk) in pre.iter().enumerate() {
            let mut branch = QMind { psi: vec![C::new(0.0, 0.0); 2] };
            branch.psi[k] = C::new(1.0, 0.0); // collapsed onto outcome k
            branch.consider(0, 1, theta);
            p_classical += pk * branch.probs()[0];
        }
        // the interference term is large and nonzero — the law of total
        // probability fails, exactly as in human order/interference effects.
        assert!((p_quantum - p_classical).abs() > 0.2, "I={}", p_quantum - p_classical);
    }

    #[test]
    fn superposition_then_collapse() {
        let mut q = QMind::prepare(&[1.0, 1.0, 1.0, 1.0], &[0.0, 0.5, 1.0, 1.5]);
        let h_before = q.entropy();
        assert!(h_before > 1.0); // genuinely "of several minds"
        q.measure(0.99);
        assert!(q.entropy() < 1e-6); // a decision is a collapse
    }
}
