using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The game's root: it owns the scene transition and carries the session
/// state scenes hand each other — the play mode, each player's preferred
/// difficulty, and the consumed route params. Scenes are children swapped
/// while the fade overlay is fully black; a torn-down scene keeps no state
/// of its own.
/// </summary>
[GlobalClass]
public partial class Game : Node
{
    public static Game Instance { get; private set; } = null!;

    public PlayMode PlayMode { get; set; } = PlayMode.Singles;

    /// <summary>
    /// The difficulty rank each player is aiming for, kept across stepfiles
    /// and scene visits; each stepfile snaps to its nearest available chart.
    /// </summary>
    public PerPlayer<int> PreferredDifficulty { get; set; } = PerPlayer.Uniform((int)DifficultyKind.Medium);

    private GameScene scene = GameScene.MainMenu;
    private Node? current;
    private FadePhase fade = FadePhase.Idle;
    private GameScene fadeTarget;
    private AnimationPlayer fadeAnim = null!;
    private ShaderMaterial fadeMaterial = null!;

    private StepfileId? wheelTarget;
    private SelectedStepfile? selectedStepfile;
    private ScoreResults? scoreResults;
    private NoteDemoParams? noteDemo;

    public GameScene Scene => scene;

    /// <summary>Input is ignored while fading out, to avoid acting on a scene already on its way out.</summary>
    public bool AcceptsInput => fade != FadePhase.FadingOut;

    /// <summary>Drives the mandatory scene transition: fade to black, swap while black, fade back in.</summary>
    public void ChangeScene(GameScene to)
    {
        if (fade == FadePhase.FadingOut)
        {
            return;
        }

        // Resume the cover from wherever the current coverage sits, so a change
        // that interrupts a fade-in reverses smoothly instead of snapping.
        var coverage = fadeMaterial.GetShaderParameter("coverage").AsSingle();
        fade = FadePhase.FadingOut;
        fadeTarget = to;
        fadeAnim.Play("fade_out");
        fadeAnim.Seek(coverage * fadeAnim.GetAnimation("fade_out").Length, update: true);
    }

    /// <summary>The wheel row to land on, inserted by whichever scene navigates there; consumed on enter.</summary>
    public void SetWheelTarget(StepfileId target) => wheelTarget = target;

    public StepfileId? TakeWheelTarget() => Take(ref wheelTarget);

    public void SetSelectedStepfile(SelectedStepfile selected) => selectedStepfile = selected;

    public SelectedStepfile? TakeSelectedStepfile() => Take(ref selectedStepfile);

    public void SetScoreResults(ScoreResults results) => scoreResults = results;

    public ScoreResults? TakeScoreResults() => Take(ref scoreResults);

    public void SetNoteDemo(NoteDemoParams parameters) => noteDemo = parameters;

    public NoteDemoParams? TakeNoteDemo() => Take(ref noteDemo);

    public override void _Ready()
    {
        if (Engine.IsEditorHint())
        {
            return;
        }

        Instance = this;
        Boot();
    }

    private void Boot()
    {
        fadeAnim = GetNode<AnimationPlayer>("%FadeAnim");
        fadeMaterial = (ShaderMaterial)GetNode<ColorRect>("%FadeRect").Material;
        fadeAnim.AnimationFinished += OnFadeFinished;

        fade = FadePhase.FadingIn;
        SwapTo(Launch.Boot(this));
        fadeAnim.Play("fade_in");
    }

    private void SwapTo(GameScene next)
    {
        current?.QueueFree();
        scene = next;
        var node = GD.Load<PackedScene>(ScenePath(next)).Instantiate();
        AddChild(node);
        current = node;
    }

    /// <summary>
    /// Fully black ends the fade-out: swap scenes behind the cover, then start
    /// the reveal; the reveal reaching clear returns input to the new scene.
    /// </summary>
    private void OnFadeFinished(StringName animation)
    {
        if (animation == "fade_out" && fade == FadePhase.FadingOut)
        {
            SwapTo(fadeTarget);
            fade = FadePhase.FadingIn;
            fadeAnim.Play("fade_in");
        }
        else if (animation == "fade_in")
        {
            fade = FadePhase.Idle;
        }
    }

    private static T? Take<T>(ref T? slot)
    {
        var value = slot;
        slot = default;
        return value;
    }

    private static string ScenePath(GameScene scene) =>
        scene switch
        {
            GameScene.MainMenu => "res://scenes/main_menu/main_menu.tscn",
            GameScene.ModeSelect => "res://scenes/mode_select/mode_select.tscn",
            GameScene.SettingsMenu => "res://scenes/settings_menu/settings_menu.tscn",
            GameScene.Keymap => "res://scenes/keymap/keymap.tscn",
            GameScene.AudioSettings => "res://scenes/audio_settings/audio_settings.tscn",
            GameScene.Wheel => "res://scenes/wheel/wheel.tscn",
            GameScene.Play => "res://scenes/play/play.tscn",
            GameScene.Score => "res://scenes/score/score.tscn",
            GameScene.GradeSheet => "res://scenes/review/grade_sheet.tscn",
            GameScene.NoteDemo => "res://scenes/review/note_demo.tscn",
            GameScene.VialDemo => "res://scenes/review/vial_demo.tscn",
            _ => throw new ArgumentOutOfRangeException(nameof(scene)),
        };

    private enum FadePhase
    {
        Idle,
        FadingOut,
        FadingIn,
    }
}
