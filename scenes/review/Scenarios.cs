
namespace Rhythm;

/// <summary>One note in a scenario: when it appears, which column, and its quantization.</summary>
public record ScenarioNote(double Beat, uint Column, uint Quant, double? LengthBeats, bool Roll);

/// <summary>One mine in a scenario: when and where it appears.</summary>
public record ScenarioMine(double Beat, uint Column);

/// <summary>A scripted action in the demo timeline: advancing the hold state, vanishing notes, etc.</summary>
public abstract record ScriptAction
{
    public sealed record Hold(int Index, HoldVisualState State) : ScriptAction;
    public sealed record Fade(int Index) : ScriptAction;
    public sealed record Vanish(int Index) : ScriptAction;
    public sealed record Press(uint Column, bool Held) : ScriptAction;
    public sealed record ExplodeMine(int Index) : ScriptAction;
}

/// <summary>One animation scenario: plain data describing every rendering behavior.</summary>
public record Scenario(
    string Name,
    IReadOnlyList<ScenarioNote> Notes,
    IReadOnlyList<ScenarioMine> Mines,
    IReadOnlyList<(double Beat, ScriptAction Action)> Script,
    IReadOnlyList<(double Beat, double Bpm)> Bpms,
    IReadOnlyList<(double Beat, double Seconds)> Stops
);

/// <summary>Catalog of all animation scenarios for visual testing and review.</summary>
public static class Scenarios
{
    /// <summary>Gets all scenario names in order.</summary>
    public static IReadOnlyList<string> Names() =>
        Matrix().Select(s => s.Name).ToList();

    /// <summary>Gets all scenarios in order.</summary>
    public static IReadOnlyList<Scenario> Matrix()
    {
        const uint Q4 = 4, Q8 = 8, Q12 = 12, Q16 = 16, Q24 = 24, Q32 = 32, Q48 = 48, Q64 = 64;
        var quants = new[] { Q4, Q8, Q12, Q16, Q24, Q32, Q48, Q64 };

        var scenarios = new List<Scenario>();

        // Single notes at various quantizations
        foreach (var quant in quants)
        {
            scenarios.Add(new(
                $"single_quant_{quant}",
                [new(0.0, 1, quant, null, false)],
                [],
                [],
                [],
                []
            ));
        }

        // Holds at various quantizations and lengths
        foreach (var quant in quants)
        {
            foreach (var (label, length) in new[] {
                ("half_beat", 0.5),
                ("one_beat", 1.0),
                ("two_and_a_half_beats", 2.5)
            })
            {
                scenarios.Add(new(
                    $"hold_quant_{quant}_{label}",
                    [new(0.0, 1, quant, length, false)],
                    [],
                    [],
                    [],
                    []
                ));
            }
        }

        scenarios.Add(new(
            "hold_held_to_ok",
            [new(0.0, 1, 4, 2.0, false)],
            [],
            [
                (0.0, new ScriptAction.Press(1, true)),
                (0.0, new ScriptAction.Hold(0, HoldVisualState.Held)),
                (2.0, new ScriptAction.Hold(0, HoldVisualState.Ok)),
                (2.0, new ScriptAction.Fade(0)),
                (2.0, new ScriptAction.Press(1, false))
            ],
            [],
            []
        ));

        scenarios.Add(new(
            "hold_released_and_regrabbed",
            [new(0.0, 1, 4, 3.0, false)],
            [],
            [
                (0.0, new ScriptAction.Press(1, true)),
                (0.0, new ScriptAction.Hold(0, HoldVisualState.Held)),
                (1.0, new ScriptAction.Press(1, false)),
                (1.0, new ScriptAction.Hold(0, HoldVisualState.Released)),
                (1.75, new ScriptAction.Press(1, true)),
                (1.75, new ScriptAction.Hold(0, HoldVisualState.Held)),
                (3.0, new ScriptAction.Hold(0, HoldVisualState.Ok)),
                (3.0, new ScriptAction.Fade(0)),
                (3.0, new ScriptAction.Press(1, false))
            ],
            [],
            []
        ));

        scenarios.Add(new(
            "hold_dropped_midway",
            [new(0.0, 1, 4, 3.0, false)],
            [],
            [
                (0.0, new ScriptAction.Press(1, true)),
                (0.0, new ScriptAction.Hold(0, HoldVisualState.Held)),
                (1.0, new ScriptAction.Press(1, false)),
                (1.0, new ScriptAction.Hold(0, HoldVisualState.Released)),
                (1.5, new ScriptAction.Hold(0, HoldVisualState.Dropped))
            ],
            [],
            []
        ));

        scenarios.Add(new(
            "hold_head_missed",
            [new(0.0, 1, 4, 2.0, false)],
            [],
            [(0.5, new ScriptAction.Hold(0, HoldVisualState.Dropped))],
            [],
            []
        ));

        scenarios.Add(new(
            "roll_two_beats",
            [new(0.0, 1, 4, 2.0, true)],
            [],
            [],
            [],
            []
        ));

        scenarios.Add(new(
            "roll_held_to_ok",
            [new(0.0, 1, 4, 2.0, true)],
            [],
            [
                (0.0, new ScriptAction.Press(1, true)),
                (0.0, new ScriptAction.Hold(0, HoldVisualState.Held)),
                (2.0, new ScriptAction.Hold(0, HoldVisualState.Ok)),
                (2.0, new ScriptAction.Fade(0)),
                (2.0, new ScriptAction.Press(1, false))
            ],
            [],
            []
        ));

        scenarios.Add(new(
            "hold_chain_one_column",
            [
                new(0.0, 1, 4, 0.5, false),
                new(1.0, 1, 4, 0.5, false),
                new(2.0, 1, 4, 0.5, false)
            ],
            [],
            [],
            [],
            []
        ));

        scenarios.Add(new(
            "hold_staircase",
            [
                new(0.0, 0, 4, 1.0, false),
                new(0.5, 1, 8, 1.0, false),
                new(1.0, 2, 4, 1.0, false),
                new(1.5, 3, 8, 1.0, false)
            ],
            [],
            [],
            [],
            []
        ));

        scenarios.Add(new(
            "jump_hold",
            [
                new(0.0, 1, 4, 2.0, false),
                new(0.0, 2, 4, 2.0, false)
            ],
            [],
            [],
            [],
            []
        ));

        scenarios.Add(new(
            "tap_vanish_at_receptor",
            [new(0.0, 1, 4, null, false)],
            [],
            [
                (0.0, new ScriptAction.Press(1, true)),
                (0.0, new ScriptAction.Vanish(0)),
                (0.4, new ScriptAction.Press(1, false))
            ],
            [],
            []
        ));

        scenarios.Add(new(
            "jump",
            [
                new(0.0, 0, 4, null, false),
                new(0.0, 3, 4, null, false)
            ],
            [],
            [],
            [],
            []
        ));

        scenarios.Add(new(
            "every_column",
            [
                new(0.0, 0, 4, null, false),
                new(1.0, 1, 4, null, false),
                new(2.0, 2, 4, null, false),
                new(3.0, 3, 4, null, false)
            ],
            [],
            [],
            [],
            []
        ));

        scenarios.Add(new(
            "stream_16ths",
            Enumerable.Range(0, 16)
                .Select(i =>
                {
                    var quant = new[] { 4u, 16u, 8u, 16u }[i % 4];
                    return new ScenarioNote(i * 0.25, (uint)(i % 4), quant, null, false);
                })
                .ToList(),
            [],
            [],
            [],
            []
        ));

        scenarios.Add(new(
            "mine",
            [],
            [new(0.0, 1)],
            [],
            [],
            []
        ));

        scenarios.Add(new(
            "mine_row",
            [],
            Enumerable.Range(0, 4)
                .Select(column => new ScenarioMine(0.0, (uint)column))
                .ToList(),
            [],
            [],
            []
        ));

        scenarios.Add(new(
            "mine_exploding",
            [],
            [new(0.0, 1)],
            [
                (-0.5, new ScriptAction.Press(1, true)),
                (0.0, new ScriptAction.ExplodeMine(0)),
                (0.5, new ScriptAction.Press(1, false))
            ],
            [],
            []
        ));

        scenarios.Add(new(
            "receptors_idle",
            [],
            [],
            [
                (1.0, new ScriptAction.Press(1, true)),
                (2.0, new ScriptAction.Press(1, false)),
                (3.0, new ScriptAction.Press(2, true)),
                (4.0, new ScriptAction.Press(2, false))
            ],
            [],
            []
        ));

        scenarios.Add(new(
            "stream_bpm_change",
            Enumerable.Range(0, 8)
                .Select(i => new ScenarioNote(i, (uint)(i % 4), 4, null, false))
                .ToList(),
            [],
            [],
            [(0.0, 125.0), (4.0, 250.0)],
            []
        ));

        scenarios.Add(new(
            "stream_stop",
            Enumerable.Range(0, 8)
                .Select(i => new ScenarioNote(i, (uint)(i % 4), 4, null, false))
                .ToList(),
            [],
            [],
            [],
            [(4.0, 1.0)]
        ));

        return scenarios;
    }
}
