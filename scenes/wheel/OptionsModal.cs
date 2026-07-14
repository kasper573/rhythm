using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The player options modal: a shared space overlay for editing each player's options.
/// The options table shows Speed Type, Speed Modifier, Note Skin, Perspective, Grade Layer, Grade Position.
/// Each active player gets a column, with a preview on their flank showing a mocked chart.
/// </summary>
public class OptionsModal
{
    private const float TransitionSeconds = 0.25f;
    private const float NameWidth = 220.0f;
    private const float ValueWidth = 200.0f;
    private const float PreviewBand = 720.0f;

    private Control root = null!;
    private ColorRect background = null!;
    private Control content = null!;
    private VBoxContainer column = null!;
    private List<Label> texts = [];
    private float t = 0.0f;
    private float dir = 1.0f;
    private List<PlayerId> players = [];
    private List<PanelState> panels = [];
    private List<Label> rowNames = [];
    private List<ValueCell> values = [];
    private List<Preview> previews = [];
    private PreviewState? state;

    private class PanelState
    {
        public required PlayerId Player { get; init; }
        public required int ActiveRow { get; set; }
    }

    private class ValueCell
    {
        public required PlayerId Player { get; init; }
        public required int Row { get; init; }
        public required Label Label { get; init; }
    }

    private class Preview
    {
        public required PlayerId Player { get; init; }
        public required Control Flank { get; init; }
        public SubViewport? Viewport { get; set; }
        public StepfilePlayer? Engine { get; set; }
    }

    private class PreviewState
    {
        public required List<Row> Rows { get; init; }
        public required StepfileTiming Timing { get; init; }
        public required Seconds LastVisible { get; set; }
        public required bool Rebuild { get; set; }
    }

    public static OptionsModal Open(Control host, List<PlayerId> playersList)
    {
        var versus = playersList.Count > 1;
        var modal = new OptionsModal { players = playersList };

        modal.root = new Control { ZIndex = 300 };
        modal.root.SetAnchorsAndOffsetsPreset(Control.LayoutPreset.FullRect);
        host.AddChild(modal.root);

        // Background
        modal.background = new ColorRect();
        modal.root.AddChild(modal.background);

        // Content container
        modal.content = new Control();
        var stripeRow = new HBoxContainer();
        stripeRow.SetAnchorsAndOffsetsPreset(Control.LayoutPreset.FullRect);
        stripeRow.AddThemeConstantOverride("separation", 0);

        // Left flank
        var left = new Control { SizeFlagsHorizontal = Control.SizeFlags.ExpandFill };
        stripeRow.AddChild(left);
        if (playersList.Count > 0)
        {
            modal.previews.Add(new Preview
            {
                Player = playersList[0],
                Flank = left,
                Viewport = null,
                Engine = null,
            });
        }

        // Center column with options. Center its children within the box, so
        // the box's padding sits evenly above and below instead of pooling all
        // at the bottom (which left the content crowding the top edge).
        modal.column = new VBoxContainer();
        modal.column.Alignment = BoxContainer.AlignmentMode.Center;
        modal.column.AddThemeConstantOverride("separation", 12);

        var title = Text.Label("Player Options", 48.0f, Screen.TitleColor);
        title.HorizontalAlignment = HorizontalAlignment.Center;
        var titleBox = new MarginContainer { SizeFlagsHorizontal = Control.SizeFlags.ShrinkCenter };
        titleBox.AddThemeConstantOverride("margin_bottom", 12);
        titleBox.AddChild(title);
        modal.column.AddChild(titleBox);
        modal.texts.Add(title);

        // Player tags (versus only)
        if (versus)
        {
            var header = new HBoxContainer();
            header.AddThemeConstantOverride("separation", 20);
            var namePad = new Control { CustomMinimumSize = new Vector2(NameWidth, 0.0f) };
            header.AddChild(namePad);

            foreach (var player in playersList)
            {
                var cell = new CenterContainer { CustomMinimumSize = new Vector2(ValueWidth, 36.0f) };
                var playerLabel = player == PlayerId.P1 ? "P1" : "P2";
                var tag = Text.Label(playerLabel, 30.0f, Screen.TitleColor);
                cell.AddChild(tag);
                modal.texts.Add(tag);
                header.AddChild(cell);
            }

            modal.column.AddChild(header);
        }

        // Option rows
        for (int index = 0; index < 6; index++)
        {
            var row = (OptionRow)index;
            var rowBox = new HBoxContainer();
            rowBox.AddThemeConstantOverride("separation", 20);

            var nameCell = new Control { CustomMinimumSize = new Vector2(NameWidth, 34.0f) };
            var name = Text.Label(RowName(row), 28.0f, Screen.InactiveColor);
            nameCell.AddChild(name);
            modal.rowNames.Add(name);
            modal.texts.Add(name);
            rowBox.AddChild(nameCell);

            foreach (var player in playersList)
            {
                var cell = new CenterContainer { CustomMinimumSize = new Vector2(ValueWidth, 34.0f) };
                var playerOptions = Settings.Instance.Player(player);
                var value = RowValue(row, playerOptions);
                var valueLabel = Text.Label(value, 28.0f, Screen.InactiveColor);
                cell.AddChild(valueLabel);
                modal.values.Add(new ValueCell
                {
                    Player = player,
                    Row = index,
                    Label = valueLabel,
                });
                modal.texts.Add(valueLabel);
                rowBox.AddChild(cell);
            }

            modal.column.AddChild(rowBox);
        }

        stripeRow.AddChild(modal.column);

        // Right flank
        var right = new Control { SizeFlagsHorizontal = Control.SizeFlags.ExpandFill };
        stripeRow.AddChild(right);
        if (playersList.Count > 1)
        {
            modal.previews.Add(new Preview
            {
                Player = playersList[1],
                Flank = right,
                Viewport = null,
                Engine = null,
            });
        }

        modal.content.AddChild(stripeRow);
        modal.root.AddChild(modal.content);

        // Initialize panel states
        foreach (var player in playersList)
        {
            modal.panels.Add(new PanelState { Player = player, ActiveRow = 0 });
        }

        return modal;
    }

    public void MarkRebuild()
    {
        if (state != null)
            state.Rebuild = true;
    }

    /// <summary>
    /// Returns true once the modal fully closes.
    /// </summary>
    public bool Update(double delta)
    {
        if (dir > 0.0f && t >= 1.0f)
        {
            HandlePulses();
            HandleClose();
        }

        if (!AnimateTransition(delta))
        {
            root.QueueFree();
            return true;
        }

        BuildPreviews();
        RefitPreviews();
        DrivePreviews();
        RefreshValues();
        HighlightRows();
        return false;
    }

    private void HandlePulses()
    {
        foreach (var pulse in NavInput.Instance.Pulses)
        {
            if (pulse.AsStep() is not (var player, var direction))
                continue;

            var panel = panels.FirstOrDefault(p => p.Player == player);
            if (panel is null)
                continue;

            bool acted = false;
            if (direction == StepDirection.Up)
            {
                panel.ActiveRow = (panel.ActiveRow + 5) % 6;
                acted = true;
            }
            else if (direction == StepDirection.Down)
            {
                panel.ActiveRow = (panel.ActiveRow + 1) % 6;
                acted = true;
            }
            else if (direction == StepDirection.Left || direction == StepDirection.Right)
            {
                var row = (OptionRow)panel.ActiveRow;
                int delta = direction == StepDirection.Left ? -1 : 1;
                Settings.Instance.EditPlayer(player, options =>
                {
                    acted = ChangeValue(row, delta, ref options);
                    return options;
                });
            }

            if (acted)
                Sfx.Navigate.Play();
        }
    }

    private void HandleClose()
    {
        if (dir < 0.0f)
            return;

        if (Actions.AnyJustPressed(players, GameActions.Cancel) ||
            Actions.AnyJustPressed(players, GameActions.Select))
        {
            Sfx.Cancel.Play();
            dir = -1.0f;
        }
    }

    private bool AnimateTransition(double delta)
    {
        bool advance = !(t >= 1.0f && dir > 0.0f);
        if (advance)
        {
            t = Mathf.Clamp(t + dir * (float)delta / TransitionSeconds, 0.0f, 1.0f);
        }

        if (t <= 0.0f && dir < 0.0f)
            return false;

        var size = root.GetSize();
        var height = column.GetCombinedMinimumSize().Y + 48.0f;
        var top = (size.Y - height) / 2.0f;
        var eased = 1.0f - Mathf.Pow(1.0f - t, 3);

        background.Position = new Vector2(-size.X * (1.0f - eased), top);
        background.Size = new Vector2(size.X, height);
        background.Color = new Color(0, 0, 0, 1.0f - Screen.LinearBlend(1.0f - eased));

        content.Position = new Vector2(size.X * (1.0f - eased), top);
        content.Size = new Vector2(size.X, height);

        foreach (var text in texts)
        {
            var modulate = text.Modulate;
            modulate.A = Screen.LinearBlend(eased);
            text.Modulate = modulate;
        }

        return true;
    }

    private void BuildPreviews()
    {
        if (state is not null)
            return;

        var music = MusicPlayer.Instance;
        var loopWindow = music.LoopWindow();
        if (loopWindow is null)
            return;

        var (timing, start, length) = loopWindow.Value;

        if (previews.Count == 0)
        {
            state = new PreviewState
            {
                Rows = MockedRows(timing, start, length),
                Timing = timing,
                LastVisible = Seconds.Zero,
                Rebuild = false,
            };
            return;
        }

        // Check if all flanks are laid out
        foreach (var preview in previews)
        {
            if (preview.Flank.GetSize().X <= 0.0f || preview.Flank.GetSize().Y <= 0.0f)
                return;
        }

        // Create viewports
        foreach (var preview in previews)
        {
            var size = preview.Flank.GetSize();
            var canvas = BandCanvas(size);
            var viewport = new SubViewport
            {
                // Opaque backdrop, not transparent: the grade text's glow shader
                // is premultiplied-alpha (it adds light onto its background, as in
                // the play scene's opaque window). Composited into a transparent
                // viewport and re-blended by the display TextureRect, that added
                // light is lost and the glow disappears. The backdrop below gives
                // it the same opaque context the play scene has.
                TransparentBg = true,
                Size = new Vector2I((int)size.X, (int)size.Y),
                // Mirror the main window's canvas_items stretch (project.godot):
                // the engine authors its HUD in logical canvas units, so the
                // preview must stretch those units to its pixels exactly as the
                // window does — otherwise the field self-scales but the text does
                // not, and the grade/combo/OK words land off-place and oversized.
                Size2DOverride = new Vector2I((int)canvas.X, (int)canvas.Y),
                Size2DOverrideStretch = true,
                RenderTargetUpdateMode = SubViewport.UpdateMode.Always,
            };
            preview.Flank.AddChild(viewport);

            // Opaque backdrop behind the field and HUD, matching the modal's own
            // black so the preview reads as a seamless panel while giving the
            // premultiplied grade-text glow the opaque surface it composites on.
            var backdrop = new ColorRect { Color = Colors.Black };
            backdrop.SetAnchorsAndOffsetsPreset(Control.LayoutPreset.FullRect);
            viewport.AddChild(backdrop);

            var display = new TextureRect();
            display.SetAnchorsAndOffsetsPreset(Control.LayoutPreset.FullRect);
            if (viewport.GetTexture() is Texture2D tex)
                display.Texture = tex;

            preview.Flank.AddChild(display);
            preview.Viewport = viewport;
        }

        state = new PreviewState
        {
            Rows = MockedRows(timing, start, length),
            Timing = timing,
            LastVisible = Seconds.Zero,
            Rebuild = true,
        };
    }

    private void RefitPreviews()
    {
        foreach (var preview in previews)
        {
            if (preview.Viewport is null || preview.Engine is null)
                continue;

            var surface = preview.Flank.GetSize();
            if (surface.X <= 0.0f || surface.Y <= 0.0f)
                continue;

            var canvas = BandCanvas(surface);
            preview.Viewport.Size = new Vector2I((int)surface.X, (int)surface.Y);
            preview.Viewport.Size2DOverride = new Vector2I((int)canvas.X, (int)canvas.Y);
            preview.Engine.SetCanvas(canvas, surface.Y / PreviewBand);
        }
    }

    private void DrivePreviews()
    {
        if (state is null)
            return;

        var settings = Settings.Instance.Machine.Timing;
        var music = MusicPlayer.Instance;
        var visible = music.VisibleNow(settings);

        if (visible is null)
            return;

        var (visibleSeconds, _) = visible.Value;

        if (visibleSeconds.Value + 0.05 < state.LastVisible.Value)
            state.Rebuild = true;

        state.LastVisible = visibleSeconds;

        if (state.Rebuild)
        {
            state.Rebuild = false;
            var live = new List<Row>();
            foreach (var chartRow in state.Rows)
            {
                if (RowUntil(chartRow, state.Timing) > visibleSeconds)
                    live.Add(chartRow);
            }

            foreach (var preview in previews)
            {
                if (preview.Engine is not null)
                    preview.Engine.QueueFree();

                if (preview.Viewport is null)
                    continue;

                var surface = preview.Flank.GetSize();
                var canvas = BandCanvas(surface);
                var arrow = PreviewArrowSize();

                var engine = StepfilePlayer.Instantiate(new StepfilePlayerOptions
                {
                    Fields = new List<FieldSpec>
                    {
                        new FieldSpec
                        {
                            Layout = new FieldLayout(
                                preview.Player,
                                0.0f,
                                4,
                                Settings.Instance.Player(preview.Player).NoteSpeed,
                                arrow
                            ),
                            Rows = live,
                            Mines = [],
                            MaxHealth = uint.MaxValue,
                            GradeLayer = Settings.Instance.Player(preview.Player).GradeLayer,
                        }
                    },
                    Timing = state.Timing,
                    Canvas = canvas,
                });

                preview.Viewport.AddChild(engine);

                var padding = Config.Current?.Stage?.ScreenEdgePadding ?? 20.0f;
                var half = PreviewBand / 2.0f;
                engine.SetCanvas(canvas, surface.Y / PreviewBand);
                engine.SetTargetY(half - padding - arrow / 2.0f);
                engine.SetGradeArea(GradeText.AreaOf(half - padding, -half + padding));

                preview.Engine = engine;
            }
        }

        // Feed input to preview engines
        foreach (var preview in previews)
        {
            if (preview.Engine is null)
                continue;

            preview.Engine.SetTime(visibleSeconds, visibleSeconds);
            preview.Engine.ClearInput();

            foreach (var chartRow in state.Rows)
            {
                var time = state.Timing.SecondsAtBeat(chartRow.Beat);
                var offset = AutoplayOffset(NoteTier(chartRow.Beat));

                if (offset is null)
                    continue;

                var due = new Seconds(time.Value - offset.Value.Value);

                foreach (var arrow in chartRow.Arrows)
                {
                    if (visibleSeconds.Value >= due.Value && visibleSeconds.Value < ArrowUntil(chartRow, arrow, state.Timing).Value)
                    {
                        var action = GameActions.Step(preview.Player, StepDirectionExtensions.OfColumn(arrow.Column));
                        preview.Engine.Press(action, true);
                    }
                }
            }
        }
    }

    private void RefreshValues()
    {
        foreach (var cell in values)
        {
            var current = RowValue((OptionRow)cell.Row, Settings.Instance.Player(cell.Player));
            cell.Label.Text = current;
        }
    }

    private void HighlightRows()
    {
        for (int index = 0; index < rowNames.Count; index++)
        {
            var active = panels.Any(p => p.ActiveRow == index);
            rowNames[index].AddThemeColorOverride("font_color", active ? Screen.ActiveColor : Screen.InactiveColor);
        }

        foreach (var cell in values)
        {
            var active = panels.Any(p => p.Player == cell.Player && p.ActiveRow == cell.Row);
            cell.Label.AddThemeColorOverride("font_color", active ? Screen.ActiveColor : Screen.InactiveColor);
        }
    }

    private static string RowName(OptionRow row) => row switch
    {
        OptionRow.SpeedType => "Speed Type",
        OptionRow.SpeedModifier => "Speed Modifier",
        OptionRow.NoteSkin => "Note Skin",
        OptionRow.Perspective => "Perspective",
        OptionRow.GradeLayer => "Grade Layer",
        OptionRow.GradePosition => "Grade Position",
        _ => "",
    };

    private static string RowValue(OptionRow row, PlayerOptions options) => row switch
    {
        OptionRow.SpeedType => options.NoteSpeed is NoteSpeed.Constant ? "Constant" : "Dynamic",
        OptionRow.SpeedModifier => FormatModifier(options.NoteSpeed.Value, options.NoteSpeed),
        OptionRow.NoteSkin => NoteSkinLibrary.Skins
            .FirstOrDefault(s => s.Name == options.NoteSkin)?.DisplayName ?? options.NoteSkin,
        OptionRow.Perspective => options.Perspective.ToString(),
        OptionRow.GradeLayer => options.GradeLayer.ToString(),
        OptionRow.GradePosition => $"{options.GradePosition.Value:F0}%",
        _ => "",
    };

    private static bool ChangeValue(OptionRow row, int delta, ref PlayerOptions options)
    {
        return row switch
        {
            OptionRow.SpeedType => ChangeSpeedType(delta, ref options),
            OptionRow.SpeedModifier => ChangeSpeedModifier(delta, ref options),
            OptionRow.NoteSkin => ChangeNoteSkin(delta, ref options),
            OptionRow.Perspective => ChangePerspective(delta, ref options),
            OptionRow.GradeLayer => ChangeGradeLayer(delta, ref options),
            OptionRow.GradePosition => ChangeGradePosition(delta, ref options),
            _ => false,
        };
    }

    private static bool ChangeSpeedType(int delta, ref PlayerOptions options)
    {
        var newSpeed = (options.NoteSpeed, delta) switch
        {
            (NoteSpeed.Dynamic d, -1) => new NoteSpeed.Constant(Config.Current?.SpeedModifiers?.Set(new NoteSpeed.Constant(1.0f)).Default ?? 1.0f),
            (NoteSpeed.Constant c, 1) => new NoteSpeed.Dynamic(Config.Current?.SpeedModifiers?.Set(new NoteSpeed.Dynamic(1.0f)).Default ?? 1.0f),
            _ => options.NoteSpeed,
        };

        if (newSpeed != options.NoteSpeed)
        {
            options = options with { NoteSpeed = newSpeed };
            return true;
        }
        return false;
    }

    private static bool ChangeSpeedModifier(int delta, ref PlayerOptions options)
    {
        var set = Config.Current?.SpeedModifiers?.Set(options.NoteSpeed);
        if (set is null)
            return false;

        var index = SelectedIndex(set.Options.ToArray(), options.NoteSpeed.Value);
        var stepped = index + delta;

        if (stepped < 0 || stepped >= set.Options.Length)
            return false;

        var newSpeed = options.NoteSpeed switch
        {
            NoteSpeed.Constant _ => new NoteSpeed.Constant(set.Options[stepped]),
            NoteSpeed.Dynamic _ => new NoteSpeed.Dynamic(set.Options[stepped]),
            _ => options.NoteSpeed,
        };

        if (newSpeed.Value != options.NoteSpeed.Value)
        {
            options = options with { NoteSpeed = newSpeed };
            return true;
        }
        return false;
    }

    private static bool ChangeNoteSkin(int delta, ref PlayerOptions options)
    {
        var skins = NoteSkinLibrary.Skins;
        int index = -1;
        for (int i = 0; i < skins.Count; i++)
        {
            if (skins[i].Name == options.NoteSkin)
            {
                index = i;
                break;
            }
        }

        if (index < 0)
            index = 0;

        var stepped = index + delta;
        if (stepped < 0 || stepped >= skins.Count)
            return false;

        if (stepped != index)
        {
            options = options with { NoteSkin = skins[stepped].Name };
            return true;
        }
        return false;
    }

    private static bool ChangePerspective(int delta, ref PlayerOptions options)
    {
        var allPerspectives = Enum.GetValues<Perspective>();
        var index = Array.IndexOf(allPerspectives, options.Perspective);
        if (index < 0)
            index = 0;

        var stepped = index + delta;
        if (stepped < 0 || stepped >= allPerspectives.Length)
            return false;

        if (stepped != index)
        {
            options = options with { Perspective = allPerspectives[stepped] };
            return true;
        }
        return false;
    }

    private static bool ChangeGradeLayer(int delta, ref PlayerOptions options)
    {
        var allLayers = Enum.GetValues<GradeLayer>();
        var index = Array.IndexOf(allLayers, options.GradeLayer);
        if (index < 0)
            index = 0;

        var stepped = index + delta;
        if (stepped < 0 || stepped >= allLayers.Length)
            return false;

        if (stepped != index)
        {
            options = options with { GradeLayer = allLayers[stepped] };
            return true;
        }
        return false;
    }

    private static bool ChangeGradePosition(int delta, ref PlayerOptions options)
    {
        const float GradePositionStep = 10.0f;
        var newValue = new Percent(Mathf.Clamp(options.GradePosition.Value + delta * GradePositionStep, 0.0f, 100.0f));
        if (newValue != options.GradePosition)
        {
            options = options with { GradePosition = newValue };
            return true;
        }
        return false;
    }

    private static int SelectedIndex(float[] options, float value)
    {
        int best = 0;
        float bestDist = float.MaxValue;
        for (int i = 0; i < options.Length; i++)
        {
            var dist = Mathf.Abs(options[i] - value);
            if (dist < bestDist)
            {
                best = i;
                bestDist = dist;
            }
        }
        return best;
    }

    private static string FormatModifier(float value, NoteSpeed speed) =>
        speed is NoteSpeed.Dynamic ? $"{FormatValue(value)}x" : FormatValue(value);

    private static string FormatValue(float value) =>
        value == (int)value ? value.ToString("F0") : value.ToString();

    private static int NoteTier(Beat beat)
    {
        int[] PreviewGrades = { 0, 1, 2 };
        var ordinal = (long)Math.Round(beat.Value * 2.0);
        return PreviewGrades[((ordinal % PreviewGrades.Length) + PreviewGrades.Length) % PreviewGrades.Length];
    }

    private static Seconds ArrowUntil(Row row, Arrow arrow, StepfileTiming timing)
    {
        const double AutoplayTapHold = 0.05;
        if (arrow.Tail is Tail tail)
            return timing.SecondsAtBeat(tail.End);
        return new Seconds(timing.SecondsAtBeat(row.Beat).Value + AutoplayTapHold);
    }

    private static Seconds RowUntil(Row row, StepfileTiming timing)
    {
        var maxSeconds = timing.SecondsAtBeat(row.Beat);
        foreach (var arrow in row.Arrows)
        {
            var until = ArrowUntil(row, arrow, timing);
            if (until.Value > maxSeconds.Value)
                maxSeconds = until;
        }
        return maxSeconds;
    }

    private static Seconds? AutoplayOffset(int tier)
    {
        var dynamic = Config.Current?.Grading?.Dynamic;
        if (dynamic is null || tier < 0 || tier >= dynamic.Count)
            return null;

        var lower = tier == 0 ? Seconds.Zero : dynamic[tier - 1].Window;
        var current = dynamic[tier].Window;
        return new Seconds((lower.Value + current.Value) / 2.0);
    }

    private static Vector2 BandCanvas(Vector2 surface)
    {
        var aspect = Mathf.Max(surface.X, 1.0f) / Mathf.Max(surface.Y, 1.0f);
        return new Vector2(PreviewBand * aspect, PreviewBand);
    }

    private static float PreviewArrowSize()
    {
        var config = Config.Current;
        if (config?.Stage is null)
            return 32.0f;

        return NoteField.FittedArrowSize(
            4.0f,
            Screen.Size.X - 2.0f * config.Stage.MarginX,
            NoteField.MaxArrowSize(config.Stage.MaxArrowSize, 1.0f)
        );
    }

    private static List<Row> MockedRows(StepfileTiming timing, Seconds start, Seconds length)
    {
        int[] Pattern = { 2, 0, 1, 2, 3, 1 };
        double[] PatternOffset = { 0.0, 0.5, 1.0, 2.0, 2.5, 3.0 };
        double?[] PatternHoldEnd = { null, null, 1.5, null, null, 3.5 };

        var first = (long)Math.Ceiling(timing.BeatAtSeconds(start).Value / 4.0);
        var last = (long)Math.Floor(timing.BeatAtSeconds(new Seconds(start.Value + length.Value)).Value / 4.0) - 1;
        last = Math.Max(last, first);

        var rows = new List<Row>();
        for (long measure = first; measure <= last; measure++)
        {
            var basebeat = measure * 4.0;
            for (int i = 0; i < Pattern.Length; i++)
            {
                var beatOffset = PatternOffset[i];
                var column = Pattern[i];
                var holdEnd = PatternHoldEnd[i];

                rows.Add(new Row(
                    new Beat(basebeat + beatOffset),
                    (uint)(i switch { 1 or 4 => 8, _ => 4 }),
                    new List<Arrow>
                    {
                        new Arrow
                        {
                            Column = column,
                            Tail = holdEnd.HasValue ? new Tail(new Beat(basebeat + holdEnd.Value), false) : null,
                        }
                    }
                ));
            }
        }

        return rows;
    }

    private enum OptionRow
    {
        SpeedType,
        SpeedModifier,
        NoteSkin,
        Perspective,
        GradeLayer,
        GradePosition,
    }
}
