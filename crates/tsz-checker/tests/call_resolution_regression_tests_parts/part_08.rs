#[test]
fn type_predicate_call_narrows_type() {
    let source = r#"
function isString(x: unknown): x is string {
    return typeof x === "string";
}
declare let val: string | number;
if (isString(val)) {
    let s: string = val;
}
"#;
    assert!(no_errors(source), "Type predicate call should narrow type");
}

// ============================================================================
// Generic call inference edge cases
// ============================================================================

#[test]
fn generic_call_with_literal_type_preservation() {
    let source = r#"
declare function identity<T>(x: T): T;
const result = identity("hello");
"#;
    // Should infer T as "hello" (literal) or string — no error either way
    assert!(
        no_errors(source),
        "Generic call with literal should not error"
    );
}

#[test]
fn generic_call_with_constrained_type_param() {
    let source = r#"
declare function first<T extends any[]>(arr: T): T[0];
let result: number = first([1, 2, 3]);
"#;
    assert!(
        no_errors(source),
        "Generic call with constrained type param should work"
    );
}

#[test]
fn generic_call_with_multiple_type_params() {
    let source = r#"
declare function pair<A, B>(a: A, b: B): [A, B];
let result = pair(1, "hello");
"#;
    assert!(
        no_errors(source),
        "Generic call with multiple type params should work"
    );
}

#[test]
fn generic_call_with_default_type_param() {
    let source = r#"
declare function create<T = string>(x?: T): T;
let result: string = create();
"#;
    assert!(
        no_errors(source),
        "Generic call with default type param should work"
    );
}

// =============================================================================
// Regression tests for call.rs query boundary refactoring
// =============================================================================

/// Tests that generic calls with Application-typed params and args
/// correctly preserve raw applications during inference.
#[test]
fn generic_call_preserves_application_during_inference() {
    let source = r#"
interface Box<T> { value: T }
declare function unbox<T>(b: Box<T>): T;
declare const boxed: Box<number>;
let result: number = unbox(boxed);
"#;
    assert!(
        no_errors(source),
        "Generic call should preserve application types during inference"
    );
}

/// Tests that the type-parameter-or-intersection check correctly skips
/// excess property checking for generic params.
#[test]
fn generic_call_skips_excess_for_type_param() {
    let source = r#"
interface Named { name: string }
declare function parrot<T extends Named>(t: T): T;
parrot({ name: "hello", extra: 42 });
"#;
    // tsc allows extra properties when param is a bare type parameter
    // (the type parameter captures the full object shape).
    assert!(
        no_errors(source),
        "Generic call with type param should skip excess property checking"
    );
}

/// Tests that intersection-containing-type-parameter is correctly detected
/// for excess property skip.
#[test]
fn generic_call_intersection_param_skips_excess() {
    let source = r#"
interface Printable { print(): void }
declare function create<T extends Printable>(t: T & Printable): T;
create({ print() {}, extra: true });
"#;
    assert!(
        no_errors(source),
        "Intersection with type param should skip excess property checking"
    );
}

/// Tests that callable argument types are correctly detected during
/// generic call refinement.
#[test]
fn generic_call_callable_arg_refinement() {
    let source = r#"
declare function map<T, U>(arr: T[], fn: (x: T) => U): U[];
let result = map([1, 2, 3], x => String(x));
"#;
    // The key behavior: the callback parameter `x` gets contextual type `number`
    // from inference, so no TS7006 (implicit any) should be emitted.
    assert!(
        !has_error(source, 7006),
        "Generic call with callable arg should provide contextual type to callback"
    );
}

/// Tests that overloaded function calls resolve to the correct signature.
#[test]
fn overload_resolution_picks_correct_signature() {
    let source = r#"
declare function convert(x: string): number;
declare function convert(x: number): string;
let a: number = convert("hello");
let b: string = convert(42);
"#;
    assert!(
        no_errors(source),
        "Overload resolution should pick the correct signature for each call"
    );
}

