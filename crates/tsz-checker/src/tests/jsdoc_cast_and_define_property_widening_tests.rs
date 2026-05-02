//! Regression tests for JSDoc `@type` casts on parenthesized expressions.
//!
//! `literal_type_from_initializer` previously walked through a parenthesized
//! initializer to find an inner literal even when the paren carried an inline
//! JSDoc `@type` cast. For `const c = /** @type {T} */(literal)` this
//! stripped the cast and gave `c` the inner literal type instead of `T`.
//!
//! The fix consults a new `paren_has_jsdoc_type_cast` helper that returns
//! `true` iff the paren has a leading `@type` JSDoc tag, and short-circuits
//! literal extraction in that case.
//!
//! A companion fix widens fresh literal types flowing out of
//! `Object.defineProperty(...)` accessor inference so getters/setters mirror
//! the widening already applied to data descriptors. That fix is exercised
//! by `conformance/jsdoc/checkObjectDefineProperty.ts` because it requires
//! the lib globals (Object, etc.) which are unavailable in unit tests.

use tsz_common::options::checker::CheckerOptions;

fn diags_for_strict_js(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let opts = CheckerOptions {
        check_js: true,
        allow_js: true,
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    crate::test_utils::check_source(source, "test.js", opts)
}

/// Primary: `const c = /** @type {string} */(/** @type {*} */(null))` gives
/// `c` type `string`, not `null`. Calling `take(c)` where `take` expects
/// `string` must not emit TS2345.
#[test]
fn jsdoc_type_cast_through_any_preserves_cast_type() {
    let source = "\
const c = /** @type {string} */(/** @type {*} */(null));
/** @param {string} s */
function take(s) {}
take(c);
";
    let diags = diags_for_strict_js(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2345),
        "TS2345 must not fire — c should have cast type 'string', not 'null'. Got: {diags:#?}",
    );
}

/// Single cast variant: `const c = /** @type {string} */(...)` must not
/// freeze `c` to the inner literal `"hello"`.
#[test]
fn jsdoc_type_cast_does_not_propagate_inner_literal_to_const_init() {
    let source = "\
const c = /** @type {string} */(/** @type {*} */(\"hello\"));
/** @param {string} s */
function take(s) {}
take(c);
";
    let diags = diags_for_strict_js(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2345),
        "TS2345 must not fire — cast should give c type 'string'. Got: {diags:#?}",
    );
}

/// Anti-hardcoding cover: identifier name `result` (not `c`), variant cast
/// to `number`, inner cast through `*` to a non-numeric literal. Confirms
/// the fix doesn't key off a specific name or shape.
#[test]
fn jsdoc_type_cast_works_with_renamed_binding_and_number_target() {
    let source = "\
const result = /** @type {number} */(/** @type {*} */(\"abc\"));
/** @param {number} n */
function consume(n) {}
consume(result);
";
    let diags = diags_for_strict_js(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2345),
        "TS2345 must not fire — cast should give result type 'number'. Got: {diags:#?}",
    );
}

/// Negative control: when the JSDoc cast type genuinely doesn't match the
/// usage site, errors are still emitted. Confirms the fix didn't accidentally
/// suppress all type checks for casts.
#[test]
fn jsdoc_type_cast_to_string_still_errors_when_used_as_number() {
    let source = "\
const c = /** @type {string} */(/** @type {*} */(null));
/** @param {number} n */
function take(n) {}
take(c);
";
    let diags = diags_for_strict_js(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2345),
        "Expected TS2345 (string passed where number required), got codes: {codes:?}\nDiagnostics: {diags:#?}",
    );
}
