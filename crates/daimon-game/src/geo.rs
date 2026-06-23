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

// --- character figures (villagers) ----------------------------------------
//
// A villager is a real little low-poly person — torso + head + two legs + two
// arms — built from the same flat-shaded boxes as the rest of the world, but
// **articulated**: limbs swing about their joints so the figure WALKS when it is
// moving and stands when it is still. Everything is authored in the villager's
// LOCAL frame `(fwd, side, up)` — `fwd` along the nose, `side` to its left, `up`
// height — then rotated by `heading` into world. Animation is pure: it is a
// function of `phase` (a per-villager logical clock) and `stride` (0 idle → 1
// striding), so the gait is deterministic and each villager steps on its own beat.

/// One articulated body part of a character: a box whose *joint* sits at local
/// `(jf, js, ju)` (fwd/side/up) and which swings `pitch` radians about the side
/// axis (a fore/aft leg/arm swing) and leans `roll` radians about the fwd axis.
/// `len_down` is how far the part extends below the joint (a limb hangs down),
/// `half` its half-extents. The local offset is then rotated by `heading` and
/// placed at world `(x, gy+? , z)`. Flat-shaded, so the box self-lights.
#[allow(clippy::too_many_arguments)]
fn push_limb(
    out: &mut Vec<LitVertex>,
    x: f32,
    gy: f32,
    z: f32,
    heading: f32,
    sc: f32,
    joint: [f32; 3],   // (fwd, side, up) of the joint, pre-scale
    pitch: f32,        // swing about side axis (fore/aft)
    len_down: f32,     // how far the limb reaches below its joint (pre-scale)
    half: [f32; 3],    // limb half-extents (pre-scale)
    color: [f32; 4],
) {
    // limb centre in the local fwd/up plane: drop `len_down*0.5` from the joint,
    // then rotate that drop vector by `pitch` about the side axis so the foot
    // swings forward/back while staying attached at the joint.
    let (sp, cp) = pitch.sin_cos();
    let r = len_down * 0.5;
    let lf = joint[0] + sp * r; // pitch rotates (0,-r) in (fwd,up) → (sp*r, -cp*r)
    let lu = joint[2] - cp * r;
    let ls = joint[1];
    // rotate the local (fwd,side) offset by heading into world (x,z). heading 0 ⇒
    // nose toward +x; side is 90° left of the nose.
    let (shh, chh) = heading.sin_cos();
    let (lf, ls, lu) = (lf * sc, ls * sc, lu * sc);
    let wx = x + lf * chh - ls * shh;
    let wz = z + lf * shh + ls * chh;
    // the box itself is yaw-rotated by heading so it tracks the body, and tilted by
    // `pitch` so a swung limb visibly angles (a thin box reads the lean).
    push_box_tilt(out, [wx, gy + lu, wz], [half[0] * sc, half[1] * sc, half[2] * sc], heading, pitch, color);
}

/// A box rotated by `yaw` about up THEN tilted by `pitch` about its local side
/// axis — lets an articulated limb both face the heading and angle as it swings.
fn push_box_tilt(out: &mut Vec<LitVertex>, center: [f32; 3], half: [f32; 3], yaw: f32, pitch: f32, color: [f32; 4]) {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    // local axes after pitch (about side=+z-local-of-the-figure... we treat fwd=x):
    // fwd' = (cp, -sp) in (x,y); up' = (sp, cp) in (x,y); side stays.
    let v = |sx: f32, syy: f32, sz: f32| -> [f32; 3] {
        // pitch tilt in the fwd(x)/up(y) plane
        let fx = half[0] * sx;
        let uy = half[1] * syy;
        let px = fx * cp + uy * sp;
        let py = -fx * sp + uy * cp;
        let pz = half[2] * sz;
        // yaw about up
        [center[0] + px * cy - pz * sy, center[1] + py, center[2] + px * sy + pz * cy]
    };
    let (a, b, c, d) = (v(-1.0, -1.0, -1.0), v(1.0, -1.0, -1.0), v(1.0, -1.0, 1.0), v(-1.0, -1.0, 1.0));
    let (e, f, g, h) = (v(-1.0, 1.0, -1.0), v(1.0, 1.0, -1.0), v(1.0, 1.0, 1.0), v(-1.0, 1.0, 1.0));
    push_quad(out, e, f, g, h, color);
    push_quad(out, a, b, c, d, color);
    push_quad(out, a, b, f, e, color);
    push_quad(out, c, d, h, g, color);
    push_quad(out, b, c, g, f, color);
    push_quad(out, d, a, e, h, color);
}

/// Push a fixed (non-swinging) part at local `(fwd, side, up)`, rotated by heading.
#[allow(clippy::too_many_arguments)]
fn push_fixed(
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
    let (fwd, side, up) = (fwd * sc, side * sc, up * sc);
    let wx = x + fwd * ch - side * sh;
    let wz = z + fwd * sh + side * ch;
    push_box(out, [wx, gy + up, wz], [half[0] * sc, half[1] * sc, half[2] * sc], heading, color);
}

/// Build a little VILLAGER character at world `(x, gy, z)` facing `heading`.
///
/// `sc` scales the whole figure (maturity: children small, adults 1.0). `phase`
/// is the villager's own logical walk clock (radians) and `stride` ∈ [0,1] is how
/// hard it is walking (0 ⇒ standing still, no leg swing, no bob). `body` is the
/// drive colour (the torso, so each mind keeps its colour); `skin` the head/hands.
///
/// Articulation: legs swing fore/aft in anti-phase (a believable stride), arms
/// counter-swing, the torso bobs up + leans a touch into the step, the head sways
/// gently. Idle, the figure stands with arms at rest. Pure function of the inputs.
#[allow(clippy::too_many_arguments)]
pub fn push_villager(
    out: &mut Vec<LitVertex>,
    x: f32,
    gy: f32,
    z: f32,
    heading: f32,
    sc: f32,
    phase: f32,
    stride: f32,
    body: [f32; 4],
    skin: [f32; 4],
) {
    let st = stride.clamp(0.0, 1.0);
    let (sw, _cw) = phase.sin_cos();
    // a vertical bob at twice the stride cadence (the body rises on each footfall).
    let bob = (phase * 2.0).cos() * 0.035 * st;
    // a subtle forward lean while walking, head sway side-to-side.
    let lean = 0.10 * st;
    let head_sway = sw * 0.05 * st;
    // proportions: children read CUTER with a bigger head. `juv` = how juvenile.
    let juv = (1.0 - sc).clamp(0.0, 1.0);
    let head_r = 0.135 * (1.0 + 0.45 * juv); // bigger head when young
    let leg_len = 0.30 * (1.0 - 0.25 * juv); // shorter legs when young
    let torso_h = 0.30 * (1.0 - 0.18 * juv);
    let hip = leg_len; // hip sits a leg-length above the ground
    let leg_dark = mul(body, 0.55);     // darker drive tone for legs (trousers)
    let arm_c = mul(body, 0.82);        // slightly lighter sleeves

    // legs — swing fore/aft in anti-phase. Joint at the hip.
    let leg_swing = sw * 0.55 * st;
    push_limb(out, x, gy, z, heading, sc, [0.0, 0.075, hip], leg_swing, leg_len, [0.052, leg_len * 0.5, 0.06], leg_dark);
    push_limb(out, x, gy, z, heading, sc, [0.0, -0.075, hip], -leg_swing, leg_len, [0.052, leg_len * 0.5, 0.06], leg_dark);
    // little feet at the toe of each leg (swing with the leg so they plant/lift).
    let (sl, cl) = leg_swing.sin_cos();
    let foot_f = sl * leg_len; // foot reaches forward by the swung leg length
    let foot_u = (1.0 - cl) * leg_len; // lifts as it swings
    push_fixed(out, x, gy, z, heading, sc, foot_f + 0.04, 0.075, (foot_u + 0.03).max(0.025), [0.07, 0.028, 0.09], mul(body, 0.4));
    push_fixed(out, x, gy, z, heading, sc, -foot_f + 0.04, -0.075, ((1.0 - (-leg_swing).cos()) * leg_len + 0.03).max(0.025), [0.07, 0.028, 0.09], mul(body, 0.4));

    // torso — a tapered drive-coloured trunk that bobs + leans into the walk.
    let torso_cy = hip + torso_h + bob;
    push_box_tilt(out, [x, gy + torso_cy * sc, z], [0.13 * sc, torso_h * sc, 0.095 * sc], heading, lean, body);
    // a slightly narrower shoulder band on top so the figure has a silhouette.
    push_box_tilt(out, [x + lean.sin() * 0.02, gy + (hip + torso_h * 2.0) * sc + bob * sc, z], [0.145 * sc, 0.05 * sc, 0.10 * sc], heading, lean, mul(body, 1.08));

    // arms — counter-swing to the legs, hung at the shoulders.
    let arm_swing = -sw * 0.45 * st;
    let sh_up = hip + torso_h * 2.0 + bob;
    push_limb(out, x, gy, z, heading, sc, [0.0, 0.165, sh_up], arm_swing, 0.27, [0.04, 0.135, 0.045], arm_c);
    push_limb(out, x, gy, z, heading, sc, [0.0, -0.165, sh_up], -arm_swing, 0.27, [0.04, 0.135, 0.045], arm_c);

    // head — a rounded pale box riding the shoulders, swaying gently with the gait.
    let head_cy = hip + torso_h * 2.0 + head_r + 0.04 + bob;
    push_fixed(out, x, gy, z, heading, sc, lean.sin() * 0.06, head_sway, head_cy, [head_r, head_r * 1.05, head_r], skin);
    // a small nose nub so the head reads as facing forward (helps in first-person).
    push_fixed(out, x, gy, z, heading, sc, head_r * 0.9, head_sway, head_cy - head_r * 0.1, [head_r * 0.28, head_r * 0.28, head_r * 0.28], skin);
}

#[inline]
fn mul(c: [f32; 4], k: f32) -> [f32; 4] {
    [c[0] * k, c[1] * k, c[2] * k, c[3]]
}

/// Draw an ERA-APPROPRIATE WEAPON in a warrior's right hand (Civilization Sprint 2).
/// `weapon` is a small code matching `sim::Weapon` (0 club, 1 sword, 2 musket, 3 energy)
/// so geo stays free of any sim dependency. The piece is built in the figure's LOCAL
/// frame (fwd/side/up) at the right-hand grip, swinging slightly with the gait, and
/// tinted per era. Pure function of the inputs; called by the renderer only for minds the
/// world reports as mustered warriors, so non-war views draw nothing here.
#[allow(clippy::too_many_arguments)]
pub fn push_weapon(
    out: &mut Vec<LitVertex>,
    x: f32,
    gy: f32,
    z: f32,
    heading: f32,
    sc: f32,
    phase: f32,
    stride: f32,
    weapon: u8,
) {
    let st = stride.clamp(0.0, 1.0);
    let (sw, _cw) = phase.sin_cos();
    // the right hand: side = -0.165 (mirrors push_villager's right arm), carried a touch
    // forward + a hand-height down from the shoulder, swinging gently with the arm.
    let arm_swing = -sw * 0.45 * st;
    let hip = 0.30 * (1.0 - 0.25 * (1.0 - sc).clamp(0.0, 1.0));
    let torso_h = 0.30 * (1.0 - 0.18 * (1.0 - sc).clamp(0.0, 1.0));
    let sh_up = hip + torso_h * 2.0; // shoulder height (local, pre-scale)
    let hand_side = -0.165;
    let grip_fwd = 0.16 + arm_swing * 0.10; // out in front of the body
    let grip_up = sh_up - 0.16; // about hand height
    // metallic/wood tints by era.
    let wood = [0.40, 0.26, 0.15, 1.0];
    let bronze = [0.72, 0.52, 0.26, 1.0];
    let steel = [0.74, 0.78, 0.84, 1.0];
    let dark_iron = [0.22, 0.22, 0.26, 1.0];
    let energy = [0.45, 0.85, 1.0, 1.0];
    match weapon {
        // CLUB (Stone): a stubby heavy timber haft with a fat head.
        0 => {
            push_fixed(out, x, gy, z, heading, sc, grip_fwd, hand_side, grip_up, [0.022, 0.022, 0.13], wood);
            push_fixed(out, x, gy, z, heading, sc, grip_fwd, hand_side, grip_up + 0.14, [0.05, 0.05, 0.06], mul(wood, 0.8));
        }
        // SWORD (Bronze/Iron): a long blade + crossguard, and a round shield on the off-hand.
        1 => {
            push_fixed(out, x, gy, z, heading, sc, grip_fwd, hand_side, grip_up + 0.10, [0.016, 0.016, 0.20], steel); // blade
            push_fixed(out, x, gy, z, heading, sc, grip_fwd, hand_side, grip_up - 0.02, [0.07, 0.012, 0.02], bronze); // crossguard
            push_fixed(out, x, gy, z, heading, sc, grip_fwd, hand_side, grip_up - 0.07, [0.018, 0.018, 0.04], wood); // grip
            // round shield carried on the LEFT (off-hand) arm.
            push_fixed(out, x, gy, z, heading, sc, 0.10, 0.165, sh_up - 0.18, [0.02, 0.10, 0.10], bronze);
        }
        // MUSKET/RIFLE (Industrial): a long barrel + stock held across the body (ranged).
        2 => {
            push_fixed(out, x, gy, z, heading, sc, grip_fwd + 0.10, hand_side, grip_up + 0.04, [0.20, 0.018, 0.018], dark_iron); // barrel (points fwd)
            push_fixed(out, x, gy, z, heading, sc, grip_fwd - 0.06, hand_side, grip_up - 0.02, [0.06, 0.03, 0.02], wood); // stock
        }
        // ENERGY ARM (Space): a sleek emitter with a glowing core (ranged).
        _ => {
            push_fixed(out, x, gy, z, heading, sc, grip_fwd + 0.06, hand_side, grip_up + 0.02, [0.12, 0.03, 0.03], steel); // body
            push_fixed(out, x, gy, z, heading, sc, grip_fwd + 0.18, hand_side, grip_up + 0.02, [0.03, 0.035, 0.035], energy); // glowing muzzle
        }
    }
}

/// Draw a TRADE accessory on a villager so its PROFESSION reads at a glance: a tool
/// in hand + a hat. `trade` is a small code — 0 hunter (spear + pointed leather cap),
/// 1 farmer (hoe + wide straw hat), 2 builder (shouldered plank + flat cap),
/// 3 elder/chief (tall staff + gold circlet). Warriors carry weapons via
/// [`push_weapon`] instead, and children are left as bare little figures, so the
/// renderer calls this only for grown civilians. Built in the figure's LOCAL frame
/// (mirrors `push_weapon`'s right-hand grip + `push_villager`'s head height), so it
/// rides the gait. Pure function of the inputs; render-only — no sim dependency.
#[allow(clippy::too_many_arguments)]
pub fn push_trade(
    out: &mut Vec<LitVertex>,
    x: f32,
    gy: f32,
    z: f32,
    heading: f32,
    sc: f32,
    phase: f32,
    stride: f32,
    trade: u8,
) {
    let st = stride.clamp(0.0, 1.0);
    let (sw, _cw) = phase.sin_cos();
    // adult proportions (callers gate on maturity), mirroring push_villager/weapon.
    let hip = 0.30;
    let torso_h = 0.30;
    let head_r = 0.135;
    let head_cy = hip + torso_h * 2.0 + head_r + 0.04;
    let arm_swing = -sw * 0.45 * st;
    let hand_side = -0.165;
    let grip_fwd = 0.16 + arm_swing * 0.10;
    let grip_up = hip + torso_h * 2.0 - 0.16; // ≈ hand height
    // tints
    let wood = [0.40, 0.26, 0.15, 1.0];
    let straw = [0.84, 0.69, 0.34, 1.0];
    let steel = [0.74, 0.78, 0.84, 1.0];
    let leather = [0.34, 0.24, 0.16, 1.0];
    let gold = [0.86, 0.70, 0.30, 1.0];
    match trade {
        // HUNTER — a tall spear held upright + a pointed leather cap.
        0 => {
            push_fixed(out, x, gy, z, heading, sc, grip_fwd, hand_side, grip_up + 0.30, [0.016, 0.34, 0.016], wood); // shaft
            push_fixed(out, x, gy, z, heading, sc, grip_fwd, hand_side, grip_up + 0.66, [0.026, 0.05, 0.026], steel); // spearhead
            push_fixed(out, x, gy, z, heading, sc, 0.0, 0.0, head_cy + head_r * 0.55, [head_r * 0.95, head_r * 0.30, head_r * 0.95], leather); // cap band
            push_fixed(out, x, gy, z, heading, sc, 0.0, 0.0, head_cy + head_r * 1.05, [head_r * 0.42, head_r * 0.30, head_r * 0.42], leather); // point
        }
        // FARMER — a hoe + a wide-brim straw hat.
        1 => {
            push_fixed(out, x, gy, z, heading, sc, grip_fwd, hand_side, grip_up + 0.22, [0.015, 0.28, 0.015], wood); // shaft
            push_fixed(out, x, gy, z, heading, sc, grip_fwd + 0.05, hand_side, grip_up + 0.48, [0.05, 0.022, 0.03], steel); // hoe head
            push_fixed(out, x, gy, z, heading, sc, 0.0, 0.0, head_cy + head_r * 0.55, [head_r * 1.9, head_r * 0.10, head_r * 1.9], straw); // wide brim
            push_fixed(out, x, gy, z, heading, sc, 0.0, 0.0, head_cy + head_r * 0.9, [head_r * 0.7, head_r * 0.40, head_r * 0.7], straw); // crown
        }
        // BUILDER — a plank shouldered across the back + a flat cap.
        2 => {
            push_fixed(out, x, gy, z, heading, sc, 0.0, 0.02, hip + torso_h * 2.0 + 0.09, [0.05, 0.025, 0.34], wood); // plank across shoulders
            push_fixed(out, x, gy, z, heading, sc, 0.0, 0.0, head_cy + head_r * 0.7, [head_r * 1.15, head_r * 0.12, head_r * 1.15], leather); // flat cap
        }
        // ELDER / CHIEF — a tall staff + a gold circlet.
        _ => {
            push_fixed(out, x, gy, z, heading, sc, grip_fwd, hand_side, grip_up + 0.32, [0.018, 0.40, 0.018], wood); // staff
            push_fixed(out, x, gy, z, heading, sc, grip_fwd, hand_side, grip_up + 0.74, [0.04, 0.04, 0.04], gold); // staff knob
            push_fixed(out, x, gy, z, heading, sc, 0.0, 0.0, head_cy + head_r * 0.8, [head_r * 1.05, head_r * 0.12, head_r * 1.05], gold); // circlet
        }
    }
}

/// Build a little VEHICLE at world `(x, gy, z)` facing `heading`, scaled by `sc`.
/// `hover` picks the look: a wheeled motorcar (Industrial) or a sleek wheel-less
/// hover-pod (Space). `body` is the paint colour. Built in the LOCAL frame
/// (fwd/side/up) like the other figures so it sits along the road direction; the
/// headlight/taillight/underglow are additive billboards added by the caller.
/// Pure function of the inputs; render-only — no sim dependency.
#[allow(clippy::too_many_arguments)]
pub fn push_car(out: &mut Vec<LitVertex>, x: f32, gy: f32, z: f32, heading: f32, sc: f32, hover: bool, body: [f32; 4]) {
    let dark = mul(body, 0.58);
    let wheel = [0.12, 0.12, 0.13, 1.0];
    let glass = [0.46, 0.56, 0.68, 1.0];
    // chassis — a low body, long along the travel direction (fwd = half[0]).
    push_fixed(out, x, gy, z, heading, sc, 0.0, 0.0, 0.20, [0.46, 0.12, 0.24], body);
    // cabin — a smaller block set a touch back; glassy on a car, body-coloured on a pod.
    push_fixed(out, x, gy, z, heading, sc, -0.05, 0.0, 0.37, [0.26, 0.11, 0.20], if hover { mul(body, 1.1) } else { glass });
    if hover {
        // a hover-pod: no wheels, a thin emissive skirt that the underglow lights.
        push_fixed(out, x, gy, z, heading, sc, 0.0, 0.0, 0.07, [0.42, 0.04, 0.22], [0.40, 0.72, 1.0, 1.0]);
    } else {
        // four dark wheels at the corners.
        for (fw, sd) in [(0.30, 0.22), (0.30, -0.22), (-0.30, 0.22), (-0.30, -0.22)] {
            push_fixed(out, x, gy, z, heading, sc, fw, sd, 0.10, [0.10, 0.10, 0.07], wheel);
        }
        // a dark bumper line so the car reads as facing forward.
        push_fixed(out, x, gy, z, heading, sc, 0.42, 0.0, 0.16, [0.06, 0.06, 0.22], dark);
    }
}

/// The local-frame right-hand grip world position for a warrior (so the renderer can
/// anchor a muzzle flash / clash spark there). Mirrors `push_weapon`'s grip math.
pub fn weapon_muzzle(x: f32, gy: f32, z: f32, heading: f32, sc: f32, ranged: bool) -> [f32; 3] {
    let hip = 0.30 * (1.0 - 0.25 * (1.0 - sc).clamp(0.0, 1.0));
    let torso_h = 0.30 * (1.0 - 0.18 * (1.0 - sc).clamp(0.0, 1.0));
    let sh_up = hip + torso_h * 2.0;
    let fwd = if ranged { 0.16 + 0.30 } else { 0.16 + 0.16 };
    let side = -0.165;
    let up = sh_up - 0.12;
    let (sh, ch) = heading.sin_cos();
    let (fwd, side, up) = (fwd * sc, side * sc, up * sc);
    [x + fwd * ch - side * sh, gy + up, z + fwd * sh + side * ch]
}

/// The height (above the ground) of a villager's crown for scale `sc` — so the
/// renderer can hover a soul-spark just above the head and place labels. Mirrors
/// the proportions inside [`push_villager`].
#[inline]
pub fn villager_crown(sc: f32) -> f32 {
    let juv = (1.0 - sc).clamp(0.0, 1.0);
    let head_r = 0.135 * (1.0 + 0.45 * juv);
    let leg_len = 0.30 * (1.0 - 0.25 * juv);
    let torso_h = 0.30 * (1.0 - 0.18 * juv);
    (leg_len + torso_h * 2.0 + head_r * 2.0 + 0.04) * sc
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
// --- modular building pieces ----------------------------------------------
//
// A small kit of pre-made architectural meshes the village renderer composes
// into real-looking, multi-floor buildings out of a built `walls` footprint.
// LIVE-ONLY: nothing here touches the sim — these are pure geometry pushers in
// the same flat-shaded box/cone idiom as the rest of the world. Everything is
// authored in *world units* (1 unit = one sim cell) so a piece spans a cell.
//
// Material palette (warm golden-hour stone + timber): authored as linear-ish
// triples that read warm under the hero key and cool in shadow.
pub mod pieces {
    use super::{push_box, push_quad, push_tri, LitVertex};

    /// Warm sandstone wall (sunlit faces glow amber, shadowed faces go cool).
    pub const STONE: [f32; 4] = [0.62, 0.50, 0.36, 1.0];
    /// A slightly darker stone for plinths / lower courses.
    pub const STONE_DARK: [f32; 4] = [0.46, 0.37, 0.27, 1.0];
    /// Timber framing / posts / door — a warm oak brown.
    pub const TIMBER: [f32; 4] = [0.40, 0.26, 0.15, 1.0];
    /// A lighter beam highlight.
    pub const TIMBER_LT: [f32; 4] = [0.54, 0.37, 0.21, 1.0];
    /// Floor / interior planking.
    pub const FLOOR: [f32; 4] = [0.50, 0.36, 0.23, 1.0];
    /// Terracotta pitched roof.
    pub const ROOF: [f32; 4] = [0.55, 0.26, 0.18, 1.0];
    /// Roof ridge / eave highlight.
    pub const ROOF_LT: [f32; 4] = [0.66, 0.34, 0.22, 1.0];
    /// Dark window frame.
    pub const FRAME: [f32; 4] = [0.22, 0.15, 0.10, 1.0];
    /// Warm glowing window pane (additive glow is layered on top in the view).
    pub const GLASS: [f32; 4] = [1.0, 0.74, 0.34, 1.0];

    // --- ERA MATERIALS (Civilization Sprint 1). Each tech era gives a settlement its
    // own architecture, and the strongest read is MATERIAL + colour. An [`EraStyle`] is
    // the per-era palette the era-aware wall/roof pieces below paint with, so a village's
    // buildings visibly change as it climbs Stone → Bronze → Iron → Industrial → Space.

    /// The per-era material palette for a building. `wall`/`wall2` are the two-tone wall
    /// stone/timber/brick/metal; `roof`/`roof2` the roof material; `trim` the framing
    /// accent; `glass` the window pane (warm hearth-glow early, cool electric late).
    #[derive(Clone, Copy)]
    pub struct EraStyle {
        pub wall: [f32; 4],
        pub wall2: [f32; 4],
        pub roof: [f32; 4],
        pub roof2: [f32; 4],
        pub trim: [f32; 4],
        pub glass: [f32; 4],
    }

    /// STONE age: thatch + rough timber + daub — earthy, hand-built, warm.
    pub const ERA_STONE: EraStyle = EraStyle {
        wall: [0.50, 0.40, 0.27, 1.0],   // wattle-and-daub clay
        wall2: [0.40, 0.26, 0.15, 1.0],  // timber post (oak brown)
        roof: [0.62, 0.47, 0.24, 1.0],   // golden thatch
        roof2: [0.50, 0.36, 0.18, 1.0],  // thatch shadow
        trim: [0.34, 0.22, 0.12, 1.0],   // dark timber
        glass: [1.0, 0.70, 0.30, 1.0],   // a warm fire-lit opening
    };
    /// BRONZE age: tidy timber frame on a stone footing — lighter, squarer, worked.
    pub const ERA_BRONZE: EraStyle = EraStyle {
        wall: [0.66, 0.56, 0.40, 1.0],   // pale lime-washed daub
        wall2: [0.46, 0.32, 0.19, 1.0],  // framing timber
        roof: [0.50, 0.30, 0.16, 1.0],   // dark thatch/shingle
        roof2: [0.40, 0.24, 0.13, 1.0],
        trim: [0.52, 0.40, 0.22, 1.0],   // golden framing
        glass: [1.0, 0.74, 0.36, 1.0],
    };
    /// IRON age: dressed stone masonry + terracotta tile — the classic stone village.
    pub const ERA_IRON: EraStyle = EraStyle {
        wall: STONE,                     // warm sandstone
        wall2: STONE_DARK,
        roof: ROOF,                      // terracotta
        roof2: [0.46, 0.22, 0.15, 1.0],
        trim: TIMBER,
        glass: GLASS,
    };
    /// INDUSTRIAL age: red brick + slate, soot-stained — the machine age (chimneys +
    /// smokestacks are added on top by the renderer for this era).
    pub const ERA_INDUSTRIAL: EraStyle = EraStyle {
        wall: [0.55, 0.27, 0.20, 1.0],   // red brick
        wall2: [0.40, 0.19, 0.14, 1.0],  // shadowed brick
        roof: [0.30, 0.30, 0.34, 1.0],   // grey slate
        roof2: [0.22, 0.22, 0.26, 1.0],
        trim: [0.20, 0.18, 0.18, 1.0],   // iron/soot
        glass: [1.0, 0.82, 0.46, 1.0],   // gas-lamp warmth
    };
    /// SPACE age: brushed metal + glass curtain, lit cool — sleek blocks crowned with
    /// domes (added by the renderer). The biggest visual leap from the rest.
    pub const ERA_SPACE: EraStyle = EraStyle {
        wall: [0.62, 0.66, 0.72, 1.0],   // brushed steel
        wall2: [0.46, 0.50, 0.58, 1.0],  // shadowed panel
        roof: [0.56, 0.62, 0.70, 1.0],   // metal deck
        roof2: [0.42, 0.48, 0.56, 1.0],
        trim: [0.30, 0.36, 0.44, 1.0],   // dark seam
        glass: [0.50, 0.82, 1.0, 1.0],   // cool electric blue-white
    };

    #[inline]
    fn tint(c: [f32; 4], k: f32) -> [f32; 4] {
        [c[0] * k, c[1] * k, c[2] * k, c[3]]
    }

    /// Which way a wall segment faces (its outward normal in the XZ plane). A
    /// segment is authored as a thin slab on the perimeter of a cell, on the side
    /// indicated, so the wall sits flush with the footprint edge.
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub enum Facing {
        North, // -Z
        South, // +Z
        East,  // +X
        West,  // -X
    }
    impl Facing {
        /// Outward unit normal in (x, z).
        fn nrm(self) -> (f32, f32) {
            match self {
                Facing::North => (0.0, -1.0),
                Facing::South => (0.0, 1.0),
                Facing::East => (1.0, 0.0),
                Facing::West => (-1.0, 0.0),
            }
        }
        /// True if the wall runs east-west (along X) — i.e. faces N or S.
        fn runs_ew(self) -> bool {
            matches!(self, Facing::North | Facing::South)
        }
    }

    /// A solid wall segment one storey tall, on the `face` edge of cell (cx,cz),
    /// resting on `y0`, `height` tall. `seed` jitters tone so the masonry reads
    /// hand-laid. A thin timber sill caps each storey.
    pub fn wall_segment(
        out: &mut Vec<LitVertex>,
        cx: f32,
        y0: f32,
        cz: f32,
        face: Facing,
        height: f32,
        seed: f32,
    ) {
        let tone = 0.90 + 0.16 * seed;
        let (nx, nz) = face.nrm();
        // place the slab on the cell edge, just inside so it sits flush.
        let edge = 0.46; // half a cell minus a sliver
        let px = cx + nx * edge;
        let pz = cz + nz * edge;
        let thick = 0.10;
        let len = 0.48; // half-extent → ~0.96 cell run
        let half = if face.runs_ew() {
            [len, height * 0.5, thick]
        } else {
            [thick, height * 0.5, len]
        };
        push_box(out, [px, y0 + height * 0.5, pz], half, 0.0, tint(STONE, tone));
        // a proud timber sill/lintel band at the top of the storey to catch light.
        let band = if face.runs_ew() {
            [len + 0.015, 0.045, thick + 0.03]
        } else {
            [thick + 0.03, 0.045, len + 0.015]
        };
        push_box(out, [px, y0 + height - 0.02, pz], band, 0.0, tint(TIMBER_LT, tone));
    }

    /// A wall segment with a doorway: two stone jambs + a timber lintel over an
    /// open threshold, and a warm-wood door leaf in the opening.
    pub fn wall_door(out: &mut Vec<LitVertex>, cx: f32, y0: f32, cz: f32, face: Facing, height: f32, seed: f32) {
        let tone = 0.90 + 0.16 * seed;
        let (nx, nz) = face.nrm();
        let edge = 0.46;
        let px = cx + nx * edge;
        let pz = cz + nz * edge;
        let thick = 0.10;
        let len = 0.48;
        let jamb = 0.13; // each side jamb half-width
        let door_h = (height * 0.78).min(height - 0.12);
        // jambs (left/right of the opening)
        for s in [-1.0f32, 1.0] {
            let half = if face.runs_ew() {
                [jamb, height * 0.5, thick]
            } else {
                [thick, height * 0.5, jamb]
            };
            let (ox, oz) = if face.runs_ew() {
                (s * (len - jamb), 0.0)
            } else {
                (0.0, s * (len - jamb))
            };
            push_box(out, [px + ox, y0 + height * 0.5, pz + oz], half, 0.0, tint(STONE, tone));
        }
        // lintel across the top
        let lintel_h = height - door_h;
        let lhalf = if face.runs_ew() {
            [len, lintel_h * 0.5, thick + 0.005]
        } else {
            [thick + 0.005, lintel_h * 0.5, len]
        };
        push_box(out, [px, y0 + door_h + lintel_h * 0.5, pz], lhalf, 0.0, tint(STONE, tone));
        // a sturdy timber door leaf set just inside the opening.
        let dw = len - jamb - 0.02;
        let dhalf = if face.runs_ew() {
            [dw, door_h * 0.5, 0.04]
        } else {
            [0.04, door_h * 0.5, dw]
        };
        let inset = 0.06;
        push_box(
            out,
            [px - nx * inset, y0 + door_h * 0.5, pz - nz * inset],
            dhalf,
            0.0,
            TIMBER,
        );
        // top sill band
        let band = if face.runs_ew() {
            [len + 0.015, 0.045, thick + 0.03]
        } else {
            [thick + 0.03, 0.045, len + 0.015]
        };
        push_box(out, [px, y0 + height - 0.02, pz], band, 0.0, tint(TIMBER_LT, tone));
    }

    /// A wall segment with a window: solid stone with a recessed framed opening and
    /// a warm glowing pane. Returns the window's world centre so the caller can add
    /// an additive glow billboard there (the emissive look).
    pub fn wall_window(
        out: &mut Vec<LitVertex>,
        cx: f32,
        y0: f32,
        cz: f32,
        face: Facing,
        height: f32,
        seed: f32,
    ) -> [f32; 3] {
        let tone = 0.90 + 0.16 * seed;
        let (nx, nz) = face.nrm();
        let edge = 0.46;
        let px = cx + nx * edge;
        let pz = cz + nz * edge;
        let thick = 0.10;
        let len = 0.48;
        // the solid wall (window is carved as overlay boxes — cheaper than CSG and
        // reads fine at this scale).
        let half = if face.runs_ew() {
            [len, height * 0.5, thick]
        } else {
            [thick, height * 0.5, len]
        };
        push_box(out, [px, y0 + height * 0.5, pz], half, 0.0, tint(STONE, tone));
        // window geometry: a frame + a glowing pane proud of the wall face.
        let wy = y0 + height * 0.55;
        let ww = 0.18; // pane half-width
        let wh = 0.16; // pane half-height
        let out_off = thick + 0.012;
        let pane_c = [px + nx * out_off, wy, pz + nz * out_off];
        // dark frame slightly larger, set first (behind), pane on top, proud.
        let (fhalf, phalf) = if face.runs_ew() {
            ([ww + 0.04, wh + 0.04, 0.02], [ww, wh, 0.015])
        } else {
            ([0.02, wh + 0.04, ww + 0.04], [0.015, wh, ww])
        };
        push_box(out, [px + nx * (thick + 0.006), wy, pz + nz * (thick + 0.006)], fhalf, 0.0, FRAME);
        push_box(out, pane_c, phalf, 0.0, GLASS);
        // a thin glazing-bar cross so the pane reads as a real window.
        let (bv, bh) = if face.runs_ew() {
            ([0.012, wh, 0.018], [ww, 0.012, 0.018])
        } else {
            ([0.018, wh, 0.012], [0.018, 0.012, ww])
        };
        push_box(out, [pane_c[0], pane_c[1], pane_c[2]], bv, 0.0, FRAME);
        push_box(out, [pane_c[0], pane_c[1], pane_c[2]], bh, 0.0, FRAME);
        // top sill band
        let band = if face.runs_ew() {
            [len + 0.015, 0.045, thick + 0.03]
        } else {
            [thick + 0.03, 0.045, len + 0.015]
        };
        push_box(out, [px, y0 + height - 0.02, pz], band, 0.0, tint(TIMBER_LT, tone));
        pane_c
    }

    /// A floor / ceiling slab spanning a rectangle [x0,x1]×[z0,z1] (world units)
    /// at height `y`, `thick` thick. Plank-grained albedo.
    pub fn floor_slab(out: &mut Vec<LitVertex>, x0: f32, z0: f32, x1: f32, z1: f32, y: f32, thick: f32, c: [f32; 4]) {
        let cx = (x0 + x1) * 0.5;
        let cz = (z0 + z1) * 0.5;
        let hx = (x1 - x0) * 0.5;
        let hz = (z1 - z0) * 0.5;
        push_box(out, [cx, y, cz], [hx, thick * 0.5, hz], 0.0, c);
    }

    /// A corner pillar/post one storey tall at (cx,cz), `y0` base, `height` tall.
    pub fn pillar(out: &mut Vec<LitVertex>, cx: f32, y0: f32, cz: f32, height: f32) {
        push_box(out, [cx, y0 + height * 0.5, cz], [0.075, height * 0.5, 0.075], 0.0, TIMBER);
        // a faint stone footing so it doesn't float.
        push_box(out, [cx, y0 + 0.04, cz], [0.10, 0.04, 0.10], 0.0, STONE_DARK);
    }

    /// A real stepped staircase climbing `rise` over its run, RISING toward -Z (the
    /// building wall) — the low entry tread is at the +Z (outer) end and the treads
    /// climb up to the landing at the wall, so you walk in from outside and up onto
    /// the upper floor. `steps` treads, sitting at floor level `y0`. Each tread is a
    /// box; the stack reads as proper stairs, not a ramp.
    pub fn staircase(out: &mut Vec<LitVertex>, cx: f32, y0: f32, cz: f32, rise: f32, steps: usize) {
        let steps = steps.max(2);
        let run = 0.78; // total horizontal run (within a cell)
        let tread_d = run / steps as f32;
        let step_h = rise / steps as f32;
        for i in 0..steps {
            // tallest at the -Z (near-wall) end, shortest at the +Z (outer) entry, so
            // the flight climbs toward the wall/landing rather than away from it.
            let h = (steps - i) as f32 * step_h;
            let z = cz - run * 0.5 + tread_d * (i as f32 + 0.5);
            push_box(
                out,
                [cx, y0 + h * 0.5, z],
                [0.20, h * 0.5, tread_d * 0.5 + 0.005],
                0.0,
                tint(TIMBER_LT, 0.95),
            );
        }
    }

    /// A pitched (gable) roof over a rectangle [x0,x1]×[z0,z1], ridge running along
    /// the longer axis, eaves overhanging a touch, `peak` above `y`.
    pub fn pitched_roof(out: &mut Vec<LitVertex>, x0: f32, z0: f32, x1: f32, z1: f32, y: f32, peak: f32) {
        let ov = 0.16; // eave overhang
        let (ax0, az0, ax1, az1) = (x0 - ov, z0 - ov, x1 + ov, z1 + ov);
        let ridge_along_x = (ax1 - ax0) >= (az1 - az0);
        let yt = y + peak;
        if ridge_along_x {
            let zm = (az0 + az1) * 0.5;
            let r0 = [ax0, yt, zm];
            let r1 = [ax1, yt, zm];
            // two slopes
            push_quad(out, [ax0, y, az0], [ax1, y, az0], r1, r0, ROOF);
            push_quad(out, [ax1, y, az1], [ax0, y, az1], r0, r1, tint(ROOF, 0.86));
            // gable triangles (the warm-lit ends)
            push_tri(out, [ax0, y, az0], r0, [ax0, y, az1], tint(ROOF_LT, 0.95));
            push_tri(out, [ax1, y, az0], [ax1, y, az1], r1, tint(ROOF_LT, 0.95));
            // a ridge beam highlight
            push_box(out, [(ax0 + ax1) * 0.5, yt, zm], [(ax1 - ax0) * 0.5, 0.03, 0.04], 0.0, ROOF_LT);
        } else {
            let xm = (ax0 + ax1) * 0.5;
            let r0 = [xm, yt, az0];
            let r1 = [xm, yt, az1];
            push_quad(out, [ax0, y, az0], [ax0, y, az1], r1, r0, ROOF);
            push_quad(out, [ax1, y, az1], [ax1, y, az0], r0, r1, tint(ROOF, 0.86));
            push_tri(out, [ax0, y, az0], r0, [ax1, y, az0], tint(ROOF_LT, 0.95));
            push_tri(out, [ax0, y, az1], [ax1, y, az1], r1, tint(ROOF_LT, 0.95));
            push_box(out, [xm, yt, (az0 + az1) * 0.5], [0.04, 0.03, (az1 - az0) * 0.5], 0.0, ROOF_LT);
        }
        // a thin eave fascia all round so the roof has a crisp lower edge.
        push_box(out, [(ax0 + ax1) * 0.5, y - 0.01, (az0 + az1) * 0.5], [(ax1 - ax0) * 0.5, 0.03, (az1 - az0) * 0.5], 0.0, tint(TIMBER, 0.9));
    }

    /// A CRENELLATED battlement crown for a watchtower: a flat roof slab plus a ring of
    /// merlons (the toothed parapet) around the edge, so the lookout reads unmistakably
    /// as a tower-top. Returns the roof-deck centre so the caller can sit a beacon there.
    pub fn battlement(out: &mut Vec<LitVertex>, x0: f32, z0: f32, x1: f32, z1: f32, y: f32) -> [f32; 3] {
        floor_slab(out, x0, z0, x1, z1, y, 0.10, tint(STONE_DARK, 1.02));
        let merlon_h = 0.20;
        let mw = 0.13; // merlon half-width
        // walk each of the four edges placing alternating merlons (toothed gaps between).
        let nx = ((x1 - x0) / 0.34).round().max(2.0) as i32;
        let nz = ((z1 - z0) / 0.34).round().max(2.0) as i32;
        for i in 0..=nx {
            if i % 2 == 1 {
                continue; // the gap between teeth
            }
            let px = x0 + (x1 - x0) * i as f32 / nx as f32;
            for pz in [z0, z1] {
                push_box(out, [px, y + 0.10 + merlon_h, pz], [mw, merlon_h, mw], 0.0, tint(STONE, 1.0));
            }
        }
        for j in 0..=nz {
            if j % 2 == 1 {
                continue;
            }
            let pz = z0 + (z1 - z0) * j as f32 / nz as f32;
            for px in [x0, x1] {
                push_box(out, [px, y + 0.10 + merlon_h, pz], [mw, merlon_h, mw], 0.0, tint(STONE, 1.0));
            }
        }
        [(x0 + x1) * 0.5, y + 0.12, (z0 + z1) * 0.5]
    }

    /// A steep CONICAL thatch roof for a granary store-hall — a tall straw cone that
    /// reads as distinct from the village's pitched stone-house roofs. Built from the
    /// `push_cone` primitive; warm straw-gold. `peak` is the cone height above `y`.
    pub fn thatch_cone(out: &mut Vec<LitVertex>, cx: f32, cz: f32, radius: f32, y: f32, peak: f32) {
        super::push_cone(out, cx, y, cz, radius, peak, 8, tint([0.72, 0.54, 0.26, 1.0], 1.0));
        // a darker eave ring so the thatch has a crisp lower edge over the wall-head.
        push_box(out, [cx, y - 0.02, cz], [radius * 0.96, 0.04, radius * 0.96], 0.0, tint(TIMBER, 0.9));
    }

    /// A flat parapet roof (a slab + a low surrounding lip) — used for the lower
    /// storeys of a stacked tower so an upper storey can sit on it.
    pub fn flat_roof(out: &mut Vec<LitVertex>, x0: f32, z0: f32, x1: f32, z1: f32, y: f32) {
        floor_slab(out, x0, z0, x1, z1, y, 0.10, tint(STONE_DARK, 1.05));
        // a low parapet lip around the edge
        let lip = 0.05;
        let (cx, cz) = ((x0 + x1) * 0.5, (z0 + z1) * 0.5);
        let (hx, hz) = ((x1 - x0) * 0.5, (z1 - z0) * 0.5);
        for (ox, oz, sx, sz) in [
            (0.0, -hz, hx, 0.04f32),
            (0.0, hz, hx, 0.04),
            (-hx, 0.0, 0.04, hz),
            (hx, 0.0, 0.04, hz),
        ] {
            push_box(out, [cx + ox, y + lip, cz + oz], [sx, lip, sz], 0.0, tint(STONE, 0.95));
        }
    }

    // --- ERA-AWARE pieces (Civilization Sprint 1) -------------------------------
    // These mirror the wall/window/door/roof pieces above but paint with a per-era
    // [`EraStyle`] instead of the fixed constants, so a village's buildings visibly
    // change material as it climbs the tech ladder. The geometry is the same shape so
    // the build silhouette is stable; only the materials (and, per-era, added chimneys/
    // smokestacks/domes via the feature pieces) change.

    /// A solid wall segment, era-styled. Same shape as [`wall_segment`].
    pub fn wall_segment_era(out: &mut Vec<LitVertex>, cx: f32, y0: f32, cz: f32, face: Facing, height: f32, seed: f32, st: &EraStyle) {
        let tone = 0.90 + 0.16 * seed;
        let (nx, nz) = face.nrm();
        let edge = 0.46;
        let (px, pz) = (cx + nx * edge, cz + nz * edge);
        let thick = 0.10;
        let len = 0.48;
        let half = if face.runs_ew() { [len, height * 0.5, thick] } else { [thick, height * 0.5, len] };
        push_box(out, [px, y0 + height * 0.5, pz], half, 0.0, tint(st.wall, tone));
        // a course/sill band in the second wall tone (timber beam, brick course, seam).
        let band = if face.runs_ew() { [len + 0.015, 0.045, thick + 0.03] } else { [thick + 0.03, 0.045, len + 0.015] };
        push_box(out, [px, y0 + height - 0.02, pz], band, 0.0, tint(st.trim, tone));
    }

    /// A wall segment with a glowing window, era-styled. Returns the pane centre (for
    /// the additive glow). Same shape as [`wall_window`].
    pub fn wall_window_era(out: &mut Vec<LitVertex>, cx: f32, y0: f32, cz: f32, face: Facing, height: f32, seed: f32, st: &EraStyle) -> [f32; 3] {
        let tone = 0.90 + 0.16 * seed;
        let (nx, nz) = face.nrm();
        let edge = 0.46;
        let (px, pz) = (cx + nx * edge, cz + nz * edge);
        let thick = 0.10;
        let len = 0.48;
        let half = if face.runs_ew() { [len, height * 0.5, thick] } else { [thick, height * 0.5, len] };
        push_box(out, [px, y0 + height * 0.5, pz], half, 0.0, tint(st.wall, tone));
        let wy = y0 + height * 0.55;
        let (ww, wh) = (0.18f32, 0.16f32);
        let out_off = thick + 0.012;
        let pane_c = [px + nx * out_off, wy, pz + nz * out_off];
        let (fhalf, phalf) = if face.runs_ew() {
            ([ww + 0.04, wh + 0.04, 0.02], [ww, wh, 0.015])
        } else {
            ([0.02, wh + 0.04, ww + 0.04], [0.015, wh, ww])
        };
        push_box(out, [px + nx * (thick + 0.006), wy, pz + nz * (thick + 0.006)], fhalf, 0.0, st.trim);
        push_box(out, pane_c, phalf, 0.0, st.glass);
        // glazing-bar cross.
        let (bv, bh) = if face.runs_ew() {
            ([0.012, wh, 0.018], [ww, 0.012, 0.018])
        } else {
            ([0.018, wh, 0.012], [0.018, 0.012, ww])
        };
        push_box(out, pane_c, bv, 0.0, st.trim);
        push_box(out, pane_c, bh, 0.0, st.trim);
        let band = if face.runs_ew() { [len + 0.015, 0.045, thick + 0.03] } else { [thick + 0.03, 0.045, len + 0.015] };
        push_box(out, [px, y0 + height - 0.02, pz], band, 0.0, tint(st.trim, tone));
        pane_c
    }

    /// A wall segment with a doorway, era-styled. Same shape as [`wall_door`].
    pub fn wall_door_era(out: &mut Vec<LitVertex>, cx: f32, y0: f32, cz: f32, face: Facing, height: f32, seed: f32, st: &EraStyle) {
        let tone = 0.90 + 0.16 * seed;
        let (nx, nz) = face.nrm();
        let edge = 0.46;
        let (px, pz) = (cx + nx * edge, cz + nz * edge);
        let thick = 0.10;
        let len = 0.48;
        let jamb = 0.13;
        let door_h = (height * 0.78).min(height - 0.12);
        for s in [-1.0f32, 1.0] {
            let half = if face.runs_ew() { [jamb, height * 0.5, thick] } else { [thick, height * 0.5, jamb] };
            let (ox, oz) = if face.runs_ew() { (s * (len - jamb), 0.0) } else { (0.0, s * (len - jamb)) };
            push_box(out, [px + ox, y0 + height * 0.5, pz + oz], half, 0.0, tint(st.wall, tone));
        }
        let lintel_h = height - door_h;
        let lhalf = if face.runs_ew() { [len, lintel_h * 0.5, thick + 0.005] } else { [thick + 0.005, lintel_h * 0.5, len] };
        push_box(out, [px, y0 + door_h + lintel_h * 0.5, pz], lhalf, 0.0, tint(st.wall, tone));
        // a recessed DARK threshold behind the leaf so the opening reads as an actual
        // entry (a real doorway, not just a painted panel), even at a distance.
        let dw = len - jamb - 0.02;
        let dark = [st.trim[0] * 0.35, st.trim[1] * 0.35, st.trim[2] * 0.35, 1.0];
        let rhalf = if face.runs_ew() { [dw, door_h * 0.5, 0.03] } else { [0.03, door_h * 0.5, dw] };
        push_box(out, [px - nx * 0.10, y0 + door_h * 0.5, pz - nz * 0.10], rhalf, 0.0, dark);
        // the door LEAF itself — a warm timber slab set in the opening, slightly proud
        // so it catches light and clearly reads as a door (two plank-tone halves).
        let leaf = [st.trim[0] * 1.15, st.trim[1] * 1.05, st.trim[2] * 0.95, 1.0];
        let dhalf = if face.runs_ew() { [dw, door_h * 0.5, 0.05] } else { [0.05, door_h * 0.5, dw] };
        let inset = 0.04;
        push_box(out, [px - nx * inset, y0 + door_h * 0.5, pz - nz * inset], dhalf, 0.0, leaf);
        // a raised stone threshold step at the foot of the door (you step up to enter).
        let shalf = if face.runs_ew() { [len * 0.7, 0.04, thick + 0.08] } else { [thick + 0.08, 0.04, len * 0.7] };
        push_box(out, [px + nx * 0.06, y0 + 0.04, pz + nz * 0.06], shalf, 0.0, tint(STONE_DARK, tone));
        // a small brass door HANDLE so it reads as a working door up close.
        let off = dw * 0.55;
        let (hx, hz) = if face.runs_ew() { (off, 0.0) } else { (0.0, off) };
        push_box(out, [px - nx * (inset + 0.04) + hx, y0 + door_h * 0.45, pz - nz * (inset + 0.04) + hz], [0.025, 0.04, 0.025], 0.0, [0.72, 0.58, 0.26, 1.0]);
    }

    /// A pitched (gable) roof, era-styled. Same shape as [`pitched_roof`].
    pub fn pitched_roof_era(out: &mut Vec<LitVertex>, x0: f32, z0: f32, x1: f32, z1: f32, y: f32, peak: f32, st: &EraStyle) {
        let ov = 0.16;
        let (ax0, az0, ax1, az1) = (x0 - ov, z0 - ov, x1 + ov, z1 + ov);
        let ridge_along_x = (ax1 - ax0) >= (az1 - az0);
        let yt = y + peak;
        if ridge_along_x {
            let zm = (az0 + az1) * 0.5;
            let (r0, r1) = ([ax0, yt, zm], [ax1, yt, zm]);
            push_quad(out, [ax0, y, az0], [ax1, y, az0], r1, r0, st.roof);
            push_quad(out, [ax1, y, az1], [ax0, y, az1], r0, r1, st.roof2);
            push_tri(out, [ax0, y, az0], r0, [ax0, y, az1], tint(st.roof, 1.08));
            push_tri(out, [ax1, y, az0], [ax1, y, az1], r1, tint(st.roof, 1.08));
            push_box(out, [(ax0 + ax1) * 0.5, yt, zm], [(ax1 - ax0) * 0.5, 0.03, 0.04], 0.0, tint(st.roof, 1.12));
        } else {
            let xm = (ax0 + ax1) * 0.5;
            let (r0, r1) = ([xm, yt, az0], [xm, yt, az1]);
            push_quad(out, [ax0, y, az0], [ax0, y, az1], r1, r0, st.roof);
            push_quad(out, [ax1, y, az1], [ax1, y, az0], r0, r1, st.roof2);
            push_tri(out, [ax0, y, az0], r0, [ax1, y, az0], tint(st.roof, 1.08));
            push_tri(out, [ax0, y, az1], [ax1, y, az1], r1, tint(st.roof, 1.08));
            push_box(out, [xm, yt, (az0 + az1) * 0.5], [0.04, 0.03, (az1 - az0) * 0.5], 0.0, tint(st.roof, 1.12));
        }
        push_box(out, [(ax0 + ax1) * 0.5, y - 0.01, (az0 + az1) * 0.5], [(ax1 - ax0) * 0.5, 0.03, (az1 - az0) * 0.5], 0.0, tint(st.trim, 0.95));
    }

    /// A flat parapet roof, era-styled. Same shape as [`flat_roof`].
    pub fn flat_roof_era(out: &mut Vec<LitVertex>, x0: f32, z0: f32, x1: f32, z1: f32, y: f32, st: &EraStyle) {
        floor_slab(out, x0, z0, x1, z1, y, 0.10, tint(st.roof, 1.05));
        let lip = 0.05;
        let (cx, cz) = ((x0 + x1) * 0.5, (z0 + z1) * 0.5);
        let (hx, hz) = ((x1 - x0) * 0.5, (z1 - z0) * 0.5);
        for (ox, oz, sx, sz) in [
            (0.0, -hz, hx, 0.04f32),
            (0.0, hz, hx, 0.04),
            (-hx, 0.0, 0.04, hz),
            (hx, 0.0, 0.04, hz),
        ] {
            push_box(out, [cx + ox, y + lip, cz + oz], [sx, lip, sz], 0.0, tint(st.trim, 0.95));
        }
    }

    /// A brick CHIMNEY on a roof: a slim stack with a darker cap. Returns the smoke
    /// origin (cap top) so the renderer can puff a soot plume there. The Industrial-age
    /// house signature.
    pub fn chimney(out: &mut Vec<LitVertex>, cx: f32, cz: f32, y: f32, h: f32, st: &EraStyle) -> [f32; 3] {
        let w = 0.11;
        push_box(out, [cx, y + h * 0.5, cz], [w, h * 0.5, w], 0.0, st.wall);
        // a soot-dark cap.
        push_box(out, [cx, y + h + 0.04, cz], [w + 0.03, 0.05, w + 0.03], 0.0, st.trim);
        [cx, y + h + 0.10, cz]
    }

    /// A factory SMOKESTACK: a tall tapered column on a brick base with a banded top.
    /// Returns the smoke origin. The Industrial-age landmark (raised on big buildings).
    pub fn smokestack(out: &mut Vec<LitVertex>, cx: f32, cz: f32, y: f32, h: f32, st: &EraStyle) -> [f32; 3] {
        // a brick base block.
        push_box(out, [cx, y + 0.18, cz], [0.20, 0.18, 0.20], 0.0, st.wall);
        // the tapered column (two stacked boxes, the upper slimmer).
        let lower = h * 0.6;
        push_box(out, [cx, y + 0.36 + lower * 0.5, cz], [0.13, lower * 0.5, 0.13], 0.0, tint(st.wall, 0.92));
        let upper = h * 0.4;
        push_box(out, [cx, y + 0.36 + lower + upper * 0.5, cz], [0.095, upper * 0.5, 0.095], 0.0, tint(st.wall, 0.86));
        // soot bands near the top.
        push_box(out, [cx, y + 0.36 + h - 0.06, cz], [0.115, 0.05, 0.115], 0.0, st.trim);
        [cx, y + 0.36 + h + 0.06, cz]
    }

    /// A sleek glass DOME crowning a Space-age block: a low metal drum + a glowing
    /// glass cupola. Returns the dome apex (for a cool electric glow). The far-future
    /// signature, the biggest read of the top of the ladder.
    pub fn dome(out: &mut Vec<LitVertex>, cx: f32, cz: f32, radius: f32, y: f32, st: &EraStyle) -> [f32; 3] {
        // a metal drum ring.
        push_box(out, [cx, y + 0.06, cz], [radius, 0.06, radius], 0.0, st.wall2);
        // the glass cupola — a squat cone in cool glass, plus a bright apex cap.
        let peak = radius * 1.05;
        super::push_cone(out, cx, y + 0.10, cz, radius * 0.92, peak, 10, st.glass);
        // a metal apex finial.
        push_box(out, [cx, y + 0.10 + peak, cz], [0.05, 0.07, 0.05], 0.0, st.wall);
        [cx, y + 0.10 + peak + 0.05, cz]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staircase_climbs_toward_the_wall() {
        // a flight at cz=10 rising 2.0 over 6 steps. The treads must get TALLER toward
        // -Z (the building wall) and shorter toward +Z (the outer entry), so the stair
        // climbs up to the landing rather than away from it.
        let mut v = Vec::new();
        pieces::staircase(&mut v, 0.0, 0.0, 10.0, 2.0, 6);
        // the highest vertex (top of the tallest tread) and the lowest non-zero one.
        let top = v.iter().max_by(|a, b| a.pos[1].total_cmp(&b.pos[1])).unwrap();
        let bot = v
            .iter()
            .filter(|p| p.pos[1] > 0.01)
            .min_by(|a, b| a.pos[1].total_cmp(&b.pos[1]))
            .unwrap();
        assert!(
            top.pos[2] < bot.pos[2],
            "tallest tread (z={}) should be toward -Z of the shortest (z={})",
            top.pos[2],
            bot.pos[2]
        );
    }

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
    fn villager_emits_trilist_and_walks() {
        // a striding villager's legs must visibly SWING with phase, while an idle one
        // holds the same pose at any phase (stride 0 ⇒ no motion). This guards the gait.
        let mut a = Vec::new();
        let hp = std::f32::consts::FRAC_PI_2;
        push_villager(&mut a, 0.0, 0.0, 0.0, 0.0, 1.0, hp, 1.0, [0.4, 0.6, 0.3, 1.0], [0.9, 0.8, 0.7, 1.0]);
        assert_eq!(a.len() % 3, 0);
        assert!(!a.is_empty());
        let mut b = Vec::new();
        push_villager(&mut b, 0.0, 0.0, 0.0, 0.0, 1.0, 3.0 * hp, 1.0, [0.4, 0.6, 0.3, 1.0], [0.9, 0.8, 0.7, 1.0]);
        // some vertex must move between the two stride phases (the legs/arms swung).
        let moved = a.iter().zip(&b).any(|(p, q)| (p.pos[0] - q.pos[0]).abs() + (p.pos[2] - q.pos[2]).abs() > 1e-3);
        assert!(moved, "a striding villager must change pose with phase");
        // idle (stride 0) holds pose regardless of phase.
        let mut c = Vec::new();
        let mut d = Vec::new();
        push_villager(&mut c, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, [0.4, 0.6, 0.3, 1.0], [0.9, 0.8, 0.7, 1.0]);
        push_villager(&mut d, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 0.0, [0.4, 0.6, 0.3, 1.0], [0.9, 0.8, 0.7, 1.0]);
        let still = c.iter().zip(&d).all(|(p, q)| (p.pos[0] - q.pos[0]).abs() + (p.pos[1] - q.pos[1]).abs() + (p.pos[2] - q.pos[2]).abs() < 1e-4);
        assert!(still, "an idle villager must not animate");
    }

    #[test]
    fn box_and_cone_emit_trilists() {
        let mut v = Vec::new();
        push_box(&mut v, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0], 0.3, [1.0, 1.0, 1.0, 1.0]);
        push_cone(&mut v, 0.0, 0.0, 0.0, 1.0, 2.0, 5, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(v.len() % 3, 0);
    }
}
