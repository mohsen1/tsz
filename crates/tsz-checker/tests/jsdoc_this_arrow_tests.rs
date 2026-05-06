//! Regression tests for JSDoc `@this` on arrow functions.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn check_js(source: &str) -> Vec<(u32, String)> {
    check_source(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

#[test]
fn jsdoc_this_on_class_field_arrow_diagnoses_and_preserves_lexical_this() {
    let source = r#"
// @ts-check

/**
 * @typedef {{ fn(a: string): void }} T
 */

class C {
  /**
   * @this {T}
   */
  p = () => this.fn("x");
}
"#;

    let diagnostics = check_js(source);
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2730),
        "expected TS2730 for @this on arrow; got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2339 && message.contains("Property 'fn'") && message.contains("'C'")
        }),
        "expected TS2339 against lexical class this; got {diagnostics:?}"
    );
}

#[test]
fn class_field_arrow_without_jsdoc_uses_lexical_this() {
    let source = r#"
// @ts-check

class C {
  p = () => this.fn("x");
}
"#;

    let diagnostics = check_js(source);
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2339 && message.contains("Property 'fn'") && message.contains("'C'")
        }),
        "expected TS2339 against lexical class this; got {diagnostics:?}"
    );
}
