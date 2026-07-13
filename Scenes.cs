namespace Rhythm;

/// <summary>Behaviours scenes share on entry.</summary>
public static class Scenes
{
    /// <summary>
    /// Scenes without music of their own start the default BGM; the player
    /// keeps it running across such scenes uninterrupted.
    /// </summary>
    public static void PlayDefaultBgm() => MusicPlayer.Instance.Play(Library.Instance.DefaultBgm.Bgm());
}
