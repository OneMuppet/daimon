//! Procedural geometry — every triangle of Daimon's world comes from these pure
//! builders. The look is "luminous low-poly isometric": flat-shaded boxes, cones
//! and a displaced island, lit per-fragment off screen-space-derivative normals
//! (vertices carry no normals — the facets light themselves), then the whole
//! frame is rendered low-res and upscaled NEAREST for chunky, painterly pixels.
//!
//! Daimon's flavour vs. a civ-builder: the minds *glow*. Agents are little
//! luminous figures with a mood aura; resources are soft light-sources; the
//! village heart is warm. Albedo is baked into vertex colour; the day/night,
//! season and weather grade live in the shader.

use bytemuck::{Pod, Zeroable};

/// Opaque lit vertex: world position + albedo (alpha < 1 ⇒ translucent overlay).
/// Normals derive per-fragment from `dpdx/dpdy`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct LitVertex {
    pub pos: [f32; 3],
    pub color: [f32; 4],
}
impl LitVertex {
    #[inline]
    pub const fn new(pos: [f32; 3], color: [f32; 4]) -> Self {
        Self { pos, color }
    }
}

/// Additive glow vertex with a quad-local UV in [-1,1]² (soft round falloff).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct AddVertex {
    pub pos: [f32; 3],
    pub color: [f32; 4],
    pub uv: [f32; 2],
}

// --- deterministic noise ---------------------------------------------------

#[inline]
fn splitmix(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
#[inline]
pub fn hash_unit(h: u64, k: u32) -> f32 {
    ((splitmix(h ^ (k as u64).wrapping_mul(0xA24B_AED4_963E_E407)) >> 40) as u32 as f32)
        / ((1u32 << 24) as f32)
}
#[inline]
fn hash2(x: f32, z: f32) -> f32 {
    let h = ((x * 374761393.0) as i64 as u64) ^ (((z * 668265263.0) as i64 as u64) << 1);
    hash_unit(h, 7)
}
fn vnoise(x: f32, z: f32) -> f32 {
    let (ix, iz) = (x.floor(), z.floor());
    let (fx, fz) = (x - ix, z - iz);
    let (sx, sz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
    let a = hash2(ix, iz);
    let b = hash2(ix + 1.0, iz);
    let c = hash2(ix, iz + 1.0);
    let d = hash2(ix + 1.0, iz + 1.0);
    let ab = a + (b - a) * sx;
    let cd = c + (d - c) * sx;
    ab + (cd - ab) * sz
}
fn fbm(mut x: f32, mut z: f32) -> f32 {
    let mut amp = 0.5;
    let mut sum = 0.0;
    for _ in 0..5 {
        sum += amp * vnoise(x, z);
        x = x * 1.97 + 4.1;
        z = z * 1.97 - 2.7;
        amp *= 0.5;
    }
    sum
}

#[inline]
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
#[inline]
fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Sea level in world units. Land is `height > SEA`.
pub const SEA_Y: f32 = 0.0;

/// The island heightfield over the sim plane (cells). A gentle hilly interior
/// that falls away to water at the rim — a contained diorama, not an infinite
/// plane. Deterministic; used by BOTH the mesh and actor placement so figures
/// stand exactly on the ground.
pub fn terrain_height(wx: f32, wz: f32, cx: f32, cz: f32, radius: f32) -> f32 {
    // organic coastline: the island's effective radius wobbles with the land.
    let coast = fbm(wx * 0.07 + 5.0, wz * 0.07 - 3.0);
    let r_eff = radius * (0.80 + 0.42 * coast);
    let d = ((wx - cx).powi(2) + (wz - cz).powi(2)).sqrt() / r_eff;
    let mask = 1.0 - smoothstep(0.70, 1.0, d);
    let hills = (fbm(wx * 0.15, wz * 0.15) - 0.40) * 4.2;
    let ridge = (1.0 - (fbm(wx * 0.085 + 11.0, wz * 0.085) * 2.0 - 1.0).abs()) * 1.8;
    let land = (hills + ridge * 0.6 + 0.7).max(0.0);
    land * mask - (1.0 - mask) * 2.6
}

/// Ground height an actor stands on at sim cell `(x, z)` — the island surface,
/// never below the waterline (so figures don't sink). Uses the SAME parameters
/// as [`build_terrain`] so actors sit exactly on the mesh.
pub fn ground_height(w: i32, h: i32, x: f32, z: f32) -> f32 {
    let (cx, cz) = (w as f32 * 0.5, h as f32 * 0.5);
    let radius = (w.max(h) as f32) * 0.62;
    terrain_height(x, z, cx, cz, radius).max(SEA_Y + 0.02)
}

// --- primitive pushers -----------------------------------------------------

#[inline]
pub fn push_tri(out: &mut Vec<LitVertex>, a: [f32; 3], b: [f32; 3], c: [f32; 3], color: [f32; 4]) {
    out.push(LitVertex::new(a, color));
    out.push(LitVertex::new(b, color));
    out.push(LitVertex::new(c, color));
}
#[inline]
pub fn push_quad(
    out: &mut Vec<LitVertex>,
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
    d: [f32; 3],
    color: [f32; 4],
) {
    push_tri(out, a, b, c, color);
    push_tri(out, a, c, d, color);
}

#[inline]
fn rot_y(p: [f32; 3], yaw: f32) -> [f32; 3] {
    let (s, c) = yaw.sin_cos();
    [p[0] * c - p[2] * s, p[1], p[0] * s + p[2] * c]
}

/// A yaw-rotated box (12 triangles), centred at `center`, half-extents `half`.
pub fn push_box(out: &mut Vec<LitVertex>, center: [f32; 3], half: [f32; 3], yaw: f32, color: [f32; 4]) {
    let v = |sx: f32, sy: f32, sz: f32| -> [f32; 3] {
        let p = rot_y([half[0] * sx, half[1] * sy, half[2] * sz], yaw);
        [center[0] + p[0], center[1] + p[1], center[2] + p[2]]
    };
    let (a, b, c, d) = (v(-1.0, -1.0, -1.0), v(1.0, -1.0, -1.0), v(1.0, -1.0, 1.0), v(-1.0, -1.0, 1.0));
    let (e, f, g, h) = (v(-1.0, 1.0, -1.0), v(1.0, 1.0, -1.0), v(1.0, 1.0, 1.0), v(-1.0, 1.0, 1.0));
    push_quad(out, e, f, g, h, color); // top
    push_quad(out, a, b, c, d, color); // bottom
    push_quad(out, a, b, f, e, color);
    push_quad(out, c, d, h, g, color);
    push_quad(out, b, c, g, f, color);
    push_quad(out, d, a, e, h, color);
}

/// An upright low-poly cone (trees, crystals, spires).
#[allow(clippy::too_many_arguments)]
pub fn push_cone(
    out: &mut Vec<LitVertex>,
    cx: f32,
    y0: f32,
    cz: f32,
    r: f32,
    h: f32,
    segs: usize,
    color: [f32; 4],
) {
    let apex = [cx, y0 + h, cz];
    for i in 0..segs {
        let a0 = (i as f32 / segs as f32) * std::f32::consts::TAU;
        let a1 = ((i + 1) as f32 / segs as f32) * std::f32::consts::TAU;
        let p0 = [cx + r * a0.cos(), y0, cz + r * a0.sin()];
        let p1 = [cx + r * a1.cos(), y0, cz + r * a1.sin()];
        push_tri(out, p0, p1, apex, color);
        push_tri(out, p0, [cx, y0, cz], p1, color);
    }
}

/// A camera-facing additive quad with UVs in [-1,1]² (soft glow / particle).
pub fn push_billboard(
    out: &mut Vec<AddVertex>,
    center: [f32; 3],
    half_w: f32,
    half_h: f32,
    right: [f32; 3],
    up: [f32; 3],
    color: [f32; 4],
) {
    let v = |sx: f32, sy: f32| AddVertex {
        pos: [
            center[0] + right[0] * half_w * sx + up[0] * half_h * sy,
            center[1] + right[1] * half_w * sx + up[1] * half_h * sy,
            center[2] + right[2] * half_w * sx + up[2] * half_h * sy,
        ],
        color,
        uv: [sx, sy],
    };
    let (a, b, c, d) = (v(-1.0, -1.0), v(1.0, -1.0), v(1.0, 1.0), v(-1.0, 1.0));
    out.push(a);
    out.push(b);
    out.push(c);
    out.push(a);
    out.push(c);
    out.push(d);
}

// --- the island ------------------------------------------------------------

/// One big quad at sea level — the water plane (the shader makes it move).
pub fn build_water(extent: f32) -> Vec<LitVertex> {
    let mut out = Vec::with_capacity(6);
    let c = [0.0, 0.0, 0.0, 1.0];
    push_quad(
        &mut out,
        [-extent, SEA_Y, -extent],
        [extent, SEA_Y, -extent],
        [extent, SEA_Y, extent],
        [-extent, SEA_Y, extent],
        c,
    );
    out
}

/// The island mesh over the sim plane — an `n`-resolution displaced grid,
/// vertex-coloured by elevation + slope biome. Season/day grading is the
/// shader's job (so one mesh serves every season).
pub fn build_terrain(w: i32, h: i32, n: usize) -> Vec<LitVertex> {
    let (cx, cz) = (w as f32 * 0.5, h as f32 * 0.5);
    let radius = (w.max(h) as f32) * 0.62;
    // a generous margin so the shoreline + sea ring sit inside the frame.
    let (x0, x1) = (-6.0, w as f32 + 6.0);
    let (z0, z1) = (-6.0, h as f32 + 6.0);
    let nx = n;
    let nz = ((n as f32) * (z1 - z0) / (x1 - x0)).round() as usize;
    let mut out = Vec::with_capacity(nx * nz * 6);
    let hf = |x: f32, z: f32| terrain_height(x, z, cx, cz, radius);
    let color_at = |x: f32, z: f32, ht: f32, slope: f32| -> [f32; 4] {
        let j = 0.92 + hash2(x * 3.1, z * 3.7) * 0.16;
        let sand = [0.78 * j, 0.70 * j, 0.46 * j];
        let grass_lo = [0.20 * j, 0.44 * j, 0.17 * j];
        let grass_hi = [0.34 * j, 0.52 * j, 0.19 * j];
        let rock = [0.40 * j, 0.39 * j, 0.42 * j];
        // base meadow varies with gentle elevation
        let meadow = lerp3(grass_lo, grass_hi, smoothstep(0.4, 2.4, ht));
        // beach near the waterline, rock on steep faces + high ground
        let mut c = lerp3(sand, meadow, smoothstep(0.05, 0.5, ht));
        c = lerp3(c, rock, smoothstep(0.45, 0.85, slope));
        c = lerp3(c, rock, smoothstep(2.6, 4.0, ht));
        [c[0], c[1], c[2], 1.0]
    };
    let step_x = (x1 - x0) / nx as f32;
    let step_z = (z1 - z0) / nz as f32;
    for iz in 0..nz {
        for ix in 0..nx {
            let ax = x0 + ix as f32 * step_x;
            let az = z0 + iz as f32 * step_z;
            let (bx, bz) = (ax + step_x, az + step_z);
            let (h00, h10, h11, h01) = (hf(ax, az), hf(bx, az), hf(bx, bz), hf(ax, bz));
            // Skip cells fully under the sea — the water plane owns them.
            if h00 < -0.6 && h10 < -0.6 && h11 < -0.6 && h01 < -0.6 {
                continue;
            }
            let hc = (h00 + h10 + h11 + h01) * 0.25;
            let slope = (h10 - h01).abs().max((h00 - h11).abs()) / step_x;
            let c = color_at((ax + bx) * 0.5, (az + bz) * 0.5, hc, slope);
            push_quad(
                &mut out,
                [ax, h00, az],
                [bx, h10, az],
                [bx, h11, bz],
                [ax, h01, bz],
                c,
            );
        }
    }
    scatter_flora(&mut out, w, h);
    out
}

/// Decorative flora strewn across the island — pine-ish trees, boulders and
/// grass tufts — so the land reads as a lush diorama, not a bare mesh.
/// Deterministic; baked into the static terrain buffer.
fn scatter_flora(out: &mut Vec<LitVertex>, w: i32, h: i32) {
    let (cx, cz) = (w as f32 * 0.5, h as f32 * 0.5);
    let radius = (w.max(h) as f32) * 0.62;
    let step = 1.4f32;
    let mut z = -4.0;
    while z < h as f32 + 4.0 {
        let mut x = -4.0;
        while x < w as f32 + 4.0 {
            let hh = splitmix(((x * 71.0) as i64 as u64) << 20 ^ ((z * 53.0) as i64 as u64));
            let jx = x + (hash_unit(hh, 1) - 0.5) * step;
            let jz = z + (hash_unit(hh, 2) - 0.5) * step;
            let g = terrain_height(jx, jz, cx, cz, radius);
            if g <= 0.35 {
                x += step;
                continue;
            }
            let roll = hash_unit(hh, 3);
            if roll < 0.22 {
                // a conifer: trunk + two foliage tiers
                let th = 0.5 + hash_unit(hh, 4) * 0.6;
                let trunk = [0.32, 0.22, 0.14, 1.0];
                let lg = 0.20 + hash_unit(hh, 6) * 0.16;
                let leaf = [0.13, 0.30 + lg, 0.16, 1.0];
                push_box(out, [jx, g + th * 0.4, jz], [0.07, th * 0.4, 0.07], 0.0, trunk);
                push_cone(out, jx, g + th * 0.35, jz, 0.42, 0.85, 5, leaf);
                push_cone(out, jx, g + th * 0.35 + 0.55, jz, 0.30, 0.7, 5, leaf);
            } else if roll < 0.40 {
                // a mossy boulder
                let r = 0.18 + hash_unit(hh, 7) * 0.28;
                let grey = 0.40 + hash_unit(hh, 8) * 0.12;
                push_box(
                    out,
                    [jx, g + r * 0.6, jz],
                    [r, r * 0.7, r * 0.9],
                    hash_unit(hh, 9) * 3.0,
                    [grey, grey * 1.02, grey * 1.04, 1.0],
                );
            } else if roll < 0.62 {
                // a grass tuft
                let gh = 0.12 + hash_unit(hh, 10) * 0.18;
                push_cone(out, jx, g, jz, 0.12, gh, 4, [0.30, 0.46, 0.20, 1.0]);
            }
            x += step;
        }
        z += step;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_is_deterministic_nonempty_trilist() {
        let a = build_terrain(40, 26, 48);
        let b = build_terrain(40, 26, 48);
        assert!(!a.is_empty());
        assert_eq!(a.len() % 3, 0);
        assert!(a.iter().zip(&b).all(|(x, y)| x.pos == y.pos && x.color == y.color));
    }

    #[test]
    fn island_is_higher_inland_than_at_the_rim() {
        let (cx, cz, r) = (20.0, 13.0, 22.0);
        assert!(terrain_height(cx, cz, cx, cz, r) > terrain_height(cx + r * 1.1, cz, cx, cz, r));
        assert!(terrain_height(cx + r * 1.1, cz, cx, cz, r) < SEA_Y, "rim sinks under the sea");
    }

    #[test]
    fn box_and_cone_emit_trilists() {
        let mut v = Vec::new();
        push_box(&mut v, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0], 0.3, [1.0, 1.0, 1.0, 1.0]);
        push_cone(&mut v, 0.0, 0.0, 0.0, 1.0, 2.0, 5, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(v.len() % 3, 0);
    }
}
