//! Tests for TS2322 assignability errors
//!
//! These tests verify that TS2322 "Type 'X' is not assignable to type 'Y'" errors
//! are properly emitted in various contexts.

use std::path::Path;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::state::CheckerState;
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
        let lib_contexts: Vec<tsz_checker::context::LibContext> = lib_files
            .iter()
            .map(|lib| tsz_checker::context::LibContext {
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

fn diagnostics_for_source(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    let file_name = "test.ts".to_string();
    let mut parser = ParserState::new(file_name.clone(), source.to_string());
    let root = parser.parse_source_file();
    let lib_files = load_lib_files_for_test();
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
        file_name,
        CheckerOptions::default(),
    );
    if !lib_files.is_empty() {
        let lib_contexts: Vec<tsz_checker::context::LibContext> = lib_files
            .iter()
            .map(|lib| tsz_checker::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
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
fn test_promise_is_assignable_to_promise_like_with_real_libs() {
    let libs = load_lib_files_for_test();
    if libs.is_empty() {
        return; // lib files not available
    }
    let source = r#"
declare const p: Promise<number>;
const q: PromiseLike<number> = p;
"#;

    let diagnostics = diagnostics_for_source(source);
    let relevant: Vec<_> = diagnostics.iter().filter(|d| d.code != 2318).collect();

    assert!(
        relevant.is_empty(),
        "Expected Promise<T> to be assignable to PromiseLike<T>, got: {relevant:?}"
    );
}

#[test]
fn test_ts2322_return_alias_instantiation_mismatch() {
    let source = r#"
        type Box<T> = { value: T };

        function returnBox(): Box<number> {
            const box: Box<string> = { value: "x" };
            return box;
        }
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn mapped_type_inference_from_apparent_type_reports_ts2322() {
    let source = r#"
type Obj = {
    [s: string]: number;
};

type foo = <T>(target: { [K in keyof T]: T[K] }) => void;
type bar = <U extends string[]>(source: { [K in keyof U]: Obj[K] }) => void;

declare let f: foo;
declare let b: bar;
b = f;
"#;

    assert!(
        has_error_with_code(source, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "generic mapped assignment should preserve the apparent array constraint and report TS2322"
    );
}

#[test]
fn generic_function_identifier_argument_still_contextually_instantiates() {
    let source = r#"
declare function takesString(fn: (x: string) => string): void;
declare function id<T>(x: T): T;
takesString(id);
"#;

    let diagnostics = get_all_diagnostics(source);
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();

    assert!(
        !relevant.iter().any(|(code, _)| {
            *code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        }),
        "generic function identifiers should still use call-argument contextual instantiation, got: {relevant:?}"
    );
}

#[test]
fn test_ts2322_generator_yield_missing_value() {
    let source = r"
        interface IterableIterator<T> {}

        function* g(): IterableIterator<number> {
            yield;
            yield 1;
        }
    ";

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
    let source = r"
        let z: string[] = [1, 2, 3];
    ";

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn mapped_numeric_handler_context_does_not_falsely_drop_to_implicit_any() {
    let source = r#"
type TypesMap = {
    [0]: { foo: 'bar' };
    [1]: { a: 'b' };
};

type P<T extends keyof TypesMap> = { t: T } & TypesMap[T];

type TypeHandlers = {
    [T in keyof TypesMap]?: (p: P<T>) => void;
};

const typeHandlers: TypeHandlers = {
    [0]: (p) => p.foo,
    [1]: (p) => p.a,
};
"#;

    let diagnostics = compile_with_options(
        source,
        "test.ts",
        CheckerOptions {
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect();

    assert!(
        !relevant
            .iter()
            .any(|(code, _)| { *code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE }),
        "mapped handler context should not be misclassified as a primitive-union overload case, got: {relevant:?}"
    );
}

#[test]
fn mapped_type_generic_indexed_access_no_ts2349() {
    // Repro from TypeScript#49338: element access with a generic key on a mapped
    // type should produce a callable result via solver template substitution,
    // not TS2349 "This expression is not callable".
    let source = r#"
type TypesMap = {
    [0]: { foo: 'bar' };
    [1]: { a: 'b' };
};

type P<T extends keyof TypesMap> = { t: T } & TypesMap[T];

type TypeHandlers = {
    [T in keyof TypesMap]?: (p: P<T>) => void;
};

declare const typeHandlers: TypeHandlers;
const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) =>
    typeHandlers[p.t]?.(p);
"#;

    let diagnostics = compile_with_options(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2349),
        "generic indexed access into mapped type should be callable, got: {diagnostics:?}"
    );
}

#[test]
fn mapped_type_generic_indexed_access_class_member() {
    // Repro from TypeScript#49242: accessing a mapped type class member
    // with a generic key derived from the same keyof should work.
    let source = r#"
type Types = {
    first: { a1: true };
    second: { a2: true };
    third: { a3: true };
};

class Test {
    entries: { [T in keyof Types]?: Types[T][] };
    constructor() { this.entries = {}; }
    addEntry<T extends keyof Types>(name: T, entry: Types[T]) {
        if (!this.entries[name]) {
            this.entries[name] = [];
        }
        this.entries[name]?.push(entry);
    }
}
"#;

    let diagnostics = compile_with_options(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );

    // Should not emit TS2349 (not callable) for .push() call
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2349),
        "push on mapped type with generic index should be callable, got: {diagnostics:?}"
    );
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
    assert!(count >= 4, "Expected at least 4 TS2322 errors, got {count}");
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

#[test]
fn test_ts2322_generic_object_literal_call_property_anchor_and_message() {
    let source = r#"
function foo<T>(x: { bar: T; baz: T }) {
    return x;
}
var r = foo<number>({ bar: 1, baz: '' });
"#;

    let diagnostics = diagnostics_for_source(source);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    let has_ts2345 = diagnostics.iter().any(|d| {
        d.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
    });

    assert_eq!(
        errors.len(),
        1,
        "Expected exactly one TS2322 diagnostic, got: {errors:?}"
    );
    let diag = errors[0];
    let expected_messages = [
        "Type 'string' is not assignable to type 'number'.",
        "Type 'number' is not assignable to type 'string'.",
    ];
    assert!(
        expected_messages.contains(&diag.message_text.as_str()),
        "Unexpected TS2322 message: {}",
        diag.message_text
    );
    assert!(
        !has_ts2345,
        "Did not expect outer TS2345 once property-level TS2322 elaboration applies, got: {diagnostics:?}"
    );

    let expected_baz_start = source
        .find("baz: ''")
        .expect("expected test snippet to contain baz property");
    let expected_bar_start = source
        .find("bar: 1")
        .expect("expected test snippet to contain bar property");
    let expected_object_start = source
        .find("{ bar: 1, baz: '' }")
        .expect("expected test snippet to contain object literal");
    assert!(
        diag.start == expected_baz_start as u32
            || diag.start == expected_bar_start as u32
            || diag.start == expected_object_start as u32,
        "Expected TS2322 on baz/bar/object literal node, got start {}",
        diag.start
    );
}

// =============================================================================
// User-Defined Generic Type Application Tests (TS2322 False Positives)
// These test the root cause of 11,000+ extra TS2322 errors
// =============================================================================

#[test]
fn test_ts2322_no_false_positive_simple_generic_identity() {
    // type Id<T> = T; let a: Id<number> = 42;
    let source = r"
        type Id<T> = T;
        let a: Id<number> = 42;
    ";

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = errors
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for Id<number> = 42, got: {ts2322_errors:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_generic_object_wrapper() {
    // type Box<T> = { value: T }; let b: Box<number> = { value: 42 };
    let source = r"
        type Box<T> = { value: T };
        let b: Box<number> = { value: 42 };
    ";

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = errors
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for Box<number> = {{ value: 42 }}, got: {ts2322_errors:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_conditional_type_true_branch() {
    // IsStr<string> should evaluate to 'true', and true is assignable to true
    let source = r"
        type IsStr<T> = T extends string ? true : false;
        let a: IsStr<string> = true;
    ";

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = errors
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for IsStr<string> = true, got: {ts2322_errors:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_conditional_type_false_branch() {
    // IsStr<number> should evaluate to 'false', and false is assignable to false
    let source = r"
        type IsStr<T> = T extends string ? true : false;
        let b: IsStr<number> = false;
    ";

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = errors
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for IsStr<number> = false, got: {ts2322_errors:?}"
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
        "Expected no TS2322 for MyPartial<Cfg>, got: {ts2322_errors:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_conditional_infer() {
    // UnpackPromise<Promise<number>> should evaluate to number
    let source = r"
        type UnpackPromise<T> = T extends Promise<infer U> ? U : T;
        let a: UnpackPromise<Promise<number>> = 42;
    ";

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = errors
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for UnpackPromise<Promise<number>> = 42, got: {ts2322_errors:?}"
    );
}

#[test]
fn test_ts2322_conditional_doesnt_leak_uninstantiated_type_parameter() {
    // SyntheticDestination<number, Synthetic<number, number>> should resolve to number, not T
    let source = r#"
        interface Synthetic<A, B extends A> {}
        type SyntheticDestination<T, U> = U extends Synthetic<T, infer V> ? V : never;
        type TestSynthetic = SyntheticDestination<number, Synthetic<number, number>>;
        const y: TestSynthetic = 3;
        const z: TestSynthetic = '3';
    "#;

    let errors = get_all_diagnostics(source);
    // Debug: All diagnostics: {errors:?}
    let _ = &errors;

    // y = 3 should NOT error (number is assignable to number)
    // z = '3' SHOULD error (string is not assignable to number)
    let ts2322_errors: Vec<_> = errors
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert_eq!(
        ts2322_errors.len(),
        1,
        "Expected exactly 1 TS2322 for string->number mismatch, got: {ts2322_errors:?}"
    );
    assert!(
        ts2322_errors[0].1.contains("not assignable"),
        "Expected assignability error, got: {:?}",
        ts2322_errors[0].1
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
        "Expected no TS2322 for conditional expression in generic function call, got: {ts2322_errors:?}"
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
        "Expected no TS2322 for nested conditional expression, got: {ts2322_errors:?}"
    );
}

#[test]
fn test_ts2322_generic_indexed_write_preserves_type_parameter_display() {
    let source = r#"
        type Item = { a: string; b: number };

        function setValue<T extends Item, K extends keyof T>(obj: T, key: K) {
            obj[key] = 123;
        }
    "#;

    let ts2322_errors: Vec<_> = get_all_diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        ts2322_errors
            .iter()
            .any(|(_, message)| message.contains("Type 'number' is not assignable to type 'T[K]'")),
        "Expected generic indexed-write TS2322 to preserve T[K] display, got: {ts2322_errors:?}"
    );
}

#[test]
fn test_ts2322_accessor_incompatible_getter_setter() {
    // TS 5.1+: when BOTH getter and setter have explicit type annotations,
    // unrelated types are allowed (no error).
    let source_both_explicit = r#"
        class C {
            get x(): string { return "s"; }
            set x(value: number) {}
        }
    "#;

    let diagnostics = get_all_diagnostics(source_both_explicit);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322.is_empty(),
        "TS 5.1+ allows unrelated types when both annotated; got: {ts2322:?}"
    );

    // But when getter has NO explicit return annotation (inferred type),
    // the inferred type must be compatible with the setter's explicit param type.
    let source_inferred_getter = r#"
        class C {
            get bar() { return 0; }
            set bar(n: string) {}
        }
    "#;

    let diagnostics = get_all_diagnostics(source_inferred_getter);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        !ts2322.is_empty(),
        "Inferred getter type (number) conflicts with explicit setter type (string) → TS2322"
    );
}

#[test]
fn test_ts2322_accessor_compatible_divergent_types() {
    // When getter return IS assignable to setter param, no error.
    let source = r#"
        class C {
            get x(): string { return "hello"; }
            set x(value: string | number) {}
        }
    "#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        ts2322.is_empty(),
        "Getter return type (string) is assignable to setter param (string|number), no TS2322; got: {ts2322:?}"
    );
}

#[test]
fn test_ts2322_annotated_getter_contextually_types_unannotated_setter_parameter() {
    let source = r#"
        class C {
            get x(): string { return ""; }
            set x(value) { value = 0; }
        }
    "#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    let ts7006: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE)
        .collect();

    assert_eq!(
        ts2322.len(),
        1,
        "expected setter body assignment to be checked against getter type: {diagnostics:?}"
    );
    assert!(
        ts7006.is_empty(),
        "paired getter should contextually type the setter parameter: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_for_of_annotation_mismatch() {
    let source = r"
        for (const x: string of [1, 2, 3]) {}
    ";

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
    // No @ts-check: JSDoc types should NOT be enforced when checkJs is false.
    let source = r#"
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
fn test_conflicting_private_intersection_reduces_before_missing_property_classification() {
    let diags = with_lib_contexts(
        r#"
class A { private x: unknown; y?: string; }
class B { private x: unknown; y?: string; }

declare let ab: A & B;
ab.y = 'hello';
ab = {};
"#,
        "test.ts",
        CheckerOptions {
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diags
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for impossible private-brand intersection assignment, got: {diags:?}"
    );
    assert!(
        diags
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Expected TS2339 on property access through never, got: {diags:?}"
    );
    assert!(
        !diags
            .iter()
            .any(|(code, _)| *code
                == diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE),
        "Intersection should reduce before TS2741 missing-property classification, got: {diags:?}"
    );
}

#[test]
fn test_ts2322_check_mjs_false_does_not_enforce_annotation_type() {
    // No @ts-check: JSDoc types should NOT be enforced when checkJs is false.
    let source = r#"
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
    // No @ts-check: JSDoc @returns should NOT be enforced when checkJs is false.
    let source = r#"
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
    let source = r"
        // @ts-check
        /** @type {number} */
        const maybeNumber = null;
    ";

    let loose = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            strict: false,
            strict_null_checks: false,
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
fn test_call_object_literal_optional_param_prefers_property_ts2322_over_ts2345() {
    let source = r#"
function foo({ x, y, z }?: { x: string; y: number; z: boolean }) {}
foo({ x: false, y: 0, z: "" });
"#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322_count = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .count();
    let has_ts2345 = diagnostics.iter().any(|(code, _)| {
        *code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
    });

    assert!(
        ts2322_count >= 2,
        "Expected property-level TS2322 for the mismatched object-literal fields, got: {diagnostics:?}"
    );
    assert!(
        !has_ts2345,
        "Did not expect outer TS2345 once property-level elaboration applies, got: {diagnostics:?}"
    );
}

#[test]
fn test_generic_callback_return_mismatch_reports_ts2345_for_identifier_expression_body() {
    // For expression-bodied arrow functions with identifier bodies (like `undefined`),
    // we report TS2345 on the outer argument rather than TS2322 on the body.
    // This avoids false elaboration when the identifier references a complex type.
    let source = r#"
function someGenerics3<T>(producer: () => T) { }
someGenerics3<number>(() => undefined);
"#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2322 = diagnostics
        .iter()
        .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    let has_ts2345 = diagnostics.iter().any(|(code, _)| {
        *code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
    });

    assert!(
        has_ts2345,
        "Expected TS2345 on the outer argument for identifier body, got: {diagnostics:?}"
    );
    assert!(
        !has_ts2322,
        "Did not expect inner TS2322 for identifier expression body, got: {diagnostics:?}"
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
fn test_ts2322_arrow_expression_body_jsdoc_cast_reports_template_return_mismatch() {
    let source = r#"
        /** @template T
         * @param {T|undefined} value value or not
         * @returns {T} result value
         */
        const foo1 = value => /** @type {string} */({ ...value });

        /** @template T
         * @param {T|undefined} value value or not
         * @returns {T} result value
         */
        const foo2 = value => /** @type {string} */(/** @type {T} */({ ...value }));
    "#;

    let diagnostics = compile_with_options(
        source,
        "mytest.js",
        CheckerOptions {
            check_js: true,
            strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    let has_2322 = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .count();

    assert_eq!(
        has_2322, 2,
        "Expected two TS2322 errors from both inline cast arrow bodies, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_namespace_export_assignment_optional_to_required() {
    let source = r#"
        // @target: es2015
        namespace __test1__ {
            export interface interfaceWithPublicAndOptional<T,U> { one: T; two?: U; };  var obj4: interfaceWithPublicAndOptional<number,string> = { one: 1 };;
            export var __val__obj4 = obj4;
        }
        namespace __test2__ {
            export var obj = {two: 1};
            export var __val__obj = obj;
        }
        __test2__.__val__obj = __test1__.__val__obj4
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    let has_2322 = diagnostics
        .iter()
        .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        has_2322,
        "Expected TS2322 for assigning optional property type to required property target, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_optional_property_required_includes_related_missing_property_detail() {
    let source = r#"
        let source: { one?: number } = {};
        let target: { one: number } = source;
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2322 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .expect("expected TS2322 for optional-to-required property assignment");

    assert!(
        ts2322.related_information.iter().any(|info| {
            info.code == diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
                && info
                    .message_text
                    .contains("Property 'one' is missing in type")
        }),
        "Expected TS2322 to include missing-property elaboration as related information, got: {ts2322:?}"
    );
}

#[test]
fn test_ts2322_property_type_mismatch_includes_related_property_detail() {
    let source = r#"
        let source: { one: string } = { one: "" };
        let target: { one: number } = source;
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2322 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .expect("expected TS2322 for property type mismatch assignment");

    assert!(
        ts2322.related_information.iter().any(|info| {
            info.message_text
                .contains("Types of property 'one' are incompatible.")
        }),
        "Expected TS2322 to include property incompatibility elaboration, got: {ts2322:?}"
    );
}

#[test]
fn test_ts2345_property_type_mismatch_includes_related_property_detail() {
    let source = r#"
        declare function takes(value: { one: number }): void;
        const arg: { one: string } = { one: "" };
        takes(arg);
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2345 = diagnostics
        .iter()
        .find(|diag| {
            diag.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .expect("expected TS2345 for argument property type mismatch");

    assert!(
        ts2345.related_information.iter().any(|info| {
            info.code == diagnostic_codes::TYPES_OF_PROPERTY_ARE_INCOMPATIBLE
                && info
                    .message_text
                    .contains("Types of property 'one' are incompatible.")
        }),
        "Expected TS2345 to include property incompatibility elaboration, got: {ts2345:?}"
    );
    assert!(
        ts2345.related_information.iter().any(|info| {
            info.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && info
                    .message_text
                    .contains("Type 'string' is not assignable to type 'number'.")
        }),
        "Expected TS2345 to include nested type mismatch elaboration, got: {ts2345:?}"
    );
}

#[test]
fn test_ts2345_missing_many_properties_formats_related_detail_once() {
    let source = r#"
        declare function takes(value: { a: number; b: number; c: number; d: number; e: number }): void;
        const arg = {};
        takes(arg);
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2345 = diagnostics
        .iter()
        .find(|diag| {
            diag.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .expect("expected TS2345 for missing-properties argument mismatch");

    let related = ts2345
        .related_information
        .iter()
        .find(|info| {
            info.code
                == diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
        })
        .expect("expected TS2740 related detail under TS2345");

    assert!(
        related.message_text.contains("a, b, c, d, and 1 more."),
        "Expected TS2345 related detail to format the extra-property suffix once, got: {related:?}"
    );
    assert!(
        !related.message_text.contains("and 1 more., and 1 more."),
        "Expected TS2345 related detail to avoid duplicating the extra-property suffix, got: {related:?}"
    );
}

#[test]
fn test_ts2345_optional_property_required_includes_related_missing_property_detail() {
    let source = r#"
        declare function takes(value: { one: number }): void;
        const arg: { one?: number } = {};
        takes(arg);
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2345 = diagnostics
        .iter()
        .find(|diag| {
            diag.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .expect("expected TS2345 for optional-to-required argument mismatch");

    assert!(
        ts2345.related_information.iter().any(|info| {
            info.code == diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
                && info
                    .message_text
                    .contains("Property 'one' is missing in type")
        }),
        "Expected TS2345 to include missing-property elaboration for optional-to-required mismatch, got: {ts2345:?}"
    );
}

#[test]
fn test_ts2345_function_return_mismatch_includes_related_return_detail() {
    let source = r#"
        declare function takes(cb: () => number): void;
        const cb: () => string = () => "";
        takes(cb);
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2345 = diagnostics
        .iter()
        .find(|diag| {
            diag.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .expect("expected TS2345 for function return type mismatch");

    assert!(
        ts2345.related_information.iter().any(|info| {
            info.message_text
                .contains("Return type 'string' is not assignable to 'number'.")
        }),
        "Expected TS2345 to include return-type elaboration, got: {ts2345:?}"
    );
    assert!(
        ts2345.related_information.iter().any(|info| {
            info.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && info
                    .message_text
                    .contains("Type 'string' is not assignable to type 'number'.")
        }),
        "Expected TS2345 to include nested type mismatch under the return-type detail, got: {ts2345:?}"
    );
}

#[test]
fn test_ts2345_index_signature_mismatch_includes_related_detail() {
    let source = r#"
        declare function takes(value: { [key: string]: number }): void;
        const arg: { [key: string]: string } = { a: "" };
        takes(arg);
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2345 = diagnostics
        .iter()
        .find(|diag| {
            diag.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .expect("expected TS2345 for index-signature mismatch");

    assert!(
        ts2345.related_information.iter().any(|info| {
            info.message_text.contains(
                "string index signature is incompatible: 'string' is not assignable to 'number'.",
            )
        }),
        "Expected TS2345 to include index-signature elaboration, got: {ts2345:?}"
    );
    assert!(
        ts2345.related_information.iter().any(|info| {
            info.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && info
                    .message_text
                    .contains("Type 'string' is not assignable to type 'number'.")
        }),
        "Expected TS2345 to include nested type mismatch under index-signature elaboration, got: {ts2345:?}"
    );
}

#[test]
fn test_ts2345_missing_index_signature_includes_related_detail() {
    let source = r#"
        declare function takes(value: { [index: number]: number }): void;
        interface Arg { one: number; two?: string; }
        const arg: Arg = { one: 1 };
        takes(arg);
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2345 = diagnostics
        .iter()
        .find(|diag| {
            diag.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .expect("expected TS2345 for missing-index-signature mismatch");

    assert!(
        ts2345.related_information.iter().any(|info| {
            info.code == diagnostic_codes::INDEX_SIGNATURE_FOR_TYPE_IS_MISSING_IN_TYPE
                && info
                    .message_text
                    .contains("Index signature for type 'number' is missing in type 'Arg'.")
        }),
        "Expected TS2345 to include missing-index-signature elaboration, got: {ts2345:?}"
    );
}

#[test]
fn test_ts2345_array_element_mismatch_includes_related_detail() {
    let source = r#"
        declare function takes(value: number[]): void;
        const arg: string[] = [""];
        takes(arg);
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2345 = diagnostics
        .iter()
        .find(|diag| {
            diag.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .expect("expected TS2345 for array-element mismatch");

    assert!(
        ts2345.related_information.iter().any(|info| {
            info.message_text
                .contains("Array element type 'string' is not assignable to 'number'.")
        }),
        "Expected TS2345 to include array-element elaboration, got: {ts2345:?}"
    );
    assert!(
        ts2345.related_information.iter().any(|info| {
            info.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && info
                    .message_text
                    .contains("Type 'string' is not assignable to type 'number'.")
        }),
        "Expected TS2345 to include nested type mismatch under array-element elaboration, got: {ts2345:?}"
    );
}

#[test]
fn test_ts2322_no_error_for_any_to_number_assignment() {
    let source = r"
        let inferredAny: any;
        let x: number = inferredAny;
    ";

    assert!(
        !has_error_with_code(source, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 when assigning `any` to `number`, got diagnostics: {:?}",
        get_all_diagnostics(source)
    );
}

#[test]
fn test_ts2322_check_js_true_reports_annotation_union_mismatch() {
    let source = r"
        // @ts-check
        /** @type {number | string} */
        const value = { };
    ";

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
        has_2322,
        "Expected TS2322 when assigning `{{}}` to `number | string` in JS mode, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_check_js_false_does_not_enforce_nested_annotation_types() {
    // No @ts-check: nested JSDoc @type should NOT be enforced when checkJs is false.
    let source = r#"
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
    let source = r"
        // @ts-check
        /** @type {number} */
        const maybeNumber = null;
    ";

    let loose = compile_with_options(
        source,
        "test.jsx",
        CheckerOptions {
            check_js: true,
            strict: false,
            strict_null_checks: false,
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
    let source = r"
        let obj: number[];
        let x: string | number | boolean | RegExp;

        function a() {
            x = true;
            for (x of obj) {
                x = x.toExponential();
            }
            x;
        }
    ";

    let diagnostics = get_all_diagnostics(source);
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 in for-of assignment flow for predeclared identifier, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_for_of_array_destructuring_assignment_no_false_positive() {
    // for ([k, v] of map) should not produce TS2322 when types match.
    // The iteration element type flows through the destructuring pattern
    // element-by-element, not as a whole-type assignability check.
    let source = r"
        var k: string, v: number;
        var arr: [string, number][] = [['a', 1]];
        for ([k, v] of arr) {
            k;
            v;
        }
    ";

    let diagnostics = get_all_diagnostics(source);
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 for array destructuring in for-of with matching types, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_for_of_array_destructuring_wrong_default_still_errors() {
    // for ([k = false] of arr) where k is string should still produce TS2322
    // because the default value `false` is not assignable to `string`.
    let source = r"
        var k: string;
        var arr: [string][] = [['a']];
        for ([k = false] of arr) {
            k;
        }
    ";

    let diagnostics = get_all_diagnostics(source);
    assert!(
        diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for wrong default value type in array destructuring for-of"
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
fn test_ts2322_assignment_destructuring_defaults_report_undefined_mismatches() {
    let source = r#"
        const a: { x?: number; y?: number } = {};
        let x: number;

        ({ x = undefined } = a);
        ({ x: x = undefined } = a);
        ({ y: x = undefined } = a);
    "#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, message)| message.as_str())
        .collect();

    assert_eq!(
        ts2322_messages.len(),
        3,
        "Expected TS2322 for each undefined default in assignment destructuring, got: {diagnostics:?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .all(|message| message.contains("Type 'undefined' is not assignable to type 'number'.")),
        "Expected all assignment destructuring default mismatches to preserve 'undefined' source display, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_nested_assignment_destructuring_default_is_not_whole_pattern_checked() {
    let source = r#"
        let a: 0 | 1 = 0;
        let b: 0 | 1 | 9;
        [{ [(a = 1)]: b } = [9, a] as const] = [];
        const bb: 0 = b;
    "#;

    let diagnostics = get_all_diagnostics(source);
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no whole-pattern TS2322 for nested assignment destructuring default, got: {diagnostics:?}"
    );
}

#[test]
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

// ── Mapped type key constraint inside conditional types (inferTypes1 parity) ──

#[test]
fn test_ts2322_mapped_type_key_in_conditional_unconstrained_t() {
    // `string extends T ? { [P in T]: void } : T` — T is NOT narrowed in the
    // true branch (check type is `string`, not a type parameter), so T is still
    // unconstrained and `[P in T]` is invalid. tsc emits TS2322 here.
    let source = r"
        type B<T> = string extends T ? { [P in T]: void; } : T;
    ";
    assert!(
        has_error_with_code(source, 2322),
        "Expected TS2322 for unconstrained T in mapped type key inside conditional (string extends T)"
    );
}

#[test]
fn test_ts2322_no_false_positive_mapped_type_key_narrowed_by_conditional() {
    // `T extends string ? { [P in T]: void } : T` — T IS narrowed to `T & string`
    // in the true branch, so `[P in T]` is valid (T is string-like). No TS2322.
    let source = r"
        type A<T> = T extends string ? { [P in T]: void; } : T;
    ";
    let errors = get_all_diagnostics(source);
    assert!(
        !errors.iter().any(|(code, _)| *code == 2322),
        "Expected no TS2322 for narrowed T in mapped type key (T extends string). Got: {errors:?}"
    );
}

#[test]
fn test_ts2322_conditional_extends_distinguishes_optional_and_optional_undefined() {
    let source = r#"
        export let a: <T>() => T extends {a?: string} ? 0 : 1 = null!;
        export let b: <T>() => T extends {a?: string | undefined} ? 0 : 1 = a;
    "#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322: Vec<&(u32, String)> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322.len(),
        1,
        "Expected one TS2322 for conditional extends optional-property identity. Actual diagnostics: {diagnostics:?}"
    );
    assert!(
        ts2322[0]
            .1
            .contains("Type '<T>() => T extends { a?: string; } ? 0 : 1' is not assignable to type '<T>() => T extends { a?: string | undefined; } ? 0 : 1'"),
        "Expected TS2322 to preserve the differing optional-property conditional signatures. Actual diagnostics: {diagnostics:?}"
    );
}
