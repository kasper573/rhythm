// The grade text's shimmer. The word is rendered live to `text` as white
// glyphs, so its alpha channel is pure coverage; every color comes from the
// uniforms, letting one shader drive every grade. The glyph is tinted to
// its grade color and brightened by an additive glow that pulses over time,
// and its coverage is spread into a soft halo the glow color bleeds into.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput

// rgb: grade color, a: fade (1 on a fresh grade, decaying to 0).
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> base: vec4<f32>;
// rgb: glow color, a: pulsed strength (grade strength times the oscillation).
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> glow: vec4<f32>;
// (halo radius u, halo radius v, halo strength, unused).
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> shape: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var text: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var text_sampler: sampler;

fn coverage(uv: vec2<f32>) -> f32 {
    return textureSample(text, text_sampler, uv).a;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let core = coverage(uv);

    // A ring of taps around the pixel spreads the glyph's coverage outward.
    let ru = shape.x;
    let rv = shape.y;
    let diag = 0.7071;
    var halo = coverage(uv + vec2<f32>(ru, 0.0));
    halo += coverage(uv + vec2<f32>(-ru, 0.0));
    halo += coverage(uv + vec2<f32>(0.0, rv));
    halo += coverage(uv + vec2<f32>(0.0, -rv));
    halo += coverage(uv + vec2<f32>(ru, rv) * diag);
    halo += coverage(uv + vec2<f32>(ru, -rv) * diag);
    halo += coverage(uv + vec2<f32>(-ru, rv) * diag);
    halo += coverage(uv + vec2<f32>(-ru, -rv) * diag);
    halo = halo / 8.0;

    let amount = glow.a;
    let halo_alpha = clamp(halo * amount * shape.z, 0.0, 1.0);

    // Premultiplied contributions: the glyph brightened by the glow, plus
    // the surrounding halo in the glow color.
    let glyph_rgb = base.rgb + glow.rgb * amount;
    let glyph_pm = glyph_rgb * core;
    let halo_pm = glow.rgb * halo_alpha;

    let cov = max(core, halo_alpha);
    if cov <= 0.0 {
        return vec4<f32>(0.0);
    }
    // Back to straight alpha for the standard alpha blend; the fade rides
    // the output alpha only.
    let rgb = (glyph_pm + halo_pm) / cov;
    return vec4<f32>(rgb, cov * base.a);
}
