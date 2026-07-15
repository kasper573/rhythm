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

    /// <summary>Whether this player's grade word and combo pop out behind the
    /// arrows or in front of them.</summary>
    public required GradeLayer GradeLayer { get; init; }
}

/// <summary>
/// The stepfile player: the reusable play driver that materializes note
/// fields from chart data, scrolls and animates them in the player's skin
/// and perspective, grades every row, and pops grade words and combos.
///
/// An adapter instantiates it and drives the two ports every frame:
/// `SetTime()` (the clock) and the input port (`ClearInput()` + `Press()`).
/// It reads only the ports and reports back through signals
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

    private StepfileTiming timing = new(Seconds.Zero, [], []);
    private Vector2 canvas = Vector2.Zero;
    private float pixelScale = 1.0f;
    private float targetY = NoteField.TargetY;
    private GradeArea gradeArea = new(0.0f, 720.0f);
    private Seconds gradedNow = Seconds.Zero;
    private Seconds visibleNow = Seconds.Zero;
    private PlayInput input = new();
    private List<StageState> stages = [];
    private List<NoteFieldRig> rigs = [];
    private List<GradeDisplay> grades = [];
    private List<ComboDisplay> combos = [];
    private List<Fading2d> fades = [];
    private Node2D? behind;
    private Node2D? overlay;

    public override void _Ready()
    {
        MouseFilter = MouseFilterEnum.Ignore;
        SetAnchorsAndOffsetsPreset(LayoutPreset.FullRect);
    }

    /// <summary>
    /// Builds a session with fields, receptors, notes, grade displays, and combos.
    /// </summary>
    public static StepfilePlayer Instantiate(StepfilePlayerOptions options)
    {
        var player = new StepfilePlayer();
        player.timing = options.Timing;
        player.canvas = options.Canvas;

        var behind = new Node2D();
        player.AddChild(behind);

        foreach (var spec in options.Fields)
        {
            player.BuildField(spec);
        }

        var overlay = new Node2D();
        player.AddChild(overlay);

        player.behind = behind;
        player.overlay = overlay;

        // The grade word and the combo under it pop out on the player's chosen
        // layer: behind the arrows, or in the overlay in front of them.
        for (int index = 0; index < player.rigs.Count; index++)
        {
            var playerId = player.rigs[index].Layout.Player;
            var originX = player.rigs[index].Layout.OriginX;
            var gradeLayer = options.Fields[index].GradeLayer == GradeLayer.InFront ? overlay : behind;

            player.grades.Add(new GradeDisplay(gradeLayer, playerId, originX));

            var comboLabel = Text.Label(string.Empty, 44.0f, Colors.White);
            comboLabel.Visible = false;
            gradeLayer.AddChild(comboLabel);

            player.combos.Add(new ComboDisplay(playerId, originX, comboLabel));
        }

        return player;
    }

    /// <summary>
    /// Sets the clock port: grading judges against `graded`;
    /// the note fields draw on `visible`.
    /// </summary>
    public void SetTime(Seconds graded, Seconds visible)
    {
        gradedNow = graded;
        visibleNow = visible;
    }

    /// <summary>Clears the frame's input; the adapter refills it every frame.</summary>
    public void ClearInput()
    {
        input.Held.Clear();
        input.Struck.Clear();
    }

    /// <summary>Records the panel as held, and freshly struck when `struck` is true.</summary>
    public void Press(GameAction action, bool struck)
    {
        input.Held.Add(action);
        if (struck)
        {
            input.Struck.Add(action);
        }
    }

    /// <summary>Anchors the receptor row (canvas-centered y-up).</summary>
    public void SetTargetY(float targetY)
    {
        this.targetY = targetY;
    }

    /// <summary>Sets the canvas Y band grade words map their height to.</summary>
    public void SetGradeArea(GradeArea area)
    {
        gradeArea = area;
    }

    /// <summary>Sets the design canvas and its pixel density.</summary>
    public void SetCanvas(Vector2 canvas, float pixelScale)
    {
        this.canvas = canvas;
        this.pixelScale = pixelScale;
        foreach (var rig in rigs)
        {
            rig.SetCanvas(canvas, pixelScale);
        }
    }

    /// <summary>Eases a player's lane camera to a new perspective in place, so
    /// changing it live glides rather than snapping.</summary>
    public void SetPerspective(PlayerId player, Perspective perspective)
    {
        foreach (var rig in rigs)
        {
            if (rig.Layout.Player == player)
            {
                rig.SetPerspective(perspective);
            }
        }
    }

    /// <summary>Re-sizes and re-places the fields without respawning them.</summary>
    public void Refit(IEnumerable<FieldLayout> layouts)
    {
        var layoutList = layouts.ToList();
        for (int i = 0; i < rigs.Count && i < layoutList.Count; i++)
        {
            rigs[i].SetLayout(layoutList[i]);
            var originX = rigs[i].Layout.OriginX;
            if (i < grades.Count)
            {
                grades[i].SetOriginX(originX);
            }
            if (i < combos.Count)
            {
                combos[i].OriginX = originX;
            }
        }
    }

    /// <summary>Whether every stage has either failed or graded its whole chart.</summary>
    public bool AllSettled() => stages.All(s => s.Failed || s.IsComplete());

    /// <summary>The active players, in field order.</summary>
    public List<PlayerId> Players => stages.Select(s => s.Player).ToList();

    /// <summary>The visible beat through the session's timing.</summary>
    public Beat VisibleBeat => timing.BeatAtSeconds(visibleNow);

    /// <summary>Every field's current layout, in field order.</summary>
    public List<FieldLayout> FieldLayouts => rigs.Select(r => r.Layout).ToList();

    /// <summary>One player's health as a 0..=1 fraction.</summary>
    public float? HealthFraction(PlayerId player)
    {
        var stage = stages.FirstOrDefault(s => s.Player == player);
        return stage?.HealthFraction();
    }

    /// <summary>Every stage's results, in field order.</summary>
    public List<StageResults> Results => stages.Select(s => s.ToResults()).ToList();

    private void BuildField(FieldSpec spec)
    {
        var layout = spec.Layout;
        var options = Settings.Instance.Player(layout.Player);
        var camera = Config.Current.LaneCamera
            ?? throw new InvalidOperationException("LaneCamera is not configured");

        var rig = NoteFieldRig.Build(
            this,
            layout,
            NoteSkin.Load(options.NoteSkin),
            options.Perspective,
            camera.FovDegrees,
            camera.TiltDegrees,
            canvas
        );

        var timing = this.timing;
        var sessionMines = new List<SessionMine>();
        foreach (var mine in spec.Mines)
        {
            var time = timing.SecondsAtBeat(mine.Beat);
            var index = rig.SpawnMine(time, mine.Beat, (uint)mine.Column);
            sessionMines.Add(new SessionMine(time, (uint)mine.Column, index));
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

                var spawn = new NoteSpawn(
                    time,
                    row.Beat,
                    (uint)Math.Abs(arrow.Column),
                    row.Quant,
                    tailSpec
                );

                var noteIndex = rig.SpawnNote(spawn);

                var sessionArrow = new SessionArrow((uint)Math.Abs(arrow.Column), noteIndex);
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

        // Warps can reorder beats, so time order isn't beat order.
        sessionRows.Sort((a, b) => a.Time.Value.CompareTo(b.Time.Value));

        rigs.Add(rig);
        stages.Add(new StageState(layout.Player, spec.MaxHealth));
        stages[^1].Rows = sessionRows;
        stages[^1].Mines = sessionMines;
    }

    private const float HoldPopupSeconds = 0.6f;

    public override void _Process(double delta)
    {
        if (stages.Count == 0)
        {
            return;
        }

        // The adapter (our parent) filled the ports before this runs.
        foreach (var evt in RunGrading(delta))
        {
            switch (evt)
            {
                case GradingEvent.Graded graded:
                    foreach (var display in grades)
                    {
                        if (display.Player == graded.Player)
                        {
                            display.Apply(Config.Current, graded.Outcome);
                        }
                    }
                    ApplyCombo(graded.Player, graded.Combo);
                    break;

                case GradingEvent.PressBanked banked:
                    EmitSignal(SignalName.PressBanked, banked.Error.Value);
                    break;

                case GradingEvent.Failed failed:
                    EmitSignal(SignalName.StageFailed, (int)failed.Player);
                    break;
            }
        }

        SyncFields();

        // The sandwich layers hold the 2D text overlays in canvas units,
        // centered on the field. The canvas stretch already scales the whole
        // scene to the window, so they must not be scaled again by pixel
        // density — doing so drove the text off toward the bottom-right and
        // blew up its size as the window grew.
        var center = canvas / 2.0f;
        if (behind is not null)
        {
            behind.Position = center;
            behind.Scale = Vector2.One;
        }
        if (overlay is not null)
        {
            overlay.Position = center;
            overlay.Scale = Vector2.One;
        }

        var clock = new FieldClock(visibleNow, timing, targetY);
        foreach (var rig in rigs)
        {
            rig.Update(clock, (float)delta);
        }

        AnimateHud(delta);
    }

    /// <summary>Pushes the session's state into the fields: pressed panels and every hold's render state.</summary>
    private void SyncFields()
    {
        for (int s = 0; s < stages.Count; s++)
        {
            var stage = stages[s];
            var rig = rigs[s];
            for (uint column = 0; column < rig.Layout.Columns; column++)
            {
                rig.SetReceptorHeld(column, input.IsHeld(rig.Layout.StepAction(column)));
            }
            foreach (var arrow in stage.Rows.SelectMany(row => row.Arrows))
            {
                if (arrow.Hold is not HoldState hold)
                {
                    continue;
                }
                var state = (hold.Engaged, hold.Result) switch
                {
                    (_, HoldOutcome.Ok) => HoldVisualState.Ok,
                    (_, HoldOutcome.Ng) => HoldVisualState.Dropped,
                    (false, null) => HoldVisualState.Pending,
                    (true, null) when hold.HeldNow => HoldVisualState.Held,
                    (true, null) => HoldVisualState.Released,
                    _ => HoldVisualState.Pending,
                };
                if (rig.HoldState(arrow.Note) != state)
                {
                    rig.SetHoldState(arrow.Note, state);
                }
            }
        }
    }

    /// <summary>Refreshes and bounces a player's combo readout on their graded row.</summary>
    private void ApplyCombo(PlayerId player, uint combo)
    {
        foreach (var display in combos)
        {
            if (display.Player != player)
            {
                continue;
            }
            if (combo > display.LastCombo)
            {
                display.Bounce();
            }
            display.LastCombo = combo;
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

    private void AnimateHud(double delta)
    {
        foreach (var grade in grades)
        {
            var position = Settings.Instance.Player(grade.Player).GradePosition;
            grade.Animate((float)delta, GradeText.GradeY(gradeArea, position));
        }
        foreach (var combo in combos)
        {
            var position = Settings.Instance.Player(combo.Player).GradePosition;
            combo.Animate((float)delta, GradeText.GradeY(gradeArea, position) - GradeText.ComboGap);
        }
        for (int i = fades.Count - 1; i >= 0; i--)
        {
            var fade = fades[i];
            fade.Remaining -= (float)delta;
            if (fade.Remaining <= 0.0f)
            {
                fade.Node.QueueFree();
                fades.RemoveAt(i);
                continue;
            }
            var alpha = fade.Remaining / fade.Total;
            if (fade.Growth != 0.0f)
            {
                fade.Node.Scale = fade.BaseScale * (1.0f + fade.Growth * (1.0f - alpha));
            }
            var modulate = fade.Node.Modulate;
            modulate.A = alpha;
            fade.Node.Modulate = modulate;
        }
    }

    private void SpawnHoldPopup(float x, HoldOutcome outcome)
    {
        if (overlay is null)
        {
            return;
        }
        var grading = Config.Current.Grading ?? throw new InvalidOperationException("Grading is not configured");
        var def = outcome == HoldOutcome.Ok
            ? grading.Ok ?? throw new InvalidOperationException("Ok grade is not configured")
            : grading.Ng ?? throw new InvalidOperationException("Ng grade is not configured");
        var popup = Text.Label(def.Name, 30.0f, def.Color);
        overlay.AddChild(popup);
        Text.Place(popup, new Vector2(x, -(targetY - 54.0f)), TextPivot.Center);
        popup.PivotOffset = popup.Size / 2.0f;
        fades.Add(new Fading2d { Node = popup, Remaining = HoldPopupSeconds, Total = HoldPopupSeconds, Growth = 0.25f, BaseScale = Vector2.One });
    }
}

/// <summary>Input tracking for one frame.</summary>
internal sealed class PlayInput
{
    public List<GameAction> Held { get; } = [];
    public List<GameAction> Struck { get; } = [];

    public bool IsHeld(GameAction action) => Held.Contains(action);
    public bool IsStruck(GameAction action) => Struck.Contains(action);
}

/// <summary>The combo readout under a player's grade word, with its bounce.</summary>
internal sealed class ComboDisplay
{
    private static readonly Seconds ComboBounce = new(0.18);

    public PlayerId Player { get; }
    public float OriginX { get; set; }
    public Label Label { get; }
    public uint LastCombo { get; set; }
    private Seconds bounce;

    public ComboDisplay(PlayerId player, float originX, Label label)
    {
        Player = player;
        OriginX = originX;
        Label = label;
    }

    /// <summary>Kicks the bounce on a growing combo.</summary>
    public void Bounce() => bounce = ComboBounce;

    public void Animate(float delta, float y)
    {
        bounce = new Seconds(Math.Max(0.0, bounce.Value - delta));
        var scale = 1.0f + 0.22f * (float)(bounce.Value / ComboBounce.Value);
        Text.Place(Label, new Vector2(OriginX, -y), TextPivot.Center);
        Label.PivotOffset = Label.Size / 2.0f;
        Label.Scale = Vector2.One * scale;
    }
}

/// <summary>A 2D label fading — and optionally growing — out over a fixed lifetime.</summary>
internal sealed class Fading2d
{
    public required Control Node { get; init; }
    public required float Remaining { get; set; }
    public required float Total { get; init; }
    public required float Growth { get; init; }
    public required Vector2 BaseScale { get; init; }
}
