//! TS2411 with template-literal (pattern) index signatures.
//!
//! A template-literal pattern index signature (`[k: `id_${number}`]: V`) is
//! stored in the same `string_index` slot as a plain `[k: string]` signature,
//! but it constrains only property names that *match the pattern*. tsc checks a
//! property against an index signature only when the property's name type is
//! assignable to the index's key type, and renders the pattern (not `string`)
//! as the index kind in the diagnostic.
//!
//! Before this was gated, every string-named property was checked against the
//! pattern's value type (a TS2411 false positive on non-matching properties such
//! as `size`), and matching properties were mislabeled `'string' index type`.

use tsz_checker::test_utils::{check_source_strict_codes, check_source_strict_messages};

fn ts2411_count(source: &str) -> usize {
    check_source_strict_codes(source)
        .into_iter()
        .filter(|code| *code == 2411)
        .count()
}

#[test]
fn pattern_index_does_not_constrain_non_matching_property() {
    // `size` does not match `id_${number}`, so tsc reports nothing.
    let source = r#"
interface Registry {
    [key: `id_${number}`]: string;
    size: number;
}
"#;
    assert_eq!(
        ts2411_count(source),
        0,
        "a pattern index signature must not constrain a non-matching property"
    );
}

#[test]
fn pattern_index_constrains_matching_property_with_pattern_label() {
    // `id_1` matches `id_${number}`; its `number` value is incompatible with the
    // `string` index value, so tsc reports TS2411 — labeled with the pattern.
    let source = r#"
interface Registry {
    [key: `id_${number}`]: string;
    id_1: number;
}
"#;
    let messages = check_source_strict_messages(source);
    let hit = messages.iter().find(|(code, _)| *code == 2411);
    assert!(
        hit.is_some_and(|(_, m)| m.contains("'id_1'")
            && m.contains("`id_${number}`")
            && m.contains("index type 'string'")
            && !m.contains("'string' index type")),
        "matching property must report TS2411 labeled with the pattern key, got: {messages:?}"
    );
}

#[test]
fn pattern_index_accepts_matching_property_with_compatible_value() {
    let source = r#"
interface Registry {
    [key: `id_${number}`]: string;
    id_1: string;
}
"#;
    assert_eq!(
        ts2411_count(source),
        0,
        "a matching property with a compatible value type is fine"
    );
}

#[test]
fn union_of_patterns_only_constrains_matching_names() {
    // Neither member of `cother` matches an `a${string}` / `b${string}` pattern.
    let source = r#"
interface Bus {
    [evt: `a${string}` | `b${string}`]: string;
    cother: number;
}
"#;
    assert_eq!(
        ts2411_count(source),
        0,
        "a union-of-patterns key must not constrain a name matching no member"
    );
}

#[test]
fn prefix_pattern_reports_only_matching_property() {
    // `p_x` matches `p_${string}`; `other` does not.
    let source = r#"
interface Slots {
    [slot: `p_${string}`]: string;
    p_x: number;
    other: boolean;
}
"#;
    assert_eq!(
        ts2411_count(source),
        1,
        "only the pattern-matching property is constrained, not the unrelated one"
    );
}

#[test]
fn plain_string_index_still_constrains_every_property() {
    // Control: a plain `string` index is unchanged — it constrains all
    // string-named properties and is labeled `'string' index type`.
    let source = r#"
interface Plain {
    [key: string]: string;
    size: number;
}
"#;
    let messages = check_source_strict_messages(source);
    let hit = messages.iter().find(|(code, _)| *code == 2411);
    assert!(
        hit.is_some_and(|(_, m)| m.contains("'size'") && m.contains("'string' index type 'string'")),
        "a plain string index keeps its `string` label and constrains every property, got: {messages:?}"
    );
}

#[test]
fn pattern_index_inherited_does_not_constrain_non_matching_derived_property() {
    // The pattern index is inherited; the derived non-matching `label`
    // must not be constrained by it.
    let source = r#"
interface Base {
    [key: `id_${number}`]: string;
}
interface Derived extends Base {
    label: number;
}
"#;
    assert_eq!(
        ts2411_count(source),
        0,
        "an inherited pattern index must not constrain a non-matching derived property"
    );
}

#[test]
fn pattern_index_rule_is_not_binder_name_dependent() {
    // Anti-hardcoding: rename the interface, the pattern prefix, the index
    // parameter, and the properties — behavior is identical to the canonical
    // cases (one matching error, zero non-matching errors).
    let source = r#"
interface Catalogue {
    [slot: `sku_${number}`]: string;
    sku_7: number;
    weight: number;
}
"#;
    let messages = check_source_strict_messages(source);
    let count = messages.iter().filter(|(code, _)| *code == 2411).count();
    assert_eq!(
        count, 1,
        "renamed binders must behave identically (only `sku_7` matches), got: {messages:?}"
    );
    assert!(
        messages.iter().any(|(code, m)| *code == 2411
            && m.contains("'sku_7'")
            && m.contains("`sku_${number}`")),
        "the matching property is labeled with its own pattern, got: {messages:?}"
    );
}

#[test]
fn derived_pattern_index_does_not_constrain_non_matching_type_alias_base_property() {
    // A derived interface that adds a pattern index over a *type-alias* base
    // must not constrain the base's non-matching properties — exercises the
    // `check_type_alias_base_properties_against_derived_string_index` path.
    let source = r#"
type Base = { foo: number; bar: string };
interface Derived extends Base {
    [k: `id_${number}`]: string;
}
"#;
    assert_eq!(
        ts2411_count(source),
        0,
        "a derived pattern index must not constrain non-matching type-alias base properties"
    );
}

#[test]
fn derived_pattern_index_reports_matching_type_alias_base_property_once() {
    // The matching base property is reported exactly once (the inherited-property
    // and type-alias-base heritage paths must agree on the pattern label so they
    // dedupe to a single diagnostic, not two).
    let source = r#"
type Base = { id_5: number };
interface Derived extends Base {
    [k: `id_${number}`]: string;
}
"#;
    let messages = check_source_strict_messages(source);
    let hits: Vec<_> = messages.iter().filter(|(code, _)| *code == 2411).collect();
    assert_eq!(
        hits.len(),
        1,
        "the matching base property is reported exactly once, got: {messages:?}"
    );
    assert!(
        hits[0].1.contains("'id_5'") && hits[0].1.contains("`id_${number}`"),
        "the single diagnostic is labeled with the pattern key, got: {messages:?}"
    );
}
