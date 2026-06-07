//! Regression tests: assigning a generic function to a concrete function type
//! must instantiate the generic source's type parameters regardless of whether
//! the *target* parameter/return type happens to contain a nested generic
//! signature whose type parameter shares a *name* with the source's.
//!
//! tsz interns `TypeParameter`s structurally by name, so two unrelated `T`s
//! collapse to the same `TypeId`. The function-subtype relation previously had a
//! shortcut that, on seeing the source's type-parameter `TypeId` anywhere inside
//! the target, assumed shared identity and cleared the source quantifier without
//! instantiation — leaving the source parameter free and producing a spurious
//! `'X' is not assignable to 'T'` (`TS2322`/`TS2345`). The shortcut now only
//! fires on *free* occurrences, so a coincidentally same-named parameter bound
//! by a nested signature in the target no longer derails instantiation.
//!
//! Witness family: kysely's `build: ColumnDefinitionBuilderCallback = noop`,
//! where `noop<T>(obj: T): T` is assigned to a callback whose parameter type
//! (`ColumnDefinitionBuilder`) declares a generic method `$call<T>(...)`.
use crate::test_utils::{check_source_diagnostics, diagnostics_with_code};

/// Core repro: source `<T>(obj: T) => T` assigned to `(b: Box) => Box` where
/// `Box` carries a generic method `call<T>` (same name as the source's `T`).
/// The nested-bound `T` must not be mistaken for the source's `T`.
#[test]
fn generic_identity_assignable_to_callback_with_colliding_nested_param() {
    let diags = check_source_diagnostics(
        r#"
function noop<T>(obj: T): T { return obj; }

interface Box {
  brand: "box";
  call<T>(f: (x: this) => T): T;
}
type BoxCb = (b: Box) => Box;

// Force `Box` to materialize to its full member shape (with `call<T>`).
declare const probe: Box;
const forced = probe.call((x) => x);

const cb: BoxCb = noop; // must be OK — T instantiates to Box
"#,
    );

    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert!(
        ts2322.is_empty(),
        "unexpected TS2322 assigning generic identity to colliding callback: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Name-independence: the same program with the nested parameter renamed must
/// behave identically (no diagnostics). Demonstrates the fix is structural, not
/// keyed on a particular identifier.
#[test]
fn generic_identity_assignable_to_callback_with_renamed_nested_param() {
    let diags = check_source_diagnostics(
        r#"
function ident<Elem>(value: Elem): Elem { return value; }

interface Widget {
  brand: "widget";
  apply<Result>(f: (x: this) => Result): Result;
}
type WidgetCb = (w: Widget) => Widget;

declare const probe: Widget;
const forced = probe.apply((x) => x);

const cb: WidgetCb = ident; // OK regardless of the nested type-param name
"#,
    );

    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert!(
        ts2322.is_empty(),
        "renamed nested param should not change assignability: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// The same collision in argument position (`TS2345`): passing the generic
/// identity to a parameter whose type declares a nested `<T>` method.
#[test]
fn generic_identity_passable_as_argument_with_colliding_nested_param() {
    let diags = check_source_diagnostics(
        r#"
function noop<T>(obj: T): T { return obj; }

interface Col {
  brand: "col";
  pipe<T>(f: (x: this) => T): T;
}

declare function addColumn(build: (b: Col) => Col): void;

declare const probe: Col;
const forced = probe.pipe((x) => x);

addColumn(noop); // must be OK — argument instantiates T to Col
"#,
    );

    let ts2345 = diagnostics_with_code(&diags, 2345);
    assert!(
        ts2345.is_empty(),
        "unexpected TS2345 passing generic identity as colliding callback arg: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Covariant self-returning override across generic builder classes whose
/// methods carry their own same-named type parameters must not raise a spurious
/// `TS2416` (the kysely `Kysely<DB>`/`QueryCreator<DB>` family). The derived
/// override returns the more specific instance type.
#[test]
fn self_returning_override_with_colliding_method_param_no_ts2416() {
    let diags = check_source_diagnostics(
        r#"
class QueryCreator<DB> {
  withPlugin(): QueryCreator<DB> { return this; }
  call<T>(f: (x: this) => T): T { return f(this); }
}

class Kysely<DB> extends QueryCreator<DB> {
  override withPlugin(): Kysely<DB> { return this; }
  override call<T>(f: (x: this) => T): T { return f(this); }
}

declare const k: Kysely<{ a: number }>;
const forced = k.call((x) => x);
"#,
    );

    let ts2416 = diagnostics_with_code(&diags, 2416);
    assert!(
        ts2416.is_empty(),
        "unexpected TS2416 on self-returning override with colliding method param: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}
