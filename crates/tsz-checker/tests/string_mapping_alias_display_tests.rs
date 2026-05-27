//! Diagnostic-display tests for string-mapping intrinsic aliases in TS2322.
//!
//! Structural rule: when the failing *source* of a TS2322 resolves to a
//! deferred string-mapping intrinsic (`Uppercase`/`Lowercase`/`Capitalize`/
//! `Uncapitalize` over a non-literal argument), tsc renders the structural
//! `Intrinsic<arg>` form — never the type-alias name — in both source and
//! target positions. tsz previously evaluated the *target* annotation to the
//! structural intrinsic but rewrote the *source* back to its declared alias
//! name, leaking an asymmetric `U` (source) vs `Uppercase<string>` (target).
//!
//! The rule is keyed on the structural `StringIntrinsic` shape, so it is
//! independent of the chosen alias name: each case below is repeated with a
//! different alias spelling to guard against name hardcoding (§25).

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
fn lowercase_alias_source_renders_structurally() {
    let msg = ts2322(
        r#"
type Lower = Lowercase<string>;
declare let v: Lower;
const x: Uppercase<string> = v;
"#,
    );
    assert!(
        msg.contains("Type 'Lowercase<string>' is not assignable to type 'Uppercase<string>'."),
        "source string-mapping alias must render structurally, got: {msg}"
    );
    assert!(
        !msg.contains("'Lower'"),
        "source must not leak the alias name `Lower`, got: {msg}"
    );
}

#[test]
fn lowercase_alias_source_is_name_independent() {
    // Same shape, different alias spelling: the structural rule must not depend
    // on the user-chosen alias name.
    let msg = ts2322(
        r#"
type Zebra = Lowercase<string>;
declare let v: Zebra;
const x: Uppercase<string> = v;
"#,
    );
    assert!(
        msg.contains("Type 'Lowercase<string>' is not assignable to type 'Uppercase<string>'."),
        "renamed source alias must still render structurally, got: {msg}"
    );
    assert!(
        !msg.contains("'Zebra'"),
        "source must not leak the alias name `Zebra`, got: {msg}"
    );
}

#[test]
fn uppercase_alias_source_renders_structurally() {
    let msg = ts2322(
        r#"
type Upper = Uppercase<string>;
declare let v: Upper;
const x: Lowercase<string> = v;
"#,
    );
    assert!(
        msg.contains("Type 'Uppercase<string>' is not assignable to type 'Lowercase<string>'."),
        "source string-mapping alias must render structurally, got: {msg}"
    );
    assert!(!msg.contains("'Upper'"), "got: {msg}");
}

#[test]
fn capitalize_alias_source_renders_structurally() {
    let msg = ts2322(
        r#"
type Cap = Capitalize<string>;
declare let v: Cap;
const x: `A${string}` = v;
"#,
    );
    assert!(
        msg.contains("Type 'Capitalize<string>' is not assignable to type '`A${string}`'."),
        "source string-mapping alias must render structurally, got: {msg}"
    );
    assert!(!msg.contains("'Cap'"), "got: {msg}");
}

#[test]
fn uncapitalize_alias_source_renders_structurally() {
    let msg = ts2322(
        r#"
type Unc = Uncapitalize<string>;
declare let v: Unc;
const x: `a${string}` = v;
"#,
    );
    assert!(
        msg.contains("Type 'Uncapitalize<string>' is not assignable to type '`a${string}`'."),
        "source string-mapping alias must render structurally, got: {msg}"
    );
    assert!(!msg.contains("'Unc'"), "got: {msg}");
}

#[test]
fn capitalize_alias_source_is_name_independent() {
    // Different alias spelling for `Capitalize` proves the rule is keyed on the
    // structural shape, not the alias name `Cap`.
    let msg = ts2322(
        r#"
type Headline = Capitalize<string>;
declare let v: Headline;
const x: `A${string}` = v;
"#,
    );
    assert!(
        msg.contains("Type 'Capitalize<string>' is not assignable to type '`A${string}`'."),
        "renamed Capitalize source must render structurally, got: {msg}"
    );
    assert!(!msg.contains("'Headline'"), "got: {msg}");
}

#[test]
fn uncapitalize_alias_source_is_name_independent() {
    let msg = ts2322(
        r#"
type Slug = Uncapitalize<string>;
declare let v: Slug;
const x: `a${string}` = v;
"#,
    );
    assert!(
        msg.contains("Type 'Uncapitalize<string>' is not assignable to type '`a${string}`'."),
        "renamed Uncapitalize source must render structurally, got: {msg}"
    );
    assert!(!msg.contains("'Slug'"), "got: {msg}");
}

#[test]
fn nested_string_mapping_alias_source_renders_structurally() {
    let msg = ts2322(
        r#"
type Nested = Lowercase<Uppercase<string>>;
declare let v: Nested;
const x: Uppercase<string> = v;
"#,
    );
    assert!(
        msg.contains(
            "Type 'Lowercase<Uppercase<string>>' is not assignable to type 'Uppercase<string>'."
        ),
        "nested string-mapping alias source must render the full structural form, got: {msg}"
    );
    assert!(!msg.contains("'Nested'"), "got: {msg}");
}

#[test]
fn non_string_mapping_object_alias_source_keeps_its_name() {
    // Negative case: the structural-display rule is scoped to string-mapping
    // intrinsics. An ordinary object/type alias must still preserve its name in
    // the source position, exactly as before — proving the fix did not broadly
    // disable declared-alias source display.
    let msg = ts2322(
        r#"
type Named = { a: number };
declare let v: Named;
const x: boolean = v;
"#,
    );
    assert!(
        msg.contains("Type 'Named' is not assignable to type 'boolean'."),
        "ordinary object alias must keep its name in the source position, got: {msg}"
    );
}
