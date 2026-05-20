//! Tests for TS2322 source-display preservation against `undefined` / `null` targets.
//!
//! tsc preserves the source's literal surface in TS2322 diagnostics whose target
//! is `undefined` or `null` — the user wrote a concrete value (`1`, `""`, `true`)
//! and the diagnostic should echo that value back rather than its widened
//! primitive base. tsz mirrors this for boolean keywords, string literals,
//! template literals, and signed numeric / bigint literals.
//!
//! Conformance test: `invalidUndefinedValues.ts`.

fn compile_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(source)
}

fn ts2322(diags: &[(u32, String)]) -> Vec<&str> {
    diags
        .iter()
        .filter_map(|(code, msg)| (*code == 2322).then_some(msg.as_str()))
        .collect()
}

#[test]
fn ts2322_preserves_number_literal_against_undefined_target() {
    let diags = compile_diagnostics(
        r#"
var x: typeof undefined;
x = 1;
"#,
    );
    let msgs = ts2322(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("Type '1'") && m.contains("'undefined'")),
        "expected literal '1' preserved against 'undefined', got: {msgs:?}"
    );
}

#[test]
fn ts2322_preserves_string_literal_against_undefined_target() {
    let diags = compile_diagnostics(
        r#"
var x: typeof undefined;
x = '';
"#,
    );
    let msgs = ts2322(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("Type '\"\"'") && m.contains("'undefined'")),
        "expected literal '\"\"' preserved against 'undefined', got: {msgs:?}"
    );
}

#[test]
fn ts2322_preserves_true_against_undefined_target() {
    let diags = compile_diagnostics(
        r#"
var x: typeof undefined;
x = true;
"#,
    );
    let msgs = ts2322(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("Type 'true'") && m.contains("'undefined'")),
        "expected preserved 'true' against 'undefined', got: {msgs:?}"
    );
}

#[test]
fn ts2322_preserves_string_literal_against_string_literal_target() {
    let diags = compile_diagnostics(
        r#"
let x: "a" = "b";
"#,
    );
    let msgs = ts2322(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("Type '\"b\"'") && m.contains("'\"a\"'")),
        "expected literal '\"b\"' kept against literal '\"a\"', got: {msgs:?}"
    );
}
