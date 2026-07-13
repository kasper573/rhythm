using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The session flow around the engine: the playback clock that drives the
/// ports, the moment the chart is over, and whether the run is underway.
/// A fixed lead-in counts up to zero, both audio tracks start together, then
/// the StepfileClock servos onto the channel's position reports so grading
/// sees a smooth, accurate timeline.
/// </summary>
internal class Playback
{
    public string Title { get; }
    private StepfileClock clock;
    private PlayPhase phase;
    private Seconds wallSincePlay;
    private List<Seconds> latencySamples = [];

    public Seconds LastNoteTime { get; }

    private enum PlayPhase
    {
        LeadIn,
        Playing,
    }

    public Playback(string title, StepfileTiming timing, Seconds leadIn, Seconds lastNoteTime)
    {
        Title = title;
        phase = PlayPhase.LeadIn;
        clock = StepfileClock.StartAt(timing, -leadIn);
        wallSincePlay = Seconds.Zero;
        LastNoteTime = lastNoteTime;
    }

    public Seconds Position => clock.Position;

    public bool IsPlaying => phase == PlayPhase.Playing;

    public Seconds VisibleNow
    {
        get
        {
            var settings = Settings.Instance;
            var timing = settings.Machine.Timing;
            return clock.VisibleNow(timing);
        }
    }

    /// <summary>
    /// Advances playback and servos the clock onto audio channels.
    /// </summary>
    /// <param name="music">The music channel, null if still loading or failed.</param>
    /// <param name="tick">The tick channel.</param>
    /// <param name="musicIsPending">True if music is still being loaded.</param>
    public void Advance(double deltaSeconds, SoundChannel? music, SoundChannel? tick, bool musicIsPending)
    {
        var delta = new Seconds(deltaSeconds);

        switch (phase)
        {
            case PlayPhase.LeadIn:
                AdvanceLeadIn(delta, music, tick, musicIsPending);
                break;

            case PlayPhase.Playing:
                AdvancePlaying(delta, music, tick);
                break;
        }
    }

    private void AdvanceLeadIn(Seconds delta, SoundChannel? music, SoundChannel? tick, bool musicIsPending)
    {
        var leadInRemaining = -clock.Position;
        leadInRemaining = (leadInRemaining - delta).Max(Seconds.Zero);
        clock.SetPosition(-leadInRemaining);

        if (leadInRemaining.Value > 0.0)
        {
            return;
        }

        // Hold at zero while music is still loading, so tracks start in lockstep.
        if (musicIsPending)
        {
            return;
        }

        // Music fetch completed (or failed); start playback.
        music?.SetPaused(false);
        tick?.SetPaused(false);
        phase = PlayPhase.Playing;
    }

    private void AdvancePlaying(Seconds delta, SoundChannel? music, SoundChannel? tick)
    {
        wallSincePlay += delta;

        // Poll channels to update their position reports.
        music?.Poll();
        tick?.Poll();

        // Servo onto the music channel (falling back to tick).
        var report = music?.Position ?? tick?.Position;
        var fresh = clock.Advance(delta, report);

        // Measure audio latency once at the start if not yet calibrated.
        var settings = Settings.Instance;
        var timing = settings.Machine.Timing;
        if (fresh && timing.AudioLatency is null && report.HasValue)
        {
            if (MeasureAudioLatency(report.Value) is Millis measured)
            {
                settings.EditMachine(machine =>
                {
                    return machine with
                    {
                        Timing = machine.Timing with { AudioLatency = measured }
                    };
                });
                GD.Print($"measured audio latency: {measured}");
            }
        }
    }

    private Millis? MeasureAudioLatency(Seconds report)
    {
        if (wallSincePlay.Value >= 0.3 && wallSincePlay.Value < 2.0)
        {
            latencySamples.Add(report - wallSincePlay);
            return null;
        }

        if (wallSincePlay.Value < 2.0 || latencySamples.Count == 0)
        {
            return null;
        }

        var samples = latencySamples.OrderBy(s => s.Value).ToList();
        var median = samples[samples.Count / 2];
        var millis = (long)Math.Round(Math.Max(median.ToMillis(), 0.0));
        return new Millis(millis);
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
