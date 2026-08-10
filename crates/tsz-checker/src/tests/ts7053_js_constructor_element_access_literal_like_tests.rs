//! A `this[key] = value` assignment in a checked-JS constructor only
//! late-binds an implicit class property when `key` is written as a
//! syntactically literal node (string, numeric, or no-substitution-template
//! literal) — tsc's `isLiteralLikeElementAccess`. A `const` reference whose
//! *type* happens to be a literal (e.g. `const _sym = "s"; this[_sym] = v;`,
//! or the `Symbol("s")` / `unique symbol` case the real conformance fixtures
//! use) does not late-bind; it stays an ordinary element access and reports
//! `TS7053` ("element implicitly has an 'any' type") like any other unmatched
//! key, exactly as it already does in a plain `.ts` file. Oracle-verified
//! against `typescript@7.0.2`. Tests here use string/numeric literal consts
//! rather than `Symbol(...)` because this crate's unit-test harness has no
//! lib (`Symbol` is unresolved) — the gate is type-agnostic (syntax-only), so
//! this is full coverage of the mechanism; the exact `Symbol()`/`unique
//! symbol` fixtures are covered by the TypeScript conformance corpus.
//!
//! Two independent owners had to change, both keyed on the same syntactic
//! literal-like predicate:
//! - `types/class_type/js_class_properties.rs`'s `extract_this_property_assignment`
//!   / `extract_jsdoc_this_property_declaration` no longer *synthesize* an
//!   implicit member from a non-literal element-access key (previously
//!   derived the property name from the key's evaluated *type*, via
//!   `literal_property_name`).
//! - `types/computation/access_helpers.rs`'s `is_direct_expando_element_write_base`
//!   no longer grants a `this`/`this`-alias write inside a class the generic
//!   JS "expando object" `TS7053` suppression that plain untyped objects and
//!   namespace/function statics legitimately get — a class instance is
//!   nominally typed and tsc never treats it as expando-shaped.
//!
//! Witness: `TypeScript/tests/cases/conformance/salsa/lateBoundClassMemberAssignmentJS{,2,3}.ts`.

use crate::CheckerOptions;
use crate::test_utils::check_js_source_codes_with_options;
use tsz_common::common::ScriptTarget;

const TS7053: u32 = 7053;

fn strict_js_codes(source: &str) -> Vec<u32> {
    check_js_source_codes_with_options(
        source,
        "test.js",
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    )
}

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

#[test]
fn numeric_literal_typed_const_key_does_not_late_bind() {
    // TypeScript/tests/cases/conformance/salsa/lateBoundClassMemberAssignmentJS.ts,
    // with `Symbol("_sym")` (needs lib) swapped for another literal-typed
    // const the harness can resolve without one; same non-literal-key rule.
    let codes = strict_js_codes(
        r#"
const _sym = 42;
export class MyClass {
    constructor() {
        this[_sym] = "ok";
    }

    method() {
        this[_sym] = "yep";
        const x = this[_sym];
    }
}
"#,
    );
    assert_eq!(
        count(&codes, TS7053),
        3,
        "one TS7053 per unmatched this[_sym] site: {codes:?}"
    );
}

#[test]
fn string_literal_typed_const_key_does_not_late_bind() {
    // TypeScript/tests/cases/conformance/salsa/lateBoundClassMemberAssignmentJS2.ts
    let codes = strict_js_codes(
        r#"
const _sym = "my-fake-sym";
export class MyClass {
    constructor() {
        this[_sym] = "ok";
    }

    method() {
        this[_sym] = "yep";
        const x = this[_sym];
    }
}
"#,
    );
    assert_eq!(
        count(&codes, TS7053),
        3,
        "the key's evaluated literal type must not stand in for a written literal: {codes:?}"
    );
}

#[test]
fn this_alias_const_key_does_not_late_bind() {
    // TypeScript/tests/cases/conformance/salsa/lateBoundClassMemberAssignmentJS3.ts —
    // `var self = this` alias, same non-literal-key rule applies through the alias.
    let codes = strict_js_codes(
        r#"
const _sym = 42;
export class MyClass {
    constructor() {
        var self = this
        self[_sym] = "ok";
    }

    method() {
        var self = this
        self[_sym] = "yep";
        const x = self[_sym];
    }
}
"#,
    );
    assert_eq!(
        count(&codes, TS7053),
        3,
        "the this-alias path shares the same literal-like gate: {codes:?}"
    );
}

#[test]
fn renamed_binder_still_does_not_late_bind() {
    // Same shape as the unique-symbol case with a different identifier, to
    // prove the gate is syntactic (literal-vs-reference), not name-driven.
    let codes = strict_js_codes(
        r#"
const key = "widget-key";
export class Widget {
    constructor() {
        this[key] = 1;
    }
}
"#,
    );
    assert_eq!(count(&codes, TS7053), 1, "{codes:?}");
}

#[test]
fn string_literal_key_still_late_binds() {
    // Positive control: a key written directly as a string literal in the
    // brackets is exactly tsc's `isLiteralLikeElementAccess` case and must
    // keep late-binding as it already correctly does.
    let codes = strict_js_codes(
        r#"
export class MyClass {
    constructor() {
        this["ok"] = "value";
    }
    method() {
        this["ok"] = "yep";
        const x = this["ok"];
    }
}
"#,
    );
    assert_eq!(count(&codes, TS7053), 0, "{codes:?}");
}

#[test]
fn numeric_literal_key_still_late_binds() {
    let codes = strict_js_codes(
        r#"
export class MyClass {
    constructor() {
        this[0] = "value";
    }
    method() {
        const x = this[0];
    }
}
"#,
    );
    assert_eq!(count(&codes, TS7053), 0, "{codes:?}");
}

#[test]
fn no_substitution_template_literal_key_still_late_binds() {
    let codes = strict_js_codes(
        r#"
export class MyClass {
    constructor() {
        this[`ok`] = "value";
    }
    method() {
        const x = this[`ok`];
    }
}
"#,
    );
    assert_eq!(count(&codes, TS7053), 0, "{codes:?}");
}

#[test]
fn jsdoc_annotated_non_literal_key_does_not_late_bind() {
    // The JSDoc-declaration sibling path (`extract_jsdoc_this_property_declaration`)
    // shares the same literal-like gate as the plain assignment path.
    let codes = strict_js_codes(
        r#"
const _sym = 42;
export class MyClass {
    constructor() {
        /** @type {string} */
        this[_sym] = "ok";
    }
    method() {
        const x = this[_sym];
    }
}
"#,
    );
    assert_eq!(
        count(&codes, TS7053),
        2,
        "a JSDoc type annotation does not make a const reference literal-like: {codes:?}"
    );
}

#[test]
fn dotted_this_property_assignment_unaffected() {
    // Regression guard: the ordinary `this.prop = value` (PropertyAccessExpression)
    // path never went through the element-access literal-like gate and must
    // keep late-binding exactly as before.
    let codes = strict_js_codes(
        r#"
export class MyClass {
    constructor() {
        this.prop = "value";
    }
    method() {
        this.prop = "yep";
        const x = this.prop;
    }
}
"#,
    );
    assert_eq!(count(&codes, TS7053), 0, "{codes:?}");
}
