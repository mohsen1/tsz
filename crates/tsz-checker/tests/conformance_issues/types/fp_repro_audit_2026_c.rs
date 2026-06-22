//! Round-3 false-positive reproduction audit (2026-06). Each test below is the
//! minimal witness extracted from a mined canary issue. A passing (non-ignored)
//! test means the false positive is FIXED on main and the test stands as a
//! regression guard. An `#[ignore = "reproduces #N"]` test still reproduces the
//! FP and preserves the witness for the eventual fix.
//!
//! Helpers mirror `spread_param_constraint_2345.rs`: single-file witnesses use
//! `compile_*_with_lib_and_options` (most repros reference lib intrinsics such
//! as `Parameters`/`Capitalize`/`Record`); multi-file witnesses use
//! `compile_named_files_get_diagnostics_with_lib_and_options` guarded by
//! `lib_files_available()`.

use super::super::core::*;
use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn strict_opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        strict_null_checks: true,
        module: ModuleKind::ESNext,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// #14164 — indexed access of a callable/hybrid interface member drops
// properties / loses call signatures. Repro A: TS2344; Repro B: TS2349.
// ---------------------------------------------------------------------------

#[test]
fn issue_14164_hybrid_interface_index_no_ts2344() {
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
        strict_opts(),
    );
    assert!(
        !has_error(&diags, 2344),
        "no TS2344 — hybrid interface indexed member must keep its `connect` property. Actual: {diags:#?}"
    );
}

#[test]
fn issue_14164_extracted_method_callable_no_ts2349() {
    let diags = compile_and_get_diagnostics_with_lib_and_options(
        r#"
interface Atom<Value> { read: (get: Getter) => Value }
type Getter = <V>(atom: Atom<V>) => V
declare const a: Atom<number>
type GetterFromIndex = Parameters<Atom<unknown>['read']>[0]
function viaIndex(get: GetterFromIndex) { return get(a) }
export { viaIndex }
"#,
        strict_opts(),
    );
    assert!(
        !has_error(&diags, 2349),
        "no TS2349 — extracted method type must keep its call signature. Actual: {diags:#?}"
    );
}

// #14164 adjacent matrix: the defect is a conditional whose check type references
// an unresolved user-interface `Lazy` collapsing to its false branch. Vary the
// binders/shape to prove the fix is structural, not witness-shaped, and pin the
// negative control so a genuinely non-callable member still reports TS2349.

/// Renamed binders + a non-generic extracted method (no `<V>` on the callback
/// alias). Still routes through `Parameters<Store<unknown>['select']>[0]` whose
/// `Store` base is an unresolved `Lazy` while the enclosing function's type is
/// computed; the call must stay callable.
#[test]
fn issue_14164_extracted_method_non_generic_renamed_callable() {
    let diags = compile_and_get_diagnostics_with_lib_and_options(
        r#"
interface Store<State> { select: (read: Reader) => State }
type Reader = (store: Store<number>) => number
declare const s: Store<number>
type ReaderFromIndex = Parameters<Store<unknown>['select']>[0]
function viaIndex(read: ReaderFromIndex) { return read(s) }
export { viaIndex }
"#,
        strict_opts(),
    );
    assert!(
        !has_error(&diags, 2349),
        "no TS2349 — non-generic extracted method (renamed binders) stays callable. Actual: {diags:#?}"
    );
}

/// Negative control: an extracted member that is genuinely NOT callable (a
/// `number`-typed property reached the same way) must still report TS2349 —
/// the deferral only applies while a referenced `Lazy` is unresolved, never to
/// a resolved non-callable member.
#[test]
fn issue_14164_extracted_non_callable_member_still_ts2349() {
    let diags = compile_and_get_diagnostics_with_lib_and_options(
        r#"
interface Atom<Value> { read: (get: Getter) => Value; size: number }
type Getter = <V>(atom: Atom<V>) => V
declare const sz: Atom<unknown>['size']
const r = sz()
export { r }
"#,
        strict_opts(),
    );
    assert!(
        has_error(&diags, 2349),
        "TS2349 expected — a number-typed indexed member is not callable. Actual: {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// #14254 — TS2536: keyof of a deferred tuple/array indexed access loses its
// key space (hkt-toolbelt).
// ---------------------------------------------------------------------------

#[test]
fn issue_14254_deferred_tuple_index_keyspace_no_ts2536() {
    let diags = compile_and_get_diagnostics_with_lib_and_options(
        r#"
type T = [['a', 'b'], ['c', 'd']]
type Index<A extends '0' | '1', B extends '0' | '1'> = T[A][B]
export type { Index }
"#,
        strict_opts(),
    );
    assert!(
        !has_error(&diags, 2536),
        "no TS2536 — `T[A][B]` over a concrete tuple must accept tuple key space. Actual: {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// #14216 — TS2552/TS2749: export-alias clobbers a local class of the same name
// in value and type space (purify-ts).
// ---------------------------------------------------------------------------

#[test]
fn issue_14216_export_alias_keeps_local_class_no_ts2749_ts2552() {
    let diags = compile_and_get_diagnostics_with_lib_and_options(
        r#"
class Box { constructor(public value: number) {} }
const useType = (b: Box): number => b.value
const useValue = (): Box => new Box(1)
const box = (n: number) => new Box(n)
export { box as Box }
export { useType, useValue }
"#,
        strict_opts(),
    );
    assert!(
        !has_error(&diags, 2749),
        "no TS2749 — local class `Box` must stay usable as a type. Actual: {diags:#?}"
    );
    assert!(
        !has_error(&diags, 2552),
        "no TS2552 — local class `Box` must stay usable as a value. Actual: {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// #14230 — TS2536: mapped type cannot be indexed by the bare symbol intrinsic
// (ts-essentials).
// ---------------------------------------------------------------------------

#[test]
#[ignore = "reproduces #14230"]
fn issue_14230_mapped_index_by_symbol_no_ts2536() {
    let diags = compile_and_get_diagnostics_with_lib_and_options(
        r#"
type M = { [K in string | number | symbol]: number };
type A = M[symbol];
export type { A };
"#,
        strict_opts(),
    );
    assert!(
        !has_error(&diags, 2536),
        "no TS2536 — a `string|number|symbol`-keyed mapped has a symbol index signature. Actual: {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// #14225 — TS2503: re-imported namespace shadowed by a same-named local type
// alias loses its namespace meaning for a qualified-type LHS (ts-toolbelt).
// Multi-file.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "reproduces #14225 (issue closed but minimal repro still emits TS2503)"]
fn issue_14225_reimported_namespace_qualified_type_no_ts2503() {
    if !lib_files_available() {
        return;
    }
    let files = &[
        ("lib.ts", "export type Intersect<A, B> = A & B\n"),
        ("index.ts", "import * as T from './lib'\nexport { T }\n"),
        (
            "use.ts",
            r#"
import { T } from './index'
type T = [1, 2, 3]
type R = T.Intersect<{ a: 1 }, { b: 2 }>
const r: R = { a: 1, b: 2 }
export { r }
"#,
        ),
    ];
    let diags =
        compile_named_files_get_diagnostics_with_lib_and_options(files, "use.ts", strict_opts());
    assert!(
        !has_error(&diags, 2503),
        "no TS2503 — re-imported namespace `T` must anchor a qualified type despite the local `type T`. Actual: {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// #14213 — TS7006: contextual type from an inner curried arrow's return
// annotation not propagated to returned object-literal method params (fp-ts).
// ---------------------------------------------------------------------------

#[test]
fn issue_14213_curried_arrow_return_annotation_contextual_no_ts7006() {
    let diags = compile_and_get_diagnostics_with_lib_and_options(
        r#"
interface Algebra { readonly meet: (x: number, y: number) => number }
export const f =
  (base: number) =>
  (): Algebra => ({
    meet: (x, y) => base + x + y
  })
"#,
        strict_opts(),
    );
    assert!(
        !has_error(&diags, 7006),
        "no TS7006 — annotated return type must contextually type the returned object-literal method params. Actual: {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// #14167 — deferred conditional check operand not constrained to its checked
// type in the true branch (SubstitutionType missing). TS2344.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "reproduces #14167 (issue closed but minimal repro still emits TS2344)"]
fn issue_14167_conditional_true_branch_substitution_no_ts2344() {
    let diags = compile_and_get_diagnostics_with_lib_and_options(
        r#"
type CamelCase<Type> = Type extends string ? `camel${Type}` : Type;
type _PascalCase<V> = CamelCase<V> extends string
	? Capitalize<CamelCase<V>>
	: CamelCase<V>;
export {};
"#,
        strict_opts(),
    );
    assert!(
        !has_error(&diags, 2344),
        "no TS2344 — `CamelCase<V>` must be narrowed to string in the conditional true branch. Actual: {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// #14263 — TS2348: imported value named like a global constructor not
// shadowing the global in call position (typebox). Multi-file.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "reproduces #14263 (issue closed but minimal repro still emits TS2348)"]
fn issue_14263_imported_value_shadows_global_ctor_no_ts2348() {
    if !lib_files_available() {
        return;
    }
    let files = &[
        (
            "lib.ts",
            r#"
export function Promise(x: number): string { return "" }
export function Map(x: number): string { return "" }
"#,
        ),
        (
            "main.ts",
            r#"
import { Promise, Map } from './lib'
const a: string = Promise(1)
const b: string = Map(2)
export { a, b }
"#,
        ),
    ];
    let diags =
        compile_named_files_get_diagnostics_with_lib_and_options(files, "main.ts", strict_opts());
    assert!(
        !has_error(&diags, 2348),
        "no TS2348 — imported `Promise`/`Map` values must shadow the global ctor in call position. Actual: {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// #14261 — TS18046: callee type-arg from contextual return blocked by a
// same-named outer type parameter (fp-ts).
// ---------------------------------------------------------------------------

#[test]
#[ignore = "reproduces #14261"]
fn issue_14261_contextual_return_binding_samename_typeparam_no_ts18046() {
    let diags = compile_and_get_diagnostics_with_lib_and_options(
        r#"
type Ordering = -1 | 0 | 1
interface Ord<A> { readonly compare: (first: A, second: A) => Ordering }
declare const fromCompare: <A>(compare: Ord<A>['compare']) => Ord<A>
export const fromBlock = <A>(O: Ord<A>): Ord<A[]> =>
  fromCompare((x, y) => { return x.length < y.length ? -1 : 1 })
"#,
        strict_opts(),
    );
    assert!(
        !has_error(&diags, 18046),
        "no TS18046 — callee type-arg from contextual return must bind by symbol identity, not name. Actual: {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// #14220 — TS2339: primitive string wrongly satisfies Record<any, any> in a
// conditional check (zod).
// ---------------------------------------------------------------------------

#[test]
#[ignore = "reproduces #14220 (issue closed but minimal repro still emits TS2339)"]
fn issue_14220_primitive_not_record_conditional_branch_no_ts2339() {
    let diags = compile_and_get_diagnostics_with_lib_and_options(
        r#"
type Flatten<T> = { [k in keyof T]: T[k] };
type Omit2<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;
type Normalize<T> = T extends undefined ? never
  : T extends Record<any, any>
    ? Flatten<{ [k in keyof Omit2<T, "error" | "message">]: T[k] }>
    : never;
interface Params { case?: "sensitive" | "insensitive" | undefined; truthy?: string[]; }
declare function norm<T>(p: T): Normalize<T>;
function f(_p?: string | Params) {
  const params = norm(_p);
  if (params.case !== "sensitive") { /* ... */ }
}
export { f };
"#,
        strict_opts(),
    );
    assert!(
        !has_error(&diags, 2339),
        "no TS2339 — `string` must not satisfy `Record<any, any>`, so the conditional takes the narrowable branch. Actual: {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// #14255 — TS2315: export-rename specifier shadows a string-mapping intrinsic
// in-module (hkt-toolbelt).
// ---------------------------------------------------------------------------

#[test]
fn issue_14255_export_rename_keeps_intrinsic_no_ts2315() {
    let diags = compile_and_get_diagnostics_with_lib_and_options(
        r#"
export type Cap<S extends string> = Capitalize<S>
interface Local { tag: 'kind' }
export { Local as Capitalize }
"#,
        strict_opts(),
    );
    assert!(
        !has_error(&diags, 2315),
        "no TS2315 — the export-rename `Local as Capitalize` must not shadow the `Capitalize` intrinsic in-module. Actual: {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// #14232 — TS2322: conditional with concrete check vs generic extends defers
// instead of resolving the false branch (ts-essentials).
// ---------------------------------------------------------------------------

#[test]
fn issue_14232_concrete_check_generic_extends_false_branch_no_ts2322() {
    let diags = compile_and_get_diagnostics_with_lib_and_options(
        r#"
function f<T>() {
  type A = [] extends [T, ...T[]] ? "yes" : "no";
  const a: A = "no";
  return a;
}
export { f };
"#,
        strict_opts(),
    );
    assert!(
        !has_error(&diags, 2322),
        "no TS2322 — concrete check vs generic extends must resolve the false branch (`A = \"no\"`). Actual: {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// #14231 — TS2677: type-predicate-to-parameter relation not resolved through a
// type alias in the function-type-node path (ts-essentials).
// ---------------------------------------------------------------------------

#[test]
#[ignore = "reproduces #14231"]
fn issue_14231_type_predicate_through_alias_no_ts2677() {
    let diags = compile_and_get_diagnostics_with_lib_and_options(
        r#"
type Alias<T> = keyof T;
let g: <T>(p: Alias<T>) => p is keyof T;
type A = string;
let g2: (p: A) => p is string;
export { g, g2 };
"#,
        strict_opts(),
    );
    assert!(
        !has_error(&diags, 2677),
        "no TS2677 — a type predicate written through a type alias must resolve before the relation. Actual: {diags:#?}"
    );
}
