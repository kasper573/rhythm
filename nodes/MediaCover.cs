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
    /// <summary>
    /// Whether the cover has real pixels to show. Owners cross-fading layers
    /// wait for this before retiring what is underneath.
    /// </summary>
    public bool IsReady { get; private set; }

    public static MediaCover? Create(string path, Color color, int z, Seconds start, bool looping, MediaPace pace)
    {
        _ = (start, looping, pace);
        var cover = new MediaCover
        {
            StretchMode = StretchModeEnum.KeepAspectCovered,
            ExpandMode = ExpandModeEnum.IgnoreSize,
            Modulate = color,
            ZIndex = z,
            MouseFilter = MouseFilterEnum.Ignore,
        };
        cover.SetAnchorsAndOffsetsPreset(LayoutPreset.FullRect);

        if (StepfileLibrary.IsVideoFile(path))
        {
            GD.PushWarning($"media cover video not yet supported: {path}");
            cover.QueueFree();
            return null;
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

    /// <summary>
    /// Drives a <see cref="MediaPace.Manual"/> cover's playback clock; image
    /// covers ignore it (video covers, once added, pace on it).
    /// </summary>
    public void SetClock(Seconds clock) => _ = clock;
}
