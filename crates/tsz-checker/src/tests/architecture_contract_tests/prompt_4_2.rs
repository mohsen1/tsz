use super::*;
// Prompt 4.2 — Dependency Direction Tests
// =============================================================================

/// Helper: recursively walk a directory collecting .rs files (skipping tests/).
fn walk_rs_files_recursive(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name == "tests" {
                continue;
            }
            walk_rs_files_recursive(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

/// CLAUDE.md §4: Binder must not import Solver.
/// The binder produces symbols, scopes, and flow graphs without type computation.
#[test]
fn test_binder_must_not_import_solver() {
    let binder_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tsz-binder/src");

    let mut files = Vec::new();
    walk_rs_files_recursive(&binder_src, &mut files);

    let mut violations = Vec::new();
    for path in files {
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("use tsz_solver") || line.contains("tsz_solver::") {
                violations.push(format!("{}:{}", path.display(), line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Binder must not import Solver (CLAUDE.md §4). Violations:\n  {}",
        violations.join("\n  ")
    );
}

/// CLAUDE.md §4: Emitter must not import Checker internals.
/// The emitter prints/transforms output; no on-the-fly semantic type validation.
#[test]
fn test_emitter_must_not_import_checker_internals() {
    let emitter_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tsz-emitter/src");

    let mut files = Vec::new();
    walk_rs_files_recursive(&emitter_src, &mut files);

    let mut violations = Vec::new();
    for path in files {
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("use tsz_checker") || line.contains("tsz_checker::") {
                violations.push(format!("{}:{}", path.display(), line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Emitter must not import Checker internals (CLAUDE.md §4/§13). Violations:\n  {}",
        violations.join("\n  ")
    );
}

/// CLAUDE.md §4: Scanner must not import downstream crates (Parser/Binder/Checker/Solver).
/// The scanner is the leaf of the pipeline; it only does lexing and string interning.
#[test]
fn test_scanner_must_not_import_downstream_crates() {
    let scanner_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tsz-scanner/src");

    let mut files = Vec::new();
    walk_rs_files_recursive(&scanner_src, &mut files);

    let downstream_crates = [
        "tsz_parser",
        "tsz_binder",
        "tsz_checker",
        "tsz_solver",
        "tsz_emitter",
    ];

    let mut violations = Vec::new();
    for path in files {
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            for crate_name in &downstream_crates {
                if line.contains(&format!("use {crate_name}"))
                    || line.contains(&format!("{crate_name}::"))
                {
                    violations.push(format!(
                        "{}:{}: imports {}",
                        path.display(),
                        line_num + 1,
                        crate_name
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Scanner must not import downstream crates (CLAUDE.md §4/§8). Violations:\n  {}",
        violations.join("\n  ")
    );
}

/// CLAUDE.md §4: Parser must not import Binder/Checker/Solver.
/// The parser produces syntax-only AST; no semantic awareness.
#[test]
fn test_parser_must_not_import_binder_checker_solver() {
    let parser_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tsz-parser/src");

    let mut files = Vec::new();
    walk_rs_files_recursive(&parser_src, &mut files);

    let downstream_crates = ["tsz_binder", "tsz_checker", "tsz_solver", "tsz_emitter"];

    let mut violations = Vec::new();
    for path in files {
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            for crate_name in &downstream_crates {
                if line.contains(&format!("use {crate_name}"))
                    || line.contains(&format!("{crate_name}::"))
                {
                    violations.push(format!(
                        "{}:{}: imports {}",
                        path.display(),
                        line_num + 1,
                        crate_name
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Parser must not import Binder/Checker/Solver (CLAUDE.md §4/§9). Violations:\n  {}",
        violations.join("\n  ")
    );
}

// =============================================================================
