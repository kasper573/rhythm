using Godot;
using Rhythm.Core;

namespace Rhythm;

[Tool]
[GlobalClass]
public partial class RhythmCycle : Resource
{
    private const float StrikeAttack = 0.06f;

    [Export(PropertyHint.Range, "0.01,20")] public double Speed { get; set; } = 1;
    [Export] public Vector4 Easing { get; set; } = new(0.7f, 0, 1, 0.3f);

    public float Progress(Beat beat)
    {
        var phase = Phase(beat);
        return Ease(phase);
    }

    public float Pulse(Beat beat)
    {
        var phase = Phase(beat);
        var p = System.Math.Abs(2 * phase - 1);
        return Ease(p);
    }

    public float Strike(Beat beat)
    {
        var phase = Phase(beat);
        var decay = 1 - StrikeAttack;

        if (phase >= decay)
        {
            return Ease((phase - decay) / StrikeAttack);
        }

        return Ease(1 - phase / decay);
    }

    private float Phase(Beat beat)
    {
        var v = (float)(beat.Value * Speed / 4.0);
        return (float)(((v % 1.0) + 1.0) % 1.0);
    }

    private float Ease(float t)
    {
        return CubicBezierEase(Easing.X, Easing.Y, Easing.Z, Easing.W, t);
    }

    private static float CubicBezierEase(float x1, float y1, float x2, float y2, float t)
    {
        t = Mathf.Clamp(t, 0, 1);

        float s = t;
        for (int i = 0; i < 8; i++)
        {
            var cs = CubicBezierAxis(x1, x2, s);
            var ds = CubicBezierDerivative(x1, x2, s);
            if (Mathf.Abs(ds) < 1e-6f) break;
            s -= (cs - t) / ds;
        }

        for (int i = 0; i < 24; i++)
        {
            var cs = CubicBezierAxis(x1, x2, s);
            if (Mathf.Abs(cs - t) < 1e-6f) break;

            if (cs < t)
                s = (s + 1.0f) / 2.0f;
            else
                s = s / 2.0f;
        }

        return CubicBezierAxis(y1, y2, s);
    }

    private static float CubicBezierAxis(float a, float b, float s)
    {
        var oneMinusS = 1 - s;
        return 3 * oneMinusS * oneMinusS * s * a + 3 * oneMinusS * s * s * (b - a) + s * s * s;
    }

    private static float CubicBezierDerivative(float a, float b, float s)
    {
        var oneMinusS = 1 - s;
        return 3 * oneMinusS * oneMinusS * a + 6 * oneMinusS * s * (b - a) + 3 * s * s * (1 - b);
    }
}
