//! Regression tests for issue #10797: type-predicate narrowing through a
//! generic type alias.
//!
//! A guard such as `function isHeld<T>(v: Wrapped<T>): v is Holder<T>`
//! (where `type Wrapped<T> = T | Holder<T>`) must narrow the value to
//! `Holder<T>` with `T` inferred from the argument. The function's type
//! parameter `T` appears only *nested* inside the parameter type (the alias),
//! so the predicate target can only be instantiated by inferring `T` from the
//! argument the same way call resolution does.
//!
//! Before the fix the predicate stayed generic (`Holder<T>`); flow narrowing
//! then intersected the already-narrowed value with the generic target,
//! over-constraining it to `Holder<string> & Holder` (the witness in the issue
//! used the library `AsyncIterable<T>` alias). The structural rule: when a
//! predicate's type parameter is nested in a parameter type, infer it from the
//! argument before instantiating the predicate target.
//!
//! The tests use a locally-declared generic interface so they do not depend on
//! a particular `lib` target, and vary every binder name (alias, function,
//! parameter, type parameter, interface) across cases so a fix that
//! special-cases one spelling fails the suite.

use tsz_checker::test_utils::{check_source_diagnostics, diagnostic_messages_with_code};

const TS2322: u32 = 2322;

/// The narrowed source type rendered in a deliberate mismatch must be the bare
/// instantiated predicate target and must never contain a spurious
/// self-intersection (` & `).
fn assert_narrowed_target_not_intersection(source: &str, expected_fragment: &str) {
    let diagnostics = check_source_diagnostics(source);
    let messages = diagnostic_messages_with_code(&diagnostics, TS2322);
    assert!(
        !messages.is_empty(),
        "expected a TS2322 mismatch to observe the narrowed type, got: {diagnostics:#?}"
    );
    for message in &messages {
        assert!(
            !message.contains(" & "),
            "narrowed predicate target should not be an intersection, got: {message}"
        );
        assert!(
            message.contains(expected_fragment),
            "narrowed target should contain `{expected_fragment}`, got: {message}"
        );
    }
}

#[test]
fn generic_alias_predicate_narrows_without_self_intersection() {
    let source = r"
interface Holder<T> { item: T; }
type Wrapped<T> = T | Holder<T>;
declare function isHeld<T>(value: Wrapped<T>): value is Holder<T>;
declare const w: Wrapped<string>;
function probe() {
  if (isHeld(w)) {
    const wrong: number = w;
  }
}
";
    assert_narrowed_target_not_intersection(source, "Holder<string>");
}

#[test]
fn generic_alias_predicate_renamed_binders_narrows_without_self_intersection() {
    // Identical shape, every binder renamed and a different element type.
    let source = r"
interface Cell<Elem> { slot: Elem; }
type Boxed<Elem> = Elem | Cell<Elem>;
declare function looksBoxed<Elem>(candidate: Boxed<Elem>): candidate is Cell<Elem>;
declare const value: Boxed<boolean>;
function inspect() {
  if (looksBoxed(value)) {
    const wrong: number = value;
  }
}
";
    assert_narrowed_target_not_intersection(source, "Cell<boolean>");
}

#[test]
fn generic_alias_predicate_keeps_element_type_usable() {
    // The narrowed value must remain a usable `Holder<string>`: reading the
    // `item` property yields `string`, so the `number` annotation is the only
    // error and its type is exactly `string` (not `any` or an intersection).
    let source = r"
interface Holder<T> { item: T; }
type Wrapped<T> = T | Holder<T>;
declare function isHeld<T>(value: Wrapped<T>): value is Holder<T>;
declare const w: Wrapped<string>;
function probe() {
  if (isHeld(w)) {
    const wrong: number = w.item;
  }
}
";
    let diagnostics = check_source_diagnostics(source);
    let messages = diagnostic_messages_with_code(&diagnostics, TS2322);
    assert_eq!(
        messages.len(),
        1,
        "expected exactly one TS2322 (the `number` annotation), got: {diagnostics:#?}"
    );
    assert!(
        messages[0].contains("string") && !messages[0].contains(" & "),
        "`w.item` should narrow to `string`, got: {}",
        messages[0]
    );
}
