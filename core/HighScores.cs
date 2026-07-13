using System.Security.Cryptography;
using System.Text;

namespace Rhythm.Core;

/// <summary>
/// One player's best total points per played chart, keyed by
/// <see cref="HighScores.Key"/>. The persistence layer owns loading and
/// saving; this is the bare key→points map plus the improve-if-better rule.
/// </summary>
public sealed class ScoreBook
{
    private readonly Dictionary<string, uint> best;

    public ScoreBook()
    {
        best = [];
    }

    public ScoreBook(IReadOnlyDictionary<string, uint> best)
    {
        this.best = new Dictionary<string, uint>(best);
    }

    public IReadOnlyDictionary<string, uint> Entries => best;

    public uint? Get(string key) => best.TryGetValue(key, out var points) ? points : null;

    /// <summary>Stores <paramref name="points"/> if it beats the current best on the chart; returns whether it did.</summary>
    public bool Improve(string key, uint points)
    {
        if (points > (best.TryGetValue(key, out var current) ? current : 0))
        {
            best[key] = points;
            return true;
        }

        return false;
    }
}

public static class HighScores
{
    /// <summary>
    /// One stable key per (group, stepfile, chart type, difficulty): the
    /// parts are joined unambiguously and hashed, so the stored key is
    /// opaque and immune to awkward characters in names. Renaming a
    /// <see cref="StepsKind"/> or <see cref="DifficultyKind"/> orphans the
    /// scores stored under it.
    /// </summary>
    public static string Key(StepfileLibrary library, StepfileId id, Chart chart)
    {
        var payload = string.Join('\x1f',
            library.GroupName(id),
            library.Stepfile(id).Name(),
            Canonical(chart.StepsType),
            Canonical(chart.Difficulty));
        var hash = SHA256.HashData(Encoding.UTF8.GetBytes(payload));
        return Convert.ToHexString(hash).ToLowerInvariant();
    }

    private static string Canonical(StepsType type) =>
        type.Kind == StepsKind.Other ? $"Other(\"{type.Raw}\")" : type.Kind.ToString();

    private static string Canonical(Difficulty difficulty) =>
        difficulty.Kind == DifficultyKind.Other ? $"Other(\"{difficulty.Raw}\")" : difficulty.Kind.ToString();
}
