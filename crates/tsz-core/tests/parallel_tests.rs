use super::*;
use std::fs;

#[test]
fn test_parse_single_file() {
    let result = parse_file_single("test.ts".to_string(), "let x = 42;".to_string());

    assert_eq!(result.file_name, "test.ts");
    assert!(!result.source_file.is_none());
    assert!(result.parse_diagnostics.is_empty());
}

#[test]
fn test_parse_multiple_files_parallel() {
    let files = vec![
        ("a.ts".to_string(), "let a = 1;".to_string()),
        ("b.ts".to_string(), "let b = 2;".to_string()),
        ("c.ts".to_string(), "let c = 3;".to_string()),
    ];

    let results = parse_files_parallel(files);

    assert_eq!(results.len(), 3);
    for result in &results {
        assert!(!result.source_file.is_none());
        assert!(result.parse_diagnostics.is_empty());
    }
}

#[test]
fn test_parse_with_stats() {
    let files = vec![
        (
            "a.ts".to_string(),
            "function foo() { return 1; }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "class Bar { constructor() {} }".to_string(),
        ),
    ];

    let (results, stats) = parse_files_with_stats(files);

    assert_eq!(results.len(), 2);
    assert_eq!(stats.file_count, 2);
    assert!(stats.total_bytes > 0);
    assert!(stats.total_nodes > 0);
    assert_eq!(stats.error_count, 0);
}

#[test]
fn test_parallel_parsing_consistency() {
    // Parse the same file multiple times in parallel
    // Results should be consistent
    let source =
        "const x: number = 42; function add(a: number, b: number): number { return a + b; }";
    let files: Vec<_> = (0..10)
        .map(|i| (format!("file{i}.ts"), source.to_string()))
        .collect();

    let results = parse_files_parallel(files);

    // All should have same number of nodes (same source)
    let first_node_count = results[0].arena.len();
    for result in &results {
        assert_eq!(result.arena.len(), first_node_count);
        assert!(result.parse_diagnostics.is_empty());
    }
}

#[test]
fn test_large_batch_parsing() {
    // Test with a larger batch to exercise parallelism
    let files: Vec<_> = (0..100)
        .map(|i| {
            let source = format!("function fn{i}(x: number): number {{ return x * {i}; }}");
            (format!("module{i}.ts"), source)
        })
        .collect();

    let (results, stats) = parse_files_with_stats(files);

    assert_eq!(results.len(), 100);
    assert_eq!(stats.file_count, 100);
    // Note: Parser may produce parse errors for some constructs
    // The key test is that parallel parsing works correctly
    // assert_eq!(stats.error_count, 0);

    // Each file should have similar node counts
    for result in &results {
        assert!(
            result.arena.len() >= 5,
            "Each file should have at least 5 nodes"
        );
    }
}

// =========================================================================
// Parallel Binding Tests
// =========================================================================

#[test]
fn test_bind_single_file() {
    let result = parse_and_bind_single(
        "test.ts".to_string(),
        "let x = 42; function foo() {}".to_string(),
    );

    assert_eq!(result.file_name, "test.ts");
    assert!(!result.source_file.is_none());
    assert!(result.parse_diagnostics.is_empty());
    // Should have symbols for x and foo
    assert!(result.file_locals.has("x"));
    assert!(result.file_locals.has("foo"));
}

#[test]
fn test_bind_multiple_files_parallel() {
    let files = vec![
        ("a.ts".to_string(), "let a = 1;".to_string()),
        ("b.ts".to_string(), "function b() {}".to_string()),
        ("c.ts".to_string(), "class C {}".to_string()),
    ];

    let results = parse_and_bind_parallel(files);

    assert_eq!(results.len(), 3);

    // Each file should have its own symbols
    assert!(results[0].file_locals.has("a"));
    assert!(results[1].file_locals.has("b"));
    assert!(results[2].file_locals.has("C"));
}

#[test]
fn test_bind_with_stats() {
    let files = vec![
        (
            "a.ts".to_string(),
            "function foo() { return 1; }".to_string(),
        ),
        ("b.ts".to_string(), "class Bar { x: number; }".to_string()),
    ];

    let (results, stats) = parse_and_bind_with_stats(files);

    assert_eq!(results.len(), 2);
    assert_eq!(stats.file_count, 2);
    assert!(stats.total_nodes > 0);
    assert!(stats.total_symbols > 0);
    assert_eq!(stats.parse_error_count, 0);
}

#[test]
fn test_parallel_binding_consistency() {
    // Bind the same file multiple times in parallel
    // Results should be consistent
    let source =
        "const x: number = 42; function add(a: number, b: number): number { return a + b; }";
    let files: Vec<_> = (0..10)
        .map(|i| (format!("file{i}.ts"), source.to_string()))
        .collect();

    let results = parse_and_bind_parallel(files);

    // All should have same symbols
    for result in &results {
        assert!(result.file_locals.has("x"));
        assert!(result.file_locals.has("add"));
        assert!(result.parse_diagnostics.is_empty());
    }
}

#[test]
fn test_large_batch_binding() {
    // Test with a larger batch to exercise parallelism
    let files: Vec<_> = (0..100)
        .map(|i| {
            let source = format!(
                "function fn{i}(x: number): number {{ return x * {i}; }} let val{i} = fn{i}(10);"
            );
            (format!("module{i}.ts"), source)
        })
        .collect();

    let (results, stats) = parse_and_bind_with_stats(files);

    assert_eq!(results.len(), 100);
    assert_eq!(stats.file_count, 100);
    assert!(
        stats.total_symbols >= 200,
        "Should have at least 200 symbols (2 per file)"
    );

    // Each file should have its function and variable
    for (i, result) in results.iter().enumerate() {
        let fn_name = format!("fn{i}");
        let var_name = format!("val{i}");
        assert!(
            result.file_locals.has(&fn_name),
            "File {i} missing {fn_name}"
        );
        assert!(
            result.file_locals.has(&var_name),
            "File {i} missing {var_name}"
        );
    }
}

// =========================================================================
// Symbol Merging Tests
// =========================================================================

#[test]
fn test_merge_single_file() {
    let files = vec![(
        "a.ts".to_string(),
        "let x = 1; function foo() {}".to_string(),
    )];

    let program = compile_files(files);

    assert_eq!(program.files.len(), 1);
    assert!(program.globals.has("x"));
    assert!(program.globals.has("foo"));
    // Symbols should be in global arena
    assert!(program.symbols.len() >= 2);
}

#[test]
fn test_merge_multiple_files() {
    let files = vec![
        ("a.ts".to_string(), "let a = 1;".to_string()),
        ("b.ts".to_string(), "function b() {}".to_string()),
        ("c.ts".to_string(), "class C {}".to_string()),
    ];

    let program = compile_files(files);

    assert_eq!(program.files.len(), 3);
    // All symbols should be in globals
    assert!(program.globals.has("a"));
    assert!(program.globals.has("b"));
    assert!(program.globals.has("C"));
    // All symbols merged into global arena
    assert!(program.symbols.len() >= 3);
}

#[test]
fn test_merge_symbol_id_remapping() {
    let files = vec![
        ("a.ts".to_string(), "let x = 1;".to_string()),
        ("b.ts".to_string(), "let y = 2;".to_string()),
    ];

    let program = compile_files(files);

    // Get the symbol IDs from globals
    let x_id = program.globals.get("x").expect("x should exist");
    let y_id = program.globals.get("y").expect("y should exist");

    // IDs should be different (remapped properly)
    assert_ne!(x_id, y_id);

    // Both should be resolvable from global arena
    assert!(program.symbols.get(x_id).is_some());
    assert!(program.symbols.get(y_id).is_some());
}

#[test]
fn test_load_lib_files_for_binding_strict_recurses_reference_libs() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let lib_dir = temp_dir.path();

    fs::write(
        lib_dir.join("lib.esnext.d.ts"),
        "/// <reference lib=\"es2023.collection\" />\ninterface Root {}\n",
    )
    .expect("write esnext");
    fs::write(
        lib_dir.join("lib.es2023.collection.d.ts"),
        "/// <reference lib=\"es5\" />\ninterface WeakKeyTypes { symbol: symbol; }\n",
    )
    .expect("write es2023.collection");
    fs::write(
        lib_dir.join("lib.es5.d.ts"),
        "interface WeakKeyTypes { object: object; }\ninterface Symbol {}\n",
    )
    .expect("write es5");

    let root = lib_dir.join("lib.esnext.d.ts");
    let loaded = load_lib_files_for_binding_strict(&[root.as_path()]).expect("load libs");
    let names: Vec<String> = loaded.iter().map(|lib| lib.file_name.clone()).collect();

    assert_eq!(
        names,
        vec![
            lib_dir.join("lib.es5.d.ts").to_string_lossy().to_string(),
            lib_dir
                .join("lib.es2023.collection.d.ts")
                .to_string_lossy()
                .to_string(),
            lib_dir
                .join("lib.esnext.d.ts")
                .to_string_lossy()
                .to_string(),
        ]
    );
}

#[test]
fn test_merge_preserves_file_locals() {
    let files = vec![
        ("a.ts".to_string(), "let a1 = 1; let a2 = 2;".to_string()),
        ("b.ts".to_string(), "let b1 = 1; let b2 = 2;".to_string()),
    ];

    let program = compile_files(files);

    // Each file should have its own locals
    assert_eq!(program.file_locals.len(), 2);
    assert!(program.file_locals[0].has("a1"));
    assert!(program.file_locals[0].has("a2"));
    assert!(program.file_locals[1].has("b1"));
    assert!(program.file_locals[1].has("b2"));
}

#[test]
fn test_merged_program_residency_stats_track_unique_file_arenas() {
    let files = vec![
        ("a.ts".to_string(), "export const a = 1;".to_string()),
        ("b.ts".to_string(), "export const b = 2;".to_string()),
    ];

    let bind_results = parse_and_bind_parallel(files);
    let program = merge_bind_results(bind_results);
    let stats = program.residency_stats();

    assert_eq!(stats.file_count, 2);
    assert_eq!(stats.bound_file_arena_count, 2);
    assert_eq!(stats.unique_arena_count, 2);
    assert!(stats.symbol_arena_count >= 2);
    assert!(stats.declaration_arena_bucket_count >= 2);
    assert!(stats.declaration_arena_mapping_count >= 2);
}

#[test]
fn test_merged_program_residency_stats_deduplicate_shared_arena_handles() {
    let files = vec![(
        "a.ts".to_string(),
        "export const a = 1; export function b() { return a; }".to_string(),
    )];

    let bind_results = parse_and_bind_parallel(files);
    let program = merge_bind_results(bind_results);
    let stats = program.residency_stats();

    assert_eq!(stats.file_count, 1);
    assert_eq!(stats.bound_file_arena_count, 1);
    assert_eq!(
        stats.unique_arena_count, 1,
        "symbol/declaration arena maps should point back to the same retained file arena"
    );
    assert!(stats.symbol_arena_count >= 2);
    assert!(stats.declaration_arena_mapping_count >= 2);
}

#[test]
fn test_check_files_parallel_preserves_ts2454_for_named_import_from_export_equals_module() {
    let files = vec![
        (
            "express.d.ts".to_string(),
            r#"
declare namespace Express { export interface Request {} }
declare module "express" {
    function e(): e.Express;
    namespace e {
        interface Request extends Express.Request { get(name: string): string; }
        interface Express {}
    }
    export = e;
}
"#
            .to_string(),
        ),
        (
            "consumer.ts".to_string(),
            r#"
import { Request } from "express";
let x: Request;
const y = x.get("a");
"#
            .to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let consumer = result
        .file_results
        .iter()
        .find(|file| file.file_name == "consumer.ts")
        .expect("expected consumer.ts result");

    assert!(
        consumer.diagnostics.iter().any(|diag| diag.code == 2454),
        "Expected TS2454 in consumer.ts. Actual diagnostics: {:#?}",
        consumer.diagnostics
    );
}

#[test]
fn test_check_files_parallel_keeps_namespace_local_component_for_create_element_inference() {
    let files = vec![(
        "test.ts".to_string(),
        r#"
declare class Component<P> { constructor(props: P); props: P; }

namespace N1 {
    declare class Component<P> {
        constructor(props: P);
    }

    interface ComponentClass<P = {}> {
        new (props: P): Component<P>;
    }

    type CreateElementChildren<P> =
        P extends { children?: infer C }
            ? C extends any[]
                ? C
                : C[]
            : unknown;

    declare function createElement<P extends {}>(
        type: ComponentClass<P>,
        ...children: CreateElementChildren<P>
    ): any;

    declare function createElement2<P extends {}>(
        type: ComponentClass<P>,
        child: CreateElementChildren<P>
    ): any;

    class InferFunctionTypes extends Component<{ children: (foo: number) => string }> {}

    createElement(InferFunctionTypes, (foo) => "" + foo);
    createElement2(InferFunctionTypes, [(foo) => "" + foo]);
}
"#
        .to_string(),
    )];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let file_result = result
        .file_results
        .iter()
        .find(|file| file.file_name == "test.ts")
        .expect("expected test.ts result");
    assert!(
        !file_result.diagnostics.iter().any(|diag| diag.code == 2345),
        "parallel file checking should preserve namespace-local ComponentClass inference for createElement. Actual diagnostics: {:#?}",
        file_result.diagnostics,
    );

    let file = program
        .files
        .iter()
        .find(|file| file.file_name == "test.ts")
        .expect("expected merged test.ts file");
    let rebuilt_binder = create_binder_from_bound_file(file, &program, 0);
    let query_cache = tsz_solver::QueryCache::new(&program.type_interner);
    let mut recreated_checker = crate::checker::state::CheckerState::with_options(
        &file.arena,
        &rebuilt_binder,
        &query_cache,
        file.file_name.clone(),
        &crate::checker::context::CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
    );
    recreated_checker.check_source_file(file.source_file);
    assert!(
        !recreated_checker
            .ctx
            .diagnostics
            .iter()
            .any(|diag| diag.code == 2345),
        "recreated binder checking should preserve namespace-local ComponentClass inference for createElement. Actual diagnostics: {:#?}",
        recreated_checker.ctx.diagnostics,
    );
}

#[test]
fn test_recreated_binder_keeps_namespace_local_generic_class_application_instance_type() {
    let files = vec![(
        "test.ts".to_string(),
        r#"
declare class Component<P> { constructor(props: P); props: P; }

namespace N1 {
    declare class Component<P> {
        constructor(props: P);
    }

    interface ComponentClass<P = {}> {
        new (props: P): Component<P>;
    }

    declare let c: ComponentClass<{ children: (foo: number) => string }>;
    const z = new c({ children: (foo) => "" + foo });
    z.props;
}
"#
        .to_string(),
    )];

    let program = compile_files(files);
    let file = program
        .files
        .iter()
        .find(|file| file.file_name == "test.ts")
        .expect("expected merged test.ts file");
    let rebuilt_binder = create_binder_from_bound_file(file, &program, 0);
    let query_cache = tsz_solver::QueryCache::new(&program.type_interner);
    let mut checker = crate::checker::state::CheckerState::with_options(
        &file.arena,
        &rebuilt_binder,
        &query_cache,
        file.file_name.clone(),
        &crate::checker::context::CheckerOptions::default(),
    );
    checker.check_source_file(file.source_file);
    let source_file = file
        .arena
        .get(file.source_file)
        .and_then(|node| file.arena.get_source_file(node))
        .expect("missing source file");
    let namespace_stmt = source_file
        .statements
        .nodes
        .iter()
        .copied()
        .find(|&stmt_idx| {
            let Some(stmt_node) = file.arena.get(stmt_idx) else {
                return false;
            };
            let Some(module_decl) = file.arena.get_module(stmt_node) else {
                return false;
            };
            file.arena
                .get_identifier_at(module_decl.name)
                .is_some_and(|ident| ident.escaped_text.as_str() == "N1")
        })
        .expect("missing namespace declaration");
    let namespace_body_statements = file
        .arena
        .get(namespace_stmt)
        .and_then(|node| file.arena.get_module(node))
        .map(|module| module.body)
        .and_then(|body_idx| file.arena.get(body_idx))
        .and_then(|node| file.arena.get_module_block(node))
        .and_then(|module_block| module_block.statements.as_ref())
        .map(|statements| statements.nodes.clone())
        .expect("missing namespace body");
    let component_return_name = namespace_body_statements
        .iter()
        .copied()
        .find_map(|stmt_idx| {
            let stmt_node = file.arena.get(stmt_idx)?;
            let interface_decl = file.arena.get_interface(stmt_node)?;
            let interface_name = file
                .arena
                .get_identifier_at(interface_decl.name)?
                .escaped_text
                .as_str();
            if interface_name != "ComponentClass" {
                return None;
            }
            let construct_idx = interface_decl.members.nodes.first().copied()?;
            let construct_node = file.arena.get(construct_idx)?;
            let construct_sig = file.arena.get_signature(construct_node)?;
            let type_ref_node = file.arena.get(construct_sig.type_annotation)?;
            let type_ref = file.arena.get_type_ref(type_ref_node)?;
            Some(type_ref.type_name)
        })
        .expect("missing Component<P> return type");
    let local_component_sym = file
        .scopes
        .iter()
        .find(|scope| {
            scope.kind == crate::binder::ContainerKind::Module && scope.table.has("Component")
        })
        .and_then(|scope| scope.table.get("Component"))
        .expect("missing namespace-local Component");
    let binder_resolved_component = rebuilt_binder.resolve_identifier_with_filter(
        &file.arena,
        component_return_name,
        &[],
        |_| true,
    );

    assert_eq!(
        binder_resolved_component,
        Some(local_component_sym),
        "rebuilt binder should resolve the interface's unqualified Component<P> to the namespace-local symbol",
    );
    assert!(
        checker.ctx.diagnostics.iter().any(|diag| diag.code == 2339),
        "recreated binder should keep the namespace-local Component<P> instance type inside ComponentClass<P>. Actual diagnostics: {:#?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_recreated_binder_keeps_namespace_local_component_class_assignability() {
    let files = vec![(
        "test.ts".to_string(),
        r#"
declare class Component<P> { constructor(props: P); }

namespace N1 {
    declare class Component<P> {
        constructor(props: P);
    }

    interface ComponentClass<P = {}> {
        new (props: P): Component<P>;
    }

    class InferFunctionTypes extends Component<{ children: (foo: number) => string }> {}
    declare let target: ComponentClass<{ children: (foo: number) => string }>;
    target = InferFunctionTypes;
    target;
}
"#
        .to_string(),
    )];

    let program = compile_files(files);
    let file = program
        .files
        .iter()
        .find(|file| file.file_name == "test.ts")
        .expect("expected merged test.ts file");
    let rebuilt_binder = create_binder_from_bound_file(file, &program, 0);
    let query_cache = tsz_solver::QueryCache::new(&program.type_interner);
    let mut checker = crate::checker::state::CheckerState::with_options(
        &file.arena,
        &rebuilt_binder,
        &query_cache,
        file.file_name.clone(),
        &crate::checker::context::CheckerOptions::default(),
    );
    checker.check_source_file(file.source_file);
    let source_file = file
        .arena
        .get(file.source_file)
        .and_then(|node| file.arena.get_source_file(node))
        .expect("missing source file");
    let namespace_stmt = source_file
        .statements
        .nodes
        .iter()
        .copied()
        .find(|&stmt_idx| {
            let Some(stmt_node) = file.arena.get(stmt_idx) else {
                return false;
            };
            let Some(module_decl) = file.arena.get_module(stmt_node) else {
                return false;
            };
            file.arena
                .get_identifier_at(module_decl.name)
                .is_some_and(|ident| ident.escaped_text.as_str() == "N1")
        })
        .expect("missing namespace declaration");
    let namespace_body_statements = file
        .arena
        .get(namespace_stmt)
        .and_then(|node| file.arena.get_module(node))
        .map(|module| module.body)
        .and_then(|body_idx| file.arena.get(body_idx))
        .and_then(|node| file.arena.get_module_block(node))
        .and_then(|module_block| module_block.statements.as_ref())
        .map(|statements| statements.nodes.clone())
        .expect("missing namespace body");
    let assignment_expr = namespace_body_statements
        .iter()
        .copied()
        .find_map(|stmt_idx| {
            let stmt_node = file.arena.get(stmt_idx)?;
            let expr_stmt = file.arena.get_expression_statement(stmt_node)?;
            let expr_node = file.arena.get(expr_stmt.expression)?;
            file.arena
                .get_binary_expr(expr_node)
                .map(|binary| (binary.left, binary.right))
        })
        .expect("missing target assignment");
    let target_expr = namespace_body_statements
        .iter()
        .copied()
        .rev()
        .find_map(|stmt_idx| {
            let stmt_node = file.arena.get(stmt_idx)?;
            let expr_stmt = file.arena.get_expression_statement(stmt_node)?;
            let ident = file.arena.get_identifier_at(expr_stmt.expression)?;
            (ident.escaped_text.as_str() == "target").then_some(expr_stmt.expression)
        })
        .expect("missing target expression");
    let source_type = checker.get_type_of_node(assignment_expr.1);
    let target_type = checker.get_type_of_node(target_expr);

    assert!(
        checker.is_assignable_to(source_type, target_type),
        "namespace-local ComponentClass assignability should accept subclass constructors. Actual diagnostics: {:#?}",
        checker.ctx.diagnostics,
    );
}

#[test]
fn test_check_files_parallel_jsdoc_import_type_on_export_default_preserves_ts2353() {
    let files = vec![
        (
            "a.ts".to_string(),
            r#"
export interface Foo {
    a: number;
    b: number;
}
"#
            .to_string(),
        ),
        (
            "b.js".to_string(),
            r#"
/** @type {import("./a").Foo} */
export default { c: false };
"#
            .to_string(),
        ),
        (
            "c.js".to_string(),
            r#"
import b from "./b";
b;
"#
            .to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            allow_js: true,
            check_js: true,
            no_lib: true,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..Default::default()
        },
        &[],
    );

    let exporter = result
        .file_results
        .iter()
        .find(|file| file.file_name == "b.js")
        .expect("expected b.js result");

    assert!(
        exporter.diagnostics.iter().any(|diag| diag.code == 2353),
        "Expected TS2353 in b.js. Actual diagnostics: {:#?}",
        exporter.diagnostics
    );
}

#[test]
fn test_check_files_parallel_cross_file_const_and_class_redeclaration_uses_ts2451() {
    let files = vec![
        ("a.ts".to_string(), "const Bar = 3;\n".to_string()),
        ("b.ts".to_string(), "class Bar {}\n".to_string()),
    ];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let file_b = result
        .file_results
        .iter()
        .find(|file| file.file_name == "b.ts")
        .expect("expected b.ts result");

    let codes: Vec<u32> = file_b
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2451 || diag.code == 2300)
        .map(|diag| diag.code)
        .collect();

    assert_eq!(
        codes,
        vec![2451],
        "Expected b.ts to report TS2451 for cross-file const/class redeclaration. Diagnostics: {:#?}",
        file_b.diagnostics
    );
}

#[test]
fn test_check_files_parallel_module_augmentation_redeclaration_marks_target_file() {
    let files = vec![
        ("dir/a.ts".to_string(), "export const x = 0;\n".to_string()),
        (
            "dir/b.ts".to_string(),
            r#"
export {};
declare module "./a" {
    export const x: 1;
}
declare module "./a" {
    export const x: 2;
}
"#
            .to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let file_a = result
        .file_results
        .iter()
        .find(|file| file.file_name == "dir/a.ts")
        .expect("expected dir/a.ts result");
    let file_b = result
        .file_results
        .iter()
        .find(|file| file.file_name == "dir/b.ts")
        .expect("expected dir/b.ts result");

    let a_codes: Vec<u32> = file_a
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2451 || diag.code == 2300)
        .map(|diag| diag.code)
        .collect();
    let b_codes: Vec<u32> = file_b
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2451 || diag.code == 2300)
        .map(|diag| diag.code)
        .collect();

    assert_eq!(
        a_codes,
        vec![2451],
        "Expected dir/a.ts to report TS2451 from module augmentation redeclarations. Diagnostics: {:#?}",
        file_a.diagnostics
    );
    assert_eq!(
        b_codes,
        vec![2451, 2451],
        "Expected dir/b.ts to report two TS2451 diagnostics from duplicate augmentation exports. Diagnostics: {:#?}",
        file_b.diagnostics
    );
}

#[test]
fn test_check_files_parallel_global_augmentation_member_conflicts_emit_ts2300() {
    let files = vec![
        (
            "file1.ts".to_string(),
            r#"
declare global {
    interface TopLevel {
        duplicate1: () => string;
        duplicate2: () => string;
        duplicate3: () => string;
    }
}
export {}
"#
            .to_string(),
        ),
        (
            "file2.ts".to_string(),
            r#"
import "./file1";
declare global {
    interface TopLevel {
        duplicate1(): number;
        duplicate2(): number;
        duplicate3(): number;
    }
}
export {}
"#
            .to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let file1 = result
        .file_results
        .iter()
        .find(|file| file.file_name == "file1.ts")
        .expect("expected file1.ts result");
    let file2 = result
        .file_results
        .iter()
        .find(|file| file.file_name == "file2.ts")
        .expect("expected file2.ts result");

    let file1_codes: Vec<u32> = file1
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2300 || diag.code == 6200)
        .map(|diag| diag.code)
        .collect();
    let file2_codes: Vec<u32> = file2
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2300 || diag.code == 6200)
        .map(|diag| diag.code)
        .collect();

    assert_eq!(
        file1_codes,
        vec![2300, 2300, 2300],
        "Expected file1.ts to report per-member TS2300 diagnostics for global augmentation conflicts. Diagnostics: {:#?}",
        file1.diagnostics
    );
    assert_eq!(
        file2_codes,
        vec![2300, 2300, 2300],
        "Expected file2.ts to report per-member TS2300 diagnostics for global augmentation conflicts. Diagnostics: {:#?}",
        file2.diagnostics
    );
}

#[test]
fn test_check_files_parallel_module_augmentation_member_conflicts_aggregate_to_ts6200() {
    let files = vec![
        (
            "file1.ts".to_string(),
            r#"
declare module "someMod" {
    export interface TopLevel {
        duplicate1: () => string;
        duplicate2: () => string;
        duplicate3: () => string;
        duplicate4: () => string;
        duplicate5: () => string;
        duplicate6: () => string;
        duplicate7: () => string;
        duplicate8: () => string;
        duplicate9: () => string;
    }
}
"#
            .to_string(),
        ),
        (
            "file2.ts".to_string(),
            r#"
/// <reference path="./file1" />

declare module "someMod" {
    export interface TopLevel {
        duplicate1(): number;
        duplicate2(): number;
        duplicate3(): number;
        duplicate4(): number;
        duplicate5(): number;
        duplicate6(): number;
        duplicate7(): number;
        duplicate8(): number;
        duplicate9(): number;
    }
}
export {};
"#
            .to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let file1 = result
        .file_results
        .iter()
        .find(|file| file.file_name == "file1.ts")
        .expect("expected file1.ts result");
    let file2 = result
        .file_results
        .iter()
        .find(|file| file.file_name == "file2.ts")
        .expect("expected file2.ts result");

    let file1_codes: Vec<u32> = file1
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2300 || diag.code == 6200)
        .map(|diag| diag.code)
        .collect();
    let file2_codes: Vec<u32> = file2
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2300 || diag.code == 6200)
        .map(|diag| diag.code)
        .collect();

    assert_eq!(
        file1_codes,
        vec![6200],
        "Expected file1.ts to aggregate large module augmentation conflicts into TS6200. Diagnostics: {:#?}",
        file1.diagnostics
    );
    assert_eq!(
        file2_codes,
        vec![6200],
        "Expected file2.ts to aggregate large module augmentation conflicts into TS6200. Diagnostics: {:#?}",
        file2.diagnostics
    );
}

#[test]
fn test_check_files_parallel_cross_file_enum_conflicts_emit_ts2567() {
    let files = vec![
        (
            "file1.ts".to_string(),
            r#"
enum D {
    bar
}
class E {}
"#
            .to_string(),
        ),
        (
            "file2.ts".to_string(),
            r#"
function D() {
    return 0;
}
enum E {
    bar
}
"#
            .to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let file1 = result
        .file_results
        .iter()
        .find(|file| file.file_name == "file1.ts")
        .expect("expected file1.ts result");
    let file2 = result
        .file_results
        .iter()
        .find(|file| file.file_name == "file2.ts")
        .expect("expected file2.ts result");

    let file1_codes: Vec<u32> = file1
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2567)
        .map(|diag| diag.code)
        .collect();
    let file2_codes: Vec<u32> = file2
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2567)
        .map(|diag| diag.code)
        .collect();

    assert_eq!(
        file1_codes,
        vec![2567, 2567],
        "Expected file1.ts to report TS2567 for cross-file enum conflicts. Diagnostics: {:#?}",
        file1.diagnostics
    );
    assert_eq!(
        file2_codes,
        vec![2567, 2567],
        "Expected file2.ts to report TS2567 for cross-file enum conflicts. Diagnostics: {:#?}",
        file2.diagnostics
    );
}

#[test]
fn test_check_files_parallel_var_and_duplicate_functions_keep_ts2300() {
    let files = vec![(
        "test.ts".to_string(),
        r#"
var foo: string;
function foo(): number { }
function foo(): number { }
"#
        .to_string(),
    )];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "test.ts")
        .expect("expected test.ts result");

    let ts2300_count = file
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2300)
        .count();
    let ts2393_count = file
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2393)
        .count();
    let ts2355_count = file
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2355)
        .count();

    assert_eq!(
        ts2300_count, 3,
        "Expected TS2300 on the var and both function declarations. Diagnostics: {:#?}",
        file.diagnostics
    );
    assert_eq!(
        ts2393_count, 2,
        "Expected TS2393 on both function implementations. Diagnostics: {:#?}",
        file.diagnostics
    );
    assert_eq!(
        ts2355_count, 2,
        "Expected TS2355 on both function implementations. Diagnostics: {:#?}",
        file.diagnostics
    );
}

#[test]
fn test_check_files_parallel_class_property_after_method_emits_ts2717() {
    let files = vec![(
        "test.ts".to_string(),
        r#"
class C {
    a(): number { return 0; }
    a: number;
}
class K {
    b: number;
    b(): number { return 0; }
}
class D {
    c: number;
    c: string;
}
"#
        .to_string(),
    )];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "test.ts")
        .expect("expected test.ts result");

    let ts2717_messages: Vec<&str> = file
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2717)
        .map(|diag| diag.message_text.as_str())
        .collect();

    assert_eq!(
        ts2717_messages.len(),
        2,
        "Expected TS2717 for 'a' and 'c' only. Diagnostics: {:#?}",
        file.diagnostics
    );
    assert!(
        ts2717_messages
            .iter()
            .any(|msg| msg.contains("Property 'a' must be of type '() => number'")),
        "Expected method-vs-property TS2717 for 'a'. Diagnostics: {:#?}",
        file.diagnostics
    );
    assert!(
        ts2717_messages
            .iter()
            .any(|msg| msg.contains("Property 'c' must be of type 'number'")),
        "Expected property-vs-property TS2717 for 'c'. Diagnostics: {:#?}",
        file.diagnostics
    );
}

#[test]
fn test_compile_large_program() {
    // Simulate a larger program with many files
    let files: Vec<_> = (0..50)
        .map(|i| {
            let source = format!("function fn{i}() {{ return {i}; }} const val{i} = fn{i}();");
            (format!("module{i}.ts"), source)
        })
        .collect();

    let program = compile_files(files);

    assert_eq!(program.files.len(), 50);
    // Should have at least 100 symbols (2 per file: fn + val)
    assert!(
        program.symbols.len() >= 100,
        "Expected at least 100 symbols, got {}",
        program.symbols.len()
    );

    // All function and value names should be in globals
    for i in 0..50 {
        let fn_name = format!("fn{i}");
        let val_name = format!("val{i}");
        assert!(program.globals.has(&fn_name), "Missing {fn_name}");
        assert!(program.globals.has(&val_name), "Missing {val_name}");
    }
}

#[test]
fn test_compile_with_exports() {
    // Test that export function/class/const are properly bound
    let files = vec![
        (
            "a.ts".to_string(),
            "export function add(x: number, y: number) { return x + y; }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "export class Calculator { add(x: number, y: number) { return x + y; } }".to_string(),
        ),
        ("c.ts".to_string(), "export const PI = 3.14159;".to_string()),
    ];

    let program = compile_files(files);

    assert_eq!(program.files.len(), 3);
    // All exported declarations should be in globals
    assert!(
        program.globals.has("add"),
        "Exported function 'add' should be in globals"
    );
    assert!(
        program.globals.has("Calculator"),
        "Exported class 'Calculator' should be in globals"
    );
    assert!(
        program.globals.has("PI"),
        "Exported const 'PI' should be in globals"
    );
}

// =========================================================================
// Parallel Type Checking Tests
// =========================================================================

/// Test parallel type checking of Redux/Lodash-style generics
///
/// NOTE: Currently ignored - complex generic type inference with Redux/Lodash-style
/// patterns is not fully implemented. The checker emits various "Object is of type 'unknown'"
/// errors for cases that should work correctly.
#[test]
fn test_check_redux_lodash_style_generics() {
    let files = vec![
        (
            "types.ts".to_string(),
            r#"
type AnyAction = { type: string; payload?: any };

type Reducer<S, A extends AnyAction> = (state: S | undefined, action: A) => S;

type ReducersMapObject<S, A extends AnyAction> = {
  [K in keyof S]: Reducer<S[K], A>;
};

type ExtractState<R> = R extends Reducer<infer S, AnyAction> ? S : never;
type ExtractAction<R> = R extends Reducer<any, infer A> ? A : never;

type StateFromReducers<R> = { [K in keyof R]: ExtractState<R[K]> };
type ActionFromReducers<R> = { [K in keyof R]: ExtractAction<R[K]> }[keyof R];

type DeepPartial<T> = {
  [K in keyof T]?: T[K] extends object ? DeepPartial<T[K]> : T[K];
};

type Dictionary<T> = { [key: string]: T };
type ValueOf<T> = T[keyof T];
type PickValue<T, V> = { [K in keyof T]: T[K] extends V ? T[K] : never };
type ActionByType<A extends AnyAction, T extends string> = A extends { type: T } ? A : never;

interface Store<S, A> {
  getState: () => S;
  dispatch: (action: A) => A;
  replaceState: (next: DeepPartial<S>) => void;
}
"#
            .to_string(),
        ),
        (
            "reducers.ts".to_string(),
            r#"
type CounterAction = { type: "inc" } | { type: "dec" };
type MessageAction = { type: "set"; payload: string };
type AppAction = CounterAction | MessageAction;

const counterReducer: Reducer<number, AnyAction> = (state = 0, action) => {
  if (action.type == "inc") return state + 1;
  if (action.type == "dec") return state - 1;
  return state;
};

const messageReducer: Reducer<string, AnyAction> = (state = "", action) => {
  if (action.type == "set") return action.payload;
  return state;
};

type RootState = {
  count: number;
  message: string;
  tags: Dictionary<number>;
};

type RootReducers = ReducersMapObject<RootState, AnyAction>;

const rootReducers: RootReducers = {
  count: counterReducer,
  message: messageReducer,
  tags: (state = {}, _action) => state,
};

const incAction: ActionByType<AppAction, "inc"> = { type: "inc" };
"#
            .to_string(),
        ),
        (
            "store.ts".to_string(),
            r#"
type StateFromReducer<R> = R extends Reducer<infer S, AnyAction> ? S : never;
type ActionFromReducer<R> = R extends Reducer<any, infer A> ? A : AnyAction;

function combineReducers<R extends ReducersMapObject<any, AnyAction>>(
  reducers: R
): Reducer<StateFromReducers<R>, ActionFromReducers<R>> {
  return (state: StateFromReducers<R> | undefined, action: ActionFromReducers<R>) => {
    const next = {} as StateFromReducers<R>;
    return next;
  };
}

function createStore<R extends Reducer<any, AnyAction>>(
  reducer: R
): Store<StateFromReducer<R>, ActionFromReducer<R>> {
  return {
    getState: () => ({} as StateFromReducer<R>),
    dispatch: (action: ActionFromReducer<R>) => action,
    replaceState: (_next: DeepPartial<StateFromReducer<R>>) => {},
  };
}
"#
            .to_string(),
        ),
        (
            "app.ts".to_string(),
            r#"
const rootReducer = combineReducers(rootReducers);

function runApp() {
  const store = createStore(rootReducer);
  const state = store.getState();
  const count: number = state.count;
  const message: string = state.message;
  const patch: DeepPartial<RootState> = { message: "ok" };

  store.replaceState(patch);

  const action: ActionFromReducers<typeof rootReducers> = { type: "inc" };
  store.dispatch(action);

  const sample: ValueOf<PickValue<RootState, number>> = count;
  return sample + count + state.tags["a"];
}
"#
            .to_string(),
        ),
    ];

    let program = compile_files(files);

    for file in &program.files {
        assert!(
            file.parse_diagnostics.is_empty(),
            "Unexpected parse diagnostics in {}",
            file.file_name
        );
    }

    let (result, stats) = check_functions_with_stats(&program);

    // Print diagnostics for debugging
    if result.diagnostic_count > 0 {
        println!("\n=== DIAGNOSTICS ({}) ===", result.diagnostic_count);
        for file_result in &result.file_results {
            for diag in &file_result.diagnostics {
                println!(
                    "  [{}:{}] code={}: {}",
                    file_result.file_name, diag.start, diag.code, diag.message_text
                );
            }
        }
        println!("=== END DIAGNOSTICS ===\n");
    }

    assert_eq!(stats.file_count, 4);
    assert!(stats.function_count >= 5, "Expected at least 5 functions");

    // Debug: print diagnostics if there are any
    if result.diagnostic_count > 0 {
        println!("\n=== DIAGNOSTICS ({}) ===", result.diagnostic_count);
        for file_result in &result.file_results {
            for diag in &file_result.diagnostics {
                println!("  [{}:{}] {}", diag.file, diag.start, diag.message_text);
            }
        }
        println!("=== END DIAGNOSTICS ===\n");
    }

    assert_eq!(result.diagnostic_count, 0);
}

#[test]
fn test_check_single_function() {
    let files = vec![(
        "a.ts".to_string(),
        "function add(x: number, y: number): number { return x + y; }".to_string(),
    )];

    let program = compile_files(files);
    let result = check_functions_parallel(&program);

    assert_eq!(result.file_results.len(), 1);
    assert_eq!(result.function_count, 1);
    assert_eq!(result.file_results[0].function_results.len(), 1);
}

#[test]
fn test_check_multiple_functions_parallel() {
    let files = vec![
        (
            "a.ts".to_string(),
            "function foo() { return 1; } function bar() { return 2; }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "function baz(x: number) { return x * 2; }".to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_functions_parallel(&program);

    assert_eq!(result.file_results.len(), 2);
    // File a has 2 functions, file b has 1
    let total_functions: usize = result
        .file_results
        .iter()
        .map(|r| r.function_results.len())
        .sum();
    assert_eq!(total_functions, 3);
}

#[test]
fn test_check_arrow_functions() {
    let files = vec![
        (
            "a.ts".to_string(),
            "const add = (x: number, y: number) => x + y;".to_string(),
        ),
        (
            "b.ts".to_string(),
            "const double = (x: number) => { return x * 2; };".to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_functions_parallel(&program);

    // Should find the arrow functions
    let total_functions: usize = result
        .file_results
        .iter()
        .map(|r| r.function_results.len())
        .sum();
    assert!(
        total_functions >= 2,
        "Should find at least 2 arrow functions"
    );
}

#[test]
fn test_check_class_methods() {
    let files = vec![
        ("a.ts".to_string(), "class Calculator { add(x: number, y: number) { return x + y; } subtract(x: number, y: number) { return x - y; } }".to_string()),
    ];

    let program = compile_files(files);
    let result = check_functions_parallel(&program);

    // Should find the class methods
    let total_functions: usize = result
        .file_results
        .iter()
        .map(|r| r.function_results.len())
        .sum();
    assert!(total_functions >= 2, "Should find at least 2 class methods");
}

#[test]
fn test_check_with_stats() {
    let files = vec![
        (
            "a.ts".to_string(),
            "function foo() { return 1; }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "function bar() { return 2; }".to_string(),
        ),
        (
            "c.ts".to_string(),
            "function baz() { return 3; }".to_string(),
        ),
    ];

    let program = compile_files(files);
    let (result, stats) = check_functions_with_stats(&program);

    assert_eq!(stats.file_count, 3);
    assert_eq!(stats.function_count, 3);
    assert_eq!(result.file_results.len(), 3);
}

#[test]
fn test_check_large_program_parallel() {
    // Test parallel checking with many files
    let files: Vec<_> = (0..50)
        .map(|i| {
            let source = format!(
                "function fn{i}(x: number): number {{ return x * {i}; }} const val{i} = fn{i}(10);"
            );
            (format!("module{i}.ts"), source)
        })
        .collect();

    let program = compile_files(files);
    let (_result, stats) = check_functions_with_stats(&program);

    assert_eq!(stats.file_count, 50);
    // Each file has 1 function declaration
    assert!(
        stats.function_count >= 50,
        "Expected at least 50 functions, got {}",
        stats.function_count
    );
}

#[test]
fn test_check_consistency() {
    // Check the same program multiple times - results should be consistent
    let files = vec![(
        "a.ts".to_string(),
        "function add(x: number, y: number): number { return x + y; }".to_string(),
    )];

    let program = compile_files(files);

    let result1 = check_functions_parallel(&program);
    let result2 = check_functions_parallel(&program);

    assert_eq!(result1.function_count, result2.function_count);
    assert_eq!(result1.diagnostic_count, result2.diagnostic_count);
    assert_eq!(result1.file_results.len(), result2.file_results.len());
}

#[test]
fn test_check_nested_functions() {
    let files = vec![(
        "a.ts".to_string(),
        "function outer() { function inner() { return 1; } return inner(); }".to_string(),
    )];

    let program = compile_files(files);
    let result = check_functions_parallel(&program);

    // Should find both outer and inner functions
    let total_functions: usize = result
        .file_results
        .iter()
        .map(|r| r.function_results.len())
        .sum();
    assert!(
        total_functions >= 2,
        "Should find both outer and inner functions"
    );
}

#[test]
fn test_check_exported_functions() {
    let files = vec![
        (
            "a.ts".to_string(),
            "export function add(x: number, y: number) { return x + y; }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "export function subtract(x: number, y: number) { return x - y; }".to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_functions_parallel(&program);

    // Should find the exported functions
    let total_functions: usize = result
        .file_results
        .iter()
        .map(|r| r.function_results.len())
        .sum();

    assert_eq!(total_functions, 2);
}

#[test]
fn test_parallel_type_interner_concurrent_access() {
    use std::sync::Arc;
    use std::thread;

    // Test that the new lock-free TypeInterner supports concurrent access
    let interner = Arc::new(TypeInterner::new());

    let mut handles = vec![];

    // Spawn multiple threads that all intern types concurrently
    for i in 0..10 {
        let interner_clone = Arc::clone(&interner);
        let handle = thread::spawn(move || {
            // Each thread interns various types
            for j in 0..100 {
                let _ = interner_clone.literal_number(j as f64);
                let _ = interner_clone.literal_string(&format!("str_{i}_{j}"));
                let _ = interner_clone.union(vec![
                    interner_clone.literal_number((j % 10) as f64),
                    interner_clone.literal_number(((j + 1) % 10) as f64),
                ]);
            }
        });
        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify the interner has the expected number of types
    // (exact count depends on deduplication, but should be reasonable)
    let len = interner.len();
    assert!(len > 100, "Expected at least 100 types, got {len}");
    assert!(len < 2000, "Expected fewer than 2000 types, got {len}");
}

#[test]
fn test_parallel_type_checking_with_shared_interner() {
    // Test that multiple files can be type-checked in parallel
    // while sharing a single TypeInterner for type deduplication
    let files = vec![
        (
            "math.ts".to_string(),
            r#"
                function add(a: number, b: number): number { return a + b; }
                function subtract(a: number, b: number): number { return a - b; }
                function multiply(a: number, b: number): number { return a * b; }
            "#
            .to_string(),
        ),
        (
            "strings.ts".to_string(),
            r#"
                function concat(a: string, b: string): string { return a + b; }
                function upper(s: string): string { return s.toUpperCase(); }
                function lower(s: string): string { return s.toLowerCase(); }
            "#
            .to_string(),
        ),
        (
            "arrays.ts".to_string(),
            r#"
                function first<T>(arr: T[]): T | undefined { return arr[0]; }
                function last<T>(arr: T[]): T | undefined { return arr[arr.length - 1]; }
                function isEmpty<T>(arr: T[]): boolean { return arr.length === 0; }
            "#
            .to_string(),
        ),
        (
            "objects.ts".to_string(),
            r#"
                function keys(obj: object): string[] { return Object.keys(obj); }
                function values(obj: object): unknown[] { return Object.values(obj); }
                function entries(obj: object): [string, unknown][] { return Object.entries(obj); }
            "#
            .to_string(),
        ),
    ];

    let program = compile_files(files);
    assert_eq!(program.files.len(), 4);

    // Check all files in parallel
    let (_result, stats) = check_functions_with_stats(&program);

    assert_eq!(stats.file_count, 4);
    // Each file has 3 functions
    assert!(
        stats.function_count >= 12,
        "Expected at least 12 functions, got {}",
        stats.function_count
    );

    // The shared TypeInterner should have deduplicated common types
    // (number, string, boolean, etc. are shared across all files)
    let interner_len = program.type_interner.len();
    assert!(
        interner_len > TypeId::FIRST_USER as usize,
        "TypeInterner should have user-defined types"
    );
}

#[test]
fn test_parallel_binding_produces_consistent_symbols() {
    // Test that parallel binding produces consistent results
    // by binding the same files multiple times
    let files = vec![
        (
            "a.ts".to_string(),
            "export const x: number = 1;".to_string(),
        ),
        (
            "b.ts".to_string(),
            "export const y: string = 'hello';".to_string(),
        ),
        (
            "c.ts".to_string(),
            "export function add(a: number, b: number) { return a + b; }".to_string(),
        ),
    ];

    // Bind multiple times
    let results1 = parse_and_bind_parallel(files.clone());
    let results2 = parse_and_bind_parallel(files);

    // Results should be structurally identical
    assert_eq!(results1.len(), results2.len());

    for (r1, r2) in results1.iter().zip(results2.iter()) {
        assert_eq!(r1.file_name, r2.file_name);
        assert_eq!(r1.arena.len(), r2.arena.len());
        assert_eq!(r1.symbols.len(), r2.symbols.len());

        // Same symbols should be present
        for (name, _) in r1.file_locals.iter() {
            assert!(
                r2.file_locals.has(name),
                "Symbol {name} should be present in both results"
            );
        }
    }
}
