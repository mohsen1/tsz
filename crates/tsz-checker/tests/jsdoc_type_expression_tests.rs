//! Tests for JSDoc type expression parsing: T[] array suffix,
//! ?Type nullable prefix, and !Type non-nullable prefix.
//!
//! These forms are handled by `jsdoc_type_from_expression` and were
//! previously missing, causing JSDoc array annotations to resolve as `any[]`.

use tsz_checker::context::CheckerOptions;

struct Diag {
    code: u32,
    message: String,
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
        .map(|d| Diag {
            code: d.code,
            message: d.message_text.clone(),
        })
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

/// `@type {<T>(param?: T) => T | undefined}` declares its own type
/// parameter `T` for the duration of the signature. The unresolved-leaf
/// walker must thread `<T>` into the template-param scope before
/// recursing into params and return — otherwise it spuriously emits
/// TS2304 ("Cannot find name 'T'") for every reference to `T` inside
/// the body of the signature.
#[test]
fn jsdoc_type_generic_signature_introduces_local_type_params() {
    let diags = check_js(
        r#"/** @type {<T>(param?: T) => T | undefined} */
function typed(param) {
    return param;
}
var n = typed(1);
"#,
    );
    let unresolved_t: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2304 && d.message.contains("'T'"))
        .collect();
    assert!(
        unresolved_t.is_empty(),
        "Expected no TS2304 for generic-signature-local `T`, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message))
            .collect::<Vec<_>>()
    );
}

/// Variant: nested generic parameters and an `extends` constraint must
/// also count toward the local scope. Only the parameter *name* gets
/// added (we deliberately don't parse the constraint expression — that
/// path stays intentionally unhandled in the diagnostic walker).
#[test]
fn jsdoc_type_generic_signature_with_constraint_no_ts2304() {
    let diags = check_js(
        r#"/** @type {<K extends string>(key: K) => K} */
function get(key) {
    return key;
}
"#,
    );
    let unresolved_k: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2304 && d.message.contains("'K'"))
        .collect();
    assert!(
        unresolved_k.is_empty(),
        "Expected no TS2304 for `<K extends string>(key: K) => K`, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message))
            .collect::<Vec<_>>()
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

#[test]
fn jsdoc_readonly_property_accepts_tab_after_modifier() {
    let diags = check_js(
        r#"
// @ts-check

/** @type {{readonly	value: string}} */
const item = { value: 123 };
"#,
    );
    let codes = diags.iter().map(|d| d.code).collect::<Vec<_>>();

    assert!(
        !codes.contains(&2353),
        "Expected readonly modifier with tab whitespace to parse without TS2353, got diagnostics: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message))
            .collect::<Vec<_>>()
    );
    assert!(
        codes.contains(&2322),
        "Expected parsed readonly property to report value mismatch TS2322, got diagnostics: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message))
            .collect::<Vec<_>>()
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

#[test]
fn jsdoc_recursive_generic_typedef_with_readonly_tuple_reports_assignment() {
    let diags = check_js(
        r#"/** @template T
 * @typedef {{ readonly [n: number]: T }} ReadonlyArray<T>
 */

/** @template K,V
 * @typedef {{ [key: string]: V }} Record<K,V>
 */

/** @typedef {ReadonlyArray<Json>} JsonArray */
/** @typedef {{ readonly [key: string]: Json }} JsonRecord */
/** @typedef {boolean | number | string | null | JsonRecord | JsonArray | readonly []} Json */

/**
 * @template T
 * @typedef {{
  $A: {
    [K in keyof T]?: XMLObject<T[K]>[]
  },
  $O: {
    [K in keyof T]?: {
      $$?: Record<string, string>
    } & (T[K] extends string ? {$:string} : XMLObject<T[K]>)
  },
  $$?: Record<string, string>,
  } & {
  [K in keyof T]?: (
    T[K] extends string ? string
      : XMLObject<T[K]>
  )
}} XMLObject<T> */

/** @type {XMLObject<{foo:string}>} */
const p = {};
"#,
    );
    let codes = diags.iter().map(|d| d.code).collect::<Vec<_>>();
    assert!(
        codes.contains(&2322),
        "Expected TS2322 for empty object assigned to recursive JSDoc generic typedef, got diagnostics: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message))
            .collect::<Vec<_>>()
    );
    assert!(
        !codes.contains(&2552),
        "Did not expect TS2552 from resolving nested JSDoc generic typedefs, got diagnostics: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn jsdoc_mapped_type_accepts_tab_before_in_keyword() {
    let diags = check_js(
        r#"
// @ts-check

/**
 * @typedef {{ value: string }} Source
 */

/** @type {{[K	in keyof Source]: Source[K]}} */
const item = { value: 123 };
"#,
    );
    let codes = diags.iter().map(|d| d.code).collect::<Vec<_>>();

    assert!(
        !codes.contains(&2353),
        "Expected tab-separated mapped type header to parse without TS2353, got diagnostics: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message))
            .collect::<Vec<_>>()
    );
    assert!(
        codes.contains(&2322),
        "Expected parsed mapped type to report value mismatch TS2322, got diagnostics: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message))
            .collect::<Vec<_>>()
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

/// `@type {(...values: string[]) => void}` should be parsed as a rest parameter,
/// not as a single `string[]` positional parameter.
#[test]
fn jsdoc_arrow_function_type_preserves_rest_parameter() {
    let diags = check_js(
        r#"/** @type {(...values: string[]) => void} */
const f = function() {};
f("a", "b", "c");
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2345 || d.code == 2554)
        .map(|d| d.code)
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected no TS2345/TS2554 when JSDoc arrow function type uses a rest parameter, got codes: {:?}",
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

#[test]
fn jsdoc_nongeneric_instantiation_reports_ts2315_and_ts2304() {
    let diags = check_js(
        r#"
/**
 * @param {Void<Missing>} c
 * @param {<T>(m: Boolean<T>) => string} fn
 * @param {fn<T>} callback
 */
function sample(c, fn, callback) {
  return fn(c) || callback;
}

function fn() {}
"#,
    );

    let ts2315 = diags.iter().filter(|d| d.code == 2315).count();
    let ts2304_messages = diags
        .iter()
        .filter(|d| d.code == 2304)
        .map(|d| d.message.as_str())
        .collect::<Vec<_>>();

    assert!(
        ts2315 >= 2,
        "Expected at least two TS2315 diagnostics for non-generic JSDoc instantiation attempts, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message))
            .collect::<Vec<_>>()
    );
    assert!(
        ts2304_messages.contains(&"Cannot find name 'T'."),
        "Expected at least one TS2304 diagnostic for unresolved JSDoc type arguments, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message))
            .collect::<Vec<_>>()
    );
    assert!(
        !ts2304_messages.contains(&"Cannot find name 'fn<T>'."),
        "Known function values should suppress the full-name TS2304 while preserving argument diagnostics, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message))
            .collect::<Vec<_>>()
    );
}
