using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// Grading logic: per-row judgment pass, hold/roll/mine mechanics, health/combo management.
/// All grading is time-based with no device interaction — the session samples its input
/// each frame and banks presses into arrows when they're stepped.
/// </summary>
public static class Grading
{
    /// <summary>
    /// Advances the grading pass: resolves rows (checking if every arrow is banked),
    /// grades them, expires misses, runs holds, and mines. Returns collected events
    /// and updates the stage's combo and health.
    /// </summary>
    public static void RunGradingPass(
        StageState stage,
        GameConfig config,
        Seconds gradedNow,
        StepfileTiming timing,
        out List<GradingEvent> events)
    {
        events = new List<GradingEvent>();

        // Expire misses: any row older than the widest grade window is a miss
        var window = config.WidestWindow();
        while ((int)stage.ExpireCursor < stage.Rows.Count)
        {
            var row = stage.Rows[(int)stage.ExpireCursor];
            if (gradedNow - row.Time > window)
            {
                if (row.Outcome == null)
                {
                    row.Outcome = new RowOutcome.Miss();
                    stage.GradedCount++;
                    stage.Combo = 0;

                    var healthOffset = config.HealthOffset(new Grade.Miss());
                    stage.Health = stage.Health > (uint)healthOffset ? stage.Health - (uint)healthOffset : 0;

                    events.Add(new GradingEvent.Graded(stage.Player, row.Outcome, stage.Combo));

                    if (stage.Health == 0 && !stage.Failed)
                    {
                        stage.Failed = true;
                        events.Add(new GradingEvent.Failed(stage.Player));
                    }
                }
                stage.ExpireCursor++;
            }
            else
            {
                break;
            }
        }

        // Grade complete rows
        for (int i = 0; i < stage.Rows.Count; i++)
        {
            var row = stage.Rows[i];
            if (row.Outcome != null)
                continue;

            if (!RowComplete(row))
                continue;

            var outcome = ResolveRow(row, config);
            row.Outcome = outcome;
            stage.GradedCount++;

            var grade = config.ClassifyGrade(outcome);
            var breaksCombo = config.BreaksCombo(grade);

            if (breaksCombo)
            {
                stage.Combo = 0;
            }
            else
            {
                stage.Combo++;
                if (stage.Combo > stage.MaxCombo)
                {
                    stage.MaxCombo = stage.Combo;
                }
            }

            var healthOffset = config.HealthOffset(grade);
            if (healthOffset > 0)
            {
                stage.Health = Math.Min(stage.Health + (uint)healthOffset, stage.MaxHealth);
            }
            else if (healthOffset < 0)
            {
                stage.Health = stage.Health > (uint)(-healthOffset) ? stage.Health - (uint)(-healthOffset) : 0;
            }

            events.Add(new GradingEvent.Graded(stage.Player, outcome, stage.Combo));

            if (stage.Health == 0 && !stage.Failed)
            {
                stage.Failed = true;
                events.Add(new GradingEvent.Failed(stage.Player));
            }
        }

        // Update hold states: check if panels are held
        for (int i = 0; i < stage.Rows.Count; i++)
        {
            var row = stage.Rows[i];
            foreach (var arrow in row.Arrows)
            {
                if (arrow.Hold == null)
                    continue;

                // Holds are resolved once the row is graded
                if (row.Outcome == null)
                    continue;

                // If hold is already resolved, skip
                if (arrow.Hold.Result.HasValue)
                    continue;

                // Check if head was stepped (error was banked)
                if (arrow.Error == null)
                    continue;

                arrow.Hold.Engaged = true;

                // Check if held to the end
                if (arrow.Hold.HeldNow)
                {
                    var graceValue = arrow.Hold.Roll
                        ? (config.Grading?.RollGraceSeconds ?? 0.0f)
                        : (config.Grading?.HoldGraceSeconds ?? 0.0f);
                    var graceSeconds = new Seconds(graceValue);

                    var endError = gradedNow - arrow.Hold.End;
                    if (Math.Abs(endError.Value) <= graceSeconds.Value)
                    {
                        arrow.Hold.Result = HoldOutcome.Ok;
                    }
                    else
                    {
                        arrow.Hold.Result = HoldOutcome.Ng;
                    }
                }
                else
                {
                    arrow.Hold.Result = HoldOutcome.Ng;
                }
            }
        }

        // Mines: check if any stepped (before the expiry window)
        for (int i = 0; i < stage.Mines.Count; i++)
        {
            var mine = stage.Mines[i];
            if (mine.Outcome.HasValue)
                continue;

            if (gradedNow > mine.Time)
            {
                // Mine is in the past; check if it was stepped
                // For now, assume avoided (a full implementation would check input)
                mine.Outcome = MineOutcome.Avoided;
            }
        }
    }

    /// <summary>Checks if a row is complete: every arrow has a banked error.</summary>
    private static bool RowComplete(SessionRow row)
    {
        return row.Arrows.TrueForAll(arrow => arrow.Error.HasValue);
    }

    /// <summary>Resolves a complete row into a RowOutcome based on its arrows' errors.</summary>
    private static RowOutcome ResolveRow(SessionRow row, GameConfig config)
    {
        if (row.Arrows.Count == 0)
            return new RowOutcome.Miss();

        var errors = row.Arrows
            .Where(a => a.Error.HasValue)
            .Select(a => a.Error!.Value)
            .ToList();

        if (errors.Count == 0)
            return new RowOutcome.Miss();

        // Use the worst (largest absolute) error
        var worstError = errors.OrderByDescending(e => Math.Abs(e.Value)).First();
        return new RowOutcome.Hit(worstError);
    }
}

/// <summary>Grading pass event, returned to the session for application.</summary>
public abstract record GradingEvent
{
    public sealed record Graded(PlayerId Player, RowOutcome Outcome, uint Combo) : GradingEvent;
    public sealed record PressBanked(Seconds Error) : GradingEvent;
    public sealed record Failed(PlayerId Player) : GradingEvent;
}

/// <summary>Hold or roll state during a play session.</summary>
public class HoldState
{
    public Seconds End { get; set; }
    public bool Roll { get; set; }
    public float Life { get; set; }
    public bool Engaged { get; set; }
    public bool HeldNow { get; set; }
    public HoldOutcome? Result { get; set; }

    public HoldState(Seconds end, bool roll)
    {
        End = end;
        Roll = roll;
        Life = 1.0f;
        Engaged = false;
        HeldNow = false;
        Result = null;
    }
}

/// <summary>One row of the chart as played in a session.</summary>
public class SessionRow
{
    public Seconds Time { get; set; }
    public RowOutcome? Outcome { get; set; }
    public List<SessionArrow> Arrows { get; set; }

    public SessionRow(Seconds time)
    {
        Time = time;
        Outcome = null;
        Arrows = new List<SessionArrow>();
    }
}

/// <summary>One arrow in a session row.</summary>
public class SessionArrow
{
    public uint Column { get; set; }
    public int NoteIndex { get; set; }
    public Seconds? Error { get; set; }
    public HoldState? Hold { get; set; }

    public SessionArrow(uint column, int noteIndex)
    {
        Column = column;
        NoteIndex = noteIndex;
        Error = null;
        Hold = null;
    }
}

/// <summary>One mine in a session.</summary>
public class SessionMine
{
    public Seconds Time { get; set; }
    public int Column { get; set; }
    public int MineIndex { get; set; }
    public MineOutcome? Outcome { get; set; }

    public SessionMine(Seconds time, int column, int mineIndex)
    {
        Time = time;
        Column = column;
        MineIndex = mineIndex;
        Outcome = null;
    }
}

/// <summary>State container for one stage (player's chart).</summary>
public class StageState
{
    public PlayerId Player { get; set; }
    public List<SessionRow> Rows { get; set; }
    public List<SessionMine> Mines { get; set; }
    public uint GradedCount { get; set; }
    public uint ExpireCursor { get; set; }
    public uint Combo { get; set; }
    public uint MaxCombo { get; set; }
    public uint Health { get; set; }
    public uint MaxHealth { get; set; }
    public bool Failed { get; set; }

    public StageState(PlayerId player, uint maxHealth)
    {
        Player = player;
        Rows = new List<SessionRow>();
        Mines = new List<SessionMine>();
        GradedCount = 0;
        ExpireCursor = 0;
        Combo = 0;
        MaxCombo = 0;
        Health = maxHealth;
        MaxHealth = maxHealth;
        Failed = false;
    }

    public bool IsComplete() => GradedCount >= Rows.Count;
    public float HealthFraction() => Health / (float)MaxHealth;

    public StageResults ToResults()
    {
        var outcomes = Rows
            .Where(r => r.Outcome != null)
            .Select(r => r.Outcome!)
            .ToList();

        var holds = Rows
            .SelectMany(r => r.Arrows)
            .Where(a => a.Hold != null)
            .Select(a => a.Hold!)
            .ToList();

        return new StageResults
        {
            Player = Player,
            Failed = Failed,
            Outcomes = outcomes,
            RowsTotal = (uint)Rows.Count,
            MaxCombo = MaxCombo,
            HoldsOk = (uint)holds.Count(h => h.Result == HoldOutcome.Ok),
            HoldsNg = (uint)holds.Count(h => h.Result == HoldOutcome.Ng),
            HoldsTotal = (uint)holds.Count,
            MinesExploded = (uint)Mines.Count(m => m.Outcome == MineOutcome.Exploded),
            MinesTotal = (uint)Mines.Count
        };
    }
}

public enum HoldOutcome
{
    Ok,
    Ng
}

public enum MineOutcome
{
    Avoided,
    Exploded
}
