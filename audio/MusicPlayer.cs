using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The global background-music autoload: at most one stepfile plays at a
/// time, looping its sample window until switched or stopped. A Play() call
/// naming the stepfile already playing is ignored, so scenes and wheel rows
/// resolving to the same music keep it running uninterrupted. Scene changes
/// never stop it by themselves — only an explicit Stop() does.
/// </summary>
[GlobalClass]
public partial class MusicPlayer : Node
{
    private PlayingBgm? playing;

    public static MusicPlayer Instance { get; private set; } = null!;

    public override void _EnterTree()
    {
        if (Engine.IsEditorHint())
        {
            return;
        }

        Instance = this;
    }

    public void Play(Bgm bgm)
    {
        if (playing is not null && playing.SmPath == bgm.SmPath)
        {
            return;
        }

        // Stop and free the outgoing music before switching, or its player
        // keeps sounding and every switch stacks another.
        playing?.Channel?.Dispose();

        playing = new PlayingBgm
        {
            SmPath = bgm.SmPath,
            Bgm = bgm,
            Fetch = null,
            Channel = null,
            Clock = null,
        };
    }

    public void Stop()
    {
        playing?.Channel?.Dispose();
        playing = null;
    }

    /// <summary>
    /// The beat the speakers are on, for UI synchronized to the music;
    /// <c>null</c> while nothing audible plays.
    /// </summary>
    public Beat? VisibleBeat(TimingSettings settings)
    {
        if (playing?.Clock is not { } clock)
        {
            return null;
        }

        return clock.VisibleBeat(settings);
    }

    /// <summary>
    /// The visible moment on the playing stepfile's timeline together with
    /// its timing, for visuals that animate on the music's own clock.
    /// </summary>
    public (Seconds Position, StepfileTiming Timing)? VisibleNow(TimingSettings settings)
    {
        if (playing?.Clock is not { } clock)
        {
            return null;
        }

        return (clock.VisibleNow(settings), playing.Bgm.Stepfile.Timing);
    }

    /// <summary>
    /// The looping sample window a preview can lay a chart over: the
    /// stepfile's timing and the [start, start+length) window the music
    /// loops. <c>null</c> until the clock exists (the music is playing), and
    /// for music that plays through instead of looping.
    /// </summary>
    public (StepfileTiming Timing, Seconds Start, Seconds Length)? LoopWindow()
    {
        if (playing?.Clock is null)
        {
            return null;
        }

        var stepfile = playing.Bgm.Stepfile;
        var timeline = stepfile.SampleTimeline();

        if (timeline is SoundTimeline.LoopWindow loop)
        {
            return (stepfile.Timing, loop.Start, loop.Length);
        }

        return null;
    }

    public override void _Process(double delta)
    {
        if (Engine.IsEditorHint())
        {
            return;
        }

        if (playing is null)
        {
            return;
        }

        if (playing.Channel is null)
        {
            if (playing.Bgm.Music is null)
            {
                return;
            }

            var poll = (playing.Fetch ??= new AssetLoader(playing.Bgm.Music)).Poll();
            switch (poll)
            {
                case AssetLoader.PollResult.Pending:
                    break;

                case AssetLoader.PollResult.Failed:
                    GD.PushWarning($"music failed to load: {playing.Bgm.Music}");
                    playing.Bgm = playing.Bgm with { Music = null };
                    playing.Fetch = null;
                    break;

                case AssetLoader.PollResult.Ready when poll is AssetLoader.PollResult.Ready ready:
                    playing.Fetch = null;
                    var fileName = System.IO.Path.GetFileName(playing.Bgm.Music);
                    var options = new SoundOptions
                    {
                        Timeline = playing.Bgm.Stepfile.SampleTimeline(),
                        Bus = AudioBuses.Music,
                        Paused = false,
                        Muted = false,
                    };
                    try
                    {
                        playing.Channel = new SoundChannel(this, ready.Bytes, fileName, options);
                    }
                    catch (Exception ex)
                    {
                        GD.PushWarning($"music cannot play: {ex.Message}");
                        playing.Bgm = playing.Bgm with { Music = null };
                    }

                    break;
            }

            return;
        }

        var active = playing.Channel;
        active.Poll();

        if (active.IsFinished)
        {
            active.Dispose();
            if (playing.Clock is not null)
            {
                playing.Channel = null;
            }
            else
            {
                GD.PushWarning($"music finishes instantly, giving up: {playing.Bgm.Music}");
                playing.Bgm = playing.Bgm with { Music = null };
                playing.Channel = null;
            }

            return;
        }

        var report = active.Position;
        playing.Clock ??= StepfileClock.StartAt(playing.Bgm.Stepfile.Timing, report);
        playing.Clock.Advance(new Seconds(delta), report);
    }

    private sealed class PlayingBgm
    {
        public required string SmPath { get; init; }
        public required Bgm Bgm { get; set; }
        public required AssetLoader? Fetch { get; set; }
        public required SoundChannel? Channel { get; set; }
        public required StepfileClock? Clock { get; set; }
    }

    private sealed class AssetLoader
    {
        private readonly string path;
        private byte[]? buffer;
        private bool attempted;

        public AssetLoader(string path)
        {
            this.path = path;
        }

        public PollResult Poll()
        {
            if (buffer is not null)
            {
                return new PollResult.Ready(buffer);
            }

            if (attempted)
            {
                return new PollResult.Failed();
            }

            attempted = true;
            try
            {
                buffer = System.IO.File.ReadAllBytes(path);
                return new PollResult.Ready(buffer);
            }
            catch
            {
                return new PollResult.Failed();
            }
        }

        public abstract record PollResult
        {
            public sealed record Pending : PollResult;
            public sealed record Failed : PollResult;
            public sealed record Ready(byte[] Bytes) : PollResult;
        }
    }
}
