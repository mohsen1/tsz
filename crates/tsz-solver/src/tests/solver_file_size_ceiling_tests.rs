use std::fs;
use std::path::Path;

fn walk_rs_files_recursive(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    let entries =
        fs::read_dir(dir).unwrap_or_else(|_| panic!("failed to read directory {}", dir.display()));
    for entry in entries {
        let entry = entry.expect("failed to read directory entry");
        let path = entry.path();
        if path.is_dir() {
            walk_rs_files_recursive(&path, files);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}

/// Path to the checked-in baseline file. The file is parsed by `read_baseline`
/// and rewritten by `update_baseline` (when `TSZ_FILE_SIZE_RATCHET_UPDATE=1`).
fn baseline_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tests/file_size_baselines.txt")
}

/// Read a single ceiling from the baseline file.
fn read_baseline(key: &str) -> usize {
    let path = baseline_path();
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let name = match parts.next() {
            Some(n) => n,
            None => continue,
        };
        if name != key {
            continue;
        }
        let value = parts
            .next()
            .unwrap_or_else(|| panic!("baseline line for {key} has no value"));
        return value
            .parse()
            .unwrap_or_else(|e| panic!("baseline value for {key} not a usize: {e}"));
    }
    panic!("baseline key {key:?} not found in {}", path.display());
}

/// `TSZ_FILE_SIZE_RATCHET_UPDATE=1` mode: rewrite the baseline file with the
/// observed `(key, value)` pairs and skip the failing assertion. The CLAUDE.md
/// pre-existing convention is that ratchet bumps are explicit, so we do not
/// auto-rewrite on every run — only when the developer opts in.
fn ratchet_update_enabled() -> bool {
    std::env::var("TSZ_FILE_SIZE_RATCHET_UPDATE")
        .ok()
        .filter(|v| matches!(v.as_str(), "1" | "true" | "yes" | "on"))
        .is_some()
}

/// Rewrite a single key in the baseline file, preserving comments and
/// surrounding lines. If the key is absent, a panic surfaces — the file ships
/// with all keys defined.
fn write_baseline(key: &str, value: usize) {
    let path = baseline_path();
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    let mut found = false;
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let trimmed = line.split('#').next().unwrap_or("").trim();
        if !trimmed.is_empty() {
            let name = trimmed.split_whitespace().next().unwrap_or("");
            if name == key {
                let pad = if key.len() < 20 { 20 - key.len() } else { 1 };
                out.push_str(&format!("{key}{:pad$}{value}\n", "", pad = pad));
                found = true;
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    if !found {
        panic!(
            "baseline key {key:?} not found in {} during update",
            path.display()
        );
    }
    fs::write(&path, out).unwrap_or_else(|e| panic!("failed to rewrite {}: {e}", path.display()));
}

/// Assert `actual <= ceiling` for `key`. In ratchet-update mode, rewrite the
/// baseline value and pass. The error message points the developer at the
/// override env var.
fn assert_within_ceiling(key: &str, actual: usize, descriptor: &str, oversized: &[String]) {
    let ceiling = read_baseline(key);
    if actual <= ceiling {
        return;
    }
    if ratchet_update_enabled() {
        write_baseline(key, actual);
        // The baseline file diff is the user-visible signal that the bump
        // happened; no extra logging needed (and clippy::print-stderr is
        // denied workspace-wide).
        return;
    }
    panic!(
        "{descriptor} grew to {actual} (baseline: {ceiling}). \
         Either split the file into smaller modules, or — if the growth is \
         intentional — re-run with TSZ_FILE_SIZE_RATCHET_UPDATE=1 to update \
         {} and commit the diff. Current oversized files:\n{}",
        baseline_path().display(),
        oversized.join("\n")
    );
}

fn measure_crate(src_dir: &Path) -> (usize, Vec<String>) {
    let mut files = Vec::new();
    walk_rs_files_recursive(src_dir, &mut files);

    let mut oversized = Vec::new();
    let mut max_lines = 0usize;

    for path in &files {
        let rel = path
            .strip_prefix(src_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if rel.starts_with("tests/") || rel.contains("/test") {
            continue;
        }

        let line_count = match fs::read_to_string(path) {
            Ok(s) => s.lines().count(),
            Err(_) => continue,
        };

        if line_count > max_lines {
            max_lines = line_count;
        }

        if line_count > 2000 {
            oversized.push(format!("  {rel} ({line_count} lines)"));
        }
    }

    (max_lines, oversized)
}

/// Ratchet guard: prevent solver source files from growing beyond
/// maintainability limits. Baseline values live in `file_size_baselines.txt`
/// and can be updated via `TSZ_FILE_SIZE_RATCHET_UPDATE=1`.
#[test]
fn test_solver_file_size_ceiling() {
    let solver_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let (max_lines, oversized) = measure_crate(&solver_src);

    assert_within_ceiling(
        "solver_oversized",
        oversized.len(),
        "Number of solver source files over 2000 LOC",
        &oversized,
    );
    assert_within_ceiling(
        "solver_max_loc",
        max_lines,
        "Largest solver source file",
        &oversized,
    );
}

/// Ratchet guard: prevent the binder crate from growing oversized files.
#[test]

fn test_binder_file_size_ceiling() {
    let binder_src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tsz-binder/src");
    if !binder_src.exists() {
        return;
    }

    let (max_lines, oversized) = measure_crate(&binder_src);

    assert_within_ceiling(
        "binder_oversized",
        oversized.len(),
        "Number of binder source files over 2000 LOC",
        &oversized,
    );
    assert_within_ceiling(
        "binder_max_loc",
        max_lines,
        "Largest binder source file",
        &oversized,
    );
}

/// Ratchet guard: prevent the emitter crate from growing oversized files.
#[test]
fn test_emitter_file_size_ceiling() {
    let emitter_src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tsz-emitter/src");
    if !emitter_src.exists() {
        return;
    }

    let (max_lines, oversized) = measure_crate(&emitter_src);

    assert_within_ceiling(
        "emitter_oversized",
        oversized.len(),
        "Number of emitter source files over 2000 LOC",
        &oversized,
    );
    assert_within_ceiling(
        "emitter_max_loc",
        max_lines,
        "Largest emitter source file",
        &oversized,
    );
}
