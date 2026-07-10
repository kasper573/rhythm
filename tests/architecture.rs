//! Enforces the architecture's systematically-testable rules:
//!
//! * `src/core` is self-contained: it may reach itself and third-party
//!   crates, never the rest of the game.
//! * Every prefab under `src/prefabs` is an isolated module: it may reach
//!   itself and `core`, never a sibling prefab (compose them or inject via
//!   resources instead) and never `src/scenes`.
//! * Every prefab exports its entry point from its root module:
//!   `pub fn <name>_prefab(opt: <Name>PrefabOptions, ..)` together with the
//!   `<Name>PrefabOptions` struct.
//! * `src/prefabs/mod.rs` stays a pure module index.
//!
//! References are checked as written in the source: `crate::` paths
//! directly, `super::` chains resolved against the file's module path, and
//! bare `scenes::`/`prefabs::` segments wherever they appear (so grouped
//! imports cannot smuggle a forbidden path in).

use std::path::{Path, PathBuf};

#[test]
fn core_is_self_contained() {
    let mut violations = Vec::new();
    for file in rust_files(&src_dir().join("core")) {
        let text = std::fs::read_to_string(&file).expect("source file is readable");
        for (target, line) in referenced_paths(&file, &text) {
            if target.first().map(String::as_str) != Some("core") {
                violations.push(format!(
                    "{}:{line}: crate::{}",
                    file.display(),
                    target.join("::")
                ));
            }
        }
        for (needle, line) in segment_mentions(&text, &["prefabs::", "scenes::"]) {
            violations.push(format!("{}:{line}: {needle}", file.display()));
        }
    }
    assert!(
        violations.is_empty(),
        "src/core must not reach outside itself:\n{}",
        violations.join("\n")
    );
}

#[test]
fn prefabs_are_isolated() {
    let mut violations = Vec::new();
    for (name, root) in prefabs() {
        let files = if root.is_dir() {
            rust_files(&root)
        } else {
            vec![root.clone()]
        };
        for file in files {
            let text = std::fs::read_to_string(&file).expect("source file is readable");
            for (target, line) in referenced_paths(&file, &text) {
                let head = target.first().map(String::as_str);
                let foreign_prefab = head == Some("prefabs")
                    && target.get(1).map(String::as_str) != Some(name.as_str());
                if foreign_prefab || head == Some("scenes") {
                    violations.push(format!(
                        "{}:{line}: crate::{}",
                        file.display(),
                        target.join("::")
                    ));
                }
            }
            for (mention, line) in segment_mentions(&text, &["scenes::"]) {
                violations.push(format!("{}:{line}: {mention}", file.display()));
            }
            for (mention, line) in segment_mentions(&text, &["prefabs::"]) {
                if mention != format!("prefabs::{name}") {
                    violations.push(format!("{}:{line}: {mention}", file.display()));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "prefabs may reach only themselves and core — compose or inject instead:\n{}",
        violations.join("\n")
    );
}

#[test]
fn prefabs_export_their_entry_points() {
    for (name, root) in prefabs() {
        let module = if root.is_dir() {
            root.join("mod.rs")
        } else {
            root
        };
        let text = std::fs::read_to_string(&module).expect("prefab module is readable");
        let condensed: String = text.chars().filter(|char| !char.is_whitespace()).collect();
        let options = format!("{}PrefabOptions", pascal_case(&name));
        for needle in [
            format!("pubfn{name}_prefab(opt:{options}"),
            format!("pubstruct{options}"),
        ] {
            assert!(
                condensed.contains(&needle),
                "{}: prefab {name} must export `pub fn {name}_prefab(opt: {options}, ..)` \
                 and `pub struct {options}`",
                module.display()
            );
        }
    }
}

#[test]
fn prefabs_index_is_pure() {
    let index = src_dir().join("prefabs/mod.rs");
    let text = std::fs::read_to_string(&index).expect("prefab index is readable");
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        let declares_module = line.starts_with("pub mod ") && line.ends_with(';');
        assert!(
            line.is_empty() || line.starts_with("//") || declares_module,
            "{}:{}: prefabs/mod.rs must only declare modules, found: {line}",
            index.display(),
            number + 1
        );
    }
}

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Every prefab as `(name, root)`: a `<name>.rs` file or a `<name>/` folder
/// directly under `src/prefabs`.
fn prefabs() -> Vec<(String, PathBuf)> {
    let dir = src_dir().join("prefabs");
    let mut prefabs = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("src/prefabs exists and is readable") {
        let path = entry.expect("directory entry is readable").path();
        let stem = path
            .file_stem()
            .expect("prefab entries are named")
            .to_string_lossy()
            .into_owned();
        let is_module_file =
            path.extension().is_some_and(|extension| extension == "rs") && stem != "mod";
        if path.is_dir() || is_module_file {
            prefabs.push((stem, path));
        }
    }
    prefabs.sort();
    assert!(
        !prefabs.is_empty(),
        "src/prefabs must hold the game's prefabs"
    );
    prefabs
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).expect("source directory is readable") {
        let path = entry.expect("directory entry is readable").path();
        if path.is_dir() {
            files.extend(rust_files(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    files.sort();
    files
}

/// The file's module path relative to the crate root, e.g.
/// `src/prefabs/foo/bar.rs` -> `["prefabs", "foo", "bar"]`.
fn module_path(file: &Path) -> Vec<String> {
    let mut path: Vec<String> = file
        .strip_prefix(src_dir())
        .expect("source files live under src")
        .with_extension("")
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    if path.last().map(String::as_str) == Some("mod") {
        path.pop();
    }
    path
}

/// Every module path the file references, resolved to crate-absolute form
/// with its line number: `crate::` paths as written, `super::` chains
/// resolved against the file's own module path.
fn referenced_paths(file: &Path, text: &str) -> Vec<(Vec<String>, usize)> {
    let mut paths = Vec::new();
    for (index, _) in text.match_indices("crate::") {
        paths.push((
            segments_at(text, index + "crate::".len()),
            line_of(text, index),
        ));
    }
    for (index, _) in text.match_indices("super::") {
        if text[..index].ends_with("::") {
            continue; // mid-chain; the chain's start already covers it
        }
        let mut base = module_path(file);
        let mut rest = index;
        while text[rest..].starts_with("super::") {
            base.pop();
            rest += "super::".len();
        }
        base.extend(segments_at(text, rest));
        paths.push((base, line_of(text, index)));
    }
    paths
}

/// Occurrences of the given path segments anywhere in the text, with the
/// segment that follows attached, e.g. `("scenes::play", 12)`.
fn segment_mentions(text: &str, segments: &[&str]) -> Vec<(String, usize)> {
    let mut mentions = Vec::new();
    for segment in segments {
        for (index, _) in text.match_indices(segment) {
            let tail = segments_at(text, index + segment.len());
            let mention = match tail.first() {
                Some(next) => format!("{segment}{next}"),
                None => (*segment).to_string(),
            };
            mentions.push((mention, line_of(text, index)));
        }
    }
    mentions
}

/// The `::`-separated identifier segments starting at `start`.
fn segments_at(text: &str, start: usize) -> Vec<String> {
    let mut segments = Vec::new();
    let mut rest = &text[start..];
    loop {
        let end = rest
            .find(|char: char| !char.is_ascii_alphanumeric() && char != '_')
            .unwrap_or(rest.len());
        if end == 0 {
            break;
        }
        segments.push(rest[..end].to_string());
        rest = &rest[end..];
        match rest.strip_prefix("::") {
            Some(after) => rest = after,
            None => break,
        }
    }
    segments
}

fn line_of(text: &str, index: usize) -> usize {
    text[..index].bytes().filter(|byte| *byte == b'\n').count() + 1
}

fn pascal_case(snake: &str) -> String {
    snake
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}
