use super::{ComboText, JudgmentShown, JudgmentText, OffsetOsd, PlaySession, direction_action};
use crate::core::config::{GameConfig, Judgment, StepOutcome, TimingFeedback};
use crate::core::input::Actions;
use crate::core::note_field::{HoldVisual, HoldVisualState, NoteFieldClock, Receptor};
use crate::core::settings::Settings;
use bevy::prelude::*;

/// Pushes the session's state into the note field: the drawn timeline, the
/// receptors' pressed panels, and every hold's render state. Runs after
/// grading and before the field's animation systems.
pub(super) fn sync_note_field(
    actions: Actions,
    session: Res<PlaySession>,
    settings: Res<Settings>,
    mut clock: ResMut<NoteFieldClock>,
    mut receptors: Query<&mut Receptor>,
    mut holds: Query<&mut HoldVisual>,
) {
    clock.visible = session.visible_now(&settings.timing);

    for mut receptor in &mut receptors {
        let held = actions.pressed(direction_action(receptor.column));
        if receptor.held != held {
            receptor.held = held;
        }
    }

    for note in &session.notes {
        let Some(hold) = &note.hold else { continue };
        let state = match (hold.engaged, hold.result) {
            (_, Some(true)) => HoldVisualState::Ok,
            (_, Some(false)) => HoldVisualState::Dropped,
            (false, None) => HoldVisualState::Pending,
            (true, None) if hold.held_now => HoldVisualState::Held,
            (true, None) => HoldVisualState::Released,
        };
        if let Ok(mut visual) = holds.get_mut(note.entity)
            && visual.state != state
        {
            visual.state = state;
        }
    }
}

pub(super) fn update_judgment_text(
    time: Res<Time>,
    config: Res<GameConfig>,
    mut shown: MessageReader<JudgmentShown>,
    mut label: Single<(&mut Text2d, &mut TextColor), With<JudgmentText>>,
) {
    let (text, color) = &mut *label;
    for message in shown.read() {
        let (value, base) = judgment_display(&config, message.outcome);
        text.0 = value;
        color.0 = base.with_alpha(1.0);
    }
    // Visible while the player keeps hitting arrows, gone within a second
    // once they stop.
    if color.0.alpha() > 0.0 {
        let alpha = (color.0.alpha() - time.delta_secs()).max(0.0);
        color.0.set_alpha(alpha);
    }
}

/// The judgment text and color for an outcome. Grades opting into timing
/// feedback mark the side of the perfect moment the input fell on: early
/// feedback leads the name, late feedback trails it.
fn judgment_display(config: &GameConfig, outcome: StepOutcome) -> (String, Color) {
    let StepOutcome::Hit { error } = outcome else {
        return (
            config.miss_appearance.name.clone(),
            config.miss_appearance.color,
        );
    };
    let Judgment::Grade(grade) = config.judge(outcome) else {
        unreachable!("hits always judge to a grade");
    };
    let definition = &config.grades[grade.0];
    let name = &definition.name;
    let early = error.0 > 0.0;
    // Displayed offset is input-relative: negative = early, positive = late.
    let offset_ms = (-error.to_millis()).round() as i64;
    let text = match definition.timing_feedback {
        TimingFeedback::Off => name.clone(),
        TimingFeedback::Sign if early => format!("-{name}"),
        TimingFeedback::Sign => format!("{name}-"),
        TimingFeedback::Millis if early => format!("({offset_ms}ms) {name}"),
        TimingFeedback::Millis => format!("{name} (+{offset_ms}ms)"),
    };
    (text, definition.color)
}

const COMBO_BOUNCE_SECONDS: f32 = 0.18;

pub(super) fn update_combo_text(
    time: Res<Time>,
    mut shown: MessageReader<JudgmentShown>,
    mut label: Single<(&mut Text2d, &mut Transform, &mut Visibility), With<ComboText>>,
    mut bounce: Local<f32>,
    mut last_combo: Local<u32>,
) {
    let (text, transform, visibility) = &mut *label;
    for message in shown.read() {
        if message.combo > *last_combo {
            *bounce = COMBO_BOUNCE_SECONDS;
        }
        *last_combo = message.combo;
        if message.combo == 0 {
            **visibility = Visibility::Hidden;
        } else {
            **visibility = Visibility::Visible;
            text.0 = format!("{} combo", message.combo);
        }
    }
    *bounce = (*bounce - time.delta_secs()).max(0.0);
    let scale = 1.0 + 0.22 * (*bounce / COMBO_BOUNCE_SECONDS);
    if transform.scale.x != scale {
        transform.scale = Vec3::splat(scale);
    }
}

pub(super) fn fade_offset_osd(time: Res<Time>, mut osd: Query<&mut TextColor, With<OffsetOsd>>) {
    for mut color in &mut osd {
        if color.0.alpha() > 0.0 {
            let alpha = (color.0.alpha() - time.delta_secs()).max(0.0);
            color.0.set_alpha(alpha);
        }
    }
}
