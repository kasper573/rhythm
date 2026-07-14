using Godot;
using Rhythm.Core;
using static Godot.Mathf;

namespace Rhythm;

/// <summary>
/// Touch controls, synthesized as the actions the game already understands.
/// Every touch acts independently: a swipe presses its direction's step the
/// moment it crosses the threshold and releases it when the finger lifts —
/// so simultaneous swipes play jumps, and a swipe kept down sustains holds.
/// A short stationary tap is Select; exactly two touches held stationary for
/// a second are Cancel.
/// </summary>
[GlobalClass]
public partial class TouchSteps : Node
{
    private Dictionary<long, TrackedTouch> touches = [];
    private double cancelHold;
    private List<(GameAction Action, double Remaining)> releases = [];
    private static TouchSteps? instance;

    public static TouchSteps Instance =>
        instance ?? throw new InvalidOperationException("TouchSteps autoload not in tree");

    private const float SwipeMin = 30.0f;
    private const double CancelHoldSeconds = 1.0;
    private const double TapPulseSeconds = 0.06;

    public override void _EnterTree()
    {
        if (Engine.IsEditorHint())
        {
            return;
        }

        instance = this;
    }

    public override void _Input(InputEvent @event)
    {
        if (Engine.IsEditorHint())
        {
            return;
        }

        if (@event is InputEventScreenTouch touch)
        {
            var index = touch.Index;
            if (touch.Pressed)
            {
                touches[index] = new TrackedTouch
                {
                    Start = touch.Position,
                    Position = touch.Position,
                    Arrow = null,
                    Consumed = false,
                };
            }
            else if (touches.Remove(index, out var tracked))
            {
                if (tracked.Arrow is { } action)
                {
                    Release(action);
                }
                else if (!tracked.Consumed)
                {
                    Pulse(GameActions.Select(PlayerId.P1));
                }
            }

            return;
        }

        if (@event is InputEventScreenDrag drag)
        {
            var index = drag.Index;
            var position = drag.Position;

            if (!touches.TryGetValue(index, out var tracked))
            {
                return;
            }

            tracked.Position = position;

            if (tracked.Arrow is not null)
            {
                return;
            }

            if (SwipeArrow(tracked) is { } direction)
            {
                var action = GameActions.Step(PlayerId.P1, direction);
                tracked.Arrow = action;
                Press(action);
            }
        }
    }

    public override void _Process(double delta)
    {
        if (Engine.IsEditorHint())
        {
            return;
        }

        var due = new List<GameAction>();
        for (int i = releases.Count - 1; i >= 0; i--)
        {
            var (action, remaining) = releases[i];
            remaining -= delta;
            if (remaining <= 0.0)
            {
                due.Add(action);
                releases.RemoveAt(i);
            }
            else
            {
                releases[i] = (action, remaining);
            }
        }

        foreach (var action in due)
        {
            Release(action);
        }

        var stationary = 0;
        foreach (var touch in touches.Values)
        {
            if (touch.Arrow is null && !touch.Consumed)
            {
                stationary++;
            }
        }

        if (stationary == 2 && touches.Count == 2)
        {
            cancelHold += delta;
            if (cancelHold >= CancelHoldSeconds)
            {
                cancelHold = 0.0;
                foreach (var touch in touches.Values)
                {
                    touch.Consumed = true;
                }

                Pulse(GameActions.Cancel(PlayerId.P1));
            }
        }
        else
        {
            cancelHold = 0.0;
        }
    }

    private static void Press(GameAction action)
    {
        Input.ActionPress(action.ActionName());
    }

    private static void Release(GameAction action)
    {
        Input.ActionRelease(action.ActionName());
    }

    private void Pulse(GameAction action)
    {
        Press(action);
        releases.Add((action, TapPulseSeconds));
    }

    private static StepDirection? SwipeArrow(TrackedTouch touch)
    {
        var delta = touch.Position - touch.Start;
        var absDeltaX = Abs(delta.X);
        var absDeltaY = Abs(delta.Y);

        if (Max(absDeltaX, absDeltaY) < SwipeMin)
        {
            return null;
        }

        if (absDeltaX > absDeltaY)
        {
            return delta.X > 0.0f ? StepDirection.Right : StepDirection.Left;
        }

        return delta.Y > 0.0f ? StepDirection.Down : StepDirection.Up;
    }

    private sealed class TrackedTouch
    {
        public required Vector2 Start { get; init; }
        public required Vector2 Position { get; set; }
        public required GameAction? Arrow { get; set; }
        public required bool Consumed { get; set; }
    }
}
