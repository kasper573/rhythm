//! Renders every grade text to a PNG so the grade shader can be tuned
//! without playing. Each grade gets a row, shown at rest (left, no glow)
//! and at peak glow (right); it reuses the real [`grade_text`] rig and
//! shader so what it shows is exactly what the game draws.
//!
//! ```text
//! cargo run --bin render_grade
//! ```

use bevy::app::SubApps;
use bevy::camera::{RenderTarget, ScalingMode};
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::render_resource::{PollType, TextureFormat, TextureUsages};
use bevy::render::renderer::RenderDevice;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::sprite_render::MeshMaterial2d;
use bevy::time::TimeUpdateStrategy;
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use rhythm::core::CLEAR_COLOR;
use rhythm::core::config::{GameConfig, RowOutcome};
use rhythm::core::font::game_font;
use rhythm::core::units::Seconds;
use rhythm::prefabs::stepfile_player::grade_text::{
    self, GradeTextMaterial, GradeTextPlugin, spawn_rig,
};
use std::path::PathBuf;
use std::time::Duration;

const WIDTH: u32 = 900;
const HEIGHT: u32 = 760;
/// The two columns' word centers.
const COLUMN_X: [f32; 2] = [-210.0, 210.0];
/// Private layers for the offscreen word cameras, clear of the capture
/// camera's layer 0.
const LAYER_BASE: usize = 20;

/// One material's target look, painted every frame so it survives the
/// asset server's deferred material insertion.
#[derive(Component)]
struct Target {
    base: Color,
    glow: Color,
    strength: f32,
    pulse: f32,
}

fn main() {
    rhythm::core::platform::install(rhythm::native::NativePlatform);
    let out = PathBuf::from("out");
    std::fs::create_dir_all(&out).expect("failed to create the output directory");
    let path = out.join("grades.png");

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            .set(RenderPlugin {
                synchronous_pipeline_compilation: true,
                ..default()
            })
            .disable::<WinitPlugin>(),
    )
    .add_plugins(GradeTextPlugin)
    .insert_resource(ClearColor(CLEAR_COLOR))
    .insert_resource(GameConfig::load())
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
        1.0 / 60.0,
    )))
    .add_systems(Update, paint);
    app.finish();
    app.cleanup();
    let mut apps = std::mem::take(app.sub_apps_mut());
    let world = apps.main.world_mut();

    let mut target = Image::new_target_texture(WIDTH, HEIGHT, TextureFormat::Rgba8UnormSrgb, None);
    target.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    let target = world.resource_mut::<Assets<Image>>().add(target);
    world
        .spawn_scene(bsn! { Camera2d })
        .expect("static scene resolution cannot fail")
        .insert((
            RenderTarget::Image(target.clone().into()),
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::Fixed {
                    width: WIDTH as f32,
                    height: HEIGHT as f32,
                },
                ..OrthographicProjection::default_2d()
            }),
        ));
    // Two backgrounds so the glow can be judged for equal visibility on
    // black and on a lighter playfield-like gray, not just pure black.
    for (column, bg) in [Color::BLACK, Color::srgb(0.30, 0.31, 0.35)]
        .into_iter()
        .enumerate()
    {
        let half = WIDTH as f32 / 4.0;
        world.spawn((
            Sprite {
                color: bg,
                custom_size: Some(Vec2::new(WIDTH as f32 / 2.0, HEIGHT as f32)),
                ..default()
            },
            Transform::from_xyz(half * (2.0 * column as f32 - 1.0), 0.0, -1.0),
        ));
    }
    for (column, label) in ["on black", "on gray"].into_iter().enumerate() {
        world
            .spawn_scene(bsn! {
                game_font(24.0)
                Text2d({label.to_string()})
                TextColor(Color::srgb(0.6, 0.6, 0.6))
            })
            .expect("static scene resolution cannot fail")
            .insert(Transform::from_xyz(
                COLUMN_X[column],
                HEIGHT as f32 / 2.0 - 26.0,
                1.0,
            ));
    }

    let config = world.resource::<GameConfig>().clone();
    let outcomes = grade_outcomes(&config);
    let row_gap = (HEIGHT as f32 - 90.0) / outcomes.len() as f32;
    let top = (outcomes.len() as f32 - 1.0) / 2.0 * row_gap;
    let asset_server = world.resource::<AssetServer>().clone();
    world.resource_scope(|world, mut images: Mut<Assets<Image>>| {
        let mut commands = world.commands();
        for (row, outcome) in outcomes.iter().enumerate() {
            let style = grade_text::grade_style(&config, *outcome);
            let y = top - row as f32 * row_gap;
            for (column, pulse) in [1.0f32, 1.0].into_iter().enumerate() {
                let layer = LAYER_BASE + row * COLUMN_X.len() + column;
                let rig = spawn_rig(&mut commands, &mut images, &asset_server, layer);
                commands.entity(rig.source).insert(Text2d::new(&style.text));
                commands.entity(rig.present).insert((
                    Transform::from_xyz(COLUMN_X[column], y, 6.0),
                    Target {
                        base: style.base,
                        glow: style.glow,
                        strength: style.strength,
                        pulse,
                    },
                ));
            }
        }
    });
    world.flush();

    // Let the font load and the offscreen words render before capturing.
    for _ in 0..120 {
        update(&mut apps);
    }
    apps.main
        .world_mut()
        .spawn(Screenshot::image(target.clone()))
        .observe(save_to_disk(path.clone()));
    for _ in 0..300 {
        update(&mut apps);
        if path.exists() {
            break;
        }
    }
    println!("wrote {}", path.display());
}

/// A representative outcome for each dynamic grade — the midpoint of its
/// timing window, so it grades to exactly that tier — followed by a miss.
fn grade_outcomes(config: &GameConfig) -> Vec<RowOutcome> {
    let mut outcomes = Vec::new();
    let mut lower = 0.0;
    for grade in &config.grading.dynamic {
        let mid = (lower + grade.window_ms) / 2.0;
        outcomes.push(RowOutcome::Hit {
            error: Seconds::from_millis(mid),
        });
        lower = grade.window_ms;
    }
    outcomes.push(RowOutcome::Miss);
    outcomes
}

fn paint(
    mut materials: ResMut<Assets<GradeTextMaterial>>,
    targets: Query<(&Target, &MeshMaterial2d<GradeTextMaterial>)>,
) {
    for (target, material) in &targets {
        if let Some(mut material) = materials.get_mut(&material.0) {
            grade_text::apply_style(
                &mut material,
                target.base,
                target.glow,
                target.strength,
                1.0,
                target.pulse,
            );
        }
    }
}

fn update(apps: &mut SubApps) {
    apps.update();
    apps.main
        .world()
        .resource::<RenderDevice>()
        .wgpu_device()
        .poll(PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .expect("gpu poll failed");
}
