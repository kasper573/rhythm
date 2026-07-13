using System.Text.Json;
using System.Text.Json.Serialization;

namespace Rhythm.Core;

/// <summary>
/// Every key the machine responds to, as one flat list: one set of player
/// actions per player slot, plus the machine-wide tuning toggles. Menus
/// listen to P1 alone; shared spaces (the wheel, the player options modal)
/// listen to every active player.
/// </summary>
public enum GameAction
{
    P1Left,
    P1Down,
    P1Up,
    P1Right,
    P1Select,
    P1Cancel,
    P2Left,
    P2Down,
    P2Up,
    P2Right,
    P2Select,
    P2Cancel,
    ToggleAutoSync,
    ToggleTickAudio,
    DecreaseAudioLatency,
    IncreaseAudioLatency,
    DecreaseVisualDelay,
    IncreaseVisualDelay,
    DecreaseMachineOffset,
    IncreaseMachineOffset,
    ToggleFps,
}

public static class GameActions
{
    public static readonly IReadOnlyList<GameAction> All = Enum.GetValues<GameAction>();

    /// <summary>Each player's step actions in <see cref="StepDirection"/> column order.</summary>
    private static readonly PerPlayer<GameAction[]> StepActions = new(
        [GameAction.P1Left, GameAction.P1Down, GameAction.P1Up, GameAction.P1Right],
        [GameAction.P2Left, GameAction.P2Down, GameAction.P2Up, GameAction.P2Right]);

    /// <summary>The human-facing name shown wherever the binding is listed.</summary>
    public static string Label(this GameAction action) =>
        action switch
        {
            GameAction.P1Left => "P1 Step left",
            GameAction.P1Down => "P1 Step down",
            GameAction.P1Up => "P1 Step up",
            GameAction.P1Right => "P1 Step right",
            GameAction.P1Select => "P1 Select",
            GameAction.P1Cancel => "P1 Cancel",
            GameAction.P2Left => "P2 Step left",
            GameAction.P2Down => "P2 Step down",
            GameAction.P2Up => "P2 Step up",
            GameAction.P2Right => "P2 Step right",
            GameAction.P2Select => "P2 Select",
            GameAction.P2Cancel => "P2 Cancel",
            GameAction.ToggleAutoSync => "Toggle AutoSync",
            GameAction.ToggleTickAudio => "Toggle tick audio",
            GameAction.DecreaseAudioLatency => "Decrease audio latency",
            GameAction.IncreaseAudioLatency => "Increase audio latency",
            GameAction.DecreaseVisualDelay => "Decrease visual delay",
            GameAction.IncreaseVisualDelay => "Increase visual delay",
            GameAction.DecreaseMachineOffset => "Decrease machine offset",
            GameAction.IncreaseMachineOffset => "Increase machine offset",
            GameAction.ToggleFps => "Toggle FPS",
            _ => action.ToString(),
        };

    /// <summary>The Godot <c>InputMap</c> action name this maps to.</summary>
    public static string ActionName(this GameAction action) => action.ToString();

    public static GameAction Step(PlayerId player, StepDirection direction) =>
        StepActions[player][direction.Column()];

    /// <summary>
    /// The <c>(player, direction)</c> a step action belongs to; <c>null</c>
    /// for everything that is not a step.
    /// </summary>
    public static (PlayerId Player, StepDirection Direction)? AsStep(this GameAction action)
    {
        foreach (var player in new[] { PlayerId.P1, PlayerId.P2 })
        {
            var column = Array.IndexOf(StepActions[player], action);
            if (column >= 0)
            {
                return (player, StepDirectionExtensions.OfColumn(column));
            }
        }

        return null;
    }

    public static GameAction Select(PlayerId player) =>
        player == PlayerId.P1 ? GameAction.P1Select : GameAction.P2Select;

    public static GameAction Cancel(PlayerId player) =>
        player == PlayerId.P1 ? GameAction.P1Cancel : GameAction.P2Cancel;
}

/// <summary>
/// A set of key bindings, each a Godot keycode name (<c>OS.GetKeycodeString</c>),
/// e.g. <c>"A"</c>, <c>"Up"</c>, <c>"Escape"</c>, <c>"F5"</c>. The machine
/// settings hold the players' overrides; actions without one resolve through
/// the config's default keymap, which binds everything.
/// </summary>
[JsonConverter(typeof(KeymapJsonConverter))]
public sealed class Keymap
{
    private readonly Dictionary<GameAction, string> bindings;

    public Keymap()
    {
        bindings = [];
    }

    public Keymap(IReadOnlyDictionary<GameAction, string> bindings)
    {
        this.bindings = new Dictionary<GameAction, string>(bindings);
    }

    public IReadOnlyDictionary<GameAction, string> Bindings => bindings;

    public string Key(GameAction action, Keymap defaults)
    {
        var binding = Binding(action);
        if (binding is not null)
        {
            return binding;
        }

        var defaultBinding = defaults.Binding(action);
        if (defaultBinding is not null)
        {
            return defaultBinding;
        }

        throw new InvalidOperationException($"default keymap must bind {action}");
    }

    public string? Binding(GameAction action) =>
        bindings.TryGetValue(action, out var key) ? key : null;

    public void Set(GameAction action, string key) => bindings[action] = key;

    public void Reset(GameAction action) => bindings.Remove(action);

    public Keymap Clone() => new(bindings);

    public bool Equals(Keymap? other) =>
        other is not null && bindings.Count == other.bindings.Count &&
        bindings.All(pair => other.bindings.TryGetValue(pair.Key, out var value) && value == pair.Value);

    public override bool Equals(object? obj) => Equals(obj as Keymap);

    public override int GetHashCode() => bindings.Count;
}

/// <summary>Persists a keymap as a plain <c>{action-name: key-name}</c> object.</summary>
internal sealed class KeymapJsonConverter : JsonConverter<Keymap>
{
    public override Keymap Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        var raw = JsonSerializer.Deserialize<Dictionary<string, string>>(ref reader, options) ?? [];
        var bindings = new Dictionary<GameAction, string>();
        foreach (var (name, key) in raw)
        {
            if (Enum.TryParse<GameAction>(name, out var action))
            {
                bindings[action] = key;
            }
        }

        return new Keymap(bindings);
    }

    public override void Write(Utf8JsonWriter writer, Keymap value, JsonSerializerOptions options)
    {
        writer.WriteStartObject();
        foreach (var (action, key) in value.Bindings)
        {
            writer.WriteString(action.ToString(), key);
        }

        writer.WriteEndObject();
    }
}
