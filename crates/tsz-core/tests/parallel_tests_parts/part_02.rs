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
    assert!(stats.has_skeleton_index);
    assert!(
        stats.skeleton_total_symbol_count >= 2,
        "skeleton should track at least 2 symbols, got {}",
        stats.skeleton_total_symbol_count
    );
    assert!(
        stats.skeleton_estimated_size_bytes > 0,
        "skeleton size estimate should be nonzero when skeleton is present"
    );
    assert!(
        stats.pre_merge_bind_total_bytes > 0,
        "pre-merge bind total should be nonzero for any non-empty merge"
    );
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
    assert!(stats.has_skeleton_index);
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
#[ignore] // TODO: TS2454 for UMD namespace qualified type member needs cross-file tracking
fn test_check_files_parallel_preserves_ts2454_for_umd_namespace_qualified_type_member() {
    let files = vec![
        (
            "foo.d.ts".to_string(),
            r#"
export var x: number;
export function fn(): void;
export interface Thing { n: typeof x }
export as namespace Foo;
"#
            .to_string(),
        ),
        (
            "a.ts".to_string(),
            r#"
/// <reference path="foo.d.ts" />
Foo.fn();
let x: Foo.Thing;
let y: number = x.n;
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

    let file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "a.ts")
        .expect("expected a.ts result");
    let relevant_codes: Vec<u32> = file
        .diagnostics
        .iter()
        .filter(|diag| diag.code != 2318)
        .map(|diag| diag.code)
        .collect();

    assert_eq!(
        relevant_codes,
        vec![2454],
        "Expected only TS2454 in a.ts. Actual diagnostics: {:#?}",
        file.diagnostics
    );
}

#[test]
fn test_check_files_parallel_preserves_tdz_after_namespace_reexport() {
    let files = vec![
        (
            "0.ts".to_string(),
            r#"
export const a = 1;
export const b = 2;
"#
            .to_string(),
        ),
        (
            "1.ts".to_string(),
            r#"
export * as ns from "./0";
ns.a;
ns.b;
let ns = { a: 1, b: 2 };
ns.a;
ns.b;
"#
            .to_string(),
        ),
        (
            "2.ts".to_string(),
            r#"
import * as foo from "./1";

foo.ns.a;
foo.ns.b;
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

    let file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "1.ts")
        .expect("expected 1.ts result");

    let ts2448_count = file
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2448)
        .count();
    let ts2454_count = file
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2454)
        .count();

    assert_eq!(
        ts2448_count, 2,
        "Expected exactly two TS2448 diagnostics in 1.ts. Actual diagnostics: {:#?}",
        file.diagnostics
    );
    assert_eq!(
        ts2454_count, 2,
        "Expected exactly two TS2454 diagnostics in 1.ts. Actual diagnostics: {:#?}",
        file.diagnostics
    );

    let importer = result
        .file_results
        .iter()
        .find(|file| file.file_name == "2.ts")
        .expect("expected 2.ts result");
    let importer_codes: Vec<u32> = importer.diagnostics.iter().map(|diag| diag.code).collect();
    assert!(
        importer_codes.is_empty(),
        "Expected no diagnostics in 2.ts. Actual diagnostics: {:#?}",
        importer.diagnostics
    );
}

#[test]
fn test_check_files_parallel_preserves_same_file_namespace_exports() {
    let files = vec![
        ("a.ts".to_string(), "export class A {}\n".to_string()),
        (
            "b.ts".to_string(),
            "export * as a from \"./a\";\n".to_string(),
        ),
        (
            "c.ts".to_string(),
            "import type { a } from \"./b\";\nexport { a };\n".to_string(),
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

    let file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "c.ts")
        .expect("expected c.ts result");
    let relevant_codes: Vec<u32> = file
        .diagnostics
        .iter()
        .filter(|diag| diag.code != 2318)
        .map(|diag| diag.code)
        .collect();

    assert!(
        !relevant_codes.contains(&2305),
        "Did not expect TS2305 in c.ts. Actual diagnostics: {:#?}",
        file.diagnostics
    );
}

#[test]
#[ignore] // TODO: Import shadowing type meaning needs parallel checking refinement
fn test_check_files_parallel_preserves_import_shadowing_type_meaning() {
    let files = vec![
        ("b.ts".to_string(), "export const zzz = 123;\n".to_string()),
        (
            "a.ts".to_string(),
            r#"import * as B from "./b";

interface B {
    x: string;
}

const x: B = { x: "" };
B.zzz;

export { B };
"#
            .to_string(),
        ),
        (
            "index.ts".to_string(),
            r#"import { B } from "./a";

const x: B = { x: "" };
B.zzz;

import * as OriginalB from "./b";
OriginalB.zzz;

const y: OriginalB = x;
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

    let a_file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "a.ts")
        .expect("expected a.ts result");
    assert!(
        !a_file.diagnostics.iter().any(|diag| diag.code == 2353),
        "Did not expect TS2353 in a.ts. Actual diagnostics: {:#?}",
        a_file.diagnostics
    );

    let index_file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "index.ts")
        .expect("expected index.ts result");
    let ts2709: Vec<_> = index_file
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2709)
        .collect();
    assert_eq!(
        ts2709.len(),
        1,
        "Expected exactly one TS2709 in index.ts. Actual diagnostics: {:#?}",
        index_file.diagnostics
    );
    assert!(
        ts2709[0].message_text.contains("OriginalB"),
        "Expected TS2709 to mention OriginalB. Actual diagnostic: {:?}",
        ts2709[0]
    );
    assert!(
        !index_file.diagnostics.iter().any(|diag| diag.code == 2353),
        "Did not expect TS2353 in index.ts. Actual diagnostics: {:#?}",
        index_file.diagnostics
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

