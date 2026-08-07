//! A wide (non-literal) computed-key object literal, mismatched against a
//! target's declared index signature, displays its `TS2322` source type using
//! the key's own source spelling — `{ [ws]: number; }` — only when `tsc` can
//! re-spell that key from its own syntax: a plain identifier or a dotted
//! `a.b.c` chain of identifiers (`ts.isEntityNameExpression`). For any other
//! computed-key expression (a binary operation, a call, a template literal,
//! ...) `tsc` falls back to a synthesized `{ [x: string]: V; }`
//! index-signature clause instead, and doing so for even ONE member of an
//! otherwise-homogeneous wide-key group folds every sibling in that group —
//! entity-named or not — into that same single clause, unioning every
//! member's value type. Oracled against pinned `typescript@7.0.2`
//! (`--strict --pretty false`). See issue #16662 (residual 1's cosmetic
//! sub-note) and the regression caught in review on #16721
//! (`computedPropertyNamesContextualType{8,9,10}_ES{5,6}.ts`).
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
fn property_access_entity_name_key_source_display_uses_source_spelling() {
    // `box.key` is a dotted entity-name reference, not a plain identifier —
    // `tsc` still re-spells it verbatim (oracle-verified).
    let message = ts2322_message(
        r#"
declare const box: { key: string };
interface OnlyStr { [k: string]: number }
const h: OnlyStr = { [box.key]: "a" };
"#,
    );
    assert_eq!(
        message,
        "Type '{ [box.key]: string; }' is not assignable to type 'OnlyStr'."
    );
}

#[test]
fn single_non_entity_wide_string_key_folds_to_synthesized_index_signature() {
    // A binary expression is NOT an entity-name reference, so even alone it
    // falls back to the synthesized `[x: string]: V` form instead of its own
    // source spelling `[""+"foo"]` (oracle-verified; this is the exact shape
    // of the regressed conformance rows `computedPropertyNamesContextualType
    // {8,9,10}_ES{5,6}.ts` caught in review on #16721).
    let message = ts2322_message(
        r#"
interface OnlyStr { [k: string]: number }
const h: OnlyStr = { [""+"foo"]: "a" };
"#,
    );
    assert_eq!(
        message,
        "Type '{ [x: string]: string; }' is not assignable to type 'OnlyStr'."
    );
}

#[test]
fn two_non_entity_wide_string_keys_merge_and_union_their_value_types() {
    // Two non-entity-name computed keys of the same kind fold into ONE
    // synthesized clause with the UNION of their value types — unlike two
    // entity-name keys, which stay separate and unmerged (oracle-verified;
    // matches `computedPropertyNamesContextualType8_ES5.ts` exactly).
    let message = ts2322_message(
        r#"
interface OnlyStr { [k: string]: number }
const h: OnlyStr = { [""+"foo"]: "a", [""+"bar"]: 1 };
"#,
    );
    assert_eq!(
        message,
        "Type '{ [x: string]: string | number; }' is not assignable to type 'OnlyStr'."
    );
}

#[test]
fn mixing_one_non_entity_key_folds_the_entity_named_sibling_too() {
    // A single non-entity-name member is enough to fold its entity-named
    // sibling into the same merged clause too — the entity-named member's own
    // spelling and individual entry are both lost, and its value type is
    // absorbed into the union (oracle-verified: `[ws]` does NOT keep its name
    // once `[""+"foo"]` is present in the same kind-group).
    let message = ts2322_message(
        r#"
declare const ws: string;
interface OnlyStr { [k: string]: boolean }
const h: OnlyStr = { [ws]: 1, [""+"foo"]: "b" };
"#,
    );
    assert_eq!(
        message,
        "Type '{ [x: string]: string | number; }' is not assignable to type 'OnlyStr'."
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
