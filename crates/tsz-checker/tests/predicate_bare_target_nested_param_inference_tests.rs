//! Regression tests for issue #14359: a generic type guard whose predicate
//! target is a *bare* type parameter (`value is T`) where `T` is inferable only
//! from another parameter's *nested* shape.
//!
//! For `declare function is<T>(value: unknown, witness: T[]): value is T`, the
//! call's `T` is inferred from the `witness` argument (`string[]` -> `T = string`),
//! but the narrowing kept the uninstantiated predicate target `T` instead of the
//! inferred type. tsc narrows the guarded value to the inferred `T`.
//!
//! The structural rule: when a predicate target is a bare type parameter that is
//! not bound by a direct (`value: T`) or union-member (`T | "x"`) parameter, the
//! same structural inference that resolves the call's type arguments must also
//! resolve the predicate target. The union-subtraction path keeps ownership of
//! the union-member case (so its false branch narrows to the subtracted member);
//! the structural fallback only fires for the genuinely nested shapes.
//!
//! Every binder name (function, value, witness, type parameter) is varied across
//! cases so a fix that special-cases one spelling fails the suite. The witness
//! position is varied across the nested shapes tsc handles: array element,
//! return position of a callback, and a generic struct field.

use tsz_checker::test_utils::{check_source_diagnostics, diagnostic_messages_with_code};

const TS2322: u32 = 2322;

/// The narrowed value must be the inferred concrete type, so a deliberate
/// `never` mismatch reports that concrete type (and never the raw parameter).
fn assert_narrows_to(source: &str, expected_fragment: &str) {
    let diagnostics = check_source_diagnostics(source);
    let messages = diagnostic_messages_with_code(&diagnostics, TS2322);
    assert_eq!(
        messages.len(),
        1,
        "expected exactly one TS2322 (the `never` annotation), got: {diagnostics:#?}"
    );
    assert!(
        messages[0].contains(expected_fragment),
        "narrowed value should be `{expected_fragment}`, got: {}",
        messages[0]
    );
}

#[test]
fn bare_target_inferred_from_array_witness() {
    // The witness arg fixes `T = string`; `value` must narrow to `string`.
    let source = r"
declare function is<T>(value: unknown, witness: T[]): value is T;
function check(value: unknown, witness: string[]) {
  if (is(value, witness)) {
    const x: never = value;
  }
}
";
    assert_narrows_to(source, "string");
}

#[test]
fn bare_target_inferred_from_array_witness_renamed_binders() {
    // Identical shape, every binder renamed and a different element type.
    let source = r"
declare function matches<Elem>(candidate: unknown, sample: Elem[]): candidate is Elem;
function inspect(candidate: unknown, sample: number[]) {
  if (matches(candidate, sample)) {
    const wrong: never = candidate;
  }
}
";
    assert_narrows_to(source, "number");
}

#[test]
fn bare_target_inferred_from_callback_return_witness() {
    // `T` appears only in the return position of a callback parameter.
    let source = r"
declare function holds<T>(value: unknown, make: () => T): value is T;
function check(value: unknown, make: () => boolean) {
  if (holds(value, make)) {
    const wrong: never = value;
  }
}
";
    assert_narrows_to(source, "boolean");
}

#[test]
fn bare_target_inferred_from_generic_struct_field_witness() {
    // `T` appears nested inside a generic struct parameter.
    let source = r"
interface Struct<T> { field: T; }
declare function is<T>(value: unknown, witness: Struct<T>): value is T;
function check(value: unknown, witness: Struct<string>) {
  if (is(value, witness)) {
    const wrong: never = value;
  }
}
";
    assert_narrows_to(source, "string");
}

/// The union-member case must keep its dedicated subtraction path: the predicate
/// target is a bare `T` that appears as a union member of the parameter, so `T`
/// is the *subtracted* member (`number`), not the whole argument union.
#[test]
fn bare_target_union_member_still_subtracts() {
    let source = r#"
declare function isValue<T>(result: T | "FAILURE"): result is T;
declare const r: number | "FAILURE";
function probe() {
  if (isValue(r)) {
    const wrong: never = r;
  }
}
"#;
    assert_narrows_to(source, "number");
}
