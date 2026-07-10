// The grade text's neon glow. The word is rendered live to the sprite's
// texture as white glyphs, so its alpha channel is pure coverage; every
// color comes from the uniforms, letting one shader drive every grade.
//
// Bright letters that bloom: the word is a bright core with an additive
// colored bloom laid over everything — over the letters themselves (so they
// glow luminous, not flat) and spreading past them into a soft halo. Output
// is PREMULTIPLIED (blend_premul_alpha) so the bloom adds light on top of
// any background, reading the same on black and on the dark playfield. Its
// strength pulses each hit (see grade_text.rs): striking bright, then
// draining.
shader_type canvas_item;
render_mode blend_premul_alpha;

// rgb: letter core color (white for hits), a: fade (1 on a fresh grade → 0).
uniform vec4 base_color;
// rgb: glow color, a: strength times the pulse.
uniform vec4 glow_color;
// (bloom radius u, bloom radius v, bloom brightness, unused).
uniform vec4 shape;

// Points spread over a disc by the golden angle (phyllotaxis), radius by
// sqrt for even density — a smooth blur with no ringing. Enough of them
// that the wide glow stays smooth instead of lumpy.
const int TAPS = 64;
const float GOLDEN_ANGLE = 2.399963;

void fragment() {
    vec2 uv = UV;
    float core = texture(TEXTURE, uv).a;

    // A soft bloom of the glyph coverage, brightest over the strokes and
    // fading outward past their edges.
    vec2 radius = shape.xy;
    float bloom = 0.0;
    float weight = 0.0;
    for (int i = 0; i < TAPS; i++) {
        float t = (float(i) + 0.5) / float(TAPS);
        float angle = float(i) * GOLDEN_ANGLE;
        float w = 1.0 - t;
        bloom += texture(TEXTURE, uv + vec2(cos(angle), sin(angle)) * sqrt(t) * radius).a * w;
        weight += w;
    }
    bloom /= max(weight, 1e-4);

    float amount = glow_color.a;
    float fade = base_color.a;

    // Bright white letter core, opaque; plus the additive colored bloom,
    // held off the solid cores (`1 - core`) so the letters stay white-hot
    // while the color blooms out from their edges into the halo.
    vec3 core_pm = base_color.rgb * core;
    vec3 glow_pm = glow_color.rgb * (bloom * amount * shape.z) * (1.0 - core);
    vec3 rgb_pm = core_pm + glow_pm;
    COLOR = vec4(rgb_pm * fade, core * fade);
}
