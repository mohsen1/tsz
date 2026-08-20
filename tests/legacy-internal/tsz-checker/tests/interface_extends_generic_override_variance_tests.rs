//! Regression tests for issue #10804 — generic method override variance in
//! interface `extends` chains (TS2430).
//!
//! When an interface `extends` a base whose method is *generic* and the
//! override drops the method-local type parameter to a concrete type, the
//! override is a valid specialization **only** when the dropped type parameter
//! appears solely in input (contravariant) positions: the override's wider
//! concrete parameter accepts every instantiation of the base's type parameter.
//! `with(k: string): Sub` is therefore a valid override of
//! `with<K extends string>(k: K): Base` — tsc accepts it. By contrast, a
//! method-local generic used in a covariant (return/output) position cannot be
//! dropped: `m(): string` is not a valid override of `m<T>(): T`, because a
//! caller could instantiate `T` with a subtype and expect it back. tsc reports
//! TS2430 for those.
//!
//! The strict no-erase relation used by the interface-extends member check
//! rejected *both* shapes, producing a false TS2430 on the sound input-only
//! case. The `class extends` (TS2416) and `implements` paths already accept the
//! input-only specialization through their generic-aware relations; this routes
//! the interface-extends path through the same discriminator so all three
//! heritage forms make identical variance decisions (matching tsc's
//! `compareSignaturesRelated`). Verified against tsc 6.0.2.

use crate::test_utils::check_source_codes;

fn assert_has_2430(src: &str) {
    let codes = check_source_codes(src);
    assert!(
        codes.contains(&2430),
        "expected TS2430, got none. Got: {codes:?}\nSource:\n{src}"
    );
}

fn assert_no_2430(src: &str) {
    let codes = check_source_codes(src);
    assert!(
        !codes.contains(&2430),
        "unexpected TS2430 (false positive). Got: {codes:?}\nSource:\n{src}"
    );
}

// ---------------------------------------------------------------------------
// Input-only method-local generic dropped to its constraint: tsc ACCEPTS
// (the override's wider concrete parameter accepts every instantiation).
// These were the false positives fixed by #10804.
// ---------------------------------------------------------------------------

#[test]
fn no_false_2430_input_only_constrained_generic_covariant_return() {
    assert_no_2430(
        "interface Builder { with<K extends string>(k: K): Builder; }
         interface SubBuilder extends Builder { with(k: string): SubBuilder; }
         declare const d: SubBuilder;",
    );
}

// Renamed method, interface, and type parameter — proves the rule is
// structural, not keyed to any identifier spelling.
#[test]
fn no_false_2430_input_only_constrained_generic_renamed() {
    assert_no_2430(
        "interface Bldr { put<Z extends string>(z: Z): Bldr; }
         interface SubB extends Bldr { put(z: string): SubB; }
         declare const d: SubB;",
    );
}

// Same (non-narrowed) return type — still input-only, still accepted.
#[test]
fn no_false_2430_input_only_constrained_generic_same_return() {
    assert_no_2430(
        "interface Builder { with<K extends string>(k: K): Builder; }
         interface Sub extends Builder { with(k: string): Builder; }
         declare const d: Sub;",
    );
}

// Unconstrained method-local generic in input-only position: the constraint is
// `unknown`, and `(k: unknown) => Sub` accepts every `(k: K) => Base`.
#[test]
fn no_false_2430_input_only_unconstrained_generic() {
    assert_no_2430(
        "interface Base { with<K>(k: K): Base; }
         interface Sub extends Base { with(k: unknown): Sub; }
         declare const d: Sub;",
    );
}

// Generic base interface plus an input-only method-local generic, with a
// covariant return narrowing of the self type.
#[test]
fn no_false_2430_generic_base_interface_input_only_method_generic() {
    assert_no_2430(
        "interface Base<T> { with<K extends string>(k: K, t: T): Base<T>; }
         interface Sub<T> extends Base<T> { with(k: string, t: T): Sub<T>; }
         declare const d: Sub<number>;",
    );
}

// Override re-declares the same method-local generic (already accepted before
// the fix; kept as a guard so the new fallback does not perturb it).
#[test]
fn no_false_2430_matching_generic_method_redeclared() {
    assert_no_2430(
        "interface Base { m<T extends string>(x: T): T; }
         interface Der extends Base { m<U extends string>(x: U): U; }
         declare const d: Der;",
    );
}

// Method-parameter bivariance must be preserved: narrowing a method parameter
// is accepted by tsc (unsound but intentional), and the fix must not regress it.
#[test]
fn no_false_2430_method_param_narrowing_stays_bivariant() {
    assert_no_2430(
        "interface Animal {}
         interface Dog extends Animal {}
         interface Base { handle(x: Animal): void; }
         interface Der extends Base { handle(x: Dog): void; }
         declare const d: Der;",
    );
}

// ---------------------------------------------------------------------------
// Method-local generic used in a covariant (return/output) position: tsc
// REJECTS (TS2430). Dropping it is unsound, so the fix must NOT over-suppress.
// ---------------------------------------------------------------------------

// Bare method-local generic in return position.
#[test]
fn keeps_2430_bare_generic_return_dropped() {
    assert_has_2430(
        "interface Base { m<T>(): T; }
         interface Der extends Base { m(): string; }
         declare const d: Der;",
    );
}

// Constrained method-local generic used in both parameter AND return position.
#[test]
fn keeps_2430_constrained_generic_param_and_return_dropped() {
    assert_has_2430(
        "interface Base { m<T extends string>(x: T): T; }
         interface Der extends Base { m(x: string): string; }
         declare const d: Der;",
    );
}

// Renamed variant of the param+return case.
#[test]
fn keeps_2430_constrained_generic_param_and_return_dropped_renamed() {
    assert_has_2430(
        "interface Base { m<K extends string>(x: K): K; }
         interface Der extends Base { m(x: string): string; }
         declare const d: Der;",
    );
}

// Method-local generic in a callback *return* position (`f: () => U`): the
// override only accepts callbacks returning `string`, so dropping `U` is
// unsound even though `U` is nested inside a parameter.
#[test]
fn keeps_2430_callback_return_generic_dropped() {
    assert_has_2430(
        "interface Base { each<U>(f: () => U): void; }
         interface Der extends Base { each(f: () => string): void; }
         declare const d: Der;",
    );
}

// Method-local generic in a covariant position reached through the method's own
// return (`map<U>(f: () => U): U`).
#[test]
fn keeps_2430_method_return_via_callback_generic_dropped() {
    assert_has_2430(
        "interface Base { map<U>(f: () => U): U; }
         interface Der extends Base { map(f: () => string): string; }
         declare const d: Der;",
    );
}

// Two method-local generics: one input-only (`I`), one used in return (`O`).
// `O`'s covariant use makes the whole drop unsound. tsc reports.
#[test]
fn keeps_2430_mixed_input_output_generics_dropped() {
    assert_has_2430(
        "interface Base { f<I extends string, O>(i: I, o: O): O; }
         interface Der extends Base { f(i: string, o: number): number; }
         declare const d: Der;",
    );
}

// ---------------------------------------------------------------------------
// Genuine non-generic member mismatch must still report (the fallback must not
// rescue real incompatibilities).
// ---------------------------------------------------------------------------

#[test]
fn keeps_2430_genuine_return_type_mismatch() {
    assert_has_2430(
        "interface Base { m(x: string): number; }
         interface Der extends Base { m(x: string): string; }
         declare const d: Der;",
    );
}
