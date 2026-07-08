use crate::core::assets::asset_server_path;
use crate::core::settings::TimingSettings;
use crate::core::stepfile::{Stepfile, StepfileClock};
use crate::core::units::{Beat, Seconds};
use bevy::audio::{AudioSinkPlayback, PlaybackMode};
use bevy::prelude::*;
use std::path::PathBuf;
use std::time::Duration;

/// Music a scene can hand to the [`MusicPlayer`]: always a real stepfile,
/// so whatever plays can synchronize UI through its timing.
pub struct Bgm {
    /// The .sm path — the player's identity for "already playing this".
    pub sm_path: PathBuf,
    pub stepfile: Stepfile,
    pub music: Option<PathBuf>,
}

/// The global background-music state machine: at most one stepfile plays
/// at a time, looping its sample window until switched or stopped. A
/// [`play`](MusicPlayer::play) naming the stepfile already playing is
/// ignored, so scenes and wheel rows resolving to the same music keep it
/// running uninterrupted. Scene changes never stop it by themselves —
/// only an explicit [`stop`](MusicPlayer::stop) does.
#[derive(Resource, Default)]
pub struct MusicPlayer {
    playing: Option<PlayingBgm>,
}

struct PlayingBgm {
    bgm: Bgm,
    entity: Option<Entity>,
    clock: Option<StepfileClock>,
}

impl MusicPlayer {
    pub fn play(&mut self, commands: &mut Commands, bgm: Bgm) {
        if let Some(playing) = &self.playing
            && playing.bgm.sm_path == bgm.sm_path
        {
            return;
        }
        self.stop(commands);
        self.playing = Some(PlayingBgm {
            bgm,
            entity: None,
            clock: None,
        });
    }

    pub fn stop(&mut self, commands: &mut Commands) {
        if let Some(playing) = self.playing.take()
            && let Some(entity) = playing.entity
        {
            commands.entity(entity).try_despawn();
        }
    }

    /// The beat the speakers are on, for UI synchronized to the music;
    /// `None` while nothing audible plays.
    pub fn visible_beat(&self, settings: &TimingSettings) -> Option<Beat> {
        let playing = self.playing.as_ref()?;
        Some(playing.clock.as_ref()?.visible_beat(settings))
    }
}

pub struct MusicPlayerPlugin;

impl Plugin for MusicPlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MusicPlayer>()
            .add_systems(Update, drive_music_player);
    }
}

/// Spawns the audio for the current stepfile once — unscoped, so it
/// survives scene changes — and keeps the shared clock servo'd onto its
/// sink. The mixer's raw position keeps growing while the sample loops,
/// so it is folded back into the loop window first; the servo's resync
/// snap absorbs the seam.
fn drive_music_player(
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    sinks: Query<&AudioSink>,
    mut player: ResMut<MusicPlayer>,
    mut commands: Commands,
) {
    let Some(playing) = &mut player.playing else {
        return;
    };
    let PlayingBgm { bgm, entity, clock } = playing;

    if entity.is_none() {
        let Some(path) = bgm.music.as_deref().and_then(asset_server_path) else {
            return;
        };
        let start = bgm.stepfile.sample_start.0.max(0.0);
        let length = bgm.stepfile.sample_length.0;
        let music = asset_server.load(path);
        *entity = Some(
            commands
                .spawn_scene(bsn! {
                    AudioPlayer({music})
                    PlaybackSettings {
                        mode: {PlaybackMode::Loop},
                        start_position: {Some(Duration::from_secs_f64(start))},
                        duration: {(length > 0.0).then(|| Duration::from_secs_f64(length))},
                    }
                })
                .id(),
        );
        return;
    }

    let Some(sink) = entity.and_then(|entity| sinks.get(entity).ok()) else {
        return;
    };
    let report = bgm
        .stepfile
        .sample_position(Seconds(sink.position().as_secs_f64()));
    let clock =
        clock.get_or_insert_with(|| StepfileClock::start_at(bgm.stepfile.timing.clone(), report));
    clock.advance(Seconds(time.delta_secs_f64()), Some(report));
}
