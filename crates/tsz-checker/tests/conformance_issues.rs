//! Unit tests documenting known conformance test failures
//!
//! These tests are marked `#[ignore]` and document specific issues found during
//! conformance test investigation (2026-02-08). They serve as:
//! - Documentation of expected vs actual behavior
//! - Easy verification when fixes are implemented
//! - Minimal reproduction cases for debugging
//!
//! See docs/conformance-*.md for full context.

use rustc_hash::{FxHashMap, FxHashSet};
use std::path::Path;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Helper to compile TypeScript and get diagnostics
fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    compile_and_get_diagnostics_with_options(source, CheckerOptions::default())
}

fn compile_and_get_diagnostics_with_options(
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    compile_and_get_diagnostics_named("test.ts", source, options)
}

fn compile_and_get_diagnostics_named(
    file_name: &str,
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// Helper to check if specific error codes are present
fn has_error(diagnostics: &[(u32, String)], code: u32) -> bool {
    diagnostics.iter().any(|(c, _)| *c == code)
}

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
        manifest_dir.join("../TypeScript/src/lib/lib.dom.d.ts"),
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
            let lib_file = LibFile::from_source("lib.d.ts".to_string(), content);
            lib_files.push(Arc::new(lib_file));
        }
    }
    lib_files
}

fn compile_and_get_diagnostics_with_lib(source: &str) -> Vec<(u32, String)> {
    compile_and_get_diagnostics_with_lib_and_options(source, CheckerOptions::default())
}

fn compile_and_get_diagnostics_with_lib_and_options(
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let lib_files = load_lib_files_for_test();

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
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
    }

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn test_class_extends_aliased_base_preserves_instance_members() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Base<T> {
    value!: T;
}

class Derived extends Base<string> {
    getValue() {
        return this.value;
    }
}

const value: string = new Derived().getValue();
"#,
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected no non-lib diagnostics for class inheritance through aliased base symbol, got: {relevant:?}"
    );
}

#[test]
fn test_deeppartial_optional_chain_mixed_property_types_remain_distinct() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type DeepPartial<T> = T extends object ? { [P in keyof T]?: DeepPartial<T[P]> } : T;
type DeepInput<T> = DeepPartial<T>;

interface RetryOptions {
    timeout: number;
    retries: number;
    nested: {
        transport: {
            backoff: {
                base: number;
                max: number;
                jitter: number;
            };
        };
        flags: {
            fast: boolean;
            safe: boolean;
        };
    };
}

declare const options: DeepInput<RetryOptions> | undefined;

const base: number = options?.nested?.transport?.backoff?.base ?? 10;
const safe: boolean = options?.nested?.flags?.safe ?? false;
const bad: number = options?.nested?.flags?.safe ?? false;
        "#,
    );

    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for boolean-to-number assignment.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_class_extends_inherits_instance_members_via_symbol_path() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Base<T> {
    value!: T;
}

class Mid<T> extends Base<T> {}

class Derived extends Mid<string> {}

const ok: string = new Derived().value;
const bad: number = new Derived().value;
        "#,
    );

    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for assigning inherited string member to number.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2506),
        "Did not expect circular-base TS2506 in linear inheritance.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_indexed_access_constrained_type_param_no_ts2536() {
    let diagnostics = compile_and_get_diagnostics(
        r"
type PropertyType<T extends object, K extends keyof T> = T[K];
        ",
    );

    assert!(
        !has_error(&diagnostics, 2536),
        "Should not emit TS2536 when index type parameter is constrained by keyof.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_indexed_access_constrained_type_param_no_false_ts2304() {
    let diagnostics = compile_and_get_diagnostics(
        r"
type PropertyType<T extends object, K extends keyof T> = T[K];
        ",
    );

    assert!(
        !has_error(&diagnostics, 2304),
        "Should not emit TS2304 for in-scope type parameters in indexed access.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_indexed_access_unconstrained_type_param_emits_ts2536() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r"
type BadPropertyType<T extends object, K> = T[K];
        ",
    );

    assert!(
        has_error(&diagnostics, 2536),
        "Should emit TS2536 when type parameter is unconstrained for indexed access.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_indexed_access_array_element_through_constrained_union_no_ts2536() {
    let diagnostics = compile_and_get_diagnostics(
        r"
type Node =
    | { name: 'a'; children: Node[] }
    | { name: 'b'; children: Node[] };

type ChildrenOf<T extends Node> = T['children'][number];
        ",
    );

    assert!(
        !has_error(&diagnostics, 2536),
        "Should not emit TS2536 for element access through constrained array property.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_indexed_access_scalar_property_then_number_index_emits_ts2536() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r"
type Boxed = { value: number };
type Bad<T extends Boxed> = T['value'][number];
        ",
    );

    assert!(
        has_error(&diagnostics, 2536),
        "Should emit TS2536 when indexing a constrained scalar property with number.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_indexed_access_type_param_in_mapped_intersection_no_ts2536() {
    // Repro from conditionalTypes1.ts (#21862): type param T indexes an intersection
    // whose keyof includes T itself (from mapped types).
    let diagnostics = compile_and_get_diagnostics(
        r"
type OldDiff<T extends keyof any, U extends keyof any> = (
    & { [P in T]: P; }
    & { [P in U]: never; }
    & { [x: string]: never; }
)[T];
        ",
    );

    assert!(
        !has_error(&diagnostics, 2536),
        "Should not emit TS2536 when type param T indexes an intersection containing mapped type over T.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_mapped_type_direct_circular_constraint_reports_ts2313() {
    let diagnostics = compile_and_get_diagnostics(
        r"
type T00 = { [P in P]: string };
",
    );

    assert!(
        has_error(&diagnostics, 2313),
        "Expected TS2313 for direct mapped type parameter self reference.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2304),
        "Should not emit TS2304 for self-reference constraint.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_mapped_type_invalid_key_constraint_emits_ts2536() {
    let diagnostics = compile_and_get_diagnostics(
        r"
type Foo2<T, F extends keyof T> = {
    pf: { [P in F]?: T[P] },
    pt: { [P in T]?: T[P] },
};

type O = { x: number; y: boolean; };
let o: O = { x: 5, y: false };
    let f: Foo2<O, 'x'> = {
        pf: { x: 7 },
        pt: { x: 7, y: false },
    };
        ",
    );

    assert!(
        has_error(&diagnostics, 2536),
        "Expected TS2536 for `T[P]` when mapped key is constrained as `P in T`.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_mapped_type_key_index_access_constraint_emits_ts2536() {
    let diagnostics = compile_and_get_diagnostics(
        r"
type AB = { a: 'a'; b: 'a' };
type T1<K extends keyof AB> = { [key in AB[K]]: true };
type T2<K extends 'a'|'b'> = T1<K>[K];
        ",
    );

    assert!(
        has_error(&diagnostics, 2536),
        "Expected TS2536 for indexing mapped result with unconstrained key subset (`AB[K]` values).\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_element_access_mismatched_keyof_source_emits_ts2536() {
    let diagnostics = compile_and_get_diagnostics(
        r"
function f<T, U extends T>(x: T, y: U, k: keyof U) {
    x[k] = y[k];
    y[k] = x[k];
}

function g<T, U extends T, K extends keyof U>(x: T, y: U, k: K) {
    x[k] = y[k];
    y[k] = x[k];
}
        ",
    );

    let ts2536_count = diagnostics.iter().filter(|(code, _)| *code == 2536).count();
    assert!(
        ts2536_count >= 4,
        "Expected TS2536 for mismatched generic key source in element access.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_record_constraint_checked_with_lib_param_prewarm_filtering() {
    if load_lib_files_for_test().is_empty() {
        return;
    }
    let diagnostics =
        compile_and_get_diagnostics_with_lib(r#"type ValidRecord = Record<string, number>;"#);
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics for valid Record<K, V> usage.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_primitive_property_access_works_with_conditional_boxed_registration() {
    if load_lib_files_for_test().is_empty() {
        return;
    }
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
const upper = "hello".toUpperCase();
        "#,
    );
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics for primitive string property access.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_global_array_augmentation_uses_lib_resolution_without_diagnostics() {
    if load_lib_files_for_test().is_empty() {
        return;
    }
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
export {};

declare global {
    interface Array<T> {
        firstOrUndefined(): T | undefined;
    }
}

const xs = [1, 2, 3];
const first = xs.firstOrUndefined();
"#,
        CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics for Array global augmentation merged with lib declarations.\nActual diagnostics: {diagnostics:#?}"
    );
}

/// Helper to compile with `report_unresolved_imports` enabled (for import-related tests)
fn compile_imports_and_get_diagnostics(
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    checker.ctx.report_unresolved_imports = true;

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// Issue: Flow analysis applies narrowing from invalid assignments
///
/// From: derivedClassTransitivity3.ts
/// Expected: TS2322 only (assignment incompatibility)
/// Actual: TS2322 + TS2345 (also reports wrong parameter type on subsequent call)
///
/// Root cause: Flow analyzer treats invalid assignment as if it succeeded,
/// narrowing the variable type to the assigned type.
///
/// Complexity: HIGH - requires binder/checker coordination
/// See: docs/conformance-work-session-summary.md
#[test]
fn test_flow_narrowing_from_invalid_assignment() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class C<T> {
    foo(x: T, y: T) { }
}

class D<T> extends C<T> {
    foo(x: T) { } // ok to drop parameters
}

class E<T> extends D<T> {
    foo(x: T, y?: number) { } // ok to add optional parameters
}

declare var c: C<string>;
declare var e: E<string>;
c = e;                      // Should error: TS2322
var r = c.foo('', '');      // Should NOT error (c is still C<string>)
        ",
    );

    // Should only have TS2322 on the assignment
    assert!(
        has_error(&diagnostics, 2322),
        "Should emit TS2322 for assignment incompatibility"
    );
    assert!(
        !has_error(&diagnostics, 2345),
        "Should NOT emit TS2345 - c.foo should use C's signature, not E's.\nActual errors: {diagnostics:#?}"
    );
}

/// Issue: Parser emitting cascading error after syntax error
///
/// From: classWithPredefinedTypesAsNames2.ts
/// Expected: TS1005 only
/// Status: FIXED (2026-02-09)
///
/// Root cause: Parser didn't consume the invalid token after emitting error
/// Fix: Added `next_token()` call in `state_statements.rs` after reserved word error
#[test]
fn test_parser_cascading_error_suppression() {
    let source = r"
// classes cannot use predefined types as names
class void {}
        ";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let parser_diagnostics: Vec<(u32, String)> = parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect();

    // Should only emit TS1005 '{' expected
    let ts1005_count = parser_diagnostics
        .iter()
        .filter(|(c, _)| *c == 1005)
        .count();

    assert!(
        has_error(&parser_diagnostics, 1005),
        "Should emit TS1005 for syntax error.\nActual errors: {parser_diagnostics:#?}"
    );
    assert_eq!(
        ts1005_count, 1,
        "Should only emit one TS1005, got {ts1005_count}"
    );
    assert!(
        !has_error(&parser_diagnostics, 1068),
        "Should NOT emit cascading TS1068 error.\nActual errors: {parser_diagnostics:#?}"
    );
}

#[test]
fn test_method_implementation_name_formatting_probe() {
    let diagnostics = compile_and_get_diagnostics(
        r#"class C {
"foo"();
"bar"() { }
}"#,
    );
    println!("ClassDeclaration22 diag: {diagnostics:?}");

    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"class C {
"foo"();
"bar"() { }
}"#
        .to_string(),
    );
    let root = parser.parse_source_file();
    let source_file = parser.get_arena().get_source_file_at(root).unwrap();
    if let Some(first_stmt) = source_file.statements.nodes.first() {
        let class_node = parser.get_arena().get(*first_stmt).unwrap();
        let class_data = parser.get_arena().get_class(class_node).unwrap();
        for member_idx in &class_data.members.nodes {
            let member_node = parser.get_arena().get(*member_idx).unwrap();
            let kind = member_node.kind;
            if let Some(method) = parser.get_arena().get_method_decl(member_node) {
                let name_node = parser.get_arena().get(method.name).unwrap();
                let text = parser
                    .get_arena()
                    .get_literal(name_node)
                    .map(|lit| lit.text.clone())
                    .unwrap_or_else(|| "<non-literal>".to_string());
                println!(
                    "member kind={kind} method body={body:?} name={name_node:?} text={text}",
                    body = method.body,
                    name_node = method.name
                );
            }
        }
    }

    let diagnostics = compile_and_get_diagnostics(
        r#"class C {
["foo"](): void
["bar"](): void;
["foo"]() {
    return 0;
}
}"#,
    );
    println!("Overload computed diag: {diagnostics:?}");
}

/// Issue: Interface with reserved word name
///
/// Expected: TS1005 only (no cascading errors)
/// Status: FIXED (2026-02-09)
///
/// Root cause: Parser must consume invalid reserved-word names to avoid cascades.
/// Fix: Reserved-word interface names emit TS1005 and recover.
#[test]
fn test_interface_reserved_word_error_suppression() {
    let source = r"
interface class {}
    ";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let parser_diagnostics: Vec<(u32, String)> = parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect();

    // Should only emit TS1005 '{' expected
    let ts1005_count = parser_diagnostics
        .iter()
        .filter(|(c, _)| *c == 1005)
        .count();

    assert!(
        has_error(&parser_diagnostics, 1005),
        "Should emit TS1005 for syntax error.\nActual errors: {parser_diagnostics:#?}"
    );
    assert_eq!(
        ts1005_count, 1,
        "Should only emit one TS1005, got {ts1005_count}"
    );
    // Check for common cascading errors
    assert!(
        !has_error(&parser_diagnostics, 1068),
        "Should NOT emit cascading TS1068 error.\nActual errors: {parser_diagnostics:#?}"
    );
}

#[test]
fn test_class_extends_primitive_reports_ts2863() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class C extends number {}
        ",
    );

    assert!(
        has_error(&diagnostics, 2863),
        "Expected TS2863 when class extends primitive type. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_class_implements_primitive_reports_ts2864() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class C implements number {}
        ",
    );

    assert!(
        has_error(&diagnostics, 2864),
        "Expected TS2864 when class implements primitive type. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_indirect_class_cycle_reports_all_ts2506_errors() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class C extends E { foo: string; }
class D extends C { bar: string; }
class E extends D { baz: number; }

class C2<T> extends E2<T> { foo: T; }
class D2<T> extends C2<T> { bar: T; }
class E2<T> extends D2<T> { baz: T; }
        ",
    );

    let ts2506_count = diagnostics.iter().filter(|(code, _)| *code == 2506).count();
    assert_eq!(
        ts2506_count, 6,
        "Expected TS2506 on all six classes in the two cycles. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_class_extends_export_default_base_resolves_instance_members() {
    let diagnostics = compile_and_get_diagnostics(
        r"
export default class Base {
    value: number = 1;
}

class Derived extends Base {
    read(): number {
        return this.value;
    }
}
        ",
    );

    let unexpected: Vec<(u32, String)> = diagnostics
        .into_iter()
        .filter(|(code, _)| matches!(*code, 2339 | 2506 | 2449))
        .collect();

    assert!(
        unexpected.is_empty(),
        "Expected extends/default-base instance resolution without TS2339/TS2506/TS2449. Actual diagnostics: {unexpected:#?}"
    );
}

#[test]
fn test_class_interface_merge_preserves_callable_and_properties() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Merged {
    value: number = 1;
}

interface Merged {
    (x: number): string;
    extra: boolean;
}

declare const merged: Merged;
const okCall: string = merged(1);
const okProp: boolean = merged.extra;
const badCall: number = merged(1);
        ",
    );

    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for assigning merged callable string result to number.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2349),
        "Did not expect TS2349; merged class/interface type should remain callable.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2339),
        "Did not expect TS2339; merged interface property should remain visible.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_generic_multi_level_extends_resolves_base_instance_member_without_cycle_noise() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Box<T> {
    value!: T;
}

class Mid<U> extends Box<U> {}

class Final extends Mid<string> {
    read(): string {
        return this.value;
    }
}

const ok: string = new Final().value;
const bad: number = new Final().value;
        ",
    );

    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for assigning inherited string member to number.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2339),
        "Did not expect TS2339 for inherited base member lookup.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2506),
        "Did not expect TS2506 in non-cyclic generic inheritance.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2449),
        "Did not expect TS2449 for this linear declaration order.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_class_used_before_declaration_does_not_also_report_cycle_error() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class A extends B {}
class B extends C {}
class C {}
        ",
    );

    let has_ts2449 = diagnostics.iter().any(|(code, _)| *code == 2449);
    let has_ts2506 = diagnostics.iter().any(|(code, _)| *code == 2506);

    assert!(
        has_ts2449,
        "Expected TS2449 for class used before declaration. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_ts2506,
        "Did not expect TS2506 for non-cyclic before-declaration extends. Actual diagnostics: {diagnostics:#?}"
    );
}

/// Forward-reference class relationships should not trigger TS2506.
/// Derived extends Base, where Base is declared after Derived.
/// The `class_instance_resolution_set` recursion guard should not be
/// confused with a real circular inheritance cycle.
#[test]
fn test_complex_class_relationships_no_ts2506() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Derived extends Base {
    public static createEmpty(): Derived {
        var item = new Derived();
        return item;
    }
}
class Base {
    ownerCollection: any;
}
        ",
    );
    assert!(
        !has_error(&diagnostics, 2506),
        "Did not expect TS2506 for forward-reference class extends. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_interface_extends_primitive_reports_ts2840() {
    let diagnostics = compile_and_get_diagnostics(
        r"
interface I extends number {}
        ",
    );

    assert!(
        has_error(&diagnostics, 2840),
        "Expected TS2840 when interface extends primitive type. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_interface_extends_classes_with_private_member_clash_reports_ts2320() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class X {
    private m: number;
}
class Y {
    private m: string;
}

interface Z extends X, Y {}
        ",
    );

    assert!(
        has_error(&diagnostics, 2320),
        "Expected TS2320 when interface extends classes with conflicting private members. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_instance_member_initializer_constructor_param_capture_reports_ts2301() {
    let diagnostics = compile_and_get_diagnostics(
        r"
declare var console: {
    log(msg?: any): void;
};
var field1: string;

class Test1 {
    constructor(private field1: string) {
    }
    messageHandler = () => {
        console.log(field1);
    };
}
        ",
    );

    assert!(
        has_error(&diagnostics, 2301),
        "Expected TS2301 for constructor parameter capture in instance initializer. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_instance_member_initializer_missing_name_reports_ts2663() {
    let diagnostics = compile_and_get_diagnostics(
        r"
declare var console: {
    log(msg?: any): void;
};

export class Test1 {
    constructor(private field1: string) {
    }
    messageHandler = () => {
        console.log(field1);
    };
}
        ",
    );

    assert!(
        has_error(&diagnostics, 2663),
        "Expected TS2663 for missing free name in module instance initializer. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_instance_member_initializer_local_shadow_does_not_report_ts2301() {
    let diagnostics = compile_and_get_diagnostics(
        r"
declare var console: {
    log(msg?: any): void;
};

class Test {
    constructor(private field: string) {
    }
    messageHandler = () => {
        var field = this.field;
        console.log(field);
    };
}
        ",
    );

    assert!(
        !has_error(&diagnostics, 2301),
        "Did not expect TS2301 for locally shadowed identifier in initializer. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_unresolved_import_namespace_access_suppresses_ts2708() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
import { alias } from "foo";
let x = new alias.Class();
        "#,
    );

    assert!(
        !has_error(&diagnostics, 2708),
        "Should not emit cascading TS2708 for unresolved imported namespace access. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_super_call_args_match_instantiated_generic_base_ctor() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Base<T> {
    constructor(public value: T) {}
}

class Derived extends Base<number> {
    constructor() {
        super("hi");
    }
}
        "#,
    );

    assert!(
        has_error(&diagnostics, 2345),
        "Expected TS2345 for super argument type mismatch against instantiated base ctor. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_derived_constructor_without_super_reports_ts2377() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Base {}

class Derived extends Base {
    constructor() {}
}
        ",
    );

    assert!(
        has_error(&diagnostics, 2377),
        "Expected TS2377 for derived constructor missing super() call. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_this_before_missing_super_reports_ts17009() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Base {}

class Derived extends Base {
    constructor() {
        this.x;
    }
}
        ",
    );

    assert!(
        has_error(&diagnostics, 17009),
        "Expected TS17009 when 'this' is used in a derived constructor without super(). Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_malformed_this_property_annotation_does_not_emit_ts2551() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class A {
    constructor() {
        this.foo: any;
    }
}
        ",
    );

    assert!(
        !has_error(&diagnostics, 2551),
        "Did not expect TS2551 in malformed syntax recovery path. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_super_property_before_super_call_reports_ts17011() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Base {
    method() {}
}

class Derived extends Base {
    constructor() {
        super.method();
        super();
    }
}
        ",
    );

    assert!(
        has_error(&diagnostics, 17011),
        "Expected TS17011 for super property access before super() call. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_super_property_access_reports_ts2855() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Base {
    value = 1;
}

class Derived extends Base {
    method() {
        return super.value;
    }
}
        ",
    );

    assert!(
        has_error(&diagnostics, 2855),
        "Expected TS2855 for super property access to class field member. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_super_in_constructor_parameter_reports_ts2336_and_ts17011() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class B {
    public foo(): number {
        return 0;
    }
}

class C extends B {
    constructor(a = super.foo()) {
    }
}
                ",
    );

    assert!(
        has_error(&diagnostics, 2336),
        "Expected TS2336 for super in constructor argument context. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 17011),
        "Expected TS17011 for super property access before super() in constructor context. Actual diagnostics: {diagnostics:#?}"
    );
}

/// Issue: Overly aggressive strict null checking
///
/// From: neverReturningFunctions1.ts
/// Expected: No errors (control flow eliminates null/undefined)
/// Actual: TS18048 (possibly undefined)
///
/// Root cause: Control flow analysis not recognizing never-returning patterns
///
/// Complexity: HIGH - requires improving control flow analysis
/// See: docs/conformance-analysis-slice3.md
#[test]
fn test_narrowing_after_never_returning_function() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
// @strict: true
declare function fail(message?: string): never;

function f01(x: string | undefined) {
    if (x === undefined) fail("undefined argument");
    x.length;  // Should NOT error - x is string after never-returning call
}
        "#,
    );

    // Filter out TS2318 (missing global types - test harness doesn't load full lib)
    let semantic_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    assert!(
        semantic_errors.is_empty(),
        "Should emit no semantic errors - x is narrowed to string after never-returning call.\nActual errors: {semantic_errors:#?}"
    );
}

#[test]
fn test_optional_chain_undefined_equality_does_not_narrow_to_never() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type Thing = { foo: string | number };
function f(o: Thing | undefined) {
    if (o?.foo === undefined) {
        o.foo;
    }
}
        "#,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..Default::default()
        },
    );

    let semantic_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    assert!(
        !semantic_errors.iter().any(|(code, _)| *code == 2339),
        "Expected no TS2339 (no over-narrow to never). Actual: {semantic_errors:#?}"
    );
}

#[test]
fn test_optional_chain_typeof_undefined_does_not_narrow_to_never() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type Thing = { foo: string | number };
function f(o: Thing | undefined) {
    if (typeof o?.foo === "undefined") {
        o.foo;
    }
}
        "#,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..Default::default()
        },
    );

    let semantic_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    assert!(
        !semantic_errors.iter().any(|(code, _)| *code == 2339),
        "Expected no TS2339 (no over-narrow to never). Actual: {semantic_errors:#?}"
    );
}

#[test]
fn test_optional_chain_not_undefined_narrows_to_object() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type Thing = { foo: string | number };
function f(o: Thing | undefined) {
    if (o?.foo !== undefined) {
        o.foo;
    }
}
        "#,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..Default::default()
        },
    );

    let semantic_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    assert!(
        !semantic_errors.iter().any(|(code, _)| *code == 18048),
        "Expected no TS18048 in non-undefined optional-chain branch. Actual: {semantic_errors:#?}"
    );
    assert!(
        !semantic_errors.iter().any(|(code, _)| *code == 2339),
        "Expected no TS2339 in non-undefined optional-chain branch. Actual: {semantic_errors:#?}"
    );
}

/// Assignment-based narrowing should use declared annotation types, not initializer flow types.
///
/// Regression pattern: `let x: T | undefined = undefined; x = makeT(); use(x);`
/// Previously, flow assignment compatibility could read `x` as `undefined` and skip narrowing.
#[test]
fn test_assignment_narrowing_prefers_declared_annotation_type() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
// @strict: true
type Browser = { close(): void };
declare function makeBrowser(): Browser;
declare function consumeBrowser(b: Browser): void;

function test() {
    let browser: Browser | undefined = undefined;
    try {
        browser = makeBrowser();
        consumeBrowser(browser);
        browser.close();
    } finally {
    }
}
        "#,
    );

    let semantic_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    assert!(
        !semantic_errors
            .iter()
            .any(|(code, _)| *code == 2345 || *code == 18048),
        "Should not emit TS2345/TS18048 after assignment narrowing, got: {semantic_errors:#?}"
    );
}

/// Issue: Private identifiers in object literals
///
/// Expected: TS18016 (private identifiers not allowed outside class bodies)
/// Status: FIXED (2026-02-09)
///
/// Root cause: Parser wasn't validating private identifier usage in object literals
/// Fix: Added validation in `state_expressions.rs` `parse_property_assignment`
#[test]
fn test_private_identifier_in_object_literal() {
    // TS18016 is a PARSER error, so we need to check parser diagnostics
    let source = r"
const obj = {
    #x: 1
};
    ";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let parser_diagnostics: Vec<(u32, String)> = parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect();

    assert!(
        parser_diagnostics.iter().any(|(c, _)| *c == 18016),
        "Should emit TS18016 for private identifier in object literal.\nActual errors: {parser_diagnostics:#?}"
    );
}

/// Issue: Private identifier access outside class
///
/// Expected: TS18013 (property not accessible outside class)
/// Status: FIXED (2026-02-09)
///
/// Root cause: `get_type_of_private_property_access` didn't check class scope
/// Fix: Added check in `state_type_analysis.rs` to emit TS18013 when !`saw_class_scope`
#[test]
fn test_private_identifier_access_outside_class() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Foo {
    #bar = 42;
}
const f = new Foo();
const x = f.#bar;  // Should error TS18013
        ",
    );

    assert!(
        has_error(&diagnostics, 18013),
        "Should emit TS18013 for private identifier access outside class.\nActual errors: {diagnostics:#?}"
    );
}

/// Issue: Private identifier access from within class should work
///
/// Expected: No errors
/// Status: VERIFIED (2026-02-09)
#[test]
fn test_private_identifier_access_inside_class() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Foo {
    #bar = 42;
    getBar() {
        return this.#bar;  // Should NOT error
    }
}
        ",
    );

    assert!(
        !has_error(&diagnostics, 18013),
        "Should NOT emit TS18013 when accessing private identifier inside class.\nActual errors: {diagnostics:#?}"
    );
}

/// Issue: Private identifiers as parameters
///
/// Expected: TS18009 (private identifiers cannot be used as parameters)
/// Status: FIXED (2026-02-09)
///
/// Root cause: Parser wasn't validating private identifier usage as parameters
/// Fix: Added validation in `state_statements.rs` `parse_parameter`
#[test]
fn test_private_identifier_as_parameter() {
    // TS18009 is a PARSER error
    let source = r"
class Foo {
    method(#param: any) {}
}
    ";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let parser_diagnostics: Vec<(u32, String)> = parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect();

    assert!(
        parser_diagnostics.iter().any(|(c, _)| *c == 18009),
        "Should emit TS18009 for private identifier as parameter.\nActual errors: {parser_diagnostics:#?}"
    );
}

/// Issue: Private identifiers in variable declarations
///
/// Expected: TS18029 (private identifiers not allowed in variable declarations)
/// Status: FIXED (2026-02-09)
///
/// Root cause: Parser wasn't validating private identifier usage in variable declarations
/// Fix: Added validation in `state_statements.rs` `parse_variable_declaration_with_flags`
#[test]
fn test_private_identifier_in_variable_declaration() {
    // TS18029 is a PARSER error
    let source = r"
const #x = 1;
    ";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let parser_diagnostics: Vec<(u32, String)> = parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect();

    assert!(
        parser_diagnostics.iter().any(|(c, _)| *c == 18029),
        "Should emit TS18029 for private identifier in variable declaration.\nActual errors: {parser_diagnostics:#?}"
    );
}

/// Issue: Optional chain with private identifiers
///
/// Expected: TS18030 (optional chain cannot contain private identifiers)
/// Status: FIXED (2026-02-09)
///
/// Root cause: Parser wasn't validating private identifier usage in optional chains
/// Fix: Added validation in `state_expressions.rs` when handling `QuestionDotToken`
#[test]
fn test_private_identifier_in_optional_chain() {
    // TS18030 is a PARSER error
    let source = r"
class Bar {
    #prop = 42;
    test() {
        return this?.#prop;
    }
}
    ";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let parser_diagnostics: Vec<(u32, String)> = parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect();

    assert!(
        parser_diagnostics.iter().any(|(c, _)| *c == 18030),
        "Should emit TS18030 for private identifier in optional chain.\nActual errors: {parser_diagnostics:#?}"
    );
}

/// Issue: TS18016 checker validation - private identifier outside class
///
/// For property access expressions (`obj.#bar`), TSC only emits TS18013 (semantic:
/// can't access private member) — NOT TS18016 (grammar: private identifier outside class).
/// TS18016 is only emitted for truly invalid syntax positions (object literals, etc.)
/// because `obj.#bar` is valid syntax even outside a class body.
///
/// Status: FIXED (2026-02-10) - corrected to match TSC behavior
#[test]
fn test_ts18016_private_identifier_outside_class() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Foo {
    #bar: number;
}

let f: Foo;
let x = f.#bar;  // Outside class - should error TS18013 only (not TS18016)
        ",
    );

    // Filter out TS2318 (missing global types) which are noise for this test
    let relevant_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    // Should NOT emit TS18016 for property access — TSC doesn't emit it here.
    // TS18016 is only for truly invalid positions (object literals, standalone expressions).
    assert!(
        !has_error(&relevant_diagnostics, 18016),
        "Should NOT emit TS18016 for property access outside class (TSC doesn't).\nActual errors: {relevant_diagnostics:#?}"
    );

    // Should emit TS18013 (semantic error - property not accessible)
    assert!(
        has_error(&relevant_diagnostics, 18013),
        "Should emit TS18013 for private identifier access outside class.\nActual errors: {relevant_diagnostics:#?}"
    );
}

/// Issue: TS2416 false positive for private field "overrides"
///
/// Expected: Private fields with same name in child class should NOT emit TS2416
/// Status: FIXED (2026-02-09)
///
/// Root cause: Override checking didn't skip private identifiers
/// Fix: Added check in `class_checker.rs` to skip override validation for names starting with '#'
#[test]
fn test_private_field_no_override_error() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Parent {
    #foo: number;
}

class Child extends Parent {
    #foo: string;  // Should NOT emit TS2416 - private fields don't participate in inheritance
}
        ",
    );

    // Filter out TS2318 (missing global types)
    let relevant_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    // Should NOT emit TS2416 (incompatible override) for private fields
    assert!(
        !has_error(&relevant_diagnostics, 2416),
        "Should NOT emit TS2416 for private field with same name in child class.\nActual errors: {relevant_diagnostics:#?}"
    );
}

/// TS2416 for class extending non-class (variable with constructor signature).
///
/// When a class extends a variable declared as `{ prototype: A; new(): A }`,
/// the AST-level class resolution fails (variable, not class), so the checker
/// falls back to type-level resolution. Property type compatibility must still
/// be checked against the resolved instance type.
#[test]
fn test_ts2416_type_level_base_class_property_incompatibility() {
    let diagnostics = compile_and_get_diagnostics(
        r"
interface A {
    n: number;
}
declare var A: {
    prototype: A;
    new(): A;
};

class B extends A {
    n = '';
}
        ",
    );

    let relevant_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        has_error(&relevant_diagnostics, 2416),
        "Should emit TS2416 when derived class property type is incompatible with base type.\nActual errors: {relevant_diagnostics:#?}"
    );
}

/// TS2416 alongside TS2426 when method overrides accessor with incompatible type.
///
/// tsc emits both TS2426 (kind mismatch: accessor -> method) and TS2416 (type incompatibility)
/// when a derived class method overrides a base class accessor.
#[test]
fn test_ts2416_emitted_alongside_ts2426_accessor_method_mismatch() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Base {
    get x() { return 1; }
    set x(v) {}
}

class Derived extends Base {
    x() { return 1; }
}
        ",
    );

    let relevant_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        has_error(&relevant_diagnostics, 2426),
        "Should emit TS2426 for accessor/method kind mismatch.\nActual errors: {relevant_diagnostics:#?}"
    );
    assert!(
        has_error(&relevant_diagnostics, 2416),
        "Should also emit TS2416 for type incompatibility alongside TS2426.\nActual errors: {relevant_diagnostics:#?}"
    );
}

/// Seam test: TS2430 should be reported for incompatible interface member types.
///
/// Guards `class_checker` interface-extension compatibility after relation-helper refactors.
#[test]
fn test_interface_extension_incompatible_property_reports_ts2430() {
    let diagnostics = compile_and_get_diagnostics(
        r"
interface Base {
  value: string;
}

interface Derived extends Base {
  value: number;
}
        ",
    );

    let relevant_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        has_error(&relevant_diagnostics, 2430),
        "Should emit TS2430 for incompatible interface extension member.\nActual errors: {relevant_diagnostics:#?}"
    );
}

/// Seam test: TS2367 should be reported when compared types have no overlap.
///
/// Guards overlap-check relation/query refactors used by equality comparisons.
#[test]
fn test_no_overlap_comparison_reports_ts2367() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
let x: "a" | "b" = "a";
if (x === 42) {
}
        "#,
    );

    let relevant_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        has_error(&relevant_diagnostics, 2367),
        "Should emit TS2367 for comparison of non-overlapping types.\nActual errors: {relevant_diagnostics:#?}"
    );
}

/// Issue: Computed property destructuring produces false TS2349
///
/// From: computed-property-destructuring.md
/// Expected: No TS2349 errors
/// Actual: TS2349 "This expression is not callable" errors
///
/// Root cause: Computed property name expression in destructuring binding
/// may be incorrectly treated or the type resolution fails.
#[test]
fn test_computed_property_destructuring_no_false_ts2349() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
let foo = "bar";
let {[foo]: bar} = {bar: "baz"};
        "#,
    );

    // Filter out TS2318 (missing global types)
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 2349),
        "Should NOT emit TS2349 for computed property destructuring.\nActual errors: {relevant:#?}"
    );
}

/// Issue: Contextual typing for generic function parameters
///
/// From: contextual-typing-generics.md
/// Expected: No TS7006 errors (parameter gets contextual type from generic function type)
/// Actual: TS7006 "Parameter implicitly has 'any' type"
///
/// Root cause: When a function expression/arrow is assigned to a generic function type
/// like `<T>(x: T) => void`, the parameter should get its type from contextual typing.
/// Currently, the parameter type is not inferred from the contextual type.
#[test]
fn test_contextual_typing_generic_function_param() {
    // Enable noImplicitAny to trigger TS7006
    let source = r"
// @noImplicitAny: true
const fn2: <T>(x: T) => void = function test(t) { };
    ";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    let diagnostics: Vec<(u32, String)> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    // Filter out TS2318 (missing global types)
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 7006),
        "Should NOT emit TS7006 - parameter 't' should be contextually typed as T.\nActual errors: {relevant:#?}"
    );
}

/// Issue: Contextual typing for arrow function assigned to generic type
#[test]
fn test_contextual_typing_generic_arrow_param() {
    let source = r"
// @noImplicitAny: true
declare function f(fun: <T>(t: T) => void): void;
f(t => { });
    ";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    let diagnostics: Vec<(u32, String)> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    // Filter out TS2318 (missing global types)
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 7006),
        "Should NOT emit TS7006 - parameter 't' should be contextually typed from generic.\nActual errors: {relevant:#?}"
    );
}

/// Issue: false-positive assignability errors with contextual generic outer type parameters.
///
/// Mirrors: contextualOuterTypeParameters.ts
/// Expected: no TS2322/TS2345 errors
#[test]
fn test_contextual_outer_type_parameters_no_false_assignability_errors() {
    let source = r"
declare function f(fun: <T>(t: T) => void): void

f(t => {
    type isArray = (typeof t)[] extends string[] ? true : false;
    type IsObject = { x: typeof t } extends { x: string } ? true : false;
});

const fn1: <T>(x: T) => void = t => {
    type isArray = (typeof t)[] extends string[] ? true : false;
    type IsObject = { x: typeof t } extends { x: string } ? true : false;
};

const fn2: <T>(x: T) => void = function test(t) {
    type isArray = (typeof t)[] extends string[] ? true : false;
    type IsObject = { x: typeof t } extends { x: string } ? true : false;
};
";

    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(source, options);

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 2322),
        "Should NOT emit TS2322 for contextual generic outer type parameters.\nActual errors: {relevant:#?}"
    );
    assert!(
        !has_error(&relevant, 2345),
        "Should NOT emit TS2345 for contextual generic outer type parameters.\nActual errors: {relevant:#?}"
    );
}

/// Issue: false-positive TS2345 in contextual signature instantiation chain.
///
/// Mirrors: contextualSignatureInstantiation2.ts
/// Expected: no TS2345
#[test]
#[ignore = "false-positive TS2345: contextual signature instantiation chain not yet supported"]
fn test_contextual_signature_instantiation_chain_no_false_ts2345() {
    let diagnostics = compile_and_get_diagnostics(
        r"
var dot: <T, S>(f: (_: T) => S) => <U>(g: (_: U) => T) => (_: U) => S;
dot = <T, S>(f: (_: T) => S) => <U>(g: (_: U) => T): (r:U) => S => (x) => f(g(x));
var id: <T>(x:T) => T;
var r23 = dot(id)(id);
        ",
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 2345),
        "Should NOT emit TS2345 for contextual signature instantiation chain.\nActual errors: {relevant:#?}"
    );
}

#[test]
fn test_settimeout_callback_assignable_to_function_union() {
    let diagnostics = compile_and_get_diagnostics(
        r"
setTimeout(() => 1, 0);
        ",
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 2345),
        "Should NOT emit TS2345 for setTimeout callback assignability.\nActual errors: {relevant:#?}"
    );
}

#[test]
fn test_typed_array_constructor_accepts_number_array() {
    let diagnostics = compile_and_get_diagnostics(
        r"
function makeTyped(obj: number[]) {
    var typedArrays = [];
    typedArrays[0] = new Int8Array(obj);
    return typedArrays;
}
        ",
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 2769),
        "Should NOT emit TS2769 for Int8Array(number[]).\nActual errors: {relevant:#?}"
    );
}

/// Regression test: TS7006 SHOULD still fire for closures without any contextual type
#[test]
fn test_ts7006_still_fires_without_contextual_type() {
    let source = r"
// @noImplicitAny: true
var f = function(x) { };
    ";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    let diagnostics: Vec<(u32, String)> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        has_error(&relevant, 7006),
        "SHOULD emit TS7006 - parameter 'x' has no contextual type.\nActual errors: {relevant:#?}"
    );
}

/// Issue: Contextual typing for mapped type generic parameters
///
/// When a generic function has a mapped type parameter like `{ [K in keyof P]: P[K] }`,
/// and P has a constraint (e.g. `P extends Props`), the lambda parameters inside the
/// object literal argument should be contextually typed from the constraint.
///
/// For example:
/// ```typescript
/// interface Props { when: (value: string) => boolean; }
/// function good2<P extends Props>(attrs: { [K in keyof P]: P[K] }) { }
/// good2({ when: value => false }); // `value` should be typed as `string`
/// ```
///
/// Root cause was two-fold:
/// 1. During two-pass generic inference, when all args are context-sensitive,
///    type parameters had no candidates. Fixed by using upper bounds (constraints)
///    in `get_current_substitution` instead of UNKNOWN.
/// 2. The instantiated mapped type contained Lazy references that the solver's
///    `NoopResolver` couldn't resolve. Fixed by evaluating the contextual type
///    with the checker's Judge (which has the full `TypeEnvironment` resolver)
///    before extracting property types.
#[test]
fn test_contextual_typing_mapped_type_generic_param() {
    let source = r"
// @noImplicitAny: true
interface Props {
    when: (value: string) => boolean;
}
function good2<P extends Props>(attrs: { [K in keyof P]: P[K] }) { }
good2({ when: value => false });
    ";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    let diagnostics: Vec<(u32, String)> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 7006),
        "Should NOT emit TS7006 - parameter 'value' should be contextually typed as string \
         from the mapped type constraint Props.\nActual errors: {relevant:#?}"
    );
}

/// Issue: TS2344 reported twice for the same type argument
///
/// When `get_type_from_type_node` re-resolves a type reference (e.g., because
/// `type_parameter_scope` changes between type environment building and statement
/// checking), `validate_type_reference_type_arguments` was called twice for the
/// same node, producing duplicate TS2344 errors.
///
/// Fix: Use `emitted_diagnostics` deduplication in `error_type_constraint_not_satisfied`
/// to prevent emitting the same TS2344 at the same source position twice.
#[test]
fn test_ts2344_no_duplicate_errors() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
interface Array<T> {}
interface Boolean {}
interface Function {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

function one<T extends string>() {}
one<number>();

function two<T extends object>() {}
two<string>();

function three<T extends { value: string }>() {}
three<number>();
        ",
        CheckerOptions {
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    // Count TS2344 errors - each should appear exactly once
    let ts2344_count = relevant.iter().filter(|(code, _)| *code == 2344).count();
    assert_eq!(
        ts2344_count, 3,
        "Should emit exactly 3 TS2344 errors (one per bad type arg), not duplicates.\nActual errors: {relevant:#?}"
    );
}

/// TS2339: Property access on `this` in static methods should use constructor type
///
/// In static methods, `this` refers to `typeof C` (the constructor type), not an
/// instance of C. Accessing instance properties on `this` in a static method should
/// emit TS2339 because instance properties don't exist on the constructor type.
#[test]
fn test_ts2339_this_in_static_method() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class C {
    public p = 0;
    static s = 0;
    static b() {
        this.p = 1; // TS2339 - 'p' is instance, doesn't exist on typeof C
        this.s = 2; // OK - 's' is static
    }
}
        ",
    );

    let ts2339_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339_errors.len(),
        1,
        "Should emit exactly 1 TS2339 for 'this.p' in static method.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        ts2339_errors[0].1.contains("'p'") || ts2339_errors[0].1.contains("\"p\""),
        "TS2339 should mention property 'p'. Got: {}",
        ts2339_errors[0].1
    );
}

#[test]
fn test_interface_accessor_declarations() {
    // Interface accessor declarations (get/set) should be recognized as properties
    let diagnostics = compile_and_get_diagnostics(
        r"
interface Test {
    get foo(): string;
    set foo(s: string | number);
}
const t = {} as Test;
let m: string = t.foo;   // OK - getter returns string
        ",
    );

    let ts2339_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339_errors.len(),
        0,
        "Interface accessors should be recognized as properties. Got TS2339 errors: {ts2339_errors:#?}"
    );
}

#[test]
fn test_type_literal_accessor_declarations() {
    // Type literal accessor declarations (get/set) should be recognized as properties
    let diagnostics = compile_and_get_diagnostics(
        r"
type Test = {
    get foo(): string;
    set foo(s: number);
};
const t = {} as Test;
let m: string = t.foo;   // OK - getter returns string
        ",
    );

    let ts2339_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339_errors.len(),
        0,
        "Type literal accessors should be recognized as properties. Got TS2339 errors: {ts2339_errors:#?}"
    );
}

/// Issue: False-positive TS2345 when interface extends another and adds call signatures
///
/// From: addMoreCallSignaturesToBaseSignature2.ts
/// Expected: No errors - `a(1)` should match inherited `(bar: number): string` signature
/// Actual: TS2345 (falsely claims argument type mismatch)
///
/// When interface Bar extends Foo (which has `(bar: number): string`),
/// and Bar adds `(key: string): string`, calling `a(1)` with a numeric
/// argument should match the inherited signature without error.
#[test]
fn test_interface_inherited_call_signature_no_false_ts2345() {
    let diagnostics = compile_and_get_diagnostics(
        r"
interface Foo {
    (bar:number): string;
}

interface Bar extends Foo {
    (key: string): string;
}

var a: Bar;
var kitty = a(1);
        ",
    );

    // Filter out TS2318 (missing global types)
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 2345),
        "Should NOT emit TS2345 - a(1) should match inherited (bar: number) => string.\nActual errors: {relevant:#?}"
    );
}

/// Issue: False-positive TS2345 with mixin pattern (class extends function return)
///
/// From: anonClassDeclarationEmitIsAnon.ts
/// Expected: No errors - `Timestamped(User)` should work as a valid base class
/// Actual: TS2345 (falsely claims User is not assignable to Constructor parameter)
///
/// The mixin pattern `function Timestamped<TBase extends Constructor>(Base: TBase)`
/// with `Constructor<T = {}> = new (...args: any[]) => T` should accept any class.
#[test]
fn test_mixin_pattern_no_false_ts2345() {
    let diagnostics = compile_and_get_diagnostics(
        r"
type Constructor<T = {}> = new (...args: any[]) => T;

function Timestamped<TBase extends Constructor>(Base: TBase) {
    return class extends Base {
        timestamp = 0;
    };
}

class User {
    name = '';
}

class TimestampedUser extends Timestamped(User) {
    constructor() {
        super();
    }
}
        ",
    );

    // Filter out TS2318 (missing global types)
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 2345),
        "Should NOT emit TS2345 - User should be assignable to Constructor<{{}}>.\nActual errors: {relevant:#?}"
    );
}

/// Issue: Contextual typing for method shorthand fails when parameter type is a union
///
/// When a function parameter is `Opts | undefined`, the contextual type should still
/// flow through to object literal method parameters. TypeScript filters out non-object
/// types from unions when computing contextual types for object literals.
#[test]
fn test_contextual_typing_union_with_undefined() {
    let opts = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
interface Opts {
    fn(x: number): void;
}

declare function a(opts: Opts | undefined): void;
a({ fn(x) {} });
        ",
        opts,
    );

    assert!(
        !has_error(&diagnostics, 7006),
        "Should NOT emit TS7006 - 'x' should be contextually typed as number from Opts.fn.\nActual errors: {diagnostics:#?}"
    );
}

/// Issue: Contextual typing for property assignment fails when parameter type is a union
#[test]
fn test_contextual_typing_property_in_union_with_null() {
    let opts = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
interface Opts {
    callback: (x: number) => void;
}

declare function b(opts: Opts | null): void;
b({ callback: (x) => {} });
        ",
        opts,
    );

    assert!(
        !has_error(&diagnostics, 7006),
        "Should NOT emit TS7006 - 'x' should be contextually typed as number from Opts.callback.\nActual errors: {diagnostics:#?}"
    );
}

// TS7022: Variable implicitly has type 'any' because it does not have a type annotation
// and is referenced directly or indirectly in its own initializer.

/// TS7022 should fire for direct self-referencing object literals under noImplicitAny.
/// From: recursiveObjectLiteral.ts
#[test]
fn test_ts7022_recursive_object_literal() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
var a = { f: a };
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7022),
        "Should emit TS7022 for self-referencing object literal.\nActual errors: {diagnostics:#?}"
    );
}

/// TS7022 should NOT fire when noImplicitAny is off (like all 7xxx diagnostics).
#[test]
fn test_ts7022_not_emitted_without_no_implicit_any() {
    let opts = CheckerOptions {
        no_implicit_any: false,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
var a = { f: a };
        ",
        opts,
    );
    assert!(
        !has_error(&diagnostics, 7022),
        "Should NOT emit TS7022 when noImplicitAny is off.\nActual errors: {diagnostics:#?}"
    );
}

/// TS7022 should NOT fire when the self-reference is in a function body (deferred context).
/// From: declFileTypeofFunction.ts
#[test]
fn test_ts7022_not_emitted_for_function_body_reference() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
var foo3 = function () {
    return foo3;
}
        ",
        opts,
    );
    assert!(
        !has_error(&diagnostics, 7022),
        "Should NOT emit TS7022 for self-reference in function body (deferred context).\nActual errors: {diagnostics:#?}"
    );
}

/// TS7022 should NOT fire for class expression initializers with method body references.
/// From: classExpression4.ts
#[test]
fn test_ts7022_not_emitted_for_class_expression_body_reference() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
let C = class {
    foo() {
        return new C();
    }
};
        ",
        opts,
    );
    assert!(
        !has_error(&diagnostics, 7022),
        "Should NOT emit TS7022 for self-reference in class method body (deferred context).\nActual errors: {diagnostics:#?}"
    );
}

/// TS7022 should NOT fire for arrow function body self-references.
/// From: simpleRecursionWithBaseCase3.ts
#[test]
fn test_ts7022_not_emitted_for_arrow_body_reference() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
const fn1 = () => {
  if (Math.random() > 0.5) {
    return fn1()
  }
  return 0
}
        ",
        opts,
    );
    assert!(
        !has_error(&diagnostics, 7022),
        "Should NOT emit TS7022 for self-reference in arrow function body (deferred context).\nActual errors: {diagnostics:#?}"
    );
}

// TS7023: Function implicitly has return type 'any' because it does not have a return
// type annotation and is referenced directly or indirectly in one of its return expressions.

/// TS7023 should fire for function expression variables that call themselves in return.
/// From: implicitAnyFromCircularInference.ts
#[test]
fn test_ts7023_function_expression_self_call() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
var f1 = function () {
    return f1();
};
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7023),
        "Should emit TS7023 for function expression self-call.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 7022),
        "Should NOT emit TS7022 for function expression (deferred context).\nActual errors: {diagnostics:#?}"
    );
}

/// TS7023 should fire for arrow function variables that call themselves in return.
/// From: implicitAnyFromCircularInference.ts
#[test]
fn test_ts7023_arrow_function_self_call() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
var f2 = () => f2();
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7023),
        "Should emit TS7023 for arrow function self-call.\nActual errors: {diagnostics:#?}"
    );
}

/// TS7023 should NOT fire when noImplicitAny is off.
#[test]
fn test_ts7023_not_emitted_without_no_implicit_any() {
    let opts = CheckerOptions {
        no_implicit_any: false,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
var f1 = function () {
    return f1();
};
        ",
        opts,
    );
    assert!(
        !has_error(&diagnostics, 7023),
        "Should NOT emit TS7023 when noImplicitAny is off.\nActual errors: {diagnostics:#?}"
    );
}

// TS7034: Variable implicitly has type 'any' in some locations where its type cannot be determined.

/// TS7034 should fire for variables without type annotation that are captured by nested functions.
/// From: implicitAnyDeclareVariablesWithoutTypeAndInit.ts
#[test]
fn test_ts7034_captured_variable_in_nested_function() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
var y;
function func(k: any) { y };
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7034),
        "Should emit TS7034 for variable captured by nested function.\nActual errors: {diagnostics:#?}"
    );
}

/// TS7034 should NOT fire for variables used only at the same scope level.
#[test]
fn test_ts7034_not_emitted_for_same_scope_usage() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
var x;
function func(k: any) {};
func(x);
        ",
        opts,
    );
    assert!(
        !has_error(&diagnostics, 7034),
        "Should NOT emit TS7034 for variable used at same scope level.\nActual errors: {diagnostics:#?}"
    );
}

/// TS7034 should NOT fire for block-scoped `let` variables, even when captured by arrow functions.
/// tsc only emits TS7034 for function-scoped `var` declarations.
/// From: controlFlowNoImplicitAny.ts (f10)
#[test]
fn test_ts7034_not_emitted_for_let_captured_by_arrow_function() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function f10() {
    let x;
    x = 'hello';
    const f = () => { x; };
}
        ",
        opts,
    );
    assert!(
        !has_error(&diagnostics, 7034),
        "Should NOT emit TS7034 for block-scoped `let` variable.\nActual errors: {diagnostics:#?}"
    );
}

/// TS7034 SHOULD fire for function-scoped `var` variables captured by arrow functions.
#[test]
fn test_ts7034_emitted_for_var_captured_by_arrow_function() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function f10() {
    var x;
    x = 'hello';
    const f = () => { x; };
}
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7034),
        "Should emit TS7034 for function-scoped `var` variable captured by arrow function.\nActual errors: {diagnostics:#?}"
    );
}

// TS2882: Cannot find module or type declarations for side-effect import

/// TS2882 should fire by default (tsc 6.0 default: noUncheckedSideEffectImports = true).
#[test]
fn test_ts2882_side_effect_import_default_on() {
    // Default CheckerOptions has no_unchecked_side_effect_imports: true (matching tsc 6.0)
    let diagnostics = compile_imports_and_get_diagnostics(
        r#"import 'nonexistent-module';"#,
        CheckerOptions::default(),
    );
    assert!(
        has_error(&diagnostics, 2882),
        "Should emit TS2882 by default (noUncheckedSideEffectImports defaults to true in tsc 6.0).\nActual errors: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2307),
        "Should NOT emit TS2307 for side-effect import (should use TS2882 instead).\nActual errors: {diagnostics:#?}"
    );
}

/// TS2882 should fire when noUncheckedSideEffectImports is explicitly true.
#[test]
fn test_ts2882_side_effect_import_option_true() {
    let opts = CheckerOptions {
        no_unchecked_side_effect_imports: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_imports_and_get_diagnostics(r#"import 'nonexistent-module';"#, opts);
    assert!(
        has_error(&diagnostics, 2882),
        "Should emit TS2882 when noUncheckedSideEffectImports is true.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2307),
        "Should NOT emit TS2307 for side-effect import (should use TS2882 instead).\nActual errors: {diagnostics:#?}"
    );
}

/// Side-effect imports should NOT emit any error when noUncheckedSideEffectImports is false.
#[test]
fn test_ts2882_side_effect_import_option_false() {
    let opts = CheckerOptions {
        no_unchecked_side_effect_imports: false,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_imports_and_get_diagnostics(r#"import 'nonexistent-module';"#, opts);
    assert!(
        !has_error(&diagnostics, 2882),
        "Should NOT emit TS2882 when noUncheckedSideEffectImports is false.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2307),
        "Should NOT emit TS2307 for side-effect import.\nActual errors: {diagnostics:#?}"
    );
}

/// Regular imports should still emit TS2307 even when noUncheckedSideEffectImports is enabled.
#[test]
fn test_ts2882_regular_import_still_emits_ts2307() {
    let diagnostics = compile_imports_and_get_diagnostics(
        r#"import { foo } from 'nonexistent-module';"#,
        CheckerOptions::default(),
    );
    assert!(
        has_error(&diagnostics, 2307) || has_error(&diagnostics, 2792),
        "Should emit TS2307 or TS2792 for regular import.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2882),
        "Should NOT emit TS2882 for regular import (only for side-effect imports).\nActual errors: {diagnostics:#?}"
    );
}

/// Node.js built-in modules should NOT trigger TS2882 when using Node module resolution.
/// TSC resolves these via @types/node; we suppress them for known builtins.
#[test]
fn test_ts2882_node_builtin_suppressed() {
    let opts = CheckerOptions {
        module: tsz_common::common::ModuleKind::Node16,
        no_unchecked_side_effect_imports: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_imports_and_get_diagnostics(r#"import "fs";"#, opts);
    assert!(
        !has_error(&diagnostics, 2882),
        "Should NOT emit TS2882 for Node.js built-in 'fs'.\nActual: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 2307),
        "Should NOT emit TS2307 for Node.js built-in 'fs'.\nActual: {diagnostics:?}"
    );
}

/// Node.js built-in modules with node: prefix should also be suppressed.
#[test]
fn test_ts2882_node_builtin_prefix_suppressed() {
    let opts = CheckerOptions {
        module: tsz_common::common::ModuleKind::Node16,
        no_unchecked_side_effect_imports: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_imports_and_get_diagnostics(r#"import "node:fs";"#, opts);
    assert!(
        !has_error(&diagnostics, 2882),
        "Should NOT emit TS2882 for Node.js built-in 'node:fs'.\nActual: {diagnostics:?}"
    );
}

// TS7051: Parameter has a name but no type. Did you mean 'arg0: string'?
// TS7006: Parameter 'x' implicitly has an 'any' type.

/// TS7051 should fire for type-keyword parameter names without type annotation.
/// From: noImplicitAnyNamelessParameter.ts
#[test]
fn test_ts7051_type_keyword_name() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function f(string, number) { }
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7051),
        "Should emit TS7051 for type-keyword parameter name.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 7006),
        "Should NOT emit TS7006 for type-keyword parameter name (should be TS7051).\nActual errors: {diagnostics:#?}"
    );
}

/// TS7051 should fire for rest parameters with type-keyword names.
/// e.g., `function f(...string)` should suggest `...args: string[]`
#[test]
fn test_ts7051_rest_type_keyword_name() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function f(...string) { }
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7051),
        "Should emit TS7051 for rest param with type-keyword name.\nActual errors: {diagnostics:#?}"
    );
}

/// TS7051 should fire for uppercase-starting parameter names.
/// e.g., `function f(MyType)` looks like a missing type annotation.
#[test]
fn test_ts7051_uppercase_name() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function f(MyType) { }
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7051),
        "Should emit TS7051 for uppercase parameter name.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 7006),
        "Should NOT emit TS7006 for uppercase parameter name (should be TS7051).\nActual errors: {diagnostics:#?}"
    );
}

/// TS7051 should NOT fire (and TS7006 SHOULD fire) for parameters with modifiers.
/// e.g., `constructor(public A)` - the modifier makes it clear A is the parameter name.
/// From: ParameterList4.ts, ParameterList5.ts, ParameterList6.ts
#[test]
fn test_ts7006_not_ts7051_with_modifier() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
class C {
    constructor(public A) { }
}
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7006),
        "Should emit TS7006 for modified parameter 'A'.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 7051),
        "Should NOT emit TS7051 when parameter has modifier.\nActual errors: {diagnostics:#?}"
    );
}

/// TS7006 should fire for lowercase parameter names without contextual type.
/// This verifies we don't regress on the basic case.
#[test]
fn test_ts7006_basic_untyped_parameter() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function f(x) { }
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7006),
        "Should emit TS7006 for untyped parameter 'x'.\nActual errors: {diagnostics:#?}"
    );
}

/// TS7006 should NOT fire when parameter has explicit type annotation.
#[test]
fn test_no_ts7006_with_type_annotation() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function f(x: number) { }
        ",
        opts,
    );
    assert!(
        !has_error(&diagnostics, 7006),
        "Should NOT emit TS7006 for typed parameter.\nActual errors: {diagnostics:#?}"
    );
}

/// TS7006 should NOT fire when noImplicitAny is disabled.
#[test]
fn test_no_ts7006_without_no_implicit_any() {
    let opts = CheckerOptions {
        no_implicit_any: false,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function f(x) { }
        ",
        opts,
    );
    assert!(
        !has_error(&diagnostics, 7006),
        "Should NOT emit TS7006 when noImplicitAny is off.\nActual errors: {diagnostics:#?}"
    );
}

/// Tagged template expressions should contextually type substitutions.
/// From: taggedTemplateContextualTyping1.ts
#[test]
fn test_tagged_template_contextual_typing() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function tag(strs: TemplateStringsArray, f: (x: number) => void) { }
tag `${ x => x }`;
        ",
        opts,
    );
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();
    assert!(
        !has_error(&relevant, 7006),
        "Should NOT emit TS7006 - 'x' should be contextually typed from tag parameter.\nActual errors: {relevant:#?}"
    );
}

/// Tagged template with generic function should infer type parameters.
/// From: taggedTemplateStringsTypeArgumentInferenceES6.ts
#[test]
fn test_tagged_template_generic_contextual_typing() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function someGenerics6<A>(strs: TemplateStringsArray, a: (a: A) => A, b: (b: A) => A, c: (c: A) => A) { }
someGenerics6 `${ (n: number) => n }${ n => n }${ n => n }`;
        ",
        opts,
    );
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();
    assert!(
        !has_error(&relevant, 7006),
        "Should NOT emit TS7006 - 'n' should be inferred as number from generic context.\nActual errors: {relevant:#?}"
    );
}

/// Test that write-only parameters are correctly flagged as unused (TS6133).
///
/// When a parameter is assigned to (`person2 = "dummy"`) but never read,
/// TS6133 should still fire. Previously, `check_const_assignment` used the
/// tracking `resolve_identifier_symbol` to look up the symbol, which added
/// the assignment target to `referenced_symbols`. This suppressed the TS6133
/// diagnostic because the unused-checker's early skip treated the symbol as
/// "used".
///
/// Fix: `get_const_variable_name` now uses the binder-level `resolve_identifier`
/// (no tracking side-effect) so assignment targets stay in `written_symbols`
/// only.
#[test]
fn test_ts6133_write_only_parameter_still_flagged() {
    let opts = CheckerOptions {
        no_unused_locals: true,
        no_unused_parameters: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function greeter(person: string, person2: string) {
    var unused = 20;
    person2 = "dummy value";
}
        "#,
        opts,
    );

    let ts6133_names: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 6133)
        .map(|(_, msg)| {
            // Extract name from "'X' is declared but its value is never read."
            msg.split('\'').nth(1).unwrap_or("?")
        })
        .collect();

    assert!(
        ts6133_names.contains(&"person"),
        "Should flag 'person' as unused. Got: {ts6133_names:?}"
    );
    assert!(
        ts6133_names.contains(&"person2"),
        "Should flag 'person2' as unused (write-only). Got: {ts6133_names:?}"
    );
    assert!(
        ts6133_names.contains(&"unused"),
        "Should flag 'unused' as unused. Got: {ts6133_names:?}"
    );
}

/// Test that const assignment detection (TS2588) still works after the
/// `resolve_identifier_symbol` → `binder.resolve_identifier` change.
#[test]
fn test_ts2588_const_assignment_still_detected() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
const x = 5;
x = 10;
        "#,
    );
    assert!(
        has_error(&diagnostics, 2588),
        "Should emit TS2588 for assignment to const. Got: {diagnostics:#?}"
    );
}

/// Test that write-only parameters with multiple params all get flagged.
#[test]
fn test_ts6133_write_only_middle_parameter() {
    let opts = CheckerOptions {
        no_unused_locals: true,
        no_unused_parameters: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function greeter(person: string, person2: string, person3: string) {
    var unused = 20;
    person2 = "dummy value";
}
        "#,
        opts,
    );

    let ts6133_names: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 6133)
        .map(|(_, msg)| msg.split('\'').nth(1).unwrap_or("?"))
        .collect();

    assert!(
        ts6133_names.contains(&"person"),
        "Should flag 'person'. Got: {ts6133_names:?}"
    );
    assert!(
        ts6133_names.contains(&"person2"),
        "Should flag 'person2' (write-only). Got: {ts6133_names:?}"
    );
    assert!(
        ts6133_names.contains(&"person3"),
        "Should flag 'person3'. Got: {ts6133_names:?}"
    );
    assert!(
        ts6133_names.contains(&"unused"),
        "Should flag 'unused'. Got: {ts6133_names:?}"
    );
}

/// Test that underscore-prefixed binding elements in destructuring are suppressed
/// but regular underscore-prefixed declarations are NOT suppressed.
/// TSC only suppresses `_`-prefixed names in destructuring patterns, not in
/// regular `let`/`const`/`var` declarations.
#[test]
fn test_ts6133_underscore_regular_declarations_still_flagged() {
    let opts = CheckerOptions {
        no_unused_locals: true,
        no_unused_parameters: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function f() {
    let _a = 1;
    let _b = "hello";
    let notUsed = 99;
    console.log("ok");
}
        "#,
        opts,
    );

    let ts6133_names: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 6133)
        .map(|(_, msg)| msg.split('\'').nth(1).unwrap_or("?"))
        .collect();

    // TSC flags regular `let _a = 1` declarations — underscore suppression
    // only applies to destructuring binding elements, not regular declarations.
    assert!(
        ts6133_names.contains(&"_a"),
        "Should flag '_a' (regular declaration, not destructuring). Got: {ts6133_names:?}"
    );
    assert!(
        ts6133_names.contains(&"_b"),
        "Should flag '_b' (regular declaration, not destructuring). Got: {ts6133_names:?}"
    );
    assert!(
        ts6133_names.contains(&"notUsed"),
        "Should flag 'notUsed'. Got: {ts6133_names:?}"
    );
}

/// Test that underscore-prefixed binding elements in destructuring are suppressed.
/// This is the main pattern seen in failing conformance tests like
/// `unusedVariablesWithUnderscoreInBindingElement.ts`.
#[test]
fn test_ts6133_underscore_destructuring_suppressed() {
    let opts = CheckerOptions {
        no_unused_locals: true,
        no_unused_parameters: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function f() {
    const [_a, b] = [1, 2];
    console.log(b);
}
        "#,
        opts,
    );

    let ts6133_names: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 6133)
        .map(|(_, msg)| msg.split('\'').nth(1).unwrap_or("?"))
        .collect();

    assert!(
        !ts6133_names.contains(&"_a"),
        "Should NOT flag '_a' in array destructuring (underscore-prefixed). Got: {ts6133_names:?}"
    );
    // `b` is used via console.log, so it shouldn't be flagged either
    assert!(
        ts6133_names.is_empty(),
        "Should have no TS6133. Got: {ts6133_names:?}"
    );
}

/// Test object destructuring with underscore-prefixed binding element.
#[test]
fn test_ts6133_underscore_object_destructuring_suppressed() {
    let opts = CheckerOptions {
        no_unused_locals: true,
        no_unused_parameters: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function f() {
    const obj = { a: 1, b: 2 };
    const { a: _a, b } = obj;
    console.log(b);
}
        "#,
        opts,
    );

    let ts6133_names: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 6133)
        .map(|(_, msg)| msg.split('\'').nth(1).unwrap_or("?"))
        .collect();

    assert!(
        !ts6133_names.contains(&"_a"),
        "Should NOT flag '_a' in object destructuring. Got: {ts6133_names:?}"
    );
}

/// Test that underscore-prefixed parameters still work (regression guard).
#[test]
fn test_ts6133_underscore_params_still_suppressed() {
    let opts = CheckerOptions {
        no_unused_locals: true,
        no_unused_parameters: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function f(_unused: string, used: string) {
    console.log(used);
}
        "#,
        opts,
    );

    let ts6133_names: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 6133)
        .map(|(_, msg)| msg.split('\'').nth(1).unwrap_or("?"))
        .collect();

    assert!(
        !ts6133_names.contains(&"_unused"),
        "Should NOT flag '_unused' parameter. Got: {ts6133_names:?}"
    );
    assert!(
        ts6133_names.is_empty(),
        "Should have no TS6133 diagnostics at all. Got: {ts6133_names:?}"
    );
}

/// Test that TS2305 diagnostic includes quoted module name matching tsc format.
/// TSC emits: Module '"./foo"' has no exported member 'Bar'.
/// (outer ' from the message template, inner " from source-level quotes)
#[test]
fn test_ts2305_module_name_includes_quotes() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
export function foo() {}
import { nonExistent } from "./thisModule";
        "#,
    );

    let ts2305_msgs: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2305 || *code == 2307)
        .map(|(_, msg)| msg.as_str())
        .collect();

    // If TS2305 is emitted, verify it includes quoted module name
    for msg in &ts2305_msgs {
        if msg.contains("has no exported member") {
            assert!(
                msg.contains("\"./thisModule\""),
                "TS2305 should include quoted module name. Got: {msg}"
            );
        }
    }
}

/// TS2451 vs TS2300: when `let` appears before `var` for the same name, tsc emits TS2451
/// ("Cannot redeclare block-scoped variable") rather than TS2300 ("Duplicate identifier").
/// The distinction depends on which declaration appears first in source order.
///
/// Regression test: the binder's declaration vector can be reordered by var hoisting,
/// so we must use source position to determine the first declaration.
#[test]
fn test_ts2451_let_before_var_emits_block_scoped_error() {
    let diagnostics = compile_and_get_diagnostics(
        r"
let x = 1;
var x = 2;
",
    );

    // Filter to only duplicate-identifier-family codes (ignore TS2318 from missing libs)
    let codes: Vec<u32> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2451 || *code == 2300)
        .map(|(code, _)| *code)
        .collect();
    // Both declarations should get TS2451 (block-scoped redeclaration)
    assert!(
        codes.iter().all(|&c| c == 2451),
        "Expected all TS2451, got codes: {codes:?}"
    );
    assert!(
        codes.len() == 2,
        "Expected 2 diagnostics (one per declaration), got {}",
        codes.len()
    );
}

/// TS2300: when `var` appears before `let` for the same name, tsc emits TS2300
/// ("Duplicate identifier") since the first declaration is function-scoped.
#[test]
fn test_ts2300_var_before_let_emits_duplicate_identifier() {
    let diagnostics = compile_and_get_diagnostics(
        r"
var x = 1;
let x = 2;
",
    );

    // Filter to only duplicate-identifier-family codes (ignore TS2318 from missing libs)
    let codes: Vec<u32> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2451 || *code == 2300)
        .map(|(code, _)| *code)
        .collect();
    // Both declarations should get TS2300 (duplicate identifier)
    assert!(
        codes.iter().all(|&c| c == 2300),
        "Expected all TS2300, got codes: {codes:?}"
    );
    assert!(
        codes.len() == 2,
        "Expected 2 diagnostics (one per declaration), got {}",
        codes.len()
    );
}

// =============================================================================
// JSX Intrinsic Element Resolution (TS2339)
// =============================================================================

#[test]
fn test_jsx_intrinsic_element_ts2339_for_unknown_tag() {
    // Mirrors tsxElementResolution1.tsx: <span /> should error when only <div> is declared
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        div: any
    }
}
<div />;
<span />;
"#;
    let diagnostics =
        compile_and_get_diagnostics_named("test.tsx", source, CheckerOptions::default());
    let ts2339_diags: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339_diags.len() == 1,
        "Expected exactly 1 TS2339 for <span />, got {}: {ts2339_diags:?}",
        ts2339_diags.len()
    );
    assert!(
        ts2339_diags[0].1.contains("span"),
        "Expected TS2339 to mention 'span', got: {}",
        ts2339_diags[0].1
    );
    assert!(
        ts2339_diags[0].1.contains("JSX.IntrinsicElements"),
        "Expected TS2339 to mention 'JSX.IntrinsicElements', got: {}",
        ts2339_diags[0].1
    );
}

#[test]
fn test_jsx_intrinsic_element_no_error_for_known_tag() {
    // Declared tags should not produce TS2339
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        div: { text?: string; };
        span: any;
    }
}
<div />;
<span />;
"#;
    let diagnostics =
        compile_and_get_diagnostics_named("test.tsx", source, CheckerOptions::default());
    let ts2339_diags: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339_diags.is_empty(),
        "Expected no TS2339 when all tags are declared, got: {ts2339_diags:?}"
    );
}

/// Template expressions in switch cases should narrow discriminated unions.
/// Before the fix, template expression case values resolved to `string` instead
/// of the literal `"cat"`, preventing discriminant narrowing and producing
/// false TS2339 errors on narrowed member accesses like `animal.meow`.
#[test]
fn test_template_expression_switch_narrows_discriminated_union() {
    let source = r#"
enum AnimalType {
  cat = "cat",
  dog = "dog",
}

type Animal =
  | { type: `${AnimalType.cat}`; meow: string; }
  | { type: `${AnimalType.dog}`; bark: string; };

function action(animal: Animal) {
  switch (animal.type) {
    case `${AnimalType.cat}`:
      console.log(animal.meow);
      break;
    case `${AnimalType.dog}`:
      console.log(animal.bark);
      break;
  }
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    let ts2339_diags: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339_diags.is_empty(),
        "Template expression switch cases should narrow discriminated unions. Got false TS2339: {ts2339_diags:?}"
    );
}

/// Template expressions with multiple substitutions should also produce
/// literal types for narrowing (e.g. `${prefix}${suffix}`).
#[test]
fn test_template_expression_multi_substitution_narrows() {
    let source = r#"
type Tag = "a-1" | "b-2";
type Item =
  | { tag: "a-1"; alpha: string; }
  | { tag: "b-2"; beta: string; };

declare const prefix: "a" | "b";

function check(item: Item) {
  if (item.tag === `a-1`) {
    const x: string = item.alpha;
  }
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    let ts2339_diags: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339_diags.is_empty(),
        "Simple template literal (no-substitution) should narrow. Got false TS2339: {ts2339_diags:?}"
    );
}

/// Exhaustiveness check: after narrowing all variants via template expression
/// switch cases, the default branch should reach `never`.
#[test]
fn test_template_expression_switch_exhaustiveness_reaches_never() {
    let source = r#"
enum Kind {
  A = "a",
  B = "b",
}

type Variant =
  | { kind: `${Kind.A}`; a: number; }
  | { kind: `${Kind.B}`; b: number; };

function check(p: never) {
  throw new Error("unreachable");
}

function process(v: Variant) {
  switch (v.kind) {
    case `${Kind.A}`:
      return v.a;
    case `${Kind.B}`:
      return v.b;
    default:
      check(v);
  }
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    // No TS2339 (member access after narrowing) and no TS2345 (v not assignable to never)
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339 || *code == 2345)
        .collect();
    assert!(
        relevant.is_empty(),
        "Template expression switch should exhaust union to never. Got: {relevant:?}"
    );
}

// ---------------------------------------------------------------------------
// Multi-file helpers for cross-file type-only export tests
// ---------------------------------------------------------------------------

/// Compile two files (a.ts and b.ts) and return diagnostics from b.ts.
/// `module_spec` is the import specifier used in b.ts to reference a.ts (e.g., "./a").
fn compile_two_files_get_diagnostics(
    a_source: &str,
    b_source: &str,
    module_spec: &str,
) -> Vec<(u32, String)> {
    let mut parser_a = ParserState::new("a.ts".to_string(), a_source.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = ParserState::new("b.ts".to_string(), b_source.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    let arena_a = Arc::new(parser_a.get_arena().clone());
    let arena_b = Arc::new(parser_b.get_arena().clone());

    let all_arenas = Arc::new(vec![Arc::clone(&arena_a), Arc::clone(&arena_b)]);

    // Merge module exports: copy a.ts exports into b.ts's binder for cross-file resolution
    let file_a_exports = binder_a.module_exports.get("a.ts").cloned();
    if let Some(exports) = &file_a_exports {
        binder_b
            .module_exports
            .insert(module_spec.to_string(), exports.clone());
    }

    // Record cross-file symbol targets: SymbolIds from binder_a need to resolve
    // in binder_a's arena, not binder_b's. Map them to file index 0 (a.ts).
    let mut cross_file_targets = FxHashMap::default();
    if let Some(exports) = &file_a_exports {
        for (_name, &sym_id) in exports.iter() {
            cross_file_targets.insert(sym_id, 0usize);
        }
    }

    let binder_a = Arc::new(binder_a);
    let binder_b = Arc::new(binder_b);
    let all_binders = Arc::new(vec![Arc::clone(&binder_a), Arc::clone(&binder_b)]);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        module: tsz_common::common::ModuleKind::CommonJS,
        no_lib: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        arena_b.as_ref(),
        binder_b.as_ref(),
        &types,
        "b.ts".to_string(),
        options,
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(1);

    // Register cross-file symbol targets so the checker looks up SymbolIds
    // from a.ts in the correct binder (file index 0).
    for (sym_id, file_idx) in &cross_file_targets {
        checker
            .ctx
            .cross_file_symbol_targets
            .borrow_mut()
            .insert(*sym_id, *file_idx);
    }

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((1, module_spec.to_string()), 0);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));

    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert(module_spec.to_string());
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(root_b);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

// ---------------------------------------------------------------------------
// Type-only export filtering: namespace import value access
// ---------------------------------------------------------------------------

/// When a module uses `export type { A }`, accessing `A` through a namespace
/// import (`import * as ns from './mod'`) in value position should produce
/// TS2339 because type-only exports are not value members of the namespace.
#[test]
fn test_type_only_export_not_accessible_as_namespace_value() {
    let a_source = r#"
class A { a!: string }
export type { A };
"#;
    let b_source = r#"
import * as types from './a';
types.A;
"#;
    let diagnostics = compile_two_files_get_diagnostics(a_source, b_source, "./a");
    // Filter out TS2318 (missing global types) since we don't load lib files in unit tests
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    let ts2339_errors: Vec<_> = relevant.iter().filter(|(code, _)| *code == 2339).collect();
    assert!(
        !ts2339_errors.is_empty(),
        "Expected TS2339 for type-only export accessed as namespace value member. Got: {relevant:?}"
    );
}

/// Multiple type-only exports should all be filtered from the namespace.
#[test]
fn test_multiple_type_only_exports_filtered_from_namespace() {
    let a_source = r#"
class A { a!: string }
class B { b!: number }
export type { A, B };
"#;
    let b_source = r#"
import * as types from './a';
types.A;
types.B;
"#;
    let diagnostics = compile_two_files_get_diagnostics(a_source, b_source, "./a");
    // Filter out TS2318 (missing global types) since we don't load lib files in unit tests
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    let ts2339_errors: Vec<_> = relevant.iter().filter(|(code, _)| *code == 2339).collect();
    assert!(
        ts2339_errors.len() >= 2,
        "Expected TS2339 for both type-only exports accessed as namespace value members. Got: {relevant:?}"
    );
}

// TS1100: eval/arguments used as function name in strict mode
#[test]
fn test_ts1100_function_named_eval_strict_mode() {
    let source = r#"
"use strict";
function eval() {}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        has_error(&diagnostics, 1100),
        "Expected TS1100 for 'function eval()' in strict mode. Got: {diagnostics:?}"
    );
}

#[test]
fn test_ts1100_function_named_arguments_strict_mode() {
    let source = r#"
"use strict";
function arguments() {}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        has_error(&diagnostics, 1100),
        "Expected TS1100 for 'function arguments()' in strict mode. Got: {diagnostics:?}"
    );
}

#[test]
fn test_ts1100_function_expression_named_eval_strict_mode() {
    let source = r#"
"use strict";
var v = function eval() {};
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        has_error(&diagnostics, 1100),
        "Expected TS1100 for function expression named 'eval' in strict mode. Got: {diagnostics:?}"
    );
}

#[test]
fn test_ts1100_eval_assignment_strict_mode() {
    let source = r#"
"use strict";
eval = 1;
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        has_error(&diagnostics, 1100),
        "Expected TS1100 for 'eval = 1' in strict mode. Got: {diagnostics:?}"
    );
}

// =========================================================================
// Iterable spread in function calls — TS2556 / TS2345
// =========================================================================

#[test]
fn test_array_spread_in_non_rest_param_emits_ts2556() {
    // Spreading a non-tuple array into a non-rest parameter must emit TS2556.
    // When TS2556 is emitted, no TS2345 should be emitted alongside it.
    let source = r#"
function foo(s: number) { }
declare var arr: number[];
foo(...arr);
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        has_error(&diagnostics, 2556),
        "Expected TS2556 for array spread to non-rest param. Got: {diagnostics:?}"
    );
    // Should NOT also emit TS2345 when TS2556 is reported
    assert!(
        !has_error(&diagnostics, 2345),
        "Should not emit TS2345 alongside TS2556. Got: {diagnostics:?}"
    );
}

#[test]
fn test_array_spread_in_rest_param_no_error() {
    // Spreading an array into a rest parameter should not emit TS2556.
    let source = r#"
function foo(...s: number[]) { }
declare var arr: number[];
foo(...arr);
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2556),
        "Should not emit TS2556 for array spread to rest param. Got: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 2345),
        "Should not emit TS2345 for compatible array spread. Got: {diagnostics:?}"
    );
}

// ========================================================================
// Reverse mapped type inference tests
// ========================================================================

#[test]
fn test_reverse_mapped_type_boxified_unbox() {
    // Core test: inferring T from Boxified<T> by reversing Box<T[P]> wrapper
    let diagnostics = compile_and_get_diagnostics(
        r#"
        type Box<T> = { value: T; }
        type Boxified<T> = { [P in keyof T]: Box<T[P]>; }
        declare function unboxify<T extends object>(obj: Boxified<T>): T;
        let b = { a: { value: 42 } as Box<number>, b: { value: "hello" } as Box<string> };
        let v = unboxify(b);
        "#,
    );
    assert!(
        !has_error(&diagnostics, 2345),
        "unboxify with Boxified<T> should not produce TS2345. Got: {diagnostics:?}"
    );
}

#[test]
fn test_reverse_mapped_type_no_regression_contravariant() {
    // Contravariant function template: { [K in keyof T]: (val: T[K]) => boolean }
    // Reverse inference should NOT fire (can't reverse through function types),
    // so this should produce no errors.
    let diagnostics = compile_and_get_diagnostics(
        r#"
        declare function conforms<T>(source: { [K in keyof T]: (val: T[K]) => boolean }): (value: T) => boolean;
        conforms({ foo: (v: string) => false });
        "#,
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "conforms with function template should not produce TS2322. Got: {diagnostics:?}"
    );
}

#[test]
fn test_reverse_mapped_type_no_regression_func_template() {
    // Mapped type with Func<T[K]> template — reverse should fail gracefully
    let diagnostics = compile_and_get_diagnostics(
        r#"
        type Func<T> = () => T;
        type Mapped<T> = { [K in keyof T]: Func<T[K]> };
        declare function reproduce<T>(options: Mapped<T>): T;
        reproduce({ name: () => { return 123 } });
        "#,
    );
    assert!(
        !has_error(&diagnostics, 2769),
        "reproduce with Func template should not produce TS2769. Got: {diagnostics:?}"
    );
}

// =============================================================================
// TS7008 — Static class member assigned in static block should not emit
// =============================================================================

#[test]
fn ts7008_static_property_assigned_in_static_block_no_error() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
        class C {
            static x;
            static {
                this.x = 1;
            }
        }
        "#,
    );
    assert!(
        !has_error(&diagnostics, 7008),
        "Static property assigned in static block should not emit TS7008. Got: {diagnostics:?}"
    );
}

#[test]
fn ts7008_static_property_assigned_before_declaration_no_error() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
        class C {
            static {
                this.x = 1;
            }
            static x;
        }
        "#,
    );
    assert!(
        !has_error(&diagnostics, 7008),
        "Static property assigned in earlier static block should not emit TS7008. Got: {diagnostics:?}"
    );
}

#[test]
fn ts7008_instance_property_without_annotation_or_initializer() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
        class C {
            x;
        }
        "#,
    );
    assert!(
        has_error(&diagnostics, 7008),
        "Instance property without annotation or initializer should emit TS7008. Got: {diagnostics:?}"
    );
}

#[test]
fn ts7008_static_property_without_assignment_in_static_block() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
        class C {
            static x;
            static {
                // no assignment to this.x
                let y = 1;
            }
        }
        "#,
    );
    assert!(
        has_error(&diagnostics, 7008),
        "Static property NOT assigned in static block should still emit TS7008. Got: {diagnostics:?}"
    );
}

// TS1479: CJS file importing ESM module
// Tests the current_is_commonjs detection logic with different file extensions.

/// Helper: compile with a custom file name and `report_unresolved_imports` enabled.
fn compile_with_file_name_and_get_diagnostics(
    file_name: &str,
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );
    checker.ctx.report_unresolved_imports = true;

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// .cts files should detect as CJS — extending the original check to also include .cjs.
/// When `file_is_esm` = Some(false), .ts files should detect as CJS.
#[test]
fn test_ts1479_cts_file_is_commonjs() {
    // A .cts file importing something — the import should be treated as CJS context.
    // Without a multi-file setup, TS1479 won't fire (needs resolved target marked ESM),
    // but we verify no crash and correct CJS classification by checking the code compiles.
    let opts = CheckerOptions {
        module: tsz_common::common::ModuleKind::Node16,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_with_file_name_and_get_diagnostics(
        "test.cts",
        r#"import { foo } from './other';"#,
        opts,
    );
    // Without multi-file resolution, we can't trigger TS1479, but we verify
    // that .cts files don't cause issues and get normal TS2307 for missing modules.
    assert!(
        has_error(&diagnostics, 2307)
            || has_error(&diagnostics, 2792)
            || has_error(&diagnostics, 2882),
        "Expected resolution error for .cts file import.\nActual: {diagnostics:?}"
    );
}

/// In single-file mode (no multi-file resolution), .js files can't trigger TS1479
/// because the import target doesn't resolve. In multi-file mode, .js files CAN
/// get TS1479 when importing .mjs targets (extension-based ESM), but NOT when
/// importing .js targets in ESM packages (package.json-based ESM).
#[test]
fn test_ts1479_js_file_single_file_no_false_positive() {
    let opts = CheckerOptions {
        module: tsz_common::common::ModuleKind::Node16,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_with_file_name_and_get_diagnostics(
        "test.js",
        r#"import { foo } from './other.mjs';"#,
        opts,
    );
    // In single-file mode, module doesn't resolve so TS1479 check isn't reached.
    // This verifies no false TS1479 from CJS detection alone.
    assert!(
        !has_error(&diagnostics, 1479),
        "Should NOT emit TS1479 in single-file mode.\nActual: {diagnostics:?}"
    );
}

/// .cjs files should NOT get TS1479 for relative imports.
/// TSC suppresses TS1479 for .cjs files importing via relative paths because
/// the imports won't be transformed to `require()` calls (already JS, not TS).
/// Non-relative (package) imports in .cjs files CAN get TS1479.
#[test]
fn test_ts1479_cjs_file_relative_import_suppressed() {
    let opts = CheckerOptions {
        module: tsz_common::common::ModuleKind::Node16,
        ..CheckerOptions::default()
    };
    // Relative import in .cjs file — should NOT emit TS1479
    let diagnostics = compile_with_file_name_and_get_diagnostics(
        "test.cjs",
        r#"import * as m from './index.mjs';"#,
        opts,
    );
    assert!(
        !has_error(&diagnostics, 1479),
        "Should NOT emit TS1479 for .cjs file with relative import.\nActual: {diagnostics:?}"
    );
}

/// TS2536 should be suppressed for deferred conditional types used as indices.
/// Example: `{ 0: X; 1: Y }[SomeConditional extends true ? 0 : 1]`
/// When the conditional can't be resolved at the generic level, TSC defers the check.
#[test]
fn test_ts2536_suppressed_for_deferred_conditional_index() {
    let code = r#"
type HasTail<T extends any[]> =
    T extends ([] | [any]) ? false : true;
type Head<T extends any[]> = T extends [any, ...any[]] ? T[0] : never;
type Tail<T extends any[]> =
    ((...t: T) => any) extends ((_: any, ...tail: infer TT) => any) ? TT : [];
type Last<T extends any[]> = {
    0: Last<Tail<T>>;
    1: Head<T>;
}[HasTail<T> extends true ? 0 : 1];
"#;
    let diagnostics = compile_and_get_diagnostics(code);
    let has_2536 = diagnostics.iter().any(|(code, _)| *code == 2536);
    assert!(
        !has_2536,
        "TS2536 should NOT be emitted for deferred conditional index types.\nActual: {diagnostics:?}"
    );
}

/// TS2536 should still be emitted for concrete invalid index types.
#[test]
fn test_ts2536_still_emitted_for_concrete_invalid_index() {
    let code = r#"
type Obj = { a: string; b: number; };
type Bad = Obj["c"];
"#;
    let diagnostics = compile_and_get_diagnostics(code);
    let has_2536 = diagnostics.iter().any(|(code, _)| *code == 2536);
    assert!(
        has_2536,
        "TS2536 should be emitted for concrete invalid index 'c'.\nActual: {diagnostics:?}"
    );
}

// =============================================================================
// Interface Merged Declaration Property-vs-Method TS2300
// =============================================================================

#[test]
fn test_ts2300_interface_property_vs_method_conflict() {
    // When merged interfaces have the same member name as both a property
    // and a method, tsc emits TS2300 "Duplicate identifier" on both.
    let diagnostics = compile_and_get_diagnostics(
        r"
interface A {
    foo: () => string;
}
interface A {
    foo(): number;
}
",
    );
    let ts2300_count = diagnostics.iter().filter(|(c, _)| *c == 2300).count();
    assert!(
        ts2300_count >= 2,
        "Expected at least 2 TS2300 for property-vs-method conflict, got {ts2300_count}.\nDiagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_no_ts2300_for_method_overloads_in_merged_interfaces() {
    // Method overloads across merged interfaces are valid and should NOT
    // produce TS2300. Multiple methods with the same name are allowed.
    let diagnostics = compile_and_get_diagnostics(
        r"
interface B {
    bar(x: number): number;
}
interface B {
    bar(x: string): string;
}
",
    );
    let ts2300_count = diagnostics.iter().filter(|(c, _)| *c == 2300).count();
    assert!(
        ts2300_count == 0,
        "Method overloads should not produce TS2300, got {ts2300_count}.\nDiagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_no_ts2304_for_method_type_params_in_merged_interface() {
    // Method signatures with their own type parameters should not cause
    // TS2304 "Cannot find name" during merged interface checking.
    let diagnostics = compile_and_get_diagnostics(
        r"
interface C<T> {
    foo(x: T): T;
}
interface C<T> {
    foo<W>(x: W, y: W): W;
}
",
    );
    let ts2304_count = diagnostics.iter().filter(|(c, _)| *c == 2304).count();
    assert!(
        ts2304_count == 0,
        "Method type params should not cause TS2304, got {ts2304_count}.\nDiagnostics: {diagnostics:?}"
    );
}

// ─── TS2427: Interface name cannot be predefined type ───

/// `interface void {}` should emit TS2427, not TS1005.
/// Previously the parser rejected `void` as a reserved word, preventing
/// the checker from emitting the correct TS2427 diagnostic.
#[test]
fn ts2427_interface_void_name() {
    let diagnostics = compile_and_get_diagnostics("interface void {}");
    assert!(
        has_error(&diagnostics, 2427),
        "Expected TS2427 for `interface void {{}}`: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 1005),
        "Should not emit TS1005 for `interface void {{}}`: {diagnostics:?}"
    );
}

/// `interface null {}` should emit TS2427.
#[test]
fn ts2427_interface_null_name() {
    let diagnostics = compile_and_get_diagnostics("interface null {}");
    assert!(
        has_error(&diagnostics, 2427),
        "Expected TS2427 for `interface null {{}}`: {diagnostics:?}"
    );
}

/// `interface string {}` should emit TS2427 for predefined type name.
#[test]
fn ts2427_interface_string_name() {
    let diagnostics = compile_and_get_diagnostics("interface string {}");
    assert!(
        has_error(&diagnostics, 2427),
        "Expected TS2427 for `interface string {{}}`: {diagnostics:?}"
    );
}

/// `interface undefined {}` should emit TS2427.
#[test]
fn ts2427_interface_undefined_name() {
    let diagnostics = compile_and_get_diagnostics("interface undefined {}");
    assert!(
        has_error(&diagnostics, 2427),
        "Expected TS2427 for `interface undefined {{}}`: {diagnostics:?}"
    );
}

/// Regular interface names should not emit TS2427.
#[test]
fn no_ts2427_for_regular_interface_name() {
    let diagnostics = compile_and_get_diagnostics("interface Foo {}");
    assert!(
        !has_error(&diagnostics, 2427),
        "Should not emit TS2427 for `interface Foo {{}}`: {diagnostics:?}"
    );
}

/// After `f ??= (a => a)`, f should be narrowed to exclude undefined.
/// The ??= creates a two-branch flow (short-circuit when non-nullish vs assignment),
/// and on the assignment branch the variable holds exactly the RHS value.
/// Regression test for false-positive TS2722.
#[test]
fn logical_nullish_assignment_narrows_out_undefined() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
function foo(f?: (a: number) => void) {
    f ??= (a => a);
    f(42);
}
"#,
    );
    assert!(
        !has_error(&diagnostics, 2722),
        "Should not emit TS2722 after f ??= ...: {diagnostics:?}"
    );
}

/// `if (x &&= y)` should narrow both x and y to truthy in the then-branch.
/// For &&=, the result is y when x was truthy, so if the if-condition is truthy
/// then y must be truthy.
#[test]
fn logical_and_assignment_condition_narrows_truthy() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface T { name: string; original?: T }
declare const v: number;
function test(thing: T | undefined, def: T | undefined) {
    if (thing &&= def) {
        thing.name;
        def.name;
    }
}
"#,
    );
    assert!(
        !has_error(&diagnostics, 18048),
        "Should not emit TS18048 inside if(thing &&= def) truthy branch: {diagnostics:?}"
    );
}

/// Test: IIFE callee gets contextual return type wrapping.
/// When a function expression is immediately invoked and the call expression
/// has a contextual type (from a variable annotation), the function expression
/// should infer its return type from the contextual type, enabling contextual
/// typing of callback parameters in the return value.
/// Without wrapping the contextual type into a callable `() => T`, the
/// function type resolver cannot extract the return type.
#[test]
fn test_iife_contextual_return_type_for_callback() {
    let options = CheckerOptions {
        no_implicit_any: true,
        strict: true,
        ..CheckerOptions::default()
    };
    // The IIFE `(() => n => n + 1)()` has contextual type `(n: number) => number`.
    // The inner arrow `n => n + 1` needs `n` contextually typed as `number`.
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
const result: (n: number) => number = (() => n => n + 1)();
"#,
        options,
    );
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();
    assert!(
        !has_error(&relevant, 7006),
        "IIFE should contextually type callback return value params. Got: {relevant:#?}"
    );
}

/// Test: Parenthesized IIFE callee also gets contextual return type.
/// Same as above but with `(function(){})()` syntax (parens around callee).
#[test]
fn test_iife_parenthesized_contextual_return_type() {
    let options = CheckerOptions {
        no_implicit_any: true,
        strict: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
const result: (n: number) => number = (function() { return function(n) { return n + 1; }; })();
"#,
        options,
    );
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();
    assert!(
        !has_error(&relevant, 7006),
        "Parenthesized IIFE should contextually type return value params. Got: {relevant:#?}"
    );
}

/// Test: IIFE with object return type provides contextual typing for nested callbacks.
#[test]
fn test_iife_contextual_return_type_object_with_callback() {
    let options = CheckerOptions {
        no_implicit_any: true,
        strict: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type Handler = { handle: (x: string) => number };
const h: Handler = (() => ({ handle: x => x.length }))();
"#,
        options,
    );
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();
    assert!(
        !has_error(&relevant, 7006),
        "IIFE returning object with callback should contextually type callback params. Got: {relevant:#?}"
    );
}

// =========================================================================
// Array spread into variadic tuple rest params — no false TS2556
// =========================================================================

#[test]
fn test_array_spread_into_variadic_tuple_rest_no_ts2556() {
    // Spreading an array into a function with variadic tuple rest parameter
    // (e.g., ...args: [...T, number]) should NOT emit TS2556.
    // The variadic_tuple_element_type function must correctly handle the
    // rest parameter probe at large indices.
    let source = r#"
declare function foo<T extends unknown[]>(x: number, ...args: [...T, number]): T;
function bar<U extends unknown[]>(u: U) {
    foo(1, ...u, 2);
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2556),
        "Should not emit TS2556 for array spread to variadic tuple rest param. Got: {diagnostics:?}"
    );
}

#[test]
fn test_array_spread_into_variadic_tuple_curry_pattern_no_ts2556() {
    // The curry pattern: spreading generic array params into a function call
    // within the body. This was a false TS2556 because the rest parameter
    // probe returned None for variadic tuple parameters.
    let source = r#"
function curry<T extends unknown[], U extends unknown[], R>(
    f: (...args: [...T, ...U]) => R, ...a: T
) {
    return (...b: U) => f(...a, ...b);
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2556),
        "Should not emit TS2556 for spread of generic arrays into variadic tuple. Got: {diagnostics:?}"
    );
}

#[test]
fn test_array_spread_into_generic_variadic_round2_no_ts2556() {
    // Generic function with context-sensitive callback arg — tests the
    // Round 2 closure correctly falls back to ctx_helper for rest param
    // probes at large indices.
    let source = r#"
declare function call<T extends unknown[], R>(
    ...args: [...T, (...args: T) => R]
): [T, R];
declare const sa: string[];
call(...sa, (...x) => 42);
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2556),
        "Should not emit TS2556 for spread+callback in generic variadic. Got: {diagnostics:?}"
    );
}

/// Return type inference should use narrowed types from type guard predicates.
/// When `isFunction(item)` narrows `item` to `Extract<T, Function>` inside an
/// if-block, the inferred return type should reflect the narrowed type, not the
/// declared parameter type `T`. Without evaluating the if-condition during
/// return type collection, flow narrowing can't find the type predicate.
#[test]
fn return_type_inference_uses_type_guard_narrowing() {
    let source = r#"
declare function isFunction<T>(value: T): value is Extract<T, Function>;

function getFunction<T>(item: T) {
    if (isFunction(item)) {
        return item;
    }
    throw new Error();
}

function f12(x: string | (() => string) | undefined) {
    const f = getFunction(x);
    f();
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2722),
        "Should not emit TS2722 for calling result of type-guard-narrowed return. Got: {diagnostics:?}"
    );
}

/// Non-generic type guard predicates should also work in return type inference.
/// User-defined type guards with non-generic predicate types should also
/// produce correct narrowing during return type inference.
#[test]
fn return_type_inference_uses_non_generic_type_guard() {
    let source = r#"
interface Callable { (): string; }
declare function isCallable(value: unknown): value is Callable;

function getCallable(item: string | Callable | undefined) {
    if (isCallable(item)) {
        return item;
    }
    throw "not callable";
}

declare const x: string | Callable | undefined;
const f = getCallable(x);
const result: string = f();
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2722),
        "Should not emit TS2722 for non-generic type guard return inference. Got: {diagnostics:?}"
    );
}

/// Switch clause narrowing must use the narrowed type from preceding control flow.
/// When `if (c !== undefined)` narrows a union, the switch default should see the
/// narrowed type (without undefined), not the original declared type.
#[test]
fn test_switch_clause_uses_narrowed_type_from_preceding_if() {
    let source = r#"
interface A { kind: 'A'; }
interface B { kind: 'B'; }
type C = A | B | undefined;
declare var c: C;
if (c !== undefined) {
    switch (c.kind) {
        case 'A': break;
        case 'B': break;
        default: let x: never = c;
    }
}
"#;
    let options = CheckerOptions {
        strict_null_checks: true,
        ..Default::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(source, options);
    assert!(
        !has_error(&diagnostics, 2322),
        "Switch default should narrow to `never` after exhaustive cases when preceded by undefined-excluding guard. Got: {diagnostics:?}"
    );
}

/// Switch clause narrowing must propagate truthiness narrowing.
/// After `if (c)` (truthy check), switch cases should see the non-falsy type.
#[test]
fn test_switch_clause_uses_truthiness_narrowing() {
    let source = r#"
interface A { kind: 'A'; }
interface B { kind: 'B'; }
type C = A | B | null | undefined;
declare var c: C;
if (c) {
    switch (c.kind) {
        case 'A': break;
        case 'B': break;
        default: let x: never = c;
    }
}
"#;
    let options = CheckerOptions {
        strict_null_checks: true,
        ..Default::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(source, options);
    assert!(
        !has_error(&diagnostics, 2322),
        "Switch default should narrow to `never` after exhaustive cases when preceded by truthiness guard. Got: {diagnostics:?}"
    );
}

#[test]
fn test_array_from_contextual_destructuring_does_not_emit_ts2339() {
    let source = r#"
interface A { a: string; }
interface B { b: string; }
declare function from<T, U>(items: Iterable<T> | ArrayLike<T>, mapfn: (value: T) => U): U[];
const inputB: B[] = [];
const result: A[] = from(inputB, ({ b }): A => ({ a: b }));
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339.is_empty(),
        "Contextual destructuring in Array.from callback should not emit TS2339. Got: {diagnostics:?}"
    );
}

/// Regression test: loop fixed-point should not leak declared type via ERROR-typed
/// back-edge assignments. When `x = len(x)` hasn't been type-checked yet during
/// loop fixed-point iteration, `node_types` returns ERROR. Since ERROR is subtype of
/// everything, `narrow_assignment` keeps all union members, incorrectly widening to
/// the full declared type. The fix filters out ERROR from `get_assigned_type` results.
///
/// Reproduces controlFlowWhileStatement.ts function h2.
#[test]
fn test_loop_fixed_point_no_false_ts2345_from_error_assigned_type() {
    let source = r#"
let cond: boolean;
declare function len(s: string | number): number;
function h2() {
    let x: string | number | boolean;
    x = "";
    while (cond) {
        x = len(x);
        x; // number
    }
    x; // string | number
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2345),
        "Loop fixed-point should not widen x to string|number|boolean via ERROR back-edge. Got: {diagnostics:?}"
    );
}

/// Regression test: loop fixed-point with function call assignment and separate
/// declaration. The call return type (number) should be used correctly in the
/// loop's fixed-point analysis, not the full declared type.
///
/// Reproduces controlFlowWhileStatement.ts function h3.
#[test]
fn test_loop_fixed_point_function_call_assignment_at_end() {
    let source = r#"
let cond: boolean;
declare function len(s: string | number): number;
function h3() {
    let x: string | number | boolean;
    x = "";
    while (cond) {
        x;           // string | number
        x = len(x);
    }
    x; // string | number
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2345),
        "Loop fixed-point with call assignment at end should not widen via ERROR type. Got: {diagnostics:?}"
    );
}

/// Boolean literal discriminant narrowing: `x.kind === false` should narrow via
/// discriminant comparison (checking `false <: prop_type`), not truthiness narrowing.
///
/// Previously, `narrow_by_boolean_comparison` intercepted `x.kind === false` and
/// treated it as a truthiness check on `x.kind`, which kept `{ kind: string }` in
/// the narrowed type (since strings can be falsy). The fix ensures property access
/// comparisons with boolean literals fall through to discriminant narrowing.
///
/// Reproduces discriminatedUnionTypes2.ts function f10.
#[test]
fn test_boolean_discriminant_narrowing_false() {
    let source = r#"
function f10(x: { kind: false, a: string } | { kind: true, b: string } | { kind: string, c: string }) {
    if (x.kind === false) {
        x.a;
    }
    else if (x.kind === true) {
        x.b;
    }
    else {
        x.c;
    }
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2339),
        "Boolean literal discriminant narrowing should filter union members by discriminant subtyping, not truthiness. Got: {diagnostics:?}"
    );
}

/// Boolean literal discriminant narrowing with switch statement.
/// `switch (x.kind) { case false: ... }` should also narrow via discriminant.
///
/// Reproduces discriminatedUnionTypes2.ts function f11.
#[test]
fn test_boolean_discriminant_narrowing_switch() {
    let source = r#"
function f11(x: { kind: false, a: string } | { kind: true, b: string } | { kind: string, c: string }) {
    switch (x.kind) {
        case false:
            x.a;
            break;
        case true:
            x.b;
            break;
        default:
            x.c;
    }
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2339),
        "Boolean discriminant narrowing via switch should work like if/else. Got: {diagnostics:?}"
    );
}

/// Ensure `instanceof === false` still works via boolean comparison handler.
/// This pattern should NOT be intercepted by the discriminant path guard,
/// because the `guard_expr` (`x instanceof Error`) is a binary expression, not
/// a property access.
#[test]
fn test_instanceof_false_still_narrows() {
    let source = r#"
function test(x: string | Error) {
    if (x instanceof Error === false) {
        const s: string = x;
    }
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2322),
        "instanceof === false should still narrow via boolean comparison. Got: {diagnostics:?}"
    );
}

/// TS2344: Type parameter constraint checking when type arg is itself a type parameter.
///
/// When a type parameter `U extends number` is passed to a generic that requires
/// `T extends string`, tsc resolves `U`'s base constraint to `number` and checks
/// `number <: string`, emitting TS2344 when it fails.
///
/// Previously, `validate_type_args_against_params` unconditionally skipped constraint
/// checking when the type argument contained type parameters (via `contains_type_parameters`).
/// Now it resolves bare type parameters to their base constraints and checks assignability.
#[test]
fn test_ts2344_type_param_constraint_mismatch() {
    // Case 1: Incompatible primitive constraints → should emit TS2344
    let diagnostics = compile_and_get_diagnostics(
        r"
interface Array<T> {}
interface Boolean {}
interface Function {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

type Foo<T extends string> = T;
type Bar<U extends number> = Foo<U>;
        ",
    );
    assert!(
        has_error(&diagnostics, 2344),
        "Should emit TS2344 when `U extends number` is used where `T extends string` is required.\nActual: {diagnostics:?}"
    );
}

#[test]
fn test_ts2344_type_param_object_constraint_mismatch() {
    // Case 2: Incompatible object constraints → should emit TS2344
    let diagnostics = compile_and_get_diagnostics(
        r"
interface Array<T> {}
interface Boolean {}
interface Function {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

type Inner<C extends { props: any }> = C;
type Outer<WithC extends { name: string }> = Inner<WithC>;
        ",
    );
    assert!(
        has_error(&diagnostics, 2344),
        "Should emit TS2344 when `WithC extends {{ name: string }}` doesn't satisfy `{{ props: any }}`.\nActual: {diagnostics:?}"
    );
}

#[test]
fn test_ts2344_type_param_compatible_constraint() {
    // Case 3: Compatible constraints → should NOT emit TS2344
    let diagnostics = compile_and_get_diagnostics(
        r"
interface Array<T> {}
interface Boolean {}
interface Function {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

type Foo<T extends string> = T;
type Bar<U extends string> = Foo<U>;
        ",
    );
    assert!(
        !has_error(&diagnostics, 2344),
        "Should NOT emit TS2344 when `U extends string` satisfies `T extends string`.\nActual: {diagnostics:?}"
    );
}

#[test]
fn test_ts2344_no_false_positive_in_conditional_type_branch() {
    // Case 4: Union-constrained type param in conditional type true branch.
    // tsc narrows `TRec` to `MyRecord` in the true branch, so
    // `MySet<TRec>` is valid. We skip union-constrained type params
    // to avoid false positives.
    let diagnostics = compile_and_get_diagnostics(
        r"
interface Array<T> {}
interface Boolean {}
interface Function {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

declare class MyRecord {}
declare class MySet<TSet extends MyRecord> {}

type DS<TRec extends MyRecord | { [key: string]: unknown }> =
    TRec extends MyRecord ? MySet<TRec> : TRec[];
        ",
    );
    assert!(
        !has_error(&diagnostics, 2344),
        "Should NOT emit TS2344 for union-constrained type param in conditional type true branch.\nActual: {diagnostics:?}"
    );
}

/// Issue: instanceof narrowing uses structural subtyping instead of nominal class identity.
///
/// When class A has only optional properties, `is_assignable_to(B, A)` returns true
/// structurally even though B is an unrelated class. This causes instanceof narrowing
/// to keep B in the true branch and exclude it from the false branch incorrectly.
///
/// Status: FIXED (2026-03-03)
#[test]
fn test_instanceof_narrowing_nominal_class_identity() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r"
class A { a?: string; }
class B { b: number = 0; }
function test(x: A | B) {
    if (x instanceof A) {
        x.a;  // OK: x is A
    } else {
        x.b;  // OK: x is B
    }
}
        ",
    );
    assert!(
        !has_error(&diagnostics, 2339),
        "Instanceof narrowing should use nominal identity for classes.\n\
         True branch should be A, false branch should be B.\n\
         Actual errors: {diagnostics:?}"
    );
}

/// Instanceof narrowing with inheritance: subclass should survive true branch.
#[test]
fn test_instanceof_narrowing_with_class_hierarchy() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r"
class Animal { name?: string; }
class Dog extends Animal { bark(): void {} }
class Cat extends Animal { meow(): void {} }
function test(x: Dog | Cat) {
    if (x instanceof Animal) {
        x;  // Dog | Cat (both extend Animal)
    }
    if (x instanceof Dog) {
        x.bark();  // OK: x is Dog
    } else {
        x.meow();  // OK: x is Cat
    }
}
        ",
    );
    assert!(
        !has_error(&diagnostics, 2339),
        "Instanceof narrowing with class hierarchy should work nominally.\n\
         Actual errors: {diagnostics:?}"
    );
}

/// TS18013 should report the declaring class name, not the object type's class name.
/// When `#prop` is declared in `Base` and accessed via `Derived`, the error message
/// should say "outside class 'Base'", not "outside class 'Derived'".
#[test]
fn test_ts18013_reports_declaring_class_name() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Base {
    #prop: number = 123;
    static method(x: Derived) {
        console.log(x.#prop);
    }
}
class Derived extends Base {
    static method(x: Derived) {
        console.log(x.#prop);
    }
}
        "#,
    );

    let ts18013_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 18013)
        .map(|(_, m)| m.as_str())
        .collect();

    assert_eq!(
        ts18013_messages.len(),
        1,
        "Should emit exactly one TS18013.\nActual errors: {diagnostics:?}"
    );
    assert!(
        ts18013_messages[0].contains("'Base'"),
        "TS18013 should reference the declaring class 'Base', not 'Derived'.\n\
         Actual message: {}",
        ts18013_messages[0]
    );
}

/// TS2416 base type name should include type arguments from the extends clause,
/// not the generic parameter names. E.g., `Base<{ bar: string; }>` instead of `Base<T>`.
#[test]
fn test_ts2416_base_type_name_includes_type_arguments() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Base<T> { foo: T; }
class Derived2 extends Base<{ bar: string; }> {
    foo: { bar?: string; }
}
        "#,
    );

    let ts2416_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2416)
        .map(|(_, m)| m.as_str())
        .collect();

    assert!(
        !ts2416_messages.is_empty(),
        "Should emit TS2416 for incompatible property type.\nActual errors: {diagnostics:?}"
    );
    assert!(
        ts2416_messages[0].contains("Base<{ bar: string; }>"),
        "TS2416 should show instantiated base type 'Base<{{ bar: string; }}>', not 'Base<T>'.\n\
         Actual message: {}",
        ts2416_messages[0]
    );
}

/// Verify that private name access works correctly for instance members accessed
/// via parameters typed as the same class (e.g., `a.#x` where `a: A` inside class A).
///
/// Previously, `resolve_lazy_class_to_constructor` was incorrectly converting the
/// parameter type to a constructor type (typeof A), causing TS2339 false positives.
#[test]
fn test_private_name_instance_access_via_parameter() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class A {
    #x = 1;
    test(a: A) {
        a.#x;
    }
}
class B {
    #y() { return 1; };
    test(b: B) {
        b.#y;
    }
}
        "#,
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Should NOT emit TS2339 for private member access within the declaring class.\n\
         Private fields/methods accessed via a parameter of the same class type should be valid.\n\
         Got: {ts2339:?}"
    );
}

/// Verify that shadowed private names in nested classes produce TS18014 without
/// spurious TS2339 for valid access on the inner class.
#[test]
fn test_private_name_nested_class_shadowing() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Base {
    #x() { };
    constructor() {
        class Derived {
            #x() { };
            testBase(x: Base) {
                console.log(x.#x);
            }
            testDerived(x: Derived) {
                console.log(x.#x);
            }
        }
    }
}
        "#,
    );

    let ts18014: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 18014).collect();
    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();

    assert!(
        !ts18014.is_empty(),
        "Should emit TS18014 for shadowed private name access (x.#x where x: Base).\n\
         Actual errors: {diagnostics:?}"
    );
    assert!(
        ts2339.is_empty(),
        "Should NOT emit TS2339 alongside TS18014 for shadowed private names.\n\
         Derived.testDerived accessing x.#x (x: Derived) should be valid.\n\
         Got: {ts2339:?}"
    );
}
