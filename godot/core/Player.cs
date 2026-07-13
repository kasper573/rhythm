namespace Rhythm.Core;

/// <summary>
/// One of the two player slots the machine offers. Every player-scoped
/// piece of state — settings, high scores, key bindings, a play session's
/// stage — is keyed by this.
/// </summary>
public enum PlayerId
{
    P1,
    P2,
}

/// <summary>
/// One value per player slot, for state that always exists for both
/// players regardless of how many are active.
/// </summary>
public struct PerPlayer<T>(T p1, T p2)
{
    public T P1 = p1;
    public T P2 = p2;

    public static PerPlayer<T> Uniform(T value) => new(value, value);

    public T this[PlayerId player]
    {
        readonly get => player == PlayerId.P1 ? P1 : P2;
        set
        {
            if (player == PlayerId.P1)
            {
                P1 = value;
            }
            else
            {
                P2 = value;
            }
        }
    }
}

/// <summary>
/// How the machine is being played: who is on it and which charts they
/// step. Selected on the mode select scene (kept on the <c>Game</c> root)
/// and read by every scene after it.
/// </summary>
public enum PlayMode
{
    Singles,
    Doubles,
    Versus,
}

public static class PlayModeExtensions
{
    /// <summary>The chart type this mode plays.</summary>
    public static StepsType StepsType(this PlayMode mode) =>
        mode == PlayMode.Doubles ? Rhythm.Core.StepsType.DanceDouble : Rhythm.Core.StepsType.DanceSingle;

    /// <summary>
    /// The active player slots: P1 alone plays singles and doubles (the
    /// doubles chart spans both pads), versus fields both.
    /// </summary>
    public static IReadOnlyList<PlayerId> Players(this PlayMode mode) =>
        mode == PlayMode.Versus ? TwoPlayers : OnePlayer;

    private static readonly PlayerId[] OnePlayer = [PlayerId.P1];
    private static readonly PlayerId[] TwoPlayers = [PlayerId.P1, PlayerId.P2];
}

/// <summary>
/// A step panel's direction, in the fixed Left/Down/Up/Right column order
/// every 4-panel pad uses.
/// </summary>
public enum StepDirection
{
    Left,
    Down,
    Up,
    Right,
}

public static class StepDirectionExtensions
{
    /// <summary>The direction of a pad-local column (<c>0..4</c>).</summary>
    public static StepDirection OfColumn(int column) =>
        column switch
        {
            0 => StepDirection.Left,
            1 => StepDirection.Down,
            2 => StepDirection.Up,
            _ => StepDirection.Right,
        };

    /// <summary>The pad-local column of this direction — <see cref="OfColumn"/>'s inverse.</summary>
    public static int Column(this StepDirection direction) => (int)direction;
}
