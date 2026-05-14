//! TS2345 source-display for JS-style constructor functions.
//!
//! Regression for `conformance/jsdoc/jsdocFunctionType.ts`: when a call
//! argument is an identifier whose symbol is a JS-style constructor function
//! (a `.js` `var = function (...) {...}` or `function f() {...}` with an
//! `@constructor` JSDoc tag), tsc renders the source type as
//! `typeof <name>` rather than expanding the constructor signature
//! (e.g. `new (n: number) => { not_length_on_purpose: number; }`).
//!
//! Without this carve-out, the TS2345 message degenerates into a verbose
//! "Argument of type 'new (n: number) => { ... }' is not assignable to..."
//! that diverges from tsc's output.
//!
//! Architecture: this is purely a checker-side display rule; the underlying
//! relation/reason still comes from `query_boundaries/assignability`.
//! The display formatter only consults `symbol_has_js_constructor_evidence`
//! to decide whether to short-circuit to `typeof <name>`.

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
/// with a `@constructor` JSDoc tag. Passing `E` to a parameter of type
/// `function(new: { length: number }, number): number` is a TS2345 because
/// `E`'s instance shape lacks the required `length` property. tsc displays
/// the source as `typeof E`, not the expanded constructor signature.
#[test]
fn ts2345_jsdoc_constructor_var_displays_typeof_source() {
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
        msg.contains("'typeof E'"),
        "TS2345 source display must be 'typeof E', got: {msg:?}"
    );
    assert!(
        !msg.contains("not_length_on_purpose"),
        "TS2345 source display must not expand the JS-constructor signature; got: {msg:?}"
    );
}

/// Same shape but with `function D(n) { ... }` declaration form (function
/// declaration + `@constructor` JSDoc) instead of `var = function`.
/// Pass `D` to a parameter whose `new`-signature has an incompatible shape
/// to force a TS2345; the source must render as `typeof D`.
#[test]
fn ts2345_jsdoc_constructor_function_decl_displays_typeof_source() {
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
        msg.contains("'typeof D'"),
        "TS2345 source display must be 'typeof D', got: {msg:?}"
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
