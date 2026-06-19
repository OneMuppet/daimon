//! Turns the world + cognition into a luminous 3-D isometric scene.
//!
//! Two registers: the **world** — a displaced island under a moving sky, with the
//! six minds as little glowing figures, resources as soft light-sources, the
//! stalker as a dark wolf — all real geometry rendered low-res and upscaled for a
//! painted-diorama look; and the **HUD** (screen-space), whose centrepiece is the
//! *mind inspector*. The inspector is the whole point: it makes the architecture
//! visible while it runs.

use crate::geo::{self, LitVertex};
use crate::math::{self, Mat4, Vec3};
use crate::scene::{Color, Scene, Sky};
use crate::sim::GameWorld;
use daimon_core::{Drive, EntityKind, Pos};
use daimon_mind::Process;

const TAU: f32 = std::f32::consts::TAU;

/// The isometric god-view camera: centre (sim cells), vertical zoom half-extent
/// (world units), and yaw.
pub struct Camera {
    pub cx: f32,
    pub cy: f32,
    pub zoom: f32,
    pub yaw: f32,
}

impl Camera {
    pub fn new(cx: f32, cy: f32) -> Self {
        Camera { cx, cy, zoom: 12.0, yaw: math::ISO_YAW_DEG.to_radians() }
    }

    fn target_y(&self, w: i32, h: i32) -> f32 {
        geo::ground_height(w, h, self.cx, self.cy) + 0.4
    }

    pub fn view_proj(&self, w: i32, h: i32, aspect: f32) -> Mat4 {
        math::iso_view_proj(self.cx, self.cy, self.target_y(w, h), self.zoom, aspect, self.yaw)
    }

    pub fn eye(&self, w: i32, h: i32) -> [f32; 3] {
        let (e, _, _, _) = math::iso_basis(self.cx, self.cy, self.target_y(w, h), self.yaw);
        [e.x, e.y, e.z]
    }

    /// Cursor pixel → sim ground coords (picking / feeding).
    pub fn pick(&self, px: f32, py: f32, sw: f32, sh: f32, w: i32, h: i32) -> (f32, f32) {
        let ndc = [px / sw * 2.0 - 1.0, 1.0 - py / sh * 2.0];
        let plane_y = geo::ground_height(w, h, self.cx, self.cy) + 0.3;
        math::screen_to_ground(ndc, self.cx, self.cy, self.target_y(w, h), self.zoom, sw / sh, plane_y, self.yaw)
    }
}

pub struct Hud {
    pub paused: bool,
    pub speed: f32,
    pub quantum: bool,
}

fn drive_color(d: Drive) -> Color {
    match d {
        Drive::Survival => Color::hex(0xff5a5a, 1.0),
        Drive::Hunger => Color::hex(0xffa14e, 1.0),
        Drive::Thirst => Color::hex(0x5aa8ff, 1.0),
        Drive::Curiosity => Color::hex(0xb98cff, 1.0),
        Drive::Social => Color::hex(0xffd24e, 1.0),
        Drive::Mastery => Color::hex(0x5fd6a0, 1.0),
    }
}

const PAPER: u32 = 0xf6f4ef;
const MUTED: u32 = 0x9a94a6;
const INK: u32 = 0x12101a;
const CORAL: u32 = 0xef6a3d;

#[inline]
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
#[inline]
fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Climate for the renderer. In an OPEN WORLD this is keyed to the *real* sim season
/// (so the snow falls exactly when food stops and the cold bites — the visuals match
/// the consequences); otherwise it is the original purely-cosmetic time-of-year cycle
/// that never touched the seeded sim. Returns (season 0..1, weather 0..1, weather_kind
/// 0 rain · 1 snow).
fn climate(world: &GameWorld) -> (f32, f32, f32) {
    let t = world.tick as f32;
    let ph = t * 0.0012;
    let raw = 0.5 + 0.5 * (ph.sin() * 0.6 + (ph * 0.41 + 1.7).sin() * 0.4);
    let weather = ((raw - 0.5) / 0.5).clamp(0.0, 1.0);
    if world.open_world {
        // the real year: season_phase 0..1 maps Spring→Summer→Autumn→Winter; winter is
        // the last quarter. Heavy snow through winter so the world reads truly cold.
        let season = world.season_phase();
        let winter = season >= 0.75;
        let weather_kind = if winter { 1.0 } else { 0.0 };
        let weather = if winter { weather.max(0.7) } else { weather };
        (season, weather, weather_kind)
    } else {
        let season = (t / 6000.0).fract();
        let winter = (0.62..0.92).contains(&season);
        (season, weather, if winter { 1.0 } else { 0.0 })
    }
}

/// The season's colour cast over the whole world (rgb + mix). Winter lays snow.
fn season_tint(season: f32) -> [f32; 4] {
    const T: [[f32; 4]; 4] = [
        [0.42, 0.60, 0.36, 0.12], // spring — fresh green
        [0.55, 0.56, 0.28, 0.08], // summer — warm gold-green
        [0.66, 0.40, 0.16, 0.24], // autumn — amber
        [0.93, 0.96, 1.06, 0.68], // winter — deep snow blanketing everything
    ];
    let s = season.fract() * 4.0;
    let i = s.floor() as usize % 4;
    let f = s - s.floor();
    let a = T[i];
    let b = T[(i + 1) % 4];
    [
        a[0] + (b[0] - a[0]) * f,
        a[1] + (b[1] - a[1]) * f,
        a[2] + (b[2] - a[2]) * f,
        a[3] + (b[3] - a[3]) * f,
    ]
}

/// The whole day/night/season palette, computed on the CPU and handed to the
/// shaders. This is where the mood lives.
fn compute_sky(world: &GameWorld, cam: &Camera, aspect: f32) -> Sky {
    let (w, h) = (world.w, world.h);
    let vp = cam.view_proj(w, h, aspect);
    let day = world.day;
    let elev = (TAU * (day - 0.25)).sin();
    let daylight = smoothstep(-0.12, 0.18, elev);

    let az = TAU * (day - 0.25) + 0.5;
    let dir_y = elev.max(0.12);
    let horiz = (1.0 - dir_y * dir_y).max(0.04).sqrt();
    let sd = Vec3::new(az.cos() * horiz, dir_y, az.sin() * horiz).normalized();

    let warm = [1.0, 0.62, 0.30];
    let white = [1.0, 0.96, 0.86];
    let moon = [0.45, 0.52, 0.74];
    let sun_c = mix3(warm, white, (elev * 0.5 + 0.5).clamp(0.0, 1.0));
    let sun_color = mix3(moon, sun_c, daylight);
    let sun_strength = 0.22 + 0.95 * daylight;

    let sky_day = [0.46, 0.56, 0.72];
    let sky_dawn = [0.52, 0.34, 0.32];
    let sky_night = [0.07, 0.10, 0.20];
    let lit = mix3(sky_dawn, sky_day, smoothstep(0.0, 0.5, elev));
    let ambient_full = mix3(sky_night, lit, daylight);
    let ambient = [ambient_full[0] * 0.8, ambient_full[1] * 0.8, ambient_full[2] * 0.85];

    let hor_day = [0.60, 0.74, 0.90];
    let hor_dawn = [0.88, 0.55, 0.40];
    let hor_night = [0.06, 0.09, 0.18];
    let hor_lit = mix3(hor_dawn, hor_day, smoothstep(0.0, 0.42, elev));
    let horizon = mix3(hor_night, hor_lit, daylight);

    let (season, weather, weather_kind) = climate(world);

    Sky {
        view_proj: vp.to_cols(),
        cam_pos: cam.eye(w, h),
        sun_dir: [sd.x, sd.y, sd.z],
        daylight,
        sun_color,
        sun_strength,
        ambient,
        horizon,
        fog_far: cam.zoom * 2.6,
        season_tint: season_tint(season),
        weather,
        weather_kind,
        fog_target: [cam.cx, cam.cy],
    }
}

#[allow(clippy::too_many_arguments)]
/// HUD payload for the live generational evolution mode. `None` means the normal
/// village; `Some` replaces the inspector panel with an evolution readout.
pub struct EvoHud {
    pub generation: u32,
    pub alive: usize,
    pub pop: usize,
    pub cycle: u64,
    pub last: Option<crate::evolve_mode::GenStats>,
}

/// Backwards-compatible entry point (village mode): identical to before.
#[allow(clippy::too_many_arguments)]
pub fn build(
    world: &GameWorld,
    cam: &Camera,
    selected: Option<usize>,
    hud: &Hud,
    sw: f32,
    sh: f32,
    time: f32,
    ui: f32,
) -> Scene {
    build_with(world, cam, selected, hud, None, sw, sh, time, ui)
}

/// Render the world + HUD, optionally with the evolution readout in place of the
/// village inspector.
#[allow(clippy::too_many_arguments)]
pub fn build_with(
    world: &GameWorld,
    cam: &Camera,
    selected: Option<usize>,
    hud: &Hud,
    evo: Option<&EvoHud>,
    sw: f32,
    sh: f32,
    time: f32,
    ui: f32,
) -> Scene {
    // `ui` is the HUD scale (device-pixel-ratio): the world renders into the full
    // backing buffer, so HUD chrome laid out in absolute pixels must be scaled by
    // dpr or it shrinks to half-size on a Retina display.
    let ui = ui.clamp(1.0, 3.0);
    let mut s = Scene::new();
    s.world_dims = (world.w, world.h);
    let aspect = sw / sh;
    s.sky = compute_sky(world, cam, aspect);
    let vp = Mat4(s.sky.view_proj);
    let (w, h) = (world.w, world.h);
    let axes = math::iso_axes(cam.cx, cam.cy, cam.yaw);
    let g = |x: f32, z: f32| geo::ground_height(w, h, x, z);
    let glow = |s: &mut Scene, p: [f32; 3], size: f32, c: Color| {
        geo::push_billboard(&mut s.add, p, size, size, axes.0, axes.1, c.0);
    };

    // ---- hearth: a warm light at the village heart, breathing, strongest at night
    if world.living_count() > 0 {
        let (mut mx, mut my) = (0.0f32, 0.0f32);
        for a in world.living() {
            mx += a.rx;
            my += a.ry;
        }
        let n = world.living_count() as f32;
        let (hx, hz) = (mx / n, my / n);
        let breathe = 0.5 + 0.5 * (time * 0.6).sin();
        let warmth = (1.0 - s.sky.daylight) * 0.7 + 0.18;
        glow(
            &mut s,
            [hx, g(hx, hz) + 0.5, hz],
            (3.6 + 0.4 * breathe) * 1.0,
            Color::hex(0xff7a2a, 0.16 * warmth),
        );
    }

    // ---- walls: thin stone slabs the minds built for shelter ----
    // Drawn straight from `world.walls` each frame (walls are few). Each is a THIN
    // STONE SLAB — full length along one cell axis (~0.9), thin in thickness
    // (~0.12), ~0.55 tall — *oriented along its wall neighbours* so a row of cells
    // reads as one continuous thin wall, not a fence of cubes. A wall with an E/W
    // neighbour runs east-west; an N/S neighbour runs north-south; both or none
    // falls back to east-west.
    for wall in &world.walls {
        let (x, z) = (wall.x as f32, wall.y as f32);
        let gy = g(x, z);
        // a faint per-cell wobble in tone + height so the masonry looks hand-laid.
        let hh = geo::hash_unit(((wall.x as i64) << 20 ^ wall.y as i64) as u64, 3);
        let tone = 0.86 + 0.12 * hh;
        let top = 0.52 + 0.06 * hh;
        let stone = Color([0.52 * tone, 0.50 * tone, 0.45 * tone, 1.0]);
        // orient along neighbours: prefer east-west if a wall sits E or W.
        let has = |dx: i32, dy: i32| world.walls.contains(&Pos::new(wall.x + dx, wall.y + dy));
        let ew = has(1, 0) || has(-1, 0);
        let ns = has(0, 1) || has(0, -1);
        let east_west = ew || !ns; // both / neither → east-west default
        let thick = 0.12;
        let length = 0.46; // half-extent → ~0.92 cell length
        let half = if east_west {
            [length, top * 0.5, thick] // long along X (east-west)
        } else {
            [thick, top * 0.5, length] // long along Z (north-south)
        };
        geo::push_box(&mut s.lit, [x, gy + top * 0.5, z], half, 0.0, stone.0);
        // a thin capstone running the same length, slightly proud, to catch light.
        let cap = if east_west {
            [length + 0.02, 0.05, thick + 0.02]
        } else {
            [thick + 0.02, 0.05, length + 0.02]
        };
        geo::push_box(
            &mut s.lit,
            [x, gy + top - 0.03, z],
            cap,
            0.0,
            Color([0.40 * tone, 0.38 * tone, 0.34 * tone, 1.0]).0,
        );
    }

    // ---- open-world: trees (harvestable wood) + the village granary ----
    // Only present in an open world (`trees` is empty otherwise). A tree is a brown
    // trunk + a green canopy; a depleted (gathered) tree loses its canopy to a bare
    // stump, so harvesting reads visually. Regrows in spring.
    for t in &world.trees {
        let (x, z) = (t.pos.x as f32, t.pos.y as f32);
        let gy = g(x, z);
        // trunk
        geo::push_box(&mut s.lit, [x, gy + 0.22, z], [0.07, 0.22, 0.07], 0.0, Color::hex(0x5a3d24, 1.0).0);
        // canopy scaled by remaining wood — a bare stump when depleted.
        if t.wood > 0.15 {
            let r = 0.18 + 0.34 * t.wood;
            let h = 0.35 + 0.55 * t.wood;
            // winter strips the green toward a frosted grey-green.
            let canopy = if matches!(world.season(), crate::sim::Season::Winter) {
                Color::hex(0x6f7d63, 1.0).0
            } else {
                Color::hex(0x2f6e3a, 1.0).0
            };
            geo::push_cone(&mut s.lit, x, gy + 0.4, z, r, h, 6, canopy);
        }
    }
    if world.open_world {
        // The granary / hearth: a squat wooden store distinct from the thin stone
        // walls — a stacked-timber box with a fill bar showing how stocked it is, and
        // a warm hearth glow that brightens with the stores (the village heart).
        let (x, z) = (world.granary.x as f32, world.granary.y as f32);
        let gy = g(x, z);
        let cap = world.granary_capacity().max(1.0);
        let fill = (world.granary_food / cap).clamp(0.0, 1.0);
        // timber base
        geo::push_box(&mut s.lit, [x, gy + 0.30, z], [0.42, 0.30, 0.42], 0.0, Color::hex(0x7a5230, 1.0).0);
        // a lighter banded upper course so it reads as stacked timber, not a crate
        geo::push_box(&mut s.lit, [x, gy + 0.62, z], [0.36, 0.10, 0.36], 0.0, Color::hex(0x9a6f44, 1.0).0);
        // a conical thatch roof
        geo::push_cone(&mut s.lit, x, gy + 0.72, z, 0.5, 0.42, 6, Color::hex(0xb98a3e, 1.0).0);
        // a golden fill bar climbing the front face with the stores
        if fill > 0.02 {
            geo::push_box(
                &mut s.lit,
                [x, gy + 0.06 + 0.5 * fill, z + 0.44],
                [0.30, 0.5 * fill, 0.03],
                0.0,
                Color::hex(0xffd166, 1.0).0,
            );
        }
        // hearth glow: always a little warmth, brighter the more is stored.
        glow(&mut s, [x, gy + 0.5, z], 1.2, Color::hex(0xffb24e, 0.10 + 0.22 * fill));
    }

    // ---- resources ----
    for r in &world.resources {
        if !r.alive {
            continue;
        }
        let (x, z) = (r.pos.x as f32, r.pos.y as f32);
        let gy = g(x, z);
        let bob = 0.5 + 0.5 * (r.pulse * TAU + time).sin();
        match r.kind {
            EntityKind::Food => {
                // a small fruiting bush: green dome + red berries + soft glow
                geo::push_cone(&mut s.lit, x, gy, z, 0.42, 0.5, 6, Color::hex(0x2c6b34, 1.0).0);
                let bh = geo::hash_unit((r.id.0 as u64) << 3, 1);
                for k in 0..4u32 {
                    let a = (k as f32 + bh) * 1.7;
                    geo::push_box(
                        &mut s.lit,
                        [x + a.cos() * 0.32, gy + 0.35 + (k as f32) * 0.04, z + a.sin() * 0.32],
                        [0.06, 0.06, 0.06],
                        0.0,
                        Color::hex(0xd83b46, 1.0).0,
                    );
                }
                glow(&mut s, [x, gy + 0.4, z], 0.9, Color::hex(0x6fe6a8, 0.10));
            }
            EntityKind::Water => {
                // a glowing spring: a shallow cyan pool + a bright bloom
                geo::push_cone(&mut s.lit, x, gy + 0.02, z, 0.5, 0.06, 8, Color::hex(0x2a90c8, 0.9).0);
                glow(&mut s, [x, gy + 0.25, z], 1.1 + 0.1 * bob, Color::hex(0x5ab8ff, 0.16));
            }
            EntityKind::Curio => {
                // a floating crystal: two cones tip-to-tip, slowly turning, bright
                let cy = gy + 0.8 + 0.12 * bob;
                let spin = time * 0.7 + r.pulse * TAU;
                let viol = Color::hex(0xc79bff, 1.0).0;
                let mut tmp = Vec::new();
                geo::push_cone(&mut tmp, 0.0, 0.0, 0.0, 0.22, 0.34, 4, viol);
                geo::push_cone(&mut tmp, 0.0, 0.0, 0.0, 0.22, -0.34, 4, viol);
                // rotate + translate the gem
                let (sc, ss) = spin.sin_cos();
                for v in &tmp {
                    let p = [v.pos[0] * ss - v.pos[2] * sc, v.pos[1], v.pos[0] * sc + v.pos[2] * ss];
                    s.lit.push(LitVertex::new([x + p[0], cy + p[1], z + p[2]], v.color));
                }
                glow(&mut s, [x, cy, z], 1.0 + 0.12 * bob, Color::hex(0xc79bff, 0.22));
            }
            _ => {}
        }
    }

    // ---- predator: a dark wolf with a red menace aura ----
    {
        let (x, z) = (world.predator.rx, world.predator.ry);
        let gy = g(x, z);
        let pulse = 0.5 + 0.5 * (time * 2.2).sin();
        push_wolf(&mut s.lit, x, gy, z, time);
        glow(&mut s, [x, gy + 0.5, z], 1.6 + 0.3 * pulse, Color::hex(0xff1d1d, 0.22));
        glow(&mut s, [x, gy + 0.45, z], 0.7, Color::hex(0xff4a3a, 0.30));
    }

    // ---- agents: little glowing figures (the living) and graves (the dead) ----
    for (i, a) in world.agents.iter().enumerate() {
        let (x, z) = (a.rx, a.ry);
        let gy = g(x, z);

        // PERMADEATH: a dead mind leaves the living world. In its place a small
        // stone cairn marks where it fell — no figure, no aura, no labels. A pale
        // memorial glow rises for a while after death, then settles to the bare
        // stone, so the village visibly depopulates and the loss is legible.
        if !a.alive {
            let age = world.tick.saturating_sub(a.death_tick.unwrap_or(world.tick)) as f32;
            // a low cairn: a squat dark stone with a paler capstone.
            geo::push_box(&mut s.lit, [x, gy + 0.18, z], [0.26, 0.18, 0.26], 0.0, Color::hex(0x4a4652, 1.0).0);
            geo::push_box(&mut s.lit, [x, gy + 0.34, z], [0.16, 0.08, 0.16], 0.0, Color::hex(0x6a6676, 1.0).0);
            // a small upright marker stone (a headstone) leaning a touch.
            geo::push_box(&mut s.lit, [x, gy + 0.5, z - 0.02], [0.07, 0.22, 0.05], 0.06, Color::hex(0x57535f, 1.0).0);
            // a fading memorial light: bright at the moment of death, easing to a
            // faint, steady ember of remembrance.
            let fresh = (1.0 - age / 240.0).clamp(0.0, 1.0);
            let memo = 0.06 + 0.22 * fresh;
            glow(&mut s, [x, gy + 0.5, z], 0.7 + 0.4 * fresh, Color::hex(0xbfc8e0, memo));
            // the fallen one's name lingers over the grave.
            if let Some((sx, sy)) = project(&vp, [x, gy + 0.9, z], sw, sh) {
                if cam.zoom < 22.0 {
                    let tw = a.name.chars().count() as f32 * 6.0 * ui;
                    s.text(&a.name, sx - tw * 0.5, sy, 10.5 * ui, Color::hex(MUTED, 0.7));
                }
            }
            continue;
        }

        let (dom, _) = a.mind.drives().dominant();
        let accent = Color::hex(a.accent, 1.0);
        let mood = Color::hex(a.mind.affect().hue(), 1.0);
        let breath = 0.5 + 0.5 * (time * 1.6 + i as f32 * 1.7).sin();

        // trail — fading motes of where it has been
        let tn = a.trail.len();
        for (k, &(wx, wy)) in a.trail.iter().enumerate() {
            let f = (k + 1) as f32 / (tn + 1) as f32;
            glow(&mut s, [wx, g(wx, wy) + 0.25, wy], 0.18 * f, accent.with_a(0.12 * f));
        }

        // body (a luminous gumdrop) + head — a little taller than the grass so
        // the minds always read as the focus of the scene.
        geo::push_cone(&mut s.lit, x, gy, z, 0.34, 0.78 + 0.04 * breath, 6, accent.0);
        geo::push_box(&mut s.lit, [x, gy + 0.96, z], [0.17, 0.17, 0.17], 0.0, Color::hex(0xf2e9da, 1.0).0);

        // mood aura (felt emotion) + a drive-coloured glow at the feet + a bright
        // soul-spark so each mind is visible even among the trees.
        glow(&mut s, [x, gy + 0.6, z], 1.05 + 0.10 * breath, mood.with_a(0.30));
        glow(&mut s, [x, gy + 0.12, z], 0.7, drive_color(dom).with_a(0.20));
        glow(&mut s, [x, gy + 0.55, z], 0.3, Color::hex(0xfff4e0, 0.5));

        // selection: a bright pale halo
        if Some(i) == selected {
            glow(&mut s, [x, gy + 0.55, z], 1.3 + 0.1 * breath, Color::hex(PAPER, 0.22));
        }
        // invented-goal (Praxis): a gold bloom
        if a.mind.acting_on_invented() {
            glow(&mut s, [x, gy + 0.7, z], 1.0 + 0.1 * breath, Color::hex(0xf0c24e, 0.28));
        }
        // process flash: reflex red / deliberate violet
        if a.flash > 0.02 {
            let fc = match a.flash_kind {
                Process::Reflex => Color::hex(0xff3030, 0.5 * a.flash),
                _ => Color::hex(0x9b6bff, 0.45 * a.flash),
            };
            glow(&mut s, [x, gy + 0.6, z], 0.9 + 0.7 * a.flash, fc);
        }

        // intent: a soft ribbon of motes toward the committed goal
        if let Some(t) = a.mind.intent_target() {
            let (tx, tz) = (t.x as f32, t.y as f32);
            let drift = (time * 0.6).fract();
            for k in 0..7 {
                let f = (k as f32 + drift) / 7.0;
                if f >= 1.0 {
                    continue;
                }
                let px = x + (tx - x) * f;
                let pz = z + (tz - z) * f;
                let lift = (f * std::f32::consts::PI).sin() * 0.5;
                let a2 = (1.0 - (f - 0.5).abs() * 1.6).max(0.12) * 0.35;
                glow(&mut s, [px, g(px, pz) + 0.4 + lift, pz], 0.12, drive_color(dom).with_a(a2));
            }
        }

        // world-anchored labels: name + speech, projected to the screen for crisp
        // text. Positions are physical px (from projection); sizes scale by `ui`.
        if let Some((sx, sy)) = project(&vp, [x, gy + 1.2, z], sw, sh) {
            if cam.zoom < 22.0 {
                let tw = a.name.chars().count() as f32 * 6.5 * ui;
                s.text(&a.name, sx - tw * 0.5, sy, 12.0 * ui, Color::hex(PAPER, 0.92));
            }
            // health bar when hurt or selected
            if a.body.health < 0.95 || Some(i) == selected {
                let bw = 34.0 * ui;
                let by = sy - 8.0 * ui;
                s.rrect(sx - bw * 0.5, by, bw, 4.0 * ui, 2.0 * ui, Color::hex(INK, 0.7));
                let hc = if a.body.health > 0.4 { Color::hex(0x5fd6a0, 0.95) } else { Color::hex(0xff5a5a, 0.95) };
                s.rrect(sx - bw * 0.5, by, bw * a.body.health.clamp(0.0, 1.0), 4.0 * ui, 2.0 * ui, hc);
            }
        }
        if let Some((text, t)) = &a.say {
            if let Some((sx, sy)) = project(&vp, [x, gy + 1.7, z], sw, sh) {
                let alpha = (t / 2.2).clamp(0.0, 1.0);
                let tw = (text.chars().count() as f32 * 7.0 + 16.0) * ui;
                let bxp = sx - tw * 0.5;
                let byp = sy - (22.0 + (i % 3) as f32 * 20.0) * ui;
                s.rrect(bxp, byp, tw, 22.0 * ui, 8.0 * ui, Color::hex(INK, 0.86 * alpha));
                s.text(text, bxp + 8.0 * ui, byp + 4.0 * ui, 12.0 * ui, Color::hex(PAPER, alpha));
            }
        }
    }

    // ---- weather particles (additive motes drifting through the air) ----
    weather_motes(&mut s, cam, axes, time);

    // ---- HUD chrome (top bar + inspector) ----
    // Laid out in *design pixels* (the backing buffer ÷ ui), then scaled up by
    // `ui` so the panels read at the right size on any device-pixel-ratio.
    let mut chrome = Scene::new();
    if let Some(evo) = evo {
        evo_top_bar(&mut chrome, evo, sw / ui);
        evo_panel(&mut chrome, world, evo, sw / ui, sh / ui);
    } else {
        top_bar(&mut chrome, world, hud, sw / ui);
        inspector(&mut chrome, world, selected, sw / ui, sh / ui);
    }
    for mut q in chrome.quads {
        q.rect = [q.rect[0] * ui, q.rect[1] * ui, q.rect[2] * ui, q.rect[3] * ui];
        q.params[0] *= ui; // corner radius / orb radius
        q.params[1] *= ui; // edge softness
        s.quads.push(q);
    }
    for mut t in chrome.texts {
        t.x *= ui;
        t.y *= ui;
        t.size *= ui;
        t.wrap = t.wrap.map(|w| w * ui);
        s.texts.push(t);
    }
    s
}

/// Project a world point to screen pixels (orthographic ⇒ w = 1).
fn project(vp: &Mat4, p: [f32; 3], sw: f32, sh: f32) -> Option<(f32, f32)> {
    let c = vp.transform_point(Vec3::new(p[0], p[1], p[2]));
    if c[3].abs() < 1e-5 {
        return None;
    }
    let ndc = [c[0] / c[3], c[1] / c[3]];
    Some(((ndc[0] * 0.5 + 0.5) * sw, (1.0 - (ndc[1] * 0.5 + 0.5)) * sh))
}

/// A wolf: a grey low-slung quadruped — body, head, tail, four stub legs.
fn push_wolf(out: &mut Vec<LitVertex>, x: f32, gy: f32, z: f32, time: f32) {
    let lope = (time * 9.0).sin() * 0.06;
    let fur = Color::hex(0x4a4750, 1.0).0;
    let dark = Color::hex(0x2b2930, 1.0).0;
    geo::push_box(out, [x, gy + 0.42 + lope, z], [0.18, 0.16, 0.34], 0.0, fur);
    geo::push_box(out, [x, gy + 0.50, z + 0.32], [0.13, 0.13, 0.14], 0.0, dark);
    geo::push_box(out, [x, gy + 0.46, z + 0.5], [0.07, 0.07, 0.10], 0.0, dark);
    geo::push_box(out, [x, gy + 0.5, z - 0.45], [0.04, 0.04, 0.16], 0.0, dark);
    for (sx, sz) in [(-1.0f32, 0.22f32), (1.0, 0.22), (-1.0, -0.26), (1.0, -0.26)] {
        geo::push_box(out, [x + sx * 0.13, gy + 0.16, z + sz], [0.05, 0.16, 0.05], 0.0, dark);
    }
    // two hot eyes
    geo::push_box(out, [x - 0.05, gy + 0.54, z + 0.58], [0.02, 0.02, 0.02], 0.0, Color::hex(0xff5a3a, 1.0).0);
    geo::push_box(out, [x + 0.05, gy + 0.54, z + 0.58], [0.02, 0.02, 0.02], 0.0, Color::hex(0xff5a3a, 1.0).0);
}

/// Falling rain or snow as additive motes in a slab around the camera centre —
/// deterministic from time, so it reads as continuous motion.
fn weather_motes(s: &mut Scene, cam: &Camera, axes: ([f32; 3], [f32; 3]), time: f32) {
    let w = s.sky.weather;
    if w < 0.05 {
        return;
    }
    let snow = s.sky.weather_kind > 0.5;
    let count = (w * if snow { 90.0 } else { 130.0 }) as i32;
    let span = cam.zoom * 1.4;
    for k in 0..count {
        let kf = k as f32;
        let rx = (kf * 12.9898).sin() * 0.5 + 0.5;
        let rz = (kf * 78.233).sin() * 0.5 + 0.5;
        let ph = (kf * 7.0).fract();
        let x = cam.cx + (rx - 0.5) * span * 2.0;
        let z = cam.cy + (rz - 0.5) * span * 2.0;
        if snow {
            let fall = 12.0 - ((time * 0.9 + ph) % 1.0) * 13.0;
            let sway = (time * 0.8 + kf).sin() * 0.4;
            geo::push_billboard(&mut s.add, [x + sway, fall, z], 0.09, 0.09, axes.0, axes.1, Color::hex(0xf2f6ff, 0.5).0);
        } else {
            let fall = 12.0 - ((time * 2.4 + ph) % 1.0) * 13.0;
            geo::push_billboard(&mut s.add, [x, fall, z], 0.03, 0.32, axes.0, axes.1, Color::hex(0xaecbe6, 0.32).0);
        }
    }
}

fn top_bar(s: &mut Scene, world: &GameWorld, hud: &Hud, sw: f32) {
    s.rrect(12.0, 12.0, sw - 24.0, 38.0, 10.0, Color::hex(INK, 0.72));
    s.orb(34.0, 31.0, 6.0, 0.8, Color::hex(0x5b3df0, 1.0));
    s.text("DAIMON · SMALLWORLD", 50.0, 21.0, 15.0, Color::hex(PAPER, 0.95));

    let phase = if world.day < 0.25 || world.day > 0.75 { "night" } else { "day" };
    let status = format!(
        "tick {}   ·   {}   ·   {}x   ·   {}",
        world.tick,
        if hud.paused { "paused" } else { "running" },
        hud.speed as u32,
        phase,
    );
    s.text(status, sw - 420.0, 21.0, 13.0, Color::hex(MUTED, 1.0));
    if hud.quantum {
        s.text("⚛ QUANTUM DECISION MODE", sw - 200.0, 21.0, 12.0, Color::hex(0xb98cff, 1.0));
    } else {
        s.text("trained policy", sw - 130.0, 21.0, 12.0, Color::hex(0x5fd6a0, 0.9));
    }
    s.text(
        "click an agent · drag to pan · scroll to zoom · space pauses · [ / ] speed · F feed · Q quantum",
        50.0,
        37.0,
        10.5,
        Color::hex(MUTED, 0.9),
    );
}

/// Top bar for the evolution mode: `Generation N · alive/pop · cycle k/10`.
fn evo_top_bar(s: &mut Scene, evo: &EvoHud, sw: f32) {
    s.rrect(12.0, 12.0, sw - 24.0, 38.0, 10.0, Color::hex(INK, 0.72));
    s.orb(34.0, 31.0, 6.0, 0.8, Color::hex(0x5b3df0, 1.0));
    s.text("DAIMON · EVOLUTION", 50.0, 21.0, 15.0, Color::hex(PAPER, 0.95));
    let status = format!(
        "Generation {}   ·   {}/{} alive   ·   cycle {}/{}",
        evo.generation, evo.alive, evo.pop, evo.cycle, crate::evolve_mode::CYCLES_PER_GEN,
    );
    s.text(status, sw - 420.0, 21.0, 13.0, Color::hex(MUTED, 1.0));
    s.text("natural selection · fast-forward", sw - 420.0, 37.0, 10.5, Color::hex(0x5fd6a0, 0.9));
}

/// Replaces the village inspector in evolution mode: how the population is faring
/// and how it evolved last generation.
fn evo_panel(s: &mut Scene, world: &GameWorld, evo: &EvoHud, sw: f32, sh: f32) {
    let pw = 354.0;
    let px = sw - pw - 14.0;
    let py = 60.0;
    let ph = sh - py - 14.0;
    s.rrect(px, py, pw, ph, 14.0, Color::hex(INK, 0.8));
    let tx = px + 18.0;
    let mut y = py + 18.0;
    s.text("EVOLUTION", tx, y, 12.0, Color::hex(CORAL, 1.0));
    y += 26.0;

    s.text(format!("Generation {}", evo.generation), tx, y, 16.0, Color::hex(PAPER, 1.0));
    y += 24.0;
    s.text(
        format!("alive {} / {}   ·   cycle {}/10", evo.alive, evo.pop, evo.cycle),
        tx, y, 12.5, Color::hex(MUTED, 1.0),
    );
    y += 18.0;
    s.text(format!("walls built: {}", world.walls.len()), tx, y, 12.0, Color::hex(MUTED, 0.9));
    y += 28.0;

    // a live die-off bar.
    let frac = evo.alive as f32 / (evo.pop.max(1) as f32);
    bar(s, tx, y, pw - 36.0, frac, Color::hex(0x5fd6a0, 1.0));
    y += 26.0;

    s.text("LAST GENERATION", tx, y, 11.0, Color::hex(CORAL, 0.9));
    y += 20.0;
    match &evo.last {
        None => {
            s.text("(first generation in progress)", tx, y, 12.0, Color::hex(MUTED, 0.8));
        }
        Some(st) => {
            s.text(
                format!("survivors at end: {}   ·   elite {}", st.survivors_end, st.elite_n),
                tx, y, 12.5, Color::hex(PAPER, 0.95),
            );
            y += 18.0;
            // best first — it's the headline of "did the population improve?".
            s.text(
                format!("best fitness:   {:.0}", st.best_fitness),
                tx, y, 12.0, Color::hex(PAPER, 1.0),
            );
            y += 18.0;
            s.text(
                format!("elite mean:     {:.0}", st.elite_mean),
                tx, y, 12.0, Color::hex(MUTED, 1.0),
            );
            y += 18.0;
            s.text(
                format!("pop mean:       {:.0}", st.mean_fitness),
                tx, y, 12.0, Color::hex(MUTED, 0.85),
            );
            y += 18.0;
            let dom = st
                .elite_dominant
                .map(|d| d.name())
                .unwrap_or("—");
            s.text(format!("elite leading drive: {dom}"), tx, y, 12.0, Color::hex(0x5fd6a0, 0.95));
        }
    }
}

fn bar(s: &mut Scene, x: f32, y: f32, w: f32, frac: f32, c: Color) {
    s.rrect(x, y, w, 8.0, 4.0, Color::hex(INK, 0.85));
    s.rrect(x, y, (w * frac.clamp(0.0, 1.0)).max(2.0), 8.0, 4.0, c);
}

fn inspector(s: &mut Scene, world: &GameWorld, selected: Option<usize>, sw: f32, sh: f32) {
    let pw = 354.0;
    let px = sw - pw - 14.0;
    let py = 60.0;
    let ph = sh - py - 14.0;
    s.rrect(px, py, pw, ph, 14.0, Color::hex(INK, 0.8));

    let Some(i) = selected else {
        s.text("THE VILLAGE", px + 18.0, py + 18.0, 12.0, Color::hex(CORAL, 1.0));
        s.text_wrapped(
            "Six minds share one field, each running the real Daimon cognitive cycle — and the trained policy that reaches the end goal. Click any one to open its mind.",
            px + 18.0,
            py + 38.0,
            12.5,
            pw - 36.0,
            Color::hex(PAPER, 0.85),
        );
        let mut yy = py + 112.0;
        let living = world.living_count();
        let total = world.agents.len();
        if living < total {
            s.text(
                format!("{living} of {total} still living"),
                px + 18.0,
                yy - 22.0,
                11.0,
                Color::hex(0xff8a8a, 0.95),
            );
        }
        for a in &world.agents {
            if a.alive {
                s.orb(px + 26.0, yy + 7.0, 6.0, 0.6, Color::hex(a.accent, 1.0));
                let (dom, _) = a.mind.drives().dominant();
                s.text(format!("{}  ·  {}", a.name, dom.name()), px + 40.0, yy, 13.0, Color::hex(PAPER, 0.9));
            } else {
                s.orb(px + 26.0, yy + 7.0, 5.0, 0.4, Color::hex(0x6a6676, 1.0));
                s.text(format!("{}  ·  lost", a.name), px + 40.0, yy, 13.0, Color::hex(MUTED, 0.7));
            }
            yy += 24.0;
        }
        yy += 10.0;
        s.text("WHAT'S RUNNING", px + 18.0, yy, 11.0, Color::hex(CORAL, 1.0));
        yy += 18.0;
        for line in [
            "Praxis — invents its own concepts & goals",
            "Anticipation — forages ahead of crisis",
            "Empowerment — seeks open ground",
            "Imagination — plans over a learned model",
            "Associative memory — Hebbian + ACT-R",
            "Theory of mind — models the others",
            "Commons — shares scarce water",
            "Quantum cognition — press Q to engage",
        ] {
            s.text(format!("· {line}"), px + 22.0, yy, 11.0, Color::hex(PAPER, 0.8));
            yy += 17.0;
        }
        return;
    };

    let a = &world.agents[i];
    let mut y = py + 16.0;
    let lx = px + 18.0;
    let cw = pw - 36.0;

    s.orb(lx + 6.0, y + 8.0, 7.0, 0.8, Color::hex(a.accent, 1.0));
    s.text(&a.name, lx + 22.0, y - 2.0, 20.0, Color::hex(PAPER, 1.0));
    y += 26.0;
    s.text_wrapped(format!("“{}”", a.mind.persona.creed), lx, y, 11.5, cw, Color::hex(MUTED, 1.0));
    y += 30.0;

    let active: Vec<&str> =
        a.mind.faculty_flags().iter().filter(|(_, on)| *on).map(|(n, _)| *n).collect();
    if !active.is_empty() {
        s.text_wrapped(format!("⚙ {}", active.join(" · ")), lx, y, 10.5, cw, Color::hex(0x7fb0ff, 0.9));
        y += 24.0;
    }

    let mood = a.mind.affect();
    s.orb(lx + 5.0, y + 5.0, 5.0, 0.8, Color::hex(mood.hue(), 1.0));
    s.text(
        format!("feeling {}  (valence {:+.2} · arousal {:.2})", mood.emotion(), mood.valence, mood.arousal),
        lx + 16.0,
        y,
        11.0,
        Color::hex(mood.hue(), 0.95),
    );
    y += 24.0;

    if let Some(pr) = a.mind.project() {
        let pct = (pr.fraction() * 100.0) as u32;
        let tag = if pr.is_done() { "✓ done" } else { "in progress" };
        s.text(format!("PROJECT — {}", pr.label()), lx, y, 11.0, Color::hex(CORAL, 1.0));
        y += 14.0;
        bar(s, lx, y + 1.0, cw - 64.0, pr.fraction(), Color::hex(0xb98cff, 1.0));
        s.text(format!("{pct}% · {tag}"), lx + cw - 58.0, y - 1.0, 10.5, Color::hex(MUTED, 1.0));
        y += 18.0;
    }

    let (proc_tag, proc_col, inner) = match &a.last {
        Some(t) => {
            let c = match t.process {
                Process::Reflex => 0xff5a5a,
                Process::Deliberate => 0x9b6bff,
                Process::Routine => 0x7fb0ff,
            };
            (t.process.tag(), c, a.inner.clone())
        }
        None => ("…", MUTED, "waking up".into()),
    };
    s.rrect(lx, y, cw, 2.0, 1.0, Color::hex(0x2a2740, 1.0));
    y += 10.0;
    s.text(proc_tag, lx, y, 11.0, Color::hex(proc_col, 1.0));
    let sur = a.mind.surprise();
    s.text(format!("surprise {:.2}", sur), lx + cw - 86.0, y, 10.5, Color::hex(MUTED, 1.0));
    bar(s, lx + cw - 86.0, y + 12.0, 86.0, sur, Color::hex(0xffd24e, 1.0));
    y += 16.0;
    s.text_wrapped(strip_tag(&inner), lx, y, 12.5, cw, Color::hex(PAPER, 0.95));
    y += 56.0;

    if a.mind.acting_on_invented() {
        s.rrect(lx, y, cw, 22.0, 6.0, Color::hex(0x2a1a08, 1.0));
        s.text("✦ ACTING ON THE UNFORESEEN", lx + 8.0, y + 5.0, 11.0, Color::hex(0xf0c24e, 1.0));
        y += 28.0;
    }

    s.text("DRIVES", lx, y, 11.0, Color::hex(CORAL, 1.0));
    s.text("level · learned value", lx + cw - 118.0, y, 9.5, Color::hex(MUTED, 0.8));
    y += 18.0;
    for (d, v) in a.mind.drives().iter() {
        let bias = a.mind.drives().bias(d);
        let mark = if bias > 1.05 { " ↑" } else if bias < 0.95 { " ↓" } else { "" };
        s.text(format!("{}{}", d.name(), mark), lx, y - 1.0, 11.5, Color::hex(MUTED, 1.0));
        bar(s, lx + 78.0, y + 1.0, cw - 78.0, v, drive_color(d));
        y += 18.0;
    }
    y += 8.0;

    s.text("BODY", lx, y, 11.0, Color::hex(CORAL, 1.0));
    y += 18.0;
    for (label, val, col) in [
        ("health", a.body.health, 0x5fd6a0u32),
        ("energy", a.body.energy, 0xffa14e),
        ("water", a.body.hydration, 0x5aa8ff),
    ] {
        s.text(label, lx, y - 1.0, 11.5, Color::hex(MUTED, 1.0));
        bar(s, lx + 78.0, y + 1.0, cw - 78.0, val, Color::hex(col, 1.0));
        y += 18.0;
    }
    y += 8.0;

    let mut skills: Vec<_> = a.mind.memory().skills().collect();
    skills.sort_by(|x, z| z.competence().total_cmp(&x.competence()));
    if !skills.is_empty() {
        s.text("SKILLS", lx, y, 11.0, Color::hex(CORAL, 1.0));
        y += 18.0;
        for sk in skills.iter().take(3) {
            s.text(
                format!("{}  {:.0}% · {}×", sk.name, sk.competence() * 100.0, sk.uses),
                lx,
                y,
                11.5,
                Color::hex(PAPER, 0.85),
            );
            y += 16.0;
        }
        y += 8.0;
    }

    let mut folk: Vec<_> = a.mind.social().known().collect();
    folk.sort_by(|x, z| z.disposition.total_cmp(&x.disposition));
    if !folk.is_empty() {
        s.text("KNOWS", lx, y, 11.0, Color::hex(CORAL, 1.0));
        y += 18.0;
        for m in folk.iter().take(3) {
            let feel = if m.disposition > 0.4 { "friend" } else if m.disposition > 0.0 { "amiable" } else { "wary" };
            let thinks = m.believed_drive.map(|d| format!(" · thinks: {}", d.name())).unwrap_or_default();
            s.text(
                format!("{}  ·  {} ({:+.1}){}", m.name, feel, m.disposition, thinks),
                lx,
                y,
                11.0,
                Color::hex(PAPER, 0.85),
            );
            y += 16.0;
        }
        y += 8.0;
    }

    let concepts: Vec<_> = a.mind.praxis().concepts.iter().filter(|c| c.seen >= 2).collect();
    if !concepts.is_empty() {
        s.text("FORMS IT COINED", lx, y, 11.0, Color::hex(CORAL, 1.0));
        y += 18.0;
        let mut cs = concepts;
        cs.sort_by_key(|c| std::cmp::Reverse(c.seen));
        for c in cs.iter().take(3) {
            s.text(format!("{}  ·  {}", c.name, c.epithet()), lx, y, 11.5, Color::hex(PAPER, 0.85));
            y += 16.0;
        }
        y += 8.0;
    }

    if let Some(ep) = a.mind.memory().episodes().max_by(|x, z| x.salience.total_cmp(&z.salience)) {
        s.text("VIVID MEMORY", lx, y, 11.0, Color::hex(CORAL, 1.0));
        y += 16.0;
        s.text_wrapped(&ep.what, lx, y, 11.5, cw, Color::hex(MUTED, 1.0));
    }
}

fn strip_tag(s: &str) -> String {
    if let Some(rest) = s.split_once("] ") {
        rest.1.to_string()
    } else {
        s.to_string()
    }
}
