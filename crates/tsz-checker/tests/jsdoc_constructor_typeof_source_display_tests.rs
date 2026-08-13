//! Closure-style `function(...)` JSDoc types under TypeScript 7.
//!
//! TypeScript 7's JSDoc parser does not accept the Closure `function(...)`
//! type spelling. It reads the head keyword `function` as a reference to the
//! global `Function` type, then reports `TS1005 '}' expected.` for the trailing
//! call-/construct-signature syntax it cannot consume, and discards it. The
//! annotated symbol is therefore typed `Function` — a plain value type — not a
//! synthesized construct/call signature and never the obsolete `typeof <name>`
//! constructor display TypeScript 6 produced for a `@constructor` JS function.
//!
//! Oracle (`typescript@7.0.2`, `--allowJs --checkJs --noImplicitAny`):
//!
//! ```text
//! /** @param {function(): number} c */ function f(c){return c;} f(0);
//!   → TS1005 '}' expected.
//!   → TS2345 Argument of type 'number' is not assignable to parameter of type 'Function'.
//! ```
//!
//! Because the resolved parameter type is `Function`, passing an ordinary
//! function value (a `@constructor`-tagged JS function is one under TS7's
//! dropped constructor-function inference) is *assignable* — so no TS2345 and,
//! crucially, no `typeof <name>` source display. These tests pin that parity:
//! the TS1005 rejection, the `Function` resolution across `@param`/`@type`/
//! `@return`, and the absence of any `typeof <name>` carve-out.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_compiled_lib_files};

/// Check a `.js` source with the real `lib.es5.d.ts` global scope loaded, so
/// the global `Function` type (the resolved form of a Closure `function(...)`
/// JSDoc type) is available. Returns `(code, message)` pairs.
fn diagnostics_for_js_with_lib(source: &str) -> Vec<(u32, String)> {
    let libs = load_compiled_lib_files(&["lib.es5.d.ts"]);
    assert!(
        !libs.is_empty(),
        "expected lib.es5.d.ts fixture to be present"
    );
    check_source_with_libs_code_messages(
        source,
        "functions.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
}

/// Sorted multiset of diagnostic codes, for order-independent parity assertions.
fn codes(diags: &[(u32, String)]) -> Vec<u32> {
    let mut v: Vec<u32> = diags.iter().map(|(c, _)| *c).collect();
    v.sort_unstable();
    v
}

/// Assert the TS7 parity for a Closure `function(...)` type used in `source`:
/// the syntax is rejected with TS1005 and the annotated symbol resolves to the
/// global `Function`, so `expected_code` (TS2345 at a call, TS2322 at an
/// assignment/return) fires with a message naming `type 'Function'`.
fn assert_closure_resolves_to_function(source: &str, expected_code: u32) {
    let diags = diagnostics_for_js_with_lib(source);
    assert!(
        diags.iter().any(|(c, _)| *c == 1005),
        "the Closure `function(...)` type must be rejected with TS1005; got: {diags:?}"
    );
    assert!(
        diags
            .iter()
            .any(|(c, m)| *c == expected_code && m.contains("type 'Function'")),
        "the Closure type must resolve to `Function` (expected TS{expected_code}); got: {diags:?}"
    );
}

/// Assert the TS7 parity for a `@constructor`-tagged JS function `ctor_name`
/// passed to a Closure `function(new: ...)` parameter: the two Closure types
/// each report TS1005, the constructor body reports TS2683 for its untyped
/// `this`, and — because a function value is assignable to `Function` — there
/// is no TS2345 and never a `typeof <name>` source display.
fn assert_closure_ctor_parity(source: &str, ctor_name: &str) {
    let diags = diagnostics_for_js_with_lib(source);
    assert_eq!(
        codes(&diags),
        vec![1005, 1005, 2683],
        "TS7 Closure-function-type parity for {ctor_name}; got: {diags:?}"
    );
    let typeof_display = format!("typeof {ctor_name}");
    assert!(
        diags.iter().all(|(_, m)| !m.contains(&typeof_display)),
        "TS7 never renders the JS constructor as `{typeof_display}`; got: {diags:?}"
    );
    assert!(
        diags.iter().all(|(c, _)| *c != 2345),
        "a function value is assignable to a `Function` parameter — no TS2345; got: {diags:?}"
    );
}

/// JS-style constructor declared as `var E = function(n) { ... };` with a
/// `@constructor` JSDoc tag, passed to a parameter whose type is written in the
/// Closure `function(new: ...)` spelling. Under TypeScript 7 that parameter type
/// is `Function` (the Closure syntax is rejected with TS1005 and its head
/// resolves to the global `Function`), and `E` — an ordinary function value —
/// is assignable to `Function`, so there is **no** TS2345 and never a
/// `typeof E` source display.
///
/// Oracle (`typescript@7.0.2`, `--allowJs --checkJs --noImplicitAny`): the two
/// Closure types (`@param` and `@return`) each report TS1005, and `E`'s body
/// reports TS2683 for the untyped `this`. Nothing else.
#[test]
fn jsdoc_constructor_var_closure_param_is_function_no_typeof() {
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
    assert_closure_ctor_parity(source, "E");
}

/// Same shape with the `function D(n) { ... }` declaration form. Identical TS7
/// parity: the Closure parameter/return types are `Function`, `D` is assignable
/// to `Function`, so no TS2345 and no `typeof D` display.
#[test]
fn jsdoc_constructor_function_decl_closure_param_is_function_no_typeof() {
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
    assert_closure_ctor_parity(source, "D");
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

// --- Closure `function(...)` JSDoc type → global `Function` (broad matrix) ---
//
// The Closure spelling is rejected with TS1005 and resolves to the global
// `Function` type, so it participates in downstream assignability. Each case
// below is measured directly against the pinned `typescript@7.0.2` oracle.

/// `@param` position: a Closure `function(...)` parameter type is `Function`,
/// so passing a non-function argument is a TS2345 against `Function`. Covers the
/// bare, positional-arg, and `new:` (construct) spellings.
#[test]
fn jsdoc_closure_function_param_type_resolves_to_function() {
    for closure in [
        "function(): number",
        "function(number): number",
        "function(new: {a: number}): void",
    ] {
        let source =
            format!("/**\n * @param {{{closure}}} c\n */\nfunction f(c) {{ return c; }}\nf(0);\n");
        assert_closure_resolves_to_function(&source, 2345);
    }
}

/// `@type` position: a Closure `function(...)` variable type is `Function`, so
/// assigning a non-function value is a TS2322 against `Function`.
#[test]
fn jsdoc_closure_function_type_tag_resolves_to_function() {
    assert_closure_resolves_to_function(
        "/** @type {function(): number} */\nvar g;\ng = 5;\n",
        2322,
    );
}

/// `@return` position: a Closure `function(...)` return type is `Function`, so
/// returning a non-function value is a TS2322 against `Function`.
#[test]
fn jsdoc_closure_function_return_type_resolves_to_function() {
    assert_closure_resolves_to_function(
        "/**\n * @return {function(number): number}\n */\nfunction h() { return 5; }\n",
        2322,
    );
}

/// Negative control: the TypeScript arrow form `(...) => T` is a real function
/// type, parsed structurally. It is unaffected by the Closure recovery — no
/// TS1005, and the TS2345 renders the arrow type, not `Function`.
#[test]
fn jsdoc_arrow_function_type_is_unaffected_by_closure_recovery() {
    let source =
        "/**\n * @param {(x: number) => number} c\n */\nfunction f(c) { return c; }\nf(0);\n";
    let diags = diagnostics_for_js_with_lib(source);
    assert!(
        diags.iter().all(|(c, _)| *c != 1005),
        "the arrow form must not trigger the Closure TS1005; got: {diags:?}"
    );
    assert!(
        diags.iter().any(|(c, m)| *c == 2345
            && m.contains("(x: number) => number")
            && !m.contains("'Function'")),
        "the arrow form must render its own type, not `Function`; got: {diags:?}"
    );
}
