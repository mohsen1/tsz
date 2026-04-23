//! Tests for JSDoc `@extends` / `@augments` type-argument constraint
//! validation (TS2344).
//!
//! When a class is decorated with `@extends {A<T>}` in a JS file, the type
//! argument `T` must satisfy the constraint declared on A's corresponding
//! `@template` parameter. Before this check, tsz emitted no diagnostic;
//! tsc emits TS2344 with the argument type and the constraint name.

use tsz_checker::context::CheckerOptions;

fn check_js_with_jsdoc(source: &str) -> Vec<(u32, String)> {
    let mut parser = tsz_parser::parser::ParserState::new("a.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let types = tsz_solver::TypeInterner::new();
    let options = CheckerOptions {
        check_js: true,
        ..CheckerOptions::default()
    };
    let mut checker = tsz_checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "a.js".to_string(),
        options,
    );
    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

#[test]
fn jsdoc_extends_single_line_violates_constraint_emits_ts2344() {
    let source = r#"
/**
 * @typedef {{
*     a: number | string;
*     b: boolean | string[];
* }} Foo
*/

/**
* @template {Foo} T
*/
class A {
   /**
    * @param {T} a
    */
   constructor(a) {
       return a
   }
}

/**
 * @extends {A<{a: string, b: string}>}
 */
class E extends A {}
"#;
    let diags = check_js_with_jsdoc(source);
    let ts2344: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2344).collect();
    assert!(
        !ts2344.is_empty(),
        "Expected TS2344 for @extends violating Foo constraint, got: {diags:?}"
    );
    assert!(
        ts2344.iter().any(|(_, m)| m.contains("Foo")),
        "Expected TS2344 message to mention the constraint name 'Foo', got: {ts2344:?}"
    );
}

#[test]
fn jsdoc_extends_satisfying_constraint_no_ts2344() {
    let source = r#"
/**
 * @typedef {{
*     a: number | string;
*     b: boolean | string[];
* }} Foo
*/

/**
* @template {Foo} T
*/
class A {
   /**
    * @param {T} a
    */
   constructor(a) { return a }
}

/**
 * @extends {A<{a: string, b: string[]}>}
 */
class D extends A {}
"#;
    let diags = check_js_with_jsdoc(source);
    let ts2344: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2344).collect();
    assert!(
        ts2344.is_empty(),
        "Should not emit TS2344 when @extends arg satisfies constraint, got: {ts2344:?}"
    );
}

#[test]
fn jsdoc_extends_multi_line_violates_constraint_emits_ts2344() {
    let source = r#"
/**
 * @typedef {{
*     a: number | string;
*     b: boolean | string[];
* }} Foo
*/

/**
* @template {Foo} T
*/
class A {
   /**
    * @param {T} a
    */
   constructor(a) { return a }
}

/**
 * @extends {A<{
 *     a: string,
 *     b: string
 * }>}
 */
class C extends A {}
"#;
    let diags = check_js_with_jsdoc(source);
    let ts2344: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2344).collect();
    assert!(
        !ts2344.is_empty(),
        "Expected TS2344 for multi-line @extends violating constraint, got: {diags:?}"
    );
}
