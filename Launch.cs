using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The generic launch directives the game understands, so tooling can drive
/// it without the game knowing anything about the tools. The initial scene
/// comes from <c>--scene &lt;name&gt;</c> (a deep link); with none, the game
/// starts at the main menu. Deep links carry their scene's params — the note
/// demo reads <c>--scenario/--skin/--bpm/--perspective</c> — which land in the
/// <see cref="Game"/> mailbox for the entered scene to consume.
/// </summary>
public static class Launch
{
    public static GameScene Boot(Game game)
    {
        var args = OS.GetCmdlineUserArgs();
        var scene = SceneArg(args) ?? GameScene.MainMenu;

        if (scene == GameScene.NoteDemo)
        {
            game.SetNoteDemo(new NoteDemoParams(
                Scenario: Value(args, "--scenario"),
                Skin: Value(args, "--skin"),
                Perspective: Value(args, "--perspective") ?? "None",
                Bpm: double.TryParse(Value(args, "--bpm"), System.Globalization.CultureInfo.InvariantCulture, out var bpm) ? bpm : 120.0));
        }

        return scene;
    }

    private static GameScene? SceneArg(string[] args) =>
        Value(args, "--scene") is { } name ? GameScenes.FromDeepLinkName(name) : null;

    private static string? Value(string[] args, string flag)
    {
        var index = Array.IndexOf(args, flag);
        return index >= 0 && index + 1 < args.Length ? args[index + 1] : null;
    }
}
