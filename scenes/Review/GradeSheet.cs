using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The grade sheet: every grade text at peak glow, on black and on a
/// playfield-like gray side by side, so the grade shader can be reviewed
/// and tuned without playing.
/// </summary>
[GlobalClass]
public partial class GradeSheet : Control
{
    /// <summary>The two columns' word centers, from the canvas center.</summary>
    private static readonly float[] ColumnX = [-210.0f, 210.0f];

    public override void _Ready()
    {
        var window = GetWindow();
        if (window is null)
        {
            GetTree().Quit();
            return;
        }

        window.ContentScaleMode = Window.ContentScaleModeEnum.Disabled;
        var size = window.Size;

        var canvas = new Node2D();
        canvas.Position = new Vector2(size.X / 2.0f, size.Y / 2.0f);
        AddChild(canvas);

        var captions = new[] { "on black", "on gray" };
        for (int column = 0; column < 2; column++)
        {
            var caption = Text.Label(captions[column], 24.0f, new Color(0.6f, 0.6f, 0.6f));
            canvas.AddChild(caption);
            Text.Place(
                caption,
                new Vector2(ColumnX[column], -(size.Y / 2.0f - 26.0f)),
                TextPivot.Center
            );
        }

        var outcomes = GetGradeOutcomes();

        var rowGap = (size.Y - 90.0f) / outcomes.Count;
        var top = (outcomes.Count - 1) * rowGap / 2.0f;

        for (int row = 0; row < outcomes.Count; row++)
        {
            var outcome = outcomes[row];
            var style = GradeText.StyleFor(Config.Current, outcome);
            var y = top - (row * rowGap);

            foreach (var x in ColumnX)
            {
                var rig = GradeRig.SpawnRig(canvas);
                rig.SetText(style.Text);
                GradeText.ApplyStyle(rig.Material, style.Base, style.Glow, style.Strength, 1.0f, GradeText.GlowPulse(0.0f));
                rig.Sprite.Position = new Vector2(x, -y);
            }
        }
    }

    /// <summary>
    /// Generates a representative outcome for each dynamic grade (at its window
    /// midpoint so it grades to exactly that tier) followed by a miss.
    /// </summary>
    private static List<RowOutcome> GetGradeOutcomes()
    {
        var outcomes = new List<RowOutcome>();
        var config = Config.Current;
        if (config.Grading?.Dynamic is null)
        {
            return outcomes;
        }

        var lower = Seconds.Zero;
        foreach (var grade in config.Grading.Dynamic)
        {
            var midpoint = new Seconds((lower.Value + grade.Window.Value) / 2.0);
            outcomes.Add(new RowOutcome.Hit(midpoint));
            lower = grade.Window;
        }

        outcomes.Add(new RowOutcome.Miss());
        return outcomes;
    }
}
