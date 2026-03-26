//! Tests for JSDoc type expression parsing: T[] array suffix,
//! ?Type nullable prefix, and !Type non-nullable prefix.
//!
//! These forms are handled by `jsdoc_type_from_expression` and were
//! previously missing, causing JSDoc array annotations to resolve as `any[]`.

use tsz_checker::context::CheckerOptions;

struct Diag {
    code: u32,
}

fn check_js(source: &str) -> Vec<Diag> {
    let options = CheckerOptions {
        check_js: true,
        ..CheckerOptions::default()
    };

    let mut parser =
        tsz_parser::parser::ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let mut checker = tsz_checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.js".to_string(),
        options,
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| Diag { code: d.code })
        .collect()
}

// =============================================================================
// T[] array suffix
// =============================================================================

/// `@type {string[]}` should resolve to `string[]`, not `any[]`.
#[test]
fn jsdoc_type_array_suffix_string_array() {
    let diags = check_js(
        r#"/** @type {string[]} */
var x = [];
/** @type {number[]} */
var y;
y = x;
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "Expected TS2322 for string[] assigned to number[], got codes: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `@type {number[][]}` should resolve to nested arrays.
#[test]
fn jsdoc_type_array_suffix_nested() {
    let diags = check_js(
        r#"/** @type {number[][]} */
var x = [[1]];
/** @type {string[][]} */
var y;
y = x;
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "Expected TS2322 for number[][] assigned to string[][], got codes: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `@type {(string | number)[]}` should resolve to `(string | number)[]`.
#[test]
fn jsdoc_type_array_suffix_parenthesized_union() {
    let diags = check_js(
        r#"/** @type {(string | number)[]} */
var x = [1, "a"];
/** @type {boolean[]} */
var y;
y = x;
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "Expected TS2322 for (string|number)[] assigned to boolean[], got codes: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `@type {string[]}` with compatible assignment should produce no errors.
#[test]
fn jsdoc_type_array_suffix_compatible_no_error() {
    let diags = check_js(
        r#"/** @type {string[]} */
var x = ["hello"];
"#,
    );
    let relevant: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        relevant.is_empty(),
        "Expected no TS2322 for string[] assigned string[], got codes: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// =============================================================================
// JSDoc tuple types
// =============================================================================

#[test]
fn jsdoc_type_tuple_basic_assignment() {
    let diags = check_js(
        r#"/** @type {[string, number]} */
var tuple = ["hello", 1];
/** @type {[number, string]} */
var other;
other = tuple;
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "Expected TS2322 for [string, number] assigned to [number, string], got codes: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn jsdoc_type_tuple_optional_and_rest_assignment() {
    let diags = check_js(
        r#"/** @type {[f?: any, ...any[]]} */
var tuple = [1, 2];
tuple = 1;
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "Expected TS2322 for number assigned to optional/rest JSDoc tuple, got codes: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// =============================================================================
// ?Type nullable prefix and !Type non-nullable prefix
// =============================================================================

/// `?string` nullable prefix should resolve to `string | null`.
#[test]
fn jsdoc_type_nullable_prefix() {
    let diags = check_js(
        r#"/** @type {?string} */
var x = null;
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322 for null assigned to ?string (string | null), got codes: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `!Type` non-nullable prefix should strip the prefix and use the inner type.
#[test]
fn jsdoc_type_non_nullable_prefix() {
    let diags = check_js(
        r#"/** @type {!number} */
var x = 42;
/** @type {!string} */
var y;
y = x;
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "Expected TS2322 for !number assigned to !string, got codes: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `@type {(this: {...}) => void}` should preserve the contextual `this`
/// parameter so bad member reads still produce TS2339.
#[test]
fn jsdoc_arrow_function_type_preserves_this_parameter() {
    let diags = check_js(
        r#"/** @type {(this: { foo: number }) => void} */
const f = function() {
    this.test;
};
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2339),
        "Expected TS2339 when JSDoc arrow function type provides an object `this`, got codes: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Function declarations with arrow-style JSDoc callable types should also
/// preserve the contextual `this` parameter during body checking.
#[test]
fn jsdoc_arrow_function_declaration_preserves_this_parameter() {
    let diags = check_js(
        r#"/** @type {(this: { foo: number }) => void} */
function f() {
    this.test;
}
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2339),
        "Expected TS2339 when JSDoc arrow function declaration provides an object `this`, got codes: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `@type {function(this: Foo): void}` should preserve the contextual `this`
/// parameter for Closure-style callable syntax too.
#[test]
fn jsdoc_closure_function_type_preserves_this_parameter() {
    let diags = check_js(
        r#"/** @type {function(this: { foo: number }): void} */
const f = function() {
    this.test;
};
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2339),
        "Expected TS2339 when Closure-style JSDoc function type provides an object `this`, got codes: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Function declarations with Closure-style JSDoc callable types should also
/// preserve the contextual `this` parameter during body checking.
#[test]
fn jsdoc_closure_function_declaration_preserves_this_parameter() {
    let diags = check_js(
        r#"/** @type {function(this: { foo: number }): void} */
function f() {
    this.test;
}
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2339),
        "Expected TS2339 when Closure-style JSDoc function declaration provides an object `this`, got codes: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}
