using Godot;

namespace Rhythm;

/// <summary>
/// The generic launch directives the game understands, so tooling can drive
/// it without the game knowing anything about the tools. The initial scene
/// comes from <c>--scene &lt;name&gt;</c> (a deep link); with none, the game
/// starts at the main menu.
/// </summary>
public static class Launch
{
    public static GameScene Boot(Game game)
    {
        _ = game;
        return SceneArg(OS.GetCmdlineUserArgs()) ?? GameScene.MainMenu;
    }

    private static GameScene? SceneArg(string[] args)
    {
        var index = Array.IndexOf(args, "--scene");
        if (index < 0 || index + 1 >= args.Length)
        {
            return null;
        }

        return GameScenes.FromDeepLinkName(args[index + 1]);
    }
}
