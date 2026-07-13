using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>One row in the wheel: a group header or a selectable stepfile.</summary>
public abstract record WheelEntry
{
    private WheelEntry() { }

    /// <summary>A group header, by its index in the library.</summary>
    public sealed record Group(int Index) : WheelEntry;

    /// <summary>A selectable stepfile, by its library id.</summary>
    public sealed record Stepfile(StepfileId Id) : WheelEntry;
}

/// <summary>
/// One spawned row slot: an unscaled root the animation places, carrying the
/// bar art, its texts, and the per-player rating widgets.
/// </summary>
public struct SlotUi
{
    public Node2D Root { get; set; }
    public Sprite2D Bar { get; set; }
    public Label Title { get; set; }
    public Label Artist { get; set; }
    public PerPlayer<RatingUi> Ratings { get; set; }
}

/// <summary>
/// The stepfile browser. Every active player scrolls the one wheel (in versus
/// they race for it); the settled row drives the scene's music, background
/// wash, and info panel. Left/right scrolls the wheel (wrapping); up/down
/// changes the active player's difficulty; a Select tap acts on the row while
/// holding Select opens the player-options modal.
/// </summary>
[GlobalClass]
public partial class Wheel : Control
{
    private const float RowHeight = 56.0f;
    private const float BarWidth = 660.0f;
    private const float BarHeight = 50.0f;
    private const float WheelX = 330.0f;
    private const float BulgePerRow = 3.0f;
    private const float WheelEaseRate = 14.0f;
    private const float SettleDelaySeconds = 0.35f;
    private const float OptionsHoldSeconds = 0.5f;
    private const float DetailsBoxSizeX = 540.0f;
    private const float DetailsBoxSizeY = 530.0f;
    private const float DetailsBoxCenterX = -320.0f;
    private const float DetailsBoxCenterY = 12.0f;
    private const float DetailsBoxAlpha = 0.78f;
    private const float BannerSizeX = DetailsBoxSizeX;
    private const float BannerSizeY = 168.0f;
    private const float BackgroundOpacity = 0.25f;

    private static readonly Color BackdropColor = new(0.05f, 0.085f, 0.03f);
    private static readonly Color StepfileBarColor = new(0.10f, 0.19f, 0.07f);
    private static readonly Color GroupBarColor = new(0.055f, 0.10f, 0.045f);
    private static readonly Color BorderColor = new(0.97f, 1.0f, 0.62f);
    private static readonly Color StepfileTextColor = new(0.35f, 0.95f, 0.4f);
    private static readonly Color ActiveStepfileTextColor = new(0.8f, 1.0f, 0.75f);
    private static readonly Color GroupTextColor = new(0.95f, 0.55f, 0.15f);
    private static readonly Color ArtistTextColor = new(0.25f, 0.75f, 0.35f);
    private static readonly Color BpmTextColor = new(0.85f, 0.95f, 0.55f);
    private static readonly Color BannerTintColor = new(0.10f, 0.18f, 0.07f);
    private static readonly Color BannerTextColor = new(0.9f, 1.0f, 0.85f);
    private static readonly Color StatsTextColor = new(0.75f, 0.9f, 0.7f);
    private static readonly Color HelpTextColor = new(0.5f, 0.62f, 0.5f);

    /// <summary>Once every beat, apex on it, decaying cubically until the next.</summary>
    private static readonly RhythmCycle HighlightPulse = new()
    {
        Speed = 4.0,
        Easing = new Vector4(0.32f, 0.0f, 0.67f, 0.0f),
    };

    private List<PlayerId> players = [];
    private StepsType stepsType = StepsType.DanceSingle;
    private List<WheelEntry> entries = [];
    private int active;
    private List<SlotUi> slots = [];
    private float scrollOffset;
    private int? expandedGroup;
    private ImageTexture? barTexture;
    private bool dirty;
    private Seconds settle = Seconds.Zero;
    private bool justSettled;
    private PerPlayer<SelectHold> selectHolds = new();
    private Node2D? canvas;
    private Sprite2D? highlight;
    private Node2D? infoPanel;
    private PendingTexture? bannerPending;
    private TextureRect? bannerRect;
    private Wash wash = new();
    private OptionsModal? modal;

    private struct SelectHold
    {
        public Seconds Held { get; set; }
        public bool Armed { get; set; }
    }

    public override void _Ready()
    {
        SetAnchorsAndOffsetsPreset(LayoutPreset.FullRect);
        Settings.Instance.Changed += OnSettingsChanged;

        var backdrop = new ColorRect { Color = BackdropColor };
        backdrop.SetAnchorsAndOffsetsPreset(LayoutPreset.FullRect);
        backdrop.MouseFilter = MouseFilterEnum.Ignore;
        AddChild(backdrop);

        canvas = new Node2D();
        AddChild(canvas);

        var mode = Game.Instance.PlayMode;
        stepsType = mode.StepsType();
        players = new List<PlayerId>(mode.Players());

        var target = Game.Instance.TakeWheelTarget() ?? WheelDefaultSelection();
        if (target is null && !Library.Instance.IsEmpty)
        {
            target = new StepfileId(0, 0);
        }

        expandedGroup = target?.Group;
        BuildEntries();
        active = FindActiveIndex(target) ?? 0;

        barTexture = RoundedTexture(512, 64, 16.0f, null);

        var detailsBox = new Sprite2D
        {
            Texture = RoundedTexture((uint)DetailsBoxSizeX, (uint)DetailsBoxSizeY, 5.0f, null),
            Modulate = new Color(0, 0, 0, DetailsBoxAlpha),
            Position = new Vector2(DetailsBoxCenterX, -DetailsBoxCenterY),
            ZIndex = 45,
        };
        canvas.AddChild(detailsBox);

        var overlaySize = new Vector2(BarWidth + 10.0f, BarHeight + 10.0f);
        highlight = new Sprite2D
        {
            Texture = RoundedTexture((uint)overlaySize.X, (uint)overlaySize.Y, 18.0f, 5.0f),
            Modulate = BorderColor,
            Position = new Vector2(WheelX, 0.0f),
            ZIndex = 120,
        };
        canvas.AddChild(highlight);

        if (entries.Count == 0)
        {
            var empty = Text.Label($"No stepfiles with {stepsType} charts found", 30.0f, new Color(0.9f, 0.4f, 0.4f));
            canvas.AddChild(empty);
            Text.Place(empty, Vector2.Zero, TextPivot.Center);
            empty.ZIndex = 200;
        }

        var help = Text.Label("up/down: change difficulty\nhold select: change options", 20.0f, HelpTextColor);
        help.HorizontalAlignment = HorizontalAlignment.Center;
        canvas.AddChild(help);
        Text.Place(help, new Vector2(DetailsBoxCenterX, 214.0f), TextPivot.Center);
        help.ZIndex = 50;

        dirty = true;
        MarkSettled();
    }

    public override void _Process(double delta)
    {
        // The canvas space tracks the visible center, so world-placed pieces
        // stay centered whatever the window reveals.
        if (canvas is not null)
        {
            canvas.Position = Screen.VisibleRect(this).GetCenter();
        }

        if (modal is not null)
        {
            if (modal.Update(delta))
            {
                modal = null;
                NavInput.Instance.Clear();
            }
        }
        else if (Game.Instance.AcceptsInput)
        {
            Navigate();
            ChangeDifficulty();
            TrackSelect(delta);
            HandleCancel();
        }

        FitWheelRows();
        AnimateWheel(delta);
        PackPlayerRatings();
        PositionRatingLabels();
        SettleWheel(delta);
        DriveWheelBgm();
        PulseActiveRow();
        RefreshWash();
        FadeWash(delta);
        RefreshWheelRows();
        RefreshWheelRatings();
        RefreshInfoPanel();
        PollBanner();

        dirty = false;
        justSettled = false;
    }

    public override void _ExitTree()
    {
        Settings.Instance.Changed -= OnSettingsChanged;
        MusicPlayer.Instance.Stop();
    }

    /// <summary>Previews mirror the options they render, so an open modal rebuilds them.</summary>
    private void OnSettingsChanged() => modal?.MarkRebuild();

    /// <summary>Discrete actions (anything but scrolling) take effect immediately.</summary>
    private void MarkSettled()
    {
        settle = new Seconds(SettleDelaySeconds);
        justSettled = true;
    }

    /// <summary>
    /// The wheel lists only what the mode can play: selectable stepfiles with
    /// at least one non-empty chart of the type, and the groups holding them.
    /// </summary>
    private void BuildEntries()
    {
        entries = [];
        var library = Library.Instance;

        for (int groupIndex = 0; groupIndex < library.Groups.Count; groupIndex++)
        {
            var group = library.Groups[groupIndex];
            var playable = new List<int>();
            for (int stepfileIndex = 0; stepfileIndex < group.Stepfiles.Count; stepfileIndex++)
            {
                var stepfile = group.Stepfiles[stepfileIndex].Stepfile;
                if (stepfile.Selectable && stepfile.PlayableCharts(stepsType).Count > 0)
                {
                    playable.Add(stepfileIndex);
                }
            }

            if (playable.Count == 0)
            {
                continue;
            }

            entries.Add(new WheelEntry.Group(groupIndex));
            if (expandedGroup == groupIndex)
            {
                foreach (var stepfileIndex in playable)
                {
                    entries.Add(new WheelEntry.Stepfile(new StepfileId(groupIndex, stepfileIndex)));
                }
            }
        }
    }

    private int? FindActiveIndex(StepfileId? target)
    {
        if (target is not StepfileId id)
        {
            return null;
        }
        for (int i = 0; i < entries.Count; i++)
        {
            if (entries[i] is WheelEntry.Stepfile s && s.Id == id)
            {
                return i;
            }
        }
        return null;
    }

    /// <summary>
    /// Resolves the configured wheel-default search pair: the first group whose
    /// name contains the group string and that holds a stepfile whose title
    /// contains the stepfile string, both case-insensitive.
    /// </summary>
    private StepfileId? WheelDefaultSelection()
    {
        if (Config.Current is null)
        {
            return null;
        }
        var groupSearch = Config.Current.WheelDefaultGroup.ToLowerInvariant();
        var stepfileSearch = Config.Current.WheelDefaultStepfile.ToLowerInvariant();

        for (int groupIndex = 0; groupIndex < Library.Instance.Groups.Count; groupIndex++)
        {
            var group = Library.Instance.Groups[groupIndex];
            if (!group.Name.ToLowerInvariant().Contains(groupSearch))
            {
                continue;
            }
            for (int stepfileIndex = 0; stepfileIndex < group.Stepfiles.Count; stepfileIndex++)
            {
                if (group.Stepfiles[stepfileIndex].DisplayTitle().ToLowerInvariant().Contains(stepfileSearch))
                {
                    return new StepfileId(groupIndex, stepfileIndex);
                }
            }
        }
        return null;
    }

    /// <summary>The entry a slot shows: the active row sits at the center slot, wrapping.</summary>
    private WheelEntry? EntryAt(int slot)
    {
        if (entries.Count == 0)
        {
            return null;
        }
        int len = entries.Count;
        int index = (((active + slot - (slots.Count / 2)) % len) + len) % len;
        return entries[index];
    }

    private void Navigate()
    {
        foreach (var pulse in NavInput.Instance.Pulses)
        {
            if (entries.Count == 0)
            {
                return;
            }
            if (pulse.AsStep() is not (var player, var direction))
            {
                continue;
            }
            if (!players.Contains(player))
            {
                continue;
            }
            int delta = direction switch
            {
                StepDirection.Left => -1,
                StepDirection.Right => 1,
                _ => 0,
            };
            if (delta == 0)
            {
                continue;
            }
            int len = entries.Count;
            active = (((active + delta) % len) + len) % len;
            scrollOffset -= delta;
            dirty = true;
            settle = Seconds.Zero;
            Sfx.WheelMove.Play();
        }
    }

    /// <summary>Each active player steps their own difficulty with their pad's up/down.</summary>
    private void ChangeDifficulty()
    {
        if (entries[active] is not WheelEntry.Stepfile row)
        {
            return;
        }
        var stepfile = Library.Instance.Stepfile(row.Id).Stepfile;

        foreach (var player in players)
        {
            int delta = 0;
            if (Actions.JustPressed(GameActions.Step(player, StepDirection.Up)))
            {
                delta += 1;
            }
            if (Actions.JustPressed(GameActions.Step(player, StepDirection.Down)))
            {
                delta -= 1;
            }
            if (delta == 0)
            {
                continue;
            }

            var charts = stepfile.PlayableCharts(stepsType);
            if (ChartFor(stepfile, player) is not int current)
            {
                continue;
            }
            int position = -1;
            for (int i = 0; i < charts.Count; i++)
            {
                if (charts[i] == current)
                {
                    position = i;
                    break;
                }
            }
            if (position < 0)
            {
                continue;
            }
            int next = Mathf.Clamp(position + delta, 0, charts.Count - 1);
            if (next != position)
            {
                Game.Instance.PreferredDifficulty[player] = stepfile.Charts[charts[next]].Difficulty.Rank();
                dirty = true;
                MarkSettled();
                Sfx.Navigate.Play();
            }
        }
    }

    /// <summary>Holding Select opens the options modal; a shorter tap acts on the row.</summary>
    private void TrackSelect(double delta)
    {
        if (entries.Count == 0)
        {
            return;
        }
        foreach (var player in players)
        {
            var select = GameActions.Select(player);
            var hold = selectHolds[player];

            if (Actions.JustPressed(select))
            {
                hold.Armed = true;
                hold.Held = Seconds.Zero;
            }
            if (hold.Armed)
            {
                if (Actions.Pressed(select))
                {
                    hold.Held += new Seconds(delta);
                    if (hold.Held.Value >= OptionsHoldSeconds)
                    {
                        hold.Armed = false;
                        selectHolds[player] = hold;
                        Sfx.Select.Play();
                        OpenOptions();
                        return;
                    }
                }
                else if (Actions.JustReleased(select))
                {
                    hold.Armed = false;
                    selectHolds[player] = hold;
                    HandleTap();
                    return;
                }
            }
            selectHolds[player] = hold;
        }
    }

    /// <summary>A tap toggles a group open, or starts each active player on their own chart.</summary>
    private void HandleTap()
    {
        Sfx.WheelSelect.Play();

        switch (entries[active])
        {
            case WheelEntry.Group group:
                // Only one group is ever expanded: opening a group closes the
                // previous one, opening it again closes it.
                expandedGroup = expandedGroup != group.Index ? group.Index : null;
                BuildEntries();
                active = 0;
                for (int i = 0; i < entries.Count; i++)
                {
                    if (entries[i] is WheelEntry.Group g && g.Index == group.Index)
                    {
                        active = i;
                        break;
                    }
                }
                dirty = true;
                MarkSettled();
                Sfx.GroupToggle.Play();
                break;

            case WheelEntry.Stepfile row:
                var stepfile = Library.Instance.Stepfile(row.Id).Stepfile;
                var charts = new List<PlayerChart>();
                foreach (var player in players)
                {
                    if (ChartFor(stepfile, player) is int chart)
                    {
                        charts.Add(new PlayerChart(player, chart));
                    }
                }
                Game.Instance.SetSelectedStepfile(new SelectedStepfile(row.Id, charts));
                Sfx.StartFile.Play();
                Game.Instance.ChangeScene(GameScene.Play);
                break;
        }
    }

    private void OpenOptions()
    {
        NavInput.Instance.Clear();
        modal = OptionsModal.Open(this, players);
    }

    private void HandleCancel()
    {
        if (Actions.AnyJustPressed(players, GameActions.Cancel))
        {
            Sfx.Cancel.Play();
            Game.Instance.ChangeScene(GameScene.ModeSelect);
        }
    }

    /// <summary>Crossing the settle delay fires the settled reactions once.</summary>
    private void SettleWheel(double delta)
    {
        if (settle.Value >= SettleDelaySeconds)
        {
            return;
        }
        settle = new Seconds(settle.Value + delta);
        if (settle.Value >= SettleDelaySeconds)
        {
            justSettled = true;
        }
    }

    private void AnimateWheel(double delta)
    {
        if (scrollOffset != 0.0f)
        {
            scrollOffset *= Mathf.Exp(-WheelEaseRate * (float)delta);
            if (Mathf.Abs(scrollOffset) < 0.01f)
            {
                scrollOffset = 0.0f;
            }
        }
        for (int index = 0; index < slots.Count; index++)
        {
            var x = SlotX(index, slots.Count, scrollOffset);
            var y = SlotY(index, slots.Count, scrollOffset);
            slots[index].Root.Position = new Vector2(x, -y);
        }
    }

    /// <summary>
    /// The settled row's stepfile is the scene's background music; rows without
    /// one (groups) fall back to the default BGM. Playing what already plays is
    /// a no-op, so rows resolving to the same music keep it uninterrupted.
    /// </summary>
    private void DriveWheelBgm()
    {
        if (!justSettled)
        {
            return;
        }
        var entry = entries.Count > active && entries[active] is WheelEntry.Stepfile row
            ? Library.Instance.Stepfile(row.Id)
            : Library.Instance.DefaultBgm;
        MusicPlayer.Instance.Play(entry.Bgm());
    }

    /// <summary>Pulses the active-row highlight on the beat, apex on it; steady while nothing plays.</summary>
    private void PulseActiveRow()
    {
        var alpha = MusicPlayer.Instance.VisibleBeat(Settings.Instance.Machine.Timing) is Beat beat
            ? 0.5f + 0.5f * HighlightPulse.Strike(beat)
            : 1.0f;
        if (highlight is not null)
        {
            var modulate = highlight.Modulate;
            modulate.A = alpha;
            highlight.Modulate = modulate;
        }
    }

    /// <summary>Respawns the wheel rows when the window's visible height changes how many are needed.</summary>
    private void FitWheelRows()
    {
        var rect = Screen.VisibleRect(this);
        int slotsNeeded = ((int)Mathf.Ceil(rect.Size.Y / RowHeight) + 2) | 1;
        if (slotsNeeded == slots.Count)
        {
            return;
        }
        foreach (var slot in slots)
        {
            slot.Root.QueueFree();
        }
        slots = [];
        if (canvas is null || barTexture is null)
        {
            return;
        }
        for (int i = 0; i < slotsNeeded; i++)
        {
            slots.Add(SpawnSlot(i, slotsNeeded));
        }
        dirty = true;
        MarkSettled();
    }

    private SlotUi SpawnSlot(int index, int slotsCount)
    {
        var root = new Node2D
        {
            Position = new Vector2(SlotX(index, slotsCount, 0.0f), -SlotY(index, slotsCount, 0.0f)),
            ZIndex = 100,
        };
        var bar = new Sprite2D
        {
            Texture = barTexture,
            Scale = new Vector2(BarWidth / 512.0f, BarHeight / 64.0f),
            Modulate = StepfileBarColor,
        };
        root.AddChild(bar);
        var title = Text.Label("", 26.0f, StepfileTextColor);
        root.AddChild(title);
        var artist = Text.Label("", 17.0f, ArtistTextColor);
        root.AddChild(artist);
        var ratings = new PerPlayer<RatingUi>(RatingUi.Spawn(root, PlayerId.P1), RatingUi.Spawn(root, PlayerId.P2));
        canvas!.AddChild(root);
        return new SlotUi { Root = root, Bar = bar, Title = title, Artist = artist, Ratings = ratings };
    }

    private void RefreshWheelRows()
    {
        if (!dirty)
        {
            return;
        }
        int center = slots.Count / 2;
        for (int index = 0; index < slots.Count; index++)
        {
            var slot = slots[index];
            switch (EntryAt(index))
            {
                case WheelEntry.Group group:
                    slot.Bar.Modulate = GroupBarColor;
                    slot.Title.Text = Library.Instance.Groups[group.Index].Name;
                    slot.Title.AddThemeColorOverride("font_color", GroupTextColor);
                    Text.Place(slot.Title, new Vector2(-BarWidth / 2.0f + 26.0f, 0.0f), TextPivot.CenterLeft);
                    slot.Artist.Text = "";
                    break;

                case WheelEntry.Stepfile row:
                    var entry = Library.Instance.Stepfile(row.Id);
                    var artist = entry.DisplayArtist();
                    slot.Bar.Modulate = StepfileBarColor;
                    slot.Title.Text = entry.DisplayTitle();
                    slot.Title.AddThemeColorOverride("font_color", index == center ? ActiveStepfileTextColor : StepfileTextColor);
                    Text.Place(slot.Title, new Vector2(-BarWidth / 2.0f + 26.0f, artist.Length > 0 ? -9.0f : 0.0f), TextPivot.CenterLeft);
                    slot.Artist.Text = artist.Length > 0 ? $"/ {artist}" : "";
                    Text.Place(slot.Artist, new Vector2(-BarWidth / 2.0f + 60.0f, 15.0f), TextPivot.CenterLeft);
                    break;

                default:
                    slot.Bar.Modulate = StepfileBarColor;
                    slot.Title.Text = "";
                    slot.Artist.Text = "";
                    break;
            }
        }
    }

    /// <summary>The chart this stepfile would play `player`, honoring their preferred difficulty.</summary>
    private int? ChartFor(Stepfile stepfile, PlayerId player) =>
        stepfile.ClosestChart(stepsType, Game.Instance.PreferredDifficulty[player]);

    private static float SlotY(int slot, int slotsTotal, float offset) =>
        ((slotsTotal / 2) - slot + offset) * RowHeight;

    private static float SlotX(int slot, int slotsTotal, float offset)
    {
        var rowsFromCenter = (slotsTotal / 2) - slot + offset;
        return WheelX + BulgePerRow * rowsFromCenter * rowsFromCenter;
    }

    /// <summary>
    /// A white vertical-gradient rounded rectangle for sprites to tint: every
    /// bar and panel in this scene, and — with a hollow border — the active-row
    /// frame, whose interior fades to a faint wash so the rows beneath stay
    /// readable. Generated at the exact size it is drawn so edges and ring stay
    /// uniformly thick.
    /// </summary>
    private static ImageTexture RoundedTexture(uint width, uint height, float radius, float? hollowBorder)
    {
        const float InteriorWash = 0.18f;
        var image = Image.CreateEmpty((int)width, (int)height, false, Image.Format.Rgba8);

        for (uint y = 0; y < height; y++)
        {
            var brightness = 255.0f - 130.0f * (y / (float)(height - 1));
            for (uint x = 0; x < width; x++)
            {
                var toEdgeX = Mathf.Abs(x + 0.5f - width / 2.0f) - (width / 2.0f - radius);
                var toEdgeY = Mathf.Abs(y + 0.5f - height / 2.0f) - (height / 2.0f - radius);
                var distance = new Vector2(Mathf.Max(toEdgeX, 0.0f), Mathf.Max(toEdgeY, 0.0f)).Length() - radius;

                var alpha = Mathf.Clamp(0.5f - distance, 0.0f, 1.0f);
                if (hollowBorder is float border)
                {
                    var interior = Mathf.Clamp(-distance - border, 0.0f, 1.0f);
                    alpha *= 1.0f - interior * (1.0f - InteriorWash);
                }

                var channel = brightness / 255.0f;
                image.SetPixel((int)x, (int)y, new Color(channel, channel, channel, alpha));
            }
        }

        return ImageTexture.CreateFromImage(image);
    }
}
