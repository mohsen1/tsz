//! Tests for JSDoc `@extends` / `@augments` type-argument constraint
//! validation (TS2344).
//!
//! When a class is decorated with `@extends {A<T>}` in a JS file, the type
//! argument `T` must satisfy the constraint declared on A's corresponding
//! `@template` parameter. Before this check, tsz emitted no diagnostic;
//! tsc emits TS2344 with the argument type and the constraint name.

use tsz_checker::context::CheckerOptions;

fn check_js_with_jsdoc(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source(
        source,
        "a.js",
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
fn jsdoc_extends_missing_required_property_emits_ts2344() {
    // Arg type `{a: string}` is missing required property `b` → TS2344.
    // This exercises the missing-property branch of the constraint walk
    // without depending on union-member assignability, which is sensitive
    // to whether `string[]` has been fully desugared via lib.
    let source = r#"
/**
 * @typedef {{
*     a: number;
*     b: string;
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
 * @extends {A<{a: number}>}
 */
class E extends A {}
"#;
    let diags = check_js_with_jsdoc(source);
    let ts2344: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2344).collect();
    assert!(
        !ts2344.is_empty(),
        "Expected TS2344 for @extends missing required property, got: {diags:?}"
    );
}

#[test]
fn jsdoc_extends_satisfying_constraint_no_ts2344() {
    let source = r#"
/**
 * @typedef {{
*     a: number;
*     b: string;
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
 * @extends {A<{a: number, b: string}>}
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
fn jsdoc_string_not_assignable_to_boolean_or_string_array_minimization() {
    // Pre-fix `string` was reported as a subtype of `boolean | string[]` via a
    // String/iterable shortcut that misclassified arrays as "purely iterable".
    // This minimization keeps that bug visible at the JSDoc-assignability layer
    // without depending on @extends/@template plumbing.
    let source = r#"
/**
 * @typedef {{ b: boolean | string[]; }} Foo
 */
/** @type {Foo} */
const x = { b: "hello" };
"#;
    let diags = check_js_with_jsdoc(source);
    let codes: Vec<u32> = diags.iter().map(|(c, _)| *c).collect();
    assert!(
        codes.contains(&2322),
        "Expected TS2322 (string not assignable to boolean | string[]), got: {diags:?}"
    );
}

#[test]
fn jsdoc_extends_incompatible_property_type_emits_ts2344() {
    // Constraint `b: boolean | string[]`; arg supplies `b: string`. All required
    // props are present but `string` is not assignable to `boolean | string[]`,
    // so tsc emits TS2344. Mirrors `extendsTag5.ts` class E.
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
 * @extends {A<{a: string, b: string}>}
 */
class E extends A {}
"#;
    let diags = check_js_with_jsdoc(source);
    let ts2344: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2344).collect();
    assert!(
        !ts2344.is_empty(),
        "Expected TS2344 for @extends incompatible property type, got: {diags:?}"
    );
}

#[test]
fn jsdoc_extends_multi_line_incompatible_property_type_emits_ts2344() {
    // Same shape but `@extends` type spans multiple JSDoc lines. Mirrors
    // `extendsTag5.ts` class C.
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
        "Expected TS2344 for multi-line @extends incompatible property type, got: {diags:?}"
    );
}

#[test]
fn jsdoc_extends_constraint_display_uses_typedef_alias_name() {
    // tsc displays the typedef alias `Foo` in the TS2344 message instead of
    // the structural expansion `{ a: ...; b: ...; }`.
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
   /** @param {T} a */
   constructor(a) { return a }
}

/**
 * @extends {A<{a: string, b: string}>}
 */
class E extends A {}
"#;
    let diags = check_js_with_jsdoc(source);
    let ts2344: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2344).collect();
    assert!(!ts2344.is_empty(), "Expected TS2344, got: {diags:?}");
    let messages: Vec<&str> = ts2344.iter().map(|(_, m)| m.as_str()).collect();
    assert!(
        messages.iter().any(|m| m.contains("constraint 'Foo'")),
        "TS2344 should display the typedef alias 'Foo'; got: {messages:?}"
    );
}

#[test]
fn jsdoc_extends_multi_line_missing_property_emits_ts2344() {
    // Multi-line `@extends {A<{...}>}` with a missing required property
    // exercises both the balanced-brace extraction and the line-continuation
    // normalizer in the arg parser.
    let source = r#"
/**
 * @typedef {{
*     a: number;
*     b: string;
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
 *     a: number
 * }>}
 */
class C extends A {}
"#;
    let diags = check_js_with_jsdoc(source);
    let ts2344: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2344).collect();
    assert!(
        !ts2344.is_empty(),
        "Expected TS2344 for multi-line @extends missing required property, got: {diags:?}"
    );
}
