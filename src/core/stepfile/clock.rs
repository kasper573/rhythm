use crate::core::settings::TimingSettings;
use crate::core::stepfile::StepfileTiming;
use crate::core::units::{Beat, Seconds};

/// Where a playing stepfile's music is, on every timeline the game
/// shares: the servo'd mixer position, the graded timeline inputs are
/// judged on, and the visible timeline everything is drawn on — with
/// beats derived through the stepfile's own timing. Every scene that
/// plays stepfile audio reads the same clock the same way.
pub struct StepfileClock {
    pub timing: StepfileTiming,
    servo: AudioServo,
}

impl StepfileClock {
    pub fn start_at(timing: StepfileTiming, position: Seconds) -> StepfileClock {
        StepfileClock {
            timing,
            servo: AudioServo {
                position,
                last_report: None,
            },
        }
    }

    /// Drives the clock directly, for pre-playback timelines like a
    /// lead-in counting up to the music's start.
    pub fn set_position(&mut self, position: Seconds) {
        self.servo.position = position;
    }

    /// Advances by frame time and servos onto the mixer report when given
    /// one; returns whether the report was a fresh edge.
    pub fn advance(&mut self, delta: Seconds, report: Option<Seconds>) -> bool {
        self.servo.advance(delta, report)
    }

    /// The raw position on the mixer-queue timeline.
    pub fn position(&self) -> Seconds {
        self.servo.position
    }

    pub fn graded_now(&self, settings: &TimingSettings) -> Seconds {
        settings.graded(self.position())
    }

    pub fn visible_now(&self, settings: &TimingSettings) -> Seconds {
        settings.visible(self.position())
    }

    pub fn visible_beat(&self, settings: &TimingSettings) -> Beat {
        self.timing.beat_at_seconds(self.visible_now(settings))
    }
}

/// A smooth clock servo'd onto the audio mixer's position reports.
///
/// The mixer consumes audio in output-callback bursts, so its reported
/// position is a staircase: exact at the moment it changes, stale in
/// between. The servo therefore advances with frame time, snaps once to
/// the first report, and then applies small, slew-limited corrections
/// toward each fresh report edge — never jumping, never running backwards
/// — so consumers see a smooth, accurate timeline. Snapping to the
/// staircase directly would make the timeline oscillate by tens of
/// milliseconds whenever the audio quantum exceeds the snap threshold.
/// Reports that leap beyond [`RESYNC_THRESHOLD`] (a seek, an underrun, a
/// loop seam) snap instead of slewing.
struct AudioServo {
    position: Seconds,
    last_report: Option<Seconds>,
}

/// Proportional correction per fresh report, slew-limited so the clock
/// stays smooth: at typical report rates the steady-state tracking error
/// is a couple of milliseconds, constant biases land in the calibrated
/// audio latency instead.
const SERVO_GAIN: f64 = 0.08;
const MAX_BACKWARD_STEP: f64 = 0.002;
const MAX_FORWARD_STEP: f64 = 0.010;
const RESYNC_THRESHOLD: f64 = 0.25;

impl AudioServo {
    fn advance(&mut self, delta: Seconds, report: Option<Seconds>) -> bool {
        self.position += delta;
        let Some(report) = report else { return false };
        if self.last_report == Some(report) {
            return false;
        }
        let first = self.last_report.is_none();
        self.last_report = Some(report);
        let error = report.0 - self.position.0;
        if first || error.abs() > RESYNC_THRESHOLD {
            self.position = report;
        } else {
            self.position.0 += (error * SERVO_GAIN).clamp(-MAX_BACKWARD_STEP, MAX_FORWARD_STEP);
        }
        true
    }
}
