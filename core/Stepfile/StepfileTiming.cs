namespace Rhythm.Core;

/// <summary>
/// The beat↔seconds mapping of one stepfile. All <see cref="Seconds"/>
/// values are positions on the audio clock of the music file: beat zero
/// maps to <c>-offset</c> seconds, matching the .sm <c>#OFFSET</c> convention.
/// </summary>
public sealed class StepfileTiming
{
    private readonly Anchor[] anchors;

    /// <summary>
    /// <paramref name="bpms"/> are <c>(beat, bpm)</c> pairs and
    /// <paramref name="stops"/> are <c>(beat, duration)</c> pairs, both as
    /// written in the .sm file. Entries with non-positive BPM are ignored.
    /// </summary>
    public StepfileTiming(Seconds offset, IReadOnlyList<(Beat, Bpm)> bpms, IReadOnlyList<(Beat, Seconds)> stops)
    {
        anchors = BuildAnchors(offset, bpms, stops);
    }

    public Seconds SecondsAtBeat(Beat beat)
    {
        var anchor = AnchorBeforeBeat(beat);
        if (anchor.BeatsPerSecond <= 0.0)
        {
            return new Seconds(anchor.Seconds);
        }

        return new Seconds(anchor.Seconds + (beat.Value - anchor.Beat) / anchor.BeatsPerSecond);
    }

    public Beat BeatAtSeconds(Seconds seconds)
    {
        var anchor = AnchorBeforeSeconds(seconds);
        return new Beat(anchor.Beat + (seconds.Value - anchor.Seconds) * anchor.BeatsPerSecond);
    }

    public (Bpm Min, Bpm Max) BpmRange()
    {
        var min = double.PositiveInfinity;
        var max = double.NegativeInfinity;
        foreach (var anchor in anchors)
        {
            var bpm = anchor.BeatsPerSecond * 60.0;
            if (bpm > 0.0)
            {
                min = Math.Min(min, bpm);
                max = Math.Max(max, bpm);
            }
        }

        return (new Bpm(min), new Bpm(max));
    }

    private Anchor AnchorBeforeBeat(Beat beat)
    {
        var index = PartitionPoint(anchors, a => a.Beat < beat.Value);
        return anchors[Math.Max(index - 1, 0)];
    }

    private Anchor AnchorBeforeSeconds(Seconds seconds)
    {
        var index = PartitionPoint(anchors, a => a.Seconds <= seconds.Value);
        return anchors[Math.Max(index - 1, 0)];
    }

    /// <summary>
    /// The index of the first element for which <paramref name="predicate"/>
    /// is false, on a sequence partitioned so all true elements precede all
    /// false ones.
    /// </summary>
    private static int PartitionPoint(Anchor[] items, Func<Anchor, bool> predicate)
    {
        var index = 0;
        while (index < items.Length && predicate(items[index]))
        {
            index++;
        }

        return index;
    }

    private static Anchor[] BuildAnchors(Seconds offset, IReadOnlyList<(Beat, Bpm)> bpms, IReadOnlyList<(Beat, Seconds)> stops)
    {
        var changes = new List<(double Beat, Change Change)>();
        foreach (var (beat, bpm) in bpms)
        {
            if (bpm.Value > 0.0)
            {
                changes.Add((Math.Max(beat.Value, 0.0), new Change.SetBpm(bpm.Value / 60.0)));
            }
        }

        foreach (var (beat, duration) in stops)
        {
            if (duration.Value > 0.0)
            {
                changes.Add((Math.Max(beat.Value, 0.0), new Change.Stop(duration.Value)));
            }
        }

        // At equal beats a BPM change applies before a stop, so time frozen
        // by the stop resumes at the new tempo.
        changes.Sort((a, b) =>
        {
            var byBeat = a.Beat.CompareTo(b.Beat);
            if (byBeat != 0)
            {
                return byBeat;
            }

            static int Order(Change change) => change is Change.SetBpm ? 0 : 1;
            return Order(a.Change).CompareTo(Order(b.Change));
        });

        var initialBps = 120.0 / 60.0;
        foreach (var (_, change) in changes)
        {
            if (change is Change.SetBpm setBpm)
            {
                initialBps = setBpm.BeatsPerSecond;
                break;
            }
        }

        var anchors = new List<Anchor> { new(0.0, -offset.Value, initialBps) };
        var beatPos = 0.0;
        var secondsPos = -offset.Value;
        var bps = initialBps;

        foreach (var (changeBeat, change) in changes)
        {
            secondsPos += (changeBeat - beatPos) / bps;
            beatPos = changeBeat;
            switch (change)
            {
                case Change.SetBpm setBpm:
                    bps = setBpm.BeatsPerSecond;
                    anchors.Add(new Anchor(beatPos, secondsPos, bps));
                    break;
                case Change.Stop stop:
                    anchors.Add(new Anchor(beatPos, secondsPos, 0.0));
                    secondsPos += stop.Duration;
                    anchors.Add(new Anchor(beatPos, secondsPos, bps));
                    break;
            }
        }

        return [.. anchors];
    }

    /// <summary>
    /// A point where the beat↔seconds mapping changes slope. From this
    /// anchor until the next one, beats advance at
    /// <see cref="BeatsPerSecond"/> (zero during a stop).
    /// </summary>
    private readonly record struct Anchor(double Beat, double Seconds, double BeatsPerSecond);

    private abstract record Change
    {
        public sealed record SetBpm(double BeatsPerSecond) : Change;

        public sealed record Stop(double Duration) : Change;
    }
}
