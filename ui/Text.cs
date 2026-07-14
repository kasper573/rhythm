using Godot;

namespace Rhythm;

/// <summary>
/// How the game makes text. The default theme supplies the font
/// (<c>res://ui/Theme.tres</c>); this adds the sizing model the layout was
/// designed on and free-floating placement for canvas compositions.
/// </summary>
public static class Text
{
    private static FontFile? font;

    /// <summary>
    /// The game font, bundled with coverage for every symbol the stepfile
    /// library's names use (Latin, Greek, Cyrillic, kana, CJK). Loaded once.
    /// </summary>
    public static FontFile Font => font ??= GD.Load<FontFile>("res://ui/ipagp.ttf");

    /// <summary>
    /// A label at a size and color. Every line box is 1.2× the font size
    /// with the glyphs centered in it — the layout model the game was
    /// designed on — where this font's own metrics would pack stacked text
    /// visibly tighter.
    /// </summary>
    public static Label Label(string text, float size, Color color)
    {
        var label = new GameLabel { Text = text, FontSize = size };
        label.AddThemeColorOverride("font_color", color);
        return label;
    }

    /// <summary>
    /// Sizes the label to its content and places it so <paramref name="pivot"/>
    /// lands on <paramref name="position"/> — free-floating text placement.
    /// Call again after changing the text.
    /// </summary>
    public static void Place(Label label, Vector2 position, TextPivot pivot)
    {
        // Measure the glyphs directly from the font: a label's
        // GetCombinedMinimumSize ignores its font-size override until it is in
        // the tree and laid out, so before that it underestimates the width
        // and pivot-centering lands the text off to one side.
        var fontSize = label is GameLabel gameLabel
            ? Mathf.RoundToInt(gameLabel.FontSize)
            : label.GetThemeFontSize("font_size");
        var size = Font.GetMultilineStringSize(label.Text, HorizontalAlignment.Left, -1.0f, fontSize);
        label.ResetSize();
        label.Position = position - new Vector2(size.X * pivot.X, size.Y * pivot.Y);
    }
}

/// <summary>
/// Where a label's position anchors relative to its rendered size, as
/// fractions of it: <c>(0, 0)</c> is top-left, <c>(0.5, 0.5)</c> dead center.
/// </summary>
public readonly record struct TextPivot(float X, float Y)
{
    public static readonly TextPivot Center = new(0.5f, 0.5f);
    public static readonly TextPivot CenterLeft = new(0.0f, 0.5f);
}
