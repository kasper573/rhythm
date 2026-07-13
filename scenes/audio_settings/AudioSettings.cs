using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The audio settings scene: one volume slider per bus, edited in place
/// (they live in the machine settings, so changes persist immediately).
/// Up/Down pick a slider, Left/Right adjust it — audible right away on the
/// scene's own music and navigation sounds.
/// </summary>
[GlobalClass]
public partial class AudioSettings : Control
{
    private int active = 0;
    private List<SliderRow> rows = [];

    private const float VOLUME_STEP = 0.05f;
    private const float SLIDER_WIDTH = 360.0f;
    private const float SLIDER_HEIGHT = 14.0f;
    private const float SLIDER_PADDING = 2.0f;

    private enum VolumeKind
    {
        Master,
        Sfx,
        Music,
    }

    private class SliderRow
    {
        public required Label Label { get; init; }
        public required ColorRect Fill { get; init; }
        public required Label Value { get; init; }
    }

    public override void _Ready()
    {
        SetAnchorsAndOffsetsPreset(LayoutPreset.FullRect);
        Scenes.PlayDefaultBgm();
        Scenes.SpawnDefaultBackground(this);

        var center = new CenterContainer();
        center.SetAnchorsAndOffsetsPreset(LayoutPreset.FullRect);

        var column = new VBoxContainer();
        column.AddThemeConstantOverride("separation", 20);

        var title = Text.Label("Audio Settings", 52.0f, Screen.TitleColor);
        title.HorizontalAlignment = HorizontalAlignment.Center;
        column.AddChild(title);

        var spacer = new Control();
        spacer.CustomMinimumSize = new Vector2(0.0f, 12.0f);
        column.AddChild(spacer);

        var volume = Settings.Instance.Machine.Volume;
        foreach (var kind in new[] { VolumeKind.Master, VolumeKind.Sfx, VolumeKind.Music })
        {
            var row = new HBoxContainer();
            row.AddThemeConstantOverride("separation", 24);

            var nameCell = new CenterContainer();
            nameCell.CustomMinimumSize = new Vector2(160.0f, 40.0f);
            var name = Text.Label(KindName(kind), 34.0f, Screen.InactiveColor);
            nameCell.AddChild(name);
            row.AddChild(nameCell);

            var track = new ColorRect();
            track.Color = new Color(1.0f, 1.0f, 1.0f, 0.12f);
            track.CustomMinimumSize = new Vector2(SLIDER_WIDTH, SLIDER_HEIGHT);
            track.SizeFlagsVertical = Control.SizeFlags.ShrinkCenter;

            var fill = new ColorRect();
            fill.Color = Screen.InactiveColor;
            fill.Position = new Vector2(SLIDER_PADDING, SLIDER_PADDING);
            fill.Size = FillSize(GetVolume(kind, volume));
            track.AddChild(fill);
            row.AddChild(track);

            var valueCell = new CenterContainer();
            valueCell.CustomMinimumSize = new Vector2(80.0f, 40.0f);
            var value = Text.Label($"{GetVolume(kind, volume) * 100.0f:F0}%", 28.0f, Screen.InactiveColor);
            valueCell.AddChild(value);
            row.AddChild(valueCell);

            column.AddChild(row);
            rows.Add(new SliderRow
            {
                Label = name,
                Fill = fill,
                Value = value,
            });
        }

        center.AddChild(column);
        AddChild(center);
    }

    public override void _Process(double delta)
    {
        if (!Game.Instance.AcceptsInput)
        {
            return;
        }

        HandlePulses();

        if (Actions.JustPressed(GameActions.Cancel(PlayerId.P1)))
        {
            SfxPlayer.Instance.Play(Sfx.Cancel);
            Game.Instance.ChangeScene(GameScene.SettingsMenu);
        }

        Refresh();
    }

    private void HandlePulses()
    {
        foreach (var pulse in NavInput.Instance.Pulses)
        {
            var step = pulse.AsStep();
            if (step is null)
            {
                continue;
            }

            var (player, direction) = step.Value;
            if (player != PlayerId.P1)
            {
                continue;
            }

            switch (direction)
            {
                case StepDirection.Up:
                    active = (active + 2) % 3;
                    SfxPlayer.Instance.Play(Sfx.Navigate);
                    break;
                case StepDirection.Down:
                    active = (active + 1) % 3;
                    SfxPlayer.Instance.Play(Sfx.Navigate);
                    break;
                case StepDirection.Left:
                    {
                        var kind = (VolumeKind)active;
                        var current = GetVolume(kind, Settings.Instance.Machine.Volume);
                        var stepped = Mathf.Round((current - VOLUME_STEP) * 100.0f) / 100.0f;
                        stepped = Mathf.Clamp(stepped, 0.0f, 1.0f);
                        if (stepped != current)
                        {
                            Settings.Instance.EditMachine(m =>
                            {
                                var vol = m.Volume;
                                vol = SetVolume(kind, vol, stepped);
                                return m with { Volume = vol };
                            });
                            SfxPlayer.Instance.Play(Sfx.Navigate);
                        }
                        break;
                    }
                case StepDirection.Right:
                    {
                        var kind = (VolumeKind)active;
                        var current = GetVolume(kind, Settings.Instance.Machine.Volume);
                        var stepped = Mathf.Round((current + VOLUME_STEP) * 100.0f) / 100.0f;
                        stepped = Mathf.Clamp(stepped, 0.0f, 1.0f);
                        if (stepped != current)
                        {
                            Settings.Instance.EditMachine(m =>
                            {
                                var vol = m.Volume;
                                vol = SetVolume(kind, vol, stepped);
                                return m with { Volume = vol };
                            });
                            SfxPlayer.Instance.Play(Sfx.Navigate);
                        }
                        break;
                    }
            }
        }
    }

    private void Refresh()
    {
        var volume = Settings.Instance.Machine.Volume;
        for (int index = 0; index < 3; index++)
        {
            var kind = (VolumeKind)index;
            var color = index == active ? Screen.ActiveColor : Screen.InactiveColor;
            var row = rows[index];
            row.Label.AddThemeColorOverride("font_color", color);
            row.Value.AddThemeColorOverride("font_color", color);
            row.Fill.Color = color;
            row.Fill.Size = FillSize(GetVolume(kind, volume));
            row.Value.Text = $"{GetVolume(kind, volume) * 100.0f:F0}%";
        }
    }

    private static string KindName(VolumeKind kind) => kind switch
    {
        VolumeKind.Master => "Master",
        VolumeKind.Sfx => "SFX",
        VolumeKind.Music => "Music",
        _ => "",
    };

    private static float GetVolume(VolumeKind kind, VolumeSettings volume) => kind switch
    {
        VolumeKind.Master => volume.Master,
        VolumeKind.Sfx => volume.Sfx,
        VolumeKind.Music => volume.Music,
        _ => 0.0f,
    };

    private static VolumeSettings SetVolume(VolumeKind kind, VolumeSettings volume, float value) => kind switch
    {
        VolumeKind.Master => volume with { Master = value },
        VolumeKind.Sfx => volume with { Sfx = value },
        VolumeKind.Music => volume with { Music = value },
        _ => volume,
    };

    private static Vector2 FillSize(float fraction)
    {
        return new Vector2(
            (SLIDER_WIDTH - 2.0f * SLIDER_PADDING) * fraction,
            SLIDER_HEIGHT - 2.0f * SLIDER_PADDING);
    }
}
