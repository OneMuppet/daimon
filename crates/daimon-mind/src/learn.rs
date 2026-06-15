//! Learning progress — competence gain as an intrinsic signal.
//!
//! Raw novelty is a poor curiosity signal: it rewards static noise the agent can
//! never master as much as a pattern it could actually learn. Oudeyer & Kaplan's
//! **Intelligent Adaptive Curiosity** (2007) fixes this by rewarding *learning
//! progress* — the **rate at which prediction error is falling** — so an agent is
//! drawn to what is *learnable right now*: not the already-mastered (error flat,
//! low), not the unlearnable (error flat, high), but the frontier where error is
//! actively shrinking. Formally, over a sliding window of the last `2θ` errors:
//!
//! ```text
//! LP = mean(errors[old half]) − mean(errors[new half])      (> 0 ⇒ improving)
//! ```
//!
//! This is the foundational primitive for two things: a *learning-progress drive*
//! (a smarter curiosity), and — next — the **gate** for cumulative cultural
//! transmission, where an agent adopts a peer's affordance only if copying it
//! yields positive learning progress (Cook et al. 2024: accumulation needs social
//! learning *balanced* against independent competence gain, not blind imitation).
//!
//! Deterministic; a few floats of state.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// A sliding-window learning-progress estimator over a stream of prediction
/// errors. `theta` is the half-window: progress compares the older `theta`
/// errors against the newer `theta`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LearningProgress {
    window: VecDeque<f32>,
    theta: usize,
}

impl Default for LearningProgress {
    fn default() -> Self {
        LearningProgress::new(12)
    }
}

impl LearningProgress {
    pub fn new(theta: usize) -> Self {
        LearningProgress { window: VecDeque::with_capacity(theta * 2), theta: theta.max(1) }
    }

    /// Record one prediction error (≥ 0; e.g. 0 = predicted correctly, 1 = wrong).
    pub fn record(&mut self, err: f32) {
        self.window.push_back(err.max(0.0));
        while self.window.len() > self.theta * 2 {
            self.window.pop_front();
        }
    }

    /// Current mean error over the window (competence proxy: lower = more skilled).
    pub fn mean_error(&self) -> f32 {
        if self.window.is_empty() {
            return 0.0;
        }
        self.window.iter().sum::<f32>() / self.window.len() as f32
    }

    /// Learning progress = error-reduction rate over the window. Positive while
    /// the agent is getting better; ~0 once mastered *or* if it can't learn it.
    pub fn progress(&self) -> f32 {
        if self.window.len() < self.theta * 2 {
            return 0.0;
        }
        let t = self.theta;
        let old: f32 = self.window.iter().take(t).sum::<f32>() / t as f32;
        let new: f32 = self.window.iter().skip(t).sum::<f32>() / t as f32;
        old - new
    }

    /// How much of the window has filled (0..1) — confidence in the estimate.
    pub fn maturity(&self) -> f32 {
        self.window.len() as f32 / (self.theta * 2) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learnable_pattern_shows_positive_progress() {
        // a learning curve: error falls from 1.0 toward 0 — LP must be positive.
        let mut lp = LearningProgress::new(6);
        for k in 0..12 {
            lp.record(1.0 - k as f32 / 12.0);
        }
        assert!(lp.progress() > 0.1, "LP {} should be clearly positive", lp.progress());
    }

    #[test]
    fn mastered_pattern_shows_no_progress() {
        // already learned: error pinned near 0 — nothing more to gain.
        let mut lp = LearningProgress::new(6);
        for _ in 0..12 {
            lp.record(0.02);
        }
        assert!(lp.progress().abs() < 0.05, "mastered ⇒ LP≈0, got {}", lp.progress());
    }

    #[test]
    fn unlearnable_noise_shows_no_progress() {
        // irreducible high error (the IAC point): high but flat ⇒ no learning progress,
        // so an LP-driven agent is NOT lured by it, unlike a novelty-driven one.
        let mut lp = LearningProgress::new(6);
        let noise = [0.9, 0.7, 1.0, 0.8, 0.95, 0.75, 0.85, 0.9, 0.7, 1.0, 0.8, 0.9];
        for e in noise {
            lp.record(e);
        }
        assert!(lp.progress().abs() < 0.15, "noise ⇒ LP≈0, got {}", lp.progress());
        assert!(lp.mean_error() > 0.6, "noise stays high-error");
    }
}
