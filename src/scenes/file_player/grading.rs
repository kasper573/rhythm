use super::{HoldOutcome, JudgmentShown, MineOutcome, PlaySession, direction_action};
use crate::core::config::{GameConfig, StepOutcome};
use crate::core::font::GameFont;
use crate::core::input::Actions;
use crate::core::note_field::{ArrowFade, Popup, TARGET_Y, column_x, spawn_mine_explosion};
use crate::core::note_skin::ActiveNoteSkin;
use crate::core::settings::Settings;
use crate::scenes::GameScene;
use bevy::prelude::*;

const GRADED_FADE_SECONDS: f32 = 0.05;
/// Hold let-go grace: life drains from full to dropped over this long once
/// the panel is released.
const HOLD_GRACE_SECONDS: f32 = 0.25;
/// Roll window: rolls drain constantly and each fresh step refills them.
const ROLL_GRACE_SECONDS: f32 = 0.5;

/// Grades ¤Left¤/¤Down¤/¤Up¤/¤Right¤ presses against the nearest ungraded
/// note in that column. Inputs that hit no grading window are harmless no-ops.
pub(super) fn grade_step_inputs(
    actions: Actions,
    settings: Res<Settings>,
    config: Res<GameConfig>,
    mut session: ResMut<PlaySession>,
    mut judgments: MessageWriter<JudgmentShown>,
    mut commands: Commands,
) {
    let widest = config.widest_window();
    let input_time = session.judged_now(&settings.timing);

    for column in 0..4 {
        if !actions.just_pressed(direction_action(column)) {
            continue;
        }
        let candidate = session
            .notes
            .iter()
            .enumerate()
            .filter(|(_, note)| {
                note.outcome.is_none()
                    && note.column == column
                    && (note.time - input_time).abs().0 <= widest.0
            })
            .min_by(|(_, a), (_, b)| {
                (a.time - input_time)
                    .abs()
                    .0
                    .total_cmp(&(b.time - input_time).abs().0)
            })
            .map(|(index, _)| index);
        let Some(index) = candidate else { continue };

        let error = session.notes[index].time - input_time;
        apply_outcome(
            &mut session,
            &config,
            index,
            StepOutcome::Hit { error },
            &mut judgments,
        );
        let note = &mut session.notes[index];
        match &mut note.hold {
            Some(hold) => {
                hold.engaged = true;
                hold.life = 1.0;
            }
            None => {
                commands
                    .entity(note.entity)
                    .insert(ArrowFade::over(GRADED_FADE_SECONDS));
            }
        }
    }
}

/// Notes expire into the always-existing Miss grade once they scroll further
/// past the player than the widest grading window. A hold whose head was
/// missed can never be caught, so it drops immediately.
pub(super) fn expire_missed_notes(
    settings: Res<Settings>,
    config: Res<GameConfig>,
    font: Res<GameFont>,
    mut session: ResMut<PlaySession>,
    mut judgments: MessageWriter<JudgmentShown>,
    mut commands: Commands,
) {
    let expire_before = session.judged_now(&settings.timing) - config.widest_window();
    while session.expire_cursor < session.notes.len() {
        let cursor = session.expire_cursor;
        let note = &session.notes[cursor];
        if note.time.0 >= expire_before.0 {
            break;
        }
        if note.outcome.is_none() {
            apply_outcome(
                &mut session,
                &config,
                cursor,
                StepOutcome::Miss,
                &mut judgments,
            );
            let note = &mut session.notes[cursor];
            match &mut note.hold {
                Some(hold) => {
                    hold.result = Some(HoldOutcome::Ng);
                    spawn_hold_popup(&mut commands, &font, note.column, HoldOutcome::Ng);
                }
                None => {
                    commands
                        .entity(note.entity)
                        .insert(ArrowFade::over(GRADED_FADE_SECONDS));
                }
            }
        }
        session.expire_cursor += 1;
    }
}

/// Runs every engaged hold's life: holds refill to full while the panel is
/// down and drain over the grace window otherwise; rolls drain constantly
/// and refill on fresh steps. Life zero drops the hold (NG); reaching the
/// tail with life left keeps it (OK).
pub(super) fn update_holds(
    actions: Actions,
    time: Res<Time>,
    settings: Res<Settings>,
    font: Res<GameFont>,
    mut session: ResMut<PlaySession>,
    mut commands: Commands,
) {
    let now = session.judged_now(&settings.timing);
    let delta = time.delta_secs();
    for index in 0..session.notes.len() {
        let note = &mut session.notes[index];
        let column = note.column;
        let entity = note.entity;
        let Some(hold) = &mut note.hold else { continue };
        if hold.result.is_some() || !hold.engaged {
            continue;
        }

        let action = direction_action(column);
        if hold.roll {
            if actions.just_pressed(action) {
                hold.life = 1.0;
            }
            hold.held_now = actions.pressed(action);
            hold.life -= delta / ROLL_GRACE_SECONDS;
        } else if actions.pressed(action) {
            hold.held_now = true;
            hold.life = 1.0;
        } else {
            hold.held_now = false;
            hold.life -= delta / HOLD_GRACE_SECONDS;
        }
        hold.life = hold.life.clamp(0.0, 1.0);

        if now.0 >= hold.end.0 && hold.life > 0.0 {
            hold.result = Some(HoldOutcome::Ok);
            commands
                .entity(entity)
                .insert(ArrowFade::over(GRADED_FADE_SECONDS));
            spawn_hold_popup(&mut commands, &font, column, HoldOutcome::Ok);
        } else if hold.life <= 0.0 {
            hold.result = Some(HoldOutcome::Ng);
            spawn_hold_popup(&mut commands, &font, column, HoldOutcome::Ng);
        }
    }
}

/// A mine explodes if its panel is being held as the mine crosses the
/// receptors; otherwise it passes by harmlessly.
pub(super) fn update_mines(
    actions: Actions,
    settings: Res<Settings>,
    skin: Res<ActiveNoteSkin>,
    mut session: ResMut<PlaySession>,
    mut commands: Commands,
) {
    let now = session.judged_now(&settings.timing);
    for mine in &mut session.mines {
        if mine.outcome.is_some() || mine.time.0 > now.0 {
            continue;
        }
        if !actions.pressed(direction_action(mine.column)) {
            mine.outcome = Some(MineOutcome::Avoided);
            continue;
        }
        mine.outcome = Some(MineOutcome::Hit);
        commands.entity(mine.entity).despawn();
        let explosion = spawn_mine_explosion(&mut commands, &skin, mine.column);
        commands
            .entity(explosion)
            .insert(DespawnOnExit(GameScene::FilePlayer));
    }
}

fn apply_outcome(
    session: &mut PlaySession,
    config: &GameConfig,
    note_index: usize,
    outcome: StepOutcome,
    judgments: &mut MessageWriter<JudgmentShown>,
) {
    session.notes[note_index].outcome = Some(outcome);
    session.judged_count += 1;
    if session.autosync
        && let StepOutcome::Hit { error } = outcome
    {
        session.autosync_samples.push(error);
    }
    if config.breaks_combo(config.judge(outcome)) {
        session.combo = 0;
    } else {
        session.combo += 1;
        session.max_combo = session.max_combo.max(session.combo);
    }
    judgments.write(JudgmentShown {
        outcome,
        combo: session.combo,
    });
}

fn spawn_hold_popup(commands: &mut Commands, font: &GameFont, column: usize, outcome: HoldOutcome) {
    let (label, color) = match outcome {
        HoldOutcome::Ok => ("OK", Color::srgb(0.45, 0.95, 0.5)),
        HoldOutcome::Ng => ("NG", Color::srgb(0.95, 0.35, 0.35)),
    };
    commands.spawn((
        DespawnOnExit(GameScene::FilePlayer),
        Popup::over(0.6),
        Text2d::new(label),
        font.sized(30.0),
        TextColor(color),
        Transform::from_xyz(column_x(column), TARGET_Y - 54.0, 21.0),
    ));
}
