using Rhythm.Core;

namespace Rhythm;

/// <summary>Options for creating and playing a sound channel.</summary>
public sealed record SoundOptions
{
    public required SoundTimeline Timeline { get; init; }
    public required bool Paused { get; init; }

    /// <summary>Muting silences the channel without touching its bus.</summary>
    public required bool Muted { get; init; }

    /// <summary>The bus the sound plays on.</summary>
    public required string Bus { get; init; }
}
