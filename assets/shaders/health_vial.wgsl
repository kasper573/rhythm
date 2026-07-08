// The health vial: a glass capsule holding a liquid whose level, surface
// waves, and bottom-to-top color gradient are driven by uniforms. The
// liquid is one coherent body attached to the walls; only its surface
// misbehaves, and only while turbulence (params.y) is nonzero.

#import bevy_ui::ui_vertex_output::UiVertexOutput
#import bevy_render::globals::Globals

@group(0) @binding(1) var<uniform> globals: Globals;
// (liquid level 0..1, turbulence 0..~1.2, unused, unused)
@group(1) @binding(0) var<uniform> params: vec4<f32>;
// The active gradient, sampled bottom-to-top at 16 even positions.
@group(1) @binding(1) var<uniform> colors: array<vec4<f32>, 16u>;

const OUTER_RADIUS: f32 = 0.46;
const WALL: f32 = 0.07;

fn gradient(height: f32) -> vec3<f32> {
    let x = clamp(height, 0.0, 1.0) * 15.0;
    let index = u32(floor(x));
    let a = colors[min(index, 15u)];
    let b = colors[min(index + 1u, 15u)];
    return mix(a, b, fract(x)).rgb;
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let level = params.x;
    let turbulence = params.y;
    let t = globals.time;

    // Width units, y growing upward from the bottom of the node.
    let aspect = in.size.y / max(in.size.x, 1.0);
    let p = vec2<f32>(in.uv.x, (1.0 - in.uv.y) * aspect);

    // The vial: a vertical capsule around the centerline.
    let bottom_cap = vec2<f32>(0.5, OUTER_RADIUS);
    let top_cap = vec2<f32>(0.5, aspect - OUTER_RADIUS);
    let along = clamp((p.y - bottom_cap.y) / (top_cap.y - bottom_cap.y), 0.0, 1.0);
    let axis = mix(bottom_cap, top_cap, along);
    let d = length(p - axis) - OUTER_RADIUS;

    let aa = max(fwidth(d), 1e-4);
    let outer = smoothstep(aa, -aa, d);
    let inner = smoothstep(aa, -aa, d + WALL);
    if outer <= 0.0 {
        return vec4<f32>(0.0);
    }

    // The liquid surface: the eased level plus decaying wave chaos.
    let cavity_bottom = bottom_cap.y - (OUTER_RADIUS - WALL);
    let cavity_top = top_cap.y + (OUTER_RADIUS - WALL);
    let waves = sin(p.x * 9.4 + t * 7.0) * 0.5
        + sin(p.x * 15.7 - t * 9.3) * 0.3
        + sin(p.x * 23.0 + t * 12.1) * 0.2;
    let surface = mix(cavity_bottom, cavity_top, level) + waves * turbulence * 0.22;
    let liquid = inner * smoothstep(aa * 2.0, -aa * 2.0, p.y - surface);

    // Liquid body: the vial-spanning gradient, rounded by darkening
    // toward the walls, with a bright meniscus line at the surface.
    let height = (p.y - cavity_bottom) / (cavity_top - cavity_bottom);
    let lateral = abs(p.x - 0.5) / OUTER_RADIUS;
    var liquid_color = gradient(height) * (1.08 - 0.38 * lateral * lateral);
    let meniscus = exp(-max(surface - p.y, 0.0) * 26.0) * liquid;
    liquid_color += vec3<f32>(0.35) * meniscus;

    // Empty cavity: the faintest cool tint so the glass reads as hollow.
    let cavity_color = vec3<f32>(0.70, 0.82, 0.90);
    var color = mix(cavity_color, liquid_color, liquid);
    var alpha = mix(0.07, 0.90, liquid);

    // Glass wall: brightest at the silhouette where a real tube bunches
    // up its reflections.
    let wall_mask = clamp(outer - inner, 0.0, 1.0);
    let rim = 0.30 + 0.55 * pow(lateral, 3.0);
    color = mix(color, vec3<f32>(0.75, 0.85, 0.92) * (0.6 + rim), wall_mask);
    alpha = mix(alpha, 0.28 + 0.5 * rim, wall_mask);

    // Two vertical gloss streaks down the whole vial, glass and liquid.
    let gloss = exp(-pow((p.x - 0.31) * 9.0, 2.0)) * 0.30
        + exp(-pow((p.x - 0.72) * 16.0, 2.0)) * 0.12;
    color += vec3<f32>(gloss);
    alpha = min(alpha + gloss * 0.5, 1.0);

    return vec4<f32>(color, alpha * outer);
}
