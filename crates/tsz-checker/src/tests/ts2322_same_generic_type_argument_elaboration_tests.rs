//! Regression tests for the TS2322 same-generic type-argument elaboration.
//!
//! Structural rule: when the assignment source and target are applications of
//! the **same generic target** (`C<A..>` vs `C<B..>`) whose variance reliably
//! pins the failure to a concrete type argument, `tsc` elaborates the failure
//! by comparing the differing type **arguments** directly — a single nested
//! line `Type 'Ai' is not assignable to type 'Bi'.` — instead of recursing
//! into a structural property comparison that emits an extra
//! `Types of property 'x' are incompatible.` line.
//!
//! See issue #11778. The fix lives in the solver relation failure-reason
//! generation (`SubtypeChecker::explain_same_generic_type_arguments`) so it
//! applies to every same-generic mismatch, not the reported spelling.

use crate::test_utils::check_source_diagnostics;

/// Collect the TS2322 diagnostic's full elaboration text (main message plus all
/// related-information lines, joined by newlines) for a single-error source.
fn ts2322_elaboration(source: &str) -> String {
    let diags = check_source_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one TS2322. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    let mut lines = vec![ts2322[0].message_text.clone()];
    lines.extend(
        ts2322[0]
            .related_information
            .iter()
            .map(|info| info.message_text.clone()),
    );
    lines.join("\n")
}

/// The reported repro: a user class with a single covariant/invariant field.
/// tsc elaborates the differing argument directly; no `Types of property`.
#[test]
fn same_generic_user_class_elaborates_type_argument_directly() {
    let text = ts2322_elaboration(
        r#"
class Box<T> { v!: T }
declare const b: Box<number>;
const n: Box<string> = b;
"#,
    );
    assert!(
        text.contains("Type 'number' is not assignable to type 'string'."),
        "Expected the differing type argument as the nested line. Got: {text:?}"
    );
    assert!(
        !text.contains("Types of property"),
        "Same-generic argument mismatch must not emit a `Types of property` line. Got: {text:?}"
    );
}

/// Anti-hardcoding cover: the rule is about same-generic *applications*, not the
/// type-parameter name. Renaming `T` to `K` and the property must not change the
/// elaboration shape.
#[test]
fn same_generic_user_class_renamed_type_param() {
    let text = ts2322_elaboration(
        r#"
class Holder<K> { value!: K }
declare const h: Holder<number>;
const h2: Holder<string> = h;
"#,
    );
    assert!(
        text.contains("Type 'number' is not assignable to type 'string'."),
        "Renamed variant must still elaborate the type argument directly. Got: {text:?}"
    );
    assert!(
        !text.contains("Types of property"),
        "Renamed variant must not emit a `Types of property` line. Got: {text:?}"
    );
}

/// Covariant container shape (type parameter only in a method return position)
/// follows the same rule — proving the fix is variance-driven, not field-shaped.
#[test]
fn same_generic_covariant_container() {
    let text = ts2322_elaboration(
        r#"
interface Producer<T> { get(): T }
declare const p: Producer<number>;
const p2: Producer<string> = p;
"#,
    );
    assert!(
        text.contains("Type 'number' is not assignable to type 'string'."),
        "Covariant container must elaborate the argument directly. Got: {text:?}"
    );
    assert!(
        !text.contains("Types of property"),
        "Covariant container mismatch must not emit a `Types of property` line. Got: {text:?}"
    );
}

/// Multi-parameter generic: only the *differing* (second) argument is reported,
/// proving the fix selects the failing argument position rather than the first.
#[test]
fn same_generic_multi_parameter_selects_failing_argument() {
    let text = ts2322_elaboration(
        r#"
interface Pair<A, B> { first: A; second: B }
declare const pr: Pair<string, number>;
const pr2: Pair<string, string> = pr;
"#,
    );
    assert!(
        text.contains("Type 'number' is not assignable to type 'string'."),
        "Multi-parameter mismatch must elaborate the failing (second) argument. Got: {text:?}"
    );
    assert!(
        !text.contains("Types of property"),
        "Multi-parameter mismatch must not emit a `Types of property` line. Got: {text:?}"
    );
}

/// Nested same-generic arguments keep elaborating: the outer application line,
/// then the inner application line, then the leaf relation — each one indent
/// level deeper, still without any `Types of property` wrapper.
#[test]
fn same_generic_nested_arguments_chain() {
    let text = ts2322_elaboration(
        r#"
class Holder<K> { value!: K }
class Wrap<T> { inner!: T }
declare const w: Wrap<Holder<number>>;
const w2: Wrap<Holder<string>> = w;
"#,
    );
    assert!(
        text.contains("Type 'Holder<number>' is not assignable to type 'Holder<string>'."),
        "Outer argument (Holder<number> vs Holder<string>) must be elaborated. Got: {text:?}"
    );
    assert!(
        text.contains("Type 'number' is not assignable to type 'string'."),
        "Inner leaf argument must be elaborated. Got: {text:?}"
    );
    assert!(
        !text.contains("Types of property"),
        "Nested same-generic mismatch must not emit a `Types of property` line. Got: {text:?}"
    );
}

/// Negative / fallback cover: *different* generic targets (`Foo<T>` vs `Bar<T>`)
/// are not the same application, so tsc keeps the structural property
/// elaboration. This locks the same-generic rule from over-firing.
#[test]
fn different_generic_targets_keep_property_elaboration() {
    let text = ts2322_elaboration(
        r#"
class Foo<T> { v!: T }
class Bar<T> { v!: T }
declare const f: Foo<number>;
const bar: Bar<string> = f;
"#,
    );
    assert!(
        text.contains("Types of property 'v' are incompatible."),
        "Distinct generic targets must keep the structural property elaboration. Got: {text:?}"
    );
    assert!(
        text.contains("Type 'number' is not assignable to type 'string'."),
        "The leaf relation should still appear beneath the property line. Got: {text:?}"
    );
}

/// Negative / fallback cover: a plain (non-generic) object property mismatch
/// must still elaborate with `Types of property`, matching tsc.
#[test]
fn plain_object_property_mismatch_keeps_property_elaboration() {
    let text = ts2322_elaboration(
        r#"
interface A { x: number }
interface B { x: string }
declare const a: A;
const b: B = a;
"#,
    );
    assert!(
        text.contains("Types of property 'x' are incompatible."),
        "Plain object property mismatch must keep `Types of property`. Got: {text:?}"
    );
}
