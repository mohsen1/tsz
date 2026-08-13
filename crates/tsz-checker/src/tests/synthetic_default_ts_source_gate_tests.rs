//! A `.ts`/`.tsx` *source* module never provides a shape-based synthetic
//! `default` export, and no module does so with interop off.
//!
//! Structural rule: `tsc`'s `canHaveSyntheticDefault` requires
//! `allowSyntheticDefaultImports` (or `esModuleInterop`, which implies it)
//! before any synthetic default exists, and for a non-JS *source* file it
//! synthesizes a default only from `export =` — a plain TS module with only
//! named exports keeps `TS1192` on `import X from` and `TS2305` on
//! `export { default } from` / `import { default as X } from`, even with
//! interop on (oracle: `reexportMissingDefault1/2`,
//! `es6ImportDefaultBindingNoDefaultProperty`). Shape-based synthesis is for
//! CommonJS-shaped `.js` modules and declaration files only.
//!
//! Regression witness: the `module_can_use_synthetic_default_import` arms in
//! `has_default_binding`
//! (`crates/tsz-checker/src/declarations/import/core/import_members.rs`) and
//! `validate_reexported_members`
//! (`crates/tsz-checker/src/declarations/module_checker.rs`) briefly ran
//! without the interop-flag and source-file gates, turning the whole
//! `es6ImportDefaultBinding*` / `reexportMissingDefault*` conformance
//! families into false negatives under `module: commonjs`.
//!
//! Harness note: mirrors `js_commonjs_default_reexport_ts2305_tests.rs` —
//! `check_multi_file` leaves `report_unresolved_imports` unset, which skips
//! `validate_reexported_members` entirely, so the harness sets it directly.
//! It also populates `file_is_esm_map` with every file marked CJS-format,
//! matching what the CLI computes under `module: commonjs` — without that
//! map, `module_can_use_synthetic_default_import`'s format fallback returns
//! `false` for every target and the gated arms never fire at all, making
//! these tests pass vacuously against the regressed code.

use crate::context::CheckerOptions;
use crate::diagnostics::diagnostic_codes;
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;

fn check(
    files: &[(&str, &str)],
    entry: &str,
    es_module_interop: bool,
) -> Vec<crate::diagnostics::Diagnostic> {
    let options = CheckerOptions {
        module: ModuleKind::CommonJS,
        es_module_interop,
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
    checker.ctx.file_is_esm_map = Some(Arc::new(
        file_names
            .iter()
            .map(|name| (name.clone(), false))
            .collect(),
    ));

    checker.prime_module_augmentation_bodies();
    checker.check_source_file(roots[entry_idx]);
    checker.ctx.diagnostics.clone()
}

fn codes(diags: &[crate::diagnostics::Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

const TS_NAMED_ONLY_MODULE: &str = "export const marker = null;\n";

/// `export { default } from` a named-exports-only TS source module is TS2305
/// with interop off (`reexportMissingDefault6`).
#[test]
fn default_reexport_from_ts_source_reports_ts2305_interop_off() {
    let diags = check(
        &[
            ("./dep.ts", TS_NAMED_ONLY_MODULE),
            (
                "./entry.ts",
                "export { marker } from \"./dep\";\nexport { default } from \"./dep\";\n",
            ),
        ],
        "./entry.ts",
        false,
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

/// Interop ON changes nothing for a TS *source* target: a synthetic default
/// needs `export =` there, so the re-export is still TS2305
/// (`reexportMissingDefault1/2`).
#[test]
fn default_reexport_from_ts_source_reports_ts2305_interop_on() {
    let diags = check(
        &[
            ("./dep.ts", TS_NAMED_ONLY_MODULE),
            (
                "./entry.ts",
                "export { marker } from \"./dep\";\nexport { default } from \"./dep\";\n",
            ),
        ],
        "./entry.ts",
        true,
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

/// The renamed spelling (`export { default as other } from`) follows the same
/// rule (`reexportMissingDefault3`, renamed binder).
#[test]
fn renamed_default_reexport_from_ts_source_reports_ts2305() {
    let diags = check(
        &[
            ("./widget.ts", "export const gadget = 1;\n"),
            (
                "./entry.ts",
                "export { default as renamedThing } from \"./widget\";\n",
            ),
        ],
        "./entry.ts",
        true,
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

/// A default-import clause from a named-exports-only TS source module is
/// TS1192 with interop off (`es6ImportDefaultBindingNoDefaultProperty`).
#[test]
fn default_import_from_ts_source_reports_ts1192_interop_off() {
    let diags = check(
        &[
            ("./dep.ts", TS_NAMED_ONLY_MODULE),
            (
                "./entry.ts",
                "import fallback from \"./dep\";\nexport const use = fallback;\n",
            ),
        ],
        "./entry.ts",
        false,
    );
    assert!(
        codes(&diags).contains(&diagnostic_codes::MODULE_HAS_NO_DEFAULT_EXPORT),
        "expected TS1192, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

/// Interop ON keeps TS1192 for the default-import clause over a TS source
/// module — the interop synthesis never applies to non-JS source files.
#[test]
fn default_import_from_ts_source_reports_ts1192_interop_on() {
    let diags = check(
        &[
            ("./dep.ts", TS_NAMED_ONLY_MODULE),
            (
                "./entry.ts",
                "import fallback from \"./dep\";\nexport const use = fallback;\n",
            ),
        ],
        "./entry.ts",
        true,
    );
    assert!(
        codes(&diags).contains(&diagnostic_codes::MODULE_HAS_NO_DEFAULT_EXPORT),
        "expected TS1192, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

/// Positive control: the gates must not re-break the JS CommonJS
/// whole-module-value case the arms exist for — with interop on,
/// `export { default as X } from` a `module.exports = <class>` `.js` module
/// resolves the synthesized default
/// (`jsDeclarationsReexportAliasesEsModuleInterop`).
#[test]
fn default_reexport_from_js_commonjs_value_still_resolves_with_interop_on() {
    let diags = check_js(
        &[
            ("./impl.js", "class Gizmo {}\nmodule.exports = Gizmo;\n"),
            (
                "./entry.js",
                "export { default as Gizmo } from \"./impl\";\n",
            ),
        ],
        "./entry.js",
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

/// Same harness as [`check`] with `allow_js`/`check_js` on and interop on,
/// for the JS positive control.
fn check_js(files: &[(&str, &str)], entry: &str) -> Vec<crate::diagnostics::Diagnostic> {
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
    checker.ctx.file_is_esm_map = Some(Arc::new(
        file_names
            .iter()
            .map(|name| (name.clone(), false))
            .collect(),
    ));

    checker.prime_module_augmentation_bodies();
    checker.check_source_file(roots[entry_idx]);
    checker.ctx.diagnostics.clone()
}
