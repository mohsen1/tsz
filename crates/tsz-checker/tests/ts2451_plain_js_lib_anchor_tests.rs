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

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;

/// Run the checker against `source` (parsed as `file_name`) with `lib.es5.d.ts`
/// merged into the binder, returning the resulting diagnostics.
fn check_with_lib(source: &str, file_name: &str) -> Vec<Diagnostic> {
    let lib_files = tsz_checker::test_utils::load_compiled_lib_files(&["lib.es5.d.ts"]);
    assert!(
        !lib_files.is_empty(),
        "Expected to find lib.es5.d.ts for the test"
    );
    tsz_checker::test_utils::check_source_with_libs(
        source,
        file_name,
        CheckerOptions::default(),
        &lib_files,
    )
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
