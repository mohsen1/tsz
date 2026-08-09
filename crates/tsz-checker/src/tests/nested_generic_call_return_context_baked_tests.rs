//! Regression tests for return-context inference through a nested generic call
//! whose contextual return type has been baked into a structural object.
//!
//! Structural rule: when an inner generic call `f(cb)` sits in call-argument
//! position and its result feeds an outer parameter of type `G<Concrete>`, the
//! inner call's type parameter must be inferred from that contextual return
//! type — `G<Param>` matched against `G<Concrete>` binds `Param := Concrete` —
//! *before* the deferred callback `cb` is contextually typed. `tsc` does this in
//! `inferTypeArguments` (return-type context, `InferencePriority.ReturnType`).
//!
//! tsz's solver-side return-context walk only decomposed a contextual type that
//! was still interned as a `TypeData::Application`. A nominal `G<Concrete>`
//! reaches the walk *baked* into a structural object (`{ ... }`) with no
//! `Application` shape, so the walk bound nothing and the inner type parameter
//! fell back to its declared constraint. The callback parameter was then typed
//! from the constraint (`{}`), producing a spurious `TS2345` — issue #17005,
//! the regression exposed by #17000. The fix recovers the baked object's
//! originating `Application` through its display-alias back-reference, matching
//! the checker-side `return_context_application_info` helper.
//!
//! Binder names vary across cases (`Params`/`received`, `V`/`x`, `A`/`B`) so no
//! test pins the behavior to a specific identifier.

use crate::test_utils::check_source_diagnostics;

fn ts2345(diags: &[crate::diagnostics::Diagnostic]) -> Vec<&crate::diagnostics::Diagnostic> {
    diags
        .iter()
        .filter(|diagnostic| diagnostic.code == 2345)
        .collect()
}

#[test]
fn variadic_tuple_callback_param_infers_from_baked_contextual_class_return() {
    // The exact #17005 shape: a variadic-tuple constraint, a rest-parameter
    // callback, and a nominal generic class result consumed by an outer call.
    // `Params` must be inferred as `[string, boolean]` from `Wrapper<[string,
    // boolean]>`, so `head` types as `string` and `head.length` is valid.
    let diags = check_source_diagnostics(
        r#"
type Item = {};
type Args = [Item, ...Item[]];
type Handler<Params extends Args> = (...received: Params) => void;
declare class Wrapper<Params extends Args> { data: Params; }
declare function build<Params extends Args>(h: Handler<Params>): Wrapper<Params>;
declare function apply(w: Wrapper<[string, boolean]>): void;

apply(build(head => console.log(head.length)));
"#,
    );
    assert!(
        diags.is_empty(),
        "expected no diagnostics, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn single_type_param_interface_wrapper_infers_callback_param_from_return_context() {
    // Interface heritage instead of a class, a single type parameter, and a
    // one-arg callback. `V := string` from `Cell<string>`, so `s.length` holds.
    let diags = check_source_diagnostics(
        r#"
interface Cell<V> { current: V; }
declare function cellOf<V>(f: (x: V) => void): Cell<V>;
declare function readCell(c: Cell<string>): void;

readCell(cellOf(s => s.length));
"#,
    );
    assert!(
        diags.is_empty(),
        "expected no diagnostics, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn two_type_params_infer_first_from_return_context_second_from_callback_body() {
    // `pair<A, B>(f: (a: A) => B): Pair<A, B>`; `A := string` from the
    // contextual `Pair<string, number>` types `x`, and `B := number` follows
    // from the body — both flow without a spurious mismatch.
    let diags = check_source_diagnostics(
        r#"
interface Pair<A, B> { a: A; b: B; }
declare function pair<A, B>(f: (a: A) => B): Pair<A, B>;
declare function usePair(p: Pair<string, number>): void;

usePair(pair(x => x.length));
"#,
    );
    assert!(
        diags.is_empty(),
        "expected no diagnostics, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn genuinely_incompatible_callback_body_still_reports_ts2345_family() {
    // Negative guard: the return context now pins the callback parameter to a
    // *concrete* type, so a body that misuses it must still be rejected. `n`
    // types as `number` (from `Cell<number>`); `n.length` has no such property.
    // Before the fix the constraint fallback masked this as a silent success.
    let diags = check_source_diagnostics(
        r#"
interface Cell<V> { current: V; }
declare function cellOf<V>(f: (x: V) => void): Cell<V>;
declare function readNum(c: Cell<number>): void;

readNum(cellOf(n => n.length));
"#,
    );
    assert!(
        !diags.is_empty(),
        "expected a diagnostic for `n.length` on a number callback parameter"
    );
}

#[test]
fn baked_contextual_return_does_not_leak_when_outer_argument_is_concrete() {
    // A concrete sibling still owns the parameter: when the outer call supplies
    // the concrete wrapper directly (no callback deferral), the return context
    // must neither spuriously error nor be needed. Keeps the fix confined to the
    // deferred-callback path.
    let diags = check_source_diagnostics(
        r#"
interface Cell<V> { current: V; }
declare function identity<V>(c: Cell<V>): Cell<V>;
declare function readCell(c: Cell<string>): void;
declare const cell: Cell<string>;

readCell(identity(cell));
"#,
    );
    assert!(
        ts2345(&diags).is_empty(),
        "expected no TS2345, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.as_str()))
            .collect::<Vec<_>>()
    );
}
