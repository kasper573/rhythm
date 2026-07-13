using Godot;
using Rhythm.Core;

namespace Rhythm;

public record WheelEntry(bool IsGroup, int GroupIndex, StepfileId StepfileId);

/// <summary>
/// The stepfile browser: a vertical wheel of every stepfile in every group.
/// Navigation via Up/Down scrolls the wheel; Left/Right changes difficulty.
/// Select chooses a stepfile; hold Select opens player options.
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

    private static readonly Color StepfileBarColor = new(0.10f, 0.19f, 0.07f);
    private static readonly Color GroupBarColor = new(0.055f, 0.10f, 0.045f);
    private static readonly Color StepfileTextColor = new(0.35f, 0.95f, 0.4f);
    private static readonly Color ActiveStepfileTextColor = new(0.8f, 1.0f, 0.75f);
    private static readonly Color GroupTextColor = new(0.95f, 0.55f, 0.15f);
    private static readonly Color ArtistTextColor = new(0.25f, 0.75f, 0.35f);

    private Node2D? canvas;
    private ImageTexture? barTexture;
    private readonly List<(Sprite2D Bar, Label Title, Label Artist, Node2D Root)> slots = [];
    private readonly List<WheelEntry> entries = [];
    private int active;
    private float scrollOffset;
    private float settleTimer;
    private bool justSettled;
    private float selectHoldTimer;
    private bool selectHoldArmed;

    public override void _Ready()
    {
        SetAnchorsAndOffsetsPreset(LayoutPreset.FullRect);
        InitializeCanvas();
        GenerateBarTexture();
        RebuildWheel();
    }

    public override void _Process(double delta)
    {
        if (Game.Instance.AcceptsInput)
        {
            Navigate();
            TrackSelect(delta);
            HandleCancel();
        }

        AnimateWheel(delta);
        SettleWheel(delta);
        RefreshWheelRows();
    }

    public override void _ExitTree()
    {
        MusicPlayer.Instance.Stop();
    }

    private void InitializeCanvas()
    {
        canvas = new Node2D { ZIndex = 0 };
        AddChild(canvas);
    }

    private void GenerateBarTexture()
    {
        var image = Image.CreateEmpty(512, 64, false, Image.Format.Rgba8);
        for (var y = 0; y < 64; y++)
        {
            for (var x = 0; x < 512; x++)
            {
                image.SetPixel(x, y, Colors.White);
            }
        }

        barTexture = ImageTexture.CreateFromImage(image);
    }

    private void RebuildWheel()
    {
        entries.Clear();
        var library = Library.Instance;
        var playMode = Game.Instance.PlayMode;
        var stepsType = playMode == PlayMode.Doubles ? StepsType.DanceDouble : StepsType.DanceSingle;

        // Build entry list: groups and their stepfiles
        for (var groupIndex = 0; groupIndex < library.Groups.Count; groupIndex++)
        {
            var group = library.Groups[groupIndex];
            var playableStepfiles = new List<int>();

            for (var stepfileIndex = 0; stepfileIndex < group.Stepfiles.Count; stepfileIndex++)
            {
                var stepfile = group.Stepfiles[stepfileIndex].Stepfile;
                var playableCharts = stepfile.PlayableCharts(stepsType);
                if (playableCharts.Count > 0)
                {
                    playableStepfiles.Add(stepfileIndex);
                }
            }

            if (playableStepfiles.Count == 0)
            {
                continue;
            }

            // Add group header
            entries.Add(new WheelEntry(IsGroup: true, GroupIndex: groupIndex, StepfileId: default));

            // Add stepfiles
            foreach (var stepfileIndex in playableStepfiles)
            {
                entries.Add(new WheelEntry(
                    IsGroup: false,
                    GroupIndex: groupIndex,
                    StepfileId: new StepfileId(groupIndex, stepfileIndex)
                ));
            }
        }

        // Set active to first entry, or use wheel target
        var wheelTarget = Game.Instance.TakeWheelTarget();
        if (wheelTarget is not null)
        {
            active = 0;
            for (var i = 0; i < entries.Count; i++)
            {
                if (!entries[i].IsGroup && entries[i].StepfileId == wheelTarget)
                {
                    active = i;
                    break;
                }
            }
        }
        else
        {
            active = 0;
        }

        FitWheelRows();
        scrollOffset = 0.0f;
        settleTimer = 0.0f;
        justSettled = true;
    }

    private void FitWheelRows()
    {
        var rect = Screen.VisibleRect(this);
        var slotsNeeded = Mathf.RoundToInt(Mathf.Ceil(rect.Size.Y / RowHeight)) + 2;
        slotsNeeded |= 1; // Force odd

        if (slots.Count == slotsNeeded)
        {
            return;
        }

        foreach (var (bar, title, artist, root) in slots)
        {
            root.QueueFree();
        }

        slots.Clear();

        if (canvas is null || barTexture is null)
        {
            return;
        }

        for (var i = 0; i < slotsNeeded; i++)
        {
            SpawnSlot(i, slotsNeeded);
        }
    }

    private void SpawnSlot(int index, int slotsCount)
    {
        if (canvas is null || barTexture is null)
        {
            return;
        }

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

        var title = Text.Label(string.Empty, 26.0f, StepfileTextColor);
        root.AddChild(title);

        var artist = Text.Label(string.Empty, 17.0f, ArtistTextColor);
        root.AddChild(artist);

        canvas.AddChild(root);
        slots.Add((bar, title, artist, root));
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

        UpdateSlotPositions();
    }

    private void UpdateSlotPositions()
    {
        for (var index = 0; index < slots.Count; index++)
        {
            var (_, _, _, root) = slots[index];
            root.Position = new Vector2(
                SlotX(index, slots.Count, scrollOffset),
                -SlotY(index, slots.Count, scrollOffset)
            );
        }
    }

    private void SettleWheel(double delta)
    {
        if (settleTimer >= SettleDelaySeconds)
        {
            return;
        }

        settleTimer += (float)delta;
        if (settleTimer >= SettleDelaySeconds)
        {
            justSettled = true;
            DriveWheelBgm();
        }
    }

    private void DriveWheelBgm()
    {
        if (!justSettled)
        {
            return;
        }

        if (active < entries.Count && !entries[active].IsGroup)
        {
            var stepfileEntry = Library.Instance.Stepfile(entries[active].StepfileId);
            MusicPlayer.Instance.Play(stepfileEntry.Bgm());
        }
        else
        {
            MusicPlayer.Instance.Play(Library.Instance.DefaultBgm.Bgm());
        }
    }

    private void RefreshWheelRows()
    {
        var center = slots.Count / 2;
        for (var index = 0; index < slots.Count; index++)
        {
            var entryIndex = active - center + index;
            var entry = (entryIndex >= 0 && entryIndex < entries.Count) ? entries[entryIndex] : null;

            var (bar, title, artist, _) = slots[index];
            bar.Modulate = entry?.IsGroup == true ? GroupBarColor : StepfileBarColor;

            if (entry is null)
            {
                title.Text = string.Empty;
                artist.Text = string.Empty;
                continue;
            }

            if (entry.IsGroup)
            {
                var group = Library.Instance.Groups[entry.GroupIndex];
                title.Text = group.Name;
                title.AddThemeColorOverride("font_color", GroupTextColor);
                Text.Place(title, new Vector2(-BarWidth / 2.0f + 26.0f, 0.0f), TextPivot.CenterLeft);
                artist.Text = string.Empty;
            }
            else
            {
                var stepfile = Library.Instance.Stepfile(entry.StepfileId);
                title.Text = stepfile.DisplayTitle();
                var titleColor = (index == center) ? ActiveStepfileTextColor : StepfileTextColor;
                title.AddThemeColorOverride("font_color", titleColor);
                Text.Place(title, new Vector2(-BarWidth / 2.0f + 26.0f, -9.0f), TextPivot.CenterLeft);

                var displayArtist = stepfile.DisplayArtist();
                artist.Text = displayArtist.Length > 0 ? $"/ {displayArtist}" : string.Empty;
                Text.Place(artist, new Vector2(-BarWidth / 2.0f + 60.0f, 15.0f), TextPivot.CenterLeft);
            }
        }

        justSettled = false;
    }

    private void Navigate()
    {
        foreach (var pulse in NavInput.Instance.Pulses)
        {
            if (pulse.AsStep() is (PlayerId.P1, var direction))
            {
                if (direction == StepDirection.Up)
                {
                    ScrollUp();
                }
                else if (direction == StepDirection.Down)
                {
                    ScrollDown();
                }
            }
        }
    }

    private void ScrollUp()
    {
        if (active > 0)
        {
            active--;
            scrollOffset += 1.0f;
            settleTimer = 0.0f;
            selectHoldTimer = 0.0f;
            selectHoldArmed = false;
            SfxPlayer.Instance.Play(Sfx.Navigate);
        }
    }

    private void ScrollDown()
    {
        if (active < entries.Count - 1)
        {
            active++;
            scrollOffset -= 1.0f;
            settleTimer = 0.0f;
            selectHoldTimer = 0.0f;
            selectHoldArmed = false;
            SfxPlayer.Instance.Play(Sfx.Navigate);
        }
    }

    private void TrackSelect(double delta)
    {
        var selectPressed = Actions.Pressed(GameActions.Select(PlayerId.P1));

        if (selectPressed && !selectHoldArmed)
        {
            selectHoldTimer += (float)delta;
            if (selectHoldTimer >= OptionsHoldSeconds)
            {
                selectHoldArmed = true;
                OpenOptions();
            }
        }
        else if (!selectPressed)
        {
            if (selectHoldArmed && selectHoldTimer >= OptionsHoldSeconds)
            {
                selectHoldArmed = false;
            }
            else if (selectHoldTimer > 0.0f && selectHoldTimer < OptionsHoldSeconds)
            {
                SelectStepfile();
            }

            selectHoldTimer = 0.0f;
        }
    }

    private void SelectStepfile()
    {
        if (active >= entries.Count)
        {
            return;
        }

        var entry = entries[active];
        if (entry.IsGroup)
        {
            SfxPlayer.Instance.Play(Sfx.GroupToggle);
            return;
        }

        var stepfileEntry = Library.Instance.Stepfile(entry.StepfileId);
        var players = GetActivePlayers();
        var playMode = Game.Instance.PlayMode;
        var stepsType = playMode == PlayMode.Doubles ? StepsType.DanceDouble : StepsType.DanceSingle;

        var charts = new List<PlayerChart>();
        foreach (var player in players)
        {
            var closest = stepfileEntry.Stepfile.ClosestChart(stepsType, Game.Instance.PreferredDifficulty[player]);
            if (closest.HasValue)
            {
                charts.Add(new PlayerChart(player, closest.Value));
            }
        }

        var selected = new SelectedStepfile(entry.StepfileId, charts);
        Game.Instance.SetSelectedStepfile(selected);
        SfxPlayer.Instance.Play(Sfx.Select);
        Game.Instance.ChangeScene(GameScene.Play);
    }

    private void OpenOptions()
    {
        SfxPlayer.Instance.Play(Sfx.Select);
        // TODO: implement options modal
    }

    private void HandleCancel()
    {
        if (Actions.JustPressed(GameActions.Cancel(PlayerId.P1)))
        {
            SfxPlayer.Instance.Play(Sfx.Cancel);
            Game.Instance.ChangeScene(GameScene.ModeSelect);
        }
    }

    private List<PlayerId> GetActivePlayers()
    {
        return Game.Instance.PlayMode switch
        {
            PlayMode.Singles => new List<PlayerId> { PlayerId.P1 },
            PlayMode.Doubles => new List<PlayerId> { PlayerId.P1, PlayerId.P2 },
            _ => new List<PlayerId> { PlayerId.P1 },
        };
    }

    private float SlotY(int slot, int slotsTotal, float offset) =>
        ((slotsTotal / 2.0f) - slot - offset) * RowHeight;

    private float SlotX(int slot, int slotsTotal, float offset)
    {
        var rowsFromCenter = ((slotsTotal / 2.0f) - slot - offset);
        return WheelX + BulgePerRow * rowsFromCenter * rowsFromCenter;
    }
}
