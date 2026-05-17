//! Tests for generic call type inference ordering:
//! non-contextual (direct) argument inference must take priority over
//! contextual (callback return type) inference.

use tsz_checker::test_utils::check_source_code_messages;

/// Assert that `source` produces no diagnostics, panicking with `context` if it does.
#[track_caller]
fn assert_no_errors(source: &str, context: &str) {
    let diags = check_source_code_messages(source);
    assert!(diags.is_empty(), "{context}; got: {diags:?}");
}

/// foo3<T,U>(x: T, cb: (a: T) => U, y: U)
/// called as foo3(1, function(a) { return ''; }, 1)
///
/// Round 1: y=1 infers U (`NakedTypeVariable` priority)
/// Round 2: callback return '' infers U (`ReturnType` priority)
///
/// tsc: U=1 from y wins, callback fails as (a:number)=>string vs (a:number)=>1
/// (error at callback position, not at y position)
#[test]
fn test_foo3_direct_arg_u_wins_over_callback_return() {
    let source = r#"
function foo3<T, U>(x: T, cb: (a: T) => U, y: U) {
    return cb(x);
}
var r8 = foo3(1, function (a) { return ''; }, 1);
"#;
    let diags = check_source_code_messages(source);
    // One error: TS2345
    let ts2345: Vec<_> = diags.iter().filter(|(code, _)| *code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expected exactly one TS2345, got: {diags:?}"
    );
    let (_, msg) = ts2345[0];
    // The error should be about the callback not matching the expected signature,
    // NOT about y=1 failing against ""
    assert!(
        !msg.contains("parameter of type '\"\"'"),
        "Error should not be about y=1 vs empty string; got: {msg}",
    );
    assert!(
        msg.contains("parameter of type '(a: number) => 1'"),
        "Expected direct literal inference to preserve callback target return display; got: {msg}",
    );
}

/// When there is NO conflicting direct argument, callback return type infers U
/// foo2<T,U>(x: T, cb: (a: T) => U)
/// called as foo2(1, (a) => '') - no error, U=string
#[test]
fn test_foo2_no_y_no_error() {
    assert_no_errors(
        r#"
function foo2<T, U>(x: T, cb: (a: T) => U) {
    return cb(x);
}
var r4 = foo2(1, function(a) { return ''; });
"#,
        "expected no errors for foo2",
    );
}

/// Parameterless lambda: infer T from `() => 'hi'` against `() => T` (direct function type)
/// Round 1 should pick up T=string from context-free `() => 'hi'`
/// so that the context-sensitive `n => n.length` has n: string, not n: unknown.
#[test]
fn test_parameterless_lambda_direct_function_type_infers_t() {
    assert_no_errors(
        r#"
function foo2<T>(o: (n: T) => void, i: () => T): void {}
foo2(n => n.length, () => 'hi');
"#,
        "expected no errors when inferring T from parameterless lambda with direct fn type",
    );
}

/// Simple case: infer T from `() => 'hi'` alone against interface `Make<T> { (): T }`
#[test]
fn test_parameterless_lambda_simple_interface_infers_t() {
    assert_no_errors(
        r#"
interface Make<T> { (): T; }
function bar<T>(i: Make<T>): T { return null!; }
var r = bar(() => 'hi');
"#,
        "expected no errors for simple interface application inference",
    );
}

/// Verifies that T is inferred as string (not unknown) from `() => 'hi'` against `Make<T>`.
/// If T is correctly inferred as string (widened from "hi"), then `x: string` is valid;
/// if T = unknown, assigning the result to `string` would fail.
#[test]
fn test_parameterless_lambda_simple_interface_infers_correct_type() {
    assert_no_errors(
        r#"
interface Make<T> { (): T; }
function bar<T>(i: Make<T>): T { return null!; }
const x: string = bar(() => 'hi');
"#,
        "expected no errors - T should be string from () => 'hi' against Make<T>",
    );
}

/// Mixed: direct function type for sensitive arg, interface application for parameterless lambda.
/// Isolates whether the issue is with interface Application in two-pass when Take<T> is direct.
#[test]
fn test_parameterless_lambda_mixed_direct_and_interface() {
    assert_no_errors(
        r#"
function foo<T>(o: (n: T) => void, i: () => T): void {}
foo(n => n.length, () => 'hi');
"#,
        "expected no errors for direct fn types",
    );
}

/// Mixed: Take interface for sensitive arg, direct function type for parameterless lambda.
#[test]
fn test_two_pass_take_interface_direct_make() {
    assert_no_errors(
        r#"
interface Take<T> { (n: T): void; }
function foo<T>(o: Take<T>, i: () => T): void {}
foo(n => n.length, () => 'hi');
"#,
        "expected no errors for Take<T> interface + direct () => T",
    );
}

/// Mixed: direct sensitive arg, Make interface for parameterless lambda.
#[test]
fn test_two_pass_direct_take_make_interface() {
    assert_no_errors(
        r#"
interface Make<T> { (): T; }
function foo<T>(o: (n: T) => void, i: Make<T>): void {}
foo(n => n.length, () => 'hi');
"#,
        "expected no errors for direct (n: T) => void + Make<T> interface",
    );
}

/// Type alias version: same as interface but uses `type Make<T> = () => T`.
/// This isolates whether the issue is interface-specific.
#[test]
fn test_two_pass_direct_take_make_type_alias() {
    assert_no_errors(
        r#"
type Make<T> = () => T;
function foo<T>(o: (n: T) => void, i: Make<T>): void {}
foo(n => n.length, () => 'hi');
"#,
        "expected no errors for type alias Make<T> + direct callback",
    );
}

/// Parameterless lambda: infer T from `() => 'hi'` against interface `Make<T> { (): T }`
/// This is the interface-wrapped version of the same inference.
#[test]
fn test_parameterless_lambda_interface_application_infers_t() {
    assert_no_errors(
        r#"
interface Make<T> { (): T; }
interface Take<T> { (n: T): void; }
function foo<T>(o: Take<T>, i: Make<T>) { }
foo(n => n.length, () => 'hi');
"#,
        "expected no errors when inferring T from parameterless lambda against interface Application",
    );
}

/// Same pattern as `test_two_pass_direct_take_make_interface` but with a renamed
/// type parameter (`V` instead of `T`) and different interface names (`Produce<V>`
/// instead of `Make<T>`). Verifies the fix is structural, not name-specific.
#[test]
fn test_two_pass_renamed_type_param_interface_application_infers_v() {
    assert_no_errors(
        r#"
interface Produce<V> { (): V; }
function bar<V>(o: (n: V) => void, i: Produce<V>): void {}
bar(n => n.length, () => 'hi');
"#,
        "expected no errors for renamed type param V with Produce<V> interface",
    );
}

/// Same pattern with multiple type params and renamed variables to confirm
/// the fix handles multi-param generics and different naming conventions.
#[test]
fn test_two_pass_multi_param_interface_application_infers_k() {
    assert_no_errors(
        r#"
interface Emit<K> { (): K; }
function baz<K, U>(o: (n: K) => U, i: Emit<K>): U { return null!; }
baz(n => n.length, () => 'hi');
"#,
        "expected no errors for multi-param generic with Emit<K> interface",
    );
}
