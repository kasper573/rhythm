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
use crate::core::scene_flow::SpawnScoped;
use crate::core::settings::{GradeLayer, PlayerSettings};
use crate::core::units::Seconds;
use crate::scenes::GameScene;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, RenderTarget, ScalingMode};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, TextureFormat};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d};

/// The word's font height, in world units.
const FONT_SIZE: f32 = 50.0;
/// World size of the presented quad; the offscreen camera frames exactly
/// this, wide enough to hold the longest timing-feedback string.
const PRESENT_W: f32 = 760.0;
const PRESENT_H: f32 = 170.0;
/// The offscreen image is rendered larger than presented so the word stays
/// crisp when the window scales the quad up.
const SUPERSAMPLE: u32 = 2;
const IMAGE_W: u32 = PRESENT_W as u32 * SUPERSAMPLE;
const IMAGE_H: u32 = PRESENT_H as u32 * SUPERSAMPLE;
/// The soft glow's reach around the glyphs, in world units.
const HALO_RADIUS: f32 = 11.0;
const HALO_STRENGTH: f32 = 0.85;
/// Full glow shimmers per second.
const FLASH_HZ: f32 = 2.5;
/// The pop each grade opens with: a brief upscale that settles.
const BOUNCE_SECONDS: f32 = 0.13;
const BOUNCE_AMOUNT: f32 = 0.18;
/// Seconds a grade takes to fade out once the player stops hitting.
const FADE_SECONDS: f32 = 1.0;
const GRADE_Z: f32 = 6.0;
/// Private render layers the offscreen word cameras draw, one per player,
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

/// Spawns one player's grade-text rig: the live word rendered white to an
/// offscreen image, plus the shader quad presenting it. `layer` picks
/// whether that quad draws behind the arrows or on the overlay in front.
pub(super) fn spawn(
    commands: &mut Commands,
    asset_server: &AssetServer,
    images: &mut Assets<Image>,
    player: PlayerId,
    origin_x: f32,
    layer: GradeLayer,
) {
    let source_layer = RenderLayers::layer(SOURCE_LAYER_BASE + player_index(player));
    // Added straight to `Assets` (not deferred through the asset server) so
    // the offscreen camera's target resolves the very frame it spawns.
    let image = images.add(Image::new_target_texture(
        IMAGE_W,
        IMAGE_H,
        TextureFormat::Rgba8UnormSrgb,
        None,
    ));

    commands
        .spawn_scoped(GameScene::FilePlayer, bsn! { Camera2d })
        .insert((
            Camera {
                order: SOURCE_CAMERA_ORDER - player_index(player) as isize,
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
            source_layer.clone(),
        ));

    let source = commands
        .spawn_scoped(
            GameScene::FilePlayer,
            bsn! {
                GradeSource
                game_font(FONT_SIZE)
                Text2d("")
                TextColor(Color::WHITE)
            },
        )
        .insert(source_layer)
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

    let mut quad = commands.spawn((
        GradeText {
            player,
            source,
            intensity: 0.0,
            pulse: 0.0,
            bounce: 0.0,
            base: Vec3::ONE,
            glow: Vec3::ONE,
            strength: 0.0,
        },
        ForPlayer(player),
        Mesh2d(mesh),
        MeshMaterial2d(material),
        Transform::from_xyz(origin_x, 0.0, GRADE_Z),
        DespawnOnExit(GameScene::FilePlayer),
    ));
    if layer == GradeLayer::InFront {
        quad.insert(RenderLayers::layer(OVERLAY_LAYER));
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
    /// Linear grade color, linear glow color, and glow strength.
    base: Vec3,
    glow: Vec3,
    strength: f32,
}

#[derive(AsBindGroup, Asset, TypePath, Clone)]
struct GradeTextMaterial {
    /// `(linear grade color, fade)`.
    #[uniform(0)]
    base: Vec4,
    /// `(linear glow color, strength times the pulse)`.
    #[uniform(1)]
    glow: Vec4,
    /// `(halo radius u, halo radius v, halo strength, unused)`.
    #[uniform(2)]
    shape: Vec4,
    #[texture(3)]
    #[sampler(4)]
    text: Handle<Image>,
}

impl Material2d for GradeTextMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/grade_text.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
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
                text.0 = style.text.clone();
            }
            grade.base = linear(style.base);
            grade.glow = linear(style.glow);
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

        // The glow rises to full the instant the grade lands (cos starts at
        // 1) and shimmers from there.
        let pulse = 0.5 + 0.5 * (std::f32::consts::TAU * FLASH_HZ * grade.pulse).cos();
        if let Some(mut material) = materials.get_mut(&material.0) {
            material.base = grade.base.extend(grade.intensity);
            material.glow = grade.glow.extend(grade.strength * pulse);
        }

        let bounce = EaseFunction::CubicOut.sample_clamped(grade.bounce);
        transform.scale = Vec3::splat(1.0 + BOUNCE_AMOUNT * bounce);
        if let Some(height) = visible_height {
            let percent = settings[grade.player].grade_position.0 / 100.0;
            transform.translation.y = height * (0.5 - percent);
        }
    }
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

/// The word, base color, glow color, and glow strength one outcome shows.
struct GradeStyle {
    text: String,
    base: Color,
    glow: Color,
    strength: f32,
}

fn grade_style(config: &GameConfig, outcome: RowOutcome) -> GradeStyle {
    match outcome {
        RowOutcome::Hit { error } => {
            let Grade::Hit(grade) = config.grade(outcome) else {
                unreachable!("hits always grade into a timed grade");
            };
            let def = &config.grading.dynamic[grade.0];
            GradeStyle {
                text: hit_text(def, error),
                base: def.color,
                glow: def.glow.color,
                strength: def.glow.strength,
            }
        }
        RowOutcome::Miss => {
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
