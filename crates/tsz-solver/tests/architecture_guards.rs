//! Cross-crate architecture guard tests.
//!
//! These tests enforce structural invariants from CLAUDE.md that cannot be
//! expressed through Rust's module system or Cargo dependency declarations.
//!
//! Guards:
//! - Emitter must not perform semantic type validation (rule 13)
//! - Binder must not import solver or checker (rule 4)
//!
//! Note: Solver file size ratchets are in `solver_file_size_ceiling_tests.rs`.

use std::fs;
use std::path::{Path, PathBuf};

fn walk_rs_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_rs_files(&path, &mut *files);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}

// =============================================================================
// Emitter semantic validation guard
// =============================================================================

/// Guard that the emitter crate does not perform on-the-fly semantic type validation.
///
/// Per CLAUDE.md section 13: "No on-the-fly semantic type validation."
/// Per CLAUDE.md section 4: "Emitter importing Checker internals for semantic checks"
/// is a forbidden shortcut.
///
/// The emitter may use solver read-only APIs (TypeInterner, type_queries, visitor)
/// for declaration emit and type printing, but must NOT use relation/compatibility
/// APIs that perform semantic validation.
#[test]
fn emitter_must_not_use_semantic_validation_apis() {
    let emitter_src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tsz-emitter/src");

    if !emitter_src.exists() {
        // Skip if emitter crate doesn't exist in this workspace layout
        return;
    }

    let mut files = Vec::new();
    walk_rs_files(&emitter_src, &mut files);

    // These are solver relation/compatibility APIs that the emitter must never use.
    // Using them would mean the emitter is performing semantic type validation.
    const FORBIDDEN_PATTERNS: &[&str] = &[
        "CompatChecker",
        "SubtypeChecker",
        "is_assignable",
        "is_subtype_of",
        "RelationResult",
        "check_assignability",
        "tsz_checker::",
    ];

    let mut violations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&emitter_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }

            for &pattern in FORBIDDEN_PATTERNS {
                if line.contains(pattern) {
                    violations.push(format!(
                        "  {}:{} — uses forbidden pattern `{}`",
                        rel,
                        line_num + 1,
                        pattern
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Emitter must not perform semantic type validation (CLAUDE.md section 13). \
         The emitter may use read-only solver APIs (TypeInterner, type_queries, visitor) \
         but must NOT use relation/compatibility/checker APIs. \
         Violations found:\n{}",
        violations.join("\n")
    );
}

// =============================================================================
// Binder semantic isolation guard
// =============================================================================

/// Guard that the binder crate does not import solver or checker types.
///
/// Per CLAUDE.md section 4: "Binder importing Solver for semantic decisions"
/// is a forbidden shortcut.
/// Per CLAUDE.md section 10: "No type inference/subtyping logic in binder."
///
/// This is enforced at the Cargo dependency level, but this test provides
/// a source-level belt-and-suspenders check and clearer error messages.
#[test]
fn binder_must_not_import_solver_or_checker() {
    let binder_src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tsz-binder/src");

    if !binder_src.exists() {
        return;
    }

    let mut files = Vec::new();
    walk_rs_files(&binder_src, &mut files);

    const FORBIDDEN_IMPORTS: &[&str] = &[
        "tsz_solver::",
        "tsz_checker::",
        "use tsz_solver",
        "use tsz_checker",
    ];

    let mut violations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&binder_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }

            for &forbidden in FORBIDDEN_IMPORTS {
                if line.contains(forbidden) {
                    violations.push(format!(
                        "  {}:{} — imports {}",
                        rel,
                        line_num + 1,
                        forbidden
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Binder must not import solver or checker (CLAUDE.md sections 4, 10). \
         The binder produces symbols, scopes, and control-flow graphs without \
         type computation. Violations found:\n{}",
        violations.join("\n")
    );
}
