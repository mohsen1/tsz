//! Scope of JSDoc `@template` names for `@param`/`@returns` type references.
//!
//! `tsc` scopes a `@template` tag to the declaration its comment is attached
//! to, plus — for a class — that class's members. A `@template` written on some
//! other declaration in the same file is not in scope, and a reference to it is
//! a genuine TS2304 `Cannot find name`.
//!
//! tsz used to suppress TS2304 whenever the name appeared in *any* `@template`
//! tag anywhere in the file, which silently accepted references that `tsc`
//! rejects. The corpus witness is `jsdocTemplateTag4`, where `@template K`/`V`
//! on a constructor function do not reach a separate JSDoc comment written on
//! `Multimap.prototype.get`.

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn js_codes(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.js", options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn cannot_find_name_count(source: &str) -> usize {
    js_codes(source).into_iter().filter(|c| *c == 2304).count()
}

// --- Positive cases: the template is out of scope, so TS2304 is correct. ---

#[test]
fn template_on_unrelated_function_is_not_in_scope() {
    let source = r"
/**
 * @template Q
 * @param {Q} x
 */
function unrelated(x) { return x; }

/**
 * @param {Q} y
 */
function standalone(y) { return y; }
";
    assert_eq!(cannot_find_name_count(source), 1);
}

/// Same shape, different binder names: the rule is structural, not keyed on
/// any particular type-parameter spelling.
#[test]
fn template_on_unrelated_function_is_not_in_scope_renamed() {
    let source = r"
/**
 * @template Element
 * @param {Element} x
 */
function unrelated(x) { return x; }

/**
 * @param {Element} y
 */
function standalone(y) { return y; }
";
    assert_eq!(cannot_find_name_count(source), 1);
}

/// The `jsdocTemplateTag4` witness: a constructor function's `@template` does
/// not reach a separate JSDoc comment on one of its prototype methods.
#[test]
fn constructor_template_is_not_in_scope_for_prototype_method() {
    let source = r"
/**
 * @template K
 */
function Multimap() { }

/**
 * @param {K} key
 */
Multimap.prototype.get = function (key) { return key; };
";
    assert_eq!(cannot_find_name_count(source), 1);
}

/// The prototype owner having no `@template` at all makes the reference
/// unambiguously file-wide, which is what the old suppression allowed.
#[test]
fn unrelated_template_does_not_reach_prototype_method() {
    let source = r"
/** @template K */
function HasTemplate() { }

function Owner() { }

/**
 * @param {K} a
 */
Owner.prototype.get = function (a) { return a; };
";
    assert_eq!(cannot_find_name_count(source), 1);
}

// --- Negative cases: the template IS in scope, so TS2304 must not fire. ---

#[test]
fn template_on_the_same_comment_is_in_scope() {
    let source = r"
/**
 * @template T
 * @param {T} x
 * @returns {T}
 */
function id(x) { return x; }
";
    assert_eq!(cannot_find_name_count(source), 0);
}

#[test]
fn enclosing_class_template_is_in_scope_for_members() {
    let source = r"
/**
 * @template T
 */
class Box {
  /**
   * @param {T} v
   */
  set(v) { this.v = v; }
}
new Box();
";
    assert_eq!(cannot_find_name_count(source), 0);
}

/// `export class` wraps the class in an export declaration, so the JSDoc sits
/// before the `export` keyword. The class-host lookup has to see through that
/// wrapper or every member reference becomes a false TS2304.
#[test]
fn exported_class_template_is_in_scope_for_members() {
    let source = r"
/**
 * @template T
 */
export class Foo {
  /**
   * @param {T} value
   */
  bar(value) { }
}
";
    assert_eq!(cannot_find_name_count(source), 0);
}

/// A method nested inside a class expression assigned to a variable: the host
/// is found by range containment, not by being a top-level statement.
#[test]
fn class_expression_template_is_in_scope_for_members() {
    let source = r"
/**
 * @template T
 */
const Holder = class {
  /**
   * @param {T} v
   */
  put(v) { this.v = v; }
};
Holder;
";
    assert_eq!(cannot_find_name_count(source), 0);
}

/// Multiple templates on the same comment all stay in scope together.
#[test]
fn multiple_templates_on_same_comment_are_in_scope() {
    let source = r"
/**
 * @template K
 * @template V
 * @param {K} k
 * @param {V} v
 */
function pair(k, v) { return [k, v]; }
";
    assert_eq!(cannot_find_name_count(source), 0);
}

/// A genuinely undefined name is still reported when a template is in scope —
/// the in-scope template must not suppress unrelated missing names.
#[test]
fn in_scope_template_does_not_suppress_other_missing_names() {
    let source = r"
/**
 * @template T
 * @param {T} x
 * @param {NoSuchType} y
 */
function mix(x, y) { return x; }
";
    assert_eq!(cannot_find_name_count(source), 1);
}
