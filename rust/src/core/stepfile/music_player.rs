use crate::core::audio::{MUSIC_BUS, SoundChannel, SoundOptions};
use crate::core::platform::{AssetFetch, FetchPoll, platform};
use crate::core::settings::TimingSettings;
use crate::core::stepfile::{Stepfile, StepfileClock, StepfileTiming};
use crate::core::units::{Beat, Seconds};
use godot::classes::Engine;
use godot::prelude::*;
use std::path::PathBuf;

/// Music a scene can hand to the [`MusicPlayer`]: always a real stepfile,
/// so whatever plays can synchronize UI through its timing.
pub struct Bgm {
    /// The .sm path — the player's identity for "already playing this".
    pub sm_path: PathBuf,
    pub stepfile: Stepfile,
    pub music: Option<PathBuf>,
}

/// The global background-music singleton: at most one stepfile plays
/// at a time, looping its sample window until switched or stopped. A
/// [`play`](MusicPlayer::play) naming the stepfile already playing is
/// ignored, so scenes and wheel rows resolving to the same music keep it
/// running uninterrupted. Scene changes never stop it by themselves —
/// only an explicit [`stop`](MusicPlayer::stop) does.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MusicPlayer {
    playing: Option<PlayingBgm>,
    base: Base<Node>,
}

struct PlayingBgm {
    bgm: Bgm,
    fetch: Option<Box<dyn AssetFetch>>,
    channel: Option<SoundChannel>,
    clock: Option<StepfileClock>,
}

#[godot_api]
impl MusicPlayer {
    pub fn singleton() -> Gd<MusicPlayer> {
        Engine::singleton()
            .get_singleton("MusicPlayer")
            .expect("MusicPlayer singleton is registered at boot")
            .cast()
    }

    pub fn play(&mut self, bgm: Bgm) {
        if let Some(playing) = &self.playing
            && playing.bgm.sm_path == bgm.sm_path
        {
            return;
        }
        self.playing = Some(PlayingBgm {
            bgm,
            fetch: None,
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

    /// The looping sample window a preview can lay a chart over: the
    /// stepfile's timing and the `[start, start+length)` window the music
    /// loops. `None` until the clock exists (the music is playing), and for
    /// music that plays through instead of looping.
    pub fn loop_window(&self) -> Option<(StepfileTiming, Seconds, Seconds)> {
        let playing = self.playing.as_ref()?;
        playing.clock.as_ref()?;
        let stepfile = &playing.bgm.stepfile;
        let crate::core::audio::SoundTimeline::LoopWindow { start, length } =
            stepfile.sample_timeline()
        else {
            return None;
        };
        Some((stepfile.timing.clone(), start, length))
    }
}

#[godot_api]
impl INode for MusicPlayer {
    fn init(base: Base<Node>) -> MusicPlayer {
        MusicPlayer {
            playing: None,
            base,
        }
    }

    /// Opens the audio for the current stepfile once its bytes arrive —
    /// looping the preview sample window — and keeps the shared clock
    /// servo'd onto the channel's position reports.
    fn process(&mut self, delta: f64) {
        let mut parent = self.base().clone().upcast::<Node>();
        let Some(playing) = &mut self.playing else {
            return;
        };
        let PlayingBgm {
            bgm,
            fetch,
            channel,
            clock,
        } = playing;

        let Some(active) = channel else {
            let Some(path) = bgm.music.clone() else {
                return;
            };
            let poll = fetch
                .get_or_insert_with(|| platform().fetch_asset(&path))
                .poll();
            match poll {
                FetchPoll::Pending => {}
                FetchPoll::Failed(error) => {
                    godot_warn!("music failed to load: {}: {error}", path.display());
                    bgm.music = None;
                    *fetch = None;
                }
                FetchPoll::Ready(bytes) => {
                    *fetch = None;
                    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                    let options = SoundOptions {
                        timeline: bgm.stepfile.sample_timeline(),
                        bus: MUSIC_BUS,
                        ..Default::default()
                    };
                    match SoundChannel::open(&mut parent, &bytes, &file_name, options) {
                        Ok(opened) => *channel = Some(opened),
                        Err(error) => {
                            godot_warn!("music cannot play: {error}");
                            bgm.music = None;
                        }
                    }
                }
            }
            return;
        };

        active.poll();
        if active.is_finished() {
            if clock.is_some() {
                // The file ran out (it had no loopable window); start it over.
                *channel = None;
            } else {
                // Finished before the clock ever ticked: nothing actually
                // plays (a sample start past the end of the file, say) —
                // restarting would respawn the audio every frame forever.
                godot_warn!("music finishes instantly, giving up: {:?}", bgm.music);
                bgm.music = None;
                *channel = None;
            }
            return;
        }
        let report = active.position();
        let clock = clock
            .get_or_insert_with(|| StepfileClock::start_at(bgm.stepfile.timing.clone(), report));
        clock.advance(Seconds(delta), Some(report));
    }
}
