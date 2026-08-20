//! TS1005 for Closure-style `function(...)` JSDoc types.
//!
//! TypeScript 7 does not accept the Closure function-type form. It reports
//! TS1005 `'}' expected.` anchored at the open paren, and the annotated symbol
//! gets an error type rather than a reconstructed signature. Across the
//! conformance corpus, 43 of 47 JSDoc `function(` sites carry TS1005 or TS1003
//! in the oracle.
//!
//! The two documented exceptions are covered here as negatives: the `@enum`
//! tag, which the pinned compiler does not implement (so it never parses the
//! tag's type — `enumTag.ts` and `jsDeclarationsEnumTag.ts` expect no TS1005),
//! and `.ts`/`.tsx` files, where JSDoc types are not used as types.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source, check_source_diagnostics};

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

fn ts1005_columns(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.js", options)
        .into_iter()
        .filter(|d| d.code == 1005)
        .map(|d| d.start)
        .collect()
}

#[test]
fn reports_in_every_tag_position() {
    // @type on a variable, and @param/@return on a function, are all rejected.
    let cases = [
        "/** @type {function(string): void} */\nconst f = (v) => {};\n",
        "/**\n * @param {function(string): number} c\n */\nfunction f(c) { return c; }\n",
        "/**\n * @return {function(string): number}\n */\nfunction f() { return null; }\n",
        "/**\n * @returns {function(string): number}\n */\nfunction f() { return null; }\n",
    ];
    for source in cases {
        assert!(
            js_codes(source).contains(&1005),
            "expected TS1005 for: {source}"
        );
    }
}

#[test]
fn accepts_the_spacing_and_prefix_variants() {
    // `function (x)` with a space, and the Closure nullability prefixes, are
    // the same construct.
    for source in [
        "/** @type {function (string): void} */\nconst f = (v) => {};\n",
        "/** @type {!function(string): void} */\nconst f = (v) => {};\n",
        "/** @type {?function(string): void} */\nconst f = (v) => {};\n",
    ] {
        assert!(
            js_codes(source).contains(&1005),
            "expected TS1005 for: {source}"
        );
    }
}

#[test]
fn covers_the_closure_only_parameter_forms() {
    // `new:` and `this:` first parameters and `...` rest are Closure-only
    // spellings of the same rejected construct.
    for source in [
        "/**\n * @param {function(new: Object, string)} c\n */\nfunction f(c) { return c; }\n",
        "/**\n * @param {function(this: string, number): number} c\n */\nfunction f(c) { return c; }\n",
        "/**\n * @param {function(...number): string} c\n */\nfunction f(c) { return c; }\n",
    ] {
        assert!(
            js_codes(source).contains(&1005),
            "expected TS1005 for: {source}"
        );
    }
}

#[test]
fn anchors_at_the_open_paren() {
    // tsc points at the `(`, not at the tag or the `{`.
    // `/** @type {function(string): void} */` — the paren is at 0-based 19.
    let cols = ts1005_columns("/** @type {function(string): void} */\nconst f = (v) => {};\n");
    assert_eq!(cols, vec![19], "TS1005 must anchor at the open paren");
}

#[test]
fn anchor_is_name_independent() {
    // Renamed binders and differing parameter types must not move the anchor
    // relative to the tag — the rule is structural.
    for (name, param) in [("f", "string"), ("compute", "number"), ("_h0", "boolean")] {
        let source =
            format!("/** @type {{function({param}): void}} */\nconst {name} = (v) => {{}};\n");
        assert_eq!(
            ts1005_columns(&source),
            vec![19],
            "name={name} param={param}"
        );
    }
}

#[test]
fn a_type_merely_named_function_like_is_not_this_construct() {
    // `functionLike` starts with the keyword but is an ordinary type name.
    let codes = js_codes("/** @type {functionLike} */\nconst f = 1;\n");
    assert!(!codes.contains(&1005), "got {codes:?}");
}

#[test]
fn enum_tag_is_not_reported() {
    // The pinned compiler does not implement `@enum`, so it never parses the
    // tag's type. `enumTag.ts` and `jsDeclarationsEnumTag.ts` expect no TS1005
    // despite carrying `@enum {function(number): number}`.
    let codes = js_codes("/** @enum {function(number): number} */\nconst Fs = { a: n => n };\n");
    assert!(!codes.contains(&1005), "got {codes:?}");
}

#[test]
fn typescript_files_are_not_reported() {
    let diags =
        check_source_diagnostics("/** @type {function(string): void} */\nconst f: any = 1;\n");
    assert!(
        diags.iter().all(|d| d.code != 1005),
        "JSDoc types are not types in .ts; got: {diags:?}"
    );
}

#[test]
fn ordinary_jsdoc_function_types_are_unaffected() {
    // The TypeScript arrow form is the supported spelling and must stay clean.
    for source in [
        "/** @type {(s: string) => void} */\nconst f = (s) => {};\n",
        "/**\n * @param {(s: string) => number} c\n */\nfunction f(c) { return c; }\n",
    ] {
        let codes = js_codes(source);
        assert!(!codes.contains(&1005), "got {codes:?} for {source}");
    }
}

#[test]
fn closure_type_on_a_function_declaration_reports_ts8030() {
    // The Closure form yields no call signature, so a `@type` tag carrying it
    // on a function declaration fails the "must have a signature with the
    // correct number of arguments" check. `jsdocFunction_missingReturn`'s
    // oracle is exactly TS1005 + TS8030 for this shape.
    let codes = js_codes("/** @type {function(): number} */\nfunction f() {}\n");
    assert!(codes.contains(&1005), "got {codes:?}");
    assert!(codes.contains(&8030), "got {codes:?}");
}

#[test]
fn a_resolvable_callable_type_does_not_report_ts8030() {
    // Control: the arrow form still supplies a signature, so the TS8030 skip
    // that the Closure form no longer earns must remain in place for it.
    let codes = js_codes("/** @type {() => number} */\nfunction f() { return 1; }\n");
    assert!(!codes.contains(&8030), "got {codes:?}");
}

#[test]
fn closure_type_on_an_object_literal_method_reports_ts8030() {
    // tsc runs the TS8030 check from `checkFunctionOrMethodDeclaration`, which
    // covers method declarations. `checkJsdocTypeTagOnObjectProperty1`'s oracle
    // reports it for the method shorthand.
    let codes = js_codes(
        "const obj = {\n  /** @type {function(number): number} */\n  method1(n1) { return n1; }\n};\n",
    );
    assert!(codes.contains(&8030), "got {codes:?}");
}

#[test]
fn closure_type_on_an_arrow_initialized_property_reports_no_ts8030() {
    // A property with a function or arrow *initializer* is an expression, not a
    // method declaration. The same oracle reports no TS8030 for `arrowFunc`
    // under an identical tag — only the TS1005 for the type itself.
    let codes = js_codes(
        "const obj = {\n  /** @type {function(number): number} */\n  arrowFunc: (num) => num\n};\n",
    );
    assert!(
        codes.contains(&1005),
        "the type is still rejected: {codes:?}"
    );
    assert!(!codes.contains(&8030), "got {codes:?}");
}

#[test]
fn class_methods_are_left_alone() {
    // The pass is restricted to methods directly inside an object literal;
    // there is no corpus witness for class methods, so it must not fire there.
    let codes =
        js_codes("class C {\n  /** @type {function(number): number} */\n  m(n) { return n; }\n}\n");
    assert!(!codes.contains(&8030), "got {codes:?}");
}
