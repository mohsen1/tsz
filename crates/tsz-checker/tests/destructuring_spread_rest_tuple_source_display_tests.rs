//! Locks the TS2322 source-type display for `var [a, b]: [T, T] = [...arr]`.
//!
//! When the LHS is an array-binding pattern with an explicit tuple annotation
//! and the initializer is `[...arr]` for some `arr: T[]`, tsz constructs a
//! source tuple `[...T[]]` to represent the variadic spread. The diagnostic
//! display path used to emit `[...T[][]]` because the per-element
//! source-display routine appended an extra `[]` to the rest element type.
//! tsc's display rule collapses single-rest tuples to the bare array type
//! `T[]` (except when the rest element is a type parameter).
//!
//! Conformance regression target:
//!   `compiler/destructuringArrayBindingPatternAndAssignment2.ts`
//! whose tsc baseline expects:
//!   `Type 'number[]' is not assignable to type '[number, number]'.`
//! tsz used to emit:
//!   `Type '[...number[][]]' is not assignable to type '[number, number]'.`

use tsz_checker::test_utils::check_source_code_messages;
use tsz_common::diagnostics::diagnostic_codes;

fn ts2322_messages(source: &str) -> Vec<String> {
    check_source_code_messages(source)
        .into_iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, msg)| msg)
        .collect()
}

/// `[...arr]` where `arr: number[]` assigned via array binding pattern with a
/// tuple annotation. The source type must render as `number[]`, mirroring tsc's
/// single-rest tuple collapse rule. Locks the rest-element display fix so the
/// rendered string never regresses to the buggy `[...number[][]]` form.
#[test]
fn destructured_var_decl_spread_array_displays_as_array_type() {
    let source = r#"
declare var arr: number[];
var [a, b]: [number, number] = [...arr];
"#;
    let msgs = ts2322_messages(source);
    assert!(
        msgs.iter().any(|m| {
            m.contains("Type 'number[]' is not assignable to type '[number, number]'")
        }),
        "TS2322 source must render as `number[]` (single-rest tuple collapse). Got: {msgs:?}",
    );
    // Negative lock: the doubled-array form must not return.
    assert!(
        !msgs.iter().any(|m| m.contains("number[][]")),
        "TS2322 source must not contain doubled-array form `number[][]`. Got: {msgs:?}",
    );
    // Negative lock: the tuple-bracket form must not return either —
    // single-rest tuples should collapse to the bare array type.
    assert!(
        !msgs.iter().any(|m| m.contains("[...number[]]")),
        "TS2322 source must not keep the bracketed `[...number[]]` form for a single-rest tuple. Got: {msgs:?}",
    );
}

/// Different element type, same shape — confirms the fix isn't tied to one name.
#[test]
fn destructured_var_decl_spread_string_array_displays_as_array_type() {
    let source = r#"
declare var arr: string[];
var [a, b]: [string, string] = [...arr];
"#;
    let msgs = ts2322_messages(source);
    assert!(
        msgs.iter().any(|m| {
            m.contains("Type 'string[]' is not assignable to type '[string, string]'")
        }),
        "TS2322 source must render as `string[]`. Got: {msgs:?}",
    );
    assert!(
        !msgs.iter().any(|m| m.contains("string[][]")),
        "TS2322 source must not contain doubled-array form `string[][]`. Got: {msgs:?}",
    );
}

/// Multi-element variadic source: `[a, ...arr]` against `[T, T]`.
/// tsc renders as `[number, ...number[]]`. The fix to `tuple_structural_source_display`
/// must also drop the spurious `[]` suffix on rest elements that share the
/// tuple with non-rest elements.
#[test]
fn destructured_var_decl_mixed_tuple_spread_renders_rest_without_extra_brackets() {
    let source = r#"
declare var arr: number[];
var [a, b, c]: [number, number, number] = [1, ...arr];
"#;
    let msgs = ts2322_messages(source);
    // Either the source displays as `[number, ...number[]]` (canonical) or
    // it widens to a bare array — both are acceptable. The bug's signature is
    // an extra `[]` after the rest type, never `number[][]` or `...number[][]`.
    assert!(
        !msgs.iter().any(|m| m.contains("number[][]")),
        "TS2322 source must not contain doubled-array form `number[][]`. Got: {msgs:?}",
    );
}
