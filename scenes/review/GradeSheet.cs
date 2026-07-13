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

    /// <summary>Gray backdrop color matching the playfield.</summary>
    private static readonly Color PlayfieldGray = new(0.30f, 0.31f, 0.35f, 1.0f);

    public override void _Ready()
    {
        var window = GetWindow();
        if (window == null)
        {
            GetTree().Quit();
            return;
        }

        window.ContentScaleMode = Window.ContentScaleModeEnum.Disabled;
        var size = window.Size;

        SetAnchorsAndOffsetsPreset(Control.LayoutPreset.FullRect);

        var backdrop = new ColorRect();
        backdrop.Color = Screen.ClearColor;
        backdrop.SetAnchorsAndOffsetsPreset(Control.LayoutPreset.FullRect);
        AddChild(backdrop);

        // Two side-by-side backgrounds: black on left, gray on right
        var colors = new[] { Colors.Black, PlayfieldGray };
        for (int column = 0; column < 2; column++)
        {
            var half = new ColorRect();
            half.Color = colors[column];
            half.Position = new Vector2(column * size.X / 2.0f, 0.0f);
            half.Size = new Vector2(size.X / 2.0f, size.Y);
            AddChild(half);
        }

        // Create the canvas for grade words
        var canvas = new Node2D();
        canvas.Position = new Vector2(size.X / 2.0f, size.Y / 2.0f);
        AddChild(canvas);

        // Add captions
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

        // Build the list of outcomes to display
        var outcomes = GetGradeOutcomes();

        // Vertical spacing and positioning
        var rowGap = (size.Y - 90.0f) / outcomes.Count;
        var top = (outcomes.Count - 1) * rowGap / 2.0f;

        // Render each grade in both columns
        for (int row = 0; row < outcomes.Count; row++)
        {
            var outcome = outcomes[row];
            var style = GradeStyleUtility.ComputeStyle(Config.Current, outcome);
            var y = top - row * rowGap;

            foreach (var x in ColumnX)
            {
                var rig = GradeRig.SpawnRig(canvas);
                rig.SetText(style.Text);
                rig.Material.SetShaderParameter("base_color",
                    new Vector4(style.BaseColor.R, style.BaseColor.G, style.BaseColor.B, 1.0f));
                rig.Material.SetShaderParameter("glow_color",
                    new Vector4(style.GlowColor.R, style.GlowColor.G, style.GlowColor.B, 1.0f));
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
        if (config.Grading?.Dynamic == null)
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
