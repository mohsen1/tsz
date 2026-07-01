//! Issue #3282: TYPE-position lookups must surface spelling suggestions
//! for core lib globals (`Array`, `Promise`, `Map`, ...). tsz used to
//! suppress every lib-origin candidate for TYPE-only lookups, so typos
//! like `Arrray`, `Prommise`, `Mapp` reported plain TS2304 instead of
//! tsc's TS2552 with a "Did you mean 'Array'?" suggestion.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;

fn check_with_es2015(source: &str) -> Vec<Diagnostic> {
    let lib_files = tsz_checker::test_utils::load_compiled_lib_files(&[
        "lib.es5.d.ts",
        "lib.es2015.d.ts",
        "lib.es2015.collection.d.ts",
        "lib.es2015.core.d.ts",
        "lib.es2015.iterable.d.ts",
        "lib.es2015.promise.d.ts",
        "lib.es2015.symbol.d.ts",
        "lib.es2015.symbol.wellknown.d.ts",
    ]);
    assert!(
        !lib_files.is_empty(),
        "expected lib.es*.d.ts to be available"
    );
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "repro.ts",
        CheckerOptions::default(),
        &lib_files,
    )
}

fn finds_suggestion(diags: &[Diagnostic], typo: &str, suggestion: &str) -> bool {
    diags.iter().any(|d| {
        d.code == 2552
            && d.message_text.contains(&format!("'{typo}'"))
            && d.message_text.contains(&format!("'{suggestion}'"))
    })
}

/// `Arrray` should suggest `Array` from the core lib.
#[test]
fn type_position_arrray_suggests_array() {
    let diags = check_with_es2015("let a: Arrray;\n");
    assert!(
        finds_suggestion(&diags, "Arrray", "Array"),
        "expected TS2552 'Arrray' -> 'Array', got: {diags:?}"
    );
}

/// `Prommise` should suggest `Promise`.
#[test]
fn type_position_prommise_suggests_promise() {
    let diags = check_with_es2015("let p: Prommise<string>;\n");
    assert!(
        finds_suggestion(&diags, "Prommise", "Promise"),
        "expected TS2552 'Prommise' -> 'Promise', got: {diags:?}"
    );
}

/// `Mapp` should suggest `Map`.
#[test]
fn type_position_mapp_suggests_map() {
    let diags = check_with_es2015("let m: Mapp<string, number>;\n");
    assert!(
        finds_suggestion(&diags, "Mapp", "Map"),
        "expected TS2552 'Mapp' -> 'Map', got: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Value- vs type-position keyword candidates (type-position half).
//
// The `VALUE` and `TYPE` symbol-flag masks overlap (both include
// `CLASS`/`ENUM`/`ENUM_MEMBER`), so gating the built-in intrinsic keywords on
// `meaning & TYPE` used to leak the lowercase *type* keywords (`number`,
// `boolean`, ...) into *value*-position lookups, and never offered the global
// *value* symbol `undefined`. The value-position half is exercised end-to-end
// in `crates/tsz-cli/tests/cannot_find_name_value_suggestion_cli_tests.rs`
// (the in-process checker harness here does not report value-position
// `cannot find name`). These two guards lock the *type*-position side: the
// intrinsic type keywords stay available, and `undefined` — a value symbol,
// not a type symbol — is never offered in type position.
// ---------------------------------------------------------------------------

/// True when the diagnostics contain a bare TS2304 for `typo` and do NOT offer
/// any TS2552 spelling suggestion for it.
fn is_bare_not_found(diags: &[Diagnostic], typo: &str) -> bool {
    let mentions = |code: u32| {
        diags
            .iter()
            .any(|d| d.code == code && d.message_text.contains(&format!("'{typo}'")))
    };
    mentions(2304) && !mentions(2552)
}

/// `undefined` is NOT a type-meaning symbol, so a type-position typo near it
/// stays a bare TS2304 (tsc reports no suggestion for `let x: undefindd`).
#[test]
fn type_position_undefined_typo_has_no_suggestion() {
    let diags = check_with_es2015("let x: undefindd;\n");
    assert!(
        is_bare_not_found(&diags, "undefindd"),
        "expected bare TS2304 for 'undefindd' in type position, got: {diags:?}"
    );
}

/// Regression guard: fixing the value-position leak must NOT remove the
/// (correct) type-keyword suggestions in *type* position — `sting` → `string`.
#[test]
fn type_position_type_keyword_still_suggested() {
    let diags = check_with_es2015("let s: sting;\n");
    assert!(
        finds_suggestion(&diags, "sting", "string"),
        "expected TS2552 'sting' -> 'string', got: {diags:?}"
    );
}
