//! A JS CommonJS module's whole-module `module.exports = <value>` assignment
//! provides an esModuleInterop-synthesized `default` for every default-shaped
//! import form, not just `import X from "./m"`.
//!
//! Structural rule: under `esModuleInterop`, `tsc` synthesizes a `default`
//! from a CJS module's `module.exports` value for `import X from "./m"`,
//! `import { default as X } from "./m"`, and `export { default as X } from
//! "./m"` alike — the import/export *shape* never changes whether the
//! interop default exists. `tsz` tracks a `module.exports = <value>`
//! assignment only as `JsExportSurface::direct_export_type` (a pure type
//! computation walked from the AST), which is invisible to the binder-level
//! `module_exports` table that `resolve_export_in_file` /
//! `resolve_effective_module_exports_with_mode` consult. Before this fix,
//! `import X from` happened to dodge the gap (its diagnostic path falls
//! through to a branch gated on `resolved_modules` containing the candidate,
//! which single/two-file compiles don't populate), while `import { default
//! as X }` (`crates/tsz-checker/src/declarations/import/core/import_members.rs`,
//! `has_default_binding`) and `export { default as X } from`
//! (`crates/tsz-checker/src/declarations/module_checker.rs`,
//! `validate_reexported_members`) both hit their unconditional TS2305 arm
//! every time. Both now also consult `module_can_use_synthetic_default_import`,
//! the same eligibility check `import X from` already relies on to suppress
//! `TS1192`.
//!
//! Harness note: `check` below cannot use
//! `crate::test_utils::check_multi_file` — that helper leaves
//! `report_unresolved_imports` at its `CheckerState::new` default of `false`,
//! and `check_export_module_specifier` returns before calling
//! `validate_reexported_members` whenever that flag is unset, so every
//! `export { … } from` case here would trivially pass (the check never runs)
//! rather than exercising the fix. `check` instead mirrors `check_multi_file`
//! with the flag set, matching `tests/ts2305_tests.rs` and
//! `src/tests/position_invalid_default_export_expression_tests.rs`.

use crate::context::CheckerOptions;
use crate::diagnostics::diagnostic_codes;
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;

/// [`crate::test_utils::check_multi_file`] leaves `report_unresolved_imports`
/// at its `CheckerState::new` default of `false`, which makes
/// `check_export_module_specifier` return before it ever calls
/// `validate_reexported_members` — so an `export { … } from` re-export test
/// built on that helper never exercises the TS2305/TS2614 path at all (every
/// production entry point sets the flag `true` before checking). This mirrors
/// `check_multi_file` but sets the flag, matching the established idiom in
/// `tests/ts2305_tests.rs` and `src/tests/position_invalid_default_export_expression_tests.rs`.
fn check(files: &[(&str, &str)], entry: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let options = CheckerOptions {
        module: ModuleKind::CommonJS,
        allow_js: true,
        check_js: true,
        es_module_interop: true,
        ..CheckerOptions::default()
    };

    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in files {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let entry_idx = file_names
        .iter()
        .position(|name| name == entry)
        .unwrap_or_else(|| panic!("entry_file {entry:?} not found in files"));
    let (resolved_module_paths, resolved_modules) =
        crate::module_resolution::build_module_resolution_maps(&file_names);

    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &types,
        file_names[entry_idx].clone(),
        options,
    );
    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    checker.ctx.set_lib_contexts(Vec::new());
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);
    checker.ctx.report_unresolved_imports = true;

    checker.prime_module_augmentation_bodies();
    checker.check_source_file(roots[entry_idx]);
    checker.ctx.diagnostics.clone()
}

fn codes(diags: &[crate::diagnostics::Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

const CLASS_VALUE_MODULE: &str = r#"class Foo {}
module.exports = Foo;
"#;

/// `import { default as X }` of a plain-value CJS module (`module.exports =
/// Foo`) must resolve the synthesized default, matching `import X from`.
#[test]
fn named_default_specifier_import_resolves_cjs_value_default() {
    let diags = check(
        &[
            ("./cls.js", CLASS_VALUE_MODULE),
            (
                "./usage.js",
                r#"import {default as Fooa} from "./cls";
export const x = new Fooa();
"#,
            ),
        ],
        "./usage.js",
    );
    assert!(
        !codes(&diags).contains(&diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER),
        "expected no TS2305, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

/// `export { default as X } from "./m"` of the same plain-value CJS module
/// must also resolve the synthesized default (the oracle repro,
/// `jsDeclarationsReexportAliasesEsModuleInterop.ts`).
#[test]
fn default_reexport_resolves_cjs_value_default() {
    let diags = check(
        &[
            ("./cls.js", CLASS_VALUE_MODULE),
            (
                "./usage.js",
                r#"export {default as Foob} from "./cls";
"#,
            ),
        ],
        "./usage.js",
    );
    assert!(
        !codes(&diags).contains(&diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER),
        "expected no TS2305, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

/// Both import forms together, mirroring the oracle repro exactly (`import`
/// on one line, `export ... from` re-exporting the same default below it).
#[test]
fn default_import_and_reexport_together_resolve_cjs_value_default() {
    let diags = check(
        &[
            ("./cls.js", CLASS_VALUE_MODULE),
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
    assert!(
        !codes(&diags).contains(&diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER),
        "expected no TS2305, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

/// An object-literal whole-module value (`module.exports = { a, b }`, no
/// backing declaration symbol) must resolve the same way as a named-class
/// value — the interop default wraps the whole exports value regardless of
/// its shape.
#[test]
fn default_reexport_resolves_cjs_object_literal_default() {
    let diags = check(
        &[
            (
                "./mod.js",
                r#"module.exports = { a: 1, b: "x" };
"#,
            ),
            (
                "./usage.js",
                r#"export {default as Y} from "./mod";
"#,
            ),
        ],
        "./usage.js",
    );
    assert!(
        !codes(&diags).contains(&diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER),
        "expected no TS2305, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

/// Renamed binders (not the oracle repro's `Foo`/`Fooa`/`Foob`) exercise the
/// same structural rule, not a name-specific special case.
#[test]
fn default_reexport_resolves_cjs_value_default_renamed_binders() {
    let diags = check(
        &[
            (
                "./widget.js",
                r#"class Zorp {}
module.exports = Zorp;
"#,
            ),
            (
                "./usage.js",
                r#"import {default as Local} from "./widget";
export const q = new Local();
export {default as Renamed} from "./widget";
"#,
            ),
        ],
        "./usage.js",
    );
    assert!(
        !codes(&diags).contains(&diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER),
        "expected no TS2305, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

/// Negative control: a genuine ESM module (`.mjs`, no CJS shape at all) with
/// no `default` export must still report TS2305 — the interop synthesis only
/// applies to CommonJS-shaped modules, never to real ES modules.
#[test]
fn default_reexport_of_genuine_esm_module_without_default_still_reports_ts2305() {
    let diags = check(
        &[
            (
                "./mod.mjs",
                r#"export const x = 1;
"#,
            ),
            (
                "./usage.mjs",
                r#"export {default as Y} from "./mod.mjs";
"#,
            ),
        ],
        "./usage.mjs",
    );
    assert!(
        codes(&diags).contains(&diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER),
        "expected TS2305, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

/// Negative control: re-exporting a name the CJS module genuinely never
/// provides must still report TS2305 — the fix only widens what counts as a
/// `default`, not general export existence.
#[test]
fn default_reexport_fix_does_not_suppress_genuine_missing_member() {
    let diags = check(
        &[
            ("./mod.js", CLASS_VALUE_MODULE),
            (
                "./usage.js",
                r#"export {doesNotExist} from "./mod";
"#,
            ),
        ],
        "./usage.js",
    );
    assert!(
        codes(&diags).contains(&diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER),
        "expected TS2305, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}
