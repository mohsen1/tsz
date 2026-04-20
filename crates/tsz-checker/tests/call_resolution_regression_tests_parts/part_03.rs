#[test]
fn overloaded_generic_call_contextually_types_inline_callback_after_inference() {
    let source = r#"
interface Collection<T> {
    length: number;
    add(x: T): void;
    remove(x: T): boolean;
}
interface Combinators {
    map<T, U>(c: Collection<T>, f: (x: T) => U): Collection<U>;
    map<T>(c: Collection<T>, f: (x: T) => any): Collection<any>;
}

declare var _: Combinators;
declare var c2: Collection<number>;

var rf1 = (x: number) => { return x.toFixed() };
var r1a = _.map(c2, (x) => { return x.toFixed() });
var r1b = _.map(c2, rf1);
var r5a = _.map<number, string>(c2, (x) => { return x.toFixed() });
var r5b = _.map<number, string>(c2, rf1);
"#;
    assert!(
        no_errors(source),
        "Overloaded generic call should infer from the collection argument before contextually typing the inline callback"
    );
}

// ============================================================================
// Spread arguments
// ============================================================================

#[test]
fn spread_arg_valid() {
    let source = r#"
function f(x: number, y: number): void {}
let args: [number, number] = [1, 2];
f(...args);
"#;
    assert!(
        no_errors(source),
        "Spread of tuple with correct types should work"
    );
}

// ============================================================================
// Property call with this context
// ============================================================================

#[test]
fn method_call_preserves_this_context() {
    let source = r#"
interface Obj {
    value: number;
    getValue(): number;
}
declare let obj: Obj;
let result: number = obj.getValue();
"#;
    assert!(
        no_errors(source),
        "Method call should preserve this context"
    );
}

// ============================================================================
// IIFE patterns
// ============================================================================

#[test]
fn iife_basic() {
    let source = r#"
let result = (function() { return 42; })();
"#;
    assert!(no_errors(source), "Basic IIFE should not error");
}

#[test]
fn arrow_iife() {
    let source = r#"
let result = (() => 42)();
"#;
    assert!(no_errors(source), "Arrow IIFE should not error");
}

// ============================================================================
// Query-boundary regression: generic call inference with application types
// ============================================================================

#[test]
fn generic_call_with_identity() {
    // Exercises generic call inference (application types) via query boundary.
    let source = r#"
declare function identity<T>(x: T): T;
let n: number = identity(42);
let s: string = identity("hello");
"#;
    assert!(
        no_errors(source),
        "Generic identity call should infer T correctly"
    );
}

#[test]
fn generic_overload_resolution_picks_correct_signature() {
    let source = r#"
declare function overloaded(x: string): string;
declare function overloaded(x: number): number;
let s: string = overloaded("hello");
let n: number = overloaded(42);
"#;
    assert!(
        no_errors(source),
        "Overload resolution should pick correct signature"
    );
}

#[test]
fn generic_overload_with_type_args() {
    let source = r#"
declare function create<T>(x: T): T;
declare function create<T>(x: T, y: T): T[];
let a: number = create<number>(1);
let b: number[] = create<number>(1, 2);
"#;
    assert!(
        no_errors(source),
        "Generic overloads with explicit type args should resolve"
    );
}

#[test]
fn property_call_on_generic_interface() {
    // Exercises application-type evaluation for interface method calls
    let source = r#"
interface Container<T> {
    get(): T;
    set(value: T): void;
}
declare let c: Container<number>;
let v: number = c.get();
c.set(42);
"#;
    assert!(
        no_errors(source),
        "Method call on generic interface should work"
    );
}

#[test]
fn deeply_any_callee_returns_any() {
    // Exercises is_type_deeply_any via query boundary
    let source = r#"
declare let f: any;
let result = f(1, 2, 3);
"#;
    assert!(
        no_errors(source),
        "Calling any-typed callee should return any without errors"
    );
}

