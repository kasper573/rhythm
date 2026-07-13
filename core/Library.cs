namespace Rhythm.Core;

/// <summary>
/// The stepfile library, following the strict layout
/// <c>assets/stepfiles/&lt;group&gt;/&lt;stepfile&gt;/*.sm</c>: every stepfile
/// lives in a group, and every stepfile has its own folder holding the .sm
/// and its media. Anything outside this convention is not loaded.
/// </summary>
public sealed class StepfileLibrary
{
    private StepfileLibrary(IReadOnlyList<StepfileGroup> groups, StepfileEntry defaultBgm)
    {
        Groups = groups;
        DefaultBgm = defaultBgm;
    }

    public IReadOnlyList<StepfileGroup> Groups { get; }

    /// <summary>
    /// The fallback background music every scene can play: the vetted
    /// stepfile at <c>assets/default_bgm/</c>, deliberately outside the
    /// wheel's library.
    /// </summary>
    public StepfileEntry DefaultBgm { get; }

    public static StepfileLibrary Scan()
    {
        var defaultBgm = LoadStepfileFolder(Assets.Path("default_bgm")).FirstOrDefault()
            ?? throw new InvalidOperationException(
                "assets/default_bgm must hold a valid .sm: it is the global fallback BGM");

        var root = Assets.Path("stepfiles");
        var groups = new List<StepfileGroup>();
        if (!Directory.Exists(root))
        {
            Console.Error.WriteLine($"no stepfile library at {root}");
            return new StepfileLibrary(groups, defaultBgm);
        }

        foreach (var groupPath in Directory.EnumerateFileSystemEntries(root))
        {
            if (!Directory.Exists(groupPath))
            {
                WarnIfStrayStepfile(groupPath);
                continue;
            }

            var name = System.IO.Path.GetFileName(groupPath);
            var stepfiles = new List<StepfileEntry>();
            foreach (var entry in Directory.EnumerateFileSystemEntries(groupPath))
            {
                if (Directory.Exists(entry))
                {
                    // Chart-less stepfiles are valid as music (the default
                    // BGM is one) but have no place on the wheel.
                    stepfiles.AddRange(LoadStepfileFolder(entry).Where(step => step.Stepfile.Charts.Count > 0));
                }
                else
                {
                    WarnIfStrayStepfile(entry);
                }
            }

            if (stepfiles.Count == 0)
            {
                continue;
            }

            stepfiles.Sort((a, b) => string.CompareOrdinal(a.DisplayTitle().ToLowerInvariant(), b.DisplayTitle().ToLowerInvariant()));
            groups.Add(new StepfileGroup(name, GroupBanner(groupPath, name), stepfiles));
        }

        groups.Sort((a, b) => string.CompareOrdinal(a.Name.ToLowerInvariant(), b.Name.ToLowerInvariant()));
        var total = groups.Sum(group => group.Stepfiles.Count);
        Console.WriteLine($"stepfile library: {total} stepfiles in {groups.Count} groups");
        return new StepfileLibrary(groups, defaultBgm);
    }

    public StepfileEntry Stepfile(StepfileId id) => Groups[id.Group].Stepfiles[id.StepfileIndex];

    /// <summary>The group a stepfile belongs to.</summary>
    public string GroupName(StepfileId id) => Groups[id.Group].Name;

    public bool IsEmpty => Groups.Count == 0;

    private static readonly string[] ImageExtensions = ["png", "jpg", "jpeg"];
    private static readonly string[] MusicExtensions = ["mp3", "ogg", "wav"];

    public static bool IsVideoFile(string name) => HasExtension(name, ["avi", "mpg", "mpeg", "mp4"]);

    private static List<StepfileEntry> LoadStepfileFolder(string dir)
    {
        var entries = new List<StepfileEntry>();
        if (!Directory.Exists(dir))
        {
            return entries;
        }

        foreach (var smPath in Directory.EnumerateFiles(dir).Where(path => HasExtension(path, ["sm"])))
        {
            try
            {
                var stepfile = StepfileParser.Parse(File.ReadAllText(smPath));
                entries.Add(new StepfileEntry(stepfile, smPath, dir));
            }
            catch (Exception error) when (error is StepfileException or IOException)
            {
                Console.Error.WriteLine($"skipping {smPath}: {error.Message}");
            }
        }

        return entries;
    }

    private static string? GroupBanner(string groupPath, string groupName)
    {
        var wantedStem = groupName.ToLowerInvariant();
        return Directory.EnumerateFiles(groupPath)
            .Where(path => HasExtension(path, ImageExtensions) &&
                System.IO.Path.GetFileNameWithoutExtension(path).ToLowerInvariant() == wantedStem)
            .FirstOrDefault();
    }

    private static void WarnIfStrayStepfile(string path)
    {
        if (HasExtension(path, ["sm"]))
        {
            Console.Error.WriteLine(
                $"ignoring {path}: stepfiles must live in a stepfiles/<group>/<stepfile>/ folder");
        }
    }

    internal static bool HasExtension(string path, IReadOnlyList<string> extensions)
    {
        var extension = System.IO.Path.GetExtension(path).TrimStart('.').ToLowerInvariant();
        return extensions.Contains(extension);
    }

    internal static string[] MusicFallbackExtensions => MusicExtensions;
}

public sealed record StepfileGroup(string Name, string? BannerPath, IReadOnlyList<StepfileEntry> Stepfiles);

public readonly record struct StepfileId(int Group, int StepfileIndex);

/// <summary>Background music a scene hands the music player: always a real stepfile.</summary>
public sealed record Bgm(string SmPath, Stepfile Stepfile, string? Music);

/// <summary>One stepfile in the library: its parsed data and its folder on disk.</summary>
public sealed class StepfileEntry(Stepfile stepfile, string smPath, string dir)
{
    public Stepfile Stepfile { get; } = stepfile;
    public string SmPath { get; } = smPath;

    /// <summary>This stepfile as background music for the music player.</summary>
    public Bgm Bgm() => new(SmPath, Stepfile, MusicPath());

    /// <summary>The stepfile's own name: its .sm file name without the extension.</summary>
    public string Name() => System.IO.Path.GetFileNameWithoutExtension(SmPath);

    public string DisplayTitle()
    {
        var title = PreferredText(Stepfile.Title, Stepfile.TitleTranslit);
        var subtitle = PreferredText(Stepfile.Subtitle, Stepfile.SubtitleTranslit);
        if (title.Length == 0)
        {
            var stem = System.IO.Path.GetFileNameWithoutExtension(SmPath);
            return stem.Length > 0 ? stem : "???";
        }

        return subtitle.Length == 0 ? title : $"{title} {subtitle}";
    }

    public string DisplayArtist() => PreferredText(Stepfile.Artist, Stepfile.ArtistTranslit);

    /// <summary>
    /// Finds a file in the stepfile's own folder by name, case-insensitively —
    /// simfile tags frequently disagree with the real file's casing.
    /// </summary>
    public string? ResolveFile(string name)
    {
        var direct = System.IO.Path.Combine(dir, name);
        if (File.Exists(direct))
        {
            return direct;
        }

        var lowered = name.ToLowerInvariant();
        return Directory.EnumerateFileSystemEntries(dir)
            .FirstOrDefault(path => System.IO.Path.GetFileName(path).ToLowerInvariant() == lowered);
    }

    public string? MusicPath()
    {
        if (Stepfile.Music is { } name && ResolveFile(name) is { } path)
        {
            return path;
        }

        return FirstFileWithExtension(StepfileLibrary.MusicFallbackExtensions);
    }

    public string? BackgroundPath() =>
        Stepfile.Background is { } name ? ResolveFile(name) : null;

    public string? BannerPath() =>
        Stepfile.Banner is { } name ? ResolveFile(name) : null;

    private string? FirstFileWithExtension(IReadOnlyList<string> extensions) =>
        Directory.EnumerateFiles(dir)
            .Where(path => StepfileLibrary.HasExtension(path, extensions))
            .OrderBy(path => path, StringComparer.Ordinal)
            .FirstOrDefault();

    /// <summary>
    /// Prefers the transliterated variant over a CJK original, so the
    /// library's displayed names read and sort consistently in one script.
    /// </summary>
    private static string PreferredText(string original, string transliterated)
    {
        var cjk = original.Any(c => c >= 0x2E80);
        return cjk && transliterated.Length > 0 ? transliterated : original;
    }
}
