//! ES (TC39 stage-3) class-member decorator arity parity.
//!
//! Structural rule:
//! - Under standard (non-`experimentalDecorators`) decorators, tsc invokes a
//!   member decorator with `min(max(paramCount, 1), 2)` arguments
//!   (`getDecoratorArgumentCount`). A decorator that declares a single
//!   parameter (`(value: any) => any`) is therefore called with only the
//!   value/target argument; the trailing context argument is never supplied.
//! - tsz previously always passed two synthetic arguments, so a 1-parameter
//!   decorator over-flowed its single parameter and produced a spurious
//!   TS1241 (method/accessor) or TS1240 (field).
//! - A decorator whose every signature takes zero parameters keeps the
//!   TS1329 "did you mean to call it" factory hint, uniformly across member
//!   kinds.
//!
//! These cases vary parameter names and member kinds to prove the fix is
//! structural — it depends on the decorator's declared arity, not on any
//! identifier spelling.

use tsz_checker::test_utils::check_source_codes;

fn assert_clean(source: &str) {
    let codes = check_source_codes(source).to_vec();
    assert!(
        !codes.contains(&1240) && !codes.contains(&1241) && !codes.contains(&1329),
        "Expected no decorator-signature diagnostic (TS1240/1241/1329); got: {codes:?}"
    );
}

// ───────────────────────── single-parameter acceptance ─────────────────────

#[test]
fn es_one_param_method_decorator_is_accepted() {
    assert_clean(
        r#"
function d(value: any): any { return value; }
class C { @d method() {} }
"#,
    );
}

#[test]
fn es_one_param_method_decorator_renamed_param_is_accepted() {
    // Different parameter name — proves the rule is arity-based, not name-based.
    assert_clean(
        r#"
function trace(original: any) { return original; }
class Service { @trace run(): void {} }
"#,
    );
}

#[test]
fn es_one_param_field_decorator_is_accepted() {
    assert_clean(
        r#"
function d(t: any) { return t; }
class C { @d field = 1; }
"#,
    );
}

#[test]
fn es_one_param_field_decorator_renamed_param_is_accepted() {
    assert_clean(
        r#"
function tag(target: any) { return target; }
class Model { @tag count = 0; }
"#,
    );
}

#[test]
fn es_one_param_get_accessor_decorator_is_accepted() {
    assert_clean(
        r#"
function d(value: any): any { return value; }
class C { @d get x(): number { return 1; } }
"#,
    );
}

#[test]
fn es_one_param_set_accessor_decorator_is_accepted() {
    assert_clean(
        r#"
function d(value: any): any { return value; }
class C { @d set x(v: number) {} }
"#,
    );
}

#[test]
fn es_one_param_auto_accessor_decorator_is_accepted() {
    assert_clean(
        r#"
function d(target: any) { return target; }
class C { @d accessor x = 1; }
"#,
    );
}

#[test]
fn es_one_param_decorator_factory_is_accepted() {
    // `@make()` resolves the decorator to the factory's 1-parameter return
    // function, which must also be accepted.
    assert_clean(
        r#"
function make() {
    return function (value: any) { return value; };
}
class C { @make() method(): void {} }
"#,
    );
}

// ───────────────────────── adjacent forms stay green ───────────────────────

#[test]
fn es_two_param_method_decorator_still_accepted() {
    assert_clean(
        r#"
function d(value: any, context: any) { return value; }
class C { @d method(): void {} }
"#,
    );
}

#[test]
fn es_rest_param_method_decorator_is_accepted() {
    // `(...args: any[])` has one declared parameter; the rest element absorbs
    // the supplied value argument.
    assert_clean(
        r#"
function d(...args: any[]) {}
class C { @d method(): void {} }
"#,
    );
}

#[test]
fn es_rest_param_field_decorator_is_accepted() {
    assert_clean(
        r#"
function d(...args: any[]) {}
class C { @d field = 1; }
"#,
    );
}

// ───────────────────────── negatives must still fire ───────────────────────

#[test]
fn es_zero_param_method_decorator_emits_ts1329() {
    let codes = check_source_codes(
        r#"
function d() {}
class C { @d method(): void {} }
"#,
    )
    .to_vec();
    assert!(
        codes.contains(&1329) && !codes.contains(&1241),
        "Zero-param method decorator should keep TS1329 (not TS1241); got: {codes:?}"
    );
}

#[test]
fn es_zero_param_field_decorator_emits_ts1329_not_ts1240() {
    // Parity broadening: tsc's `isPotentiallyUncalledDecorator` reports TS1329
    // for a zero-parameter field decorator too — not the generic TS1240.
    let codes = check_source_codes(
        r#"
function d() {}
class C { @d field = 1; }
"#,
    )
    .to_vec();
    assert!(
        codes.contains(&1329) && !codes.contains(&1240),
        "Zero-param field decorator should emit TS1329, not TS1240; got: {codes:?}"
    );
}

#[test]
fn es_one_param_method_decorator_wrong_value_type_still_rejected() {
    // The first (value) argument is still type-checked: a method's function
    // value is not assignable to `string`, so TS1241 must still fire.
    let codes = check_source_codes(
        r#"
function d(value: string): any { return value; }
class C { @d method(): number { return 1; } }
"#,
    )
    .to_vec();
    assert!(
        codes.contains(&1241),
        "A 1-param method decorator whose value parameter rejects the member type must still emit TS1241; got: {codes:?}"
    );
}

#[test]
fn es_three_required_param_method_decorator_still_rejected() {
    // Three required parameters under-flow the two-argument cap, so tsc (and
    // tsz) still report TS1241.
    let codes = check_source_codes(
        r#"
function d(a: any, b: any, c: any) {}
class C { @d method(): void {} }
"#,
    )
    .to_vec();
    assert!(
        codes.contains(&1241),
        "A 3-required-param method decorator should still emit TS1241; got: {codes:?}"
    );
}
