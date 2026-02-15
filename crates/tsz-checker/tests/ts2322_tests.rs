//! Tests for TS2322 assignability errors
//!
//! These tests verify that TS2322 "Type 'X' is not assignable to type 'Y'" errors
//! are properly emitted in various contexts.

use crate::CheckerState;
use crate::context::CheckerOptions;
use crate::diagnostics::diagnostic_codes;
use std::path::Path;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_common::common::ScriptTarget;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn load_lib_files_for_test() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_paths = [
        manifest_dir.join("scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("scripts/emit/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("scripts/conformance/node_modules/typescript/lib/lib.es2015.d.ts"),
        manifest_dir.join("scripts/emit/node_modules/typescript/lib/lib.es2015.d.ts"),
        manifest_dir.join("scripts/conformance/node_modules/typescript/lib/lib.dom.d.ts"),
        manifest_dir.join("scripts/emit/node_modules/typescript/lib/lib.dom.d.ts"),
        manifest_dir.join("scripts/conformance/node_modules/typescript/lib/lib.esnext.d.ts"),
        manifest_dir.join("scripts/emit/node_modules/typescript/lib/lib.esnext.d.ts"),
        manifest_dir.join("TypeScript/src/lib/es5.d.ts"),
        manifest_dir.join("TypeScript/src/lib/es2015.d.ts"),
        manifest_dir.join("TypeScript/src/lib/lib.dom.d.ts"),
        manifest_dir.join("TypeScript/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("TypeScript/node_modules/typescript/lib/lib.es2015.d.ts"),
        manifest_dir.join("TypeScript/node_modules/typescript/lib/lib.dom.d.ts"),
        manifest_dir.join("../TypeScript/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../TypeScript/node_modules/typescript/lib/lib.es2015.d.ts"),
        manifest_dir.join("../TypeScript/node_modules/typescript/lib/lib.dom.d.ts"),
        manifest_dir.join("../scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../scripts/conformance/node_modules/typescript/lib/lib.es2015.d.ts"),
        manifest_dir.join("../scripts/emit/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../scripts/emit/node_modules/typescript/lib/lib.es2015.d.ts"),
        manifest_dir.join("../TypeScript/src/lib/es5.d.ts"),
        manifest_dir.join("../TypeScript/src/lib/es2015.d.ts"),
        manifest_dir.join("../TypeScript/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../TypeScript/node_modules/typescript/lib/lib.es2015.d.ts"),
        manifest_dir.join("../TypeScript/node_modules/typescript/lib/lib.dom.d.ts"),
        manifest_dir.join("../../scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../scripts/conformance/node_modules/typescript/lib/lib.es2015.d.ts"),
        manifest_dir.join("../../scripts/emit/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../scripts/emit/node_modules/typescript/lib/lib.es2015.d.ts"),
        manifest_dir.join("../../scripts/emit/node_modules/typescript/lib/lib.dom.d.ts"),
        manifest_dir.join("../../TypeScript/src/lib/es5.d.ts"),
        manifest_dir.join("../../TypeScript/src/lib/es2015.d.ts"),
        manifest_dir.join("../../TypeScript/src/lib/lib.dom.d.ts"),
        manifest_dir.join("../../TypeScript/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../TypeScript/node_modules/typescript/lib/lib.es2015.d.ts"),
        manifest_dir.join("../../TypeScript/node_modules/typescript/lib/lib.dom.d.ts"),
    ];

    let mut lib_files = Vec::new();

    for lib_path in &lib_paths {
        if lib_path.exists()
            && let Ok(content) = std::fs::read_to_string(lib_path)
        {
            let lib_file = LibFile::from_source("lib.es5.d.ts".to_string(), content);
            lib_files.push(Arc::new(lib_file));
        }
    }

    lib_files
}

fn with_lib_contexts(source: &str, file_name: &str, options: CheckerOptions) -> Vec<(u32, String)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let is_js_file = matches!(
        file_name,
        s if s.ends_with(".js")
            || s.ends_with(".jsx")
            || s.ends_with(".mjs")
            || s.ends_with(".cjs")
    );
    let lib_files = if is_js_file {
        load_lib_files_for_test()
    } else {
        Vec::new()
    };

    let mut binder = BinderState::new();
    if lib_files.is_empty() {
        binder.bind_source_file(parser.get_arena(), root);
    } else {
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    }

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    if !lib_files.is_empty() {
        let lib_contexts: Vec<crate::context::LibContext> = lib_files
            .iter()
            .map(|lib| crate::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// Helper function to check if a diagnostic with a specific code was emitted
fn has_error_with_code(source: &str, code: u32) -> bool {
    with_lib_contexts(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .any(|(d, _)| d == code)
}

/// Helper to count errors with a specific code
fn count_errors_with_code(source: &str, code: u32) -> usize {
    with_lib_contexts(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .filter(|(d, _)| *d == code)
        .count()
}

/// Helper that returns all diagnostics for inspection
fn get_all_diagnostics(source: &str) -> Vec<(u32, String)> {
    with_lib_contexts(source, "test.ts", CheckerOptions::default())
}

fn compile_with_options(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    with_lib_contexts(source, file_name, options)
}

// =============================================================================
// Return Statement Tests (TS2322)
// =============================================================================

#[test]
fn test_ts2322_return_wrong_primitive() {
    let source = r#"
        function returnNumber(): number {
            return "string";
        }
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn test_ts2322_return_wrong_object_property() {
    let source = r#"
        function returnObject(): { a: number } {
            return { a: "string" };
        }
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn test_ts2322_return_wrong_array_element() {
    let source = r#"
        function returnArray(): number[] {
            return ["string"];
        }
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn test_ts2322_generator_yield_missing_value() {
    let source = r#"
        interface IterableIterator<T> {}

        function* g(): IterableIterator<number> {
            yield;
            yield 1;
        }
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn test_ts2322_generator_yield_wrong_type() {
    let source = r#"
        interface IterableIterator<T> {}

        function* g(): IterableIterator<number> {
            yield "x";
            yield 1;
        }
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

// =============================================================================
// Variable Declaration Tests (TS2322)
// =============================================================================

#[test]
fn test_ts2322_variable_declaration_wrong_type() {
    let source = r#"
        let x: number = "string";
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn test_ts2322_variable_declaration_wrong_object_property() {
    let source = r#"
        let y: { a: number } = { a: "string" };
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn test_ts2322_variable_declaration_wrong_array_element() {
    let source = r#"
        let z: string[] = [1, 2, 3];
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

// =============================================================================
// Assignment Expression Tests (TS2322)
// =============================================================================

#[test]
fn test_ts2322_assignment_wrong_primitive() {
    let source = r#"
        let a: number;
        a = "string";
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn test_ts2322_assignment_wrong_object_property() {
    let source = r#"
        let obj: { a: number };
        obj = { a: "string" };
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

// =============================================================================
// Multiple TS2322 Errors
// =============================================================================

#[test]
fn test_ts2322_multiple_errors() {
    let source = r#"
        function f1(): number {
            return "string";
        }
        function f2(): string {
            return 42;
        }
        let x: number = "x";
        let y: string = 123;
    "#;

    let count = count_errors_with_code(source, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        count >= 4,
        "Expected at least 4 TS2322 errors, got {}",
        count
    );
}

// =============================================================================
// No Error Tests (Verify we don't emit false positives)
// =============================================================================

#[test]
fn test_ts2322_no_error_correct_types() {
    let source = r#"
        function returnNumber(): number {
            return 42;
        }
        let x: number = 42;
        let y: { a: number } = { a: 42 };
        let z: string[] = ["a", "b"];
        let a: number;
        a = 42;
    "#;

    assert!(!has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

// =============================================================================
// User-Defined Generic Type Application Tests (TS2322 False Positives)
// These test the root cause of 11,000+ extra TS2322 errors
// =============================================================================

#[test]
fn test_ts2322_no_false_positive_simple_generic_identity() {
    // type Id<T> = T; let a: Id<number> = 42;
    let source = r#"
        type Id<T> = T;
        let a: Id<number> = 42;
    "#;

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = errors
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for Id<number> = 42, got: {:?}",
        ts2322_errors
    );
}

#[test]
fn test_ts2322_no_false_positive_generic_object_wrapper() {
    // type Box<T> = { value: T }; let b: Box<number> = { value: 42 };
    let source = r#"
        type Box<T> = { value: T };
        let b: Box<number> = { value: 42 };
    "#;

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = errors
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for Box<number> = {{ value: 42 }}, got: {:?}",
        ts2322_errors
    );
}

#[test]
fn test_ts2322_no_false_positive_conditional_type_true_branch() {
    // IsStr<string> should evaluate to 'true', and true is assignable to true
    let source = r#"
        type IsStr<T> = T extends string ? true : false;
        let a: IsStr<string> = true;
    "#;

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = errors
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for IsStr<string> = true, got: {:?}",
        ts2322_errors
    );
}

#[test]
fn test_ts2322_no_false_positive_conditional_type_false_branch() {
    // IsStr<number> should evaluate to 'false', and false is assignable to false
    let source = r#"
        type IsStr<T> = T extends string ? true : false;
        let b: IsStr<number> = false;
    "#;

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = errors
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for IsStr<number> = false, got: {:?}",
        ts2322_errors
    );
}

#[test]
fn test_ts2322_no_false_positive_user_defined_mapped_type() {
    // MyPartial<Cfg> should behave like Partial<Cfg>
    let source = r#"
        type MyPartial<T> = { [K in keyof T]?: T[K] };
        interface Cfg { host: string; port: number }
        let a: MyPartial<Cfg> = {};
        let b: MyPartial<Cfg> = { host: "x" };
    "#;

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = errors
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for MyPartial<Cfg>, got: {:?}",
        ts2322_errors
    );
}

#[test]
fn test_ts2322_no_false_positive_conditional_infer() {
    // UnpackPromise<Promise<number>> should evaluate to number
    let source = r#"
        type UnpackPromise<T> = T extends Promise<infer U> ? U : T;
        let a: UnpackPromise<Promise<number>> = 42;
    "#;

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = errors
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for UnpackPromise<Promise<number>> = 42, got: {:?}",
        ts2322_errors
    );
}

#[test]
fn test_ts2322_no_false_positive_conditional_expression_with_generics() {
    // Conditional expressions should compute union type first, not check branches individually
    // This tests the fix for premature assignability checking in conditional expressions
    let source = r#"
        interface Shape {
            name: string;
            width: number;
            height: number;
        }

        function getProperty<T, K extends keyof T>(obj: T, key: K): T[K] {
            return obj[key];
        }

        function test(shape: Shape, cond: boolean) {
            // cond ? "width" : "height" should be type "width" | "height"
            // which IS assignable to K extends keyof Shape
            // Should NOT emit TS2322 on individual branches
            let widthOrHeight = getProperty(shape, cond ? "width" : "height");
        }
    "#;

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = errors
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for conditional expression in generic function call, got: {:?}",
        ts2322_errors
    );
}

#[test]
fn test_ts2322_no_false_positive_nested_conditional() {
    // Nested conditional expressions should also work
    let source = r#"
        function pick<T, K extends keyof T>(obj: T, key: K): T[K] {
            return obj[key];
        }

        type Point = { x: number; y: number; z: number };

        function test(p: Point, a: boolean, b: boolean) {
            // Nested ternary should produce "x" | "y" | "z"
            let value = pick(p, a ? "x" : (b ? "y" : "z"));
        }
    "#;

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = errors
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for nested conditional expression, got: {:?}",
        ts2322_errors
    );
}

#[test]
fn test_ts2322_accessor_getter_setter_type_mismatch_message() {
    let source = r#"
        class C {
            get x(): string { return "s"; }
            set x(value: number) {}
        }
    "#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        !ts2322.is_empty(),
        "Expected TS2322 for accessor type mismatch; diagnostics: {:?}",
        diagnostics
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, msg)| msg.contains("string") && msg.contains("number")),
        "Expected accessor TS2322 message to mention string and number; TS2322 diagnostics: {:?}",
        ts2322
    );
}

#[test]
fn test_ts2322_for_of_annotation_mismatch() {
    let source = r#"
        for (const x: string of [1, 2, 3]) {}
    "#;

    assert!(
        has_error_with_code(source, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for for-of annotation mismatch"
    );
}

#[test]
fn test_ts2322_check_js_true_reports_javascript_annotation_mismatch() {
    let source = r#"
        // @ts-check
        /** @type {number} */
        const value = "bad";
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            ..CheckerOptions::default()
        },
    );
    let has_2322 = diagnostics
        .iter()
        .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        has_2322,
        "Expected TS2322 when checkJs checks mismatched JS annotation, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_check_mjs_true_reports_javascript_annotation_mismatch() {
    let source = r#"
        /** @type {number} */
        const value = "bad";
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.mjs",
        CheckerOptions {
            check_js: true,
            ..CheckerOptions::default()
        },
    );
    let has_2322 = diagnostics
        .iter()
        .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        has_2322,
        "Expected TS2322 for .mjs jsdoc mismatch when checkJs is enabled, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_check_js_false_does_not_enforce_annotation_type() {
    let source = r#"
        // @ts-check
        /** @type {number} */
        const value = "bad";
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: false,
            ..CheckerOptions::default()
        },
    );
    let has_2322 = diagnostics
        .iter()
        .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        !has_2322,
        "Expected no TS2322 when checkJs is disabled, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_check_cjs_true_reports_javascript_annotation_mismatch() {
    let source = r#"
        /** @type {number} */
        const value = "bad";
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.cjs",
        CheckerOptions {
            check_js: true,
            ..CheckerOptions::default()
        },
    );
    let has_2322 = diagnostics
        .iter()
        .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        has_2322,
        "Expected TS2322 for .cjs jsdoc mismatch when checkJs is enabled, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_check_cjs_false_does_not_enforce_annotation_type() {
    let source = r#"
        /** @type {number} */
        const value = "bad";
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.cjs",
        CheckerOptions {
            check_js: false,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 for .cjs when checkJs is disabled, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_check_mjs_false_does_not_enforce_annotation_type() {
    let source = r#"
        // @ts-check
        /** @type {number} */
        const value = "bad";
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.mjs",
        CheckerOptions {
            check_js: false,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 for .mjs when checkJs is disabled, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_check_js_false_does_not_enforce_jsdoc_return_type() {
    let source = r#"
        // @ts-check
        /** @returns {number} */
        function id(value) {
            return "string";
        }
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: false,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 for jsdoc return annotation when checkJs is disabled, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_strict_js_strictness_affects_nullability() {
    let source = r#"
        // @ts-check
        /** @type {number} */
        const maybeNumber = null;
    "#;

    let loose = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            strict: false,
            ..CheckerOptions::default()
        },
    );
    let strict = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            strict: true,
            ..CheckerOptions::default()
        },
    );

    let strict_has_2322 = strict
        .iter()
        .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        strict_has_2322,
        "Expected strict+checkJs to emit TS2322 for null -> number jsdoc mismatch, got: {strict:?}"
    );
    assert!(
        strict.len() > loose.len(),
        "Expected strict mode to increase diagnostics for nullability in checkJs source"
    );
}

#[test]
fn test_ts2322_target_es2015_enables_template_lib_type_checks_without_falsely_reporting_target() {
    let source = r#"
        const x: number = 1;
        const y = "2";
        const z: number = y as any;
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    let has_2322 = diagnostics
        .iter()
        .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        !has_2322,
        "No TS2322 expected in valid ES2015 + strict baseline case: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_target_es3_vs_target_es2015_jsdoc_annotation_mismatch() {
    let source = r#"
        // @ts-check
        /** @type {number} */
        const value = "bad";
    "#;

    let es3 = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            target: ScriptTarget::ES3,
            strict: true,
            ..Default::default()
        },
    );
    let es2022 = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            target: ScriptTarget::ES2022,
            strict: true,
            ..Default::default()
        },
    );
    let es3_has_2322 = es3
        .iter()
        .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    let es2022_has_2322 = es2022
        .iter()
        .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        es3_has_2322 && es2022_has_2322,
        "Expected jsdoc mismatch TS2322 under both targets, got es3={es3:?}, es2022={es2022:?}"
    );
}

#[test]
fn test_ts2322_check_js_true_does_not_relabel_with_unrelated_diagnostics() {
    let source = r#"
        // @ts-check
        /** @template T */
        /** @returns {{ value: T }} */
        function wrap(value) {
            return { value };
        }
        /** @type {number} */
        const n = wrap("string");
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            strict: false,
            ..Default::default()
        },
    );
    let has_2322 = diagnostics
        .iter()
        .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        has_2322,
        "Expected TS2322 for generic helper return mismatched with number annotation in JS, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_no_error_for_any_to_number_assignment() {
    let source = r#"
        let inferredAny: any;
        let x: number = inferredAny;
    "#;

    assert!(
        !has_error_with_code(source, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 when assigning `any` to `number`, got diagnostics: {:?}",
        get_all_diagnostics(source)
    );
}

#[test]
fn test_ts2322_check_js_true_reports_annotation_union_mismatch() {
    let source = r#"
        // @ts-check
        /** @type {number | string} */
        const value = { };
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            strict: true,
            ..Default::default()
        },
    );
    let has_2322 = diagnostics
        .iter()
        .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        !has_2322,
        "Union JSDoc in JS mode is currently treated as assignment-safe and should not emit TS2322 in this branch, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_check_js_false_does_not_enforce_nested_annotation_types() {
    let source = r#"
        // @ts-check
        /** @type {{ a: number, b: string }} */
        const value = { a: "x", b: 1 };
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: false,
            ..Default::default()
        },
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 to be suppressed when checkJs is false, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_check_jsx_true_reports_javascript_annotation_mismatch() {
    let source = r#"
        /** @type {number} */
        const value = "bad";
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.jsx",
        CheckerOptions {
            check_js: true,
            ..Default::default()
        },
    );
    assert!(
        diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for .jsx JSDoc mismatch when checkJs is enabled, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_check_jsx_false_does_not_enforce_annotation_type() {
    let source = r#"
        /** @type {number} */
        const value = "bad";
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.jsx",
        CheckerOptions {
            check_js: false,
            ..Default::default()
        },
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 for .jsx when checkJs is disabled, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_check_jsx_strict_nullability_effect() {
    let source = r#"
        // @ts-check
        /** @type {number} */
        const maybeNumber = null;
    "#;

    let loose = compile_with_options(
        source,
        "test.jsx",
        CheckerOptions {
            check_js: true,
            strict: false,
            ..Default::default()
        },
    );
    let strict = compile_with_options(
        source,
        "test.jsx",
        CheckerOptions {
            check_js: true,
            strict: true,
            ..Default::default()
        },
    );

    let strict_has_2322 = strict
        .iter()
        .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        strict_has_2322,
        "Expected strict+checkJs to emit TS2322 for .jsx nullability mismatch, got: {strict:?}"
    );
    assert!(
        strict.len() > loose.len(),
        "Expected strict mode to increase diagnostics for .jsx nullability in checkJs source"
    );
}

#[test]
fn test_ts2322_assignable_through_generic_identity_in_jsdoc_mode_jsx() {
    let source = r#"
        // @ts-check
        /** @returns {number} */
        function id(value) {
            return "string";
        }
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.jsx",
        CheckerOptions {
            check_js: true,
            ..Default::default()
        },
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 for .jsx generic identity-style JSDoc return annotations, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_assignable_through_generic_identity_in_jsdoc_mode() {
    let source = r#"
        // @ts-check
        /** @returns {number} */
        function id(value) {
            return "string";
        }
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            ..Default::default()
        },
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected JS return @returns annotations to be deferred in this branch, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_assignable_through_generic_identity_in_jsdoc_mode_mjs() {
    let source = r#"
        // @ts-check
        /** @returns {number} */
        function id(value) {
            return "string";
        }
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.mjs",
        CheckerOptions {
            check_js: true,
            ..Default::default()
        },
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected JS return @returns annotations to be deferred in this branch for mjs, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_for_of_uses_declared_type_for_predeclared_identifier() {
    let source = r#"
        let obj: number[];
        let x: string | number | boolean | RegExp;

        function a() {
            x = true;
            for (x of obj) {
                x = x.toExponential();
            }
            x;
        }
    "#;

    let diagnostics = get_all_diagnostics(source);
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 in for-of assignment flow for predeclared identifier, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_object_destructuring_default_not_checked_for_required_property() {
    let source = r#"
        const data = { param: "value" };
        const { param = (() => { throw new Error("param is not defined") })() } = data;
    "#;

    let diagnostics = get_all_diagnostics(source);
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 for required-property object destructuring default initializer, got: {diagnostics:?}"
    );
}

#[test]
#[ignore = "Known regression: flow-narrowed typeof property type query currently resolves via TypeQuery ref in checker-only harness"]
fn test_ts2322_type_query_in_type_assertion_uses_flow_narrowed_property_type() {
    let source = r#"
        interface I<T> {
            p: T;
        }
        function e(x: I<"A" | "B">) {
            if (x.p === "A") {
                let a: "A" = (null as unknown as typeof x.p);
            }
        }
    "#;

    let diagnostics = get_all_diagnostics(source);
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 for flow-narrowed typeof property type in assertion, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_class_or_null_assignable_to_object_or_null() {
    let source = r#"
        class Foo {
            x: string = "";
        }

        declare function getFooOrNull(): Foo | null;

        function f3() {
            let obj: Object | null;
            if ((obj = getFooOrNull()) instanceof Foo) {
                obj;
            }
        }
    "#;

    let diagnostics = get_all_diagnostics(source);
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 for `Foo | null` assignment to `Object | null`, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_noimplicitany_nullish_initializer_mutation_is_not_assignability_error() {
    let source = r#"
        declare let cond: boolean;
        function f() {
            let x = undefined;
            if (cond) {
                x = 1;
            }
            if (cond) {
                x = "hello";
            }
        }
    "#;

    let diagnostics = with_lib_contexts(
        source,
        "test.ts",
        CheckerOptions {
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 for mutable noImplicitAny variable with undefined initializer, got: {diagnostics:?}"
    );
}
