//! Spatial and entity primitives shared by the world and the mind.

use serde::{Deserialize, Serialize};

/// A grid coordinate. The world is a discrete lattice; embodiment is
/// deliberately simple so the interesting behaviour lives in cognition, not in
/// a physics engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Pos {
    pub x: i32,
    pub y: i32,
}

impl Pos {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Manhattan distance — the world's movement metric.
    pub fn manhattan(self, other: Pos) -> i32 {
        (self.x - other.x).abs() + (self.y - other.y).abs()
    }

    pub fn step(self, dir: Dir) -> Pos {
        let (dx, dy) = dir.delta();
        Pos::new(self.x + dx, self.y + dy)
    }

    /// The cardinal direction that most reduces distance to `target`.
    pub fn toward(self, target: Pos) -> Dir {
        let dx = target.x - self.x;
        let dy = target.y - self.y;
        if dx.abs() >= dy.abs() {
            if dx >= 0 {
                Dir::East
            } else {
                Dir::West
            }
        } else if dy >= 0 {
            Dir::South
        } else {
            Dir::North
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dir {
    North,
    South,
    East,
    West,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::North, Dir::South, Dir::East, Dir::West];

    pub fn delta(self) -> (i32, i32) {
        match self {
            Dir::North => (0, -1),
            Dir::South => (0, 1),
            Dir::East => (1, 0),
            Dir::West => (-1, 0),
        }
    }
}

/// Stable handle for anything in the world the agent can perceive or remember.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntityId(pub u32);

/// What a thing *is*, as far as cognition is concerned. The mind reasons over
/// these categories, not over raw sprites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityKind {
    /// Restores energy when eaten.
    Food,
    /// Restores thirst when drunk.
    Water,
    /// A fellow agent — a subject of theory-of-mind, not just an object.
    Agent,
    /// A hazard that drains health on contact and should be fled.
    Predator,
    /// An interesting, novel object — fuel for curiosity.
    Curio,
}

impl EntityKind {
    /// A coarse affective valence in `[-1, 1]`: how the agent feels about
    /// encountering this kind before any reasoning. Innate, like an instinct.
    pub fn innate_valence(self) -> f32 {
        match self {
            EntityKind::Food | EntityKind::Water => 0.6,
            EntityKind::Agent => 0.2,
            EntityKind::Curio => 0.4,
            EntityKind::Predator => -0.9,
        }
    }
}

/// A perceived entity: a category at a place, plus an opaque payload the world
/// uses (e.g. how much energy a berry holds).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub id: EntityId,
    pub kind: EntityKind,
    pub pos: Pos,
    /// Free-form label for narration and semantic memory ("river", "elder").
    pub label: String,
}

/// The agent's own bodily/internal status, sensed each tick (interoception).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SelfState {
    pub pos: Pos,
    pub health: f32, // 0..1
    pub energy: f32, // 0..1
    pub hydration: f32, // 0..1
    /// How sheltered the agent's cell feels right now (0 = open ground, 1 = fully
    /// ringed by walls/edge). The body's spatial sense of safety — high enclosure
    /// calms the felt threat; low enclosure under a threat drives the urge to wall
    /// in. Default 0 in a world without walls, so non-building runs are unaffected.
    #[serde(default)]
    pub enclosure: f32,
    /// The single best still-open adjacent side to wall next (the direction whose
    /// cell, once walled, most increases enclosure), or `None` when fully enclosed
    /// or no buildable side exists. The agent's sense of *where the gap is* — what
    /// turns "I feel exposed" into a concrete next block to place.
    #[serde(default)]
    pub shelter_gap: Option<Dir>,
    /// OPEN-WORLD interoception (all inert when the world's `open_world` flag is
    /// off — defaults below keep non-open worlds bit-identical). The current
    /// season: `0` Spring · `1` Summer · `2` Autumn · `3` Winter. Default `0`
    /// (eternal spring) so a closed world feels no seasonal pressure.
    #[serde(default)]
    pub season: u8,
    /// Ticks until winter begins (the foresight faculty reads this so a provisioning
    /// mind can act *ahead* of the cold). Large default = "winter is never coming".
    #[serde(default = "winter_never")]
    pub winter_in: f32,
    /// Provisions the body is currently carrying (gathered surplus not yet stored).
    /// Default `0`.
    #[serde(default)]
    pub carrying: f32,
    /// The step toward the nearest harvestable provision source when it is worth
    /// stocking up, or `None`. The agent's sense of *where to gather*.
    #[serde(default)]
    pub gather_dir: Option<Dir>,
    /// The step toward the village granary when carrying a surplus to store, or
    /// `None`. The agent's sense of *where the cache is*.
    #[serde(default)]
    pub store_dir: Option<Dir>,
}

/// Serde default for [`SelfState::winter_in`]: a closed world's winter never comes.
fn winter_never() -> f32 {
    f32::MAX
}

impl SelfState {
    pub fn new(pos: Pos) -> Self {
        Self {
            pos,
            health: 1.0,
            energy: 1.0,
            hydration: 1.0,
            enclosure: 0.0,
            shelter_gap: None,
            season: 0,
            winter_in: f32::MAX,
            carrying: 0.0,
            gather_dir: None,
            store_dir: None,
        }
    }
}
