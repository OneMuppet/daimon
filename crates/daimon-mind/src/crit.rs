//! Neural criticality — tuning cognition to the *edge of chaos*.
//!
//! Cortex appears to operate near a **critical point** between order and chaos.
//! Activity propagates as **neuronal avalanches** whose sizes are power-law
//! distributed (Beggs & Plenz 2003), the fingerprint of a branching process with
//! branching ratio `σ ≈ 1`: each active unit triggers, on average, one more.
//! Below it (`σ < 1`, subcritical) activity dies out — a rigid, unresponsive
//! mind; above it (`σ > 1`, supercritical) activity explodes — a seizing,
//! chaotic mind. *At* `σ = 1` the system maximises its **dynamic range**: it
//! distinguishes the widest span of stimulus intensities (Kinouchi & Copelli,
//! Nature Physics 2006). Criticality is, in this sense, the operating regime an
//! intelligence *wants*.
//!
//! This module is the substrate, not a metaphor: a network of excitable units
//! whose salience propagation **is** a branching process, plus a homeostatic
//! controller that drives it to criticality on its own (**self-organised
//! criticality**) — the same role synaptic scaling plays biologically. Two
//! classically-checkable signatures are tested in the harness:
//!
//! * **AC25 — self-organised criticality.** From an arbitrary coupling the
//!   controller tunes the measured branching ratio to `σ ≈ 1`.
//! * **AC26 — dynamic range peaks at criticality.** Sweeping stimulus intensity,
//!   the response curve's dynamic range is largest at `σ ≈ 1`, smaller in the
//!   sub- and supercritical regimes.
//!
//! Deterministic given the seed; runs on an ordinary CPU.

use daimon_core::Rng;

/// An excitable network (Kinouchi–Copelli style). Units cycle
/// quiescent → active → refractory → quiescent. An active unit excites each of
/// its `k` out-neighbours with probability `w`, so the branching ratio is
/// `σ ≈ k·w`. External stimulus activates quiescent units at rate `h`.
#[derive(Clone, Debug)]
pub struct CriticalNet {
    n: usize,
    k: usize,
    /// Per-synapse activation probability. `σ ≈ k · w`.
    pub w: f32,
    /// Refractory length (steps a unit is unexcitable after firing).
    ref_len: u8,
    /// State: 0 quiescent, 1 active, 2..=(1+ref_len) refractory (counts up to 0).
    state: Vec<u8>,
    out: Vec<Vec<u32>>,
}

impl CriticalNet {
    /// Build a random `k`-out-regular network of `n` units with branching ratio
    /// `sigma` (so `w = sigma / k`).
    pub fn new(n: usize, k: usize, sigma: f32, ref_len: u8, rng: &mut Rng) -> CriticalNet {
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let mut targets = Vec::with_capacity(k);
            while targets.len() < k {
                let t = rng.below(n) as u32;
                if t as usize != i && !targets.contains(&t) {
                    targets.push(t);
                }
            }
            out.push(targets);
        }
        CriticalNet { n, k, w: sigma / k as f32, ref_len, state: vec![0; n], out }
    }

    pub fn sigma(&self) -> f32 {
        self.k as f32 * self.w
    }

    fn quiescent(&mut self) {
        for s in &mut self.state {
            *s = 0;
        }
    }

    fn active_count(&self) -> usize {
        self.state.iter().filter(|&&s| s == 1).count()
    }

    /// Advance one step under external drive `h`; returns the number of units
    /// that fire (enter the active state) this step.
    pub fn step(&mut self, h: f32, rng: &mut Rng) -> usize {
        let mut next = vec![0u8; self.n];
        // Advance everything already firing/refractory through its cycle.
        for (n, &s) in next.iter_mut().zip(self.state.iter()) {
            match s {
                0 => {}
                1 => *n = if self.ref_len > 0 { 2 } else { 0 },
                s => *n = if s > self.ref_len { 0 } else { s + 1 },
            }
        }
        // External drive onto quiescent units.
        if h > 0.0 {
            for (n, &s) in next.iter_mut().zip(self.state.iter()) {
                if s == 0 && *n == 0 && rng.chance(h) {
                    *n = 1;
                }
            }
        }
        // Propagation from currently-active units (the branching process).
        for j in 0..self.n {
            if self.state[j] == 1 {
                for idx in 0..self.out[j].len() {
                    let t = self.out[j][idx] as usize;
                    if self.state[t] == 0 && next[t] == 0 && rng.chance(self.w) {
                        next[t] = 1;
                    }
                }
            }
        }
        self.state = next;
        self.active_count()
    }

    /// Empirical branching ratio: seed one unit, let the avalanche run with no
    /// external drive, and average descendants-per-ancestor over many trials.
    pub fn branching_ratio(&mut self, trials: usize, rng: &mut Rng) -> f32 {
        let mut ancestors = 0u64; // active units that get a chance to propagate
        let mut descendants = 0u64; // units they activate next step
        for _ in 0..trials {
            self.quiescent();
            let seed = rng.below(self.n);
            self.state[seed] = 1;
            let mut cur = 1usize;
            let mut guard = 0;
            while cur > 0 && guard < 64 {
                let next = self.step(0.0, rng);
                ancestors += cur as u64;
                descendants += next as u64;
                cur = next;
                guard += 1;
            }
        }
        if ancestors == 0 {
            0.0
        } else {
            descendants as f32 / ancestors as f32
        }
    }

    /// Mean fraction of active units per step under steady external drive `h`.
    pub fn mean_response(&mut self, h: f32, warmup: usize, steps: usize, rng: &mut Rng) -> f32 {
        self.quiescent();
        for _ in 0..warmup {
            self.step(h, rng);
        }
        let mut total = 0u64;
        for _ in 0..steps {
            total += self.step(h, rng) as u64;
        }
        total as f32 / (steps as f32 * self.n as f32)
    }

    /// **Self-organised criticality.** Homeostatically adjust `w` so the measured
    /// branching ratio approaches 1 — the network finds its own critical point.
    /// Returns the final measured σ.
    pub fn self_organise(&mut self, iters: usize, lr: f32, rng: &mut Rng) -> f32 {
        let mut measured = self.branching_ratio(64, rng);
        for _ in 0..iters {
            // facilitate when subcritical, depress when supercritical.
            self.w *= 1.0 + lr * (1.0 - measured);
            self.w = self.w.clamp(0.001, 0.95);
            measured = self.branching_ratio(64, rng);
        }
        measured
    }
}

/// Dynamic range (in decibels) of a response curve: how wide a span of stimulus
/// intensities the system encodes between 10% and 90% of its active range.
/// `Δ = 10·log10(h₉₀ / h₁₀)`. Maximised at criticality (Kinouchi & Copelli 2006).
pub fn dynamic_range(stimuli: &[f32], responses: &[f32]) -> f32 {
    let f0 = responses.first().copied().unwrap_or(0.0);
    let fmax = responses.last().copied().unwrap_or(0.0);
    let span = fmax - f0;
    if span <= 1e-6 {
        return 0.0;
    }
    let h_at = |frac: f32| -> f32 {
        let target = f0 + frac * span;
        // first stimulus whose response crosses `target`, linearly interpolated.
        for i in 1..responses.len() {
            if responses[i] >= target {
                let (r0, r1) = (responses[i - 1], responses[i]);
                let (s0, s1) = (stimuli[i - 1].max(1e-9), stimuli[i].max(1e-9));
                let frac = if (r1 - r0).abs() < 1e-9 { 0.0 } else { (target - r0) / (r1 - r0) };
                // interpolate in log-stimulus space.
                let l = s0.ln() + frac * (s1.ln() - s0.ln());
                return l.exp();
            }
        }
        stimuli.last().copied().unwrap_or(1.0)
    };
    let h10 = h_at(0.1).max(1e-9);
    let h90 = h_at(0.9).max(1e-9);
    10.0 * (h90 / h10).log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_organises_to_criticality() {
        let mut rng = Rng::new(0xC417);
        // start badly subcritical; the controller should climb to σ≈1.
        let mut net = CriticalNet::new(600, 10, 0.4, 2, &mut rng);
        let s = net.self_organise(40, 0.4, &mut rng);
        assert!((0.8..=1.25).contains(&s), "σ converged to {s}");
    }

    #[test]
    fn dynamic_range_peaks_at_criticality() {
        let mut rng = Rng::new(0xED9E);
        let stimuli: Vec<f32> =
            (0..18).map(|i| 10f32.powf(-4.0 + i as f32 * 4.0 / 17.0)).collect();
        let dr = |sigma: f32, rng: &mut Rng| {
            let mut net = CriticalNet::new(500, 10, sigma, 2, rng);
            let resp: Vec<f32> =
                stimuli.iter().map(|&h| net.mean_response(h, 60, 120, rng)).collect();
            dynamic_range(&stimuli, &resp)
        };
        let sub = dr(0.6, &mut rng);
        let crit = dr(1.0, &mut rng);
        let sup = dr(1.6, &mut rng);
        assert!(crit > sub && crit > sup, "Δ: sub={sub} crit={crit} sup={sup}");
    }
}
