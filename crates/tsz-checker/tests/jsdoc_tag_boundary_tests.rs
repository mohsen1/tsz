//! Regression tests for JSDoc tag prefix matching.
//!
//! Issue #2916: tag detection paths used raw substring/prefix checks instead
//! of requiring an identifier boundary. Longer invalid tag names such as
//! `@satisfiesx`, `@importx`, `@overridex`, `@thisx`, `@typedefx`, and
//! `@templateX` were therefore treated as their shorter cousins. tsc rejects
//! all of these as unknown tags.
//!
//! Each test exercises one of the cited prefix-bug scenarios. The boundary
//! rule must hold regardless of which suffix character appears, so where
//! practical we cover multiple suffixes.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn check_js(source: &str) -> Vec<u32> {
    check_source(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

#[test]
fn satisfiesx_prefix_does_not_trigger_satisfies_check() {
    let source = r#"
// @ts-check

/** @satisfiesx {{ a: number }} */
const value = { a: 1, b: 2 };

value;
"#;
    let codes = check_js(source);
    assert!(
        !codes.contains(&2353) && !codes.contains(&1005),
        "@satisfiesx should not be parsed as @satisfies; got codes {codes:?}"
    );
}

#[test]
fn satisfiesy_prefix_does_not_trigger_satisfies_check() {
    let source = r#"
// @ts-check

/** @satisfiesy {{ a: number }} */
const value = { a: 1, b: 2 };

value;
"#;
    let codes = check_js(source);
    assert!(
        !codes.contains(&2353) && !codes.contains(&1005),
        "@satisfiesy should not be parsed as @satisfies; got codes {codes:?}"
    );
}

#[test]
fn satisfies_with_trailing_punctuation_still_matches() {
    // Sanity check: tsc treats `@satisfies{...}` (no space) as a valid
    // satisfies tag. The boundary rule must not break the canonical form.
    let source = r#"
// @ts-check

/** @satisfies {{ a: number }} */
const value = { a: 1, b: 2 };

value;
"#;
    let codes = check_js(source);
    assert!(
        codes.contains(&2353),
        "Real @satisfies should still trigger excess-property check; got codes {codes:?}"
    );
}

#[test]
fn importx_prefix_does_not_introduce_alias() {
    // tsc rejects `@importx` as an unknown tag, so `Foo` is not bound and
    // `@type {Foo}` cannot resolve. This must surface TS2304.
    let source = r#"
// @ts-check

/**
 * @importx { Foo } from "./types"
 */

/** @type {Foo} */
const value = { n: 1 };

value;
"#;
    let codes = check_js(source);
    assert!(
        codes.contains(&2304),
        "@importx should not bind import alias; expected TS2304, got {codes:?}"
    );
}

#[test]
fn typedefx_prefix_does_not_create_typedef() {
    let source = r#"
// @ts-check

/**
 * @typedefx {{ n: number }} Foo
 */

/** @type {Foo} */
const value = { n: 1 };

value;
"#;
    let codes = check_js(source);
    assert!(
        codes.contains(&2304),
        "@typedefx must not create a typedef binding; expected TS2304, got {codes:?}"
    );
    assert!(
        !codes.contains(&8021),
        "@typedefx must not raise TS8021 malformed-typedef; got {codes:?}"
    );
}

#[test]
fn templatey_prefix_does_not_create_type_parameter() {
    // `@templateY` is not `@template`; the function below should not gain a
    // type parameter named `T`. tsc reports TS2304 for the unresolved name.
    let source = r#"
// @ts-check

/**
 * @templateY T
 * @param {T} x
 * @returns {T}
 */
function id(x) { return x; }

id(1);
"#;
    let codes = check_js(source);
    assert!(
        codes.contains(&2304),
        "@templateY must not introduce type parameter T; expected TS2304, got {codes:?}"
    );
}

#[test]
fn thisx_prefix_does_not_set_this_type() {
    // With `@thisx`, tsc reports the implicit-`this` diagnostic TS2683.
    let source = r#"
// @ts-check

/**
 * @thisx {{ n: number }}
 */
function f() {
  this.n;
}

f;
"#;
    let codes = check_js(source);
    assert!(
        codes.contains(&2683),
        "@thisx must not suppress implicit-this TS2683; got {codes:?}"
    );
}

#[test]
fn satisfies_underscore_suffix_does_not_match() {
    // Identifier-continuation includes underscore, so `@satisfies_alias` is
    // not the `@satisfies` tag.
    let source = r#"
// @ts-check

/** @satisfies_alias {{ a: number }} */
const value = { a: 1, b: 2 };

value;
"#;
    let codes = check_js(source);
    assert!(
        !codes.contains(&2353),
        "@satisfies_alias must not trigger excess-property check; got {codes:?}"
    );
}
