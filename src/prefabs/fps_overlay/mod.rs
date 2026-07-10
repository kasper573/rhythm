use crate::core::font::game_font;
use crate::core::input::{Actions, GameAction};
use bevy::asset::embedded_asset;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

pub struct FpsOverlayPrefabOptions {
    /// Bar and readout color.
    pub fg: Color,
    /// Panel and graph backdrop.
    pub bg: Color,
    /// Distance from the bottom-right screen corner.
    pub edge_padding: f32,
}

/// A frame-rate meter pinned to the bottom-right corner: the current FPS and
/// its observed range as text above a scrolling histogram of recent frames.
/// [`FpsOverlayPlugin`] drives every mounted overlay from the frame-time
/// diagnostics; the returned root carries [`FpsOverlay`] for the owner to
/// show and hide.
pub fn fps_overlay_prefab(
    opt: FpsOverlayPrefabOptions,
    commands: &mut Commands,
    materials: &mut Assets<FpsGraphMaterial>,
) -> Entity {
    let material = materials.add(FpsGraphMaterial {
        fg: to_vec4(opt.fg),
        bg: to_vec4(opt.bg),
        samples: [Vec4::ZERO; SAMPLE_VECS],
    });
    let root = commands
        .spawn_scene(bsn! {
            FpsOverlay
            Node {
                position_type: PositionType::Absolute,
                right: {Val::Px(opt.edge_padding)},
                bottom: {Val::Px(opt.edge_padding)},
                flex_direction: FlexDirection::Column,
                row_gap: px(3),
                padding: {UiRect::all(Val::Px(4.0))},
            }
            BackgroundColor({opt.bg})
            GlobalZIndex(500)
            Children [(
                FpsReadout
                game_font(READOUT_SIZE)
                Text("")
                TextColor({opt.fg})
            )]
        })
        .insert(FpsHistory::default())
        .id();
    commands
        .spawn_scene(bsn! {
            Node { width: px(GRAPH_WIDTH), height: px(GRAPH_HEIGHT) }
        })
        .insert((MaterialNode(material), ChildOf(root)));
    root
}

/// The overlay root's marker: the owner drives its [`Visibility`] to show or
/// hide the whole meter.
#[derive(Component, Default, Clone)]
pub struct FpsOverlay;

pub struct FpsOverlayPlugin;

impl Plugin for FpsOverlayPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "fps_overlay.wgsl");
        app.add_plugins((
            FrameTimeDiagnosticsPlugin::default(),
            UiMaterialPlugin::<FpsGraphMaterial>::default(),
        ))
        .add_systems(Startup, mount)
        .add_systems(Update, (drive, toggle));
    }
}

#[derive(AsBindGroup, Asset, TypePath, Clone)]
pub struct FpsGraphMaterial {
    #[uniform(0)]
    fg: Vec4,
    #[uniform(1)]
    bg: Vec4,
    /// Recent frames oldest-to-newest, normalized `0..=1` to the window's
    /// peak; four columns to a vector.
    #[uniform(2)]
    samples: [Vec4; SAMPLE_VECS],
}

impl UiMaterial for FpsGraphMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://rhythm/prefabs/fps_overlay/fps_overlay.wgsl".into()
    }
}

/// The game's single, hidden-by-default overlay in the corner.
fn mount(mut commands: Commands, mut materials: ResMut<Assets<FpsGraphMaterial>>) {
    let root = fps_overlay_prefab(
        FpsOverlayPrefabOptions {
            fg: Color::srgb(0.0, 1.0, 1.0),
            bg: Color::srgb(0.0, 0.0, 0.13),
            edge_padding: 12.0,
        },
        &mut commands,
        &mut materials,
    );
    commands.entity(root).insert(Visibility::Hidden);
}

fn toggle(actions: Actions, mut overlays: Query<&mut Visibility, With<FpsOverlay>>) {
    if !actions.just_pressed(GameAction::ToggleFps) {
        return;
    }
    for mut visibility in &mut overlays {
        *visibility = match *visibility {
            Visibility::Hidden => Visibility::Visible,
            _ => Visibility::Hidden,
        };
    }
}

fn drive(
    diagnostics: Res<DiagnosticsStore>,
    mut materials: ResMut<Assets<FpsGraphMaterial>>,
    mut panels: Query<(&mut FpsHistory, &Children)>,
    graphs: Query<&MaterialNode<FpsGraphMaterial>>,
    mut readouts: Query<&mut Text, With<FpsReadout>>,
) {
    let Some(fps) = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|fps| fps.smoothed())
    else {
        return;
    };
    let fps = fps as f32;
    for (mut history, children) in &mut panels {
        history.push(fps);
        let samples = history.normalized();
        let (low, high) = history.range().unwrap_or((fps, fps));
        for child in children.iter() {
            if let Ok(node) = graphs.get(child)
                && let Some(mut material) = materials.get_mut(&node.0)
            {
                material.samples = samples;
            }
            if let Ok(mut text) = readouts.get_mut(child) {
                text.0 = format!("{fps:.0} FPS ({low:.0}-{high:.0})");
            }
        }
    }
}

/// The readout text above the graph.
#[derive(Component, Default, Clone)]
struct FpsReadout;

const READOUT_SIZE: f32 = 13.0;
const GRAPH_WIDTH: f32 = 120.0;
const GRAPH_HEIGHT: f32 = 34.0;
const COLUMNS: usize = 96;
const SAMPLE_VECS: usize = COLUMNS / 4;

/// A ring of the most recent per-frame FPS readings, feeding both the graph's
/// bars and the readout's min/max.
#[derive(Component)]
struct FpsHistory {
    ring: [f32; COLUMNS],
    next: usize,
}

impl Default for FpsHistory {
    fn default() -> FpsHistory {
        FpsHistory {
            ring: [0.0; COLUMNS],
            next: 0,
        }
    }
}

impl FpsHistory {
    fn push(&mut self, fps: f32) {
        self.ring[self.next] = fps;
        self.next = (self.next + 1) % COLUMNS;
    }

    /// The samples oldest-to-newest, each normalized to the window's peak so
    /// the tallest recent frame fills the graph.
    fn normalized(&self) -> [Vec4; SAMPLE_VECS] {
        let peak = self.ring.iter().copied().fold(0.0, f32::max).max(1.0);
        let mut samples = [Vec4::ZERO; SAMPLE_VECS];
        for column in 0..COLUMNS {
            let value = self.ring[(self.next + column) % COLUMNS] / peak;
            samples[column / 4][column % 4] = value.clamp(0.0, 1.0);
        }
        samples
    }

    /// The `(min, max)` over the frames observed so far, or `None` before the
    /// first frame.
    fn range(&self) -> Option<(f32, f32)> {
        let mut observed = self.ring.iter().copied().filter(|fps| *fps > 0.0);
        let first = observed.next()?;
        Some(observed.fold((first, first), |(low, high), fps| {
            (low.min(fps), high.max(fps))
        }))
    }
}

fn to_vec4(color: Color) -> Vec4 {
    let color = color.to_linear();
    Vec4::new(color.red, color.green, color.blue, color.alpha)
}
