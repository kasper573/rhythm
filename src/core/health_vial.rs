use crate::core::config::{GameConfig, HealthColorStop};
use crate::core::note_field::NoteFieldClock;
use crate::core::units::Percent;
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
            .add_systems(Update, (clamp_vial_width, animate_vials));
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

/// The vial's layout rect (the shader node hangs off it by the glow
/// margin); its width is clamped in real screen pixels.
#[derive(Component, Clone, Default)]
struct HealthVialFrame;

/// UI values scale with the window, but the vial must stay a readable
/// sliver: its on-screen width is clamped by counter-scaling the node.
fn clamp_vial_width(ui_scale: Res<UiScale>, mut frames: Query<&mut Node, With<HealthVialFrame>>) {
    if ui_scale.0 <= 0.0 {
        return;
    }
    let on_screen = (VIAL_WIDTH * ui_scale.0).clamp(VIAL_MIN_WIDTH, VIAL_MAX_WIDTH);
    let width = Val::Px(on_screen / ui_scale.0);
    for mut node in &mut frames {
        if node.width != width {
            node.width = width;
        }
    }
}

/// Which screen edge a vial is pinned to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VialSide {
    Left,
    Right,
}

/// `edge_padding` is the configured screen-edge padding (see
/// `StageConfig::screen_edge_padding`).
pub fn spawn_health_vial(
    commands: &mut Commands,
    materials: &mut Assets<HealthVialMaterial>,
    fill: f32,
    side: VialSide,
    edge_padding: f32,
) -> Entity {
    let material = materials.add(HealthVialMaterial {
        params: Vec4::ZERO,
        geometry: Vec4::new(GLOW_MARGIN, 0.0, 0.0, 0.0),
        colors: [Vec4::ZERO; GRADIENT_SAMPLES],
    });
    let (left, right) = match side {
        VialSide::Left => (Val::Px(edge_padding), Val::Auto),
        VialSide::Right => (Val::Auto, Val::Px(edge_padding)),
    };
    // The outer node is the vial's spec rect and carries the [`HealthVial`]
    // the owner drives — on the returned entity, so callers can attach
    // their own markers next to it. The shader node hangs past it on every
    // side so the glow has room to breathe without moving the vial itself.
    let vial = commands
        .spawn_scene(bsn! {
            HealthVialFrame
            Node {
                position_type: PositionType::Absolute,
                left: {left},
                right: {right},
                top: percent(10),
                width: px(VIAL_WIDTH),
                height: percent(80),
            }
        })
        // HealthVial rides the insert rather than the scene patch so the
        // first animation frame is guaranteed to see the real fill, not a
        // default.
        .insert(HealthVial { fill })
        .id();
    commands
        .spawn_scene(bsn! {
            Node {
                position_type: PositionType::Absolute,
                left: px(-GLOW_MARGIN),
                right: px(-GLOW_MARGIN),
                top: px(-GLOW_MARGIN),
                bottom: px(-GLOW_MARGIN),
            }
        })
        .insert((MaterialNode(material), VialMotion::default(), ChildOf(vial)));
    vial
}

#[derive(AsBindGroup, Asset, TypePath, Clone)]
pub struct HealthVialMaterial {
    /// `(liquid level, turbulence, glow pulse, gradient scroll)`.
    #[uniform(0)]
    params: Vec4,
    /// `(glow margin in physical pixels, 0, 0, 0)`; kept in the node's
    /// own units by [`animate_vials`] whatever the UI scale.
    #[uniform(1)]
    geometry: Vec4,
    /// The active gradient sampled bottom-to-top at fixed positions.
    #[uniform(2)]
    colors: [Vec4; GRADIENT_SAMPLES],
}

impl UiMaterial for HealthVialMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/health_vial.wgsl".into()
    }
}

const GRADIENT_SAMPLES: usize = 16;
/// Node pixels reserved around the vial for the pulsing glow; the shader
/// tapers the glow to exactly zero at this boundary.
const GLOW_MARGIN: f32 = 32.0;
const VIAL_WIDTH: f32 = 50.0;
/// On-screen bounds of the vial's width, whatever the UI scale.
const VIAL_MIN_WIDTH: f32 = 20.0;
const VIAL_MAX_WIDTH: f32 = 50.0;
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
    clock: Option<Res<NoteFieldClock>>,
    mut materials: ResMut<Assets<HealthVialMaterial>>,
    vials: Query<&HealthVial>,
    mut shader_nodes: Query<(
        &mut VialMotion,
        &MaterialNode<HealthVialMaterial>,
        &ComputedNode,
        &ChildOf,
    )>,
) {
    let delta = time.delta_secs();
    // The same beat the arrows animate on, so every pulse shares one clock.
    let beat = clock.map(|clock| clock.beat()).unwrap_or(0.0);
    let glow = config.healthbar.glow.pulse(beat);
    // One full trip through the mirrored gradient (two units) per cycle.
    let scroll = (config.healthbar.liquid.progress(beat) * 2.0).rem_euclid(2.0);
    for (mut motion, node, computed, child_of) in &mut shader_nodes {
        let Ok(vial) = vials.get(child_of.parent()) else {
            continue;
        };
        let stops = &config
            .healthbar
            .gradient_at(Percent(vial.fill * 100.0))
            .stops;
        let targets: Vec<Vec4> = (0..GRADIENT_SAMPLES)
            .map(|sample| {
                let at = sample as f32 / (GRADIENT_SAMPLES - 1) as f32;
                let color = LinearRgba::from(sample_stops(stops, Percent(at * 100.0)));
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
            material.params = Vec4::new(motion.level, motion.turbulence, glow, scroll);
            // The shader works in the node's physical pixels; the margin is
            // authored in logical ui pixels.
            material.geometry.x = GLOW_MARGIN / computed.inverse_scale_factor;
            material.colors = motion.colors;
        }
    }
}

/// Samples the stops at `percent` like a CSS gradient: flat beyond the
/// outermost stops, linear sRGB interpolation between adjacent ones.
fn sample_stops(stops: &[HealthColorStop], percent: Percent) -> Srgba {
    let first = stops.first().expect("config validates stops are non-empty");
    if percent <= first.percent {
        return first.color.to_srgba();
    }
    for pair in stops.windows(2) {
        if percent <= pair[1].percent {
            let span = pair[1].percent.0 - pair[0].percent.0;
            let t = (percent.0 - pair[0].percent.0) / span;
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
