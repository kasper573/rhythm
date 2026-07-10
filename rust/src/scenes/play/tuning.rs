use super::PlayScene;
use crate::core::font::label;
use crate::core::input::{Actions, GameAction, shift_held};
use crate::core::settings::Settings;
use crate::core::units::{Millis, Seconds};
use godot::classes::Label;
use godot::classes::control::LayoutPreset;
use godot::prelude::*;

/// The machine-tuning controls live during play: toggling the tick track,
/// AutoSync, and nudging the three synchronization offsets — all surfacing
/// through the offset OSD.
pub(super) struct Tuning {
    autosync_enabled: bool,
    samples: Vec<Seconds>,
    shown: Option<(bool, usize)>,
    osd: Gd<Label>,
    osd_alpha: f32,
    status: Gd<Label>,
}

/// AutoSync: with enough hit samples, fold their median error into the
/// machine offset (surfacing it through the usual offset OSD), reset, and
/// keep collecting until toggled off.
const AUTOSYNC_SAMPLES: usize = 24;

impl Tuning {
    pub fn new(scene: &mut godot::classes::Control) -> Tuning {
        let mut osd = label("", 24.0, Color::from_rgba(1.0, 1.0, 1.0, 0.0));
        osd.set_anchors_preset(LayoutPreset::BOTTOM_RIGHT);
        osd.set_horizontal_alignment(godot::global::HorizontalAlignment::RIGHT);
        osd.set_position(Vector2::new(-424.0, -40.0));
        osd.set_size(Vector2::new(400.0, 30.0));
        scene.add_child(&osd);
        let mut status = label("", 24.0, Color::from_rgb(0.5, 0.9, 1.0));
        status.set_anchors_preset(LayoutPreset::BOTTOM_RIGHT);
        status.set_horizontal_alignment(godot::global::HorizontalAlignment::RIGHT);
        status.set_position(Vector2::new(-424.0, -72.0));
        status.set_size(Vector2::new(400.0, 30.0));
        status.set_visible(false);
        scene.add_child(&status);
        Tuning {
            autosync_enabled: false,
            samples: Vec::new(),
            shown: None,
            osd,
            osd_alpha: 0.0,
            status,
        }
    }

    /// Samples every banked press's timing error the engine reports.
    pub fn push_sample(&mut self, error: Seconds) {
        if self.autosync_enabled {
            self.samples.push(error);
        }
    }

    fn flash(&mut self, line: String) {
        self.osd.set_text(&line);
        self.osd_alpha = 1.0;
    }
}

impl PlayScene {
    pub(super) fn run_tuning(&mut self, delta: f64) {
        let Some(tuning) = &mut self.tuning else {
            return;
        };

        if Actions::just_pressed(GameAction::ToggleTickAudio)
            && let Some(tick) = &mut self.tick
        {
            let muted = tick.is_muted();
            tick.set_muted(!muted);
        }

        if Actions::just_pressed(GameAction::ToggleAutoSync) {
            tuning.autosync_enabled = !tuning.autosync_enabled;
            tuning.samples.clear();
        }

        if tuning.autosync_enabled && tuning.samples.len() >= AUTOSYNC_SAMPLES {
            let mut samples = std::mem::take(&mut tuning.samples);
            samples.sort_by(|a, b| a.0.total_cmp(&b.0));
            let median = samples[samples.len() / 2];
            let delta_ms = Millis(median.to_millis().round() as i64);
            if delta_ms != Millis(0) {
                let mut settings = Settings::singleton();
                settings.bind_mut().edit_machine(|machine| {
                    machine.timing.machine_offset = machine.timing.machine_offset + delta_ms;
                });
                let offset = settings.bind().machine().timing.machine_offset;
                tuning.flash(format!("Machine offset: {offset}"));
            }
        }

        let state = (tuning.autosync_enabled, tuning.samples.len());
        if tuning.shown != Some(state) {
            tuning.shown = Some(state);
            if tuning.autosync_enabled {
                tuning.status.set_text(&format!(
                    "AutoSync ({}/{AUTOSYNC_SAMPLES} samples)",
                    state.1
                ));
                tuning.status.set_visible(true);
            } else {
                tuning.status.set_visible(false);
            }
        }

        adjust_timing_offsets(tuning);

        if tuning.osd_alpha > 0.0 {
            tuning.osd_alpha = (tuning.osd_alpha - delta as f32).max(0.0);
            let mut color = tuning.osd.get_modulate();
            color.a = tuning.osd_alpha;
            tuning.osd.set_modulate(color);
        }
    }
}

/// Adjusts the three synchronization offsets by 1ms (10ms with SHIFT held)
/// and surfaces the new value on the OSD.
fn adjust_timing_offsets(tuning: &mut Tuning) {
    let step = if shift_held() { 10 } else { 1 };
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
        if Actions::just_pressed(increase) {
            delta += step;
        }
        if Actions::just_pressed(decrease) {
            delta -= step;
        }
        if delta == 0 {
            continue;
        }
        let mut settings = Settings::singleton();
        let mut line = String::new();
        settings.bind_mut().edit_machine(|machine| {
            let timing = &mut machine.timing;
            line = match index {
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
            };
        });
        osd_line = Some(line);
    }
    if let Some(line) = osd_line {
        tuning.flash(line);
    }
}
