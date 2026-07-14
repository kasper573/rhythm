using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>What paces a cover's playback clock.</summary>
public enum MediaPace
{
    /// <summary>Wall time — for backgrounds that simply play.</summary>
    Wall,

    /// <summary>
    /// The owner writes the clock every frame — for backgrounds locked to
    /// another timeline, like the play stage's music.
    /// </summary>
    Manual,
}

/// <summary>
/// A viewport-covering surface showing a media file, scaled to fill and
/// cropped, never stretched. The one way scenes put a full-screen background
/// on stage. The node renders; the owner orchestrates through its modulate.
/// <see cref="Create"/> returns <c>null</c> (after a warning) when the media
/// cannot be shown, so callers keep whatever they already display.
/// </summary>
[GlobalClass]
public partial class MediaCover : TextureRect
{
    private MediaVideoPlayback? _videoPlayback;
    private MediaPace _pace;

    /// <summary>
    /// Whether the cover has real pixels to show. Owners cross-fading layers
    /// wait for this before retiring what is underneath.
    /// </summary>
    public bool IsReady { get; private set; }

    public static MediaCover? Create(string path, Color color, int z, Seconds start, bool looping, MediaPace pace)
    {
        var cover = new MediaCover
        {
            StretchMode = StretchModeEnum.KeepAspectCovered,
            ExpandMode = ExpandModeEnum.IgnoreSize,
            Modulate = color,
            ZIndex = z,
            MouseFilter = MouseFilterEnum.Ignore,
            _pace = pace,
        };
        cover.SetAnchorsAndOffsetsPreset(LayoutPreset.FullRect);

        if (StepfileLibrary.IsVideoFile(path))
        {
            var playback = MediaVideoPlayback.Open(cover, path, looping, start, pace);
            if (playback is null)
            {
                GD.PushWarning($"media cover video unavailable: {path}");
                cover.QueueFree();
                return null;
            }
            // The decoder's first frame arrives a few frames later; the cover
            // stays not-ready (invisible to cross-fading owners) until then.
            cover._videoPlayback = playback;
            return cover;
        }

        var image = new Image();
        if (image.Load(path) != Error.Ok)
        {
            GD.PushWarning($"media cover image cannot load: {path}");
            cover.QueueFree();
            return null;
        }

        cover.Texture = ImageTexture.CreateFromImage(image);
        cover.IsReady = true;
        return cover;
    }

    public override void _Process(double delta)
    {
        if (_videoPlayback?.GetTexture() is { } texture)
        {
            Texture = texture;
            IsReady = true;
        }
    }

    /// <summary>
    /// Drives a <see cref="MediaPace.Manual"/> cover's playback clock; image
    /// covers ignore it (video covers pace on it).
    /// </summary>
    public void SetClock(Seconds clock)
    {
        if (_videoPlayback is not null)
        {
            _videoPlayback.SetClock(clock);
        }
    }
}
