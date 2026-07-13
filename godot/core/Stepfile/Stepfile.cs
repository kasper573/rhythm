namespace Rhythm.Core;

/// <summary>A parsed simfile: its metadata, timing, and charts.</summary>
public sealed class Stepfile
{
    public required string Title { get; init; }
    public required string Subtitle { get; init; }
    public required string Artist { get; init; }
    public required string TitleTranslit { get; init; }
    public required string SubtitleTranslit { get; init; }
    public required string ArtistTranslit { get; init; }
    public required string Credit { get; init; }

    /// <summary>File names relative to the stepfile's own folder.</summary>
    public required string? Banner { get; init; }
    public required string? Background { get; init; }
    public required string? CdTitle { get; init; }
    public required string? Music { get; init; }
    public required Seconds SampleStart { get; init; }
    public required Seconds SampleLength { get; init; }

    /// <summary><c>#SELECTABLE:NO</c> hides the stepfile from the wheel.</summary>
    public required bool Selectable { get; init; }
    public required DisplayBpm? DisplayBpm { get; init; }
    public required StepfileTiming Timing { get; init; }
    public required IReadOnlyList<BgChange> BgChanges { get; init; }
    public required IReadOnlyList<Chart> Charts { get; init; }

    /// <summary>Every tag the game has no use for yet, keyed by upper-case name.</summary>
    public required IReadOnlyDictionary<string, string> ExtraTags { get; init; }

    /// <summary>
    /// Indices of the playable charts of one type — non-empty note data —
    /// ordered easiest first.
    /// </summary>
    public IReadOnlyList<int> PlayableCharts(StepsType stepsType)
    {
        var charts = new List<int>();
        for (var index = 0; index < Charts.Count; index++)
        {
            if (Charts[index].StepsType == stepsType && Charts[index].Rows.Count > 0)
            {
                charts.Add(index);
            }
        }

        charts.Sort((a, b) =>
        {
            var byRank = Charts[a].Difficulty.Rank().CompareTo(Charts[b].Difficulty.Rank());
            return byRank != 0 ? byRank : Charts[a].Meter.CompareTo(Charts[b].Meter);
        });
        return charts;
    }

    /// <summary>
    /// The preview playback the .sm sample tags describe: loop the sample
    /// window, or play the file whole from its start when <c>#SAMPLELENGTH</c>
    /// is absent or non-positive.
    /// </summary>
    public SoundTimeline SampleTimeline() =>
        SampleLength.Value > 0.0
            ? new SoundTimeline.LoopWindow(SampleStart, SampleLength)
            : new SoundTimeline.From(SampleStart);

    /// <summary>The playable chart whose difficulty rank sits closest to <paramref name="preferred"/>.</summary>
    public int? ClosestChart(StepsType stepsType, int preferred)
    {
        int? best = null;
        var bestKey = (int.MaxValue, int.MaxValue);
        foreach (var index in PlayableCharts(stepsType))
        {
            var rank = Charts[index].Difficulty.Rank();
            var key = (Math.Abs(rank - preferred), rank);
            if (best is null || key.CompareTo(bestKey) < 0)
            {
                best = index;
                bestKey = key;
            }
        }

        return best;
    }
}

public sealed class StepfileException(string message) : Exception(message);

/// <summary>
/// One chart of a simfile: a difficulty of a play style, with its
/// steppable rows and mines.
/// </summary>
public sealed class Chart
{
    public required StepsType StepsType { get; init; }
    public required string Description { get; init; }
    public required Difficulty Difficulty { get; init; }
    public required uint Meter { get; init; }
    public required IReadOnlyList<float> Radar { get; init; }
    public required int Columns { get; init; }

    /// <summary>
    /// Sorted by beat. The row is the unit the game grades: every arrow in
    /// it must be stepped for the row to count, and rows with two or more
    /// arrows are the jumps shown on the file select.
    /// </summary>
    public required IReadOnlyList<Row> Rows { get; init; }

    /// <summary>Sorted by beat.</summary>
    public required IReadOnlyList<Mine> Mines { get; init; }

    public Beat? LastNoteBeat()
    {
        Beat? last = null;
        foreach (var row in Rows)
        {
            foreach (var arrow in row.Arrows)
            {
                var end = arrow.EndBeat(row.Beat);
                if (last is null || end > last.Value)
                {
                    last = end;
                }
            }
        }

        foreach (var mine in Mines)
        {
            if (last is null || mine.Beat > last.Value)
            {
                last = mine.Beat;
            }
        }

        return last;
    }

    public ChartStats Stats()
    {
        var steps = 0;
        var jumps = 0;
        var holds = 0;
        foreach (var row in Rows)
        {
            steps += row.Arrows.Count;
            if (row.IsJump())
            {
                jumps++;
            }

            foreach (var arrow in row.Arrows)
            {
                if (arrow.Tail is not null)
                {
                    holds++;
                }
            }
        }

        return new ChartStats(steps, jumps, holds, Mines.Count);
    }
}

/// <summary>Simultaneous arrows on one beat, stepped and graded as a single unit.</summary>
public sealed record Row(Beat Beat, uint Quant, IReadOnlyList<Arrow> Arrows)
{
    public bool IsJump() => Arrows.Count >= 2;
}

public readonly record struct Arrow(int Column, Tail? Tail)
{
    /// <summary>
    /// The beat where this arrow is over: the tail beat for holds and
    /// rolls, the row's own beat otherwise.
    /// </summary>
    public Beat EndBeat(Beat rowBeat) => Tail?.End ?? rowBeat;
}

public readonly record struct Tail(Beat End, bool Roll);

public readonly record struct Mine(Beat Beat, int Column);

public readonly record struct ChartStats(int Steps, int Jumps, int Holds, int Mines);

/// <summary>
/// One <c>#BGCHANGES</c> entry: switch the background to <paramref name="File"/>
/// at <paramref name="Beat"/>.
/// </summary>
public readonly record struct BgChange(Beat Beat, string File, bool Crossfade, bool Loops);

/// <summary>The <c>#DISPLAYBPM</c> the wheel shows in place of the real tempo.</summary>
public abstract record DisplayBpm
{
    public sealed record Single(Bpm Bpm) : DisplayBpm;

    public sealed record Range(Bpm Low, Bpm High) : DisplayBpm;

    public sealed record Random : DisplayBpm;
}
