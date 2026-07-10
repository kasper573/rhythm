// The FPS graph: a scrolling histogram of recent frame rates, each column a
// bar rising from the bottom to its sample's height. Samples arrive already
// normalized to the window's peak (oldest at index 0, newest at the right).
shader_type canvas_item;

uniform vec4 fg;
uniform vec4 bg;
uniform float samples[96];

void fragment() {
    int column = min(int(floor(UV.x * 96.0)), 95);
    float height = samples[column];
    float from_bottom = 1.0 - UV.y;
    if (from_bottom <= height) {
        COLOR = fg;
    } else {
        // A faint tint above each bar, so even idle columns read as a graph.
        COLOR = mix(bg, fg, 0.08);
    }
}
