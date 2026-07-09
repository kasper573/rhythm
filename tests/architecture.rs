//! Enforces the architecture's hard rule: `src/core` is self-contained.
//! It may depend on itself and third-party crates, never on the rest of
//! the game — any `crate::` path out of `core` fails here.

use std::path::Path;

#[test]
fn core_is_self_contained() {
    let core = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/core");
    let mut violations = Vec::new();
    scan(&core, &mut violations);
    assert!(
        violations.is_empty(),
        "src/core must not reach outside itself:\n{}",
        violations.join("\n")
    );
}

fn scan(dir: &Path, violations: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).expect("source directory is readable") {
        let path = entry.expect("directory entry is readable").path();
        if path.is_dir() {
            scan(&path, violations);
            continue;
        }
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("source file is readable");
        for (index, line) in text.lines().enumerate() {
            for (offset, _) in line.match_indices("crate::") {
                let rest = &line[offset + "crate::".len()..];
                if !rest.starts_with("core") {
                    violations.push(format!("{}:{}: {}", path.display(), index + 1, line.trim()));
                }
            }
        }
    }
}
