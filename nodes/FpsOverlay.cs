using Godot;

namespace Rhythm;

/// <summary>Options for the FPS overlay.</summary>
public class FpsOverlayOptions
{
    public required Color Fg { get; init; }
    public required Color Bg { get; init; }
    public required float EdgePadding { get; init; }
}

/// <summary>
/// A frame-rate meter pinned to the bottom-right corner: the current FPS and
/// its observed range as text above a scrolling histogram of recent frames.
/// Hidden until the ToggleFps action shows it; the node listens for the
/// toggle itself.
/// </summary>
[GlobalClass]
public partial class FpsOverlay : Control
{
    private const float ReadoutSize = 13.0f;
    private const float PanelPadding = 4.0f;
    private const float GraphWidth = 120.0f;
    private const float GraphHeight = 34.0f;
    private const int Columns = 96;

    private FpsHistory history = new();
    private float smoothed;
    private Label? readout;
    private ShaderMaterial? graph;

    public static FpsOverlay Instantiate(FpsOverlayOptions opt)
    {
        var overlay = new FpsOverlay();
        overlay.SetAnchorsAndOffsetsPreset(Control.LayoutPreset.FullRect);
        overlay.Visible = false;
        overlay.MouseFilter = MouseFilterEnum.Ignore;

        var panel = new ColorRect { Color = opt.Bg };
        var column = new VBoxContainer();
        column.AddThemeConstantOverride("separation", 3);

        var readoutLabel = Text.Label("", ReadoutSize, opt.Fg);
        column.AddChild(readoutLabel);

        var shader = GD.Load<Shader>("res://nodes/fps_overlay.gdshader");
        var material = new ShaderMaterial { Shader = shader };
        material.SetShaderParameter("fg", opt.Fg);
        material.SetShaderParameter("bg", opt.Bg);

        var graph = new ColorRect
        {
            Material = material,
            CustomMinimumSize = new Vector2(GraphWidth, GraphHeight)
        };
        column.AddChild(graph);

        panel.AddChild(column);
        column.Position = new Vector2(PanelPadding, PanelPadding);
        overlay.AddChild(panel);

        var panelSize = new Vector2(
            GraphWidth + PanelPadding * 2.0f,
            ReadoutSize + 8.0f + GraphHeight + PanelPadding * 2.0f
        );
        panel.SetAnchorsPreset(Control.LayoutPreset.BottomRight);
        panel.SetOffset(Side.Left, -panelSize.X - opt.EdgePadding);
        panel.SetOffset(Side.Top, -panelSize.Y - opt.EdgePadding);
        panel.SetOffset(Side.Right, -opt.EdgePadding);
        panel.SetOffset(Side.Bottom, -opt.EdgePadding);

        overlay.readout = readoutLabel;
        overlay.graph = material;

        return overlay;
    }

    public override void _Process(double delta)
    {
        if (Actions.JustPressed(Rhythm.Core.GameAction.ToggleFps))
        {
            Visible = !Visible;
        }

        if (!Visible || delta <= 0.0)
        {
            return;
        }

        var fps = (float)(1.0 / delta);
        smoothed += (fps - smoothed) * (smoothed == 0.0f ? 1.0f : 0.1f);
        history.Push(fps);

        var (low, high) = history.Range() ?? (fps, fps);

        if (readout is not null)
        {
            readout.Text = $"{smoothed:F0} FPS ({low:F0}-{high:F0})";
        }

        if (graph is not null)
        {
            var samples = new Godot.Collections.Array();
            foreach (var sample in history.Normalized())
            {
                samples.Add(sample);
            }
            graph.SetShaderParameter("samples", samples);
        }
    }

    private struct FpsHistory
    {
        private float[] ring;
        private int next;

        public FpsHistory()
        {
            ring = new float[Columns];
            next = 0;
        }

        public void Push(float fps)
        {
            ring[next] = fps;
            next = (next + 1) % Columns;
        }

        public float[] Normalized()
        {
            var peak = 0.0f;
            foreach (var f in ring)
            {
                if (f > peak)
                    peak = f;
            }
            peak = Mathf.Max(peak, 1.0f);

            var samples = new float[Columns];
            for (int i = 0; i < Columns; i++)
            {
                samples[i] = Mathf.Clamp(ring[(next + i) % Columns] / peak, 0.0f, 1.0f);
            }
            return samples;
        }

        public (float, float)? Range()
        {
            float? first = null;
            float low = float.MaxValue;
            float high = float.MinValue;

            foreach (var fps in ring)
            {
                if (fps > 0.0f)
                {
                    first ??= fps;
                    if (fps < low) low = fps;
                    if (fps > high) high = fps;
                }
            }

            return first is null ? null : (low, high);
        }
    }
}
