/// The `has_any_call_signatures` query unifies Function and Callable checks
/// to decide whether an arg type is callable during generic inference refinement.
#[test]
fn callable_arg_type_detected_for_refinement() {
    let source = r#"
declare function apply<T, R>(fn: (x: T) => R, arg: T): R;
let result: number = apply(x => x + 1, 5);
"#;
    assert!(
        no_errors(source),
        "Callable arg type should be detected for generic refinement"
    );
}

/// Spread arguments in calls should be handled correctly.
#[test]
fn spread_args_in_generic_call() {
    let source = r#"
declare function concat<T>(...args: T[]): T[];
let arr = [1, 2, 3];
let result = concat(...arr);
"#;
    assert!(no_errors(source), "Spread args in generic call should work");
}

// ============================================================================
// Overload resolution edge cases
// ============================================================================

#[test]
fn overload_resolution_preserves_first_match_ordering() {
    // When multiple overloads could match, tsc picks the first one.
    let source = r#"
declare function f(x: string): string;
declare function f(x: string | number): number;
let result: string = f("hello");
"#;
    assert!(
        no_errors(source),
        "First matching overload should be selected"
    );
}

#[test]
fn overload_with_rest_params() {
    let source = r#"
declare function f(...args: string[]): void;
declare function f(x: number): void;
f("a", "b", "c");
f(42);
"#;
    assert!(
        no_errors(source),
        "Overloads with rest params should resolve"
    );
}

#[test]
fn overload_with_type_arg_count_mismatch_recovery() {
    // TS2558 for wrong type arg count; should still recover return type
    let source = r#"
declare function f<T>(x: T): T;
f<string, number>("hello");
"#;
    assert!(
        has_error(source, 2558),
        "Wrong type argument count should emit TS2558"
    );
}

// ============================================================================
// Property-call patterns
// ============================================================================

#[test]
fn method_call_on_class_instance() {
    let source = r#"
class Foo {
    bar(x: number): string { return ""; }
}
let f = new Foo();
let result: string = f.bar(42);
"#;
    assert!(
        no_errors(source),
        "Method call on class instance should work"
    );
}

#[test]
fn method_call_on_nested_property() {
    let source = r#"
declare let obj: { inner: { method(x: string): number } };
let result: number = obj.inner.method("hello");
"#;
    assert!(no_errors(source), "Nested property method call should work");
}

#[test]
fn optional_chain_method_call_on_union() {
    let source = r#"
declare let x: { f(): number } | undefined;
let result = x?.f();
"#;
    assert!(
        no_errors(source),
        "Optional chain method call on union should work"
    );
}

#[test]
fn element_access_call() {
    let source = r#"
declare let obj: { [key: string]: (x: number) => string };
let result: string = obj["test"](42);
"#;
    assert!(no_errors(source), "Element access call should work");
}

// ============================================================================
// Generic call inference with callbacks
// ============================================================================

#[test]
fn generic_callback_contextual_typing_preserves_param_type() {
    let source = r#"
declare function map<T, U>(arr: T[], fn: (x: T) => U): U[];
let result = map([1, 2, 3], x => x + 1);
"#;
    assert!(
        no_errors(source),
        "Generic callback should have contextual param type"
    );
}

