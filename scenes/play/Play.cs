using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The play scene: the real gameplay adapter around the stepfile player.
/// It fills the engine's ports from the audio clock and the keyboard,
/// composes the stage furniture (health vials), and turns the
/// session's end into ScoreResults.
/// </summary>
[GlobalClass]
public partial class Play : Control
{
    private SelectedStepfile? selected;
    private StepfilePlayer? engine;
    private Playback? playback;
    private List<(PlayerId, HealthVial)> vials = [];
    private bool checkFailure;
    private bool finished;

    public override void _Ready()
    {
        if (Engine.IsEditorHint())
        {
            return;
        }

        SetAnchorsAndOffsetsPreset(LayoutPreset.FullRect);

        var game = Game.Instance;
        selected = game.TakeSelectedStepfile();

        if (selected is null)
        {
            game.ChangeScene(GameScene.Wheel);
            return;
        }

        MusicPlayer.Instance.Stop();

        var library = Library.Instance;
        var entry = library.Stepfile(selected.Id);
        var timing = entry.Stepfile.Timing;

        var charts = new List<(PlayerId, Chart)>();
        foreach (var playerChart in selected.Charts)
        {
            if (playerChart.Chart < entry.Stepfile.Charts.Count)
            {
                charts.Add((playerChart.Player, entry.Stepfile.Charts[playerChart.Chart]));
            }
        }

        if (charts.Count == 0)
        {
            game.ChangeScene(GameScene.Wheel);
            return;
        }

        var config = Config.Current!;
        var fieldSpecs = BuildFieldSpecs(charts);

        engine = StepfilePlayer.Instantiate(new StepfilePlayerOptions
        {
            Fields = fieldSpecs,
            Timing = timing,
            Canvas = Screen.Size
        });
        AddChild(engine);

        var lastNoteTime = engine.LastNoteTime;

        foreach (var (player, _) in charts)
        {
            var side = player == PlayerId.P1 ? VialSide.Left : VialSide.Right;
            var vial = HealthVial.Instantiate(new HealthVialOptions
            {
                Fill = 1.0f,
                Side = side,
                EdgePadding = config.Stage?.ScreenEdgePadding ?? 20
            });
            AddChild(vial);
            vials.Add((player, vial));
        }

        playback = new Playback(entry.DisplayTitle(), timing, config.Stage?.LeadIn ?? Seconds.Zero, lastNoteTime);
    }

    private readonly record struct PackSpec(PlayerId Player, uint Columns, NoteSpeed Speed);

    private List<FieldSpec> BuildFieldSpecs(List<(PlayerId, Chart)> charts)
    {
        var config = Config.Current!;
        var settings = Settings.Instance;
        var packs = charts.Select(chart => new PackSpec(chart.Item1, (uint)chart.Item2.Columns, settings.Player(chart.Item1).NoteSpeed)).ToList();
        var layouts = PackStageFields(packs, Screen.Size.X, 1.0f);
        return charts.Select((chart, i) => new FieldSpec
        {
            Layout = layouts[i],
            Rows = chart.Item2.Rows,
            Mines = chart.Item2.Mines,
            MaxHealth = (uint)config.PlayerMaxHealth,
        }).ToList();
    }

    /// <summary>
    /// Sizes and places one field per stage: arrows grow to the configured
    /// pixel cap when the window has room and shrink until every column — plus
    /// the gaps between fields — fits between the reserved screen edges. The
    /// fields pack left-to-right, centered as a block.
    /// </summary>
    private static List<FieldLayout> PackStageFields(IReadOnlyList<PackSpec> specs, float visibleWidth, float pixelsPerUnit)
    {
        var stage = Config.Current!.Stage!;
        var columns = specs.Sum(spec => (int)spec.Columns);
        var gapUnits = stage.FieldGapColumns * (specs.Count - 1);
        var arrowSize = NoteField.FittedArrowSize(columns + gapUnits, visibleWidth - (2.0f * stage.MarginX), NoteField.MaxArrowSize(stage.MaxArrowSize, pixelsPerUnit));

        var layouts = specs.Select(spec => new FieldLayout(spec.Player, 0.0f, spec.Columns, spec.Speed, arrowSize)).ToList();
        var gap = stage.FieldGapColumns * layouts[0].Spacing;
        var total = layouts.Sum(layout => layout.Width) + (gap * (layouts.Count - 1));
        var x = -total / 2.0f;
        for (var i = 0; i < layouts.Count; i++)
        {
            layouts[i] = layouts[i] with { OriginX = x + (layouts[i].Width / 2.0f) };
            x += layouts[i].Width + gap;
        }

        return layouts;
    }

    /// <summary>
    /// Refits the fields to the window every frame: arrows to the fitted size,
    /// the receptor row to its padded top edge, and the grade band to the
    /// padded window — whatever extra world a non-16:9 window reveals.
    /// </summary>
    private void RefitToWindow()
    {
        if (engine is null)
        {
            return;
        }

        var rect = Screen.VisibleRect(this);
        var window = GetWindow();
        var pixelsPerUnit = window is not null ? window.Size.X / Math.Max(rect.Size.X, 1.0f) : 1.0f;
        var settings = Settings.Instance;
        var specs = engine.Players.Zip(engine.FieldLayouts, (player, layout) => new PackSpec(player, layout.Columns, settings.Player(player).NoteSpeed)).ToList();
        if (specs.Count == 0)
        {
            return;
        }

        // The arrow-size cap budgets LOGICAL pixels: hidpi windows report
        // device pixels (a phone's ~3x), which would shrink the cap into
        // arrows a third of their designed screen size.
        var deviceScale = Math.Max(DisplayServer.Singleton.ScreenGetScale(), 1.0f);
        var layouts = PackStageFields(specs, rect.Size.X, pixelsPerUnit / deviceScale);
        var arrowSize = layouts[0].ArrowSize;
        engine.Refit(layouts);
        engine.SetCanvas(rect.Size, pixelsPerUnit);

        var padding = Config.Current!.Stage!.ScreenEdgePadding;
        engine.SetTargetY((rect.Size.Y / 2.0f) - padding - (arrowSize / 2.0f));
        engine.SetGradeArea(GradeText.AreaOf((rect.Size.Y / 2.0f) - padding, (-rect.Size.Y / 2.0f) + padding));
    }

    public override void _Process(double delta)
    {
        if (playback is null || engine is null)
            return;

        RefitToWindow();
        playback.Advance(delta);
        var (graded, visible) = playback.GetClockPorts();
        engine.SetTime(graded, visible);

        WireKeyboard();
        SyncHealthVials();

        if (engine.AllSettled())
        {
            FinishSession();
        }

        HandleCancel();
    }

    private void WireKeyboard()
    {
        if (engine is null || !Game.Instance.AcceptsInput)
        {
            engine?.ClearInput();
            return;
        }

        engine.ClearInput();
        foreach (var player in new[] { PlayerId.P1, PlayerId.P2 })
        {
            for (int col = 0; col < 4; col++)
            {
                var direction = col switch { 0 => StepDirection.Left, 1 => StepDirection.Down, 2 => StepDirection.Up, _ => StepDirection.Right };
                var action = GameActions.Step(player, direction);
                if (Actions.Pressed(action))
                {
                    engine.Press(action, Actions.JustPressed(action));
                }
            }
        }
    }

    private void SyncHealthVials()
    {
        if (engine is null)
            return;

        var beat = engine.VisibleBeat;
        foreach (var (player, vial) in vials)
        {
            var fill = engine.HealthFraction(player);
            if (fill.HasValue)
            {
                vial.SetFill(fill.Value);
                vial.SetBeat(beat);
            }
        }
    }

    private void FinishSession()
    {
        if (finished || engine is null || selected is null || playback is null)
            return;

        finished = true;
        var players = new List<PlayerResult>();
        var results = engine.Results;

        for (int i = 0; i < selected.Charts.Count && i < results.Count; i++)
        {
            players.Add(new PlayerResult(
                selected.Charts[i].Chart,
                results[i]
            ));
        }

        Game.Instance.SetScoreResults(new ScoreResults(
            selected.Id,
            playback.Title,
            players
        ));

        Game.Instance.ChangeScene(GameScene.Score);
    }

    private void HandleCancel()
    {
        if (!Game.Instance.AcceptsInput || engine is null)
            return;

        var cancelled = Actions.AnyJustPressed(engine.Players, p =>
            p == PlayerId.P1 ? GameAction.P1Cancel : GameAction.P2Cancel);
        if (cancelled)
        {
            Sfx.Cancel.Play();
            if (selected is not null)
            {
                Game.Instance.SetWheelTarget(selected.Id);
            }
            Game.Instance.ChangeScene(GameScene.Wheel);
        }
    }

    public override void _ExitTree()
    {
        MusicPlayer.Instance.Stop();
    }
}
