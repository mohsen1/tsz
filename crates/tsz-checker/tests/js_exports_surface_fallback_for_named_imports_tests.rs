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

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostic_codes_for_two_files(target_source: &str, importer_source: &str) -> Vec<u32> {
    let mut parser_target = ParserState::new("main.js".to_string(), target_source.to_string());
    let root_target = parser_target.parse_source_file();
    let mut binder_target = BinderState::new();
    binder_target.bind_source_file(parser_target.get_arena(), root_target);

    let mut parser_importer =
        ParserState::new("importer.js".to_string(), importer_source.to_string());
    let root_importer = parser_importer.parse_source_file();
    let mut binder_importer = BinderState::new();
    binder_importer.bind_source_file(parser_importer.get_arena(), root_importer);

    let arena_target = Arc::new(parser_target.get_arena().clone());
    let arena_importer = Arc::new(parser_importer.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena_target), Arc::clone(&arena_importer)]);

    let binder_target = Arc::new(binder_target);
    let binder_importer = Arc::new(binder_importer);
    let all_binders = Arc::new(vec![
        Arc::clone(&binder_target),
        Arc::clone(&binder_importer),
    ]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_importer.as_ref(),
        binder_importer.as_ref(),
        &types,
        "importer.js".to_string(),
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            no_lib: true,
            ..Default::default()
        },
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(1);

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((1, "./main".to_string()), 0);
    resolved_module_paths.insert((1, "./main.js".to_string()), 0);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));

    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert("./main".to_string());
    resolved_modules.insert("./main.js".to_string());
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(root_importer);

    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
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
