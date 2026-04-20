#[test]
fn union_callee_incompatible_param_count() {
    let source = r#"
declare let f: ((x: number) => void) | ((x: number, y: string) => void);
f(42);
"#;
    // Union call requires valid for ALL members - missing arg for second member
    assert!(
        has_error(source, 2554) || has_error(source, 2769),
        "Union callee with incompatible param counts should error"
    );
}

// ============================================================================
// Super calls
// ============================================================================

#[test]
fn super_call_returns_void() {
    // super() is treated as a construct call that returns void
    let source = r#"
class Base {
    constructor(x: number) {}
}
class Derived extends Base {
    constructor() {
        super(42);
    }
}
"#;
    assert!(
        no_errors(source),
        "Basic super call with correct args should not error"
    );
}

// ============================================================================
// Type argument validation (TS2558, TS2344)
// ============================================================================

#[test]
fn too_many_type_arguments_ts2558() {
    let source = r#"
function f<T>(x: T): T { return x; }
f<number, string>(42);
"#;
    assert!(
        has_error(source, 2558),
        "Too many type arguments should emit TS2558"
    );
}

#[test]
fn untyped_call_with_type_args_ts2347() {
    let source = r#"
declare let f: any;
f<number>(42);
"#;
    assert!(
        has_error(source, 2347),
        "Untyped function call with type args should emit TS2347"
    );
}

// ============================================================================
// Generic overload resolution
// ============================================================================

#[test]
fn generic_overload_selects_correct_signature() {
    let source = r#"
function id<T>(x: T): T;
function id<T, U>(x: T, y: U): [T, U];
function id(...args: any[]): any { return args[0]; }
let result: number = id(42);
"#;
    assert!(
        no_errors(source),
        "Generic overload should select matching signature"
    );
}

#[test]
fn generic_call_infers_type_param() {
    let source = r#"
function id<T>(x: T): T { return x; }
let result: number = id(42);
"#;
    assert!(
        no_errors(source),
        "Generic call should infer T=number from argument"
    );
}

#[test]
fn generic_call_explicit_type_arg() {
    let source = r#"
function id<T>(x: T): T { return x; }
let result: number = id<number>(42);
"#;
    assert!(
        no_errors(source),
        "Generic call with explicit type arg should work"
    );
}

#[test]
fn generic_call_explicit_type_arg_mismatch() {
    let source = r#"
function id<T>(x: T): T { return x; }
id<number>("hello");
"#;
    assert!(
        has_error(source, 2345),
        "Generic call with explicit type arg and wrong arg should emit TS2345"
    );
}

// ============================================================================
// Contextual callback typing through calls
// ============================================================================

#[test]
fn callback_param_contextually_typed() {
    let source = r#"
declare function map<T, U>(arr: T[], fn: (x: T) => U): U[];
declare let nums: number[];
let result = map(nums, x => x + 1);
"#;
    assert!(
        no_errors(source),
        "Callback param should be contextually typed from generic"
    );
}

#[test]
fn callback_return_type_inferred() {
    let source = r#"
declare function apply<T>(fn: () => T): T;
let result: number = apply(() => 42);
"#;
    assert!(
        no_errors(source),
        "Callback return type should contribute to generic inference"
    );
}

