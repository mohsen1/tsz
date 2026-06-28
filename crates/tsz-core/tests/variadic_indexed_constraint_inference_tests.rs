//! Full-pipeline regression coverage (issue #14748) for generic calls whose
//! variadic-rest parameter is a nested generic alias over an indexed access
//! `T[number]` of a *constrained* type parameter. This is remeda's
//! `purryOrderRules` family (`nthBy`, `firstBy`, `sortBy`, `takeFirstBy`).
//!
//! These cases require the embedded standard library (`Readonly<…>`) and the
//! full checker pass, so they live here rather than in a lib-less unit harness.
//!
//! Structural rule: a generic indexed access `T[K]` (whose object/index still
//! carries free type parameters) must stay a deferred indexed-access type
//! during type-argument expansion. tsc consults `T`'s constraint only as the
//! access's *base constraint* for relations and never bakes it into the type,
//! so a later substitution `T = number[]` resolves `T[number]` to the real
//! element type. tsz previously evaluated such an argument eagerly inside the
//! nested alias `Readonly<NonEmptyArray<OrderRule<T[number]>>>`, collapsing
//! `T[number]` to `(readonly unknown[])[number] = unknown` before the leading
//! `data: T` argument fixed `T`, producing a spurious `TS2769`/`TS2345`.

use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::test_fixtures::{merge_shared_lib_symbols, setup_lib_contexts};
use tsz_solver::construction::TypeInterner;

fn strict_diagnostic_codes(source: &str) -> Vec<u32> {
    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "unexpected parse diagnostics: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions {
            strict: true,
            ..crate::checker::context::CheckerOptions::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);
    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

/// The reported witness: a `dataFirst` overload `(data: T, index, ...rules)` and
/// a `dataLast` overload `(index, ...rules)`, both with the variadic rest
/// constrained by `T[number]`. tsc accepts the `dataFirst` call cleanly.
#[test]
fn datafirst_overload_with_variadic_indexed_constraint_is_clean() {
    let source = r#"
type IterableContainer<T = unknown> = readonly T[] | readonly [];
type NonEmptyArray<T> = [T, ...T[]];
type OrderRule<T> = (x: T) => number;

declare function nb<T extends IterableContainer>(
  data: T,
  index: number,
  ...rules: Readonly<NonEmptyArray<OrderRule<T[number]>>>
): T[number] | undefined;
declare function nb<T extends IterableContainer>(
  index: number,
  ...rules: Readonly<NonEmptyArray<OrderRule<T[number]>>>
): (data: T) => T[number] | undefined;

declare const ident: (x: number) => number;
const data = [2, 1, 3];
const r = nb(data, 0, ident);
"#;
    let codes = strict_diagnostic_codes(source);
    assert!(
        codes.is_empty(),
        "dataFirst overload with variadic `T[number]` constraint should be clean, got: {codes:?}"
    );
}

/// A single overload reproduces the same collapse — the defect is the nested
/// alias over `T[number]`, not the two-overload interaction. Binder names are
/// varied to prove the fix is structural, not keyed to identifiers.
#[test]
fn single_overload_nested_alias_indexed_constraint_is_clean() {
    let source = r#"
type Many<U> = [U, ...U[]];
type Rule<U> = (value: U) => number;

declare function pick<Coll extends readonly unknown[]>(
  source: Coll,
  ...rules: Readonly<Many<Rule<Coll[number]>>>
): void;

declare const byNum: (value: number) => number;
pick([1, 2, 3], byNum);
"#;
    let codes = strict_diagnostic_codes(source);
    assert!(
        codes.is_empty(),
        "single overload with nested-alias `Coll[number]` rest should be clean, got: {codes:?}"
    );
}

/// The inferred element type must be the *real* element, so a genuinely
/// mismatched callback still reports. `data: string[]` fixes
/// `T[number] = string`, and the `(x: number) => number` rule is rejected: the
/// fix preserves the parameter link rather than blanket-suppressing the access.
#[test]
fn mismatched_element_callback_still_reports() {
    let source = r#"
type NonEmptyArray<T> = [T, ...T[]];
type OrderRule<T> = (x: T) => number;

declare function nb<T extends readonly unknown[]>(
  data: T,
  index: number,
  ...rules: Readonly<NonEmptyArray<OrderRule<T[number]>>>
): void;

declare const ident: (x: number) => number;
const data: string[] = ["a"];
nb(data, 0, ident);
"#;
    let codes = strict_diagnostic_codes(source);
    assert!(
        codes.contains(&2345) || codes.contains(&2769),
        "string-element data must reject the number callback (TS2345/TS2769), got: {codes:?}"
    );
}

/// A contextually-typed callback receives the real element type, so a misuse of
/// the parameter (`number` assigned to `string`) reports TS2322 — the access did
/// not collapse to `unknown`.
#[test]
fn contextual_callback_parameter_resolves_to_real_element() {
    let source = r#"
type NonEmptyArray<T> = [T, ...T[]];
type OrderRule<T> = (x: T) => number;

declare function nb<T extends readonly unknown[]>(
  data: T,
  ...rules: Readonly<NonEmptyArray<OrderRule<T[number]>>>
): void;

const data = [1, 2, 3];
nb(data, (x) => { const y: string = x; return 0; });
"#;
    let codes = strict_diagnostic_codes(source);
    assert!(
        codes.contains(&2322),
        "callback parameter (real type `number`) assigned to `string` should report TS2322, got: {codes:?}"
    );
}
