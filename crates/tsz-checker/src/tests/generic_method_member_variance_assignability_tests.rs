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
//! The fix extends the covariant guard from bare occurrences (`T`, `T[]`,
//! `T | null`) to *application-mediated* ones (`Box<T>`, `Cell<T>`) by asking
//! the variance computer whether the parameter is observable covariantly (or
//! invariantly) through the return type. Purely contravariant (`FBox<T>` with
//! `apply(value: T)`) and phantom occurrences carry no covariant bit, so they
//! stay erasable; the overloaded-builder row stays accepted through the
//! separate multi-signature erase-to-`any` retry.
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
// Application-mediated covariant return positions: the method type parameter
// appears in the return type *only* as a generic application argument
// (`Box<T>`, `RawBuilder<A>`, …) whose enclosing generic reads it out. The
// caller still observes the parameter covariantly, so a concrete member is not
// a valid implementation and `tsc` reports TS2322 — matched now via the
// variance-aware covariant guard. (Previously a documented gap.)
// ---------------------------------------------------------------------------

#[test]
fn keeps_2322_application_mediated_covariant_return_box() {
    assert_has_2322(
        "interface Box<V> { v: V }
         type A = { m<T extends string>(): Box<T> };
         const a: A = { m(): Box<string> { return { v: '' }; } };",
    );
}

// Renamed parameters — proves the guard reads structure, not identifiers.
#[test]
fn keeps_2322_application_mediated_covariant_return_box_renamed() {
    assert_has_2322(
        "interface Holder<W> { w: W }
         type A = { m<K extends string>(): Holder<K> };
         const a: A = { m(): Holder<string> { return { w: '' }; } };",
    );
}

#[test]
fn keeps_2322_application_mediated_covariant_return_builder() {
    assert_has_2322(
        "interface RawBuilder<O> { o: O }
         interface DB { raw<A extends object>(): RawBuilder<A>; }
         const db: DB = { raw(): RawBuilder<object> { return { o: {} }; } };",
    );
}

// Invariant application (`Cell<T>` reads and writes `T`) is also observable
// covariantly, so it stays opaque and the concrete member is rejected.
#[test]
fn keeps_2322_application_mediated_invariant_return_cell() {
    assert_has_2322(
        "interface Cell<P extends string> { read: P; write(v: P): void }
         type A = { m<T extends string>(x: number): Cell<T> };
         const a: A = { m(x: number): Cell<string> { return {} as any; } };",
    );
}

// Application-mediated covariant return nested one level deeper inside another
// covariant application (`Wrap<Box<T>>`) — kept lib-independent with two
// user-defined covariant wrappers so the variance composition is exercised
// without depending on the test harness's global lib surface.
#[test]
fn keeps_2322_application_mediated_covariant_return_nested() {
    assert_has_2322(
        "interface Box<V> { v: V }
         interface Wrap<X> { x: X }
         type A = { m<T extends string>(): Wrap<Box<T>> };
         const a: A = { m(): Wrap<Box<string>> { return { x: { v: '' } }; } };",
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

// Contravariant application-mediated *return* position: `FBox<T>` only writes
// `T` (`apply(value: T)`), so the parameter is observed contravariantly and
// carries no covariant bit. The concrete `FBox<string>` member is a valid
// implementation — method-parameter bivariance covers it — and must stay
// accepted, proving the covariant guard discriminates by variance rather than
// blanket-rejecting every application-mediated return occurrence.
#[test]
fn no_false_2322_application_mediated_contravariant_return() {
    assert_no_2322(
        "interface FBox<P extends string> { apply(value: P): void }
         type A = { m<T extends string>(x: number): FBox<T> };
         const a: A = { m(x: number): FBox<string> { return {} as any; } };",
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
