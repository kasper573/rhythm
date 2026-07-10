use crate::core::library::StepfileEntry;
use crate::core::stepfile::StepfileTiming;
use crate::core::units::Seconds;
use crate::prefabs::media_cover::{
    MediaCover, MediaCoverPrefabOptions, MediaPace, media_cover_prefab,
};
use crate::prefabs::stepfile_player::{PlaySet, PlayTime};
use crate::scenes::GameScene;
use bevy::prelude::*;
use std::path::PathBuf;

/// The play stage's backgrounds: the stepfile's `#BGCHANGES` timeline of
/// media covers, cued on the musical timeline, cross-faded, and paced by
/// the session's visible clock so videos stay locked to the music.
pub(super) fn plugin(app: &mut App) {
    app.add_message::<BackgroundCue>()
        .add_systems(OnExit(GameScene::Play), exit)
        .add_systems(
            Update,
            (
                cue_background_changes,
                apply_background_cues,
                fade_background_layers,
                pace_background_covers,
            )
                .chain()
                .in_set(PlaySet::Present)
                .run_if(in_state(GameScene::Play)),
        );
}

fn exit(mut commands: Commands) {
    commands.remove_resource::<BackgroundTimeline>();
}

/// How long a `CrossFade` transition blends between backgrounds.
const CROSSFADE_SECONDS: f32 = 0.5;

/// Dimmed so arrows and text stay readable in front of the background.
const DIM: f32 = 0.5;

/// Background switches from the stepfile's `#BGCHANGES`, resolved to files
/// that actually exist, ordered by time.
#[derive(Resource)]
struct BackgroundTimeline {
    /// The stepfile's own background, shown before any timed change.
    initial: Option<PathBuf>,
    changes: Vec<BackgroundChange>,
    next: usize,
}

struct BackgroundChange {
    time: Seconds,
    path: PathBuf,
    crossfade: bool,
    loops: bool,
}

/// One background cover on screen, easing toward its target opacity;
/// fully faded-out layers retire.
#[derive(Component)]
struct BackgroundLayer {
    target: f32,
}

pub(super) fn spawn_background(
    commands: &mut Commands,
    entry: &StepfileEntry,
    timing: &StepfileTiming,
) {
    let mut changes: Vec<BackgroundChange> = entry
        .stepfile
        .bg_changes
        .iter()
        .filter_map(|change| {
            let path = entry.resolve_file(&change.file)?;
            Some(BackgroundChange {
                time: timing.seconds_at_beat(change.beat),
                path,
                crossfade: change.crossfade,
                loops: change.loops,
            })
        })
        .collect();
    changes.sort_by(|a, b| a.time.0.total_cmp(&b.time.0));
    commands.insert_resource(BackgroundTimeline {
        initial: entry.background_path(),
        changes,
        next: 0,
    });
}

/// A background change whose time has come, recognized on the musical
/// timeline and applied separately.
#[derive(Message)]
struct BackgroundCue {
    time: Seconds,
    path: PathBuf,
    crossfade: bool,
    loops: bool,
}

fn cue_background_changes(
    play_time: Res<PlayTime>,
    mut timeline: ResMut<BackgroundTimeline>,
    mut cues: MessageWriter<BackgroundCue>,
) {
    if let Some(path) = timeline.initial.take() {
        cues.write(BackgroundCue {
            time: Seconds::ZERO,
            path,
            crossfade: false,
            loops: false,
        });
    }
    let now = play_time.visible;
    while timeline.next < timeline.changes.len() && timeline.changes[timeline.next].time.0 <= now.0
    {
        let change = &timeline.changes[timeline.next];
        cues.write(BackgroundCue {
            time: change.time,
            path: change.path.clone(),
            crossfade: change.crossfade,
            loops: change.loops,
        });
        timeline.next += 1;
    }
}

fn apply_background_cues(
    mut cues: MessageReader<BackgroundCue>,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
    mut layers: Query<(Entity, &mut BackgroundLayer)>,
    mut layer_count: Local<u32>,
) {
    for cue in cues.read() {
        // Newer layers draw above the ones fading out; the small cycling
        // bump stays below the note field.
        let z = 0.3 + ((*layer_count + 1) % 100) as f32 * 0.002;
        let alpha = if cue.crossfade { 0.0 } else { 1.0 };
        let cover = media_cover_prefab(
            MediaCoverPrefabOptions {
                path: cue.path.clone(),
                color: Color::srgba(DIM, DIM, DIM, alpha),
                z,
                start: cue.time,
                looping: cue.loops,
                pace: MediaPace::Manual,
            },
            &mut commands,
            &asset_server,
            &mut images,
        );
        // An unshowable cue keeps the current background instead.
        let Some(cover) = cover else { continue };
        *layer_count += 1;

        for (entity, mut layer) in &mut layers {
            if cue.crossfade {
                layer.target = 0.0;
            } else {
                commands.entity(entity).despawn();
            }
        }
        commands.entity(cover).insert((
            BackgroundLayer { target: 1.0 },
            DespawnOnExit(GameScene::Play),
        ));
    }
}

/// Runs every layer's timed linear blend and retires the faded-out ones.
fn fade_background_layers(
    time: Res<Time>,
    mut layers: Query<(Entity, &BackgroundLayer, &mut Sprite)>,
    mut commands: Commands,
) {
    let step = time.delta_secs() / CROSSFADE_SECONDS;
    for (entity, layer, mut sprite) in &mut layers {
        let alpha = sprite.color.alpha();
        let next = if layer.target > alpha {
            (alpha + step).min(layer.target)
        } else {
            (alpha - step).max(layer.target)
        };
        if next != alpha {
            sprite.color.set_alpha(next);
        }
        if layer.target <= 0.0 && next <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

/// Locks every background video to the session's visible timeline.
fn pace_background_covers(
    play_time: Res<PlayTime>,
    mut covers: Query<&mut MediaCover, With<BackgroundLayer>>,
) {
    for mut cover in &mut covers {
        cover.clock = play_time.visible;
    }
}
