using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>Behaviours scenes share on entry.</summary>
public static class Scenes
{
    /// <summary>
    /// Scenes without music of their own start the default BGM; the player
    /// keeps it running across such scenes uninterrupted.
    /// </summary>
    public static void PlayDefaultBgm() => MusicPlayer.Instance.Play(Library.Instance.DefaultBgm.Bgm());

    /// <summary>
    /// Adds a dimmed, looping background showing the default BGM's album art
    /// behind the scene at z=-100. Does nothing if no background is available.
    /// </summary>
    public static void SpawnDefaultBackground(Control scene)
    {
        var path = Library.Instance.DefaultBgm.BackgroundPath();
        if (path is null) return;
        var cover = MediaCover.Create(path, new Color(0.5f, 0.5f, 0.5f), -100, Seconds.Zero, looping: true, MediaPace.Wall);
        if (cover is not null) scene.AddChild(cover);
    }
}
