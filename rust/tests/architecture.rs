//! Enforces the architecture's systematically-testable rules:
//!
//! * `src/core` is self-contained: it may reach itself and third-party
//!   crates, never the rest of the game.
//! * Every custom node under `src/nodes` is an isolated module: it may
//!   reach itself and `core`, never a sibling node (compose them or inject
//!   instead) and never `src/scenes`, `src/game.rs`, or `src/dev`.
//! * Every node exports its entry point from its root module:
//!   `pub fn instantiate(opt: <Name>Options, ..)` together with the
//!   `<Name>Options` struct.
//! * `src/nodes/mod.rs` stays a pure module index.
//!
//! References are checked as written in the source: `crate::` paths
//! directly, `super::` chains resolved against the file's module path, and
//! bare `nodes::`/`scenes::` segments wherever they appear (so grouped
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
        for (needle, line) in segment_mentions(&text, &["nodes::", "scenes::", "game::", "dev::"]) {
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
fn nodes_are_isolated() {
    let mut violations = Vec::new();
    for (name, root) in nodes() {
        let files = if root.is_dir() {
            rust_files(&root)
        } else {
            vec![root.clone()]
        };
        for file in files {
            let text = std::fs::read_to_string(&file).expect("source file is readable");
            for (target, line) in referenced_paths(&file, &text) {
                let head = target.first().map(String::as_str);
                let foreign_node = head == Some("nodes")
                    && target.get(1).map(String::as_str) != Some(name.as_str());
                if foreign_node || matches!(head, Some("scenes") | Some("game") | Some("dev")) {
                    violations.push(format!(
                        "{}:{line}: crate::{}",
                        file.display(),
                        target.join("::")
                    ));
                }
            }
            for (mention, line) in segment_mentions(&text, &["scenes::", "game::", "dev::"]) {
                violations.push(format!("{}:{line}: {mention}", file.display()));
            }
            for (mention, line) in segment_mentions(&text, &["nodes::"]) {
                if mention != format!("nodes::{name}") {
                    violations.push(format!("{}:{line}: {mention}", file.display()));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "nodes may reach only themselves and core — compose or inject instead:\n{}",
        violations.join("\n")
    );
}

#[test]
fn nodes_export_their_entry_points() {
    for (name, root) in nodes() {
        let module = if root.is_dir() {
            root.join("mod.rs")
        } else {
            root
        };
        let text = std::fs::read_to_string(&module).expect("node module is readable");
        let condensed: String = text.chars().filter(|char| !char.is_whitespace()).collect();
        let options = format!("{}Options", pascal_case(&name));
        for needle in [
            format!("pubfninstantiate(opt:{options}"),
            format!("pubstruct{options}"),
        ] {
            assert!(
                condensed.contains(&needle),
                "{}: node {name} must export `pub fn instantiate(opt: {options}, ..)` \
                 and `pub struct {options}`",
                module.display()
            );
        }
    }
}

/// Scenes never reach into each other, with one earned exception: a route
/// param's type lives with the scene that consumes it, so the scene
/// PRODUCING the handoff may name the consumer's module. The allowed
/// targets are derived from `game.rs` itself — exactly the scenes whose
/// params `Game`'s mailboxes carry — so a new cross-scene edge only
/// becomes legal by routing it through `Game`.
#[test]
fn scenes_are_isolated() {
    let scene_names: Vec<String> = units(&src_dir().join("scenes"))
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    let game = src_dir().join("game.rs");
    let game_text = std::fs::read_to_string(&game).expect("game.rs is readable");
    let mailbox_scenes: Vec<String> = referenced_paths(&game, &game_text)
        .into_iter()
        .filter(|(target, _)| target.first().map(String::as_str) == Some("scenes"))
        .filter_map(|(target, _)| target.get(1).cloned())
        .filter(|module| scene_names.contains(module))
        .collect();
    let mut violations = Vec::new();
    for (name, root) in units(&src_dir().join("scenes")) {
        let files = if root.is_dir() {
            rust_files(&root)
        } else {
            vec![root.clone()]
        };
        for file in files {
            let text = std::fs::read_to_string(&file).expect("source file is readable");
            for (target, line) in referenced_paths(&file, &text) {
                let foreign = target.first().map(String::as_str) == Some("scenes")
                    && target.get(1).is_some_and(|module| {
                        *module != name
                            && scene_names.contains(module)
                            && !mailbox_scenes.contains(module)
                    });
                if foreign {
                    violations.push(format!(
                        "{}:{line}: crate::{}",
                        file.display(),
                        target.join("::")
                    ));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "scenes may not reach into other scenes — route params travel through Game:\n{}",
        violations.join("\n")
    );
}

#[test]
fn nodes_index_is_pure() {
    let index = src_dir().join("nodes/mod.rs");
    let text = std::fs::read_to_string(&index).expect("node index is readable");
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        let declares_module = line.starts_with("pub mod ") && line.ends_with(';');
        assert!(
            line.is_empty() || line.starts_with("//") || declares_module,
            "{}:{}: nodes/mod.rs must only declare modules, found: {line}",
            index.display(),
            number + 1
        );
    }
}

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Every custom node as `(name, root)`: a `<name>.rs` file or a `<name>/`
/// folder directly under `src/nodes`.
fn nodes() -> Vec<(String, PathBuf)> {
    let nodes = units(&src_dir().join("nodes"));
    assert!(!nodes.is_empty(), "src/nodes must hold the game's nodes");
    nodes
}

/// Every submodule as `(name, root)`: a `<name>.rs` file or a `<name>/`
/// folder directly under `dir`.
fn units(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut units = Vec::new();
    for entry in std::fs::read_dir(dir).expect("source directory is readable") {
        let path = entry.expect("directory entry is readable").path();
        let stem = path
            .file_stem()
            .expect("module entries are named")
            .to_string_lossy()
            .into_owned();
        let is_module_file =
            path.extension().is_some_and(|extension| extension == "rs") && stem != "mod";
        if path.is_dir() || is_module_file {
            units.push((stem, path));
        }
    }
    units.sort();
    units
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
/// `src/nodes/foo/bar.rs` -> `["nodes", "foo", "bar"]`.
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
