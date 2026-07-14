shader_type canvas_item;

// Coverage 0 = clear, 1 = fully black. The animation drives this linearly; the
// sRGB exponent here restores the designed perceptual brightness of the fade,
// matching Screen.LinearBlend so the sweep reads as even to the eye.
uniform float coverage : hint_range(0.0, 1.0) = 0.0;

void fragment() {
	float alpha = 1.0 - pow(clamp(1.0 - coverage, 0.0, 1.0), 1.0 / 2.2);
	COLOR = vec4(0.0, 0.0, 0.0, alpha);
}
