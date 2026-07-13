namespace Rhythm.Core;

/// <summary>One player's presentation choices for playing stepfiles.</summary>
public sealed record PlayerOptions
{
    /// <summary>Folder name of the note skin under <c>assets/note_skins</c>.</summary>
    public required string NoteSkin { get; init; }
    public required NoteSpeed NoteSpeed { get; init; }
    public required Perspective Perspective { get; init; }
    public required GradeLayer GradeLayer { get; init; }

    /// <summary>
    /// Grade text height as a percentage down the screen: 0 hugs the top
    /// edge, 100 the bottom edge, ignoring the stage's edge padding.
    /// </summary>
    public required Percent GradePosition { get; init; }
}

/// <summary>Whether the grade text pops out behind the arrows or in front of them.</summary>
public enum GradeLayer
{
    Behind,
    InFront,
}

/// <summary>How fast notes scroll for one player.</summary>
public abstract record NoteSpeed
{
    public abstract float Value { get; }

    /// <summary>
    /// A constant rate regardless of the chart's tempo, expressed as the
    /// scroll BPM at which <see cref="Dynamic"/> would move equally fast.
    /// </summary>
    public sealed record Constant(float Multiplier) : NoteSpeed
    {
        public override float Value => Multiplier;
    }

    /// <summary>
    /// Spacing follows the chart's beats — one arrow height per beat at
    /// multiplier 1 — so BPM changes stretch the scroll and stops freeze it.
    /// </summary>
    public sealed record Dynamic(float Multiplier) : NoteSpeed
    {
        public override float Value => Multiplier;
    }
}

/// <summary>
/// Where a player's lane camera watches their arrows from. The receptor row
/// stays put on screen; the rest of the lane foreshortens around it.
/// </summary>
public enum Perspective
{
    /// <summary>Head on: no perspective.</summary>
    None,

    /// <summary>From above: notes rise out of the distance below.</summary>
    Above,

    /// <summary>From below: the lane recedes upward.</summary>
    Below,
}
