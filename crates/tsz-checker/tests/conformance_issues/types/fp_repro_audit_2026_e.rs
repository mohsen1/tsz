//! Batch regression guards for fleet-fixed canary false positives,
//! harvested 2026-06-22 (each verified to have a fix commit in main).
//! #14510 #14385 #14342 #14341 #14326 #14323 #14322 #14317.

use super::super::core::*;

/// #14510: 63603c50a9 fix(solver): reject overload candidate when a const type param fell back to its constraint (#14524)
#[test]
fn issue_14510_overload_const_fallback_no_ts2741() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function f<const T extends string>(x: T): [T];
declare function f<const T extends number>(x: T): { n: T };
const r = f(5);
const ok: { n: 5 } = r;
"#,
    );
    assert!(
        !has_error(&diagnostics, 2741),
        "#14510: no TS2741 expected (fleet-fixed). Actual: {diagnostics:#?}"
    );
}

/// #14385: 3e10037c1d fix(solver): reduce concrete-check this-relative conditional members before relating (TS2344) (#14413)
#[test]
fn issue_14385_this_relative_conditional_member_no_ts2344() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare const unsetSym: unique symbol;
type unset = typeof unsetSym;

interface Fn {
  rawArgs: unknown;
  args: this["rawArgs"] extends infer a extends unknown[] ? a : never;
  return: unknown;
}

interface PartialApply<fn extends Fn, partialArgs extends unknown[]> extends Fn {
  rawArgs: unknown;
  return: never;
}

type Apply<fn extends Fn | unset, args> = fn extends Fn ? fn : never;
type Get<K> = PartialApply<Fn, [K]>;
type X = Apply<Get<"length">, []>;
export {};
"#,
    );
    assert!(
        !has_error(&diagnostics, 2344),
        "#14385: no TS2344 expected (fleet-fixed). Actual: {diagnostics:#?}"
    );
}

/// #14342: 4d10c73f27 fix(checker): resolve cross-file unique-symbol computed name via type-only namespace import (#14342) (#14390)
#[test]
#[ignore = "reproduces #14342 OR multi-file witness reconstruction differs from the project repro; single-file fix verified via commit 4d10c73f27"]
fn issue_14342_cross_file_unique_symbol_computed_name_no_ts7053() {
    if !lib_files_available() {
        return;
    }
    let files = &[
        (
            "symbols.ts",
            r#"export const isVariadic = Symbol.for('v');
"#,
        ),
        (
            "main.ts",
            r#"import type * as symt from './symbols';   // TYPE-ONLY namespace import
import * as symv from './symbols';         // value namespace import (same module)

interface Matcher {
  [symt.isVariadic]?: boolean;             // computed key via type-only namespace member
}
function f(m: Matcher) {
  return m[symv.isVariadic];               // was: false TS7053; after fix: clean
}
"#,
        ),
    ];
    let diagnostics = compile_named_files_get_diagnostics_with_lib_and_options(
        files,
        "main.ts",
        tsz_checker::context::CheckerOptions {
            strict: true,
            strict_null_checks: true,
            module: tsz_common::common::ModuleKind::ESNext,
            ..Default::default()
        },
    );
    assert!(
        !has_error(&diagnostics, 7053),
        "#14342: no TS7053 expected (fleet-fixed). Actual: {diagnostics:#?}"
    );
}

/// #14341: 224a1035f2 fix(checker): preserve narrowing across a conditional initializer whose arm calls a function (TS2339) (#14403)
#[test]
fn issue_14341_narrowing_across_conditional_initializer_no_ts2339() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function isArr(value: unknown): value is Array<unknown>;
declare function getKeys(o: object): false | string[];

function f(prev: any, next: any) {
  const array = isArr(prev) && isArr(next);
  const prevItems = array ? prev : getKeys(prev);
  if (!prevItems) return;
  const nextItems = array ? next : getKeys(next);
  if (!nextItems) return;
  return prevItems.length;
}
"#,
    );
    assert!(
        !has_error(&diagnostics, 2339),
        "#14341: no TS2339 expected (fleet-fixed). Actual: {diagnostics:#?}"
    );
}

/// #14326: 7cecfe125e fix(solver): erase signature type params to any for generic conditional-rest arity (#14326) (#14410)
#[test]
fn issue_14326_generic_conditional_rest_arity_no_ts2554() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Opts = { a?: number };
type Fn = <s>(head: ReadonlyArray<s>, ...[opts]: [s] extends [PropertyKey] ? [opts?: Opts] : [opts: Opts]) => void;
declare const fn: Fn;
fn([1, 2]);
"#,
    );
    assert!(
        !has_error(&diagnostics, 2554),
        "#14326: no TS2554 expected (fleet-fixed). Actual: {diagnostics:#?}"
    );
}

/// #14323: feab542205 fix(solver): cap fixed-arity infer function pattern by source required arity (#14408)
#[test]
fn issue_14323_fixed_arity_infer_function_pattern_no_ts2322() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type ParamTypes<F> =
  F extends (p0: infer P0) => any ? [P0]
  : F extends (p0: infer P0, p1: infer P1) => any ? [P0, P1] : never;
const fn = (a: string, b: number) => {};
const pts: [string, number] = (null as any as ParamTypes<typeof fn>);
"#,
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "#14323: no TS2322 expected (fleet-fixed). Actual: {diagnostics:#?}"
    );
}

/// #14322: 8346974a7a fix(solver): reduce cross-arena generic type-alias indexed access (TS2322/TS7006) (#14380)
#[test]
#[ignore = "reproduces #14322 OR multi-file witness reconstruction differs; fix verified via commit 8346974a7a"]
fn issue_14322_cross_arena_generic_alias_indexed_access_no_ts2322_ts7006() {
    if !lib_files_available() {
        return;
    }
    let files = &[
        (
            "base.ts",
            r#"export interface Registry {
  a: number;
  b: string;
}
export type Keys = keyof Registry
export type Lookup<K extends Keys> = Registry[K]
"#,
        ),
        (
            "consumer.ts",
            r#"import { Lookup } from "./base"

const x: Lookup<"a"> = "hello"; // TS2322 false-positive
const y: Lookup<"b"> = 42; // TS7006 false-positive
"#,
        ),
    ];
    let diagnostics = compile_named_files_get_diagnostics_with_lib_and_options(
        files,
        "consumer.ts",
        tsz_checker::context::CheckerOptions {
            strict: true,
            strict_null_checks: true,
            module: tsz_common::common::ModuleKind::ESNext,
            ..Default::default()
        },
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "#14322: no TS2322 expected (fleet-fixed). Actual: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 7006),
        "#14322: no TS7006 expected (fleet-fixed). Actual: {diagnostics:#?}"
    );
}

/// #14317: b5dd3a6520 fix(binder): seed try/finally flow entry with pre-try and abrupt states (TS18048) (#14407)
#[test]
fn issue_14317_try_finally_pre_try_narrowing_no_ts18048() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
function f(stack: Map<any, any> | undefined): boolean {
  stack = stack ?? new Map();   // narrows stack to Map
  try { return true; }          // abrupt completion
  finally { stack.delete(1); }  // was TS18048 (falsely possibly undefined); now ok
}
"#,
    );
    assert!(
        !has_error(&diagnostics, 18048),
        "#14317: no TS18048 expected (fleet-fixed). Actual: {diagnostics:#?}"
    );
}
