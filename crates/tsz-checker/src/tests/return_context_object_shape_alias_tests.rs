//! Return-context inference must recover a nested generic call's type argument
//! from a contextual target that has already been materialized to its structural
//! object shape, not only from the `Application` form.
//!
//! Structural rule: when a generic call `make<T>(f): Box<T>` sits in argument
//! position against a concrete contextual target (`take(b: Box<string>)`), `tsc`
//! infers `T` from the return context (`Box<T>` vs `Box<string>` ⇒ `T = string`)
//! so the callback parameter types as `string`. tsz's return-context matcher
//! compared the *raw* source/target in its object-vs-object arm, so when the
//! callee's parameter type had been eagerly reduced to its structural object
//! shape while the callee return `Box<T>` was still an unevaluated `Application`,
//! that arm was skipped and `T` fell back to its constraint, producing a spurious
//! TS2345 at the callback. Matching the *evaluated* forms recovers the shared
//! property structure so the binding is found regardless of which representation
//! the contextual type happens to carry.
//!
//! (The sibling nominal-class shape — a `class` return rather than an object type
//! alias — is not covered here: a class instance with a free type argument stays
//! nominal under evaluation, so it never reaches this structural arm. That case is
//! the eager-materialization structure loss tracked separately.)

use crate::test_utils::check_source_codes as codes;

#[test]
fn nested_generic_call_binds_type_arg_from_object_alias_return_context() {
    // `make(s => s.length)` in argument position: `T` must infer as `string`
    // from `take`'s `Box<string>` parameter, so `s.length` is valid.
    assert!(
        codes(
            "type Box<T> = { from: (x: T) => void }; \
             declare function make<T>(f: (x: T) => void): Box<T>; \
             declare function take(b: Box<string>): void; \
             take(make(s => s.length));"
        )
        .is_empty()
    );
}

#[test]
fn nested_generic_call_object_return_context_renamed_binders() {
    // Same shape with every binder renamed, so the rule is not keyed on any
    // identifier (`Wrap`/`U`/`build`/`consume`/`slot`).
    assert!(
        codes(
            "type Wrap<U> = { slot: (v: U) => void }; \
             declare function build<U>(g: (v: U) => void): Wrap<U>; \
             declare function consume(w: Wrap<string>): void; \
             consume(build(v => v.length));"
        )
        .is_empty()
    );
}

#[test]
fn direct_annotated_object_alias_return_context_stays_clean() {
    // Control: the same call as a direct variable initializer already worked
    // (the contextual type keeps its `Application` form there). It must stay
    // clean after the change.
    assert!(
        codes(
            "type Box<T> = { from: (x: T) => void }; \
             declare function make<T>(f: (x: T) => void): Box<T>; \
             const b: Box<string> = make(s => s.length);"
        )
        .is_empty()
    );
}

#[test]
fn nested_generic_call_object_return_context_reports_genuine_body_error() {
    // Negative control: with `T = number` inferred from `Box<number>`, the
    // callback parameter is `number`, so `s.length` is a genuine property error.
    // The return-context binding must not silence real errors under the concrete
    // type argument.
    assert_eq!(
        codes(
            "type Box<T> = { from: (x: T) => void }; \
             declare function make<T>(f: (x: T) => void): Box<T>; \
             declare function take(b: Box<number>): void; \
             take(make(s => s.length));"
        ),
        vec![2339],
    );
}

#[test]
fn nested_generic_call_object_return_context_no_companion_binding_unchanged() {
    // Negative control: when the outer target does not constrain the type
    // argument (no return context to bind `T`), `T` defaults to its `unknown`
    // constraint, so the callback parameter is `unknown` and `s.length` is
    // TS18046 exactly as before — the change only adds a binding where the return
    // context makes one recoverable, it does not invent one.
    assert_eq!(
        codes(
            "type Box<T> = { from: (x: T) => void }; \
             declare function make<T>(f: (x: T) => void): Box<T>; \
             const b = make(s => s.length);"
        ),
        vec![18046],
    );
}
