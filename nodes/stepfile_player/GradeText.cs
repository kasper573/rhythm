using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// Constants for grade text display.
/// </summary>
public static class GradeTextConstants
{
    public const float FontSize = 50.0f;
    public const float PresentW = 760.0f;
    public const float PresentH = 170.0f;
    public const float Supersample = 2.0f;
    public const float HaloRadius = 18.0f;
    public const float HaloStrength = 1.6f;
    public const float PulseTau = 0.3f;
    public const float GlowFloor = 0.32f;
    public const float GradeHalfHeight = 36.0f;
    public const float ComboHalfHeight = 24.0f;
    public const float ComboGap = 62.0f;
    public const float BounceSeconds = 0.13f;
    public const float BounceAmount = 0.18f;
    public const float FadeSeconds = 1.0f;
}

/// <summary>Bounds for grade word placement on screen.</summary>
public record GradeArea(float Top, float Bottom)
{
    public float Height => Bottom - Top;

    /// <summary>Computes y position for a grade word within this area.</summary>
    public float GradeY(Percent position)
    {
        var t = position.Value / 100.0f;
        return Top + (Height / 2.0f) * t + Height / 2.0f * (1.0f - t);
    }
}

/// <summary>
/// The pieces of one grade-text rig: the offscreen white word and the
/// shader sprite presenting it. Owners position and re-layer the sprite;
/// freeing it frees the whole rig (the viewport rides along as its child).
/// </summary>
public class GradeRig
{
    public Sprite2D Sprite { get; private set; }
    private Label _label;
    public ShaderMaterial Material { get; private set; }

    private GradeRig(Sprite2D sprite, Label label, ShaderMaterial material)
    {
        Sprite = sprite;
        _label = label;
        Material = material;
    }

    /// <summary>
    /// Builds a grade-text rig under <paramref name="layer"/>, presenting at the
    /// layer-local origin until positioned.
    /// </summary>
    public static GradeRig SpawnRig(Node2D layer)
    {
        var viewport = new SubViewport();
        viewport.TransparentBg = true;
        viewport.Size = new Vector2I(
            (int)(GradeTextConstants.PresentW * GradeTextConstants.Supersample),
            (int)(GradeTextConstants.PresentH * GradeTextConstants.Supersample)
        );

        var word = new Label();
        word.AddThemeColorOverride("font_color", Colors.White);
        var fontFile = GD.Load<FontFile>("res://assets/fonts/JetBrainsMono-Regular.ttf");
        if (fontFile != null)
        {
            word.AddThemeFontOverride("font", fontFile);
        }
        word.AddThemeFontSizeOverride("font_size", (int)(GradeTextConstants.FontSize * GradeTextConstants.Supersample));
        word.Text = "";
        viewport.AddChild(word);
        var wordSize = word.GetSize();
        word.Position = new Vector2(
            (GradeTextConstants.PresentW * GradeTextConstants.Supersample) / 2.0f - wordSize.X / 2.0f,
            (GradeTextConstants.PresentH * GradeTextConstants.Supersample) / 2.0f - wordSize.Y / 2.0f
        );

        var shader = GD.Load<Shader>("res://nodes/stepfile_player/grade_text.gdshader");
        if (shader == null)
        {
            GD.PushError("Failed to load grade_text.gdshader");
            shader = new Shader();
        }

        var material = new ShaderMaterial { Shader = shader };
        material.SetShaderParameter("shape",
            new Vector4(
                GradeTextConstants.HaloRadius / GradeTextConstants.PresentW,
                GradeTextConstants.HaloRadius / GradeTextConstants.PresentH,
                GradeTextConstants.HaloStrength,
                0.0f
            )
        );

        var sprite = new Sprite2D();
        sprite.AddChild(viewport);
        if (viewport.GetTexture() is Texture2D texture)
        {
            sprite.Texture = texture;
        }
        sprite.Scale = Vector2.One / GradeTextConstants.Supersample;
        sprite.Material = material;
        layer.AddChild(sprite);

        return new GradeRig(sprite, word, material);
    }

    /// <summary>Updates the grade word text and repositions it.</summary>
    public void SetText(string text)
    {
        _label.Text = text;
        var size = _label.GetSize();
        _label.Position = new Vector2(
            (GradeTextConstants.PresentW * GradeTextConstants.Supersample) / 2.0f - size.X / 2.0f,
            (GradeTextConstants.PresentH * GradeTextConstants.Supersample) / 2.0f - size.Y / 2.0f
        );
    }

    /// <summary>Queues the sprite for deletion.</summary>
    public void Free()
    {
        Sprite.QueueFree();
    }
}

/// <summary>
/// One player's grade word on stage, driven by the session: refreshed on
/// each graded row, animated every frame.
/// </summary>
public class GradeDisplay
{
    public PlayerId Player { get; private set; }
    public GradeRig Rig { get; private set; }
    private float _originX;
    private float _intensity;
    private float _pulse;
    private float _bounce;
    private Color _base;
    private Color _glow;
    private float _strength;

    public GradeDisplay(Node2D layer, PlayerId player, float originX)
    {
        Player = player;
        Rig = GradeRig.SpawnRig(layer);
        Rig.Sprite.Position = new Vector2(originX, 0.0f);
        Rig.Sprite.Visible = false;
        _originX = originX;
        _intensity = 0.0f;
        _pulse = 0.0f;
        _bounce = 0.0f;
        _base = Colors.White;
        _glow = Colors.White;
        _strength = 0.0f;
    }

    /// <summary>Updates the origin x position.</summary>
    public void SetOriginX(float originX)
    {
        _originX = originX;
        Rig.Sprite.Position = new Vector2(originX, 0.0f);
    }

    /// <summary>
    /// Applies a graded row outcome: updates the text, colors, and starts
    /// the bounce and glow animation.
    /// </summary>
    public void Apply(GameConfig config, RowOutcome outcome)
    {
        var grade = config.ClassifyGrade(outcome);
        var style = GradeStyleUtility.ComputeStyle(config, outcome);

        Rig.SetText(style.Text);
        _base = style.BaseColor;
        _glow = style.GlowColor;
        _strength = style.GlowStrength;
        _intensity = 1.0f;
        _pulse = 0.0f;
        _bounce = GradeTextConstants.BounceSeconds;
        Rig.Sprite.Visible = true;
    }

    /// <summary>
    /// Animates the grade word: fades intensity, pulses the glow, and bounces
    /// the sprite scale. Called every frame.
    /// </summary>
    public void Animate(float delta, float y)
    {
        _intensity = Mathf.Max(0.0f, _intensity - delta / GradeTextConstants.FadeSeconds);
        _pulse += delta;

        var glow = _strength * GlowPulse(_pulse);
        var baseColor = new Vector4(_base.R, _base.G, _base.B, _intensity);
        var glowColor = new Vector4(_glow.R, _glow.G, _glow.B, glow);

        Rig.Material.SetShaderParameter("base_color", baseColor);
        Rig.Material.SetShaderParameter("glow_color", glowColor);

        if (_bounce > 0.0f)
        {
            _bounce -= delta;
            var t = 1.0f - _bounce / GradeTextConstants.BounceSeconds;
            var easeOut = Mathf.Sin(t * Mathf.Pi / 2.0f);
            var scale = 1.0f + (1.0f - easeOut) * GradeTextConstants.BounceAmount;
            Rig.Sprite.Scale = Vector2.One * scale / GradeTextConstants.Supersample;
        }
        else if (!Rig.Sprite.Scale.IsEqualApprox(Vector2.One / GradeTextConstants.Supersample))
        {
            Rig.Sprite.Scale = Vector2.One / GradeTextConstants.Supersample;
        }

        Rig.Sprite.Position = new Vector2(_originX, y);
    }

    private static float GlowPulse(float seconds)
    {
        var tau = Mathf.Exp(-seconds / GradeTextConstants.PulseTau);
        return GradeTextConstants.GlowFloor + (1.0f - GradeTextConstants.GlowFloor) * tau;
    }
}

/// <summary>Style parameters for a grade outcome.</summary>
public record GradeStyle(string Text, Color BaseColor, Color GlowColor, float GlowStrength);

/// <summary>Utility functions for grade styling.</summary>
public static class GradeStyleUtility
{
    /// <summary>Computes the display style for a graded outcome.</summary>
    public static GradeStyle ComputeStyle(GameConfig config, RowOutcome outcome)
    {
        var grade = config.ClassifyGrade(outcome);

        return grade switch
        {
            Grade.Hit hit =>
                new GradeStyle(
                    config.Grading?.Dynamic[hit.Index.Value].Name ?? "HIT",
                    Colors.White,
                    config.Grading?.Dynamic[hit.Index.Value].GlowColor ?? Colors.Yellow,
                    config.Grading?.Dynamic[hit.Index.Value].GlowStrength ?? 1.0f
                ),
            Grade.Miss =>
                new GradeStyle(
                    config.Grading?.Miss?.Name ?? "MISS",
                    Colors.White,
                    config.Grading?.Miss?.GlowColor ?? Colors.Red,
                    config.Grading?.Miss?.GlowStrength ?? 1.0f
                ),
            _ => throw new InvalidOperationException($"Unknown grade type: {grade.GetType().Name}")
        };
    }
}
