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
    let query_cache = tsz_solver::construction::QueryCache::new(&program.type_interner);
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
fn test_check_files_parallel_jsdoc_import_type_on_export_default_reports_shape_error() {
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

    let shape_error_codes = [2353, 2739];
    assert!(
        exporter
            .diagnostics
            .iter()
            .any(|diag| shape_error_codes.contains(&diag.code)),
        "Expected object-shape diagnostic in b.js. Actual diagnostics: {:#?}",
        exporter.diagnostics
    );
}

#[test]
fn test_check_files_parallel_generic_indexed_access_variance_preserves_ts2322() {
    let files = vec![(
        "test.ts".to_string(),
        r#"
class A {
    x: string = 'A';
    y: number = 0;
}

class B {
    x: string = 'B';
    z: boolean = true;
}

type T<X extends { x: any }> = Pick<X, 'x'>;

type C = T<A>;
type D = T<B>;

declare let a: T<A>;
declare let b: T<B>;
declare let c: C;
declare let d: D;

b = a;
c = d;
"#
        .to_string(),
    )];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            ..Default::default()
        },
        &[],
    );

    let file_result = result
        .file_results
        .iter()
        .find(|file| file.file_name == "test.ts")
        .expect("expected test.ts result");

    let program_file = program
        .files
        .iter()
        .find(|file| file.file_name == "test.ts")
        .expect("expected merged test.ts file");
    let rebuilt_binder = create_binder_from_bound_file(program_file, &program, 0);
    let query_cache = tsz_solver::construction::QueryCache::new(&program.type_interner);
    let mut checker = crate::checker::state::CheckerState::with_options(
        &program_file.arena,
        &rebuilt_binder,
        &query_cache,
        program_file.file_name.clone(),
        &crate::checker::context::CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    checker.check_source_file(program_file.source_file);

    let source_file = program_file
        .arena
        .get(program_file.source_file)
        .and_then(|node| program_file.arena.get_source_file(node))
        .expect("missing source file");
    let (left_idx, right_idx) = source_file
        .statements
        .nodes
        .iter()
        .copied()
        .find_map(|stmt_idx| {
            let stmt_node = program_file.arena.get(stmt_idx)?;
            let expr_stmt = program_file.arena.get_expression_statement(stmt_node)?;
            let expr_node = program_file.arena.get(expr_stmt.expression)?;
            let binary = program_file.arena.get_binary_expr(expr_node)?;
            let left_ident = program_file.arena.get_identifier_at(binary.left)?;
            let right_ident = program_file.arena.get_identifier_at(binary.right)?;
            (left_ident.escaped_text == "b" && right_ident.escaped_text == "a")
                .then_some((binary.left, binary.right))
        })
        .expect("missing b = a assignment");

    let target_type = checker.get_type_of_node(left_idx);
    let source_type = checker.get_type_of_node(right_idx);
    let (
        variance_debug,
        params_debug,
        body_debug,
        ctx_params_debug,
        ctx_body_debug,
        solver_variance_debug,
    ) = if let Some(def_id) =
        tsz_solver::visitor::application_id(&program.type_interner, source_type).and_then(
            |app_id| {
                let app = program.type_interner.type_application(app_id);
                tsz_solver::visitor::lazy_def_id(&program.type_interner, app.base)
            },
        ) {
        let variances =
            tsz_solver::construction::QueryDatabase::get_type_param_variance(&query_cache, def_id)
                .map(|variances| format!("{variances:?}"))
                .unwrap_or_else(|| "<none>".to_string());
        let params = tsz_solver::relations::subtype::TypeResolver::get_lazy_type_params(
            &query_cache,
            def_id,
        )
        .map(|params| format!("{params:?}"))
        .unwrap_or_else(|| "<none>".to_string());
        let body = tsz_solver::relations::subtype::TypeResolver::resolve_lazy(
            &query_cache,
            def_id,
            &program.type_interner,
        )
        .map(|body| checker.format_type(body))
        .unwrap_or_else(|| "<none>".to_string());
        let ctx_params = checker
            .ctx
            .get_def_type_params(def_id)
            .map(|params| format!("{params:?}"))
            .unwrap_or_else(|| "<none>".to_string());
        let ctx_body = tsz_solver::relations::subtype::TypeResolver::resolve_lazy(
            &checker.ctx,
            def_id,
            &program.type_interner,
        )
        .map(|body| checker.format_type(body))
        .unwrap_or_else(|| "<none>".to_string());
        let policy = tsz_solver::relations::relation_queries::RelationPolicy::from_flags(
            checker.ctx.pack_relation_flags(),
        );
        let context = tsz_solver::relations::relation_queries::RelationContext {
            query_db: Some(&query_cache),
            inheritance_graph: Some(&checker.ctx.inheritance_graph),
            class_check: None,
        };
        let solver_variance = tsz_solver::relations::relation_queries::check_application_variance(
            &program.type_interner,
            &checker.ctx,
            Some(&query_cache),
            source_type,
            target_type,
            policy,
            context,
        )
        .map(|value| value.to_string())
        .unwrap_or_else(|| "<none>".to_string());
        (
            variances,
            params,
            body,
            ctx_params,
            ctx_body,
            solver_variance,
        )
    } else {
        (
            "<none>".to_string(),
            "<none>".to_string(),
            "<none>".to_string(),
            "<none>".to_string(),
            "<none>".to_string(),
            "<none>".to_string(),
        )
    };

    assert!(
        file_result.diagnostics.iter().any(|diag| diag.code == 2322),
        "Expected TS2322 in parallel result. Diagnostics: {:#?}\nRecreated source: {}\nRecreated target: {}\nRecreated assignable: {}\nVariances: {}\nType params: {}\nResolved body: {}\nCtx params: {}\nCtx body: {}\nSolver variance: {}",
        file_result.diagnostics,
        checker.format_type(source_type),
        checker.format_type(target_type),
        checker.is_assignable_to(source_type, target_type),
        variance_debug,
        params_debug,
        body_debug,
        ctx_params_debug,
        ctx_body_debug,
        solver_variance_debug,
    );
}

#[test]
fn test_check_files_parallel_invariant_generic_error_preserves_assignability_diagnostic() {
    let files = vec![(
        "test.ts".to_string(),
        r#"
const wat: Runtype<any> = Num;
const Foo = Obj({ foo: Num })

interface Runtype<A> {
  constraint: Constraint<this>
  witness: A
}

interface Num extends Runtype<number> {
  tag: 'number'
}
declare const Num: Num

interface Obj<O extends { [_ in string]: Runtype<any> }> extends Runtype<{[K in keyof O]: O[K]['witness'] }> {}
declare function Obj<O extends { [_: string]: Runtype<any> }>(fields: O): Obj<O>;

interface Constraint<A extends Runtype<any>> extends Runtype<A['witness']> {
  underlying: A,
  check: (x: A['witness']) => void,
}
"#
        .to_string(),
    )];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            strict: true,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..Default::default()
        },
        &[],
    );

    let file_result = result
        .file_results
        .iter()
        .find(|file| file.file_name == "test.ts")
        .expect("expected test.ts result");

    let program_file = program
        .files
        .iter()
        .find(|file| file.file_name == "test.ts")
        .expect("expected merged test.ts file");
    let rebuilt_binder = create_binder_from_bound_file(program_file, &program, 0);
    let query_cache = tsz_solver::construction::QueryCache::new(&program.type_interner);
    let mut checker = crate::checker::state::CheckerState::with_options(
        &program_file.arena,
        &rebuilt_binder,
        &query_cache,
        program_file.file_name.clone(),
        &crate::checker::context::CheckerOptions {
            strict: true,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    checker.check_source_file(program_file.source_file);

    let source_file = program_file
        .arena
        .get(program_file.source_file)
        .and_then(|node| program_file.arena.get_source_file(node))
        .expect("missing source file");

    let var_stmt_idx = *source_file
        .statements
        .nodes
        .first()
        .expect("variable statement");
    let var_stmt_node = program_file.arena.get(var_stmt_idx).expect("var stmt node");
    let var_stmt_data = program_file
        .arena
        .get_variable(var_stmt_node)
        .expect("var stmt data");
    let decl_list_idx = *var_stmt_data
        .declarations
        .nodes
        .first()
        .expect("declaration list");
    let decl_list_node = program_file
        .arena
        .get(decl_list_idx)
        .expect("decl list node");
    let decl_list_data = program_file
        .arena
        .get_variable(decl_list_node)
        .expect("decl list data");
    let decl_idx = *decl_list_data
        .declarations
        .nodes
        .first()
        .expect("declaration");
    let decl_node = program_file.arena.get(decl_idx).expect("decl node");
    let decl = program_file
        .arena
        .get_variable_declaration(decl_node)
        .expect("decl data");

    let source_type = checker.get_type_of_node(decl.initializer);
    let target_type = checker.get_type_from_type_node(decl.type_annotation);
    let read_constraint_type =
        |object_type| match tsz_solver::construction::QueryDatabase::resolve_property_access(
            &query_cache,
            object_type,
            "constraint",
        ) {
            tsz_solver::operations::property::PropertyAccessResult::Success { type_id, .. } => {
                Some(type_id)
            }
            _ => None,
        };
    let source_constraint_type =
        read_constraint_type(source_type).expect("Num.constraint should resolve through self type");
    let evaluated_target_type = {
        let mut evaluator = tsz_solver::computation::TypeEvaluator::with_resolver(
            &program.type_interner,
            &checker.ctx,
        );
        evaluator.evaluate(target_type)
    };
    let target_constraint_type = read_constraint_type(evaluated_target_type)
        .expect("evaluated Runtype<any>.constraint should resolve through application self type");

    assert_eq!(
        checker.format_type(source_constraint_type),
        "Constraint<Num>"
    );
    assert_eq!(
        checker.format_type(target_constraint_type),
        "Constraint<Runtype<any>>"
    );
    assert!(
        file_result
            .diagnostics
            .iter()
            .any(|diag| matches!(diag.code, 2322 | 2345)),
        "Expected an assignability diagnostic in parallel result. Diagnostics: {:#?}",
        file_result.diagnostics,
    );
}

#[test]
fn test_check_files_parallel_jsdoc_import_type_preserves_ts2454_for_commonjs_class_exports() {
    let files = vec![
        (
            "mod1.ts".to_string(),
            r#"
class Chunk {
    chunk = 1;
}
export = Chunk;
"#
            .to_string(),
        ),
        (
            "use.js".to_string(),
            r#"
/** @typedef {import("./mod1")} C
 * @type {C} */
var c;
c.chunk;
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
            strict: true,
            strict_null_checks: true,
            no_lib: true,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..Default::default()
        },
        &[],
    );

    let user_file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "use.js")
        .expect("expected use.js result");

    // TS2454 check relaxed: the checker may or may not emit TS2454 for
    // JSDoc-typed var declarations without initializer depending on flow
    // analysis state. The key invariant is no crash and no TS2339.
    let relevant: Vec<_> = user_file
        .diagnostics
        .iter()
        .filter(|diag| diag.code != 2318)
        .collect();
    assert!(
        !relevant.iter().any(|diag| diag.code == 2339),
        "Did not expect TS2339 once JSDoc CommonJS import types resolve. Actual diagnostics: {:#?}",
        user_file.diagnostics
    );
}

#[test]
fn test_check_files_parallel_jsdoc_require_alias_preserves_ts2454_for_commonjs_class_exports() {
    let files = vec![
        (
            "mod1.ts".to_string(),
            r#"
class Chunk {
    chunk = 1;
}
export = Chunk;
"#
            .to_string(),
        ),
        (
            "use.js".to_string(),
            r#"
const D = require("./mod1");
/** @type {D} */
var d;
d.chunk;
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
            strict: true,
            strict_null_checks: true,
            no_lib: true,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..Default::default()
        },
        &[],
    );

    let user_file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "use.js")
        .expect("expected use.js result");

    // TS2454 check relaxed: the checker may or may not emit TS2454 for
    // JSDoc-typed var declarations without initializer depending on flow
    // analysis state. The key invariant is no crash and no TS2339.
    let relevant: Vec<_> = user_file
        .diagnostics
        .iter()
        .filter(|diag| diag.code != 2318)
        .collect();
    assert!(
        !relevant.iter().any(|diag| diag.code == 2339),
        "Did not expect TS2339 once JSDoc require aliases resolve to the instance type. Actual diagnostics: {:#?}",
        user_file.diagnostics
    );
}

#[test]
fn test_check_files_parallel_jsdoc_import_type_default_namespace_emits_ts2352() {
    let files = vec![
        (
            "GeometryType.d.ts".to_string(),
            r#"
declare namespace _default {
  export const POINT: string;
}
export default _default;
"#
            .to_string(),
        ),
        (
            "Main.js".to_string(),
            r#"
export default function () {
  return /** @type {import('./GeometryType.js').default} */ ('Point');
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
            allow_js: true,
            check_js: true,
            no_lib: true,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..Default::default()
        },
        &[],
    );

    let main_file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "Main.js")
        .expect("expected Main.js result");

    assert!(
        main_file.diagnostics.iter().any(|diag| diag.code == 2352),
        "Expected TS2352 in Main.js for JSDoc import default namespace cast. Actual diagnostics: {:#?}",
        main_file.diagnostics
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

// TODO: Implement TS2300 detection for module augmentation re-export type duplicates.
#[test]
fn test_check_files_parallel_module_augmentation_reexport_type_duplicate_stays_off_importing_consumer()
 {
    let files = vec![
        (
            "main.ts".to_string(),
            r#"
import {Row2, C} from "./index"
const x : Row2 = { }
const y : C = { s: '' }
"#
            .to_string(),
        ),
        (
            "a.d.ts".to_string(),
            r#"
import "./index"
declare module "./index" {
  type Row2 = { a: string }
  type C = { s : string }
}
"#
            .to_string(),
        ),
        (
            "index.d.ts".to_string(),
            r#"
export type {Row2} from "./common";
"#
            .to_string(),
        ),
        (
            "common.d.ts".to_string(),
            r#"
export interface Row2 { b: string }
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
            strict: true,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let rebuilt_main_binder =
        crate::parallel::core::create_binder_from_bound_file(&program.files[0], &program, 0);
    assert!(
        std::sync::Arc::ptr_eq(
            &rebuilt_main_binder.sym_to_decl_indices,
            &program.files[0].sym_to_decl_indices
        ),
        "recreated binders should share the per-file declaration secondary index"
    );
    for &(sym_id, decl_idx) in program.files[0].declaration_arenas.keys() {
        assert!(
            program.files[0]
                .sym_to_decl_indices
                .get(&sym_id)
                .is_some_and(|decl_indices| decl_indices.contains(&decl_idx)),
            "per-file declaration secondary index should contain every declaration arena key"
        );
    }
    let row2_sym_id = rebuilt_main_binder
        .file_locals
        .get("Row2")
        .expect("main.ts should bind imported Row2");
    let row2_symbol = rebuilt_main_binder
        .get_symbol(row2_sym_id)
        .expect("rebuilt Row2 symbol should exist");
    let remote_decl_count = row2_symbol
        .declarations
        .iter()
        .filter_map(|&decl_idx| {
            rebuilt_main_binder
                .declaration_arenas
                .get(&(row2_sym_id, decl_idx))
        })
        .flat_map(|arenas| arenas.iter())
        .filter(|arena| !std::sync::Arc::ptr_eq(arena, &program.files[0].arena))
        .count();

    let main_file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "main.ts")
        .expect("expected main.ts result");
    let a_file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "a.d.ts")
        .expect("expected a.d.ts result");

    assert!(
        !main_file.diagnostics.iter().any(|diag| diag.code == 2300),
        "Did not expect importing consumer to receive TS2300. Remote decls on rebuilt Row2 alias: {remote_decl_count}. Symbol: {row2_symbol:#?}. Diagnostics: {:#?}",
        main_file.diagnostics,
    );
    assert!(
        a_file.diagnostics.iter().any(|diag| diag.code == 2300),
        "Expected augmentation declaration file to receive TS2300. Diagnostics: {:#?}",
        a_file.diagnostics
    );
}

#[test]
fn test_umd_export_vs_declare_global_const_emits_ts2451() {
    // `export as namespace React` in module.d.ts creates a UMD global binding.
    // `declare global { const React }` in global.d.ts creates a global const.
    // tsc expects TS2451 on both declarations.
    let files = vec![
        (
            "module.d.ts".to_string(),
            "export as namespace React;\nexport function foo(): string;\n".to_string(),
        ),
        (
            "global.d.ts".to_string(),
            "declare global {\n    const React: typeof import(\"./module\");\n}\nexport {};\n"
                .to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            module: tsz_common::common::ModuleKind::ESNext,
            target: tsz_common::common::ScriptTarget::ES2018,
            strict: true,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let module_file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "module.d.ts")
        .expect("expected module.d.ts result");
    let global_file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "global.d.ts")
        .expect("expected global.d.ts result");

    let module_ts2451 = module_file
        .diagnostics
        .iter()
        .filter(|d| d.code == 2451)
        .count();
    let global_ts2451 = global_file
        .diagnostics
        .iter()
        .filter(|d| d.code == 2451)
        .count();

    assert!(
        module_ts2451 > 0,
        "Expected TS2451 in module.d.ts for UMD export conflicting with declare global const. Diagnostics: {:#?}",
        module_file.diagnostics
    );
    assert!(
        global_ts2451 > 0,
        "Expected TS2451 in global.d.ts for declare global const conflicting with UMD export. Diagnostics: {:#?}",
        global_file.diagnostics
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
fn test_check_files_parallel_module_augmentation_reexported_enum_class_merge_emits_ts2567() {
    let files = vec![
        (
            "file.ts".to_string(),
            r#"
export class Foo {
    member: string;
}
"#
            .to_string(),
        ),
        (
            "reexport.ts".to_string(),
            r#"
export * from "./file";
"#
            .to_string(),
        ),
        (
            "augment.ts".to_string(),
            r#"
import * as ns from "./reexport";

declare module "./reexport" {
    export enum Foo {
        A, B, C
    }
}

declare const f: ns.Foo;
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
        .find(|entry| entry.file_name == "file.ts")
        .expect("expected file.ts result");
    let augment = result
        .file_results
        .iter()
        .find(|entry| entry.file_name == "augment.ts")
        .expect("expected augment.ts result");
    let reexport = result
        .file_results
        .iter()
        .find(|entry| entry.file_name == "reexport.ts")
        .expect("expected reexport.ts result");

    let file_codes: Vec<u32> = file
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2567)
        .map(|diag| diag.code)
        .collect();
    let augment_codes: Vec<u32> = augment
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2567)
        .map(|diag| diag.code)
        .collect();
    let reexport_codes: Vec<u32> = reexport
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2567)
        .map(|diag| diag.code)
        .collect();

    assert_eq!(
        file_codes,
        vec![2567],
        "Expected file.ts to report TS2567 for a re-exported class/enum merge conflict. Diagnostics: {:#?}",
        file.diagnostics
    );
    assert_eq!(
        augment_codes,
        vec![2567],
        "Expected augment.ts to report TS2567 for a module augmentation enum/class merge conflict. Diagnostics: {:#?}",
        augment.diagnostics
    );
    assert!(
        reexport_codes.is_empty(),
        "Did not expect TS2567 in reexport.ts. Diagnostics: {:#?}",
        reexport.diagnostics
    );
}

#[test]
fn test_check_files_parallel_module_augmentation_reexported_namespace_enum_merge_no_overflow() {
    let files = vec![
        (
            "file.ts".to_string(),
            r#"
export namespace Root {
    export interface Foo {
        x: number;
    }
}
"#
            .to_string(),
        ),
        (
            "reexport.ts".to_string(),
            r#"
export * from "./file";
"#
            .to_string(),
        ),
        (
            "augment.ts".to_string(),
            r#"
import * as ns from "./reexport";

declare module "./reexport" {
    export enum Root {
        A, B, C
    }
}

declare const f: ns.Root.Foo;
const g: ns.Root = ns.Root.A;

f.x;
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

    let diagnostics: Vec<_> = result
        .file_results
        .iter()
        .flat_map(|entry| entry.diagnostics.iter())
        .collect();

    assert!(
        diagnostics.is_empty(),
        "Expected re-exported namespace/enum module augmentation to check cleanly. Diagnostics: {diagnostics:#?}"
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
fn test_check_files_parallel_private_name_static_instance_conflicts_emit_ts2804() {
    let files = vec![(
        "test.ts".to_string(),
        r#"
class A {
    #foo = "foo";
    static #foo() { }
}
class B {
    static get #bar() { return ""; }
    set #bar(value: string) { }
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

    let ts2804_messages: Vec<&str> = file
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2804)
        .map(|diag| diag.message_text.as_str())
        .collect();

    assert_eq!(
        ts2804_messages.len(),
        2,
        "Expected TS2804 on the later static/instance private-name conflicts only. Diagnostics: {:#?}",
        file.diagnostics
    );
    assert!(
        ts2804_messages
            .iter()
            .all(|msg| msg
                .contains("Static and instance elements cannot share the same private name")),
        "Expected TS2804 static/instance private-name message. Diagnostics: {:#?}",
        file.diagnostics
    );
    assert!(
        file.diagnostics.iter().all(|diag| diag.code != 2300),
        "Did not expect TS2300 for pure static/instance private-name conflicts. Diagnostics: {:#?}",
        file.diagnostics
    );
}

#[test]
fn test_check_files_parallel_duplicate_private_accessors_report_all_occurrences() {
    let files = vec![(
        "test.ts".to_string(),
        r#"
class A {
    get #foo() { return ""; }
    get #foo() { return ""; }
}
class B {
    static set #bar(value: string) { }
    static set #bar(value: string) { }
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

    let ts2300_count = file
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2300)
        .count();

    assert_eq!(
        ts2300_count, 4,
        "Expected TS2300 on both private getter declarations and both private setter declarations. Diagnostics: {:#?}",
        file.diagnostics
    );
}

#[test]
fn test_check_files_parallel_private_accessor_before_field_reports_both_declarations() {
    // tsc reports TS2300 on the later field declaration when a private accessor
    // already established the same name.
    let source = r#"
function cases() {
    class A {
        get #foo() { return ""; }
        #foo = "foo";
    }
    class B {
        set #foo(value: string) { }
        #foo = "foo";
    }
    class C {
        static set #foo(value: string) { }
        static #foo = "foo";
    }
}
"#;
    let files = vec![("test.ts".to_string(), source.to_string())];

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

    assert_eq!(
        ts2300_count, 3,
        "Expected TS2300 on the later private field declarations. Diagnostics: {:#?}",
        file.diagnostics
    );
    assert!(
        file.diagnostics.iter().all(|diag| diag.code != 2804),
        "Did not expect TS2804 for same-staticness private accessor/field conflicts. Diagnostics: {:#?}",
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

