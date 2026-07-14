using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// One playing sound: an AudioStreamPlayer owned by this handle and parented
/// under an owner. The owner calls Poll() every frame — it starts playback
/// once the player has entered the tree, wraps loop windows, and keeps the
/// finish flag honest — and disposing the channel stops and frees the player.
/// </summary>
public sealed class SoundChannel : IDisposable
{
    private readonly AudioStreamPlayer player;
    private readonly SoundTimeline timeline;
    private bool muted;
    private bool paused;
    private bool finished;
    private bool started;

    public SoundChannel(Node parent, byte[] bytes, string fileName, SoundOptions options)
        : this(parent, LoadStream(bytes, fileName) ?? throw new InvalidOperationException($"unsupported sound format: {fileName}"), options)
    {
    }

    public SoundChannel(Node parent, AudioStream stream, SoundOptions options)
    {
        player = new AudioStreamPlayer();
        timeline = options.Timeline;
        muted = options.Muted;
        paused = options.Paused;

        player.Stream = stream;
        player.Bus = options.Bus;
        parent.AddChild(player);

        player.Finished += () =>
        {
            finished = true;
        };

        ApplyGain();
        EnsureStarted();
    }

    /// <summary>
    /// Starts pending playback, wraps loop windows, and restarts a windowed
    /// file that ran out early; the owner calls this every frame.
    /// </summary>
    public void Poll()
    {
        EnsureStarted();

        if (timeline is not SoundTimeline.LoopWindow window)
        {
            return;
        }

        var start = Math.Max(window.Start.Value, 0.0);
        var end = start + Math.Max(window.Length.Value, 0.0);

        if (finished)
        {
            finished = false;
            player.Play();
            player.Seek((float)start);
            return;
        }

        var position = player.GetPlaybackPosition();
        if (position >= end)
        {
            var wrapped = start + (position - end) % Math.Max(window.Length.Value, 1e-6);
            player.Seek((float)wrapped);
        }
    }

    public void SetPaused(bool pause)
    {
        paused = pause;
        if (started)
        {
            player.StreamPaused = pause;
        }
        else if (!pause)
        {
            EnsureStarted();
        }
    }

    public void SetMuted(bool mute)
    {
        muted = mute;
        ApplyGain();
    }

    public bool IsMuted => muted;

    /// <summary>
    /// Seconds into the sound's own timeline, on the mixer-queue clock:
    /// the player's position plus what the mixer consumed since its last
    /// report. Runs ahead of the speakers by the output latency, which the
    /// timing settings compensate for.
    /// </summary>
    public Seconds Position
    {
        get
        {
            if (!started)
            {
                var startPos = timeline switch
                {
                    SoundTimeline.WholeFile => 0.0,
                    SoundTimeline.From from => Math.Max(from.Position.Value, 0.0),
                    SoundTimeline.LoopWindow loop => Math.Max(loop.Start.Value, 0.0),
                    _ => 0.0,
                };
                return new Seconds(startPos);
            }

            var position = (double)player.GetPlaybackPosition();
            if (player.Playing && !player.StreamPaused)
            {
                position += AudioServer.Singleton.GetTimeSinceLastMix();
            }

            return new Seconds(position);
        }
    }

    /// <summary>The sound ran out; looping windows never finish.</summary>
    public bool IsFinished => timeline is not SoundTimeline.LoopWindow && finished;

    private void EnsureStarted()
    {
        if (started || !player.IsInsideTree())
        {
            return;
        }

        var startPos = timeline switch
        {
            SoundTimeline.WholeFile => 0.0f,
            SoundTimeline.From from => (float)Math.Max(from.Position.Value, 0.0),
            SoundTimeline.LoopWindow loop => (float)Math.Max(loop.Start.Value, 0.0),
            _ => 0.0f,
        };

        player.Play();
        if (startPos > 0.0f)
        {
            player.Seek(startPos);
        }

        if (paused)
        {
            player.StreamPaused = true;
        }

        started = true;
    }

    private void ApplyGain()
    {
        player.VolumeDb = muted ? -80.0f : 0.0f;
    }

    private static AudioStream? LoadStream(byte[] bytes, string fileName)
    {
        var extension = System.IO.Path.GetExtension(fileName).ToLowerInvariant().TrimStart('.');

        return extension switch
        {
            "ogg" => AudioStreamOggVorbis.LoadFromBuffer(bytes),
            "mp3" => AudioStreamMP3.LoadFromBuffer(bytes),
            "wav" => AudioStreamWav.LoadFromBuffer(bytes),
            _ => null,
        };
    }

    public void Dispose()
    {
        if (player is not null)
        {
            player.QueueFree();
        }
    }
}
