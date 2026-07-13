using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// Partial class for Wheel: info panel display (banner, bpm, difficulty, stats).
/// </summary>
public partial class Wheel
{
    private void RefreshInfoPanel()
    {
        if (!justSettled)
            return;

        if (infoPanel != null)
            infoPanel.QueueFree();

        bannerPending = null;
        bannerRect = null;

        if (active >= entries.Count)
            return;

        string? bannerPath;
        string fallbackTitle;
        string headline;
        var charts = new List<(PlayerId, StepfileId, int)>();

        switch (entries[active])
        {
            case WheelEntry.Stepfile row:
                var entry = Library.Instance.Stepfile(row.Id);
                bannerPath = entry.BannerPath();
                fallbackTitle = entry.DisplayTitle();
                headline = BpmLabel(entry.Stepfile);
                foreach (var player in players)
                {
                    if (ChartFor(entry.Stepfile, player) is int chartIndex)
                        charts.Add((player, row.Id, chartIndex));
                }
                break;

            case WheelEntry.Group groupRow:
                var group = Library.Instance.Groups[groupRow.Index];
                bannerPath = group.BannerPath;
                fallbackTitle = group.Name;
                headline = group.Stepfiles.Count switch
                {
                    1 => "1 stepfile",
                    var count => $"{count} stepfiles"
                };
                break;

            default:
                return;
        }

        // Fall back to default BGM banner
        bannerPath ??= Library.Instance.DefaultBgm.BannerPath();

        infoPanel = new Node2D
        {
            Position = new Vector2(DetailsBoxCenterX, 0.0f),
            ZIndex = 50,
        };

        // Banner
        var bannerY = DetailsBoxCenterY + (DetailsBoxSizeY - BannerSizeY) / 2.0f;
        bannerRect = new TextureRect
        {
            Size = new Vector2(BannerSizeX, BannerSizeY),
            Position = new Vector2(-BannerSizeX / 2.0f, -bannerY - BannerSizeY / 2.0f),
            ExpandMode = TextureRect.ExpandModeEnum.IgnoreSize,
            ClipContents = true,
        };
        infoPanel.AddChild(bannerRect);

        if (bannerPath != null)
        {
            bannerRect.StretchMode = TextureRect.StretchModeEnum.KeepAspectCovered;
            bannerPending = PendingTexture.Load(bannerPath);
        }
        else
        {
            bannerRect.StretchMode = TextureRect.StretchModeEnum.Scale;
            bannerRect.Texture = barTexture;
            bannerRect.Modulate = BannerTintColor;

            var titleLabel = Text.Label(fallbackTitle, 24.0f, BannerTextColor);
            infoPanel.AddChild(titleLabel);
            Text.Place(titleLabel, new Vector2(0.0f, -bannerY), TextPivot.Center);
            titleLabel.ZIndex = 1;
        }

        // Headline (BPM or file count)
        var headlineLabel = Text.Label(headline, 28.0f, BpmTextColor);
        infoPanel.AddChild(headlineLabel);
        Text.Place(headlineLabel, new Vector2(0.0f, -70.0f), TextPivot.Center);

        // Difficulty lines and stats
        var tagged = charts.Count > 1;
        for (int row = 0; row < charts.Count; row++)
        {
            var (player, chartStepfileId, chartIndex) = charts[row];
            var stepfile = Library.Instance.Stepfile(chartStepfileId).Stepfile;
            var chart = stepfile.Charts[chartIndex];
            var (name, color) = DifficultyStyle(chart.Difficulty);

            var playerLabel = player == PlayerId.P1 ? "P1" : "P2";
            var line = tagged ? $"{playerLabel}  {name} {chart.Meter}" : $"{name} {chart.Meter}";
            var chartLine = Text.Label(line, 34.0f, color);
            infoPanel.AddChild(chartLine);
            Text.Place(chartLine, new Vector2(0.0f, -(18.0f - row * 42.0f)), TextPivot.Center);

            if (!tagged)
            {
                foreach (var (text, x, y) in StatGrid(stepfile, chartIndex))
                {
                    var cell = Text.Label(text, 22.0f, StatsTextColor);
                    infoPanel.AddChild(cell);
                    Text.Place(cell, new Vector2(x, -y), TextPivot.CenterLeft);
                }
            }
        }

        canvas?.AddChild(infoPanel);
    }

    private void PollBanner()
    {
        if (bannerPending is null || bannerRect is null)
            return;

        var loaded = bannerPending.Poll();
        if (loaded is null)
            return;

        if (loaded.Value.Texture != null)
            bannerRect.Texture = loaded.Value.Texture;

        bannerPending = null;
    }

    private string BpmLabel(Stepfile stepfile)
    {
        if (stepfile.DisplayBpm is DisplayBpm.Single single)
            return $"BPM {single.Bpm}";
        if (stepfile.DisplayBpm is DisplayBpm.Range range)
            return $"BPM {range.Low}-{range.High}";
        if (stepfile.DisplayBpm is DisplayBpm.Random)
            return "BPM ???";

        var (low, high) = stepfile.Timing.BpmRange();
        if (Math.Abs(high.Value - low.Value) < 0.5)
            return $"BPM {low}";
        return $"BPM {low}-{high}";
    }

    private (string, Color) DifficultyStyle(Difficulty difficulty) =>
        difficulty.Kind switch
        {
            DifficultyKind.Beginner => ("Beginner", new Color(0.35f, 0.9f, 0.95f)),
            DifficultyKind.Easy => ("Basic", new Color(0.95f, 0.8f, 0.25f)),
            DifficultyKind.Medium => ("Difficult", new Color(0.95f, 0.35f, 0.3f)),
            DifficultyKind.Hard => ("Expert", new Color(0.4f, 0.95f, 0.4f)),
            DifficultyKind.Challenge => ("Challenge", new Color(0.8f, 0.45f, 0.95f)),
            DifficultyKind.Edit => ("Edit", new Color(0.7f, 0.7f, 0.75f)),
            DifficultyKind.Other => (difficulty.Raw, new Color(0.7f, 0.7f, 0.75f)),
            _ => ("Unknown", new Color(0.7f, 0.7f, 0.75f)),
        };

    private List<(string, float, float)> StatGrid(Stepfile stepfile, int chartIndex)
    {
        const float StatColumnLabel0 = -170.0f;
        const float StatColumnValue0 = -75.0f;
        const float StatColumnLabel1 = 35.0f;
        const float StatColumnValue1 = 130.0f;
        const float StatTopY = -48.0f;
        const float StatRowHeight = 28.0f;

        var chart = stepfile.Charts[chartIndex];
        var stats = chart.Stats();
        var duration = chart.LastNoteBeat() is Beat beat
            ? stepfile.Timing.SecondsAtBeat(beat)
            : Seconds.Zero;

        int minutes = (int)(double.Max(duration.Value, 0.0) / 60.0);
        int seconds = (int)(double.Max(duration.Value, 0.0) % 60.0);

        var pairs = new[]
        {
            ("Steps", stats.Steps.ToString()),
            ("Jumps", stats.Jumps.ToString()),
            ("Holds", stats.Holds.ToString()),
            ("Mines", stats.Mines.ToString()),
            ("Length", $"{minutes}:{seconds:D2}"),
        };

        var cells = new List<(string, float, float)>();
        for (int i = 0; i < pairs.Length; i++)
        {
            var (name, value) = pairs[i];
            int row = i / 2;
            int col = i % 2;

            float labelX = col == 0 ? StatColumnLabel0 : StatColumnLabel1;
            float valueX = col == 0 ? StatColumnValue0 : StatColumnValue1;
            float y = StatTopY - row * StatRowHeight;

            cells.Add((name, labelX, y));
            cells.Add((value, valueX, y));
        }

        return cells;
    }
}
