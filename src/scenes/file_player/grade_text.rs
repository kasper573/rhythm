//! The judgment word that pops on each graded row, shaded live: the word
//! is rendered white to an offscreen image so its alpha is pure coverage,
//! then presented on a quad whose material tints it to the grade color and
//! layers on an additive glow that pulses — one shader, per-grade colors
//! and strengths (see [`grade_text.wgsl`](../../../assets/shaders/grade_text.wgsl)).

use super::{ForPlayer, PlaySet, RowGraded};
use crate::core::OVERLAY_LAYER;
use crate::core::config::{DynamicGradeDef, GameConfig, Grade, RowOutcome, TimingFeedback};
use crate::core::font::game_font;
use crate::core::note_field::visible_world_size;
use crate::core::player::PlayerId;
use crate::core::settings::{GradeLayer, PlayerSettings};
use crate::core::units::{Percent, Seconds};
use crate::scenes::GameScene;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, RenderTarget, ScalingMode};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, BlendState, RenderPipelineDescriptor, SpecializedMeshPipelineError, TextureFormat,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{
    AlphaMode2d, Material2d, Material2dKey, Material2dPlugin, MeshMaterial2d,
};

/// The word's font height, in world units.
const FONT_SIZE: f32 = 50.0;
/// World size of the presented quad; the offscreen camera frames exactly
/// this, wide enough to hold the longest timing-feedback string.
pub const PRESENT_W: f32 = 760.0;
pub const PRESENT_H: f32 = 170.0;
/// The offscreen image is rendered larger than presented so the word stays
/// crisp when the window scales the quad up.
const SUPERSAMPLE: u32 = 2;
const IMAGE_W: u32 = PRESENT_W as u32 * SUPERSAMPLE;
const IMAGE_H: u32 = PRESENT_H as u32 * SUPERSAMPLE;
/// The neon glow's reach around the glyphs, in world units, and its
/// brightness.
const HALO_RADIUS: f32 = 18.0;
const HALO_STRENGTH: f32 = 1.6;
/// The glow strikes to full the instant a grade lands, then drains toward
/// [`GLOW_FLOOR`] with this time constant — a per-hit pulse, not a steady
/// shine, in the spirit of the wheel and health-bar pulses.
const PULSE_TAU: f32 = 0.3;
const GLOW_FLOOR: f32 = 0.32;
/// The grade group's vertical extent, keeping the word and the combo under
/// it inside the screen's padded band.
const GRADE_HALF_HEIGHT: f32 = 36.0;
const COMBO_HALF_HEIGHT: f32 = 24.0;
/// The combo readout sits this far under the grade's center.
pub const COMBO_GAP: f32 = 62.0;
/// The pop each grade opens with: a brief upscale that settles.
const BOUNCE_SECONDS: f32 = 0.13;
const BOUNCE_AMOUNT: f32 = 0.18;
/// Seconds a grade takes to fade out once the player stops hitting.
const FADE_SECONDS: f32 = 1.0;
const GRADE_Z: f32 = 6.0;
/// Private render layers the offscreen word cameras draw, one per rig,
/// clear of the lane and overlay layers.
const SOURCE_LAYER_BASE: usize = 20;
/// The offscreen cameras render to images, not the window, so their order
/// only needs to be distinct and out of the window cameras' way.
const SOURCE_CAMERA_ORDER: isize = -100;

pub(super) fn plugin(app: &mut App) {
    app.add_plugins(Material2dPlugin::<GradeTextMaterial>::default())
        .add_systems(
            Update,
            (apply_grades, animate_grades)
                .chain()
                .in_set(PlaySet::Present),
        );
}

/// The entities of one grade-text rig, for callers to position, re-layer,
/// and scope to their own context.
pub struct GradeRig {
    /// The shader quad presenting the word.
    pub present: Entity,
    /// The offscreen white word; set its `Text2d` to change the grade.
    pub source: Entity,
    /// The offscreen camera rendering `source` into the sampled image.
    pub camera: Entity,
    pub material: Handle<GradeTextMaterial>,
}

/// Builds a grade-text rig drawing on the private `source_layer`: the
/// offscreen white word, its camera and image, and the shader quad
/// presenting it at the origin on the default layer. Callers position,
/// re-layer, and scope the returned entities. Shared by the play stage,
/// the options preview, and the `render_grade` inspector.
pub fn spawn_rig(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    asset_server: &AssetServer,
    source_layer: usize,
) -> GradeRig {
    let layer = RenderLayers::layer(source_layer);
    // Added straight to `Assets` (not deferred through the asset server) so
    // the offscreen camera's target resolves the very frame it spawns.
    let image = images.add(Image::new_target_texture(
        IMAGE_W,
        IMAGE_H,
        TextureFormat::Rgba8UnormSrgb,
        None,
    ));
    let camera = commands
        .spawn_scene(bsn! { Camera2d })
        .insert((
            Camera {
                order: SOURCE_CAMERA_ORDER - source_layer as isize,
                clear_color: ClearColorConfig::Custom(Color::NONE),
                ..default()
            },
            RenderTarget::Image(image.clone().into()),
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::Fixed {
                    width: PRESENT_W,
                    height: PRESENT_H,
                },
                ..OrthographicProjection::default_2d()
            }),
            layer.clone(),
        ))
        .id();
    let source = commands
        .spawn_scene(bsn! {
            GradeSource
            game_font(FONT_SIZE)
            Text2d("")
            TextColor(Color::WHITE)
        })
        .insert(layer)
        .id();
    let material = asset_server.add(GradeTextMaterial {
        base: Vec4::new(1.0, 1.0, 1.0, 0.0),
        glow: Vec4::ZERO,
        shape: Vec4::new(
            HALO_RADIUS / PRESENT_W,
            HALO_RADIUS / PRESENT_H,
            HALO_STRENGTH,
            0.0,
        ),
        text: image,
    });
    let mesh = asset_server.add(Mesh::from(Rectangle::new(PRESENT_W, PRESENT_H)));
    let present = commands
        .spawn((
            Mesh2d(mesh),
            MeshMaterial2d(material.clone()),
            Transform::from_xyz(0.0, 0.0, GRADE_Z),
        ))
        .id();
    GradeRig {
        present,
        source,
        camera,
        material,
    }
}

/// Spawns one player's grade-text rig on the play stage, positioned over
/// the field and layered behind or in front of the arrows per `layer`.
pub(super) fn spawn(
    commands: &mut Commands,
    asset_server: &AssetServer,
    images: &mut Assets<Image>,
    player: PlayerId,
    origin_x: f32,
    layer: GradeLayer,
) {
    let rig = spawn_rig(
        commands,
        images,
        asset_server,
        SOURCE_LAYER_BASE + player_index(player),
    );
    for entity in [rig.camera, rig.source] {
        commands
            .entity(entity)
            .insert(DespawnOnExit(GameScene::FilePlayer));
    }
    let mut present = commands.entity(rig.present);
    present.insert((
        GradeText {
            player,
            source: rig.source,
            intensity: 0.0,
            pulse: 0.0,
            bounce: 0.0,
            base: Color::WHITE,
            glow: Color::WHITE,
            strength: 0.0,
        },
        ForPlayer(player),
        Transform::from_xyz(origin_x, 0.0, GRADE_Z),
        DespawnOnExit(GameScene::FilePlayer),
    ));
    if layer == GradeLayer::InFront {
        present.insert(RenderLayers::layer(OVERLAY_LAYER));
    }
}

/// The offscreen word entity, rendered white so the shader owns its color.
#[derive(Component, Default, Clone)]
struct GradeSource;

/// The presented shader quad and its running animation state.
#[derive(Component)]
struct GradeText {
    player: PlayerId,
    source: Entity,
    /// Fade level: 1 on a fresh grade, decaying to 0 once hits stop.
    intensity: f32,
    /// Seconds since the last grade, driving the glow's oscillation.
    pulse: f32,
    /// The pop-in bounce, 1 on a fresh grade decaying to 0.
    bounce: f32,
    /// The current grade's color, glow color, and glow strength.
    base: Color,
    glow: Color,
    strength: f32,
}

#[derive(AsBindGroup, Asset, TypePath, Clone)]
pub struct GradeTextMaterial {
    /// `(linear grade color, fade)`.
    #[uniform(0)]
    pub base: Vec4,
    /// `(linear glow color, strength times the pulse)`.
    #[uniform(1)]
    pub glow: Vec4,
    /// `(halo radius u, halo radius v, halo strength, unused)`.
    #[uniform(2)]
    pub shape: Vec4,
    #[texture(3)]
    #[sampler(4)]
    pub text: Handle<Image>,
}

impl Material2d for GradeTextMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/grade_text.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    /// The shader outputs premultiplied color, so the glow adds light on top
    /// of any background instead of blending toward it.
    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        if let Some(fragment) = &mut descriptor.fragment {
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING);
            }
        }
        Ok(())
    }
}

/// Each graded row refreshes its player's word, color, and glow, and
/// restarts the pop and fade.
fn apply_grades(
    config: Res<GameConfig>,
    mut graded: MessageReader<RowGraded>,
    mut grades: Query<&mut GradeText>,
    mut sources: Query<&mut Text2d, With<GradeSource>>,
) {
    for message in graded.read() {
        for mut grade in &mut grades {
            if grade.player != message.player {
                continue;
            }
            let style = grade_style(&config, message.outcome);
            if let Ok(mut text) = sources.get_mut(grade.source) {
                text.0 = style.text;
            }
            grade.base = style.base;
            grade.glow = style.glow;
            grade.strength = style.strength;
            grade.intensity = 1.0;
            grade.pulse = 0.0;
            grade.bounce = 1.0;
        }
    }
}

/// Advances every word's fade, glow pulse, and pop, keeps it at the
/// player's configured height, and feeds the shader its uniforms.
fn animate_grades(
    time: Res<Time>,
    config: Res<GameConfig>,
    settings: Res<PlayerSettings>,
    windows: Query<&Window>,
    mut materials: ResMut<Assets<GradeTextMaterial>>,
    mut grades: Query<(
        &mut GradeText,
        &MeshMaterial2d<GradeTextMaterial>,
        &mut Transform,
    )>,
) {
    let delta = time.delta_secs();
    let visible_height = windows
        .single()
        .map(|window| visible_world_size(window).y)
        .ok();
    for (mut grade, material, mut transform) in &mut grades {
        if grade.intensity <= 0.0 {
            continue;
        }
        grade.intensity = (grade.intensity - delta / FADE_SECONDS).max(0.0);
        grade.pulse += delta;
        grade.bounce = (grade.bounce - delta / BOUNCE_SECONDS).max(0.0);

        if let Some(mut material) = materials.get_mut(&material.0) {
            apply_style(
                &mut material,
                grade.base,
                grade.glow,
                grade.strength,
                grade.intensity,
                glow_pulse(grade.pulse),
            );
        }

        let bounce = EaseFunction::CubicOut.sample_clamped(grade.bounce);
        transform.scale = Vec3::splat(1.0 + BOUNCE_AMOUNT * bounce);
        if let Some(height) = visible_height {
            let padding = config.stage.screen_edge_padding;
            transform.translation.y =
                grade_y(height, padding, settings[grade.player].grade_position);
        }
    }
}

/// The grade word's world Y for a player's grade-position percentage,
/// keeping the whole grade group — the word and the combo tracking under
/// it — inside the screen's padded band. 0% pins the word's top to the top
/// padding; 100% pins the combo's bottom to the bottom padding.
pub fn grade_y(visible_height: f32, padding: f32, grade_position: Percent) -> f32 {
    let top = visible_height / 2.0 - padding - GRADE_HALF_HEIGHT;
    let bottom = -visible_height / 2.0 + padding + COMBO_GAP + COMBO_HALF_HEIGHT;
    let t = (grade_position.0 / 100.0).clamp(0.0, 1.0);
    top + (bottom - top) * t
}

fn player_index(player: PlayerId) -> usize {
    match player {
        PlayerId::P1 => 0,
        PlayerId::P2 => 1,
    }
}

fn linear(color: Color) -> Vec3 {
    let c = color.to_linear();
    Vec3::new(c.red, c.green, c.blue)
}

/// Packs a grade's colors into the shader uniforms at a given fade and glow
/// pulse — the one place the color/glow packing lives, shared by the live
/// animation and the `render_grade` inspector.
pub fn apply_style(
    material: &mut GradeTextMaterial,
    base: Color,
    glow: Color,
    strength: f32,
    intensity: f32,
    pulse: f32,
) {
    material.base = linear(base).extend(intensity);
    material.glow = linear(glow).extend(strength * pulse);
}

/// The glow strength at a moment since the grade landed: full at the strike
/// (`seconds` 0), draining toward [`GLOW_FLOOR`].
pub fn glow_pulse(seconds: f32) -> f32 {
    GLOW_FLOOR + (1.0 - GLOW_FLOOR) * (-seconds / PULSE_TAU).exp()
}

/// The glow's drained resting level, so the word stays lit between strikes.
pub const GLOW_FLOOR_LEVEL: f32 = GLOW_FLOOR;

/// The word, base color, glow color, and glow strength one outcome shows.
pub struct GradeStyle {
    pub text: String,
    pub base: Color,
    pub glow: Color,
    pub strength: f32,
}

pub fn grade_style(config: &GameConfig, outcome: RowOutcome) -> GradeStyle {
    match outcome {
        RowOutcome::Hit { error } => {
            let Grade::Hit(grade) = config.grade(outcome) else {
                unreachable!("hits always grade into a timed grade");
            };
            let def = &config.grading.dynamic[grade.0];
            // Like ITG: the letters are white, the grade's color is the glow.
            GradeStyle {
                text: hit_text(def, error),
                base: Color::WHITE,
                glow: def.glow.color,
                strength: def.glow.strength,
            }
        }
        RowOutcome::Miss => {
            // ITG's Miss is the exception — its letters carry the red.
            let miss = &config.grading.fixed.miss;
            GradeStyle {
                text: miss.name.clone(),
                base: miss.color,
                glow: miss.glow.color,
                strength: miss.glow.strength,
            }
        }
    }
}

/// The word for a hit, marking the side of the perfect moment the input
/// fell on: early feedback leads the name, late feedback trails it.
fn hit_text(def: &DynamicGradeDef, error: Seconds) -> String {
    let name = &def.name;
    let early = error.0 > 0.0;
    // Displayed offset is input-relative: negative = early, positive = late.
    let offset_ms = (-error.to_millis()).round() as i64;
    match def.timing_feedback {
        TimingFeedback::Off => name.clone(),
        TimingFeedback::Sign if early => format!("-{name}"),
        TimingFeedback::Sign => format!("{name}-"),
        TimingFeedback::Millis if early => format!("({offset_ms}ms) {name}"),
        TimingFeedback::Millis => format!("{name} (+{offset_ms}ms)"),
    }
}
