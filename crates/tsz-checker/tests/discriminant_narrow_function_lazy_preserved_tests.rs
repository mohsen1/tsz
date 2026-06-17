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

/// Regression: `typeof x === 'function'` narrowing over a *generic type-alias
/// instantiation* whose alias resolves to a union containing a call signature.
/// `narrow_to_function` inspected the raw `Application`/`Lazy` wrapper, which
/// `union_list_id` does not recognize as a union, so the call-signature member
/// was dropped and the result was no longer callable (spurious TS2349) and the
/// non-function member rendered as a lost-member union (spurious TS2322).
///
/// Mirrors `tanstack-query`'s `resolveStaleTime`/`resolveQueryBoolean`:
/// `typeof option === 'function' ? option(query) : option` over
/// `option: undefined | Fn<T>`.
#[test]
fn typeof_function_narrows_generic_alias_union_member() {
    let source = r#"
type Fn<T> = number | ((x: T) => number)
function resolve<T>(option: undefined | Fn<T>, x: T): number | undefined {
    return typeof option === 'function' ? option(x) : option
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    assert!(
        diags.is_empty(),
        "Expected no diagnostics narrowing a generic-alias union, got: {diags:#?}"
    );
}

/// Adjacent: the same alias-union reached *directly* (not behind an outer
/// `undefined`), narrowed in an `if` block rather than a ternary, with the
/// negative (`else`) branch keeping only the non-function members.
#[test]
fn typeof_function_narrows_generic_alias_if_and_else_branches() {
    let source = r#"
type Fn<T> = number | ((x: T) => number)
function resolve<T>(option: Fn<T>, x: T): number {
    if (typeof option === 'function') {
        return option(x)
    }
    return option
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    assert!(
        diags.is_empty(),
        "Expected no diagnostics on if/else alias-union narrowing, got: {diags:#?}"
    );
}

/// Adjacent: a concrete instantiation `Fn<string>` (no free type parameter)
/// must narrow identically to the generic form — the bug was in the wrapper
/// handling, not the type parameter.
#[test]
fn typeof_function_narrows_concrete_alias_instantiation() {
    let source = r#"
type Fn<T> = number | ((x: T) => number)
function resolve(option: Fn<string>, x: string): number {
    if (typeof option === 'function') { return option(x) }
    return option
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    assert!(
        diags.is_empty(),
        "Expected no diagnostics on concrete alias instantiation, got: {diags:#?}"
    );
}
