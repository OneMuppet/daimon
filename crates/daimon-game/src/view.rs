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

// DEBUG look-iteration toggle: when set, the per-mind glow is suppressed so a
// headless shot can inspect the bare villager CHARACTER mesh. Off by default and
// never touched by the harness; a pure render aid set from `?noglow=1` / DAIMON_NOGLOW.
thread_local!(static NOGLOW: std::cell::Cell<bool> = const { std::cell::Cell::new(false) });
/// Set the debug "suppress mind glow" flag (look-iteration aid only).
pub fn set_noglow(on: bool) {
    NOGLOW.with(|c| c.set(on));
}
#[inline]
fn noglow() -> bool {
    NOGLOW.with(|c| c.get())
}

/// First-person "drop in" state: you stand at `(eye_x, eye_z)` in sim coords, at
/// human eye-height above the terrain there, looking along `yaw`/`pitch`. Pure
/// render+input — it never touches the sim; it's just an alternative way to look
/// at the same world. `Some` ⇒ the camera renders first-person; `None` ⇒ the iso
/// god-view, exactly as before.
#[derive(Clone, Copy)]
pub struct FpView {
    pub eye_x: f32,
    pub eye_z: f32,
    pub yaw: f32,
    pub pitch: f32,
}

/// Human eye-height in world units above the terrain you're standing on.
pub const FP_EYE_HEIGHT: f32 = 1.7;

impl FpView {
    /// The world-space eye position: standing height above the terrain under you.
    pub fn eye_pos(&self, w: i32, h: i32) -> Vec3 {
        let gy = geo::ground_height(w, h, self.eye_x, self.eye_z);
        Vec3::new(self.eye_x, gy + FP_EYE_HEIGHT, self.eye_z)
    }
}

/// The isometric god-view camera: centre (sim cells), vertical zoom half-extent
/// (world units), and yaw. When `fp` is `Some`, the camera instead renders a
/// first-person walk-through view from that eye (render+input only — the iso
/// fields are preserved untouched so exiting FP restores the exact god-view).
pub struct Camera {
    pub cx: f32,
    pub cy: f32,
    pub zoom: f32,
    pub yaw: f32,
    /// First-person "drop in" view, or `None` for the iso god-view.
    pub fp: Option<FpView>,
    /// WALKABLE INTERIORS: when `Some(floor_y)`, the first-person eye stands on a
    /// FLAT interior floor at `floor_y` (eye = floor_y + eye-height) instead of the
    /// island terrain — so a room reads as a level room, not draped on the hills.
    /// `None` ⇒ the eye-height tracks the terrain, exactly as before.
    pub interior_floor: Option<f32>,
}

impl Camera {
    pub fn new(cx: f32, cy: f32) -> Self {
        Camera { cx, cy, zoom: 12.0, yaw: math::ISO_YAW_DEG.to_radians(), fp: None, interior_floor: None }
    }

    /// The first-person eye position, honouring a flat interior floor if set.
    fn fp_eye(&self, fp: &FpView, w: i32, h: i32) -> Vec3 {
        match self.interior_floor {
            Some(fy) => Vec3::new(fp.eye_x, fy + FP_EYE_HEIGHT, fp.eye_z),
            None => fp.eye_pos(w, h),
        }
    }

    /// True while in first-person "drop in" mode.
    pub fn is_fp(&self) -> bool {
        self.fp.is_some()
    }

    fn target_y(&self, w: i32, h: i32) -> f32 {
        geo::ground_height(w, h, self.cx, self.cy) + 0.4
    }

    pub fn view_proj(&self, w: i32, h: i32, aspect: f32) -> Mat4 {
        match self.fp {
            Some(fp) => math::perspective_view_proj(
                self.fp_eye(&fp, w, h),
                fp.yaw,
                fp.pitch,
                math::FP_FOV_DEG.to_radians(),
                aspect,
            ),
            None => math::iso_view_proj(self.cx, self.cy, self.target_y(w, h), self.zoom, aspect, self.yaw),
        }
    }

    pub fn eye(&self, w: i32, h: i32) -> [f32; 3] {
        match self.fp {
            Some(fp) => {
                let e = self.fp_eye(&fp, w, h);
                [e.x, e.y, e.z]
            }
            None => {
                let (e, _, _, _) = math::iso_basis(self.cx, self.cy, self.target_y(w, h), self.yaw);
                [e.x, e.y, e.z]
            }
        }
    }

    /// Cursor pixel → sim ground coords (picking / feeding). Iso god-view only;
    /// first-person uses pointer-lock mouse-look, not ground picking.
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

/// Blend two 0xRRGGBB colours, `t` of the way from `a` to `b` (`t` in `[0,1]`).
/// Used to tint a mind toward its village's banner hue while keeping its accent.
fn blend_rgb(a: u32, b: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let lerp = |x: u32, y: u32| ((x as f32) * (1.0 - t) + (y as f32) * t) as u32;
    let r = lerp((a >> 16) & 0xff, (b >> 16) & 0xff);
    let g = lerp((a >> 8) & 0xff, (b >> 8) & 0xff);
    let bl = lerp(a & 0xff, b & 0xff);
    (r << 16) | (g << 8) | bl
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
    // SHOWCASE HERO LIGHT: instead of letting the deterministic day/night cycle sit
    // wherever it lands (often flat night), bias the *render-time* time-of-day to a
    // flattering warm golden hour and let it drift only gently across that range, so
    // the island always reads as lit, dimensional and cinematic. This touches only
    // the look — `world.day` (the sim field) is never written here.
    let day = 0.205 + 0.03 * (TAU * world.day).sin(); // ~07:00–08:00 low warm sun
    let elev = (TAU * (day - 0.25)).sin();
    // Keep a strong, confident key even at this low elevation — never washes to night.
    let daylight = smoothstep(-0.12, 0.18, elev).max(0.78);

    let az = TAU * (day - 0.25) + 0.5;
    // A low, raking sun gives the terrain long, shapely shading; floor it so figures
    // and trees still catch a clear top-light rather than going pitch-dark.
    let dir_y = elev.max(0.26);
    let horiz = (1.0 - dir_y * dir_y).max(0.04).sqrt();
    let sd = Vec3::new(az.cos() * horiz, dir_y, az.sin() * horiz).normalized();

    let warm = [1.0, 0.58, 0.26];
    let white = [1.0, 0.92, 0.78];
    let moon = [0.45, 0.52, 0.74];
    // Bias the key strongly toward warm gold (the golden-hour cast).
    let sun_c = mix3(warm, white, (elev * 0.5 + 0.18).clamp(0.0, 1.0));
    let sun_color = mix3(moon, sun_c, daylight);
    let sun_strength = 1.55 + 0.5 * daylight; // a strong, confident hero key

    let sky_day = [0.46, 0.56, 0.72];
    let sky_dusk = [0.62, 0.40, 0.40];
    let sky_night = [0.07, 0.10, 0.20];
    // A cool sky/ambient FILL against the warm key so terrain has form (warm light /
    // cool shadow), without ever going as dark as true night.
    let lit = mix3(sky_dusk, sky_day, smoothstep(0.0, 0.5, elev));
    let ambient_full = mix3(sky_night, lit, daylight);
    let ambient = [
        ambient_full[0] * 0.72 + 0.05,
        ambient_full[1] * 0.78 + 0.06,
        ambient_full[2] * 0.92 + 0.10,
    ];

    let hor_day = [0.66, 0.78, 0.92];
    let hor_dusk = [0.96, 0.62, 0.42]; // warm gold band at the horizon
    let hor_night = [0.10, 0.13, 0.24];
    let hor_lit = mix3(hor_dusk, hor_day, smoothstep(0.0, 0.42, elev));
    let horizon = mix3(hor_night, hor_lit, daylight);

    let (season, weather, weather_kind) = climate(world);

    // Fog is measured as horizontal distance from a centre. In the iso god-view
    // that centre is the framed point and the reach scales with zoom. In FIRST
    // PERSON the centre is the eye itself and the reach is a generous fixed depth,
    // so distant land softens into the horizon haze the way a real ground-level
    // view does — without ever fogging out what's right in front of you.
    let (fog_far, fog_target) = match cam.fp {
        Some(fp) => ((w.max(h) as f32) * 1.2, [fp.eye_x, fp.eye_z]),
        None => (cam.zoom * 2.6, [cam.cx, cam.cy]),
    };

    Sky {
        view_proj: vp.to_cols(),
        cam_pos: cam.eye(w, h),
        sun_dir: [sd.x, sd.y, sd.z],
        daylight,
        sun_color,
        sun_strength,
        ambient,
        horizon,
        fog_far,
        season_tint: season_tint(season),
        weather,
        weather_kind,
        fog_target,
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

/// Render a WALKABLE HOUSE INTERIOR — a self-contained little room scene drawn in
/// place of the island while the player is inside. The room geometry is the
/// interior's own generated map; the eye is the first-person camera standing on the
/// flat interior floor (the camera's `interior_floor` override). Warm hearth glows
/// light the room; a small HUD prompt explains the controls.
fn build_interior(
    inter: &crate::interior::Interior,
    cam: &Camera,
    sw: f32,
    sh: f32,
    time: f32,
    ui: f32,
) -> Scene {
    let mut s = Scene::new();
    // suppress the island: zero dims ⇒ the renderer builds an empty terrain, and
    // the `interior` flag tells it to skip the sea too.
    s.world_dims = (0, 0);
    s.interior = true;
    let aspect = sw / sh;

    // Camera: first-person from the eye standing on the flat interior floor. (The
    // caller has set `cam.interior_floor = Some(floor_y)`, so `view_proj`/`eye`
    // already use the flat floor; if somehow not in FP, fall back to identity.)
    let vp = cam.view_proj(0, 0, aspect);
    let billboard_axes = match cam.fp {
        Some(fp) => math::fp_axes(fp.yaw, fp.pitch),
        None => math::fp_axes(0.0, 0.0),
    };

    // A warm, even indoor light — a low ambient so corners read, a soft "sun"
    // (skylight through the door/windows) from above, and the hearth glow carries
    // the mood. No day/night, no weather, no fog distance (it's a small room).
    s.sky = Sky {
        view_proj: vp.to_cols(),
        cam_pos: cam.eye(0, 0),
        sun_dir: [0.35, 0.86, 0.30],
        daylight: 0.85,
        sun_color: [1.0, 0.90, 0.74], // warm interior key
        sun_strength: 1.25,
        ambient: [0.34, 0.30, 0.26], // warm low ambient so the room isn't black
        horizon: [0.07, 0.05, 0.04], // a warm-dark "beyond the walls" (the clear colour)
        fog_far: 600.0, // effectively off — a small room shouldn't fog its own walls
        season_tint: [0.0, 0.0, 0.0, 0.0],
        weather: 0.0,
        weather_kind: 0.0,
        fog_target: [0.0, 0.0],
    };

    // Build the room geometry (floor, walls with a door gap, ceiling, furniture).
    let hearths = inter.build(&mut s.lit).to_vec();

    // Warm hearth/brazier glows — additive billboards facing the eye, breathing.
    let breathe = 0.6 + 0.4 * (time * 1.7).sin();
    for hp in &hearths {
        // the fire sits a little above the floor in the cavity.
        let p = [hp[0], inter.floor_y() + 0.35, hp[2] + 0.10];
        glow_add(&mut s, p, 0.55 * breathe, billboard_axes, Color::hex(0xff7a2a, 0.40));
        glow_add(&mut s, p, 0.26 * breathe, billboard_axes, Color::hex(0xffd28a, 0.55));
        glow_add(&mut s, p, 0.13, billboard_axes, Color::hex(0xfff0d0, 0.6));
    }
    // a soft fill glow at the room centre so the whole interior reads lit, not just
    // by the hearth — like daylight spilling through the open door.
    glow_add(
        &mut s,
        [0.0, inter.wall_h * 0.7, inter.hz * 0.3],
        inter.hx.max(inter.hz) * 0.9,
        billboard_axes,
        Color::hex(0xfff2dc, 0.10),
    );

    // HUD: a small indoor prompt strip (controls), scaled by dpr.
    let pad = 16.0 * ui;
    let fs = 15.0 * ui;
    let txt = if inter.at_door(
        cam.fp.map(|f| f.eye_x).unwrap_or(0.0),
        cam.fp.map(|f| f.eye_z).unwrap_or(0.0),
    ) {
        "INSIDE  ·  WASD walk  ·  mouse look  ·  E or Esc to step outside"
    } else {
        "INSIDE  ·  WASD walk  ·  mouse look  ·  go to the door + E (or Esc) to leave"
    };
    s.rrect(pad * 0.6, pad * 0.6, fs * txt.len() as f32 * 0.52 + pad, fs + pad * 0.8, 8.0, Color::hex(0x000000, 0.42));
    s.text(txt, pad, pad, fs, Color::hex(0xfff2dc, 0.96));

    s
}

/// Push an additive glow billboard into the scene's `add` list (interior lighting).
fn glow_add(s: &mut Scene, p: [f32; 3], size: f32, axes: ([f32; 3], [f32; 3]), c: Color) {
    geo::push_billboard(&mut s.add, p, size, size, axes.0, axes.1, c.0);
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
    build_full(world, cam, selected, hud, evo, None, sw, sh, time, ui)
}

/// Full render entry, including the optional WALKABLE INTERIOR. When `interior` is
/// `Some`, the renderer draws that house's generated room (floor/walls/furniture)
/// instead of the island, and the HUD shows an indoor prompt. Otherwise it is the
/// exact incumbent island render.
#[allow(clippy::too_many_arguments)]
pub fn build_full(
    world: &GameWorld,
    cam: &Camera,
    selected: Option<usize>,
    hud: &Hud,
    evo: Option<&EvoHud>,
    interior: Option<&crate::interior::Interior>,
    sw: f32,
    sh: f32,
    time: f32,
    ui: f32,
) -> Scene {
    // `ui` is the HUD scale (device-pixel-ratio): the world renders into the full
    // backing buffer, so HUD chrome laid out in absolute pixels must be scaled by
    // dpr or it shrinks to half-size on a Retina display.
    let ui = ui.clamp(1.0, 3.0);
    // INSIDE A HOUSE: render the interior scene only (a separate, self-contained
    // little map), suppressing the island terrain + sea.
    if let Some(inter) = interior {
        return build_interior(inter, cam, sw, sh, time, ui);
    }
    let mut s = Scene::new();
    s.world_dims = (world.w, world.h);
    let aspect = sw / sh;
    s.sky = compute_sky(world, cam, aspect);
    let vp = Mat4(s.sky.view_proj);
    let (w, h) = (world.w, world.h);
    // Billboard axes face the camera: the iso angle in god-view, the live look
    // direction in first person (so glows always face you as you turn around).
    let axes = match cam.fp {
        Some(fp) => math::fp_axes(fp.yaw, fp.pitch),
        None => math::iso_axes(cam.cx, cam.cy, cam.yaw),
    };
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

    // ---- buildings: the minds' built `walls` footprint composed into real,
    // multi-floor architecture (modular stone/timber pieces + glowing windows).
    // LIVE-ONLY: reads `world.walls`/`world.tick` only — never writes the sim, so
    // the harness `walls`/`enclosure`/`shelter_gap` semantics are byte-identical.
    build_structures(&mut s, world, time, &g, &glow);

    // ---- SOCIETY (Sprint 4): village banners + inter-village relation links ----
    // Each VILLAGE plants a standard at its (live) territory centre, coloured by its
    // banner hue, so settlements read as distinct places. Between centres a soft link
    // shows the standing RELATION: a warm green-gold thread for ALLIES, a wary red
    // ember for ENEMIES/RIVALS — subtle, never gaudy. Empty (no draw) off a society
    // world, so a non-society render is the exact incumbent scene.
    if world.society {
        use crate::sim::RelationKind;
        // TERRITORY (faction control): each village claims a soft coloured ground-zone in
        // its banner hue, sized by how far its living members range from the centre — so a
        // glance shows WHO CONTROLS WHERE, and where two zones overlap reads as a contested
        // frontier. Drawn first (lowest) as additive ground-glow, so it tints the land
        // without ever occluding the minds, buildings, or wildlife standing on it.
        let nv = world.villages.len();
        let mut reach = vec![0.0f32; nv];
        for a in &world.agents {
            if !a.alive {
                continue;
            }
            if let Some(vid) = a.village {
                let v = &world.villages[vid as usize];
                let d = (a.rx - v.center.x as f32).hypot(a.ry - v.center.y as f32);
                let i = vid as usize;
                reach[i] = reach[i].max(d.min(40.0));
            }
        }
        for v in &world.villages {
            if v.population == 0 {
                continue;
            }
            let i = v.id as usize;
            let r = reach[i].max(7.0) + 3.0; // a generous margin past the outermost member
            let (cx, cz) = (v.center.x as f32, v.center.y as f32);
            let pulse = 0.55 + 0.45 * (time * 0.7 + v.id as f32 * 1.7).sin();
            // a flat ALPHA-BLENDED ground wash in the banner colour marks the claimed land
            // — finely TESSELLATED (rings × segments, height sampled per vertex + a small
            // lift) so it hugs the bumpy terrain instead of getting buried under it, and
            // additive glow just blows out on sunlit grass. Drawn here (before buildings /
            // minds) so figures on it render on top; overlapping washes read as a contested
            // frontier. Alpha eases out toward the rim for a soft edge.
            let segs = 36u32;
            let rings = 5u32;
            for ri in 0..rings {
                let f0 = ri as f32 / rings as f32;
                let f1 = (ri + 1) as f32 / rings as f32;
                let (r0, r1) = (r * f0, r * f1);
                let a_in = 0.52 * (1.0 - f0 * 0.6);
                let a_out = 0.52 * (1.0 - f1 * 0.6);
                let c_in = Color::hex(v.hue, a_in).0;
                let c_out = Color::hex(v.hue, a_out).0;
                for k in 0..segs {
                    let a0 = k as f32 / segs as f32 * std::f32::consts::TAU;
                    let a1 = (k + 1) as f32 / segs as f32 * std::f32::consts::TAU;
                    let p = |rr: f32, an: f32| {
                        let (x, z) = (cx + an.cos() * rr, cz + an.sin() * rr);
                        [x, g(x, z) + 0.12, z]
                    };
                    // inner edge uses c_in, outer edge c_out (push_tri is flat-shaded, so
                    // the per-ring step gives a gentle radial fade overall).
                    geo::push_tri(&mut s.lit, p(r0, a0), p(r1, a0), p(r1, a1), c_out);
                    geo::push_tri(&mut s.lit, p(r0, a0), p(r1, a1), p(r0, a1), c_in);
                }
            }
            // a bold, bright BORDER ring of motes around the territory edge — the clearest
            // "this land is theirs" signal, drawn as billboards above the ground so it is
            // never occluded by terrain; where two villages' rings overlap, the border
            // reads as a contested frontier. A double row makes a thick, legible band.
            let bsegs = segs * 3;
            for k in 0..bsegs {
                let ang = k as f32 / bsegs as f32 * std::f32::consts::TAU;
                for rr in [r, r - 0.6] {
                    let bx = cx + ang.cos() * rr;
                    let bz = cz + ang.sin() * rr;
                    glow(&mut s, [bx, g(bx, bz) + 0.22, bz], 0.8, Color::hex(v.hue, 0.85 * pulse));
                }
            }
        }
        // relation links first (drawn low, so banners sit on top).
        for r in &world.relations {
            let (va, vb) = (&world.villages[r.a as usize], &world.villages[r.b as usize]);
            if va.population == 0 || vb.population == 0 {
                continue;
            }
            let kind = r.kind();
            // only show meaningful relations — neutrals stay quiet.
            let col = match kind {
                RelationKind::Allied => Some((0x8fe6a8u32, 0.5)),
                RelationKind::Friendly => Some((0xbfe6c8u32, 0.26)),
                RelationKind::Enemy => Some((0xff6a5au32, 0.55)),
                RelationKind::Rival => Some((0xe69a7au32, 0.30)),
                RelationKind::Neutral => None,
            };
            let Some((hue, a0)) = col else { continue };
            let (ax, az) = (va.center.x as f32, va.center.y as f32);
            let (bx, bz) = (vb.center.x as f32, vb.center.y as f32);
            // a dotted thread of motes along the centre-to-centre line, breathing.
            let segs = 14u32;
            let pulse = 0.6 + 0.4 * (time * 1.4 + r.a as f32 + r.b as f32).sin();
            for k in 0..=segs {
                let t = k as f32 / segs as f32;
                let mx = ax + (bx - ax) * t;
                let mz = az + (bz - az) * t;
                let my = g(mx, mz) + 0.5 + 0.25 * (t * std::f32::consts::PI).sin();
                glow(&mut s, [mx, my, mz], 0.16, Color::hex(hue, a0 * pulse));
            }
        }
        // village standards: a slim pole + a glowing pennant in the banner hue.
        for v in &world.villages {
            if v.population == 0 {
                continue;
            }
            let (x, z) = (v.center.x as f32, v.center.y as f32);
            let gy = g(x, z);
            // pole.
            geo::push_box(&mut s.lit, [x, gy + 0.9, z], [0.04, 0.9, 0.04], 0.0, Color::hex(0x4a4438, 1.0).0);
            // pennant — a small bright banner near the top, in the village hue.
            geo::push_box(
                &mut s.lit,
                [x + 0.22, gy + 1.5, z],
                [0.22, 0.16, 0.02],
                0.0,
                Color::hex(v.hue, 1.0).0,
            );
            // a soft territory-tint glow at the heart, breathing, so the centre reads.
            let pulse = 0.6 + 0.4 * (time * 1.1 + v.id as f32 * 1.3).sin();
            glow(&mut s, [x, gy + 0.7, z], 1.6, Color::hex(v.hue, 0.12 * pulse));
            glow(&mut s, [x + 0.22, gy + 1.5, z], 0.4, Color::hex(v.hue, 0.7));
            // the village name floats over its standard at village zoom.
            if let Some((sx, sy)) = project(&vp, [x, gy + 2.0, z], sw, sh) {
                if cam.zoom < 34.0 {
                    let tw = v.name.chars().count() as f32 * 6.0 * ui;
                    s.text(&v.name, sx - tw * 0.5, sy, 11.0 * ui, Color::hex(v.hue, 0.9));
                }
            }
        }
    }

    // ---- CIVILIZATION CAPSTONE (Sprint 3): WONDERS, SPACE-AGE launchpads + ROCKETS,
    // and the MOON in the sky. Empty (no draw) off a civ world, so a non-civ render is the
    // exact incumbent scene. Drawn after the society banners so monuments sit on the land.
    if world.civ {
        build_civ(&mut s, world, time, &g, &glow, &axes);
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
    // ---- materials economy: quarry rocks (the stone source) ----
    // Warm stone outcrops the minds quarry for building stone; they shrink visibly as
    // they're worked and grow back as they replenish, so the stone half of the economy
    // reads at a glance. Empty (no draw) unless the materials economy is on.
    for r in &world.rocks {
        let (x, z) = (r.pos.x as f32, r.pos.y as f32);
        let gy = g(x, z);
        let s0 = 0.16 + 0.26 * r.stone; // size tracks how much stone is left
        // a clustered boulder: a main rounded block + two smaller shoulders, warm grey.
        geo::push_box(&mut s.lit, [x, gy + s0 * 0.5, z], [s0, s0 * 0.7, s0], 0.0, Color::hex(0x8d8579, 1.0).0);
        geo::push_box(&mut s.lit, [x + s0 * 0.7, gy + s0 * 0.3, z - s0 * 0.4], [s0 * 0.5, s0 * 0.42, s0 * 0.5], 0.0, Color::hex(0x787064, 1.0).0);
        geo::push_box(&mut s.lit, [x - s0 * 0.6, gy + s0 * 0.28, z + s0 * 0.5], [s0 * 0.45, s0 * 0.38, s0 * 0.45], 0.0, Color::hex(0x9a9286, 1.0).0);
    }
    // ---- materials economy: the village build-yard at the heart ----
    // A timber + stone stockpile by the hearth showing the village's materials on hand:
    // a stack of logs (wood) + a cairn of dressed stone, each scaled to the live stock.
    // Inert (no draw) unless the materials economy is on.
    if world.materials_econ {
        let (gx, gz) = (world.granary.x as f32, world.granary.y as f32);
        let gy = g(gx, gz);
        // log stack — height tracks the wood stockpile (capped so it stays a tidy pile).
        let woodf = (world.wood_stock / 40.0).clamp(0.0, 1.0);
        let logs = 1 + (woodf * 4.0).round() as i32;
        for i in 0..logs {
            let ly = gy + 0.10 + i as f32 * 0.13;
            geo::push_box(&mut s.lit, [gx - 1.4, ly, gz + 1.2], [0.5, 0.06, 0.12], 0.0, Color::hex(0x6e4a28, 1.0).0);
            geo::push_box(&mut s.lit, [gx - 1.4, ly, gz + 1.44], [0.5, 0.06, 0.12], 0.0, Color::hex(0x7d5530, 1.0).0);
        }
        // dressed-stone cairn — size tracks the stone stockpile.
        let stonef = (world.stone_stock / 28.0).clamp(0.0, 1.0);
        let sr = 0.16 + 0.30 * stonef;
        geo::push_box(&mut s.lit, [gx + 1.4, gy + sr * 0.5, gz + 1.3], [sr, sr * 0.6, sr], 0.0, Color::hex(0x9a9286, 1.0).0);
        geo::push_box(&mut s.lit, [gx + 1.4, gy + sr * 1.1, gz + 1.3], [sr * 0.6, sr * 0.4, sr * 0.6], 0.0, Color::hex(0x837b6f, 1.0).0);
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

    // ---- natural ecosystem: deer herd, wolf packs, bears (live-only; empty otherwise).
    // Drawn UNDER the agents so the minds read clearly on top. Deer first (ambient
    // background life), then the predators with a subtle threat aura.
    for d in &world.deer {
        if world.deer_hidden(d) {
            continue; // a caught deer is gone until it respawns
        }
        let (x, z) = (d.rx, d.ry);
        let gy = g(x, z);
        // gait gate: the render pos chases the grid cell, so the gap to the target is
        // a clean "is it moving" signal — wide while crossing cells, ~0 when grazing.
        let mv = anim_move01(d.rx, d.ry, d.pos.x as f32, d.pos.y as f32);
        push_deer(&mut s.lit, x, gy, z, d.heading, time, d.fleeing, mv);
        // a soft cool-white ground halo so the tan deer reads against the warm grass
        // and its tan terrain tufts (a cream halo would vanish into them) — gentle
        // enough to stay natural, brighter while it bolts.
        let da = if d.fleeing { 0.40 } else { 0.22 };
        glow(&mut s, [x, gy + 0.40, z], 0.85, Color::hex(0xd8ecff, da));
    }
    // sheep flock: small, round, woolly, cream — drawn as ambient life like the deer.
    for sh in &world.sheep {
        if world.sheep_hidden(sh) {
            continue;
        }
        let (x, z) = (sh.rx, sh.ry);
        let gy = g(x, z);
        let mv = anim_move01(sh.rx, sh.ry, sh.pos.x as f32, sh.pos.y as f32);
        push_sheep(&mut s.lit, x, gy, z, sh.heading, time, sh.fleeing, mv);
        // a cool-white ground halo so the cream flock reads against the warm grass
        // (a warm halo would vanish into it) — like the deer's halo, sized to the sheep.
        let sa = if sh.fleeing { 0.42 } else { 0.26 };
        glow(&mut s, [x, gy + 0.34, z], 0.8, Color::hex(0xeaf3ff, sa));
    }
    // horses: tall, sleek, brown, maned — fast roaming grazers, ambient like the deer.
    for hs in &world.horses {
        if world.horse_hidden(hs) {
            continue;
        }
        let (x, z) = (hs.rx, hs.ry);
        let gy = g(x, z);
        let mv = anim_move01(hs.rx, hs.ry, hs.pos.x as f32, hs.pos.y as f32);
        push_horse(&mut s.lit, x, gy, z, hs.heading, time, hs.fleeing, mv);
        // a warm amber ground halo marks the big chestnut horse against the grass.
        let ha = if hs.fleeing { 0.40 } else { 0.26 };
        glow(&mut s, [x, gy + 0.55, z], 1.2, Color::hex(0xf0c98a, ha));
    }
    for w in &world.wolves {
        let (x, z) = (w.rx, w.ry);
        let gy = g(x, z);
        let mv = anim_move01(w.rx, w.ry, w.pos.x as f32, w.pos.y as f32);
        push_wild_wolf(&mut s.lit, x, gy, z, w.heading, time, w.flash, mv);
        // a low, cool threat aura — far subtler than the stalker's red menace, but
        // enough to pick the grey pack out of the foliage.
        glow(&mut s, [x, gy + 0.40, z], 0.9 + 0.6 * w.flash, Color::hex(0x9fb6e0, 0.22 + 0.25 * w.flash));
    }
    for b in &world.bears {
        let (x, z) = (b.rx, b.ry);
        let gy = g(x, z);
        let mv = anim_move01(b.rx, b.ry, b.pos.x as f32, b.pos.y as f32);
        push_bear(&mut s.lit, x, gy, z, b.heading, time, b.flash, mv);
        // a warm amber aura marks the big solitary bear — the most dangerous animal.
        glow(&mut s, [x, gy + 0.55, z], 1.3 + 0.5 * b.flash, Color::hex(0xe09a4a, 0.26 + 0.28 * b.flash));
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
            // a peaceful NATURAL death (old age) departs in a warmer golden-hour light;
            // a violent death (predator/starvation) keeps the cool pale wisp. Both are
            // grievable — the warmth just reads as a gentle, earned farewell.
            let natural = a.death_cause == "old age";
            let memo_col = if natural { 0xf4d9a8 } else { 0xbfc8e0 };
            let wisp_col = if natural { 0xffdca0 } else { 0xd8e2f5 };
            glow(&mut s, [x, gy + 0.5, z], 0.7 + 0.4 * fresh, Color::hex(memo_col, memo));
            // a soul-wisp rising from a fresh grave: a few motes drifting up and
            // fading, so a death reads as something departing the world.
            if fresh > 0.01 {
                for k in 0..5u32 {
                    let ph = (age * 0.012 + k as f32 * 0.21).fract();
                    let sway = (age * 0.05 + k as f32 * 1.7).sin() * 0.18;
                    let rise = 0.6 + ph * 2.6;
                    let fade = (1.0 - ph) * fresh * 0.4;
                    // a natural passing rises a touch larger and softer — a calm exhale.
                    let sz = if natural { 0.26 } else { 0.22 };
                    glow(&mut s, [x + sway, gy + rise, z], sz, Color::hex(wisp_col, fade));
                }
            }
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
        // VILLAGE IDENTITY (Sprint 4 society): tint the mind's body toward its
        // village's banner hue, so a settlement reads as a coloured group at a
        // glance while each mind keeps a hint of its own persona accent. On a
        // non-society world `village` is `None`, so this is the exact incumbent accent.
        let accent = match world.village_of(i) {
            Some(v) => Color::hex(blend_rgb(a.accent, v.hue, 0.62), 1.0),
            None => Color::hex(a.accent, 1.0),
        };
        let drive = drive_color(dom);
        let mood = Color::hex(a.mind.affect().hue(), 1.0);
        let breath = 0.5 + 0.5 * (time * 1.6 + i as f32 * 1.7).sin();
        // a faster heartbeat-style pulse for the soul-spark so each mind visibly
        // *lives* — the orbs breathe rather than sit static.
        let pulse = 0.5 + 0.5 * (time * 2.4 + i as f32 * 2.3).sin();

        // motion trail — a comet-tail of drive-coloured motes fading behind it.
        let tn = a.trail.len();
        for (k, &(wx, wy)) in a.trail.iter().enumerate() {
            let f = (k + 1) as f32 / (tn + 1) as f32;
            glow(&mut s, [wx, g(wx, wy) + 0.28, wy], 0.30 * f, drive.with_a(0.22 * f));
            glow(&mut s, [wx, g(wx, wy) + 0.28, wy], 0.12 * f, Color::hex(0xfff4e0, 0.18 * f));
        }

        // CHILDREN read as smaller figures that grow: scale the whole body/head/glow
        // by maturity (newborn ≈ 0.18 → adult 1.0). On a non-lifecycle world every
        // mind is maturity 1.0, so this is the exact incumbent size. `sc` maps
        // maturity into [~0.45, 1.0] so even a newborn is visible, not a speck.
        let sc = 0.45 + 0.55 * a.maturity;
        // HAPPINESS warms the light: a content mind glows a touch brighter/larger, a
        // miserable one dims — a readable felt-state without being gaudy. Reads the
        // world's display happiness (well-being + family + safety). On a
        // non-lifecycle world this is the flat-neutral 0.5 ⇒ `hap_b ≈ 1.0` (no-op).
        let hap = world.happiness_of(a);
        let hap_b = 0.78 + 0.44 * hap; // ≈ [0.78, 1.22] brightness/size factor

        // WALKING DRIVER: derive a heading + a stride strength from the mind's own
        // recent motion. The sim lerps `(rx,ry)` toward the grid cell and lays down a
        // `trail` breadcrumb when it moves; comparing the live render position to the
        // most-recent breadcrumb gives a movement vector this frame — direction =
        // facing, magnitude = how hard it's walking. A mind that's standing still has
        // ~zero vector ⇒ stride 0 ⇒ it stands; a mind crossing the island strides.
        // Purely a read of render state — no sim is touched.
        let (mvx, mvz) = match a.trail.last() {
            Some(&(lx, ly)) => (x - lx, z - ly),
            None => (0.0, 0.0),
        };
        let mspeed = mvx.hypot(mvz);
        let heading = if mspeed > 1e-4 { mvz.atan2(mvx) } else { 0.0 };
        // stride ramps in over a small speed window so a dawdle is a gentle step and a
        // purposeful walk is a full stride; smoothed so it never snaps on/off.
        let stride = smoothstep(0.03, 0.32, mspeed);
        // each mind walks on its OWN beat: a per-entity phase offset + a cadence that
        // ticks with logical time (faster while striding harder), so the village never
        // marches in lockstep. Phase is logical-time-derived ⇒ deterministic.
        let cadence = 6.0 + 5.0 * stride;
        let phase = time * cadence + i as f32 * 2.39996; // golden-angle offset → well spread

        // the body — a real little villager (torso + head + swinging limbs), torso in
        // the mind's drive/village colour so it still reads as a coloured figure, head
        // in warm "skin". Scaled by maturity (children small, bigger-headed). It walks
        // when moving and stands when idle. A faint breath bob keeps an idle mind alive.
        let idle_bob = (1.0 - stride) * (0.5 + 0.5 * breath) * 0.02;
        geo::push_villager(
            &mut s.lit,
            x,
            gy + idle_bob * sc,
            z,
            heading,
            sc,
            phase,
            stride,
            accent.0,
            Color::hex(0xf0d9b8, 1.0).0,
        );

        // WARFARE (live-only): a mustered WARRIOR carries an era-appropriate weapon in
        // hand and wears a hostile red battle aura; on a clash / shot its weapon_flash
        // spikes and the renderer pops a clash spark (melee) or a muzzle flash + tracer
        // (ranged). Drawn only for minds the world reports as warriors, so peacetime /
        // non-war views draw nothing extra. The weapon scales with the village's era.
        if world.war && a.warband.is_some() {
            if let Some(v) = world.village_of(i) {
                let weapon = v.era.weapon();
                let code: u8 = match weapon {
                    crate::sim::Weapon::Club => 0,
                    crate::sim::Weapon::Sword => 1,
                    crate::sim::Weapon::Musket => 2,
                    crate::sim::Weapon::Energy => 3,
                };
                geo::push_weapon(&mut s.lit, x, gy + idle_bob * sc, z, heading, sc, phase, stride, code);
                // a low, hostile red aura marks this mind as AT WAR (distinct from the
                // drive halo) — a warband reads as a red-lit cluster marching the border.
                glow(&mut s, [x, gy + 0.5 * sc, z], 1.1 * sc, Color::hex(0xff4534, 0.20));
                // the BATTLE flash: a clash spark (melee) or muzzle flash + tracer (ranged).
                if a.weapon_flash > 0.02 {
                    let f = a.weapon_flash;
                    let muzzle = geo::weapon_muzzle(x, gy + idle_bob * sc, z, heading, sc, weapon.is_ranged());
                    if weapon.is_ranged() {
                        // bright muzzle flash + a short hot tracer streaking forward.
                        glow(&mut s, muzzle, (0.42 + 0.25 * f) * sc, Color::hex(0xffe39a, 0.95 * f));
                        glow(&mut s, muzzle, 0.18 * sc, Color::hex(0xffffff, 0.9 * f));
                        let (sh, ch) = heading.sin_cos();
                        for k in 1..=4 {
                            let d = k as f32 * 0.6 * sc;
                            let tp = [muzzle[0] + d * ch, muzzle[1], muzzle[2] + d * sh];
                            glow(&mut s, tp, 0.10 * sc, Color::hex(0xfff2c0, 0.5 * f * (1.0 - k as f32 / 5.0)));
                        }
                    } else {
                        // a sharp metallic clash spark at the weapon, white-hot core.
                        glow(&mut s, muzzle, (0.35 + 0.3 * f) * sc, Color::hex(0xffd14a, 0.9 * f));
                        glow(&mut s, muzzle, 0.16 * sc, Color::hex(0xffffff, 0.85 * f));
                    }
                }
            }
        }

        // ROLE / TRADE (live-only render aid): give each grown civilian a profession
        // the eye can read — a tool in hand + a hat. Warriors already carry weapons
        // (above) and CHILDREN stay bare small figures (the maturity scale `sc`
        // already shrinks them), so this draws only for adults who aren't mustered.
        // The village LEADER reads as an elder/chief (staff + circlet); everyone else
        // gets a stable per-mind trade (hunter / farmer / builder) keyed off their
        // slot so a villager keeps the same job frame to frame. Honest scope: warrior
        // and chief are real sim state; the hunter/farmer/builder split is a cosmetic
        // layer over civilians (the sim has forager/builder behaviours, not guilds).
        // Pure read of render state — no sim is touched.
        let is_warrior = world.war && a.warband.is_some();
        if !is_warrior && a.maturity >= 0.85 {
            let is_leader = world.village_of(i).and_then(|v| v.leader) == Some(a.id);
            let trade: u8 = if is_leader {
                3
            } else {
                // SplitMix-style bit-mix on the slot so the three civilian trades
                // spread evenly (a plain multiply-shift clustered onto one trade).
                let mut hsh = (i as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
                hsh = (hsh ^ (hsh >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
                hsh = (hsh ^ (hsh >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
                hsh ^= hsh >> 31;
                (hsh % 3) as u8
            };
            geo::push_trade(&mut s.lit, x, gy + idle_bob * sc, z, heading, sc, phase, stride, trade);
        }

        // THE GLOW — each mind still glows, but now it is a CHARACTER that glows
        // rather than a featureless orb: the aura BACKLIGHTS the little figure and a
        // bright soul-spark floats just above its head (the unmistakable point of
        // living light) instead of a hot core blowing out the body. So the villager
        // reads as a person, and the "minds glow" signature is kept as a halo + a
        // hovering mind-light. Layered additive orbs still stack toward HDR at the
        // spark so it blooms in the post pass. Size scales with maturity (children
        // glow smaller), brightness with happiness.
        let top = geo::villager_crown(sc) + gy; // crown of the figure (just above the head)
        if !noglow() {
        // a wide, soft, body-height mood aura that frames the figure without hiding it.
        glow(&mut s, [x, gy + 0.55 * sc, z], (1.9 + 0.5 * breath) * sc, mood.with_a(0.14 * hap_b));
        // a drive-coloured halo wrapping the torso — saturated but translucent, so the
        // character shows through (its alpha is well under the old opaque rings).
        glow(&mut s, [x, gy + 0.55 * sc, z], (1.05 + 0.2 * pulse) * sc, drive.with_a(0.38 * hap_b));
        // the SOUL-SPARK: a small bright mind-light hovering over the head, breathing.
        // Stacked additive layers push it HDR so it blooms — the living-light read,
        // now lifted clear of the face.
        let spark = top + (0.16 + 0.03 * breath) * sc;
        glow(&mut s, [x, spark, z], (0.30 + 0.05 * pulse) * sc, drive.with_a(0.95));
        glow(&mut s, [x, spark, z], 0.20 * sc, Color::hex(0xfffaf0, 1.0));
        glow(&mut s, [x, spark, z], 0.13 * sc, Color::hex(0xffffff, 1.0)); // (stack → super-bright)
        // a coloured pool of light cast on the ground beneath each mind, so it feels
        // like it is genuinely emitting light onto the island.
        glow(&mut s, [x, gy + 0.05, z], 1.2 * sc, drive.with_a(0.32));
        } // end !noglow

        // selection: a bright pale halo
        if Some(i) == selected {
            glow(&mut s, [x, gy + 0.62, z], 1.8 + 0.2 * breath, Color::hex(PAPER, 0.35));
        }
        // invented-goal (Praxis): a gold bloom
        if a.mind.acting_on_invented() {
            glow(&mut s, [x, gy + 0.75, z], 1.4 + 0.2 * breath, Color::hex(0xf0c24e, 0.45));
        }
        // process flash: a bright burst — reflex red / deliberate violet — that
        // pops the mind for a moment when it reacts or deliberates.
        if a.flash > 0.02 {
            let fc = match a.flash_kind {
                Process::Reflex => Color::hex(0xff3030, 0.8 * a.flash),
                _ => Color::hex(0x9b6bff, 0.7 * a.flash),
            };
            glow(&mut s, [x, gy + 0.62, z], 1.3 + 1.1 * a.flash, fc);
            glow(&mut s, [x, gy + 0.62, z], 0.5, Color::hex(0xffffff, 0.6 * a.flash));
        }

        // PAIR-BOND TETHER: a soft warm thread of motes between romantic partners —
        // a readable "these two are together" without being gaudy. Drawn once per
        // couple (from the lower-id partner) and only when the partner is alive. On a
        // non-lifecycle world no mind has a partner, so this never draws.
        if let Some(pid) = a.partner {
            if a.id.0 < pid.0 {
                if let Some(p) = world.agents.iter().find(|b| b.id == pid && b.alive) {
                    let (qx, qz) = (p.rx, p.ry);
                    let drift = (time * 0.4).fract();
                    // a gentle warm gold (love), arcing slightly above the ground.
                    for k in 0..8 {
                        let f = (k as f32 + drift) / 8.0;
                        if f >= 1.0 {
                            continue;
                        }
                        let px = x + (qx - x) * f;
                        let pz = z + (qz - z) * f;
                        let lift = (f * std::f32::consts::PI).sin() * 0.35;
                        // brightest in the middle of the link, soft at the ends.
                        let a2 = (1.0 - (f - 0.5).abs() * 1.6).max(0.1) * 0.30;
                        glow(&mut s, [px, g(px, pz) + 0.55 + lift, pz], 0.13, Color::hex(0xffc98a, a2));
                    }
                }
            }
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
        // Float the name just above the figure's crown + soul-spark so it never
        // overlaps the (now taller) character; scales with maturity so a child's
        // label hugs its smaller head.
        if let Some((sx, sy)) = project(&vp, [x, geo::villager_crown(sc) + gy + 0.35 * sc, z], sw, sh) {
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

    // ---- mist wreathing the floating island's shoreline ----
    // The island floats in sky and sea, so ring its coast with low, slow-drifting
    // banks of cool haze — it sells the "floating diorama" read and softens the
    // land/water seam into something atmospheric instead of a hard edge.
    {
        let (cx, cz) = (w as f32 * 0.5, h as f32 * 0.5);
        let radius = (w.max(h) as f32) * 0.62;
        let ring = 40;
        for k in 0..ring {
            let base = k as f32 / ring as f32 * TAU;
            // jitter each bank's angle/radius/phase so the ring isn't a clean circle.
            let jh = geo::hash_unit(k as u64, 5);
            let jr = geo::hash_unit(k as u64, 6);
            let ang = base + (time * 0.03 + jh * TAU);
            let rr = radius * (0.86 + 0.14 * jr) + 1.5 * (time * 0.15 + jh * TAU).sin();
            let mx = cx + ang.cos() * rr;
            let mz = cz + ang.sin() * rr;
            let drift = 0.5 + 0.5 * (time * 0.4 + jh * 6.0).sin();
            let y = geo::SEA_Y + 0.6 + 0.5 * drift;
            geo::push_billboard(
                &mut s.add,
                [mx, y, mz],
                3.4 + 1.2 * drift,
                1.8 + 0.6 * drift,
                axes.0,
                axes.1,
                Color::hex(0xcfe0f0, 0.05 + 0.04 * drift).0,
            );
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
        top_bar(&mut chrome, world, hud, cam.is_fp(), sw / ui);
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
    // Reject points behind the camera. Under the orthographic god-view `w` is
    // always 1, so this is a no-op there; under the first-person PERSPECTIVE a
    // point behind the eye has `w <= 0` and must not project (it would mirror to a
    // bogus on-screen spot).
    if c[3] <= 1e-5 {
        return None;
    }
    let ndc = [c[0] / c[3], c[1] / c[3]];
    Some(((ndc[0] * 0.5 + 0.5) * sw, (1.0 - (ndc[1] * 0.5 + 0.5)) * sh))
}

/// Compose the built `walls` footprint into real, multi-floor buildings.
///
/// The minds build a ring of `walls` cells for shelter (emergent — the shelter
/// drive under stalker pressure). This is a LIVE-ONLY *render* of that footprint:
/// each contiguous (8-connected) cluster of built cells becomes one building —
/// ground walls with a door + windows, an interior floor and a roof, and, once
/// it is large and *established* enough, a second/third storey with a real
/// staircase and upper windows. Height grows over a run from a deterministic
/// live-only "establishment" metric (footprint size + sim tick), so settled
/// shelters visibly become tall buildings — WITHOUT touching the sim fields the
/// harness reads (`walls`, `enclosure`, `shelter_gap` are never mutated here).
/// The kind of building a wall-cluster reads as — inferred from its footprint so the
/// village shows real variety. Live-only (everything is a `Home` unless the materials
/// economy is on); a purely cosmetic classification, never fed back into the sim.
#[derive(Clone, Copy, PartialEq, Eq)]
enum BuildKind {
    /// A dwelling: compact footprint, warm pitched roof, glowing windows.
    Home,
    /// A long mead-hall: elongated footprint, low long-ridged roof, a row of windows.
    Longhouse,
    /// A store-hall near the village heart: raised plinth, steep conical thatch roof.
    Granary,
    /// A lookout: slim, very tall, a crenellated flat top with a beacon glow.
    Watchtower,
}

/// Map a village's [`Era`] to the material palette its buildings are raised in
/// (Civilization Sprint 1). Stone keeps the warm hand-built look; each rung up the
/// ladder swaps materials (thatch → lime-wash → dressed stone → red brick → brushed
/// metal/glass), so a settlement's architecture announces how far it has come.
fn era_style(era: crate::sim::Era) -> crate::geo::pieces::EraStyle {
    use crate::geo::pieces;
    use crate::sim::Era;
    match era {
        Era::Stone => pieces::ERA_STONE,
        Era::Bronze => pieces::ERA_BRONZE,
        Era::Iron => pieces::ERA_IRON,
        Era::Industrial => pieces::ERA_INDUSTRIAL,
        Era::Space => pieces::ERA_SPACE,
    }
}

fn build_structures(
    s: &mut Scene,
    world: &GameWorld,
    time: f32,
    g: &impl Fn(f32, f32) -> f32,
    glow: &impl Fn(&mut Scene, [f32; 3], f32, Color),
) {
    use crate::geo::pieces::{self, Facing};
    use std::collections::HashSet;

    if world.walls.is_empty() {
        return;
    }
    let cells: &HashSet<Pos> = &world.walls;
    let inside = |p: Pos| cells.contains(&p);
    let storey_h = 1.05f32; // one storey, world units (reads as a real floor)

    // 8-connected clustering into footprints. Deterministic: cells are visited in
    // a sorted order so the same world always yields the same buildings.
    let mut sorted: Vec<Pos> = cells.iter().copied().collect();
    sorted.sort_by_key(|p| (p.y, p.x));
    let mut seen: HashSet<Pos> = HashSet::new();

    for &start in &sorted {
        if seen.contains(&start) {
            continue;
        }
        // flood-fill this cluster (deterministic stack order via sorted neighbours).
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

        // footprint bbox + a stable per-cluster hash (lowest cell) for jitter.
        let (mut x0, mut x1, mut z0, mut z1) = (i32::MAX, i32::MIN, i32::MAX, i32::MIN);
        for p in &comp {
            x0 = x0.min(p.x);
            x1 = x1.max(p.x);
            z0 = z0.min(p.y);
            z1 = z1.max(p.y);
        }
        // Clamp the rendered envelope to a human scale: the minds build large, sparse
        // rings, but an 8×8 building reads as a roofless courtyard. Cap each axis and
        // centre the building on the footprint, so a big sparse cluster becomes a
        // solid, well-proportioned house that grows UP (storeys) rather than sprawling.
        // With the MATERIALS ECONOMY on, a well-supplied village raises BIGGER footprints
        // (a larger cap) — real, large buildings, not just huts. Live-only render choice.
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
        let chash = ((anchor.x as i64) << 20 ^ anchor.y as i64) as u64;
        let jit = |k: u32| geo::hash_unit(chash, k);
        // The building's VISUAL size is its bounding-box envelope (the perimeter we
        // wall), so scale height by the bbox footprint, not the raw cell count (a few
        // cells flung to opposite corners still make a big house).
        let bw = (x1 - x0 + 1) as f32;
        let bd = (z1 - z0 + 1) as f32;
        let span = bw.max(bd);
        let area = bw * bd;
        let cells = comp.len() as f32; // raw blocks the minds laid in this cluster

        // BUILDING TYPE (live-only, deterministic) — give the village real variety so it
        // reads as a settlement of distinct buildings, not one repeated hut. Inferred
        // from the footprint's shape + a stable per-cluster hash, so the same world always
        // yields the same mix. Only diversified when the materials economy is on (the
        // showcase); otherwise everything is a Home, exactly as before.
        let elong = span / span.min(bw).min(bd).max(1.0); // long axis / short axis
        let centre = Pos::new((x0 + x1) / 2, (z0 + z1) / 2);
        let near_heart = centre.manhattan(world.granary) <= 18;
        // The minds build mostly square shelter-rings, so reading type purely from the
        // emergent footprint yields almost all "homes". To make the settlement read as a
        // real mix, the TYPE is chosen from a stable per-cluster hash (so the same world
        // always gives the same building in the same place), nudged by footprint cues:
        //   • a genuinely long footprint is always a longhouse;
        //   • a big footprint near the village heart tends toward a granary store-hall;
        //   • a small compact footprint can spike into a watchtower;
        //   • everything else is a home (the commonest, as a village should be).
        let roll = jit(91); // stable 0..1 per cluster
        // a granary store-hall wants a decent footprint; it favours the village heart but
        // need not sit on it (a store-hall can stand anywhere among the homes).
        let granary_ok = area >= 9.0 && (roll < if near_heart { 0.55 } else { 0.22 });
        let btype = if !world.materials_econ {
            BuildKind::Home
        } else if elong >= 2.0 && span >= 4.0 {
            BuildKind::Longhouse
        } else if granary_ok {
            BuildKind::Granary
        } else if area <= 12.0 && cells >= 3.0 && roll > 0.78 {
            BuildKind::Watchtower
        } else if area >= 16.0 && roll > 0.62 {
            // a big settled compound reads as a longhouse hall too.
            BuildKind::Longhouse
        } else {
            BuildKind::Home
        };

        // TECH ERA (Civilization Sprint 1): which era's architecture this cluster wears —
        // the era of the nearest village centre. `era_at` returns `Era::Stone` when eras
        // are off, so a non-era world renders exactly as before (Stone == the incumbent
        // warm-timber look's closest match; Iron is the old sandstone). The era picks the
        // material palette every era-aware piece below paints with, plus per-era roof
        // crowns (chimneys, smokestacks, domes), so a village's buildings visibly EVOLVE
        // as it climbs the ladder — and you can SEE which settlement has advanced.
        let era = world.era_at(centre);
        let style = era_style(era);

        // ESTABLISHMENT (live-only, deterministic): how settled this structure is,
        // 0..1. Bigger footprints establish; time on the island raises it further so
        // a long-standing shelter keeps growing upward even after its ring is closed
        // (when the minds stop building because shelter_gap == None). Derived purely
        // from render-time data; never fed back into the sim.
        let size_est = smoothstep(4.0, 22.0, area);
        // Ramp establishment over the first ~900 ticks (~2 min at the live speed) so a
        // watcher sees the village visibly rise into multi-floor buildings within a
        // couple of minutes, rather than waiting many minutes for the top storeys.
        let age_est = smoothstep(150.0, 900.0, world.tick as f32);
        let establish = ((0.5 + 0.5 * age_est) * size_est.max(0.2)).clamp(0.0, 1.0);
        // stories scale with the building's footprint AND establishment: small huts
        // stay 1 storey; larger, settled compounds rise to 2-3 floors. The MATERIALS
        // ECONOMY lets a well-supplied village raise TALLER buildings, and each type has
        // its own profile: watchtowers spike tall, longhouses stay low + long, granaries
        // and homes climb a few floors. Live-only render choice; the sim is unaffected.
        let max_stories = if world.materials_econ {
            match btype {
                BuildKind::Watchtower => 5,
                BuildKind::Longhouse => 2,
                BuildKind::Granary => 3,
                BuildKind::Home => {
                    if area >= 20.0 {
                        4
                    } else if area >= 9.0 {
                        3
                    } else {
                        2
                    }
                }
            }
        } else if area >= 16.0 {
            3
        } else if area >= 6.0 {
            2
        } else {
            1
        };
        // a watchtower establishes fast (it's a slim spike, built up not out), so it
        // reaches near-full height sooner; others ramp with footprint + age as before.
        let est_t = if matches!(btype, BuildKind::Watchtower) {
            establish.max(age_est)
        } else {
            establish
        };
        let stories = (1 + (est_t * (max_stories - 1) as f32).round() as usize)
            .min(max_stories)
            .max(1);

        // ground footing reference height: the lowest ground under the footprint, so
        // the whole building sits on one level pad (no floating on a slope).
        let mut base_y = f32::MAX;
        for p in &comp {
            base_y = base_y.min(g(p.x as f32, p.y as f32));
        }
        if base_y == f32::MAX {
            base_y = 0.0;
        }

        // pick a single doorway cell + a set of window cells deterministically. The
        // door is on the most-south outward face of the lowest cell; windows on a
        // sampling of the other outward faces.
        let door_cell = comp
            .iter()
            .filter(|p| !inside(Pos::new(p.x, p.y + 1)))
            .max_by_key(|p| (p.y, p.x))
            .copied()
            .unwrap_or(anchor);

        // a foundation pad slab under the whole footprint so the interior reads as a
        // floor, not bare grass, and the walls have a plinth to stand on.
        pieces::floor_slab(
            &mut s.lit,
            x0 as f32 - 0.5,
            z0 as f32 - 0.5,
            x1 as f32 + 0.5,
            z1 as f32 + 0.5,
            base_y + 0.02,
            0.10,
            pieces::STONE_DARK,
        );

        // Walls, storey by storey. The minds build sparse/partial rings, so rather
        // than render each loose cell (which reads as scattered rubble), we treat the
        // footprint's bounding box as the building ENVELOPE and lay a CONTINUOUS
        // perimeter wall around it, cell by cell along each of the four sides. This
        // turns a rough emergent footprint into a clean, solid, multi-floor building
        // while still being driven entirely by what the minds built (cluster + size).
        //
        // Each perimeter slot gets a deterministic role: the door slot (one, ground
        // floor, on the south face nearest the mind-built door cell), windows
        // (sampled), or solid wall. Interior faces never exist here (it's the outer
        // envelope), so there is no doubled geometry / z-fighting.
        let mut window_lights: Vec<[f32; 3]> = Vec::new();
        // build the ordered list of perimeter (cell, facing) slots once.
        let mut slots: Vec<(i32, i32, Facing)> = Vec::new();
        for x in x0..=x1 {
            slots.push((x, z0, Facing::North));
        }
        for z in z0..=z1 {
            slots.push((x1, z, Facing::East));
        }
        for x in (x0..=x1).rev() {
            slots.push((x, z1, Facing::South));
        }
        for z in (z0..=z1).rev() {
            slots.push((x0, z, Facing::West));
        }
        // choose the door slot: the south-facing slot whose cell is closest to the
        // mind-built door cell (so the entrance sits where the minds actually opened).
        let door_idx = slots
            .iter()
            .enumerate()
            .filter(|(_, (_, z, f))| *f == Facing::South && *z == z1)
            .min_by_key(|(_, (x, _, _))| (x - door_cell.x).abs())
            .map(|(i, _)| i);

        // upper storeys step IN slightly so the silhouette tapers and each floor is
        // visibly its own course (a stacked tower reads as stacked, not one tall box).
        let inset = 0.14f32;
        let cxm = (x0 + x1) as f32 * 0.5;
        let czm = (z0 + z1) as f32 * 0.5;
        for st in 0..stories {
            let y0 = base_y + 0.05 + st as f32 * storey_h;
            let ins = st as f32 * inset;
            // shrink the floor footprint for this storey's terrace below it.
            let (fx0, fz0, fx1, fz1) = (
                x0 as f32 - 0.5 + ins,
                z0 as f32 - 0.5 + ins,
                x1 as f32 + 0.5 - ins,
                z1 as f32 + 0.5 - ins,
            );
            for (si, (cx, cz, face)) in slots.iter().enumerate() {
                // pull each slot toward the building centre by the storey inset.
                let cxf = *cx as f32 + (cxm - *cx as f32).signum() * ins.min((*cx as f32 - cxm).abs());
                let czf = *cz as f32 + (czm - *cz as f32).signum() * ins.min((*cz as f32 - czm).abs());
                let seed = jit(si as u32 * 7 + st as u32 * 131);
                if st == 0 && Some(si) == door_idx {
                    pieces::wall_door_era(&mut s.lit, cxf, y0, czf, *face, storey_h, seed, &style);
                    continue;
                }
                // windows: deterministic sampling. Denser on upper floors (each storey
                // wants light); the ground floor is more solid (it's the base). The
                // Space age glazes nearly everything (a glass-curtain look).
                let wt = jit(si as u32 * 13 + st as u32 * 101 + 17);
                let win_thr = if matches!(era, crate::sim::Era::Space) {
                    if st == 0 { 0.30 } else { 0.15 }
                } else if st == 0 { 0.55 } else { 0.40 };
                let want_window = wt > win_thr;
                if want_window {
                    let c = pieces::wall_window_era(&mut s.lit, cxf, y0, czf, *face, storey_h, seed, &style);
                    window_lights.push(c);
                } else {
                    pieces::wall_segment_era(&mut s.lit, cxf, y0, czf, *face, storey_h, seed, &style);
                }
            }

            // an interior floor slab for this storey (a touch inside the envelope so
            // it tucks behind the walls — the floor you'd stand on / the ceiling
            // below). A thin timber ledge marks where an upper storey steps in.
            if x1 > x0 && z1 > z0 {
                let fy = if st == 0 { base_y + 0.06 } else { y0 - 0.02 };
                pieces::floor_slab(&mut s.lit, fx0 + 0.18, fz0 + 0.18, fx1 - 0.18, fz1 - 0.18, fy, 0.06, pieces::FLOOR);
                // a slim string-course ledge at the base of each upper storey (the
                // step-in), so floors are legibly separate without opening a courtyard.
                if st >= 1 {
                    let (lx0, lz0, lx1, lz1) = (
                        x0 as f32 - 0.5 + (st - 1) as f32 * inset,
                        z0 as f32 - 0.5 + (st - 1) as f32 * inset,
                        x1 as f32 + 0.5 - (st - 1) as f32 * inset,
                        z1 as f32 + 0.5 - (st - 1) as f32 * inset,
                    );
                    pieces::floor_slab(&mut s.lit, lx0, lz0, lx1, lz1, y0 - 0.03, 0.05, pieces::TIMBER);
                }
            }
        }

        // an EXTERNAL stone stair climbing the south face to the first upper floor —
        // a real, visible staircase (interior flights would be hidden by the roof).
        // It hugs the wall and lands at a small terrace by an upper door, so a
        // multi-storey building plainly shows how you get up. Placed deterministically.
        if stories >= 2 {
            // run it up against the south wall, offset along X by a stable jitter.
            let sx = x0 as f32 + 0.6 + (x1 - x0).max(0) as f32 * (0.2 + 0.5 * jit(3));
            let sz = z1 as f32 + 0.78; // just outside the south wall
            // climb one storey of real stepped treads, facing the wall (+Z run handled
            // inside the piece). Then a small landing slab at the top.
            pieces::staircase(&mut s.lit, sx, base_y + 0.06, sz, storey_h, 6);
            pieces::floor_slab(
                &mut s.lit,
                sx - 0.24,
                z1 as f32 + 0.28,
                sx + 0.24,
                z1 as f32 + 0.62,
                base_y + 0.05 + storey_h,
                0.07,
                pieces::STONE,
            );
            // a slim handrail post at the foot, for a touch of structure.
            pieces::pillar(&mut s.lit, sx + 0.22, base_y + 0.06, sz, storey_h * 0.5);
        }

        // ROOF on the top storey — sized to the (inset) top storey footprint so it
        // caps the building neatly. The roof is the strongest read of building TYPE, so
        // each kind gets its own silhouette: watchtowers a crenellated battlement crown
        // with a beacon; granaries a tall conical thatch; longhouses a long low pitch;
        // homes a warm pitched roof (or a flat terrace once big + tall).
        let top_y = base_y + 0.05 + stories as f32 * storey_h;
        let tins = (stories - 1) as f32 * inset;
        let rx0 = x0 as f32 - 0.5 + tins;
        let rz0 = z0 as f32 - 0.5 + tins;
        let rx1 = x1 as f32 + 0.5 - tins;
        let rz1 = z1 as f32 + 0.5 - tins;
        // beacon glow position (set by the watchtower crown), lit below with the windows.
        let mut beacon: Option<[f32; 3]> = None;
        // ERA roof crowns (Civilization Sprint 1): the Industrial age caps homes/halls
        // with smoking CHIMNEYS and big buildings with a factory SMOKESTACK; the Space
        // age crowns blocks with a glowing glass DOME. Collected here, lit/puffed below.
        let mut smoke: Vec<[f32; 3]> = Vec::new();
        let mut domes: Vec<[f32; 3]> = Vec::new();
        let rcx = (rx0 + rx1) * 0.5;
        let rcz = (rz0 + rz1) * 0.5;
        let is_industrial = matches!(era, crate::sim::Era::Industrial);
        let is_space = matches!(era, crate::sim::Era::Space);
        match btype {
            BuildKind::Watchtower => {
                let deck = pieces::battlement(&mut s.lit, rx0, rz0, rx1, rz1, top_y);
                beacon = Some([deck[0], deck[1] + 0.18, deck[2]]);
                // a Space-age watchtower also wears a small sensor dome on its deck.
                if is_space {
                    domes.push(pieces::dome(&mut s.lit, deck[0], deck[2], 0.26, deck[1] + 0.12, &style));
                }
            }
            BuildKind::Granary => {
                let radius = ((rx1 - rx0).max(rz1 - rz0)) * 0.5 + 0.16;
                if is_space {
                    // a store-hall in the Space age is a domed silo.
                    domes.push(pieces::dome(&mut s.lit, rcx, rcz, radius, top_y, &style));
                } else if is_industrial {
                    // an Industrial granary is a brick mill with a tall smokestack.
                    pieces::flat_roof_era(&mut s.lit, rx0, rz0, rx1, rz1, top_y, &style);
                    let sh = 1.0 + 0.4 * span.clamp(2.0, 6.0) / 6.0;
                    smoke.push(pieces::smokestack(&mut s.lit, rcx, rcz, top_y + 0.05, sh, &style));
                } else {
                    // a tall straw/shingle cone centred on the top footprint.
                    let peak = 0.6 + 0.22 * span.clamp(2.0, 6.0) / 6.0;
                    pieces::thatch_cone(&mut s.lit, rcx, rcz, radius, top_y, peak);
                }
            }
            BuildKind::Longhouse => {
                if is_space {
                    pieces::flat_roof_era(&mut s.lit, rx0, rz0, rx1, rz1, top_y, &style);
                    domes.push(pieces::dome(&mut s.lit, rcx, rcz, ((rx1 - rx0).min(rz1 - rz0)) * 0.5, top_y, &style));
                } else {
                    // a long, low pitch running the hall's length.
                    let peak = 0.42 + 0.10 * jit(8);
                    pieces::pitched_roof_era(&mut s.lit, rx0, rz0, rx1, rz1, top_y, peak, &style);
                    if is_industrial {
                        // a hall in the machine age streams smoke from an end chimney.
                        smoke.push(pieces::chimney(&mut s.lit, rx1 - 0.3, rcz, top_y + peak * 0.5, 0.5, &style));
                    }
                }
            }
            BuildKind::Home => {
                let big = area >= 20.0 && stories >= 3;
                if is_space {
                    // a sleek metal-and-glass block crowned with a dome.
                    pieces::flat_roof_era(&mut s.lit, rx0, rz0, rx1, rz1, top_y, &style);
                    domes.push(pieces::dome(&mut s.lit, rcx, rcz, ((rx1 - rx0).min(rz1 - rz0)) * 0.5 + 0.05, top_y, &style));
                } else if big {
                    pieces::flat_roof_era(&mut s.lit, rx0, rz0, rx1, rz1, top_y, &style);
                    for (px, pz) in [(rx0 + 0.2, rz0 + 0.2), (rx1 - 0.2, rz1 - 0.2), (rx0 + 0.2, rz1 - 0.2), (rx1 - 0.2, rz0 + 0.2)] {
                        pieces::pillar(&mut s.lit, px, top_y + 0.1, pz, 0.4);
                    }
                    if is_industrial {
                        smoke.push(pieces::smokestack(&mut s.lit, rcx, rcz, top_y + 0.10, 0.9, &style));
                    }
                } else {
                    // a modest pitch that grows a touch with span.
                    let peak = 0.38 + 0.16 * jit(8) + 0.14 * (span - 3.0).clamp(0.0, 4.0) / 4.0;
                    pieces::pitched_roof_era(&mut s.lit, rx0, rz0, rx1, rz1, top_y, peak, &style);
                    if is_industrial {
                        // the Industrial-age cottage signature: a brick chimney puffing soot.
                        let cx2 = rx0 + (rx1 - rx0) * (0.28 + 0.4 * jit(45));
                        smoke.push(pieces::chimney(&mut s.lit, cx2, rcz, top_y + peak * 0.45, 0.55, &style));
                    }
                }
            }
        }

        // corner posts the full height, so tall towers have visible structure and a
        // crisp vertical silhouette (and they hide any wall-seam at the corners).
        let h_total = stories as f32 * storey_h;
        for (px, pz) in [
            (x0 as f32 - 0.46, z0 as f32 - 0.46),
            (x1 as f32 + 0.46, z0 as f32 - 0.46),
            (x0 as f32 - 0.46, z1 as f32 + 0.46),
            (x1 as f32 + 0.46, z1 as f32 + 0.46),
        ] {
            pieces::pillar(&mut s.lit, px, base_y + 0.05, pz, h_total);
        }

        // GLOWING WINDOWS: an additive bloom at each window pane so they read as lit
        // from within. Warm hearth-light through the early eras; a COOL electric blue-
        // white in the Space age (the strongest night-time read of an advanced village).
        // Brighter as the sun lowers (the glow carries the dusk).
        let warmth = (1.0 - s.sky.daylight) * 0.6 + 0.4;
        let (g_outer, g_inner) = if is_space {
            (0x6fb8ff, 0xdff0ff) // cool electric
        } else {
            (0xffb24e, 0xfff0d0) // warm hearth
        };
        for c in &window_lights {
            glow(s, *c, 0.22, Color::hex(g_outer, 0.28 * warmth));
            glow(s, *c, 0.10, Color::hex(g_inner, 0.40 * warmth));
        }
        // a watchtower's BEACON: a small fire-pot on the crown, glowing warm — the
        // lookout reads as lit + watching at the golden hour and into the dusk.
        if let Some(b) = beacon {
            pieces::pillar(&mut s.lit, b[0], b[1] - 0.18, b[2], 0.16);
            glow(s, b, 0.30, Color::hex(0xff8a2a, 0.55 * warmth + 0.18));
            glow(s, b, 0.14, Color::hex(0xffe2a0, 0.6 * warmth + 0.2));
        }
        // INDUSTRIAL smoke: a soft soot plume drifting up from each chimney/stack — a
        // few translucent grey puffs, animated up + sideways on _time so a machine-age
        // village reads as working. Live render only.
        for sm in &smoke {
            for p in 0..3u32 {
                let t = (time * 0.45 + sm[0] * 0.7 + p as f32 * 0.6).fract();
                let rise = 0.18 + t * 0.85;
                let drift = (t * 2.4 + p as f32).sin() * 0.18;
                let r = 0.12 + t * 0.20;
                let fade = (1.0 - t) * 0.30;
                glow(s, [sm[0] + drift, sm[1] + rise, sm[2]], r, Color::hex(0x9a9690, fade));
            }
        }
        // SPACE dome glow: a cool electric halo on each glass cupola so the far-future
        // blocks read as luminous tech, day or night.
        for d in &domes {
            glow(s, *d, 0.34, Color::hex(0x7fd0ff, 0.30 + 0.18 * warmth));
            glow(s, *d, 0.16, Color::hex(0xeaf6ff, 0.40 + 0.18 * warmth));
        }
    }
}

/// CIVILIZATION CAPSTONE render (Sprint 3): the MOON in the sky, each advanced village's
/// WONDER, a Space-era village's LAUNCHPAD, and any ROCKET in flight (rising on a plume
/// + arcing to the moon). Live-only — reads only civ-gated world state, writes nothing to
/// the sim, so the harness scene is byte-identical (it never arms civ). Called from
/// `build_full` only when `world.civ`.
#[allow(clippy::too_many_arguments)]
fn build_civ(
    s: &mut Scene,
    world: &GameWorld,
    time: f32,
    g: &impl Fn(f32, f32) -> f32,
    glow: &impl Fn(&mut Scene, [f32; 3], f32, Color),
    axes: &([f32; 3], [f32; 3]),
) {
    use crate::sim::WonderKind;

    // ---- the MOON: a pale disc high over the island, the rockets' destination. Placed
    // off to one side of the map at altitude, gently drifting, so it always reads as "the
    // moon up there" and rockets visibly arc TOWARD it. A lit core + a soft additive halo.
    let (mw, mh) = (world.w as f32, world.h as f32);
    let moon_pos = [mw * 0.22, 19.0 + 1.0 * (time * 0.1).sin(), mh * 0.12];
    // a solid pale disc (a low-seg cone seen edge-on reads as a flat moon face); the
    // additive halo + glow give it body and a cool corona.
    geo::push_billboard(&mut s.add, moon_pos, 4.4, 4.4, axes.0, axes.1, Color::hex(0xdfe6f5, 0.95).0);
    geo::push_billboard(&mut s.add, moon_pos, 3.2, 3.2, axes.0, axes.1, Color::hex(0xf2f6ff, 0.6).0);
    glow(s, moon_pos, 9.0, Color::hex(0xbcd0f5, 0.16));
    glow(s, moon_pos, 14.0, Color::hex(0x95b4e6, 0.08));

    // ---- WONDERS: a great monument at each village that has raised one. Bigger than any
    // building, tinted by the village hue, and ramping up from the ground over its first
    // ~300 ticks so it reads as RISING when it appears. Three silhouettes (pyramid / spire
    // / rotunda) so the island shows variety. Topped with a bright capstone glow.
    for v in &world.villages {
        let Some(w) = &v.wonder else { continue };
        if v.population == 0 {
            continue;
        }
        let (cx, cz) = (v.center.x as f32, v.center.y as f32);
        let gy = g(cx, cz);
        // rise ramp: 0→1 over ~300 ticks from when it was raised.
        let age = world.tick.saturating_sub(w.raised) as f32;
        let rise = smoothstep(0.0, 300.0, age).max(0.04);
        let hue = Color::hex(v.hue, 1.0).0;
        let stone = [hue[0] * 0.55 + 0.30, hue[1] * 0.55 + 0.30, hue[2] * 0.55 + 0.30, 1.0];
        let trim = [hue[0] * 0.9 + 0.1, hue[1] * 0.9 + 0.1, hue[2] * 0.9 + 0.1, 1.0];
        let cap; // the bright apex point for the crowning glow
        match w.kind {
            WonderKind::Pyramid => {
                // a stepped great pyramid: 5 shrinking stone tiers.
                let tiers = 5;
                let base = 3.4f32;
                let tier_h = 0.95 * rise;
                for t in 0..tiers {
                    let f = t as f32 / tiers as f32;
                    let half = base * (1.0 - f) * 0.9;
                    let y0 = gy + 0.1 + t as f32 * tier_h;
                    geo::push_box(
                        &mut s.lit,
                        [cx, y0 + tier_h * 0.5, cz],
                        [half, tier_h * 0.5, half],
                        0.0,
                        if t % 2 == 0 { stone } else { trim },
                    );
                }
                cap = [cx, gy + 0.1 + tiers as f32 * tier_h + 0.2, cz];
                // a golden capstone.
                geo::push_box(&mut s.lit, [cx, cap[1] - 0.1, cz], [0.4 * rise, 0.4 * rise, 0.4 * rise], 0.0, [0.95, 0.82, 0.35, 1.0]);
            }
            WonderKind::Spire => {
                // a soaring obelisk: a tall tapering shaft on a small plinth + a spike.
                let h = 9.0 * rise;
                geo::push_box(&mut s.lit, [cx, gy + 0.25, cz], [1.7, 0.25, 1.7], 0.0, stone);
                geo::push_box(&mut s.lit, [cx, gy + 0.5 + h * 0.5, cz], [0.7, h * 0.5, 0.7], 0.0, stone);
                geo::push_box(&mut s.lit, [cx, gy + 0.5 + h, cz], [0.45, 0.5, 0.45], 0.0, trim);
                geo::push_cone(&mut s.lit, cx, gy + 0.5 + h + 1.0, cz, 0.5, 2.0 * rise, 6, [0.95, 0.82, 0.35, 1.0]);
                cap = [cx, gy + 0.5 + h + 3.0 * rise, cz];
            }
            WonderKind::Rotunda => {
                // a grand rotunda: a wide cylindrical drum ringed by pillars + a cupola.
                let drum_h = 3.2 * rise;
                let r = 3.0f32;
                geo::push_box(&mut s.lit, [cx, gy + 0.2, cz], [r + 0.4, 0.2, r + 0.4], 0.0, stone);
                // drum (octagonal-ish via a low-seg cone-as-cylinder is overkill; a box drum reads fine)
                geo::push_box(&mut s.lit, [cx, gy + 0.4 + drum_h * 0.5, cz], [r, drum_h * 0.5, r], 0.0, stone);
                // a ring of pillars around the drum.
                let np = 8;
                for i in 0..np {
                    let a = i as f32 / np as f32 * std::f32::consts::TAU;
                    let px = cx + a.cos() * (r + 0.5);
                    let pz = cz + a.sin() * (r + 0.5);
                    geo::push_box(&mut s.lit, [px, gy + 0.4 + drum_h * 0.5, pz], [0.22, drum_h * 0.5, 0.22], 0.0, trim);
                }
                // the cupola.
                let dome_y = gy + 0.4 + drum_h;
                geo::push_cone(&mut s.lit, cx, dome_y, cz, r * 0.95, 2.2 * rise, 12, trim);
                cap = [cx, dome_y + 2.4 * rise, cz];
                geo::push_box(&mut s.lit, [cx, cap[1] - 0.15, cz], [0.3 * rise, 0.3 * rise, 0.3 * rise], 0.0, [0.95, 0.82, 0.35, 1.0]);
            }
        }
        // a bright crowning glow on the wonder's apex (breathing) — a beacon visible afar.
        let pulse = 0.6 + 0.4 * (time * 0.8 + v.id as f32).sin();
        glow(s, cap, 1.6, Color::hex(v.hue, 0.6 * pulse));
        glow(s, cap, 0.6, Color::hex(0xfff0c0, 0.85 * pulse));
    }

    // ---- SPACE AGE: a LAUNCHPAD at every Space-era village, and any ROCKET aloft.
    for v in &world.villages {
        if v.population == 0 || v.era != crate::sim::Era::Space {
            continue;
        }
        // the launchpad sits a few cells off the village centre so it doesn't collide with
        // the wonder/standard: a dark concrete pad + a slim gantry tower.
        let (cx, cz) = (v.center.x as f32 + 4.0, v.center.y as f32 + 4.0);
        let gy = g(cx, cz);
        geo::push_box(&mut s.lit, [cx, gy + 0.12, cz], [1.6, 0.12, 1.6], 0.0, [0.22, 0.23, 0.26, 1.0]);
        // pad markings (a brighter inner square).
        geo::push_box(&mut s.lit, [cx, gy + 0.20, cz], [1.0, 0.04, 1.0], 0.0, [0.55, 0.55, 0.20, 1.0]);
        // gantry tower beside the pad.
        let gx = cx + 1.3;
        for st in 0..5 {
            let yy = gy + 0.3 + st as f32 * 0.7;
            geo::push_box(&mut s.lit, [gx, yy, cz], [0.08, 0.35, 0.08], 0.0, [0.5, 0.52, 0.56, 1.0]);
        }
        geo::push_box(&mut s.lit, [gx, gy + 3.8, cz], [0.08, 0.05, 0.5], 0.0, [0.5, 0.52, 0.56, 1.0]);
        // a faint hazard glow on the pad.
        glow(s, [cx, gy + 0.3, cz], 0.7, Color::hex(0xffcf4e, 0.18));
    }

    // ---- ROCKETS in flight: each rises on a plume and ARCS toward the moon, then fades.
    for r in &world.rockets {
        let age = world.tick.saturating_sub(r.launched) as f32;
        let total = 240.0; // ROCKET_FLIGHT_TICKS
        let ph = (age / total).clamp(0.0, 1.0); // 0=liftoff … 1=arrived
        let (px, pz) = (r.pad.x as f32, r.pad.y as f32);
        let gy = g(px, pz);
        // arc: interpolate ground pad → moon, lofted on a parabola so it rises then crosses.
        // bearing jitter rotates the launch line slightly around vertical-toward-moon.
        let to_moon = [moon_pos[0] - px, moon_pos[2] - pz];
        let mlen = (to_moon[0] * to_moon[0] + to_moon[1] * to_moon[1]).sqrt().max(0.001);
        let dir = [to_moon[0] / mlen, to_moon[1] / mlen];
        // rotate horizontal direction by the per-rocket bearing for variety.
        let (cb, sb) = (r.bearing.cos(), r.bearing.sin());
        let hd = [dir[0] * cb - dir[1] * sb, dir[0] * sb + dir[1] * cb];
        // ease-out so it leaps off the pad then sails: position along the line.
        let along = ph * ph * (3.0 - 2.0 * ph); // smoothstep ease
        let rx = px + hd[0] * mlen * along;
        let rz = pz + hd[1] * mlen * along;
        // altitude: a high parabolic loft toward the moon's altitude.
        let apex = moon_pos[1];
        let ry = gy + 1.0 + (apex - gy) * (along.powf(0.75));
        // rocket orientation: tilt from vertical toward the flight direction as it arcs.
        // (we render a simple upright-ish body; tilt via a small lean on the nose.)
        let lean = ph * 0.5;
        let body_c = [0.92, 0.93, 0.96, 1.0];
        // body: a slim white cylinder (approximated by a tall thin box).
        geo::push_box(&mut s.lit, [rx, ry, rz], [0.18, 0.55, 0.18], 0.0, body_c);
        // a coloured band in the village hue.
        geo::push_box(&mut s.lit, [rx, ry + 0.2, rz], [0.19, 0.12, 0.19], 0.0, Color::hex(world.villages.get(r.village as usize).map(|v| v.hue).unwrap_or(0xffffff), 1.0).0);
        // nose cone.
        geo::push_cone(&mut s.lit, rx, ry + 0.55, rz, 0.18, 0.5, 6, [0.85, 0.2, 0.18, 1.0]);
        // three little fins at the base.
        for i in 0..3 {
            let a = i as f32 / 3.0 * std::f32::consts::TAU + lean;
            geo::push_box(&mut s.lit, [rx + a.cos() * 0.22, ry - 0.5, rz + a.sin() * 0.22], [0.1, 0.18, 0.04], a, [0.8, 0.2, 0.18, 1.0]);
        }
        // EXHAUST PLUME: a bright hot core just under the rocket + a trailing tail of smoke
        // back down toward the pad along the arc, fading with distance. Reads as thrust.
        let plume_n = 9;
        for i in 0..plume_n {
            let f = i as f32 / plume_n as f32;
            // sample a point behind the rocket along the arc (toward the pad).
            let back_ph = (ph - f * 0.12).max(0.0);
            let ba = back_ph * back_ph * (3.0 - 2.0 * back_ph);
            let bx = px + hd[0] * mlen * ba;
            let bz = pz + hd[1] * mlen * ba;
            let by = gy + 1.0 + (apex - gy) * (ba.powf(0.75));
            let flick = 0.7 + 0.3 * (time * 18.0 + i as f32).sin();
            let hot = 1.0 - f;
            // hot near the nozzle (orange/white), cooling to grey smoke down the trail.
            let col = if f < 0.35 {
                Color::hex(0xffd060, (0.7 * hot) * flick)
            } else {
                Color::hex(0xb8b8c0, (0.34 * (1.0 - f)) * flick)
            };
            glow(s, [bx, by - 0.55, bz], 0.5 + f * 0.9, col);
        }
        // a bright muzzle flash right at the nozzle for the "lift" punch.
        glow(s, [rx, ry - 0.7, rz], 1.1, Color::hex(0xfff0b0, 0.8 * (1.0 - ph * 0.5)));
        // a faint smoke pillar still hanging at the pad just after liftoff.
        if ph < 0.4 {
            let s_a = (0.4 - ph) / 0.4;
            for k in 0..4 {
                let yy = gy + 0.4 + k as f32 * 0.5;
                glow(s, [px, yy, pz], 0.7, Color::hex(0xc8c8d0, 0.3 * s_a));
            }
        }
    }
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

/// Place a body part for a heading-oriented animal: `fwd` is along the animal's
/// nose (+), `side` is to its left(+)/right(−), `up` is height. Rotates the local
/// (fwd, side) offset by `heading` into world (x, z) and pushes a box there. Lets the
/// natural wildlife meshes turn to face the way they move.
#[allow(clippy::too_many_arguments)]
fn push_part(
    out: &mut Vec<LitVertex>,
    x: f32,
    gy: f32,
    z: f32,
    heading: f32,
    sc: f32,
    fwd: f32,
    side: f32,
    up: f32,
    half: [f32; 3],
    color: [f32; 4],
) {
    let (sh, ch) = heading.sin_cos();
    // a uniform `sc` scale enlarges the whole animal (offsets, height, and box size)
    // so it reads clearly at the village zoom against the busy terrain.
    let (fwd, side, up) = (fwd * sc, side * sc, up * sc);
    let half = [half[0] * sc, half[1] * sc, half[2] * sc];
    // heading 0 ⇒ nose toward +x. fwd along heading, side 90° left of it.
    let wx = x + fwd * ch - side * sh;
    let wz = z + fwd * sh + side * ch;
    geo::push_box(out, [wx, gy + up, wz], half, heading, color);
}

/// Gait gate for a heading-oriented animal: the render position lerps toward the
/// grid cell each frame, so the *gap* between the smoothed render pos and the grid
/// target is a clean "is it moving" signal — wide while the animal is crossing
/// cells, ~0 when it has settled. Mapped through a smoothstep into [0,1] so the
/// gait eases in/out instead of snapping. Pure read of render state.
#[inline]
fn anim_move01(rx: f32, ry: f32, tx: f32, ty: f32) -> f32 {
    smoothstep(0.04, 0.45, (rx - tx).hypot(ry - ty))
}

/// A swung four-legged limb: a leg whose foot strides fore/aft about its hip by
/// `swing` radians (a believable trotting plant-and-lift), gated by `mv` so it
/// only strides while the animal moves and stands square when idle. `fz/sx` are
/// the hip's fwd/side offset, `leg_h` the leg half-height, `up` the hip height.
#[allow(clippy::too_many_arguments)]
fn push_leg(out: &mut Vec<LitVertex>, x: f32, gy: f32, z: f32, heading: f32, sc: f32, fz: f32, sx: f32, up: f32, leg_h: f32, swing: f32, color: [f32; 4]) {
    let (ss, cs) = swing.sin_cos();
    // the foot swings forward and lifts as the leg rotates about the hip.
    let f = fz + ss * leg_h;
    let u = up - cs * leg_h * 0.5;
    push_part(out, x, gy, z, heading, sc, f, sx, u.max(0.04), [0.05, leg_h * 0.5, 0.05], color);
}

/// A natural wolf: a lean grey pack hunter, oriented by `heading`, with a four-beat
/// loping gait. Distinct from the dark red-eyed stalker `push_wolf`: cooler grey fur,
/// no menace eyes, just a wild animal. `flash` brightens it on a strike. `mv` ∈ [0,1]
/// is how hard it's moving — the legs trot when it runs and stand square when idle.
fn push_wild_wolf(out: &mut Vec<LitVertex>, x: f32, gy: f32, z: f32, heading: f32, time: f32, flash: f32, mv: f32) {
    // a fast trot phase, gated so an idle wolf doesn't paddle on the spot.
    let phase = time * 12.0 + x * 1.7;
    let lope = phase.sin() * 0.05 * mv;
    let warm = flash * 0.5;
    let fur = Color::hex(0xa7adba, 1.0).0; // cool light grey — reads against the grass
    let fur = [fur[0] + warm, fur[1] + warm * 0.5, fur[2] + warm * 0.4, 1.0];
    let dark = Color::hex(0x686672, 1.0).0; // darker grey
    let snout = Color::hex(0x3f3d45, 1.0).0;
    let sc = 1.5; // enlarged so the pack reads clearly at the village zoom
    // body (long axis along fwd) — a slight bob with the lope.
    push_part(out, x, gy, z, heading, sc, 0.0, 0.0, 0.40 + lope, [0.34, 0.15, 0.17], fur);
    // shoulders a touch taller
    push_part(out, x, gy, z, heading, sc, 0.14, 0.0, 0.46 + lope, [0.16, 0.15, 0.16], fur);
    // head + snout out front
    push_part(out, x, gy, z, heading, sc, 0.34, 0.0, 0.48, [0.13, 0.13, 0.13], dark);
    push_part(out, x, gy, z, heading, sc, 0.50, 0.0, 0.44, [0.09, 0.07, 0.08], snout);
    // ears
    push_part(out, x, gy, z, heading, sc, 0.30, 0.07, 0.60, [0.03, 0.05, 0.03], dark);
    push_part(out, x, gy, z, heading, sc, 0.30, -0.07, 0.60, [0.03, 0.05, 0.03], dark);
    // bushy tail trailing
    push_part(out, x, gy, z, heading, sc, -0.42, 0.0, 0.46, [0.16, 0.05, 0.05], fur);
    // four legs trotting on a diagonal beat (front-left + rear-right together).
    let sw = phase.sin() * 0.55 * mv;
    for (fz, sx, d) in [(0.22f32, 0.13f32, 1.0f32), (0.22, -0.13, -1.0), (-0.24, 0.13, -1.0), (-0.24, -0.13, 1.0)] {
        push_leg(out, x, gy, z, heading, sc, fz, sx, 0.30, 0.20, sw * d, dark);
    }
}

/// A bear: big, brown, powerful, solitary — a bulky body, broad head, stumpy legs,
/// rounded ears. Oriented by `heading`, a slow heavy amble. `flash` warms it on a maul.
/// `mv` ∈ [0,1] gates the lumbering stride (a still bear stands square).
fn push_bear(out: &mut Vec<LitVertex>, x: f32, gy: f32, z: f32, heading: f32, time: f32, flash: f32, mv: f32) {
    // slow, heavy cadence — the bear ambles, it does not trot.
    let phase = time * 6.0 + z * 1.3;
    let sway = phase.sin() * 0.04 * mv;
    let warm = flash * 0.5;
    let coat = Color::hex(0x5a3a22, 1.0).0; // warm brown
    let coat = [coat[0] + warm, coat[1] + warm * 0.4, coat[2] + warm * 0.2, 1.0];
    let dark = Color::hex(0x3e2716, 1.0).0;
    let muzzle = Color::hex(0x6b4a30, 1.0).0;
    let sc = 1.8; // a bear is big — clearly the largest animal on the island
    // a big bulky body
    push_part(out, x, gy, z, heading, sc, 0.0, 0.0, 0.50 + sway, [0.40, 0.28, 0.26], coat);
    // a hump at the shoulders (grizzly silhouette)
    push_part(out, x, gy, z, heading, sc, 0.16, 0.0, 0.66, [0.18, 0.16, 0.20], coat);
    // broad head out front, low — sways gently with the amble.
    push_part(out, x, gy, z, heading, sc, 0.42, 0.0, 0.50 + sway * 0.5, [0.18, 0.16, 0.18], dark);
    push_part(out, x, gy, z, heading, sc, 0.58, 0.0, 0.44 + sway * 0.5, [0.10, 0.09, 0.11], muzzle);
    // round ears
    push_part(out, x, gy, z, heading, sc, 0.38, 0.13, 0.70, [0.05, 0.05, 0.05], dark);
    push_part(out, x, gy, z, heading, sc, 0.38, -0.13, 0.70, [0.05, 0.05, 0.05], dark);
    // four heavy legs with a slow, low-amplitude lope (diagonal beat).
    let sw = phase.sin() * 0.36 * mv;
    for (fz, sx, d) in [(0.26f32, 0.20f32, 1.0f32), (0.26, -0.20, -1.0), (-0.26, 0.20, -1.0), (-0.26, -0.20, 1.0)] {
        push_leg(out, x, gy, z, heading, sc, fz, sx, 0.36, 0.22, sw * d, dark);
    }
}

/// A deer: tan, slender, antlered — a graceful body on long thin legs, a raised neck
/// and a small head with a branching antler rack, and a flick of white tail. Oriented
/// by `heading`. When `fleeing`, it poses alert/bounding (head higher, longer stride).
/// `mv` ∈ [0,1] gates the stride so a grazing deer stands and a travelling one steps;
/// a fleeing deer bounds hard regardless.
fn push_deer(out: &mut Vec<LitVertex>, x: f32, gy: f32, z: f32, heading: f32, time: f32, fleeing: bool, mv: f32) {
    // a fleeing deer bounds at full tilt; otherwise stride tracks how hard it's moving.
    let drive = if fleeing { 1.0 } else { mv };
    let cadence = if fleeing { 16.0 } else { 9.0 };
    let phase = time * cadence + x * 1.9;
    let bound = phase.sin() * if fleeing { 0.10 } else { 0.04 } * drive;
    let tan = Color::hex(0xcf9a5e, 1.0).0; // warm tan — a shade brighter than the grass
    let pale = Color::hex(0xeed3a6, 1.0).0;
    let dark = Color::hex(0x4a3320, 1.0).0;
    let antler = Color::hex(0xb89a6e, 1.0).0;
    let neck_up = if fleeing { 0.78 } else { 0.70 };
    let sc = 1.45; // graceful but legible at the village zoom
    // slim body, rises a touch on each bound
    push_part(out, x, gy, z, heading, sc, 0.0, 0.0, 0.50 + bound.max(0.0), [0.26, 0.12, 0.14], tan);
    // raised neck (forward + up)
    push_part(out, x, gy, z, heading, sc, 0.24, 0.0, neck_up, [0.06, 0.12, 0.06], tan);
    // small head + muzzle
    push_part(out, x, gy, z, heading, sc, 0.30, 0.0, neck_up + 0.16, [0.07, 0.07, 0.08], pale);
    push_part(out, x, gy, z, heading, sc, 0.40, 0.0, neck_up + 0.14, [0.04, 0.04, 0.05], dark);
    // ears
    push_part(out, x, gy, z, heading, sc, 0.26, 0.08, neck_up + 0.24, [0.02, 0.05, 0.02], tan);
    push_part(out, x, gy, z, heading, sc, 0.26, -0.08, neck_up + 0.24, [0.02, 0.05, 0.02], tan);
    // a small branching antler rack (a couple of tines each side)
    push_part(out, x, gy, z, heading, sc, 0.28, 0.06, neck_up + 0.30, [0.015, 0.10, 0.015], antler);
    push_part(out, x, gy, z, heading, sc, 0.28, -0.06, neck_up + 0.30, [0.015, 0.10, 0.015], antler);
    push_part(out, x, gy, z, heading, sc, 0.34, 0.10, neck_up + 0.36, [0.05, 0.015, 0.015], antler);
    push_part(out, x, gy, z, heading, sc, 0.34, -0.10, neck_up + 0.36, [0.05, 0.015, 0.015], antler);
    // white scut tail
    push_part(out, x, gy, z, heading, sc, -0.26, 0.0, 0.52, [0.04, 0.05, 0.04], pale);
    // four long thin legs, a diagonal trot that lengthens into a bound when fleeing.
    let sw = phase.sin() * if fleeing { 0.7 } else { 0.5 } * drive;
    for (fz, sx, d) in [(0.18f32, 0.10f32, 1.0f32), (0.18, -0.10, -1.0), (-0.20, 0.10, -1.0), (-0.20, -0.10, 1.0)] {
        push_leg(out, x, gy, z, heading, sc, fz, sx, 0.40, 0.22, sw * d, dark);
    }
}

/// A sheep: small, round, woolly — a fat cream fleece body with a small darker face and
/// dark stubby legs. Oriented by `heading`; a gentle, small-stepped gait (slower cadence
/// and shorter swing than the deer). `mv` ∈ [0,1] gates the stride so a grazing sheep
/// stands square; `fleeing` quickens it a touch. Clearly distinct from the deer: shorter,
/// rounder, woolly white instead of slender tan.
fn push_sheep(out: &mut Vec<LitVertex>, x: f32, gy: f32, z: f32, heading: f32, time: f32, fleeing: bool, mv: f32) {
    let drive = if fleeing { 1.0 } else { mv };
    let cadence = if fleeing { 10.0 } else { 6.0 }; // slow, mincing steps
    let phase = time * cadence + x * 1.5;
    let bob = phase.sin() * 0.025 * drive;
    let wool = Color::hex(0xfdf8ee, 1.0).0; // bright woolly cream / near-white — pops on grass
    let wool_d = Color::hex(0xece2d0, 1.0).0; // slightly shaded fleece
    let face = Color::hex(0x352d26, 1.0).0; // dark face/legs (strong contrast vs the fleece)
    let sc = 1.4; // small and low, but legible at the village zoom (the smallest grazer)
    // a fat, round woolly body (wide + tall, short along fwd → reads as a blob of fleece)
    push_part(out, x, gy, z, heading, sc, 0.0, 0.0, 0.42 + bob, [0.24, 0.20, 0.22], wool);
    // a fleece cap on top so the silhouette domes (round + woolly)
    push_part(out, x, gy, z, heading, sc, -0.02, 0.0, 0.58 + bob, [0.18, 0.10, 0.18], wool_d);
    // a small dark face out front, low (sheep graze head-down)
    push_part(out, x, gy, z, heading, sc, 0.24, 0.0, 0.40, [0.08, 0.09, 0.08], face);
    // little ears
    push_part(out, x, gy, z, heading, sc, 0.20, 0.09, 0.46, [0.03, 0.03, 0.05], face);
    push_part(out, x, gy, z, heading, sc, 0.20, -0.09, 0.46, [0.03, 0.03, 0.05], face);
    // a tiny tail nub
    push_part(out, x, gy, z, heading, sc, -0.24, 0.0, 0.40, [0.05, 0.05, 0.04], wool);
    // four short dark stubby legs, a gentle small-stepped gait.
    let sw = phase.sin() * if fleeing { 0.45 } else { 0.30 } * drive;
    for (fz, sx, d) in [(0.13f32, 0.12f32, 1.0f32), (0.13, -0.12, -1.0), (-0.16, 0.12, -1.0), (-0.16, -0.12, 1.0)] {
        push_leg(out, x, gy, z, heading, sc, fz, sx, 0.22, 0.16, sw * d, face);
    }
}

/// A horse: tall, sleek, chestnut/brown — a long body on long legs, an arched neck, a
/// small head, a flowing MANE down the neck and a long TAIL. Oriented by `heading`; a
/// cantering gait (longer stride, taller carriage than the deer). `mv` gates the stride;
/// `fleeing` opens it into a gallop. Clearly distinct: the biggest, leggiest grazer,
/// sleek brown with a mane and tail rather than antlers.
fn push_horse(out: &mut Vec<LitVertex>, x: f32, gy: f32, z: f32, heading: f32, time: f32, fleeing: bool, mv: f32) {
    let drive = if fleeing { 1.0 } else { mv };
    let cadence = if fleeing { 14.0 } else { 8.0 };
    let phase = time * cadence + x * 1.6;
    let canter = phase.sin() * if fleeing { 0.09 } else { 0.05 } * drive;
    let coat = Color::hex(0x9c5e28, 1.0).0; // rich chestnut — warm, reads against the grass
    let coat_d = Color::hex(0x6e451f, 1.0).0;
    let mane = Color::hex(0x2f1c0e, 1.0).0; // near-black mane/tail (strong silhouette accent)
    let muzzle = Color::hex(0x59391d, 1.0).0;
    let neck_up = if fleeing { 0.92 } else { 0.86 };
    let sc = 1.95; // tall — clearly the largest grazer, taller and leggier than a deer
    // a long sleek body, rises a touch on each canter beat
    push_part(out, x, gy, z, heading, sc, 0.0, 0.0, 0.58 + canter.max(0.0), [0.34, 0.15, 0.17], coat);
    // a rounded rump behind
    push_part(out, x, gy, z, heading, sc, -0.26, 0.0, 0.56 + canter.max(0.0), [0.16, 0.15, 0.16], coat_d);
    // a strong arched neck (forward + up)
    push_part(out, x, gy, z, heading, sc, 0.30, 0.0, neck_up, [0.10, 0.16, 0.08], coat);
    // a mane crest running down the back of the neck
    push_part(out, x, gy, z, heading, sc, 0.24, 0.0, neck_up + 0.10, [0.04, 0.16, 0.05], mane);
    // a small head + muzzle out at the top of the neck
    push_part(out, x, gy, z, heading, sc, 0.42, 0.0, neck_up + 0.12, [0.08, 0.09, 0.07], coat);
    push_part(out, x, gy, z, heading, sc, 0.52, 0.0, neck_up + 0.08, [0.05, 0.06, 0.05], muzzle);
    // ears
    push_part(out, x, gy, z, heading, sc, 0.38, 0.06, neck_up + 0.22, [0.02, 0.05, 0.02], coat_d);
    push_part(out, x, gy, z, heading, sc, 0.38, -0.06, neck_up + 0.22, [0.02, 0.05, 0.02], coat_d);
    // a long flowing tail trailing low behind the rump
    push_part(out, x, gy, z, heading, sc, -0.40, 0.0, 0.44, [0.06, 0.18, 0.05], mane);
    // four long legs, a cantering diagonal beat.
    let sw = phase.sin() * if fleeing { 0.8 } else { 0.55 } * drive;
    for (fz, sx, d) in [(0.24f32, 0.12f32, 1.0f32), (0.24, -0.12, -1.0), (-0.26, 0.12, -1.0), (-0.26, -0.12, 1.0)] {
        push_leg(out, x, gy, z, heading, sc, fz, sx, 0.46, 0.26, sw * d, coat_d);
    }
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

fn top_bar(s: &mut Scene, world: &GameWorld, hud: &Hud, is_fp: bool, sw: f32) {
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
    if is_fp {
        // first-person "drop in" badge — make the live mode unmistakable.
        s.text("◉ FIRST-PERSON", sw - 160.0, 21.0, 12.0, Color::hex(0xffc24e, 1.0));
    } else if hud.quantum {
        s.text("⚛ QUANTUM DECISION MODE", sw - 200.0, 21.0, 12.0, Color::hex(0xb98cff, 1.0));
    } else {
        s.text("trained policy", sw - 130.0, 21.0, 12.0, Color::hex(0x5fd6a0, 0.9));
    }
    // The hint line names the controls for the current mode (the V toggle is shown
    // in both so the player can always find their way in / out of the walk-through).
    let hint = if is_fp {
        "FIRST-PERSON · WASD walk · mouse look · Shift run · V / Esc exit to god-view · space pauses"
    } else {
        "click an agent · WASD / drag to pan · scroll zooms to cursor · V drop in (first-person) · space pauses · [ / ] speed · F feed · Q quantum"
    };
    s.text(hint, 50.0, 37.0, 10.5, Color::hex(MUTED, 0.9));
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
        let (title, intro) = if world.society {
            (
                "THE SOCIETY",
                "A living island of minds gathered into VILLAGES — kin who settle together. Over time they intermarry into ALLIANCES and contest borders into RIVALRIES, all running the real Daimon cognitive cycle. Click any mind to open it.",
            )
        } else {
            (
                "THE VILLAGE",
                "Minds share one field, each running the real Daimon cognitive cycle — and the trained policy that reaches the end goal. Click any one to open its mind.",
            )
        };
        s.text(title, px + 18.0, py + 18.0, 12.0, Color::hex(CORAL, 1.0));
        s.text_wrapped(intro, px + 18.0, py + 38.0, 12.5, pw - 36.0, Color::hex(PAPER, 0.85));
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
        // FACTIONS first (society mode), so the countries' stats sit at the TOP of the
        // panel rather than below the long inhabitant roster; the per-mind list follows.
        // FACTIONS (society + civ): one entry per living village — its banner, name and
        // era with a research-progress bar, then a stats sub-line (population · buildings
        // · standing with its neighbours, and ⚔ if it is at war) — so each settlement
        // reads as a country with a legible size, tech level and diplomatic posture, and
        // you can SEE which has advanced or is fighting. Off a society world, skipped.
        if world.society && !world.villages.is_empty() {
            use crate::sim::{Era, RelationKind, ERA_THRESHOLDS};
            yy += 10.0;
            s.text("FACTIONS", px + 18.0, yy, 11.0, Color::hex(CORAL, 1.0));
            yy += 18.0;
            for v in &world.villages {
                if v.population == 0 {
                    continue;
                }
                let at_war = world.war && world.wars.iter().any(|w| w.a == v.id || w.b == v.id);
                s.orb(px + 24.0, yy + 6.0, 5.0, 0.8, Color::hex(v.hue, 1.0));
                let title = if world.eras {
                    format!("{}  ·  {}", v.name, v.era.name())
                } else {
                    v.name.clone()
                };
                s.text(title, px + 36.0, yy, 11.5, Color::hex(PAPER, 0.92));
                if world.eras {
                    let cur = v.era as usize;
                    let frac = if matches!(v.era, Era::Space) {
                        1.0
                    } else {
                        let (lo, hi) = (ERA_THRESHOLDS[cur], ERA_THRESHOLDS[cur + 1]);
                        ((v.research - lo) / (hi - lo)).clamp(0.0, 1.0)
                    };
                    bar(s, px + pw - 92.0, yy + 5.0, 74.0, frac, Color::hex(0xffc24e, 1.0));
                }
                yy += 17.0;
                // diplomatic posture: count this village's standing with living neighbours.
                let mut allies = 0u32;
                let mut foes = 0u32;
                for r in &world.relations {
                    let other = if r.a == v.id {
                        Some(r.b)
                    } else if r.b == v.id {
                        Some(r.a)
                    } else {
                        None
                    };
                    if let Some(o) = other {
                        if world.villages[o as usize].population == 0 {
                            continue;
                        }
                        match r.kind() {
                            RelationKind::Allied | RelationKind::Friendly => allies += 1,
                            RelationKind::Enemy | RelationKind::Rival => foes += 1,
                            RelationKind::Neutral => {}
                        }
                    }
                }
                let mut stat = format!("pop {} · {} bld", v.population, v.buildings);
                if allies > 0 {
                    stat.push_str(&format!(" · {allies} ally"));
                }
                if foes > 0 {
                    stat.push_str(&format!(" · {foes} rival"));
                }
                let (extra, scol) = if at_war {
                    (" · ⚔ at war".to_string(), 0xff4534u32)
                } else {
                    (String::new(), MUTED)
                };
                s.text(format!("{stat}{extra}"), px + 36.0, yy, 10.5, Color::hex(scol, 0.9));
                yy += 16.0;
                // CIVILIZATION CAPSTONE (Sprint 3): the village's LEADER, its formalized
                // TREATIES (named alliances), and its WONDER. Off a civ world these are all
                // empty, so the panel is exactly the incumbent (society) layout.
                if world.civ {
                    if !v.leader_name.is_empty() {
                        s.text(
                            format!("Leader: {}", v.leader_name),
                            px + 36.0,
                            yy,
                            10.5,
                            Color::hex(0xd9c79a, 0.92),
                        );
                        yy += 15.0;
                    }
                    // named treaties this village has signed.
                    for t in &world.treaties {
                        let other = if t.a == v.id {
                            Some(t.b)
                        } else if t.b == v.id {
                            Some(t.a)
                        } else {
                            None
                        };
                        if let Some(o) = other {
                            let oname = &world.villages[o as usize].name;
                            s.text(
                                format!("⚜ {} (w/ {oname})", t.name),
                                px + 36.0,
                                yy,
                                10.0,
                                Color::hex(0x9ee6b4, 0.9),
                            );
                            yy += 14.0;
                        }
                    }
                    if let Some(w) = &v.wonder {
                        s.text(
                            format!("★ Wonder: {}", w.name),
                            px + 36.0,
                            yy,
                            10.0,
                            Color::hex(0xffd86a, 0.95),
                        );
                        yy += 14.0;
                    }
                }
                yy += 6.0;
            }
        }
        // AT WAR (Civilization Sprint 2): list every active war — the two settlements,
        // their warband sizes, and casualties so far — so a watcher can SEE which
        // villages are fighting. Off a war world (or in peacetime), this whole block
        // is skipped, so it only ever appears when a war is genuinely on.
        if world.war && !world.wars.is_empty() {
            yy += 10.0;
            s.text("AT WAR", px + 18.0, yy, 11.0, Color::hex(0xff4534, 1.0));
            yy += 18.0;
            for w in &world.wars {
                let (na, nb) = (&world.villages[w.a as usize].name, &world.villages[w.b as usize].name);
                let (ba, bb) = (world.warband_size(w.a), world.warband_size(w.b));
                s.orb(px + 24.0, yy + 6.0, 5.0, 0.9, Color::hex(0xff4534, 1.0));
                s.text(
                    format!("{na} ⚔ {nb}"),
                    px + 36.0,
                    yy,
                    11.5,
                    Color::hex(0xffb0a4, 0.95),
                );
                yy += 16.0;
                s.text(
                    format!("warband {ba} vs {bb}  ·  fallen {}", w.dead_a + w.dead_b),
                    px + 36.0,
                    yy,
                    10.5,
                    Color::hex(MUTED, 0.85),
                );
                yy += 20.0;
            }
        }
        // INHABITANTS: the full roster of minds (living + lost), each clickable to open.
        yy += 10.0;
        if world.society {
            s.text("INHABITANTS", px + 18.0, yy, 11.0, Color::hex(CORAL, 1.0));
            yy += 18.0;
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

    // LIFE — the life-cycle facts (only meaningful on a lifecycle world): how happy,
    // how old, who its partner is, and how many children. A child shows its growth.
    if world.lifecycle {
        let hap = world.happiness_of(a);
        let hap_word = if hap > 0.66 {
            "content"
        } else if hap > 0.4 {
            "getting by"
        } else if hap > 0.18 {
            "weary"
        } else {
            "suffering"
        };
        s.text("LIFE", lx, y, 11.0, Color::hex(CORAL, 1.0));
        y += 15.0;
        // happiness bar — a warm gold the fuller the happier.
        s.text("happiness", lx, y - 1.0, 10.5, Color::hex(MUTED, 1.0));
        bar(s, lx + 70.0, y + 1.0, cw - 130.0, hap, Color::hex(0xffc24e, 1.0));
        s.text(hap_word, lx + cw - 56.0, y - 1.0, 10.5, Color::hex(0xffc24e, 0.95));
        y += 16.0;
        // age + life-stage. Age shown in "years" (1 year = 600 ticks) for readability.
        let age_ticks = world.age_of(a);
        let years = age_ticks as f32 / 600.0;
        let stage = if a.maturity < 0.92 {
            format!("child · {}% grown", (a.maturity * 100.0) as u32)
        } else {
            let frac = age_ticks as f32 / a.lifespan.max(1) as f32;
            if frac > 0.8 {
                "elder".to_string()
            } else {
                "adult".to_string()
            }
        };
        s.text(format!("age {years:.1} · {stage}"), lx, y, 11.0, Color::hex(PAPER, 0.9));
        y += 16.0;
        // partner.
        let partner_name = a
            .partner
            .and_then(|pid| world.agents.iter().find(|b| b.id == pid && b.alive))
            .map(|p| p.name.clone());
        match partner_name {
            Some(name) => {
                s.orb(lx + 5.0, y + 5.0, 4.0, 0.8, Color::hex(0xffc98a, 1.0));
                s.text(format!("partnered with {name}"), lx + 14.0, y, 11.0, Color::hex(0xffc98a, 0.95));
            }
            None => {
                s.text("unpartnered", lx, y, 11.0, Color::hex(MUTED, 0.85));
            }
        }
        y += 16.0;
        // children (living count of the recorded children), and parents if a child.
        let kids = a.children.iter().filter(|&&c| world.agents.iter().any(|b| b.id == c && b.alive)).count();
        if kids > 0 {
            s.text(format!("{kids} child{}", if kids == 1 { "" } else { "ren" }), lx, y, 11.0, Color::hex(PAPER, 0.9));
            y += 16.0;
        }
        if !a.parents.is_empty() {
            let pn: Vec<String> = a
                .parents
                .iter()
                .filter_map(|pid| world.agents.iter().find(|b| b.id == *pid).map(|b| b.name.clone()))
                .collect();
            if !pn.is_empty() {
                s.text(format!("born to {}", pn.join(" & ")), lx, y, 10.5, Color::hex(MUTED, 0.85));
                y += 16.0;
            }
        }
        y += 8.0;
    }

    // VILLAGE — the society facts (only meaningful on a society world): which
    // settlement this mind belongs to, and that village's standing alliances and
    // enmities. Empty (no draw) off a society world, so the panel is the incumbent.
    if world.society {
        if let Some(v) = world.village_of(i) {
            use crate::sim::RelationKind;
            s.text("VILLAGE", lx, y, 11.0, Color::hex(CORAL, 1.0));
            y += 15.0;
            s.orb(lx + 5.0, y + 5.0, 5.0, 0.9, Color::hex(v.hue, 1.0));
            s.text(format!("of {} · pop {}", v.name, v.population), lx + 16.0, y, 11.5, Color::hex(v.hue, 0.95));
            y += 17.0;
            // TECH ERA + RESEARCH (Civilization Sprint 1): which age this settlement has
            // climbed to, and how far toward the next. A no-op label off an eras world.
            if world.eras {
                use crate::sim::{Era, ERA_THRESHOLDS};
                // progress within the current era band toward the next threshold.
                let cur = v.era as usize;
                let lo = ERA_THRESHOLDS[cur];
                let frac = match v.era.next() {
                    Some(_) => {
                        let hi = ERA_THRESHOLDS[cur + 1];
                        ((v.research - lo) / (hi - lo)).clamp(0.0, 1.0)
                    }
                    None => 1.0, // Space — topped out
                };
                s.text(format!("TECH — {}", v.era.name()), lx, y, 11.0, Color::hex(0xffc24e, 1.0));
                let label = if matches!(v.era, Era::Space) {
                    "research maxed".to_string()
                } else {
                    format!("→ {}", v.era.next().map(|e| e.name()).unwrap_or(""))
                };
                s.text(label, lx + cw - 96.0, y, 10.0, Color::hex(MUTED, 0.9));
                y += 15.0;
                bar(s, lx, y + 1.0, cw - 64.0, frac, Color::hex(0xffc24e, 1.0));
                s.text(format!("{}%", (frac * 100.0) as u32), lx + cw - 58.0, y - 1.0, 10.5, Color::hex(MUTED, 1.0));
                y += 18.0;
            }
            // gather this village's allies and enemies from the relation matrix.
            let mut allies: Vec<&str> = Vec::new();
            let mut enemies: Vec<&str> = Vec::new();
            for other in &world.villages {
                if other.id == v.id || other.population == 0 {
                    continue;
                }
                if let Some(r) = world.relation_between(v.id, other.id) {
                    match r.kind() {
                        RelationKind::Allied | RelationKind::Friendly => allies.push(&other.name),
                        RelationKind::Enemy | RelationKind::Rival => enemies.push(&other.name),
                        RelationKind::Neutral => {}
                    }
                }
            }
            if !allies.is_empty() {
                s.text(format!("allied with {}", allies.join(", ")), lx, y, 10.5, Color::hex(0x8fe6a8, 0.95));
                y += 15.0;
            }
            if !enemies.is_empty() {
                s.text(format!("at odds with {}", enemies.join(", ")), lx, y, 10.5, Color::hex(0xff7a6a, 0.95));
                y += 15.0;
            }
            if allies.is_empty() && enemies.is_empty() {
                s.text("keeps to itself", lx, y, 10.5, Color::hex(MUTED, 0.85));
                y += 15.0;
            }
            y += 8.0;
        }
    }

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
