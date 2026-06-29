//! TS2556 must not fire for a spread whose argument *value* is typed `any`.
//!
//! Structural rule: spreading a value whose type is exactly `any` (or the error
//! type) contributes an unknown number of `any` arguments. `any` is assignable
//! both to a rest parameter and to a tuple of whatever arity the callee needs,
//! so `tsc` accepts `f(...anyValue)` against *any* parameter list — no TS2556
//! ("A spread argument must either have a tuple type or be passed to a rest
//! parameter") and no arity diagnostic. This is distinct from an `any[]` (or any
//! other opaque, non-tuple array) spread, whose definite array-ness gives it an
//! indeterminate length that still overflows a non-rest parameter, where TS2556
//! is correct.
//!
//! Owner layer: the spread-argument collector
//! (`call_checker::candidate_collection`) short-circuits a scalar-`any`/`error`
//! spread before the array/iterable landing-position checks that emit TS2556.
//!
//! Witness: jotai `freezeAtom` (#14746). A callback contextually typed by a
//! cross-module generic `Setter` degrades to an `any`-typed rest parameter when
//! the `Function.prototype.call` instantiation crosses the module boundary;
//! spreading it into `set(...)` then wrongly tripped TS2556. The structural rule
//! (scalar-`any` spread) is exercised directly below with binder names varied so
//! nothing keys on `any`, the identifier `set`, or a file name.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_multi_file_with_libs, check_source_diagnostics, diagnostic_count, load_default_lib_files,
};
use tsz_common::diagnostics::Diagnostic;

const TS2556: u32 = 2556;
// Arity diagnostics that must also stay silent: a scalar-`any` spread satisfies
// any arity, so neither "expected N arguments" (TS2554) nor "expected at least
// N" (TS2555) may appear.
const TS2554: u32 = 2554;
const TS2555: u32 = 2555;

fn spread_codes(diags: &[Diagnostic]) -> Vec<u32> {
    diags
        .iter()
        .map(|d| d.code)
        .filter(|c| *c == TS2556 || *c == TS2554 || *c == TS2555)
        .collect()
}

// ---------------------------------------------------------------------------
// Positive: spreading a scalar `any` value is clean against any callee shape.
// ---------------------------------------------------------------------------

#[test]
fn scalar_any_spread_into_fixed_arity_function_is_clean() {
    // The callee has two required, non-rest parameters and *fewer* are visibly
    // provided; the `any` spread must satisfy both arity and TS2556.
    let src = r#"
        declare const payload: any;
        function consume(first: number, second: string): void {}
        consume(...payload);
    "#;
    let diags = check_source_diagnostics(src);
    assert!(
        spread_codes(&diags).is_empty(),
        "scalar `any` spread into a fixed-arity callee must be clean: {diags:?}"
    );
}

#[test]
fn scalar_any_spread_into_many_required_params_is_clean() {
    // Even when the callee declares many required parameters, an `any` spread is
    // treated as supplying an unknown count — no TS2554.
    let src = r#"
        declare const blob: any;
        function widen(a: number, b: number, c: number, d: number): void {}
        widen(...blob);
    "#;
    let diags = check_source_diagnostics(src);
    assert!(
        spread_codes(&diags).is_empty(),
        "scalar `any` spread into a many-required-param callee must be clean: {diags:?}"
    );
}

#[test]
fn scalar_any_spread_with_leading_fixed_arg_is_clean() {
    let src = r#"
        declare const rest: any;
        function lead(head: number, tail: string): void {}
        lead(1, ...rest);
    "#;
    let diags = check_source_diagnostics(src);
    assert!(
        spread_codes(&diags).is_empty(),
        "leading fixed arg then scalar `any` spread must be clean: {diags:?}"
    );
}

#[test]
fn scalar_any_spread_into_constructor_is_clean() {
    let src = r#"
        declare const args: any;
        class Widget {
            constructor(x: number, y: string) {}
        }
        new Widget(...args);
    "#;
    let diags = check_source_diagnostics(src);
    assert!(
        spread_codes(&diags).is_empty(),
        "scalar `any` spread into a constructor must be clean: {diags:?}"
    );
}

#[test]
fn scalar_any_spread_into_method_is_clean() {
    let src = r#"
        declare const data: any;
        declare const sink: { absorb(only: number): void };
        sink.absorb(...data);
    "#;
    let diags = check_source_diagnostics(src);
    assert!(
        spread_codes(&diags).is_empty(),
        "scalar `any` spread into a method call must be clean: {diags:?}"
    );
}

#[test]
fn scalar_any_spread_into_overload_set_without_rest_is_clean() {
    // No overload has a rest parameter; the spread still must not trip TS2556,
    // because the `any` value can satisfy either fixed-arity overload.
    let src = r#"
        declare const params: any;
        function pick(a: number): void;
        function pick(a: number, b: number): void;
        function pick(a: number, b?: number): void {}
        pick(...params);
    "#;
    let diags = check_source_diagnostics(src);
    assert!(
        spread_codes(&diags).is_empty(),
        "scalar `any` spread into a no-rest overload set must be clean: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative controls: the exemption is specific to a *scalar* `any`/`error`
// value. An opaque (non-tuple) array spread into a non-rest parameter still has
// an indeterminate length and must keep TS2556 — including an `any[]` spread, so
// the rule is not "anything involving `any` is exempt".
// ---------------------------------------------------------------------------

#[test]
fn any_array_spread_into_non_rest_still_emits_ts2556() {
    let src = r#"
        declare const items: any[];
        function take(first: number, second: string): void {}
        take(...items);
    "#;
    let diags = check_source_diagnostics(src);
    assert_eq!(
        diagnostic_count(&diags, TS2556),
        1,
        "opaque `any[]` spread into a non-rest callee must still emit TS2556: {diags:?}"
    );
}

#[test]
fn typed_array_spread_into_non_rest_still_emits_ts2556() {
    // The same with a concretely typed array (binder names varied) so the
    // negative control is not keyed on `any`.
    let src = r#"
        declare const values: string[];
        function gather(alpha: number): void {}
        gather(...values);
    "#;
    let diags = check_source_diagnostics(src);
    assert_eq!(
        diagnostic_count(&diags, TS2556),
        1,
        "opaque `string[]` spread into a non-rest callee must still emit TS2556: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Cross-module witness (#14746): the jotai `freezeAtom` shape, where the
// callback contextually typed by an imported generic `Setter` degrades its rest
// parameter to `any` across the module boundary. The whole project must be clean
// exactly as `tsc` reports.
// ---------------------------------------------------------------------------

fn check_project(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
    let libs = load_default_lib_files();
    check_multi_file_with_libs(files, entry, CheckerOptions::default(), &libs)
}

#[test]
fn cross_module_generic_rest_setter_spread_is_clean() {
    let atom = r#"
        type Getter = <Value>(atom: Atom<Value>) => Value
        export type Setter = <Value, Args extends unknown[], Result>(
            atom: WritableAtom<Value, Args, Result>, ...args: Args) => Result
        type Write<Args extends unknown[], Result> =
            (get: Getter, set: Setter, ...args: Args) => Result
        export interface Atom<Value> { read: (get: Getter) => Value }
        export interface WritableAtom<Value, Args extends unknown[], Result>
            extends Atom<Value> {
            write: Write<Args, Result>
        }
    "#;
    let freeze = r#"
        import type { WritableAtom } from './atom.ts'
        declare const deepFreeze: <T>(v: T) => T
        export function freezeAtom<Value, Args extends unknown[], Result>(
            anAtom: WritableAtom<Value, Args, Result>,
        ) {
            const origWrite = anAtom.write
            anAtom.write = function (get, set, ...args) {
                return origWrite.call(this, get,
                    (...setArgs) => {
                        if (setArgs[0] === anAtom) { setArgs[1] = deepFreeze(setArgs[1]) }
                        return set(...setArgs)
                    }, ...args)
            }
        }
    "#;
    let diags = check_project(&[("atom.ts", atom), ("freeze.ts", freeze)], "freeze.ts");
    assert_eq!(
        diagnostic_count(&diags, TS2556),
        0,
        "cross-module generic-rest `Setter` spread (jotai #14746) must be clean: {diags:?}"
    );
}
