#[test]
fn test_intra_expression_conditional_return_seeds_receiver_context() {
    let source = r#"
declare const console: { log(value: any): void };
declare function simplified<T>(props: { generator: () => T, receiver: (t: T) => any }): void;
declare function withProps<T>(props: { generator: (bob: any) => T, receiver: (t: T) => void }): void;
declare function withArgs<T>(generator: (bob: any) => T, receiver: (t: T) => void): void;

simplified({ generator: () => 123, receiver: t => console.log(t + 2) });
withProps({ generator: bob => bob ? 1 : 2, receiver: t => console.log(t + 2) });
withArgs(bob => bob ? 1 : 2, t => console.log(t + 2));
"#;
    let diagnostics = check_with_options(
        source,
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );
    let inference_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2322 || d.code == 18046)
        .collect();
    assert!(
        inference_errors.is_empty(),
        "Expected conditional callback return to seed receiver context, got {inference_errors:?}"
    );
}

/// Widening round-2 contextual substitution preserves literals when constraint allows.
/// When the type parameter has a constraint like `string`, inferred literal types
/// should be widened to `string` in round-2.
#[test]
fn test_widen_round2_preserves_literals_with_string_constraint() {
    let source = r#"
declare function tag<T extends string>(value: T): { tag: T };
const t = tag("hello");
"#;
    let diagnostics = check_default(source);
    assert!(
        diagnostics.is_empty(),
        "Expected no errors for literal-preserving generic, got diagnostics={diagnostics:?}"
    );
}

/// Generic function with binding-pattern parameter should sanitize the destructured
/// param to `unknown` so it doesn't pollute inference for other type parameters.
#[test]
fn test_binding_pattern_param_sanitized_for_inference() {
    let source = r#"
declare function process<T>(items: T[], handler: (item: T) => void): T[];
const result = process([1, 2, 3], ({ }) => {});
"#;
    let diagnostics = check_default(source);
    // The binding pattern `{ }` should not cause a type error —
    // its param type is sanitized to unknown during inference.
    assert!(
        !diagnostics.iter().any(|d| d.code == 2345),
        "Expected no TS2345 for binding pattern param, got diagnostics={diagnostics:?}"
    );
}

#[test]
fn test_destructuring_with_generic_parameter_fixture_shape_has_no_ts2345() {
    let source = r#"
class GenericClass<T> {
    payload: T;
}

var genericObject = new GenericClass<{ greeting: string }>();

function genericFunction<T>(object: GenericClass<T>, callback: (payload: T) => void) {
    callback(object.payload);
}

genericFunction(genericObject, ({greeting}) => {
    var s = greeting.toLocaleLowerCase();
});
"#;
    let diagnostics = check_default(source);
    assert!(
        !diagnostics.iter().any(|d| d.code == 2345),
        "Expected no TS2345 for destructuring generic parameter fixture, got diagnostics={diagnostics:?}"
    );
}

/// Generic function instantiation against target: source generic function arg
/// should be instantiated using target parameter types as context.
#[test]
fn test_generic_function_arg_instantiation_against_target() {
    let source = r#"
declare function apply<T>(fn: (x: T) => T, value: T): T;
const result: number = apply(x => x, 42);
"#;
    let diagnostics = check_default(source);
    assert!(
        diagnostics.is_empty(),
        "Expected no errors for generic instantiation against target, got diagnostics={diagnostics:?}"
    );
}

/// Return-context substitution: generic function's return type should be matched
/// against the expected return type to infer type parameters from the return context.
#[test]
fn test_return_context_substitution_with_generic() {
    let source = r#"
declare function wrap<T>(value: T): { wrapped: T };
const w: { wrapped: string } = wrap("hello");
"#;
    let diagnostics = check_default(source);
    assert!(
        diagnostics.is_empty(),
        "Expected no errors for return context substitution, got diagnostics={diagnostics:?}"
    );
}

/// Return-context substitution through array element matching: when the return type
/// is an array and the contextual type is also an array, element types should match.
#[test]
fn test_return_context_substitution_through_array() {
    let source = r#"
declare function toArray<T>(value: T): T[];
const a: string[] = toArray("hello");
"#;
    let diagnostics = check_default(source);
    assert!(
        diagnostics.is_empty(),
        "Expected no errors for return context through array, got diagnostics={diagnostics:?}"
    );
}

/// Shape-to-defaults instantiation: when a generic function's type parameters have
/// defaults, they should be used for contextual matching when no argument-driven
/// substitution is available.
#[test]
fn test_shape_to_defaults_instantiation() {
    let source = r#"
declare function withDefault<T = string>(fn: (x: T) => void): T;
const d = withDefault((x) => {});
"#;
    let diagnostics = check_default(source);
    assert!(
        diagnostics.is_empty(),
        "Expected no errors for shape-to-defaults instantiation, got diagnostics={diagnostics:?}"
    );
}

/// Multiple generic type parameters resolved across multiple callback arguments
/// should work with the instantiated function shape boundary helper.
#[test]
fn test_multi_param_generic_call_with_callbacks() {
    let source = r#"
declare function combine<A, B>(a: A, b: B, fn: (x: A, y: B) => A): A;
const r: number = combine(1, "hello", (x, y) => x + y.length);
"#;
    let diagnostics = check_default(source);
    assert!(
        diagnostics.is_empty(),
        "Expected no errors for multi-param generic call, got diagnostics={diagnostics:?}"
    );
}

/// Callable type (overloaded) with binding pattern parameter should have all
/// signatures sanitized, not just the first.
#[test]
fn test_callable_binding_pattern_sanitization() {
    let source = r#"
declare function useCallback(cb: (item: { x: number }) => void): void;
useCallback(({ x }) => {});
"#;
    let diagnostics = check_default(source);
    assert!(
        diagnostics.is_empty(),
        "Expected no errors for callable binding pattern, got diagnostics={diagnostics:?}"
    );
}

/// When a generic method like `.then()` returns `Promise<TResult>` and the
/// contextual type is `Promise<DooDad>` where `DooDad = 'A' | 'B'`, the
/// callback return literal should not be widened to `string` during inference.
/// This matches tsc's behavior where literals contextually typed by type
/// parameters are treated as non-fresh.
#[test]
fn test_generic_call_return_context_preserves_literal_in_callback() {
    let source = r#"
type DooDad = 'SOMETHING' | 'ELSE';

declare function invoke<T>(f: () => T): T;
let xx: DooDad = invoke(() => 'ELSE');
"#;
    let diagnostics = check_default(source);
    assert!(
        diagnostics.is_empty(),
        "invoke(() => 'ELSE') should infer T = 'ELSE' (subtype of DooDad), got: {diagnostics:?}"
    );
}
