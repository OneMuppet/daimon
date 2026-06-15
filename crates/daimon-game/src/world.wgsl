// Daimon's world shaders — luminous low-poly isometric.
//
// Four passes share one uniform block, drawn into a LOW-RES offscreen target
// (then upscaled NEAREST for chunky painterly pixels):
//   1. lit   — terrain + actors. Normals come from dpdx/dpdy, so the facets
//              light themselves; season + day/night grade and cloud shadows on
//              top; distance fog dissolves the rim into the sky.
//   2. water — one translucent plane: fresnel, scrolling shimmer, a sun glint.
//   3. add   — additive soft glows (mood auras, the hearth, weather motes).
//   4. blit  — fullscreen triangle, NEAREST-samples the RT onto the swapchain.
//
// The whole sky palette (sun colour/strength, ambient, horizon, daylight) is
// computed on the CPU per time-of-day and handed in — the shader stays generic.

struct U {
    view_proj          : mat4x4<f32>,
    cam_pos_time       : vec4<f32>, // xyz eye, w time
    sun_dir            : vec4<f32>, // xyz toward sun, w daylight 0..1
    sun_color_strength : vec4<f32>,
    ambient            : vec4<f32>, // rgb ambient, w unused
    horizon_fogfar     : vec4<f32>, // rgb fog/horizon, w fog far
    season_tint        : vec4<f32>, // rgb cast, w mix strength
    misc               : vec4<f32>, // x weather, y weather_kind, z,w unused
};
@group(0) @binding(0) var<uniform> u : U;

fn hash21(p : vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453123);
}
fn vnoise(p : vec2<f32>) -> f32 {
    let i = floor(p); let f = fract(p);
    let s = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, s.x), mix(c, d, s.x), s.y);
}

fn shade(albedo : vec3<f32>, n : vec3<f32>) -> vec3<f32> {
    let sun = max(dot(n, u.sun_dir.xyz), 0.0) * u.sun_color_strength.rgb * u.sun_color_strength.w;
    let amb = u.ambient.rgb * (0.6 + 0.4 * max(n.y, 0.0));
    return albedo * (amb + sun);
}
fn tonemap(c : vec3<f32>) -> vec3<f32> {
    return vec3<f32>(1.0) - exp(-c * 1.5);
}
fn fog_mix(c : vec3<f32>, wp : vec3<f32>) -> vec3<f32> {
    // Horizontal distance from the view centre — an orthographic eye is
    // equidistant from every fragment, so eye-distance fog would tint uniformly.
    let d = length(wp.xz - vec2<f32>(u.misc.z, u.misc.w));
    let f = smoothstep(u.horizon_fogfar.w * 0.6, u.horizon_fogfar.w, d) * 0.85;
    return mix(c, u.horizon_fogfar.rgb, f);
}
fn facet_normal(wp : vec3<f32>) -> vec3<f32> {
    var n = normalize(cross(dpdx(wp), dpdy(wp)));
    let v = normalize(u.cam_pos_time.xyz - wp);
    if (dot(n, v) < 0.0) { n = -n; }
    return n;
}

struct LitIn  { @location(0) pos : vec3<f32>, @location(1) color : vec4<f32> };
struct LitOut { @builtin(position) clip : vec4<f32>, @location(0) wp : vec3<f32>, @location(1) color : vec4<f32> };

@vertex
fn vs_lit(in : LitIn) -> LitOut {
    var out : LitOut;
    out.clip = u.view_proj * vec4<f32>(in.pos, 1.0);
    out.wp = in.pos;
    out.color = in.color;
    return out;
}

@fragment
fn fs_lit(in : LitOut) -> @location(0) vec4<f32> {
    let n = facet_normal(in.wp);
    // The season lays its colour over every albedo (a LERP, so winter is snow).
    let albedo = mix(in.color.rgb, u.season_tint.rgb, u.season_tint.a);
    // Cloud shadows: two drifting octaves slide soft patches across the land.
    let t = u.cam_pos_time.w;
    let cp = in.wp.xz * 0.05 + vec2<f32>(t * 0.02, t * 0.013);
    let cl = vnoise(cp) * 0.65 + vnoise(cp * 2.3 + vec2<f32>(t * 0.01, -t * 0.014)) * 0.35;
    let cloud = 1.0 - smoothstep(0.55, 0.85, cl) * (0.30 * (0.4 + 0.6 * u.sun_dir.w));
    var c = tonemap(shade(albedo, n) * cloud);
    // Weather veil: a faint cool wash that thickens with the storm.
    let veil = mix(vec3<f32>(0.42, 0.47, 0.55), vec3<f32>(0.80, 0.84, 0.92), u.misc.y);
    c = mix(c, veil * (0.4 + 0.6 * u.sun_dir.w), u.misc.x * 0.18);
    c = fog_mix(c, in.wp);
    return vec4<f32>(c, in.color.a);
}

// Water: fresnel toward the horizon, scrolling shimmer, a sharp sun glint.
@fragment
fn fs_water(in : LitOut) -> @location(0) vec4<f32> {
    let t = u.cam_pos_time.w;
    let v = normalize(u.cam_pos_time.xyz - in.wp);
    let p = in.wp.xz * 0.5;
    let w1 = vnoise(p + vec2<f32>(t * 0.06, t * 0.045));
    let w2 = vnoise(p * 1.9 - vec2<f32>(t * 0.05, t * 0.07));
    let n = normalize(vec3<f32>((w1 - 0.5) * 0.22, 1.0, (w2 - 0.5) * 0.22));
    let fres = pow(1.0 - max(dot(n, v), 0.0), 3.0);
    let deep = vec3<f32>(0.03, 0.10, 0.20)
        * (u.ambient.rgb * 2.2 + u.sun_color_strength.rgb * u.sun_color_strength.w * 0.4);
    var col = mix(deep, u.horizon_fogfar.rgb, fres * 0.6);
    let hv = normalize(v + u.sun_dir.xyz);
    col += u.sun_color_strength.rgb * u.sun_color_strength.w
        * pow(max(dot(n, hv), 0.0), 140.0) * 1.4 * u.sun_dir.w;
    col = fog_mix(tonemap(col), in.wp);
    let alpha = clamp(0.7 + fres * 0.25, 0.0, 0.95);
    return vec4<f32>(col, alpha);
}

struct AddIn  { @location(0) pos : vec3<f32>, @location(1) color : vec4<f32>, @location(2) uv : vec2<f32> };
struct AddOut { @builtin(position) clip : vec4<f32>, @location(0) color : vec4<f32>, @location(1) uv : vec2<f32> };

@vertex
fn vs_add(in : AddIn) -> AddOut {
    var out : AddOut;
    out.clip = u.view_proj * vec4<f32>(in.pos, 1.0);
    out.color = in.color;
    out.uv = in.uv;
    return out;
}

// A soft round glow: bright core, smooth falloff to nothing at the rim.
@fragment
fn fs_add(in : AddOut) -> @location(0) vec4<f32> {
    let r = length(in.uv);
    let g = pow(1.0 - smoothstep(0.0, 1.0, r), 2.0);
    let a = g * in.color.a;
    return vec4<f32>(in.color.rgb * a, a);
}

// ---- the pixel pass: low-res RT -> swapchain, nearest sampled --------------
@group(0) @binding(1) var blit_tex : texture_2d<f32>;
@group(0) @binding(2) var blit_smp : sampler;

struct BlitOut { @builtin(position) clip : vec4<f32>, @location(0) uv : vec2<f32> };

@vertex
fn vs_blit(@builtin(vertex_index) vi : u32) -> BlitOut {
    var out : BlitOut;
    let x = f32(i32(vi % 2u) * 4 - 1);
    let y = f32(i32(vi / 2u) * 4 - 1);
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x * 0.5 + 0.5, 1.0 - (y * 0.5 + 0.5));
    return out;
}

@fragment
fn fs_blit(in : BlitOut) -> @location(0) vec4<f32> {
    return textureSample(blit_tex, blit_smp, in.uv);
}
