//! Tests for TS1239 (parameter decorator signature mismatch).
//!
//! Closes #5890. The structural rule:
//!
//! > When `experimentalDecorators` is on and a parameter-position
//! > decorator's resolved signature does not accept the runtime calling
//! > convention `(target, key, parameterIndex)` — with `key = undefined`
//! > for constructor parameters and `key = string` for method/accessor
//! > parameters — the checker emits TS1239 at the decorator's anchor.
//!
//! tsc reference: `Unable to resolve signature of parameter decorator
//! when called as an expression.`

use tsz_checker::test_utils::{check_source_codes, check_source_codes_experimental_decorators};

fn check(source: &str) -> Vec<u32> {
    check_source_codes_experimental_decorators(source).to_vec()
}

// =========================================================================
// Constructor parameter — rejects `key: string`
// =========================================================================

/// Direct repro from #5890. `required(target, propertyKey: string, idx)`
/// applied to a constructor param: tsc passes `undefined` for `propertyKey`,
/// which doesn't fit `string` → TS1239.
#[test]
fn constructor_param_decorator_with_string_key_emits_ts1239() {
    let diags = check(
        "function required(target: Object, propertyKey: string, parameterIndex: number) { }\n\
         class BugReport {\n\
             constructor(@required title: string) {}\n\
         }\n",
    );
    assert!(
        diags.contains(&1239),
        "Constructor param decorator with `key: string` must emit TS1239; got: {diags:?}",
    );
}

/// .claude/CLAUDE.md §25 anti-hardcoding: the structural rule must not depend on
/// the spelling of the decorator function or the class. Same shape,
/// different names.
#[test]
fn constructor_param_decorator_ts1239_independent_of_names() {
    let diags = check(
        "function inject(t: Object, k: string, i: number) { }\n\
         class Service {\n\
             constructor(@inject value: number) {}\n\
         }\n",
    );
    assert!(
        diags.contains(&1239),
        "Constructor param decorator with `key: string` must emit TS1239 \
         regardless of function/class names; got: {diags:?}",
    );
}

// =========================================================================
// Constructor parameter — accepts the runtime shape: no TS1239
// =========================================================================

/// A decorator that DOES accept the runtime calling convention for a
/// constructor parameter (`key` typed as `string | symbol | undefined` or
/// `any`) must NOT emit TS1239.
#[test]
fn constructor_param_decorator_with_compatible_signature_no_ts1239() {
    let diags = check(
        "function required(target: Object, propertyKey: string | symbol | undefined, parameterIndex: number) { }\n\
         class BugReport {\n\
             constructor(@required title: string) {}\n\
         }\n",
    );
    assert!(
        !diags.contains(&1239),
        "Compatible constructor param decorator must NOT emit TS1239; got: {diags:?}",
    );
}

// =========================================================================
// Method parameter — accepts `key: string`
// =========================================================================

/// For METHOD parameters tsc passes the method name as a string, so a
/// decorator typed `key: string` is fine — TS1239 should NOT fire.
#[test]
fn method_param_decorator_with_string_key_no_ts1239() {
    let diags = check(
        "function required(target: Object, propertyKey: string, parameterIndex: number) { }\n\
         class BugReport {\n\
             setTitle(@required title: string) {}\n\
         }\n",
    );
    assert!(
        !diags.contains(&1239),
        "Method param decorator with `key: string` must NOT emit TS1239; got: {diags:?}",
    );
}

// =========================================================================
// experimentalDecorators OFF — the gate disables TS1239
// =========================================================================

/// TS1239 is only meaningful under `experimentalDecorators`. With the flag
/// off the parameter-decorator runtime ABI is the stage-3 shape, so the
/// classic check must not run. (The classic-only diagnostic TS1206
/// "decorators are not valid here" handles the experimentalDecorators=off
/// case instead.)
#[test]
fn no_ts1239_without_experimental_decorators() {
    // Note: we use the default (experimental_decorators=false) here to
    // confirm the gate suppresses TS1239 entirely. TS1206 may still fire.
    let diags = check_source_codes(
        "function required(target: Object, propertyKey: string, parameterIndex: number) { }\n\
         class BugReport {\n\
             constructor(@required title: string) {}\n\
         }\n",
    )
    .to_vec();
    assert!(
        !diags.contains(&1239),
        "TS1239 must not fire without --experimentalDecorators; got: {diags:?}",
    );
}
