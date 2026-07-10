use super::{ActiveRowHighlight, Wheel, WheelEntry};
use crate::core::config::RhythmCycle;
use crate::core::library::StepfileLibrary;
use crate::core::settings::MachineSettings;
use crate::core::stepfile::MusicPlayer;
use bevy::prelude::*;

/// Once every beat, apex on it, decaying cubically until the next.
const HIGHLIGHT_PULSE: RhythmCycle = RhythmCycle {
    speed: 4.0,
    easing: [0.32, 0.0, 0.67, 0.0],
};

/// The settled row's stepfile is the scene's background music; rows
/// without one (groups) fall back to the default BGM. Switching to what
/// is already playing is the player's no-op, so rows that resolve to the
/// same music keep it running uninterrupted.
pub(super) fn drive_wheel_bgm(
    wheel: Res<Wheel>,
    library: Res<StepfileLibrary>,
    mut player: ResMut<MusicPlayer>,
) {
    if !wheel.just_settled {
        return;
    }
    let entry = match wheel.entries.get(wheel.active) {
        Some(WheelEntry::Stepfile { id }) => library.stepfile(*id),
        _ => &library.default_bgm,
    };
    player.play(entry.bgm());
}

/// Pulses the active-row highlight's opacity between 0.5 and 1 on the
/// music's beat, apex on the beat; a steady 1 while nothing plays.
pub(super) fn pulse_active_row(
    settings: Res<MachineSettings>,
    player: Res<MusicPlayer>,
    mut highlight: Single<&mut Sprite, With<ActiveRowHighlight>>,
) {
    let alpha = match player.visible_beat(&settings.timing) {
        Some(beat) => 0.5 + 0.5 * HIGHLIGHT_PULSE.strike(beat),
        None => 1.0,
    };
    if highlight.color.alpha() != alpha {
        highlight.color.set_alpha(alpha);
    }
}
