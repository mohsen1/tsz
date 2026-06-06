//! Regression tests for higher-order function type inference (TypeScript 3.4)
//! re-generalization through generic wrappers — issue #10792.
//!
//! When a generic function flows into a contextual function-typed parameter of
//! an outer generic call, the free type parameters of the argument must be
//! propagated into the call result and re-generalized into a fresh generic
//! signature, displayed with their original source names. Previously `tsz`
//! widened them to `unknown` (severing the parameter/return link) or leaked the
//! internal `__infer_src_*` placeholder into the result.
//!
//! Each repro is tsc-clean (or matches tsc's exact diagnostic) and varies
//! binder names so the fix stays structural rather than tied to identifier
//! spellings.
use crate::test_utils::check_source_diagnostics;

fn inference_diags(diags: &[crate::diagnostics::Diagnostic]) -> Vec<(u32, String)> {
    diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2345 || d.code == 18046)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// `lift(box)` must re-generalize to `<T>(a: T) => { v: T }`. Calling it with a
/// number and assigning the result to `{ v: number }` is tsc-clean; a result
/// widened to `(a: unknown) => unknown` would reject the assignment (TS2322).
#[test]
fn naked_single_wrapper_regeneralizes_parameter_and_return() {
    let diags = check_source_diagnostics(
        r#"
declare function box<T>(x: T): { v: T };
declare function lift<A, B>(f: (a: A) => B): (a: A) => B;
const l = lift(box);
const ok: { v: number } = l(5);
"#,
    );
    let unexpected = inference_diags(&diags);
    assert!(
        unexpected.is_empty(),
        "expected tsc-clean higher-order wrapper, got: {unexpected:?}"
    );
}

/// Same structural shape with entirely different binder names, locking the fix
/// to structure rather than the `T`/`A`/`B` spellings.
#[test]
fn naked_single_wrapper_regeneralizes_with_varied_names() {
    let diags = check_source_diagnostics(
        r#"
declare function wrapValue<Elem>(x: Elem): { boxed: Elem };
declare function through<In, Out>(f: (a: In) => Out): (a: In) => Out;
const t = through(wrapValue);
const ok: { boxed: string } = t("hi");
"#,
    );
    let unexpected = inference_diags(&diags);
    assert!(
        unexpected.is_empty(),
        "expected tsc-clean higher-order wrapper, got: {unexpected:?}"
    );
}

/// A generic function argument whose type parameter is *non-naked* (wrapped in
/// `Box<W>`) must also re-generalize, displaying the source name `W`. This is
/// the existing source-placeholder path; the regression guards its display.
#[test]
fn non_naked_single_wrapper_regeneralizes() {
    let diags = check_source_diagnostics(
        r#"
interface Box<W> { value: W }
declare function unwrap<W>(b: Box<W>): W;
declare function adapt<A, B>(f: (a: A) => B): (a: A) => B;
const u = adapt(unwrap);
const ok: number = u({ value: 1 });
"#,
    );
    let unexpected = inference_diags(&diags);
    assert!(
        unexpected.is_empty(),
        "expected tsc-clean non-naked higher-order wrapper, got: {unexpected:?}"
    );
}

/// The re-generalized signature must display with the source type-parameter
/// name (`T`), never the internal `__infer_src_*` placeholder and never a
/// widened `unknown`. `makeGetter(box)` is `<T>() => { v: T }`; assigning it to
/// `number` reproduces tsc's exact message.
#[test]
fn regeneralized_signature_uses_source_name_not_placeholder() {
    let diags = check_source_diagnostics(
        r#"
declare function box<T>(x: T): { v: T };
declare function makeGetter<A, B>(f: (a: A) => B): () => B;
const g = makeGetter(box);
const bad: number = g;
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322, got: {:?}",
        inference_diags(&diags)
    );
    let message = &ts2322[0].message_text;
    assert!(
        message.contains("() => { v: T; }"),
        "re-generalized type should display the source name, got: {message:?}"
    );
    assert!(
        !message.contains("__infer"),
        "internal inference placeholder leaked into diagnostic: {message:?}"
    );
    assert!(
        !message.contains("unknown"),
        "higher-order free type parameter was widened to unknown: {message:?}"
    );
}

/// When the wrapper pins the type parameter through a second argument (the
/// `compose`-style shared placeholder), re-generalization is suppressed and the
/// type parameter is determined concretely. `applyTo(pair, 7)` is tsc-clean.
#[test]
fn shared_placeholder_argument_still_infers_concretely() {
    let diags = check_source_diagnostics(
        r#"
declare function pair<A>(x: A): [A, A];
declare function applyTo<X, Y>(f: (a: X) => Y, x: X): Y;
const r = applyTo(pair, 7);
const ok: [number, number] = r;
"#,
    );
    let unexpected = inference_diags(&diags);
    assert!(
        unexpected.is_empty(),
        "shared-placeholder argument must infer concretely, got: {unexpected:?}"
    );
}
