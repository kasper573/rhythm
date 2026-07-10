use super::TickTrack;
use crate::core::audio::SoundChannel;
use crate::core::config::GameConfig;
use crate::core::font::game_font;
use crate::core::input::{Actions, GameAction, shift_held};
use crate::core::scene_flow::SpawnScoped;
use crate::core::settings::MachineSettings;
use crate::core::units::{Millis, Seconds};
use crate::prefabs::stepfile_player::{PlaySet, PressBanked};
use crate::scenes::GameScene;
use bevy::prelude::*;

/// The machine-tuning controls live during play: toggling the tick track,
/// AutoSync, and nudging the three synchronization offsets — all surfacing
/// through the offset OSD.
pub(super) fn plugin(app: &mut App) {
    app.add_message::<OffsetOsdLine>()
        .add_systems(OnEnter(GameScene::Play), enter)
        .add_systems(OnExit(GameScene::Play), exit)
        .add_systems(
            Update,
            (
                toggle_tick_audio,
                toggle_autosync,
                collect_autosync_samples,
                fold_autosync,
                update_autosync_status,
                adjust_timing_offsets,
                run_offset_osd,
            )
                .chain()
                .in_set(PlaySet::Present)
                .run_if(in_state(GameScene::Play)),
        );
}

/// A line to flash on the timing-offset OSD.
#[derive(Message)]
struct OffsetOsdLine(String);

/// While enabled, hit errors accumulate and the median of every batch is
/// folded into the machine offset.
#[derive(Resource, Default)]
struct AutoSync {
    enabled: bool,
    samples: Vec<Seconds>,
}

#[derive(Component, Default, Clone)]
struct OffsetOsd;

#[derive(Component, Default, Clone)]
struct AutoSyncText;

/// The machine-wide readouts: the timing-offset OSD and AutoSync status.
fn enter(mut commands: Commands) {
    commands.init_resource::<AutoSync>();
    commands.spawn_scoped(
        GameScene::Play,
        bsn! {
            OffsetOsd
            game_font(24.0)
            Text("")
            TextColor(Color::srgba(1.0, 1.0, 1.0, 0.0))
            Node {
                position_type: PositionType::Absolute,
                right: px(24),
                bottom: px(16),
            }
        },
    );
    commands.spawn_scoped(
        GameScene::Play,
        bsn! {
            AutoSyncText
            game_font(24.0)
            Text("")
            TextColor(Color::srgb(0.5, 0.9, 1.0))
            Node {
                position_type: PositionType::Absolute,
                right: px(24),
                bottom: px(48),
            }
            Visibility::Hidden
        },
    );
}

fn exit(mut commands: Commands) {
    commands.remove_resource::<AutoSync>();
}

fn toggle_autosync(actions: Actions, mut autosync: ResMut<AutoSync>) {
    if !actions.just_pressed(GameAction::ToggleAutoSync) {
        return;
    }
    autosync.enabled = !autosync.enabled;
    autosync.samples.clear();
}

/// Samples every banked press's timing error the engine reports.
fn collect_autosync_samples(
    mut banked: MessageReader<PressBanked>,
    mut autosync: ResMut<AutoSync>,
) {
    for press in banked.read() {
        if autosync.enabled {
            autosync.samples.push(press.error);
        }
    }
}

/// AutoSync: with enough hit samples, fold their median error into the
/// machine offset (surfacing it through the usual offset OSD), reset, and
/// keep collecting until toggled off.
const AUTOSYNC_SAMPLES: usize = 24;

fn fold_autosync(
    mut autosync: ResMut<AutoSync>,
    mut settings: ResMut<MachineSettings>,
    mut osd: MessageWriter<OffsetOsdLine>,
) {
    if !autosync.enabled || autosync.samples.len() < AUTOSYNC_SAMPLES {
        return;
    }
    let mut samples = std::mem::take(&mut autosync.samples);
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
    autosync: Res<AutoSync>,
    mut status: Single<(&mut Text, &mut Visibility), With<AutoSyncText>>,
    mut shown: Local<Option<(bool, usize)>>,
) {
    let state = (autosync.enabled, autosync.samples.len());
    if *shown == Some(state) {
        return;
    }
    *shown = Some(state);
    let (text, visibility) = &mut *status;
    if autosync.enabled {
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

fn run_offset_osd(
    time: Res<Time>,
    mut lines: MessageReader<OffsetOsdLine>,
    mut osd: Single<(&mut Text, &mut TextColor), With<OffsetOsd>>,
) {
    let (text, color) = &mut *osd;
    for line in lines.read() {
        text.0 = line.0.clone();
        color.0.set_alpha(1.0);
    }
    if color.0.alpha() > 0.0 {
        let alpha = (color.0.alpha() - time.delta_secs()).max(0.0);
        color.0.set_alpha(alpha);
    }
}
