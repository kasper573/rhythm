// The health vial: a glass capsule holding a liquid whose level, surface
// waves, and bottom-to-top color gradient are driven by uniforms. The
// liquid is one coherent body attached to the walls; only its surface
// misbehaves, and only while turbulence (params.y) is nonzero. The node
// is larger than the vial by a glow margin on every side, where the
// beat-pulsed glass glow (params.z) falls off.

#import bevy_ui::ui_vertex_output::UiVertexOutput
#import bevy_render::globals::Globals

@group(0) @binding(1) var<uniform> globals: Globals;
// (liquid level 0..1, turbulence 0..~1.2, glow pulse 0..1, gradient scroll 0..2)
@group(1) @binding(0) var<uniform> params: vec4<f32>;
// (glow margin in node pixels, unused, unused, unused)
@group(1) @binding(1) var<uniform> geometry: vec4<f32>;
// The active gradient, sampled bottom-to-top at 16 even positions.
@group(1) @binding(2) var<uniform> colors: array<vec4<f32>, 16u>;

const OUTER_RADIUS: f32 = 0.46;
const WALL: f32 = 0.07;
const GLASS_TINT: vec3<f32> = vec3<f32>(0.75, 0.85, 0.92);

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
    let glow_pulse = params.z;
    let scroll = params.w;
    let margin = geometry.x;

    // Vial-local coordinates in width units, y growing upward from the
    // vial's bottom; the glow margin around it maps outside 0..extent.
    let vial_size = in.size - vec2<f32>(margin * 2.0);
    let vial_px = in.uv * in.size - vec2<f32>(margin);
    let aspect = vial_size.y / max(vial_size.x, 1.0);
    let p = vec2<f32>(vial_px.x / vial_size.x, (vial_size.y - vial_px.y) / vial_size.x);

    // The vial: a vertical capsule around the centerline.
    let bottom_cap = vec2<f32>(0.5, OUTER_RADIUS);
    let top_cap = vec2<f32>(0.5, aspect - OUTER_RADIUS);
    let along = clamp((p.y - bottom_cap.y) / (top_cap.y - bottom_cap.y), 0.0, 1.0);
    let axis = mix(bottom_cap, top_cap, along);
    let d = length(p - axis) - OUTER_RADIUS;

    let aa = max(fwidth(d), 1e-4);
    let outer = smoothstep(aa, -aa, d);
    let inner = smoothstep(aa, -aa, d + WALL);

    // The glass glow: a soft halo hugging the silhouette, breathing with
    // the beat. Tapered to exactly zero at the node boundary so the quad
    // edge can never clip a visible remainder into a rectangle.
    let margin_units = margin / max(vial_size.x, 1.0);
    let edge = clamp(1.0 - max(d, 0.0) / max(margin_units, 1e-4), 0.0, 1.0);
    let halo = exp(-max(d, 0.0) / 0.16) * glow_pulse * edge;
    if outer <= 0.0 {
        return vec4<f32>(GLASS_TINT * halo, halo * 0.45);
    }

    // The liquid surface: the eased level plus decaying wave chaos.
    let cavity_bottom = bottom_cap.y - (OUTER_RADIUS - WALL);
    let cavity_top = top_cap.y + (OUTER_RADIUS - WALL);
    let t = globals.time;
    let waves = sin(p.x * 9.4 + t * 7.0) * 0.5
        + sin(p.x * 15.7 - t * 9.3) * 0.3
        + sin(p.x * 23.0 + t * 12.1) * 0.2;
    let surface = mix(cavity_bottom, cavity_top, level) + waves * turbulence * 0.22;
    let liquid = inner * smoothstep(aa * 2.0, -aa * 2.0, p.y - surface);

    // Liquid body: the gradient spans the fluid itself — its bottom stop
    // at the cavity floor, its top stop at the liquid level — so it
    // compresses as health drains. The scroll offset cycles through the
    // mirrored (doubled) gradient so the loop never seams. Darkened
    // toward the walls for volume, with a bright meniscus line at the
    // surface.
    let liquid_top = mix(cavity_bottom, cavity_top, level);
    let height = (p.y - cavity_bottom) / max(liquid_top - cavity_bottom, 1e-4);
    let cycle = fract((height + scroll) * 0.5) * 2.0;
    let mirrored = 1.0 - abs(1.0 - cycle);
    let lateral = abs(p.x - 0.5) / OUTER_RADIUS;
    var liquid_color = gradient(mirrored) * (1.08 - 0.38 * lateral * lateral);
    let meniscus = exp(-max(surface - p.y, 0.0) * 26.0) * liquid;
    liquid_color += vec3<f32>(0.35) * meniscus;

    // Empty cavity: the faintest cool tint so the glass reads as hollow.
    let cavity_color = vec3<f32>(0.70, 0.82, 0.90);
    var color = mix(cavity_color, liquid_color, liquid);
    var alpha = mix(0.07, 0.90, liquid);

    // Glass wall: brightest at the silhouette where a real tube bunches
    // up its reflections, lifted further while the glow pulses.
    let wall_mask = clamp(outer - inner, 0.0, 1.0);
    let rim = 0.30 + 0.55 * pow(lateral, 3.0);
    color = mix(color, GLASS_TINT * (0.6 + rim + 0.35 * glow_pulse), wall_mask);
    alpha = mix(alpha, 0.28 + 0.5 * rim + 0.2 * glow_pulse, wall_mask);

    // Two vertical gloss streaks down the whole vial, glass and liquid.
    let gloss = exp(-pow((p.x - 0.31) * 9.0, 2.0)) * 0.30
        + exp(-pow((p.x - 0.72) * 16.0, 2.0)) * 0.12;
    color += vec3<f32>(gloss);
    alpha = min(alpha + gloss * 0.5, 1.0);

    return vec4<f32>(color, alpha * outer);
}
