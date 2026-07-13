namespace Rhythm;

/// <summary>The scenes the <see cref="Game"/> root swaps between.</summary>
public enum GameScene
{
    MainMenu,
    ModeSelect,
    SettingsMenu,
    Keymap,
    AudioSettings,
    Wheel,
    Play,
    Score,

    /// <summary>Review scenes, reachable only by deep link.</summary>
    GradeSheet,
    NoteDemo,
}

public static class GameScenes
{
    /// <summary>The kebab-case name a deep link (<c>--scene main-menu</c>) uses.</summary>
    public static string DeepLinkName(this GameScene scene) =>
        scene switch
        {
            GameScene.MainMenu => "main-menu",
            GameScene.ModeSelect => "mode-select",
            GameScene.SettingsMenu => "settings-menu",
            GameScene.Keymap => "keymap",
            GameScene.AudioSettings => "audio-settings",
            GameScene.Wheel => "wheel",
            GameScene.Play => "play",
            GameScene.Score => "score",
            GameScene.GradeSheet => "grade-sheet",
            GameScene.NoteDemo => "note-demo",
            _ => throw new ArgumentOutOfRangeException(nameof(scene)),
        };

    public static GameScene? FromDeepLinkName(string name) =>
        Enum.GetValues<GameScene>()
            .Cast<GameScene?>()
            .FirstOrDefault(scene => scene!.Value.DeepLinkName() == name);
}
