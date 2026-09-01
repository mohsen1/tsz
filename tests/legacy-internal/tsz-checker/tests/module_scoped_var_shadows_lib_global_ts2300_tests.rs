//! TS2300 coverage for module-scoped declarations that share a name with a
//! lib global (issue #16208).
//!
//! Structural rule: a module-scoped declaration lives in a different scope
//! than the lib's global scope, so `tsc` never forms a duplicate-identifier
//! conflict between them. tsz's binder intentionally leaves function-scoped
//! `var` shadowing disabled for modules (some inference paths rely on the
//! merged-symbol behavior), so a module-scoped `var eval` merges its
//! declaration directly into the lib's `eval` symbol. The checker's
//! duplicate-identifier pass re-derives each declaration's arena through
//! `arena_for_declaration_or`, which falls back to the symbol's stale
//! per-symbol `symbol_arenas` entry (the lib arena) whenever no precise
//! per-declaration entry exists — as is the case for a declaration that
//! merged in without registering one. That stale fallback made the merged
//! local declaration and the lib declaration look like they shared one file,
//! corrupting `same_source_file` and bypassing the module-scope skip that
//! already exists for genuinely cross-file conflicts.
//!
//! Owner layer: `crates/tsz-checker/src/types/type_checking/duplicate_identifiers.rs`'s
//! pairwise conflict scan, which now trusts the declaration's already-derived
//! `is_local` flag instead of re-resolving the arena through the legacy
//! per-symbol fallback.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs_code_messages, load_lib_files};

fn module_codes(source: &str) -> Vec<u32> {
    let libs = load_lib_files(&["es5.d.ts"]);
    check_source_with_libs_code_messages(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|(code, _)| code)
    .collect()
}

/// `export {}; var eval;` — `eval` is module-scoped, so it cannot collide
/// with the global `eval` in `lib.es5.d.ts`. Only TS1215 (the unrelated
/// strict-mode name restriction) should fire.
#[test]
fn module_scoped_var_eval_reports_only_strict_mode_diagnostic() {
    let codes = module_codes("export {};\nvar eval;\n");
    assert_eq!(
        codes,
        vec![1215],
        "module-scoped `var eval` must not draw TS2300 against the lib global"
    );
}

/// Same shape, a non-reserved lib global name (`Array`) that draws no
/// strict-mode restriction at all — the file should be entirely clean.
#[test]
fn module_scoped_var_array_reports_nothing() {
    let codes = module_codes("export {};\nvar Array;\n");
    assert!(
        codes.is_empty(),
        "module-scoped `var Array` must not collide with the lib global Array, got {codes:?}"
    );
}

/// `declare global { var eval; }` is the true augmentation case this skip
/// must NOT suppress: the declaration explicitly requests global scope, so
/// it still conflicts with the lib global exactly like `tsc`.
#[test]
fn declare_global_var_eval_still_reports_ts2300() {
    let codes = module_codes("export {};\ndeclare global {\n  var eval;\n}\n");
    assert!(
        codes.contains(&2300),
        "`declare global {{ var eval }}` must still conflict with the lib global, got {codes:?}"
    );
}

/// A plain script (no `export`/`import`) keeps the pre-existing script-scope
/// behavior: `var eval` at global scope still conflicts with the lib global.
#[test]
fn script_scoped_var_eval_still_reports_ts2300() {
    let codes = module_codes("var eval;\n");
    assert!(
        codes.contains(&2300),
        "script-scoped `var eval` must still conflict with the lib global, got {codes:?}"
    );
}
