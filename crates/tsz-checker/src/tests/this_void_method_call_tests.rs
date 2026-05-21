//! TS2684 must be skipped when the called callable declares `this: void`.
//!
//! Structural rule: per tsc's `checkApplicableSignature`, the receiver
//! assignability check is gated by `thisType !== voidType`. A callable
//! annotated `this: void` explicitly opts out of using `this`, so it accepts
//! any receiver — even one that is not structurally assignable to `void`.
//!
//! Owning layer: `tsz-solver` — the gate lives in
//! `tsz_solver::operations::core::call_resolution::receiver_constraining_this_type`
//! and is applied at every call-resolution site that decides whether to
//! produce a `ThisTypeMismatch` (single function, union, multi-overload,
//! generic). The checker just renders the diagnostic when the solver reports
//! the mismatch.

use crate::test_utils::check_source_diagnostics;

fn diagnostic_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|diag| diag.code)
        .collect()
}

#[test]
fn this_void_function_as_object_method_no_error() {
    let codes = diagnostic_codes(
        r#"
function f(this: void, x: number) {}
const obj = { m: f };
obj.m(1);
"#,
    );
    assert!(
        codes.is_empty(),
        "expected no diagnostics for `this: void` function called as object method; got {codes:?}"
    );
}

#[test]
fn this_void_renamed_property_no_error() {
    // Same structural rule, different property name — proves the fix is not
    // matching a particular spelling.
    let codes = diagnostic_codes(
        r#"
function log(this: void, msg: string) {}
const logger = { log };
logger.log("hi");
"#,
    );
    assert!(
        codes.is_empty(),
        "expected no diagnostics for `this: void` shorthand property; got {codes:?}"
    );
}

#[test]
fn this_void_bare_call_still_clean_regression_guard() {
    let codes = diagnostic_codes(
        r#"
function f(this: void, x: number) {}
f(1);
"#,
    );
    assert!(
        codes.is_empty(),
        "expected no diagnostics for bare `this: void` call; got {codes:?}"
    );
}

#[test]
fn this_concrete_object_lacking_property_still_errors() {
    // Negative control — a real `this` mismatch must still produce TS2684.
    // Verifies the gate is keyed on `void`, not "any `this` annotation".
    let codes = diagnostic_codes(
        r#"
function f(this: { a: number }, x: number) {}
const obj = { m: f };
obj.m(1);
"#,
    );
    assert!(
        codes.contains(&2684),
        "expected TS2684 for concrete `this` mismatch; got {codes:?}"
    );
}

#[test]
fn this_void_through_interface_method_slot_no_error() {
    // The `this: void` function is assigned through an interface method slot,
    // then invoked. The receiver check must still be skipped.
    let codes = diagnostic_codes(
        r#"
interface Logger { log(msg: string): void; }
function logImpl(this: void, msg: string) {}
const logger: Logger = { log: logImpl };
logger.log("hi");
"#,
    );
    assert!(
        codes.is_empty(),
        "expected no diagnostics when `this: void` flows through an interface method slot; got {codes:?}"
    );
}

#[test]
fn this_any_as_object_method_no_error_regression_guard() {
    // `this: any` already worked (top type is trivially assignable from any
    // receiver). Guard against the new gate accidentally narrowing this case.
    let codes = diagnostic_codes(
        r#"
function f(this: any, x: number) {}
const obj = { m: f };
obj.m(1);
"#,
    );
    assert!(
        codes.is_empty(),
        "expected no diagnostics for `this: any` function as method; got {codes:?}"
    );
}

#[test]
fn this_void_callable_interface_multi_overload_no_error() {
    // Multi-overload callable: every signature declares `this: void`, so the
    // per-signature gate should skip the receiver check on every overload.
    let codes = diagnostic_codes(
        r#"
interface F {
  (this: void, x: number): void;
  (this: void, x: string): void;
}
declare const f: F;
const obj = { m: f };
obj.m(1);
obj.m("x");
"#,
    );
    let twosixeightfour: Vec<_> = codes.iter().filter(|&&c| c == 2684).collect();
    assert!(
        twosixeightfour.is_empty(),
        "expected no TS2684 for multi-overload `this: void` callable as method; got {codes:?}"
    );
}

#[test]
fn this_void_generic_function_as_method_no_error() {
    // Generic function with explicit `this: void`. After type-parameter
    // inference and substitution, the receiver gate must still skip.
    let codes = diagnostic_codes(
        r#"
function id<T>(this: void, x: T): T { return x; }
const obj = { m: id };
obj.m(1);
obj.m("x");
"#,
    );
    let twosixeightfour: Vec<_> = codes.iter().filter(|&&c| c == 2684).collect();
    assert!(
        twosixeightfour.is_empty(),
        "expected no TS2684 for generic `this: void` function as method; got {codes:?}"
    );
}

#[test]
fn this_void_union_member_alongside_concrete_still_errors_on_concrete() {
    // Mixed union: one member is `this: void` (no constraint), the other has a
    // concrete `this`. The concrete member's constraint must still drive the
    // intersection — the receiver must be assignable to the concrete shape,
    // otherwise TS2684 fires. Excluding void from the intersection must not
    // collapse the constraint when at least one member is concrete.
    let codes = diagnostic_codes(
        r#"
type Concrete = { a: number };
type F = ((this: void, x: number) => void) | ((this: Concrete, x: number) => void);
declare const f: F;
const obj = { m: f };
obj.m(1);
"#,
    );
    assert!(
        codes.contains(&2684),
        "expected TS2684 for union where the concrete member's `this` is unsatisfied; got {codes:?}"
    );
}
