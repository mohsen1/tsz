//! TS2322 source/target display for class property `/** @type {T} */ name = expr`.
//!
//! Regression for `jsdocPrivateName1.ts`: a JSDoc `@type` annotation on a
//! class property declaration declares the property's type (the assignment
//! target), not the initializer's source type. The TS2322 diagnostic must
//! show the initializer's actual type as the source (e.g. `number` for `3`),
//! not the JSDoc-declared property type (which would produce a tautological
//! "Type 'boolean' is not assignable to type 'boolean'.").
//!
//! Same shape as the existing `module.exports = X` and `Foo.prototype = X`
//! carve-outs in `jsdoc_annotated_expression_display`.

use rustc_hash::FxHashSet;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostics_for_js(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        no_implicit_any: false,
        ..CheckerOptions::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.js".to_string(),
        options,
    );
    let _: FxHashSet<u32> = FxHashSet::default();
    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// `class A { /** @type {boolean} */ #foo = 3 }` must emit
/// `Type 'number' is not assignable to type 'boolean'.` — source uses the
/// initializer's actual type (`number`), not the JSDoc-declared target type
/// (`boolean`). Without the carve-out the property name node picks up the
/// `@type {boolean}` annotation as the "source" string and the diagnostic
/// degenerates into "Type 'boolean' is not assignable to type 'boolean'.".
#[test]
fn ts2322_for_private_class_property_jsdoc_uses_initializer_type_for_source() {
    let diags = diagnostics_for_js(
        r#"
class A {
    /** @type {boolean} some number value */
    #foo = 3
}
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322; got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("'number'") && msg.contains("'boolean'"),
        "TS2322 must show source as 'number' and target as 'boolean'; got: {msg:?}"
    );
    assert!(
        !msg.contains("Type 'boolean' is not assignable to type 'boolean'"),
        "TS2322 must not collapse both sides to the JSDoc-declared target type; got: {msg:?}"
    );
}

/// Same bug, public (non-`#`) class property — verify the fix is not specific
/// to private identifiers.
#[test]
fn ts2322_for_public_class_property_jsdoc_uses_initializer_type_for_source() {
    let diags = diagnostics_for_js(
        r#"
class A {
    /** @type {boolean} */
    foo = 3
}
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322; got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("'number'") && msg.contains("'boolean'"),
        "TS2322 must show source as 'number' and target as 'boolean'; got: {msg:?}"
    );
    assert!(
        !msg.contains("Type 'boolean' is not assignable to type 'boolean'"),
        "TS2322 must not collapse both sides to the JSDoc-declared target type; got: {msg:?}"
    );
}

/// String initializer + JSDoc `@type {boolean}` — different source/target
/// combo to confirm the source string is genuinely the initializer's type
/// rather than just any other primitive.
#[test]
fn ts2322_for_string_initializer_class_property_jsdoc_uses_initializer_type_for_source() {
    let diags = diagnostics_for_js(
        r#"
class A {
    /** @type {boolean} */
    foo = "hello"
}
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322; got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("'string'") && msg.contains("'boolean'"),
        "TS2322 must show source as 'string' and target as 'boolean'; got: {msg:?}"
    );
}
