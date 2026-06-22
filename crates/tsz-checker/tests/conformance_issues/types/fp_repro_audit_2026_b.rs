//! False-positive reproduction audit, round 2 (2026-06).
//!
//! Each test pins the MINIMAL repro from a canary-mined false-positive issue and
//! asserts the spurious diagnostic is absent. A passing (non-ignored) test is a
//! regression guard proving the false positive is fixed; an `#[ignore]`d test is
//! a preserved witness that still reproduces the bug.
//!
//! Candidates: #14164, #14238, #14232, #14254, #14237, #14167.

use super::super::core::*;
use tsz_common::common::ModuleKind;

/// Options matching the canary sweep flags: `--strict --target esnext
/// --module esnext --moduleResolution bundler`.
fn strict_esnext_opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        strict_null_checks: true,
        module: ModuleKind::ESNext,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// #14164 — indexed access of a callable/hybrid interface member drops
// properties / loses call signatures.
// ---------------------------------------------------------------------------

/// Repro A (zustand): a hybrid interface (call signature + `connect` property)
/// reached through `(... ? infer T : {...})['connect']` must keep `connect`.
/// tsz collapsed the hybrid to its call signature, dropping `connect`, so
/// `Parameters<...['connect']>[0]` failed to resolve -> false TS2344.
#[test]
fn issue_14164_hybrid_interface_indexed_member_keeps_property() {
    if !lib_files_available() {
        return;
    }
    let diags = compile_and_get_diagnostics_with_lib_and_options(
        r#"
interface ReduxDevtoolsExtension {
  (config?: { type?: string }): unknown
  connect: (preConfig: { type?: string }) => { send: (a: unknown) => void }
}
interface Win { __REDUX_DEVTOOLS_EXTENSION__?: ReduxDevtoolsExtension }
type Config = Parameters<
  (Win extends { __REDUX_DEVTOOLS_EXTENSION__?: infer T } ? T : { connect: (param: any) => unknown })['connect']
>[0]
const c: Config = { type: 'x' }
export { c }
"#,
        strict_esnext_opts(),
    );
    assert!(
        !has_error(&diags, 2344),
        "no TS2344 expected — indexing a hybrid interface member by ['connect'] must \
         preserve the property. Actual: {diags:#?}"
    );
}

/// Repro B (jotai): a method extracted via `Interface['method']` must keep its
/// call signature so it stays callable. tsz yielded a function type with zero
/// call signatures -> false TS2349 "not callable". Fixed: a conditional whose
/// check type referenced an unresolved user-interface `Lazy` (here the `Atom`
/// base of `Atom<unknown>['read']`) collapsed to its `never` false branch
/// instead of deferring. The still-deferred indexed access now defers rather
/// than committing the false branch, so `Parameters<...>` keeps the extracted
/// parameter callable (#14164).
#[test]
fn issue_14164_indexed_method_type_stays_callable() {
    if !lib_files_available() {
        return;
    }
    let diags = compile_and_get_diagnostics_with_lib_and_options(
        r#"
interface Atom<Value> { read: (get: Getter) => Value }
type Getter = <V>(atom: Atom<V>) => V
declare const a: Atom<number>
type GetterFromIndex = Parameters<Atom<unknown>['read']>[0]
function viaIndex(get: GetterFromIndex) { return get(a) }
export { viaIndex }
"#,
        strict_esnext_opts(),
    );
    assert!(
        !has_error(&diags, 2349),
        "no TS2349 expected — a method extracted via Interface['read'] must keep its \
         call signature and remain callable. Actual: {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// #14238 — conditional takes false branch on an undetermined relation from an
// unregistered re-entrant Lazy body (hotscript).
// ---------------------------------------------------------------------------

/// Two identically-shaped interfaces: `[MyFn] extends Fn[]` must hold, so the
/// conditional yields `1`. tsz wrongly took the false branch (`0`) on an
/// undetermined element relation -> false TS2322 assigning `1`.
#[test]
fn issue_14238_identical_shape_array_element_relation_holds() {
    let diags = compile_and_get_diagnostics(
        r#"
type Fn = { args: unknown extends infer a ? a : never };
type MyFn = { args: unknown extends infer a ? a : never };
type InArray = [MyFn] extends Fn[] ? 1 : 0;
const inArray: InArray = 1;
export { inArray };
"#,
    );
    assert!(
        !has_error(&diags, 2322),
        "no TS2322 expected — `[MyFn] extends Fn[]` holds (identical shapes), so \
         InArray = 1. Actual: {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// #14232 — conditional with concrete check vs generic extends defers instead of
// resolving the false branch (ts-essentials).
// ---------------------------------------------------------------------------

/// `[] extends [T, ...T[]]` is concretely false (empty tuple can't match a
/// non-empty tuple), so `A = "no"`. tsz deferred because the extends type
/// carries `T`, leaving `A` opaque -> false TS2322 assigning `"no"`.
// Now fixed on `main`; kept as a live regression guard (was #[ignore] for #14232).
#[test]
fn issue_14232_concrete_check_generic_extends_resolves_false_branch() {
    let diags = compile_and_get_diagnostics(
        r#"
function f<T>() {
  type A = [] extends [T, ...T[]] ? "yes" : "no";
  const a: A = "no";
  return a;
}
export { f };
"#,
    );
    assert!(
        !has_error(&diags, 2322),
        "no TS2322 expected — `[] extends [T, ...T[]]` resolves to its false branch, \
         so A = \"no\". Actual: {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// #14254 — keyof of a deferred tuple/array indexed access loses its key space
// (hkt-toolbelt).
// ---------------------------------------------------------------------------

/// `T[A]` (A constrained to `'0' | '1'`) over a concrete nested tuple must
/// expose its element's key space, so `T[A][B]` is valid. tsz left `T[A]`
/// opaque and rejected `[B]` -> false TS2536.
// Now fixed on `main`; kept as a live regression guard (was #[ignore] for #14254).
#[test]
fn issue_14254_keyof_deferred_tuple_indexed_access_keeps_key_space() {
    let diags = compile_and_get_diagnostics(
        r#"
type T = [['a', 'b'], ['c', 'd']]
type Index<A extends '0' | '1', B extends '0' | '1'> = T[A][B]
export type { Index }
"#,
    );
    assert!(
        !has_error(&diags, 2536),
        "no TS2536 expected — T[A] reduces to a tuple element whose key space accepts \
         B. Actual: {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// #14237 — indexed access of a member of a namespace whose name equals a global
// builtin left unevaluated (hotscript).
// ---------------------------------------------------------------------------

/// A local `namespace Iterator` must shadow the global `Iterator` for the LHS of
/// the qualified type name `Iterator.Obj`, so `Iterator.Obj["foo"]` is `number`.
/// tsz resolved `Iterator` to the global builtin and left `N.X[...]`
/// unevaluated -> false TS2322 assigning `5`.
#[test]
fn issue_14237_local_namespace_shadows_global_builtin_in_qualified_type() {
    if !lib_files_available() {
        return;
    }
    let diags = compile_and_get_diagnostics_with_lib_and_options(
        r#"
namespace Iterator { export type Obj = { foo: number }; }
const f: Iterator.Obj["foo"] = 5;
export { f };
"#,
        strict_esnext_opts(),
    );
    assert!(
        !has_error(&diags, 2322),
        "no TS2322 expected — local `namespace Iterator` shadows the global, so \
         Iterator.Obj[\"foo\"] is number. Actual: {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// #14167 — deferred conditional check operand not constrained to its checked
// type in the true branch (missing SubstitutionType).
// ---------------------------------------------------------------------------

/// Inside the true branch of `CamelCase<V> extends string ? ...`, the operand
/// `CamelCase<V>` must be seen as narrowed to `string` so `Capitalize<...>`'s
/// constraint passes. The fix wraps the structured check operand in a
/// `SubstitutionType`. Kept as a live regression guard.
#[test]
fn issue_14167_conditional_true_branch_constrains_check_operand() {
    if !lib_files_available() {
        return;
    }
    let diags = compile_and_get_diagnostics_with_lib_and_options(
        r#"
type CamelCase<Type> = Type extends string ? `camel${Type}` : Type;
type _PascalCase<V> = CamelCase<V> extends string
	? Capitalize<CamelCase<V>>
	: CamelCase<V>;
export {};
"#,
        strict_esnext_opts(),
    );
    assert!(
        !has_error(&diags, 2344),
        "no TS2344 expected — in the true branch, CamelCase<V> is narrowed to string \
         so Capitalize<CamelCase<V>>'s constraint passes. Actual: {diags:#?}"
    );
}
