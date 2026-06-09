//! TS2556 for open-ended (variable-length) tuple spreads in call arguments.
//!
//! Structural rule: a tuple whose flattened rest is array-backed (e.g.
//! `[number, ...string[]]`, `[...number[], string]`, `[number, ...string[],
//! boolean]`) has an *indeterminate* length, exactly like a bare array spread.
//! When such a spread is used as a call argument, its variable portion must
//! land on a rest parameter; otherwise `tsc` reports
//!
//!   TS2556: A spread argument must either have a tuple type or be passed to a
//!           rest parameter.
//!
//! and only TS2556 (no cascading TS2554/TS2555 arity error). A fully
//! fixed-length tuple (including fully-fixed nested tuple rests) keeps its known
//! length and is spread positionally.
//!
//! Owner layer: the open-endedness predicate is a solver type query
//! (`tuple_variable_rest_offset`); the checker's call-argument collection routes
//! the spread-position decision through the shared
//! `allows_non_tuple_spread_position` boundary, the same gateway the array and
//! iterable spread paths use.

use tsz_checker::test_utils::{check_source_diagnostics, diagnostic_codes, diagnostic_count};

/// Build a tiny program that spreads `tuple_ty` into a call to a function with
/// `params`, using `f`/`t` binder names that the caller varies (anti-hardcoding:
/// the rule is structural, not keyed on any identifier).
fn spread_call_src(fn_name: &str, params: &str, var: &str, tuple_ty: &str, value: &str) -> String {
    format!(
        "declare function {fn_name}({params}): void;\n\
         const {var}: {tuple_ty} = {value};\n\
         {fn_name}(...{var});\n"
    )
}

// ---------------------------------------------------------------------------
// Positive: open-ended tuple spread into a fixed parameter list -> TS2556.
// ---------------------------------------------------------------------------

#[test]
fn trailing_rest_tuple_into_fixed_params_emits_ts2556() {
    let src = spread_call_src(
        "g",
        "a: number, b: string",
        "t",
        "[number, ...string[]]",
        "[1]",
    );
    let diags = check_source_diagnostics(&src);
    assert_eq!(
        diagnostic_count(&diags, 2556),
        1,
        "open-ended trailing-rest tuple spread into fixed params must emit TS2556: {:?}",
        diagnostic_codes(&diags)
    );
    // tsc emits *only* TS2556 here — no TS2554/TS2555 arity cascade.
    assert_eq!(
        diagnostic_count(&diags, 2554),
        0,
        "no TS2554 cascade: {:?}",
        diagnostic_codes(&diags)
    );
    assert_eq!(
        diagnostic_count(&diags, 2555),
        0,
        "no TS2555 cascade: {:?}",
        diagnostic_codes(&diags)
    );
}

#[test]
fn leading_rest_tuple_into_fixed_params_emits_ts2556() {
    let src = spread_call_src(
        "apply",
        "a: number, b: string",
        "args",
        "[...number[], string]",
        "[1] as any",
    );
    let diags = check_source_diagnostics(&src);
    assert_eq!(
        diagnostic_count(&diags, 2556),
        1,
        "open-ended leading-rest tuple spread into fixed params must emit TS2556: {:?}",
        diagnostic_codes(&diags)
    );
}

#[test]
fn middle_rest_tuple_into_fixed_params_emits_ts2556() {
    let src = spread_call_src(
        "invoke",
        "a: number, b: string, c: boolean",
        "row",
        "[number, ...string[], boolean]",
        "[1, true] as any",
    );
    let diags = check_source_diagnostics(&src);
    assert_eq!(
        diagnostic_count(&diags, 2556),
        1,
        "open-ended middle-rest tuple spread into fixed params must emit TS2556: {:?}",
        diagnostic_codes(&diags)
    );
    assert_eq!(
        diagnostic_count(&diags, 2554),
        0,
        "no TS2554 cascade: {:?}",
        diagnostic_codes(&diags)
    );
}

#[test]
fn nested_variable_tuple_rest_into_fixed_params_emits_ts2556() {
    // The outer rest is a *fixed* tuple shell, but it nests a variable rest, so
    // the whole tuple is still open-ended.
    let src = spread_call_src(
        "h",
        "a: number, b: string",
        "t",
        "[number, ...[string, ...boolean[]]]",
        "[1, \"x\"]",
    );
    let diags = check_source_diagnostics(&src);
    assert_eq!(
        diagnostic_count(&diags, 2556),
        1,
        "nested variable-rest tuple spread must emit TS2556: {:?}",
        diagnostic_codes(&diags)
    );
    // Must be TS2556, not the pre-fix TS2554 arity miscount.
    assert_eq!(
        diagnostic_count(&diags, 2554),
        0,
        "no TS2554: {:?}",
        diagnostic_codes(&diags)
    );
}

#[test]
fn readonly_open_ended_tuple_into_fixed_params_emits_ts2556() {
    let src = spread_call_src(
        "g",
        "a: number, b: string",
        "t",
        "readonly [number, ...string[]]",
        "[1] as any",
    );
    let diags = check_source_diagnostics(&src);
    assert_eq!(
        diagnostic_count(&diags, 2556),
        1,
        "readonly open-ended tuple spread must still emit TS2556: {:?}",
        diagnostic_codes(&diags)
    );
}

/// Anti-hardcoding: the rule must be structural, not keyed on identifiers. The
/// same open-ended shape with different binder/function names must still fire.
#[test]
fn ts2556_is_not_keyed_on_identifier_names() {
    for (fname, vname) in [("zip", "pair"), ("dispatch", "payload"), ("call", "rest")] {
        let src = spread_call_src(
            fname,
            "a: number, b: string",
            vname,
            "[number, ...string[]]",
            "[1]",
        );
        let diags = check_source_diagnostics(&src);
        assert_eq!(
            diagnostic_count(&diags, 2556),
            1,
            "TS2556 must fire regardless of names ({fname}/{vname}): {:?}",
            diagnostic_codes(&diags)
        );
    }
}

// ---------------------------------------------------------------------------
// Negative: spreads that are valid must NOT emit TS2556.
// ---------------------------------------------------------------------------

#[test]
fn fixed_tuple_into_fixed_params_no_ts2556() {
    let src = spread_call_src(
        "g",
        "a: number, b: string",
        "t",
        "[number, string]",
        "[1, \"x\"]",
    );
    let diags = check_source_diagnostics(&src);
    assert_eq!(
        diagnostic_count(&diags, 2556),
        0,
        "fixed-length tuple spread must not emit TS2556: {:?}",
        diagnostic_codes(&diags)
    );
}

#[test]
fn nested_fixed_tuple_rest_into_fixed_params_no_ts2556() {
    // `[number, ...[string, boolean]]` is fully fixed-length -> [number, string, boolean].
    let src = spread_call_src(
        "g",
        "a: number, b: string, c: boolean",
        "t",
        "[number, ...[string, boolean]]",
        "[1, \"x\", true]",
    );
    let diags = check_source_diagnostics(&src);
    assert_eq!(
        diagnostic_count(&diags, 2556),
        0,
        "fully-fixed nested tuple rest must not emit TS2556: {:?}",
        diagnostic_codes(&diags)
    );
}

#[test]
fn open_ended_tuple_into_rest_param_no_ts2556() {
    let src = spread_call_src(
        "g",
        "a: number, ...b: string[]",
        "t",
        "[number, ...string[]]",
        "[1]",
    );
    let diags = check_source_diagnostics(&src);
    assert_eq!(
        diagnostic_count(&diags, 2556),
        0,
        "open-ended tuple landing on a rest parameter must not emit TS2556: {:?}",
        diagnostic_codes(&diags)
    );
}

#[test]
fn open_ended_tuple_into_optional_trailing_no_ts2556() {
    // The variable portion only covers an optional trailing parameter -> allowed.
    let src = spread_call_src(
        "g",
        "a: number, b?: string",
        "t",
        "[number, ...string[]]",
        "[1]",
    );
    let diags = check_source_diagnostics(&src);
    assert_eq!(
        diagnostic_count(&diags, 2556),
        0,
        "open-ended tuple covering only optional trailing params must not emit TS2556: {:?}",
        diagnostic_codes(&diags)
    );
}

// ---------------------------------------------------------------------------
// Contextual `restTuplesFromContextualTypes` suppression: an inline function /
// arrow expression whose parameter at the variable-rest position is
// un-annotated gets that parameter contextually typed from the tuple's rest
// element, so tsc (and tsz) report no TS2556. The suppression hinges on the
// parameter AT the rest position being un-annotated, and is specific to tuple
// spreads (a bare array spread still errors).
// ---------------------------------------------------------------------------

#[test]
fn open_ended_tuple_into_iife_unannotated_param_no_ts2556() {
    // `[number, boolean, ...string[]]` rest lands at index 2; param `c` is
    // un-annotated, so it is contextually typed and absorbs the rest.
    let src = "const t: [number, boolean, ...string[]] = [1, true];\n\
               (function (a, b, c) {})(...t);\n";
    let diags = check_source_diagnostics(src);
    assert_eq!(
        diagnostic_count(&diags, 2556),
        0,
        "inline function with un-annotated rest-position param must not emit TS2556: {:?}",
        diagnostic_codes(&diags)
    );
}

#[test]
fn open_ended_tuple_into_iife_arrow_unannotated_param_no_ts2556() {
    let src = "const t: [number, ...string[]] = [1];\n\
               ((a, b) => {})(...t);\n";
    let diags = check_source_diagnostics(src);
    assert_eq!(
        diagnostic_count(&diags, 2556),
        0,
        "inline arrow with un-annotated rest-position param must not emit TS2556: {:?}",
        diagnostic_codes(&diags)
    );
}

#[test]
fn open_ended_tuple_into_iife_annotated_rest_position_emits_ts2556() {
    // Param `c` at the rest position is annotated -> fixed type -> TS2556 fires,
    // exactly as for a declared function.
    let src = "const t: [number, boolean, ...string[]] = [1, true];\n\
               (function (a, b, c: string) {})(...t);\n";
    let diags = check_source_diagnostics(src);
    assert_eq!(
        diagnostic_count(&diags, 2556),
        1,
        "inline function with annotated rest-position param must emit TS2556: {:?}",
        diagnostic_codes(&diags)
    );
}

#[test]
fn open_ended_tuple_into_iife_too_few_params_emits_ts2556() {
    // Only two params; the rest at index 2 has no parameter to land on -> TS2556.
    let src = "const t: [number, boolean, ...string[]] = [1, true];\n\
               (function (a, b) {})(...t);\n";
    let diags = check_source_diagnostics(src);
    assert_eq!(
        diagnostic_count(&diags, 2556),
        1,
        "inline function with no param at the rest position must emit TS2556: {:?}",
        diagnostic_codes(&diags)
    );
}

#[test]
fn open_ended_tuple_into_iife_all_annotated_emits_ts2556() {
    // Every parameter annotated -> fixed signature -> behaves like a declared
    // function -> TS2556.
    let src = "const t: [number, boolean, ...string[]] = [1, true];\n\
               (function (a: number, b: boolean, c: boolean) {})(...t);\n";
    let diags = check_source_diagnostics(src);
    assert_eq!(
        diagnostic_count(&diags, 2556),
        1,
        "inline function with all-annotated params must emit TS2556: {:?}",
        diagnostic_codes(&diags)
    );
}

// ---------------------------------------------------------------------------
// `any` callee: no parameter-arity shape, so non-tuple spreads never overflow
// a fixed parameter list. tsc resolves `new anyCtor(...args)` through the
// any-signature path with no TS2556 (comlink canary, issue #13042).
// ---------------------------------------------------------------------------

#[test]
fn any_constructor_spread_arguments_do_not_emit_ts2556() {
    let source = "\
declare const makerish: any;
declare const packedBlob: any;
declare const packedList: any[];
const built = new makerish(...packedBlob);
const builtFromList = new makerish(...packedList);
";
    let diags = check_source_diagnostics(source);
    assert_eq!(
        diagnostic_count(&diags, 2556),
        0,
        "spread into an `any` constructor must not emit TS2556, got {:?}",
        diagnostic_codes(&diags)
    );
}

#[test]
fn any_callee_call_spread_arguments_do_not_emit_ts2556() {
    let source = "\
declare const invoker: any;
declare const looseArgs: any[];
const out = invoker(...looseArgs);
";
    let diags = check_source_diagnostics(source);
    assert_eq!(
        diagnostic_count(&diags, 2556),
        0,
        "spread into an `any` callee must not emit TS2556, got {:?}",
        diagnostic_codes(&diags)
    );
}

#[test]
fn known_constructor_non_tuple_spread_still_emits_ts2556() {
    let source = "\
declare class CrateBox {
    constructor(width: number, label: string);
}
declare const loosePair: number[];
const crated = new CrateBox(...loosePair);
";
    let diags = check_source_diagnostics(source);
    assert_eq!(
        diagnostic_count(&diags, 2556),
        1,
        "non-tuple spread into a fixed-arity constructor keeps TS2556, got {:?}",
        diagnostic_codes(&diags)
    );
}
