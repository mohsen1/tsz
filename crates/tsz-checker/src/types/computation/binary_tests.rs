use crate::test_utils::{check_js_source_diagnostics, check_source_diagnostics};

#[test]
fn ts2839_strict_equality_object_literal() {
    let diags = check_source_diagnostics("if ({a: 1} === {a: 1}) {}");
    assert!(
        diags.iter().any(|d| d.code == 2839),
        "Expected TS2839 for object literal strict equality, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2839_strict_inequality_array_literal() {
    let diags = check_source_diagnostics("if ([1] !== [1]) {}");
    assert!(
        diags.iter().any(|d| d.code == 2839),
        "Expected TS2839 for array literal strict inequality, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2839_loose_equality_in_ts_file() {
    // In TS files, loose equality (==) also triggers TS2839
    let diags = check_source_diagnostics("if ({a: 1} == {a: 1}) {}");
    assert!(
        diags.iter().any(|d| d.code == 2839),
        "Expected TS2839 for object literal loose equality in TS, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2839_one_sided_literal() {
    // TS2839 fires even when only ONE side is a literal
    let diags = check_source_diagnostics("const a = {x: 1};\nif (a === {x: 1}) {}");
    assert!(
        diags.iter().any(|d| d.code == 2839),
        "Expected TS2839 for one-sided object literal equality, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2839_no_error_for_non_literals() {
    // No TS2839 when neither side is a literal
    let diags = check_source_diagnostics("const a = {x: 1};\nconst b = {x: 1};\nif (a === b) {}");
    let has_2839 = diags.iter().any(|d| d.code == 2839);
    assert!(
        !has_2839,
        "Should NOT emit TS2839 when no operand is a literal, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn no_duplicate_ts2367_for_same_type_comparison() {
    // Comparing values of the same type should not emit TS2367.
    // This verifies the duplicate TS2367 check removal doesn't regress
    // by ensuring same-type comparisons remain clean.
    let diags =
        check_source_diagnostics("const a: number = 1; const b: number = 2; if (a === b) {}");
    let has_2367 = diags.iter().any(|d| d.code == 2367);
    assert!(
        !has_2367,
        "Should NOT emit TS2367 for number vs number comparison, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2367_typeof_vs_invalid_typeof_string() {
    // typeof x returns "string"|"number"|"bigint"|"boolean"|"symbol"|"undefined"|"object"|"function"
    // Comparing with "Object" (capital O) should trigger TS2367 — no overlap.
    let diags =
        check_source_diagnostics(r#"declare var x: string | number; if (typeof x == "Object") {}"#);
    // Filter TS2318 (missing global types in test environment)
    let relevant: Vec<_> = diags.iter().filter(|d| d.code != 2318).collect();
    assert!(
        relevant.iter().any(|d| d.code == 2367),
        "Expected TS2367 for typeof vs 'Object' (non-typeof string), got: {:?}",
        relevant.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2367_typeof_vs_valid_typeof_string_no_error() {
    // typeof x compared with "string" (valid typeof result) — no TS2367
    let diags =
        check_source_diagnostics(r#"declare var x: string | number; if (typeof x == "string") {}"#);
    let relevant: Vec<_> = diags.iter().filter(|d| d.code != 2318).collect();
    let has_2367 = relevant.iter().any(|d| d.code == 2367);
    assert!(
        !has_2367,
        "Should NOT emit TS2367 for typeof vs valid typeof string, got: {:?}",
        relevant.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2839_js_file_strict_eq_only() {
    // In JS files, only strict equality (===) triggers TS2839, not loose (==)
    let diags_strict = check_js_source_diagnostics("if ({} === {}) {}");
    assert!(
        diags_strict.iter().any(|d| d.code == 2839),
        "Expected TS2839 for strict equality in JS file, got: {:?}",
        diags_strict.iter().map(|d| d.code).collect::<Vec<_>>()
    );

    let diags_loose = check_js_source_diagnostics("if ({} == {}) {}");
    let has_2839_loose = diags_loose.iter().any(|d| d.code == 2839);
    assert!(
        !has_2839_loose,
        "Should NOT emit TS2839 for loose equality in JS file, got: {:?}",
        diags_loose.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

fn check_source_diagnostics_no_implicit_any(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    crate::test_utils::check_source(
        source,
        "test.ts",
        crate::context::CheckerOptions {
            no_implicit_any: true,
            ..crate::context::CheckerOptions::default()
        },
    )
}

fn check_js_source_diagnostics_with_options(
    source: &str,
    options: crate::context::CheckerOptions,
) -> Vec<crate::diagnostics::Diagnostic> {
    crate::test_utils::check_source(source, "test.js", options)
}

#[test]
fn no_ts7006_for_null_default_parameter() {
    // A parameter with `= null` should NOT trigger TS7006 because
    // tsc infers the type as `null`, not implicit `any`.
    let diags = check_source_diagnostics_no_implicit_any("function f(x = null) { return x; }");
    let has_7006 = diags.iter().any(|d| d.code == 7006);
    assert!(
        !has_7006,
        "Should NOT emit TS7006 for parameter with null default, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn no_ts7006_for_undefined_default_parameter() {
    // A parameter with `= undefined` should NOT trigger TS7006 because
    // tsc infers the type as `undefined`, not implicit `any`.
    let diags = check_source_diagnostics_no_implicit_any("function f(x = undefined) { return x; }");
    let has_7006 = diags.iter().any(|d| d.code == 7006);
    assert!(
        !has_7006,
        "Should NOT emit TS7006 for parameter with undefined default, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts7006_still_emitted_for_bare_parameter() {
    // A parameter without a type annotation or initializer should still
    // trigger TS7006 under noImplicitAny.
    let diags = check_source_diagnostics_no_implicit_any("function f(x) { return x; }");
    let has_7006 = diags.iter().any(|d| d.code == 7006);
    assert!(
        has_7006,
        "Expected TS7006 for bare parameter under noImplicitAny, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn js_empty_array_default_parameter_reports_implicit_any_array_under_no_implicit_any() {
    let diags = check_js_source_diagnostics_with_options(
        r#"
/** @type {number | undefined} */
var n;
function f(a = null, b = n, l = []) {
    b = "error";
    l.push("ok");
}
"#,
        crate::context::CheckerOptions {
            check_js: true,
            no_implicit_any: true,
            strict_null_checks: false,
            ..crate::context::CheckerOptions::default()
        },
    );

    let ts7006_messages: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 7006)
        .map(|d| d.message_text.as_str())
        .collect();
    assert!(
        ts7006_messages
            .iter()
            .any(|msg| msg.contains("Parameter 'l' implicitly has an 'any[]' type.")),
        "Expected JS empty-array default parameter to report TS7006 any[], got: {diags:?}"
    );
    assert!(
        ts7006_messages
            .iter()
            .any(|msg| msg.contains("Parameter 'a' implicitly has an 'any' type.")),
        "Expected JS null default parameter to report TS7006 any under non-strict null checks, got: {diags:?}"
    );
}

#[test]
fn js_null_default_parameter_still_only_reports_empty_array_implicit_any_in_strict_mode() {
    let diags = check_js_source_diagnostics_with_options(
        r#"
function f(a = null, l = []) {
            a = 1;
    l.push("ok");
}
"#,
        crate::context::CheckerOptions {
            check_js: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..crate::context::CheckerOptions::default()
        },
    );

    let ts7006_messages: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 7006)
        .map(|d| d.message_text.as_str())
        .collect();
    assert!(
        !ts7006_messages
            .iter()
            .any(|msg| msg.contains("Parameter 'a' implicitly has an 'any' type.")),
        "Did not expect JS null default parameter to report implicit any under strictNullChecks, got: {diags:?}"
    );
    assert!(
        ts7006_messages
            .iter()
            .any(|msg| msg.contains("Parameter 'l' implicitly has an 'any[]' type.")),
        "Expected JS empty-array default parameter to report TS7006 any[] under strictNullChecks, got: {diags:?}"
    );
}

#[test]
fn js_undefined_default_parameter_reports_implicit_any_only_without_strict_null_checks() {
    let non_strict = check_js_source_diagnostics_with_options(
        r#"
function f(a = undefined, l = []) {
    a = 1;
    l.push("ok");
}
"#,
        crate::context::CheckerOptions {
            check_js: true,
            no_implicit_any: true,
            strict_null_checks: false,
            ..crate::context::CheckerOptions::default()
        },
    );
    let non_strict_messages: Vec<_> = non_strict
        .iter()
        .filter(|d| d.code == 7006)
        .map(|d| d.message_text.as_str())
        .collect();
    assert!(
        non_strict_messages
            .iter()
            .any(|msg| msg.contains("Parameter 'a' implicitly has an 'any' type.")),
        "Expected JS undefined default parameter to report TS7006 any without strictNullChecks, got: {non_strict:?}"
    );

    let strict = check_js_source_diagnostics_with_options(
        r#"
function f(a = undefined, l = []) {
    a = 1;
    l.push("ok");
}
"#,
        crate::context::CheckerOptions {
            check_js: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..crate::context::CheckerOptions::default()
        },
    );
    let strict_messages: Vec<_> = strict
        .iter()
        .filter(|d| d.code == 7006)
        .map(|d| d.message_text.as_str())
        .collect();
    assert!(
        !strict_messages
            .iter()
            .any(|msg| msg.contains("Parameter 'a' implicitly has an 'any' type.")),
        "Did not expect JS undefined default parameter to report implicit any under strictNullChecks, got: {strict:?}"
    );
    assert!(
        strict_messages
            .iter()
            .any(|msg| msg.contains("Parameter 'l' implicitly has an 'any[]' type.")),
        "Expected JS empty-array default parameter to keep reporting TS7006 any[] under strictNullChecks, got: {strict:?}"
    );
}

// TS18050 tests: null/undefined suppression for string concatenation and `any`

#[test]
fn ts18050_null_plus_number_emits_error() {
    // null + 1 should emit TS18050 (arithmetic context)
    let diags = check_source_diagnostics("var x = null + 1;");
    assert!(
        diags.iter().any(|d| d.code == 18050),
        "Expected TS18050 for null + number, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts18050_undefined_plus_number_emits_error() {
    let diags = check_source_diagnostics("var x = undefined + 1;");
    assert!(
        diags.iter().any(|d| d.code == 18050),
        "Expected TS18050 for undefined + number, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts18050_string_plus_null_no_error() {
    // "test" + null is string concatenation — no TS18050
    let diags = check_source_diagnostics("var d: string;\nvar x = d + null;");
    let has_18050 = diags.iter().any(|d| d.code == 18050);
    assert!(
        !has_18050,
        "Should NOT emit TS18050 for string + null (concatenation), got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts18050_null_plus_string_no_error() {
    // null + "test" is string concatenation — no TS18050
    let diags = check_source_diagnostics("var d: string;\nvar x = null + d;");
    let has_18050 = diags.iter().any(|d| d.code == 18050);
    assert!(
        !has_18050,
        "Should NOT emit TS18050 for null + string (concatenation), got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts18050_string_literal_plus_null_no_error() {
    // '' + null is string concatenation — no TS18050
    let diags = check_source_diagnostics("var x = '' + null;");
    let has_18050 = diags.iter().any(|d| d.code == 18050);
    assert!(
        !has_18050,
        "Should NOT emit TS18050 for string literal + null, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts18050_any_plus_null_no_error() {
    // any + null should not emit TS18050 — any suppresses type errors
    let diags = check_source_diagnostics("declare var a: any;\nvar x = a + null;");
    let has_18050 = diags.iter().any(|d| d.code == 18050);
    assert!(
        !has_18050,
        "Should NOT emit TS18050 for any + null, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts18050_null_plus_any_no_error() {
    let diags = check_source_diagnostics("declare var a: any;\nvar x = null + a;");
    let has_18050 = diags.iter().any(|d| d.code == 18050);
    assert!(
        !has_18050,
        "Should NOT emit TS18050 for null + any, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts18050_null_minus_number_emits_error() {
    // null - 1 should still emit TS18050 (not a + operator)
    let diags = check_source_diagnostics("var x = null - 1;");
    assert!(
        diags.iter().any(|d| d.code == 18050),
        "Expected TS18050 for null - number, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

fn check_source_diagnostics_no_strict_null(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    crate::test_utils::check_source(
        source,
        "test.ts",
        crate::context::CheckerOptions {
            strict_null_checks: false,
            ..crate::context::CheckerOptions::default()
        },
    )
}

#[test]
fn ts18050_not_emitted_without_strict_null_checks() {
    // Without strictNullChecks, null/undefined are in every type's domain,
    // so tsc does NOT emit TS18050 for binary operations on null/undefined.
    let diags = check_source_diagnostics_no_strict_null("var x = null + 1;");
    let has_18050 = diags.iter().any(|d| d.code == 18050);
    assert!(
        !has_18050,
        "Should NOT emit TS18050 for null + number without strictNullChecks, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts18050_not_emitted_for_undefined_multiply_without_strict_null_checks() {
    // Without strictNullChecks, undefined is assignable to number, so
    // undefined * boolean should only emit TS2363 (for boolean), not TS18050.
    let diags =
        check_source_diagnostics_no_strict_null("declare var a: boolean;\nvar x = undefined * a;");
    let has_18050 = diags.iter().any(|d| d.code == 18050);
    assert!(
        !has_18050,
        "Should NOT emit TS18050 for undefined * boolean without strictNullChecks, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// =========================================================================
// TS2365: Mixed-orderable relational operator checks
// =========================================================================

#[test]
fn ts2365_number_less_than_string() {
    // TSC rejects `number < string` — they are individually orderable but
    // not of the same orderable kind.
    let diags =
        check_source_diagnostics("declare var a: number; declare var b: string; var r = a < b;");
    assert!(
        diags.iter().any(|d| d.code == 2365),
        "Expected TS2365 for number < string, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2365_string_greater_than_number() {
    // Same test in reverse direction.
    let diags =
        check_source_diagnostics("declare var a: string; declare var b: number; var r = a > b;");
    assert!(
        diags.iter().any(|d| d.code == 2365),
        "Expected TS2365 for string > number, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn no_ts2365_for_same_orderable_kind() {
    // Same orderable kind should not produce TS2365.
    let diags =
        check_source_diagnostics("declare var a: number; declare var b: number; var r = a < b;");
    assert!(
        !diags.iter().any(|d| d.code == 2365),
        "Should NOT emit TS2365 for number < number, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// =========================================================================
// TS1345: Void truthiness gated on strictNullChecks
// =========================================================================

fn check_source_diagnostics_no_strict(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    crate::test_utils::check_source(
        source,
        "test.ts",
        crate::context::CheckerOptions {
            strict: false,
            strict_null_checks: false,
            strict_function_types: false,
            strict_property_initialization: false,
            no_implicit_this: false,
            no_implicit_any: false,
            use_unknown_in_catch_variables: false,
            ..crate::context::CheckerOptions::default()
        },
    )
}

#[test]
fn ts1345_void_truthiness_with_strict() {
    // With strict (default), TS1345 should fire for void truthiness.
    let diags = check_source_diagnostics("declare var a: void; if (a) {}");
    assert!(
        diags.iter().any(|d| d.code == 1345),
        "Expected TS1345 for void truthiness with strict, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn no_ts1345_void_truthiness_without_strict() {
    // Without strictNullChecks, TS1345 should NOT fire for void truthiness.
    // TSC does not emit this diagnostic when strictNullChecks is off.
    let diags = check_source_diagnostics_no_strict("declare var a: void; if (a) {}");
    assert!(
        !diags.iter().any(|d| d.code == 1345),
        "Should NOT emit TS1345 for void truthiness without strict, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn no_ts1345_void_in_logical_and_without_strict() {
    // void && any — should NOT emit TS1345 without strictNullChecks.
    let diags = check_source_diagnostics_no_strict(
        "declare var a: void; declare var b: any; var r = a && b;",
    );
    assert!(
        !diags.iter().any(|d| d.code == 1345),
        "Should NOT emit TS1345 for void && any without strict, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// Tests for optional property overlap (TS2365/TS2367)

#[test]
fn no_ts2365_for_objects_with_all_optional_properties() {
    // Objects where ALL properties are optional overlap at `{}`, so comparison
    // operators should not emit TS2365 even if the optional property types differ.
    let diags = check_source_diagnostics(
        "interface A { b?: number; } interface B { b?: string; }
             declare var a: A; declare var b: B;
             var r = a < b;",
    );
    assert!(
        !diags.iter().any(|d| d.code == 2365),
        "Should NOT emit TS2365 for all-optional property objects, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn no_ts2367_for_objects_with_all_optional_properties() {
    // Objects where ALL properties are optional overlap at `{}`, so equality
    // operators should not emit TS2367 even if the optional property types differ.
    let diags = check_source_diagnostics(
        "interface A { b?: number; } interface B { b?: string; }
             declare var a: A; declare var b: B;
             var r = a === b;",
    );
    assert!(
        !diags.iter().any(|d| d.code == 2367),
        "Should NOT emit TS2367 for all-optional property objects, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2365_still_emitted_for_objects_with_required_properties() {
    // Objects with required properties of incompatible types should still emit TS2365.
    let diags = check_source_diagnostics(
        "interface A { b: number; } interface B { b: string; }
             declare var a: A; declare var b: B;
             var r = a < b;",
    );
    assert!(
        diags.iter().any(|d| d.code == 2365),
        "Expected TS2365 for objects with incompatible required properties, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2363_any_times_type_parameter() {
    // When left operand is `any` and right is a type parameter T,
    // tsc emits TS2363 for the right-hand side. The evaluator returns
    // Success(number) for `any * T`, but T is not a valid arithmetic operand.
    let diags = check_source_diagnostics("function f<T>(t: T) { let a: any; var r = a * t; }");
    assert!(
        diags.iter().any(|d| d.code == 2363),
        "Expected TS2363 for type parameter in `any * T`, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2362_type_parameter_times_any() {
    // When left operand is a type parameter T and right is `any`,
    // tsc emits TS2362 for the left-hand side.
    let diags = check_source_diagnostics("function f<T>(t: T) { let a: any; var r = t * a; }");
    assert!(
        diags.iter().any(|d| d.code == 2362),
        "Expected TS2362 for type parameter in `T * any`, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2363_any_bitwise_and_type_parameter() {
    // Bitwise operators also require per-operand checks:
    // `any & T` should emit TS2363 for T.
    let diags = check_source_diagnostics("function f<T>(t: T) { let a: any; var r = a & t; }");
    assert!(
        diags.iter().any(|d| d.code == 2363),
        "Expected TS2363 for type parameter in `any & T`, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn no_ts2362_for_any_times_any() {
    // `any * any` should NOT emit TS2362 or TS2363 — both operands are valid.
    let diags = check_source_diagnostics("function f() { let a: any; let b: any; var r = a * b; }");
    let has_2362_or_2363 = diags.iter().any(|d| d.code == 2362 || d.code == 2363);
    assert!(
        !has_2362_or_2363,
        "Should NOT emit TS2362/TS2363 for `any * any`, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn no_ts2362_for_number_times_any() {
    // `number * any` should NOT emit any arithmetic errors.
    let diags =
        check_source_diagnostics("function f() { let a: number; let b: any; var r = a * b; }");
    let has_2362_or_2363 = diags.iter().any(|d| d.code == 2362 || d.code == 2363);
    assert!(
        !has_2362_or_2363,
        "Should NOT emit TS2362/TS2363 for `number * any`, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// =========================================================================
// TS2367: Declared-type overlap suppression for loop narrowing
// =========================================================================

#[test]
fn no_ts2367_for_loop_narrowed_union_variable() {
    // When a variable is declared as `0 | 1`, initialized to `0`, and
    // compared with `1` inside a loop, flow narrows it to `0`. tsc widens
    // at the loop boundary; we suppress TS2367 by checking the declared type.
    let diags = check_source_diagnostics(
        "function f() { let code: 0 | 1 = 0; while (true) { code = code === 1 ? 0 : 1; } }",
    );
    let has_2367 = diags.iter().any(|d| d.code == 2367);
    assert!(
        !has_2367,
        "Should NOT emit TS2367 for loop-narrowed union variable, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2367_still_emitted_for_genuinely_unrelated_types() {
    // Genuine no-overlap: string vs number should still trigger TS2367.
    let diags = check_source_diagnostics("declare var x: string; if (x === 1) {}");
    assert!(
        diags.iter().any(|d| d.code == 2367),
        "Expected TS2367 for string === number (no overlap), got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2367_widens_cross_family_literal_against_constrained_intersection() {
    let diags = check_source_diagnostics(
        r#"function f<T extends string | number>(x: T & number) {
    const t1 = x === "hello";
}"#,
    );
    let relevant: Vec<_> = diags.iter().filter(|d| d.code == 2367).collect();
    assert_eq!(
        relevant.len(),
        1,
        "Expected exactly one TS2367, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.as_str()))
            .collect::<Vec<_>>()
    );
    assert!(
        relevant[0]
            .message_text
            .contains("types 'T & number' and 'string' have no overlap"),
        "Expected widened string display, got: {:?}",
        relevant[0].message_text
    );
}

#[test]
fn ts2367_widens_literal_unions_to_comparison_base_type() {
    let diags = check_source_diagnostics(r#"declare let x: 1 | 2; if (x === "hello") {}"#);
    let relevant: Vec<_> = diags.iter().filter(|d| d.code == 2367).collect();
    assert_eq!(
        relevant.len(),
        1,
        "Expected exactly one TS2367, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.as_str()))
            .collect::<Vec<_>>()
    );
    assert!(
        relevant[0]
            .message_text
            .contains("types 'number' and 'string' have no overlap"),
        "Expected comparison-base display for numeric literal union, got: {:?}",
        relevant[0].message_text
    );
}

#[test]
fn no_ts2367_for_three_member_union_narrowed_in_loop() {
    // Three-member union `0 | 1 | 2` narrowed by control flow in a for-of loop.
    // This matches the f1() case from controlFlowNoIntermediateErrors.
    let diags = check_source_diagnostics(
        "function f() {
                let code: 0 | 1 | 2 = 0;
                const arr: (0 | 1 | 2)[] = [2, 0, 1];
                for (const c of arr) {
                    if (c === 0) { code = code === 2 ? 1 : 0; }
                    else { code = 2; }
                }
            }",
    );
    let has_2367 = diags.iter().any(|d| d.code == 2367);
    assert!(
        !has_2367,
        "Should NOT emit TS2367 for 3-member union narrowed in loop, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2367_emitted_after_switch_true_clause_narrowing() {
    let diags = check_source_diagnostics(
        r#"type Shape =
    | { kind: "circle", radius: number }
    | { kind: "square", sideLength: number };

function wat(shape: Shape) {
    switch (true) {
        case shape.kind === "circle":
            return Math.PI * shape.radius ** 2;
        case shape.kind === "circle":
    }

    if (shape.kind === "circle") {
        return Math.PI * shape.radius ** 2;
    } else if (shape.kind === "circle") {
    }
}"#,
    );
    let ts2367_count = diags.iter().filter(|d| d.code == 2367).count();
    assert_eq!(
        ts2367_count,
        2,
        "Expected TS2367 after switch(true) clause narrowing, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn no_ts2363_for_constrained_type_parameter() {
    // `any * T` where T extends number should NOT emit TS2363
    // because constrained T is a valid arithmetic operand.
    let diags = check_source_diagnostics(
        "function f<T extends number>(t: T) { let a: any; var r = a * t; }",
    );
    let has_2363 = diags.iter().any(|d| d.code == 2363);
    assert!(
        !has_2363,
        "Should NOT emit TS2363 for `any * T` where T extends number, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}
