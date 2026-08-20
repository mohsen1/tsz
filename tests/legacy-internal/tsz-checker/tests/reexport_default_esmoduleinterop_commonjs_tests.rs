//! `import { default as X } from "mod"` must accept the same synthesized
//! `default` a plain `import X from "mod"` accepts under `esModuleInterop`.
//!
//! Structural rule: when a source `.js` module is CommonJS-shaped (e.g.
//! `module.exports = Foo;` — no explicit `export =`, not a `.d.ts`) and
//! `esModuleInterop`/`allowSyntheticDefaultImports` is on, `tsc` synthesizes
//! a `default` export for it, and both import spellings — the default-import
//! clause (`import X from "mod"`) and the named specifier that spells
//! `default` (`import { default as X } from "mod"`) — accept that synthesized
//! default identically. `emit_no_default_export_error`
//! (`crates/tsz-checker/src/state/type_resolution/module.rs`) special-cased
//! the named-specifier spelling: it resolved `named_default_specifier_node`
//! and emitted `TS2305` *before* running any of the function's
//! `module_can_use_synthetic_default_import` / `allow_synthetic_default_imports`
//! suppression checks, which the default-clause spelling (falling through to
//! the same checks below) already honored. Re-exporting that named default
//! (`export { default as Y } from "mod"`) inherited the false positive
//! because it re-checks the import first. See
//! `jsDeclarationsReexportAliasesEsModuleInterop.ts` (#17326-adjacent).

use crate::context::CheckerOptions;
use crate::diagnostics::diagnostic_codes;
use crate::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

fn check(files: &[(&str, &str)], entry: &str) -> Vec<crate::diagnostics::Diagnostic> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            es_module_interop: true,
            allow_js: true,
            check_js: true,
            ..CheckerOptions::default()
        },
    )
}

fn codes(diags: &[crate::diagnostics::Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

/// The exact oracle repro shape: a named default-import specifier plus a
/// re-export of the same synthesized default, both against a CJS class
/// export. `tsc` reports nothing for either line.
#[test]
fn named_default_import_and_reexport_from_commonjs_class_export_is_clean() {
    let diags = check(
        &[
            (
                "./cls.js",
                r#"class Foo {}
module.exports = Foo;
"#,
            ),
            (
                "./usage.js",
                r#"import {default as Fooa} from "./cls";
export const x = new Fooa();
export {default as Foob} from "./cls";
"#,
            ),
        ],
        "./usage.js",
    );
    assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
}

/// Renamed binders + a function export (instead of a class) must behave
/// identically — the rule is structural, not name- or shape-driven.
#[test]
fn named_default_import_from_commonjs_function_export_is_clean_renamed() {
    let diags = check(
        &[
            (
                "./thing.js",
                r#"function Thing() {}
module.exports = Thing;
"#,
            ),
            (
                "./consumer.js",
                r#"import {default as Renamed} from "./thing";
export const y = Renamed;
"#,
            ),
        ],
        "./consumer.js",
    );
    assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
}

/// A plain default-import clause against the same CJS module already worked
/// before this fix — kept as a control so a future regression in the shared
/// suppression cascade shows up on both spellings, not just the named one.
#[test]
fn plain_default_import_clause_from_commonjs_class_export_is_clean() {
    let diags = check(
        &[
            (
                "./cls2.js",
                r#"class Bar {}
module.exports = Bar;
"#,
            ),
            (
                "./consumer2.js",
                r#"import Fooa from "./cls2";
export const x = new Fooa();
"#,
            ),
        ],
        "./consumer2.js",
    );
    assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
}

/// A genuinely missing named (non-`default`) member from the same
/// CommonJS-shaped module must still report `TS2305` — this fix only widens
/// suppression for the `default` name specifically.
#[test]
fn named_import_of_missing_member_from_commonjs_export_still_reports() {
    let diags = check(
        &[
            (
                "./cls3.js",
                r#"class Baz {}
module.exports = Baz;
"#,
            ),
            (
                "./consumer4.js",
                r#"import {missing as X} from "./cls3";
export const z = X;
"#,
            ),
        ],
        "./consumer4.js",
    );
    assert_eq!(
        codes(&diags),
        vec![diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER],
        "got {diags:?}"
    );
}
