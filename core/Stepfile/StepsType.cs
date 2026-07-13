namespace Rhythm.Core;

/// <summary>The play styles the game recognizes, plus a passthrough for the rest.</summary>
public enum StepsKind
{
    DanceSingle,
    DanceDouble,
    DanceSolo,
    DanceCouple,
    Other,
}

/// <summary>
/// A chart's play style. Known styles carry a fixed column count; unknown
/// ones keep their raw <c>#NOTES</c> name and infer columns from the data.
/// </summary>
public readonly record struct StepsType(StepsKind Kind, string Raw)
{
    public static readonly StepsType DanceSingle = new(StepsKind.DanceSingle, "dance-single");
    public static readonly StepsType DanceDouble = new(StepsKind.DanceDouble, "dance-double");
    public static readonly StepsType DanceSolo = new(StepsKind.DanceSolo, "dance-solo");
    public static readonly StepsType DanceCouple = new(StepsKind.DanceCouple, "dance-couple");

    public static StepsType Parse(string raw) =>
        raw.Trim().ToLowerInvariant() switch
        {
            "dance-single" => DanceSingle,
            "dance-double" => DanceDouble,
            "dance-solo" => DanceSolo,
            "dance-couple" => DanceCouple,
            _ => new StepsType(StepsKind.Other, raw.Trim()),
        };

    /// <summary>
    /// Column count for the known styles; unknown styles infer their column
    /// count from the note data instead.
    /// </summary>
    public int? Columns() =>
        Kind switch
        {
            StepsKind.DanceSingle => 4,
            StepsKind.DanceDouble or StepsKind.DanceCouple => 8,
            StepsKind.DanceSolo => 6,
            _ => null,
        };
}

/// <summary>The difficulty tiers, easiest to hardest, plus a passthrough.</summary>
public enum DifficultyKind
{
    Beginner,
    Easy,
    Medium,
    Hard,
    Challenge,
    Edit,
    Other,
}

/// <summary>
/// A chart's difficulty. The canonical names plus the legacy aliases still
/// found in old files parse to a known tier; anything else keeps its raw name.
/// </summary>
public readonly record struct Difficulty(DifficultyKind Kind, string Raw)
{
    public static Difficulty Parse(string raw) =>
        raw.Trim().ToLowerInvariant() switch
        {
            "beginner" => new(DifficultyKind.Beginner, "Beginner"),
            "easy" or "basic" or "light" => new(DifficultyKind.Easy, "Easy"),
            "medium" or "another" or "trick" or "standard" => new(DifficultyKind.Medium, "Medium"),
            "hard" or "ssr" or "maniac" or "heavy" => new(DifficultyKind.Hard, "Hard"),
            "challenge" or "smaniac" or "expert" or "oni" => new(DifficultyKind.Challenge, "Challenge"),
            "edit" => new(DifficultyKind.Edit, "Edit"),
            _ => new(DifficultyKind.Other, raw.Trim()),
        };

    /// <summary>
    /// Canonical easiest-to-hardest ordering, used to keep the selected
    /// difficulty stable while browsing between stepfiles.
    /// </summary>
    public int Rank() => (int)Kind;

    public override string ToString() => Raw;
}
