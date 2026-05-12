//! TS2339 ("Property 'X' does not exist on type 'typeof globalThis'") for
//! `globalThis.X` writes/reads where `X` is a user-declared `let`/`const`.
//!
//! In TypeScript, only `var`/`function`/`class`/etc. declarations become
//! properties of `typeof globalThis`. Block-scoped (`let`/`const`)
//! declarations do NOT — even though they're in the script's top-level scope.
//!
//! Regression: `conformance/es2019/globalThisReadonlyProperties.ts` failed
//! because `resolve_lib_global_var_symbol` walked `lib_symbol_ids` looking for
//! a "shadowed lib `var`", and was matching parameter symbols like the `y` in
//! `Math.atan2(y, x)` from `lib.es5.d.ts`. The parameter has
//! `FUNCTION_SCOPED_VARIABLE` flag (same as a `var`), so the existing flag
//! filter let it through, spoofing a "lib var `y` exists" answer and
//! suppressing the legitimate TS2339 for `globalThis.y`.
//!
//! The fix narrows the lookup to declarations whose syntactic kind is
//! plausibly a global value (not `Parameter`).

use tsz_checker::context::CheckerOptions;

fn diagnostic_codes_with_lib(source: &str) -> Vec<u32> {
    let lib_files = tsz_checker::test_utils::load_lib_files(&["es5.d.ts"]);
    assert!(
        !lib_files.is_empty(),
        "es5.d.ts not found — required for this regression test"
    );
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions::default(),
        &lib_files,
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

/// `const y` is block-scoped and not a property of `typeof globalThis`.
/// `globalThis.y = 4` must report TS2339, even though `lib.es5.d.ts` happens
/// to mention parameters named `y` (e.g. `Math.atan2(y, x)`).
#[test]
fn const_not_property_of_globalthis_writes_emit_ts2339() {
    let source = "const y = 2;\nglobalThis.y = 4;\n";
    let codes = diagnostic_codes_with_lib(source);
    assert!(
        codes.contains(&2339),
        "expected TS2339 for globalThis.y assignment with `const y`, got {codes:?}"
    );
}

/// `var x` IS a property of `typeof globalThis`, so `globalThis.x = 3` must
/// not report TS2339. Pairs with the test above to lock the flag-filter
/// correctness from both directions.
#[test]
fn var_is_property_of_globalthis_writes_no_ts2339() {
    let source = "var x = 1;\nglobalThis.x = 3;\n";
    let codes = diagnostic_codes_with_lib(source);
    assert!(
        !codes.contains(&2339),
        "did not expect TS2339 for globalThis.x assignment with `var x`, got {codes:?}"
    );
}

/// Read access mirrors the write case — `globalThis.y` reads on a `const y`
/// must report TS2339, otherwise the lib-parameter lookup is silently
/// substituting the parameter's type.
#[test]
fn const_not_property_of_globalthis_reads_emit_ts2339() {
    let source = "const y = 2;\nconst zz: number = globalThis.y;\n";
    let codes = diagnostic_codes_with_lib(source);
    assert!(
        codes.contains(&2339),
        "expected TS2339 for globalThis.y read with `const y`, got {codes:?}"
    );
}
