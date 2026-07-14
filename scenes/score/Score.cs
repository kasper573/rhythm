using System.Globalization;
using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The score scene: displays a finished session's results, one player column
/// per stage, showing the outcome, score percentage, rating art, and tallies.
/// </summary>
[GlobalClass]
public partial class Score : Control
{
    private List<PlayerId> players = [];
    private StepfileId? id;
    private List<(PendingTexture, TextureRect)> ratings = [];

    public override void _Ready()
    {
        if (Engine.IsEditorHint())
        {
            return;
        }

        var game = Game.Instance;
        var results = game.TakeScoreResults();

        if (results is null)
        {
            game.ChangeScene(GameScene.Wheel);
            return;
        }

        Scenes.PlayDefaultBgm();
        Scenes.SpawnDefaultBackground(this);

        var title = GetNode<Label>("%Title");
        title.Text = results.Title;

        var columnsRow = GetNode<HBoxContainer>("%ColumnsRow");

        var tagged = results.Players.Count > 1;
        id = results.Id;

        var config = Config.Current!;
        var library = Library.Instance;

        foreach (var player in results.Players)
        {
            var stage = player.Stage;
            var tally = ComputeTally(stage, config);
            var chart = library.Stepfile(results.Id).Stepfile.Charts[player.Chart];
            var key = HighScores.HighscoreKey(library, results.Id, chart);
            var newHighScore = HighScores.Instance.Record(stage.Player, key, tally.TotalPoints);

            var playerColumn = PlayerColumn(stage, tally, config, newHighScore, tagged);
            columnsRow.AddChild(playerColumn);
            players.Add(stage.Player);
        }
    }

    private VBoxContainer PlayerColumn(StageResults stage, Tally tally, GameConfig config, bool newHighScore, bool tagged)
    {
        var column = new VBoxContainer();
        column.AddThemeConstantOverride("separation", 8);
        column.Alignment = BoxContainer.AlignmentMode.Center;

        if (tagged)
        {
            var playerLabel = stage.Player == PlayerId.P1 ? "P1" : "P2";
            var tag = Text.Label(playerLabel, 36.0f, Screen.TitleColor);
            tag.HorizontalAlignment = HorizontalAlignment.Center;
            tag.SizeFlagsHorizontal = Control.SizeFlags.ShrinkCenter;
            column.AddChild(tag);
        }

        var (resultLabel, resultColor) = stage.Failed
            ? ("FAILED", new Color(0.95f, 0.25f, 0.25f, 1.0f))
            : ("CLEARED", new Color(0.5f, 0.95f, 0.6f, 1.0f));

        var result = Text.Label(resultLabel, 34.0f, resultColor);
        result.HorizontalAlignment = HorizontalAlignment.Center;
        var resultBox = new MarginContainer();
        resultBox.AddThemeConstantOverride("margin_bottom", 12);
        resultBox.SizeFlagsHorizontal = Control.SizeFlags.ShrinkCenter;
        resultBox.AddChild(result);
        column.AddChild(resultBox);

        var scoreRow = new HBoxContainer();
        scoreRow.AddThemeConstantOverride("separation", 16);
        scoreRow.SizeFlagsHorizontal = Control.SizeFlags.ShrinkCenter;

        var percent = Text.Label(tally.Percent.ToString(), 42.0f, new Color(0.95f, 0.97f, 1.0f, 1.0f));
        scoreRow.AddChild(percent);

        var ratingBox = new Control();
        ratingBox.CustomMinimumSize = new Vector2(56.0f, 56.0f);
        var rating = new TextureRect();
        rating.ExpandMode = TextureRect.ExpandModeEnum.IgnoreSize;
        rating.StretchMode = TextureRect.StretchModeEnum.KeepAspect;
        rating.SetAnchorsAndOffsetsPreset(LayoutPreset.FullRect);
        ratingBox.AddChild(rating);

        if (newHighScore)
        {
            var ribbon = Text.Label("New high score!", 16.0f, new Color(1.0f, 0.85f, 0.35f, 1.0f));
            ribbon.HorizontalAlignment = HorizontalAlignment.Center;
            ribbon.SetAnchorsAndOffsetsPreset(LayoutPreset.BottomWide);
            ribbon.SetOffset(Side.Top, -4.0f);
            ribbon.SetOffset(Side.Bottom, 20.0f);
            ratingBox.AddChild(ribbon);
        }

        scoreRow.AddChild(ratingBox);

        var scoreBox = new MarginContainer();
        scoreBox.AddThemeConstantOverride("margin_bottom", 10);
        scoreBox.SizeFlagsHorizontal = Control.SizeFlags.ShrinkCenter;
        scoreBox.AddChild(scoreRow);
        column.AddChild(scoreBox);

        var image = config.Rating(tally.Percent, tally.WorstGrade).Image;
        ratings.Add((PendingTexture.Load(Assets.Path(image)), rating));

        var tallies = new HBoxContainer();
        tallies.AddThemeConstantOverride("separation", 28);
        tallies.SizeFlagsHorizontal = Control.SizeFlags.ShrinkCenter;

        var labelsColumn = new VBoxContainer();
        labelsColumn.AddThemeConstantOverride("separation", 2);
        var valuesColumn = new VBoxContainer();
        valuesColumn.AddThemeConstantOverride("separation", 2);

        if (config.Grading is not null && config.Grading.Dynamic.Count > 0)
        {
            for (int i = 0; i < config.Grading.Dynamic.Count && i < tally.GradeCounts.Length; i++)
            {
                var grade = config.Grading.Dynamic[i];
                labelsColumn.AddChild(Text.Label(grade.Name, 30.0f, grade.Color));
                valuesColumn.AddChild(Text.Label(tally.GradeCounts[i].ToString(CultureInfo.InvariantCulture), 30.0f, grade.Color));
            }
        }

        if (config.Grading?.Miss is { } miss)
        {
            labelsColumn.AddChild(Text.Label(miss.Name, 30.0f, miss.Color));
            valuesColumn.AddChild(Text.Label(tally.MissCount.ToString(CultureInfo.InvariantCulture), 30.0f, miss.Color));
        }

        labelsColumn.AddChild(Text.Label("Holds", 30.0f, new Color(0.8f, 0.85f, 0.8f, 1.0f)));
        valuesColumn.AddChild(Text.Label($"{stage.HoldsOk}/{stage.HoldsTotal}", 30.0f, new Color(0.8f, 0.85f, 0.8f, 1.0f)));

        labelsColumn.AddChild(Text.Label("Mines", 30.0f, new Color(0.8f, 0.85f, 0.8f, 1.0f)));
        var avoided = stage.MinesTotal - stage.MinesExploded;
        valuesColumn.AddChild(Text.Label($"{avoided}/{stage.MinesTotal}", 30.0f, new Color(0.8f, 0.85f, 0.8f, 1.0f)));

        tallies.AddChild(labelsColumn);
        tallies.AddChild(valuesColumn);
        column.AddChild(tallies);

        var comboGap = new Control();
        comboGap.CustomMinimumSize = new Vector2(0.0f, 8.0f);
        column.AddChild(comboGap);

        var combo = Text.Label($"Max combo: {stage.MaxCombo}", 32.0f, new Color(0.7f, 0.85f, 1.0f, 1.0f));
        combo.HorizontalAlignment = HorizontalAlignment.Center;
        combo.SizeFlagsHorizontal = Control.SizeFlags.ShrinkCenter;
        column.AddChild(combo);

        return column;
    }

    public override void _Process(double delta)
    {
        ratings.RemoveAll(pair =>
        {
            var (pending, target) = pair;
            if (pending.Poll() is PendingTexture.Loaded loaded)
            {
                if (loaded.Texture is not null)
                {
                    target.Texture = loaded.Texture;
                }
                return true;
            }
            return false;
        });

        if (!Game.Instance.AcceptsInput)
            return;

        if (Actions.AnyJustPressed(players, p => p == PlayerId.P1 ? GameAction.P1Select : GameAction.P2Select))
        {
            Sfx.Select.Play();
        }
        else if (Actions.AnyJustPressed(players, p => p == PlayerId.P1 ? GameAction.P1Cancel : GameAction.P2Cancel))
        {
            Sfx.Cancel.Play();
        }
        else
        {
            return;
        }

        if (id.HasValue)
        {
            Game.Instance.SetWheelTarget(id.Value);
        }
        Game.Instance.ChangeScene(GameScene.Wheel);
    }

    private static Tally ComputeTally(StageResults stage, GameConfig config)
    {
        if (config.Grading is null)
        {
            return new Tally
            {
                GradeCounts = [],
                MissCount = 0,
                Percent = new Percent(0),
                TotalPoints = 0,
                WorstGrade = null,
            };
        }

        var gradeCounts = new uint[config.Grading.Dynamic.Count];
        var totalPoints = 0u;
        var missCount = 0u;

        foreach (var outcome in stage.Outcomes)
        {
            var grade = config.ClassifyGrade(outcome);
            if (grade is Grade.Hit hitGrade)
            {
                gradeCounts[hitGrade.Index.Value]++;
                totalPoints += (uint)config.Grading.Dynamic[hitGrade.Index.Value].Points;
            }
            else if (grade is Grade.Miss)
            {
                missCount++;
            }
        }

        totalPoints += stage.HoldsOk * (uint)(config.Grading.Ok?.Points ?? 0);
        totalPoints += stage.HoldsNg * (uint)(config.Grading.Ng?.Points ?? 0);

        var percent = config.ScorePercent(totalPoints, stage.RowsTotal, stage.HoldsTotal);

        var complete = stage.Outcomes.Count == stage.RowsTotal;
        Grade? worstGrade = null;
        if (complete)
        {
            if (missCount > 0)
            {
                worstGrade = new Grade.Miss();
            }
            else
            {
                for (int i = gradeCounts.Length - 1; i >= 0; i--)
                {
                    if (gradeCounts[i] > 0)
                    {
                        worstGrade = new Grade.Hit(new GradeIndex(i));
                        break;
                    }
                }
            }
        }

        return new Tally
        {
            GradeCounts = gradeCounts,
            MissCount = missCount,
            Percent = percent,
            TotalPoints = totalPoints,
            WorstGrade = worstGrade,
        };
    }

    private sealed class Tally
    {
        public required uint[] GradeCounts { get; init; }
        public required uint MissCount { get; init; }
        public required Percent Percent { get; init; }
        public required uint TotalPoints { get; init; }
        public required Grade? WorstGrade { get; init; }
    }
}
