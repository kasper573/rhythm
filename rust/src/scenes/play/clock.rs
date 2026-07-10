use super::PlayScene;
use crate::core::settings::Settings;
use crate::core::stepfile::{StepfileClock, StepfileTiming};
use crate::core::units::{Millis, Seconds};
use godot::global::godot_print;

/// The scene's session flow around the engine: the playback clock that
/// drives the ports, the moment the chart is over, and whether the run is
/// underway. A fixed lead-in counts up to zero, both tracks start
/// together, then the [`StepfileClock`] servos onto the channel's position
/// reports so grading sees a smooth, accurate timeline.
pub(super) struct Playback {
    pub title: String,
    phase: PlayPhase,
    /// The shared stepfile music clock, servo'd onto the audio.
    music: StepfileClock,
    /// Wall-clock time since the tracks were started, for measuring how far
    /// the mixer's queue runs ahead of real time (the audio latency).
    wall_since_play: Seconds,
    latency_samples: Vec<Seconds>,
    pub last_note_time: Seconds,
}

enum PlayPhase {
    LeadIn { remaining: Seconds },
    Playing,
}

impl Playback {
    pub fn new(
        title: String,
        timing: StepfileTiming,
        lead_in: Seconds,
        last_note_time: Seconds,
    ) -> Playback {
        Playback {
            title,
            phase: PlayPhase::LeadIn { remaining: lead_in },
            music: StepfileClock::start_at(timing, -lead_in),
            wall_since_play: Seconds::ZERO,
            latency_samples: Vec::new(),
            last_note_time,
        }
    }

    pub fn position(&self) -> Seconds {
        self.music.position()
    }

    pub fn is_playing(&self) -> bool {
        matches!(self.phase, PlayPhase::Playing)
    }

    pub fn visible_now(&self) -> Seconds {
        let settings = Settings::singleton();
        let timing = settings.bind().machine().timing.clone();
        self.music.visible_now(&timing)
    }
}

impl PlayScene {
    /// The real adapter's clock driver: keeps [`Playback`] on the audio
    /// clock and publishes the graded/visible moments to the engine's
    /// clock port.
    pub(super) fn advance_clock(&mut self, delta: f64) {
        let delta = Seconds(delta);
        let Some(playback) = &mut self.playback else {
            return;
        };
        match playback.phase {
            PlayPhase::LeadIn { remaining } => {
                let remaining = remaining - delta;
                playback.music.set_position(-remaining.max(Seconds::ZERO));
                if remaining.0 > 0.0 {
                    playback.phase = PlayPhase::LeadIn { remaining };
                } else if self.music_fetch.is_some() {
                    // Hold at zero while the music is still on its way, so
                    // the music and the tick track start in lockstep. A
                    // failed fetch never holds the start: the session
                    // plays with whatever survives, silent if nothing does.
                    playback.phase = PlayPhase::LeadIn {
                        remaining: Seconds::ZERO,
                    };
                } else {
                    for channel in [self.music.as_mut(), self.tick.as_mut()]
                        .into_iter()
                        .flatten()
                    {
                        channel.set_paused(false);
                    }
                    playback.phase = PlayPhase::Playing;
                }
            }
            PlayPhase::Playing => {
                playback.wall_since_play += delta;
                for channel in [self.music.as_mut(), self.tick.as_mut()]
                    .into_iter()
                    .flatten()
                {
                    channel.poll();
                }
                let report = self
                    .music
                    .as_ref()
                    .or(self.tick.as_ref())
                    .map(|channel| channel.position());
                let fresh = playback.music.advance(delta, report);

                let settings = Settings::singleton();
                let unmeasured = settings.bind().machine().timing.audio_latency.is_none();
                if fresh
                    && unmeasured
                    && let Some(report) = report
                    && let Some(measured) = measure_audio_latency(playback, report)
                {
                    Settings::singleton()
                        .bind_mut()
                        .edit_machine(|machine| machine.timing.audio_latency = Some(measured));
                    godot_print!("measured audio latency: {measured}");
                }
            }
        }

        let settings = Settings::singleton();
        let timing = settings.bind().machine().timing.clone();
        let graded = playback.music.graded_now(&timing);
        let visible = playback.music.visible_now(&timing);
        if let Some(engine) = &mut self.engine {
            engine.bind_mut().set_time(graded, visible);
        }
    }
}

/// The mixer consumes samples ahead of real time by roughly the output
/// buffer it keeps queued — which is how far the reported position runs
/// ahead of the speakers. Returns the steady-state median of that lead once
/// enough samples are in: the first-start audio latency estimate.
fn measure_audio_latency(playback: &mut Playback, report: Seconds) -> Option<Millis> {
    let wall = playback.wall_since_play;
    if (0.3..2.0).contains(&wall.0) {
        playback.latency_samples.push(report - wall);
        return None;
    }
    if wall.0 < 2.0 || playback.latency_samples.is_empty() {
        return None;
    }
    let mut samples = std::mem::take(&mut playback.latency_samples);
    samples.sort_by(|a, b| a.0.total_cmp(&b.0));
    let median = samples[samples.len() / 2];
    Some(Millis(median.to_millis().round().max(0.0) as i64))
}
