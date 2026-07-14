using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The audio settings scene: one volume slider per bus, edited in place (they
/// live in the machine settings, so changes persist immediately). Up/Down pick
/// a slider, Left/Right adjust it — audible right away on the scene's own music
/// and navigation sounds. The layout is authored in AudioSettings.tscn; this
/// binds the sliders to the volumes and drives them.
/// </summary>
[GlobalClass]
public partial class AudioSettings : Control
{
    private const float VolumeStep = 0.05f;
    private const float SliderWidth = 360.0f;
    private const float SliderHeight = 14.0f;
    private const float SliderPadding = 2.0f;

    private int active;
    private SliderRow[] rows = [];

    private enum VolumeKind
    {
        Master,
        Sfx,
        Music,
    }

    private sealed record SliderRow(VolumeKind Kind, Label Name, ColorRect Fill, Label Value);

    public override void _Ready()
    {
        Scenes.PlayDefaultBgm();
        Scenes.SpawnDefaultBackground(this);
        rows =
        [
            Bind(VolumeKind.Master),
            Bind(VolumeKind.Sfx),
            Bind(VolumeKind.Music),
        ];
        Refresh();
    }

    private SliderRow Bind(VolumeKind kind) => new(
        kind,
        GetNode<Label>($"%{kind}Name"),
        GetNode<ColorRect>($"%{kind}Fill"),
        GetNode<Label>($"%{kind}Value"));

    public override void _Process(double delta)
    {
        if (!Game.Instance.AcceptsInput)
        {
            return;
        }

        HandlePulses();

        if (Actions.JustPressed(GameActions.Cancel(PlayerId.P1)))
        {
            Sfx.Cancel.Play();
            Game.Instance.ChangeScene(GameScene.SettingsMenu);
        }

        Refresh();
    }

    private void HandlePulses()
    {
        foreach (var pulse in NavInput.Instance.Pulses)
        {
            if (pulse.AsStep() is not (PlayerId.P1, var direction))
            {
                continue;
            }
            switch (direction)
            {
                case StepDirection.Up:
                    active = (active + rows.Length - 1) % rows.Length;
                    Sfx.Navigate.Play();
                    break;
                case StepDirection.Down:
                    active = (active + 1) % rows.Length;
                    Sfx.Navigate.Play();
                    break;
                case StepDirection.Left:
                    Adjust(-VolumeStep);
                    break;
                case StepDirection.Right:
                    Adjust(VolumeStep);
                    break;
            }
        }
    }

    private void Adjust(float delta)
    {
        var kind = rows[active].Kind;
        var current = Volume(kind, Settings.Instance.Machine.Volume);
        var stepped = Mathf.Clamp(Mathf.Round((current + delta) * 100.0f) / 100.0f, 0.0f, 1.0f);
        if (stepped == current)
        {
            return;
        }
        Settings.Instance.EditMachine(machine => machine with { Volume = WithVolume(kind, machine.Volume, stepped) });
        Sfx.Navigate.Play();
    }

    private void Refresh()
    {
        var volume = Settings.Instance.Machine.Volume;
        for (int index = 0; index < rows.Length; index++)
        {
            var row = rows[index];
            var color = index == active ? Screen.ActiveColor : Screen.InactiveColor;
            var fraction = Volume(row.Kind, volume);
            row.Name.AddThemeColorOverride("font_color", color);
            row.Value.AddThemeColorOverride("font_color", color);
            row.Fill.Color = color;
            row.Fill.Size = new Vector2((SliderWidth - (2.0f * SliderPadding)) * fraction, SliderHeight - (2.0f * SliderPadding));
            row.Value.Text = $"{fraction * 100.0f:F0}%";
        }
    }

    private static float Volume(VolumeKind kind, VolumeSettings volume) => kind switch
    {
        VolumeKind.Master => volume.Master,
        VolumeKind.Sfx => volume.Sfx,
        VolumeKind.Music => volume.Music,
        _ => 0.0f,
    };

    private static VolumeSettings WithVolume(VolumeKind kind, VolumeSettings volume, float value) => kind switch
    {
        VolumeKind.Master => volume with { Master = value },
        VolumeKind.Sfx => volume with { Sfx = value },
        VolumeKind.Music => volume with { Music = value },
        _ => volume,
    };
}
