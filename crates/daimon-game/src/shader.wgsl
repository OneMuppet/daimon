// One pipeline draws everything: rounded rectangles and soft glowing orbs,
// both as signed-distance fields so edges and halos are crisp at any scale.

struct Uniforms {
    screen: vec2<f32>,
    time: f32,
    _pad: f32,
};
@group(0) @binding(0) var<uniform> U: Uniforms;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) params: vec4<f32>,
};

var<private> CORNERS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
    vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
);

@vertex
fn vs(
    @builtin(vertex_index) vi: u32,
    @location(0) rect: vec4<f32>,
    @location(1) color: vec4<f32>,
    @location(2) params: vec4<f32>,
) -> VsOut {
    let corner = CORNERS[vi];
    let px = rect.xy + corner * rect.zw;
    let ndc = vec2<f32>(px.x / U.screen.x * 2.0 - 1.0, 1.0 - px.y / U.screen.y * 2.0);
    var o: VsOut;
    o.clip = vec4<f32>(ndc, 0.0, 1.0);
    o.local = corner * rect.zw;
    o.size = rect.zw;
    o.color = color;
    o.params = params;
    return o;
}

fn sd_round_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r, r);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0, 0.0))) - r;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let radius = in.params.x;
    let soft = max(in.params.y, 0.5);
    let shape = in.params.z;
    let glow = in.params.w;
    let center = in.size * 0.5;
    let p = in.local - center;
    var alpha = 1.0;

    if (shape > 0.5) {
        // orb with optional soft halo
        let d = length(p) - radius;
        let core = 1.0 - smoothstep(-soft, soft, d);
        var a = core;
        if (glow > 0.001) {
            let halo = exp(-max(d, 0.0) / (radius * glow + 0.001));
            a = max(core, halo * 0.55);
        }
        alpha = a;
    } else {
        let d = sd_round_box(p, center, min(radius, min(center.x, center.y)));
        alpha = 1.0 - smoothstep(-soft, soft, d);
    }

    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
