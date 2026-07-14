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
        t = Mathf.Clamp(t, 0.0f, 1.0f);

        var s = t;
        for (int i = 0; i < 8; i++)
        {
            var error = CubicBezierAxis(x1, x2, s) - t;
            if (Mathf.Abs(error) < 1e-5f)
            {
                return CubicBezierAxis(y1, y2, s);
            }
            var derivative = CubicBezierDerivative(x1, x2, s);
            if (Mathf.Abs(derivative) < 1e-6f)
            {
                break;
            }
            s = Mathf.Clamp(s - error / derivative, 0.0f, 1.0f);
        }

        var low = 0.0f;
        var high = 1.0f;
        for (int i = 0; i < 24; i++)
        {
            s = (low + high) / 2.0f;
            if (CubicBezierAxis(x1, x2, s) < t)
                low = s;
            else
                high = s;
        }
        return CubicBezierAxis(y1, y2, s);
    }

    private static float CubicBezierAxis(float a, float b, float s)
    {
        var oneMinusS = 1.0f - s;
        return 3.0f * oneMinusS * oneMinusS * s * a + 3.0f * oneMinusS * s * s * b + s * s * s;
    }

    private static float CubicBezierDerivative(float a, float b, float s)
    {
        var oneMinusS = 1.0f - s;
        return 3.0f * oneMinusS * oneMinusS * a + 6.0f * oneMinusS * s * (b - a) + 3.0f * s * s * (1.0f - b);
    }
}
