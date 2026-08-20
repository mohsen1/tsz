//! Inferring a type parameter from a polymorphic `this` argument.
//!
//! Structural rule: when the inference *source* is a polymorphic `this` type
//! and the corresponding target is (or structurally contains) a type parameter
//! being inferred, `tsc` resolves `this` to the enclosing class/interface
//! instance type and produces inference candidates from that resolved type.
//! Without this resolution the solver reaches candidate collection with an
//! unresolved `ThisType` source, collects zero candidates, and the inference
//! variable falls back to its constraint (`unknown`). In a covariant position
//! the fallback is invisible; in a contravariant (function-parameter) position
//! it surfaces as a false `TS2345`.
//!
//! Owning layer: `tsz-solver` inference — `infer_from_types_inner` resolves the
//! `ThisType` source via the resolver's active `this` binding before any
//! candidate collection.
//!
//! The matrix below intentionally varies binder names (function, type
//! parameter, interface, class, property) so the fix cannot be keyed on any
//! identifier, and pairs each positive case with a negative one so the fix is
//! a real inference, not a blanket `any`/`unknown` suppression.

use crate::test_utils::check_source_diagnostics;

fn diagnostic_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|diag| diag.code)
        .collect()
}

// (B) Non-generic class, `this` source, contravariant (function-parameter)
// target position. This is the minimal witness of the bug: `U` must infer to
// the resolved member type (`string`), not fall back to `unknown`.
#[test]
fn this_source_contravariant_infers_member_type_no_error() {
    let codes = diagnostic_codes(
        r#"
interface Box<U> { fn: (value: U) => void }
declare function check<U>(b: Box<U>): void;
class Foo {
  fn!: (value: string) => void;
  m() { check(this); }
}
"#,
    );
    assert!(
        codes.is_empty(),
        "expected no diagnostics inferring U from a `this` source in a contravariant position; got {codes:?}"
    );
}

// (C) The SAME resolved instance type passed *explicitly* already inferred
// correctly; this guards that the `this` path now matches the explicit path.
#[test]
fn explicit_self_param_contravariant_no_error() {
    let codes = diagnostic_codes(
        r#"
interface Box<U> { fn: (value: U) => void }
declare function check<U>(b: Box<U>): void;
class Foo {
  fn!: (value: string) => void;
  m(self: Foo) { check(self); }
}
"#,
    );
    assert!(
        codes.is_empty(),
        "expected no diagnostics inferring U from an explicit `self: Foo` source; got {codes:?}"
    );
}

// Covariant control: must remain clean. `this` resolves to the instance type
// and `U` infers to the member type, but the position is covariant so the
// result was already accepted — this guards against a regression in the
// opposite variance.
#[test]
fn this_source_covariant_control_no_error() {
    let codes = diagnostic_codes(
        r#"
interface CovBox<U> { val: U }
declare function check2<U>(value: unknown, b: CovBox<U>): void;
class Bar<T> {
  val!: T;
  assert(value: unknown) { return check2(value, this); }
}
"#,
    );
    assert!(
        codes.is_empty(),
        "expected no diagnostics for the covariant `this`-source control; got {codes:?}"
    );
}

// Renamed binders: vary every identifier (function `verify`, type param `S`,
// interface `Sink`, class `Widget`, method `run`, property `handle`). A fix
// keyed on any name would miss this; a structural fix must accept it.
#[test]
fn this_source_renamed_binders_no_error() {
    let codes = diagnostic_codes(
        r#"
interface Sink<S> { handle: (value: S) => void }
declare function verify<S>(target: Sink<S>): void;
class Widget {
  handle!: (value: number) => void;
  run() { verify(this); }
}
"#,
    );
    assert!(
        codes.is_empty(),
        "expected no diagnostics with renamed binders; got {codes:?}"
    );
}

// `this` nested one level inside an object-literal argument: the resolved
// instance type must still be reachable for candidate collection through the
// wrapping property.
#[test]
fn this_source_nested_in_object_literal_no_error() {
    let codes = diagnostic_codes(
        r#"
interface Box<U> { fn: (value: U) => void }
declare function checkWrap<U>(w: { self: Box<U> }): void;
class Foo {
  fn!: (value: string) => void;
  m() { checkWrap({ self: this }); }
}
"#,
    );
    assert!(
        codes.is_empty(),
        "expected no diagnostics inferring through `{{ self: this }}`; got {codes:?}"
    );
}

// `this` inside an array argument: element-level inference must resolve the
// `this` element source.
#[test]
fn this_source_in_array_argument_no_error() {
    let codes = diagnostic_codes(
        r#"
interface Box<U> { fn: (value: U) => void }
declare function checkAll<U>(items: Box<U>[]): void;
class Foo {
  fn!: (value: string) => void;
  m() { checkAll([this]); }
}
"#,
    );
    assert!(
        codes.is_empty(),
        "expected no diagnostics inferring through `[this]`; got {codes:?}"
    );
}

// Negative: a genuine contravariant mismatch must STILL error after the fix.
// `Bad.fn` takes `number`; the target demands a `string` consumer, so the
// resolved `this` (= `Bad`) really is incompatible. No blanket suppression.
#[test]
fn this_source_genuine_mismatch_still_errors() {
    let codes = diagnostic_codes(
        r#"
interface Box<U> { fn: (value: U) => void }
declare function checkString(b: Box<string>): void;
class Bad {
  fn!: (value: number) => void;
  m() { checkString(this); }
}
"#,
    );
    assert!(
        codes.contains(&2345),
        "expected TS2345 for a genuine contravariant mismatch through `this`; got {codes:?}"
    );
}

// Negative read-back: capture the inferred `U` into an incompatible
// annotation. The diagnostic must report the *resolved* member type
// (`string`), proving `U` was inferred from the resolved `this`, not left as
// `unknown`.
#[test]
fn this_source_readback_reports_resolved_member_type() {
    let diags = check_source_diagnostics(
        r#"
interface CovBox<U> { val: U }
declare function pick<U>(b: CovBox<U>): U;
class Baz {
  val!: string;
  m() {
    const out: number = pick(this);
    return out;
  }
}
"#,
    );
    // Collect the top-level message plus any nested elaboration so the
    // assertion sees the full "Type 'string' is not assignable to ..." chain.
    let messages: Vec<String> = diags
        .iter()
        .flat_map(|d| {
            std::iter::once(d.message_text.clone())
                .chain(d.related_information.iter().map(|r| r.message_text.clone()))
        })
        .collect();
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "expected TS2322 assigning the inferred result to `number`; got {messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("string")),
        "diagnostic must reference the resolved member type `string`, not `unknown`; got {messages:?}"
    );
}
