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

/// Ratchet guard: prevent solver source files from growing beyond maintainability limits.
///
/// Per CLAUDE.md section 19: "Avoid growth of monolith modules; split before crossing
/// maintainability threshold." While section 12 specifically targets checker files at
/// ~2000 LOC, the same principle applies to solver files.
///
/// This test enforces two ceilings:
/// 1. The total count of files exceeding 2000 LOC must not increase.
/// 2. The maximum line count of any single file must not increase.
///
/// Both ceilings can only shrink as files are split into smaller modules.
#[test]
fn test_solver_file_size_ceiling() {
    let solver_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&solver_src, &mut files);

    let mut oversized = Vec::new();
    let mut max_lines = 0usize;

    for path in &files {
        let rel = path
            .strip_prefix(&solver_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        // Skip test files — they are not subject to the LOC guideline
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

    // Ceiling: number of solver source files exceeding 2000 LOC.
    // This number must only shrink as files are split into smaller modules.
    // Current oversized files (as of 2026-03-24):
    //   operations/generic_call.rs (3488), diagnostics/format.rs (3447),
    //   type_queries/data.rs (3246), operations/constraints.rs (3148),
    //   operations/core.rs (2313), relations/subtype/rules/functions.rs (2192),
    //   relations/subtype/core.rs (2152), intern/core.rs (2120)
    const FILE_COUNT_CEILING: usize = 8;
    assert!(
        oversized.len() <= FILE_COUNT_CEILING,
        "Number of solver source files over 2000 LOC has grown to {} (ceiling: {FILE_COUNT_CEILING}). \
         Split oversized files into smaller modules before adding new code. \
         Current oversized files:\n{}",
        oversized.len(),
        oversized.join("\n")
    );

    // Ceiling: maximum line count of any single solver source file.
    // This prevents existing large files from growing further.
    // Current largest: operations/generic_call.rs at 3488 lines
    const MAX_LOC_CEILING: usize = 3488;
    assert!(
        max_lines <= MAX_LOC_CEILING,
        "Largest solver source file has grown to {max_lines} lines (ceiling: {MAX_LOC_CEILING}). \
         Split the file into smaller modules. Current oversized files:\n{}",
        oversized.join("\n")
    );
}

/// Ratchet guard: prevent the binder crate from growing oversized files.
///
/// The binder is simpler than checker/solver but should still maintain
/// file size discipline to stay maintainable.
#[test]
fn test_binder_file_size_ceiling() {
    let binder_src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tsz-binder/src");
    if !binder_src.exists() {
        return;
    }

    let mut files = Vec::new();
    walk_rs_files_recursive(&binder_src, &mut files);

    let mut oversized = Vec::new();
    let mut max_lines = 0usize;

    for path in &files {
        let rel = path
            .strip_prefix(&binder_src)
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

    // Capture current state as ceiling (1 file: binding/declaration.rs)
    const FILE_COUNT_CEILING: usize = 1;
    assert!(
        oversized.len() <= FILE_COUNT_CEILING,
        "Number of binder source files over 2000 LOC has grown to {} (ceiling: {FILE_COUNT_CEILING}). \
         Split oversized files into smaller modules. Current oversized files:\n{}",
        oversized.len(),
        oversized.join("\n")
    );

    // binding/declaration.rs is currently the largest at 2711 lines
    const MAX_LOC_CEILING: usize = 2711;
    assert!(
        max_lines <= MAX_LOC_CEILING,
        "Largest binder source file has grown to {max_lines} lines (ceiling: {MAX_LOC_CEILING}). \
         Split the file into smaller modules. Current oversized files:\n{}",
        oversized.join("\n")
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

    let mut files = Vec::new();
    walk_rs_files_recursive(&emitter_src, &mut files);

    let mut oversized = Vec::new();
    let mut max_lines = 0usize;

    for path in &files {
        let rel = path
            .strip_prefix(&emitter_src)
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

    // Current oversized files (12 as of 2026-03-24):
    //   declaration_emitter/helpers.rs (8938), declaration_emitter/core.rs (4301),
    //   emitter/declarations/class.rs (3796), transforms/class_es5_ir.rs (2667),
    //   emitter/statements.rs (2574), emitter/types/printer.rs (2520),
    //   emitter/jsx.rs (2263), declaration_emitter/exports.rs (2253),
    //   emitter/expressions/core.rs (2216), emitter/module_emission/core.rs (2160),
    //   emitter/source_file.rs (2081), transforms/ir_printer.rs (2016).
    const FILE_COUNT_CEILING: usize = 12;
    assert!(
        oversized.len() <= FILE_COUNT_CEILING,
        "Number of emitter source files over 2000 LOC has grown to {} (ceiling: {FILE_COUNT_CEILING}). \
         Split oversized files into smaller modules. Current oversized files:\n{}",
        oversized.len(),
        oversized.join("\n")
    );

    // declaration_emitter/helpers.rs is currently the largest at 8938 lines.
    const MAX_LOC_CEILING: usize = 8938;
    assert!(
        max_lines <= MAX_LOC_CEILING,
        "Largest emitter source file has grown to {max_lines} lines (ceiling: {MAX_LOC_CEILING}). \
         Split the file into smaller modules. Current oversized files:\n{}",
        oversized.join("\n")
    );
}
