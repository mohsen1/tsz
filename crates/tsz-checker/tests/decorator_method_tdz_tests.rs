//! TS2448/TS2450: block-scoped variables and enums used before their declaration
//! in method and parameter decorators must trigger TDZ diagnostics.
//!
//! Method decorators (`@dec` on a method/accessor) and parameter decorators
//! (`@dec` on a method parameter) execute at class-definition time, before the
//! class body initializers. Therefore a `const`/`let`/`enum` that is textually
//! declared after the class is in TDZ at the time the decorator argument is
//! evaluated.
//!
//! Regression: the walk in `is_class_or_enum_used_before_declaration` bailed
//! out when it encountered the `METHOD_DECLARATION` function-like boundary,
//! suppressing TS2448/TS2450 for all method and parameter decorator positions.

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

// ── Method decorator: const used before declaration ─────────────────────────

#[test]
fn method_decorator_const_before_declaration_emits_ts2448() {
    let source = r#"
class C {
    @lambda
    greet() {}
}
const lambda = (...args: any[]): any => {};
"#;
    let codes = check(source);
    assert!(
        codes.contains(&2448),
        "expected TS2448 for `lambda` in method decorator used before declaration, got {codes:?}"
    );
}

#[test]
fn method_decorator_factory_const_before_declaration_emits_ts2448() {
    let source = r#"
class C {
    @lambda(1)
    greet() {}
}
const lambda = (...args: any[]): any => {};
"#;
    let codes = check(source);
    assert!(
        codes.contains(&2448),
        "expected TS2448 for `lambda(1)` method decorator before declaration, got {codes:?}"
    );
}

// ── Method decorator: enum used before declaration ───────────────────────────

#[test]
fn method_decorator_enum_before_declaration_emits_ts2450() {
    let source = r#"
declare const dec: any;
class C {
    @dec(E.A)
    greet() {}
}
enum E { A = 0 }
"#;
    let codes = check(source);
    assert!(
        codes.contains(&2450),
        "expected TS2450 for `E` in method decorator used before declaration, got {codes:?}"
    );
}

// ── Parameter decorator: const used before declaration ───────────────────────

#[test]
fn parameter_decorator_const_before_declaration_emits_ts2448() {
    let source = r#"
class C {
    greet(@lambda param: any) {}
}
const lambda = (...args: any[]): any => {};
"#;
    let codes = check(source);
    assert!(
        codes.contains(&2448),
        "expected TS2448 for `lambda` in parameter decorator before declaration, got {codes:?}"
    );
}

#[test]
fn parameter_decorator_factory_const_before_declaration_emits_ts2448() {
    let source = r#"
class C {
    greet(@lambda(1) param: any) {}
}
const lambda = (...args: any[]): any => {};
"#;
    let codes = check(source);
    assert!(
        codes.contains(&2448),
        "expected TS2448 for `lambda(1)` parameter decorator before declaration, got {codes:?}"
    );
}

// ── Parameter decorator: enum used before declaration ────────────────────────

#[test]
fn parameter_decorator_enum_before_declaration_emits_ts2450() {
    let source = r#"
declare const dec: any;
class C {
    greet(@dec(E.A) param: any) {}
}
enum E { A = 0 }
"#;
    let codes = check(source);
    assert!(
        codes.contains(&2450),
        "expected TS2450 for `E` in parameter decorator before declaration, got {codes:?}"
    );
}

// ── Negative: method body usage is NOT TDZ (deferred) ────────────────────────

#[test]
fn method_body_usage_after_declaration_no_ts2448() {
    let source = r#"
declare const dec: any;
class C {
    @dec
    greet() {
        return lambda(); // inside method body = deferred, not TDZ
    }
}
const lambda = () => 1;
"#;
    let codes = check(source);
    assert!(
        !codes.contains(&2448),
        "must NOT emit TS2448 for `lambda` used inside a method body (deferred), got {codes:?}"
    );
}

// ── Negative: non-IIFE arrow inside decorator is deferred ────────────────────

#[test]
fn arrow_inside_decorator_is_deferred_no_ts2448() {
    let source = r#"
class C {
    @(() => lambda) // arrow = deferred, not immediately executed
    greet() {}
}
const lambda = 1;
"#;
    let codes = check(source);
    assert!(
        !codes.contains(&2448),
        "must NOT emit TS2448 for `lambda` inside a non-IIFE arrow in a decorator, got {codes:?}"
    );
}

// ── Negative: no TDZ when declared before class ──────────────────────────────

#[test]
fn method_decorator_after_declaration_no_ts2448() {
    let source = r#"
const lambda = (...args: any[]): any => {};
class C {
    @lambda
    greet() {}
}
"#;
    let codes = check(source);
    assert!(
        !codes.contains(&2448),
        "must NOT emit TS2448 when `lambda` is declared before the class, got {codes:?}"
    );
}

// ── Negative: TS2449 for CLASS used in parameter decorator stays suppressed ──

#[test]
fn class_in_parameter_decorator_no_ts2449() {
    // Per tsc behavior, CLASS symbols referenced in parameter decorators are
    // NOT a TDZ violation (TS2449). This must not regress.
    let source = r#"
declare const dec: any;
class C {
    constructor(@dec(C) a: any) {}
    static m1(@dec(C) a: any) {}
    m2(@dec(C) a: any) {}
}
"#;
    let codes = check(source);
    assert!(
        !codes.contains(&2449),
        "must NOT emit TS2449 for CLASS `C` in parameter decorator, got {codes:?}"
    );
}
