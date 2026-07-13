using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// One lane group on stage: a player's columns, centered on
/// <see cref="OriginX"/>, scrolling at that player's speed and drawn in
/// their skin at the field's arrow size. Coordinates are canvas-centered and
/// y-up, matching the lane's own 3D scene.
/// </summary>
public sealed record FieldLayout(PlayerId Player, float OriginX, uint Columns, NoteSpeed Speed, float ArrowSize)
{
    public float Spacing => ArrowSize * NoteField.ColumnSpacingRatio;

    public float Width => Columns * Spacing;

    public float ColumnX(uint column) => OriginX + ((column - ((Columns - 1.0f) / 2.0f)) * Spacing);

    /// <summary>
    /// The key that steps <paramref name="column"/>: a field wider than one
    /// pad continues onto the second player's pad (doubles), otherwise every
    /// column belongs to the field's owner.
    /// </summary>
    public GameAction StepAction(uint column)
    {
        var side = column < NoteField.PadColumns ? Player : PlayerId.P2;
        return GameActions.Step(side, StepDirectionExtensions.OfColumn((int)(column % NoteField.PadColumns)));
    }
}

/// <summary>
/// Paces every note-field animation: <see cref="Visible"/> is the current
/// moment on the drawn timeline and <see cref="Timing"/> converts it to
/// beats. The stage's owner advances <see cref="Visible"/> and anchors
/// <see cref="TargetY"/> (canvas-centered, y-up).
/// </summary>
public sealed record FieldClock(Seconds Visible, StepfileTiming Timing, float TargetY)
{
    public Beat Beat => Timing.BeatAtSeconds(Visible);

    internal NoteScroll Scroll() => new(Visible, Beat, TargetY);
}

/// <summary>
/// A per-frame snapshot placing notes on screen: constant speed spaces notes
/// by their seconds, dynamic by their beats — one arrow height per beat at
/// multiplier 1, whatever the field's size.
/// </summary>
internal readonly record struct NoteScroll(Seconds Now, Beat NowBeat, float TargetY)
{
    public float YAt(FieldLayout layout, Seconds time, Beat beat)
    {
        var arrowsUntil = layout.Speed switch
        {
            NoteSpeed.Constant scrollBpm => (time - Now).Value * scrollBpm.Value / 60.0,
            NoteSpeed.Dynamic multiplier => (beat.Value - NowBeat.Value) * multiplier.Value,
            _ => 0.0,
        };
        return TargetY - (float)(arrowsUntil * layout.ArrowSize);
    }
}

/// <summary>Render state of a hold.</summary>
public enum HoldVisualState
{
    /// <summary>Not yet stepped: scrolls by whole, inactive textures.</summary>
    Pending,

    /// <summary>Stepped and satisfied: head pinned at the receptor, active textures.</summary>
    Held,

    /// <summary>Stepped but the panel is up; still alive: pinned, inactive textures.</summary>
    Released,

    /// <summary>Kept to the end: body and cap disappear.</summary>
    Ok,

    /// <summary>Dropped, or the head was missed: dimmed, scrolls away.</summary>
    Dropped,
}

public static class HoldVisualStates
{
    public static bool Pinned(this HoldVisualState state) =>
        state is HoldVisualState.Held or HoldVisualState.Released;

    public static bool Active(this HoldVisualState state) => state == HoldVisualState.Held;
}

public sealed record NoteSpawn(Seconds Time, Beat Beat, uint Column, uint Quant, NoteTail? Tail);

/// <summary>A hold or roll note's tail.</summary>
public sealed record NoteTail(Seconds Time, Beat Beat, bool Roll);

/// <summary>A spawned note's handle within its field.</summary>
public readonly record struct NoteIndex(int Value);

/// <summary>A spawned mine's handle within its field.</summary>
public readonly record struct MineIndex(int Value);

/// <summary>Field-wide constants and arrow-size fitting.</summary>
public static class NoteField
{
    /// <summary>
    /// The receptor row's y where no window overrides it (headless
    /// renderers), in the lane world's y-up centered coordinates.
    /// </summary>
    public const float TargetY = 260.0f;

    /// <summary>
    /// Columns sit slightly further apart than the arrows are wide, keeping
    /// the classic gap whatever size a field is scaled to.
    /// </summary>
    public const float ColumnSpacingRatio = 100.0f / 88.0f;

    /// <summary>Columns on one physical pad; wider fields span several pads.</summary>
    public const uint PadColumns = 4;

    /// <summary>
    /// The largest arrow size — capped at <paramref name="maxSize"/> — whose
    /// columns fit <paramref name="spacingUnits"/> column spacings into
    /// <paramref name="available"/> canvas width.
    /// </summary>
    public static float FittedArrowSize(float spacingUnits, float available, float maxSize) =>
        Math.Min(available / spacingUnits / ColumnSpacingRatio, maxSize);

    /// <summary>
    /// The configured arrow-size cap — a screen pixel budget — as canvas
    /// units: <paramref name="pixelsPerUnit"/> grows with the window, so the
    /// canvas-unit cap shrinks to keep arrows at most the configured pixel
    /// size on screen.
    /// </summary>
    public static float MaxArrowSize(float maxArrowSize, float pixelsPerUnit) =>
        maxArrowSize / Math.Max(pixelsPerUnit, float.Epsilon);
}
