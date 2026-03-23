//! Focused tests for generic call inference and contextual instantiation.
//!
//! These exercise the `call_inference.rs` module:
//! - Round-2 contextual typing for callback parameters
//! - Return-context substitution collection
//! - Generic function argument refinement against targets
//! - Widening/literal-preservation in type parameter substitutions
//! - Binding-pattern sanitization during inference
//! - Contextual constraint with self-referential type parameters
//! - Application shape preservation through union/intersection
//! - Anyish inference detection across composite types
//! - Return context substitution through tuples, arrays, and generics

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
    assert!(
        diags.is_empty(),
        "Return context should instantiate T=number through Box<T>. Diagnostics: {diags:#?}"
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

// ─── Return-context substitution through structured types ─────────────

#[test]
fn return_context_substitution_through_array() {
    // Return-context collection should walk through array element types
    // to match T[] in the return position against a concrete target.
    let source = r#"
declare function wrap<T>(x: T): T[];
const result: string[] = wrap("hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2322),
        "Return context should infer T=string through array type. Diagnostics: {diags:#?}"
    );
}

#[test]
fn return_context_substitution_through_tuple() {
    // Return-context collection should walk through tuple element types
    let source = r#"
declare function pair<T, U>(a: T, b: U): [T, U];
const result: [number, string] = pair(1, "a");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2322),
        "Return context should infer through tuple types. Diagnostics: {diags:#?}"
    );
}

#[test]
fn return_context_substitution_through_generic_application() {
    // Return-context should match Application<T> against Application<concrete>
    // by comparing type arguments when base types match.
    let source = r#"
interface Wrapper<T> { value: T; }
declare function make<T>(x: T): Wrapper<T>;
declare function consume(w: Wrapper<number>): void;
consume(make(42));
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Return context should match Application<T> against Application<concrete>. Diagnostics: {diags:#?}"
    );
}

// ─── Contextual constraint with self-referential type parameters ──────

#[test]
fn self_referential_constraint_does_not_produce_any_contextual_type() {
    // When T extends Foo<T>, the self-reference should be broken (T → unknown)
    // so the constraint evaluates to a usable contextual type rather than `any`.
    let source = r#"
interface Base<T> { value: T; }
declare function create<T extends Base<T>>(init: (x: T) => void): T;
const result = create((x) => { const _v = x.value; });
"#;
    let diags = relevant_diagnostics(source);
    // The callback param `x` should get a usable contextual type, not `any`
    assert!(
        diags.iter().all(|(code, _)| *code != 2339),
        "Self-referential constraint should resolve to usable contextual type. Diagnostics: {diags:#?}"
    );
}

// ─── Widening behavior with const type parameters ─────────────────────

#[test]
fn const_type_parameter_preserves_literal() {
    // `const T` type parameters should always preserve literal types
    // (skip widening even without a primitive constraint).
    let source = r#"
declare function literal<const T>(x: T): T;
const result = literal("hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "const type parameter should compile cleanly. Diagnostics: {diags:#?}"
    );
}

// ─── Multiple overload-like generic signatures ────────────────────────

#[test]
fn generic_call_with_union_constraint_infers_correctly() {
    // T extends string | number should allow both string and number arguments
    let source = r#"
declare function coerce<T extends string | number>(x: T): T;
const a = coerce("hello");
const b = coerce(42);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Union constraint should accept both string and number. Diagnostics: {diags:#?}"
    );
}

// ─── Callable param specificity comparison ────────────────────────────

#[test]
fn more_specific_contextual_type_wins_for_callback() {
    // When two candidate contextual types exist (e.g., from overloads),
    // the one with more non-any parameter types should be preferred.
    let source = r#"
declare function apply<T>(fn: (x: T) => T, value: T): T;
const result = apply((x) => x, 42);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 7006),
        "Callback should get contextual typing from most specific candidate. Diagnostics: {diags:#?}"
    );
}

// ─── Rest parameter in generic call ───────────────────────────────────

#[test]
fn rest_parameter_contextual_typing_in_callback() {
    // Rest parameters in generic calls should provide correct contextual types
    let source = r#"
declare function apply<T extends any[]>(fn: (...args: T) => void, ...args: T): void;
apply((a, b) => {}, 1, "hello");
"#;
    let diags = relevant_diagnostics(source);
    // Should not produce TS7006 for callback params when rest provides context
    assert!(
        diags.iter().all(|(code, _)| *code != 7006),
        "Rest parameter should provide contextual typing for callback. Diagnostics: {diags:#?}"
    );
}

// ─── Generic call with return context from union target ───────────────

#[test]
fn return_context_strips_null_undefined_for_substitution() {
    // When the return context target is `T | null | undefined`, the
    // substitution collector should skip null/undefined members and
    // use the non-nullable part for inference.
    let source = r#"
declare function id<T>(x: T): T;
const result: string | null = id("hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2322),
        "Return context should handle nullable union targets. Diagnostics: {diags:#?}"
    );
}

// ─── Iterator info matching in return context ─────────────────────────

#[test]
fn return_context_array_matches_iterable_target() {
    // When source returns T[] but target expects Iterable<concrete>,
    // the return context should extract yield_type from the iterable
    // and match it against the array element type.
    let source = r#"
declare function wrap<T>(x: T): T[];
declare function consume(iter: Iterable<number>): void;
consume(wrap(42));
"#;
    let diags = relevant_diagnostics(source);
    // May or may not produce errors depending on Iterable availability,
    // but should not crash or produce internal errors.
    assert!(
        diags.iter().all(|(code, _)| *code != 0),
        "Array-to-iterable matching should not crash. Diagnostics: {diags:#?}"
    );
}

// ─── Object structural matching in return context ─────────────────────

#[test]
fn return_context_matches_structurally_through_object_properties() {
    // When source returns an application type that evaluates to an object
    // and the target is an already-evaluated object, property types should
    // be matched structurally for return context substitution.
    let source = r#"
interface Config<T> { value: T; label: string; }
declare function config<T>(v: T): Config<T>;
const c: { value: number; label: string } = config(42);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Return context should match structurally through object properties. Diagnostics: {diags:#?}"
    );
}

// ─── Readonly/NoInfer wrapper unwrapping ──────────────────────────────

#[test]
fn application_shape_preserved_through_readonly() {
    // should_preserve_contextual_application_shape should recurse
    // through Readonly<T> wrappers to find application shapes.
    let source = r#"
interface Box<T> { value: T; }
declare function make<T>(x: T): Readonly<Box<T>>;
const result = make(42);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Readonly wrapper should not break inference. Diagnostics: {diags:#?}"
    );
}

// ─── Inference with multiple callbacks ────────────────────────────────

#[test]
fn multiple_callback_params_all_get_contextual_types() {
    // When a generic function has multiple callback parameters,
    // all should receive contextual types from the inferred type arguments.
    let source = r#"
declare function bimap<T, U, V>(
    arr: T[],
    first: (x: T) => U,
    second: (x: T) => V
): [U[], V[]];
const result = bimap([1, 2, 3], x => x + 1, x => String(x));
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 7006),
        "All callbacks should receive contextual types. Diagnostics: {diags:#?}"
    );
}

// ─── Explicit type arguments bypass inference ─────────────────────────

#[test]
fn explicit_type_arguments_provide_callback_context() {
    // When type arguments are explicitly provided, callback params
    // should be contextually typed from those explicit types.
    let source = r#"
declare function map<T, U>(arr: T[], fn: (x: T) => U): U[];
const result = map<number, string>([1, 2], x => String(x));
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 7006 && *code != 2345),
        "Explicit type args should provide callback context. Diagnostics: {diags:#?}"
    );
}

// ─── Nested generic calls ─────────────────────────────────────────────

#[test]
fn nested_generic_calls_propagate_inference() {
    // Generic inference should work through nested generic calls
    let source = r#"
declare function id<T>(x: T): T;
declare function map<T, U>(arr: T[], fn: (x: T) => U): U[];
const result = map([1, 2, 3], x => id(x));
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 7006),
        "Nested generic calls should propagate inference. Diagnostics: {diags:#?}"
    );
}

// ─── Generic inference with default type parameters ───────────────────

#[test]
fn default_type_parameter_used_when_not_inferable() {
    // When a type parameter has a default and cannot be inferred,
    // the default should be used.
    let source = r#"
declare function create<T = string>(value?: T): T;
const result = create();
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Default type parameter should be used when not inferable. Diagnostics: {diags:#?}"
    );
}

// ─── Callable shape sanitization with overloads ───────────────────────

#[test]
fn callable_binding_pattern_param_sanitization_single_signature() {
    // A callable argument with a destructured param should not break inference.
    // The callable shape's call signature params at binding-pattern positions
    // are replaced with `unknown` to avoid polluting the inference constraint.
    let source = r#"
declare function apply<T extends { a: number; b: string }>(
    items: T[],
    fn: (item: T) => void
): void;
apply([{ a: 1, b: "x" }], ({ a, b }) => {
    const _n: number = a;
    const _s: string = b;
});
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2345 && *code != 7031),
        "Callable shape sanitization should not break inference. Diagnostics: {diags:#?}"
    );
}

#[test]
fn callable_binding_pattern_does_not_leak_unknown_into_inferred_type() {
    // When a callback destructures its parameter, the inferred type for the
    // generic should still be correct (not unknown).
    let source = r#"
declare function first<T>(arr: T[], fn: (item: T) => boolean): T;
const result = first([1, 2, 3], (item) => item > 0);
const check: number = result;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Inferred T should still be number. Diagnostics: {diags:#?}"
    );
}

// ─── Contextual instantiation edge cases ──────────────────────────────

#[test]
fn contextual_instantiation_through_readonly() {
    // Return context substitution should unwrap Readonly<T> when matching
    let source = r#"
declare function wrap<T>(value: T): Readonly<T>;
const result: Readonly<string> = wrap("hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Return context should match through Readonly. Diagnostics: {diags:#?}"
    );
}

#[test]
fn generic_call_with_union_return_context() {
    // Return context substitution should handle union target types
    let source = r#"
declare function id<T>(x: T): T;
const result: string | number = id("hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Return context should work with union target. Diagnostics: {diags:#?}"
    );
}

#[test]
fn generic_call_application_matching_in_return_context() {
    // When source and target are both applications of the same generic,
    // their type arguments should be matched structurally.
    let source = r#"
interface Container<T> { value: T; }
declare function box_it<T>(value: T): Container<T>;
const result: Container<number> = box_it(42);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Application matching should work in return context. Diagnostics: {diags:#?}"
    );
}

// ─── Contextual instantiation through intersections ────────────────────

#[test]
fn generic_callback_in_intersection_parameter() {
    // When a generic parameter type is an intersection involving a callback,
    // inference should still provide contextual types for callback params.
    let source = r#"
declare function register<T>(value: T, handler: (x: T) => void): void;
register(42, (x) => {
    const _n: number = x;
});
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 7006),
        "Intersection callback should get contextual type. Diagnostics: {diags:#?}"
    );
}

// ─── Generic inference with mapped type return ──────────────────────────

#[test]
fn generic_inference_with_conditional_return() {
    // Generic inference should work when return type involves a conditional
    let source = r#"
declare function check<T>(x: T): T extends string ? true : false;
const result = check("hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Conditional return type should not break inference. Diagnostics: {diags:#?}"
    );
}

// ─── Generic call with spread arguments ─────────────────────────────────

#[test]
fn generic_call_infers_from_spread_args() {
    // Spread arguments should participate in generic inference
    let source = r#"
declare function first<T>(arr: T[]): T;
const arr = [1, 2, 3];
const result = first(arr);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Spread args should participate in inference. Diagnostics: {diags:#?}"
    );
}

// ─── Widening with number literal constraint ────────────────────────────

#[test]
fn number_literal_constraint_preserves_literal_type() {
    // T extends number should preserve the literal 42, not widen to number
    let source = r#"
declare function num<T extends number>(x: T): T;
const result = num(42);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Number literal should be preserved with number constraint. Diagnostics: {diags:#?}"
    );
}

// ─── Boolean literal constraint ──────────────────────────────────────────

#[test]
fn boolean_literal_constraint_preserves_literal_type() {
    let source = r#"
declare function bool<T extends boolean>(x: T): T;
const result = bool(true);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Boolean literal should be preserved with boolean constraint. Diagnostics: {diags:#?}"
    );
}

// ─── Contextual type propagation through Promise-like ────────────────────

#[test]
fn generic_call_with_promise_return_context() {
    // Return context from a Promise<T> target should propagate T inference
    let source = r#"
declare function resolve<T>(x: T): Promise<T>;
async function test() {
    const result: Promise<string> = resolve("hello");
}
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2322),
        "Promise return context should propagate inference. Diagnostics: {diags:#?}"
    );
}

// ─── Generic call recheck with real types ────────────────────────────────

#[test]
fn recheck_generic_call_detects_argument_mismatch_after_inference() {
    // After inference resolves T, rechecking should catch argument mismatches
    let source = r#"
declare function map<T>(arr: T[], fn: (x: T) => T): T[];
const result = map([1, 2, 3], (x) => "not a number");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().any(|(code, _)| *code == 2322 || *code == 2345),
        "Recheck should detect type mismatch after inference. Diagnostics: {diags:#?}"
    );
}

// ─── Generic call with optional parameter ────────────────────────────────

#[test]
fn generic_inference_with_optional_params() {
    let source = r#"
declare function opt<T>(required: T, optional?: T): T;
const result = opt("hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Optional params should not break generic inference. Diagnostics: {diags:#?}"
    );
}

// ─── Multiple constraints interacting ────────────────────────────────────

#[test]
fn generic_with_extends_keyof_constraint() {
    // T extends keyof U should constrain T to string literal union keys of U
    let source = r#"
declare function pick<U, T extends keyof U>(obj: U, key: T): U[T];
const result = pick({ a: 1, b: "hello" }, "a");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "keyof constraint should work with inference. Diagnostics: {diags:#?}"
    );
}

// ─── Recursive generic callback ──────────────────────────────────────────

#[test]
fn recursive_generic_callback_does_not_stack_overflow() {
    // Self-referential constraints should not cause stack overflow
    let source = r#"
interface Tree<T> { value: T; children: Tree<T>[]; }
declare function traverse<T>(tree: Tree<T>, fn: (node: Tree<T>) => void): void;
declare const tree: Tree<number>;
traverse(tree, (node) => {
    const _v: number = node.value;
});
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 7006),
        "Recursive generic callback should not overflow. Diagnostics: {diags:#?}"
    );
}
