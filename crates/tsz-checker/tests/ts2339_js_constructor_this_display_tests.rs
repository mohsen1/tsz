//! TS2339 message-rendering for `this.<unknown>` access inside a JS
//! constructor function.
//!
//! In a JS file with `// @checkJs: true`, a function whose body contains
//! `this.<prop> = …` assignments is treated by salsa inference as a
//! constructor function. tsc displays the synthesized `this` type using
//! the function's NAME (e.g. `'toString'`) when reporting TS2339 against
//! a missing property — not the structural shape (`'{ someValue: …; }'`)
//! that tsz previously printed.
//!
//! Regression: `inexistentPropertyInsideToStringType.ts`
//! (TS issue #36031). The check is fingerprint-only at the conformance
//! level (codes already match), so this test asserts on the rendered
//! diagnostic message.

use tsz_checker::test_utils::check_js_source_diagnostics;

/// Inside `function toString() { this.yadda; this.someValue = "" }`,
/// the access `this.yadda` should report TS2339 with the receiver
/// printed as `'toString'`, not as the synthesized object shape.
#[test]
fn this_in_js_constructor_function_renders_function_name_as_receiver() {
    let source = r#"
function toString() {
    this.yadda;
    this.someValue = "";
}
"#;

    let diagnostics = check_js_source_diagnostics(source);
    let ts2339: Vec<_> = diagnostics.iter().filter(|d| d.code == 2339).collect();
    assert_eq!(
        ts2339.len(),
        1,
        "expected exactly one TS2339 for `this.yadda`; got: {diagnostics:#?}"
    );
    let msg = &ts2339[0].message_text;
    assert!(
        msg.contains("'toString'"),
        "TS2339 message should print receiver as `'toString'` (the JS \
         constructor function's name), got: {msg:?}"
    );
    assert!(
        !msg.contains("{ someValue:"),
        "TS2339 message must NOT print the synthesized structural shape \
         when the receiver is `this` inside a salsa-inferred JS \
         constructor function, got: {msg:?}"
    );
}

/// Counter-test: when the enclosing function has NO `this.<prop> = …`
/// assignments, salsa doesn't classify it as a constructor, so the
/// receiver display must NOT collapse to the function name. We don't
/// assert on a specific shape here — just that the function-name
/// shortcut does not fire when the salsa precondition is absent.
#[test]
fn this_in_non_constructor_js_function_does_not_use_function_name() {
    let source = r#"
function toString() {
    this.yadda;
}
"#;

    let diagnostics = check_js_source_diagnostics(source);
    let ts2339: Vec<_> = diagnostics.iter().filter(|d| d.code == 2339).collect();
    // `this.yadda` will still TS2339, but the receiver must not be
    // rendered as `'toString'` since there is no `this.<prop> = …`
    // assignment in the body.
    for d in &ts2339 {
        assert!(
            !d.message_text.contains("'toString'"),
            "TS2339 receiver should not be `'toString'` for a JS \
             function with no `this.<prop> = …` assignments, got: {:?}",
            d.message_text
        );
    }
}
