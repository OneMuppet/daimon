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
}

impl SelfState {
    pub fn new(pos: Pos) -> Self {
        Self {
            pos,
            health: 1.0,
            energy: 1.0,
            hydration: 1.0,
        }
    }
}
