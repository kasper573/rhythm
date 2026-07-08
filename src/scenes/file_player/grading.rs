use super::{HoldOutcome, JudgmentShown, MineOutcome, PlaySession, PlaySet, direction_action};
use crate::core::at;
use crate::core::config::{GameConfig, Judgment, StepOutcome};
use crate::core::font::game_font;
use crate::core::input::Actions;
use crate::core::note_field::{
    FadeOut, GRADED_FADE_SECONDS, TARGET_Y, column_x, spawn_arrow_flash, spawn_mine_explosion,
};
use crate::core::note_skin::ActiveNoteSkin;
use crate::core::scene_flow::SpawnScoped;
use crate::core::settings::Settings;
use crate::scenes::{GameScene, scene_accepts_input};
use bevy::prelude::*;

/// Hold let-go grace: life drains from full to dropped over this long once
/// the panel is released.
const HOLD_GRACE_SECONDS: f32 = 0.25;
/// Roll window: rolls drain constantly and each fresh step refills them.
const ROLL_GRACE_SECONDS: f32 = 0.5;
const HOLD_POPUP_SECONDS: f32 = 0.6;

/// A step (row) is the judgeable unit. Presses bank
/// silently into their arrows; the step resolves into one judgment when its
/// last arrow is banked — graded by that completing press — or expires into
/// a single Miss if any arrow times out, voiding the banked presses.
pub(super) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            bank_step_inputs.run_if(scene_accepts_input),
            expire_missed_steps,
            update_holds,
            update_mines,
        )
            .chain()
            .in_set(PlaySet::Judge),
    );
}

/// Banks ¤Left¤/¤Down¤/¤Up¤/¤Right¤ presses into the nearest unresolved
/// step with an unbanked arrow in that column, and resolves steps whose
/// last arrow just arrived. Inputs that hit no grading window are no-ops.
fn bank_step_inputs(
    actions: Actions,
    settings: Res<Settings>,
    config: Res<GameConfig>,
    skin: Res<ActiveNoteSkin>,
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
            .steps
            .iter()
            .enumerate()
            .filter(|(_, step)| {
                step.outcome.is_none()
                    && (step.time - input_time).abs().0 <= widest.0
                    && step
                        .arrows
                        .iter()
                        .any(|arrow| arrow.column == column && arrow.error.is_none())
            })
            .min_by(|(_, a), (_, b)| {
                (a.time - input_time)
                    .abs()
                    .0
                    .total_cmp(&(b.time - input_time).abs().0)
            })
            .map(|(index, _)| index);
        let Some(index) = candidate else { continue };

        let error = session.steps[index].time - input_time;
        if session.autosync {
            session.autosync_samples.push(error);
        }
        let arrow = session.steps[index]
            .arrows
            .iter_mut()
            .find(|arrow| arrow.column == column && arrow.error.is_none())
            .expect("candidate steps have an unbanked arrow in this column");
        arrow.error = Some(error);
        if let Some(hold) = &mut arrow.hold {
            hold.engaged = true;
            hold.life = 1.0;
        }

        if session.steps[index].complete() {
            resolve_step(
                &mut session,
                &config,
                &skin,
                index,
                &mut judgments,
                &mut commands,
            );
        }
    }
}

/// The step's single payout once every arrow is banked. The completing
/// press decides the grade: the chronologically last one, which is the
/// smallest signed error since late presses go negative.
fn resolve_step(
    session: &mut PlaySession,
    config: &GameConfig,
    skin: &ActiveNoteSkin,
    index: usize,
    judgments: &mut MessageWriter<JudgmentShown>,
    commands: &mut Commands,
) {
    let error = session.steps[index]
        .arrows
        .iter()
        .filter_map(|arrow| arrow.error)
        .reduce(|a, b| if a.0 <= b.0 { a } else { b })
        .expect("resolved steps have every arrow banked");
    apply_outcome(
        session,
        config,
        index,
        StepOutcome::Hit { error },
        judgments,
    );

    // The vanish: grades with an arrow flash play it at every arrow of
    // the step and the tap arrows disappear on the spot. Lesser grades
    // leave the arrows scrolling on, judged but visible.
    let Judgment::Grade(grade) = config.judge(StepOutcome::Hit { error }) else {
        return;
    };
    let Some(color) = config.grades[grade.0].arrow_flash else {
        return;
    };
    let bright = session.combo >= config.bright_arrow_flash_combo;
    for arrow_index in 0..session.steps[index].arrows.len() {
        let arrow = &session.steps[index].arrows[arrow_index];
        let column = arrow.column;
        let entity = arrow.entity;
        let is_hold = arrow.hold.is_some();
        let flash = spawn_arrow_flash(commands, skin, column, color, bright);
        commands
            .entity(flash)
            .insert(DespawnOnExit(GameScene::FilePlayer));
        if !is_hold {
            commands.entity(entity).despawn();
        }
    }
}

/// Steps expire into a single Miss once they scroll further past the
/// player than the widest grading window with any arrow still unbanked —
/// banked presses on the other arrows are voided. A hold whose own head
/// was never stepped can never be caught, so it drops immediately; a hold
/// whose head was banked fights on even though its step missed.
fn expire_missed_steps(
    settings: Res<Settings>,
    config: Res<GameConfig>,
    mut session: ResMut<PlaySession>,
    mut judgments: MessageWriter<JudgmentShown>,
    mut commands: Commands,
) {
    let expire_before = session.judged_now(&settings.timing) - config.widest_window();
    while session.expire_cursor < session.steps.len() {
        let cursor = session.expire_cursor;
        let step = &session.steps[cursor];
        if step.time.0 >= expire_before.0 {
            break;
        }
        if step.outcome.is_none() {
            apply_outcome(
                &mut session,
                &config,
                cursor,
                StepOutcome::Miss,
                &mut judgments,
            );
            for arrow in &mut session.steps[cursor].arrows {
                let Some(hold) = &mut arrow.hold else {
                    continue;
                };
                if arrow.error.is_none() {
                    hold.result = Some(HoldOutcome::Ng);
                    spawn_hold_popup(&mut commands, arrow.column, HoldOutcome::Ng);
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
fn update_holds(
    actions: Actions,
    time: Res<Time>,
    settings: Res<Settings>,
    mut session: ResMut<PlaySession>,
    mut commands: Commands,
) {
    let now = session.judged_now(&settings.timing);
    let delta = time.delta_secs();
    for arrow in session
        .steps
        .iter_mut()
        .flat_map(|step| step.arrows.iter_mut())
    {
        let Some(hold) = &mut arrow.hold else {
            continue;
        };
        if hold.result.is_some() || !hold.engaged {
            continue;
        }

        let action = direction_action(arrow.column);
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
                .entity(arrow.entity)
                .insert(FadeOut::over(GRADED_FADE_SECONDS));
            spawn_hold_popup(&mut commands, arrow.column, HoldOutcome::Ok);
        } else if hold.life <= 0.0 {
            hold.result = Some(HoldOutcome::Ng);
            spawn_hold_popup(&mut commands, arrow.column, HoldOutcome::Ng);
        }
    }
}

/// A mine explodes if its panel is being held as the mine crosses the
/// receptors; otherwise it passes by harmlessly.
fn update_mines(
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
    step_index: usize,
    outcome: StepOutcome,
    judgments: &mut MessageWriter<JudgmentShown>,
) {
    session.steps[step_index].outcome = Some(outcome);
    session.judged_count += 1;
    if config.breaks_combo(config.judge(outcome)) {
        session.combo = 0;
    } else {
        // Every arrow of the step feeds the combo, so a clean jump pays +2.
        session.combo += session.steps[step_index].arrows.len() as u32;
        session.max_combo = session.max_combo.max(session.combo);
    }
    judgments.write(JudgmentShown {
        outcome,
        combo: session.combo,
    });
}

fn spawn_hold_popup(commands: &mut Commands, column: usize, outcome: HoldOutcome) {
    let (label, color) = match outcome {
        HoldOutcome::Ok => ("OK", Color::srgb(0.45, 0.95, 0.5)),
        HoldOutcome::Ng => ("NG", Color::srgb(0.95, 0.35, 0.35)),
    };
    commands
        .spawn_scoped(
            GameScene::FilePlayer,
            bsn! {
                game_font(30.0)
                Text2d({label.to_string()})
                TextColor({color})
                at(column_x(column), TARGET_Y - 54.0, 21.0)
            },
        )
        .insert(FadeOut::growing(HOLD_POPUP_SECONDS, 0.25));
}
