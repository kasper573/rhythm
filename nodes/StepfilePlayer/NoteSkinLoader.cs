using System.Text.Json;
using System.Text.Json.Serialization;
using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The materials note-field elements draw with. All are unshaded and
/// alpha-blended, so the art shows exactly as painted; render priorities
/// encode the lane's stacking on top of camera-distance sorting.
/// </summary>
public static class SkinMaterials
{
    public const int LaneFloorPriority = -100;
    public const int LaneTailPriority = -50;
    public const int LaneShellPriority = 1;
    public const int LaneEffectPriority = 100;

    /// <summary>
    /// An unshaded alpha-blended material for a note-field quad or model:
    /// lighting-free so the art shows exactly as painted, blended for smooth
    /// edges and translucency (model meshes ship their triangles ordered
    /// back-to-front, so their layers composite correctly without depth).
    /// </summary>
    public static StandardMaterial3D Unlit(Texture2D texture, Vector2 uvScale, Vector2 uvOffset)
    {
        var material = new StandardMaterial3D
        {
            ShadingMode = BaseMaterial3D.ShadingModeEnum.Unshaded,
            Transparency = BaseMaterial3D.TransparencyEnum.Alpha,
            CullMode = BaseMaterial3D.CullModeEnum.Disabled,
            TextureFilter = BaseMaterial3D.TextureFilterEnum.Linear,
            AlbedoTexture = texture,
            Uv1Scale = new Vector3(uvScale.X, uvScale.Y, 1.0f),
            Uv1Offset = new Vector3(uvOffset.X, uvOffset.Y, 0.0f),
        };
        material.SetFlag(BaseMaterial3D.Flags.AlbedoTextureForceSrgb, false);
        material.SetFlag(BaseMaterial3D.Flags.UseTextureRepeat, true);
        return material;
    }

    /// <summary>The material of a lane effect: tinted, per-entity (its owner animates the alpha), above everything.</summary>
    public static StandardMaterial3D Effect(Texture2D texture, Color color)
    {
        var material = Unlit(texture, Vector2.One, Vector2.Zero);
        material.AlbedoColor = color;
        material.RenderPriority = LaneEffectPriority;
        return material;
    }

    /// <summary>The material of a hold-tail part: per-entity (the hold animator drives its window), under the notes.</summary>
    public static StandardMaterial3D Tail(Texture2D texture)
    {
        var material = Unlit(texture, Vector2.One, Vector2.Zero);
        material.RenderPriority = LaneTailPriority;
        return material;
    }
}

/// <summary>Every note skin found under <c>assets/note_skins</c>, for the options list.</summary>
public static class NoteSkinLibrary
{
    private static IReadOnlyList<NoteSkinEntry>? skins;

    public static IReadOnlyList<NoteSkinEntry> Skins => skins ??= Scan();

    private static List<NoteSkinEntry> Scan()
    {
        var root = Assets.Path("note_skins");
        if (!Directory.Exists(root))
        {
            throw new InvalidOperationException($"failed to read {root}: no note skins found");
        }

        var entries = Directory.EnumerateDirectories(root)
            .Select(dir => new NoteSkinEntry(Path.GetFileName(dir), NoteSkinLoader.ReadManifest(Path.GetFileName(dir)).DisplayName))
            .OrderBy(entry => entry.Name, StringComparer.Ordinal)
            .ToList();
        if (entries.Count == 0)
        {
            throw new InvalidOperationException($"failed to read {root}: no note skins found");
        }

        return entries;
    }
}

public sealed record NoteSkinEntry(string Name, string DisplayName);

/// <summary>Loads a note skin from <c>assets/note_skins/&lt;name&gt;/</c>.</summary>
public static class NoteSkinLoader
{
    private static readonly JsonSerializerOptions ManifestOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        ReadCommentHandling = JsonCommentHandling.Skip,
        AllowTrailingCommas = true,
    };

    /// <summary>Panics on a missing or invalid skin: the requested skin must exist.</summary>
    public static NoteSkin Load(string name)
    {
        var manifest = ReadManifest(name);
        var loader = new SkinAssetLoader(name);
        var quad = new QuadMesh { Size = Vector2.One };

        var note = LoadNote(manifest.Note, loader, name);
        var receptor = LoadReceptor(manifest.Receptor, loader, name);
        var hold = LoadHold(manifest.Hold, loader, name);
        var roll = manifest.Roll is { } rollManifest ? LoadHold(rollManifest, loader, name) : hold;
        var mine = LoadMine(manifest.Mine, loader, name);

        return new NoteSkin(name, note, receptor, hold, roll, mine, manifest.Mine.SpinBeats,
            loader.Sprite(manifest.MineExplosion), loader.Sprite(manifest.Flash.Bright),
            loader.Sprite(manifest.Flash.Dim), manifest.DroppedBrightness, quad);
    }

    internal static Manifest ReadManifest(string name)
    {
        var path = Assets.Path($"note_skins/{name}/manifest.json");
        return JsonSerializer.Deserialize<Manifest>(File.ReadAllText(path), ManifestOptions)
            ?? throw new InvalidOperationException($"invalid {path}");
    }

    private static NoteArt LoadNote(NoteManifest manifest, SkinAssetLoader loader, string name)
    {
        if (manifest.Sheet is { } sheet)
        {
            if (sheet.Quants.Count == 0 || sheet.Frames <= 0 || sheet.BeatsPerCycle <= 0.0)
            {
                throw new InvalidOperationException($"note skin {name}: invalid sheet notes");
            }

            var taps = loader.Texture(sheet.Taps);
            var headInactive = loader.Texture(sheet.HoldHeadInactive);
            var headActive = loader.Texture(sheet.HoldHeadActive);
            var quants = sheet.Quants.Count;
            var rows = new List<SheetRow>();
            for (var row = 0; row < quants; row++)
            {
                var tapScale = new Vector2(1.0f / sheet.Frames, 1.0f / quants);
                var tapOffset = new Vector2(0.0f, (float)row / quants);
                var headScale = new Vector2(1.0f / quants, 1.0f);
                var headOffset = new Vector2((float)row / quants, 0.0f);
                rows.Add(new SheetRow(
                    SkinMaterials.Unlit(taps, tapScale, tapOffset),
                    SkinMaterials.Unlit(headInactive, headScale, headOffset),
                    SkinMaterials.Unlit(headActive, headScale, headOffset)));
            }

            return new NoteArt.Sheet(new SheetNotes(sheet.BeatsPerCycle, sheet.Frames, [.. sheet.Quants], rows));
        }

        return new NoteArt.Model(loader.ModelNotes(manifest.Model ?? throw new InvalidOperationException($"note skin {name}: note art missing"), name));
    }

    private static ReceptorArt LoadReceptor(ReceptorManifest manifest, SkinAssetLoader loader, string name)
    {
        var image = loader.Texture(manifest.Image);
        var cycle = manifest.Frames.Sum();
        if (cycle <= 0.0)
        {
            throw new InvalidOperationException($"note skin {name}: receptor frames must cover a positive beat span");
        }

        var material = SkinMaterials.Unlit(image, new Vector2(1.0f / manifest.Frames.Count, 1.0f), Vector2.Zero);
        material.RenderPriority = SkinMaterials.LaneFloorPriority;
        return new ReceptorArt(material, [.. manifest.Frames], cycle, manifest.Pulse);
    }

    private static HoldArt LoadHold(HoldManifest manifest, SkinAssetLoader loader, string name)
    {
        if (manifest.BodySize.Concat(manifest.CapSize).Any(side => side <= 0.0f))
        {
            throw new InvalidOperationException($"note skin {name}: hold body and cap sizes must be positive");
        }

        return new HoldArt(
            loader.Texture(manifest.BodyActive), loader.Texture(manifest.BodyInactive),
            new Vector2(manifest.BodySize[0], manifest.BodySize[1]), manifest.StopAboveTail,
            loader.Texture(manifest.CapActive), loader.Texture(manifest.CapInactive),
            new Vector2(manifest.CapSize[0], manifest.CapSize[1]));
    }

    private static MineArt LoadMine(MineManifest manifest, SkinAssetLoader loader, string name)
    {
        if (manifest.SpinBeats <= 0.0)
        {
            throw new InvalidOperationException($"note skin {name}: mine spin_beats must be positive");
        }

        if (manifest.Art.Sheet is { } sheet)
        {
            return new MineArt.Sheet(SkinMaterials.Unlit(loader.Texture(sheet.Image), Vector2.One, Vector2.Zero));
        }

        var model = manifest.Art.Model ?? throw new InvalidOperationException($"note skin {name}: mine art missing");
        var texture = loader.Texture(model.Texture);
        return new MineArt.Model(new ModelArt(loader.Mesh(model.Mesh),
            SkinMaterials.Unlit(texture, Vector2.One, Vector2.Zero),
            SkinAssetLoader.Shell(model.StaticShell, texture),
            new Vector2(model.UvScroll[0], model.UvScroll[1])));
    }
}

/// <summary>Loads a skin's asset files, all addressed relative to its folder.</summary>
internal sealed class SkinAssetLoader(string name)
{
    private string Path(string file) => Assets.Path($"note_skins/{name}/{file}");

    public Texture2D Texture(string file)
    {
        var path = Path(file);
        var image = new Image();
        if (image.Load(path) != Error.Ok)
        {
            throw new InvalidOperationException($"note skin {name}: {path} is not a valid image");
        }

        return ImageTexture.CreateFromImage(image);
    }

    public Mesh Mesh(string file)
    {
        var path = Path(file);
        var doc = new GltfDocument();
        var state = new GltfState();
        if (doc.AppendFromFile(path, state) != Error.Ok)
        {
            throw new InvalidOperationException($"note skin {name}: {path} is not a valid glb");
        }

        var scene = doc.GenerateScene(state) ?? throw new InvalidOperationException($"{path} holds no scene");
        var mesh = FindMesh(scene) ?? throw new InvalidOperationException($"{path} holds no mesh");
        scene.Free();
        return mesh;
    }

    /// <summary>The model's texture-static outline/sheen shell — the glb's second surface — as one shared material.</summary>
    public static StandardMaterial3D? Shell(bool staticShell, Texture2D texture)
    {
        if (!staticShell)
        {
            return null;
        }

        var material = SkinMaterials.Unlit(texture, Vector2.One, Vector2.Zero);
        material.RenderPriority = SkinMaterials.LaneShellPriority;
        return material;
    }

    public ModelNotes ModelNotes(ModelManifest model, string skin)
    {
        if (model.QuantOffsets.Count == 0)
        {
            throw new InvalidOperationException($"note skin {skin}: quant_offsets must not be empty");
        }

        var texture = Texture(model.Texture);
        var rows = model.QuantOffsets
            .Select(pair => new ModelRow((uint)pair[0], pair[1], SkinMaterials.Unlit(texture, Vector2.One, new Vector2(pair[1], 0.0f))))
            .ToList();
        return new ModelNotes(Mesh(model.Mesh), Shell(model.StaticShell, texture), new Vector2(model.UvScroll[0], model.UvScroll[1]), rows);
    }

    public SpriteArt Sprite(SpriteManifest manifest) =>
        new(Texture(manifest.Image), new Vector2(manifest.Size[0], manifest.Size[1]));

    private static Mesh? FindMesh(Node node)
    {
        if (node is MeshInstance3D instance && instance.Mesh is { } mesh)
        {
            return mesh;
        }

        foreach (var child in node.GetChildren())
        {
            if (FindMesh(child) is { } found)
            {
                return found;
            }
        }

        return null;
    }
}

internal sealed record Manifest(
    string DisplayName, float DroppedBrightness, NoteManifest Note, ReceptorManifest Receptor,
    HoldManifest Hold, HoldManifest? Roll, MineManifest Mine, SpriteManifest MineExplosion, FlashManifest Flash);

internal sealed record NoteManifest(
    [property: JsonPropertyName("Sheet")] SheetManifest? Sheet,
    [property: JsonPropertyName("Model")] ModelManifest? Model);

internal sealed record SheetManifest(
    string Taps, IReadOnlyList<uint> Quants, int Frames, double BeatsPerCycle,
    string HoldHeadActive, string HoldHeadInactive);

internal sealed record ModelManifest(
    string Mesh, string Texture, IReadOnlyList<float[]> QuantOffsets, float[] UvScroll, bool StaticShell);

internal sealed record ReceptorManifest(string Image, IReadOnlyList<double> Frames, float? Pulse);

internal sealed record HoldManifest(
    string BodyActive, string BodyInactive, float[] BodySize, float StopAboveTail,
    string CapActive, string CapInactive, float[] CapSize);

internal sealed record MineManifest(MineArtManifest Art, double SpinBeats);

internal sealed record MineArtManifest(
    [property: JsonPropertyName("Sheet")] ImageManifest? Sheet,
    [property: JsonPropertyName("Model")] MineModelManifest? Model);

internal sealed record ImageManifest(string Image);

internal sealed record MineModelManifest(string Mesh, string Texture, float[] UvScroll, bool StaticShell);

internal sealed record SpriteManifest(string Image, float[] Size);

internal sealed record FlashManifest(SpriteManifest Bright, SpriteManifest Dim);
