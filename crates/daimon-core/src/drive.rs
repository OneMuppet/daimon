//! The drive system — Daimon's motivational core.
//!
//! Scripted NPCs do what they are told. A Daimon does what it *wants*, and what
//! it wants emerges from a small homeostatic drive system: a handful of needs,
//! each with a current urgency in `[0, 1]`, that drift over time and react to
//! what the agent senses. The dominant drive shapes which goal the agent
//! adopts. Nobody scripts "go eat"; hunger simply wins the arbitration.
//!
//! This is the classic homeostatic-motivation model (Hull's drive-reduction,
//! Maslow-style prioritisation) combined with an *intrinsic* curiosity drive in
//! the spirit of Schmidhuber's formal theory of curiosity and Pathak et al.'s
//! Intrinsic Curiosity Module: surprise is rewarding, so the agent explores
//! even when all its physiological needs are met. That intrinsic pressure is
//! what stops a well-fed agent from standing still — and what makes it look
//! *alive*.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Drive {
    /// Stay alive — spikes when health is low or a predator is near.
    Survival,
    /// Eat — grows as energy depletes.
    Hunger,
    /// Drink — grows as hydration depletes.
    Thirst,
    /// Explore the unknown — fed by novelty, the intrinsic engine.
    Curiosity,
    /// Be near and interact with other agents.
    Social,
    /// Get better at things — rewards practising known skills.
    Mastery,
}

impl Drive {
    pub const ALL: [Drive; 6] = [
        Drive::Survival,
        Drive::Hunger,
        Drive::Thirst,
        Drive::Curiosity,
        Drive::Social,
        Drive::Mastery,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Drive::Survival => "survival",
            Drive::Hunger => "hunger",
            Drive::Thirst => "thirst",
            Drive::Curiosity => "curiosity",
            Drive::Social => "social",
            Drive::Mastery => "mastery",
        }
    }

    /// Innate weight: how loudly this drive shouts at equal urgency. Survival
    /// outshouts curiosity — but a very urgent curiosity can still beat a faint
    /// hunger, which is exactly where surprising, lifelike choices come from.
    pub fn salience_weight(self) -> f32 {
        match self {
            Drive::Survival => 2.5,
            Drive::Hunger => 1.4,
            Drive::Thirst => 1.4,
            Drive::Curiosity => 1.0,
            Drive::Social => 0.9,
            Drive::Mastery => 0.7,
        }
    }

    /// Per-tick *additive* creep of this drive when nothing acts on it — the rate
    /// at which an unmet need climbs toward crisis. (Curiosity is excluded: it
    /// relaxes toward a baseline rather than creeping, so it is not something to
    /// anticipate.) Single source of truth for both natural decay and the
    /// anticipatory-homeostasis forecast.
    pub fn creep(self) -> f32 {
        match self {
            Drive::Hunger => 0.012,
            Drive::Thirst => 0.016,
            Drive::Mastery => 0.004,
            Drive::Social => 0.003,
            Drive::Survival | Drive::Curiosity => 0.0,
        }
    }
}

/// A snapshot of urgencies. `BTreeMap` keeps iteration order deterministic so
/// arbitration ties break the same way every run. `bias` is the agent's *learned*
/// re-weighting of each drive (meta-motivation): it can come to value a drive
/// more or less than its innate setting, based on how pursuing it has gone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveSystem {
    levels: BTreeMap<Drive, f32>,
    #[serde(default)]
    bias: BTreeMap<Drive, f32>,
    /// Anticipatory-homeostasis lead time, in ticks. A need is weighed as if it
    /// had already crept forward this many ticks, so the agent forages *before*
    /// the crisis instead of after it. `0` = purely reactive (the default).
    #[serde(default)]
    foresight: f32,
}

impl Default for DriveSystem {
    fn default() -> Self {
        let mut levels = BTreeMap::new();
        let mut bias = BTreeMap::new();
        for d in Drive::ALL {
            // Everyone starts a little curious and a little hungry.
            let v = match d {
                Drive::Curiosity => 0.3,
                Drive::Hunger | Drive::Thirst => 0.15,
                _ => 0.1,
            };
            levels.insert(d, v);
            bias.insert(d, 1.0);
        }
        Self { levels, bias, foresight: 0.0 }
    }
}

impl DriveSystem {
    pub fn level(&self, d: Drive) -> f32 {
        *self.levels.get(&d).unwrap_or(&0.0)
    }

    pub fn set(&mut self, d: Drive, v: f32) {
        self.levels.insert(d, v.clamp(0.0, 1.0));
    }

    /// Add `delta` to a drive, clamping to `[0, 1]`.
    pub fn bump(&mut self, d: Drive, delta: f32) {
        let v = self.level(d) + delta;
        self.set(d, v);
    }

    /// The agent's learned re-weighting of a drive (1.0 = innate).
    pub fn bias(&self, d: Drive) -> f32 {
        *self.bias.get(&d).unwrap_or(&1.0)
    }

    /// Meta-motivation: nudge a drive's learned weight by a multiplicative factor,
    /// clamped so an agent can shift its values but never erase or fixate a drive.
    pub fn nudge_bias(&mut self, d: Drive, factor: f32) {
        let v = (self.bias(d) * factor).clamp(0.35, 2.5);
        self.bias.insert(d, v);
    }

    /// Anticipatory-homeostasis lead time (ticks). `0` = purely reactive.
    pub fn foresight(&self) -> f32 {
        self.foresight
    }

    /// Set the anticipatory lead time. With `ticks > 0` a need is weighed as if it
    /// had crept forward that long, so the agent acts *ahead* of crisis — a small,
    /// computable step toward active inference (minimising *expected* future need).
    pub fn set_foresight(&mut self, ticks: f32) {
        self.foresight = ticks.max(0.0);
    }

    /// Effective pressure of a drive: *anticipated* urgency × innate weight ×
    /// learned bias. The anticipated urgency creeps the level forward by
    /// `foresight` ticks, so imminent needs shout before they are critical.
    pub fn pressure(&self, d: Drive) -> f32 {
        let anticipated = (self.level(d) + self.foresight * d.creep()).min(1.0);
        anticipated * d.salience_weight() * self.bias(d)
    }

    /// The drive currently dominating attention, by effective pressure.
    pub fn dominant(&self) -> (Drive, f32) {
        let mut best = (Drive::Curiosity, f32::MIN);
        for d in Drive::ALL {
            let pressure = self.pressure(d);
            if pressure > best.1 {
                best = (d, pressure);
            }
        }
        best
    }

    /// Iterate urgencies in a stable order (for narration / introspection).
    pub fn iter(&self) -> impl Iterator<Item = (Drive, f32)> + '_ {
        Drive::ALL.into_iter().map(move |d| (d, self.level(d)))
    }

    /// Natural drift each tick when nothing acts on a drive: physiological
    /// needs creep up, satisfied curiosity fades back toward a restless
    /// baseline. This is why an idle Daimon never truly idles.
    pub fn decay(&mut self) {
        self.bump(Drive::Hunger, Drive::Hunger.creep());
        self.bump(Drive::Thirst, Drive::Thirst.creep());
        self.bump(Drive::Mastery, Drive::Mastery.creep());
        // Curiosity sinks slowly toward a baseline of 0.25 so the agent is
        // never fully content — there is always *something* worth a look.
        let c = self.level(Drive::Curiosity);
        self.set(Drive::Curiosity, c + (0.25 - c) * 0.05);
        // Loneliness grows gently.
        self.bump(Drive::Social, Drive::Social.creep());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn survival_dominates_at_equal_urgency() {
        let mut d = DriveSystem::default();
        d.set(Drive::Survival, 0.5);
        d.set(Drive::Curiosity, 0.5);
        assert_eq!(d.dominant().0, Drive::Survival);
    }

    #[test]
    fn intense_curiosity_can_beat_faint_hunger() {
        let mut d = DriveSystem::default();
        d.set(Drive::Hunger, 0.1);
        d.set(Drive::Curiosity, 0.9);
        assert_eq!(d.dominant().0, Drive::Curiosity);
    }

    #[test]
    fn decay_keeps_levels_bounded() {
        let mut d = DriveSystem::default();
        for _ in 0..10_000 {
            d.decay();
        }
        for drive in Drive::ALL {
            let v = d.level(drive);
            assert!((0.0..=1.0).contains(&v), "{drive:?} = {v}");
        }
    }
}
