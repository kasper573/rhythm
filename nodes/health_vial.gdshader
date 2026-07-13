// The health vial: a glass capsule holding a liquid whose level, surface
// waves, and bottom-to-top color gradient are driven by uniforms. The
// liquid is one coherent body attached to the walls; only its surface
// misbehaves, and only while turbulence (params.y) is nonzero. The rect
// is larger than the vial by a glow margin on every side, where the
// beat-pulsed glass glow (params.z) falls off.
shader_type canvas_item;

// (liquid level 0..1, turbulence 0..~1.2, glow pulse 0..1, gradient scroll 0..2)
uniform vec4 params;
uniform vec2 rect_size;
uniform float glow_margin;
// The active gradient, sampled bottom-to-top at 16 even positions.
uniform vec4 colors[16];

const float OUTER_RADIUS = 0.46;
const float WALL = 0.07;
const vec3 GLASS_TINT = vec3(0.75, 0.85, 0.92);

vec3 gradient(float height) {
    float x = clamp(height, 0.0, 1.0) * 15.0;
    int index = int(floor(x));
    vec4 a = colors[min(index, 15)];
    vec4 b = colors[min(index + 1, 15)];
    return mix(a, b, fract(x)).rgb;
}

void fragment() {
    float level = params.x;
    float turbulence = params.y;
    float glow_pulse = params.z;
    float scroll = params.w;
    float margin = glow_margin;

    // Vial-local coordinates in width units, y growing upward from the
    // vial's bottom; the glow margin around it maps outside 0..extent.
    vec2 vial_size = rect_size - vec2(margin * 2.0);
    vec2 vial_px = UV * rect_size - vec2(margin);
    float aspect = vial_size.y / max(vial_size.x, 1.0);
    vec2 p = vec2(vial_px.x / vial_size.x, (vial_size.y - vial_px.y) / vial_size.x);

    // The vial: a vertical capsule around the centerline.
    vec2 bottom_cap = vec2(0.5, OUTER_RADIUS);
    vec2 top_cap = vec2(0.5, aspect - OUTER_RADIUS);
    float along = clamp((p.y - bottom_cap.y) / (top_cap.y - bottom_cap.y), 0.0, 1.0);
    vec2 axis = mix(bottom_cap, top_cap, along);
    float d = length(p - axis) - OUTER_RADIUS;

    float aa = max(fwidth(d), 1e-4);
    float outer = smoothstep(aa, -aa, d);
    float inner = smoothstep(aa, -aa, d + WALL);

    // The glass glow: a soft halo hugging the silhouette, breathing with
    // the beat. Tapered to exactly zero at the rect boundary so the quad
    // edge can never clip a visible remainder into a rectangle.
    float margin_units = margin / max(vial_size.x, 1.0);
    float edge = clamp(1.0 - max(d, 0.0) / max(margin_units, 1e-4), 0.0, 1.0);
    float halo = exp(-max(d, 0.0) / 0.16) * glow_pulse * edge;
    if (outer <= 0.0) {
        COLOR = vec4(GLASS_TINT * halo, halo * 0.45);
    } else {
        // The liquid surface: the eased level plus decaying wave chaos.
        float cavity_bottom = bottom_cap.y - (OUTER_RADIUS - WALL);
        float cavity_top = top_cap.y + (OUTER_RADIUS - WALL);
        float t = TIME;
        float waves = sin(p.x * 9.4 + t * 7.0) * 0.5
            + sin(p.x * 15.7 - t * 9.3) * 0.3
            + sin(p.x * 23.0 + t * 12.1) * 0.2;
        float surface = mix(cavity_bottom, cavity_top, level) + waves * turbulence * 0.22;
        float liquid = inner * smoothstep(aa * 2.0, -aa * 2.0, p.y - surface);

        // Liquid body: the gradient spans the fluid itself — its bottom stop
        // at the cavity floor, its top stop at the liquid level — so it
        // compresses as health drains. The scroll offset cycles through the
        // mirrored (doubled) gradient so the loop never seams. Darkened
        // toward the walls for volume, with a bright meniscus line at the
        // surface.
        float liquid_top = mix(cavity_bottom, cavity_top, level);
        float height = (p.y - cavity_bottom) / max(liquid_top - cavity_bottom, 1e-4);
        float cycle = fract((height + scroll) * 0.5) * 2.0;
        float mirrored = 1.0 - abs(1.0 - cycle);
        float lateral = abs(p.x - 0.5) / OUTER_RADIUS;
        vec3 liquid_color = gradient(mirrored) * (1.08 - 0.38 * lateral * lateral);
        float meniscus = exp(-max(surface - p.y, 0.0) * 26.0) * liquid;
        liquid_color += vec3(0.35) * meniscus;

        // Empty cavity: the faintest cool tint so the glass reads as hollow.
        vec3 cavity_color = vec3(0.70, 0.82, 0.90);
        vec3 color = mix(cavity_color, liquid_color, liquid);
        float alpha = mix(0.07, 0.90, liquid);

        // Glass wall: brightest at the silhouette where a real tube bunches
        // up its reflections, lifted further while the glow pulses.
        float wall_mask = clamp(outer - inner, 0.0, 1.0);
        float rim = 0.30 + 0.55 * pow(lateral, 3.0);
        color = mix(color, GLASS_TINT * (0.6 + rim + 0.35 * glow_pulse), wall_mask);
        alpha = mix(alpha, 0.28 + 0.5 * rim + 0.2 * glow_pulse, wall_mask);

        // Two vertical gloss streaks down the whole vial, glass and liquid.
        float gloss = exp(-pow((p.x - 0.31) * 9.0, 2.0)) * 0.30
            + exp(-pow((p.x - 0.72) * 16.0, 2.0)) * 0.12;
        color += vec3(gloss);
        alpha = min(alpha + gloss * 0.5, 1.0);

        COLOR = vec4(color, alpha * outer);
    }
}
