//! WALKABLE HOUSE INTERIORS — a live-only presentation feature.
//!
//! The minds build a footprint of `walls` cells for shelter (emergent, in the
//! sim). [`view::build_structures`] composes those clusters into the *exterior*
//! buildings you see on the island. This module is the matching INSIDE: when the
//! player walks first-person up to a house's door and presses **E**, we generate
//! that house's own little interior map — a floor, four walls with a door gap,
//! and FURNITURE — and drop the eye inside it.
//!
//! Two hard rules keep the cognition byte-identical:
//!   1. Nothing here ever touches the sim. We only READ a house's footprint
//!      (position / size / era) to *seed* the interior; we never write a sim field.
//!   2. The interior is generated off a dedicated LOCAL [`Rng`] seeded from the
//!      house's identity (its anchor cell), NOT the world rng. So a given house
//!      always yields the exact same room ("the internal map, loaded separately,
//!      keyed by house"), reproducibly, without perturbing the main stream.
//!
//! Coordinate frame: the interior is authored in WORLD units but in its own local
//! frame centred on the origin `(0,0)`, sitting on a flat floor at `y = 0`. The
//! first-person eye walks within it and is clamped to the room bounds (wall
//! collision). The island terrain/water are suppressed while inside (the renderer
//! sees `world_dims = (0,0)` and skips the sea), so you see only the room.

use crate::geo::{self, pieces, LitVertex};
use crate::sim::{Era, GameWorld};
use daimon_core::Pos;

/// A stable handle to one house on the island — the "address" of an internal map.
/// Derived deterministically from the same clustering [`view::build_structures`]
/// uses, so house index `N` here is the same building you see rendered.
#[derive(Clone, Copy)]
pub struct HouseRef {
    /// Stable per-house identity (its anchor cell hashed) — the interior seed.
    pub id: u64,
    /// Footprint centre in world (sim) coords.
    pub cx: f32,
    pub cz: f32,
    /// Ground height the building sits on (so we exit back onto the right level).
    pub base_y: f32,
    /// The door's world position (just outside the south wall) + facing, so the
    /// FP "near a door?" test and the re-entry placement agree with the exterior.
    pub door_x: f32,
    pub door_z: f32,
    /// Footprint extent (cells) — drives how big/fancy the interior is.
    pub span: f32,
    pub area: f32,
    /// The era this house wears (its material palette inside, too).
    pub era: Era,
}

/// One piece of furniture, already reduced to a kind + placement. Built into
/// geometry in [`Interior::build`]. Placement is in the interior's LOCAL frame.
#[derive(Clone, Copy)]
enum Furniture {
    /// A table (top + four legs) at (x,z), half-size hw×hd.
    Table { x: f32, z: f32, hw: f32, hd: f32 },
    /// A stool/chair at (x,z).
    Chair { x: f32, z: f32 },
    /// A bed (frame + mattress + pillow) at (x,z), facing +x if `along_x`.
    Bed { x: f32, z: f32, along_x: bool },
    /// A hearth/fireplace against a wall at (x,z) — glows warm.
    Hearth { x: f32, z: f32 },
    /// A chest at (x,z).
    Chest { x: f32, z: f32 },
    /// A bookshelf/cupboard against a wall at (x,z), facing `face` (0=+z back wall…).
    Shelf { x: f32, z: f32, along_x: bool },
    /// A floor rug at (x,z), half-size hw×hd.
    Rug { x: f32, z: f32, hw: f32, hd: f32 },
}

/// A generated, walkable interior map for one house.
pub struct Interior {
    /// Which house this is the inside of (so exit returns to the right door).
    pub house: HouseRef,
    /// Interior half-extents (local frame): the room spans ±hx by ±hz.
    pub hx: f32,
    pub hz: f32,
    /// Wall height (world units).
    pub wall_h: f32,
    /// The door gap is centred on the +z (south) wall, half-width `door_hw`.
    pub door_hw: f32,
    furniture: Vec<Furniture>,
    /// Era palette for the interior surfaces.
    style: pieces::EraStyle,
    /// Warm hearth glow positions (local frame), lit additively each frame.
    hearths: Vec<[f32; 3]>,
}

/// A tiny local SplitMix RNG — completely independent of the sim stream. Used to
/// place furniture deterministically per-house without touching the world rng.
struct LocalRng(u64);
impl LocalRng {
    fn new(seed: u64) -> Self {
        // mix the seed once so adjacent house ids don't give near-identical rooms.
        LocalRng(seed ^ 0x5DEE_CE66_D1B2_F00D)
    }
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// A unit float in [0,1).
    fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
    /// A bool with probability `p`.
    fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }
}

fn era_style(era: Era) -> pieces::EraStyle {
    match era {
        Era::Stone => pieces::ERA_STONE,
        Era::Bronze => pieces::ERA_BRONZE,
        Era::Iron => pieces::ERA_IRON,
        Era::Industrial => pieces::ERA_INDUSTRIAL,
        Era::Space => pieces::ERA_SPACE,
    }
}

/// Replicate the deterministic 8-connected clustering [`view::build_structures`]
/// uses, but emit only each house's IDENTITY + placement (no geometry). House
/// index `N` here is exactly the N-th building rendered on the island, so the
/// `?interior=N` seam and the door-proximity test address the same houses.
///
/// LIVE-ONLY and read-only: it never mutates the sim. Returns `[]` if no walls.
pub fn house_anchors(world: &GameWorld) -> Vec<HouseRef> {
    use std::collections::HashSet;
    if world.walls.is_empty() {
        return Vec::new();
    }
    let cells: &HashSet<Pos> = &world.walls;
    let inside = |p: Pos| cells.contains(&p);
    let mut sorted: Vec<Pos> = cells.iter().copied().collect();
    sorted.sort_by_key(|p| (p.y, p.x));
    let mut seen: HashSet<Pos> = HashSet::new();
    let mut out = Vec::new();

    for &start in &sorted {
        if seen.contains(&start) {
            continue;
        }
        let mut stack = vec![start];
        let mut comp: Vec<Pos> = Vec::new();
        seen.insert(start);
        while let Some(p) = stack.pop() {
            comp.push(p);
            let mut nbrs = Vec::new();
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let n = Pos::new(p.x + dx, p.y + dy);
                    if cells.contains(&n) && !seen.contains(&n) {
                        nbrs.push(n);
                    }
                }
            }
            nbrs.sort_by_key(|q| (q.y, q.x));
            for n in nbrs {
                if seen.insert(n) {
                    stack.push(n);
                }
            }
        }

        let (mut x0, mut x1, mut z0, mut z1) = (i32::MAX, i32::MIN, i32::MAX, i32::MIN);
        for p in &comp {
            x0 = x0.min(p.x);
            x1 = x1.max(p.x);
            z0 = z0.min(p.y);
            z1 = z1.max(p.y);
        }
        let max_axis: i32 = if world.materials_econ { 7 } else { 5 };
        if x1 - x0 + 1 > max_axis {
            let cm = (x0 + x1) / 2;
            x0 = cm - max_axis / 2;
            x1 = x0 + max_axis - 1;
        }
        if z1 - z0 + 1 > max_axis {
            let cm = (z0 + z1) / 2;
            z0 = cm - max_axis / 2;
            z1 = z0 + max_axis - 1;
        }
        let anchor = comp.iter().min_by_key(|p| (p.y, p.x)).copied().unwrap();
        let id = ((anchor.x as i64) << 20 ^ anchor.y as i64) as u64;
        let bw = (x1 - x0 + 1) as f32;
        let bd = (z1 - z0 + 1) as f32;
        let span = bw.max(bd);
        let area = bw * bd;

        // base footing height = lowest ground under the footprint (matches the
        // exterior's level pad).
        let mut base_y = f32::MAX;
        for p in &comp {
            base_y = base_y.min(geo::ground_height(world.w, world.h, p.x as f32, p.y as f32));
        }
        if base_y == f32::MAX {
            base_y = 0.0;
        }

        // door cell: the most-south outward cell (matches build_structures).
        let door_cell = comp
            .iter()
            .filter(|p| !inside(Pos::new(p.x, p.y + 1)))
            .max_by_key(|p| (p.y, p.x))
            .copied()
            .unwrap_or(anchor);
        // the exterior door sits on the south face of the south-edge cell nearest
        // the door cell; its world position is just outside that wall.
        let door_world_x = door_cell.x as f32;
        let door_world_z = z1 as f32 + 0.5;

        out.push(HouseRef {
            id,
            cx: (x0 + x1) as f32 * 0.5,
            cz: (z0 + z1) as f32 * 0.5,
            base_y,
            door_x: door_world_x,
            door_z: door_world_z,
            span,
            area,
            era: world.era_at(Pos::new((x0 + x1) / 2, (z0 + z1) / 2)),
        });
    }
    out
}

impl Interior {
    /// Generate the interior map for a house — its room dimensions + furniture —
    /// deterministically from the house's identity. The SAME house always yields
    /// the SAME room (the "internal map loaded separately, keyed by house"). Bigger
    /// / later-era houses get larger rooms and more, finer furniture.
    pub fn for_house(house: HouseRef) -> Self {
        let mut rng = LocalRng::new(house.id);
        let style = era_style(house.era);

        // Room size scales with the footprint, but is always a comfortable walk-in
        // size (never a cupboard). Span ~3..7 cells maps to a ~5..9 unit room.
        let s = house.span.clamp(2.0, 7.0);
        let hx = (2.2 + 0.55 * s + rng.unit() * 0.6).clamp(2.2, 5.2);
        let hz = (2.2 + 0.55 * s + rng.unit() * 0.6).clamp(2.2, 5.2);
        let wall_h = 2.4 + 0.25 * rng.unit();
        let door_hw = 0.55;

        // "fanciness" 0..1 — drives how much furniture: footprint size + era rung.
        let era_rung = match house.era {
            Era::Stone => 0.0,
            Era::Bronze => 0.25,
            Era::Iron => 0.5,
            Era::Industrial => 0.75,
            Era::Space => 1.0,
        };
        let size_f = ((house.area - 4.0) / 24.0).clamp(0.0, 1.0);
        let fancy = (0.45 * size_f + 0.55 * era_rung).clamp(0.0, 1.0);

        let mut furniture = Vec::new();
        let mut hearths = Vec::new();

        // margin to keep furniture off the walls.
        let m = 0.55f32;
        let ix = hx - m;
        let iz = hz - m;

        // 1) HEARTH against the back (-z) wall, slightly off-centre. Always present —
        // it's the heart of the home and the warm light source inside.
        let hearth_x = (rng.unit() - 0.5) * ix * 0.8;
        let hearth_z = -iz + 0.15;
        furniture.push(Furniture::Hearth { x: hearth_x, z: hearth_z });
        hearths.push([hearth_x, 0.0, hearth_z]);

        // 2) RUG in the middle (a softer floor read). Bigger homes get a rug.
        if fancy > 0.2 {
            furniture.push(Furniture::Rug {
                x: 0.0,
                z: 0.2,
                hw: (ix * 0.5).min(1.4),
                hd: (iz * 0.45).min(1.2),
            });
        }

        // 3) TABLE + CHAIRS near the centre. A home always has a table.
        let tx = (rng.unit() - 0.5) * ix * 0.4;
        let tz = 0.2 + (rng.unit() - 0.5) * 0.4;
        let thw = (0.55 + 0.2 * fancy).min(ix * 0.4);
        let thd = (0.40 + 0.15 * fancy).min(iz * 0.4);
        furniture.push(Furniture::Table { x: tx, z: tz, hw: thw, hd: thd });
        // chairs around the table: 2 base, up to 4 when fancy.
        let n_chairs = 2 + (fancy * 2.0).round() as i32;
        let chair_spots = [
            (tx, tz - thd - 0.45),
            (tx, tz + thd + 0.45),
            (tx - thw - 0.45, tz),
            (tx + thw + 0.45, tz),
        ];
        for spot in chair_spots.iter().take(n_chairs.clamp(2, 4) as usize) {
            // keep chairs inside the room.
            if spot.0.abs() < ix && spot.1.abs() < iz {
                furniture.push(Furniture::Chair { x: spot.0, z: spot.1 });
            }
        }

        // 4) BED in a corner (the -x,+z corner — away from the door gap centre).
        let along_x = rng.chance(0.5);
        furniture.push(Furniture::Bed {
            x: -ix + if along_x { 0.9 } else { 0.55 },
            z: -iz + if along_x { 0.55 } else { 0.9 },
            along_x,
        });

        // 5) CHEST against a side wall.
        furniture.push(Furniture::Chest {
            x: ix - 0.3,
            z: -iz + 0.6 + rng.unit() * (iz * 0.6),
        });

        // 6) SHELF / cupboard — fancier homes get one (or two).
        if fancy > 0.35 {
            furniture.push(Furniture::Shelf { x: ix - 0.18, z: 0.3, along_x: false });
        }
        if fancy > 0.7 {
            furniture.push(Furniture::Shelf { x: -ix + 0.18, z: 0.4, along_x: false });
        }

        // 7) A second small hearth/brazier glow for the largest, latest homes (more
        // light, reads as a grander hall).
        if fancy > 0.8 {
            let bx = ix - 0.5;
            let bz = iz - 0.5;
            hearths.push([bx, 0.0, bz]);
        }

        Interior {
            house,
            hx,
            hz,
            wall_h,
            door_hw,
            furniture,
            style,
            hearths,
        }
    }

    /// The interior's flat floor level in WORLD y. We author at y=0 locally, so this
    /// is just 0 — kept as a method so the camera/eye code reads clearly.
    pub fn floor_y(&self) -> f32 {
        0.0
    }

    /// Clamp a local (x,z) walk position to stay inside the room walls (collision).
    /// Standing radius `r` keeps the eye off the surfaces. The door gap on the +z
    /// wall lets you stand in the threshold (so E-to-exit there reads naturally).
    pub fn clamp_walk(&self, x: f32, z: f32, r: f32) -> (f32, f32) {
        let lim_x = self.hx - r;
        let mut cx = x.clamp(-lim_x, lim_x);
        let lim_z = self.hz - r;
        // The +z wall has a door gap; if you're within the gap, you may step a touch
        // past the wall line (into the threshold) but not out of the world.
        let in_gap = cx.abs() < self.door_hw;
        let max_z = if in_gap { self.hz + 0.4 } else { lim_z };
        let mut cz = z.clamp(-lim_z, max_z);
        // keep x clamped after the gap check, too.
        cx = cx.clamp(-lim_x, lim_x);
        if !in_gap {
            cz = cz.clamp(-lim_z, lim_z);
        }
        (cx, cz)
    }

    /// True when the local (x,z) is at the inside of the door gap — close enough to
    /// the +z threshold and within the gap width — so pressing E there EXITS.
    pub fn at_door(&self, x: f32, z: f32) -> bool {
        x.abs() < self.door_hw + 0.2 && z > self.hz - 0.9
    }

    /// Build the interior geometry into the scene (floor, walls with a door gap,
    /// ceiling beams, and all the furniture). Authored in the local frame (origin
    /// centred, floor at y=0). Returns the warm hearth glow points to add as
    /// additive billboards by the caller.
    pub fn build(&self, lit: &mut Vec<LitVertex>) -> &[[f32; 3]] {
        let st = &self.style;
        let (hx, hz, h) = (self.hx, self.hz, self.wall_h);
        let wt = 0.12; // wall thickness

        // FLOOR — warm planking across the whole room (a touch below 0 so furniture
        // legs sit on it cleanly).
        pieces::floor_slab(lit, -hx, -hz, hx, hz, -0.04, 0.08, pieces::FLOOR);

        // CEILING — a darker slab overhead so it reads as enclosed (not the sky).
        let ceil_c = mul(st.wall2, 0.7);
        pieces::floor_slab(lit, -hx, -hz, hx, hz, h, 0.10, ceil_c);

        // four WALLS. The +z (south) wall has a centred door GAP; the others are
        // solid. Walls are simple boxes (two-tone era stone), with a skirting band.
        let wall_c = st.wall;
        let skirt_c = mul(st.wall2, 0.85);
        // -z (back) wall
        push_wall_x(lit, -hx, hx, -hz, h, wt, wall_c, skirt_c);
        // -x (left) and +x (right) walls
        push_wall_z(lit, -hz, hz, -hx, h, wt, wall_c, skirt_c);
        push_wall_z(lit, -hz, hz, hx, h, wt, wall_c, skirt_c);
        // +z (front) wall with a door gap: two segments left/right of the gap, plus
        // a lintel over the opening.
        let g = self.door_hw;
        push_wall_x(lit, -hx, -g, hz, h, wt, wall_c, skirt_c); // left of door
        push_wall_x(lit, g, hx, hz, h, wt, wall_c, skirt_c); // right of door
        // lintel over the door opening (top portion of the gap).
        let door_h = (h * 0.78).min(h - 0.2);
        push_box(
            lit,
            [0.0, door_h + (h - door_h) * 0.5, hz],
            [g, (h - door_h) * 0.5, wt * 0.5],
            0.0,
            wall_c,
        );
        // a warm timber door-frame around the opening + an open door leaf swung in.
        let frame = pieces::TIMBER;
        for s in [-1.0f32, 1.0] {
            push_box(lit, [s * g, door_h * 0.5, hz], [0.05, door_h * 0.5, wt * 0.6], 0.0, frame);
        }
        // the door leaf, hinged on the left jamb and swung ~80° open into the room.
        let leaf_w = g - 0.06;
        let hinge_x = -g + 0.05;
        let ang = 1.4f32; // ~80°, open inward
        let (sa, ca) = ang.sin_cos();
        let lcx = hinge_x + leaf_w * ca;
        let lcz = hz - leaf_w * sa;
        push_box(lit, [lcx, door_h * 0.5, lcz], [leaf_w, door_h * 0.5, 0.04], ang, frame);

        // a couple of CEILING BEAMS for structure (run along x).
        let beam_c = pieces::TIMBER;
        for k in 0..3 {
            let bz = -hz * 0.6 + k as f32 * hz * 0.6;
            push_box(lit, [0.0, h - 0.12, bz], [hx, 0.06, 0.06], 0.0, beam_c);
        }

        // FURNITURE.
        for f in &self.furniture {
            self.build_furniture(lit, *f);
        }

        &self.hearths
    }

    fn build_furniture(&self, lit: &mut Vec<LitVertex>, f: Furniture) {
        let st = &self.style;
        let oak = pieces::TIMBER;
        let oak_lt = pieces::TIMBER_LT;
        match f {
            Furniture::Table { x, z, hw, hd } => {
                let top_y = 0.62;
                // table top
                push_box(lit, [x, top_y, z], [hw, 0.04, hd], 0.0, oak_lt);
                // four legs
                for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                    let lx = x + sx * (hw - 0.07);
                    let lz = z + sz * (hd - 0.07);
                    push_box(lit, [lx, top_y * 0.5 - 0.02, lz], [0.05, top_y * 0.5, 0.05], 0.0, oak);
                }
            }
            Furniture::Chair { x, z } => {
                let seat_y = 0.34;
                push_box(lit, [x, seat_y, z], [0.16, 0.04, 0.16], 0.0, oak_lt);
                for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                    push_box(
                        lit,
                        [x + sx * 0.12, seat_y * 0.5, z + sz * 0.12],
                        [0.03, seat_y * 0.5, 0.03],
                        0.0,
                        oak,
                    );
                }
                // a low backrest on the -z side
                push_box(lit, [x, seat_y + 0.18, z - 0.13], [0.16, 0.18, 0.03], 0.0, oak);
            }
            Furniture::Bed { x, z, along_x } => {
                let (hw, hd) = if along_x { (0.95, 0.55) } else { (0.55, 0.95) };
                // frame
                push_box(lit, [x, 0.18, z], [hw, 0.10, hd], 0.0, oak);
                // mattress (pale linen)
                let linen = [0.82, 0.78, 0.68, 1.0];
                push_box(lit, [x, 0.30, z], [hw - 0.06, 0.06, hd - 0.06], 0.0, linen);
                // pillow at one end + a coloured blanket band.
                let pillow = [0.92, 0.90, 0.84, 1.0];
                let blanket = mul(st.glass, 0.5);
                if along_x {
                    push_box(lit, [x - hw + 0.22, 0.37, z], [0.18, 0.05, hd - 0.10], 0.0, pillow);
                    push_box(lit, [x + 0.25, 0.37, z], [hw - 0.45, 0.04, hd - 0.06], 0.0, blanket);
                } else {
                    push_box(lit, [x, 0.37, z - hd + 0.22], [hw - 0.10, 0.05, 0.18], 0.0, pillow);
                    push_box(lit, [x, 0.37, z + 0.25], [hw - 0.06, 0.04, hd - 0.45], 0.0, blanket);
                }
            }
            Furniture::Hearth { x, z } => {
                // a stone fireplace box against the wall, with a dark fire cavity and
                // a couple of log billets. The warm glow itself is added by the view.
                let stone = pieces::STONE;
                let dark = [0.10, 0.07, 0.05, 1.0];
                push_box(lit, [x, 0.45, z], [0.5, 0.45, 0.22], 0.0, stone);
                // fire cavity (recessed dark)
                push_box(lit, [x, 0.30, z + 0.12], [0.30, 0.26, 0.12], 0.0, dark);
                // logs
                let ember = [0.6, 0.22, 0.08, 1.0];
                push_box(lit, [x - 0.10, 0.16, z + 0.14], [0.18, 0.05, 0.05], 0.4, ember);
                push_box(lit, [x + 0.10, 0.16, z + 0.14], [0.18, 0.05, 0.05], -0.4, ember);
                // a mantel shelf + a stub chimney up to the ceiling.
                push_box(lit, [x, 0.92, z], [0.55, 0.04, 0.26], 0.0, oak_lt);
                push_box(lit, [x, (0.92 + self.wall_h) * 0.5, z - 0.02], [0.30, (self.wall_h - 0.92) * 0.5, 0.18], 0.0, stone);
            }
            Furniture::Chest { x, z } => {
                push_box(lit, [x, 0.16, z], [0.26, 0.16, 0.18], 0.0, oak);
                // a domed/banded lid hint + a brass clasp.
                push_box(lit, [x, 0.34, z], [0.27, 0.04, 0.19], 0.0, oak_lt);
                push_box(lit, [x, 0.30, z + 0.18], [0.04, 0.05, 0.02], 0.0, [0.7, 0.6, 0.25, 1.0]);
            }
            Furniture::Shelf { x, z, along_x } => {
                let (hw, hd) = if along_x { (0.45, 0.12) } else { (0.12, 0.45) };
                let height = 1.4;
                // the cabinet body
                push_box(lit, [x, height * 0.5, z], [hw, height * 0.5, hd], 0.0, oak);
                // 3 shelves of "books" (coloured spines) — derived from era glass tone.
                for sh in 0..3 {
                    let sy = 0.35 + sh as f32 * 0.42;
                    let bc = [
                        0.4 + 0.2 * (sh as f32 * 1.3).sin().abs(),
                        0.28,
                        0.20 + 0.15 * sh as f32,
                        1.0,
                    ];
                    push_box(lit, [x, sy, z], [hw - 0.03, 0.10, hd - 0.03], 0.0, bc);
                }
            }
            Furniture::Rug { x, z, hw, hd } => {
                let rug = mul(st.glass, 0.4);
                let border = mul(st.trim, 1.1);
                // a thin slab just above the floor with a darker border ring.
                push_box(lit, [x, 0.005, z], [hw, 0.006, hd], 0.0, border);
                push_box(lit, [x, 0.008, z], [hw - 0.08, 0.006, hd - 0.08], 0.0, rug);
            }
        }
    }
}

// --- small local geometry helpers (kept here so the module is self-contained) ---

fn push_box(out: &mut Vec<LitVertex>, c: [f32; 3], half: [f32; 3], yaw: f32, color: [f32; 4]) {
    geo::push_box(out, c, half, yaw, color);
}

fn mul(c: [f32; 4], k: f32) -> [f32; 4] {
    [c[0] * k, c[1] * k, c[2] * k, c[3]]
}

/// A wall running along X from x0..x1 at fixed z, height h, thickness t. Two-tone
/// (a darker skirting band at the foot) so the surfaces read.
fn push_wall_x(
    out: &mut Vec<LitVertex>,
    x0: f32,
    x1: f32,
    z: f32,
    h: f32,
    t: f32,
    c: [f32; 4],
    skirt: [f32; 4],
) {
    let cx = (x0 + x1) * 0.5;
    let hw = (x1 - x0) * 0.5;
    if hw <= 0.0 {
        return;
    }
    push_box(out, [cx, h * 0.5, z], [hw, h * 0.5, t * 0.5], 0.0, c);
    push_box(out, [cx, 0.12, z + t * 0.4], [hw, 0.12, t * 0.2], 0.0, skirt);
}

/// A wall running along Z from z0..z1 at fixed x.
fn push_wall_z(
    out: &mut Vec<LitVertex>,
    z0: f32,
    z1: f32,
    x: f32,
    h: f32,
    t: f32,
    c: [f32; 4],
    skirt: [f32; 4],
) {
    let cz = (z0 + z1) * 0.5;
    let hd = (z1 - z0) * 0.5;
    if hd <= 0.0 {
        return;
    }
    push_box(out, [x, h * 0.5, cz], [t * 0.5, h * 0.5, hd], 0.0, c);
    push_box(out, [x + t * 0.4, 0.12, cz], [t * 0.2, 0.12, hd], 0.0, skirt);
}
