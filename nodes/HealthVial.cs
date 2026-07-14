using Godot;
using Rhythm.Core;

namespace Rhythm;

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
[Tool]
[GlobalClass]
public partial class HealthVial : Control
{
    private const float GlowMargin = 32.0f;
    private const float LevelTau = 0.25f;
    private const float TurbulenceTau = 0.9f;
    private const float ColorTau = 0.35f;
    private const int GradientSamples = 16;

    /// <summary>The vial's designed on-screen width and the pixel range its
    /// on-screen width is clamped to, so it stays a readable sliver whatever
    /// the window scale (mirrors the reference's VIAL_WIDTH/MIN/MAX).</summary>
    public const float NaturalWidth = 50.0f;
    public const float MinScreenWidth = 20.0f;
    public const float MaxScreenWidth = 50.0f;

    private VialSide sideType = VialSide.Left;
    private float fill;
    private Beat beat;
    private VialMotion motion = new();
    private ShaderMaterial? material;
    private ColorRect? shaderRect;

    [ExportGroup("Vial")]
    [Export]
    public VialSide Side
    {
        get => sideType;
        set { sideType = value; Build(); }
    }

    /// <summary>0..=1 of the vial's capacity.</summary>
    public void SetFill(float value) => fill = value;

    /// <summary>
    /// The musical beat the glow and liquid pulse on; hold it still and
    /// the vial rests.
    /// </summary>
    public void SetBeat(Beat value) => beat = value;

    /// <summary>
    /// Places the vial's glass rect in canvas units. The owner drives this
    /// from the visible screen every frame so the vial keeps a fixed screen
    /// inset and a clamped on-screen width whatever the window size — nothing
    /// about its edges changes with the window, only its width clamps.
    /// </summary>
    public void Place(Rect2 rect)
    {
        Position = rect.Position;
        Size = rect.Size;
    }

    public override void _Ready()
    {
        Build();
    }

    public override void _Process(double delta)
    {
        if (Engine.IsEditorHint())
        {
            return;
        }

        var deltaF = (float)delta;
        var config = Config.Current;
        var healthBar = config?.HealthBar;
        if (healthBar is null || healthBar.Glow is null || healthBar.Liquid is null)
            return;

        var glow = healthBar.Glow.Pulse(beat);
        var scroll = Mathf.PosMod(healthBar.Liquid.Progress(beat) * 2.0f, 2.0f);

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

        if (material is not null)
        {
            material.SetShaderParameter("params", new Vector4(motion.Level, motion.Turbulence, glow, scroll));
            material.SetShaderParameter("rect_size", size);
            material.SetShaderParameter("glow_margin", GlowMargin);
            material.SetShaderParameter("colors", motion.Colors);
        }
    }

    private void Build()
    {
        if (!IsInsideTree())
        {
            return;
        }

        foreach (var child in GetChildren())
        {
            child.QueueFree();
        }

        SetAnchorsPreset(Control.LayoutPreset.TopLeft);
        MouseFilter = MouseFilterEnum.Ignore;

        var shader = GD.Load<Shader>("res://nodes/HealthVial.gdshader");
        var shaderMat = new ShaderMaterial { Shader = shader };

        var rect = new ColorRect
        {
            Material = shaderMat,
            MouseFilter = MouseFilterEnum.Ignore
        };
        rect.SetAnchorsAndOffsetsPreset(Control.LayoutPreset.FullRect);
        rect.SetOffset(Godot.Side.Left, -GlowMargin);
        rect.SetOffset(Godot.Side.Top, -GlowMargin);
        rect.SetOffset(Godot.Side.Right, GlowMargin);
        rect.SetOffset(Godot.Side.Bottom, GlowMargin);

        AddChild(rect);
        material = shaderMat;
        shaderRect = rect;
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
