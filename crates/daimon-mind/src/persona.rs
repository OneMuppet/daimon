//! Persona — a stable identity layered over the shared architecture.
//!
//! Two Daimons running identical code should not feel identical. A persona is a
//! small vector of trait biases that colours appraisal and arbitration: a bold
//! agent tolerates a closer predator before fleeing; a sociable one weights the
//! social drive more heavily; a curious one finds novelty more rewarding. This
//! is how one engine yields a *cast* of characters rather than a clone army —
//! and a consistent persona over a long life is most of what we mean by a
//! "believable" character.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Persona {
    pub name: String,
    /// 0 = timid (flees early), 1 = fearless (flees late). Scales the reflex
    /// distance and the survival-drive gain.
    pub boldness: f32,
    /// Multiplier on the social drive's pull.
    pub sociability: f32,
    /// Multiplier on intrinsic curiosity reward.
    pub curiosity: f32,
    /// A one-line self-concept, surfaced in narration and seeded into memory.
    pub creed: String,
}

impl Persona {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            boldness: 0.5,
            sociability: 0.5,
            curiosity: 0.5,
            creed: "I want to understand this place.".to_string(),
        }
    }

    pub fn with_boldness(mut self, v: f32) -> Self {
        self.boldness = v.clamp(0.0, 1.0);
        self
    }
    pub fn with_sociability(mut self, v: f32) -> Self {
        self.sociability = v.clamp(0.0, 1.0);
        self
    }
    pub fn with_curiosity(mut self, v: f32) -> Self {
        self.curiosity = v.clamp(0.0, 1.0);
        self
    }
    pub fn with_creed(mut self, creed: &str) -> Self {
        self.creed = creed.to_string();
        self
    }

    /// Manhattan distance at which a predator triggers the flee reflex. Bold
    /// agents let it get closer.
    pub fn reflex_distance(&self) -> i32 {
        2 + (self.boldness * 2.0) as i32 // 2..4
    }
}
