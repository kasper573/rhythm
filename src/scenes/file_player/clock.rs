use super::{MusicTrack, PlayPhase, PlaySession, PlaySet, TickTrack};
use crate::core::settings::Settings;
use crate::core::units::{Millis, Seconds};
use bevy::audio::AudioSinkPlayback;
use bevy::prelude::*;

/// Keeps [`PlaySession::clock`] on the audio clock: a fixed lead-in counts up
/// to zero, both tracks start together, then the clock advances with frame
/// time and servos onto the mixer's position reports.
///
/// The mixer consumes audio in output-callback bursts, so its reported
/// position is a staircase: exact at the moment it changes, stale in
/// between. The clock therefore snaps once to the first report and then
/// applies small, slew-limited corrections toward each fresh report edge —
/// never jumping, never running backwards — so grading sees a smooth,
/// accurate timeline. Snapping to the staircase directly would make the
/// graded timeline oscillate by tens of milliseconds whenever the audio
/// quantum exceeds the snap threshold.
pub(super) fn plugin(app: &mut App) {
    app.add_systems(Update, advance_clock.in_set(PlaySet::Clock));
}

fn advance_clock(
    time: Res<Time>,
    mut session: ResMut<PlaySession>,
    mut settings: ResMut<Settings>,
    music: Query<&AudioSink, With<MusicTrack>>,
    tick: Query<&AudioSink, With<TickTrack>>,
) {
    let delta = Seconds(time.delta_secs_f64());
    match session.phase {
        PlayPhase::LeadIn { remaining } => {
            let remaining = remaining - delta;
            session.clock = -remaining.max(Seconds::ZERO);
            if remaining.0 > 0.0 {
                session.phase = PlayPhase::LeadIn { remaining };
                return;
            }
            // Hold at zero until every spawned track has a live sink, so the
            // music and the tick track always start in lockstep.
            let music_sink = music.single();
            let tick_sink = tick.single();
            if music_sink.is_ok() || tick_sink.is_ok() {
                if let Ok(sink) = music_sink {
                    sink.play();
                }
                if let Ok(sink) = tick_sink {
                    sink.play();
                }
                session.phase = PlayPhase::Playing;
            } else {
                session.phase = PlayPhase::LeadIn {
                    remaining: Seconds::ZERO,
                };
            }
        }
        PlayPhase::Playing => {
            session.clock += delta;
            session.wall_since_play += delta;
            let position = music
                .single()
                .or(tick.single())
                .map(|sink| Seconds(sink.position().as_secs_f64()));
            if let Ok(position) = position
                && position != session.last_sink_position
            {
                let first_report = session.last_sink_position.0 < 0.0;
                session.last_sink_position = position;
                servo_clock(&mut session, position, first_report);

                // Reading through the ResMut must not touch it mutably:
                // change detection would flag Settings every frame and the
                // auto-save would hammer the disk.
                if settings.timing.audio_latency.is_none()
                    && let Some(measured) = measure_audio_latency(&mut session, position)
                {
                    settings.timing.audio_latency = Some(measured);
                    info!("measured audio latency: {measured}");
                }
            }
        }
    }
}

/// Proportional correction per fresh report, slew-limited so the clock stays
/// smooth: at typical report rates the steady-state tracking error is a
/// couple of milliseconds, constant biases land in the calibrated audio
/// latency instead.
const SERVO_GAIN: f64 = 0.08;
const MAX_BACKWARD_STEP: f64 = 0.002;
const MAX_FORWARD_STEP: f64 = 0.010;
/// Beyond this the stream underran or seeked; tracking smoothly is wrong.
const RESYNC_THRESHOLD: f64 = 0.25;

fn servo_clock(session: &mut PlaySession, position: Seconds, first_report: bool) {
    let error = position.0 - session.clock.0;
    if first_report || error.abs() > RESYNC_THRESHOLD {
        session.clock = position;
        return;
    }
    session.clock.0 += (error * SERVO_GAIN).clamp(-MAX_BACKWARD_STEP, MAX_FORWARD_STEP);
}

/// The mixer consumes samples ahead of real time by roughly the output
/// buffer it keeps queued — which is how far the reported position runs
/// ahead of the speakers. Returns the steady-state median of that lead once
/// enough samples are in: the first-start audio latency estimate.
fn measure_audio_latency(session: &mut PlaySession, position: Seconds) -> Option<Millis> {
    let wall = session.wall_since_play;
    if (0.3..2.0).contains(&wall.0) {
        session.latency_samples.push(position - wall);
        return None;
    }
    if wall.0 < 2.0 || session.latency_samples.is_empty() {
        return None;
    }
    let mut samples = std::mem::take(&mut session.latency_samples);
    samples.sort_by(|a, b| a.0.total_cmp(&b.0));
    let median = samples[samples.len() / 2];
    Some(Millis(median.to_millis().round().max(0.0) as i64))
}
