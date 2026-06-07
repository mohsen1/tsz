//! Regression tests for issue #10812 — generic method-member variance in
//! structural (assignability) object comparison.
//!
//! Issue #10869 fixed the class `extends`/`implements` *override* paths, which
//! run through the strict member-compatibility relation (`NO_ERASE_GENERICS`).
//! The general assignability relation (variable initialisers, function
//! arguments, return positions, array elements, plain `interface`/object-type
//! to object-type assignment) instead runs with generic erasure enabled.
//!
//! In that mode a generic *target* method whose method-local type parameter is
//! only observed through a generic application — e.g. `T[]`, `Box<T>`,
//! `T | null` in the **return** type — was erased to its constraint before the
//! comparison. A concrete implementation whose result merely satisfies the
//! constraint was then wrongly accepted, hiding the TS2322 that `tsc` reports.
//!
//! The structural rule (verified against `tsc` 6.0.2): erasing a target method
//! type parameter to its constraint *widens* it. That is safe in a
//! contravariant (parameter) position — method-parameter bivariance already
//! covers it — but unsafe in a covariant (return/output) position, where it
//! turns a genuine mismatch into an apparent match. A type parameter that
//! occurs anywhere in the return type must therefore stay opaque, so the
//! concrete member is rejected exactly as in the `extends`/`implements` paths.
//!
//! A subtlety the fix had to account for: a signature's `type_params` list and
//! its body can carry distinct `TypeId`s for the same logical parameter, while
//! the erase substitution keys on the parameter *name*. The covariant guard
//! matches the parameter by name so it cannot be bypassed by that `TypeId`
//! skew.

use crate::test_utils::check_source_codes;

fn assert_has_2322(src: &str) {
    let codes = check_source_codes(src);
    assert!(
        codes.contains(&2322),
        "expected TS2322, got none. Got: {codes:?}\nSource:\n{src}"
    );
}

fn assert_no_2322(src: &str) {
    let codes = check_source_codes(src);
    assert!(
        !codes.contains(&2322),
        "unexpected TS2322 (false positive). Got: {codes:?}\nSource:\n{src}"
    );
}

// ---------------------------------------------------------------------------
// Covariant return positions: concrete member is NOT a valid implementation of
// a generic-method target, so tsc (and now tsz) reports TS2322.
// ---------------------------------------------------------------------------

// Method-shorthand member, constrained generic in `T[]` return — the headline
// repro from #10812.
#[test]
fn keeps_2322_objlit_method_constrained_array_return() {
    assert_has_2322(
        "type A = { m<T extends string>(x: number): T[] };
         const a: A = { m(x: number): string[] { return []; } };",
    );
}

// Renamed type parameter — proves the rule is structural, not identifier-keyed.
#[test]
fn keeps_2322_objlit_method_constrained_array_return_renamed() {
    assert_has_2322(
        "type A = { m<K extends string>(x: number): K[] };
         const a: A = { m(x: number): string[] { return []; } };",
    );
}

// Bare method-local generic in return position.
#[test]
fn keeps_2322_objlit_method_bare_generic_return() {
    assert_has_2322(
        "type A = { m<T extends string>(x: number): T };
         const a: A = { m(x: number): string { return ''; } };",
    );
}

// Union (`T | null`) in covariant return position.
#[test]
fn keeps_2322_objlit_method_union_return() {
    assert_has_2322(
        "type A = { m<T extends string>(): T | null };
         const a: A = { m(): string | null { return null; } };",
    );
}

// Object-literal type with the parameter nested in a covariant return field.
#[test]
fn keeps_2322_objlit_method_nested_object_return() {
    assert_has_2322(
        "type A = { m<T extends string>(): { v: T } };
         const a: A = { m(): { v: string } { return { v: '' }; } };",
    );
}

// The source need not be a fresh object literal — a concrete *variable* assigned
// to a generic-method target is rejected the same way.
#[test]
fn keeps_2322_concrete_variable_to_generic_method_target() {
    assert_has_2322(
        "type Conc = { m(x: number): string[] };
         type Gen = { m<T extends string>(x: number): T[] };
         declare const c: Conc;
         const g: Gen = c;",
    );
}

// The check fires through every assignability context, not only `const` init:
// nested members, function arguments, return statements and array elements.
#[test]
fn keeps_2322_nested_member_context() {
    assert_has_2322(
        "type Inner = { m<T extends string>(x: number): T[] };
         type Outer = { inner: Inner };
         const o: Outer = { inner: { m(x: number): string[] { return []; } } };",
    );
}

#[test]
fn keeps_2322_function_argument_context() {
    assert_has_2322(
        "type A = { m<T extends string>(x: number): T[] };
         declare function take(a: A): void;
         take({ m(x: number): string[] { return []; } });",
    );
}

#[test]
fn keeps_2322_return_statement_context() {
    assert_has_2322(
        "type A = { m<T extends string>(x: number): T[] };
         function make(): A { return { m(x: number): string[] { return []; } }; }",
    );
}

#[test]
fn keeps_2322_array_element_context() {
    assert_has_2322(
        "type A = { m<T extends string>(x: number): T[] };
         const arr: A[] = [{ m(x: number): string[] { return []; } }];",
    );
}

// ---------------------------------------------------------------------------
// Known remaining gap (documented, not yet at parity).
//
// When the method type parameter appears in the return type *only* as a generic
// application argument (`Box<T>`, `RawBuilder<A>`, `PromiseLike<T>`, …), tsz
// still erases it to its constraint, so the concrete member is accepted even
// though `tsc` reports TS2322. That erase path is load-bearing for generic
// inference and overloaded-builder rows (a `PromiseLike<TResult>` return drives
// `Promise.then` inference, for instance), so widening the covariant guard to
// cover application-mediated returns is deferred to dedicated follow-up work.
// These tests pin the *current* behavior so a future fix surfaces here.
// ---------------------------------------------------------------------------

#[test]
fn known_gap_application_mediated_covariant_return_box() {
    // `tsc` 6.0.2 reports TS2322 here; tsz currently does not.
    assert_no_2322(
        "interface Box<V> { v: V }
         type A = { m<T extends string>(): Box<T> };
         const a: A = { m(): Box<string> { return { v: '' }; } };",
    );
}

#[test]
fn known_gap_application_mediated_covariant_return_builder() {
    // `tsc` 6.0.2 reports TS2322 here; tsz currently does not.
    assert_no_2322(
        "interface RawBuilder<O> { o: O }
         interface DB { raw<A extends object>(): RawBuilder<A>; }
         const db: DB = { raw(): RawBuilder<object> { return { o: {} }; } };",
    );
}

// ---------------------------------------------------------------------------
// Must STILL be accepted (no false positives).
// ---------------------------------------------------------------------------

// Source method re-declares the same generic — universal quantification is
// preserved, so the implementation is valid.
#[test]
fn no_false_2322_objlit_method_matching_generic() {
    assert_no_2322(
        "type A = { m<T extends string>(x: T): T };
         const a: A = { m<U extends string>(x: U): U { return x; } };",
    );
}

// Phantom/unused method-local generic: dropping it is sound, tsc accepts.
#[test]
fn no_false_2322_objlit_method_phantom_generic() {
    assert_no_2322(
        "type A = { m<T extends string>(x: number): number };
         const a: A = { m(x: number): number { return x; } };",
    );
}

// Contravariant (parameter) position: erasing the parameter to its constraint
// is neutralised by method-parameter bivariance, so the concrete member is
// accepted — the leniency the erase branch exists to provide is preserved.
#[test]
fn no_false_2322_contravariant_parameter_wrapped() {
    assert_no_2322(
        "interface Box<O> { o: O }
         interface DB { take<A extends object>(b: Box<A>): void; }
         const db: DB = { take(b: Box<object>): void {} };",
    );
}

// Overloaded generic builder: the concrete implementation satisfies the
// non-generic overload signature, so overload matching accepts it.
#[test]
fn no_false_2322_overloaded_generic_builder() {
    assert_no_2322(
        "interface RawBuilder<O> { o: O }
         interface DB {
           raw(sql: string): RawBuilder<unknown>;
           raw<A extends object>(sql: string): RawBuilder<A>;
         }
         const db: DB = { raw(sql: string): RawBuilder<unknown> { return { o: {} }; } };",
    );
}

// An arrow-function-typed property (not a method shorthand) was already handled
// correctly; keep it as a guard that the two member forms agree.
#[test]
fn no_false_regression_arrow_property_still_reports() {
    // This shape *should* report TS2322 (covariant mismatch); the guard ensures
    // method and property members make the same decision.
    assert_has_2322(
        "type A = { m: <T extends string>(x: number) => T[] };
         const a: A = { m: (x: number): string[] => [] };",
    );
}
