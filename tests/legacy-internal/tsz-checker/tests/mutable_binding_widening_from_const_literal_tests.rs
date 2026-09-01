//! Regression tests for #3446: mutable bindings (`let`/`var`) initialized from
//! an unannotated `const` literal must widen to the primitive type.
//!
//! Structural rule: when a mutable binding's initializer is an identifier
//! resolving to an unannotated `const` declaration whose initializer is itself
//! a fresh literal expression, that identifier is also a fresh literal
//! expression for widening purposes (tsc's "widening literal type" semantics).

use crate::test_utils::{check_source_strict_codes, strict_checker_options};

/// `let m = tag;` where `const tag = "start";` widens to `string`.
///
/// Subsequent assignment of a different string literal must succeed (no
/// TS2322), and assigning the binding back to a literal-typed slot must
/// fail because the binding is `string`, not `"start"`.
#[test]
fn let_widens_when_initialized_from_unannotated_const_string_literal() {
    let source = "\
const tag = \"start\";
let mutable = tag;

mutable = \"next\";

const exact: \"start\" = mutable;
";
    let codes = check_source_strict_codes(source);
    assert_eq!(
        codes,
        vec![2322],
        "expected exactly one TS2322 (the final literal-typed assignment), got: {codes:?}",
    );
}

/// Same shape with a numeric literal — widens to `number`.
#[test]
fn let_widens_when_initialized_from_unannotated_const_numeric_literal() {
    let source = "\
const n = 1;
let m = n;
m = 2;
const exact: 1 = m;
";
    let codes = check_source_strict_codes(source);
    assert_eq!(
        codes,
        vec![2322],
        "expected exactly one TS2322 (the literal-typed final), got: {codes:?}",
    );
}

/// Same shape with `var`, which has the same widening semantics.
#[test]
fn var_widens_when_initialized_from_unannotated_const_string_literal() {
    let source = "\
const tag = \"start\";
var mutable = tag;
mutable = \"next\";
";
    let codes = check_source_strict_codes(source);
    assert!(
        !codes.contains(&2322),
        "var initialized from an unannotated const literal must widen to string, \
         so `mutable = \"next\"` must be accepted. Got: {codes:?}",
    );
}

/// Transitivity: an unannotated const initialized from another unannotated
/// const literal still propagates widening when copied into a `let`.
#[test]
fn let_widens_through_chain_of_unannotated_const_literals() {
    let source = "\
const a = \"x\";
const b = a;
const c = b;
let m = c;
m = \"y\";
";
    let codes = check_source_strict_codes(source);
    assert!(
        !codes.contains(&2322),
        "let copied from a chain of unannotated const literals must still widen; \
         `m = \"y\"` must be accepted. Got: {codes:?}",
    );
}

/// Negative: an explicitly literal-typed `const` (with a type annotation)
/// must NOT widen — the user opted out of widening by annotating.
#[test]
fn let_does_not_widen_when_initialized_from_typed_const_literal() {
    let source = "\
const tag: \"start\" = \"start\";
let mutable = tag;
mutable = \"next\";
";
    let codes = check_source_strict_codes(source);
    assert!(
        codes.contains(&2322),
        "let copied from an explicitly literal-typed const must keep the literal \
         type; `mutable = \"next\"` must be rejected. Got: {codes:?}",
    );
}

/// Negative: a `let` source is non-fresh, so copying into another `let`
/// must keep `string` (no double-widening surprises). This exercises the
/// "don't follow non-const declarations" branch of the predicate.
#[test]
fn let_initialized_from_let_does_not_use_widening_literal_path() {
    // `let outer = "a"` — outer is already `string`, so `inner` is `string`
    // regardless of any widening rule. The point of this test is to confirm
    // we don't accidentally treat a `let` source as a widening literal.
    let source = "\
let outer = \"a\";
let inner = outer;
inner = \"b\";
const exact: \"a\" = inner;
";
    let codes = check_source_strict_codes(source);
    assert!(
        codes.contains(&2322),
        "inner must be string, so the const literal-typed assignment fails. \
         Got: {codes:?}",
    );
}

/// The recursion through identifier references must terminate even for
/// pathological `const a = a` self-references. This source is itself a TDZ
/// error; the test only asserts the checker does not loop.
#[test]
fn const_self_reference_does_not_loop_through_freshness_predicate() {
    let source = "\
const a = a;
let m = a;
";
    // Just exercise the path; we do not assert specific codes here.
    let _ = crate::test_utils::check_source(source, "test.ts", strict_checker_options());
}
