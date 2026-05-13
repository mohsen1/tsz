//! Tests for TS1241 (method/accessor decorator signature mismatch).
//!
//! Structural rule:
//! > When `experimentalDecorators` is on and a method/accessor-position
//! > decorator's resolved signature cannot be called with the runtime
//! > convention `(target: any, propertyKey: string, descriptor: any)`,
//! > the checker emits TS1241.
//!
//! TS1329 is emitted instead when the decorator has only zero-parameter
//! call signatures (looks like an uninvoked factory: `@dec()` not `@dec`).

use tsz_checker::test_utils::check_source_codes_experimental_decorators;

fn check(source: &str) -> Vec<u32> {
    check_source_codes_experimental_decorators(source).to_vec()
}

// =========================================================================
// Table-driven helpers shared across method / getter / setter
// =========================================================================

/// Wraps `decorator_decl` as a method decorator, getter decorator, and setter
/// decorator in turn and asserts that `expected_code` appears in every case.
fn assert_code_for_all_member_kinds(decorator_decl: &str, expected_code: u32) {
    for (label, member) in [
        ("method", "method() {}"),
        ("getter", "get value() { return 1; }"),
        ("setter", "set value(v: number) {}"),
    ] {
        let src = format!("{decorator_decl}\nclass C {{ @dec {member} }}\n");
        let diags = check(&src);
        assert!(
            diags.contains(&expected_code),
            "Expected {expected_code} on {label}; got: {diags:?}",
        );
    }
}

/// Same as above but asserts `expected_code` is ABSENT in every case.
fn assert_no_code_for_all_member_kinds(decorator_decl: &str, expected_code: u32) {
    for (label, member) in [
        ("method", "method() {}"),
        ("getter", "get value() { return 1; }"),
        ("setter", "set value(v: number) {}"),
    ] {
        let src = format!("{decorator_decl}\nclass C {{ @dec {member} }}\n");
        let diags = check(&src);
        assert!(
            !diags.contains(&expected_code),
            "Expected NO {expected_code} on {label}; got: {diags:?}",
        );
    }
}

// =========================================================================
// TS1241 — incompatible signature across all member kinds
// =========================================================================

/// 1-param decorator: runtime passes 3 args but max is 1 → TS1241.
#[test]
fn one_param_decorator_emits_ts1241_on_all_member_kinds() {
    assert_code_for_all_member_kinds("function dec(target: any) {}", 1241);
}

/// 2-param decorator: runtime passes 3 args but max is 2 → TS1241.
#[test]
fn two_param_decorator_emits_ts1241_on_all_member_kinds() {
    assert_code_for_all_member_kinds("function dec(target: any, key: string) {}", 1241);
}

/// Incompatible `propertyKey` type: `number` rejects a `string` argument → TS1241.
#[test]
fn incompatible_key_type_emits_ts1241_on_all_member_kinds() {
    assert_code_for_all_member_kinds(
        "function dec(target: any, key: number, descriptor: any) {}",
        1241,
    );
}

/// Anti-hardcoding: the structural rule must not depend on the spelling of
/// the decorator or class — different names, same shape must behave identically.
#[test]
fn ts1241_independent_of_decorator_and_class_names() {
    let diags = check(
        "function myDecorator(t: any, k: number, d: any) {}\n\
         class MyService { @myDecorator doWork() {} }\n",
    );
    assert!(
        diags.contains(&1241),
        "TS1241 must fire regardless of decorator/class spelling; got: {diags:?}",
    );
}

// =========================================================================
// Compatible signatures — no TS1241 across all member kinds
// =========================================================================

/// Fully compatible 3-param decorator: no error.
#[test]
fn compatible_3_param_decorator_no_ts1241_on_all_member_kinds() {
    assert_no_code_for_all_member_kinds(
        "function dec(target: any, key: string, descriptor: any) {}",
        1241,
    );
}

/// `any`-typed decorator: no error.
#[test]
fn any_typed_decorator_no_ts1241_on_all_member_kinds() {
    assert_no_code_for_all_member_kinds("declare const dec: any;", 1241);
}

/// Rest-param decorator `(...args: any[]) => void` accepts unlimited args: no error.
#[test]
fn rest_param_decorator_no_ts1241_on_all_member_kinds() {
    assert_no_code_for_all_member_kinds("function dec(...args: any[]) {}", 1241);
}

/// Optional extra params beyond 3 are fine: `(target, key, desc?, extra?)`.
#[test]
fn optional_params_decorator_no_ts1241_on_all_member_kinds() {
    assert_no_code_for_all_member_kinds(
        "function dec(target: any, key: string, desc?: any, extra?: any) {}",
        1241,
    );
}

// =========================================================================
// TS1329 — zero-param factory hint (not TS1241)
// =========================================================================

/// Zero-param decorator on a method: tsc suggests calling it → TS1329, NOT TS1241.
#[test]
fn zero_param_decorator_on_method_emits_ts1329_not_ts1241() {
    let diags = check(
        "function dec() {}\n\
         class C { @dec method() {} }\n",
    );
    assert!(
        diags.contains(&1329),
        "Zero-param decorator on method must emit TS1329; got: {diags:?}",
    );
    assert!(
        !diags.contains(&1241),
        "Zero-param decorator must NOT also emit TS1241; got: {diags:?}",
    );
}

// =========================================================================
// experimentalDecorators OFF — TS1241 must not fire
// =========================================================================

/// Without experimentalDecorators, the classic TS1241 path is not active.
#[test]
fn no_ts1241_without_experimental_decorators() {
    use tsz_checker::test_utils::check_source_codes;
    let diags = check_source_codes(
        "function dec(target: any) {}\n\
         class C { @dec method() {} }\n",
    )
    .to_vec();
    assert!(
        !diags.contains(&1241),
        "TS1241 must not fire without --experimentalDecorators; got: {diags:?}",
    );
}
