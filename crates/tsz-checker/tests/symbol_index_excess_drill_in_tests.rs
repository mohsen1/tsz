//! Adjacent cases for the #17623 fix family: an object-literal value written
//! through a computed unique-symbol key checks against the target's
//! `[k: symbol]` index signature even when the target ALSO carries a
//! `[k: string]` index signature.
//!
//! The primary rows (flat TS2418, nested TS2322 drill-in, nested TS2353
//! excess) live in `state/state_checking/property.rs`'s in-source tests; this
//! file holds the renamed-binder / alias-wrapper / split-keyspace matrix. All
//! expectations oracled against the pinned `typescript@7.0.2`
//! (`--strict --target es2022 --lib es2022`).

use tsz_checker::test_utils::check_source_diagnostics;

#[test]
fn ts2353_symbol_index_nested_excess_renamed_binders() {
    // Same shape as the #16649 drill-in rows but with every user-chosen
    // name changed, so no name-keyed fast path can satisfy the family.
    let diags = check_source_diagnostics(
        r#"
declare const marker: unique symbol;
interface Payload { count: number; }
interface Bag { [key: string]: number; [key: symbol]: Payload; }
const v1: Bag = { [marker]: { count: 1, extra: 2 } };
"#,
    );
    let ts2353: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
    assert_eq!(
        ts2353.len(),
        1,
        "expected one TS2353 on the nested excess property 'extra', got: {diags:?}"
    );
    assert!(
        ts2353[0].message_text.contains("'extra'"),
        "TS2353 should mention 'extra', got: {}",
        ts2353[0].message_text
    );
}

#[test]
fn ts2322_symbol_only_index_nested_mismatch_drills_in() {
    // Control without a string index signature: the symbol index is the
    // only applicable info and its nested member mismatch still anchors
    // TS2322 (this arm worked before the #17623 fix and must keep doing
    // so — the bug was specific to a target carrying BOTH index flavors).
    let diags = check_source_diagnostics(
        r#"
declare const marker: unique symbol;
interface Payload { count: number; }
interface Bag { [key: symbol]: Payload; }
const v2: Bag = { [marker]: { count: "no" } };
"#,
    );
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2322),
        "expected the nested TS2322 for a symbol-only index target, got: {diags:?}"
    );
}

#[test]
fn symbol_index_alias_target_valid_symbol_and_string_props_clean() {
    // Negative control through an alias wrapper: a matching symbol-keyed
    // nested literal plus a matching string-keyed property emit nothing.
    let diags = check_source_diagnostics(
        r#"
declare const marker: unique symbol;
interface Payload { count: number; }
interface Bag { [key: string]: number; [key: symbol]: Payload; }
type BagAlias = Bag;
const v3: BagAlias = { [marker]: { count: 2 }, plain: 3 };
"#,
    );
    assert!(
        diags.is_empty(),
        "expected no diagnostics for matching symbol + string keyed members, got: {diags:?}"
    );
}

#[test]
fn symbol_member_absorbed_by_wide_string_index_backwards_compat() {
    // tsc "Permitted for backwards compatibility" (indexSignatures1.ts): a
    // WIDE `[k: string]` index with no symbol index absorbs a symbol-keyed
    // member outright — no excess report and no value check, even when the
    // value mismatches the string index's value type or carries a nested
    // excess property. Oracled on 7.0.2.
    let mismatched_value = check_source_diagnostics(
        r#"
declare const marker: unique symbol;
const o2: { [key: string]: string } = { [marker]: 42 };
"#,
    );
    assert!(
        mismatched_value.is_empty(),
        "wide string index absorbs a symbol member without a value check, got: {mismatched_value:?}"
    );

    let nested_excess = check_source_diagnostics(
        r#"
declare const marker: unique symbol;
const o3: { [key: string]: { n: number } } = { [marker]: { n: 1, extra: 2 } };
"#,
    );
    assert!(
        nested_excess.is_empty(),
        "wide string index absorbs a symbol member without a nested drill-in, got: {nested_excess:?}"
    );
}

#[test]
fn ts2353_symbol_member_not_absorbed_by_template_string_index() {
    // The backwards-compat absorption is specific to the wide `string` key: a
    // template-literal string index does not cover a symbol-keyed member, so
    // with no symbol index the member is excess. Oracled on 7.0.2.
    let diags = check_source_diagnostics(
        r#"
declare const marker: unique symbol;
const t3: { [key: `data${string}`]: string } = { [marker]: 42 };
"#,
    );
    let ts2353: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
    assert_eq!(
        ts2353.len(),
        1,
        "expected TS2353 for a symbol member against a template-key-only index target, got: {diags:?}"
    );
}

#[test]
fn relation_symbol_member_not_checked_against_string_only_index() {
    // Relation level (non-fresh source): a symbol-keyed property is simply
    // not constrained by a string-only-index target — assignment is clean.
    // With a symbol index present and violated, the relation fails (covered
    // by the flat-TS2418 in-source test via the fresh-literal path).
    let diags = check_source_diagnostics(
        r#"
declare const marker: unique symbol;
declare const src: { [marker]: number };
const t1: { [key: string]: string } = src;
"#,
    );
    assert!(
        diags.is_empty(),
        "a symbol-keyed property is not constrained by a string-only index target, got: {diags:?}"
    );
}

#[test]
fn symbol_index_and_string_index_each_check_their_own_props() {
    // The string-keyed property still checks against the STRING index
    // value even when a symbol-keyed member is present and fine: the two
    // index infos apply independently per property.
    let clean = check_source_diagnostics(
        r#"
declare const marker: unique symbol;
interface Bag { [key: string]: number; [key: symbol]: string; }
const v4: Bag = { [marker]: "fine", plain: 1 };
"#,
    );
    assert!(
        clean.is_empty(),
        "expected no diagnostics when each key matches its own index, got: {clean:?}"
    );

    let bad_string_prop = check_source_diagnostics(
        r#"
declare const marker: unique symbol;
interface Bag { [key: string]: number; [key: symbol]: string; }
const v5: Bag = { [marker]: "fine", plain: "bad" };
"#,
    );
    let codes: Vec<u32> = bad_string_prop.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2322),
        "expected TS2322 on the string-keyed property against the string index, got: {bad_string_prop:?}"
    );
    assert!(
        !codes.contains(&2418),
        "the symbol-keyed member is fine and must not report, got: {bad_string_prop:?}"
    );
}
