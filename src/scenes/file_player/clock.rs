use super::{MusicTrack, PlayPhase, PlaySession, PlaySet, PlayTime, PlaybackClock, TickTrack};
use crate::core::audio::{SoundChannel, SoundPlayer};
use crate::core::settings::MachineSettings;
use crate::core::units::{Millis, Seconds};
use crate::scenes::GameScene;
use bevy::prelude::*;

/// Keeps the session's [`PlaybackClock`] on the audio clock: a fixed
/// lead-in counts up to zero, both tracks start together, then the
/// [`StepfileClock`](crate::core::stepfile::StepfileClock) servos onto
/// the channel's position reports so grading sees a smooth, accurate
/// timeline. Publishes the resulting graded/visible moments to [`PlayTime`]
/// for the shared engine. The options preview drives [`PlayTime`] itself,
/// so this stays on the play stage.
pub(super) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        advance_clock
            .in_set(PlaySet::Clock)
            .run_if(in_state(GameScene::FilePlayer)),
    );
}

type TrackChannel = (Option<&'static mut SoundChannel>, Has<SoundPlayer>);

fn advance_clock(
    time: Res<Time>,
    mut session: ResMut<PlaySession>,
    mut settings: ResMut<MachineSettings>,
    mut play_time: ResMut<PlayTime>,
    mut music: Query<TrackChannel, (With<MusicTrack>, Without<TickTrack>)>,
    mut tick: Query<TrackChannel, (With<TickTrack>, Without<MusicTrack>)>,
) {
    let delta = Seconds(time.delta_secs_f64());
    let clock = &mut session.clock;
    match clock.phase {
        PlayPhase::LeadIn { remaining } => {
            let remaining = remaining - delta;
            clock.music.set_position(-remaining.max(Seconds::ZERO));
            if remaining.0 > 0.0 {
                clock.phase = PlayPhase::LeadIn { remaining };
            } else {
                // Hold at zero while any track is still loading or decoding,
                // so the music and the tick track start in lockstep. Tracks
                // that failed outright never hold the start: the session
                // plays with whatever survives, silent if nothing does.
                let mut tracks: Vec<_> = music.iter_mut().chain(tick.iter_mut()).collect();
                let pending = tracks.iter().any(|(channel, queued)| match channel {
                    Some(channel) => !channel.is_ready(),
                    None => *queued,
                });
                if pending {
                    clock.phase = PlayPhase::LeadIn {
                        remaining: Seconds::ZERO,
                    };
                } else {
                    for (channel, _) in &mut tracks {
                        if let Some(channel) = channel {
                            channel.set_paused(false);
                        }
                    }
                    clock.phase = PlayPhase::Playing;
                }
            }
        }
        PlayPhase::Playing => {
            clock.wall_since_play += delta;
            let report = music
                .iter()
                .find_map(|(channel, _)| channel)
                .or_else(|| tick.iter().find_map(|(channel, _)| channel))
                .map(|channel| channel.position());
            let fresh = clock.music.advance(delta, report);

            // Reading through the ResMut must not touch it mutably:
            // change detection would flag Settings every frame and the
            // auto-save would hammer the disk.
            if fresh
                && settings.timing.audio_latency.is_none()
                && let Some(report) = report
                && let Some(measured) = measure_audio_latency(clock, report)
            {
                settings.timing.audio_latency = Some(measured);
                info!("measured audio latency: {measured}");
            }
        }
    }
    play_time.graded = session.clock.music.graded_now(&settings.timing);
    play_time.visible = session.clock.music.visible_now(&settings.timing);
}

/// The mixer consumes samples ahead of real time by roughly the output
/// buffer it keeps queued — which is how far the reported position runs
/// ahead of the speakers. Returns the steady-state median of that lead once
/// enough samples are in: the first-start audio latency estimate.
fn measure_audio_latency(clock: &mut PlaybackClock, report: Seconds) -> Option<Millis> {
    let wall = clock.wall_since_play;
    if (0.3..2.0).contains(&wall.0) {
        clock.latency_samples.push(report - wall);
        return None;
    }
    if wall.0 < 2.0 || clock.latency_samples.is_empty() {
        return None;
    }
    let mut samples = std::mem::take(&mut clock.latency_samples);
    samples.sort_by(|a, b| a.0.total_cmp(&b.0));
    let median = samples[samples.len() / 2];
    Some(Millis(median.to_millis().round().max(0.0) as i64))
}
