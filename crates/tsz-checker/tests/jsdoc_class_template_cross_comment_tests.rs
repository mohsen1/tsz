//! Tests for class-level `@template T` being recognized in JSDoc on
//! class members (different comment).
//!
//! Without the cross-comment scope, a class-level `/** @template T */`
//! followed by a class member with `/** @param {T} x */` produces a
//! false-positive TS2304 ("Cannot find name 'T'") because the
//! file-level @param/@type validation runs per-comment without seeing
//! the class-level @template declaration. The fix lives in
//! `tsz_checker::jsdoc::diagnostics::source_file_declares_jsdoc_template_at`,
//! gated at the diagnostic-emit sites in `emit_jsdoc_cannot_find_name`
//! (jsdoc/diagnostics.rs) and the generic-instantiation path in
//! `report_jsdoc_simple_generic_instantiation_errors`
//! (`jsdoc/params_generic_instantiation.rs`).
//!
//! Scope rule: only **class** declarations propagate `@template T` across
//! comments. Function- and typedef-level @template declarations are NOT
//! file-wide — they apply only to their own JSDoc comment, which is
//! already handled by the existing local-comment skip in
//! `check_jsdoc_typedef_base_types`. Suppressing those file-wide would
//! regress conformance tests like `jsdocTemplateConstructorFunction2.ts`
//! and `typedefTagTypeResolution.ts`, where a standalone typedef
//! legitimately fails to resolve a name declared by an unrelated
//! function's `@template`.
//!
//! Conformance impact: PASSes `jsdocTemplateClass.ts` and
//! `contravariantOnlyInferenceFromAnnotatedFunctionJs.ts`; preserves
//! `jsdocTemplateConstructorFunction2.ts` and `typedefTagTypeResolution.ts`.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn check_js(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        check_js: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.js", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

#[test]
fn class_template_visible_to_method_param_jsdoc() {
    // `@template T` on the class, and a method's `@param {T}` references
    // it from a different JSDoc comment. Must NOT produce TS2304 for `'T'`.
    let source = r#"
/** @template T */
class Foo {
    /**
     * @param {T} x
     * @return {T}
     */
    foo(x) {
        return x;
    }
}
"#;
    let diags = check_js(source);
    let ts2304_for_t: Vec<&(u32, String)> = diags
        .iter()
        .filter(|(c, m)| *c == 2304 && m.contains("'T'"))
        .collect();
    assert!(
        ts2304_for_t.is_empty(),
        "expected no TS2304 for cross-comment @template T, got: {diags:?}",
    );
}

#[test]
fn class_template_visible_to_typedef_inside_class_body() {
    // Inner `/** @typedef {(t: T) => T} Id2 */` declared INSIDE a class
    // body — references the class's @template T. Must not emit TS2304.
    let source = r#"
/** @template T */
class Foo {
    /** @typedef {(t: T) => T} Id2 */
    /** @param {Id2} x */
    bar(x) {}
}
"#;
    let diags = check_js(source);
    let ts2304_for_t: Vec<&(u32, String)> = diags
        .iter()
        .filter(|(c, m)| *c == 2304 && m.contains("'T'"))
        .collect();
    assert!(
        ts2304_for_t.is_empty(),
        "expected no TS2304 for typedef referencing class @template, got: {diags:?}",
    );
}

#[test]
fn class_template_visible_to_method_generic_arg() {
    // The generic-instantiation code path: `@param {Foo<T>}` where Foo is
    // a typedef and T is the class's @template. Must not emit TS2304 for T.
    let source = r#"
/**
 * @template T
 * @typedef {(t: T) => T} Id
 */
/** @template T */
class Foo {
    /** @param {Id<T>} y */
    foo(y) {}
}
"#;
    let diags = check_js(source);
    let ts2304_for_t: Vec<&(u32, String)> = diags
        .iter()
        .filter(|(c, m)| *c == 2304 && m.contains("'T'"))
        .collect();
    assert!(
        ts2304_for_t.is_empty(),
        "expected no TS2304 for generic instantiation referencing class @template, got: {diags:?}",
    );
}
