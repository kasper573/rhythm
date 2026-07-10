//! The grading pass. The row is the unit the engine grades, independently
//! per stage. Presses bank silently into their arrows; the row resolves
//! into one grade when its last arrow is banked — decided by that
//! completing press — or expires into a single Miss if any arrow times
//! out, voiding the banked presses. A stage that drains to zero health
//! fails on the spot and stops grading, its field fading away, while any
//! surviving stage plays on.

use super::note_field::HOLD_OK_FADE_SECONDS;
use super::{FAIL_FADE_SECONDS, HoldOutcome, MineOutcome, Stage, StageEvent, StepfilePlayer};
use crate::core::config::{GameConfig, Grade, RowOutcome, config};

impl StepfilePlayer {
    /// One frame of grading, in the fixed order the rules depend on;
    /// returns what happened for the presentation layer to apply.
    pub(super) fn run_grading(&mut self, delta: f64) -> Vec<StageEvent> {
        let mut events = Vec::new();
        self.bank_row_inputs(&mut events);
        self.expire_missed_rows(&mut events);
        self.update_holds(delta, &mut events);
        self.update_mines();
        self.fail_drained_stages(&mut events);
        events
    }

    /// Banks step presses into the nearest unresolved row with an unbanked
    /// arrow in that column, per stage, and resolves rows whose last arrow
    /// just arrived. Inputs that hit no grading window are no-ops.
    fn bank_row_inputs(&mut self, events: &mut Vec<StageEvent>) {
        let config = config();
        let widest = config.widest_window();
        let input_time = self.graded_now;
        let target_y = self.target_y;

        for (stage, rig) in self.stages.iter_mut().zip(&mut self.rigs) {
            if stage.failed {
                continue;
            }
            for column in 0..rig.layout.columns {
                if !self.input.struck(rig.layout.step_action(column)) {
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
                events.push(StageEvent::PressBanked { error });
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
                    // The completing press decides the row: the
                    // chronologically last one, which is the smallest
                    // signed error since late presses go negative.
                    let error = stage.rows[index]
                        .arrows
                        .iter()
                        .filter_map(|arrow| arrow.error)
                        .reduce(|a, b| if a.0 <= b.0 { a } else { b })
                        .expect("resolved rows have every arrow banked");
                    let outcome = RowOutcome::Hit { error };
                    apply_outcome(stage, config, index, outcome, events);

                    // The vanish: grades with an arrow flash play it at
                    // every arrow of the row and the tap arrows disappear
                    // on the spot. Lesser grades leave the arrows
                    // scrolling on, graded but visible.
                    let Grade::Hit(grade) = config.grade(outcome) else {
                        continue;
                    };
                    let Some(color) = config.grading.dynamic[grade.0].arrow_flash else {
                        continue;
                    };
                    let bright = stage.combo >= config.bright_arrow_flash_combo;
                    for arrow in &stage.rows[index].arrows {
                        rig.arrow_flash(arrow.column, target_y, color, bright);
                        if arrow.hold.is_none() {
                            rig.vanish_note(arrow.note);
                        }
                    }
                }
            }
        }
    }

    /// Rows expire into a single Miss once they scroll further past the
    /// player than the widest grading window with any arrow still unbanked —
    /// banked presses on the other arrows are voided. A hold whose own head
    /// was never stepped can never be caught, so it drops immediately; a hold
    /// whose head was banked fights on even though its row missed.
    fn expire_missed_rows(&mut self, events: &mut Vec<StageEvent>) {
        let config = config();
        let expire_before = self.graded_now - config.widest_window();
        let mut popups = Vec::new();
        for (stage, rig) in self.stages.iter_mut().zip(&self.rigs) {
            if stage.failed {
                continue;
            }
            while stage.expire_cursor < stage.rows.len() {
                let cursor = stage.expire_cursor;
                if stage.rows[cursor].time.0 >= expire_before.0 {
                    break;
                }
                if stage.rows[cursor].outcome.is_none() {
                    apply_outcome(stage, config, cursor, RowOutcome::Miss, events);
                    let Stage {
                        rows,
                        health,
                        max_health,
                        ..
                    } = stage;
                    for arrow in &mut rows[cursor].arrows {
                        let Some(hold) = &mut arrow.hold else {
                            continue;
                        };
                        if arrow.error.is_none() {
                            hold.result = Some(HoldOutcome::Ng);
                            apply_hold_health(health, *max_health, config, HoldOutcome::Ng);
                            popups.push((rig.layout.column_x(arrow.column), HoldOutcome::Ng));
                        }
                    }
                }
                stage.expire_cursor += 1;
            }
        }
        for (x, outcome) in popups {
            self.spawn_hold_popup(x, outcome);
        }
    }

    /// Runs every engaged hold's life: holds refill to full while the panel
    /// is down and drain over the grace window otherwise; rolls drain
    /// constantly and refill on fresh steps. Life zero drops the hold (NG);
    /// reaching the tail with life left keeps it (OK).
    fn update_holds(&mut self, delta: f64, _events: &mut [StageEvent]) {
        let config = config();
        let now = self.graded_now;
        let delta = delta as f32;
        let mut popups = Vec::new();
        for (stage, rig) in self.stages.iter_mut().zip(&mut self.rigs) {
            if stage.failed {
                continue;
            }
            let Stage {
                rows,
                health,
                max_health,
                ..
            } = stage;
            for arrow in rows.iter_mut().flat_map(|row| row.arrows.iter_mut()) {
                let Some(hold) = &mut arrow.hold else {
                    continue;
                };
                if hold.result.is_some() || !hold.engaged {
                    continue;
                }

                let action = rig.layout.step_action(arrow.column);
                if hold.roll {
                    if self.input.struck(action) {
                        hold.life = 1.0;
                    }
                    hold.held_now = self.input.held(action);
                    hold.life -= delta / config.grading.roll_grace_seconds.0 as f32;
                } else if self.input.held(action) {
                    hold.held_now = true;
                    hold.life = 1.0;
                } else {
                    hold.held_now = false;
                    hold.life -= delta / config.grading.hold_grace_seconds.0 as f32;
                }
                hold.life = hold.life.clamp(0.0, 1.0);

                if now.0 >= hold.end.0 && hold.life > 0.0 {
                    hold.result = Some(HoldOutcome::Ok);
                    apply_hold_health(health, *max_health, config, HoldOutcome::Ok);
                    rig.fade_out_note(arrow.note, HOLD_OK_FADE_SECONDS);
                    popups.push((rig.layout.column_x(arrow.column), HoldOutcome::Ok));
                } else if hold.life <= 0.0 {
                    hold.result = Some(HoldOutcome::Ng);
                    apply_hold_health(health, *max_health, config, HoldOutcome::Ng);
                    popups.push((rig.layout.column_x(arrow.column), HoldOutcome::Ng));
                }
            }
        }
        for (x, outcome) in popups {
            self.spawn_hold_popup(x, outcome);
        }
    }

    /// A mine explodes if its panel is being held as the mine crosses the
    /// receptors; otherwise it passes by harmlessly.
    fn update_mines(&mut self) {
        let now = self.graded_now;
        let target_y = self.target_y;
        for (stage, rig) in self.stages.iter_mut().zip(&mut self.rigs) {
            if stage.failed {
                continue;
            }
            for mine in &mut stage.mines {
                if mine.outcome.is_some() || mine.time.0 > now.0 {
                    continue;
                }
                if !self.input.held(rig.layout.step_action(mine.column)) {
                    mine.outcome = Some(MineOutcome::Avoided);
                    continue;
                }
                mine.outcome = Some(MineOutcome::Exploded);
                rig.remove_mine(mine.mine);
                rig.mine_explosion(mine.column, target_y);
            }
        }
    }

    /// Zero health fails that stage on the spot: its remaining notes fade
    /// out and its grading stops, while any surviving stage plays on.
    fn fail_drained_stages(&mut self, events: &mut Vec<StageEvent>) {
        for (stage, rig) in self.stages.iter_mut().zip(&mut self.rigs) {
            if stage.failed || stage.health > 0 {
                continue;
            }
            stage.failed = true;
            events.push(StageEvent::Failed {
                player: stage.player,
            });
            rig.fail_out(FAIL_FADE_SECONDS);
        }
    }
}

fn apply_outcome(
    stage: &mut Stage,
    config: &GameConfig,
    row_index: usize,
    outcome: RowOutcome,
    events: &mut Vec<StageEvent>,
) {
    stage.rows[row_index].outcome = Some(outcome);
    stage.graded_count += 1;
    let grade = config.grade(outcome);
    stage.health = stage
        .health
        .saturating_add_signed(config.health_offset(grade))
        .min(stage.max_health);
    if config.breaks_combo(grade) {
        stage.combo = 0;
    } else {
        // Every arrow of the row feeds the combo, so a clean jump pays +2.
        stage.combo += stage.rows[row_index].arrows.len() as u32;
        stage.max_combo = stage.max_combo.max(stage.combo);
    }
    events.push(StageEvent::Graded {
        player: stage.player,
        outcome,
        combo: stage.combo,
    });
}

/// Holds pay their fixed grade's health offset the moment they resolve.
fn apply_hold_health(health: &mut u32, max_health: u32, config: &GameConfig, outcome: HoldOutcome) {
    let offset = match outcome {
        HoldOutcome::Ok => config.grading.fixed.ok.health_offset,
        HoldOutcome::Ng => config.grading.fixed.ng.health_offset,
    };
    *health = health.saturating_add_signed(offset).min(max_health);
}
