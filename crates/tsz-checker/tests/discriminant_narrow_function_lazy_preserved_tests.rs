use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

/// Regression: when a property-discriminant narrows a non-union source whose
/// type is a `Lazy(DefId)` reference to the global `Function` interface, the
/// narrowing must preserve the original Lazy form. Otherwise downstream
/// `typeof === "function"` narrowing on the result fails to recognize the
/// resolved Object shape as callable and collapses to `never`, producing a
/// spurious TS2339 on subsequent property access.
///
/// Mirrors the failing pattern in
/// `tests/cases/compiler/typeGuardConstructorClassAndNumber.ts` where
/// `instance.prototype.constructor` after `instance.prototype == null` was
/// reporting "Property 'prototype' does not exist on type 'never'".
#[test]
fn typeof_function_then_property_discriminant_keeps_function() {
    let source = r#"
function f(instance: Function | object) {
    if (typeof instance === 'function') {
        if (instance.prototype == null) {
            return;
        }
        instance.prototype;
        instance.prototype.constructor;
    }
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let ts2339: Vec<_> = diags.iter().filter(|d| d.code == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 after typeof+property-discriminant narrowing, got: {diags:#?}"
    );
}

/// Same root-cause as above, but expressed via the OR pattern from the
/// `typeGuardConstructorClassAndNumber.ts` repro (#37660). The right operand
/// of `||` evaluates `instance.prototype.constructor` after the left
/// operand's false branch narrows `instance` by `prototype != null`.
#[test]
fn typeof_function_or_property_discriminant_keeps_function() {
    let source = r#"
function f(instance: Function | object) {
    if (typeof instance === 'function') {
        if (instance.prototype == null || instance.prototype.constructor == null) {
            return instance.length;
        }
    }
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let ts2339: Vec<_> = diags.iter().filter(|d| d.code == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 in OR-chain narrowing, got: {diags:#?}"
    );
}

/// Variant: `Function | string` to confirm the fix isn't specific to
/// `object`. Any union containing the `Function` Lazy must preserve it
/// after discriminant narrowing.
#[test]
fn typeof_function_then_property_discriminant_function_or_string() {
    let source = r#"
function f(instance: Function | string) {
    if (typeof instance === 'function') {
        if (instance.prototype == null) {
            return;
        }
        instance.prototype.constructor;
    }
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let ts2339: Vec<_> = diags.iter().filter(|d| d.code == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 with Function | string, got: {diags:#?}"
    );
}
