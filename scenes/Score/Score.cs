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
            game.ChangeScene(GameScene.StepfileSelect);
            return;
        }

        Scenes.PlayDefaultBgm();
        Scenes.SpawnDefaultBackground(this);

        var title = GetNode<Label>("%Title");
        title.Text = results.Title;

        var columnsRow = GetNode<HBoxContainer>("%ColumnsRow");
        var columns = columnsRow.GetChildren().OfType<PlayerColumn>().ToList();

        var tagged = results.Players.Count > 1;
        id = results.Id;

        var config = Config.Current;
        var library = Library.Instance;

        for (int i = 0; i < columns.Count; i++)
        {
            if (i >= results.Players.Count)
            {
                columns[i].Visible = false;
                continue;
            }

            var player = results.Players[i];
            var stage = player.Stage;
            var tally = ComputeTally(stage, config);
            var chart = library.Stepfile(results.Id).Stepfile.Charts[player.Chart];
            var key = HighScores.Key(library, results.Id, chart);
            var newHighScore = HighScores.Instance.Record(stage.Player, key, tally.TotalPoints);

            columns[i].Visible = true;
            Populate(columns[i], stage, tally, config, newHighScore, tagged);
            players.Add(stage.Player);
        }
    }

    /// <summary>Fills one authored player column with a stage's result, score, rating, tallies, and combo.</summary>
    private void Populate(PlayerColumn column, StageResults stage, Tally tally, GameConfig config, bool newHighScore, bool tagged)
    {
        var tag = column.GetNode<Label>("%PlayerTag");
        tag.Text = stage.Player == PlayerId.P1 ? "P1" : "P2";
        tag.Visible = tagged;

        var (resultText, resultColor) = stage.Failed
            ? ("FAILED", new Color(0.95f, 0.25f, 0.25f))
            : ("CLEARED", new Color(0.5f, 0.95f, 0.6f));
        var result = column.GetNode<Label>("%Result");
        result.Text = resultText;
        result.AddThemeColorOverride("font_color", resultColor);

        column.GetNode<Label>("%Percent").Text = tally.Percent.ToString();

        if (newHighScore)
        {
            column.GetNode<Label>("%Ribbon").Visible = true;
        }

        var image = config.Rating(tally.Percent, tally.WorstGrade).Image;
        ratings.Add((PendingTexture.Load(Assets.Path(image)), column.GetNode<TextureRect>("%Rating")));

        column.FillTallies(config, tally.GradeCounts, tally.MissCount, stage.HoldsOk, stage.HoldsTotal, stage.MinesTotal - stage.MinesExploded, stage.MinesTotal);

        column.GetNode<Label>("%Combo").Text = $"Max combo: {stage.MaxCombo}";
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
        Game.Instance.ChangeScene(GameScene.StepfileSelect);
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
