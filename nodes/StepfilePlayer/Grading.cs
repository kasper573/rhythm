using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The session's per-frame grading model: the mutable play state the grading
/// pass banks presses into and reads outcomes from. The row is the unit
/// graded, independently per stage.
/// </summary>
public sealed class StageState
{
    public PlayerId Player { get; }
    public List<SessionRow> Rows { get; set; } = [];
    public List<SessionMine> Mines { get; set; } = [];
    public uint GradedCount { get; set; }
    public uint ExpireCursor { get; set; }
    public uint Combo { get; set; }
    public uint MaxCombo { get; set; }
    public uint Health { get; set; }
    public uint MaxHealth { get; }
    public bool Failed { get; set; }

    public StageState(PlayerId player, uint maxHealth)
    {
        Player = player;
        Health = maxHealth;
        MaxHealth = maxHealth;
    }

    /// <summary>Every row graded and every hold and mine resolved — nothing left to grade.</summary>
    public bool IsComplete() =>
        GradedCount >= Rows.Count
        && Rows.All(row => row.Arrows.All(arrow => arrow.Hold is null || arrow.Hold.Result.HasValue))
        && Mines.All(mine => mine.Outcome.HasValue);

    public float HealthFraction() => Health / (float)MaxHealth;

    public StageResults ToResults()
    {
        var outcomes = Rows.Select(r => r.Outcome).OfType<RowOutcome>().ToList();
        var holds = Rows.SelectMany(r => r.Arrows).Select(a => a.Hold).OfType<HoldState>().ToList();
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
            MinesTotal = (uint)Mines.Count,
        };
    }
}

/// <summary>One row of the chart as played: it resolves once every arrow is banked.</summary>
public sealed class SessionRow(Seconds time)
{
    public Seconds Time { get; } = time;
    public RowOutcome? Outcome { get; set; }
    public List<SessionArrow> Arrows { get; } = [];

    /// <summary>Every arrow of the row has a banked press.</summary>
    public bool Complete() => Arrows.TrueForAll(arrow => arrow.Error.HasValue);
}

/// <summary>One arrow in a session row: its banked timing error and hold state, if any.</summary>
public sealed class SessionArrow(uint column, NoteIndex note)
{
    public uint Column { get; } = column;
    public NoteIndex Note { get; } = note;
    public Seconds? Error { get; set; }
    public HoldState? Hold { get; set; }
}

/// <summary>One mine in a session: it explodes if its panel is held as it crosses the receptors.</summary>
public sealed class SessionMine(Seconds time, uint column, MineIndex mine)
{
    public Seconds Time { get; } = time;
    public uint Column { get; } = column;
    public MineIndex Mine { get; } = mine;
    public MineOutcome? Outcome { get; set; }
}

/// <summary>A hold or roll's live state through a session.</summary>
public sealed class HoldState(Seconds end, bool roll)
{
    public Seconds End { get; } = end;
    public bool Roll { get; } = roll;
    public float Life { get; set; } = 1.0f;
    public bool Engaged { get; set; }
    public bool HeldNow { get; set; }
    public HoldOutcome? Result { get; set; }
}

/// <summary>A grading-pass event, returned to the presentation layer to apply.</summary>
public abstract record GradingEvent
{
    public sealed record Graded(PlayerId Player, RowOutcome Outcome, uint Combo) : GradingEvent;
    public sealed record PressBanked(Seconds Error) : GradingEvent;
    public sealed record Failed(PlayerId Player) : GradingEvent;
}
