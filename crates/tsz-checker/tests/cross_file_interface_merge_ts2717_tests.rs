//! Tests for cross-file (cross-arena) global interface merge TS2717.
//!
//! When the same global-scope interface is declared in two or more global
//! script files (non-module `.ts` / `.d.ts`) and the declarations disagree on a
//! member's type, `tsc` reports TS2717 ("Subsequent property declarations must
//! have the same type") anchored at the subsequent declaration. The same-file
//! merge path only sees the file currently being checked, so a conflict that
//! spans files used to be silently dropped. These tests pin the cross-file
//! behavior and guard against false positives.

use crate::context::CheckerOptions;

fn opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
}

/// Type-check the whole multi-file program (every file checked with a shared
/// type universe and the global symbol index wired, like the driver) and report
/// whether `file` reports `code`.
fn file_has_code(files: &[(&str, &str)], file: &str, code: u32) -> bool {
    crate::test_utils::check_all_multi_file_with_global_index(files, opts())
        .into_iter()
        .any(|d| d.file == file && d.code == code)
}

/// Whether `code` is reported anywhere in the program.
fn program_has_code(files: &[(&str, &str)], code: u32) -> bool {
    crate::test_utils::check_all_multi_file_with_global_index(files, opts())
        .into_iter()
        .any(|d| d.code == code)
}

#[test]
fn cross_file_conflicting_property_emits_ts2717_on_subsequent_declaration() {
    // a.ts declares `a: number` first; b.ts declares `a: string`. tsc reports
    // TS2717 at b.ts (the subsequent declaration in program order) and not at
    // the first declaration.
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
    // The same gap exists for ambient `.d.ts` global scripts.
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
    // Declaration merging with identical (or non-overlapping) member types is
    // legal: no TS2717 false positive.
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
    // First=number; the string declaration conflicts, a later number one does
    // not (tsc compares each subsequent declaration against the first).
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
    // The rule is structural, not keyed to any particular identifier.
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
    // `export` makes each file an external module; same-named interfaces are
    // distinct symbols and must not be compared for TS2717.
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

#[test]
fn same_file_merge_still_reports_ts2717() {
    // The pre-existing same-file merge path must keep working unchanged.
    let files = [(
        "only.ts",
        "interface C2 { a: number; }\ninterface C2 { a: string; }",
    )];
    assert!(
        file_has_code(&files, "only.ts", 2717),
        "same-file interface merge conflict must still report TS2717"
    );
}

#[test]
fn single_file_consistent_interface_no_spurious_ts2717() {
    // A lone, internally-consistent global interface must stay clean.
    let files = [(
        "only.ts",
        "interface Solo { a: number; }\nconst s: Solo = { a: 1 };",
    )];
    assert!(
        !program_has_code(&files, 2717),
        "a single consistent interface must not report TS2717"
    );
}
