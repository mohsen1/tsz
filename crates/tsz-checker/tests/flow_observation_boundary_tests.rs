//! Regression tests for the FlowObservation boundary (query_boundaries/flow.rs).
//!
//! These tests verify that control-flow narrowing decisions are correctly
//! routed through the boundary layer rather than being made locally in the
//! checker.  Each test targets a specific observation kind.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_source(source: &str, file_name: &str, options: CheckerOptions) -> Vec<Diagnostic> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let source_file = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), source_file);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );
    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(source_file);
    checker.ctx.diagnostics.clone()
}

fn check_ts(source: &str) -> Vec<Diagnostic> {
    check_source(source, "test.ts", CheckerOptions::default())
}

fn codes(diags: &[Diagnostic], code: u32) -> Vec<&Diagnostic> {
    diags.iter().filter(|d| d.code == code).collect()
}

// =============================================================================
// Destructuring control-flow
// =============================================================================

/// Destructuring with defaults should strip `undefined` from optional properties.
#[test]
fn destructuring_property_default_strips_undefined() {
    let diags = check_ts(
        r#"
function f(opts: { name?: string }) {
    const { name = "default" } = opts;
    const x: string = name;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Destructuring default should strip undefined, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Destructuring element with default in array pattern.
#[test]
fn destructuring_element_default_strips_undefined() {
    let diags = check_ts(
        r#"
function f(arr: [string | undefined]) {
    const [first = "fallback"] = arr;
    const x: string = first;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Array destructuring default should strip undefined, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Nested destructuring with defaults.
#[test]
fn nested_destructuring_with_defaults() {
    let diags = check_ts(
        r#"
interface Config {
    server?: {
        port?: number;
    };
}
function f(config: Config) {
    const { server: { port = 3000 } = {} } = config;
    const p: number = port;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Nested destructuring default should strip undefined, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// =============================================================================
// Optional-chain narrowing
// =============================================================================

/// Optional chain in truthy branch should narrow the base to non-nullish.
#[test]
fn optional_chain_truthy_narrows_non_nullish() {
    let diags = check_ts(
        r#"
function f(x: { y: number } | null | undefined) {
    if (x?.y) {
        const n: number = x.y;
    }
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Optional chain truthy should narrow non-nullish, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Chained optional access in condition.
#[test]
fn chained_optional_access_narrows() {
    let diags = check_ts(
        r#"
interface A { b?: { c: number } }
function f(a: A | null) {
    if (a?.b?.c) {
        const n: number = a.b.c;
    }
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Chained optional access should narrow, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// =============================================================================
// Catch variable unknown behavior
// =============================================================================

/// With useUnknownInCatchVariables (default strict), catch var should be `unknown`.
#[test]
fn catch_variable_is_unknown_by_default() {
    let diags = check_ts(
        r#"
try {
    throw new Error("oops");
} catch (e) {
    const msg: string = e.message;
}
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2571 || d.code == 18046)
        .collect();
    assert!(
        !relevant.is_empty(),
        "Catch variable should be 'unknown' with strict mode, expected error for e.message"
    );
}

/// With useUnknownInCatchVariables=false, catch var should be `any`.
#[test]
fn catch_variable_is_any_when_disabled() {
    let diags = check_source(
        r#"
try {
    throw new Error("oops");
} catch (e) {
    const msg: string = e.message;
}
"#,
        "test.ts",
        CheckerOptions {
            use_unknown_in_catch_variables: false,
            ..CheckerOptions::default()
        },
    );
    let errs = codes(&diags, 2571);
    assert!(
        errs.is_empty(),
        "Catch variable should be 'any' with flag disabled, got TS2571: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Typeof narrowing on catch variable should work from unknown domain.
#[test]
fn catch_variable_typeof_narrows_from_unknown() {
    let diags = check_ts(
        r#"
try {
    throw new Error("oops");
} catch (e) {
    if (typeof e === "string") {
        const s: string = e;
    }
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Typeof narrowing on catch variable should work, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// =============================================================================
// For-of destructuring
// =============================================================================

/// For-of with simple variable binding.
#[test]
fn for_of_simple_variable() {
    let diags = check_ts(
        r#"
const arr: number[] = [1, 2, 3];
for (const x of arr) {
    const n: number = x;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "For-of element should have correct type, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// For-of with destructuring pattern.
#[test]
fn for_of_with_destructuring() {
    let diags = check_ts(
        r#"
const pairs: [string, number][] = [["a", 1], ["b", 2]];
for (const [key, value] of pairs) {
    const k: string = key;
    const v: number = value;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "For-of destructuring should resolve types, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// For-of with object destructuring and defaults.
#[test]
fn for_of_object_destructuring_with_default() {
    let diags = check_ts(
        r#"
interface Item { name?: string; value: number }
const items: Item[] = [];
for (const { name = "unknown", value } of items) {
    const n: string = name;
    const v: number = value;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "For-of object destructuring with default should work, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// =============================================================================
// Dependent destructured variables
// =============================================================================

/// Multiple bindings from same destructuring share the parent type.
#[test]
fn dependent_destructured_bindings() {
    let diags = check_ts(
        r#"
interface Pair { first: string; second: number }
function f(p: Pair) {
    const { first, second } = p;
    const s: string = first;
    const n: number = second;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Dependent destructured variables should have correct types, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Rest element in destructuring.
#[test]
fn destructuring_rest_element() {
    let diags = check_ts(
        r#"
function f(arr: [number, string, ...boolean[]]) {
    const [first, second, ...rest] = arr;
    const n: number = first;
    const s: string = second;
    const b: boolean[] = rest;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Rest element destructuring should work, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// =============================================================================
// FlowObservation boundary integration
// =============================================================================

/// Verify catch variable type is correctly routed through the boundary.
#[test]
fn flow_observation_boundary_catch_variable_integration() {
    // With strict mode (default), catch variable should be unknown
    let diags_strict = check_ts(
        r#"
try {} catch (e) {
    let x = e;
}
"#,
    );
    let ts2322_strict = codes(&diags_strict, 2322);
    assert!(ts2322_strict.is_empty());

    // Without useUnknownInCatchVariables, catch variable should be any
    let diags_lax = check_source(
        r#"
try {} catch (e) {
    let x: number = e;
}
"#,
        "test.ts",
        CheckerOptions {
            use_unknown_in_catch_variables: false,
            ..CheckerOptions::default()
        },
    );
    let ts2322_lax = codes(&diags_lax, 2322);
    assert!(
        ts2322_lax.is_empty(),
        "catch variable with useUnknown=false should be any"
    );
}
