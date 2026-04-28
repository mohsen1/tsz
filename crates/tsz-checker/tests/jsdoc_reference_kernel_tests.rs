//! Focused regression tests for the unified JSDoc reference-resolution kernel.
//!
//! These tests validate that typedef/import/template/callback reference
//! resolution goes through the single authoritative kernel
//! (`resolve_jsdoc_reference`) and produces correct diagnostics.
//!
//! Target conformance families:
//! - importTag*
//! - typedefCrossModule*
//! - templateInsideCallback
//! - jsdocTypeReferenceToImport*
//! - jsDeclarationsTypeReassignmentFromDeclaration*

use tsz_checker::context::CheckerOptions;

fn check_js(source: &str) -> Vec<u32> {
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

    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

fn check_js_strict(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        check_js: true,
        strict: true,
        strict_null_checks: true,
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

    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

// =============================================================================
// Unified resolution kernel: typedef lookup
// =============================================================================

/// A simple @typedef should resolve through the kernel and allow property access.
#[test]
fn typedef_basic_resolution_through_kernel() {
    let codes = check_js(
        r#"
/**
 * @typedef {Object} MyConfig
 * @property {string} name
 * @property {number} age
 */

/** @type {MyConfig} */
var config = { name: "test", age: 42 };
var n = config.name;
"#,
    );
    // Should not produce TS2339 (property does not exist)
    assert!(
        !codes.contains(&2339),
        "Expected no TS2339 for typedef property access, got: {codes:?}"
    );
}

/// A @callback typedef should resolve through the kernel.
#[test]
fn callback_typedef_resolution_through_kernel() {
    let codes = check_js(
        r#"
/**
 * @callback Predicate
 * @param {number} value
 * @returns {boolean}
 */

/** @type {Predicate} */
var pred = function(value) { return value > 0; };
"#,
    );
    // Should not produce TS2322 (type not assignable)
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 for callback typedef, got: {codes:?}"
    );
}

/// @typedef with template params should resolve generics through the kernel.
#[test]
fn generic_typedef_resolution_through_kernel() {
    let codes = check_js(
        r#"
/**
 * @template T
 * @typedef {Object} Container
 * @property {T} value
 */

/** @type {Container<string>} */
var c = { value: "hello" };
"#,
    );
    assert!(
        !codes.contains(&2339),
        "Expected no TS2339 for generic typedef, got: {codes:?}"
    );
}

// =============================================================================
// TS8039: @template after @typedef/@callback
// =============================================================================

/// A bare @template after @typedef does not produce TS8039; TS8021 covers
/// the malformed typedef instead.
#[test]
fn template_after_typedef_no_ts8039() {
    let codes = check_js(
        r#"
/**
 * @typedef {Object} Foo
 * @template T
 */
"#,
    );
    assert!(
        !codes.contains(&8039),
        "Expected no TS8039 when @template immediately follows @typedef, got: {codes:?}"
    );
}

/// @template tags placed AFTER a single @typedef bind to the typedef host,
/// matching tsc's lenient template-placement rule for @typedef. Without this,
/// `Funcs<A, B>` use sites would fire TS2315 ("Type 'Funcs' is not generic").
/// See conformance test `contravariantOnlyInferenceFromAnnotatedFunctionJs.ts`.
#[test]
fn template_after_typedef_binds_as_generic_params() {
    let codes = check_js_strict(
        r#"
/**
 * @typedef {{ [K in keyof B]: { fn: (a: A) => B[K]; } }} Funcs
 * @template A
 * @template {Record<string, unknown>} B
 */

/**
 * @template A
 * @template {Record<string, unknown>} B
 * @param {Funcs<A, B>} fns
 */
function foo(fns) {}

foo({});
"#,
    );
    assert!(
        !codes.contains(&2315),
        "Funcs<A, B> must not emit TS2315 when templates are after @typedef, got: {codes:?}"
    );
}

#[test]
fn template_after_typedef_property_emits_ts8039() {
    let codes = check_js(
        r#"
/**
 * @typedef {Object} Foo
 * @property {number} value
 * @template T
 */
"#,
    );
    assert!(
        codes.contains(&8039),
        "Expected TS8039 when @template follows a typedef child tag, got: {codes:?}"
    );
}

/// @template after @callback is invalid and does not define callback params.
#[test]
fn template_after_callback_emits_ts8039() {
    let codes = check_js(
        r#"
/**
 * @callback Call
 * @param {number} x
 * @template T
 */
"#,
    );
    assert!(
        codes.contains(&8039),
        "Expected TS8039 when @template follows @callback, got: {codes:?}"
    );
}

/// @template BEFORE @typedef should NOT emit TS8039.
#[test]
fn template_before_typedef_no_ts8039() {
    let codes = check_js(
        r#"
/**
 * @template T
 * @typedef {Object} Foo
 * @property {T} value
 */
"#,
    );
    assert!(
        !codes.contains(&8039),
        "Expected no TS8039 when @template is before @typedef, got: {codes:?}"
    );
}

/// @template BEFORE @callback should NOT emit TS8039.
#[test]
fn template_before_callback_no_ts8039() {
    let codes = check_js(
        r#"
/**
 * @template T
 * @callback Mapper
 * @param {T} value
 * @returns {T}
 */
"#,
    );
    assert!(
        !codes.contains(&8039),
        "Expected no TS8039 when @template is before @callback, got: {codes:?}"
    );
}

#[test]
fn template_inside_callback_reports_invalid_template_and_fallout() {
    let codes = check_js_strict(
        r#"
/**
 * @callback Call
 * @template T
 * @param {T} x
 * @returns {T}
 */
/**
 * @template T
 * @type {Call<T>}
 */
const identity = x => x;
"#,
    );

    assert!(
        codes.contains(&8039),
        "Expected invalid @template placement TS8039, got: {codes:?}"
    );
    assert!(
        codes.contains(&2304),
        "Expected unresolved T after invalid callback template, got: {codes:?}"
    );
    assert!(
        codes.contains(&2315),
        "Expected non-generic callback alias TS2315, got: {codes:?}"
    );
    assert!(
        codes.contains(&7006),
        "Expected implicit-any fallout TS7006, got: {codes:?}"
    );
}

// =============================================================================
// TS8021: @typedef without type or properties
// =============================================================================

/// Bare @typedef with no type annotation and no @property tags should emit TS8021.
#[test]
fn bare_typedef_emits_ts8021() {
    let codes = check_js(
        r#"
/** @typedef T */
"#,
    );
    assert!(
        codes.contains(&8021),
        "Expected TS8021 for bare @typedef, got: {codes:?}"
    );
}

/// @typedef with type annotation should NOT emit TS8021.
#[test]
fn typedef_with_type_no_ts8021() {
    let codes = check_js(
        r#"
/** @typedef {Object} Foo */
"#,
    );
    assert!(
        !codes.contains(&8021),
        "Expected no TS8021 for @typedef with type, got: {codes:?}"
    );
}

// =============================================================================
// Kernel: no duplicate resolution (architectural regression test)
// =============================================================================

/// Ensure that `resolve_jsdoc_reference` handles import-style type references
/// without needing a separate fallback chain.
#[test]
fn import_type_expression_resolved_through_kernel() {
    let codes = check_js(
        r#"
/** @type {import("./types").Foo} */
var x;
"#,
    );
    // We expect TS2307 (cannot find module) since the module doesn't exist,
    // but NOT TS2339 (property does not exist) which would indicate the
    // import expression wasn't parsed at all.
    assert!(
        !codes.contains(&2339),
        "Expected no TS2339 for import type expression, got: {codes:?}"
    );
}

/// @type with a typedef name should resolve through the unified kernel
/// without needing separate `typedef/import/file_locals` fallback.
#[test]
fn type_tag_with_typedef_name_resolved_through_kernel() {
    let codes = check_js(
        r#"
/**
 * @typedef {Object} Point
 * @property {number} x
 * @property {number} y
 */

/** @type {Point} */
var p = { x: 1, y: 2 };
var px = p.x;
var py = p.y;
"#,
    );
    assert!(
        !codes.contains(&2339),
        "Expected no TS2339 for typedef property access, got: {codes:?}"
    );
}

/// Recursive @typedef should not cause infinite loop.
#[test]
fn recursive_typedef_no_infinite_loop() {
    let codes = check_js(
        r#"
/**
 * @typedef {string | Json[]} Json
 */

/** @type {Json} */
var j = "hello";
"#,
    );
    // Should complete without hanging; the actual diagnostics don't matter much
    // as long as we don't loop.
    let _ = codes;
}

// =============================================================================
// @typedef with @property: compound resolution
// =============================================================================

/// @typedef with nested @property should resolve through kernel.
#[test]
fn typedef_with_nested_properties() {
    let codes = check_js(
        r#"
/**
 * @typedef {Object} Options
 * @property {Object} nested
 * @property {string} nested.name
 * @property {number} nested.value
 */

/** @type {Options} */
var opts = { nested: { name: "test", value: 42 } };
"#,
    );
    assert!(
        !codes.contains(&2339),
        "Expected no TS2339 for nested property access, got: {codes:?}"
    );
}

// =============================================================================
// Optional @property with strict null checks
// =============================================================================

/// Optional @property should include undefined in union with strict null checks.
#[test]
fn typedef_optional_property_strict() {
    let codes = check_js_strict(
        r#"
/**
 * @typedef {Object} Config
 * @property {string} name
 * @property {number} [age]
 */

/** @type {Config} */
var c = { name: "test" };
"#,
    );
    // With strict null checks, optional property should be string | undefined,
    // so assigning without 'age' should be fine.
    assert!(
        !codes.contains(&2741),
        "Expected no TS2741 for optional property, got: {codes:?}"
    );
}
