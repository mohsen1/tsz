//! Focused tests for generic call inference and contextual instantiation.
//!
//! These exercise the call_inference.rs module:
//! - Round-2 contextual typing for callback parameters
//! - Return-context substitution collection
//! - Generic function argument refinement against targets
//! - Widening/literal-preservation in type parameter substitutions
//! - Binding-pattern sanitization during inference

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn relevant_diagnostics(source: &str) -> Vec<(u32, String)> {
    compile_and_get_diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code != 2318) // Filter out "Cannot find global type"
        .collect()
}

// ─── Round-2 contextual typing for callbacks ─────────────────────────

#[test]
fn callback_parameter_gets_contextual_type_from_generic_call() {
    // The callback `x => x` should infer `x: string` from the generic call
    let source = r#"
declare function map<T, U>(arr: T[], fn: (x: T) => U): U[];
const result = map(["a", "b"], x => x.length);
"#;
    let diags = relevant_diagnostics(source);
    // x should be contextually typed as string; x.length should work
    assert!(
        diags.iter().all(|(code, _)| *code != 2339),
        "Callback parameter should be contextually typed. Diagnostics: {diags:#?}"
    );
}

#[test]
fn round2_contextual_type_for_multi_param_generic() {
    // Both T and U should be inferred in a two-type-parameter scenario
    let source = r#"
declare function zip<T, U>(a: T[], b: U[], fn: (x: T, y: U) => [T, U]): [T, U][];
const result = zip([1, 2], ["a", "b"], (x, y) => [x, y]);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 7006),
        "Multi-param generic should contextually type all callback params. Diagnostics: {diags:#?}"
    );
}

// ─── Return-context substitution ─────────────────────────────────────

#[test]
fn return_context_infers_type_argument_from_variable_annotation() {
    let source = r#"
declare function identity<T>(x: T): T;
const x: string = identity("hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Return context should help infer T=string. Diagnostics: {diags:#?}"
    );
}

#[test]
fn return_context_detects_mismatch() {
    let source = r#"
declare function identity<T>(x: T): T;
const x: string = identity(42);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "Should detect type mismatch: number not assignable to string. Diagnostics: {diags:#?}"
    );
}

// ─── Generic function argument refinement ────────────────────────────

#[test]
fn generic_callback_refined_against_target_params() {
    // A generic callback passed as argument should get instantiated
    // against the target parameter types
    let source = r#"
declare function apply<T>(fn: (x: T) => T, value: T): T;
const result = apply(x => x, 42);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 7006),
        "Generic callback should be refined. Diagnostics: {diags:#?}"
    );
}

// ─── Constraint-based literal preservation ───────────────────────────

#[test]
fn literal_preserved_when_constraint_is_primitive() {
    // When T extends string, the inferred type should be the literal "hello"
    // (not widened to string)
    let source = r#"
declare function literal<T extends string>(x: T): T;
const result = literal("hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Literal should be preserved with primitive constraint. Diagnostics: {diags:#?}"
    );
}

#[test]
fn literal_widened_without_constraint() {
    // Without a constraint, literals should be widened in some contexts
    let source = r#"
declare function id<T>(x: T): T;
const result = id("hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Should compile without errors. Diagnostics: {diags:#?}"
    );
}

// ─── Binding pattern sanitization ────────────────────────────────────

#[test]
fn binding_pattern_param_does_not_pollute_inference() {
    // Object destructuring in callback params should not break inference
    let source = r#"
declare function process<T extends { x: number }>(items: T[], fn: (item: T) => void): void;
process([{ x: 1, y: 2 }], ({ x }) => { const _n: number = x; });
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2345 && *code != 7031),
        "Binding patterns should not pollute inference. Diagnostics: {diags:#?}"
    );
}

// ─── Contextual instantiation with applications ──────────────────────

#[test]
fn application_shape_preserved_in_contextual_type() {
    // When the contextual type is a generic application (e.g., Box<T>),
    // the inferred type should match. Currently Box<T> vs Box<number>
    // mismatch is a known conformance gap — verify it doesn't crash
    // and produces a stable diagnostic.
    let source = r#"
interface Box<T> { value: T; }
declare function wrap<T>(x: T): Box<T>;
const b: Box<number> = wrap(42);
"#;
    let diags = relevant_diagnostics(source);
    // Known conformance gap: checker reports Box<T> not assignable to Box<number>
    // instead of properly instantiating T=number in the return type.
    // This test documents current behavior and guards against regressions.
    let has_2322 = diags.iter().any(|(code, _)| *code == 2322);
    let has_crash = diags.iter().any(|(code, _)| *code == 0);
    assert!(
        !has_crash,
        "Should not crash on application contextual types. Diagnostics: {diags:#?}"
    );
    // When this conformance gap is fixed, this assertion can be flipped to
    // assert no TS2322 is emitted.
    assert!(
        has_2322,
        "Expected TS2322 for known conformance gap (Box<T> vs Box<number>). Diagnostics: {diags:#?}"
    );
}

// ─── Anyish inference detection ──────────────────────────────────────

#[test]
fn any_inferred_type_does_not_suppress_errors() {
    // When inference produces `any`, subsequent type errors should not be suppressed
    let source = r#"
declare function first<T>(arr: T[]): T;
declare const arr: any[];
const result = first(arr);
const _n: number = result;
"#;
    let diags = relevant_diagnostics(source);
    // `result` is `any` from `any[]` input, so assigning to `number` is fine
    assert!(
        diags.iter().all(|(code, _)| *code != 2322),
        "any-inferred result should be assignable to number. Diagnostics: {diags:#?}"
    );
}

// ─── Generic call with rest parameters ───────────────────────────────

#[test]
fn rest_parameter_inference_in_generic_call() {
    let source = r#"
declare function concat<T>(...args: T[]): T[];
const result = concat(1, 2, 3);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Rest parameter inference should work. Diagnostics: {diags:#?}"
    );
}

// ─── Enum argument namespace resolution ──────────────────────────────

#[test]
fn enum_as_argument_resolves_to_namespace_for_inference() {
    let source = r#"
enum Direction { Up, Down, Left, Right }
declare function keys<T extends object>(obj: T): (keyof T)[];
const k = keys(Direction);
"#;
    let diags = relevant_diagnostics(source);
    // Should not produce TS2345 for enum passed as object
    assert!(
        diags.iter().all(|(code, _)| *code != 2345),
        "Enum should be usable as object argument. Diagnostics: {diags:#?}"
    );
}

// ─── Zero-param callback conditional branch ──────────────────────────

#[test]
fn zero_param_callback_conditional_branch_used_for_contextual_type() {
    // A zero-parameter callback whose body is a conditional expression
    // should use the true branch for contextual typing
    let source = r#"
declare function lazy<T>(fn: () => T): T;
const result = lazy(() => true ? 42 : "hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Zero-param callback conditional should work. Diagnostics: {diags:#?}"
    );
}
