use crate::core::assets::asset_root;
use crate::core::platform::platform;
use crate::core::stepfile::{Bgm, Stepfile};
use bevy::prelude::*;
use std::path::{Path, PathBuf};

/// The stepfile library, following the strict layout
/// `assets/stepfiles/<group>/<stepfile>/*.sm`: every stepfile lives in a
/// group, and every stepfile has its own folder holding the .sm and its
/// media. Anything outside this convention is not loaded.
#[derive(Resource, Debug)]
pub struct StepfileLibrary {
    pub groups: Vec<StepfileGroup>,
    /// The fallback background music every scene can play: the vetted
    /// stepfile at `assets/default_bgm/`, deliberately outside the wheel's
    /// library.
    pub default_bgm: StepfileEntry,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepfileId {
    pub group: usize,
    pub stepfile: usize,
}

pub fn is_video_file(name: &str) -> bool {
    has_extension(Path::new(name), &["avi", "mpg", "mpeg", "mp4"])
}

impl StepfileLibrary {
    pub fn scan() -> StepfileLibrary {
        let default_bgm = load_stepfile_folder(&asset_root().join("default_bgm"))
            .into_iter()
            .next()
            .expect("assets/default_bgm must hold a valid .sm: it is the global fallback BGM");
        let root = asset_root().join("stepfiles");
        let mut groups: Vec<StepfileGroup> = Vec::new();

        let group_dirs = platform().list_asset_dir(&root);
        if group_dirs.is_empty() {
            warn!("no stepfile library at {}", root.display());
            return StepfileLibrary {
                groups,
                default_bgm,
            };
        }

        for group in group_dirs {
            if !group.is_dir {
                warn_if_stray_stepfile(&group.path);
                continue;
            }
            let Some(name) = group.path.file_name() else {
                continue;
            };
            let name = name.to_string_lossy().into_owned();

            let mut stepfiles = Vec::new();
            for entry in platform().list_asset_dir(&group.path) {
                if entry.is_dir {
                    // Chart-less stepfiles are valid as music (the
                    // default BGM is one) but have no place on the wheel.
                    stepfiles.extend(
                        load_stepfile_folder(&entry.path)
                            .into_iter()
                            .filter(|entry| !entry.stepfile.charts.is_empty()),
                    );
                } else {
                    warn_if_stray_stepfile(&entry.path);
                }
            }
            if stepfiles.is_empty() {
                continue;
            }
            stepfiles.sort_by_key(|entry| entry.display_title().to_lowercase());
            groups.push(StepfileGroup {
                banner_path: group_banner(&group.path, &name),
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
        StepfileLibrary {
            groups,
            default_bgm,
        }
    }

    pub fn stepfile(&self, id: StepfileId) -> &StepfileEntry {
        &self.groups[id.group].stepfiles[id.stepfile]
    }

    /// The group a stepfile belongs to.
    pub fn group_name(&self, id: StepfileId) -> &str {
        &self.groups[id.group].name
    }

    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }
}

impl StepfileEntry {
    /// This stepfile as background music for the [`MusicPlayer`](crate::core::stepfile::MusicPlayer).
    pub fn bgm(&self) -> Bgm {
        Bgm {
            sm_path: self.sm_path.clone(),
            stepfile: self.stepfile.clone(),
            music: self.music_path(),
        }
    }

    /// The stepfile's own name: its .sm file name without the extension.
    pub fn name(&self) -> String {
        self.sm_path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    pub fn display_title(&self) -> String {
        let stepfile = &self.stepfile;
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
        if platform().asset_exists(&direct) {
            return Some(direct);
        }
        let lowered = name.to_lowercase();
        platform()
            .list_asset_dir(&self.dir)
            .into_iter()
            .map(|entry| entry.path)
            .find(|path| {
                path.file_name()
                    .is_some_and(|file| file.to_string_lossy().to_lowercase() == lowered)
            })
    }

    pub fn music_path(&self) -> Option<PathBuf> {
        if let Some(name) = &self.stepfile.music
            && let Some(path) = self.resolve_file(name)
        {
            return Some(path);
        }
        self.first_file_with_extension(&["mp3", "ogg", "wav", "flac"])
    }

    pub fn background_path(&self) -> Option<PathBuf> {
        if let Some(name) = &self.stepfile.background
            && let Some(path) = self.resolve_file(name)
        {
            return Some(path);
        }
        None
    }

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
        let mut files: Vec<PathBuf> = platform()
            .list_asset_dir(&self.dir)
            .into_iter()
            .filter(|entry| !entry.is_dir)
            .map(|entry| entry.path)
            .filter(|path| has_extension(path, extensions))
            .collect();
        files.sort();
        files.into_iter().next()
    }
}

fn load_stepfile_folder(dir: &Path) -> Vec<StepfileEntry> {
    platform()
        .list_asset_dir(dir)
        .into_iter()
        .filter(|entry| !entry.is_dir && has_extension(&entry.path, &["sm"]))
        .map(|entry| entry.path)
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

fn group_banner(group_path: &Path, group_name: &str) -> Option<PathBuf> {
    let wanted_stem = group_name.to_lowercase();
    platform()
        .list_asset_dir(group_path)
        .into_iter()
        .find(|entry| {
            !entry.is_dir
                && has_extension(&entry.path, IMAGE_EXTENSIONS)
                && entry
                    .path
                    .file_stem()
                    .is_some_and(|stem| stem.to_string_lossy().to_lowercase() == wanted_stem)
        })
        .map(|entry| entry.path)
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

/// Prefers the transliterated variant over a CJK original, so the library's
/// displayed names read and sort consistently in one script.
fn preferred_text<'a>(original: &'a str, transliterated: &'a str) -> &'a str {
    let cjk = original.chars().any(|char| char as u32 >= 0x2E80);
    if cjk && !transliterated.is_empty() {
        transliterated
    } else {
        original
    }
}
