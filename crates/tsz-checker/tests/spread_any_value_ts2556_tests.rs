//! TS2556 for spreads whose argument *value* is typed `any`.
//!
//! Structural rule (tsc 7.0.2 and 6.0.3 agree): the spread-position arity rule
//! is type-independent. A spread whose type is not a tuple — a scalar `any`
//! included — is legal only where the parameter list accepts a variable number
//! of arguments: at or after `minArgumentCount`, flowing into a rest parameter
//! or trailing optionals. `f(...anyValue)` against required, non-rest
//! parameters gets exactly one TS2556 ("A spread argument must either have a
//! tuple type or be passed to a rest parameter") and no TS2554/TS2555 arity
//! diagnostic; `f(...anyValue)` where every remaining parameter is optional or
//! rest is clean.
//!
//! Owner layer: the shared spread-position predicate
//! (`call_checker::non_tuple_spread_signature`) classifies scalar `any`/error
//! spreads as non-tuple spreads; the argument collector still contributes a
//! single `any` argument so no separate arity error stacks on top.
//!
//! History: #15067 exempted scalar-`any` spreads from TS2556 entirely on the
//! premise that tsc accepts them against any parameter list. Both pinned
//! oracles disprove that premise; the real jotai #14746 false positive was
//! fixed by contextual rest-tuple recovery (#15045), which the cross-module
//! witness below still covers.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_multi_file_with_libs, check_source_diagnostics, diagnostic_count, load_default_lib_files,
};
use tsz_common::diagnostics::Diagnostic;

const TS2556: u32 = 2556;
// A scalar-`any` spread must produce TS2556 *alone* at illegal positions: the
// spread still relaxes the visible-argument count, so no "expected N
// arguments" (TS2554) / "expected at least N" (TS2555) may stack on top.
const TS2554: u32 = 2554;
const TS2555: u32 = 2555;

fn spread_codes(diags: &[Diagnostic]) -> Vec<u32> {
    diags
        .iter()
        .map(|d| d.code)
        .filter(|c| *c == TS2556 || *c == TS2554 || *c == TS2555)
        .collect()
}

fn assert_single_ts2556(diags: &[Diagnostic], what: &str) {
    assert_eq!(
        spread_codes(diags),
        vec![TS2556],
        "{what} must emit exactly one TS2556 and no arity diagnostic: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Scalar `any` spread over required, non-rest parameters: TS2556, nothing else.
// ---------------------------------------------------------------------------

#[test]
fn scalar_any_spread_into_fixed_arity_function_emits_ts2556() {
    let src = r#"
        declare const payload: any;
        function consume(first: number, second: string): void {}
        consume(...payload);
    "#;
    let diags = check_source_diagnostics(src);
    assert_single_ts2556(&diags, "scalar `any` spread into a fixed-arity callee");
}

#[test]
fn scalar_any_spread_into_many_required_params_emits_ts2556() {
    let src = r#"
        declare const blob: any;
        function widen(a: number, b: number, c: number, d: number): void {}
        widen(...blob);
    "#;
    let diags = check_source_diagnostics(src);
    assert_single_ts2556(
        &diags,
        "scalar `any` spread into a many-required-param callee",
    );
}

#[test]
fn scalar_any_spread_with_leading_fixed_arg_emits_ts2556() {
    // The spread sits at index 1 while `minArgumentCount` is 2, so the
    // position is still fixed — TS2556 fires despite the leading argument.
    let src = r#"
        declare const rest: any;
        function lead(head: number, tail: string): void {}
        lead(1, ...rest);
    "#;
    let diags = check_source_diagnostics(src);
    assert_single_ts2556(&diags, "leading fixed arg then scalar `any` spread");
}

#[test]
fn scalar_any_spread_into_constructor_emits_ts2556() {
    let src = r#"
        declare const args: any;
        class Widget {
            constructor(x: number, y: string) {}
        }
        new Widget(...args);
    "#;
    let diags = check_source_diagnostics(src);
    assert_single_ts2556(&diags, "scalar `any` spread into a constructor");
}

#[test]
fn scalar_any_spread_into_method_emits_ts2556() {
    let src = r#"
        declare const data: any;
        declare const sink: { absorb(only: number): void };
        sink.absorb(...data);
    "#;
    let diags = check_source_diagnostics(src);
    assert_single_ts2556(&diags, "scalar `any` spread into a method call");
}

#[test]
fn scalar_any_spread_into_overload_set_without_rest_emits_ts2556() {
    // Every overload requires a first argument, so index 0 is a fixed
    // position in each; tsc reports the plain TS2556, not an overload error.
    let src = r#"
        declare const params: any;
        function pick(a: number): void;
        function pick(a: number, b: number): void;
        function pick(a: number, b?: number): void {}
        pick(...params);
    "#;
    let diags = check_source_diagnostics(src);
    assert_single_ts2556(&diags, "scalar `any` spread into a no-rest overload set");
}

// ---------------------------------------------------------------------------
// Legal positions: rest parameters and all-optional tails accept a non-tuple
// spread, so a scalar `any` spread is clean there.
// ---------------------------------------------------------------------------

#[test]
fn scalar_any_spread_into_rest_param_is_clean() {
    let src = r#"
        declare const feed: any;
        function collect(...entries: number[]): void {}
        collect(...feed);
    "#;
    let diags = check_source_diagnostics(src);
    assert!(
        spread_codes(&diags).is_empty(),
        "scalar `any` spread into a rest parameter must be clean: {diags:?}"
    );
}

#[test]
fn scalar_any_spread_after_required_args_into_rest_is_clean() {
    let src = r#"
        declare const tail: any;
        function fixedThenRest(head: number, ...extra: string[]): void {}
        fixedThenRest(1, ...tail);
    "#;
    let diags = check_source_diagnostics(src);
    assert!(
        spread_codes(&diags).is_empty(),
        "scalar `any` spread past the required prefix into a rest parameter must be clean: {diags:?}"
    );
}

#[test]
fn scalar_any_spread_into_all_optional_params_is_clean() {
    let src = r#"
        declare const maybe: any;
        function optionalOnly(a?: number, b?: string): void {}
        optionalOnly(...maybe);
    "#;
    let diags = check_source_diagnostics(src);
    assert!(
        spread_codes(&diags).is_empty(),
        "scalar `any` spread into an all-optional parameter list must be clean: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Array spreads at fixed positions keep TS2556 too (the rule is not `any`
// specific in either direction).
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
// Cross-module witness (#14746): the jotai `freezeAtom` shape. The callback's
// rest parameter is recovered as a contextual rest tuple (#15045), so the
// spread has a tuple type and the project stays clean — without the scalar-any
// exemption.
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
