//! A wide (non-literal) computed-key object literal, mismatched against a
//! target's declared index signature, must display its `TS2322` source type
//! using the key's own source spelling — `{ [ws]: number; }` — not a
//! synthesized `{ [x: string]: number; }` index-signature form. `tsc`'s
//! `checkObjectLiteral` never collapses distinct computed members into one
//! index-signature clause for display purposes, even when every member is a
//! wide `string`/`number`/`symbol` key that structurally folds into the same
//! index signature for assignability. Oracled against pinned `typescript@7.0.2`
//! (`--strict --pretty false`). See issue #16662 (residual 1's cosmetic
//! sub-note).
use tsz_checker::test_utils::check_source_diagnostics;

fn ts2322_message(source: &str) -> String {
    let diags = check_source_diagnostics(source);
    diags
        .iter()
        .find(|d| d.code == 2322)
        .unwrap_or_else(|| panic!("expected a TS2322, got: {diags:?}"))
        .message_text
        .clone()
}

#[test]
fn wide_string_key_source_display_uses_source_spelling() {
    let message = ts2322_message(
        r#"
declare const ws: string;
interface OnlyStr { [k: string]: string }
const hs: OnlyStr = { [ws]: 1 };
"#,
    );
    assert_eq!(
        message,
        "Type '{ [ws]: number; }' is not assignable to type 'OnlyStr'."
    );
}

#[test]
fn wide_number_key_source_display_uses_source_spelling() {
    let message = ts2322_message(
        r#"
declare const wn: number;
interface OnlyNum { [k: number]: string }
const hn: OnlyNum = { [wn]: 1 };
"#,
    );
    assert_eq!(
        message,
        "Type '{ [wn]: number; }' is not assignable to type 'OnlyNum'."
    );
}

#[test]
fn wide_symbol_key_source_display_still_uses_source_spelling() {
    // Regression guard: the symbol sibling already rendered correctly before
    // this fix (it never routed through the two hardcoded-`x` display paths
    // this change removes) — pin it stays that way.
    let message = ts2322_message(
        r#"
declare const w: symbol;
interface OnlySym { [k: symbol]: string }
const h: OnlySym = { [w]: 1 };
"#,
    );
    assert_eq!(
        message,
        "Type '{ [w]: number; }' is not assignable to type 'OnlySym'."
    );
}

#[test]
fn renamed_wide_string_key_source_display_uses_source_spelling() {
    // Anti-hardcoding: a differently-named binder must not change the outcome.
    let message = ts2322_message(
        r#"
declare const registryKey: string;
interface OnlyStr { [k: string]: string }
const h: OnlyStr = { [registryKey]: 1 };
"#,
    );
    assert_eq!(
        message,
        "Type '{ [registryKey]: number; }' is not assignable to type 'OnlyStr'."
    );
}

#[test]
fn two_distinct_wide_string_keys_display_each_member_separately() {
    // Multiple wide-string computed members must NOT collapse into one
    // synthesized index-signature clause; each keeps its own source-spelled
    // name and value type, matching tsc exactly (oracle-verified).
    let message = ts2322_message(
        r#"
declare const ws1: string;
declare const ws2: string;
interface OnlyStr { [k: string]: number }
const hs: OnlyStr = { [ws1]: 1, [ws2]: "x" };
"#,
    );
    assert_eq!(
        message,
        "Type '{ [ws1]: number; [ws2]: string; }' is not assignable to type 'OnlyStr'."
    );
}

#[test]
fn repeated_same_wide_string_key_appends_both_occurrences() {
    // A computed key that never resolves to a static name cannot collide in
    // the display's property table, so a repeated occurrence of the SAME
    // source variable is always appended rather than merged/deduped
    // (oracle-verified: tsc shows both `[s]:` clauses).
    let message = ts2322_message(
        r#"
declare const s: string;
interface OnlyStr { [k: string]: number }
const h: OnlyStr = { [s]: 1, [s]: "x" };
"#,
    );
    assert_eq!(
        message,
        "Type '{ [s]: number; [s]: string; }' is not assignable to type 'OnlyStr'."
    );
}

#[test]
fn plain_named_properties_against_string_index_target_are_unaffected() {
    // Negative control: ordinary (non-computed) named properties against a
    // target index signature take tsc's per-property elaboration path (a
    // leaf TS2322 per property), never the whole-object display this fix
    // touches. Must stay untouched by removing the wide-computed-key
    // display shortcut.
    let diags = check_source_diagnostics(
        r#"
interface OnlyStr { [k: string]: string }
const h: OnlyStr = { a: 1, b: 2 };
"#,
    );
    let messages: Vec<&str> = diags
        .iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text.as_str())
        .collect();
    assert_eq!(
        messages,
        vec![
            "Type 'number' is not assignable to type 'string'.",
            "Type 'number' is not assignable to type 'string'.",
        ]
    );
}
