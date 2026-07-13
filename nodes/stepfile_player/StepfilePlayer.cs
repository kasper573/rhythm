using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// Options for building a StepfilePlayer session.
/// </summary>
public class StepfilePlayerOptions
{
    public required List<FieldSpec> Fields { get; init; }
    public required StepfileTiming Timing { get; init; }
    public required Vector2 Canvas { get; init; }
}

/// <summary>
/// One player's field spec: what to build and how much health backs it.
/// </summary>
public class FieldSpec
{
    public required FieldLayout Layout { get; init; }
    public required IReadOnlyList<Row> Rows { get; init; }
    public required IReadOnlyList<Mine> Mines { get; init; }
    public required uint MaxHealth { get; init; }
}

/// <summary>
/// The stepfile player: the reusable play engine that materializes note
/// fields from chart data, scrolls and animates them in the player's skin
/// and perspective, grades every row, and pops grade words and combos.
///
/// An adapter instantiates it and drives the two ports every frame:
/// `SetTime()` (the clock) and the input port (`ClearInput()` + `Press()`).
/// The engine reads only the ports and reports back through signals
/// and session state.
/// </summary>
[GlobalClass]
public partial class StepfilePlayer : Control
{
    /// <summary>
    /// A press banked into an arrow, with its signed timing error in
    /// seconds (positive = early).
    /// </summary>
    [Signal]
    public delegate void PressBankedEventHandler(double error);

    /// <summary>
    /// A stage drained to zero health. The session's owner decides what
    /// a failure means.
    /// </summary>
    [Signal]
    public delegate void StageFailedEventHandler(int player);

    private StepfileTiming _timing = new(Seconds.Zero, [], []);
    private Vector2 _canvas = Vector2.Zero;
    private float _pixelScale = 1.0f;
    private float _targetY = NoteField.TargetY;
    private GradeArea _gradeArea = new(0.0f, 720.0f);
    private Seconds _gradedNow = Seconds.Zero;
    private Seconds _visibleNow = Seconds.Zero;
    private PlayInput _input = new();
    private List<StageState> _stages = [];
    private List<NoteFieldRig> _rigs = [];
    private List<GradeDisplay> _grades = [];
    private List<ComboDisplay> _combos = [];
    private Node2D? _behind;
    private Node2D? _overlay;
    private Seconds _lastNoteTime = Seconds.Zero;

    public override void _Ready()
    {
        MouseFilter = MouseFilterEnum.Ignore;
        AnchorsPreset = (int)LayoutPreset.FullRect;
    }

    /// <summary>
    /// Builds a session with fields, receptors, notes, grade displays, and combos.
    /// </summary>
    public static StepfilePlayer Instantiate(StepfilePlayerOptions options)
    {
        var player = new StepfilePlayer();
        player._timing = options.Timing;
        player._canvas = options.Canvas;

        var behind = new Node2D();
        player.AddChild(behind);

        foreach (var spec in options.Fields)
        {
            player.BuildField(spec);
        }

        var overlay = new Node2D();
        player.AddChild(overlay);

        player._behind = behind;
        player._overlay = overlay;

        // Add grade displays and combos
        for (int index = 0; index < player._rigs.Count; index++)
        {
            var playerId = player._rigs[index].Layout.Player;
            var originX = player._rigs[index].Layout.OriginX;

            player._grades.Add(new GradeDisplay(behind, playerId, originX));

            var comboLabel = new Label();
            comboLabel.AddThemeColorOverride("font_color", Colors.White);
            var fontFile = GD.Load<FontFile>("res://assets/fonts/JetBrainsMono-Regular.ttf");
            if (fontFile != null)
            {
                comboLabel.AddThemeFontOverride("font", fontFile);
            }
            comboLabel.AddThemeFontSizeOverride("font_size", 44);
            comboLabel.Text = "";
            comboLabel.Visible = false;
            behind.AddChild(comboLabel);

            player._combos.Add(new ComboDisplay(playerId, originX, comboLabel));
        }

        return player;
    }

    /// <summary>
    /// The moment the last note (or hold tail) is over.
    /// </summary>
    public Seconds LastNoteTime => _lastNoteTime;

    /// <summary>
    /// Sets the engine's clock port: grading judges against `graded`;
    /// the note fields draw on `visible`.
    /// </summary>
    public void SetTime(Seconds graded, Seconds visible)
    {
        _gradedNow = graded;
        _visibleNow = visible;
    }

    /// <summary>Clears the frame's input; the adapter refills it every frame.</summary>
    public void ClearInput()
    {
        _input.Held.Clear();
        _input.Struck.Clear();
    }

    /// <summary>Records the panel as held, and freshly struck when `struck` is true.</summary>
    public void Press(GameAction action, bool struck)
    {
        _input.Held.Add(action);
        if (struck)
        {
            _input.Struck.Add(action);
        }
    }

    /// <summary>Anchors the receptor row (canvas-centered y-up).</summary>
    public void SetTargetY(float targetY)
    {
        _targetY = targetY;
    }

    /// <summary>Sets the canvas Y band grade words map their height to.</summary>
    public void SetGradeArea(GradeArea area)
    {
        _gradeArea = area;
    }

    /// <summary>Sets the design canvas and its pixel density.</summary>
    public void SetCanvas(Vector2 canvas, float pixelScale)
    {
        _canvas = canvas;
        _pixelScale = pixelScale;
        foreach (var rig in _rigs)
        {
            rig.SetCanvas(canvas, pixelScale);
        }
    }

    /// <summary>Re-sizes and re-places the fields without respawning them.</summary>
    public void Refit(IEnumerable<FieldLayout> layouts)
    {
        var layoutList = layouts.ToList();
        for (int i = 0; i < _rigs.Count && i < layoutList.Count; i++)
        {
            _rigs[i].SetLayout(layoutList[i]);
            var originX = _rigs[i].Layout.OriginX;
            if (i < _grades.Count)
            {
                _grades[i].SetOriginX(originX);
            }
            if (i < _combos.Count)
            {
                _combos[i].OriginX = originX;
            }
        }
    }

    /// <summary>Whether every stage has either failed or graded its whole chart.</summary>
    public bool AllSettled() => _stages.All(s => s.Failed || s.IsComplete());

    /// <summary>Whether all stages have failed.</summary>
    public bool AllFailed() => _stages.All(s => s.Failed);

    /// <summary>The active players, in field order.</summary>
    public List<PlayerId> Players => _stages.Select(s => s.Player).ToList();

    /// <summary>The visible beat through the session's timing.</summary>
    public Beat VisibleBeat => _timing.BeatAtSeconds(_visibleNow);

    /// <summary>Every field's current layout, in field order.</summary>
    public List<FieldLayout> FieldLayouts => _rigs.Select(r => r.Layout).ToList();

    /// <summary>One player's health as a 0..=1 fraction.</summary>
    public float? HealthFraction(PlayerId player)
    {
        var stage = _stages.FirstOrDefault(s => s.Player == player);
        return stage?.HealthFraction();
    }

    /// <summary>Every stage's results, in field order.</summary>
    public List<StageResults> Results => _stages.Select(s => s.ToResults()).ToList();

    private void BuildField(FieldSpec spec)
    {
        var layout = spec.Layout;
        var noteSkin = NoteSkin.Load("default");

        var rig = NoteFieldRig.Build(
            this,
            layout,
            noteSkin,
            Perspective.None,
            45.0f,
            0.0f,
            _canvas
        );

        var timing = _timing;
        var sessionMines = new List<SessionMine>();
        foreach (var mine in spec.Mines)
        {
            var time = timing.SecondsAtBeat(mine.Beat);
            _lastNoteTime = _lastNoteTime.Max(time);
            var index = rig.SpawnMine(time, mine.Beat, (uint)mine.Column);
            sessionMines.Add(new SessionMine(time, mine.Column, index.Value));
        }

        var sessionRows = new List<SessionRow>();
        foreach (var row in spec.Rows)
        {
            var time = timing.SecondsAtBeat(row.Beat);
            var rowState = new SessionRow(time);

            foreach (var arrow in row.Arrows)
            {
                NoteTail? tailSpec = arrow.Tail is { } tail
                    ? new NoteTail(timing.SecondsAtBeat(tail.End), tail.End, tail.Roll)
                    : null;
                var noteTime = tailSpec?.Time ?? time;
                _lastNoteTime = _lastNoteTime.Max(noteTime);

                var spawn = new NoteSpawn(
                    time,
                    row.Beat,
                    (uint)Math.Abs(arrow.Column),
                    row.Quant,
                    tailSpec
                );

                var noteIndex = rig.SpawnNote(spawn);

                var sessionArrow = new SessionArrow((uint)Math.Abs(arrow.Column), noteIndex.Value);
                if (arrow.Tail.HasValue)
                {
                    sessionArrow.Hold = new HoldState(
                        timing.SecondsAtBeat(arrow.Tail.Value.End),
                        arrow.Tail.Value.Roll
                    );
                }

                rowState.Arrows.Add(sessionArrow);
            }

            sessionRows.Add(rowState);
        }

        // Sort by time (warps can reorder beats)
        sessionRows.Sort((a, b) => a.Time.Value.CompareTo(b.Time.Value));

        _rigs.Add(rig);
        _stages.Add(new StageState(layout.Player, spec.MaxHealth));
        _stages[^1].Rows = sessionRows;
        _stages[^1].Mines = sessionMines;
    }

    public override void _Process(double delta)
    {
        if (_stages.Count == 0) return;

        // Run grading pass
        Grading.RunGradingPass(_stages[0], Config.Current, _gradedNow,
            _timing,
            out var events);

        // Apply grading events
        foreach (var evt in events)
        {
            if (evt is GradingEvent.Graded graded)
            {
                ApplyCombo(graded.Player, graded.Combo);
                var grade = Config.Current.ClassifyGrade(graded.Outcome);

                foreach (var display in _grades)
                {
                    if (display.Player == graded.Player)
                    {
                        display.Apply(Config.Current, graded.Outcome);
                    }
                }

                EmitSignal(SignalName.PressBanked,
                    graded.Outcome is RowOutcome.Hit hit ? hit.Error.Value : 0.0);
            }
            else if (evt is GradingEvent.Failed failed)
            {
                EmitSignal(SignalName.StageFailed, (int)failed.Player);
            }
        }

        // Update fields
        foreach (var rig in _rigs)
        {
            rig.Update(new FieldClock(_visibleNow, _timing, _targetY), (float)delta);
        }

        // Animate grades
        foreach (var display in _grades)
        {
            display.Animate((float)delta, GradeTextConstants.ComboGap);
        }

        // Animate combos
        foreach (var combo in _combos)
        {
            combo.Animate((float)delta);
        }
    }

    private void ApplyCombo(PlayerId player, uint combo)
    {
        foreach (var display in _combos)
        {
            if (display.Player == player)
            {
                display.Combo = combo;
                if (combo == 0)
                {
                    display.Label.Visible = false;
                }
                else
                {
                    display.Label.Visible = true;
                    display.Label.Text = $"{combo} combo";
                }
            }
        }
    }
}

/// <summary>Input tracking for one frame.</summary>
internal class PlayInput
{
    public List<GameAction> Held { get; } = [];
    public List<GameAction> Struck { get; } = [];

    public bool IsHeld(GameAction action) => Held.Contains(action);
    public bool IsStruck(GameAction action) => Struck.Contains(action);
}

/// <summary>One player's combo display with bounce animation.</summary>
internal class ComboDisplay
{
    private static readonly Seconds ComboBounce = new(0.18);

    public PlayerId Player { get; }
    public float OriginX { get; set; }
    public Label Label { get; }
    public uint Combo { get; set; }
    private Seconds _bounce;

    public ComboDisplay(PlayerId player, float originX, Label label)
    {
        Player = player;
        OriginX = originX;
        Label = label;
        Combo = 0;
        _bounce = Seconds.Zero;
    }

    public void Animate(float delta)
    {
        _bounce = new Seconds(Math.Max(0.0, _bounce.Value - delta));

        if (_bounce.Value > 0.0)
        {
            var t = (float)(1.0 - _bounce.Value / ComboBounce.Value);
            var easeOut = Mathf.Sin(t * Mathf.Pi / 2.0f);
            var scale = 1.0f + (1.0f - easeOut) * 0.15f;
            Label.Scale = Vector2.One * scale;
        }
        else if (!Label.Scale.IsEqualApprox(Vector2.One))
        {
            Label.Scale = Vector2.One;
        }

        Label.Position = new Vector2(OriginX, -GradeTextConstants.ComboGap);
    }
}
