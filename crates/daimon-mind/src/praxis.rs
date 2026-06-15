//! Praxis — emergent concept and goal genesis. The autonomy frontier.
//!
//! Everywhere else, the agent is autonomous *inside a vocabulary the designer
//! gave it*: it knows `Food` means eat, `Predator` means flee. Praxis removes
//! that ceiling. Here the agent is handed only a **perceptual fingerprint** of a
//! thing — never its designer-meaning — and must:
//!
//! 1. **invent concepts** by clustering fingerprints it has seen (online leader
//!    clustering): things that look alike become one self-named *form*;
//! 2. **learn affordances** by attributing changes in its own body to being
//!    *near* a form (a learned, causal-ish model: "while I was beside form-β my
//!    health rose");
//! 3. **invent goals** from those affordances — pursuables that were never
//!    coded ("form-β mends me; seek it when hurt").
//!
//! The consequence is the thing "proper autonomy" actually requires: drop in an
//! entity the architecture has no idea about, and the agent can still carve it
//! into a concept, discover what it's good for, and decide on its own to use it.
//! Nothing in the drive system, the planner, or the goal set mentions the new
//! thing. Only lived experience does.

use daimon_core::{Entity, SelfState};
use serde::{Deserialize, Serialize};

/// Fingerprint dimensionality (a stand-in for perceptual features).
const K: usize = 3;

/// Strip a trailing `-<digits>` so instances of a type share a fingerprint
/// ("berries-3" and "berries-5" look alike) while types differ.
fn base_label(label: &str) -> &str {
    match label.rsplit_once('-') {
        Some((head, tail)) if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) => head,
        _ => label,
    }
}

fn fnv(s: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

/// The agent's perceptual fingerprint of an entity — derived from how it *looks*
/// (a coarse kind-channel plus a label-keyed signature), never its meaning.
pub fn fingerprint(e: &Entity) -> [f32; K] {
    use daimon_core::EntityKind::*;
    let kind_channel = match e.kind {
        Food => 0.12,
        Water => 0.34,
        Curio => 0.58,
        Agent => 0.79,
        Predator => 0.97,
    };
    let h = fnv(base_label(&e.label));
    [
        kind_channel,
        (h & 0xffff) as f32 / 65535.0,
        ((h >> 16) & 0xffff) as f32 / 65535.0,
    ]
}

fn dist2(a: &[f32; K], b: &[f32; K]) -> f32 {
    (0..K).map(|i| (a[i] - b[i]).powi(2)).sum()
}

/// A self-invented category, with a learned sense of what it does to the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Concept {
    pub proto: [f32; K],
    pub name: String,
    pub seen: u32,
    /// Mean per-tick body deltas observed while *engaged* (adjacent) with it.
    pub d_energy: f32,
    pub d_hydration: f32,
    pub d_health: f32,
    pub engagements: u32,
}

impl Concept {
    /// A short, learned epithet — what the agent has come to feel this form is.
    pub fn epithet(&self) -> &'static str {
        if self.engagements < 3 {
            "unknown"
        } else if self.d_health > 0.015 {
            "it mends me"
        } else if self.d_energy > 0.05 {
            "it feeds me"
        } else if self.d_hydration > 0.05 {
            "it quenches me"
        } else if self.d_health < -0.05 {
            "it harms me"
        } else {
            "harmless"
        }
    }
    pub fn mends(&self) -> bool {
        self.engagements >= 3 && self.d_health > 0.015
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Praxis {
    pub concepts: Vec<Concept>,
    radius2: f32,
    /// Engagement carried from last tick: (concept idx, body snapshot) to which
    /// we'll attribute this tick's body change.
    pending: Option<(usize, SelfState)>,
    coined: u32,
}

impl Default for Praxis {
    fn default() -> Self {
        Self {
            concepts: Vec::new(),
            radius2: 0.02, // clustering tightness in fingerprint space
            pending: None,
            coined: 0,
        }
    }
}

impl Praxis {
    /// Assign a fingerprint to an existing concept, or invent a new one.
    fn assign(&mut self, fp: [f32; K]) -> usize {
        let mut best = (usize::MAX, f32::INFINITY);
        for (i, c) in self.concepts.iter().enumerate() {
            let d = dist2(&fp, &c.proto);
            if d < best.1 {
                best = (i, d);
            }
        }
        if best.1 <= self.radius2 {
            // nudge the prototype toward the new exemplar
            let c = &mut self.concepts[best.0];
            for (p, f) in c.proto.iter_mut().zip(fp.iter()) {
                *p += (f - *p) * 0.15;
            }
            c.seen += 1;
            best.0
        } else {
            self.coined += 1;
            let names = ["α", "β", "γ", "δ", "ε", "ζ", "η", "θ", "ι", "κ"];
            let name = format!("form-{}", names.get((self.coined as usize - 1) % names.len()).unwrap_or(&"?"));
            self.concepts.push(Concept {
                proto: fp,
                name,
                seen: 1,
                d_energy: 0.0,
                d_hydration: 0.0,
                d_health: 0.0,
                engagements: 0,
            });
            self.concepts.len() - 1
        }
    }

    /// Read-only classification (no learning) — which known concept a thing is.
    pub fn classify(&self, e: &Entity) -> Option<usize> {
        let fp = fingerprint(e);
        let mut best = (usize::MAX, f32::INFINITY);
        for (i, c) in self.concepts.iter().enumerate() {
            let d = dist2(&fp, &c.proto);
            if d < best.1 {
                best = (i, d);
            }
        }
        if best.1 <= self.radius2 {
            Some(best.0)
        } else {
            None
        }
    }

    /// One perception step: cluster everything visible into concepts, then learn
    /// the affordance of whatever the agent is *engaged* with (adjacent to).
    pub fn observe(&mut self, visible: &[Entity], me_pos: daimon_core::Pos, body: SelfState) {
        // form/refresh concepts for everything seen
        for e in visible {
            let _ = self.assign(fingerprint(e));
        }
        // what are we engaged with *right now* (nearest adjacent thing)?
        let current = visible
            .iter()
            .filter(|e| e.pos.manhattan(me_pos) <= 1)
            .min_by_key(|e| e.pos.manhattan(me_pos))
            .and_then(|e| self.classify(e));

        // Attribute last tick's body change to a concept ONLY under *continuous*
        // contact with the same form. This is both more causally honest and
        // robust to teleports/discontinuities (a body jump between unrelated ticks
        // must never be blamed on a form we merely stood near once).
        if let (Some((idx, before)), Some(cur)) = (self.pending.take(), current) {
            if idx == cur {
                let de = (body.energy - before.energy).clamp(-0.25, 0.25);
                let dh = (body.hydration - before.hydration).clamp(-0.25, 0.25);
                let dhp = (body.health - before.health).clamp(-0.25, 0.25);
                let c = &mut self.concepts[idx];
                c.engagements += 1;
                let n = c.engagements as f32;
                c.d_energy += (de - c.d_energy) / n;
                c.d_hydration += (dh - c.d_hydration) / n;
                c.d_health += (dhp - c.d_health) / n;
            }
        }
        // carry this tick's engagement forward (cleared if not engaged).
        self.pending = current.map(|idx| (idx, body));
    }

    /// The first concept the agent has learned *mends* it, if any.
    pub fn mending_concept(&self) -> Option<usize> {
        self.concepts.iter().position(|c| c.mends())
    }

    /// The concept most worth teaching a peer: the best-engaged one carrying a
    /// clear, useful (or dangerous) affordance. Cultural transmission passes this.
    pub fn teachable(&self) -> Option<&Concept> {
        self.concepts
            .iter()
            .filter(|c| {
                c.engagements >= 3
                    && (c.d_health.abs() > 0.015 || c.d_energy > 0.05 || c.d_hydration > 0.05)
            })
            .max_by_key(|c| c.engagements)
    }

    /// Adopt a peer's concept affordance as a *probationary* belief — learning a
    /// form's meaning from someone else without having touched it (cumulative
    /// culture). Returns true if newly adopted. The agent's own later contact
    /// refines these deltas via [`observe`]'s running average, so a *false* meme
    /// is corrected by experience — the learning-progress gate (Cook et al. 2024:
    /// social learning must be balanced by independent competence gain).
    pub fn adopt(&mut self, c: &Concept) -> bool {
        let m = self.concepts.iter().position(|k| dist2(&k.proto, &c.proto) <= self.radius2);
        match m {
            // already well-learned from own experience — trust that, don't overwrite.
            Some(i) if self.concepts[i].engagements >= 3 => false,
            // know the form but not its affordance — accept the peer's, probationally.
            Some(i) => {
                let k = &mut self.concepts[i];
                k.d_energy = c.d_energy;
                k.d_hydration = c.d_hydration;
                k.d_health = c.d_health;
                k.engagements = k.engagements.max(3); // enough to act on; contact will refine
                true
            }
            // unknown form — adopt the concept wholesale (still probationary).
            None => {
                let mut nc = c.clone();
                nc.engagements = 3;
                nc.seen = nc.seen.max(1);
                self.concepts.push(nc);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daimon_core::{EntityId, EntityKind, Pos};

    fn ent(id: u32, kind: EntityKind, label: &str, x: i32, y: i32) -> Entity {
        Entity { id: EntityId(id), kind, pos: Pos::new(x, y), label: label.into() }
    }

    #[test]
    fn invents_distinct_concepts_and_generalises_instances() {
        let mut p = Praxis::default();
        let body = SelfState::new(Pos::new(0, 0));
        // two berries (same type), one wellspring (novel type)
        p.observe(
            &[
                ent(1, EntityKind::Food, "berries-0", 9, 9),
                ent(2, EntityKind::Food, "berries-1", 9, 9),
                ent(3, EntityKind::Curio, "wellspring", 9, 9),
            ],
            Pos::new(0, 0),
            body,
        );
        // berries collapse to one concept; wellspring is its own.
        assert_eq!(p.concepts.len(), 2, "got {:?}", p.concepts.iter().map(|c| &c.name).collect::<Vec<_>>());
    }

    #[test]
    fn learns_a_novel_healing_affordance_with_no_builtin_knowledge() {
        let mut p = Praxis::default();
        let well = ent(3, EntityKind::Curio, "wellspring", 5, 5);
        let mut body = SelfState { pos: Pos::new(5, 5), health: 0.4, energy: 0.9, hydration: 0.9 };
        // stand beside the wellspring; the (simulated) world heals on proximity.
        for _ in 0..6 {
            p.observe(std::slice::from_ref(&well), Pos::new(5, 5), body);
            body.health = (body.health + 0.05).min(1.0); // world's hidden effect
        }
        let m = p.mending_concept().expect("should have learned a mending form");
        assert!(p.concepts[m].mends());
        assert_eq!(p.concepts[m].epithet(), "it mends me");
    }
}
