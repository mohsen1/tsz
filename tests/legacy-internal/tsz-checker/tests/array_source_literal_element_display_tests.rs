//! A non-fresh array source keeps its literal element types in assignability
//! diagnostics.
//!
//! `tsc` renders the source of a `TS2322`/`TS2345` from a *declared* (non-fresh)
//! `Array<1>` / `1[]` / `(1 | 2)[]` / `ReadonlyArray<1>` source verbatim — `1[]`,
//! `(1 | 2)[]`, `readonly 1[]` — because `getWidenedType` widens only types
//! carrying the fresh-literal flag. A *fresh* array literal source
//! (`const y: string = [1, 2]`) is already widened to its primitive element type
//! at expression typing, so it still displays as `number[]`.
//!
//! tsz's diagnostic source-display helper `rebuilt_array_source_display`
//! previously widened the array element unconditionally, rendering `number[]`
//! where `tsc` shows `1[]`. All expectations below were verified against the
//! pinned oracle (`typescript@7.0.2`, `--noEmit --strict --target es2022
//! --lib es2022`). Binder names are varied across cases per the anti-hardcoding
//! contract. The real lib is loaded so the `Array` / `ReadonlyArray` interface
//! references in the `Array<1>` spelling resolve.

use crate::CheckerOptions;
use crate::test_utils::{check_source_with_libs_code_messages, load_default_lib_files};

/// The message text of the single `TS2322`/`TS2345` a fixture produces.
fn assignability_message(src: &str) -> String {
    let messages: Vec<String> = check_source_with_libs_code_messages(
        src,
        "case.ts",
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
        &load_default_lib_files(),
    )
    .into_iter()
    .filter(|(code, _)| *code == 2322 || *code == 2345)
    .map(|(_, message)| message)
    .collect();
    assert_eq!(
        messages.len(),
        1,
        "expected exactly one TS2322/TS2345 for this fixture, got: {messages:?}"
    );
    messages.into_iter().next().unwrap()
}

#[test]
fn annotated_generic_array_source_keeps_literal_element() {
    // `Array<1>` — the interface-application spelling — is the case that
    // regressed: it rendered `number[]` instead of `1[]`.
    let msg =
        assignability_message("declare const scores: Array<1>;\nconst label: string = scores;\n");
    assert!(
        msg.contains("Type '1[]'"),
        "expected source rendered as `1[]`, got: {msg}"
    );
    assert!(
        !msg.contains("number[]"),
        "the annotated `Array<1>` source must not widen to `number[]`, got: {msg}"
    );
}

#[test]
fn annotated_shorthand_array_source_keeps_literal_element() {
    let msg = assignability_message("declare const ids: 1[];\nconst tag: string = ids;\n");
    assert!(
        msg.contains("Type '1[]'"),
        "expected source rendered as `1[]`, got: {msg}"
    );
}

#[test]
fn annotated_literal_union_element_array_source_stays_structural() {
    // The element union must survive; `getWidenedType` never widens a declared
    // `1 | 2` element to `number`.
    let msg = assignability_message(
        "declare const picks: Array<1 | 2>;\nconst joined: string = picks;\n",
    );
    assert!(
        msg.contains("(1 | 2)[]"),
        "expected source rendered as `(1 | 2)[]`, got: {msg}"
    );
    assert!(
        !msg.contains("number[]"),
        "the `1 | 2` element union must not widen, got: {msg}"
    );
}

#[test]
fn annotated_readonly_array_source_keeps_literal_element() {
    let msg = assignability_message(
        "declare const frozen: ReadonlyArray<1>;\nconst text: string = frozen;\n",
    );
    assert!(
        msg.contains("readonly 1[]"),
        "expected source rendered as `readonly 1[]`, got: {msg}"
    );
    assert!(
        !msg.contains("number[]"),
        "the `ReadonlyArray<1>` source must not widen to `readonly number[]`, got: {msg}"
    );
}

#[test]
fn annotated_string_literal_array_source_keeps_literal_element() {
    // Renamed binders + a string literal element instead of a number, so the
    // rule cannot be keyed on the `1`/`scores` spelling.
    let msg = assignability_message(
        "declare const names: Array<\"hi\">;\nconst total: number = names;\n",
    );
    assert!(
        msg.contains("\"hi\"[]"),
        "expected source rendered as `\"hi\"[]`, got: {msg}"
    );
}

#[test]
fn nested_array_of_literal_array_source_keeps_literal_element() {
    let msg =
        assignability_message("declare const grid: Array<Array<1>>;\nconst flat: string = grid;\n");
    assert!(
        msg.contains("1[][]"),
        "expected source rendered as `1[][]`, got: {msg}"
    );
    assert!(
        !msg.contains("number[][]"),
        "the nested literal element must not widen, got: {msg}"
    );
}

#[test]
fn function_return_array_source_keeps_literal_element() {
    let msg = assignability_message(
        "declare const items: Array<1>;\nfunction render(): string { return items; }\n",
    );
    assert!(
        msg.contains("Type '1[]'"),
        "expected return source rendered as `1[]`, got: {msg}"
    );
}

#[test]
fn call_argument_array_source_keeps_literal_element() {
    // Renamed binders; the argument path (`TS2345`) shares the same
    // `rebuilt_array_source_display` helper as the assignment path.
    let msg = assignability_message(
        "declare const values: Array<1>;\ndeclare function take(word: string): void;\ntake(values);\n",
    );
    assert!(
        msg.contains("of type '1[]'"),
        "expected argument rendered as `1[]`, got: {msg}"
    );
    assert!(
        !msg.contains("number[]"),
        "the array argument must not widen to `number[]`, got: {msg}"
    );
}

#[test]
fn plain_assignment_array_source_keeps_literal_element() {
    let msg =
        assignability_message("declare const data: Array<1>;\nlet sink: string;\nsink = data;\n");
    assert!(
        msg.contains("Type '1[]'"),
        "expected source rendered as `1[]`, got: {msg}"
    );
}

#[test]
fn homomorphic_mapped_over_literal_array_source_keeps_literal_element() {
    // `Copy<1[]>` reduces to `1[]`; the display must not widen the reduced
    // array's element (the previously-`#[ignore]`d display half of the family).
    let msg = assignability_message(
        "type Copy<T> = { [K in keyof T]: T[K] };\ntype Mapped = Copy<1[]>;\ndeclare const mapped: Mapped;\nconst n: number = mapped;\n",
    );
    assert!(
        msg.contains("Type '1[]'"),
        "expected reduced mapped source rendered as `1[]`, got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Negative controls: a *fresh* array literal source still widens, and a bare
// primitive-element array is unaffected.
// ---------------------------------------------------------------------------

#[test]
fn fresh_array_literal_source_still_widens() {
    // `[7, 8]` is typed `number[]` at expression typing (no contextual type), so
    // its diagnostic source stays `number[]`, matching tsc. The fix must not turn
    // this into `(7 | 8)[]`.
    let msg = assignability_message("const heading: string = [7, 8];\n");
    assert!(
        msg.contains("number[]"),
        "a fresh array literal source must still widen to `number[]`, got: {msg}"
    );
}

#[test]
fn primitive_element_array_source_is_unchanged() {
    let msg =
        assignability_message("declare const counts: number[];\nconst caption: string = counts;\n");
    assert!(
        msg.contains("number[]"),
        "a `number[]` source is unaffected, got: {msg}"
    );
}
