using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The note demo: one animation scenario from the catalog, played through
/// the real note field with a scripted stand-in for the gameplay systems,
/// then the game exits.
/// </summary>
[GlobalClass]
public partial class NoteDemo : Control
{
    /// <summary>The demo's arrow size: the classic in-game proportions.</summary>
    private const float ArrowSize = 88.0f;

    /// <summary>
    /// The first note starts this far below the receptors: past the bottom
    /// edge whatever the scroll speed.
    /// </summary>
    private const float LeadPixels = 760.0f;

    /// <summary>Tail delay after the last event on the timeline.</summary>
    private const double TailSeconds = 1.2;

    private NoteFieldRig? rig;
    private StepfileTiming? timing;
    private Seconds start;
    private Seconds end;
    private double elapsed;
    private List<(NoteIndex, uint)> notes = [];
    private List<(MineIndex, uint)> mines = [];
    private List<(Seconds, ScriptAction)> script = [];
    private int nextAction;

    public override void _Ready()
    {
        var @params = Game.Instance.TakeNoteDemo();
        if (@params is null)
        {
            PrintCatalog();
            GetTree().Quit();
            return;
        }

        var scenario = Scenarios.Matrix()
            .FirstOrDefault(s => s.Name == @params.Scenario);
        if (scenario is null)
        {
            PrintCatalog();
            GetTree().Quit();
            return;
        }

        if (@params.Bpm.Value <= 0.0)
        {
            GD.PrintErr("--bpm must be positive");
            GetTree().Quit();
            return;
        }

        // The demo draws 1:1 at whatever size the window was launched with
        // (the tooling picks its capture resolution with `--resolution`).
        var window = GetWindow();
        if (window is null)
        {
            GetTree().Quit();
            return;
        }
        window.ContentScaleMode = Window.ContentScaleModeEnum.Disabled;
        var size = window.Size;

        var backdrop = new ColorRect();
        backdrop.Color = Screen.ClearColor;
        backdrop.SetAnchorsAndOffsetsPreset(Control.LayoutPreset.FullRect);
        AddChild(backdrop);

        var settingsDefaults = Config.Current.Defaults;
        if (settingsDefaults is null)
        {
            GD.PrintErr("SettingsDefaults not configured");
            GetTree().Quit();
            return;
        }

        var skinName = @params.Skin ?? settingsDefaults.NoteSkin;
        var perspective = @params.Perspective switch
        {
            "None" => Perspective.None,
            "Above" => Perspective.Above,
            "Below" => Perspective.Below,
            _ => Perspective.None
        };

        var layout = new FieldLayout(
            PlayerId.P1,
            0.0f,
            4,
            settingsDefaults.ToPlayerOptions().NoteSpeed,
            ArrowSize
        );

        var laneCamera = Config.Current.LaneCamera;
        if (laneCamera is null)
        {
            GD.PrintErr("LaneCamera not configured");
            GetTree().Quit();
            return;
        }

        rig = NoteFieldRig.Build(
            this,
            layout,
            NoteSkin.Load(skinName),
            perspective,
            laneCamera.FovDegrees,
            laneCamera.TiltDegrees,
            size
        );

        timing = BuildScenarioTiming(scenario, @params.Bpm);

        foreach (var note in scenario.Notes)
        {
            var time = timing.SecondsAtBeat(new Beat(note.Beat));
            NoteTail? tail = null;
            if (note.LengthBeats.HasValue)
            {
                var endBeat = new Beat(note.Beat + note.LengthBeats.Value);
                tail = new NoteTail(timing.SecondsAtBeat(endBeat), endBeat, note.Roll);
            }

            var spawn = new NoteSpawn(
                time,
                new Beat(note.Beat),
                note.Column,
                note.Quant,
                tail
            );
            var index = rig.SpawnNote(spawn);
            notes.Add((index, note.Column));
        }

        foreach (var mine in scenario.Mines)
        {
            var time = timing.SecondsAtBeat(new Beat(mine.Beat));
            var index = rig.SpawnMine(time, new Beat(mine.Beat), mine.Column);
            mines.Add((index, mine.Column));
        }

        script = scenario.Script
            .Select(pair =>
            {
                var beat = pair.Beat;
                var action = pair.Action;
                var seconds = timing.SecondsAtBeat(new Beat(beat));
                return (seconds, action);
            })
            .OrderBy(p => p.Item1.Value)
            .ToList();

        (start, end) = ComputeDemoWindow(scenario, timing, settingsDefaults.ToPlayerOptions().NoteSpeed);
        elapsed = 0.0;
        nextAction = 0;
    }

    public override void _Process(double delta)
    {
        if (rig is null || timing is null)
            return;

        var now = new Seconds(start.Value + elapsed);
        elapsed += delta;

        while (nextAction < script.Count && script[nextAction].Item1.Value <= now.Value)
        {
            var action = script[nextAction].Item2;
            ApplyAction(rig, action);
            nextAction++;
        }

        var clock = new FieldClock(now, timing, NoteField.TargetY);
        rig.Update(clock, (float)delta);

        if (now.Value >= end.Value)
        {
            GetTree().Quit();
        }
    }

    private static void PrintCatalog()
    {
        foreach (var name in Scenarios.Names())
        {
            GD.Print($"scenario: {name}");
        }
    }

    private void ApplyAction(NoteFieldRig rig, ScriptAction action)
    {
        switch (action)
        {
            case ScriptAction.Hold hold:
                rig.SetHoldState(notes[hold.Index].Item1, hold.State);
                break;

            case ScriptAction.Fade fade:
                rig.FadeOutNote(notes[fade.Index].Item1, NoteFieldRig.HoldOkFadeSeconds);
                break;

            case ScriptAction.Vanish vanish:
                {
                    var (noteIndex, column) = notes[vanish.Index];
                    rig.VanishNote(noteIndex);
                    var flashColor = Config.Current.Grading?.Dynamic.FirstOrDefault()?
                        .ArrowFlash ?? Colors.White;
                    rig.ArrowFlash(column, NoteField.TargetY, flashColor, false, Config.Current.FlashTiming(false));
                    break;
                }

            case ScriptAction.Press press:
                rig.SetReceptorHeld(press.Column, press.Held);
                break;

            case ScriptAction.ExplodeMine explode:
                {
                    var (mineIndex, column) = mines[explode.Index];
                    rig.RemoveMine(mineIndex);
                    rig.MineExplosion(column, NoteField.TargetY);
                    break;
                }
        }
    }

    private static StepfileTiming BuildScenarioTiming(Scenario scenario, Bpm cliBpm)
    {
        var bpms = scenario.Bpms.Count > 0
            ? scenario.Bpms.Select(p => (new Beat(p.Beat), new Bpm(p.Bpm))).ToList()
            : new List<(Beat, Bpm)> { (new Beat(0.0), cliBpm) };

        var stops = scenario.Stops
            .Select(p => (new Beat(p.Beat), new Seconds(p.Seconds)))
            .ToList();

        return new StepfileTiming(Seconds.Zero, bpms, stops);
    }

    private static (Seconds, Seconds) ComputeDemoWindow(
        Scenario scenario,
        StepfileTiming timing,
        NoteSpeed speed)
    {
        var first = double.PositiveInfinity;
        var last = double.NegativeInfinity;

        foreach (var note in scenario.Notes)
        {
            first = Math.Min(first, note.Beat);
            last = Math.Max(last, note.Beat + (note.LengthBeats ?? 0.0));
        }

        foreach (var mine in scenario.Mines)
        {
            first = Math.Min(first, mine.Beat);
            last = Math.Max(last, mine.Beat);
        }

        foreach (var (beat, _) in scenario.Script)
        {
            first = Math.Min(first, beat);
            last = Math.Max(last, beat);
        }

        if (!double.IsFinite(first))
        {
            throw new InvalidOperationException("Scenario has empty timeline");
        }

        var leadArrows = LeadPixels / ArrowSize;
        var startSeconds = speed switch
        {
            NoteSpeed.Constant constant =>
                timing.SecondsAtBeat(new Beat(first)).Value -
                (leadArrows * 60.0 / constant.Value),

            NoteSpeed.Dynamic dynamic =>
                timing.SecondsAtBeat(new Beat(first - leadArrows / dynamic.Value)).Value,

            _ => throw new InvalidOperationException($"Unknown speed: {speed.GetType().Name}")
        };

        var endSeconds = timing.SecondsAtBeat(new Beat(last)).Value + TailSeconds;

        return (new Seconds(startSeconds), new Seconds(endSeconds));
    }
}
