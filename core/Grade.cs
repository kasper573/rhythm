namespace Rhythm.Core;

/// <summary>Index into the configured timed grades, best (smallest window) first.</summary>
public readonly record struct GradeIndex(int Value);

/// <summary>An outcome classified under the config: one of the timed grades, or the built-in Miss.</summary>
public abstract record Grade
{
    public sealed record Hit(GradeIndex Index) : Grade;

    public sealed record Miss : Grade;
}

/// <summary>
/// What actually happened to one row: the input's signed timing error, or
/// expiry without any input. The raw error is the single source of truth;
/// the grade it represents is derived on demand.
/// </summary>
public abstract record RowOutcome
{
    /// <summary>The row was hit <paramref name="Error"/> away from its moment; positive = early.</summary>
    public sealed record Hit(Seconds Error) : RowOutcome;

    public sealed record Miss : RowOutcome;
}

/// <summary>A hold's resolution once it leaves the field: kept to the end, or dropped.</summary>
public enum HoldOutcome
{
    Ok,
    Ng,
}

/// <summary>A mine's resolution as it crosses the receptors.</summary>
public enum MineOutcome
{
    Avoided,
    Exploded,
}
