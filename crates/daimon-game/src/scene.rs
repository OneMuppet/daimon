//! The frame's drawables: the 3-D **world** (lit geometry + additive glows, with
//! the sky/camera parameters that light it) plus the screen-space **HUD** (the
//! mind inspector + top bar, drawn as SDF rounded-rects and glyphon text).
//!
//! [`crate::view`] fills a `Scene` each frame; [`crate::gfx`] turns it into draw
//! calls (world → low-res RT → nearest blit → HUD on top).

use crate::geo::{AddVertex, LitVertex};

/// A linear-space RGBA colour. Author with [`Color::srgb`] / [`Color::hex`].
#[derive(Clone, Copy, Debug)]
pub struct Color(pub [f32; 4]);

impl Color {
    pub fn srgb(r: u8, g: u8, b: u8, a: f32) -> Self {
        Color([s2l(r), s2l(g), s2l(b), a])
    }
    pub fn hex(rgb: u32, a: f32) -> Self {
        Color::srgb(((rgb >> 16) & 0xff) as u8, ((rgb >> 8) & 0xff) as u8, (rgb & 0xff) as u8, a)
    }
    pub fn with_a(self, a: f32) -> Self {
        Color([self.0[0], self.0[1], self.0[2], a])
    }
    pub fn lerp(self, other: Color, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        Color(std::array::from_fn(|i| self.0[i] + (other.0[i] - self.0[i]) * t))
    }
    /// linear rgb triple (for feeding vertex colours).
    pub fn rgb3(self) -> [f32; 3] {
        [self.0[0], self.0[1], self.0[2]]
    }
}

fn s2l(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// One instanced HUD quad (rounded rect or soft orb), screen-pixel space.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Quad {
    /// x, y, w, h in screen pixels (top-left origin).
    pub rect: [f32; 4],
    pub color: [f32; 4],
    /// corner_radius_px, edge_softness_px, shape (0 = rect, 1 = orb), glow.
    pub params: [f32; 4],
}

/// A run of HUD text (glyphon).
pub struct Text {
    pub content: String,
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub color: [u8; 4],
    pub wrap: Option<f32>,
}

/// Everything the world shaders need to light one frame — the camera transform
/// and the time-of-day / season / weather palette (all computed on the CPU).
#[derive(Clone, Copy, Debug)]
pub struct Sky {
    pub view_proj: [f32; 16],
    pub cam_pos: [f32; 3],
    /// unit direction TOWARD the sun, and 0..1 daylight.
    pub sun_dir: [f32; 3],
    pub daylight: f32,
    pub sun_color: [f32; 3],
    pub sun_strength: f32,
    pub ambient: [f32; 3],
    /// fog/horizon colour (also the RT clear) and the fog far distance.
    pub horizon: [f32; 3],
    pub fog_far: f32,
    /// season cast rgb + mix strength (winter lays snow).
    pub season_tint: [f32; 4],
    pub weather: f32,
    pub weather_kind: f32,
    /// world XZ of the view centre — fog is measured horizontally from here (an
    /// orthographic eye is equidistant from everything, so eye-distance fog can't
    /// work).
    pub fog_target: [f32; 2],
}

impl Default for Sky {
    fn default() -> Self {
        Sky {
            view_proj: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            cam_pos: [0.0, 10.0, 0.0],
            sun_dir: [0.4, 0.7, 0.5],
            daylight: 1.0,
            sun_color: [1.0, 0.95, 0.85],
            sun_strength: 1.0,
            ambient: [0.35, 0.4, 0.5],
            horizon: [0.6, 0.72, 0.85],
            fog_far: 120.0,
            season_tint: [0.5, 0.6, 0.4, 0.1],
            weather: 0.0,
            weather_kind: 0.0,
            fog_target: [20.0, 13.0],
        }
    }
}

#[derive(Default)]
pub struct Scene {
    /// Opaque/lit 3-D geometry (terrain is added by the renderer; this is actors).
    pub lit: Vec<LitVertex>,
    /// Additive glows (mood auras, hearth, weather motes, markers).
    pub add: Vec<AddVertex>,
    /// Screen-space HUD quads.
    pub quads: Vec<Quad>,
    /// Screen-space HUD text.
    pub texts: Vec<Text>,
    pub sky: Sky,
    /// Sim plane size (cells) — the renderer builds/caches the island from this.
    pub world_dims: (i32, i32),
    /// WALKABLE INTERIORS: true when the player is inside a house — the renderer
    /// then skips the sea (you're indoors). Terrain is already suppressed via
    /// `world_dims = (0,0)`. Default false (out on the island, exactly as before).
    pub interior: bool,
    /// ROAD corridors `[ax, az, bx, bz]` (village-centre to village-centre). The
    /// terrain bake CLEARS decorative flora along these so a road runs clean instead
    /// of having trees/boulders poke up through it. Empty off a road-built world.
    pub roads: Vec<[f32; 4]>,
}

impl Scene {
    pub fn new() -> Self {
        Scene {
            lit: Vec::with_capacity(8192),
            add: Vec::with_capacity(1024),
            quads: Vec::with_capacity(1024),
            texts: Vec::with_capacity(256),
            sky: Sky::default(),
            world_dims: (40, 26),
            interior: false,
            roads: Vec::new(),
        }
    }

    /// A filled, rounded HUD rectangle.
    pub fn rrect(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, c: Color) {
        self.quads.push(Quad { rect: [x, y, w, h], color: c.0, params: [radius, 1.0, 0.0, 0.0] });
    }

    /// A soft, optionally glowing HUD orb centred at (cx, cy).
    pub fn orb(&mut self, cx: f32, cy: f32, radius: f32, glow: f32, c: Color) {
        let pad = radius * (1.0 + glow * 2.6);
        self.quads.push(Quad {
            rect: [cx - pad, cy - pad, pad * 2.0, pad * 2.0],
            color: c.0,
            params: [radius, 1.5, 1.0, glow],
        });
    }

    pub fn text(&mut self, content: impl Into<String>, x: f32, y: f32, size: f32, c: Color) {
        self.texts.push(Text { content: content.into(), x, y, size, color: to_u8(c), wrap: None });
    }

    pub fn text_wrapped(
        &mut self,
        content: impl Into<String>,
        x: f32,
        y: f32,
        size: f32,
        wrap: f32,
        c: Color,
    ) {
        self.texts.push(Text {
            content: content.into(),
            x,
            y,
            size,
            color: to_u8(c),
            wrap: Some(wrap),
        });
    }
}

fn to_u8(c: Color) -> [u8; 4] {
    let l2s = |v: f32| {
        let v = v.clamp(0.0, 1.0);
        let s = if v <= 0.0031308 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
        (s * 255.0).round() as u8
    };
    [l2s(c.0[0]), l2s(c.0[1]), l2s(c.0[2]), (c.0[3] * 255.0) as u8]
}
