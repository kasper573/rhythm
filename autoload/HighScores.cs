using System.Security.Cryptography;
using System.Text;
using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The high scores autoload: each player's best total points per played
/// chart. Recording an improvement persists it immediately, one file per
/// player; each file contains only the bare key→points map.
/// </summary>
[GlobalClass]
public partial class HighScores : Node
{
    private PerPlayer<ScoreBook> books;

    public static HighScores Instance { get; private set; } = null!;

    public override void _EnterTree()
    {
        if (Engine.IsEditorHint())
        {
            return;
        }

        Instance = this;

        books = new PerPlayer<ScoreBook>(
            Persistence.Load<ScoreBook>(HighScoresFile(PlayerId.P1)),
            Persistence.Load<ScoreBook>(HighScoresFile(PlayerId.P2)));
    }

    public uint? Get(PlayerId player, string key)
    {
        var book = books[player];
        return book.Scores.TryGetValue(key, out var points) ? points : null;
    }

    /// <summary>
    /// Stores points if it beats the player's best on the chart; returns
    /// whether it did.
    /// </summary>
    public bool Record(PlayerId player, string key, uint points)
    {
        var book = books[player];
        var existing = book.Scores.TryGetValue(key, out var best) ? best : 0u;

        if (points > existing)
        {
            book.Scores[key] = points;
            Persistence.Save(HighScoresFile(player), book);
            return true;
        }

        return false;
    }

    private static string HighScoresFile(PlayerId player) => player switch
    {
        PlayerId.P1 => "p1_highscores.json",
        PlayerId.P2 => "p2_highscores.json",
        _ => throw new System.ArgumentOutOfRangeException(nameof(player)),
    };

    /// <summary>
    /// One stable key per (group, stepfile, chart type, difficulty): the
    /// parts are joined unambiguously and hashed, so the stored key is opaque
    /// and immune to awkward characters in names.
    /// </summary>
    public static string HighscoreKey(StepfileLibrary library, StepfileId id, Chart chart)
    {
        using var hasher = SHA256.Create();
        var input = $"{library.GroupName(id)}\x1f{library.Stepfile(id).Name()}\x1f{chart.StepsType:G}\x1f{chart.Difficulty:G}";
        var hash = hasher.ComputeHash(Encoding.UTF8.GetBytes(input));
        var sb = new StringBuilder();
        foreach (var byte_ in hash)
        {
            sb.Append(byte_.ToString("x2"));
        }

        return sb.ToString();
    }
}

/// <summary>Serialization wrapper for a score book.</summary>
[System.Serializable]
public sealed class ScoreBook
{
    [System.Text.Json.Serialization.JsonPropertyName("scores")]
    public Dictionary<string, uint> Scores { get; set; } = [];
}
