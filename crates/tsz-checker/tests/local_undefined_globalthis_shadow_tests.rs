//! Local declarations named `undefined` or `globalThis` shadow the built-in
//! globals. Flow narrowing and globalThis-property-access paths must resolve
//! the identifier to a symbol and only treat lib (global) resolutions as the
//! built-in. A same-file local (parameter, `const`, etc.) is a regular value.
//!
//! Regression coverage for #2885.

use tsz_checker::context::CheckerOptions;
use tsz_common::checker_options::JsxMode;

fn diag_codes(source: &str) -> Vec<u32> {
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();
    tsz_checker::test_utils::check_source(source, "test.ts", opts)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn local_undefined_parameter_does_not_narrow_via_nullish_comparison() {
    // `undefined` is bound as the first parameter (literal type `1`). Inside
    // the function body, `value === undefined` compares against that local
    // parameter, not the global `undefined` sentinel, so `value` narrows to
    // `1`. Assigning the narrowed value to `string` must error with TS2322.
    let source = r#"
function f(undefined: 1, value: string | 1) {
  if (value === undefined) {
    const asOne: 1 = value;
    const asString: string = value;
    asOne;
    asString;
  }
}
f;
"#;
    let codes = diag_codes(source);
    assert!(
        codes.contains(&2322),
        "Expected TS2322 because local `undefined` parameter narrows `value` to `1`, got: {codes:?}"
    );
}

#[test]
fn local_undefined_const_in_module_does_not_narrow_via_nullish_comparison() {
    // Same structural rule, different declaration shape (module-local
    // `const undefined`). Verifies the fix is not parameter-specific.
    let source = r#"
const undefined: 2 = 2;
function f(value: string | 2) {
  if (value === undefined) {
    const asTwo: 2 = value;
    const asString: string = value;
    asTwo;
    asString;
  }
}
f;

export {};
"#;
    let codes = diag_codes(source);
    assert!(
        codes.contains(&2322),
        "Expected TS2322 because module-local `const undefined` narrows `value` to `2`, got: {codes:?}"
    );
}

#[test]
fn module_local_globalthis_property_access_does_not_emit_ts7017() {
    // Module-local `const globalThis` shadows the built-in global. Property
    // access on it must resolve through the local's structural type, not the
    // synthetic `typeof globalThis` table. tsc emits no diagnostics.
    let source = r#"
export {};

const globalThis = { y: 1 };

globalThis.y.toFixed();
"#;
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&7017),
        "Expected no TS7017 because local `const globalThis` shadows the global, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2339),
        "Expected no TS2339 for `y` since it is on the local object, got: {codes:?}"
    );
}

#[test]
fn module_local_globalthis_via_let_also_shadows() {
    // Same rule for `let` declarations. The structural test is "any local
    // value declaration named globalThis", not "any const named globalThis".
    let source = r#"
export {};

let globalThis: { y: number } = { y: 1 };

globalThis.y.toFixed();
"#;
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&7017),
        "Expected no TS7017 because local `let globalThis` shadows the global, got: {codes:?}"
    );
}
