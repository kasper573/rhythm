namespace Rhythm.Core;

/// <summary>
/// Where a playing stepfile's music is, on every timeline the game shares:
/// the servo'd mixer position, the graded timeline inputs are judged on,
/// and the visible timeline everything is drawn on — with beats derived
/// through the stepfile's own timing. Every scene that plays stepfile audio
/// reads the same clock the same way.
/// </summary>
public sealed class StepfileClock
{
    private readonly AudioServo servo;

    private StepfileClock(StepfileTiming timing, Seconds position)
    {
        Timing = timing;
        servo = new AudioServo(position);
    }

    public StepfileTiming Timing { get; }

    public static StepfileClock StartAt(StepfileTiming timing, Seconds position) => new(timing, position);

    /// <summary>
    /// Drives the clock directly, for pre-playback timelines like a lead-in
    /// counting up to the music's start.
    /// </summary>
    public void SetPosition(Seconds position) => servo.Position = position;

    /// <summary>
    /// Advances by frame time and servos onto the mixer report when given
    /// one; returns whether the report was a fresh edge.
    /// </summary>
    public bool Advance(Seconds delta, Seconds? report) => servo.Advance(delta, report);

    /// <summary>The raw position on the mixer-queue timeline.</summary>
    public Seconds Position => servo.Position;

    public Seconds GradedNow(TimingSettings settings) => settings.Graded(Position);

    public Seconds VisibleNow(TimingSettings settings) => settings.Visible(Position);

    public Beat VisibleBeat(TimingSettings settings) => Timing.BeatAtSeconds(VisibleNow(settings));
}

/// <summary>
/// A smooth clock servo'd onto the audio mixer's position reports.
///
/// <para>
/// The mixer consumes audio in output-callback bursts, so its reported
/// position is a staircase: exact at the moment it changes, stale in
/// between. The servo therefore advances with frame time, snaps once to the
/// first report, and then applies small, slew-limited corrections toward
/// each fresh report edge — never jumping, never running backwards — so
/// consumers see a smooth, accurate timeline. Snapping to the staircase
/// directly would make the timeline oscillate by tens of milliseconds
/// whenever the audio quantum exceeds the snap threshold. Reports that leap
/// beyond <see cref="ResyncThreshold"/> (a seek, an underrun, a loop seam)
/// snap instead of slewing.
/// </para>
/// </summary>
internal sealed class AudioServo(Seconds position)
{
    /// <summary>
    /// Proportional correction per fresh report, slew-limited so the clock
    /// stays smooth: at typical report rates the steady-state tracking error
    /// is a couple of milliseconds, constant biases land in the calibrated
    /// audio latency instead.
    /// </summary>
    private const double ServoGain = 0.08;
    private const double MaxBackwardStep = 0.002;
    private const double MaxForwardStep = 0.010;
    private const double ResyncThreshold = 0.25;

    private Seconds? lastReport;

    public Seconds Position { get; set; } = position;

    public bool Advance(Seconds delta, Seconds? report)
    {
        Position += delta;
        if (report is not { } edge)
        {
            return false;
        }

        if (lastReport == edge)
        {
            return false;
        }

        var first = lastReport is null;
        lastReport = edge;
        var error = edge.Value - Position.Value;
        if (first || Math.Abs(error) > ResyncThreshold)
        {
            Position = edge;
        }
        else
        {
            Position = new Seconds(Position.Value + Math.Clamp(error * ServoGain, -MaxBackwardStep, MaxForwardStep));
        }

        return true;
    }
}
