use super::grade_text::{COMBO_GAP, grade_y};
use super::{ComboText, ForPlayer, HoldOutcome, OffsetOsd, PlaySession, PlaySet, RowGraded};
use crate::core::config::GameConfig;
use crate::core::health_vial::HealthVial;
use crate::core::input::Actions;
use crate::core::note_field::{
    HoldVisual, HoldVisualState, InColumn, InField, NoteField, NoteFieldClock, Receptor,
    visible_world_size,
};
use crate::core::settings::{MachineSettings, PlayerSettings};
use crate::core::units::Seconds;
use bevy::prelude::*;

pub(super) fn plugin(app: &mut App) {
    app.add_message::<OffsetOsdLine>()
        .add_systems(
            Update,
            (sync_note_field, anchor_stage_to_window, sync_health_vials).in_set(PlaySet::Sync),
        )
        .add_systems(
            Update,
            (update_combo_texts, run_offset_osd)
                .chain()
                .in_set(PlaySet::Present),
        );
}

/// A line to flash on the timing-offset OSD.
#[derive(Message)]
pub(super) struct OffsetOsdLine(pub(super) String);

/// Keeps the receptor arrows' top edge the configured screen-edge padding
/// below the window's top edge — the same breathing room the health vials
/// keep to their side — whatever extra world a non-16:9 window reveals
/// and whatever size the arrows were fitted to. Headless renderers have
/// no window and keep the design-canvas default.
fn anchor_stage_to_window(
    config: Res<GameConfig>,
    windows: Query<&Window>,
    fields: Query<&NoteField>,
    mut clock: ResMut<NoteFieldClock>,
) {
    let Ok(window) = windows.single() else { return };
    let Some(arrow_size) = fields.iter().map(|field| field.arrow_size).reduce(f32::max) else {
        return;
    };
    let visible_top = visible_world_size(window).y / 2.0;
    let target_y = visible_top - config.stage.screen_edge_padding - arrow_size / 2.0;
    if clock.target_y != target_y {
        clock.target_y = target_y;
    }
}

fn sync_health_vials(
    session: Res<PlaySession>,
    config: Res<GameConfig>,
    mut vials: Query<(&mut HealthVial, &ForPlayer)>,
) {
    for (mut vial, owner) in &mut vials {
        let Some(stage) = session.stages.iter().find(|stage| stage.player == owner.0) else {
            continue;
        };
        vial.fill = stage.health as f32 / config.player_max_health as f32;
    }
}

/// Pushes the session's state into the note fields: the drawn timeline,
/// each field's receptors' pressed panels, and every hold's render state.
/// Runs after grading and before the fields' animation systems.
fn sync_note_field(
    actions: Actions,
    session: Res<PlaySession>,
    settings: Res<MachineSettings>,
    mut clock: ResMut<NoteFieldClock>,
    fields: Query<&NoteField>,
    mut receptors: Query<(&mut Receptor, &InColumn, &InField)>,
    mut holds: Query<&mut HoldVisual>,
) {
    clock.visible = session.visible_now(&settings.timing);

    for (mut receptor, anchor, in_field) in &mut receptors {
        let Ok(field) = fields.get(in_field.0) else {
            continue;
        };
        let held = actions.pressed(field.step_action(anchor.column));
        if receptor.held != held {
            receptor.held = held;
        }
    }

    for stage in &session.stages {
        for arrow in stage.rows.iter().flat_map(|row| &row.arrows) {
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
}

const COMBO_BOUNCE: Seconds = Seconds(0.18);

fn update_combo_texts(
    time: Res<Time>,
    config: Res<GameConfig>,
    settings: Res<PlayerSettings>,
    windows: Query<&Window>,
    mut graded: MessageReader<RowGraded>,
    mut labels: Query<(
        &ForPlayer,
        &mut ComboText,
        &mut Text2d,
        &mut Transform,
        &mut Visibility,
    )>,
) {
    for message in graded.read() {
        for (owner, mut combo, mut text, _, mut visibility) in &mut labels {
            if owner.0 != message.player {
                continue;
            }
            if message.combo > combo.last_combo {
                combo.bounce = COMBO_BOUNCE;
            }
            combo.last_combo = message.combo;
            if message.combo == 0 {
                *visibility = Visibility::Hidden;
            } else {
                *visibility = Visibility::Visible;
                text.0 = format!("{} combo", message.combo);
            }
        }
    }
    let visible_height = windows
        .single()
        .map(|window| visible_world_size(window).y)
        .ok();
    for (owner, mut combo, _, mut transform, _) in &mut labels {
        combo.bounce = (combo.bounce - Seconds(time.delta_secs_f64())).max(Seconds::ZERO);
        let scale = 1.0 + 0.22 * (combo.bounce / COMBO_BOUNCE) as f32;
        if transform.scale.x != scale {
            transform.scale = Vec3::splat(scale);
        }
        // Tracks under the grade, whose height is the player's grade-position
        // option; headless renderers keep the spawn position.
        if let Some(height) = visible_height {
            let padding = config.stage.screen_edge_padding;
            let y = grade_y(height, padding, settings[owner.0].grade_position) - COMBO_GAP;
            if transform.translation.y != y {
                transform.translation.y = y;
            }
        }
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
