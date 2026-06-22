//! Round-7 false-positive reproduction audit (2026-06). Each test below is the
//! minimal witness extracted from a mined canary issue. A passing (non-ignored)
//! test means the false positive is FIXED on main and the test stands as a
//! regression guard. An `#[ignore = "reproduces #N"]` test still reproduces the
//! FP and preserves the witness for the eventual fix.
//!
//! Helpers mirror `spread_param_constraint_2345.rs`: single-file witnesses use
//! `compile_*_with_lib_and_options` (every repro here references lib intrinsics
//! such as `keyof`/`Record`/`PropertyKey`/`Array`).

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
// #14460 — a homomorphic mapped type `{ [K in keyof T]: ... }` whose source
// type variable `T` instantiates to `never` must reduce to `never` (tsc), so
// assigning `H = Map2<never>` to `never` is clean. tsz materialized a spurious
// object shape and reported a false TS2322.
// ---------------------------------------------------------------------------

#[test]
fn issue_14460_homomorphic_mapped_over_never_no_ts2322() {
    if !lib_files_available() {
        return;
    }
    let diags = compile_and_get_diagnostics_with_lib_and_options(
        r#"
type Map2<T> = { [K in keyof T]: T[K] };
type H = Map2<never>;
const h: never = (null as any as H);
"#,
        strict_opts(),
    );
    assert!(
        !has_error(&diags, 2322),
        "no TS2322 expected — a homomorphic mapped type over a `never` source \
         reduces to `never`, so `H` is assignable to `never`. Actual: {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// #14425 — calling a generic whose parameter is `Record<K, T>` (a non-
// homomorphic mapped type) with a keyless object source (intrinsic `object`)
// must infer `K = never`, `T = never`; `Record<never, never>` is satisfied by
// `object`, so the call is accepted. tsz fell back to the type parameters'
// constraints and emitted a false TS2345.
// ---------------------------------------------------------------------------

#[test]
fn issue_14425_record_inferred_from_keyless_object_no_ts2345() {
    if !lib_files_available() {
        return;
    }
    let diags = compile_and_get_diagnostics_with_lib_and_options(
        r#"
declare const everyEntry: <K extends PropertyKey, V>(
  table: Record<K, V>,
  predicate: (key: K, value: V) => boolean
) => boolean;

function run(value: object): void {
  everyEntry(value, (k, v) => true);
}
"#,
        strict_opts(),
    );
    assert!(
        !has_error(&diags, 2345),
        "no TS2345 expected — a `Record<K, V>` param inferred from a keyless \
         `object` source collapses to `Record<never, never>`, which `object` \
         satisfies. Actual: {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// #14319 — an overloaded call with a fixed-arity overload before a rest-
// parameter overload, called with a non-tuple array spread, must resolve to
// the rest overload (tsc). tsz committed TS2556 against the first overload and
// never fell back.
// ---------------------------------------------------------------------------

#[test]
fn issue_14319_array_spread_selects_rest_overload_no_ts2556() {
    if !lib_files_available() {
        return;
    }
    let diags = compile_and_get_diagnostics_with_lib_and_options(
        r#"
declare function flow(f: () => any): any;
declare function flow(...funcs: Array<() => any>): any;
function f(arr: Array<() => any>): any { return flow(...arr); }
"#,
        strict_opts(),
    );
    assert!(
        !has_error(&diags, 2556),
        "no TS2556 expected — a non-tuple array spread must fall back to the \
         rest-parameter overload declared after the fixed-arity one. \
         Actual: {diags:#?}"
    );
}
