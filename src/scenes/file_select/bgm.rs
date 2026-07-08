use super::{ActiveRowHighlight, Wheel, WheelEntry};
use crate::core::config::RhythmCycle;
use crate::core::library::StepfileLibrary;
use crate::core::settings::Settings;
use crate::core::stepfile::MusicPlayer;
use crate::core::units::Seconds;
use bevy::prelude::*;
use std::path::PathBuf;

/// Scrolling must settle before the music switches, so passing rows don't
/// each restart it.
const SWITCH_DEBOUNCE: Seconds = Seconds(0.35);

/// Once every beat, apex on it, decaying cubically until the next.
const HIGHLIGHT_PULSE: RhythmCycle = RhythmCycle {
    speed: 4.0,
    easing: [0.32, 0.0, 0.67, 0.0],
};

#[derive(Default)]
pub(super) struct PendingSwitch {
    target: Option<PathBuf>,
    wait: Seconds,
}

/// The active row's stepfile is the scene's background music; rows without
/// one (groups) fall back to the default BGM. Switching to what is already
/// playing is the player's no-op, so browsing rows that resolve to the
/// same music never interrupts it.
pub(super) fn drive_wheel_bgm(
    time: Res<Time>,
    wheel: Res<Wheel>,
    library: Res<StepfileLibrary>,
    mut player: ResMut<MusicPlayer>,
    mut pending: Local<PendingSwitch>,
    mut commands: Commands,
) {
    let entry = match wheel.entries.get(wheel.active) {
        Some(WheelEntry::Stepfile { id }) => library.stepfile(*id),
        _ => &library.default_bgm,
    };
    if pending.target.as_deref() != Some(entry.sm_path.as_path()) {
        pending.target = Some(entry.sm_path.clone());
        pending.wait = Seconds::ZERO;
        return;
    }
    pending.wait += Seconds(time.delta_secs_f64());
    if pending.wait >= SWITCH_DEBOUNCE {
        player.play(&mut commands, entry.bgm());
    }
}

/// Pulses the active-row highlight's opacity between 0.5 and 1 on the
/// music's beat, apex on the beat; a steady 1 while nothing plays.
pub(super) fn pulse_active_row(
    settings: Res<Settings>,
    player: Res<MusicPlayer>,
    mut highlight: Single<&mut Sprite, With<ActiveRowHighlight>>,
) {
    let alpha = match player.visible_beat(&settings.timing) {
        Some(beat) => 0.5 + 0.5 * HIGHLIGHT_PULSE.strike(beat.0),
        None => 1.0,
    };
    if highlight.color.alpha() != alpha {
        highlight.color.set_alpha(alpha);
    }
}
