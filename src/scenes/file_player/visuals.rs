use super::{
    ComboText, GradeText, HoldOutcome, OffsetOsd, PlaySession, PlaySet, RowGraded, direction_action,
};
use crate::core::config::{GameConfig, Grade, RowOutcome, TimingFeedback};
use crate::core::health_vial::HealthVial;
use crate::core::input::Actions;
use crate::core::note_field::{HoldVisual, HoldVisualState, NoteFieldClock, Receptor};
use crate::core::settings::Settings;
use crate::core::units::Seconds;
use bevy::prelude::*;

pub(super) fn plugin(app: &mut App) {
    app.add_message::<OffsetOsdLine>()
        .add_systems(
            Update,
            (sync_note_field, sync_health_vial).in_set(PlaySet::Sync),
        )
        .add_systems(
            Update,
            (update_grade_text, update_combo_text, run_offset_osd)
                .chain()
                .in_set(PlaySet::Present),
        );
}

/// A line to flash on the timing-offset OSD.
#[derive(Message)]
pub(super) struct OffsetOsdLine(pub(super) String);

/// Pushes the session's state into the note field: the drawn timeline, the
/// receptors' pressed panels, and every hold's render state. Runs after
/// grading and before the field's animation systems.
fn sync_health_vial(
    session: Res<PlaySession>,
    config: Res<GameConfig>,
    mut vials: Query<&mut HealthVial>,
) {
    for mut vial in &mut vials {
        vial.fill = session.health as f32 / config.player_max_health as f32;
    }
}

fn sync_note_field(
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

    for arrow in session.rows.iter().flat_map(|row| &row.arrows) {
        let Some(hold) = &arrow.hold else { continue };
        let state = match (hold.engaged, hold.result) {
            (_, Some(HoldOutcome::Ok)) => HoldVisualState::Ok,
            (_, Some(HoldOutcome::Ng)) => HoldVisualState::Dropped,
            (false, None) => HoldVisualState::Pending,
            (true, None) if hold.held_now => HoldVisualState::Held,
            (true, None) => HoldVisualState::Released,
        };
        if let Ok(mut visual) = holds.get_mut(arrow.entity)
            && visual.state != state
        {
            visual.state = state;
        }
    }
}

fn update_grade_text(
    time: Res<Time>,
    config: Res<GameConfig>,
    mut graded: MessageReader<RowGraded>,
    mut label: Single<(&mut Text2d, &mut TextColor), With<GradeText>>,
) {
    let (text, color) = &mut *label;
    for message in graded.read() {
        let (value, base) = grade_display(&config, message.outcome);
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

/// The grade text and color for an outcome. Grades opting into timing
/// feedback mark the side of the perfect moment the input fell on: early
/// feedback leads the name, late feedback trails it.
fn grade_display(config: &GameConfig, outcome: RowOutcome) -> (String, Color) {
    let RowOutcome::Hit { error } = outcome else {
        return (config.miss.name.clone(), config.miss.color);
    };
    let Grade::Hit(grade) = config.grade(outcome) else {
        unreachable!("hits always grade into a timed grade");
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

const COMBO_BOUNCE: Seconds = Seconds(0.18);

fn update_combo_text(
    time: Res<Time>,
    mut graded: MessageReader<RowGraded>,
    mut label: Single<(&mut Text2d, &mut Transform, &mut Visibility), With<ComboText>>,
    mut bounce: Local<Seconds>,
    mut last_combo: Local<u32>,
) {
    let (text, transform, visibility) = &mut *label;
    for message in graded.read() {
        if message.combo > *last_combo {
            *bounce = COMBO_BOUNCE;
        }
        *last_combo = message.combo;
        if message.combo == 0 {
            **visibility = Visibility::Hidden;
        } else {
            **visibility = Visibility::Visible;
            text.0 = format!("{} combo", message.combo);
        }
    }
    *bounce = (*bounce - Seconds(time.delta_secs_f64())).max(Seconds::ZERO);
    let scale = 1.0 + 0.22 * (*bounce / COMBO_BOUNCE) as f32;
    if transform.scale.x != scale {
        transform.scale = Vec3::splat(scale);
    }
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
