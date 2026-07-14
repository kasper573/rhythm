using Godot;
using Godot.Collections;
using Rhythm.Core;

namespace Rhythm;

[Tool]
[GlobalClass]
public partial class GameConfig : Resource
{
    [ExportGroup("Wheel")]
    [Export] public string WheelDefaultGroup { get; set; } = "";
    [Export] public string WheelDefaultStepfile { get; set; } = "";

    [ExportGroup("Defaults")]
    [Export] public SettingsDefaults? Defaults { get; set; }

    [ExportGroup("Stage")]
    [Export] public StageConfig? Stage { get; set; }

    [ExportGroup("Lane Camera")]
    [Export] public LaneCameraConfig? LaneCamera { get; set; }

    [ExportGroup("Grading")]
    [Export] public GradingConfig? Grading { get; set; }

    [ExportGroup("Arrow Flash")]
    [Export(PropertyHint.Range, "0,1000")] public int BrightArrowFlashCombo { get; set; }

    [ExportSubgroup("Dim (below the bright combo)")]
    [Export(PropertyHint.Range, "0.02,1,0.01,or_greater")] public float FlashDimSeconds { get; set; } = 0.18f;
    [Export(PropertyHint.Range, "0.1,3,0.05")] public float FlashDimZoom { get; set; } = 1.0f;
    [Export(PropertyHint.Range, "0,2,0.05")] public float FlashDimGrowth { get; set; } = 0.4f;

    [ExportSubgroup("Bright (at or above the bright combo)")]
    [Export(PropertyHint.Range, "0.02,1,0.01,or_greater")] public float FlashBrightSeconds { get; set; } = 0.13f;
    [Export(PropertyHint.Range, "0.1,3,0.05")] public float FlashBrightZoom { get; set; } = 0.8f;
    [Export(PropertyHint.Range, "0,2,0.05")] public float FlashBrightGrowth { get; set; } = 0.5f;

    /// <summary>The arrow-flash lifetime, size, and growth for the given combo tier.</summary>
    public ArrowFlashTiming FlashTiming(bool bright) => bright
        ? new ArrowFlashTiming(new Seconds(FlashBrightSeconds), FlashBrightZoom, FlashBrightGrowth)
        : new ArrowFlashTiming(new Seconds(FlashDimSeconds), FlashDimZoom, FlashDimGrowth);

    [ExportGroup("Health")]
    [Export(PropertyHint.Range, "1,1000")] public int PlayerMaxHealth { get; set; }
    [Export] public HealthBarConfig? HealthBar { get; set; }

    [ExportGroup("Ratings")]
    [Export] public Array<RatingDef> Ratings { get; set; } = [];

    [ExportGroup("Audio")]
    [Export(PropertyHint.Range, "0,2")] public float TickVolume { get; set; }

    [ExportGroup("Notes")]
    [Export] public int[] NoteQuants { get; set; } = [];

    [ExportGroup("Speed Modifiers")]
    [Export] public SpeedModifiers? SpeedModifiers { get; set; }

    public Seconds WidestWindow()
    {
        if (Grading == null || Grading.Dynamic.Count == 0)
            throw new InvalidOperationException("Dynamic grades are empty");

        var last = Grading.Dynamic[Grading.Dynamic.Count - 1];
        return last.Window;
    }

    public Grade ClassifyGrade(RowOutcome outcome)
    {
        return outcome switch
        {
            RowOutcome.Hit hit => FindGradeForError(hit.Error),
            RowOutcome.Miss => new Grade.Miss(),
            _ => throw new InvalidOperationException($"Unknown RowOutcome type: {outcome.GetType().Name}"),
        };
    }

    private Grade.Hit FindGradeForError(Seconds error)
    {
        if (Grading == null)
            throw new InvalidOperationException("Grading is null");

        var absError = error.Abs();

        for (int i = 0; i < Grading.Dynamic.Count; i++)
        {
            if (absError <= Grading.Dynamic[i].Window)
            {
                return new Grade.Hit(new GradeIndex(i));
            }
        }

        throw new InvalidOperationException($"No grade window matches error {error}");
    }

    public int HealthOffset(Grade grade)
    {
        if (Grading == null)
            throw new InvalidOperationException("Grading is null");

        return grade switch
        {
            Grade.Hit hit =>
                hit.Index.Value < Grading.Dynamic.Count
                    ? Grading.Dynamic[hit.Index.Value].HealthOffset
                    : throw new InvalidOperationException($"Grade index {hit.Index.Value} out of range"),
            Grade.Miss => Grading.Miss?.HealthOffset ?? 0,
            _ => throw new InvalidOperationException($"Unknown Grade type: {grade.GetType().Name}"),
        };
    }

    public uint Points(Grade grade)
    {
        if (Grading == null)
            throw new InvalidOperationException("Grading is null");

        return grade switch
        {
            Grade.Hit hit =>
                hit.Index.Value < Grading.Dynamic.Count
                    ? (uint)Grading.Dynamic[hit.Index.Value].Points
                    : throw new InvalidOperationException($"Grade index {hit.Index.Value} out of range"),
            Grade.Miss => (uint)(Grading.Miss?.Points ?? 0),
            _ => throw new InvalidOperationException($"Unknown Grade type: {grade.GetType().Name}"),
        };
    }

    public bool BreaksCombo(Grade grade)
    {
        if (Grading == null)
            throw new InvalidOperationException("Grading is null");

        return grade switch
        {
            Grade.Hit hit =>
                hit.Index.Value < Grading.Dynamic.Count
                    ? Grading.Dynamic[hit.Index.Value].BreaksCombo
                    : throw new InvalidOperationException($"Grade index {hit.Index.Value} out of range"),
            Grade.Miss => true,
            _ => throw new InvalidOperationException($"Unknown Grade type: {grade.GetType().Name}"),
        };
    }

    public Percent ScorePercent(uint points, uint rows, uint holds)
    {
        if (Grading == null || Grading.Dynamic.Count == 0)
            throw new InvalidOperationException("Dynamic grades are empty");

        var bestDynamicPoints = (uint)Grading.Dynamic[0].Points;
        var okPoints = (uint)(Grading.Ok?.Points ?? 0);

        var max = rows * bestDynamicPoints + holds * okPoints;
        if (max <= 0)
            return new Percent(0);

        return new Percent(points * 100.0f / max);
    }

    public RatingDef Rating(Percent percent, Grade? worstGrade)
    {
        foreach (var rating in Ratings)
        {
            var matches = rating.RuleKind switch
            {
                RuleKind.PointPercentage => percent.Value >= rating.PointPercentage,
                RuleKind.AllGradesGte => MatchesAllGradesGte(rating.AllGradesGte, worstGrade),
                _ => false,
            };

            if (matches)
                return rating;
        }

        throw new InvalidOperationException("No rating matched");
    }

    private bool MatchesAllGradesGte(string gradeName, Grade? worstGrade)
    {
        if (worstGrade is not Grade.Hit hit)
            return false;

        if (Grading == null)
            throw new InvalidOperationException("Grading is null");

        var targetIndex = -1;
        for (int i = 0; i < Grading.Dynamic.Count; i++)
        {
            if (Grading.Dynamic[i].Name == gradeName)
            {
                targetIndex = i;
                break;
            }
        }

        if (targetIndex < 0)
            throw new InvalidOperationException($"Grade '{gradeName}' not found in dynamic grades");

        return hit.Index.Value <= targetIndex;
    }

    public uint RecognizedQuant(uint quant)
    {
        foreach (var q in NoteQuants)
        {
            if (q == quant)
                return quant;
        }

        return (uint)NoteQuants[NoteQuants.Length - 1];
    }

    public void Validate()
    {
        if (Grading == null)
            throw new InvalidOperationException("Grading is null");

        if (Grading.Dynamic.Count == 0)
            throw new InvalidOperationException("Grading.Dynamic is empty");

        if (Grading.Miss == null)
            throw new InvalidOperationException("Grading.Miss is null");

        if (Grading.Ok == null)
            throw new InvalidOperationException("Grading.Ok is null");

        if (Grading.Ng == null)
            throw new InvalidOperationException("Grading.Ng is null");

        for (int i = 0; i < Grading.Dynamic.Count - 1; i++)
        {
            var current = Grading.Dynamic[i].Window;
            var next = Grading.Dynamic[i + 1].Window;
            if (current >= next)
                throw new InvalidOperationException($"Grade windows must be strictly ascending: grade {i} window {current} >= grade {i + 1} window {next}");

            if (current.Value <= 0)
                throw new InvalidOperationException($"Grade window must be positive: grade {i} is {current}");
        }

        if (Grading.Dynamic[Grading.Dynamic.Count - 1].Window.Value <= 0)
            throw new InvalidOperationException("Last grade window must be positive");

        if (TickVolume < 0 || TickVolume > 2)
            throw new InvalidOperationException($"TickVolume must be in 0..2, got {TickVolume}");

        if (NoteQuants == null || NoteQuants.Length == 0)
            throw new InvalidOperationException("NoteQuants is empty");

        foreach (var q in NoteQuants)
        {
            if (q <= 0)
                throw new InvalidOperationException($"NoteQuants contains non-positive value: {q}");
        }

        if (SpeedModifiers == null || SpeedModifiers.Constant == null || SpeedModifiers.Dynamic == null)
            throw new InvalidOperationException("SpeedModifiers is misconfigured");

        if (SpeedModifiers.Constant.Options.Length == 0 || SpeedModifiers.Dynamic.Options.Length == 0)
            throw new InvalidOperationException("SpeedModifiers option sets are empty");

        if (PlayerMaxHealth <= 0)
            throw new InvalidOperationException($"PlayerMaxHealth must be positive, got {PlayerMaxHealth}");

        if (HealthBar == null || HealthBar.Colors.Count == 0)
            throw new InvalidOperationException("HealthBar.Colors is empty");

        foreach (var gradient in HealthBar.Colors)
        {
            if (gradient.Stops.Count == 0)
                throw new InvalidOperationException("HealthGradient.Stops is empty");
        }

        if (Ratings.Count == 0)
            throw new InvalidOperationException("Ratings is empty");

        var hasZeroPercentage = false;
        foreach (var rating in Ratings)
        {
            if (rating.RuleKind == RuleKind.PointPercentage && rating.PointPercentage == 0)
            {
                hasZeroPercentage = true;
                break;
            }
        }

        if (!hasZeroPercentage)
            throw new InvalidOperationException("No rating with PointPercentage 0 exists");

        foreach (var rating in Ratings)
        {
            if (rating.RuleKind == RuleKind.AllGradesGte)
            {
                var found = false;
                foreach (var grade in Grading.Dynamic)
                {
                    if (grade.Name == rating.AllGradesGte)
                    {
                        found = true;
                        break;
                    }
                }

                if (!found)
                    throw new InvalidOperationException($"AllGradesGte references unknown grade: {rating.AllGradesGte}");
            }
        }

        if (HealthBar.Glow == null || HealthBar.Liquid == null)
            throw new InvalidOperationException("HealthBar cycles are null");

        if (HealthBar.Glow.Speed <= 0 || HealthBar.Liquid.Speed <= 0)
            throw new InvalidOperationException("RhythmCycle speeds must be positive");

        var easingX1 = HealthBar.Glow.Easing.X;
        var easingY1 = HealthBar.Glow.Easing.Y;
        var easingX2 = HealthBar.Glow.Easing.Z;
        var easingY2 = HealthBar.Glow.Easing.W;

        if (easingX1 < 0 || easingX1 > 1 || easingX2 < 0 || easingX2 > 1)
            throw new InvalidOperationException("RhythmCycle easing x values must be in 0..1");

        if (Stage == null || Stage.MaxArrowSize <= 0)
            throw new InvalidOperationException("Stage.MaxArrowSize must be positive");

        if (LaneCamera == null || LaneCamera.FovDegrees <= 0 || LaneCamera.FovDegrees >= 180)
            throw new InvalidOperationException("LaneCamera.FovDegrees must be in (0, 180)");

        if (LaneCamera.TiltDegrees < 0 || LaneCamera.TiltDegrees >= 90)
            throw new InvalidOperationException("LaneCamera.TiltDegrees must be in [0, 90)");

        if (Grading.HoldGrace.Value <= 0 || Grading.RollGrace.Value <= 0)
            throw new InvalidOperationException("Hold/Roll grace periods must be positive");

        if (Defaults == null)
            throw new InvalidOperationException("Defaults is null");

        var keymap = Defaults.ToKeymap();
        foreach (var action in GameActions.All)
        {
            if (!keymap.Bindings.ContainsKey(action))
                throw new InvalidOperationException($"Keymap is missing binding for {action}");
        }

        var masterVolume = Defaults.MasterVolume;
        var sfxVolume = Defaults.SfxVolume;
        var musicVolume = Defaults.MusicVolume;

        if (masterVolume < 0 || masterVolume > 1)
            throw new InvalidOperationException($"Master volume must be in 0..1, got {masterVolume}");

        if (sfxVolume < 0 || sfxVolume > 1)
            throw new InvalidOperationException($"SFX volume must be in 0..1, got {sfxVolume}");

        if (musicVolume < 0 || musicVolume > 1)
            throw new InvalidOperationException($"Music volume must be in 0..1, got {musicVolume}");

        foreach (var seconds in new[] { FlashDimSeconds, FlashBrightSeconds })
        {
            if (seconds <= 0)
                throw new InvalidOperationException($"Arrow-flash seconds must be positive, got {seconds}");
        }
    }
}

/// <summary>How an arrow flash plays: its lifetime, its size relative to the
/// arrow, and how much it grows over that lifetime.</summary>
public readonly record struct ArrowFlashTiming(Seconds Life, float BaseZoom, float Growth);
