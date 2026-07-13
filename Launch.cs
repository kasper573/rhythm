using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The generic launch directives the game understands, so tooling can drive
/// it without the game knowing anything about the tools. The initial scene
/// comes from <c>--scene &lt;name&gt;</c> (a deep link); with none, the game
/// starts at the main menu. Deep links carry their scene's params — the note
/// demo reads <c>--scenario/--skin/--bpm/--perspective</c>, the play scene
/// reads <c>--stepfile &lt;group&gt;/&lt;title&gt;</c> — which land in the
/// <see cref="Game"/> mailbox for the entered scene to consume.
/// </summary>
public static class Launch
{
    public static GameScene Boot(Game game)
    {
        var args = OS.GetCmdlineUserArgs();
        var scene = SceneArg(args) ?? GameScene.MainMenu;

        switch (scene)
        {
            case GameScene.NoteDemo:
                game.SetNoteDemo(new NoteDemoParams(
                    Scenario: Value(args, "--scenario"),
                    Skin: Value(args, "--skin"),
                    Perspective: Value(args, "--perspective") ?? "None",
                    Bpm: double.TryParse(Value(args, "--bpm"), System.Globalization.CultureInfo.InvariantCulture, out var bpm) ? bpm : 120.0));
                break;

            case GameScene.Play when Value(args, "--stepfile") is { } spec:
                if (ResolveStepfile(game, spec) is { } selected)
                {
                    game.SetSelectedStepfile(selected);
                }

                break;
        }

        return scene;
    }

    /// <summary>Builds the play param for a <c>group/title</c> deep link: the named stepfile, each player on their nearest chart.</summary>
    private static SelectedStepfile? ResolveStepfile(Game game, string spec)
    {
        var slash = spec.IndexOf('/');
        if (slash < 0)
        {
            return null;
        }

        var groupName = spec[..slash];
        var title = spec[(slash + 1)..];
        var library = Library.Instance;
        for (var group = 0; group < library.Groups.Count; group++)
        {
            if (library.Groups[group].Name != groupName)
            {
                continue;
            }

            var stepfiles = library.Groups[group].Stepfiles;
            for (var index = 0; index < stepfiles.Count; index++)
            {
                if (stepfiles[index].DisplayTitle() != title && stepfiles[index].Name() != title)
                {
                    continue;
                }

                var stepsType = game.PlayMode.StepsType();
                var charts = new List<PlayerChart>();
                foreach (var player in game.PlayMode.Players())
                {
                    if (stepfiles[index].Stepfile.ClosestChart(stepsType, game.PreferredDifficulty[player]) is { } chart)
                    {
                        charts.Add(new PlayerChart(player, chart));
                    }
                }

                return charts.Count > 0 ? new SelectedStepfile(new StepfileId(group, index), charts) : null;
            }
        }

        return null;
    }

    private static GameScene? SceneArg(string[] args) =>
        Value(args, "--scene") is { } name ? GameScenes.FromDeepLinkName(name) : null;

    private static string? Value(string[] args, string flag)
    {
        var index = Array.IndexOf(args, flag);
        return index >= 0 && index + 1 < args.Length ? args[index + 1] : null;
    }
}
