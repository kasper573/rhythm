use super::{HoldOutcome, MineOutcome, PlaySession, PlaySet, RowGraded, direction_action};
use crate::core::at;
use crate::core::config::{FixedGradeDef, GameConfig, Grade, RowOutcome};
use crate::core::font::game_font;
use crate::core::input::Actions;
use crate::core::note_field::{
    FadeOut, HOLD_OK_FADE_SECONDS, TARGET_Y, column_x, spawn_arrow_flash, spawn_mine_explosion,
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

/// The row is the unit the game grades. Presses bank silently into their
/// arrows; the row resolves into one grade when its last arrow is banked —
/// decided by that completing press — or expires into a single Miss if any
/// arrow times out, voiding the banked presses.
pub(super) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            bank_row_inputs.run_if(scene_accepts_input),
            expire_missed_rows,
            update_holds,
            update_mines,
        )
            .chain()
            .in_set(PlaySet::Grade),
    );
}

/// Banks ¤Left¤/¤Down¤/¤Up¤/¤Right¤ presses into the nearest unresolved
/// row with an unbanked arrow in that column, and resolves rows whose
/// last arrow just arrived. Inputs that hit no grading window are no-ops.
fn bank_row_inputs(
    actions: Actions,
    settings: Res<Settings>,
    config: Res<GameConfig>,
    skin: Res<ActiveNoteSkin>,
    mut session: ResMut<PlaySession>,
    mut graded: MessageWriter<RowGraded>,
    mut commands: Commands,
) {
    let widest = config.widest_window();
    let input_time = session.graded_now(&settings.timing);

    for column in 0..4 {
        if !actions.just_pressed(direction_action(column)) {
            continue;
        }
        let candidate = session
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                row.outcome.is_none()
                    && (row.time - input_time).abs().0 <= widest.0
                    && row
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

        let error = session.rows[index].time - input_time;
        if session.autosync.enabled {
            session.autosync.samples.push(error);
        }
        let arrow = session.rows[index]
            .arrows
            .iter_mut()
            .find(|arrow| arrow.column == column && arrow.error.is_none())
            .expect("candidate rows have an unbanked arrow in this column");
        arrow.error = Some(error);
        if let Some(hold) = &mut arrow.hold {
            hold.engaged = true;
            hold.life = 1.0;
        }

        if session.rows[index].complete() {
            resolve_row(
                &mut session,
                &config,
                &skin,
                index,
                &mut graded,
                &mut commands,
            );
        }
    }
}

/// Resolves the row into its single grade once every arrow is banked. The
/// completing press decides: the chronologically last one, which is the
/// smallest signed error since late presses go negative.
fn resolve_row(
    session: &mut PlaySession,
    config: &GameConfig,
    skin: &ActiveNoteSkin,
    index: usize,
    graded: &mut MessageWriter<RowGraded>,
    commands: &mut Commands,
) {
    let error = session.rows[index]
        .arrows
        .iter()
        .filter_map(|arrow| arrow.error)
        .reduce(|a, b| if a.0 <= b.0 { a } else { b })
        .expect("resolved rows have every arrow banked");
    apply_outcome(session, config, index, RowOutcome::Hit { error }, graded);

    // The vanish: grades with an arrow flash play it at every arrow of
    // the row and the tap arrows disappear on the spot. Lesser grades
    // leave the arrows scrolling on, graded but visible.
    let Grade::Hit(grade) = config.grade(RowOutcome::Hit { error }) else {
        return;
    };
    let Some(color) = config.grading.dynamic[grade.0].arrow_flash else {
        return;
    };
    let bright = session.combo >= config.bright_arrow_flash_combo;
    for arrow in &session.rows[index].arrows {
        let flash = spawn_arrow_flash(commands, skin, arrow.column, color, bright);
        commands
            .entity(flash)
            .insert(DespawnOnExit(GameScene::FilePlayer));
        if arrow.hold.is_none() {
            commands.entity(arrow.entity).despawn();
        }
    }
}

/// Rows expire into a single Miss once they scroll further past the
/// player than the widest grading window with any arrow still unbanked —
/// banked presses on the other arrows are voided. A hold whose own head
/// was never stepped can never be caught, so it drops immediately; a hold
/// whose head was banked fights on even though its row missed.
fn expire_missed_rows(
    settings: Res<Settings>,
    config: Res<GameConfig>,
    mut session: ResMut<PlaySession>,
    mut graded: MessageWriter<RowGraded>,
    mut commands: Commands,
) {
    let expire_before = session.graded_now(&settings.timing) - config.widest_window();
    while session.expire_cursor < session.rows.len() {
        let cursor = session.expire_cursor;
        let row = &session.rows[cursor];
        if row.time.0 >= expire_before.0 {
            break;
        }
        if row.outcome.is_none() {
            apply_outcome(&mut session, &config, cursor, RowOutcome::Miss, &mut graded);
            let session = &mut *session;
            for arrow in &mut session.rows[cursor].arrows {
                let Some(hold) = &mut arrow.hold else {
                    continue;
                };
                if arrow.error.is_none() {
                    hold.result = Some(HoldOutcome::Ng);
                    apply_hold_health(&mut session.health, &config, HoldOutcome::Ng);
                    spawn_hold_popup(&mut commands, &config, arrow.column, HoldOutcome::Ng);
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
    config: Res<GameConfig>,
    mut session: ResMut<PlaySession>,
    mut commands: Commands,
) {
    let now = session.graded_now(&settings.timing);
    let delta = time.delta_secs();
    let session = &mut *session;
    for arrow in session
        .rows
        .iter_mut()
        .flat_map(|row| row.arrows.iter_mut())
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
            apply_hold_health(&mut session.health, &config, HoldOutcome::Ok);
            commands
                .entity(arrow.entity)
                .insert(FadeOut::over(HOLD_OK_FADE_SECONDS));
            spawn_hold_popup(&mut commands, &config, arrow.column, HoldOutcome::Ok);
        } else if hold.life <= 0.0 {
            hold.result = Some(HoldOutcome::Ng);
            apply_hold_health(&mut session.health, &config, HoldOutcome::Ng);
            spawn_hold_popup(&mut commands, &config, arrow.column, HoldOutcome::Ng);
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
    let now = session.graded_now(&settings.timing);
    for mine in &mut session.mines {
        if mine.outcome.is_some() || mine.time.0 > now.0 {
            continue;
        }
        if !actions.pressed(direction_action(mine.column)) {
            mine.outcome = Some(MineOutcome::Avoided);
            continue;
        }
        mine.outcome = Some(MineOutcome::Exploded);
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
    row_index: usize,
    outcome: RowOutcome,
    graded: &mut MessageWriter<RowGraded>,
) {
    session.rows[row_index].outcome = Some(outcome);
    session.graded_count += 1;
    let grade = config.grade(outcome);
    session.health = session
        .health
        .saturating_add_signed(config.health_offset(grade))
        .min(config.player_max_health);
    if config.breaks_combo(grade) {
        session.combo = 0;
    } else {
        // Every arrow of the row feeds the combo, so a clean jump pays +2.
        session.combo += session.rows[row_index].arrows.len() as u32;
        session.max_combo = session.max_combo.max(session.combo);
    }
    graded.write(RowGraded {
        outcome,
        combo: session.combo,
    });
}

fn hold_def(config: &GameConfig, outcome: HoldOutcome) -> &FixedGradeDef {
    match outcome {
        HoldOutcome::Ok => &config.grading.fixed.ok,
        HoldOutcome::Ng => &config.grading.fixed.ng,
    }
}

/// Holds pay their fixed grade's health offset the moment they resolve.
fn apply_hold_health(health: &mut u32, config: &GameConfig, outcome: HoldOutcome) {
    *health = health
        .saturating_add_signed(hold_def(config, outcome).health_offset)
        .min(config.player_max_health);
}

fn spawn_hold_popup(
    commands: &mut Commands,
    config: &GameConfig,
    column: usize,
    outcome: HoldOutcome,
) {
    let def = hold_def(config, outcome);
    let label = def.name.clone();
    let color = def.color;
    commands
        .spawn_scoped(
            GameScene::FilePlayer,
            bsn! {
                game_font(30.0)
                Text2d({label})
                TextColor({color})
                at(column_x(column), TARGET_Y - 54.0, 21.0)
            },
        )
        .insert(FadeOut::growing(HOLD_POPUP_SECONDS, 0.25));
}
