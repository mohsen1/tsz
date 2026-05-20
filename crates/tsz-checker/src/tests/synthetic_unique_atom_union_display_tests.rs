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
