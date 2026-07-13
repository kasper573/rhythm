using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The one way game code polls the rebindable actions, backed by Godot's
/// <c>InputMap</c> state that <see cref="Apply"/> installs from the keymap.
/// </summary>
public static class Actions
{
    /// <summary>
    /// Installs the resolved bindings as Godot <c>InputMap</c> actions, so
    /// game code reads them through this class. Call after any change.
    /// </summary>
    public static void Apply(Keymap keymap, Keymap defaults)
    {
        var map = InputMap.Singleton;
        foreach (var action in GameActions.All)
        {
            var name = action.ActionName();
            if (map.HasAction(name))
            {
                map.ActionEraseEvents(name);
            }
            else
            {
                map.AddAction(name);
            }

            var key = OS.Singleton.FindKeycodeFromString(keymap.Key(action, defaults));
            map.ActionAddEvent(name, new InputEventKey { PhysicalKeycode = key });
        }
    }

    public static bool JustPressed(GameAction action) =>
        Input.Singleton.IsActionJustPressed(action.ActionName());

    public static bool Pressed(GameAction action) =>
        Input.Singleton.IsActionPressed(action.ActionName());

    public static bool JustReleased(GameAction action) =>
        Input.Singleton.IsActionJustReleased(action.ActionName());

    /// <summary>
    /// Whether any of the given players just pressed their variant of an
    /// action — the shared-space check for anything both players may drive.
    /// </summary>
    public static bool AnyJustPressed(IReadOnlyList<PlayerId> players, Func<PlayerId, GameAction> action) =>
        players.Any(player => JustPressed(action(player)));

    public static bool ShiftHeld() => Input.Singleton.IsPhysicalKeyPressed(Key.Shift);

    /// <summary>The physical key a name (<c>OS.GetKeycodeString</c>) denotes, or <c>Key.None</c>.</summary>
    public static Key KeyFromName(string name) => OS.Singleton.FindKeycodeFromString(name);

    /// <summary>The name of a physical key, for persistence and display.</summary>
    public static string KeyName(Key key) => OS.Singleton.GetKeycodeString(key);
}
