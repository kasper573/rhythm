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
    private const float FadeSeconds = 0.3f;

    public static Game Instance { get; private set; } = null!;

    public PlayMode PlayMode { get; set; } = PlayMode.Singles;

    /// <summary>
    /// The difficulty rank each player is aiming for, kept across stepfiles
    /// and scene visits; each stepfile snaps to its nearest available chart.
    /// </summary>
    public PerPlayer<int> PreferredDifficulty = PerPlayer<int>.Uniform((int)DifficultyKind.Medium);

    private GameScene scene = GameScene.MainMenu;
    private Node? current;
    private FadePhase fade = FadePhase.Idle;
    private GameScene fadeTarget;
    private float fadeAlpha;
    private Tween? fadeTween;
    private ColorRect? fadeRect;

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

        fade = FadePhase.FadingOut;
        fadeTarget = to;
        FadeTo(1.0f, nameof(FinishFadeOut));
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
        var fadeLayer = new CanvasLayer { Layer = 100 };
        fadeRect = new ColorRect { Color = Colors.Black, MouseFilter = Control.MouseFilterEnum.Ignore };
        fadeRect.SetAnchorsAndOffsetsPreset(Control.LayoutPreset.FullRect);
        fadeLayer.AddChild(fadeRect);
        AddChild(fadeLayer);

        fade = FadePhase.FadingIn;
        fadeAlpha = 1.0f;
        ApplyFade(1.0f);
        FadeTo(0.0f, nameof(FinishFadeIn));

        SwapTo(Launch.Boot(this));
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
    /// Tweens the overlay from the current alpha to <paramref name="target"/>
    /// at the fade's constant rate, then calls <paramref name="then"/>.
    /// Interrupting an opposite-direction fade continues smoothly from
    /// wherever the alpha is.
    /// </summary>
    private void FadeTo(float target, string then)
    {
        fadeTween?.Kill();
        var duration = FadeSeconds * Mathf.Abs(target - fadeAlpha);
        fadeTween = CreateTween();
        fadeTween.TweenMethod(Callable.From<float>(ApplyFade), fadeAlpha, target, duration);
        fadeTween.TweenCallback(Callable.From(() => Call(then)));
    }

    /// <summary>The overlay's coverage, encoded for the canvas' sRGB blending.</summary>
    private void ApplyFade(float alpha)
    {
        fadeAlpha = alpha;
        if (fadeRect is not null)
        {
            var color = fadeRect.Color;
            color.A = 1.0f - Screen.LinearBlend(1.0f - alpha);
            fadeRect.Color = color;
        }
    }

    /// <summary>Fully black: swap scenes behind the overlay, then fade back in.</summary>
    private void FinishFadeOut()
    {
        if (fade != FadePhase.FadingOut)
        {
            return;
        }

        SwapTo(fadeTarget);
        fade = FadePhase.FadingIn;
        FadeTo(0.0f, nameof(FinishFadeIn));
    }

    private void FinishFadeIn()
    {
        fade = FadePhase.Idle;
        fadeTween = null;
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
            _ => throw new ArgumentOutOfRangeException(nameof(scene)),
        };

    private enum FadePhase
    {
        Idle,
        FadingOut,
        FadingIn,
    }
}
