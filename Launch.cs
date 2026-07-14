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
///
/// Input automation and frame reporting:
/// <c>--pulse &lt;action&gt;[:&lt;seconds&gt;]</c> taps an action on a cycle.
/// <c>--hold &lt;action&gt;</c> holds an action down.
/// <c>--frame-report &lt;file&gt;</c> writes per-frame delta times as JSON on exit.
/// <c>--quit-after-seconds &lt;s&gt;</c> quits after this duration.
/// </summary>
public static class Launch
{
    public static GameScene Boot(Game game)
    {
        var args = OS.GetCmdlineUserArgs();
        var scene = SceneArg(args) ?? GameScene.MainMenu;

        if (Value(args, "--mode") is { } mode)
        {
            game.PlayMode = ParsePlayMode(mode);
        }

        if (Value(args, "--difficulty") is { } rank && byte.TryParse(rank, out var difficulty))
        {
            var preferred = game.PreferredDifficulty;
            preferred.P1 = difficulty;
            preferred.P2 = difficulty;
            game.PreferredDifficulty = preferred;
        }

        switch (scene)
        {
            case GameScene.NoteDemo:
                game.SetNoteDemo(new NoteDemoParams(
                    Scenario: Value(args, "--scenario"),
                    Skin: Value(args, "--skin"),
                    Perspective: Value(args, "--perspective") ?? "None",
                    Bpm: new Bpm(double.TryParse(Value(args, "--bpm"), System.Globalization.CultureInfo.InvariantCulture, out var bpm) ? bpm : 120.0)));
                break;

            case GameScene.Play when Value(args, "--stepfile") is { } spec:
                if (ResolveStepfile(game, spec) is { } selected)
                {
                    game.SetSelectedStepfile(selected);
                }

                break;
        }

        (GameAction, double)? pulse = null;
        if (Value(args, "--pulse") is { } pulseSpec)
        {
            pulse = ParsePulse(pulseSpec);
        }

        GameAction? hold = null;
        if (Value(args, "--hold") is { } holdSpec)
        {
            hold = ParseAction(holdSpec);
        }

        var frameReport = Value(args, "--frame-report");

        double? quitAfter = null;
        if (Value(args, "--quit-after-seconds") is { } quitSpec && double.TryParse(quitSpec, System.Globalization.CultureInfo.InvariantCulture, out var seconds))
        {
            quitAfter = seconds;
        }

        if (pulse is not null || hold is not null || frameReport is not null || quitAfter is not null)
        {
            var rig = new LaunchRig
            {
                Pulse = pulse,
                Hold = hold,
                FrameReport = frameReport,
                QuitAfter = quitAfter,
            };
            game.AddChild(rig);
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

    private static PlayMode ParsePlayMode(string mode)
    {
        return mode.ToLowerInvariant() switch
        {
            "singles" => PlayMode.Singles,
            "doubles" => PlayMode.Doubles,
            "versus" => PlayMode.Versus,
            _ => throw new InvalidOperationException($"unknown --mode {mode}; one of: singles, doubles, versus"),
        };
    }

    private static GameAction ParseAction(string name)
    {
        if (Enum.TryParse<GameAction>(name, ignoreCase: true, out var action))
        {
            return action;
        }

        throw new InvalidOperationException($"unknown action {name}");
    }

    private static (GameAction, double) ParsePulse(string spec)
    {
        var parts = spec.Split(':', 2);
        var action = ParseAction(parts[0]);
        var interval = parts.Length > 1
            ? double.Parse(parts[1], System.Globalization.CultureInfo.InvariantCulture)
            : 0.5;
        return (action, interval);
    }
}
