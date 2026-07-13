namespace Rhythm.Core;

/// <summary>
/// Settings that belong to the machine rather than to either player: the
/// key bindings for both player slots, the rig's timing calibration, and
/// its playback volumes.
/// </summary>
public sealed record MachineSettings
{
    public required Keymap Keymap { get; init; }
    public required TimingSettings Timing { get; init; }
    public required VolumeSettings Volume { get; init; }
}

/// <summary>
/// Playback volumes, each <c>0..=1</c>: <see cref="Master"/> scales
/// everything, the others their own audio bus.
/// </summary>
public sealed record VolumeSettings(float Master, float Sfx, float Music);

/// <summary>
/// The synchronization model:
/// <code>
/// heard   = audio position - audio_latency   (what the speakers play now)
/// graded  = heard + machine_offset           (timeline inputs are graded on)
/// visible = graded - visual_delay            (timeline arrows are drawn on)
/// </code>
/// The audio backend only reports the mixer's queue position, so the latency
/// between queue and speakers is measured on first play and stored here.
/// <see cref="MachineOffset"/> shifts the graded timeline to compensate for
/// the rig as a whole; <see cref="VisualDelay"/> shifts only what is drawn.
/// </summary>
public sealed record TimingSettings
{
    public required Millis MachineOffset { get; init; }
    public required Millis VisualDelay { get; init; }

    /// <summary><c>null</c> until measured on first play; editable afterwards.</summary>
    public required Millis? AudioLatency { get; init; }

    public Millis Latency => AudioLatency ?? new Millis(0);

    /// <summary>What the speakers are playing right now, given the mixer's queue position.</summary>
    private Seconds Heard(Seconds position) => position - Latency.ToSeconds();

    /// <summary>The timeline inputs are graded on.</summary>
    public Seconds Graded(Seconds position) => Heard(position) + MachineOffset.ToSeconds();

    /// <summary>The timeline everything is drawn on.</summary>
    public Seconds Visible(Seconds position) => Graded(position) - VisualDelay.ToSeconds();
}
