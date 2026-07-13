using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The grading pass. The row is the unit the engine grades, independently per
/// stage. Presses bank silently into their arrows; the row resolves into one
/// grade when its last arrow is banked — decided by that completing press — or
/// expires into a single Miss if any arrow times out, voiding the banked
/// presses. A stage that drains to zero health fails on the spot and stops
/// grading, its field fading away, while any surviving stage plays on.
/// </summary>
public partial class StepfilePlayer
{
    private const float FailFadeSeconds = 0.8f;

    /// <summary>One frame of grading, in the fixed order the rules depend on.</summary>
    private List<GradingEvent> RunGrading(double delta)
    {
        var events = new List<GradingEvent>();
        BankRowInputs(events);
        ExpireMissedRows(events);
        UpdateHolds(delta);
        UpdateMines();
        FailDrainedStages(events);
        return events;
    }

    /// <summary>
    /// Banks step presses into the nearest unresolved row with an unbanked
    /// arrow in that column, per stage, and resolves rows whose last arrow just
    /// arrived. Inputs that hit no grading window are no-ops.
    /// </summary>
    private void BankRowInputs(List<GradingEvent> events)
    {
        var config = Config.Current!;
        var widest = config.WidestWindow();
        var inputTime = _gradedNow;

        for (int s = 0; s < _stages.Count; s++)
        {
            var stage = _stages[s];
            var rig = _rigs[s];
            if (stage.Failed)
            {
                continue;
            }
            for (uint column = 0; column < rig.Layout.Columns; column++)
            {
                if (!_input.IsStruck(rig.Layout.StepAction(column)))
                {
                    continue;
                }

                int? candidate = null;
                var bestDistance = double.MaxValue;
                for (int i = 0; i < stage.Rows.Count; i++)
                {
                    var row = stage.Rows[i];
                    if (row.Outcome is not null)
                    {
                        continue;
                    }
                    var distance = Math.Abs((row.Time - inputTime).Value);
                    if (distance > widest.Value)
                    {
                        continue;
                    }
                    if (!row.Arrows.Exists(arrow => arrow.Column == column && arrow.Error is null))
                    {
                        continue;
                    }
                    if (distance < bestDistance)
                    {
                        bestDistance = distance;
                        candidate = i;
                    }
                }
                if (candidate is not int index)
                {
                    continue;
                }

                var error = stage.Rows[index].Time - inputTime;
                events.Add(new GradingEvent.PressBanked(error));
                var banked = stage.Rows[index].Arrows.Find(arrow => arrow.Column == column && arrow.Error is null)!;
                banked.Error = error;
                if (banked.Hold is HoldState hold)
                {
                    hold.Engaged = true;
                    hold.Life = 1.0f;
                }

                if (!stage.Rows[index].Complete())
                {
                    continue;
                }

                // The completing press decides the row: the chronologically
                // last one, which is the smallest signed error since late
                // presses go negative.
                Seconds completing = stage.Rows[index].Arrows[0].Error!.Value;
                foreach (var arrow in stage.Rows[index].Arrows)
                {
                    if (arrow.Error is Seconds e && e.Value < completing.Value)
                    {
                        completing = e;
                    }
                }
                var outcome = new RowOutcome.Hit(completing);
                ApplyOutcome(stage, config, index, outcome, events);

                // The vanish: grades with an arrow flash play it at every arrow
                // of the row and the tap arrows disappear on the spot. Lesser
                // grades leave the arrows scrolling on, graded but visible.
                if (config.ClassifyGrade(outcome) is not Grade.Hit hit)
                {
                    continue;
                }
                var grade = config.Grading!.Dynamic[hit.Index.Value];
                if (!grade.HasArrowFlash)
                {
                    continue;
                }
                var bright = stage.Combo >= (uint)config.BrightArrowFlashCombo;
                foreach (var arrow in stage.Rows[index].Arrows)
                {
                    rig.ArrowFlash(arrow.Column, _targetY, grade.ArrowFlash, bright);
                    if (arrow.Hold is null)
                    {
                        rig.VanishNote(arrow.Note);
                    }
                }
            }
        }
    }

    /// <summary>
    /// Rows expire into a single Miss once they scroll further past the player
    /// than the widest grading window with any arrow still unbanked — banked
    /// presses on the other arrows are voided. A hold whose own head was never
    /// stepped can never be caught, so it drops immediately.
    /// </summary>
    private void ExpireMissedRows(List<GradingEvent> events)
    {
        var config = Config.Current!;
        var expireBefore = _gradedNow - config.WidestWindow();
        var popups = new List<(float X, HoldOutcome Outcome)>();

        for (int s = 0; s < _stages.Count; s++)
        {
            var stage = _stages[s];
            var rig = _rigs[s];
            if (stage.Failed)
            {
                continue;
            }
            while (stage.ExpireCursor < stage.Rows.Count)
            {
                int cursor = (int)stage.ExpireCursor;
                if (stage.Rows[cursor].Time.Value >= expireBefore.Value)
                {
                    break;
                }
                if (stage.Rows[cursor].Outcome is null)
                {
                    ApplyOutcome(stage, config, cursor, new RowOutcome.Miss(), events);
                    foreach (var arrow in stage.Rows[cursor].Arrows)
                    {
                        if (arrow.Hold is HoldState hold && arrow.Error is null)
                        {
                            hold.Result = HoldOutcome.Ng;
                            ApplyHoldHealth(stage, config, HoldOutcome.Ng);
                            popups.Add((rig.Layout.ColumnX(arrow.Column), HoldOutcome.Ng));
                        }
                    }
                }
                stage.ExpireCursor++;
            }
        }
        foreach (var (x, outcome) in popups)
        {
            SpawnHoldPopup(x, outcome);
        }
    }

    /// <summary>
    /// Runs every engaged hold's life: holds refill while the panel is down and
    /// drain over the grace window otherwise; rolls drain constantly and refill
    /// on fresh steps. Life zero drops the hold (NG); reaching the tail with
    /// life left keeps it (OK).
    /// </summary>
    private void UpdateHolds(double delta)
    {
        var config = Config.Current!;
        var now = _gradedNow;
        var step = (float)delta;
        var popups = new List<(float X, HoldOutcome Outcome)>();

        for (int s = 0; s < _stages.Count; s++)
        {
            var stage = _stages[s];
            var rig = _rigs[s];
            if (stage.Failed)
            {
                continue;
            }
            foreach (var row in stage.Rows)
            {
                foreach (var arrow in row.Arrows)
                {
                    if (arrow.Hold is not HoldState hold || hold.Result.HasValue || !hold.Engaged)
                    {
                        continue;
                    }
                    var action = rig.Layout.StepAction(arrow.Column);
                    if (hold.Roll)
                    {
                        if (_input.IsStruck(action))
                        {
                            hold.Life = 1.0f;
                        }
                        hold.HeldNow = _input.IsHeld(action);
                        hold.Life -= step / config.Grading!.RollGraceSeconds;
                    }
                    else if (_input.IsHeld(action))
                    {
                        hold.HeldNow = true;
                        hold.Life = 1.0f;
                    }
                    else
                    {
                        hold.HeldNow = false;
                        hold.Life -= step / config.Grading!.HoldGraceSeconds;
                    }
                    hold.Life = Mathf.Clamp(hold.Life, 0.0f, 1.0f);

                    if (now.Value >= hold.End.Value && hold.Life > 0.0f)
                    {
                        hold.Result = HoldOutcome.Ok;
                        ApplyHoldHealth(stage, config, HoldOutcome.Ok);
                        rig.FadeOutNote(arrow.Note, NoteFieldRig.HoldOkFadeSeconds);
                        popups.Add((rig.Layout.ColumnX(arrow.Column), HoldOutcome.Ok));
                    }
                    else if (hold.Life <= 0.0f)
                    {
                        hold.Result = HoldOutcome.Ng;
                        ApplyHoldHealth(stage, config, HoldOutcome.Ng);
                        popups.Add((rig.Layout.ColumnX(arrow.Column), HoldOutcome.Ng));
                    }
                }
            }
        }
        foreach (var (x, outcome) in popups)
        {
            SpawnHoldPopup(x, outcome);
        }
    }

    /// <summary>
    /// A mine explodes if its panel is being held as the mine crosses the
    /// receptors; otherwise it passes by harmlessly.
    /// </summary>
    private void UpdateMines()
    {
        var now = _gradedNow;
        for (int s = 0; s < _stages.Count; s++)
        {
            var stage = _stages[s];
            var rig = _rigs[s];
            if (stage.Failed)
            {
                continue;
            }
            foreach (var mine in stage.Mines)
            {
                if (mine.Outcome.HasValue || mine.Time.Value > now.Value)
                {
                    continue;
                }
                if (!_input.IsHeld(rig.Layout.StepAction(mine.Column)))
                {
                    mine.Outcome = MineOutcome.Avoided;
                    continue;
                }
                mine.Outcome = MineOutcome.Exploded;
                rig.RemoveMine(mine.Mine);
                rig.MineExplosion(mine.Column, _targetY);
            }
        }
    }

    /// <summary>Zero health fails that stage on the spot; any surviving stage plays on.</summary>
    private void FailDrainedStages(List<GradingEvent> events)
    {
        for (int s = 0; s < _stages.Count; s++)
        {
            var stage = _stages[s];
            var rig = _rigs[s];
            if (stage.Failed || stage.Health > 0)
            {
                continue;
            }
            stage.Failed = true;
            events.Add(new GradingEvent.Failed(stage.Player));
            rig.FailOut(FailFadeSeconds);
        }
    }

    private static void ApplyOutcome(StageState stage, GameConfig config, int rowIndex, RowOutcome outcome, List<GradingEvent> events)
    {
        stage.Rows[rowIndex].Outcome = outcome;
        stage.GradedCount++;
        var grade = config.ClassifyGrade(outcome);
        stage.Health = SaturateHealth(stage.Health, config.HealthOffset(grade), stage.MaxHealth);
        if (config.BreaksCombo(grade))
        {
            stage.Combo = 0;
        }
        else
        {
            // Every arrow of the row feeds the combo, so a clean jump pays +2.
            stage.Combo += (uint)stage.Rows[rowIndex].Arrows.Count;
            stage.MaxCombo = Math.Max(stage.MaxCombo, stage.Combo);
        }
        events.Add(new GradingEvent.Graded(stage.Player, outcome, stage.Combo));
    }

    /// <summary>Holds pay their fixed grade's health offset the moment they resolve.</summary>
    private static void ApplyHoldHealth(StageState stage, GameConfig config, HoldOutcome outcome)
    {
        var offset = outcome == HoldOutcome.Ok ? config.Grading!.Ok!.HealthOffset : config.Grading!.Ng!.HealthOffset;
        stage.Health = SaturateHealth(stage.Health, offset, stage.MaxHealth);
    }

    private static uint SaturateHealth(uint health, int offset, uint maxHealth) =>
        (uint)Math.Clamp((long)health + offset, 0L, maxHealth);
}
