//! Stigmergy — coordination through the environment, not through messages.
//!
//! Ants find shortest paths with no map, no leader, and no communication: each
//! leaves a little **pheromone** where it walks, pheromone **evaporates**, and the
//! next ant is biased toward stronger trails. Shorter paths are traversed faster
//! and more often, so they accumulate pheromone faster, so more ants take them —
//! a positive feedback that makes the colony *converge on the optimum* purely
//! through traces left in the world (Grassé 1959 coined "stigmergy"; Dorigo &
//! Stützle 2004 formalised it as Ant Colony Optimization). This is collective
//! intelligence as an emergent property of the environment — a deeply different,
//! and for believable game AI genuinely novel, way for a crowd to coordinate
//! (worn paths, shared routes, self-organising flow) without any agent deciding
//! for the group.
//!
//! Here is the canonical falsifier — Deneubourg's **double-bridge**: two routes
//! between a nest and food, one short, one long. With reinforcement the colony
//! self-organises onto the short one; with symmetric routes it stays split. All
//! deterministic on a seeded RNG.

use daimon_core::Rng;
use serde::{Deserialize, Serialize};

/// A two-route double-bridge with evaporating pheromone trails.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DoubleBridge {
    /// Pheromone on [short, long].
    tau: [f64; 2],
    /// Route lengths [short, long]; shorter ⇒ more pheromone deposited per ant
    /// (faster round trip) and a shorter path to reinforce.
    len: [f64; 2],
    /// Trail-following sharpness (ACO α).
    alpha: f64,
    /// Deposit constant (ACO Q).
    q: f64,
    /// Evaporation rate ρ ∈ (0,1).
    evap: f64,
}

impl DoubleBridge {
    pub fn new(short_len: f64, long_len: f64) -> DoubleBridge {
        DoubleBridge { tau: [1.0, 1.0], len: [short_len.max(1.0), long_len.max(1.0)], alpha: 2.0, q: 1.0, evap: 0.1 }
    }

    /// Trail-following sharpness. `0` disables stigmergy (choices stay 50/50) — the
    /// control that isolates the feedback as the cause of convergence.
    pub fn set_alpha(&mut self, a: f64) {
        self.alpha = a.max(0.0);
    }

    /// Probability the next ant takes the short route, by pheromone (ACO rule).
    pub fn p_short(&self) -> f64 {
        let s = self.tau[0].powf(self.alpha);
        let l = self.tau[1].powf(self.alpha);
        if s + l <= 0.0 {
            0.5
        } else {
            s / (s + l)
        }
    }

    /// One foraging round: `ants` ants each pick a route (pheromone-biased),
    /// deposit `q/len` on it, then all trails evaporate.
    pub fn round(&mut self, ants: usize, rng: &mut Rng) {
        let p = self.p_short();
        let mut dep = [0.0f64; 2];
        for _ in 0..ants {
            let k = if (rng.next_f32() as f64) < p { 0 } else { 1 };
            dep[k] += self.q / self.len[k];
        }
        for (t, d) in self.tau.iter_mut().zip(dep.iter()) {
            *t = (1.0 - self.evap) * *t + *d;
        }
    }

    /// Run many rounds and return the final short-route share.
    pub fn run(&mut self, rounds: usize, ants: usize, rng: &mut Rng) -> f64 {
        for _ in 0..rounds {
            self.round(ants, rng);
        }
        self.p_short()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colony_self_organises_onto_the_short_route() {
        let mut rng = Rng::new(0x57161);
        let mut b = DoubleBridge::new(5.0, 10.0);
        let p = b.run(60, 24, &mut rng);
        assert!(p > 0.85, "colony should converge on the short route, got P={p}");
    }

    #[test]
    fn without_trail_following_no_convergence() {
        // CONTROL: same short/long asymmetry, but α=0 disables stigmergy — choices
        // stay 50/50, so the colony does NOT find the short route. This isolates
        // the trail feedback (not the geometry) as the cause of optimisation.
        let mut rng = Rng::new(0x5161);
        let mut b = DoubleBridge::new(5.0, 10.0);
        b.set_alpha(0.0);
        let p = b.run(60, 24, &mut rng);
        assert!((p - 0.5).abs() < 0.1, "no trail-following ⇒ stays split, got P={p}");
    }

    #[test]
    fn reinforcement_beats_the_initial_coin_flip() {
        let mut rng = Rng::new(0xAC0);
        let mut b = DoubleBridge::new(4.0, 9.0);
        assert!((b.p_short() - 0.5).abs() < 1e-9); // starts at a coin flip
        let p = b.run(50, 20, &mut rng);
        assert!(p > 0.5 + 0.2, "stigmergic feedback should amplify the short route");
    }
}
