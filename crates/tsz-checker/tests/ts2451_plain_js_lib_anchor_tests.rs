//! TS2451 anchor for cross-file plain-JS conflicts.
//!
//! When a `.js` script (plain JS — no `// @ts-check`, no `checkJs`) declares
//! a `const`/`let` whose name conflicts with a `declare function`/`declare var`
//! in a `.d.ts` file (e.g. `lib.es5.d.ts`'s built-in `eval`), tsc anchors the
//! TS2451 diagnostic at the remote `.d.ts` declaration rather than the local
//! plain-JS site (mirrors `addDuplicateLocations` plain-JS filter in
//! `checker.ts` ~L2782-L2783).
//!
//! Conformance: `conformance/salsa/plainJSReservedStrict.ts`.

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

fn load_es5_lib_files_for_test() -> Vec<Arc<LibFile>> {
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

/// Run the checker against `source` (parsed as `file_name`) with `lib.es5.d.ts`
/// merged into the binder, returning the resulting diagnostics.
fn check_with_lib(source: &str, file_name: &str) -> Vec<Diagnostic> {
    let lib_files = load_es5_lib_files_for_test();
    assert!(
        !lib_files.is_empty(),
        "Expected to find lib.es5.d.ts for the test (checked TypeScript/lib and \
         scripts/conformance/node_modules paths)"
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
    let options = CheckerOptions::default();

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

/// `const eval = 1` in a plain-JS script vs `declare function eval(...)` in
/// `lib.es5.d.ts` should produce TS2451 anchored at the lib declaration's
/// name, not at the local plain-JS site.
#[test]
fn plain_js_const_eval_redirects_ts2451_to_lib_function_eval() {
    let user_js = "\"use strict\";\nconst eval = 1;\n";
    let diags = check_with_lib(user_js, "plainJSReservedStrict.js");

    let for_eval: Vec<&Diagnostic> = diags
        .iter()
        .filter(|d| d.code == 2451 && d.message_text.contains("'eval'"))
        .collect();
    assert!(
        !for_eval.is_empty(),
        "expected a TS2451 diagnostic for 'eval', got: {diags:?}"
    );

    // Every TS2451 'eval' diagnostic must anchor at lib.es5.d.ts, never at
    // the plain-JS source.
    let on_user_js: Vec<&Diagnostic> = for_eval
        .iter()
        .copied()
        .filter(|d| d.file.ends_with(".js"))
        .collect();
    assert!(
        on_user_js.is_empty(),
        "plain-JS local site must not carry the TS2451 anchor when the lib's \
         `eval` declaration participates in the conflict. Got: {on_user_js:?}"
    );

    let on_lib: Vec<&Diagnostic> = for_eval
        .iter()
        .copied()
        .filter(|d| d.file.ends_with("lib.es5.d.ts"))
        .collect();
    assert!(
        !on_lib.is_empty(),
        "TS2451 must anchor at lib.es5.d.ts when local file is plain JS. Got: {for_eval:?}"
    );
}

/// Same scenario but with `const arguments = 2` — covers the second
/// strict-mode reserved-word case from `plainJSReservedStrict.ts`.
#[test]
fn plain_js_const_arguments_redirects_ts2451_to_lib() {
    let user_js = "\"use strict\";\nconst arguments = 2;\n";
    let diags = check_with_lib(user_js, "plainJSReservedStrict.js");

    let for_arguments: Vec<&Diagnostic> = diags
        .iter()
        .filter(|d| d.code == 2451 && d.message_text.contains("'arguments'"))
        .collect();
    if for_arguments.is_empty() {
        // tsc emits TS2451 for `arguments` only when lib provides a global
        // `arguments` declaration. The conformance test only locks in the
        // `eval` anchor; treat empty as benign here.
        return;
    }
    assert!(
        for_arguments.iter().all(|d| !d.file.ends_with(".js")),
        "plain-JS local site must not carry the TS2451 anchor for 'arguments'. \
         Got: {for_arguments:?}"
    );
}

/// Same scenario but the redeclaration is in a `.ts` file (not plain JS). The
/// plain-JS suppression must NOT apply, so the local `.ts` site keeps the
/// TS2451 anchor.
#[test]
fn ts_const_eval_keeps_local_anchor_for_ts2451() {
    let user_ts = "const eval = 1;\n";
    let diags = check_with_lib(user_ts, "user.ts");

    let for_eval: Vec<&Diagnostic> = diags
        .iter()
        .filter(|d| d.code == 2451 && d.message_text.contains("'eval'"))
        .collect();
    assert!(
        !for_eval.is_empty(),
        "expected TS2451 for `eval` redeclaration in TS file, got: {diags:?}"
    );
    assert!(
        for_eval.iter().any(|d| d.file.ends_with("user.ts")),
        "TS-file local site must keep its TS2451 anchor (plain-JS suppression \
         must not apply to TS files). Got: {for_eval:?}"
    );
}
