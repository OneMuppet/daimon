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
    let ndl = dot(n, u.sun_dir.xyz);
    // Warm key with a soft wrap so the terminator isn't a hard flat line — gives
    // form to the low-poly facets without crushing the shadow side to black.
    let key = clamp(ndl * 0.78 + 0.22, 0.0, 1.0);
    let sun = key * u.sun_color_strength.rgb * u.sun_color_strength.w;
    // Cool sky FILL from above + a faint warm bounce from below (ground reflection),
    // so shadowed faces read blue-ish and lit faces warm — the golden-hour contrast.
    let sky = vec3<f32>(0.5, 0.62, 0.85) * (0.5 + 0.5 * max(n.y, 0.0));
    let bounce = vec3<f32>(0.32, 0.24, 0.16) * max(-n.y, 0.0) * 0.5;
    let amb = u.ambient.rgb * (sky + bounce) * 1.25;
    // A tight rim/backlight along silhouette edges sells the floating-island glow.
    let v = normalize(u.cam_pos_time.xyz - vec3<f32>(0.0));
    let rim = pow(clamp(1.0 - max(n.y, 0.0), 0.0, 1.0), 3.0) * max(ndl, 0.0) * 0.35;
    return albedo * (amb + sun) + u.sun_color_strength.rgb * rim;
}
fn tonemap(c : vec3<f32>) -> vec3<f32> {
    // ACES-ish filmic curve — richer roll-off into the highlights than a plain
    // exponential, so the warm sun and the glows bloom toward white gracefully.
    let x = c * 1.05;
    let a = 2.51; let b = 0.03; let cc = 2.43; let d = 0.59; let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (cc * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
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

// ---- the post pass: low-res RT -> swapchain, with bloom + god-rays + grade ---
// Single-pass post: the upscaled scene plus (1) a thresholded multi-tap BLOOM so
// the minds' additive orbs actually halo and glow, (2) volumetric GOD-RAYS radiating
// from the sun's screen position, (3) a warm cinematic colour GRADE, and (4) a soft
// VIGNETTE. Kept to one extra pass with cheap tap counts so it stays smooth at 3x.
@group(0) @binding(1) var blit_tex : texture_2d<f32>;
@group(0) @binding(2) var blit_smp : sampler;
@group(1) @binding(0) var<uniform> pu : U; // world uniform (sun dir + daylight) for the post pass

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

// only genuinely HOT pixels contribute to bloom (a high knee), so the minds' hot
// cores and the sun bloom — not the merely-bright lit grass. This keeps the glows
// reading as emissive light against the terrain rather than a uniform haze.
fn bright(c : vec3<f32>) -> vec3<f32> {
    let l = dot(c, vec3<f32>(0.299, 0.587, 0.114));
    let k = smoothstep(0.80, 1.25, l);
    return c * k * (1.0 + k * 1.5); // brighter pixels bloom super-linearly
}

// the sun's position in screen UV (or off-screen), from its world direction.
fn sun_screen_uv() -> vec2<f32> {
    // place the sun far along its direction and project it.
    let far = pu.cam_pos_time.xyz + pu.sun_dir.xyz * 400.0;
    let clip = pu.view_proj * vec4<f32>(far, 1.0);
    let ndc = clip.xy / max(clip.w, 0.0001);
    return vec2<f32>(ndc.x * 0.5 + 0.5, 1.0 - (ndc.y * 0.5 + 0.5));
}

@fragment
fn fs_blit(in : BlitOut) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(blit_tex));
    let texel = 1.0 / dims;
    var base = textureSample(blit_tex, blit_smp, in.uv).rgb;

    // --- ATMOSPHERIC SKY: the world pass clears to the flat horizon colour, which
    // leaves a dead grey-blue slab above the island. Detect those background pixels
    // (they sit at ~the clear colour) and paint a rich vertical gradient over them —
    // a warm gold band low on the horizon rising into deep blue, with a soft sun
    // bloom where the sun sits — so the floating island reads against a real sky. ---
    let bg = pu.horizon_fogfar.rgb;
    let is_sky = 1.0 - smoothstep(0.015, 0.10, distance(base, bg));
    let suv0 = sun_screen_uv();
    let sky_top = vec3<f32>(0.16, 0.23, 0.44);
    let sky_mid = vec3<f32>(0.46, 0.46, 0.62);
    let sky_low = vec3<f32>(1.00, 0.66, 0.40); // warm gold horizon band
    let gy = in.uv.y; // 0 top .. 1 bottom
    var sky = mix(sky_top, sky_mid, smoothstep(0.0, 0.55, gy));
    sky = mix(sky, sky_low, smoothstep(0.42, 0.72, gy));
    // a soft sun disc + halo glow on the sky.
    let sd = distance(in.uv, suv0);
    sky += vec3<f32>(1.0, 0.82, 0.55) * exp(-sd * 9.0) * 1.2 * pu.sun_dir.w;
    sky += vec3<f32>(1.0, 0.74, 0.46) * exp(-sd * 2.2) * 0.35 * pu.sun_dir.w;
    base = mix(base, sky, is_sky);

    // --- BLOOM: two rings of taps around the fragment, weighted, off the bright
    // pass only. Cheap separable-ish gather that gives the orbs a soft halo. ---
    var bloom = vec3<f32>(0.0);
    var wsum = 0.0;
    for (var i = 0; i < 12; i = i + 1) {
        let a = f32(i) / 12.0 * 6.2831853;
        let dir = vec2<f32>(cos(a), sin(a));
        // inner ring (tight) + outer ring (wide) for a layered falloff.
        let s1 = textureSample(blit_tex, blit_smp, in.uv + dir * texel * 2.5).rgb;
        let s2 = textureSample(blit_tex, blit_smp, in.uv + dir * texel * 6.0).rgb;
        bloom += bright(s1) * 1.0 + bright(s2) * 0.55;
        wsum += 1.55;
    }
    bloom = bloom / wsum;

    // --- GOD-RAYS: march toward the sun's screen position, accumulating brightness
    // so the light shafts down through the haze. Only when the sun is up. ---
    var god = vec3<f32>(0.0);
    let suv = sun_screen_uv();
    let to_sun = suv - in.uv;
    let steps = 16;
    var col = vec3<f32>(0.0);
    var illum = 0.45;
    for (var j = 0; j < steps; j = j + 1) {
        let t = f32(j) / f32(steps);
        let p = in.uv + to_sun * t;
        let s = bright(textureSample(blit_tex, blit_smp, p).rgb);
        god += s * illum;
        illum *= 0.92;
    }
    god = god / f32(steps);
    // fade rays with distance from the sun and only in daylight; warm-tint them.
    let sun_d = clamp(1.0 - length(in.uv - suv) * 0.8, 0.0, 1.0);
    let warm_ray = vec3<f32>(1.0, 0.78, 0.46);
    let rays = god * warm_ray * sun_d * pu.sun_dir.w * 0.9;

    var c = base + bloom * 1.7 + rays;

    // --- COLOUR GRADE: a decisive warm golden-hour cast. Lift saturation, warm the
    // highlights toward gold and cool the shadows toward blue (split-tone), then a
    // gentle overall amber wash so the whole frame reads sunlit, not midday-flat. ---
    let luma = dot(c, vec3<f32>(0.299, 0.587, 0.114));
    c = mix(vec3<f32>(luma), c, 1.18);                       // +saturation
    let warm = vec3<f32>(1.10, 1.00, 0.86);
    let cool = vec3<f32>(0.92, 0.98, 1.10);
    c = c * mix(cool, warm, smoothstep(0.12, 0.72, luma));   // split-tone
    c *= vec3<f32>(1.09, 1.02, 0.90);                        // overall amber wash

    // --- VIGNETTE: soft darkening toward the corners to focus the eye. ---
    let q = (in.uv - vec2<f32>(0.5)) * vec2<f32>(1.0, dims.y / dims.x);
    let vig = smoothstep(0.90, 0.28, length(q) * 1.25);
    c *= mix(0.70, 1.0, vig);

    return vec4<f32>(c, 1.0);
}
