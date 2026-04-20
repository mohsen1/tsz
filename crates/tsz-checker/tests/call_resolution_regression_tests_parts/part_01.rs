#[test]
fn overload_selects_matching_signature() {
    let source = r#"
function f(x: number): number;
function f(x: string): string;
function f(x: any): any { return x; }
let result: number = f(42);
"#;
    assert!(
        no_errors(source),
        "Overload resolution should select matching signature"
    );
}

#[test]
fn overload_type_mismatch_ts2345() {
    let source = r#"
function f(x: number): number;
function f(x: string): string;
function f(x: any): any { return x; }
let result: number = f("hello");
"#;
    assert!(
        has_error(source, 2322),
        "Overload resolution with wrong return type assignment should emit TS2322"
    );
}

#[test]
fn overload_no_matching_signature_ts2769() {
    let source = r#"
function f(x: number): number;
function f(x: string): string;
function f(x: any): any { return x; }
f(true);
"#;
    assert!(
        has_error(source, 2769),
        "No matching overload should emit TS2769"
    );
}

#[test]
fn overload_different_param_counts() {
    let source = r#"
function f(): void;
function f(x: number): void;
function f(x?: any): void {}
f();
f(1);
"#;
    assert!(
        no_errors(source),
        "Overload with different param counts should work"
    );
}

// ============================================================================
// Property/method calls
// ============================================================================

#[test]
fn method_call_on_object() {
    let source = r#"
declare let obj: { greet(name: string): string };
let result: string = obj.greet("world");
"#;
    assert!(
        no_errors(source),
        "Method call on typed object should not error"
    );
}

#[test]
fn missing_method_ts2339() {
    let source = r#"
declare let obj: { greet(name: string): string };
obj.missing();
"#;
    assert!(
        has_error(source, 2339),
        "Calling non-existent method should emit TS2339"
    );
}

#[test]
fn method_wrong_arg_type() {
    let source = r#"
declare let obj: { add(x: number): number };
obj.add("hello");
"#;
    assert!(
        has_error(source, 2345),
        "Method call with wrong arg type should emit TS2345"
    );
}

// ============================================================================
// Optional chaining calls
// ============================================================================

#[test]
fn optional_chain_call_valid() {
    let source = r#"
declare let obj: { greet?(name: string): string } | undefined;
let result = obj?.greet?.("world");
"#;
    assert!(no_errors(source), "Optional chain call should not error");
}

#[test]
fn optional_chain_call_on_non_callable() {
    let source = r#"
declare let obj: { x: number } | undefined;
obj?.x();
"#;
    assert!(
        has_error(source, 2349),
        "Optional chain call on non-callable property should emit TS2349"
    );
}

// ============================================================================
// Union callee types
// ============================================================================

#[test]
fn union_callee_compatible_calls() {
    let source = r#"
declare let f: ((x: number) => void) | ((x: number) => void);
f(42);
"#;
    assert!(
        no_errors(source),
        "Union callee with compatible signatures should work"
    );
}

