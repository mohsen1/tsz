//! TS8030 wording for a JS function declaration whose `@type` tag is not callable.
//!
//! The message text moved with the compiler version this corpus is pinned to:
//! it used to read "The type of a function declaration must match the
//! function's signature." and now reads "A JSDoc '@type' tag on a function must
//! have a signature with the correct number of arguments." tsz had the current
//! wording in its diagnostics table but the emission site carried a hardcoded
//! copy of the old string, so every TS8030 rendered stale.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source, check_source_diagnostics};

const EXPECTED: &str =
    "A JSDoc '@type' tag on a function must have a signature with the correct number of arguments.";

fn js_diagnostics(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.js", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn ts8030_messages(source: &str) -> Vec<String> {
    js_diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code == 8030)
        .map(|(_, message)| message)
        .collect()
}

#[test]
fn non_callable_type_tag_uses_current_wording() {
    let messages = ts8030_messages("/** @type {number} */\nfunction f() { return 1; }\n");
    assert_eq!(messages, vec![EXPECTED.to_string()]);
}

#[test]
fn wording_is_binder_name_independent() {
    // Same structural situation under renamed binders and different
    // non-callable annotations.
    for (name, annotation) in [
        ("f", "number"),
        ("compute", "string"),
        ("_handler0", "boolean"),
    ] {
        let source =
            format!("/** @type {{{annotation}}} */\nfunction {name}() {{ return undefined; }}\n");
        assert_eq!(
            ts8030_messages(&source),
            vec![EXPECTED.to_string()],
            "name={name} annotation={annotation}"
        );
    }
}

#[test]
fn callable_type_tag_reports_nothing() {
    // Positive control: a callable annotation must stay silent, so the wording
    // change cannot be masking a condition change.
    for annotation in ["(a: number) => number", "() => void"] {
        let source = format!("/** @type {{{annotation}}} */\nfunction f(a) {{ return a; }}\n");
        assert!(
            ts8030_messages(&source).is_empty(),
            "annotation={annotation} must not report TS8030"
        );
    }
}

#[test]
fn ts8030_is_not_reported_in_typescript_files() {
    let diags =
        check_source_diagnostics("/** @type {number} */\nfunction f(): number { return 1; }\n");
    assert!(
        diags.iter().all(|d| d.code != 8030),
        "TS8030 is JS-only; got: {diags:?}"
    );
}
