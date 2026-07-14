using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The judgment word that pops on each graded row, shaded live: the word is
/// rendered white into an offscreen viewport so its alpha is pure coverage,
/// then presented on a sprite whose material tints it to the grade color and
/// layers on an additive glow that pulses — one shader (the colocated
/// <c>GradeText.gdshader</c>), per-grade colors and strengths.
/// </summary>
public static class GradeText
{
    /// <summary>The word's font height, in canvas units.</summary>
    public const float FontSize = 50.0f;

    /// <summary>Canvas size of the presented sprite; the offscreen viewport frames exactly this.</summary>
    public const float PresentW = 760.0f;
    public const float PresentH = 170.0f;

    /// <summary>The offscreen word renders larger than presented so it stays crisp when scaled up.</summary>
    public const float Supersample = 2.0f;

    /// <summary>The neon glow's reach around the glyphs, in canvas units, and its brightness.</summary>
    public const float HaloRadius = 18.0f;
    public const float HaloStrength = 1.6f;

    /// <summary>The glow strikes to full when a grade lands, then drains toward <see cref="GlowFloor"/>.</summary>
    public const float PulseTau = 0.3f;
    public const float GlowFloor = 0.32f;

    public const float GradeHalfHeight = 36.0f;
    public const float ComboHalfHeight = 24.0f;

    /// <summary>The combo readout sits this far under the grade's center.</summary>
    public const float ComboGap = 62.0f;

    public const float BounceSeconds = 0.13f;
    public const float BounceAmount = 0.18f;

    /// <summary>Seconds a grade takes to fade out once the player stops hitting.</summary>
    public const float FadeSeconds = 1.0f;

    public static readonly Shader GlowShader = GD.Load<Shader>("res://nodes/StepfilePlayer/GradeText.gdshader");

    /// <summary>
    /// The grade word's y for a player's grade-position percentage within its
    /// area: 0% at the top, 100% at the bottom.
    /// </summary>
    public static float GradeY(GradeArea area, Percent gradePosition)
    {
        var t = Math.Clamp(gradePosition.Value / 100.0f, 0.0f, 1.0f);
        return area.Top + ((area.Bottom - area.Top) * t);
    }

    /// <summary>
    /// The grade area for a usable band spanning <paramref name="topEdge"/>..
    /// <paramref name="bottomEdge"/> (centered y-up), inset so the word and
    /// the combo tracking under it both stay inside.
    /// </summary>
    public static GradeArea AreaOf(float topEdge, float bottomEdge) =>
        new(topEdge - GradeHalfHeight, bottomEdge + ComboGap + ComboHalfHeight);

    /// <summary>Packs a grade's colors into the shader uniforms at a given fade and glow pulse.</summary>
    public static void ApplyStyle(ShaderMaterial material, Color @base, Color glow, float strength, float intensity, float pulse)
    {
        material.SetShaderParameter("base_color", new Vector4(@base.R, @base.G, @base.B, intensity));
        material.SetShaderParameter("glow_color", new Vector4(glow.R, glow.G, glow.B, strength * pulse));
    }

    /// <summary>The glow strength at a moment since the grade landed: full at the strike, draining toward the floor.</summary>
    public static float GlowPulse(float seconds) => GlowFloor + ((1.0f - GlowFloor) * Mathf.Exp(-seconds / PulseTau));

    public static GradeStyle StyleFor(GameConfig config, RowOutcome outcome)
    {
        var grading = config.Grading ?? throw new InvalidOperationException("Grading is not configured");
        switch (outcome)
        {
            case RowOutcome.Hit hit:
                var grade = (Grade.Hit)config.ClassifyGrade(outcome);
                var def = grading.Dynamic[grade.Index.Value];
                // Like ITG: the letters are white, the grade's color is the glow.
                return new GradeStyle(HitText(def, hit.Error), Colors.White, def.GlowColor, def.GlowStrength);
            default:
                // ITG's Miss is the exception — its letters carry the red.
                var miss = grading.Miss ?? throw new InvalidOperationException("Miss grade is not configured");
                return new GradeStyle(miss.Name, miss.Color, miss.GlowColor, miss.GlowStrength);
        }
    }

    /// <summary>
    /// The word for a hit, marking the side of the perfect moment the input
    /// fell on: early feedback leads the name, late feedback trails it.
    /// </summary>
    private static string HitText(GradeDef def, Seconds error)
    {
        var early = error.Value > 0.0;
        var offsetMs = (long)Math.Round(-error.ToMillis());
        return def.TimingFeedback switch
        {
            TimingFeedbackKind.Off => def.Name,
            TimingFeedbackKind.Sign when early => $"-{def.Name}",
            TimingFeedbackKind.Sign => $"{def.Name}-",
            TimingFeedbackKind.Millis when early => $"({offsetMs}ms) {def.Name}",
            _ => $"{def.Name} (+{offsetMs}ms)",
        };
    }

    internal static float EaseCubicOut(float t) => 1.0f - Mathf.Pow(1.0f - t, 3);
}

/// <summary>
/// The canvas Y band the grade group occupies (centered y-up coordinates),
/// top (0%) to bottom (100%). The session's owner fills it.
/// </summary>
public readonly record struct GradeArea(float Top, float Bottom);

/// <summary>The word, base color, glow color, and glow strength one outcome shows.</summary>
public readonly record struct GradeStyle(string Text, Color Base, Color Glow, float Strength);

/// <summary>
/// The pieces of one grade-text rig: the offscreen white word and the shader
/// sprite presenting it. Owners position and re-layer the sprite; freeing it
/// frees the whole rig (the viewport rides along as its child).
/// </summary>
public sealed class GradeRig
{
    private readonly Label label;

    private GradeRig(Sprite2D sprite, Label label, ShaderMaterial material)
    {
        Sprite = sprite;
        this.label = label;
        Material = material;
    }

    public Sprite2D Sprite { get; }
    public ShaderMaterial Material { get; }

    /// <summary>Builds a grade-text rig under <paramref name="layer"/>, presenting at the layer-local origin.</summary>
    public static GradeRig SpawnRig(Node2D layer)
    {
        var viewport = new SubViewport
        {
            TransparentBg = true,
            Size = new Vector2I((int)(GradeText.PresentW * GradeText.Supersample), (int)(GradeText.PresentH * GradeText.Supersample)),
            RenderTargetUpdateMode = SubViewport.UpdateMode.Always,
        };
        var word = Text.Label(string.Empty, GradeText.FontSize * GradeText.Supersample, Colors.White);
        viewport.AddChild(word);
        Text.Place(word, new Vector2(GradeText.PresentW, GradeText.PresentH) * GradeText.Supersample / 2.0f, TextPivot.Center);

        var material = new ShaderMaterial { Shader = GradeText.GlowShader };
        material.SetShaderParameter("shape", new Vector4(GradeText.HaloRadius / GradeText.PresentW, GradeText.HaloRadius / GradeText.PresentH, GradeText.HaloStrength, 0.0f));

        var sprite = new Sprite2D { Scale = Vector2.One / GradeText.Supersample, Material = material };
        sprite.AddChild(viewport);
        sprite.Texture = viewport.GetTexture();
        layer.AddChild(sprite);
        return new GradeRig(sprite, word, material);
    }

    public void SetText(string text)
    {
        label.Text = text;
        Text.Place(label, new Vector2(GradeText.PresentW, GradeText.PresentH) * GradeText.Supersample / 2.0f, TextPivot.Center);
    }

    public void Free() => Sprite.QueueFree();
}

/// <summary>One player's grade word on stage, driven by the session: refreshed on each graded row, animated every frame.</summary>
public sealed class GradeDisplay
{
    private float originX;
    private float intensity;
    private float pulse;
    private float bounce;
    private Color @base = Colors.White;
    private Color glow = Colors.White;
    private float strength;

    public GradeDisplay(Node2D layer, PlayerId player, float originX)
    {
        Player = player;
        Rig = GradeRig.SpawnRig(layer);
        Rig.Sprite.Position = new Vector2(originX, 0.0f);
        Rig.Sprite.Visible = false;
        this.originX = originX;
    }

    public PlayerId Player { get; }
    public GradeRig Rig { get; }

    public void SetOriginX(float originX) => this.originX = originX;

    /// <summary>A graded row refreshes the word, color, and glow, and restarts the pop and fade.</summary>
    public void Apply(GameConfig config, RowOutcome outcome)
    {
        var style = GradeText.StyleFor(config, outcome);
        Rig.SetText(style.Text);
        @base = style.Base;
        glow = style.Glow;
        strength = style.Strength;
        intensity = 1.0f;
        pulse = 0.0f;
        bounce = 1.0f;
        Rig.Sprite.Visible = true;
    }

    /// <summary>Advances the word's fade, glow pulse, and pop, and keeps it at <paramref name="y"/> (layer-local y-up).</summary>
    public void Animate(float delta, float y)
    {
        if (intensity <= 0.0f)
        {
            return;
        }

        intensity = Math.Max(intensity - (delta / GradeText.FadeSeconds), 0.0f);
        pulse += delta;
        bounce = Math.Max(bounce - (delta / GradeText.BounceSeconds), 0.0f);

        GradeText.ApplyStyle(Rig.Material, @base, glow, strength, intensity, GradeText.GlowPulse(pulse));

        var eased = GradeText.EaseCubicOut(bounce);
        Rig.Sprite.Scale = Vector2.One * ((1.0f + (GradeText.BounceAmount * eased)) / GradeText.Supersample);
        Rig.Sprite.Position = new Vector2(originX, -y);
    }
}
