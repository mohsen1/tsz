//! Diagnostic-display tests for template-literal / string-intrinsic aliases that
//! reduce to a finite literal union (issue #14790).
//!
//! Structural rule: a type alias whose body is a template-literal type
//! (`` `${"a" | "b"}-x` ``) or a string-mapping intrinsic
//! (`Capitalize<"foo" | "bar">`) and which reduces to a finite union of literals
//! carries no `aliasSymbol` in tsc — `getTemplateLiteralType` /
//! `getStringMappingType` build the union directly via `getUnionType`. tsc prints
//! the expanded union (`"a-x" | "b-x"`, `"Foo" | "Bar"`); tsz previously kept the
//! alias name because the reduced union was registered as the alias body and the
//! reverse `find_def_for_type` lookup repainted it.
//!
//! The rule is keyed on the body's syntactic form (template literal) or the raw
//! `StringIntrinsic` shape, never the alias name: the cases use distinct alias
//! spellings (`Pair`, `Flag`, `Caps`, `Loud`) to guard against name hardcoding
//! (§25), and each asserts the alias name is absent from the rendered message.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn ts2322(source: &str) -> String {
    let diagnostics = check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            strict_function_types: true,
            ..CheckerOptions::default()
        },
    );
    diagnostics
        .iter()
        .find(|diag| diag.code == 2322)
        .unwrap_or_else(|| panic!("expected TS2322, got: {diagnostics:#?}"))
        .message_text
        .clone()
}

#[test]
fn template_literal_union_alias_target_expands() {
    // Primary repro from #14790.
    let msg = ts2322(
        r#"
type Pair = `${"a" | "b"}-x`;
const bad: Pair = "z";
"#,
    );
    assert!(
        msg.contains(r#"Type '"z"' is not assignable to type '"a-x" | "b-x"'."#),
        "template-literal alias target must expand to the literal union, got: {msg}"
    );
    assert!(
        !msg.contains("'Pair'"),
        "target must not keep the alias name `Pair`, got: {msg}"
    );
}

#[test]
fn template_literal_boolean_span_alias_expands() {
    let msg = ts2322(
        r#"
type Flag = `flag-${boolean}`;
const bad: Flag = "z";
"#,
    );
    assert!(
        msg.contains(r#"Type '"z"' is not assignable to type '"flag-false" | "flag-true"'."#),
        "`flag-${{boolean}}` alias must expand to the literal union, got: {msg}"
    );
    assert!(!msg.contains("'Flag'"), "got: {msg}");
}

#[test]
fn capitalize_literal_union_alias_expands() {
    let msg = ts2322(
        r#"
type Caps = Capitalize<"foo" | "bar">;
const bad: Caps = "z";
"#,
    );
    assert!(
        msg.contains(r#"Type '"z"' is not assignable to type '"Foo" | "Bar"'."#),
        "Capitalize over a literal union must expand, got: {msg}"
    );
    assert!(!msg.contains("'Caps'"), "got: {msg}");
}

#[test]
fn uppercase_literal_union_alias_expands() {
    let msg = ts2322(
        r#"
type Loud = Uppercase<"a" | "b">;
const bad: Loud = "z";
"#,
    );
    assert!(
        msg.contains(r#"Type '"z"' is not assignable to type '"A" | "B"'."#),
        "Uppercase over a literal union must expand, got: {msg}"
    );
    assert!(!msg.contains("'Loud'"), "got: {msg}");
}

#[test]
fn directly_written_literal_union_alias_keeps_name() {
    // Control: a directly-written literal-union alias keeps its name in BOTH tsz
    // and tsc — its body node is a `UnionType`, never a template/intrinsic, so it
    // never reaches the computed-body arms. This proves the fix did not broadly
    // disable literal-union alias names.
    let msg = ts2322(
        r#"
type Plain = "a-x" | "b-x";
const bad: Plain = "z";
"#,
    );
    assert!(
        msg.contains("is not assignable to type 'Plain'."),
        "directly-written union alias must keep its name, got: {msg}"
    );
}
