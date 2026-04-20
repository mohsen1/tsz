#[test]
fn call_non_callable_emits_ts2349() {
    let source = r#"
let x: number = 42;
x();
"#;
    assert!(
        has_error(source, 2349),
        "Calling a non-callable type should emit TS2349"
    );
}

#[test]
fn call_any_returns_any_no_error() {
    let source = r#"
declare let x: any;
let result = x();
"#;
    assert!(no_errors(source), "Calling any should not produce errors");
}

#[test]
fn call_unknown_emits_ts18046_with_strict() {
    let source = r#"
declare let x: unknown;
x();
"#;
    // TS18046: 'x' is of type 'unknown'
    assert!(
        has_error(source, 18046),
        "Calling unknown should emit TS18046"
    );
}

#[test]
fn call_never_returns_never() {
    let source = r#"
declare let f: never;
let result: string = f();
"#;
    // Calling never should emit TS2349 (not callable)
    assert!(
        has_error(source, 2349),
        "Calling never directly should emit TS2349"
    );
}

#[test]
fn call_error_type_no_cascade() {
    // When callee type is error, the call returns error without cascading TS2349
    let source = r#"
declare let x: never;
function f(y: string) {}
f(x);
"#;
    // Passing never to string should not error (never is assignable to anything)
    assert!(
        no_errors(source),
        "Passing never to any param type should not error"
    );
}

// ============================================================================
// Argument count checking (TS2554)
// ============================================================================

#[test]
fn too_many_arguments_ts2554() {
    let source = r#"
function f(x: number): void {}
f(1, 2);
"#;
    assert!(
        has_error(source, 2554),
        "Too many arguments should emit TS2554"
    );
}

#[test]
fn too_few_arguments_ts2554() {
    let source = r#"
function f(x: number, y: string): void {}
f(1);
"#;
    assert!(
        has_error(source, 2554),
        "Too few arguments should emit TS2554"
    );
}

#[test]
fn optional_params_no_error() {
    let source = r#"
function f(x: number, y?: string): void {}
f(1);
"#;
    assert!(
        no_errors(source),
        "Optional params should allow fewer arguments"
    );
}

// ============================================================================
// Argument type mismatch (TS2345)
// ============================================================================

#[test]
fn argument_type_mismatch_ts2345() {
    let source = r#"
function f(x: number): void {}
f("hello");
"#;
    assert!(
        has_error(source, 2345),
        "Passing string to number param should emit TS2345"
    );
}

#[test]
fn argument_subtype_no_error() {
    let source = r#"
function f(x: number | string): void {}
f(42);
"#;
    assert!(
        no_errors(source),
        "Passing subtype argument should not error"
    );
}

// ============================================================================
// Overload resolution
// ============================================================================

