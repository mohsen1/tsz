//! Tests for TS2322 assignability errors
//!
//! These tests verify that TS2322 "Type 'X' is not assignable to type 'Y'" errors
//! are properly emitted in various contexts.

use rustc_hash::FxHashSet;
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
    let lib_roots = [
        manifest_dir.join("../../crates/tsz-core/src/lib-assets"),
        manifest_dir.join("../../crates/tsz-core/src/lib-assets-stripped"),
        manifest_dir.join("../../TypeScript/src/lib"),
    ];
    let lib_names = [
        "es5.d.ts",
        "es2015.d.ts",
        "es2015.core.d.ts",
        "es2015.collection.d.ts",
        "es2015.iterable.d.ts",
        "es2015.generator.d.ts",
        "es2015.promise.d.ts",
        "es2015.proxy.d.ts",
        "es2015.reflect.d.ts",
        "es2015.symbol.d.ts",
        "es2015.symbol.wellknown.d.ts",
        "dom.d.ts",
        "dom.generated.d.ts",
        "dom.iterable.d.ts",
        "esnext.d.ts",
    ];

    let mut lib_files = Vec::new();
    let mut seen_files = FxHashSet::default();
    for file_name in lib_names {
        for root in &lib_roots {
            let lib_path = root.join(file_name);
            if lib_path.exists()
                && let Ok(content) = std::fs::read_to_string(&lib_path)
            {
                if !seen_files.insert(file_name.to_string()) {
                    break;
                }
                let lib_file = LibFile::from_source(file_name.to_string(), content);
                lib_files.push(Arc::new(lib_file));
                break;
            }
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

#[test]
fn test_ts2322_identifier_literal_initializer_display_for_literal_sensitive_targets() {
    let diagnostics = get_all_diagnostics(
        r#"
var x = true;
var n: number = x;
var u: typeof undefined = x;
enum E { A }
var e: E = x;
var s = "value";
var su: typeof undefined = s;
var i = 1;
var iu: typeof undefined = i;
"#,
    );
    let ts2322 = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, message)| message.as_str())
        .collect::<Vec<_>>();

    assert!(
        ts2322
            .iter()
            .any(|message| message.contains("Type 'boolean' is not assignable to type 'number'.")),
        "expected widened boolean display for non-literal target, got: {ts2322:#?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|message| message.contains("Type 'true' is not assignable to type 'undefined'.")),
        "expected literal initializer display for undefined target, got: {ts2322:#?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|message| message.contains("Type 'true' is not assignable to type 'E'.")),
        "expected literal initializer display for enum target, got: {ts2322:#?}"
    );
    assert!(
        ts2322.iter().any(
            |message| message.contains("Type 'string' is not assignable to type 'undefined'.")
        ),
        "expected string initializer display to remain widened, got: {ts2322:#?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|message| message.contains("Type 'number' is not assignable to type 'undefined'.")),
        "expected numeric initializer display to remain widened, got: {ts2322:#?}"
    );
}

#[test]
fn test_ts2322_type_parameter_union_display_preserves_declaration_order() {
    let diagnostics = get_all_diagnostics(
        r#"
function diamondTop<Top>() {
    function diamondMiddle<T, U>() {
        let top!: Top;
        let middle!: Top | T | U;
        top = middle;
    }
}
"#,
    );

    let message = diagnostics
        .iter()
        .find_map(|(code, message)| {
            (*code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && message.contains("is not assignable to type 'Top'."))
            .then_some(message.as_str())
        })
        .expect("expected TS2322 diagnostic for top = middle assignment");

    assert!(
        message.contains("Type 'Top | T | U' is not assignable to type 'Top'."),
        "expected declaration-order union display, got: {message}"
    );
}

#[test]
fn test_ts2322_narrowed_string_literal_residual_union_to_never_display() {
    let diagnostics = get_all_diagnostics(
        r#"
type Variants = "a" | "b" | "c" | "d";

function fx1(x: Variants) {
    if (x === "a" || x === "b") {
    } else {
        const y: never = x;
    }
}
"#,
    );

    let message = diagnostics
        .iter()
        .find_map(|(code, message)| {
            (*code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && message.contains("is not assignable to type 'never'."))
            .then_some(message.as_str())
        })
        .expect("expected TS2322 diagnostic for narrowed residual union assigned to never");

    assert!(
        message.contains(r#"Type '"d" | "c"' is not assignable to type 'never'."#),
        "expected residual string-literal union display to match tsc, got: {message}"
    );
}

fn compile_with_options(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    with_lib_contexts(source, file_name, options)
}

fn compile_with_libs_for_ts(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
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
fn generic_signature_assignment_reports_expected_ts2322s() {
    let source = r#"
type A3 = <T>(x: T) => void;
type B3 = <T>(x: T) => T;
declare let a3: A3;
declare let b3: B3;
a3 = b3;
b3 = a3;

type A11 = <T>(x: { foo: T }, y: { foo: T; bar: T }) => void;
type B11 = <T, U>(x: { foo: T }, y: { foo: U; bar: U }) => void;
declare let a11: A11;
declare let b11: B11;
a11 = b11;
b11 = a11;

type Base = { foo: string };
type A16 = <T extends Base>(x: { a: T; b: T }) => T[];
type B16 = <T>(x: { a: T; b: T }) => T[];
declare let a16: A16;
declare let b16: B16;
a16 = b16;
b16 = a16;
"#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322_errors.len(),
        3,
        "Expected the three invalid reverse generic signature assignments to report TS2322, got: {ts2322_errors:?}"
    );
    assert!(
        ts2322_errors
            .iter()
            .any(|(_, message)| message.contains("Type 'A3' is not assignable to type 'B3'")),
        "Expected the void-return reverse assignment to surface as the A3/B3 TS2322, got: {ts2322_errors:?}"
    );
    assert!(
        ts2322_errors
            .iter()
            .any(|(_, message)| message.contains("Type 'A11' is not assignable to type 'B11'")),
        "Expected the mismatched correlated generic assignment to surface as the A11/B11 TS2322, got: {ts2322_errors:?}"
    );
    assert!(
        ts2322_errors
            .iter()
            .any(|(_, message)| message.contains("Type 'A16' is not assignable to type 'B16'")),
        "Expected the constrained generic reverse assignment to surface as the A16/B16 TS2322, got: {ts2322_errors:?}"
    );
}

#[test]
fn generic_construct_signature_assignment_reports_expected_ts2322s() {
    let source = r#"
type Base = { foo: string };

type A3 = new <T>(x: T) => void;
type B3 = new <T>(x: T) => T;
declare let a3: A3;
declare let b3: B3;
a3 = b3;
b3 = a3;

type A11 = new <T>(x: { foo: T }, y: { foo: T; bar: T }) => Base;
type B11 = new <T, U>(x: { foo: T }, y: { foo: U; bar: U }) => Base;
declare let a11: A11;
declare let b11: B11;
a11 = b11;
b11 = a11;

type A16 = new <T extends Base>(x: { a: T; b: T }) => T[];
type B16 = new <U, V>(x: { a: U; b: V }) => U[];
declare let a16: A16;
declare let b16: B16;
a16 = b16;
b16 = a16;
"#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322_errors.len(),
        3,
        "Expected the three invalid reverse generic construct-signature assignments to report TS2322, got: {ts2322_errors:?}"
    );
}

#[test]
fn generic_interface_member_signature_assignments_report_ts2322s() {
    let source = r#"
type Base = { foo: string };

interface A {
    a3: <T>(x: T) => void;
    a11: <T>(x: { foo: T }, y: { foo: T; bar: T }) => Base;
    a16: <T extends Base>(x: { a: T; b: T }) => T[];
}

declare let x: A;

declare let b3: <T>(x: T) => T;
x.a3 = b3;
b3 = x.a3;

declare let b11: <T, U>(x: { foo: T }, y: { foo: U; bar: U }) => Base;
x.a11 = b11;
b11 = x.a11;

declare let b16: <T>(x: { a: T; b: T }) => T[];
x.a16 = b16;
b16 = x.a16;
"#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322_errors.len(),
        3,
        "Expected the three invalid reverse member-signature assignments to report TS2322, got: {ts2322_errors:?}"
    );
}

#[test]
fn generic_interface_member_construct_signature_assignments_report_ts2322s() {
    let source = r#"
type Base = { foo: string };

interface A {
    a3: new <T>(x: T) => void;
    a11: new <T>(x: { foo: T }, y: { foo: T; bar: T }) => Base;
    a16: new <T extends Base>(x: { a: T; b: T }) => T[];
}

declare let x: A;

declare let b3: new <T>(x: T) => T;
x.a3 = b3;
b3 = x.a3;

declare let b11: new <T, U>(x: { foo: T }, y: { foo: U; bar: U }) => Base;
x.a11 = b11;
b11 = x.a11;

declare let b16: new <T>(x: { a: T; b: T }) => T[];
x.a16 = b16;
b16 = x.a16;
"#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322_errors.len(),
        3,
        "Expected the three invalid reverse member construct-signature assignments to report TS2322, got: {ts2322_errors:?}"
    );
}

#[test]
fn mapped_source_generic_call_reports_ts2345() {
    let source = r#"
type A = "number" | "null" | A[];

type F<T> = null extends T
    ? [F<NonNullable<T>>, "null"]
    : T extends number
    ? "number"
    : never;

type G<T> = { [k in keyof T]: F<T[k]> };

interface K {
    b: number | null;
}

const gK: { [key in keyof K]: A } = { b: ["number", "null"] };

function foo<T>(g: G<T>): T {
    return {} as any;
}

foo(gK);
"#;

    assert!(
        has_error_with_code(
            source,
            diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        ),
        "mapped source generic call should preserve concrete keys and report TS2345"
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
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2349),
        "generic indexed access into mapped type should be callable, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2344),
        "generic indexed access into mapped type should preserve the `keyof TypesMap` constraint, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "mapped type object literal handlers should contextually type callback params, got: {diagnostics:?}"
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

#[test]
fn mapped_type_generic_indexed_access_full_file_has_no_ts2344_or_ts7006() {
    let source = r#"
type Types = {
    first: { a1: true };
    second: { a2: true };
    third: { a3: true };
};

class Test {
    entries: { [T in keyof Types]?: Types[T][] };

    constructor() {
        this.entries = {};
    }

    addEntry<T extends keyof Types>(name: T, entry: Types[T]) {
        if (!this.entries[name]) {
            this.entries[name] = [];
        }
        this.entries[name]?.push(entry);
    }
}

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

const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) =>
    typeHandlers[p.t]?.(p);
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2344),
        "full mapped-type generic indexed-access repro should not emit TS2344, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "full mapped-type generic indexed-access repro should not emit TS7006, got: {diagnostics:?}"
    );
}

#[test]
fn mapped_type_recursive_inference_generic_call_preserves_nested_callback_context() {
    let source = r#"
type MorphTuple = [string, "|>", any];

type validateMorph<def extends MorphTuple> = def[1] extends "|>"
    ? [validateDefinition<def[0]>, "|>", (In: def[0]) => unknown]
    : def;

type validateDefinition<def> = def extends MorphTuple
    ? validateMorph<def>
    : {
          [k in keyof def]: validateDefinition<def[k]>
      };

declare function type<def>(def: validateDefinition<def>): def;

const shallow = type(["ark", "|>", (x) => x.length]);
const objectLiteral = type({ a: ["ark", "|>", (x) => x.length] });
const nestedTuple = type([["ark", "|>", (x) => x.length]]);
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "recursive mapped/conditional generic call should contextually type nested callbacks, got: {diagnostics:?}"
    );
}

#[test]
fn union_of_overloaded_array_method_aliases_preserves_callback_context() {
    let source = r#"
interface Fizz { id: number; fizz: string }
interface Buzz { id: number; buzz: string }
interface Arr<T> {
  filter<S extends T>(pred: (value: T) => value is S): S[];
  filter(pred: (value: T) => unknown): T[];
}
declare const m: Arr<Fizz>["filter"] | Arr<Buzz>["filter"];
m(item => item.id < 5);
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "union of overloaded array method aliases should contextually type callback params, got: {diagnostics:?}"
    );
}

#[test]
fn union_of_builtin_array_methods_preserves_callback_context() {
    let source = r#"
interface Fizz { id: number; fizz: string }
interface Buzz { id: number; buzz: string }

([] as Fizz[] | Buzz[]).filter(item => item.id < 5);
([] as Fizz[] | readonly Buzz[]).filter(item => item.id < 5);
([] as Fizz[] | Buzz[]).find(item => item);
([] as Fizz[] | Buzz[]).every(item => item.id < 5);
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "union of built-in array methods should contextually type callback params, got: {diagnostics:?}"
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

#[test]
fn test_ts2322_distinct_type_parameters_are_not_suppressed() {
    let source = r#"
        function unconstrained<T, U>(t: T, u: U) {
            t = u;
            u = t;
        }

        function constrained<T extends { foo: string }, U extends { foo: string }>(t: T, u: U) {
            t = u;
            u = t;
        }

        class Box<T extends { foo: string }, U extends { foo: string }> {
            t!: T;
            u!: U;

            assign() {
                this.t = this.u;
                this.u = this.t;
            }
        }
    "#;

    let count = count_errors_with_code(source, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert_eq!(
        count, 6,
        "Expected TS2322 for each distinct type-parameter assignment, got {count}"
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

#[test]
fn test_ts2322_generic_private_class_assignment_preserves_type_arguments() {
    let source = r#"
class C<T> {
    #foo: T;
    #method(): T { return this.#foo; }
    get #prop(): T { return this.#foo; }
    set #prop(value: T) { this.#foo = value; }

    bar(x: C<T>) { return x.#foo; }
    bar2(x: C<T>) { return x.#method(); }
    bar3(x: C<T>) { return x.#prop; }

    baz(x: C<number>) { return x.#foo; }
    baz2(x: C<number>) { return x.#method; }
    baz3(x: C<number>) { return x.#prop; }

    quux(x: C<string>) { return x.#foo; }
    quux2(x: C<string>) { return x.#method; }
    quux3(x: C<string>) { return x.#prop; }
}

declare let a: C<number>;
declare let b: C<string>;
a.#foo;
a.#method;
a.#prop;
a = b;
b = a;
"#;

    let diagnostics = compile_with_options(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            strict: true,
            strict_property_initialization: false,
            ..CheckerOptions::default()
        },
    );
    let messages: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, message)| message.as_str())
        .collect();

    assert_eq!(
        messages.len(),
        2,
        "expected exactly two TS2322 assignment diagnostics, got: {diagnostics:?}"
    );
    assert!(
        messages
            .iter()
            .all(|message| !message.contains("Type 'C' is not assignable to type 'C'.")),
        "generic class TS2322 should not erase type arguments, got: {diagnostics:?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message
                .contains("Type 'C<string>' is not assignable to type 'C<number>'.")),
        "expected C<string> -> C<number> TS2322 display, got: {diagnostics:?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message
                .contains("Type 'C<number>' is not assignable to type 'C<string>'.")),
        "expected C<number> -> C<string> TS2322 display, got: {diagnostics:?}"
    );
}

#[test]
fn generic_object_assign_initializer_keeps_outer_ts2322() {
    let source = r#"
type Omit<T, K> = Pick<T, Exclude<keyof T, K>>;
type Assign<T, U> = Omit<T, keyof U> & U;

class Base<T> {
    constructor(public t: T) {}
}

export class Foo<T> extends Base<T> {
    update(): Foo<Assign<T, { x: number }>> {
        const v: Assign<T, { x: number }> = Object.assign(this.t, { x: 1 });
        return new Foo(v);
    }
}
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let codes: Vec<_> = diagnostics.iter().map(|(code, _)| *code).collect();
    assert!(
        codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected outer TS2322 for generic Object.assign initializer, got: {diagnostics:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL),
        "Expected initializer TS2769 for generic Object.assign initializer, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_string_intrinsic_targets_widen_literal_sources() {
    let source = r#"
let x: Uppercase<string>;
x = "AbC";

let y: Lowercase<string>;
y = "AbC";
"#;

    let diagnostics = diagnostics_for_source(source);
    let messages: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|d| d.message_text.as_str())
        .collect();

    assert!(
        messages.contains(&"Type 'string' is not assignable to type 'Uppercase<string>'."),
        "Expected widened source diagnostic for Uppercase<string>, got: {messages:?}"
    );
    assert!(
        messages.contains(&"Type 'string' is not assignable to type 'Lowercase<string>'."),
        "Expected widened source diagnostic for Lowercase<string>, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|message| message.contains("\"AbC\"")),
        "String intrinsic diagnostics should widen the source literal, got: {messages:?}"
    );
}

#[test]
fn test_ts2322_string_mapping_alias_displays_resolved_literal_target() {
    let source = r#"
type A = "aA";
type B = Uppercase<A>;
type ATemplate = `aA${string}`;
type BTemplate = Uppercase<ATemplate>;

declare let lit: B;
declare let tpl: BTemplate;

lit = tpl;
"#;

    let diagnostics = diagnostics_for_source(source);
    let message = diagnostics
        .iter()
        .find_map(|d| {
            (d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && d.message_text.contains("Type '`AA${Uppercase<string>}`'"))
            .then_some(d.message_text.as_str())
        })
        .expect("expected TS2322 for assigning uppercase template to uppercase literal");

    assert!(
        message.contains(r#"is not assignable to type '"AA"'."#),
        "expected evaluated uppercase literal target display, got: {message}"
    );
    assert!(
        !message.contains("Uppercase<A>"),
        "did not expect intrinsic alias repaint for literal target, got: {message}"
    );
}

#[test]
fn test_ts2322_string_mapping_alias_displays_resolved_template_target() {
    let source = r#"
type Source = `aA${string}`;
type Target = Uppercase<Source>;

declare let sourceValue: Source;
declare let targetValue: Target;

targetValue = sourceValue;
"#;

    let diagnostics = diagnostics_for_source(source);
    let message = diagnostics
        .iter()
        .find_map(|d| {
            (d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && d.message_text.contains("Type '`aA${string}`'"))
            .then_some(d.message_text.as_str())
        })
        .expect("expected TS2322 for assigning unmapped template to mapped template target");

    assert!(
        message.contains("is not assignable to type '`AA${Uppercase<string>}`'."),
        "expected evaluated uppercase template target display, got: {message}"
    );
    assert!(
        !message.contains("Uppercase<Source>"),
        "did not expect intrinsic alias repaint for template target, got: {message}"
    );
}

#[test]
fn test_ts2322_string_intrinsic_target_does_not_gain_nested_alias_display() {
    let source = r#"
declare let upper: Uppercase<string>;
declare let lowerUpper: Lowercase<Uppercase<string>>;

upper = lowerUpper;
"#;

    let diagnostics = diagnostics_for_source(source);
    let message = diagnostics
        .iter()
        .find_map(|d| {
            (d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && d.message_text.contains("Lowercase<Uppercase<string>>"))
            .then_some(d.message_text.as_str())
        })
        .expect("expected TS2322 for lowerUpper assigned to upper");

    assert!(
        message.contains("is not assignable to type 'Uppercase<string>'."),
        "expected resolved intrinsic target display, got: {message}"
    );
    assert!(
        !message.contains("Uppercase<Uppercase<string>>"),
        "did not expect nested intrinsic repaint in target display, got: {message}"
    );
}

#[test]
fn test_ts2322_parameter_string_intrinsic_target_does_not_gain_nested_alias_display() {
    let source = r#"
function f(
    upper: Uppercase<string>,
    upperUpper: Uppercase<Uppercase<string>>,
    lowerUpper: Lowercase<Uppercase<string>>,
) {
    upper = lowerUpper;
}
"#;

    let diagnostics = diagnostics_for_source(source);
    let message = diagnostics
        .iter()
        .find_map(|d| {
            (d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && d.message_text.contains("Lowercase<Uppercase<string>>"))
            .then_some(d.message_text.as_str())
        })
        .expect("expected TS2322 for lowerUpper assigned to upper parameter");

    assert!(
        message.contains("is not assignable to type 'Uppercase<string>'."),
        "expected resolved intrinsic parameter target display, got: {message}"
    );
    assert!(
        !message.contains("Uppercase<Uppercase<string>>"),
        "did not expect nested intrinsic repaint for parameter target, got: {message}"
    );
}

#[test]
fn test_ts2322_parameter_nested_same_kind_string_intrinsic_simplifies_target_display() {
    let source = r#"
function f(
    upper: Uppercase<string>,
    upperUpper: Uppercase<Uppercase<string>>,
    lowerUpper: Lowercase<Uppercase<string>>,
) {
    upperUpper = lowerUpper;
}
"#;

    let diagnostics = diagnostics_for_source(source);
    let message = diagnostics
        .iter()
        .find_map(|d| {
            (d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && d.message_text.contains("Lowercase<Uppercase<string>>"))
            .then_some(d.message_text.as_str())
        })
        .expect("expected TS2322 for lowerUpper assigned to upperUpper parameter");

    assert!(
        message.contains("is not assignable to type 'Uppercase<string>'."),
        "expected simplified same-kind intrinsic target display, got: {message}"
    );
    assert!(
        !message.contains("Uppercase<Uppercase<string>>"),
        "did not expect nested same-kind intrinsic target display, got: {message}"
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
fn test_ts2322_js_accessor_jsdoc_does_not_force_inferred_getter_mismatch() {
    let source = r#"
        export class Foo {
            /**
             * @type {null | string}
             */
            _bar = null;

            get bar() {
                return this._bar;
            }
            /**
             * @type {string}
             */
            set bar(value) {
                this._bar = value;
            }
        }
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            allow_js: true,
            strict: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected JS accessor JSDoc pair to avoid TS2322 getter/setter mismatch. Actual diagnostics: {diagnostics:?}"
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
    // For contextually-typed expression-bodied arrow functions with identifier bodies
    // (like `undefined`), tsc elaborates the return type mismatch and reports TS2322
    // on the body expression rather than TS2345 on the whole callback argument.
    // This matches tsc behavior for contextual callbacks (no explicit param annotations).
    let source = r#"
function someGenerics3<T>(producer: () => T) { }
someGenerics3<number>(() => undefined);
"#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2322 = diagnostics
        .iter()
        .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);

    assert!(
        has_ts2322,
        "Expected TS2322 on the body expression for contextual callback, got: {diagnostics:?}"
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
fn test_ts2345_function_return_mismatch_related_detail_qualifies_same_named_returns() {
    let source = r#"
        declare namespace N { export interface Token { kind: "n"; } }
        declare namespace M { export interface Token { kind: "m"; } }
        declare function takes(cb: () => M.Token): void;
        declare const cb: () => N.Token;
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
                .contains("Return type 'N.Token' is not assignable to 'M.Token'.")
        }),
        "Expected TS2345 related return detail to qualify same-named return types, got: {ts2345:?}"
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
fn test_ts2345_index_signature_mismatch_related_detail_qualifies_same_named_values() {
    let source = r#"
        declare namespace N { export interface Token { kind: "n"; } }
        declare namespace M { export interface Token { kind: "m"; } }
        declare function takes(value: { [key: string]: M.Token }): void;
        declare const arg: { [key: string]: N.Token };
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
                "string index signature is incompatible: 'N.Token' is not assignable to 'M.Token'.",
            )
        }),
        "Expected TS2345 related info to qualify same-named index value types, got: {ts2345:?}"
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
fn test_ts2345_array_element_mismatch_related_detail_qualifies_same_named_elements() {
    let source = r#"
        declare namespace N { export interface Token { kind: "n"; } }
        declare namespace M { export interface Token { kind: "m"; } }
        declare function takes(value: M.Token[]): void;
        declare const arg: N.Token[];
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
                .contains("Array element type 'N.Token' is not assignable to 'M.Token'.")
        }),
        "Expected TS2345 related info to qualify same-named element types, got: {ts2345:?}"
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
        diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for 'string' not assignable to 'number' in @returns (.jsx), got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_assignable_through_generic_identity_in_jsdoc_mode() {
    // In @ts-check JS files, @returns {number} annotations ARE checked by tsc.
    // Returning "string" from a @returns {number} function should emit TS2322.
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
        diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for 'string' not assignable to 'number' in @returns, got: {diagnostics:?}"
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
        diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for 'string' not assignable to 'number' in @returns (.mjs), got: {diagnostics:?}"
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

    // TSC emits TWO TS2322 errors for shorthand defaults (combined type + default alone)
    // and ONE per non-shorthand default (default alone only — tsc does NOT check the full
    // source property type for non-shorthand). With 1 shorthand and 2 non-shorthand
    // assignments, we expect 2 + 1 + 1 = 4 errors.
    assert_eq!(
        ts2322_messages.len(),
        4,
        "Expected TS2322 for each undefined default in assignment destructuring (shorthand gets both combined and default-alone; non-shorthand gets default-alone only), got: {diagnostics:?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .any(|message| message.contains("Type 'undefined' is not assignable to type 'number'.")),
        "Expected at least one 'undefined' source display, got: {diagnostics:?}"
    );
    assert!(
        ts2322_messages.iter().any(|message| message
            .contains("Type 'number | undefined' is not assignable to type 'number'.")),
        "Expected at least one 'number | undefined' source display, got: {diagnostics:?}"
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

#[test]
fn test_ts2322_constructor_default_value_diagnostics_do_not_timeout() {
    let source = r#"
class C {
    constructor(x);
    constructor(public x: string = 1) {
        var y = x;
    }
}

class D<T, U> {
    constructor(x: T, y: U);
    constructor(x: T = 1, public y: U = x) {
        var z = x;
    }
}

class E<T extends Date> {
    constructor(x);
    constructor(x: T = new Date()) {
        var y = x;
    }
}
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: false,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322.len(),
        4,
        "Expected four TS2322 diagnostics for constructor parameter defaults, got: {diagnostics:?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, msg)| msg.contains("Type 'number' is not assignable to type 'string'")),
        "Expected string default initializer TS2322, got: {diagnostics:?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, msg)| msg.contains("Type 'number' is not assignable to type 'T'")),
        "Expected generic T default initializer TS2322, got: {diagnostics:?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, msg)| msg.contains("Type 'T' is not assignable to type 'U'")),
        "Expected generic parameter-property TS2322, got: {diagnostics:?}"
    );
    assert!(
        ts2322.iter().any(|(_, msg)| {
            msg.ends_with("is not assignable to type 'T'.")
                && !msg.contains("Type 'number' is not assignable to type 'T'.")
        }),
        "Expected constrained default initializer TS2322 for T, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_new_date_assignment_uses_nominal_date_display() {
    let source = r#"
function foo4<T extends U, U extends V, V extends Date>(t: T, u: U, v: V) {
    t = new Date();
    u = new Date();
    v = new Date();
}
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: false,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322.len(),
        3,
        "Expected three TS2322 diagnostics for Date-constrained generic assignments, got: {diagnostics:?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, msg)| msg.contains("Type 'Date' is not assignable to type 'T'.")),
        "Expected nominal Date display for T assignment, got: {diagnostics:?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, msg)| msg.contains("Type 'Date' is not assignable to type 'U'.")),
        "Expected nominal Date display for U assignment, got: {diagnostics:?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, msg)| msg.contains("Type 'Date' is not assignable to type 'V'.")),
        "Expected nominal Date display for V assignment, got: {diagnostics:?}"
    );
    assert!(
        ts2322.iter().all(|(_, msg)| !msg.contains("getVarDate")),
        "Did not expect structural Date expansion in TS2322 diagnostics, got: {diagnostics:?}"
    );
}

#[test]
fn indexed_access_on_intersection_preserves_deferred_constraints() {
    // Repro from TypeScript#14723 / conformance test compiler/indexedAccessRelation.ts.
    //
    // Fixed: when evaluating (S & State<T>)["a"] in the mapped type
    // template for Pick<S & State<T>, K>, the solver now preserves deferred
    // IndexAccess types for unconstrained type parameters.
    // This ensures S["a"] is included in the result (S["a"] & (T | undefined)),
    // making T not assignable and TS2322 correctly emitted.
    //
    // tsc keeps (S & State<T>)["a"] as a deferred indexed access type,
    // which correctly rejects T as not assignable to the full expression.
    //
    // Fix requires changes to either:
    // 1. Mapped type evaluation to preserve deferred indexed access for
    //    non-homomorphic mapped types (but Application eval caching
    //    prevents the fix from taking effect), OR
    // 2. The indexed access intersection distribution to include deferred
    //    results (but this causes false positives in homomorphic mapped
    //    types like Readonly<TType & { name: string }>).
    let source = r#"
class Component<S> {
    setState<K extends keyof S>(state: Pick<S, K>) {}
}

export interface State<T> {
    a?: T;
}

class Foo {}

class Comp<T extends Foo, S> extends Component<S & State<T>>
{
    foo(a: T) {
        this.setState({ a: a });
    }
}
"#;
    let diagnostics = get_all_diagnostics(source);
    let ts2322 = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect::<Vec<_>>();
    assert!(
        !ts2322.is_empty(),
        "Expected TS2322 for indexed access on intersection with unconstrained type parameter. Actual diagnostics: {diagnostics:?}"
    );
    assert!(
        ts2322.iter().any(|(_, msg)| msg
            .contains("Type 'T' is not assignable to type '(S & State<T>)[\"a\"] | undefined'.")),
        "Expected top-level TS2322 to preserve the contextual indexed-access target surface, got: {diagnostics:?}"
    );
}

/// Regression test: arrays should NOT be assignable to interfaces that extend
/// ReadonlyArray/Array but have additional required properties.
///
/// In TypeScript, `TemplateStringsArray` extends `ReadonlyArray<string>` with
/// `readonly raw: readonly string[]`. An empty array `[]` (type `never[]`) lacks
/// the `raw` property, so `var x: TemplateStringsArray = []` should produce TS2322.
///
/// This was previously incorrectly accepted because the array-to-interface subtype
/// shortcut (`check_array_interface_subtype`) checked only `Array<T> <: target`
/// without verifying the target's extra declared properties.
#[test]
fn test_ts2322_array_not_assignable_to_interface_extending_array_with_extra_props() {
    let source = r#"
        interface ArrayWithExtra extends ReadonlyArray<string> {
            readonly raw: readonly string[];
        }
        var x: string[] = [];
        var y: ArrayWithExtra = x;
    "#;

    let diagnostics = diagnostics_for_source(source);
    let assignability_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE  // TS2322
                || d.code == 2741  // TS2741: Property 'X' is missing
                || d.code == 2739 // TS2739: Type 'X' is missing properties
        })
        .collect();
    assert!(
        !assignability_errors.is_empty(),
        "Expected TS2322/TS2741/TS2739 when assigning string[] to interface extending ReadonlyArray with extra properties. All diagnostics: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn nested_weak_type_in_intersection_target_emits_ts2322() {
    // When assigning to an intersection target where nested properties are weak types,
    // the weak type check must still apply to the inner property comparison.
    // `in_intersection_member_check` should only suppress weak type checks at the
    // direct intersection member level, not for nested property types.
    // See: nestedExcessPropertyChecking.ts
    let source = r#"
        type A1 = { x: { a?: string } };
        type B1 = { x: { b?: string } };
        type C1 = { x: { c: string } };
        const ab1: A1 & B1 = {} as C1;
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2322 = diagnostics.iter().any(|(code, _)| *code == 2322);
    let has_ts2559 = diagnostics.iter().any(|(code, _)| *code == 2559);
    assert!(
        has_ts2322 || has_ts2559,
        "Expected TS2322 or TS2559 for nested weak type mismatch in intersection target. Got: {diagnostics:?}"
    );
}

#[test]
fn flat_weak_type_in_intersection_target_emits_ts2559() {
    // For flat (non-nested) weak types in an intersection, TS2559 should be emitted.
    let source = r#"
        type A2 = { a?: string };
        type B2 = { b?: string };
        type C2 = { c: string };
        const ab2: A2 & B2 = {} as C2;
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2559 = diagnostics.iter().any(|(code, _)| *code == 2559);
    assert!(
        has_ts2559,
        "Expected TS2559 for flat weak type mismatch in intersection target. Got: {diagnostics:?}"
    );
}

#[test]
fn intersection_member_weak_type_suppression_still_works() {
    // When the source has properties that overlap with one intersection member
    // but not with a weak-type member, the assignment should still pass.
    // The weak type suppression during intersection member checking should work
    // at the DIRECT level but not for nested property types.
    let source = r#"
        interface ITreeItem {
            Parent?: ITreeItem;
        }
        interface IDecl {
            Id?: number;
        }
        const x: ITreeItem & IDecl = {} as ITreeItem;
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2322 = diagnostics.iter().any(|(code, _)| *code == 2322);
    let has_ts2559 = diagnostics.iter().any(|(code, _)| *code == 2559);
    assert!(
        !has_ts2322 && !has_ts2559,
        "ITreeItem should be assignable to ITreeItem & IDecl without error. Got: {diagnostics:?}"
    );
}

#[test]
fn primitive_number_literal_vs_weak_type_emits_ts2559() {
    // A number literal assigned to a weak type (all optional properties)
    // should emit TS2559, not TS2322/TS2345.
    // See: weakType.ts - `doSomething(12)`
    let source = r#"
        interface Settings {
            timeout?: number;
            onError?(): void;
        }
        function doSomething(settings: Settings) {}
        doSomething(12);
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2559 = diagnostics.iter().any(|(code, _)| *code == 2559);
    assert!(
        has_ts2559,
        "Expected TS2559 for number literal assigned to weak type. Got: {diagnostics:?}"
    );
}

#[test]
fn primitive_string_literal_vs_weak_type_emits_ts2559() {
    // A string literal assigned to a weak type should emit TS2559.
    let source = r#"
        interface Settings {
            timeout?: number;
            onError?(): void;
        }
        function doSomething(settings: Settings) {}
        doSomething("completely wrong");
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2559 = diagnostics.iter().any(|(code, _)| *code == 2559);
    assert!(
        has_ts2559,
        "Expected TS2559 for string literal assigned to weak type. Got: {diagnostics:?}"
    );
}

#[test]
fn primitive_boolean_literal_vs_weak_type_emits_ts2559() {
    // A boolean literal assigned to a weak type should emit TS2559.
    let source = r#"
        interface Settings {
            timeout?: number;
            onError?(): void;
        }
        function doSomething(settings: Settings) {}
        doSomething(false);
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2559 = diagnostics.iter().any(|(code, _)| *code == 2559);
    assert!(
        has_ts2559,
        "Expected TS2559 for boolean literal assigned to weak type. Got: {diagnostics:?}"
    );
}

#[test]
fn enum_member_vs_weak_type_emits_ts2559() {
    // A string enum member assigned to a weak type with no common properties
    // should emit TS2559.
    // See: nestedExcessPropertyChecking.ts - `let x: { nope?: any } = E.A`
    let source = r#"
        enum E { A = "A" }
        let x: { nope?: any } = E.A;
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2559 = diagnostics.iter().any(|(code, _)| *code == 2559);
    assert!(
        has_ts2559,
        "Expected TS2559 for enum member assigned to weak type. Got: {diagnostics:?}"
    );
}

#[test]
fn primitive_with_matching_property_passes_weak_type() {
    // A string assigned to a weak type that has 'length' property should NOT
    // trigger TS2559 because strings have a 'length' property.
    let source = r#"
        let x: { length?: number } = "hello" as any as string;
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2559 = diagnostics.iter().any(|(code, _)| *code == 2559);
    assert!(
        !has_ts2559,
        "String should not trigger TS2559 for weak type with 'length' property. Got: {diagnostics:?}"
    );
}

#[test]
fn callable_value_to_weak_type_emits_ts2560_not_ts2559() {
    // When passing a callable value to a parameter with a weak type (all optional
    // properties), and calling the value would produce a compatible type,
    // tsc emits TS2560 ("did you mean to call it?") instead of TS2559.
    // See: weakType.ts - `doSomething(getDefaultSettings)`
    let source = r#"
        interface Settings {
            timeout?: number;
            onError?(): void;
        }
        function getDefaultSettings() {
            return { timeout: 1000 };
        }
        function doSomething(settings: Settings) {}
        doSomething(getDefaultSettings);
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2560 = diagnostics.iter().any(|(code, _)| *code == 2560);
    let has_ts2559 = diagnostics.iter().any(|(code, _)| *code == 2559);
    assert!(
        has_ts2560,
        "Expected TS2560 for callable value assigned to weak type. Got: {diagnostics:?}"
    );
    assert!(
        !has_ts2559,
        "Should emit TS2560, not TS2559, for callable value. Got: {diagnostics:?}"
    );
}

#[test]
fn arrow_function_to_weak_type_emits_ts2560() {
    // An arrow function returning a compatible type should emit TS2560.
    // See: weakType.ts - `doSomething(() => ({ timeout: 1000 }))`
    let source = r#"
        interface Settings {
            timeout?: number;
            onError?(): void;
        }
        function doSomething(settings: Settings) {}
        doSomething(() => ({ timeout: 1000 }));
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2560 = diagnostics.iter().any(|(code, _)| *code == 2560);
    assert!(
        has_ts2560,
        "Expected TS2560 for arrow function assigned to weak type. Got: {diagnostics:?}"
    );
}

#[test]
fn primitive_still_emits_ts2559_not_ts2560() {
    // Primitives (non-callable) should still emit TS2559, not TS2560.
    let source = r#"
        interface Settings {
            timeout?: number;
            onError?(): void;
        }
        function doSomething(settings: Settings) {}
        doSomething(12);
        doSomething(false);
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2559 = diagnostics.iter().any(|(code, _)| *code == 2559);
    let has_ts2560 = diagnostics.iter().any(|(code, _)| *code == 2560);
    assert!(
        has_ts2559,
        "Expected TS2559 for primitives assigned to weak type. Got: {diagnostics:?}"
    );
    assert!(
        !has_ts2560,
        "Should not emit TS2560 for non-callable primitives. Got: {diagnostics:?}"
    );
}

/// Regression: genericFunctionCallSignatureReturnTypeMismatch.ts
/// `{ <S>(): S[] }` assigned to `{ <T>(x: T): T }` should emit TS2322
/// because the return types are incompatible (S[] is not assignable to type param S).
#[test]
fn test_generic_callable_return_type_mismatch_emits_ts2322() {
    let source = r#"
        declare var f: { <T>(x: T): T; };
        declare var g: { <S>(): S[]; };
        f = g;
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2322 = diagnostics.iter().any(|(code, _)| *code == 2322);
    assert!(
        has_ts2322,
        "Expected TS2322 for incompatible generic callable assignment. Got: {diagnostics:?}"
    );
}

// ============================================================================
// TS2741 → TS2322 downgrade guards
// ============================================================================

/// When a function type is assigned to a class with private members, TSC emits TS2322
/// (generic assignability), not TS2741 (missing property). Private brands should be
/// handled as nominal class mismatches.
#[test]
fn test_function_to_class_with_private_emits_ts2322_not_ts2741() {
    let source = r#"
        class C { private x = 1; }
        class D extends C { }
        function foo(x: "hi", items: string[]): typeof foo;
        function foo(x: string, items: string[]): typeof foo { return null as any; }
        var a: D = foo("hi", []);
    "#;
    let diagnostics = get_all_diagnostics(source);
    let has_ts2741 = diagnostics.iter().any(|(code, _)| *code == 2741);
    assert!(
        !has_ts2741,
        "Should not emit TS2741 for function→class assignment with private members. Got: {diagnostics:?}"
    );
    let has_ts2322 = diagnostics.iter().any(|(code, _)| *code == 2322);
    assert!(
        has_ts2322,
        "Expected TS2322 for function→class assignment. Got: {diagnostics:?}"
    );
}

/// When assigning to a type with an index signature, and the "missing" property comes
/// from the index signature value type (not a direct named property), TSC emits TS2322.
#[test]
fn test_index_signature_target_missing_prop_emits_ts2322_not_ts2741() {
    let source = r#"
        type A = { a: string };
        type B = { b: string };
        declare let sb1: { x: A } & { y: B };
        declare let tb1: { [key: string]: A };
        tb1 = sb1;
    "#;
    let diagnostics = get_all_diagnostics(source);
    let has_ts2741 = diagnostics.iter().any(|(code, _)| *code == 2741);
    assert!(
        !has_ts2741,
        "Should not emit TS2741 for index signature target mismatch. Got: {diagnostics:?}"
    );
    let has_ts2322 = diagnostics.iter().any(|(code, _)| *code == 2322);
    assert!(
        has_ts2322,
        "Expected TS2322 for index signature target mismatch. Got: {diagnostics:?}"
    );
}

#[test]
fn test_union_index_signature_object_literal_value_mismatches_emit_ts2322() {
    let source = r#"
interface IValue {
  value: string
}

interface StringKeys {
    [propertyName: string]: IValue;
};

interface NumberKeys {
    [propertyName: number]: IValue;
}

type ObjectDataSpecification = StringKeys | NumberKeys;

const dataSpecification: ObjectDataSpecification = {
    foo: "asdfsadffsd"
};

const obj1: { [x: string]: number } | { [x: number]: number } = { a: 'abc' };
const obj2: { [x: string]: number } | { a: number } = { a: 5, c: 'abc' };
"#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        3,
        "Expected three TS2322 index-signature value mismatches. Got: {diagnostics:?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, message)| message
                .contains("Type 'string' is not assignable to type 'IValue'.")),
        "Expected string-to-IValue mismatch. Got: {diagnostics:?}"
    );
    assert_eq!(
        ts2322
            .iter()
            .filter(|(_, message)| message
                .contains("Type 'string' is not assignable to type 'number'."))
            .count(),
        2,
        "Expected two string-to-number mismatches. Got: {diagnostics:?}"
    );
}

#[test]
fn test_nested_discriminated_union_property_mismatch_emits_ts2322() {
    let source = r#"
type AN = { a: string } | { c: string }
type BN = { b: string }
type AB = { kind: "A", n: AN } | { kind: "B", n: BN }

const abab: AB = {
    kind: "A",
    n: {
        a: "a",
        b: "b",
    }
}
"#;

    let diagnostics = diagnostics_for_source(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected one nested union TS2322 mismatch. Got: {diagnostics:?}"
    );
    assert!(
        ts2322.iter().any(|diagnostic| {
            diagnostic.message_text.contains(
            "Type '{ kind: \"A\"; n: { a: string; b: string; }; }' is not assignable to type 'AB'."
        )
        }),
        "Expected outer AB assignability message. Got: {diagnostics:?}"
    );
    let expected_start = source.find("b: \"b\"").expect("expected b property") as u32;
    assert_eq!(
        ts2322[0].start, expected_start,
        "Expected TS2322 to anchor at the rejected nested property. Got: {diagnostics:?}"
    );

    let ok_source = r#"
type AN = { a: string } | { c: string }
type BN = { b: string }
type AB = { kind: "A", n: AN } | { kind: "B", n: BN }

const abac: AB = {
    kind: "A",
    n: {
        a: "a",
        c: "c",
    }
}
"#;

    let ok_diagnostics = get_all_diagnostics(ok_source);
    assert!(
        !ok_diagnostics.iter().any(|(code, _)| matches!(
            *code,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE | 2353
        )),
        "Expected valid nested union object to stay accepted. Got: {ok_diagnostics:?}"
    );
}

#[test]
fn object_freeze_preserves_literal_property_values_for_readonly_return() {
    let source = r#"
const PUPPETEER_REVISIONS = Object.freeze({
    chromium: '1011831',
    firefox: 'latest',
});

let preferredRevision = PUPPETEER_REVISIONS.chromium;
preferredRevision = PUPPETEER_REVISIONS.firefox;
"#;

    let diagnostics = diagnostics_for_source(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected one TS2322 for Object.freeze literal property mismatch. Got: {diagnostics:?}"
    );
    assert!(
        ts2322[0]
            .message_text
            .contains("Type '\"latest\"' is not assignable to type '\"1011831\"'."),
        "Expected literal property values to be preserved through Object.freeze. Got: {diagnostics:?}"
    );
    assert_eq!(
        ts2322[0].start,
        source
            .find("preferredRevision = PUPPETEER_REVISIONS.firefox")
            .expect("assignment should exist") as u32,
        "Expected TS2322 to anchor at the assignment expression. Got: {diagnostics:?}"
    );
}

/// Regression: assignFromStringInterface2.ts
/// When both source and target have number index signatures but the source is
/// missing named properties from the target, TS2739/TS2740 should be emitted
/// (not TS2322). Number index signatures (common on String, Array, etc.) must
/// NOT suppress the missing-properties diagnostic.
#[test]
fn test_missing_properties_not_suppressed_by_number_index_signatures() {
    let source = r#"
        interface Target {
            foo(): string;
            bar(): string;
            baz(): string;
            qux(): string;
            quux(): string;
            corge(): string;
            grault(): string;
            [index: number]: string;
        }

        interface Source {
            foo(): string;
            [index: number]: string;
        }

        declare var target: Target;
        declare var source: Source;
        target = source;
    "#;

    let diagnostics = get_all_diagnostics(source);
    // TS2740 = "missing the following properties ... and N more" (6+ missing)
    let has_missing_props = diagnostics.iter().any(|(code, _)| {
        *code == diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
    });
    assert!(
        has_missing_props,
        "Expected TS2740 (missing properties) when both types have number index signatures \
         but source is missing named properties. Number index signatures should NOT suppress \
         missing-property diagnostics in favor of TS2322. Got: {diagnostics:?}"
    );
    // Should NOT have TS2322 for this case — TS2740 replaces it
    let has_ts2322 = diagnostics.iter().any(|(code, _)| *code == 2322);
    assert!(
        !has_ts2322,
        "Expected TS2740, not TS2322, when source is missing named properties. Got: {diagnostics:?}"
    );
}

/// Regression: didYouMeanElaborationsForExpressionsWhichCouldBeCalled.ts
/// `toLocaleString` (and other Object-prototype methods) must always be filtered
/// from TS2740/TS2739 missing-property lists — even when the target overrides it.
/// tsc's `getMissingMembersOfType` treats a property as missing only when the
/// source lacks any member with that name, and Object inheritance always
/// satisfies the name lookup for `toLocaleString`.  Including it in the
/// missing list inflates the "and N more" count by 1.
#[test]
fn test_ts2740_does_not_list_tolocalestring_as_missing() {
    // Synthesize a target with 6+ missing properties so TS2740 (with truncation)
    // fires.  The target adds a `toLocaleString` overload that the source does
    // not match, which in tsz used to surface `toLocaleString` as a missing
    // property.  tsz must always filter Object-prototype names from the missing
    // list since the source has them by name via Object inheritance.
    let source = r#"
interface Target {
    toLocaleString(): string;
    toLocaleString(locale: string, options: object): string;
    m1: number;
    m2: number;
    m3: number;
    m4: number;
    m5: number;
    m6: number;
    m7: number;
}

declare const s: { foo: string };
const tt: Target = s;
"#;

    let diagnostics = get_all_diagnostics(source);
    let ts2740 = diagnostics
        .iter()
        .find(|(code, _)| {
            *code == diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
        })
        .expect("expected TS2740 for assigning narrower type to Target");
    // The missing list is the substring after the colon.  Splitting at ": "
    // yields the source display first, then the target display, then the list.
    let missing_list = ts2740
        .1
        .split(": ")
        .nth(2)
        .expect("TS2740 message should contain `: <list>`");
    assert!(
        !missing_list.contains("toLocaleString"),
        "TS2740 missing list must not include `toLocaleString` (Object-prototype method), got: {missing_list}"
    );
    assert!(
        missing_list.contains("and 3 more"),
        "TS2740 missing list should report `and 3 more` for 7 missing m1..m7, got: {missing_list}"
    );
}

/// When `strictBuiltinIteratorReturn` is true, `BuiltinIteratorReturn` resolves to `undefined`.
/// Assigning `undefined` to `number` must produce TS2322.
#[test]
fn test_strict_builtin_iterator_return_ts2322() {
    // Use BuiltinIteratorReturn directly — it's defined as `type BuiltinIteratorReturn = intrinsic`
    // in lib.es2015.iterable.d.ts and resolves to `undefined` when strict.
    let source = r#"
type R = BuiltinIteratorReturn;
const x: number = undefined as R;
"#;
    let options = CheckerOptions {
        strict_builtin_iterator_return: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_with_libs_for_ts(source, "test.ts", options);

    let ts2322_count = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected TS2322 for assigning BuiltinIteratorReturn (=undefined) to number when \
         strictBuiltinIteratorReturn is true. Got: {diagnostics:?}"
    );
}

/// When `strictBuiltinIteratorReturn` is false, `BuiltinIteratorReturn` resolves to `any`.
/// Assigning `any` to `number` is always allowed, so no error.
#[test]
fn test_no_error_without_strict_builtin_iterator_return() {
    let source = r#"
declare const x: BuiltinIteratorReturn;
const r1: number = x;
"#;
    let options = CheckerOptions {
        strict_builtin_iterator_return: false,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_with_libs_for_ts(source, "test.ts", options);

    let ts2322_count = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .count();
    assert!(
        ts2322_count == 0,
        "Expected no TS2322 when strictBuiltinIteratorReturn is false \
         (BuiltinIteratorReturn=any). Got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_keeps_outer_object_error_for_direct_index_access_target() {
    let source = r#"
interface TextChannel {
    id: string;
    type: 'text';
    phoneNumber: string;
}

interface EmailChannel {
    id: string;
    type: 'email';
    addres: string;
}

type Channel = TextChannel | EmailChannel;

export type ChannelType = Channel extends { type: infer R } ? R : never;

type Omit<T, K extends keyof T> = Pick<
    T,
    ({ [P in keyof T]: P } & { [P in K]: never } & { [x: string]: never })[keyof T]
>;

type ChannelOfType<T extends ChannelType, A = Channel> = A extends { type: T }
    ? A
    : never;

export type NewChannel<T extends Channel> = Pick<T, 'type'> &
    Partial<Omit<T, 'type' | 'id'>> & { localChannelId: string };

export function makeNewChannel<T extends ChannelType>(type: T): NewChannel<ChannelOfType<T>> {
    const localChannelId = `blahblahblah`;
    return { type, localChannelId };
}

const newTextChannel = makeNewChannel('text');
newTextChannel.phoneNumber = '613-555-1234';

const newTextChannel2 : NewChannel<TextChannel> = makeNewChannel('text');
newTextChannel2.phoneNumber = '613-555-1234';
"#;
    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: false,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected one outer TS2322 for the return object. Got: {diagnostics:?}"
    );
    assert!(
        ts2322.iter().any(|(_, message)| message.contains(
            "Type '{ type: T; localChannelId: string; }' is not assignable to type 'NewChannel<"
        ) && message.contains("ChannelOfType<T, TextChannel>")
            && message.contains("ChannelOfType<T, EmailChannel>")),
        "Expected TS2322 to stay on the outer object literal. Got: {diagnostics:?}"
    );
    assert!(
        ts2322
            .iter()
            .all(|(_, message)| !message.contains("never[\"type\"]")),
        "Did not expect property-level never[\"type\"] elaboration. Got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_const_type_param_multi() {
    // When a function has multiple type params and the first is `const`,
    // the solver's full inference path (used for >1 type params) must not
    // produce a false TS2322 on the argument. Previously, the final argument
    // check compared the checker's const-asserted arg type against the
    // solver's independently const-inferred type (different TypeIds for
    // semantically identical readonly types).
    let source = r#"
function f<const T, U>(x: T): T { return x; }
const t = f({ a: 1, b: "c", d: ["e", 2] });
"#;
    assert!(
        !has_error_with_code(source, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should not emit TS2322 for const type parameter with multiple type params"
    );
}

#[test]
fn non_primitive_conditional_with_type_params_matches_tsc_errors() {
    let source = r#"
type A<T, V> = { [P in keyof T]: T[P] extends V ? 1 : 0; };
type B<T, V> = { [P in keyof T]: T[P] extends V | object ? 1 : 0; };

let a: A<{ a: 0 | 1 }, 0> = { a: 0 };
let b: B<{ a: 0 | 1 }, 0> = { a: 0 };

function foo<T, U>(x: T) {
    let a: object = x;
    let b: U | object = x;
}
"#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322_messages = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, message)| message.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        ts2322_messages.len(),
        2,
        "expected only the two generic assignment errors, got: {diagnostics:?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .any(|message| message.contains("Type 'T' is not assignable to type 'object'.")),
        "missing T to object diagnostic, got: {ts2322_messages:?}"
    );
    assert!(
        ts2322_messages.iter().any(|message| {
            message.contains("Type 'T' is not assignable to type 'object | U'.")
                || message.contains("Type 'T' is not assignable to type 'U | object'.")
        }),
        "missing T to U | object diagnostic, got: {ts2322_messages:?}"
    );
    assert!(
        !ts2322_messages
            .iter()
            .any(|message| message.contains("B<{")),
        "mapped conditional assignment should not fail, got: {ts2322_messages:?}"
    );
}

#[test]
fn ts2322_optional_property_vs_number_index_preserves_implicit_undefined() {
    // tsc: `{ 1?: string }` assigned to `{ [k: number]: string }` must error
    // because the optional `1` contributes `string | undefined` to the check
    // against the number index value type `string`. Regression test for
    // `optionalPropertyAssignableToStringIndexSignature.ts`.
    let source = r#"
declare let probablyArray: { [key: number]: string };
declare let numberLiteralKeys: { 1?: string };
probablyArray = numberLiteralKeys;
"#;
    let options = CheckerOptions {
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = with_lib_contexts(source, "test.ts", options);
    assert!(
        diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "expected TS2322 for optional numeric property vs number index, got: {diagnostics:?}"
    );
}

#[test]
fn ts2322_optional_string_property_vs_string_index_still_ok() {
    // Regression guard: tsc allows `{ k1?: string }` assigned to
    // `{ [k: string]: string }` because the string index strips the implicit
    // `| undefined` contributed by the optional flag.
    let source = r#"
declare let optionalProperties: { k1?: string };
let stringDictionary: { [key: string]: string } = optionalProperties;
"#;
    let options = CheckerOptions {
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = with_lib_contexts(source, "test.ts", options);
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "expected no TS2322 for `{{ k1?: string }}` vs string index, got: {diagnostics:?}"
    );
}

#[test]
fn exact_optional_property_write_uses_ts2412() {
    let source = r#"
interface U2 {
    email?: string | number;
}
declare const e: string | boolean | undefined;
declare let u2: U2;
u2.email = e;
"#;
    let options = CheckerOptions {
        exact_optional_property_types: true,
        ..CheckerOptions::default()
    };
    let diagnostics = with_lib_contexts(source, "test.ts", options);

    assert!(
        diagnostics.iter().any(|(code, _)| {
            *code
                == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_WITH_EXACTOPTIONALPROPERTYTYPES_TRUE_CONSIDER_ADD_2
        }),
        "Expected TS2412 for exact-optional property write mismatch, got: {diagnostics:#?}"
    );
}

#[test]
fn exact_optional_property_object_message_preserves_optional_target_surface() {
    let source = r#"
const x: { foo?: number } = { foo: undefined };
"#;
    let options = CheckerOptions {
        exact_optional_property_types: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = with_lib_contexts(source, "test.ts", options);

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code
                == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_WITH_EXACTOPTIONALPROPERTYTYPES_TRUE_CONSIDER_ADD
                && message.contains("type '{ foo?: number; }'")
                && !message.contains("foo?: number | undefined")
        }),
        "Expected TS2375 target display to omit synthetic undefined, got: {diagnostics:#?}"
    );
}

/// Regression test for `widen_fresh_object_literal_properties_for_display`:
/// the helper must only widen literal property types when the outer object
/// is itself a *fresh* object literal. Annotated types like `{ a: "x" }`
/// carry the user's intent and must not have their literal property types
/// widened away in TS2741/TS2345 diagnostics.
///
/// Before the fix, `widen_fresh_object_literal_properties_for_display`
/// always widened all literal properties regardless of freshness, so the
/// annotated parameter type `{ a: "x" }` was rendered as `{ a: string; }`
/// in TS2345/TS2741 diagnostics — diverging from `tsc`, which preserves
/// `{ a: "x" }` because the user wrote it that way.
#[test]
fn test_ts2741_annotated_literal_target_preserves_literal_property() {
    let source = r#"
const fn1 = (s: { a: "x" }) => {};
fn1({});
"#;
    let diagnostics = get_all_diagnostics(source);
    let target_messages: Vec<&str> = diagnostics
        .iter()
        .map(|(_, message)| message.as_str())
        .collect();
    assert!(
        target_messages
            .iter()
            .any(|m| m.contains("'{ a: \"x\"; }'")),
        "expected annotated literal target `{{ a: \"x\"; }}` to be preserved verbatim, got: {target_messages:#?}"
    );
    assert!(
        !target_messages
            .iter()
            .any(|m| m.contains("'{ a: string; }'")),
        "annotated `{{ a: \"x\" }}` must not be widened to `{{ a: string; }}` in diagnostics, got: {target_messages:#?}"
    );
}

/// Regression for `inferenceShouldFailOnEvolvingArrays.ts`:
///
/// Calling a generic function whose parameter type is `{ [K in U]: T }[U]`
/// (e.g. `function f<T extends string[], U extends string>(arg: { [K in U]: T }[U])`)
/// with an array literal whose element types violate the constraint of `T`
/// (e.g. `f([42])` against `T extends string[]`) should produce a TS2322
/// element-level error pointing at the offending element, matching tsc's
/// behavior. Previously the elaboration was suppressed because the *raw*
/// parameter type contains type parameters; the elaboration must still run
/// when the resolved source/target types are concrete.
#[test]
fn ts2322_array_element_elaborated_when_generic_param_resolves_to_concrete_constraint() {
    let source = r#"
function logFirstLength<T extends string[], U extends string>(arg: { [K in U]: T }[U]): T {
    return arg;
}
logFirstLength([42]);
"#;
    let options = CheckerOptions {
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = with_lib_contexts(source, "test.ts", options);

    let ts2322_count = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .count();
    let ts2345_count = diagnostics
        .iter()
        .filter(|(code, _)| {
            *code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .count();

    assert!(
        ts2322_count >= 1,
        "Expected at least one TS2322 element elaboration for array-literal arg in generic call, got 0. Diagnostics: {diagnostics:#?}"
    );
    assert_eq!(
        ts2345_count, 0,
        "Expected no TS2345 on the whole array argument once element-level TS2322 is emitted. Diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, msg)| {
            *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && msg.contains("'number'")
                && msg.contains("'string'")
        }),
        "Expected TS2322 message mentioning 'number' and 'string' for the array element, got: {diagnostics:#?}"
    );
}
