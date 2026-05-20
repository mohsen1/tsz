//! Two related failure surfaces for strict-mode reserved keywords used as
//! parameter names:
//!
//! 1. **Function-type parsing.** `(private) => void` and similar function
//!    types had their parameter-modifier lookahead unconditionally eat the
//!    reserved keyword, then concluded "not a function type" when it found
//!    `)` next. The outer parenthesized-type branch then tried to parse
//!    `private` as a type identifier and failed with cascading TS1005/
//!    TS1109/TS1144. Fix: when the modifier-eating lookahead leaves us at
//!    a non-parameter-name token, roll back so the keyword is treated as
//!    the parameter name.
//!
//! 2. **TS7051 vs TS7006 routing for non-modifier reserved-word params.**
//!    `function f(implements) {}` got TS7051 ("Did you mean 'arg0:
//!    implements'?") because the unconditional list-based heuristic
//!    treated all non-modifier reserved words as "probably types". tsc
//!    only emits TS7051 for these names when the enclosing function-like
//!    is a *signature without a body* (interface method, call signature,
//!    construct signature) where types are conventionally required.
//!    Fix: gate the upgrade on the signature-only context.

use tsz_checker::test_utils::check_source_diagnostics;

fn count(diags: &[tsz_checker::diagnostics::Diagnostic], code: u32) -> usize {
    diags.iter().filter(|d| d.code == code).count()
}

/// Function-type parameter using `private` as the name must parse cleanly
/// (no TS1005/TS1109/TS1144) and produce only the type-level diagnostics
/// that tsc emits for reserved-word usage in strict mode.
#[test]
fn function_type_with_reserved_keyword_parameter_parses() {
    let source = r#"
function barn(cb: (private) => void) { }
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(count(&diags, 1005), 0, "no TS1005; got: {diags:#?}");
    assert_eq!(count(&diags, 1109), 0, "no TS1109; got: {diags:#?}");
    assert_eq!(count(&diags, 1144), 0, "no TS1144; got: {diags:#?}");
}

/// Anti-hardcoding (§25): the parser fix is "any modifier-shaped keyword
/// followed by `)` is the parameter name". Re-run with multiple keywords.
#[test]
fn function_type_with_various_reserved_keywords_as_param_name() {
    for kw in ["private", "public", "protected", "readonly"] {
        let source = format!("declare function f(cb: ({kw}) => void): void;\n");
        let diags = check_source_diagnostics(&source);
        assert_eq!(
            count(&diags, 1005),
            0,
            "no TS1005 for keyword '{kw}'; got: {diags:#?}"
        );
        assert_eq!(
            count(&diags, 1109),
            0,
            "no TS1109 for keyword '{kw}'; got: {diags:#?}"
        );
        assert_eq!(
            count(&diags, 1144),
            0,
            "no TS1144 for keyword '{kw}'; got: {diags:#?}"
        );
    }
}

/// Top-level `function f(implements) {}`: tsc emits TS1212 + TS7006, NOT
/// TS7051. The TS7051 upgrade is reserved for signature-only contexts.
#[test]
fn top_level_function_reserved_param_emits_ts7006_not_ts7051() {
    let source = r#"
function bar(implements) { return implements; }
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 7051),
        0,
        "must NOT emit TS7051 for top-level function; got: {diags:#?}"
    );
    assert!(
        count(&diags, 7006) >= 1,
        "must emit TS7006 implicit-any; got: {diags:#?}"
    );
}

/// Anti-hardcoding (§25): same expectation across multiple non-modifier
/// reserved keywords, in a top-level function (not a signature).
#[test]
fn top_level_function_various_reserved_params_emit_ts7006() {
    for kw in ["implements", "interface", "let", "package", "static"] {
        let source = format!("function bar({kw}) {{ return {kw}; }}\n");
        let diags = check_source_diagnostics(&source);
        assert_eq!(
            count(&diags, 7051),
            0,
            "no TS7051 for top-level fn with '{kw}'; got: {diags:#?}"
        );
        assert!(
            count(&diags, 7006) >= 1,
            "TS7006 must fire for '{kw}'; got: {diags:#?}"
        );
    }
}

/// Interface method signature with non-modifier reserved name: tsc emits
/// TS7051 (signature-only context where types are conventionally required).
#[test]
fn interface_method_signature_reserved_param_emits_ts7051() {
    let source = r#"
"use strict";
interface I {
    foo(package);
}
"#;
    let diags = check_source_diagnostics(source);
    assert!(
        count(&diags, 7051) >= 1,
        "TS7051 must fire for interface method signature with 'package'; got: {diags:#?}"
    );
}

/// Anti-hardcoding (§25): the rule is "non-modifier reserved word in a
/// signature-only slot", not specific to `package`. Re-run across the set.
#[test]
fn interface_method_various_non_modifier_reserved_params() {
    for kw in ["implements", "interface", "let", "package", "static"] {
        let source = format!("\"use strict\";\ninterface I {{\n    foo({kw});\n}}\n");
        let diags = check_source_diagnostics(&source);
        assert!(
            count(&diags, 7051) >= 1,
            "TS7051 must fire for interface method with '{kw}'; got: {diags:#?}"
        );
    }
}

/// Negative companion: modifier-style reserved words (`public`/`private`/
/// `protected`) in the same signature slot keep TS7006 — tsc treats them
/// as clearly intended parameter names. The fix must not collapse these
/// into TS7051.
#[test]
fn interface_method_modifier_keyword_param_still_ts7006() {
    let source = r#"
"use strict";
interface I {
    foo(protected);
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 7051),
        0,
        "must NOT upgrade modifier keyword to TS7051; got: {diags:#?}"
    );
    assert!(
        count(&diags, 7006) >= 1,
        "TS7006 must fire for 'protected' param; got: {diags:#?}"
    );
}
