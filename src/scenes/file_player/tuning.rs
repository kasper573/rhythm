use super::{AutoSyncText, PlaySession, PlaySet, TickTrack, visuals::OffsetOsdLine};
use crate::core::audio::SoundChannel;
use crate::core::config::GameConfig;
use crate::core::input::{Actions, GameAction, shift_held};
use crate::core::settings::MachineSettings;
use crate::core::units::Millis;
use bevy::prelude::*;

/// The machine-tuning controls live during play: toggling the tick track,
/// AutoSync, and nudging the three synchronization offsets — all surfacing
/// through the offset OSD.
pub(super) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            toggle_tick_audio,
            toggle_autosync,
            fold_autosync,
            update_autosync_status,
            adjust_timing_offsets,
        )
            .chain()
            .in_set(PlaySet::Tune),
    );
}

fn toggle_autosync(actions: Actions, mut session: ResMut<PlaySession>) {
    if !actions.just_pressed(GameAction::ToggleAutoSync) {
        return;
    }
    session.autosync.enabled = !session.autosync.enabled;
    session.autosync.samples.clear();
}

/// AutoSync: with enough hit samples, fold their median error into the
/// machine offset (surfacing it through the usual offset OSD), reset, and
/// keep collecting until toggled off.
const AUTOSYNC_SAMPLES: usize = 24;

fn fold_autosync(
    mut session: ResMut<PlaySession>,
    mut settings: ResMut<MachineSettings>,
    mut osd: MessageWriter<OffsetOsdLine>,
) {
    if !session.autosync.enabled || session.autosync.samples.len() < AUTOSYNC_SAMPLES {
        return;
    }
    let mut samples = std::mem::take(&mut session.autosync.samples);
    samples.sort_by(|a, b| a.0.total_cmp(&b.0));
    let median = samples[samples.len() / 2];
    let delta = Millis(median.to_millis().round() as i64);
    if delta == Millis(0) {
        return;
    }
    settings.timing.machine_offset = settings.timing.machine_offset + delta;
    osd.write(OffsetOsdLine(format!(
        "Machine offset: {}",
        settings.timing.machine_offset
    )));
}

fn update_autosync_status(
    session: Res<PlaySession>,
    mut status: Single<(&mut Text, &mut Visibility), With<AutoSyncText>>,
    mut shown: Local<Option<(bool, usize)>>,
) {
    let state = (session.autosync.enabled, session.autosync.samples.len());
    if *shown == Some(state) {
        return;
    }
    *shown = Some(state);
    let (text, visibility) = &mut *status;
    if session.autosync.enabled {
        text.0 = format!("AutoSync ({}/{AUTOSYNC_SAMPLES} samples)", state.1);
        **visibility = Visibility::Visible;
    } else {
        **visibility = Visibility::Hidden;
    }
}

fn toggle_tick_audio(actions: Actions, mut tick: Query<&mut SoundChannel, With<TickTrack>>) {
    if !actions.just_pressed(GameAction::ToggleTickAudio) {
        return;
    }
    for mut channel in &mut tick {
        let muted = channel.is_muted();
        channel.set_muted(!muted);
    }
}

/// Adjusts the three synchronization offsets by 1ms (10ms with SHIFT held)
/// and surfaces the new value on the OSD.
fn adjust_timing_offsets(
    keys: Res<ButtonInput<KeyCode>>,
    mut settings: ResMut<MachineSettings>,
    config: Res<GameConfig>,
    mut osd: MessageWriter<OffsetOsdLine>,
) {
    let step = if shift_held(&keys) { 10 } else { 1 };
    let pairs = [
        (
            GameAction::DecreaseMachineOffset,
            GameAction::IncreaseMachineOffset,
        ),
        (
            GameAction::DecreaseVisualDelay,
            GameAction::IncreaseVisualDelay,
        ),
        (
            GameAction::DecreaseAudioLatency,
            GameAction::IncreaseAudioLatency,
        ),
    ];
    let mut osd_line = None;
    for (index, (decrease, increase)) in pairs.into_iter().enumerate() {
        let mut delta: i64 = 0;
        if settings.keymap.just_pressed(&keys, increase, &config) {
            delta += step;
        }
        if settings.keymap.just_pressed(&keys, decrease, &config) {
            delta -= step;
        }
        if delta == 0 {
            continue;
        }
        let timing = &mut settings.timing;
        osd_line = Some(match index {
            0 => {
                timing.machine_offset = timing.machine_offset + Millis(delta);
                format!("Machine offset: {}", timing.machine_offset)
            }
            1 => {
                timing.visual_delay = timing.visual_delay + Millis(delta);
                format!("Visual delay: {}", timing.visual_delay)
            }
            _ => {
                let latency = timing.audio_latency() + Millis(delta);
                timing.audio_latency = Some(latency);
                format!("Audio latency: {latency}")
            }
        });
    }
    let Some(line) = osd_line else { return };
    osd.write(OffsetOsdLine(line));
}
