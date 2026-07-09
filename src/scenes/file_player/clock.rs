use super::{MusicTrack, PlayPhase, PlaySession, PlaySet, PlaybackClock, TickTrack};
use crate::core::audio::SoundChannel;
use crate::core::settings::Settings;
use crate::core::units::{Millis, Seconds};
use bevy::prelude::*;

/// Keeps the session's [`PlaybackClock`] on the audio clock: a fixed
/// lead-in counts up to zero, both tracks start together, then the
/// [`AudioClock`](crate::core::audio_clock::AudioClock) servos onto the
/// channel's position reports so grading sees a smooth, accurate timeline.
pub(super) fn plugin(app: &mut App) {
    app.add_systems(Update, advance_clock.in_set(PlaySet::Clock));
}

fn advance_clock(
    time: Res<Time>,
    mut session: ResMut<PlaySession>,
    mut settings: ResMut<Settings>,
    mut music: Query<Option<&mut SoundChannel>, (With<MusicTrack>, Without<TickTrack>)>,
    mut tick: Query<Option<&mut SoundChannel>, (With<TickTrack>, Without<MusicTrack>)>,
) {
    let delta = Seconds(time.delta_secs_f64());
    let clock = &mut session.clock;
    match clock.phase {
        PlayPhase::LeadIn { remaining } => {
            let remaining = remaining - delta;
            clock.music.set_position(-remaining.max(Seconds::ZERO));
            if remaining.0 > 0.0 {
                clock.phase = PlayPhase::LeadIn { remaining };
                return;
            }
            // Hold at zero until every spawned track is decoded and
            // playable, so the music and the tick track always start in
            // lockstep.
            let mut channels: Vec<_> = music.iter_mut().chain(tick.iter_mut()).collect();
            let ready = !channels.is_empty()
                && channels
                    .iter()
                    .all(|channel| channel.as_ref().is_some_and(|channel| channel.is_ready()));
            if ready {
                for channel in channels.iter_mut().flatten() {
                    channel.set_paused(false);
                }
                clock.phase = PlayPhase::Playing;
            } else {
                clock.phase = PlayPhase::LeadIn {
                    remaining: Seconds::ZERO,
                };
            }
        }
        PlayPhase::Playing => {
            clock.wall_since_play += delta;
            let report = music
                .iter()
                .flatten()
                .next()
                .or_else(|| tick.iter().flatten().next())
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
