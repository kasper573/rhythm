use crate::core::config::{GameConfig, HealthColorStop};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

/// The player's health as a glass vial of liquid pinned to the left edge
/// of the screen. The entire visual — glass, liquid, waves, gradient — is
/// one fragment shader; the systems here only feed it smoothed uniforms.
pub struct HealthVialPlugin;

impl Plugin for HealthVialPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiMaterialPlugin::<HealthVialMaterial>::default())
            .add_systems(Update, animate_vials);
    }
}

/// Set `fill` to the current health fraction; the liquid level eases after
/// it, any change stirs up waves that settle back to a flat surface, and
/// the gradient cross-fades between the configured presets.
#[derive(Component, Clone, Default)]
pub struct HealthVial {
    /// `0..=1` of the vial's capacity.
    pub fill: f32,
}

pub fn spawn_health_vial(
    commands: &mut Commands,
    materials: &mut Assets<HealthVialMaterial>,
    fill: f32,
) -> Entity {
    let material = materials.add(HealthVialMaterial {
        params: Vec4::ZERO,
        colors: [Vec4::ZERO; GRADIENT_SAMPLES],
    });
    // HealthVial rides the insert rather than the scene patch so the first
    // animation frame is guaranteed to see the real fill, not a default.
    commands
        .spawn_scene(bsn! {
            Node {
                position_type: PositionType::Absolute,
                left: px(50),
                top: percent(10),
                width: px(50),
                height: percent(80),
            }
        })
        .insert((
            HealthVial { fill },
            MaterialNode(material),
            VialMotion::default(),
        ))
        .id()
}

#[derive(AsBindGroup, Asset, TypePath, Clone)]
pub struct HealthVialMaterial {
    /// `(liquid level, turbulence, 0, 0)`.
    #[uniform(0)]
    params: Vec4,
    /// The active gradient sampled bottom-to-top at fixed positions.
    #[uniform(1)]
    colors: [Vec4; GRADIENT_SAMPLES],
}

impl UiMaterial for HealthVialMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/health_vial.wgsl".into()
    }
}

const GRADIENT_SAMPLES: usize = 16;
/// The liquid level's time constant toward the target fill.
const LEVEL_TAU: f32 = 0.25;
/// How long stirred-up waves take to settle back to a flat surface.
const TURBULENCE_TAU: f32 = 0.9;
/// The gradient cross-fade's time constant between presets.
const COLOR_TAU: f32 = 0.35;

/// Per-vial animation state, smoothed toward what [`HealthVial`] asks for.
#[derive(Component, Default)]
struct VialMotion {
    level: f32,
    turbulence: f32,
    last_fill: f32,
    colors: [Vec4; GRADIENT_SAMPLES],
    settled: bool,
}

fn animate_vials(
    time: Res<Time>,
    config: Res<GameConfig>,
    mut materials: ResMut<Assets<HealthVialMaterial>>,
    mut vials: Query<(
        &HealthVial,
        &mut VialMotion,
        &MaterialNode<HealthVialMaterial>,
    )>,
) {
    let delta = time.delta_secs();
    for (vial, mut motion, node) in &mut vials {
        let stops = &config.healthbar.gradient_at(vial.fill * 100.0).stops;
        let targets: Vec<Vec4> = (0..GRADIENT_SAMPLES)
            .map(|sample| {
                let at = sample as f32 / (GRADIENT_SAMPLES - 1) as f32;
                let color = LinearRgba::from(sample_stops(stops, at * 100.0));
                Vec4::new(color.red, color.green, color.blue, color.alpha)
            })
            .collect();

        if !motion.settled {
            motion.settled = true;
            motion.level = vial.fill;
            motion.last_fill = vial.fill;
            motion.colors.copy_from_slice(&targets);
        }

        let stirred = (vial.fill - motion.last_fill).abs();
        if stirred > 0.0 {
            motion.turbulence = (motion.turbulence + 0.35 + stirred * 5.0).min(1.2);
            motion.last_fill = vial.fill;
        }
        motion.turbulence *= (-delta / TURBULENCE_TAU).exp();

        let ease = 1.0 - (-delta / LEVEL_TAU).exp();
        motion.level += (vial.fill - motion.level) * ease;

        let blend = 1.0 - (-delta / COLOR_TAU).exp();
        for (color, target) in motion.colors.iter_mut().zip(&targets) {
            *color += (*target - *color) * blend;
        }

        if let Some(mut material) = materials.get_mut(&node.0) {
            material.params = Vec4::new(motion.level, motion.turbulence, 0.0, 0.0);
            material.colors = motion.colors;
        }
    }
}

/// Samples the stops at `percent` like a CSS gradient: flat beyond the
/// outermost stops, linear sRGB interpolation between adjacent ones.
fn sample_stops(stops: &[HealthColorStop], percent: f32) -> Srgba {
    let first = stops.first().expect("config validates stops are non-empty");
    if percent <= first.percent {
        return first.color.to_srgba();
    }
    for pair in stops.windows(2) {
        if percent <= pair[1].percent {
            let span = pair[1].percent - pair[0].percent;
            let t = (percent - pair[0].percent) / span;
            let a = pair[0].color.to_srgba();
            let b = pair[1].color.to_srgba();
            return Srgba::new(
                a.red + (b.red - a.red) * t,
                a.green + (b.green - a.green) * t,
                a.blue + (b.blue - a.blue) * t,
                a.alpha + (b.alpha - a.alpha) * t,
            );
        }
    }
    stops
        .last()
        .expect("config validates stops are non-empty")
        .color
        .to_srgba()
}
