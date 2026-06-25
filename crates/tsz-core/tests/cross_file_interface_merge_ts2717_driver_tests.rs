//! Driver-level tests for cross-file (cross-arena) global interface merge
//! TS2717.
//!
//! These exercise the full parse → bind → merge → check pipeline (the path the
//! CLI uses), where global-scope interfaces declared in separate script files
//! merge into a single symbol whose property types resolve to the first
//! declaration in program order. A property declared with a conflicting type in
//! a subsequent file must report TS2717 anchored at that subsequent declaration.
//!
//! The lightweight `tsz-checker` harness does not merge global script symbols
//! across files, so the positive cross-file path can only be observed here.

use super::{check_files_parallel, compile_files_with_libs};
use crate::checker::context::CheckerOptions;

/// Compile and check a multi-file global-script program with no libs, returning
/// every (`file_name`, `code`) pair across all files.
fn diagnostics(files: &[(&str, &str)]) -> Vec<(String, u32)> {
    let owned: Vec<(String, String)> = files
        .iter()
        .map(|(name, src)| ((*name).to_string(), (*src).to_string()))
        .collect();
    let program = compile_files_with_libs(owned, &[]);
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    };
    let result = check_files_parallel(&program, &options, &[]);
    result
        .file_results
        .iter()
        .flat_map(|file| {
            file.diagnostics
                .iter()
                .map(move |d| (file.file_name.clone(), d.code))
        })
        .collect()
}

fn file_has_code(files: &[(&str, &str)], file: &str, code: u32) -> bool {
    diagnostics(files)
        .iter()
        .any(|(f, c)| f.ends_with(file) && *c == code)
}

fn program_has_code(files: &[(&str, &str)], code: u32) -> bool {
    diagnostics(files).iter().any(|(_, c)| *c == code)
}

#[test]
fn cross_file_conflicting_property_emits_ts2717_on_subsequent_declaration() {
    let files = [
        ("a.ts", "interface Conflict { a: number; }"),
        ("b.ts", "interface Conflict { a: string; }"),
    ];
    assert!(
        file_has_code(&files, "b.ts", 2717),
        "expected TS2717 anchored at the subsequent declaration (b.ts)"
    );
    assert!(
        !file_has_code(&files, "a.ts", 2717),
        "the first declaration's file must not report TS2717"
    );
}

#[test]
fn cross_file_dts_files_conflict_emits_ts2717() {
    let files = [
        ("a.d.ts", "interface Conflict { a: number; }"),
        ("b.d.ts", "interface Conflict { a: string; }"),
    ];
    assert!(
        file_has_code(&files, "b.d.ts", 2717),
        "expected TS2717 across two ambient .d.ts global scripts"
    );
}

#[test]
fn cross_file_matching_property_types_emit_nothing() {
    let files = [
        ("a.ts", "interface Same { a: number; }"),
        ("b.ts", "interface Same { a: number; b: string; }"),
    ];
    assert!(
        !program_has_code(&files, 2717),
        "matching member types must not report TS2717"
    );
}

#[test]
fn cross_file_three_declarations_only_mismatched_subsequent_errors() {
    let files = [
        ("a.ts", "interface Tri { a: number; }"),
        ("b.ts", "interface Tri { a: string; }"),
        ("c.ts", "interface Tri { a: number; }"),
    ];
    assert!(
        file_has_code(&files, "b.ts", 2717),
        "the conflicting subsequent declaration (b.ts) must report TS2717"
    );
    assert!(
        !file_has_code(&files, "c.ts", 2717),
        "a later declaration that matches the first must not report TS2717"
    );
    assert!(
        !file_has_code(&files, "a.ts", 2717),
        "the first declaration must not report TS2717"
    );
}

#[test]
fn cross_file_ts2717_is_binder_name_independent() {
    let files = [
        ("a.ts", "interface Banana { fruit: number; }"),
        ("b.ts", "interface Banana { fruit: boolean; }"),
    ];
    assert!(
        file_has_code(&files, "b.ts", 2717),
        "cross-file TS2717 must fire regardless of the interface/property name"
    );
}

#[test]
fn module_scoped_interfaces_do_not_merge_across_files() {
    let files = [
        (
            "a.ts",
            "export interface Mod { a: number; }\nexport const x = 1;",
        ),
        (
            "b.ts",
            "export interface Mod { a: string; }\nexport const y = 2;",
        ),
    ];
    assert!(
        !program_has_code(&files, 2717),
        "module-scoped same-named interfaces must not merge across files"
    );
}

