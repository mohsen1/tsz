//! Two distinct JSDoc `@typedef` declarations whose body types intern to the
//! same structural shape must each preserve their OWN alias name in
//! diagnostic messages. Reusing a body-matched DefId across names previously
//! collapsed them, so an assignment to the second alias was reported with
//! the first alias's name.

use tsz_common::options::checker::CheckerOptions;

fn diags_for_strict_eopt_js(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let opts = CheckerOptions {
        check_js: true,
        allow_js: true,
        strict: true,
        strict_null_checks: true,
        exact_optional_property_types: true,
        ..CheckerOptions::default()
    };
    crate::test_utils::check_source(source, "test.js", opts)
}

#[test]
fn ts2375_message_uses_typedef_b_name_when_assigning_to_b() {
    // `A` (multi-property `@property` form) and `B` (inline object literal
    // form) both produce structural body `{ value?: number }`. Each must keep
    // its own alias name in the TS2375 message.
    let diags = diags_for_strict_eopt_js(
        r#"
/**
 * @typedef {object} A
 * @property {number} [value]
 */

/** @type {A} */
const a = { value: undefined };

/**
 * @typedef {{ value?: number }} B
 */

/** @type {B} */
const b = { value: undefined };
"#,
    );
    let ts2375: Vec<&crate::diagnostics::Diagnostic> =
        diags.iter().filter(|d| d.code == 2375).collect();
    assert_eq!(
        ts2375.len(),
        2,
        "Expected exactly two TS2375 diagnostics (one per assignment), got: {ts2375:?}"
    );
    assert!(
        ts2375
            .iter()
            .any(|d| d.message_text.contains("not assignable to type 'A'")),
        "Expected one TS2375 to mention type 'A', got: {ts2375:?}"
    );
    assert!(
        ts2375
            .iter()
            .any(|d| d.message_text.contains("not assignable to type 'B'")),
        "Expected one TS2375 to mention type 'B' (not collapse to 'A'), got: {ts2375:?}"
    );
}

#[test]
fn ts2375_message_uses_typedef_b_name_when_b_declared_first() {
    // Order independence: register B first, then A. B's name still wins for
    // its own assignment site.
    let diags = diags_for_strict_eopt_js(
        r#"
/**
 * @typedef {{ value?: number }} B
 */

/** @type {B} */
const b = { value: undefined };

/**
 * @typedef {object} A
 * @property {number} [value]
 */

/** @type {A} */
const a = { value: undefined };
"#,
    );
    let ts2375: Vec<&crate::diagnostics::Diagnostic> =
        diags.iter().filter(|d| d.code == 2375).collect();
    assert!(
        ts2375
            .iter()
            .any(|d| d.message_text.contains("not assignable to type 'A'")),
        "Expected one TS2375 to mention type 'A', got: {ts2375:?}"
    );
    assert!(
        ts2375
            .iter()
            .any(|d| d.message_text.contains("not assignable to type 'B'")),
        "Expected one TS2375 to mention type 'B', got: {ts2375:?}"
    );
}
