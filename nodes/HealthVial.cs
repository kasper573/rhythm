using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>Options for a health vial.</summary>
public class HealthVialOptions
{
    public required float Fill { get; init; }
    public required VialSide Side { get; init; }
    public required float EdgePadding { get; init; }
}

/// <summary>Which screen edge a vial is pinned to.</summary>
public enum VialSide
{
    Left,
    Right,
}

/// <summary>
/// A glass vial of liquid pinned to a screen edge — a health bar. The
/// entire visual (glass, liquid, waves, gradient) is one fragment shader;
/// the node only feeds it smoothed uniforms, with the gradient presets and
/// pulse cycles coming from the config.
///
/// The owner drives the ports every frame: the liquid level eases after
/// <see cref="SetFill"/> (changes stir up waves that settle back flat) and
/// the glow and gradient scroll pulse on <see cref="SetBeat"/>.
/// </summary>
[GlobalClass]
public partial class HealthVial : Control
{
    private const float GlowMargin = 32.0f;
    private const float VialWidth = 50.0f;
    private const float LevelTau = 0.25f;
    private const float TurbulenceTau = 0.9f;
    private const float ColorTau = 0.35f;
    private const int GradientSamples = 16;

    private float fill;
    private Beat beat;
    private VialMotion motion = new();
    private ShaderMaterial? material;
    private ColorRect? shaderRect;

    /// <summary>0..=1 of the vial's capacity.</summary>
    public void SetFill(float value) => fill = value;

    /// <summary>
    /// The musical beat the glow and liquid pulse on; hold it still and
    /// the vial rests.
    /// </summary>
    public void SetBeat(Beat value) => beat = value;

    public static HealthVial Instantiate(HealthVialOptions opt)
    {
        var vial = new HealthVial();
        var preset = opt.Side == VialSide.Left
            ? Control.LayoutPreset.LeftWide
            : Control.LayoutPreset.RightWide;
        vial.SetAnchorsAndOffsetsPreset(preset);
        vial.SetAnchor(Side.Top, 0.1f);
        vial.SetAnchor(Side.Bottom, 0.9f);

        var (leftOffset, rightOffset) = opt.Side == VialSide.Left
            ? (opt.EdgePadding, opt.EdgePadding + VialWidth)
            : (-opt.EdgePadding - VialWidth, -opt.EdgePadding);
        vial.SetOffset(Side.Left, leftOffset);
        vial.SetOffset(Side.Right, rightOffset);
        vial.MouseFilter = MouseFilterEnum.Ignore;

        var shader = GD.Load<Shader>("res://nodes/health_vial.gdshader");
        var shaderMat = new ShaderMaterial { Shader = shader };

        var rect = new ColorRect
        {
            Material = shaderMat,
            MouseFilter = MouseFilterEnum.Ignore
        };
        rect.SetAnchorsAndOffsetsPreset(Control.LayoutPreset.FullRect);
        rect.SetOffset(Side.Left, -GlowMargin);
        rect.SetOffset(Side.Top, -GlowMargin);
        rect.SetOffset(Side.Right, GlowMargin);
        rect.SetOffset(Side.Bottom, GlowMargin);

        vial.AddChild(rect);
        vial.fill = opt.Fill;
        vial.material = shaderMat;
        vial.shaderRect = rect;

        return vial;
    }

    public override void _Process(double delta)
    {
        var deltaF = (float)delta;
        var config = Config.Current;
        var healthBar = config?.HealthBar;
        if (healthBar is null || healthBar.Glow is null || healthBar.Liquid is null)
            return;

        var glow = healthBar.Glow.Pulse(beat);
        var scroll = (healthBar.Liquid.Progress(beat) * 2.0f) % 2.0f;

        var gradient = healthBar.GradientAt(new Percent(fill * 100.0f));

        var targets = new Color[GradientSamples];
        for (int i = 0; i < GradientSamples; i++)
        {
            var at = i / (GradientSamples - 1.0f);
            targets[i] = SampleStops(gradient, new Percent(at * 100.0f));
        }

        if (!motion.Settled)
        {
            motion.Settled = true;
            motion.Level = fill;
            motion.LastFill = fill;
            motion.Colors = (Color[])targets.Clone();
        }

        var stirred = Mathf.Abs(fill - motion.LastFill);
        if (stirred > 0.0f)
        {
            motion.Turbulence = Mathf.Min(motion.Turbulence + 0.35f + stirred * 5.0f, 1.2f);
            motion.LastFill = fill;
        }
        motion.Turbulence *= Mathf.Exp(-deltaF / TurbulenceTau);

        var ease = 1.0f - Mathf.Exp(-deltaF / LevelTau);
        motion.Level += (fill - motion.Level) * ease;

        var blend = 1.0f - Mathf.Exp(-deltaF / ColorTau);
        for (int i = 0; i < motion.Colors.Length; i++)
        {
            var color = motion.Colors[i];
            var target = targets[i];
            motion.Colors[i] = new Color(
                color.R + (target.R - color.R) * blend,
                color.G + (target.G - color.G) * blend,
                color.B + (target.B - color.B) * blend,
                color.A + (target.A - color.A) * blend
            );
        }

        var size = shaderRect?.Size ?? Vector2.One;
        var colors = new Godot.Collections.Array();
        foreach (var c in motion.Colors)
        {
            colors.Add(c);
        }

        if (material is not null)
        {
            material.SetShaderParameter("params", new Vector4(motion.Level, motion.Turbulence, glow, scroll));
            material.SetShaderParameter("rect_size", size);
            material.SetShaderParameter("glow_margin", GlowMargin);
            material.SetShaderParameter("colors", colors);
        }
    }

    private static Color SampleStops(HealthGradient gradient, Percent percent)
    {
        if (gradient.Stops.Count == 0)
            return Colors.Black;

        var first = gradient.Stops[0];
        if (percent.Value <= first.Percent)
            return first.Color;

        for (int i = 0; i < gradient.Stops.Count - 1; i++)
        {
            var a = gradient.Stops[i];
            var b = gradient.Stops[i + 1];
            if (percent.Value <= b.Percent)
            {
                var span = b.Percent - a.Percent;
                var t = (percent.Value - a.Percent) / span;
                return a.Color.Lerp(b.Color, t);
            }
        }

        return gradient.Stops[^1].Color;
    }

    private struct VialMotion
    {
        public float Level;
        public float Turbulence;
        public float LastFill;
        public Color[] Colors;
        public bool Settled;

        public VialMotion()
        {
            Level = 0.0f;
            Turbulence = 0.0f;
            LastFill = 0.0f;
            Colors = new Color[GradientSamples];
            Settled = false;
        }
    }
}
