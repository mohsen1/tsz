//! Regression for the `extractInferenceImprovement` conformance failure:
//! synthetic `__unique_<n>` string-literal atoms (which encode unique
//! symbol keys internally) must be stripped from union display, since
//! tsc never surfaces them in diagnostics.

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
        !ts2345.is_empty(),
        "expected TS2345 from passing 'sym' to keyof StrNum; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    for d in &ts2345 {
        let msg = &d.message_text;
        assert!(
            !msg.contains("__unique_"),
            "TS2345 must not surface synthetic __unique_<n> atom in union display; got: {msg}"
        );
    }
}
