using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The play scene: the real gameplay adapter around the stepfile player.
/// It fills the engine's ports from the audio clock and the keyboard,
/// composes the stage furniture (health vials, backgrounds, tuning HUD),
/// and turns the session's end into ScoreResults.
/// </summary>
[GlobalClass]
public partial class Play : Control
{
    private SelectedStepfile? selected;
    private StepfilePlayer? engine;
    private Playback? playback;
    private List<(PlayerId, HealthVial)> vials = [];
    private Backgrounds? backgrounds;
    private Tuning? tuning;
    private SoundChannel? musicChannel;
    private SoundChannel? tickChannel;
    private AssetLoader? musicFetch;
    private string musicFileName = "";
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
            game.ChangeScene(GameScene.StepfileSelect);
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
            game.ChangeScene(GameScene.StepfileSelect);
            return;
        }

        var config = Config.Current!;
        var fieldSpecs = BuildFieldSpecs(charts);

        // The background sits behind everything: added first so the note
        // field, vials, and HUD draw on top of it.
        backgrounds = new Backgrounds(this, entry, timing);

        engine = StepfilePlayer.Instantiate(new StepfilePlayerOptions
        {
            Fields = fieldSpecs,
            Timing = timing,
            Canvas = Screen.Size
        });
        AddChild(engine);

        // Wire the signals
        engine.Connect(StepfilePlayer.SignalName.PressBanked, Callable.From((float error) =>
        {
            tuning?.PushSample(new Seconds(error));
        }));
        engine.Connect(StepfilePlayer.SignalName.StageFailed, Callable.From((int player) =>
        {
            Sfx.Fail.Play();
            checkFailure = true;
        }));

        var lastNoteTime = engine.LastNoteTime;

        // Add health vials
        foreach (var (player, _) in charts)
        {
            var side = player == PlayerId.P1 ? VialSide.Left : VialSide.Right;
            var vial = new HealthVial { Side = side };
            vial.SetFill(1.0f);
            AddChild(vial);
            vials.Add((player, vial));
        }

        // Initialize tuning HUD
        tuning = new Tuning(this);

        // Render tick track
        var tickTimes = new List<Seconds>();
        foreach (var (_, chart) in charts)
        {
            foreach (var row in chart.Rows)
            {
                var seconds = timing.SecondsAtBeat(row.Beat);
                if (!tickTimes.Contains(seconds))
                {
                    tickTimes.Add(seconds);
                }
            }
        }
        tickTimes.Sort((a, b) => a.Value.CompareTo(b.Value));

        var tickWavPath = Assets.Path("sfx/tick.wav");
        try
        {
            if (TickTrackRenderer.Render(File.ReadAllBytes(tickWavPath), tickTimes, config.TickVolume) is { } tickTrack)
            {
                tickChannel = new SoundChannel(this, tickTrack, new SoundOptions
                {
                    Timeline = new SoundTimeline.WholeFile(),
                    Paused = true,
                    Muted = true,
                    Bus = AudioBuses.Sfx,
                });
            }
        }
        catch (Exception ex)
        {
            GD.PushWarning($"could not render tick track: {ex.Message}");
        }

        // Start music loading
        var musicPath = entry.MusicPath();
        if (musicPath == null)
        {
            GD.Print($"no music file for \"{entry.DisplayTitle()}\", playing silent");
        }
        else
        {
            musicFileName = System.IO.Path.GetFileName(musicPath);
            musicFetch = new AssetLoader(musicPath);
        }

        playback = new Playback(entry.DisplayTitle(), timing, config.Stage?.LeadIn ?? Seconds.Zero, lastNoteTime);
    }

    private readonly record struct PackSpec(PlayerId Player, uint Columns, NoteSpeed Speed);

    private static List<FieldSpec> BuildFieldSpecs(List<(PlayerId, Chart)> charts)
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
            GradeLayer = settings.Player(chart.Item1).GradeLayer,
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

        PlaceVials(rect, pixelsPerUnit / deviceScale);
    }

    /// <summary>
    /// Keeps each vial a fixed <see cref="StageConfig.ScreenEdgePadding"/>
    /// screen pixels from the top, bottom, and its player's edge, at every
    /// window size and aspect; only its on-screen width clamps.
    /// </summary>
    private void PlaceVials(Rect2 rect, float pixelScale)
    {
        pixelScale = Math.Max(pixelScale, 0.001f);
        var inset = Config.Current!.Stage!.ScreenEdgePadding / pixelScale;
        var widthOnScreen = Math.Clamp(HealthVial.NaturalWidth * pixelScale, HealthVial.MinScreenWidth, HealthVial.MaxScreenWidth);
        var width = widthOnScreen / pixelScale;
        var top = rect.Position.Y + inset;
        var height = Math.Max(rect.Size.Y - (2.0f * inset), 1.0f);
        foreach (var (player, vial) in vials)
        {
            var left = player == PlayerId.P1
                ? rect.Position.X + inset
                : rect.Position.X + rect.Size.X - inset - width;
            vial.Place(new Rect2(left, top, width, height));
        }
    }

    public override void _Process(double delta)
    {
        if (playback is null || engine is null)
            return;

        PollMusic();
        RefitToWindow();
        playback.Advance(delta, musicChannel, tickChannel, musicFetch != null);
        var (graded, visible) = playback.GetClockPorts();
        engine.SetTime(graded, visible);

        // Update backgrounds with visible time
        if (backgrounds is not null)
        {
            backgrounds.Update(playback.VisibleNow, (float)delta);
        }

        WireKeyboard();
        SyncHealthVials();
        tuning?.Update(tickChannel, delta);

        FinishWhenComplete();
        checkFailure = false;
        HandleCancel();
    }

    /// <summary>
    /// Opens the music channel (paused) once its bytes arrive; failures
    /// drop the music and the session plays with whatever survives.
    /// </summary>
    private void PollMusic()
    {
        if (musicFetch is null)
        {
            return;
        }

        var poll = musicFetch.Poll();
        if (poll is AssetLoader.PollResult.Ready ready)
        {
            musicFetch = null;
            try
            {
                musicChannel = new SoundChannel(this, ready.Bytes, musicFileName, new SoundOptions
                {
                    Timeline = new SoundTimeline.WholeFile(),
                    Paused = true,
                    Muted = false,
                    Bus = AudioBuses.Music,
                });
            }
            catch (Exception ex)
            {
                GD.PushWarning($"music cannot play: {ex.Message}");
            }
        }
        else if (poll is AssetLoader.PollResult.Failed)
        {
            GD.PushWarning("music failed to load");
            musicFetch = null;
        }
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

    /// <summary>
    /// The session ends when every stage settled and the audio ran out (or
    /// nothing plays and the chart is over); the grades given so far become
    /// the final result.
    /// </summary>
    private void FinishWhenComplete()
    {
        if (finished)
        {
            return;
        }

        if (engine is null || playback is null)
        {
            return;
        }

        // Check if all stages have failed (hard stop)
        var failedOut = checkFailure && engine.AllFailed();
        if (!failedOut)
        {
            // Check if all stages settled (grading complete)
            if (!engine.AllSettled())
            {
                return;
            }

            // Check if audio has finished
            var audioDone = musicChannel?.IsFinished ?? (musicFetch is null && tickChannel?.IsFinished != false);

            // Trailing mines and hold tails can outlive the audio; wait for them.
            var chartDone = playback.Position.Value >= playback.LastNoteTime.Value;

            // Only finish if both audio and chart are done
            if (!audioDone || !chartDone || !playback.IsPlaying)
            {
                return;
            }
        }

        finished = true;
        if (selected is null)
        {
            return;
        }

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
            Game.Instance.ChangeScene(GameScene.StepfileSelect);
        }
    }

    public override void _ExitTree()
    {
        backgrounds?.Dispose();
        tuning?.Dispose();
        MusicPlayer.Instance.Stop();
    }
}

/// <summary>
/// Loads an asset file asynchronously.
/// </summary>
internal sealed class AssetLoader
{
    private readonly string path;
    private byte[]? buffer;
    private bool attempted;

    public AssetLoader(string path)
    {
        this.path = path;
    }

    public PollResult Poll()
    {
        if (buffer is not null)
        {
            return new PollResult.Ready(buffer);
        }

        if (attempted)
        {
            return new PollResult.Failed();
        }

        attempted = true;
        try
        {
            buffer = File.ReadAllBytes(path);
            return new PollResult.Ready(buffer);
        }
        catch
        {
            return new PollResult.Failed();
        }
    }

    internal abstract record PollResult
    {
        public sealed record Pending : PollResult;
        public sealed record Failed : PollResult;
        public sealed record Ready(byte[] Bytes) : PollResult;
    }
}
