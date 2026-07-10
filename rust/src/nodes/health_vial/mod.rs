use crate::core::config::{HealthColorStop, config};
use crate::core::units::{Beat, Percent};
use godot::classes::control::LayoutPreset;
use godot::classes::{ColorRect, Control, IControl, Shader, ShaderMaterial};
use godot::prelude::*;

pub struct HealthVialOptions {
    /// Initial `0..=1` of the vial's capacity.
    pub fill: f32,
    pub side: VialSide,
    /// Padding between the screen edge and the vial (the stage's
    /// `screen_edge_padding`).
    pub edge_padding: f32,
}

/// Which screen edge a vial is pinned to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VialSide {
    Left,
    Right,
}

/// A glass vial of liquid pinned to a screen edge — a health bar. The
/// entire visual (glass, liquid, waves, gradient) is one fragment shader;
/// the node only feeds it smoothed uniforms, with the gradient presets and
/// pulse cycles coming from the config's `healthbar`.
///
/// The owner drives the ports every frame: the liquid level eases after
/// [`set_fill`](HealthVial::set_fill) (changes stir up waves that settle
/// back flat) and the glow and gradient scroll pulse on
/// [`set_beat`](HealthVial::set_beat).
#[derive(GodotClass)]
#[class(base=Control)]
pub struct HealthVial {
    fill: f32,
    beat: Beat,
    motion: VialMotion,
    material: Option<Gd<ShaderMaterial>>,
    shader_rect: Option<Gd<ColorRect>>,
    base: Base<Control>,
}

#[godot_api]
impl HealthVial {
    pub fn instantiate(opt: HealthVialOptions) -> Gd<HealthVial> {
        let mut vial = HealthVial::new_alloc();
        // The vial's spec rect: pinned to its side, 10%..90% of the screen
        // height. The shader rect hangs past it on every side so the glow
        // has room to breathe without moving the vial itself.
        let preset = match opt.side {
            VialSide::Left => LayoutPreset::LEFT_WIDE,
            VialSide::Right => LayoutPreset::RIGHT_WIDE,
        };
        vial.set_anchors_and_offsets_preset(preset);
        vial.set_anchor(godot::builtin::Side::TOP, 0.1);
        vial.set_anchor(godot::builtin::Side::BOTTOM, 0.9);
        let offset = match opt.side {
            VialSide::Left => (opt.edge_padding, opt.edge_padding + VIAL_WIDTH),
            VialSide::Right => (-opt.edge_padding - VIAL_WIDTH, -opt.edge_padding),
        };
        vial.set_offset(godot::builtin::Side::LEFT, offset.0);
        vial.set_offset(godot::builtin::Side::RIGHT, offset.1);
        vial.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);

        let mut shader = Shader::new_gd();
        shader.set_code(include_str!("health_vial.gdshader"));
        let mut material = ShaderMaterial::new_gd();
        material.set_shader(&shader);
        let mut rect = ColorRect::new_alloc();
        rect.set_material(&material);
        rect.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        rect.set_offset(godot::builtin::Side::LEFT, -GLOW_MARGIN);
        rect.set_offset(godot::builtin::Side::TOP, -GLOW_MARGIN);
        rect.set_offset(godot::builtin::Side::RIGHT, GLOW_MARGIN);
        rect.set_offset(godot::builtin::Side::BOTTOM, GLOW_MARGIN);
        rect.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
        vial.add_child(&rect);

        let mut bound = vial.bind_mut();
        bound.fill = opt.fill;
        bound.material = Some(material);
        bound.shader_rect = Some(rect);
        drop(bound);
        vial
    }

    /// `0..=1` of the vial's capacity.
    pub fn set_fill(&mut self, fill: f32) {
        self.fill = fill;
    }

    /// The musical beat the glow and liquid pulse on; hold it still and
    /// the vial rests.
    pub fn set_beat(&mut self, beat: Beat) {
        self.beat = beat;
    }
}

#[godot_api]
impl IControl for HealthVial {
    fn init(base: Base<Control>) -> HealthVial {
        HealthVial {
            fill: 0.0,
            beat: Beat(0.0),
            motion: VialMotion::default(),
            material: None,
            shader_rect: None,
            base,
        }
    }

    fn process(&mut self, delta: f64) {
        let delta = delta as f32;
        let config = config();
        let glow = config.healthbar.glow.pulse(self.beat);
        // One full trip through the mirrored gradient (two units) per cycle.
        let scroll = (config.healthbar.liquid.progress(self.beat) * 2.0).rem_euclid(2.0);
        let stops = &config
            .healthbar
            .gradient_at(Percent(self.fill * 100.0))
            .stops;
        let targets: Vec<Color> = (0..GRADIENT_SAMPLES)
            .map(|sample| {
                let at = sample as f32 / (GRADIENT_SAMPLES - 1) as f32;
                sample_stops(stops, Percent(at * 100.0))
            })
            .collect();

        let motion = &mut self.motion;
        if !motion.settled {
            motion.settled = true;
            motion.level = self.fill;
            motion.last_fill = self.fill;
            motion.colors.copy_from_slice(&targets);
        }

        let stirred = (self.fill - motion.last_fill).abs();
        if stirred > 0.0 {
            motion.turbulence = (motion.turbulence + 0.35 + stirred * 5.0).min(1.2);
            motion.last_fill = self.fill;
        }
        motion.turbulence *= (-delta / TURBULENCE_TAU).exp();

        let ease = 1.0 - (-delta / LEVEL_TAU).exp();
        motion.level += (self.fill - motion.level) * ease;

        let blend = 1.0 - (-delta / COLOR_TAU).exp();
        for (color, target) in motion.colors.iter_mut().zip(&targets) {
            *color = Color::from_rgba(
                color.r + (target.r - color.r) * blend,
                color.g + (target.g - color.g) * blend,
                color.b + (target.b - color.b) * blend,
                color.a + (target.a - color.a) * blend,
            );
        }

        let params = Vector4::new(motion.level, motion.turbulence, glow, scroll);
        let size = self
            .shader_rect
            .as_ref()
            .map(|rect| rect.get_size())
            .unwrap_or(Vector2::ONE);
        let colors = PackedColorArray::from(motion.colors.as_slice());
        if let Some(material) = &mut self.material {
            material.set_shader_parameter("params", &params.to_variant());
            material.set_shader_parameter("rect_size", &size.to_variant());
            material.set_shader_parameter("glow_margin", &GLOW_MARGIN.to_variant());
            material.set_shader_parameter("colors", &colors.to_variant());
        }
    }
}

const GRADIENT_SAMPLES: usize = 16;
/// Canvas pixels reserved around the vial for the pulsing glow; the shader
/// tapers the glow to exactly zero at this boundary.
const GLOW_MARGIN: f32 = 32.0;
const VIAL_WIDTH: f32 = 50.0;
/// The liquid level's time constant toward the target fill.
const LEVEL_TAU: f32 = 0.25;
/// How long stirred-up waves take to settle back to a flat surface.
const TURBULENCE_TAU: f32 = 0.9;
/// The gradient cross-fade's time constant between presets.
const COLOR_TAU: f32 = 0.35;

/// Per-vial animation state, smoothed toward what the ports ask for.
struct VialMotion {
    level: f32,
    turbulence: f32,
    last_fill: f32,
    colors: [Color; GRADIENT_SAMPLES],
    settled: bool,
}

impl Default for VialMotion {
    fn default() -> VialMotion {
        VialMotion {
            level: 0.0,
            turbulence: 0.0,
            last_fill: 0.0,
            colors: [Color::BLACK; GRADIENT_SAMPLES],
            settled: false,
        }
    }
}

/// Samples the stops at `percent` like a CSS gradient: flat beyond the
/// outermost stops, linear interpolation between adjacent ones.
fn sample_stops(stops: &[HealthColorStop], percent: Percent) -> Color {
    let first = stops.first().expect("config validates stops are non-empty");
    if percent <= first.percent {
        return first.color;
    }
    for pair in stops.windows(2) {
        if percent <= pair[1].percent {
            let span = pair[1].percent.0 - pair[0].percent.0;
            let t = (percent.0 - pair[0].percent.0) / span;
            let a = pair[0].color;
            let b = pair[1].color;
            return Color::from_rgba(
                a.r + (b.r - a.r) * t,
                a.g + (b.g - a.g) * t,
                a.b + (b.b - a.b) * t,
                a.a + (b.a - a.a) * t,
            );
        }
    }
    stops
        .last()
        .expect("config validates stops are non-empty")
        .color
}
