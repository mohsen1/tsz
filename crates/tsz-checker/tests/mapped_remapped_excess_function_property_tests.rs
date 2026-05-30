//! Regression tests for a false-positive `TS2322` on object-literal assignment.
//!
//! Structural rule: when an object literal is assigned to a key-remapping mapped
//! type (`{ [K in keyof T as <name-type>]: ... }`) and the literal carries an
//! *excess* property — one that the remapping does not produce — whose value is
//! a **function-like** expression (arrow, function expression, or method
//! shorthand), `tsc` reports only `TS2353` (object literal may only specify
//! known properties). It must not additionally report `TS2322` against `never`.
//!
//! tsz previously synthesized a `never` contextual type for absent function-like
//! properties (used purely to drive parameter implicit-any handling) and then,
//! for remapped mapped targets, re-checked the property value against that
//! synthetic `never`, emitting a spurious `TS2322: Type '() => void' is not
//! assignable to type 'never'`. The fix marks that `never` as a sentinel and
//! skips the re-check for it.
//!
//! The rule is independent of the iteration-variable name, the filter style
//! (value-based vs key-based vs rename), and the function-expression flavor.

use tsz_checker::test_utils::check_source_strict_codes;

fn codes_ignoring_lib_noise(source: &str) -> Vec<u32> {
    // TS2318 (cannot find global type) is unrelated lib-loading noise in the
    // unit harness; these fixtures reference no lib globals.
    check_source_strict_codes(source)
        .into_iter()
        .filter(|&code| code != 2318)
        .collect()
}

/// Reported repro shape: value-based filter (`T[K] extends X ? never : K`) with
/// an excess arrow-function property. Only `TS2353` is expected.
#[test]
fn excess_arrow_property_on_value_filtered_mapped_target_is_only_ts2353() {
    let source = r#"
type DropNums<T> = { [K in keyof T as T[K] extends number ? never : K]: T[K] };
interface M { a: number; s: string; }
type R = DropNums<M>;
const bad: R = { s: "x", fn: () => {} };
"#;
    let codes = codes_ignoring_lib_noise(source);
    assert!(
        !codes.contains(&2322),
        "excess function-valued property must not produce a spurious TS2322 against `never`; got {codes:?}"
    );
    assert!(
        codes.contains(&2353),
        "excess property should still be reported as TS2353; got {codes:?}"
    );
}

/// Same rule with a renamed iteration variable (`Prop` instead of `K`) — proves
/// the fix is not keyed on a particular binder name.
#[test]
fn excess_arrow_property_renamed_binder_is_only_ts2353() {
    let source = r#"
type Filter<X> = { [Prop in keyof X as X[Prop] extends number ? never : Prop]: X[Prop] };
interface M { a: number; s: string; }
type R = Filter<M>;
const bad: R = { s: "y", handler: () => {} };
"#;
    let codes = codes_ignoring_lib_noise(source);
    assert!(!codes.contains(&2322), "got {codes:?}");
    assert!(codes.contains(&2353), "got {codes:?}");
}

/// Key-based filter (`P extends Bad ? never : P`, the `Omit` shape) with an
/// excess arrow property.
#[test]
fn excess_arrow_property_on_key_filtered_mapped_target_is_only_ts2353() {
    let source = r#"
type DropKey<T, Bad extends string> = { [P in keyof T as P extends Bad ? never : P]: T[P] };
interface M { keep: string; }
type R = DropKey<M, "x">;
const bad: R = { keep: "z", cb: () => {} };
"#;
    let codes = codes_ignoring_lib_noise(source);
    assert!(!codes.contains(&2322), "got {codes:?}");
    assert!(codes.contains(&2353), "got {codes:?}");
}

/// Rename remap (`as `p_${K}``) — the remapped key space is entirely renamed, so
/// any unprefixed function-valued property is excess.
#[test]
fn excess_arrow_property_on_rename_remapped_mapped_target_is_only_ts2353() {
    let source = r#"
type Prefix<T> = { [K in keyof T as `p_${string & K}`]: T[K] };
interface M { a: number; }
type R = Prefix<M>;
const bad: R = { p_a: 1, extra: () => {} };
"#;
    let codes = codes_ignoring_lib_noise(source);
    assert!(!codes.contains(&2322), "got {codes:?}");
    assert!(codes.contains(&2353), "got {codes:?}");
}

/// Method-shorthand and function-expression excess properties exercise the same
/// `initializer_is_function_like` path as arrows.
#[test]
fn excess_method_and_function_expression_properties_are_only_ts2353() {
    let method = r#"
type DropNums<T> = { [K in keyof T as T[K] extends number ? never : K]: T[K] };
interface M { a: number; s: string; }
type R = DropNums<M>;
const bad: R = { s: "x", run() { return 1; } };
"#;
    let func_expr = r#"
type DropNums<T> = { [K in keyof T as T[K] extends number ? never : K]: T[K] };
interface M { a: number; s: string; }
type R = DropNums<M>;
const bad: R = { s: "x", run: function () {} };
"#;
    for source in [method, func_expr] {
        let codes = codes_ignoring_lib_noise(source);
        assert!(!codes.contains(&2322), "got {codes:?} for {source}");
        assert!(codes.contains(&2353), "got {codes:?} for {source}");
    }
}

/// Negative case: a mapped type that genuinely types every member as `never`
/// (no remapping/filtering) must still reject a function-valued initializer
/// with `TS2322` — the property is present, not excess.
#[test]
fn function_value_against_real_never_member_still_errors_ts2322() {
    let source = r#"
type AllNever<T> = { [K in keyof T]: never };
interface M { a: number; }
type R = AllNever<M>;
const bad: R = { a: () => {} };
"#;
    let codes = codes_ignoring_lib_noise(source);
    assert!(
        codes.contains(&2322),
        "assigning a function to a real `never`-typed member must still error TS2322; got {codes:?}"
    );
    assert!(
        !codes.contains(&2353),
        "the property is present (not excess), so no TS2353 is expected; got {codes:?}"
    );
}

/// Negative case: a remapped key that *survives* with an incompatible
/// function-parameter type must still be re-checked and produce `TS2322`.
#[test]
fn surviving_remapped_function_property_with_wrong_signature_still_errors() {
    let source = r#"
type Wrap<T> = { [K in keyof T as `w_${string & K}`]: T[K] };
interface P { fn: (x: number) => void; }
type R = Wrap<P>;
const bad: R = { w_fn: (x: string) => {} };
"#;
    let codes = codes_ignoring_lib_noise(source);
    assert!(
        codes.contains(&2322),
        "a surviving remapped property with an incompatible signature must still error; got {codes:?}"
    );
}

/// Valid assignment (only surviving keys, correct values) must be clean.
#[test]
fn valid_assignment_to_remapped_mapped_target_is_clean() {
    let source = r#"
type DropNums<T> = { [K in keyof T as T[K] extends number ? never : K]: T[K] };
interface M { a: number; s: string; }
type R = DropNums<M>;
const ok: R = { s: "x" };
"#;
    let codes = codes_ignoring_lib_noise(source);
    assert!(codes.is_empty(), "expected no diagnostics, got {codes:?}");
}
