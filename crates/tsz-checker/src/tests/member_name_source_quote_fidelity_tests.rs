//! Regression tests for issue #16213: a string-literal member name loses its
//! source quote character in the implicit-any diagnostics (TS7008 / TS7010 /
//! TS7032 / TS7033).
//!
//! `tsc` names a declaration through `declarationNameToString`, which is
//! `getTextOfNode` — the name node's verbatim source spelling, with no quote
//! convention of its own. Each of these diagnostics' own message template
//! already wraps the placeholder in a literal `'{0}'`, so the name text
//! itself must not add a second layer of quoting, and must not normalize
//! which quote character the author typed.
//!
//! Verified against an in-container `tsc@6.0.2` oracle
//! (`/opt/node22/lib/node_modules/typescript/lib/tsc.js --noEmit --strict
//! --lib es2022 --target es2022 --pretty false`):
//!
//! ```text
//! declare class C { get "foo"(); }   TS7033  Property '"foo"' implicitly has type 'any', ...
//! interface I { get "foo"(); }       TS7033  Property '"foo"' implicitly has type 'any', ...
//! declare class C { "foo"(); }       TS7010  '"foo"', which lacks return-type annotation, ...
//! declare class C { "foo"; }         TS7008  Member '"foo"' implicitly has an 'any' type.
//! declare class C { get 'bar'(); }   TS7033  Property ''bar'' implicitly has type 'any', ...
//! ```
//!
//! A single-quoted source name legitimately renders as `''bar''` — the
//! template's own `'{0}'` wraps whatever quote character the author used, so
//! doubled single quotes are correct precisely when the source used single
//! quotes, and wrong precisely when it used double quotes (the pre-fix bug).

use std::sync::{Arc, OnceLock};
use tsz_binder::lib_loader::LibFile;

use crate::CheckerOptions;
use crate::test_utils::{check_source_with_libs_code_messages, load_default_lib_files};

fn default_libs() -> &'static [Arc<LibFile>] {
    static DEFAULT_LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
    DEFAULT_LIBS.get_or_init(load_default_lib_files)
}

fn check(src: &str) -> Vec<(u32, String)> {
    check_source_with_libs_code_messages(src, "test.ts", CheckerOptions::default(), default_libs())
}

fn assert_single(src: &str, code: u32, expected_message: &str) {
    let diags = check(src);
    assert_eq!(
        diags,
        vec![(code, expected_message.to_string())],
        "source: {src}"
    );
}

#[test]
fn ts7033_getter_double_quoted_name_keeps_double_quotes() {
    assert_single(
        r#"declare class C { get "foo"(); }"#,
        7033,
        "Property '\"foo\"' implicitly has type 'any', because its get accessor lacks a return type annotation.",
    );
}

#[test]
fn ts7033_interface_getter_double_quoted_name_keeps_double_quotes() {
    assert_single(
        r#"interface I { get "foo"(); }"#,
        7033,
        "Property '\"foo\"' implicitly has type 'any', because its get accessor lacks a return type annotation.",
    );
}

#[test]
fn ts7033_type_literal_getter_double_quoted_name_keeps_double_quotes() {
    assert_single(
        r#"declare const t: { get "foo"(); };"#,
        7033,
        "Property '\"foo\"' implicitly has type 'any', because its get accessor lacks a return type annotation.",
    );
}

#[test]
fn ts7032_setter_double_quoted_name_keeps_double_quotes() {
    let diags = check(r#"declare class C { set "foo"(v); }"#);
    assert_eq!(diags.len(), 2, "expected TS7006 + TS7032, got: {diags:?}");
    assert!(
        diags.contains(&(
            7032,
            "Property '\"foo\"' implicitly has type 'any', because its set accessor lacks a parameter type annotation.".to_string()
        )),
        "missing quote-faithful TS7032, got: {diags:?}"
    );
    assert!(
        diags.contains(&(
            7006,
            "Parameter 'v' implicitly has an 'any' type.".to_string()
        )),
        "missing TS7006, got: {diags:?}"
    );
}

#[test]
fn ts7033_getter_single_quoted_name_keeps_single_quotes() {
    assert_single(
        r#"declare class C { get 'bar'(); }"#,
        7033,
        "Property ''bar'' implicitly has type 'any', because its get accessor lacks a return type annotation.",
    );
}

#[test]
fn ts7010_ambient_method_double_quoted_name_keeps_double_quotes() {
    assert_single(
        r#"declare class C { "foo"(); }"#,
        7010,
        "'\"foo\"', which lacks return-type annotation, implicitly has an 'any' return type.",
    );
}

#[test]
fn ts7008_property_double_quoted_name_keeps_double_quotes() {
    assert_single(
        r#"declare class C { "foo"; }"#,
        7008,
        "Member '\"foo\"' implicitly has an 'any' type.",
    );
}

#[test]
fn ts7008_property_single_quoted_name_keeps_single_quotes() {
    assert_single(
        r#"declare class C { 'foo'; }"#,
        7008,
        "Member ''foo'' implicitly has an 'any' type.",
    );
}

#[test]
fn ts7032_type_literal_property_signature_double_quoted_name_keeps_double_quotes() {
    let diags = check(r#"declare const t: { "foo"(); };"#);
    assert!(
        diags.iter().any(|(code, msg)| *code == 7010
            && msg == "'\"foo\"', which lacks return-type annotation, implicitly has an 'any' return type."),
        "expected a TS7010 row with the double-quoted spelling, got: {diags:?}"
    );
}

/// Control: a plain (non-computed, non-literal) identifier name must keep
/// rendering unquoted (the message template supplies the quotes).
#[test]
fn identifier_name_unaffected() {
    assert_single(
        "declare class C { foo; }",
        7008,
        "Member 'foo' implicitly has an 'any' type.",
    );
}

/// Control: a numeric-literal name's raw source spelling must stay
/// uncanonicalized (`1.0`, not `1`) — this path was not touched by this fix
/// and must not regress.
#[test]
fn numeric_name_source_spelling_preserved() {
    assert_single(
        "declare class C { 1.0; }",
        7008,
        "Member '1.0' implicitly has an 'any' type.",
    );
}

/// Control: an escaped quote inside the string-literal name must survive
/// verbatim, proving the fix reads the source span rather than re-deriving
/// a quote convention from `lit.text`.
#[test]
fn embedded_escaped_quote_survives_verbatim() {
    assert_single(
        r#"declare class C { "a\"b"; }"#,
        7008,
        "Member '\"a\\\"b\"' implicitly has an 'any' type.",
    );
}
