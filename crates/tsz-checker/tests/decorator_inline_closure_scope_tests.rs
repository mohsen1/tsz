//! Tests for local-variable resolution inside an inline function-literal
//! decorator expression (`@((x, p, d) => { ...here... })`).
//!
//! Structural rule: a decorator expression that is itself a function literal
//! creates its own self-contained lexical scope, exactly like any other
//! function expression. tsc resolves identifiers inside that scope with
//! ordinary lexical rules and fully checks the body (including argument
//! types and callability) — it does not extend "decorators execute in the
//! surrounding scope" leniency into a nested function's own body. tsz did,
//! through two checker helpers (`is_in_decorator_expression` and
//! `node_is_within_decorator_owner`, `crates/tsz-checker/src/symbols/scope_finder.rs`)
//! that walked all the way from a reference/declaration up to the nearest
//! `DECORATOR` ancestor without stopping at an intervening function-like
//! boundary. A local var/let declared inside the decorator's own function
//! body was then wrongly treated as "declared inside the decorated member"
//! and filtered out of scope-chain lookup, so referencing it two lines later
//! in the very same function body fell through to the unresolved-identifier
//! path and reported a spurious `TS2552` ("Cannot find name '<x>'. Did you
//! mean '<SimilarlyCasedSibling>'?") instead of the real diagnostic.

use tsz_checker::test_utils::{check_source_codes, check_source_codes_experimental_decorators};

/// Assert the decorator's own `var`/`let` resolved to the real `TS2345`
/// number-vs-string mismatch, with no spurious `TS2552`/`TS2304` unresolved-
/// identifier diagnostic. The no-lib unit harness (`CheckerOptions::default()`,
/// no `TypedPropertyDescriptor`/`ClassMethodDecoratorContext`) can add its own
/// unrelated `TS2318`/`TS2304` "missing global type" noise, so this checks the
/// specific signal rather than the full code list.
fn assert_resolves_to_real_mismatch(codes: &[u32]) {
    assert!(
        codes.contains(&2345),
        "expected the real number-vs-string TS2345 mismatch; got: {codes:?}"
    );
    assert!(
        !codes.contains(&2552),
        "the decorator's own local var/let should resolve normally, not fall through to a spurious \"did you mean\" TS2552; got: {codes:?}"
    );
}

#[test]
fn legacy_method_decorator_inline_arrow_resolves_local_var() {
    let codes = check_source_codes_experimental_decorators(
        r#"
function func(s: string): void {}
class A {
    @((x, p, d) => {
        var a = 3;
        func(a);
        return d;
    })
    m() {}
}
"#,
    );
    assert_resolves_to_real_mismatch(&codes);
}

#[test]
fn legacy_method_decorator_inline_arrow_resolves_local_let() {
    let codes = check_source_codes_experimental_decorators(
        r#"
function func(s: string): void {}
class A {
    @((x, p, d) => {
        let a = 3;
        func(a);
        return d;
    })
    m() {}
}
"#,
    );
    assert_resolves_to_real_mismatch(&codes);
}

#[test]
fn legacy_property_decorator_inline_arrow_resolves_local_var() {
    let codes = check_source_codes_experimental_decorators(
        r#"
function func(s: string): void {}
class A {
    @((x, p) => {
        var a = 3;
        func(a);
    })
    m = 1;
}
"#,
    );
    assert_resolves_to_real_mismatch(&codes);
}

#[test]
fn legacy_accessor_decorator_inline_arrow_resolves_local_var() {
    let codes = check_source_codes_experimental_decorators(
        r#"
function func(s: string): void {}
class A {
    @((x, p, d) => {
        var a = 3;
        func(a);
        return d;
    })
    get m() { return 1; }
}
"#,
    );
    assert_resolves_to_real_mismatch(&codes);
}

#[test]
fn legacy_class_decorator_inline_arrow_resolves_local_var() {
    let codes = check_source_codes_experimental_decorators(
        r#"
function func(s: string): void {}
@((t) => {
    var a = 3;
    func(a);
    return t;
})
class A {}
"#,
    );
    assert_resolves_to_real_mismatch(&codes);
}

#[test]
fn es_decorator_inline_arrow_resolves_local_var() {
    // Same shape without `experimentalDecorators` (TC39 Stage-3 decorators).
    let codes = check_source_codes(
        r#"
function func(s: string): void {}
class A {
    @((x: unknown, c: ClassMethodDecoratorContext) => {
        var a = 3;
        func(a);
        return x;
    })
    m() {}
}
"#,
    );
    assert_resolves_to_real_mismatch(&codes);
}

#[test]
fn renamed_binders_still_resolve_inside_decorator_closure() {
    // Same shape with different identifier names throughout, guarding
    // against a fix that accidentally keys off the specific names used in
    // the original repro (`a`, `A`, `func`).
    let codes = check_source_codes_experimental_decorators(
        r#"
function needsString(input: string): void {}
class Widget {
    @((ctx, prop, descriptor) => {
        var count = 42;
        needsString(count);
        return descriptor;
    })
    render() {}
}
"#,
    );
    assert_resolves_to_real_mismatch(&codes);
}

#[test]
fn named_factory_decorator_still_resolves_local_var() {
    // Negative/control case: a decorator that references a NAMED,
    // separately-declared function (not an inline literal) was never
    // affected by the bug — it must keep working after the fix.
    let codes = check_source_codes_experimental_decorators(
        r#"
function func(s: string): void {}
function makeDecorator(x: any, p: any, d: any) {
    var a = 3;
    func(a);
    return d;
}
class A {
    @makeDecorator
    m() {}
}
"#,
    );
    assert_resolves_to_real_mismatch(&codes);
}

#[test]
fn bare_decorator_argument_still_excludes_sibling_member() {
    // Positive control for the original exclusion this code exists for:
    // a bare identifier that is DIRECTLY the decorator's own argument (not
    // nested inside a further function) must still fail to see a
    // same-named declaration that lives inside the decorated member's own
    // body — decorators genuinely execute before that body exists.
    let codes = check_source_codes_experimental_decorators(
        r#"
declare function dec(v: unknown): MethodDecorator;
class A {
    @dec(localOnlyInM)
    m() {
        var localOnlyInM = 1;
    }
}
"#,
    )
    .to_vec();

    assert!(
        codes.contains(&2304) || codes.contains(&2552) || codes.contains(&2663),
        "a decorator argument referencing a name declared only inside the decorated \
         method's own body must still fail to resolve to it; got: {codes:?}"
    );
}
