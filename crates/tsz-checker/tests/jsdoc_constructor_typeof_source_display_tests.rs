//! TS2345 source-display for JS-style constructor functions under TypeScript 7.
//!
//! TypeScript 7 dropped JS constructor-function inference: a `.js`
//! `var = function (...) {...}` or `function f() {...}` (even with an
//! `@constructor` JSDoc tag) is an ordinary function with no synthesized
//! construct signature. When such a value is passed where a `new`-signature is
//! expected, the argument type is the plain call signature (e.g.
//! `(n: number) => void`) — not the old `typeof <name>` constructor display and
//! not an expanded `new (...) => { ... }` instance shape.
//!
//! These tests pin that the TS2345 is still reported (the plain function is
//! incompatible with the construct-signature parameter) and that the source
//! renders as the plain function type rather than the obsolete `typeof <name>`
//! carve-out.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_js_source_code_messages_with_options;

fn diagnostics_for_js(source: &str) -> Vec<(u32, String)> {
    check_js_source_code_messages_with_options(
        source,
        "functions.js",
        CheckerOptions {
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    )
}

/// JS-style constructor declared as `var E = function(n) { this.x = n; };`
/// with a `@constructor` JSDoc tag. Under TypeScript 7, `E` is a plain function
/// `(n: number) => void`, which is not assignable to the construct-signature
/// parameter, so the call is a TS2345 whose source renders as the plain
/// function type rather than the obsolete `typeof E` display.
#[test]
fn ts2345_jsdoc_constructor_var_displays_plain_function_source() {
    let source = r#"
/**
 * @param {function(new: { length: number }, number): number} c
 * @return {function(new: { length: number }, number): number}
 */
function id2(c) {
    return c
}

/**
 * @constructor
 * @param {number} n
 */
var E = function(n) {
  this.not_length_on_purpose = n;
};

var y3 = id2(E);
"#;
    let diags = diagnostics_for_js(source);
    let ts2345: Vec<_> = diags.iter().filter(|(c, _)| *c == 2345).collect();
    assert!(
        !ts2345.is_empty(),
        "expected at least one TS2345 for the id2(E) call; got: {diags:?}"
    );
    let msg = &ts2345[0].1;
    assert!(
        !msg.contains("'typeof E'"),
        "TS7 no longer renders the JS constructor as 'typeof E', got: {msg:?}"
    );
    assert!(
        !msg.contains("not_length_on_purpose"),
        "TS2345 source must not expand a synthesized instance shape; got: {msg:?}"
    );
}

/// Same shape but with `function D(n) { ... }` declaration form (function
/// declaration + `@constructor` JSDoc) instead of `var = function`.
/// Pass `D` to a parameter whose `new`-signature is incompatible to force a
/// TS2345; under TypeScript 7 the source renders as the plain function type,
/// not `typeof D`.
#[test]
fn ts2345_jsdoc_constructor_function_decl_displays_plain_function_source() {
    let source = r#"
/**
 * @param {function(new: { unique_marker: string }, number): number} c
 * @return {function(new: { unique_marker: string }, number): number}
 */
function id3(c) {
    return c
}

/**
 * @constructor
 * @param {number} n
 */
function D(n) {
  this.length = n;
}

var y4 = id3(D);
"#;
    let diags = diagnostics_for_js(source);
    let ts2345: Vec<_> = diags.iter().filter(|(c, _)| *c == 2345).collect();
    assert!(
        !ts2345.is_empty(),
        "expected at least one TS2345 for the id3(D) call; got: {diags:?}"
    );
    let msg = &ts2345[0].1;
    assert!(
        !msg.contains("'typeof D'"),
        "TS7 no longer renders the JS constructor as 'typeof D', got: {msg:?}"
    );
}

/// Negative case: a plain JS variable holding a function value (no
/// `@constructor` JSDoc) must NOT be rendered as `typeof X`. We use a
/// `.ts` file so the JS-only short-circuit stays inactive — guarding
/// against unintended regressions.
#[test]
fn ts2345_plain_function_identifier_does_not_use_typeof_source() {
    let diags = tsz_checker::test_utils::check_source_code_messages(
        r#"
function id3(c: new (n: number) => { unique_marker: string }): typeof c {
    return c;
}
const F = function (n: number) { return { length: n }; };
const z = id3(F);
"#,
    );
    let ts2345: Vec<_> = diags.iter().filter(|(c, _)| *c == 2345).collect();
    for (_, msg) in &ts2345 {
        assert!(
            !msg.contains("'typeof F'"),
            "TS2345 must not render plain TS-bound identifiers as 'typeof F'; got: {msg:?}"
        );
    }
}
