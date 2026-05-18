//! Regression tests for namespace-local types that shadow global lib types.
//!
//! TypeScript's lexical scoping means that a type declared inside a namespace
//! takes priority over any global lib type with the same name, even when the
//! namespace-local type is not exported. The structural rule: "When a type name
//! appears inside a namespace body, resolution must first check the namespace's
//! local scope before falling back to file-level or global lookup."
//!
//! Related conformance test:
//! `genericCallToOverloadedMethodWithOverloadedArguments.ts`

use std::sync::Arc;
use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};

fn diag_code_messages(source: &str) -> Vec<(u32, String)> {
    let libs = load_default_lib_files();
    assert!(!libs.is_empty(), "default lib files must be available");
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
        .into_iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn diag_codes(source: &str) -> Vec<u32> {
    diag_code_messages(source)
        .into_iter()
        .map(|(c, _)| c)
        .collect()
}

/// Single-overload testFunction assigned to single-overload then — no error.
#[test]
fn namespace_local_promise_single_overload_no_error() {
    let codes = diag_codes(
        r#"
namespace m1 {
    interface Promise<T> {
        then<U>(cb: (x: T) => Promise<U>): Promise<U>;
    }
    declare function testFunction(n: number): Promise<number>;
    declare var numPromise: Promise<number>;
    var newPromise = numPromise.then(testFunction);
}
"#,
    );
    assert!(
        codes.is_empty(),
        "namespace-local Promise.then(testFunction) with matching types must have no error; got: {codes:?}"
    );
}

/// Overloaded testFunction assigned to single-overload then — TS2345, not TS2769.
#[test]
fn namespace_local_promise_overloaded_arg_single_overload_then_ts2345() {
    let msgs = diag_code_messages(
        r#"
namespace m2 {
    interface Promise<T> {
        then<U>(cb: (x: T) => Promise<U>): Promise<U>;
    }
    declare function testFunction(n: number): Promise<number>;
    declare function testFunction(s: string): Promise<string>;
    declare var numPromise: Promise<number>;
    var newPromise = numPromise.then(testFunction);
}
"#,
    );
    let codes: Vec<u32> = msgs.iter().map(|(c, _)| *c).collect();
    assert!(
        codes.contains(&2345),
        "overloaded testFunction vs single-overload then should produce TS2345; got: {codes:?}"
    );
    // The error message must reference the namespace-local Promise type, not
    // the global lib's PromiseLike<string> union form.
    let msg = msgs
        .iter()
        .find(|(c, _)| *c == 2345)
        .map(|(_, m)| m.as_str())
        .unwrap_or("");
    assert!(
        !msg.contains("PromiseLike"),
        "TS2345 message must reference namespace-local Promise, not global lib PromiseLike; got: {msg}"
    );
    assert!(
        msg.contains("Promise"),
        "TS2345 message must reference the namespace-local Promise type; got: {msg}"
    );
}

/// Overloaded testFunction + overloaded then — TS2769, not TS2345.
#[test]
fn namespace_local_promise_overloaded_arg_overloaded_then_ts2769() {
    let codes = diag_codes(
        r#"
namespace m4 {
    interface Promise<T> {
        then<U>(cb: (x: T) => Promise<U>): Promise<U>;
        then<U>(cb: (x: T) => Promise<U>, error?: (error: any) => Promise<U>): Promise<U>;
    }
    declare function testFunction(n: number): Promise<number>;
    declare function testFunction(s: string): Promise<string>;
    declare var numPromise: Promise<number>;
    var newPromise = numPromise.then(testFunction);
}
"#,
    );
    assert!(
        codes.contains(&2769),
        "overloaded testFunction vs overloaded then should produce TS2769; got: {codes:?}"
    );
    assert!(
        !codes.contains(&2345),
        "TS2345 must not fire when TS2769 applies; got: {codes:?}"
    );
}

/// Single-overload testFunction + overloaded then — no error.
#[test]
fn namespace_local_promise_single_arg_overloaded_then_no_error() {
    let codes = diag_codes(
        r#"
namespace m3 {
    interface Promise<T> {
        then<U>(cb: (x: T) => Promise<U>): Promise<U>;
        then<U>(cb: (x: T) => Promise<U>, error?: (error: any) => Promise<U>): Promise<U>;
    }
    declare function testFunction(n: number): Promise<number>;
    declare var numPromise: Promise<number>;
    var newPromise = numPromise.then(testFunction);
}
"#,
    );
    assert!(
        codes.is_empty(),
        "namespace-local single-arg vs overloaded-then must have no error; got: {codes:?}"
    );
}

/// Verify the fix also works with renamed type parameters (K instead of T/U).
/// If the fix hardcoded type-parameter names it would fail here.
#[test]
fn namespace_local_type_param_name_independence() {
    let codes = diag_codes(
        r#"
namespace renamed {
    interface Box<K> {
        map<V>(cb: (x: K) => Box<V>): Box<V>;
    }
    declare function id(n: number): Box<number>;
    declare var b: Box<number>;
    var result = b.map(id);
}
"#,
    );
    assert!(
        codes.is_empty(),
        "namespace-local generic type with non-standard parameter names must resolve correctly; got: {codes:?}"
    );
}

/// Nested namespace: the inner namespace's local type must take priority.
#[test]
fn nested_namespace_local_type_priority() {
    let codes = diag_codes(
        r#"
namespace outer {
    namespace inner {
        interface Promise<T> {
            then<U>(cb: (x: T) => Promise<U>): Promise<U>;
        }
        declare function testFn(n: number): Promise<number>;
        declare var p: Promise<number>;
        var result = p.then(testFn);
    }
}
"#,
    );
    assert!(
        codes.is_empty(),
        "nested namespace-local Promise must shadow global Promise; got: {codes:?}"
    );
}
