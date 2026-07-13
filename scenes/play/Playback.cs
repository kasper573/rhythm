using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The session flow around the engine: the playback clock that drives the
/// ports, the moment the chart is over, and whether the run is underway.
/// </summary>
internal class Playback
{
    public string Title { get; }
    private StepfileClock clock;
    private Seconds leadInRemaining;
    private bool isPlaying;

    public Playback(string title, StepfileTiming timing, Seconds leadIn, Seconds lastNoteTime)
    {
        Title = title;
        leadInRemaining = leadIn;
        clock = StepfileClock.StartAt(timing, -leadIn);
        isPlaying = false;
        LastNoteTime = lastNoteTime;
    }

    public Seconds Position => clock.Position;

    public bool IsPlaying => isPlaying;

    public Seconds LastNoteTime { get; }

    public void Advance(double deltaSeconds)
    {
        var delta = new Seconds(deltaSeconds);

        if (!isPlaying)
        {
            leadInRemaining = (leadInRemaining - delta).Max(Seconds.Zero);
            clock.SetPosition(-leadInRemaining);

            if (leadInRemaining.Value <= 0.0)
            {
                isPlaying = true;
            }
        }
        else
        {
            clock.Advance(delta, null);
        }
    }

    public (Seconds graded, Seconds visible) GetClockPorts()
    {
        var settings = Settings.Instance;
        var timing = settings.Machine.Timing;
        var graded = clock.GradedNow(timing);
        var visible = clock.VisibleNow(timing);
        return (graded, visible);
    }
}
