//! Tests for arity computation against a generic conditional/mapped rest tuple.
//!
//! When a signature's trailing rest parameter is a conditional tuple whose
//! branch is selected by the signature's own (still-uninstantiated) type
//! parameter — e.g. `...[opts]: [s] extends [PropertyKey] ? [opts?: Opts] :
//! [opts: Opts]` — `tsc` computes call arity against the erased signature
//! (type parameters → `any`). `[any] extends [PropertyKey]` is true, so the
//! permissive `[opts?: Opts]` branch is chosen and the trailing argument is
//! optional. tsz previously evaluated the conditional with `s` unresolved,
//! collapsing it to the false/required branch `[opts: Opts]` and over-counting
//! the minimum argument count → spurious TS2554. See issue #14326.

use crate::test_utils::check_source_codes as get_error_codes;

/// Repro from arktype: generic function-type value whose conditional rest tuple
/// resolves to the optional branch under erasure. `fn([1, 2])` is one arg and
/// must be accepted.
#[test]
fn test_generic_conditional_rest_optional_branch_function_type() {
    let codes = get_error_codes(
        r#"
type Opts = { a?: number };
type Fn = <s>(head: ReadonlyArray<s>, ...[opts]: [s] extends [PropertyKey] ? [opts?: Opts] : [opts: Opts]) => void;
declare const fn: Fn;
fn([1, 2]);
"#,
    );
    assert!(
        !codes.contains(&2554),
        "Should not emit TS2554 for a generic conditional rest resolving to the optional branch, got: {codes:?}"
    );
}

/// Same shape declared as a generic function declaration.
#[test]
fn test_generic_conditional_rest_optional_branch_declared_fn() {
    let codes = get_error_codes(
        r#"
type Opts = { a?: number };
declare function fn<s>(head: ReadonlyArray<s>, ...[opts]: [s] extends [PropertyKey] ? [opts?: Opts] : [opts: Opts]): void;
fn([1, 2]);
"#,
    );
    assert!(
        !codes.contains(&2554),
        "Should not emit TS2554 for a generic conditional rest declared fn, got: {codes:?}"
    );
}

/// Explicit type argument that satisfies the conditional's true branch is also
/// clean (this path already worked; guards against regressing it).
#[test]
fn test_generic_conditional_rest_explicit_type_arg() {
    let codes = get_error_codes(
        r#"
type Opts = { a?: number };
declare function fn<s>(head: ReadonlyArray<s>, ...[opts]: [s] extends [PropertyKey] ? [opts?: Opts] : [opts: Opts]): void;
fn<number>([1, 2]);
"#,
    );
    assert!(
        !codes.contains(&2554),
        "Should not emit TS2554 with an explicit type argument, got: {codes:?}"
    );
}

/// A constrained type parameter behaves the same — erasure maps `s` to `any`,
/// not to its constraint, so the permissive branch is still selected.
#[test]
fn test_generic_conditional_rest_constrained_param() {
    let codes = get_error_codes(
        r#"
type Opts = { a?: number };
declare function fn<s extends number>(head: ReadonlyArray<s>, ...[opts]: [s] extends [PropertyKey] ? [opts?: Opts] : [opts: Opts]): void;
fn([1, 2]);
"#,
    );
    assert!(
        !codes.contains(&2554),
        "Should not emit TS2554 for a constrained generic conditional rest, got: {codes:?}"
    );
}

/// Negative control: a generic conditional rest whose BOTH branches are
/// required must still over-count — erasure must not silence a genuinely
/// required trailing argument.
#[test]
fn test_generic_conditional_rest_both_branches_required_still_2554() {
    let codes = get_error_codes(
        r#"
type Opts = { a?: number };
declare function fn<s>(head: ReadonlyArray<s>, ...[opts]: [s] extends [PropertyKey] ? [opts: Opts] : [opts: Opts]): void;
fn([1, 2]);
"#,
    );
    assert!(
        codes.contains(&2554),
        "Should still emit TS2554 when the trailing argument is required in both branches, got: {codes:?}"
    );
}

/// Negative control: a plain generic required trailing parameter still reports
/// TS2554 when omitted.
#[test]
fn test_generic_required_trailing_param_still_2554() {
    let codes = get_error_codes(
        r#"
type Opts = { a?: number };
declare function fn<s>(head: ReadonlyArray<s>, opts: Opts): void;
fn([1, 2]);
"#,
    );
    assert!(
        codes.contains(&2554),
        "Should emit TS2554 for a genuinely required trailing parameter, got: {codes:?}"
    );
}

/// Non-generic arity (the common path threaded with an empty type-param slice)
/// is unaffected: a required single-element rest tuple still demands one
/// argument, so a zero-argument call reports TS2554.
#[test]
fn test_concrete_required_rest_tuple_still_2554() {
    let codes = get_error_codes(
        r#"
type Opts = { a?: number };
type D = (...args: [Opts]) => void;
declare const d: D;
d();
"#,
    );
    assert!(
        codes.contains(&2554),
        "Should emit TS2554 for a required rest-tuple element with no arguments, got: {codes:?}"
    );
}

/// Non-generic arity: an optional rest-tuple element accepts the zero-argument
/// call (the empty type-param slice means no erasure round-trip).
#[test]
fn test_concrete_optional_rest_tuple_ok() {
    let codes = get_error_codes(
        r#"
type Opts = { a?: number };
type C = (...args: [opts?: Opts]) => void;
declare const c: C;
c();
"#,
    );
    assert!(
        !codes.contains(&2554),
        "Should not emit TS2554 for an optional rest-tuple element, got: {codes:?}"
    );
}
