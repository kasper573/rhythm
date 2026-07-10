// The grade text's neon glow. The word is rendered live to `text` as white
// glyphs, so its alpha channel is pure coverage; every color comes from the
// uniforms, letting one shader drive every grade.
//
// Bright letters that bloom: the word is a bright core with an additive
// colored bloom laid over everything — over the letters themselves (so they
// glow luminous, not flat) and spreading past them into a soft halo. Output
// is PREMULTIPLIED (the material sets premultiplied-alpha blending) so the
// bloom adds light on top of any background, reading the same on black and
// on the dark playfield. Its strength pulses each hit (see grade_text.rs):
// striking bright, then draining.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput

// rgb: letter core color (white for hits), a: fade (1 on a fresh grade → 0).
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> base: vec4<f32>;
// rgb: glow color, a: strength times the pulse.
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> glow: vec4<f32>;
// (bloom radius u, bloom radius v, bloom brightness, unused).
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> shape: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var text: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var text_sampler: sampler;

// Points spread over a disc by the golden angle (phyllotaxis), radius by
// sqrt for even density — a smooth blur with no ringing. Enough of them
// that the wide glow stays smooth instead of lumpy.
const TAPS: u32 = 64u;
const GOLDEN_ANGLE: f32 = 2.399963;

fn coverage(uv: vec2<f32>) -> f32 {
    return textureSample(text, text_sampler, uv).a;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let core = coverage(uv);

    // A soft bloom of the glyph coverage, brightest over the strokes and
    // fading outward past their edges.
    let radius = shape.xy;
    var bloom = 0.0;
    var weight = 0.0;
    for (var i = 0u; i < TAPS; i = i + 1u) {
        let t = (f32(i) + 0.5) / f32(TAPS);
        let angle = f32(i) * GOLDEN_ANGLE;
        let w = 1.0 - t;
        bloom += coverage(uv + vec2<f32>(cos(angle), sin(angle)) * sqrt(t) * radius) * w;
        weight += w;
    }
    bloom /= max(weight, 1e-4);

    let amount = glow.a;
    let fade = base.a;

    // Bright white letter core, opaque; plus the additive colored bloom,
    // held off the solid cores (`1 - core`) so the letters stay white-hot
    // while the color blooms out from their edges into the halo.
    let core_pm = base.rgb * core;
    let glow_pm = glow.rgb * (bloom * amount * shape.z) * (1.0 - core);
    let rgb_pm = core_pm + glow_pm;
    return vec4<f32>(rgb_pm * fade, core * fade);
}
