using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The shared navigation vocabulary: a press of a step action, re-fired
/// while held so lists scroll comfortably. Menus and menu-like scenes (the
/// wheel, options panels) consume Pulses instead of raw key state. Processes
/// before every scene (it sits first under the root), so a frame's pulses
/// are ready when consumers run; the root suspends it while a scene
/// transition runs.
/// </summary>
[GlobalClass]
public partial class NavInput : Node
{
    private const double RepeatDelaySeconds = 0.4;
    private const double RepeatIntervalSeconds = 0.09;

    private static readonly GameAction[] PulseActions =
    [
        GameAction.P1Left,
        GameAction.P1Down,
        GameAction.P1Up,
        GameAction.P1Right,
        GameAction.P2Left,
        GameAction.P2Down,
        GameAction.P2Up,
        GameAction.P2Right,
    ];

    private double[] held = new double[PulseActions.Length];
    private List<GameAction> pulses = [];
    private bool suspended;
    private static NavInput? instance;

    public IReadOnlyList<GameAction> Pulses => pulses;

    public bool Active => !suspended;

    public static NavInput Instance =>
        instance ?? throw new InvalidOperationException("NavInput autoload not in tree");

    public override void _EnterTree()
    {
        if (Engine.IsEditorHint())
        {
            return;
        }

        instance = this;
    }

    /// <summary>
    /// Suspends or resumes pulse emission; suspension also drops buffered
    /// pulses so a resumed consumer never replays stale ones.
    /// </summary>
    public void SetSuspended(bool suspend)
    {
        suspended = suspend;
        if (suspended)
        {
            pulses.Clear();
        }
    }

    /// <summary>
    /// Clears this frame's pulses — for focus handoffs where the pulse must
    /// not reach the mode that did not consume it.
    /// </summary>
    public void Clear()
    {
        pulses.Clear();
    }

    public override void _Process(double delta)
    {
        if (Engine.IsEditorHint())
        {
            return;
        }

        pulses.Clear();
        if (suspended)
        {
            return;
        }

        for (var slot = 0; slot < PulseActions.Length; slot++)
        {
            var action = PulseActions[slot];

            if (Actions.JustPressed(action))
            {
                held[slot] = 0.0;
                pulses.Add(action);
            }
            else if (Actions.Pressed(action))
            {
                var before = held[slot];
                held[slot] += delta;

                var repeatsBefore = System.Math.Floor((before - RepeatDelaySeconds) / RepeatIntervalSeconds);
                var repeatsAfter = System.Math.Floor((held[slot] - RepeatDelaySeconds) / RepeatIntervalSeconds);

                if (held[slot] >= RepeatDelaySeconds && repeatsAfter > repeatsBefore)
                {
                    pulses.Add(action);
                }
            }
            else
            {
                held[slot] = 0.0;
            }
        }
    }
}
