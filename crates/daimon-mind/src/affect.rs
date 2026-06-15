//! Affect — a felt emotional state, not just a stack of drives.
//!
//! Drives say *what the agent needs*; affect says *how it feels* about its
//! situation as a whole. Following Russell's **circumplex model** (1980), emotion
//! is two dimensions: **valence** (−1 unpleasant … +1 pleasant) and **arousal**
//! (0 calm … 1 activated). Their quadrants name the felt state — content, elated,
//! afraid, weary — exactly the legible "moods" that make an NPC read as alive
//! rather than as a utility function. The state is set by *appraisal* (Scherer;
//! Lazarus): a thriving, safe body drifts pleasant and calm; threat and harm push
//! unpleasant and activated; surprise spikes arousal.
//!
//! Tracked read-only by default (it colours the inspector and the agent's aura);
//! an optional modulation can let it bias behaviour. Deterministic; two floats.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Affect {
    /// −1 unpleasant … +1 pleasant.
    pub valence: f32,
    /// 0 calm … 1 activated.
    pub arousal: f32,
}

impl Default for Affect {
    fn default() -> Self {
        Affect { valence: 0.2, arousal: 0.2 } // mildly content, calm
    }
}

impl Affect {
    /// Appraise the situation into feeling. `condition` is overall body wellbeing
    /// (0..1), `threat` is danger (0..1: predator proximity + injury), `surprise`
    /// is prediction error (0..1), `urgency` is the strongest drive pressure
    /// (0..1). Valence and arousal ease toward their appraised targets (affect has
    /// inertia — moods don't snap).
    pub fn update(&mut self, condition: f32, threat: f32, surprise: f32, urgency: f32) {
        let target_v = (condition * 2.0 - 1.0 - threat).clamp(-1.0, 1.0);
        let target_a = (0.5 * surprise + threat + 0.4 * urgency).clamp(0.0, 1.0);
        self.valence += (target_v - self.valence) * 0.15;
        self.arousal += (target_a - self.arousal) * 0.2;
    }

    /// The felt state's name, by circumplex quadrant.
    pub fn emotion(&self) -> &'static str {
        match (self.valence >= 0.0, self.arousal >= 0.5) {
            (true, true) => "elated",
            (true, false) => "content",
            (false, true) => "afraid",
            (false, false) => "weary",
        }
    }

    /// A colour for the mood (0xRRGGBB) — for tinting the agent's aura.
    pub fn hue(&self) -> u32 {
        match (self.valence >= 0.0, self.arousal >= 0.5) {
            (true, true) => 0xffd24e,  // elated — warm gold
            (true, false) => 0x5fd6a0, // content — calm green
            (false, true) => 0xff5a5a, // afraid — alarm red
            (false, false) => 0x6a78a0, // weary — cold slate
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thriving_and_safe_feels_content() {
        let mut a = Affect::default();
        for _ in 0..30 {
            a.update(1.0, 0.0, 0.0, 0.1); // full body, no threat, calm
        }
        assert!(a.valence > 0.3, "valence {}", a.valence);
        assert!(a.arousal < 0.3, "arousal {}", a.arousal);
        assert_eq!(a.emotion(), "content");
    }

    #[test]
    fn threat_and_harm_feels_afraid() {
        let mut a = Affect::default();
        for _ in 0..30 {
            a.update(0.3, 0.9, 0.6, 0.9); // hurt, predator near, surprised, urgent
        }
        assert!(a.valence < -0.2, "valence {}", a.valence);
        assert!(a.arousal > 0.6, "arousal {}", a.arousal);
        assert_eq!(a.emotion(), "afraid");
    }

    #[test]
    fn affect_has_inertia() {
        // a single bad tick does not instantly flip a content mood.
        let mut a = Affect::default();
        for _ in 0..20 {
            a.update(1.0, 0.0, 0.0, 0.1);
        }
        let calm = a.arousal;
        a.update(0.2, 0.9, 0.8, 0.9); // one alarming tick
        assert!(a.arousal < calm + 0.25, "mood should not snap: {} -> {}", calm, a.arousal);
    }
}
