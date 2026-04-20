#[test]
fn overload_with_spread_args() {
    let source = r#"
declare function foo(a: number, b: string): void;
declare function foo(a: string): void;
foo("hello");
"#;
    assert!(
        no_errors(source),
        "Overload resolution with fewer args should pick matching signature"
    );
}

#[test]
fn overload_wrong_arg_count_emits_ts2554() {
    let source = r#"
declare function bar(x: number): void;
bar(1, 2);
"#;
    assert!(has_error(source, 2554), "Too many args should emit TS2554");
}

#[test]
fn generic_call_inference_with_callback() {
    let source = r#"
declare function map<T, U>(arr: T[], fn: (x: T) => U): U[];
let result: number[] = map(["a", "b"], x => x.length);
"#;
    assert!(
        no_errors(source),
        "Generic call with callback inference should work"
    );
}

// ============================================================================
// Regression tests for solver query-based call resolution (query boundary layer)
// ============================================================================

/// When a generic function parameter is `T` (bare type parameter), sanitization
/// should replace the sensitive placeholder with `unknown` to avoid contaminating
/// the solver's second inference pass. The query `is_type_param_at_top_or_in_intersection`
/// drives this decision.
#[test]
fn generic_call_bare_type_param_sanitizes_callback() {
    let source = r#"
declare function wrap<T>(fn: T): T;
let result = wrap((x: number) => x + 1);
"#;
    assert!(
        no_errors(source),
        "Bare type param sanitization should not cause false errors"
    );
}

/// Same sanitization applies when the shape parameter is `T & SomeInterface`.
#[test]
fn generic_call_intersection_type_param_sanitizes_callback() {
    let source = r#"
interface HasLength { length: number; }
declare function constrained<T extends HasLength>(fn: T & HasLength): T;
let result = constrained({ length: 5 });
"#;
    assert!(
        no_errors(source),
        "Intersection containing type param should sanitize correctly"
    );
}

/// When a generic shape parameter is a concrete callable like `Predicate<A>`,
/// the sensitive placeholder should NOT be sanitized because its callable
/// structure helps infer the inner type param A.
#[test]
fn generic_call_concrete_callable_param_preserves_placeholder() {
    let source = r#"
type Predicate<T> = (x: T) => boolean;
declare function filter<T>(arr: T[], pred: Predicate<T>): T[];
let nums = filter([1, 2, 3], x => x > 0);
"#;
    assert!(
        no_errors(source),
        "Concrete callable param should preserve placeholder for inner inference"
    );
}

/// When both param and arg are Application types and param contains type params,
/// the pre-evaluation step should preserve raw Application form. The query
/// `both_are_applications_with_generic_param` drives this decision.
#[test]
fn generic_call_preserves_raw_application_for_aligned_shapes() {
    let source = r#"
interface Opts<S> { state: S; }
declare function createStore<S>(opts: Opts<S>): S;
let store = createStore({ state: 42 });
"#;
    assert!(
        no_errors(source),
        "Aligned Application shapes should be preserved during pre-evaluation"
    );
}

/// Overload resolution: when multiple signatures exist, the first matching one wins.
#[test]
fn overload_resolution_picks_first_match() {
    let source = r#"
declare function overloaded(x: string): string;
declare function overloaded(x: number): number;
let r1: string = overloaded("hello");
let r2: number = overloaded(42);
"#;
    assert!(
        no_errors(source),
        "Overload resolution should pick correct signature"
    );
}

/// Overload resolution should emit TS2769 when no overload matches.
#[test]
fn overload_resolution_no_match_emits_error() {
    let source = r#"
declare function overloaded(x: string): string;
declare function overloaded(x: number): number;
overloaded(true);
"#;
    assert!(
        has_error(source, 2769),
        "No matching overload should emit TS2769"
    );
}

/// Property call: calling a method via property access on a typed object.
#[test]
fn property_call_method_on_interface() {
    let source = r#"
interface Obj {
    greet(name: string): string;
}
declare const obj: Obj;
let result: string = obj.greet("world");
"#;
    assert!(
        no_errors(source),
        "Property method call should resolve correctly"
    );
}

