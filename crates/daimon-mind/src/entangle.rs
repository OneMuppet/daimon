//! Conceptual entanglement — cognition past the classical floor (Bell/CHSH).
//!
//! `qcog` modeled one mind as a *superposition*. This goes a floor deeper, to the
//! place where the universe's own rules abandon classical intuition: **quantum
//! entanglement**. Bell's theorem (1964) proved that some correlations between two
//! systems cannot arise from *any* classical assignment of pre-existing values to
//! the parts — no "hidden variables", no local realism. The CHSH inequality
//! (Clauser–Horne–Shimony–Holt 1969) makes it a number: a classical (separable,
//! locally-real) joint state obeys `|S| ≤ 2`; an entangled quantum state can reach
//! the Tsirelson bound `|S| ≤ 2√2 ≈ 2.828` (Tsirelson 1980), and nothing reaches
//! higher.
//!
//! Cognitive science finds the *same* signature in **concept combination**: when
//! two concepts are bound into a pair, people's joint judgments can violate CHSH —
//! the parts have no separable, pre-existing truth-values that compose
//! classically (Aerts & Sozzo 2011; Bruza, Wang & Busemeyer 2015). This is the
//! formal face of the neuroscientific **binding problem** — how separate features
//! become one bound percept. We model a bound concept-pair as a two-qubit state,
//! measure its CHSH `S`, and quantify non-separability by the **von Neumann
//! entanglement entropy** of one concept's reduced state (`ln 2` when maximally
//! bound, `0` when independent — an information-theoretic measure of how much one
//! concept's meaning is *irreducibly tied up with* the other's).
//!
//! **Scope, stated plainly.** This is quantum *probability / contextuality as a
//! model of cognition* — descriptive mathematics that fits non-classical
//! correlations in human concept combination — **not** a claim the brain is a
//! physical quantum computer, and **not** a claim about consciousness. A CHSH
//! value above 2 here means exactly one thing: the bound pair's joint statistics
//! admit *no* classical joint distribution over pre-existing values. Everything is
//! simulated on an ordinary CPU and is deterministic.

use serde::{Deserialize, Serialize};

use crate::qcog::C;

/// A two-"concept" (two-qubit) cognitive state: 4 complex amplitudes over the
/// basis |00>, |01>, |10>, |11> (index = 2·a + b). Squared magnitudes sum to 1.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entangled {
    pub psi: [C; 4],
}

impl Entangled {
    /// The maximally-entangled Bell state |Φ⁺> = (|00> + |11>)/√2 — two concepts
    /// bound so tightly that neither has a separate state of its own.
    pub fn bell() -> Entangled {
        let s = 1.0 / 2.0_f64.sqrt();
        Entangled { psi: [C::new(s, 0.0), C::new(0.0, 0.0), C::new(0.0, 0.0), C::new(s, 0.0)] }
    }

    /// A separable (classical, independent) pair: the tensor product of two
    /// single-qubit states `|α> ⊗ |β>`, each given as amplitudes `(a0,a1)`.
    /// Such a state can never violate CHSH — it is the classical control.
    pub fn product(alpha: (C, C), beta: (C, C)) -> Entangled {
        let mut e = Entangled {
            psi: [
                alpha.0.mul(beta.0),
                alpha.0.mul(beta.1),
                alpha.1.mul(beta.0),
                alpha.1.mul(beta.1),
            ],
        };
        e.normalize();
        e
    }

    /// A partially-entangled pair tuned by `t ∈ [0,1]`: `cos·|00> + sin·|11>`,
    /// interpolating a fully-separable pair (`t=0`) and the Bell state (`t=π/4`).
    pub fn tuned(theta: f64) -> Entangled {
        let mut e = Entangled {
            psi: [
                C::new(theta.cos(), 0.0),
                C::new(0.0, 0.0),
                C::new(0.0, 0.0),
                C::new(theta.sin(), 0.0),
            ],
        };
        e.normalize();
        e
    }

    pub fn normalize(&mut self) {
        let n: f64 = self.psi.iter().map(|c| c.norm2()).sum::<f64>().sqrt();
        if n > 1e-12 {
            for c in &mut self.psi {
                *c = c.scale(1.0 / n);
            }
        }
    }

    /// Correlation `E(a,b) = <ψ| σ(a) ⊗ σ(b) |ψ>` of two ±1 "judgments", each a
    /// spin measurement at its angle: `σ(θ) = cosθ·Z + sinθ·X`. Real-valued.
    pub fn correlation(&self, a: f64, b: f64) -> f64 {
        let sa = pauli(a);
        let sb = pauli(b);
        // M = σ(a) ⊗ σ(b), a real 4×4 matrix; E = Σ_r Σ_c ψ*_r M_rc ψ_c.
        let mut e = 0.0;
        for r in 0..4 {
            let (ra, rb) = (r >> 1, r & 1);
            for c in 0..4 {
                let (ca, cb) = (c >> 1, c & 1);
                let m = sa[ra][ca] * sb[rb][cb];
                if m != 0.0 {
                    // Re(ψ*_r · ψ_c) · m  (M real ⇒ imaginary parts cancel in sum).
                    let prod = self.psi[r].conj().mul(self.psi[c]);
                    e += m * prod.re;
                }
            }
        }
        e
    }

    /// The CHSH statistic with measurement angles `(a, a')` for concept A and
    /// `(b, b')` for concept B:  `S = E(a,b) + E(a,b') + E(a',b) − E(a',b')`.
    /// Classical/separable: `|S| ≤ 2`. Quantum: up to `2√2`.
    pub fn chsh(&self, a: f64, ap: f64, b: f64, bp: f64) -> f64 {
        self.correlation(a, b) + self.correlation(a, bp) + self.correlation(ap, b)
            - self.correlation(ap, bp)
    }

    /// The canonical CHSH value at the optimal angles (0, π/2, π/4, −π/4), where a
    /// Bell state attains the Tsirelson bound 2√2.
    pub fn chsh_optimal(&self) -> f64 {
        use std::f64::consts::{FRAC_PI_2, FRAC_PI_4};
        self.chsh(0.0, FRAC_PI_2, FRAC_PI_4, -FRAC_PI_4)
    }

    /// Von Neumann entanglement entropy `S(ρ_A) = −Σ λ ln λ` of one concept's
    /// reduced state (partial trace over the other). `0` = separable/independent;
    /// `ln 2 ≈ 0.693` = maximally entangled (the concept has *no* meaning of its
    /// own apart from the pair). An information-theoretic non-separability measure.
    pub fn entanglement_entropy(&self) -> f64 {
        // reduced density matrix ρ_A (2×2, Hermitian): ρ_A[i][k] = Σ_j ψ_{2i+j} ψ*_{2k+j}.
        let mut rho = [[C::new(0.0, 0.0); 2]; 2];
        for (i, row) in rho.iter_mut().enumerate() {
            for (k, cell) in row.iter_mut().enumerate() {
                let mut s = C::new(0.0, 0.0);
                for j in 0..2 {
                    s = s.add(self.psi[2 * i + j].mul(self.psi[2 * k + j].conj()));
                }
                *cell = s;
            }
        }
        // eigenvalues of a 2×2 Hermitian [[a, b],[b*, d]]:
        // λ = (a+d)/2 ± sqrt(((a−d)/2)² + |b|²).
        let a = rho[0][0].re;
        let d = rho[1][1].re;
        let off2 = rho[0][1].norm2();
        let mid = (a + d) / 2.0;
        let rad = (((a - d) / 2.0).powi(2) + off2).max(0.0).sqrt();
        let l1 = (mid + rad).clamp(0.0, 1.0);
        let l2 = (mid - rad).clamp(0.0, 1.0);
        let term = |l: f64| if l > 1e-12 { -l * l.ln() } else { 0.0 };
        term(l1) + term(l2)
    }
}

/// The single-qubit observable `σ(θ) = cosθ·Z + sinθ·X` as a real 2×2 matrix
/// (eigenvalues ±1 — a yes/no judgment along direction θ).
fn pauli(theta: f64) -> [[f64; 2]; 2] {
    let (c, s) = (theta.cos(), theta.sin());
    [[c, s], [s, -c]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qcog::C;

    #[test]
    fn bell_state_violates_chsh() {
        // entangled concept-pair breaches the classical bound, up to Tsirelson 2√2.
        let s = Entangled::bell().chsh_optimal();
        assert!(s > 2.0, "S = {s} must exceed the classical bound 2");
        assert!((s - 8.0_f64.sqrt()).abs() < 1e-9, "S = {s} should hit 2√2 ≈ 2.828");
    }

    #[test]
    fn product_state_respects_classical_bound() {
        // any separable (independent) pair obeys |S| ≤ 2 — Bell's classical limit.
        let zero = (C::new(1.0, 0.0), C::new(0.0, 0.0));
        let plus = (C::new(1.0, 0.0), C::new(1.0, 0.0));
        for st in [
            Entangled::product(zero, zero),
            Entangled::product(zero, plus),
            Entangled::product(plus, plus),
        ] {
            assert!(st.chsh_optimal().abs() <= 2.0 + 1e-9);
        }
    }

    #[test]
    fn entropy_measures_binding() {
        // maximally-entangled pair: entropy ln 2; independent pair: 0.
        assert!((Entangled::bell().entanglement_entropy() - 2.0_f64.ln()).abs() < 1e-9);
        let zero = (C::new(1.0, 0.0), C::new(0.0, 0.0));
        assert!(Entangled::product(zero, zero).entanglement_entropy() < 1e-9);
    }

    #[test]
    fn entanglement_rises_with_binding() {
        // sweeping the bind angle from separable to Bell raises both the CHSH
        // violation and the entanglement entropy monotonically — a coherent dial.
        let weak = Entangled::tuned(0.15);
        let strong = Entangled::tuned(std::f64::consts::FRAC_PI_4);
        assert!(strong.chsh_optimal() > weak.chsh_optimal());
        assert!(strong.entanglement_entropy() > weak.entanglement_entropy());
    }
}
