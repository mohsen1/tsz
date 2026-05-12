//! Tests for TS2353 against mapped types with template-literal key constraints.
//!
//! Issue #5882: For non-generic mapped types like `{ [K in `data-${string}`]: V }`,
//! tsz emitted TS2353 for every source property because the solver's
//! `is_key_in_mapped_constraint` had no case for `TypeData::TemplateLiteral`
//! and fell through to a conservative "reject" branch. The fix matches the
//! source property name against the template literal pattern.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_lib_files};

fn diags(source: &str) -> Vec<(u32, String)> {
    let libs = load_lib_files(&["es5.d.ts"]);
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn template_literal_key_constraint_accepts_matching_property() {
    let source = r#"
type TemplateIndex = { [K in `data-${string}`]: string };
const ti: TemplateIndex = { "data-id": "123", "data-name": "test" };
"#;
    let ds = diags(source);
    let ts2353: Vec<_> = ds.iter().filter(|d| d.0 == 2353).collect();
    assert!(
        ts2353.is_empty(),
        "Expected no TS2353 for keys matching the template pattern, got: {ts2353:?}",
    );
}

#[test]
fn template_literal_key_constraint_rejects_non_matching_property() {
    let source = r#"
type TemplateIndex = { [J in `data-${string}`]: string };
const ti: TemplateIndex = { "data-id": "ok", "other": "no" };
"#;
    let ds = diags(source);
    let ts2353: Vec<_> = ds.iter().filter(|d| d.0 == 2353).collect();
    assert_eq!(
        ts2353.len(),
        1,
        "Expected exactly one TS2353 for the non-matching key, got: {ts2353:?}",
    );
    assert!(
        ts2353[0].1.contains("'other'"),
        "Expected TS2353 to mention 'other', got: {}",
        ts2353[0].1
    );
}

#[test]
fn template_literal_key_constraint_with_alternate_iteration_var_name() {
    // Confirms the fix is not pinned to a specific iteration-variable name.
    let source = r#"
type TemplateIndex = { [Q in `data-${string}`]: string };
const ti: TemplateIndex = { "data-id": "123" };
"#;
    let ds = diags(source);
    let ts2353: Vec<_> = ds.iter().filter(|d| d.0 == 2353).collect();
    assert!(
        ts2353.is_empty(),
        "Expected no TS2353 for matching key under alternate iter name, got: {ts2353:?}",
    );
}

#[test]
fn template_literal_key_constraint_with_number_segment() {
    let source = r#"
type NumIndex = { [K in `item-${number}`]: string };
const ni: NumIndex = { "item-1": "a", "item-42": "b" };
"#;
    let ds = diags(source);
    let ts2353: Vec<_> = ds.iter().filter(|d| d.0 == 2353).collect();
    assert!(
        ts2353.is_empty(),
        "Expected no TS2353 for keys matching `item-${{number}}`, got: {ts2353:?}",
    );
}

#[test]
fn template_literal_key_constraint_suffix_segment() {
    let source = r#"
type SuffixIndex = { [K in `${string}-end`]: string };
const si: SuffixIndex = { "foo-end": "a", "bar-end": "b" };
"#;
    let ds = diags(source);
    let ts2353: Vec<_> = ds.iter().filter(|d| d.0 == 2353).collect();
    assert!(
        ts2353.is_empty(),
        "Expected no TS2353 for suffix template pattern, got: {ts2353:?}",
    );
}
