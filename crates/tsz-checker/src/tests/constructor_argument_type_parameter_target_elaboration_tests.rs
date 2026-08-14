//! Tests that `tsc`'s bare-type-parameter-target elaboration
//! (`TS5082`/`TS5075`) is attached to constructor (`new`) argument
//! mismatches, closing the last call-argument surface that dropped it.
//!
//! Structural rule: `tsc`'s `reportRelationError` appends the
//! type-parameter note whenever a concrete source fails to relate to a bare
//! type-parameter target, regardless of *where* the failing relation arose —
//! assignment (`TS2322`), a plain call argument (`TS2345`), or a constructor
//! argument (`TS2345` on a `new` expression). The `TS2322` path and the
//! call-argument "preserve param display" path
//! (`error_argument_not_assignable_preserving_param_display`) already emitted
//! the note, but the `new`-expression argument check funnels through the
//! shared `error_argument_not_assignable_at_impl` sink
//! (`error_reporter/call_errors/error_emission.rs`), which built its TS2345
//! head with no related-info note at all (#17447). Attaching the note in that
//! shared emitter fixes the `new` surface and every other caller that routes
//! through it, and is a no-op for concrete parameter targets.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_diagnostics;

/// `TS5082` — "`'{T}'` could be instantiated with an arbitrary type …".
const COULD_BE_INSTANTIATED_ARBITRARY: u32 = 5082;
/// `TS5075` — "… could be instantiated with a different subtype of constraint …".
const COULD_BE_INSTANTIATED_DIFFERENT_SUBTYPE: u32 = 5075;

fn ts2345(source: &str) -> Diagnostic {
    check_source_diagnostics(source)
        .into_iter()
        .find(|d| d.code == 2345)
        .unwrap_or_else(|| panic!("expected a TS2345 for source:\n{source}"))
}

fn has_note(diag: &Diagnostic, code: u32) -> bool {
    diag.related_information.iter().any(|r| r.code == code)
}

#[test]
fn new_explicit_type_arg_unconstrained_target_gets_arbitrary_type_note() {
    // `new C<T>(null)` where the explicit type argument is the caller's own
    // unconstrained `T`: the constructor parameter target is a bare,
    // uninstantiated type parameter, so `null` fails and tsc reports TS5082.
    let diag = ts2345(
        r#"
class C<T extends unknown> {
    constructor(x: T) {}
}
function g<T>() {
    new C<T>(null);
}
"#,
    );
    assert!(
        has_note(&diag, COULD_BE_INSTANTIATED_ARBITRARY),
        "expected TS5082 arbitrary-type note on the constructor argument; got: {:?}",
        diag.related_information
    );
    assert!(!has_note(&diag, COULD_BE_INSTANTIATED_DIFFERENT_SUBTYPE));
}

#[test]
fn new_inferred_type_param_target_gets_arbitrary_type_note() {
    // No explicit type argument: `C`'s `T` is inferred from the first argument
    // (the caller's bare `T`), leaving the second parameter target a bare type
    // parameter that `null` cannot satisfy.
    let diag = ts2345(
        r#"
class C<T> {
    constructor(a: T, b: T) {}
}
function g<T>(x: T) {
    new C(x, null);
}
"#,
    );
    assert!(
        has_note(&diag, COULD_BE_INSTANTIATED_ARBITRARY),
        "expected TS5082 arbitrary-type note on the inferred constructor argument; got: {:?}",
        diag.related_information
    );
    assert!(!has_note(&diag, COULD_BE_INSTANTIATED_DIFFERENT_SUBTYPE));
}

#[test]
fn new_constrained_target_satisfied_gets_different_subtype_note() {
    // The caller's `T extends string` is satisfied by the `string` argument, so
    // tsc reports TS5075 ("assignable to the constraint... but could be
    // instantiated with a different subtype"), not TS5082.
    let diag = ts2345(
        r#"
class C<T extends unknown> {
    constructor(x: T) {}
}
function g<T extends string>(y: string) {
    new C<T>(y);
}
"#,
    );
    assert!(
        has_note(&diag, COULD_BE_INSTANTIATED_DIFFERENT_SUBTYPE),
        "expected TS5075 different-subtype note on the constructor argument; got: {:?}",
        diag.related_information
    );
    assert!(!has_note(&diag, COULD_BE_INSTANTIATED_ARBITRARY));
}

#[test]
fn new_constrained_target_not_satisfied_gets_arbitrary_type_note() {
    // The caller's `T extends string` is NOT satisfied by the `number`
    // argument, so the parameter could be instantiated with something entirely
    // unrelated — tsc reports the TS5082 fallback, not TS5075.
    let diag = ts2345(
        r#"
class C<T extends unknown> {
    constructor(x: T) {}
}
function g<T extends string>(n: number) {
    new C<T>(n);
}
"#,
    );
    assert!(
        has_note(&diag, COULD_BE_INSTANTIATED_ARBITRARY),
        "expected TS5082 fallback note when the source misses the constraint; got: {:?}",
        diag.related_information
    );
    assert!(!has_note(&diag, COULD_BE_INSTANTIATED_DIFFERENT_SUBTYPE));
}

#[test]
fn new_concrete_constructor_param_gets_no_type_parameter_note() {
    // A plain concrete constructor parameter is not a bare type parameter at
    // all — no elaboration note should be attached, matching the ordinary
    // TS2345 and confirming the shared-emitter change is a strict no-op for
    // concrete targets.
    let diag = ts2345(
        r#"
class C {
    constructor(x: number) {}
}
new C("str");
"#,
    );
    assert!(!has_note(&diag, COULD_BE_INSTANTIATED_ARBITRARY));
    assert!(!has_note(&diag, COULD_BE_INSTANTIATED_DIFFERENT_SUBTYPE));
}

#[test]
fn new_renamed_binders_still_elaborate() {
    // Anti-hardcoding: the same shape under different identifier names must
    // still elaborate — the decision is structural (is the target a bare type
    // parameter?), not keyed on any particular binder name.
    let diag = ts2345(
        r#"
class Container<Element extends unknown> {
    constructor(value: Element) {}
}
function builder<Item>() {
    new Container<Item>(null);
}
"#,
    );
    assert!(
        has_note(&diag, COULD_BE_INSTANTIATED_ARBITRARY),
        "expected TS5082 note under renamed binders; got: {:?}",
        diag.related_information
    );
}
