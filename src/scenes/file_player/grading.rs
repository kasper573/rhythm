use super::{
    HoldOutcome, MineOutcome, PlayInput, PlaySession, PlaySet, PlayTime, RowGraded, Stage,
};
use crate::core::config::{FixedGradeDef, GameConfig, Grade, RowOutcome};
use crate::core::font::game_font;
use crate::core::note_field::{
    FadeOut, HOLD_OK_FADE_SECONDS, LaneEffects, NoteField, NoteFieldClock,
};
use crate::core::note_skin::ActiveNoteSkins;
use crate::core::scene_flow::SpawnScoped;
use crate::core::{OVERLAY_LAYER, at};
use crate::scenes::{GameScene, scene_accepts_input};
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

const HOLD_POPUP_SECONDS: f32 = 0.6;

/// The row is the unit the game grades, independently per stage. Presses
/// bank silently into their arrows; the row resolves into one grade when
/// its last arrow is banked — decided by that completing press — or
/// expires into a single Miss if any arrow times out, voiding the banked
/// presses. Failed stages stop grading entirely.
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

/// The read-only stage state every grading system judges against; each
/// stage's geometry and input mapping come from its [`NoteField`]. Presses
/// come solely from the [`PlayInput`] port an adapter fills, so the same
/// grading runs off the keyboard or the preview's autoplay with no branch.
#[derive(SystemParam)]
struct GradingContext<'w, 's> {
    play_time: Res<'w, PlayTime>,
    config: Res<'w, GameConfig>,
    skins: Res<'w, ActiveNoteSkins>,
    asset_server: Res<'w, AssetServer>,
    clock: Res<'w, NoteFieldClock>,
    input: Res<'w, PlayInput>,
    fields: Query<'w, 's, &'static NoteField>,
}

impl GradingContext<'_, '_> {
    /// Whether `column` of `field` is held this frame.
    fn held(&self, field: &NoteField, column: usize) -> bool {
        self.input.held(field.step_action(column))
    }

    /// Whether `column` of `field` went down this frame.
    fn struck(&self, field: &NoteField, column: usize) -> bool {
        self.input.struck(field.step_action(column))
    }
}

/// Banks step presses into the nearest unresolved row with an unbanked
/// arrow in that column, per stage, and resolves rows whose last arrow
/// just arrived. Inputs that hit no grading window are no-ops.
fn bank_row_inputs(
    ctx: GradingContext,
    mut session: ResMut<PlaySession>,
    mut graded: MessageWriter<RowGraded>,
    mut commands: Commands,
) {
    let widest = ctx.config.widest_window();
    let input_time = ctx.play_time.graded;

    let session = &mut *session;
    for stage in &mut session.stages {
        let Ok(field) = ctx.fields.get(stage.field) else {
            continue;
        };
        if stage.failed {
            continue;
        }
        for column in 0..field.columns {
            if !ctx.struck(field, column) {
                continue;
            }
            let candidate = stage
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

            let error = stage.rows[index].time - input_time;
            if session.autosync.enabled {
                session.autosync.samples.push(error);
            }
            let arrow = stage.rows[index]
                .arrows
                .iter_mut()
                .find(|arrow| arrow.column == column && arrow.error.is_none())
                .expect("candidate rows have an unbanked arrow in this column");
            arrow.error = Some(error);
            if let Some(hold) = &mut arrow.hold {
                hold.engaged = true;
                hold.life = 1.0;
            }

            if stage.rows[index].complete() {
                resolve_row(stage, field, &ctx, index, &mut graded, &mut commands);
            }
        }
    }
}

/// Resolves the row into its single grade once every arrow is banked. The
/// completing press decides: the chronologically last one, which is the
/// smallest signed error since late presses go negative.
fn resolve_row(
    stage: &mut Stage,
    field: &NoteField,
    ctx: &GradingContext,
    index: usize,
    graded: &mut MessageWriter<RowGraded>,
    commands: &mut Commands,
) {
    let error = stage.rows[index]
        .arrows
        .iter()
        .filter_map(|arrow| arrow.error)
        .reduce(|a, b| if a.0 <= b.0 { a } else { b })
        .expect("resolved rows have every arrow banked");
    apply_outcome(stage, &ctx.config, index, RowOutcome::Hit { error }, graded);

    // The vanish: grades with an arrow flash play it at every arrow of
    // the row and the tap arrows disappear on the spot. Lesser grades
    // leave the arrows scrolling on, graded but visible.
    let Grade::Hit(grade) = ctx.config.grade(RowOutcome::Hit { error }) else {
        return;
    };
    let Some(color) = ctx.config.grading.dynamic[grade.0].arrow_flash else {
        return;
    };
    let bright = stage.combo >= ctx.config.bright_arrow_flash_combo;
    let mut effects = LaneEffects {
        commands,
        asset_server: &ctx.asset_server,
        skin: ctx.skins.get(field.player),
        layout: field,
    };
    for arrow in &stage.rows[index].arrows {
        let flash = effects.arrow_flash(arrow.column, ctx.clock.target_y, color, bright);
        effects
            .commands
            .entity(flash)
            .insert(DespawnOnExit(GameScene::FilePlayer));
        if arrow.hold.is_none() {
            effects.commands.entity(arrow.entity).despawn();
        }
    }
}

/// Rows expire into a single Miss once they scroll further past the
/// player than the widest grading window with any arrow still unbanked —
/// banked presses on the other arrows are voided. A hold whose own head
/// was never stepped can never be caught, so it drops immediately; a hold
/// whose head was banked fights on even though its row missed.
fn expire_missed_rows(
    ctx: GradingContext,
    mut session: ResMut<PlaySession>,
    mut graded: MessageWriter<RowGraded>,
    mut commands: Commands,
) {
    let GradingContext {
        config,
        clock,
        fields,
        ..
    } = &ctx;
    let expire_before = ctx.play_time.graded - config.widest_window();
    for stage in &mut session.stages {
        let Ok(field) = fields.get(stage.field) else {
            continue;
        };
        if stage.failed {
            continue;
        }
        while stage.expire_cursor < stage.rows.len() {
            let cursor = stage.expire_cursor;
            let row = &stage.rows[cursor];
            if row.time.0 >= expire_before.0 {
                break;
            }
            if row.outcome.is_none() {
                apply_outcome(stage, config, cursor, RowOutcome::Miss, &mut graded);
                let Stage { rows, health, .. } = stage;
                for arrow in &mut rows[cursor].arrows {
                    let Some(hold) = &mut arrow.hold else {
                        continue;
                    };
                    if arrow.error.is_none() {
                        hold.result = Some(HoldOutcome::Ng);
                        apply_hold_health(health, config, HoldOutcome::Ng);
                        spawn_hold_popup(
                            &mut commands,
                            config,
                            field,
                            arrow.column,
                            clock.target_y,
                            HoldOutcome::Ng,
                        );
                    }
                }
            }
            stage.expire_cursor += 1;
        }
    }
}

/// Runs every engaged hold's life: holds refill to full while the panel is
/// down and drain over the grace window otherwise; rolls drain constantly
/// and refill on fresh steps. Life zero drops the hold (NG); reaching the
/// tail with life left keeps it (OK).
fn update_holds(
    time: Res<Time>,
    ctx: GradingContext,
    mut session: ResMut<PlaySession>,
    mut commands: Commands,
) {
    let GradingContext {
        config,
        clock,
        fields,
        ..
    } = &ctx;
    let now = ctx.play_time.graded;
    let delta = time.delta_secs();
    for stage in &mut session.stages {
        let Ok(field) = fields.get(stage.field) else {
            continue;
        };
        if stage.failed {
            continue;
        }
        let Stage { rows, health, .. } = stage;
        for arrow in rows.iter_mut().flat_map(|row| row.arrows.iter_mut()) {
            let Some(hold) = &mut arrow.hold else {
                continue;
            };
            if hold.result.is_some() || !hold.engaged {
                continue;
            }

            if hold.roll {
                if ctx.struck(field, arrow.column) {
                    hold.life = 1.0;
                }
                hold.held_now = ctx.held(field, arrow.column);
                hold.life -= delta / config.grading.roll_grace_seconds;
            } else if ctx.held(field, arrow.column) {
                hold.held_now = true;
                hold.life = 1.0;
            } else {
                hold.held_now = false;
                hold.life -= delta / config.grading.hold_grace_seconds;
            }
            hold.life = hold.life.clamp(0.0, 1.0);

            if now.0 >= hold.end.0 && hold.life > 0.0 {
                hold.result = Some(HoldOutcome::Ok);
                apply_hold_health(health, config, HoldOutcome::Ok);
                commands
                    .entity(arrow.entity)
                    .insert(FadeOut::over(HOLD_OK_FADE_SECONDS));
                spawn_hold_popup(
                    &mut commands,
                    config,
                    field,
                    arrow.column,
                    clock.target_y,
                    HoldOutcome::Ok,
                );
            } else if hold.life <= 0.0 {
                hold.result = Some(HoldOutcome::Ng);
                apply_hold_health(health, config, HoldOutcome::Ng);
                spawn_hold_popup(
                    &mut commands,
                    config,
                    field,
                    arrow.column,
                    clock.target_y,
                    HoldOutcome::Ng,
                );
            }
        }
    }
}

/// A mine explodes if its panel is being held as the mine crosses the
/// receptors; otherwise it passes by harmlessly.
fn update_mines(ctx: GradingContext, mut session: ResMut<PlaySession>, mut commands: Commands) {
    let GradingContext {
        skins,
        asset_server,
        clock,
        fields,
        ..
    } = &ctx;
    let now = ctx.play_time.graded;
    for stage in &mut session.stages {
        let Ok(field) = fields.get(stage.field) else {
            continue;
        };
        if stage.failed {
            continue;
        }
        for mine in &mut stage.mines {
            if mine.outcome.is_some() || mine.time.0 > now.0 {
                continue;
            }
            if !ctx.held(field, mine.column) {
                mine.outcome = Some(MineOutcome::Avoided);
                continue;
            }
            mine.outcome = Some(MineOutcome::Exploded);
            commands.entity(mine.entity).despawn();
            let mut effects = LaneEffects {
                commands: &mut commands,
                asset_server,
                skin: skins.get(field.player),
                layout: field,
            };
            let explosion = effects.mine_explosion(mine.column, clock.target_y);
            commands
                .entity(explosion)
                .insert(DespawnOnExit(GameScene::FilePlayer));
        }
    }
}

fn apply_outcome(
    stage: &mut Stage,
    config: &GameConfig,
    row_index: usize,
    outcome: RowOutcome,
    graded: &mut MessageWriter<RowGraded>,
) {
    stage.rows[row_index].outcome = Some(outcome);
    stage.graded_count += 1;
    let grade = config.grade(outcome);
    stage.health = stage
        .health
        .saturating_add_signed(config.health_offset(grade))
        .min(config.player_max_health);
    if config.breaks_combo(grade) {
        stage.combo = 0;
    } else {
        // Every arrow of the row feeds the combo, so a clean jump pays +2.
        stage.combo += stage.rows[row_index].arrows.len() as u32;
        stage.max_combo = stage.max_combo.max(stage.combo);
    }
    graded.write(RowGraded {
        player: stage.player,
        outcome,
        combo: stage.combo,
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
    field: &NoteField,
    column: usize,
    target_y: f32,
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
                at(field.column_x(column), target_y - 54.0, 21.0)
            },
        )
        .insert((
            FadeOut::growing(HOLD_POPUP_SECONDS, 0.25),
            RenderLayers::layer(OVERLAY_LAYER),
        ));
}
