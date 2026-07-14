using Godot;

namespace Rhythm;

/// <summary>
/// The shared visual frame: the design canvas, the clear color, the
/// interface palette, and the helpers that read the currently visible
/// region for layout that hugs the screen.
/// </summary>
public static class Screen
{
    /// <summary>
    /// The fixed logical design canvas. The project's <c>canvas_items</c>
    /// stretch with <c>expand</c> aspect keeps it fully visible and
    /// uniformly scaled; windows with a different aspect see extra canvas
    /// past it.
    /// </summary>
    public static readonly Vector2 Size = new(1280.0f, 720.0f);

    public static readonly Color ClearColor = new(0.04f, 0.04f, 0.07f);

    /// <summary>The interface palette every menu-like surface shares.</summary>
    public static readonly Color TitleColor = new(0.95f, 0.85f, 0.4f);
    public static readonly Color ActiveColor = Colors.White;
    public static readonly Color InactiveColor = new(0.45f, 0.45f, 0.55f);

    /// <summary>
    /// The canvas rect the window currently shows: the whole design canvas
    /// plus whatever extra the window's aspect reveals. Layout that hugs
    /// screen edges or centers on the screen derives from this every frame.
    /// </summary>
    public static Rect2 VisibleRect(Node node) =>
        node.GetViewport()?.GetVisibleRect() ?? new Rect2(Vector2.Zero, Size);

    /// <summary>
    /// A canvas blend factor compensated for the 2D pipeline blending in
    /// sRGB space: the game's look was designed on a linear-blending
    /// renderer where the same factor mixes brighter, so partial alphas are
    /// encoded with the sRGB exponent to restore the designed brightness
    /// (the grade shader's bloom does the same). Apply to the surviving side
    /// of a blend: a wash at <c>LinearBlend(0.25)</c>, a black fade covering
    /// at <c>1 - LinearBlend(1 - t)</c>.
    /// </summary>
    public static float LinearBlend(float factor) =>
        Mathf.Pow(Mathf.Clamp(factor, 0.0f, 1.0f), 1.0f / 2.2f);
}
