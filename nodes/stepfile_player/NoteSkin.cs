using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// One loaded note skin. Everything a skin draws inside the note field is
/// unshaded 3D: either the skin's own mesh (model skins) or a shared unit
/// quad textured through an unshaded material (sprite skins) — receptors,
/// notes, holds, mines, flashes, and explosions are all geometry in the
/// lane's own scene, so a lane camera's perspective applies to every one of
/// them consistently.
/// </summary>
public sealed class NoteSkin
{
    /// <summary>
    /// Skin art is authored on a 64px arrow cell — sprite frames are 64px
    /// squares and model meshes span ±32 units — and fields scale it to
    /// their arrow size.
    /// </summary>
    public const float NoteCell = 64.0f;

    private readonly QuadMesh quad;

    internal NoteSkin(string name, NoteArt note, ReceptorArt receptor, HoldArt hold, HoldArt roll,
        MineArt mine, double mineSpinBeats, SpriteArt mineExplosion, SpriteArt flashBright,
        SpriteArt flashDim, float droppedBrightness, QuadMesh quad)
    {
        Name = name;
        Note = note;
        Receptor = receptor;
        Hold = hold;
        Roll = roll;
        Mine = mine;
        MineSpinBeats = mineSpinBeats;
        MineExplosion = mineExplosion;
        FlashBright = flashBright;
        FlashDim = flashDim;
        DroppedBrightness = droppedBrightness;
        this.quad = quad;
    }

    public string Name { get; }
    public NoteArt Note { get; }
    public ReceptorArt Receptor { get; }
    public HoldArt Hold { get; }
    public HoldArt Roll { get; }
    public MineArt Mine { get; }
    public double MineSpinBeats { get; }
    public SpriteArt MineExplosion { get; }
    public SpriteArt FlashBright { get; }
    public SpriteArt FlashDim { get; }

    /// <summary>Brightness of dropped (NG) hold parts.</summary>
    public float DroppedBrightness { get; }

    public static NoteSkin Load(string name) => NoteSkinLoader.Load(name);

    public ElementVisual TapVisual(int row) =>
        Note switch
        {
            NoteArt.Sheet sheet => QuadVisual(sheet.Notes.Rows[row].Tap),
            NoteArt.Model model => model.Notes.Visual(row),
            _ => throw new InvalidOperationException("unreachable note art"),
        };

    /// <summary>Model skins draw hold heads with the tap model in both states.</summary>
    public ElementVisual HeadVisual(int row, bool active) =>
        Note switch
        {
            NoteArt.Sheet sheet => QuadVisual(sheet.Notes.Rows[row].Head(active)),
            NoteArt.Model model => model.Notes.Visual(row),
            _ => throw new InvalidOperationException("unreachable note art"),
        };

    public ElementVisual MineVisual() =>
        Mine switch
        {
            MineArt.Sheet sheet => QuadVisual(sheet.Material),
            MineArt.Model model => new ElementVisual(model.Art.Mesh, model.Art.Material, model.Art.Shell, NoteCell),
            _ => throw new InvalidOperationException("unreachable mine art"),
        };

    public ElementVisual ReceptorVisual() => QuadVisual(Receptor.Material);

    public ElementVisual QuadVisual(StandardMaterial3D material) => new(quad, material, null, 1.0f);

    /// <summary>
    /// Materials whose texture coordinates drift over time, with their base
    /// offset and velocity in UV per second.
    /// </summary>
    public IReadOnlyList<(StandardMaterial3D Material, Vector2 Offset, Vector2 Velocity)> ScrollingMaterials()
    {
        var scrolling = new List<(StandardMaterial3D, Vector2, Vector2)>();
        if (Note is NoteArt.Model model)
        {
            foreach (var row in model.Notes.Rows)
            {
                scrolling.Add((row.Material, new Vector2(row.UvOffset, 0.0f), model.Notes.UvScroll));
            }
        }

        if (Mine is MineArt.Model mineModel)
        {
            scrolling.Add((mineModel.Art.Material, Vector2.Zero, mineModel.Art.UvScroll));
        }

        return scrolling;
    }
}

/// <summary>
/// One drawable note-field element: a mesh (an arrow model or the shared unit
/// quad) and its unshaded material. <see cref="Cell"/> is the mesh's authored
/// size for one arrow cell — display scale is <c>arrow_size / cell</c>.
/// </summary>
public sealed record ElementVisual(Mesh Mesh, StandardMaterial3D Material, StandardMaterial3D? Shell, float Cell);

/// <summary>
/// The note art: a sprite sheet animated by sliding the materials' texture
/// coordinates frame by frame, or a 3D model whose per-quant color comes from
/// a texture-coordinate offset.
/// </summary>
public abstract record NoteArt
{
    public sealed record Sheet(SheetNotes Notes) : NoteArt;

    public sealed record Model(ModelNotes Notes) : NoteArt;

    /// <summary>The skin row for a quant; unknown quants use the last (finest) row.</summary>
    public int QuantRow(uint quant) =>
        this switch
        {
            Sheet sheet => Index(sheet.Notes.Quants, quant) ?? (sheet.Notes.Quants.Count - 1),
            Model model => Row(model.Notes.Rows, quant),
            _ => throw new InvalidOperationException("unreachable note art"),
        };

    private static int Row(IReadOnlyList<ModelRow> rows, uint quant)
    {
        for (var i = 0; i < rows.Count; i++)
        {
            if (rows[i].Quant == quant)
            {
                return i;
            }
        }

        return rows.Count - 1;
    }

    private static int? Index(IReadOnlyList<uint> quants, uint quant)
    {
        for (var i = 0; i < quants.Count; i++)
        {
            if (quants[i] == quant)
            {
                return i;
            }
        }

        return null;
    }
}

/// <summary>
/// Sprite-sheet notes: the taps texture is a grid with one row per quant and
/// <c>frames</c> animation columns; hold-head strips hold one frame per quant.
/// Every tap of a quant shares one material, animated in unison by sliding its
/// texture coordinates.
/// </summary>
public sealed class SheetNotes(double beatsPerCycle, int frames, IReadOnlyList<uint> quants, IReadOnlyList<SheetRow> rows)
{
    public double BeatsPerCycle { get; } = beatsPerCycle;
    public IReadOnlyList<uint> Quants { get; } = quants;
    public IReadOnlyList<SheetRow> Rows { get; } = rows;

    /// <summary>The texture-coordinate x of the frame shown at <paramref name="beat"/>, for every row material.</summary>
    public float FrameXAt(Beat beat)
    {
        var cycle = RemEuclid(beat.Value, BeatsPerCycle) / BeatsPerCycle;
        var frame = Math.Min((int)(cycle * frames), frames - 1);
        return (float)frame / frames;
    }

    public IEnumerable<StandardMaterial3D> TapMaterials() => Rows.Select(row => row.Tap);

    internal static double RemEuclid(double value, double modulus)
    {
        var result = value % modulus;
        return result < 0.0 ? result + modulus : result;
    }
}

public sealed class SheetRow(StandardMaterial3D tap, StandardMaterial3D headInactive, StandardMaterial3D headActive)
{
    public StandardMaterial3D Tap { get; } = tap;

    public StandardMaterial3D Head(bool active) => active ? headActive : headInactive;
}

/// <summary>
/// Model notes: one shared arrow mesh, one unshaded material per quant — all
/// pointing into the same texture, apart by <c>uv_offset</c> — plus the
/// texture-static shell every quant shares (the mesh's second surface).
/// </summary>
public sealed class ModelNotes(Mesh mesh, StandardMaterial3D? shell, Vector2 uvScroll, IReadOnlyList<ModelRow> rows)
{
    public Vector2 UvScroll { get; } = uvScroll;
    public IReadOnlyList<ModelRow> Rows { get; } = rows;

    public ElementVisual Visual(int row) => new(mesh, Rows[row].Material, shell, NoteSkin.NoteCell);
}

public sealed record ModelRow(uint Quant, float UvOffset, StandardMaterial3D Material);

/// <summary>
/// The receptor's animation strip: frames cycle on the beat clock, each shown
/// for its duration in beats, optionally pulsing brightness on the beat.
/// </summary>
public sealed class ReceptorArt(StandardMaterial3D material, IReadOnlyList<double> frames, double cycle, float? pulse)
{
    public StandardMaterial3D Material { get; } = material;

    /// <summary>The texture-coordinate x of the frame shown at <paramref name="beat"/>.</summary>
    public float FrameXAt(Beat beat)
    {
        var intoCycle = SheetNotes.RemEuclid(beat.Value, cycle);
        var frame = 0;
        for (var index = 0; index < frames.Count; index++)
        {
            if (intoCycle < frames[index])
            {
                frame = index;
                break;
            }

            intoCycle -= frames[index];
        }

        return (float)frame / frames.Count;
    }

    /// <summary>Full brightness on the beat, decaying to the pulse floor before the next; constant without a pulse.</summary>
    public float BrightnessAt(Beat beat) =>
        pulse is { } floor ? floor + ((1.0f - floor) * (1.0f - (float)SheetNotes.RemEuclid(beat.Value, 1.0))) : 1.0f;
}

/// <summary>
/// Hold (or roll) tail art. The parts get their own materials at spawn (their
/// texture windows vary per hold), built from these textures.
/// </summary>
public sealed record HoldArt(
    Texture2D BodyActive, Texture2D BodyInactive, Vector2 BodySize, float StopAboveTail,
    Texture2D CapActive, Texture2D CapInactive, Vector2 CapSize);

public abstract record MineArt
{
    public sealed record Sheet(StandardMaterial3D Material) : MineArt;

    public sealed record Model(ModelArt Art) : MineArt;
}

/// <summary>A mesh with a single unshaded material, plus its optional texture-static shell surface material.</summary>
public sealed record ModelArt(Mesh Mesh, StandardMaterial3D Material, StandardMaterial3D? Shell, Vector2 UvScroll);

/// <summary>A 2D sprite texture at its native size — flashes and explosions.</summary>
public sealed record SpriteArt(Texture2D Texture, Vector2 Size);
