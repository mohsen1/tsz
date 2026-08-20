//! Regression tests for issue #10804 — generic method override variance through
//! the class `implements` interface heritage form (mixed inheritance chains).
//!
//! When a class `implements` an interface whose method is *generic* and the
//! class member drops the method-local type parameter(s) to a concrete type,
//! the member is a valid implementation **iff** the dropped parameter appears
//! only in **input** (contravariant) positions: the wider concrete parameter
//! admits every instantiation. A method-local generic used covariantly in the
//! return is *not* satisfiable by a concrete type (a caller relies on getting a
//! specific instantiation back), so it must still be rejected.
//!
//! `tsc`'s `compareSignaturesRelated` makes exactly this decision for all three
//! heritage forms. The interface-`extends` (TS2430) and class-`extends`
//! (TS2416) paths already routed through
//! `nongeneric_input_only_generic_override_is_valid`; the `implements` member
//! check used the strict no-erase relation directly, producing a false TS2416 on
//! the sound input-only drop. These tests pin the implements form to the same
//! variance decision (own members and members inherited from a base class).
//!
//! The decision is structural — keyed on the signature shapes, not on any
//! identifier — so the accepted/rejected cases below vary the method, interface,
//! and type-parameter names.

use crate::test_utils::check_source_codes;

fn assert_has_2416(src: &str) {
    let codes = check_source_codes(src);
    assert!(
        codes.contains(&2416),
        "expected TS2416, got none. Got: {codes:?}\nSource:\n{src}"
    );
}

fn assert_no_2416(src: &str) {
    let codes = check_source_codes(src);
    assert!(
        !codes.contains(&2416),
        "unexpected TS2416 (false positive). Got: {codes:?}\nSource:\n{src}"
    );
}

// ---------------------------------------------------------------------------
// Must be ACCEPTED (input-only generic dropped to its constraint). These were
// false positives before the fix.
// ---------------------------------------------------------------------------

// Input-only constrained generic, same interface return type.
#[test]
fn no_false_2416_implements_input_only_same_return() {
    assert_no_2416(
        "interface Builder { with<K extends string>(k: K): Builder; }
         class Impl implements Builder { with(k: string): Builder { return this; } }",
    );
}

// Renamed method-local type parameter — proves the rule is structural.
#[test]
fn no_false_2416_implements_input_only_same_return_renamed() {
    assert_no_2416(
        "interface Builder { attach<C extends string>(k: C): Builder; }
         class Impl implements Builder { attach(k: string): Builder { return this; } }",
    );
}

// Input-only generic with a covariant (subtype) return narrows soundly.
#[test]
fn no_false_2416_implements_input_only_covariant_return() {
    assert_no_2416(
        "interface Builder { with<K extends string>(k: K): Builder; }
         class Sub implements Builder { with(k: string): Sub { return this; } }",
    );
}

// Input-only generic with a `void` return.
#[test]
fn no_false_2416_implements_input_only_void_return() {
    assert_no_2416(
        "interface Builder { with<K extends string>(k: K): void; }
         class Impl implements Builder { with(k: string): void {} }",
    );
}

// Unconstrained input-only generic dropped to `unknown`.
#[test]
fn no_false_2416_implements_input_only_unconstrained() {
    assert_no_2416(
        "interface Sink { push<T>(x: T): void; }
         class Impl implements Sink { push(x: unknown): void {} }",
    );
}

// Phantom (unused) method-local generic dropped — sound to drop.
#[test]
fn no_false_2416_implements_phantom_unused_generic() {
    assert_no_2416(
        "interface Reader { read<Z>(): number; }
         class Impl implements Reader { read(): number { return 0; } }",
    );
}

// The satisfying member is INHERITED from a base class (mixed chain:
// `class Impl extends BaseImpl implements Builder`).
#[test]
fn no_false_2416_implements_inherited_member_input_only() {
    assert_no_2416(
        "interface Builder { with<K extends string>(k: K): Builder; }
         class BaseImpl { with(k: string): Builder { return this as unknown as Builder; } }
         class Impl extends BaseImpl implements Builder {}",
    );
}

// Inherited member, renamed binders.
#[test]
fn no_false_2416_implements_inherited_member_input_only_renamed() {
    assert_no_2416(
        "interface Builder { join<P extends string>(k: P): Builder; }
         class BaseImpl { join(k: string): Builder { return this as unknown as Builder; } }
         class Impl extends BaseImpl implements Builder {}",
    );
}

// Class member re-declares the same method-local generic — universal
// quantification preserved, already accepted; kept as a guard.
#[test]
fn no_false_2416_implements_matching_generic_redeclared() {
    assert_no_2416(
        "interface Box { get<T extends string>(x: T): T; }
         class Impl implements Box { get<U extends string>(x: U): U { return x; } }",
    );
}

// Three-level mixed chain whose every drop is input-only.
#[test]
fn no_false_2416_implements_three_level_input_only_chain() {
    assert_no_2416(
        "interface Top { tag<K extends string>(k: K): Top; }
         interface Mid extends Top { tag(k: string): Mid; }
         class Leaf implements Mid { tag(k: string): Leaf { return this; } }",
    );
}

// ---------------------------------------------------------------------------
// Must STILL be REJECTED (TS2416) — the suppression must not over-fire. A
// method-local generic used covariantly in the return, or a genuine parameter /
// callback-position mismatch, is not a valid implementation.
// ---------------------------------------------------------------------------

// Bare method-local generic in the return position.
#[test]
fn keeps_2416_implements_return_only_generic() {
    assert_has_2416(
        "interface Box { get<T>(): T; }
         class Impl implements Box { get(): string { return \"\"; } }",
    );
}

// Constrained method-local generic used in BOTH input and output: a concrete
// `string` return is not assignable to the opaque `T`.
#[test]
fn keeps_2416_implements_constrained_generic_covariant_return() {
    assert_has_2416(
        "interface Spec { run<T extends string>(x: T): T; }
         class Svc implements Spec { run(x: string): string { return x; } }",
    );
}

// Two method-local generics, one input one output: the output one is unsound.
#[test]
fn keeps_2416_implements_two_generics_one_output() {
    assert_has_2416(
        "interface Conv { map<I, O>(i: I): O; }
         class Impl implements Conv { map(i: string): number { return 0; } }",
    );
}

// Method-local generic in a callback RETURN position is still covariant.
#[test]
fn keeps_2416_implements_callback_return_generic() {
    assert_has_2416(
        "interface Mapper { each<U>(f: () => U): U; }
         class Impl implements Mapper { each(f: () => string): string { return f(); } }",
    );
}

// Genuine callback-PARAMETER mismatch must not be hidden by the erased
// bivariant check: `(x: number) => void` is not assignable to the erased
// `(x: string) => void`.
#[test]
fn keeps_2416_implements_genuine_callback_param_mismatch() {
    assert_has_2416(
        "interface Sink { take<T extends string>(cb: (x: T) => void): void; }
         class Impl implements Sink { take(cb: (x: number) => void): void {} }",
    );
}
