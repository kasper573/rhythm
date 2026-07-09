use crate::core::assets::asset_server_path;
use crate::core::audio::Sound;
use crate::core::platform::{AudioChannel, SoundOptions, platform};
use crate::core::settings::{MachineSettings, TimingSettings};
use crate::core::stepfile::{Stepfile, StepfileClock, StepfileTiming};
use crate::core::units::{Beat, Seconds};
use bevy::prelude::*;
use std::path::PathBuf;

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
    source: Option<Handle<Sound>>,
    channel: Option<Box<dyn AudioChannel>>,
    clock: Option<StepfileClock>,
}

impl MusicPlayer {
    pub fn play(&mut self, bgm: Bgm) {
        if let Some(playing) = &self.playing
            && playing.bgm.sm_path == bgm.sm_path
        {
            return;
        }
        self.playing = Some(PlayingBgm {
            bgm,
            source: None,
            channel: None,
            clock: None,
        });
    }

    pub fn stop(&mut self) {
        self.playing = None;
    }

    /// The beat the speakers are on, for UI synchronized to the music;
    /// `None` while nothing audible plays.
    pub fn visible_beat(&self, settings: &TimingSettings) -> Option<Beat> {
        let playing = self.playing.as_ref()?;
        Some(playing.clock.as_ref()?.visible_beat(settings))
    }

    /// The visible moment on the playing stepfile's timeline together with
    /// its timing, for visuals that animate on the music's own clock.
    pub fn visible_now(&self, settings: &TimingSettings) -> Option<(Seconds, &StepfileTiming)> {
        let playing = self.playing.as_ref()?;
        let clock = playing.clock.as_ref()?;
        Some((clock.visible_now(settings), &playing.bgm.stepfile.timing))
    }
}

pub struct MusicPlayerPlugin;

impl Plugin for MusicPlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MusicPlayer>()
            .add_systems(Update, drive_music_player);
    }
}

/// Opens the audio for the current stepfile once its bytes load — looping
/// the preview sample window — and keeps the shared clock servo'd onto
/// the channel's position reports and the volume on the music setting.
fn drive_music_player(
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    sounds: Res<Assets<Sound>>,
    settings: Res<MachineSettings>,
    mut player: ResMut<MusicPlayer>,
) {
    let Some(playing) = &mut player.playing else {
        return;
    };
    let PlayingBgm {
        bgm,
        source,
        channel,
        clock,
    } = playing;

    let Some(active) = channel else {
        let Some(path) = bgm.music.as_deref().and_then(asset_server_path) else {
            return;
        };
        let handle = source.get_or_insert_with(|| asset_server.load(path));
        let Some(sound) = sounds.get(handle.id()) else {
            return;
        };
        let options = SoundOptions {
            window: Some((bgm.stepfile.sample_start, bgm.stepfile.sample_length)),
            volume: settings.volume.music_gain(),
            ..default()
        };
        match platform().open_audio(sound.bytes.clone(), options) {
            Ok(opened) => *channel = Some(opened),
            Err(error) => {
                warn!("music cannot play: {error}");
                bgm.music = None;
            }
        }
        return;
    };

    if settings.is_changed() {
        active.set_volume(settings.volume.music_gain());
    }
    if !active.is_ready() {
        return;
    }
    if active.is_finished() {
        // The file ran out (it had no loopable window); start it over.
        *channel = None;
        return;
    }
    let report = active.position();
    let clock =
        clock.get_or_insert_with(|| StepfileClock::start_at(bgm.stepfile.timing.clone(), report));
    clock.advance(Seconds(time.delta_secs_f64()), Some(report));
}
