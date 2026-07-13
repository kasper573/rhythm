namespace Rhythm.Core;

/// <summary>
/// The play scene's entry param: the stepfile to play and the chart each
/// active player steps.
/// </summary>
public sealed record SelectedStepfile(StepfileId Id, IReadOnlyList<PlayerChart> Charts);

/// <summary>Index into a stepfile's charts for one player.</summary>
public readonly record struct PlayerChart(PlayerId Player, int Chart);

/// <summary>
/// One stage's complete run, produced by the stepfile player and carried to
/// the score scene inside a <see cref="ScoreResults"/>.
/// </summary>
public sealed record StageResults
{
    public required PlayerId Player { get; init; }

    /// <summary>The run drained to zero health before the chart ended.</summary>
    public required bool Failed { get; init; }
    public required IReadOnlyList<RowOutcome> Outcomes { get; init; }

    /// <summary>Every row of the chart, so partial (failed) runs still rate against the whole song.</summary>
    public required uint RowsTotal { get; init; }
    public required uint MaxCombo { get; init; }
    public required uint HoldsOk { get; init; }
    public required uint HoldsNg { get; init; }
    public required uint HoldsTotal { get; init; }
    public required uint MinesExploded { get; init; }
    public required uint MinesTotal { get; init; }
}

/// <summary>
/// A finished session's results, inserted by the play scene (or the bench)
/// as the score scene's entry param; consumed on enter.
/// </summary>
public sealed record ScoreResults(StepfileId Id, string Title, IReadOnlyList<PlayerResult> Players);

/// <summary>One player's complete run: the chart they played and its results.</summary>
public sealed record PlayerResult(int Chart, StageResults Stage);

/// <summary>
/// The note demo's entry params, inserted by the launch directives; consumed
/// on enter. A <c>null</c> scenario prints the catalog and exits.
/// </summary>
public sealed record NoteDemoParams(string? Scenario, string? Skin, string Perspective, double Bpm);
