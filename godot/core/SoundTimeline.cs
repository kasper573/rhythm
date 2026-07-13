namespace Rhythm.Core;

/// <summary>How playback traverses a sound's own timeline.</summary>
public abstract record SoundTimeline
{
    /// <summary>The whole file, once, from the top.</summary>
    public sealed record WholeFile : SoundTimeline;

    /// <summary>The whole file, once, from this position.</summary>
    public sealed record From(Seconds Position) : SoundTimeline;

    /// <summary>
    /// This <c>[start, start+length)</c> window, looping forever — a looping
    /// window never finishes.
    /// </summary>
    public sealed record LoopWindow(Seconds Start, Seconds Length) : SoundTimeline;
}
