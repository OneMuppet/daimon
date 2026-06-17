//! System 2 — a tiny, deterministic, **evolved-plastic** neural overlay.
//!
//! Daimon's mind is hand-built *instinct* (System 1): drives → appraisal → a
//! priority cascade that picks a goal. The 9 machine-checked proofs cover that
//! layer. This module adds a **learned overlay** (System 2) that rides on top:
//! a small network reads the situation and emits a *bounded bias* on the drive
//! pressures the instinct arbitrates over. It **nudges**, it never replaces — and
//! when it is disabled it contributes *exactly zero*, so the instinct (and the
//! whole seeded harness) is byte-identical.
//!
//! Two timescales, after Baldwin (Hinton & Nowlan 1987):
//! * **Phylogeny (germline).** The genome carries only the *learning machinery*
//!   — an init seed, a learning rate, and an output-modulation scale (indirect
//!   encoding). The evolutionary search tunes *how to learn*, not the weights.
//! * **Ontogeny (a single life).** Within one life the weights adapt by a
//!   **reward-modulated three-factor Hebbian rule** `Δw = η · r · pre · post`,
//!   where the reward `r` is the change in the mind's *own* well-being. The net
//!   learns the directions that historically improved this individual's life.
//!
//! Everything here is deterministic f32 arithmetic with a seeded init, so the
//! reproducibility guarantee (proof T1) is untouched. The overlay is validated
//! empirically (ablation criteria), never claimed as *proved*.

use serde::{Deserialize, Serialize};

use daimon_core::rng::Rng;

/// Situation features fed in (assembled in `mind.rs`). Keep in lockstep there.
pub const N_IN: usize = 16;
/// Hidden units (tanh).
pub const N_HID: usize = 12;
/// One bias output per drive (`Drive::ALL` order).
pub const N_OUT: usize = 6;

const W_CLIP: f32 = 3.0; // keep weights bounded — no blow-up, no NaN
const INIT_SCALE: f32 = 0.4; // small initial weights

/// A tiny MLP (`N_IN → N_HID → N_OUT`, tanh) with reward-modulated Hebbian
/// plasticity. Inert and zero-output when `enabled` is false.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Overlay {
    enabled: bool,
    lr: f32,         // learning rate η
    modulation: f32, // how strongly the outputs bias drive pressures
    w1: Vec<f32>,    // [N_HID][N_IN]
    w2: Vec<f32>,    // [N_OUT][N_HID]
    // last forward pass, cached for the three-factor Hebbian update
    last_in: Vec<f32>,
    last_hid: Vec<f32>,
    last_out: Vec<f32>,
    primed: bool,
}

impl Default for Overlay {
    fn default() -> Self {
        Overlay::disabled()
    }
}

impl Overlay {
    /// An inert overlay: never fires, never learns, contributes zero bias.
    pub fn disabled() -> Self {
        Overlay {
            enabled: false,
            lr: 0.0,
            modulation: 0.0,
            w1: Vec::new(),
            w2: Vec::new(),
            last_in: Vec::new(),
            last_hid: Vec::new(),
            last_out: Vec::new(),
            primed: false,
        }
    }

    /// Build an enabled overlay with weights seeded deterministically from the
    /// genome's init seed (indirect encoding). `lr` and `modulation` come from
    /// the genome too. The germline EA explores weight-space *coarsely* through
    /// the seed; lifetime plasticity does the fine adaptation.
    pub fn seeded(seed: u64, lr: f32, modulation: f32) -> Self {
        let mut rng = Rng::new(seed ^ 0x5EED_5EED_5EED_5EED);
        let mut w1 = vec![0.0f32; N_HID * N_IN];
        let mut w2 = vec![0.0f32; N_OUT * N_HID];
        for w in w1.iter_mut() {
            *w = (rng.next_f32() * 2.0 - 1.0) * INIT_SCALE;
        }
        for w in w2.iter_mut() {
            *w = (rng.next_f32() * 2.0 - 1.0) * INIT_SCALE;
        }
        Overlay {
            enabled: true,
            lr: lr.clamp(0.0, 0.2),
            modulation: modulation.clamp(0.0, 1.0),
            w1,
            w2,
            last_in: vec![0.0; N_IN],
            last_hid: vec![0.0; N_HID],
            last_out: vec![0.0; N_OUT],
            primed: false,
        }
    }

    #[inline]
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Forward pass. Returns the per-drive bias vector already scaled by the
    /// modulation gene. When disabled, returns all-zeros and caches nothing —
    /// so the caller adds exactly 0.0 and the instinct is unchanged.
    // The flat weight matrices are indexed by computed offsets (`base + i`), so
    // the range loops are deliberate, not iterator-replaceable.
    #[allow(clippy::needless_range_loop)]
    pub fn bias(&mut self, x: &[f32; N_IN]) -> [f32; N_OUT] {
        if !self.enabled {
            return [0.0; N_OUT];
        }
        // hidden = tanh(W1 · x)
        let mut hid = [0.0f32; N_HID];
        for h in 0..N_HID {
            let mut s = 0.0f32;
            let base = h * N_IN;
            for i in 0..N_IN {
                s += self.w1[base + i] * x[i];
            }
            hid[h] = s.tanh();
        }
        // out = tanh(W2 · hidden)  ∈ [-1, 1]
        let mut out = [0.0f32; N_OUT];
        for o in 0..N_OUT {
            let mut s = 0.0f32;
            let base = o * N_HID;
            for h in 0..N_HID {
                s += self.w2[base + h] * hid[h];
            }
            out[o] = s.tanh();
        }
        // cache for the next Hebbian update
        self.last_in.copy_from_slice(x);
        self.last_hid.copy_from_slice(&hid);
        self.last_out.copy_from_slice(&out);
        self.primed = true;
        // scale into a bounded bias
        let m = self.modulation;
        let mut biased = [0.0f32; N_OUT];
        for o in 0..N_OUT {
            biased[o] = out[o] * m;
        }
        biased
    }

    /// Reward-modulated three-factor Hebbian update on the *last* forward pass.
    /// `reward` is the change in the mind's own well-being (intrinsic; clipped to
    /// `[-1, 1]`). A no-op when disabled or before the first `bias()` call.
    pub fn learn(&mut self, reward: f32) {
        if !self.enabled || !self.primed || self.lr == 0.0 {
            return;
        }
        let r = reward.clamp(-1.0, 1.0);
        if r == 0.0 {
            return;
        }
        let eta = self.lr;
        // output layer: Δw2[o,h] = η · r · hid[h] · out[o]
        for o in 0..N_OUT {
            let post = self.last_out[o];
            let base = o * N_HID;
            for h in 0..N_HID {
                let dw = eta * r * self.last_hid[h] * post;
                let w = (self.w2[base + h] + dw).clamp(-W_CLIP, W_CLIP);
                self.w2[base + h] = w;
            }
        }
        // input layer: Δw1[h,i] = η · r · in[i] · hid[h]
        for h in 0..N_HID {
            let post = self.last_hid[h];
            let base = h * N_IN;
            for i in 0..N_IN {
                let dw = eta * r * self.last_in[i] * post;
                let w = (self.w1[base + i] + dw).clamp(-W_CLIP, W_CLIP);
                self.w1[base + i] = w;
            }
        }
    }

    /// Sum of |weights| — a cheap probe used by tests to confirm learning moved
    /// the network (and that it stays bounded).
    pub fn weight_magnitude(&self) -> f32 {
        self.w1.iter().chain(self.w2.iter()).map(|w| w.abs()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feats(v: f32) -> [f32; N_IN] {
        [v; N_IN]
    }

    #[test]
    fn disabled_is_zero_and_inert() {
        let mut o = Overlay::disabled();
        assert_eq!(o.bias(&feats(0.7)), [0.0; N_OUT]);
        o.learn(1.0); // no-op
        assert_eq!(o.weight_magnitude(), 0.0);
        assert!(!o.enabled());
    }

    #[test]
    fn seeded_is_deterministic() {
        let a = Overlay::seeded(0xABCD, 0.05, 0.5);
        let b = Overlay::seeded(0xABCD, 0.05, 0.5);
        assert_eq!(a.weight_magnitude(), b.weight_magnitude());
        let c = Overlay::seeded(0x1234, 0.05, 0.5);
        assert_ne!(a.weight_magnitude(), c.weight_magnitude());
    }

    #[test]
    fn output_is_bounded_by_modulation() {
        let mut o = Overlay::seeded(0x7, 0.05, 0.5);
        let b = o.bias(&feats(1.0));
        for v in b {
            assert!(v.abs() <= 0.5 + 1e-6, "bias {v} exceeded modulation 0.5");
        }
    }

    #[test]
    fn learning_moves_weights_and_stays_bounded() {
        let mut o = Overlay::seeded(0x42, 0.1, 0.5);
        let before = o.weight_magnitude();
        for _ in 0..200 {
            let _ = o.bias(&feats(0.8));
            o.learn(0.6);
        }
        let after = o.weight_magnitude();
        assert!((after - before).abs() > 1e-3, "weights did not change with learning");
        // bounded: every weight within the clip
        assert!(o.w1.iter().chain(o.w2.iter()).all(|w| w.abs() <= W_CLIP + 1e-6));
        assert!(after.is_finite());
    }

    #[test]
    fn zero_reward_does_not_learn() {
        let mut o = Overlay::seeded(0x9, 0.1, 0.5);
        let _ = o.bias(&feats(0.5));
        let before = o.weight_magnitude();
        o.learn(0.0);
        assert_eq!(o.weight_magnitude(), before);
    }
}
