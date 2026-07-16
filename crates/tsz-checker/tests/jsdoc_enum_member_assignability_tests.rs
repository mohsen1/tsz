//! JSDoc `@enum {T}` under TypeScript 7.
//!
//! TypeScript 7 dropped `@enum` type synthesis: the tag creates only a value
//! binding, contributing no element type. Two consequences, both oracle-checked
//! against the pinned tsc 7.0.2 (`allowJs`/`checkJs`/`strict`):
//!   * The object-literal members are no longer validated against the tag's
//!     element type — a member whose value is unassignable to `T` produces no
//!     TS2322 (previously issue #9761 locked a per-member elaboration that the
//!     7.0 compiler no longer performs).
//!   * A bare reference to the enum name in a JSDoc type position is the TS2749
//!     value-used-as-type error.
//!
//! Oracle witness (`tsc 7.0.2`, `strict`):
//! ```text
//! /** @enum {number} */
//! const E = { A: 0, B: "wrong" };   // no diagnostic
//! /** @type {E} */ var e;           // TS2749 at `E`
//! ```

use tsz_checker::CheckerOptions;
use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::test_utils::check_source;

fn js_check_options() -> CheckerOptions {
    CheckerOptions {
        allow_js: true,
        check_js: true,
        strict: true,
        ..Default::default()
    }
}

fn ts2322_count(diagnostics: &[tsz_checker::diagnostics::Diagnostic]) -> usize {
    diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .count()
}

#[test]
fn member_mismatch_is_not_validated() {
    // TS7: `@enum {number}` with a string member is not per-member validated.
    // tsc 7.0.2 emits no TS2322 (the tag no longer contributes an element type
    // for the object literal to be checked against).
    let source = "/** @enum {number} */\nconst E = { A: 0, B: \"wrong\" };\n";
    let diagnostics = check_source(source, "repro.js", js_check_options());
    assert_eq!(
        ts2322_count(&diagnostics),
        0,
        "TS7 drops @enum member validation, got: {diagnostics:#?}"
    );
}

#[test]
fn symmetric_enum_string_with_numeric_member_is_not_validated() {
    // `@enum {string}` with a numeric member: same rule, source/target swapped.
    let source = "/** @enum {string} */\nconst E = { A: \"a\", B: 42 };\n";
    let diagnostics = check_source(source, "sym.js", js_check_options());
    assert_eq!(
        ts2322_count(&diagnostics),
        0,
        "TS7 drops @enum member validation, got: {diagnostics:#?}"
    );
}

#[test]
fn multiple_offending_members_are_not_validated() {
    // Adjacent case: several unassignable members still produce no TS2322.
    let source = "/** @enum {number} */\nconst Foo = { Bar: 1, Baz: \"no\", Qux: true };\n";
    let diagnostics = check_source(source, "multi.js", js_check_options());
    assert_eq!(
        ts2322_count(&diagnostics),
        0,
        "TS7 drops @enum member validation, got: {diagnostics:#?}"
    );
}

#[test]
fn renamed_enum_and_members_unchanged() {
    // Anti-hardcoding guard: the structural rule keys on the `@enum` tag, not
    // any specific identifier. Renaming must not resurrect member validation.
    let source = "/** @enum {number} */\nconst PaletteSlot = { Primary: 0, Accent: \"bad\" };\n";
    let diagnostics = check_source(source, "renamed.js", js_check_options());
    assert_eq!(
        ts2322_count(&diagnostics),
        0,
        "TS7 drops @enum member validation, got: {diagnostics:#?}"
    );
}

#[test]
fn object_freeze_wrapper_is_not_validated() {
    // `Object.freeze({...})` @enum is likewise not member-validated.
    let source = "/** @enum {number} */\nconst F = Object.freeze({ A: 0, B: \"wrong\" });\n";
    let diagnostics = check_source(source, "freeze.js", js_check_options());
    assert_eq!(
        ts2322_count(&diagnostics),
        0,
        "Object.freeze-wrapped @enum is not member-validated, got: {diagnostics:#?}"
    );
}

#[test]
fn all_matching_members_produce_no_error() {
    // Negative control: a fully-conforming `@enum` still emits nothing.
    let source = "/** @enum {number} */\nconst Ok = { A: 1, B: 2, C: 3 };\n";
    let diagnostics = check_source(source, "ok.js", js_check_options());
    assert_eq!(ts2322_count(&diagnostics), 0, "got: {diagnostics:#?}");
}

#[test]
fn bare_enum_name_in_type_position_is_ts2749() {
    // TS7: the enum name carries only value meaning, so a bare JSDoc type
    // reference to it is the value-used-as-type error (TS2749), anchored at the
    // name. Oracle: `repro.js(3,12): error TS2749: 'E' refers to a value …`.
    let source = "/** @enum {number} */\nconst E = { A: 0, B: 1 };\n/** @type {E} */\nvar e;\n";
    let diagnostics = check_source(source, "use.js", js_check_options());
    let ts2749: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.code
                == diagnostic_codes::REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF
        })
        .collect();
    assert_eq!(
        ts2749.len(),
        1,
        "bare @enum name in a type position is TS2749, got: {diagnostics:#?}"
    );
    let expected_start = source.rfind('E').expect("test source malformed") as u32;
    assert_eq!(
        ts2749[0].start, expected_start,
        "TS2749 must anchor at the enum name in the type annotation",
    );
    assert_eq!(ts2322_count(&diagnostics), 0, "no member validation in TS7");
}
