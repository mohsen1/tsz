//! When a tuple type has an optional element (e.g. `[string, number?]`),
//! `undefined` must be assignable to that slot. The structural rule:
//! `[string?]` is shorthand for `[(string | undefined)?]` — the optional
//! marker carries an implicit `| undefined` for assignability purposes,
//! independent of whether the source slot is itself optional or required.
//!
//! Two failure surfaces share this rule and both are exercised here:
//! 1. Direct tuple-to-tuple assignment (subtype rule in
//!    `tsz-solver/src/relations/subtype/rules/tuples.rs`).
//! 2. Array-literal initializer with contextual tuple type
//!    (element elaboration in
//!    `tsz-checker/src/error_reporter/call_errors/elaboration_array_mismatch.rs`).
//!
//! 3. Variadic rest argument into a generic tuple-typed rest param
//!    (e.g. `f<U extends unknown[]>(cb: (...args: U) => T, ...args: U)`
//!    called with an `undefined` trailing arg) — handled by the
//!    `param_is_optional_for_arg_index` helper in
//!    `tsz-solver/src/operations/call_args.rs`.

use tsz_checker::test_utils::check_source_diagnostics;

fn count(diags: &[tsz_checker::diagnostics::Diagnostic], code: u32) -> usize {
    diags.iter().filter(|d| d.code == code).count()
}

/// Direct tuple assignment: `[string, undefined?]` → `[string, number?]`.
/// Both source and target have an optional second slot; the target's slot
/// must accept the source's `undefined` value.
#[test]
fn optional_tuple_element_accepts_undefined_in_tuple_to_tuple() {
    let source = r#"
declare const t1: [string, undefined?];
const t2: [string, number?] = t1;
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2322),
        0,
        "[string, undefined?] must be assignable to [string, number?]; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Array-literal initializer with optional tuple contextual type.
#[test]
fn array_literal_undefined_into_optional_tuple_slot() {
    let source = r#"
const x: [string, number?] = ["foo", undefined];
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2322),
        0,
        "['foo', undefined] must be assignable to [string, number?]; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Anti-hardcoding (§25): the rule is structural ("optional tuple element
/// at position i accepts undefined"), not specific to two-element tuples
/// or to the second position. Re-run with three-element tuples and
/// optional at different positions.
#[test]
fn optional_tuple_element_accepts_undefined_at_various_positions() {
    let cases: &[(&str, &str)] = &[
        // Last optional slot
        ("[string, number, boolean?]", r#"["foo", 1, undefined]"#),
        // Two trailing optionals
        ("[string, number?, boolean?]", r#"["foo", undefined, true]"#),
    ];
    for (target_ty, init) in cases {
        let source = format!("const x: {target_ty} = {init};\n");
        let diags = check_source_diagnostics(&source);
        assert_eq!(
            count(&diags, 2322),
            0,
            "{init} → {target_ty} must not emit TS2322; got: {:?}",
            diags
                .iter()
                .map(|d| (d.code, d.message_text.clone()))
                .collect::<Vec<_>>()
        );
    }
}

/// Variadic rest into a generic tuple-typed rest param: when the inferred
/// tuple has an optional element, passing `undefined` at that position must
/// be accepted. Mirrors the failing line in
/// `TypeScript/tests/cases/compiler/promiseTry.ts`.
#[test]
fn variadic_rest_undefined_into_inferred_optional_tuple_slot() {
    let source = r#"
declare function tryCb<T, U extends unknown[]>(
    callbackFn: (...args: U) => T,
    ...args: U
): T;
tryCb((foo: string, bar?: number) => "result", "foo", undefined);
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2345),
        0,
        "undefined at optional position in inferred U must not emit TS2345; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Anti-hardcoding (§25): re-run the variadic rest test with a different
/// type-parameter name (`Args` instead of `U`). The fix lives in inference
/// and call-arg helpers, not in any name-based rule.
#[test]
fn variadic_rest_undefined_into_inferred_optional_tuple_slot_alt_name() {
    let source = r#"
declare function tryCb<R, Args extends unknown[]>(
    callbackFn: (...args: Args) => R,
    ...args: Args
): R;
tryCb((p1: string, p2?: number) => 0, "ok", undefined);
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2345),
        0,
        "with alt-name type params; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Negative companion: required tuple slot still rejects `undefined`. The
/// fix must be scoped to the optional flag.
#[test]
fn required_tuple_element_still_rejects_undefined() {
    let source = r#"
const x: [string, number] = ["foo", undefined];
"#;
    let diags = check_source_diagnostics(source);
    let assignability_errors = count(&diags, 2322) + count(&diags, 2741);
    assert!(
        assignability_errors >= 1,
        "required slot must still reject undefined; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}
