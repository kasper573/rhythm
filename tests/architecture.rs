use std::path::Path;

/// `src/core` must be self contained: it may not depend on anything outside
/// of `src/core` except third-party dependencies.
#[test]
fn core_is_self_contained() {
    let core = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/core");
    let mut violations = Vec::new();
    check_dir(&core, &mut violations);
    assert!(
        violations.is_empty(),
        "src/core references code outside src/core:\n{}",
        violations.join("\n")
    );
}

fn check_dir(dir: &Path, violations: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap().filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            check_dir(&path, violations);
            continue;
        }
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        for (index, line) in source.lines().enumerate() {
            if line.contains("crate::scenes") || line.contains("crate::integrations") {
                violations.push(format!("{}:{}: {line}", path.display(), index + 1));
            }
        }
    }
}
