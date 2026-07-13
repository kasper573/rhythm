using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// A video decoded by the FFmpeg extension through a hidden
/// <see cref="VideoStreamPlayer"/> whose frame texture the owning
/// <see cref="MediaCover"/> pulls each frame (so the cover keeps its
/// crop-to-fill and modulate). Wall pace plays and loops on its own; Manual
/// pace seeks to the clock the owner writes each frame.
/// </summary>
internal sealed class MediaVideoPlayback
{
    private readonly VideoStreamPlayer player;
    private readonly Seconds start;
    private readonly MediaPace pace;

    private MediaVideoPlayback(VideoStreamPlayer player, Seconds start, MediaPace pace)
    {
        this.player = player;
        this.start = start;
        this.pace = pace;
    }

    /// <summary>
    /// Opens the video under <paramref name="host"/>, or null when the FFmpeg
    /// stream cannot be created. The frame texture arrives a few frames later.
    /// </summary>
    public static MediaVideoPlayback? Open(Node host, string path, bool looping, Seconds start, MediaPace pace)
    {
        if (ClassDB.Instantiate("FFmpegVideoStream").As<VideoStream>() is not { } stream)
        {
            return null;
        }
        stream.Call("set_file", path);

        var player = new VideoStreamPlayer
        {
            Stream = stream,
            Loop = looping,
            Autoplay = true,
            Visible = false,
        };
        host.AddChild(player);
        return new MediaVideoPlayback(player, start, pace);
    }

    /// <summary>The current frame, or null until the first one decodes.</summary>
    public Texture2D? GetTexture() => player.GetVideoTexture();

    /// <summary>Locks a Manual cover to the owner's clock; Wall covers play freely.</summary>
    public void SetClock(Seconds clock)
    {
        if (pace == MediaPace.Manual)
        {
            player.StreamPosition = Math.Max(0.0, (clock - start).Value);
        }
    }

    public void Stop() => player.QueueFree();
}
