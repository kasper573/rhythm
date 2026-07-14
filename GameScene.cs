namespace Rhythm;

/// <summary>The scenes the <see cref="Game"/> root swaps between.</summary>
public enum GameScene
{
    MainMenu,
    ModeSelect,
    SettingsMenu,
    Keymap,
    AudioSettings,
    StepfileSelect,
    Play,
    Score,

    /// <summary>Review scenes, reachable only by deep link.</summary>
    GradeSheet,
    NoteDemo,
    VialDemo,
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
            GameScene.StepfileSelect => "stepfile-select",
            GameScene.Play => "play",
            GameScene.Score => "score",
            GameScene.GradeSheet => "grade-sheet",
            GameScene.NoteDemo => "note-demo",
            GameScene.VialDemo => "vial-demo",
            _ => throw new ArgumentOutOfRangeException(nameof(scene)),
        };

    public static GameScene? FromDeepLinkName(string name)
    {
        foreach (var scene in Enum.GetValues<GameScene>())
        {
            if (scene.DeepLinkName() == name)
            {
                return scene;
            }
        }

        return null;
    }
}
