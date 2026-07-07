use rhythm::core::stepfile::StepfileTiming;
use rhythm::core::units::{Beat, Seconds};

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn constant_bpm_maps_beats_linearly() {
    let timing = StepfileTiming::new(Seconds::ZERO, &[(Beat(0.0), 120.0)], &[]);
    assert_close(timing.seconds_at_beat(Beat(0.0)).0, 0.0);
    assert_close(timing.seconds_at_beat(Beat(4.0)).0, 2.0);
    assert_close(timing.beat_at_seconds(Seconds(2.0)).0, 4.0);
}

#[test]
fn offset_shifts_beat_zero() {
    let timing = StepfileTiming::new(Seconds(-0.5), &[(Beat(0.0), 60.0)], &[]);
    assert_close(timing.seconds_at_beat(Beat(0.0)).0, 0.5);
    assert_close(timing.seconds_at_beat(Beat(2.0)).0, 2.5);
    assert_close(timing.beat_at_seconds(Seconds(0.5)).0, 0.0);
}

#[test]
fn bpm_change_alters_slope() {
    let timing = StepfileTiming::new(Seconds::ZERO, &[(Beat(0.0), 60.0), (Beat(4.0), 120.0)], &[]);
    assert_close(timing.seconds_at_beat(Beat(4.0)).0, 4.0);
    assert_close(timing.seconds_at_beat(Beat(6.0)).0, 5.0);
    assert_close(timing.beat_at_seconds(Seconds(5.0)).0, 6.0);
    assert_eq!(timing.bpm_range(), (60.0, 120.0));
}

#[test]
fn stop_freezes_time() {
    let timing = StepfileTiming::new(
        Seconds::ZERO,
        &[(Beat(0.0), 60.0)],
        &[(Beat(4.0), Seconds(2.0))],
    );
    // A note on the stop beat is hit before the pause.
    assert_close(timing.seconds_at_beat(Beat(4.0)).0, 4.0);
    // Beats after the stop are delayed by its duration.
    assert_close(timing.seconds_at_beat(Beat(5.0)).0, 7.0);
    // The beat holds still while the clock runs through the stop.
    assert_close(timing.beat_at_seconds(Seconds(4.5)).0, 4.0);
    assert_close(timing.beat_at_seconds(Seconds(6.0)).0, 4.0);
    assert_close(timing.beat_at_seconds(Seconds(7.0)).0, 5.0);
}

#[test]
fn beats_before_zero_extrapolate() {
    let timing = StepfileTiming::new(Seconds::ZERO, &[(Beat(0.0), 60.0)], &[]);
    assert_close(timing.seconds_at_beat(Beat(-2.0)).0, -2.0);
    assert_close(timing.beat_at_seconds(Seconds(-2.0)).0, -2.0);
}

#[test]
fn beat_phase_wraps_within_beat() {
    let timing = StepfileTiming::new(Seconds::ZERO, &[(Beat(0.0), 60.0)], &[]);
    assert_close(timing.beat_phase(Seconds(2.25)), 0.25);
    assert_close(timing.beat_phase(Seconds(3.0)), 0.0);
}
