use crate::core::assets::asset_root;
use crate::core::stepfile::Stepfile;
use bevy::prelude::*;
use std::path::{Path, PathBuf};

/// The stepfile library, following the strict layout
/// `assets/stepfiles/<group>/<stepfile>/*.sm`: every stepfile lives in a
/// group, and every stepfile has its own folder holding the .sm and its
/// media. Anything outside this convention is not loaded.
#[derive(Resource, Debug)]
pub struct StepfileLibrary {
    pub groups: Vec<StepfileGroup>,
}

#[derive(Debug)]
pub struct StepfileGroup {
    pub name: String,
    /// The group's banner: `stepfiles/<group>/<group>.<image extension>`.
    pub banner_path: Option<PathBuf>,
    pub stepfiles: Vec<StepfileEntry>,
}

#[derive(Debug)]
pub struct StepfileEntry {
    pub stepfile: Stepfile,
    pub sm_path: PathBuf,
    pub dir: PathBuf,
}

/// Identifies one stepfile in the library by group and position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepfileId {
    pub group: usize,
    pub stepfile: usize,
}

/// Whether a background-change file name refers to a video.
pub fn is_video_file(name: &str) -> bool {
    has_extension(Path::new(name), &["avi", "mpg", "mpeg", "mp4"])
}

impl StepfileLibrary {
    /// Loads the library from `stepfiles/<group>/<stepfile>/*.sm`. Files that
    /// break the convention and stepfiles that fail to parse are skipped with
    /// a logged warning.
    pub fn scan() -> StepfileLibrary {
        let root = asset_root().join("stepfiles");
        let mut groups: Vec<StepfileGroup> = Vec::new();

        let group_dirs = match std::fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(error) => {
                warn!(
                    "cannot read stepfile library at {}: {error}",
                    root.display()
                );
                return StepfileLibrary { groups };
            }
        };

        for group in group_dirs.filter_map(Result::ok) {
            let group_path = group.path();
            if !group_path.is_dir() {
                warn_if_stray_stepfile(&group_path);
                continue;
            }
            let name = group.file_name().to_string_lossy().into_owned();

            let mut stepfiles = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&group_path) {
                for entry in entries.filter_map(Result::ok) {
                    let path = entry.path();
                    if path.is_dir() {
                        stepfiles.extend(load_stepfile_folder(&path));
                    } else {
                        warn_if_stray_stepfile(&path);
                    }
                }
            }
            if stepfiles.is_empty() {
                continue;
            }
            stepfiles.sort_by_key(|entry| entry.display_title().to_lowercase());
            groups.push(StepfileGroup {
                banner_path: group_banner(&group_path, &name),
                name,
                stepfiles,
            });
        }

        groups.sort_by_key(|group| group.name.to_lowercase());
        let total: usize = groups.iter().map(|group| group.stepfiles.len()).sum();
        info!(
            "stepfile library: {total} stepfiles in {} groups",
            groups.len()
        );
        StepfileLibrary { groups }
    }

    pub fn stepfile(&self, id: StepfileId) -> &StepfileEntry {
        &self.groups[id.group].stepfiles[id.stepfile]
    }

    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }
}

impl StepfileEntry {
    pub fn display_title(&self) -> String {
        let stepfile = &self.stepfile;
        // The bundled font has no CJK glyphs, so fall back to the
        // transliterated title when the original wouldn't render.
        let title = preferred_text(&stepfile.title, &stepfile.title_translit);
        let subtitle = preferred_text(&stepfile.subtitle, &stepfile.subtitle_translit);
        if title.is_empty() {
            return self
                .sm_path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| "???".to_string());
        }
        match subtitle.is_empty() {
            true => title.to_string(),
            false => format!("{title} {subtitle}"),
        }
    }

    /// Finds a file in the stepfile's own folder by name, case-insensitively —
    /// simfile tags frequently disagree with the real file's casing.
    pub fn resolve_file(&self, name: &str) -> Option<PathBuf> {
        let direct = self.dir.join(name);
        if direct.exists() {
            return Some(direct);
        }
        let lowered = name.to_lowercase();
        std::fs::read_dir(&self.dir)
            .ok()?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .is_some_and(|file| file.to_string_lossy().to_lowercase() == lowered)
            })
    }

    /// The music file to play: the `#MUSIC` tag if it resolves, otherwise any
    /// audio file in the folder. `None` means this stepfile has no music on
    /// disk and plays silent.
    pub fn music_path(&self) -> Option<PathBuf> {
        if let Some(name) = &self.stepfile.music
            && let Some(path) = self.resolve_file(name)
        {
            return Some(path);
        }
        self.first_file_with_extension(&["mp3", "ogg", "wav", "flac"])
    }

    /// The static background image, if one exists on disk.
    pub fn background_path(&self) -> Option<PathBuf> {
        if let Some(name) = &self.stepfile.background
            && let Some(path) = self.resolve_file(name)
        {
            return Some(path);
        }
        None
    }

    /// The banner image, if one exists on disk.
    pub fn banner_path(&self) -> Option<PathBuf> {
        if let Some(name) = &self.stepfile.banner
            && let Some(path) = self.resolve_file(name)
        {
            return Some(path);
        }
        None
    }

    pub fn display_artist(&self) -> &str {
        preferred_text(&self.stepfile.artist, &self.stepfile.artist_translit)
    }

    fn first_file_with_extension(&self, extensions: &[&str]) -> Option<PathBuf> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&self.dir)
            .ok()?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| has_extension(path, extensions))
            .collect();
        files.sort();
        files.into_iter().next()
    }
}

/// Parses every `*.sm` directly inside one stepfile folder.
fn load_stepfile_folder(dir: &Path) -> Vec<StepfileEntry> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && has_extension(path, &["sm"]))
        .filter_map(|sm_path| match Stepfile::load(&sm_path) {
            Ok(stepfile) => Some(StepfileEntry {
                dir: dir.to_path_buf(),
                sm_path,
                stepfile,
            }),
            Err(error) => {
                warn!("skipping {}: {error}", sm_path.display());
                None
            }
        })
        .collect()
}

/// The group banner is the image file named after the group folder itself:
/// `stepfiles/<group>/<group>.<image extension>`.
fn group_banner(group_path: &Path, group_name: &str) -> Option<PathBuf> {
    let wanted_stem = group_name.to_lowercase();
    std::fs::read_dir(group_path)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.is_file()
                && has_extension(path, IMAGE_EXTENSIONS)
                && path
                    .file_stem()
                    .is_some_and(|stem| stem.to_string_lossy().to_lowercase() == wanted_stem)
        })
}

/// Image formats the renderer can load.
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg"];

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension().is_some_and(|extension| {
        let extension = extension.to_string_lossy().to_lowercase();
        extensions.contains(&extension.as_str())
    })
}

fn warn_if_stray_stepfile(path: &Path) {
    if has_extension(path, &["sm"]) {
        warn!(
            "ignoring {}: stepfiles must live in a stepfiles/<group>/<stepfile>/ folder",
            path.display()
        );
    }
}

fn preferred_text<'a>(original: &'a str, transliterated: &'a str) -> &'a str {
    let unrenderable = original.chars().any(|char| char as u32 >= 0x2E80);
    if unrenderable && !transliterated.is_empty() {
        transliterated
    } else {
        original
    }
}
