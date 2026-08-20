//! Tests for TS2498: Module uses 'export =' and cannot be used with 'export *'.
//!
//! When a module uses `export = X`, re-exporting via `export *` or
//! `export * as ns` must emit TS2498.

use crate::context::CheckerOptions;
use crate::test_utils::check_multi_file;

/// Set up a two-file project and check file `a.ts` which does
/// `export * as ns from './b'` where `b.ts` has `export = {}`.
fn check_export_star_from_export_equals(source_a: &str, source_b: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        module: tsz_common::common::ModuleKind::CommonJS,
        ..Default::default()
    };
    let diagnostics = check_multi_file(&[("a.ts", source_a), ("b.ts", source_b)], "a.ts", options);
    diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn export_star_as_ns_from_export_equals_emits_ts2498() {
    let diagnostics =
        check_export_star_from_export_equals("export * as ns from './b';", "export = {}");
    let ts2498_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2498)
        .collect();
    assert!(
        !ts2498_errors.is_empty(),
        "Expected TS2498 for `export * as ns` from a module with `export =`, got: {diagnostics:?}"
    );
    assert!(
        ts2498_errors[0].1.contains("export ="),
        "TS2498 message should mention 'export =', got: {}",
        ts2498_errors[0].1
    );
}

#[test]
fn export_star_bare_from_export_equals_emits_ts2498() {
    let diagnostics = check_export_star_from_export_equals("export * from './b';", "export = {}");
    let ts2498_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2498)
        .collect();
    assert!(
        !ts2498_errors.is_empty(),
        "Expected TS2498 for `export *` from a module with `export =`, got: {diagnostics:?}"
    );
}

#[test]
fn export_named_from_export_equals_no_ts2498() {
    // Named re-exports should NOT emit TS2498
    let diagnostics =
        check_export_star_from_export_equals("export { default } from './b';", "export = {}");
    let ts2498_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2498)
        .collect();
    assert!(
        ts2498_errors.is_empty(),
        "Named export should NOT emit TS2498, got: {diagnostics:?}"
    );
}

#[test]
fn export_star_from_normal_module_no_ts2498() {
    // Normal module (no export =) should not emit TS2498
    let diagnostics =
        check_export_star_from_export_equals("export * as ns from './b';", "export const x = 1;");
    let ts2498_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2498)
        .collect();
    assert!(
        ts2498_errors.is_empty(),
        "Normal module should NOT emit TS2498, got: {diagnostics:?}"
    );
}
