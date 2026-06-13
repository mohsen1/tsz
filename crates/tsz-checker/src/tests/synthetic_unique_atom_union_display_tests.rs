//! Regression coverage for synthetic `__unique_<n>` atoms, which encode unique
//! symbol keys internally. They must behave as real unique-symbol keys for
//! `keyof`, and they must not leak into diagnostic display.

use crate::test_utils::check_source_diagnostics;

#[test]
fn keyof_with_unique_symbol_keys_strips_synthetic_atom_from_union_display() {
    let diags = check_source_diagnostics(
        r#"
declare const sym: unique symbol;
interface StrNum {
    first: number;
    second: number;
    [sym]: number;
}
declare function pickKey<K extends keyof StrNum>(k: K): K;
const result: "first" = pickKey(sym);
"#,
    );

    let ts2345: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert!(
        ts2345.is_empty(),
        "unique symbol property keys should be part of keyof StrNum; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "pickKey(sym) should be accepted and then fail on assignment to \"first\"; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    for d in &ts2322 {
        let msg = &d.message_text;
        assert!(
            !msg.contains("__unique_"),
            "diagnostics must not surface synthetic __unique_<n> atoms; got: {msg}"
        );
    }
}

#[test]
fn keyof_keeps_user_authored_unique_like_string_property_as_string_key() {
    let source = r#"
interface Weird {
    "__unique_1": string;
}

declare let key: keyof Weird;
const lit: "__unique_1" = key;
"#;

    let diags = check_source_diagnostics(source);
    assert!(
        diags.is_empty(),
        "a user-authored string key that looks like an internal unique-symbol key must remain a string key; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

// Regression coverage for #13402: a `unique symbol` const that is name-merged
// with a same-named `type X = typeof X` alias must still key a computed member
// `[X]` by the symbol's own value identity. Before the fix, value-position
// resolution of `X` degraded to the general `symbol` type, so the interface
// member and the object literal minted disagreeing keys and produced a spurious
// TS2322 / phantom `__unique_NNNNN` member.

fn assert_clean(diags: &[crate::diagnostics::Diagnostic], context: &str) {
    assert!(
        diags.is_empty(),
        "{context}; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn namemerged_unique_symbol_method_key_is_symbol_keyed() {
    let diags = check_source_diagnostics(
        r#"
declare const tag: unique symbol;
type tag = typeof tag;
interface Protocol { [tag](): number; }
const impl: Protocol = { [tag]: () => 1 };
"#,
    );
    assert_clean(
        &diags,
        "name-merged unique-symbol method key should be clean",
    );
}

#[test]
fn namemerged_unique_symbol_property_key_is_symbol_keyed() {
    let diags = check_source_diagnostics(
        r#"
declare const t2: unique symbol;
type t2 = typeof t2;
interface HA2 { [t2]: number; }
const ha2: HA2 = { [t2]: 1 };
"#,
    );
    assert_clean(
        &diags,
        "name-merged unique-symbol property key should be clean",
    );
}

#[test]
fn unique_symbol_key_without_alias_is_symbol_keyed() {
    // Control: same shape with no `type X = typeof X` merge. Isolates the
    // defect to the value/type name-merge rather than unique-symbol keys.
    let diags = check_source_diagnostics(
        r#"
declare const k2: unique symbol;
interface H2 { [k2](): number; }
const h2: H2 = { [k2]: () => 1 };
"#,
    );
    assert_clean(&diags, "unique-symbol key without an alias should be clean");
}

#[test]
fn namemerged_unique_symbol_renamed_binder_is_symbol_keyed() {
    // Structural, not name-keyed: a differently named binder behaves the same.
    let diags = check_source_diagnostics(
        r#"
declare const brandKey: unique symbol;
type brandKey = typeof brandKey;
interface Brand { [brandKey](): string; }
const b: Brand = { [brandKey]: () => "x" };
"#,
    );
    assert_clean(
        &diags,
        "renamed name-merged unique-symbol key should be clean",
    );
}

#[test]
fn namemerged_unique_symbol_value_read_keeps_unique_identity() {
    // The value identity must survive the merge: `tag` read as a value keeps
    // its own `typeof tag` identity, so it is assignable to the alias type and
    // not to a foreign `unique symbol`.
    let diags = check_source_diagnostics(
        r#"
declare const tag: unique symbol;
type tag = typeof tag;
declare const other: unique symbol;
const ok: tag = tag;
const bad: typeof other = tag;
"#,
    );
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2322),
        "assigning `tag` to a foreign unique symbol must still fail (TS2322); got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        codes.iter().filter(|&&c| c == 2322).count(),
        1,
        "only the foreign assignment should fail; `const ok: tag = tag` must be clean; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn keyof_keeps_computed_unique_like_string_property_as_string_key() {
    let source = r#"
const k = "__unique_1" as const;
interface Weird {
    [k]: string;
}

declare let key: keyof Weird;
const lit: "__unique_1" = key;
"#;

    let diags = check_source_diagnostics(source);
    assert!(
        diags.is_empty(),
        "a computed string key that looks like an internal unique-symbol key must remain a string key; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}
