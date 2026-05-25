//! Tests for JSDoc `@template {Constraint} T` constraint enforcement.
//!
//! A constrained `@template` parameter must behave like a TypeScript
//! `<T extends Constraint>`: the constraint participates in inference and
//! argument assignability (TS2345), and its members are visible on `T`
//! inside the function/method body. Previously the constraint clause parsed
//! without error but was dropped when lowering the JSDoc `@template` to a
//! solver type parameter, so violating arguments were silently accepted and
//! the body treated `T` as fully unconstrained.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_js_source_code_messages_with_options;

fn check_js(source: &str) -> Vec<(u32, String)> {
    check_js_source_code_messages_with_options(source, "a.js", CheckerOptions::default())
}

fn codes(diags: &[(u32, String)]) -> Vec<u32> {
    diags.iter().map(|(c, _)| *c).collect()
}

#[test]
fn string_constraint_rejects_number_argument() {
    let source = r#"
/**
 * @template {string} T
 * @param {T} x
 * @returns {T}
 */
function id(x) { return x; }
id(123);
"#;
    let diags = check_js(source);
    assert!(
        codes(&diags).contains(&2345),
        "expected TS2345 for number arg violating {{string}} constraint, got: {diags:?}"
    );
}

#[test]
fn constraint_is_independent_of_type_parameter_name() {
    // Same rule, the bound variable spelled `K` instead of `T`. If the fix
    // were hardcoded to a particular name this would silently pass.
    let source = r#"
/**
 * @template {string} K
 * @param {K} x
 * @returns {K}
 */
function id(x) { return x; }
id(123);
"#;
    let diags = check_js(source);
    assert!(
        codes(&diags).contains(&2345),
        "expected TS2345 for renamed param `K`, got: {diags:?}"
    );
}

#[test]
fn number_constraint_rejects_string_argument() {
    // Proves the rule is structural, not pinned to the spelling `string`.
    let source = r#"
/**
 * @template {number} T
 * @param {T} x
 * @returns {T}
 */
function id(x) { return x; }
id("nope");
"#;
    let diags = check_js(source);
    assert!(
        codes(&diags).contains(&2345),
        "expected TS2345 for string arg violating {{number}} constraint, got: {diags:?}"
    );
}

#[test]
fn object_constraint_members_visible_in_body() {
    // `T extends {length: number}` makes `x.length` a `number`, so assigning
    // it to a `string` must report TS2322 (not TS2339 "no length on T").
    let source = r#"
/**
 * @template {{ length: number }} T
 * @param {T} x
 */
function h(x) {
  /** @type {string} */
  const s = x.length;
  return s;
}
"#;
    let diags = check_js(source);
    let cs = codes(&diags);
    assert!(
        cs.contains(&2322),
        "expected TS2322 from constrained member access, got: {diags:?}"
    );
    assert!(
        !cs.contains(&2339),
        "constraint members must be visible on T (no TS2339), got: {diags:?}"
    );
}

#[test]
fn unconstrained_template_still_accepts_any_argument() {
    // Negative control: a bare `@template T` remains unconstrained.
    let source = r#"
/**
 * @template T
 * @param {T} x
 * @returns {T}
 */
function id(x) { return x; }
id(123);
"#;
    let diags = check_js(source);
    assert!(
        !codes(&diags).contains(&2345),
        "unconstrained @template must accept any argument, got: {diags:?}"
    );
}

#[test]
fn bracket_default_form_preserves_constraint() {
    // The combined brace-constraint + bracket-default spelling
    // `@template {C} [T=D]` must still attach the constraint `C`.
    let source = r#"
/**
 * @template {string} [T=string]
 * @param {T} x
 * @returns {T}
 */
function id(x) { return x; }
id(123);
"#;
    let diags = check_js(source);
    assert!(
        codes(&diags).contains(&2345),
        "expected TS2345 for {{string}} constraint in bracket-default form, got: {diags:?}"
    );
}

#[test]
fn class_template_constraint_enforced_on_constructor_argument() {
    let source = r#"
/**
 * @template {string} T
 */
class Box {
  /** @param {T} v */
  constructor(v) { this.v = v; }
}
new Box(123);
"#;
    let diags = check_js(source);
    assert!(
        codes(&diags).contains(&2345),
        "expected TS2345 for class @template constraint violation, got: {diags:?}"
    );
}
