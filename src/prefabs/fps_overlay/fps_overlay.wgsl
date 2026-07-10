// The FPS graph: a scrolling histogram of recent frame rates, each column a
// bar rising from the bottom to its sample's height. Samples arrive already
// normalized to the window's peak (oldest at index 0, newest at the right).

#import bevy_ui::ui_vertex_output::UiVertexOutput

@group(1) @binding(0) var<uniform> fg: vec4<f32>;
@group(1) @binding(1) var<uniform> bg: vec4<f32>;
@group(1) @binding(2) var<uniform> samples: array<vec4<f32>, 24u>;

const COLUMNS: f32 = 96.0;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let col = min(u32(floor(in.uv.x * COLUMNS)), 95u);
    let height = samples[col / 4u][col % 4u];
    let from_bottom = 1.0 - in.uv.y;
    if from_bottom <= height {
        return fg;
    }
    // A faint tint above each bar, so even idle columns read as a graph.
    return mix(bg, fg, 0.08);
}
