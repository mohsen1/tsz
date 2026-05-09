//! Regression test: when a JS module's exports come *only* from CommonJS
//! property assignments (`exports.X = …` / `module.exports.X = …`), the
//! binder's `module_exports` table is empty even though the file has
//! genuine exports. The named-import diagnostic pass in
//! `check_imported_members` used to consult only that binder table for
//! its `has_export_surface` short-circuit, so a JS module like
//!
//!   exports.j = 1;
//!   exports.k = void 0;          // dropped from the `JsExportSurface`
//!
//! looked "empty" to the importer pass, which short-circuited and never
//! emitted TS2305 for `import { k } from './main'`.
//!
//! The fix falls back to the `JsExportSurface` (which captures
//! `exports.X = …` assignments) before short-circuiting. The check keys
//! off the structural condition (surface has any named/CJS/prototype
//! exports), not on a specific identifier name.

use tsz_checker::context::CheckerOptions;

fn diagnostic_codes_for_two_files(target_source: &str, importer_source: &str) -> Vec<u32> {
    tsz_checker::test_utils::check_multi_file(
        &[("main.js", target_source), ("importer.js", importer_source)],
        "importer.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            no_lib: true,
            ..Default::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

const TS2305: u32 = 2305;

#[test]
fn named_import_of_void_zero_export_emits_ts2305() {
    // The void-zero suppression in `infer_commonjs_export_rhs_type` already
    // drops `k` from the JS Export Surface; without the surface fallback
    // in `has_export_surface`, the importer pass short-circuits and TS2305
    // is silently lost.
    let codes = diagnostic_codes_for_two_files(
        "exports.j = 1;\nexports.k = void 0;\n",
        "import { j, k } from './main';\n",
    );
    assert!(
        codes.contains(&TS2305),
        "expected TS2305 for `import {{ k }} from './main'` (k is void 0 → not exported), got codes: {codes:?}"
    );
}

#[test]
fn shape_holds_for_module_exports_dot_form() {
    // The fix must also trigger for `module.exports.X = void 0` (not just
    // bare `exports.X = void 0`) because both shapes feed the same surface.
    let codes = diagnostic_codes_for_two_files(
        "module.exports.j = 1;\nmodule.exports.k = void 0;\n",
        "import { j, k } from './main';\n",
    );
    assert!(
        codes.contains(&TS2305),
        "expected TS2305 for `module.exports.X = void 0` shape, got: {codes:?}"
    );
}

#[test]
fn valid_named_import_does_not_emit_ts2305() {
    // Negative invariant: when the import name *is* a real export, no
    // TS2305 fires. Without this case, a regression that always emits
    // TS2305 would still pass the positive tests above.
    let codes = diagnostic_codes_for_two_files("exports.j = 1;\n", "import { j } from './main';\n");
    assert!(
        !codes.contains(&TS2305),
        "must NOT emit TS2305 when the import name is a real export, got codes: {codes:?}"
    );
}
