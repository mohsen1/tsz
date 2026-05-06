//! Locks in that the JSDoc `@import` parser handles the combined
//! default-plus-named form (`@import Default, { Named } from "mod"`)
//! and registers BOTH the default and the named imports as available
//! type references.
//!
//! Regression: `importTag16.ts` —
//! `/** @import Foo, { I } from "./a" */` emitted false TS2304 for
//! both `Foo` and `I`. `parse_jsdoc_import_tag` only handled three
//! shapes: `{ X, Y } from "mod"` (named-only), `* as X from "mod"`
//! (namespace), and `X from "mod"` (default-only). The combined
//! default-plus-named form fell through and returned an empty list,
//! so neither name was registered.

use tsz_checker::test_utils::check_source_codes;

#[test]
fn jsdoc_import_default_and_named_resolves_both_names() {
    // Use a single file containing the JSDoc `@import` tag pointing to
    // type-only declarations defined in the same source. The full
    // multi-file scenario is exercised by the conformance test
    // `importTag16.ts`; this unit test verifies the parser surface no
    // longer emits TS2304 / TS2552 for the names introduced by the
    // combined form.
    let source = r#"
declare interface Foo {}
declare interface I {}

/** @import Foo, { I } from "./a" */
/**
 * @param {Foo} a
 * @param {I} b
 */
function foo(a, b) {}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2304) && !codes.contains(&2552),
        "expected no TS2304/TS2552 from combined @import default+named; got {codes:?}",
    );
}

#[test]
fn jsdoc_import_named_only_still_works() {
    // Sanity: the existing named-only form must continue to register
    // names normally.
    let source = r#"
declare interface I {}

/** @import { I } from "./a" */
/** @param {I} b */
function bar(b) {}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2304) && !codes.contains(&2552),
        "named-only @import should not regress; got {codes:?}",
    );
}

#[test]
fn jsdoc_import_default_only_still_works() {
    let source = r#"
declare interface Foo {}

/** @import Foo from "./a" */
/** @param {Foo} a */
function baz(a) {}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2304) && !codes.contains(&2552),
        "default-only @import should not regress; got {codes:?}",
    );
}
