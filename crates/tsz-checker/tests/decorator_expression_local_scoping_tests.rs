//! A `var`/parameter declared *inside* a decorator expression's own body
//! must stay resolvable to later references within that same body.
//!
//! `resolve_identifier_symbol_inner`'s decorator-owner filter
//! (`crates/tsz-checker/src/symbols/symbol_resolver.rs`) hides declarations
//! nested inside the decorated member (e.g. sibling class members, the
//! member's own parameters/body) from bare-identifier lookups made from
//! within a decorator expression, since decorators execute in the outer
//! scope before the member/class exists. That containment check is a plain
//! AST subtree test against the decorated member's node span — and a
//! decorator's own body is *also* a descendant of that span, so the filter
//! wrongly hid locals the decorator declares for itself too.
//!
//! Oracle: `typescript@7.0.2`, `conformance/decorators/class/decoratorChecksFunctionBodies.ts`.
//! tsc resolves `a` normally inside the decorator's own arrow-function body
//! and reports only the real `TS2345` argument mismatch; tsz reported a
//! spurious `TS2552`/`TS2304` for `a` instead (and lost the `TS2345`, since
//! resolution failure short-circuits argument checking).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn check(source: &str) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            experimental_decorators: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

// ── Positive: the exact oracle repro ─────────────────────────────────────────

#[test]
fn method_decorator_var_declared_and_used_in_same_body_resolves() {
    let source = r#"
function func(s: string): void {
}

class A {
    @((x, p, d) => {
        var a = 3;
        func(a);
        return d;
    })
    m() {

    }
}
"#;
    let codes = check(source);
    assert!(
        !codes.contains(&2304) && !codes.contains(&2552),
        "must NOT emit TS2304/TS2552 for `a` declared and used inside the same decorator body, got {codes:?}"
    );
    assert!(
        codes.contains(&2345),
        "expected TS2345 (number not assignable to string) once `a` resolves, got {codes:?}"
    );
}

// ── Renamed binder: not a hardcoded name match ───────────────────────────────

#[test]
fn method_decorator_renamed_var_declared_and_used_in_same_body_resolves() {
    let source = r#"
function consume(s: string): void {
}

class Widget {
    @((target, propertyKey, descriptor) => {
        var count = 7;
        consume(count);
        return descriptor;
    })
    render() {

    }
}
"#;
    let codes = check(source);
    assert!(
        !codes.contains(&2304) && !codes.contains(&2552),
        "must NOT emit TS2304/TS2552 for a renamed local declared and used inside the same decorator body, got {codes:?}"
    );
    assert!(
        codes.contains(&2345),
        "expected TS2345 once the renamed local resolves, got {codes:?}"
    );
}

// ── Parameter decorator variant ──────────────────────────────────────────────

#[test]
fn parameter_decorator_var_declared_and_used_in_same_body_resolves() {
    let source = r#"
function consume(s: string): void {
}

class Widget {
    render(@((target, key, index) => {
        var flag = 1;
        consume(flag);
    })() p: any) {

    }
}
"#;
    let codes = check(source);
    assert!(
        !codes.contains(&2304) && !codes.contains(&2552),
        "must NOT emit TS2304/TS2552 for a local declared and used inside a parameter decorator's own body, got {codes:?}"
    );
    assert!(
        codes.contains(&2345),
        "expected TS2345 once the local resolves, got {codes:?}"
    );
}

// ── Nested arrow inside the decorator ────────────────────────────────────────

#[test]
fn method_decorator_var_used_inside_nested_arrow_resolves() {
    let source = r#"
function consume(s: string): void {
}

class Widget {
    @((target, key, descriptor) => {
        var value = 9;
        const wrap = () => consume(value);
        wrap();
        return descriptor;
    })
    render() {

    }
}
"#;
    let codes = check(source);
    assert!(
        !codes.contains(&2304) && !codes.contains(&2552),
        "must NOT emit TS2304/TS2552 for a decorator-local used from a nested arrow in the same decorator, got {codes:?}"
    );
    assert!(
        codes.contains(&2345),
        "expected TS2345 once the nested-arrow reference resolves, got {codes:?}"
    );
}

// ── Negative: sibling class members must stay hidden from the decorator ─────

#[test]
fn method_decorator_cannot_see_sibling_class_member() {
    // Regression guard for the filter's original purpose: a decorator still
    // must NOT resolve another member of the class it decorates — decorators
    // execute before the class exists, so `helper` is genuinely out of scope
    // here, unlike a name the decorator declares for itself.
    let source = r#"
class Widget {
    @((target) => {
        return helper();
    })
    render() {

    }

    helper() {
        return 1;
    }
}
"#;
    let codes = check(source);
    // tsc reports "cannot find name" for `helper` here through whichever
    // specific variant applies (plain TS2304, a spelling-suggestion TS2552,
    // or — since `helper` really is an instance member of the enclosing
    // class, just not in scope yet — the more specific "did you mean the
    // instance member 'this.helper'" TS2663). Any of the three proves the
    // symbol did NOT silently resolve as a bare in-scope name.
    assert!(
        codes.contains(&2304) || codes.contains(&2552) || codes.contains(&2663),
        "expected `helper` to stay unresolved inside the decorator (TS2304/TS2552/TS2663) — sibling class members must not leak into decorator scope, got {codes:?}"
    );
}
