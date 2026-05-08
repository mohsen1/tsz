//! Issue #3133: a JSDoc `@typedef` in a global-script JS file (no
//! imports/exports) whose name collides with a lib global must emit
//! TS2300 "Duplicate identifier" at the typedef site, mirroring tsc.
//!
//! Repro from the issue: `// @ts-check` JS file with
//! `/** @typedef {string} Object */` is expected to surface
//! `TS2300: Duplicate identifier 'Object'.` because the lib defines the
//! global `Object` type/value.

use std::path::Path;
use std::sync::Arc;
use tsz_binder::state::LibContext as BinderLibContext;
use tsz_binder::{BinderState, lib_loader::LibFile};
use tsz_checker::context::CheckerOptions;
use tsz_checker::context::LibContext as CheckerLibContext;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn load_es5_lib_files() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_paths = [
        manifest_dir.join("../../TypeScript/lib/lib.es5.d.ts"),
        manifest_dir.join("scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
    ];
    let mut lib_files = Vec::new();
    for lib_path in &lib_paths {
        if lib_path.exists()
            && let Ok(content) = std::fs::read_to_string(lib_path)
        {
            let file_name = lib_path.file_name().unwrap().to_string_lossy().to_string();
            let lib_file = LibFile::from_source(file_name, content);
            lib_files.push(Arc::new(lib_file));
        }
    }
    lib_files
}

fn check_js_with_lib(source: &str, file_name: &str) -> Vec<Diagnostic> {
    let lib_files = load_es5_lib_files();
    assert!(
        !lib_files.is_empty(),
        "Expected to find lib.es5.d.ts for the test"
    );

    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    let lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| BinderLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    binder.merge_lib_contexts_into_binder(&lib_contexts);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..Default::default()
    };

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );
    let checker_lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| CheckerLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(checker_lib_contexts);
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

/// `@typedef Object` in a global-script JS file collides with the lib
/// `Object` global → TS2300 must fire on the typedef name.
#[test]
fn jsdoc_typedef_object_in_global_script_js_emits_ts2300() {
    let source = "// @ts-check\n\n/** @typedef {string} Object */\n";
    let diags = check_js_with_lib(source, "lib_collide.js");

    let object_dups: Vec<&Diagnostic> = diags
        .iter()
        .filter(|d| d.code == 2300 && d.message_text.contains("'Object'"))
        .collect();
    assert!(
        !object_dups.is_empty(),
        "expected TS2300 for `@typedef Object` colliding with lib global, got: {diags:?}"
    );
}

/// Same shape but with a non-lib name (`MyAlias`) must NOT emit TS2300 —
/// guards against the fix over-triggering on every typedef.
#[test]
fn jsdoc_typedef_non_lib_name_does_not_emit_ts2300() {
    let source = "// @ts-check\n\n/** @typedef {string} MyAlias */\n";
    let diags = check_js_with_lib(source, "no_collide.js");

    let dups: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == 2300).collect();
    assert!(
        dups.is_empty(),
        "expected NO TS2300 for non-lib-colliding typedef, got: {dups:?}"
    );
}

/// A different lib global (`Promise`) shadowed by a typedef should also
/// emit TS2300 — verifies the rule is structural, not name-specific.
#[test]
fn jsdoc_typedef_promise_in_global_script_js_emits_ts2300() {
    let source = "// @ts-check\n\n/** @typedef {string} Promise */\n";
    let diags = check_js_with_lib(source, "promise_collide.js");

    let promise_dups: Vec<&Diagnostic> = diags
        .iter()
        .filter(|d| d.code == 2300 && d.message_text.contains("'Promise'"))
        .collect();
    assert!(
        !promise_dups.is_empty(),
        "expected TS2300 for `@typedef Promise` colliding with lib global, got: {diags:?}"
    );
}
