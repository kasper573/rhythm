namespace Rhythm.Core;

/// <summary>A position or span on the audio clock, in seconds.</summary>
public readonly record struct Seconds(double Value) : IComparable<Seconds>
{
    public static readonly Seconds Zero = new(0.0);

    public static Seconds FromMillis(double millis) => new(millis / 1000.0);

    public double ToMillis() => Value * 1000.0;

    public Seconds Abs() => new(Math.Abs(Value));

    public Seconds Max(Seconds other) => new(Math.Max(Value, other.Value));

    public static Seconds operator +(Seconds a, Seconds b) => new(a.Value + b.Value);

    public static Seconds operator -(Seconds a, Seconds b) => new(a.Value - b.Value);

    public static Seconds operator -(Seconds a) => new(-a.Value);

    public static Seconds operator *(Seconds a, double factor) => new(a.Value * factor);

    public static double operator /(Seconds a, Seconds b) => a.Value / b.Value;

    public static bool operator <(Seconds a, Seconds b) => a.Value < b.Value;

    public static bool operator >(Seconds a, Seconds b) => a.Value > b.Value;

    public static bool operator <=(Seconds a, Seconds b) => a.Value <= b.Value;

    public static bool operator >=(Seconds a, Seconds b) => a.Value >= b.Value;

    public int CompareTo(Seconds other) => Value.CompareTo(other.Value);

    public override string ToString() => $"{Value:0.000}s";
}

/// <summary>A whole-millisecond calibration offset.</summary>
public readonly record struct Millis(long Value) : IComparable<Millis>
{
    public Seconds ToSeconds() => new(Value / 1000.0);

    public static Millis operator +(Millis a, Millis b) => new(a.Value + b.Value);

    public static bool operator <(Millis a, Millis b) => a.Value < b.Value;

    public static bool operator >(Millis a, Millis b) => a.Value > b.Value;

    public static bool operator <=(Millis a, Millis b) => a.Value <= b.Value;

    public static bool operator >=(Millis a, Millis b) => a.Value >= b.Value;

    public int CompareTo(Millis other) => Value.CompareTo(other.Value);

    public override string ToString() => $"{Value}ms";
}

/// <summary>A position on the musical beat timeline.</summary>
public readonly record struct Beat(double Value) : IComparable<Beat>
{
    public static Beat operator +(Beat a, Beat b) => new(a.Value + b.Value);

    public static Beat operator -(Beat a, Beat b) => new(a.Value - b.Value);

    public static bool operator <(Beat a, Beat b) => a.Value < b.Value;

    public static bool operator >(Beat a, Beat b) => a.Value > b.Value;

    public static bool operator <=(Beat a, Beat b) => a.Value <= b.Value;

    public static bool operator >=(Beat a, Beat b) => a.Value >= b.Value;

    public int CompareTo(Beat other) => Value.CompareTo(other.Value);

    public override string ToString() => $"beat {Value:0.000}";
}

/// <summary>A tempo in beats per minute.</summary>
public readonly record struct Bpm(double Value)
{
    public override string ToString() => $"{Value:0}";
}

/// <summary>A percentage in <c>0..=100</c> space — never a <c>0..=1</c> fraction.</summary>
public readonly record struct Percent(float Value) : IComparable<Percent>
{
    public static Percent operator +(Percent a, Percent b) => new(a.Value + b.Value);

    public static Percent operator -(Percent a, Percent b) => new(a.Value - b.Value);

    public static bool operator <(Percent a, Percent b) => a.Value < b.Value;

    public static bool operator >(Percent a, Percent b) => a.Value > b.Value;

    public static bool operator <=(Percent a, Percent b) => a.Value <= b.Value;

    public static bool operator >=(Percent a, Percent b) => a.Value >= b.Value;

    public int CompareTo(Percent other) => Value.CompareTo(other.Value);

    public override string ToString() => $"{Value:0.0}%";
}
