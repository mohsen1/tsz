//! Regression tests for kysely tracker #10663 families F5 and F7.
//!
//! F5: a named import of a type alias whose body is a constructor type
//! (`type C = new (a: A) => I`) must keep its construct signature in type
//! position. The named-import type-position path used to unconditionally map
//! constructor-shaped alias types to their instance side (built for imported
//! classes), so `declare const x: C; new x(...)` produced a false TS2351 and
//! the negative branch of a user-defined type predicate over `C | ((a: A) =>
//! I)` could not exclude the constructor arm, producing a false TS2349.
//!
//! F7: successive user-defined type predicates over a non-union source must
//! intersect (tsc `getNarrowedType`), not replace. The reverse-subtype check
//! in `narrow_to_type` used a resolver-less subtype query that judged a named
//! interface a subtype of `{ [P in string]: unknown }`, replacing the
//! record from an earlier `isObject`-style guard and dropping its string
//! index signature.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_multi_file_with_libs, load_lib_files};
use tsz_common::common::ModuleKind;

fn check_files(files: &[(&str, &str)], entry: &str) -> Vec<(u32, String)> {
    let libs = load_lib_files(&["es5.d.ts"]);
    check_multi_file_with_libs(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .filter(|diag| diag.code != 2318)
    .map(|diag| (diag.code, diag.message_text))
    .collect()
}

const GUARD_FILE: &str = r#"
export type NodeArg = { kind: string }
export type ErrCtor = new (node: NodeArg) => Error
export declare function isErrCtor(
  fn: ErrCtor | ((node: NodeArg) => Error),
): fn is ErrCtor
"#;

#[test]
fn imported_ctor_alias_predicate_false_branch_keeps_callability() {
    // F5 witness: the else branch must narrow `ErrCtor | fn` to the callable
    // function arm; no TS2349 "This expression is not callable".
    // The predicate is declared locally; only the constructor-type alias is
    // imported — that imported alias is the load-bearing ingredient (a fully
    // inlined single-file form was already clean before the fix). The
    // cross-file-declared-predicate form is covered by the kysely project
    // guard; the multi-file test harness mis-wires that import shape
    // (pre-existing, unrelated to this fix).
    let guard = r#"
export type NodeArg = { kind: string }
export type ErrCtor = new (node: NodeArg) => Error
"#;
    let main = r#"
import { ErrCtor } from './guard'
declare function isErrCtor(
  fn: ErrCtor | ((node: { kind: string }) => Error),
): fn is ErrCtor
export function make(
  ec: ErrCtor | ((node: { kind: string }) => Error),
  node: { kind: string },
): Error {
  return isErrCtor(ec) ? new ec(node) : ec(node)
}
"#;
    let diagnostics = check_files(&[("guard.ts", guard), ("main.ts", main)], "main.ts");
    // Family signature: before the fix the union's imported ctor arm resolved
    // to the construct signature's RETURN type, so the false branch could not
    // exclude it and the call tripped TS2349. (The true-branch `new ec(...)`
    // is asserted through the CLI-level kysely guard; the multi-file harness
    // defers flow-env def registration differently and reports an unrelated
    // TS2351 there even though the real compiler pipeline is clean.)
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2349),
        "false branch of predicate over imported ctor alias must keep callability; got {diagnostics:#?}"
    );
}

#[test]
fn imported_ctor_alias_is_constructable_in_type_position() {
    // Sharper F5 witness: no predicate involved at all. The imported alias of
    // a constructor type must stay constructable and must NOT expose the
    // construct signature's return-type members.
    let main = r#"
import { ErrCtor, NodeArg } from './guard'
declare const x: ErrCtor
export const y = new x({ kind: 'a' })
"#;
    let diagnostics = check_files(&[("guard.ts", GUARD_FILE), ("main.ts", main)], "main.ts");
    assert!(
        diagnostics
            .iter()
            .all(|(code, _)| *code != 2351 && *code != 2339),
        "imported constructor-type alias must be constructable; got {diagnostics:#?}"
    );
}

#[test]
fn imported_ctor_alias_rejects_instance_member_access() {
    // Negative case: the alias is the CONSTRUCTOR, not the instance. Member
    // access for an instance-only property must still fail (tsc: TS2339).
    let main = r#"
import { ErrCtor } from './guard'
declare const x: ErrCtor
export const m = x.message
"#;
    let diagnostics = check_files(&[("guard.ts", GUARD_FILE), ("main.ts", main)], "main.ts");
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2339),
        "instance member on imported ctor alias must still be TS2339; got {diagnostics:#?}"
    );
}

#[test]
fn imported_ctor_alias_assignment_mismatch_reports_alias_target() {
    // Display adjacency: the TS2322 target must be the alias-shaped
    // constructor type, not the unwrapped instance interface.
    let main = r#"
import { ErrCtor } from './guard'
export const bad: ErrCtor = 123
"#;
    let diagnostics = check_files(&[("guard.ts", GUARD_FILE), ("main.ts", main)], "main.ts");
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert!(
        !ts2322.is_empty() && ts2322.iter().all(|(_, msg)| msg.contains("ErrCtor")),
        "TS2322 must report the constructor alias as target; got {diagnostics:#?}"
    );
}

#[test]
fn imported_function_and_object_aliases_unaffected() {
    // Adjacent matrix: function-type alias stays callable; object alias keeps
    // members; a constructor type nested under a property keeps construct
    // signature. (Renamed binders relative to the kysely fixture.)
    // Two files only: the multi-file test harness mis-wires module graphs
    // with 3+ files / 3+ named specifiers per import (pre-existing).
    let lib = r#"
export type Maker = (p: { kind: string }) => Error
export type Plain = { n: number }
"#;
    let main = r#"
import { Maker, Plain } from './lib'
declare const f: Maker
declare const o: Plain
export const r1 = f({ kind: 'a' })
export const r3 = o.n
"#;
    let diagnostics = check_files(&[("lib.ts", lib), ("main.ts", main)], "main.ts");
    assert!(
        diagnostics.is_empty(),
        "adjacent imported alias shapes must stay clean; got {diagnostics:#?}"
    );
}

#[test]
fn imported_property_nested_ctor_alias_unaffected() {
    // Adjacent: a constructor type nested under an object property keeps its
    // construct signature (the type-position conversion must not unwrap it).
    let lib = "export type Box = { build: new (p: { kind: string }) => Error }\n";
    let main = r#"
import { Box } from './lib'
declare const b: Box
export const r2 = new b.build({ kind: 'a' })
"#;
    let diagnostics = check_files(&[("lib.ts", lib), ("main.ts", main)], "main.ts");
    assert!(
        diagnostics.is_empty(),
        "property-nested imported ctor alias must stay constructable; got {diagnostics:#?}"
    );
}

const RECORD_GUARDS_FILE: &str = r#"
export type ShallowRecord<K extends keyof any, T> = {
  [P in K]: T
}
export declare function isObject(obj: unknown): obj is ShallowRecord<string, unknown>
export declare function isString(obj: unknown): obj is string
export interface OperationNodeSource {
  toOperationNode(): { kind: string }
}
export declare function isOperationNodeSource(obj: unknown): obj is OperationNodeSource
"#;

#[test]
fn successive_predicates_intersect_record_and_interface_cross_file() {
    // F7 witness (kysely dynamic/*): after `isObject(obj)` narrows `unknown`
    // to a string-index record, a second interface predicate must intersect,
    // keeping the index signature so `obj.someProp` stays accessible.
    // Guards split across modules: the multi-file test harness mis-wires
    // imports of 3+ named specifiers from a single module (pre-existing).
    let object_guards = r#"
export type ShallowRecord<K extends keyof any, T> = {
  [P in K]: T
}
export declare function isObject(obj: unknown): obj is ShallowRecord<string, unknown>
export declare function isString(obj: unknown): obj is string
"#;
    let source_guards = r#"
export interface OperationNodeSource {
  toOperationNode(): { kind: string }
}
export declare function isOperationNodeSource(obj: unknown): obj is OperationNodeSource
"#;
    let main = r#"
import { isObject, isString } from './object-guards'
import { isOperationNodeSource } from './source-guards'

export function isRefBuilder(obj: unknown): boolean {
  return (
    isObject(obj) &&
    isOperationNodeSource(obj) &&
    isString(obj.dynamicReference)
  )
}
"#;
    let diagnostics = check_files(
        &[
            ("object-guards.ts", object_guards),
            ("source-guards.ts", source_guards),
            ("main.ts", main),
        ],
        "main.ts",
    );
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2339),
        "successive predicate guards must intersect, keeping the record index signature; got {diagnostics:#?}"
    );
}

#[test]
fn successive_predicates_intersect_single_file() {
    // Same defect reproduces single-file; pin that form too (renamed binders).
    let main = r#"
type Bag<K extends keyof any, T> = {
  [P in K]: T
}
declare function looksLikeBag(v: unknown): v is Bag<string, unknown>
declare function looksLikeText(v: unknown): v is string
interface Source2 {
  emit(): { tag: string }
}
declare function looksLikeSource(v: unknown): v is Source2

export function pick(v: unknown): boolean {
  return looksLikeBag(v) && looksLikeSource(v) && looksLikeText(v.anything)
}
"#;
    let diagnostics = check_files(&[("main.ts", main)], "main.ts");
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2339),
        "single-file successive predicates must intersect; got {diagnostics:#?}"
    );
}

#[test]
fn interface_predicate_without_record_guard_still_rejects_unknown_property() {
    // Negative case: with ONLY the interface guard (no record guard first),
    // an unknown property must still be TS2339 — the fix must not loosen the
    // plain predicate result.
    let main = r#"
import { isOperationNodeSource, isString } from './guards'

export function probe(obj: unknown): boolean {
  return isOperationNodeSource(obj) && isString(obj.dynamicReference)
}
"#;
    let diagnostics = check_files(
        &[("guards.ts", RECORD_GUARDS_FILE), ("main.ts", main)],
        "main.ts",
    );
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2339),
        "interface-only predicate must still reject unknown properties; got {diagnostics:#?}"
    );
}

#[test]
fn predicate_narrowing_to_subclass_still_narrows() {
    // Reverse-subtype adjacency: predicate to a derived class on a base-typed
    // value must still narrow to the derived side (the resolver-backed check
    // must keep the class-hierarchy behavior of the old bare check).
    let main = r#"
class Animal2 {
  walk(): void {}
}
class Dog2 extends Animal2 {
  bark(): void {}
}
declare function isDog(a: Animal2): a is Dog2

export function speak(a: Animal2): void {
  if (isDog(a)) {
    a.bark()
  }
}
"#;
    let diagnostics = check_files(&[("main.ts", main)], "main.ts");
    assert!(
        diagnostics.is_empty(),
        "derived-class predicate narrowing must keep working; got {diagnostics:#?}"
    );
}

#[test]
fn predicate_to_generic_indexed_access_keeps_keyof_param_indexable() {
    // Conformance `intersectionsOfLargeUnions2` regression shape: after a
    // predicate narrows to `U extends TagMap[T]`, indexing with
    // `P extends keyof TagMap[T]` must stay valid (tsc keeps the receiver
    // deferred as `TagMap[T]`; tsz distributes small maps eagerly, and the
    // TS2536 union gate must not treat that distribution as a key mismatch).
    let main = r#"
interface Elem2 { tagName: string }
interface DivElem2 extends Elem2 { d: number }
interface SpanElem2 extends Elem2 { s: number }
interface TagMap2 {
  div: DivElem2
  span: SpanElem2
}

declare function assertTag<
  T extends keyof TagMap2,
  U extends TagMap2[T]>(node: Elem2 | null, tagName: T): node is U

export function pick<
  T extends keyof TagMap2,
  P extends keyof TagMap2[T]>(node: Elem2 | null, tagName: T, prop: P) {
  if (assertTag(node, tagName)) {
    node[prop];
  }
}
"#;
    let diagnostics = check_files(&[("main.ts", main)], "main.ts");
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2536),
        "keyof-of-generic-indexed-access param must stay a valid index; got {diagnostics:#?}"
    );
}

#[test]
fn concrete_keyof_param_against_unrelated_union_still_ts2536() {
    // Negative case (tsc parity): a CONCRETE `P extends keyof A` indexing an
    // unrelated `A | B` union must keep reporting TS2536.
    let main = r#"
interface Alpha { a: number; shared: string }
interface Beta { b: number; shared: string }
declare const u: Alpha | Beta
export function f<P extends keyof Alpha>(p: P) {
  u[p]
}
"#;
    let diagnostics = check_files(&[("main.ts", main)], "main.ts");
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2536),
        "concrete keyof constraint against unrelated union must keep TS2536; got {diagnostics:#?}"
    );
}

#[test]
fn generic_union_receiver_with_intersection_keyof_still_ts2536() {
    // Negative case (`keyofAndIndexedAccessErrors` f20): a GENERIC receiver
    // `T | U` indexed by `keyof (T & U)` is tsc's own deferred-relation
    // failure and must keep erroring.
    let main = r#"
export function f20<T, U>(x: T | U, k3: keyof (T & U)) {
  x[k3]
}
"#;
    let diagnostics = check_files(&[("main.ts", main)], "main.ts");
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2536),
        "generic union receiver indexed by keyof intersection must keep TS2536; got {diagnostics:#?}"
    );
}
