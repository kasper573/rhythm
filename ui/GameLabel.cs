using Godot;

namespace Rhythm;

/// <summary>
/// A label carrying the game's sizing model, so designers author text in the
/// editor and see it as it renders: every line box is 1.2× the font size with
/// the glyphs centered in it (this font's own metrics pack stacked text
/// tighter). Set <see cref="Size"/> in the inspector; colour is a normal theme
/// override on the node. The default theme supplies the font.
/// </summary>
[Tool]
[GlobalClass]
public partial class GameLabel : Label
{
    private float fontSize = 30.0f;

    [Export(PropertyHint.Range, "8,120,1,or_greater")]
    public float FontSize
    {
        get => fontSize;
        set
        {
            fontSize = value;
            ApplySizing();
        }
    }

    public override void _Ready() => ApplySizing();

    private void ApplySizing()
    {
        var pixels = Mathf.RoundToInt(fontSize);
        VerticalAlignment = VerticalAlignment.Center;
        AddThemeFontSizeOverride("font_size", pixels);
        CustomMinimumSize = new Vector2(CustomMinimumSize.X, Mathf.Round(fontSize * 1.2f));
        AddThemeConstantOverride("line_spacing", Mathf.RoundToInt((fontSize * 1.2f) - Rhythm.Text.Font.GetHeight(pixels)));
    }
}
