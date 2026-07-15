using System.Globalization;
using Godot;

namespace Rhythm;

/// <summary>
/// One player's result column on the score screen. The fixed rows (result,
/// score, combo) are authored in the scene; the per-grade tally rows are built
/// from the configured grades so the breakdown always mirrors the current
/// config — a representative sample in the editor, the run's real counts at
/// runtime.
/// </summary>
[Tool]
[GlobalClass]
public partial class PlayerColumn : VBoxContainer
{
    private const float TallyFontSize = 30.0f;
    private static readonly Color TallyColor = new(0.8f, 0.85f, 0.8f);

    public override void _Ready()
    {
        if (Engine.IsEditorHint())
        {
            ShowPreview();
        }
    }

    /// <summary>Rebuilds the tally rows from the configured grades and these counts.</summary>
    public void FillTallies(GameConfig config, IReadOnlyList<uint> gradeCounts, uint missCount, uint holdsOk, uint holdsTotal, uint minesAvoided, uint minesTotal)
    {
        var labels = GetNode<VBoxContainer>("%Labels");
        var values = GetNode<VBoxContainer>("%Values");
        ClearChildren(labels);
        ClearChildren(values);

        if (config.Grading is { } grading)
        {
            for (int i = 0; i < grading.Dynamic.Count; i++)
            {
                var grade = grading.Dynamic[i];
                var count = i < gradeCounts.Count ? gradeCounts[i] : 0;
                AddRow(labels, values, grade.Name, count.ToString(CultureInfo.InvariantCulture), grade.Color);
            }
            if (grading.Miss is { } miss)
            {
                AddRow(labels, values, miss.Name, missCount.ToString(CultureInfo.InvariantCulture), miss.Color);
            }
        }

        AddRow(labels, values, "Holds", $"{holdsOk}/{holdsTotal}", TallyColor);
        AddRow(labels, values, "Mines", $"{minesAvoided}/{minesTotal}", TallyColor);
    }

    private static void AddRow(VBoxContainer labels, VBoxContainer values, string name, string value, Color color)
    {
        labels.AddChild(Text.Label(name, TallyFontSize, color));
        values.AddChild(Text.Label(value, TallyFontSize, color));
    }

    private static void ClearChildren(Node node)
    {
        foreach (var child in node.GetChildren())
        {
            node.RemoveChild(child);
            child.QueueFree();
        }
    }

    /// <summary>A config-shaped sample so the editor shows exactly the configured grades.</summary>
    private void ShowPreview()
    {
        var config = Config.Current;
        var count = config.Grading?.Dynamic.Count ?? 0;
        var sample = new uint[count];
        for (int i = 0; i < count; i++)
        {
            sample[i] = (uint)(217 / (i + 1));
        }
        FillTallies(config, sample, missCount: 1, holdsOk: 8, holdsTotal: 8, minesAvoided: 2, minesTotal: 2);
    }
}
