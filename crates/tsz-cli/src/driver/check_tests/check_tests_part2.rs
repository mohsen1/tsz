    fn resolved_options_for_esnext_strict_test() -> ResolvedCompilerOptions {
        let mut args = default_cli_args_for_test();
        args.ignore_config = true;
        args.strict = true;
        args.target = Some(crate::args::Target::EsNext);

        let mut resolved = crate::config::resolve_compiler_options(None)
            .expect("resolve default compiler options");
        crate::driver::apply_cli_overrides(&mut resolved, &args).expect("apply cli overrides");
        if matches!(resolved.printer.module, ModuleKind::None) {
            resolved.printer.module = ModuleKind::ESNext;
            resolved.checker.module = ModuleKind::ESNext;
        }
        resolved
    }
    #[test]
    fn test_compile_inner_program_build_promise_is_assignable_to_promise_like() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"
declare const p: Promise<number>;
const q: PromiseLike<number> = p;
"#,
        )
        .expect("write source");

        let resolved = resolved_options_for_es2015_strict_test();
        let file_paths = vec![file_path];
        let SourceReadResult {
            sources,
            dependencies: _,
            module_resolutions: _,
            type_reference_errors,
            resolution_mode_errors,
            ..
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths = super::resolve_effective_lib_paths(
            &resolved,
            &sources,
            dir.path(),
            disable_default_libs,
        )
        .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let checker_libs = load_checker_libs(&lib_files);
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        let diagnostics = collect_diagnostics(
            &CollectDiagnosticsInput {
                program: &program,
                options: &resolved,
                base_dir: dir.path(),
                checker_libs: &checker_libs,
                typescript_dom_replacement_globals: (false, false, false),
                has_deprecation_diagnostics: false,
                collect_compile_stats: false,
            },
            None,
            &type_cache_output,
        )
        .diagnostics;

        assert!(
            diagnostics.is_empty(),
            "Expected compile-inner program build Promise<T> -> PromiseLike<T> assignability, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_compile_inner_program_reports_ts2851_for_async_iterator_await_using() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"
declare const ai: AsyncIterator<string, undefined>;
declare const aio: AsyncIteratorObject<string, undefined, unknown>;
declare const ag: AsyncGenerator<string, void>;

async function f() {
    await using it0 = aio;
    await using it1 = ag;
    await using it2 = ai;
}
"#,
        )
        .expect("write source");

        let resolved = resolved_options_for_esnext_strict_test();
        let file_paths = vec![file_path];
        let SourceReadResult {
            sources,
            dependencies: _,
            module_resolutions: _,
            type_reference_errors,
            resolution_mode_errors,
            ..
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths = super::resolve_effective_lib_paths(
            &resolved,
            &sources,
            dir.path(),
            disable_default_libs,
        )
        .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let checker_libs = load_checker_libs(&lib_files);
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        let diagnostics = collect_diagnostics(
            &CollectDiagnosticsInput {
                program: &program,
                options: &resolved,
                base_dir: dir.path(),
                checker_libs: &checker_libs,
                typescript_dom_replacement_globals: (false, false, false),
                has_deprecation_diagnostics: false,
                collect_compile_stats: false,
            },
            None,
            &type_cache_output,
        )
        .diagnostics;

        let ts2851_count = diagnostics.iter().filter(|diag| diag.code == 2851).count();
        assert_eq!(
            ts2851_count, 1,
            "Expected one TS2851 for await using AsyncIterator, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_compile_inner_program_reports_ts2456_for_recursive_mapped_type_aliases() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"
type Recurse = {
    [K in keyof Recurse]: Recurse[K]
}

type Recurse1 = {
    [K in keyof Recurse2]: Recurse2[K]
}

type Recurse2 = {
    [K in keyof Recurse1]: Recurse1[K]
}
"#,
        )
        .expect("write source");

        let resolved = resolved_options_for_es2015_strict_test();
        let file_paths = vec![file_path];
        let SourceReadResult {
            sources,
            dependencies: _,
            module_resolutions: _,
            type_reference_errors,
            resolution_mode_errors,
            ..
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths = super::resolve_effective_lib_paths(
            &resolved,
            &sources,
            dir.path(),
            disable_default_libs,
        )
        .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let checker_libs = load_checker_libs(&lib_files);
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        let diagnostics = collect_diagnostics(
            &CollectDiagnosticsInput {
                program: &program,
                options: &resolved,
                base_dir: dir.path(),
                checker_libs: &checker_libs,
                typescript_dom_replacement_globals: (false, false, false),
                has_deprecation_diagnostics: false,
                collect_compile_stats: false,
            },
            None,
            &type_cache_output,
        )
        .diagnostics;

        let ts2456_count = diagnostics.iter().filter(|diag| diag.code == 2456).count();
        assert_eq!(
            ts2456_count, 3,
            "Expected TS2456 for the recursive mapped aliases, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_collect_diagnostics_preserves_invariant_generic_error_elaboration_ts2322() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"// Repro from #19746

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
"#,
        )
        .expect("write source");

        let resolved = resolved_options_for_es2015_strict_test();
        let file_paths = vec![file_path];
        let SourceReadResult {
            sources,
            dependencies: _,
            module_resolutions: _,
            type_reference_errors,
            resolution_mode_errors,
            ..
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths = super::resolve_effective_lib_paths(
            &resolved,
            &sources,
            dir.path(),
            disable_default_libs,
        )
        .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let checker_libs = load_checker_libs(&lib_files);
        let direct_lib_contexts: Vec<LibContext> = lib_files
            .iter()
            .map(|lib| LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        let direct_checker_libs = CheckerLibSet {
            files: lib_files.clone(),
            contexts: Arc::new(direct_lib_contexts.clone()),
        };
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));
        let parallel_result =
            parallel::check_files_parallel(&program, &resolved.checker, &lib_files);
        let _parallel_ts2322_count = parallel_result
            .file_results
            .iter()
            .flat_map(|file| file.diagnostics.iter())
            .filter(|diag| diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
            .count();
        let rebuilt_binder = create_binder_from_bound_file(&program.files[0], &program, 0);
        program
            .type_interner
            .set_no_unchecked_indexed_access(resolved.checker.no_unchecked_indexed_access);
        program
            .type_interner
            .set_exact_optional_property_types(resolved.checker.exact_optional_property_types);
        let query_cache = tsz_solver::construction::QueryCache::new(&program.type_interner);
        let mut checker = CheckerState::with_options(
            &program.files[0].arena,
            &rebuilt_binder,
            &query_cache,
            program.files[0].file_name.clone(),
            &resolved.checker,
        );
        checker.ctx.set_lib_contexts(direct_lib_contexts.clone());
        checker
            .ctx
            .set_actual_lib_file_count(direct_lib_contexts.len());
        let all_arenas = Arc::new(
            program
                .files
                .iter()
                .map(|file| Arc::clone(&file.arena))
                .collect::<Vec<_>>(),
        );
        let all_binders = Arc::new(vec![Arc::new(create_binder_from_bound_file(
            &program.files[0],
            &program,
            0,
        ))]);
        checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
        if let Some(ref skel) = program.skeleton_index {
            let (exact, patterns) = skel.build_declared_module_sets();
            checker.ctx.set_declared_modules_from_skeleton(Arc::new(
                tsz::checker::context::GlobalDeclaredModules::from_skeleton(exact, patterns),
            ));
            checker
                .ctx
                .set_expando_index_from_skeleton(Arc::clone(&skel.expando_properties));
        }
        checker.ctx.set_all_binders(Arc::clone(&all_binders));
        checker.ctx.set_current_file_idx(0);
        checker.check_source_file(program.files[0].source_file);

        let source_file = program.files[0]
            .arena
            .get(program.files[0].source_file)
            .and_then(|node| program.files[0].arena.get_source_file(node))
            .expect("missing source file");
        let var_stmt_idx = *source_file
            .statements
            .nodes
            .first()
            .expect("variable statement");
        let var_stmt_node = program.files[0]
            .arena
            .get(var_stmt_idx)
            .expect("var stmt node");
        let var_stmt_data = program.files[0]
            .arena
            .get_variable(var_stmt_node)
            .expect("var stmt data");
        let decl_list_idx = *var_stmt_data
            .declarations
            .nodes
            .first()
            .expect("declaration list");
        let decl_list_node = program.files[0]
            .arena
            .get(decl_list_idx)
            .expect("decl list node");
        let decl_list_data = program.files[0]
            .arena
            .get_variable(decl_list_node)
            .expect("decl list data");
        let decl_idx = *decl_list_data
            .declarations
            .nodes
            .first()
            .expect("declaration");
        let decl_node = program.files[0].arena.get(decl_idx).expect("decl node");
        let decl = program.files[0]
            .arena
            .get_variable_declaration(decl_node)
            .expect("decl data");
        let _source_type = checker.get_type_of_node(decl.initializer);
        let target_type = checker.get_type_from_type_node(decl.type_annotation);
        let _read_constraint_type =
            |object_type| match tsz_solver::construction::QueryDatabase::resolve_property_access(
                &query_cache,
                object_type,
                "constraint",
            ) {
                tsz_solver::operations::property::PropertyAccessResult::Success {
                    type_id, ..
                } => Some(type_id),
                _ => None,
            };
        let _evaluated_target_type = {
            let mut evaluator = tsz_solver::computation::TypeEvaluator::with_resolver(
                &program.type_interner,
                &checker.ctx,
            );
            evaluator.evaluate(target_type)
        };
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());
        let direct_diagnostics = collect_diagnostics(
            &CollectDiagnosticsInput {
                program: &program,
                options: &resolved,
                base_dir: dir.path(),
                checker_libs: &direct_checker_libs,
                typescript_dom_replacement_globals: (false, false, false),
                has_deprecation_diagnostics: false,
                collect_compile_stats: false,
            },
            None,
            &type_cache_output,
        )
        .diagnostics;
        let direct_ts2322_count = direct_diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
            .count();
        assert_eq!(
            direct_ts2322_count, 2,
            "Expected collect_diagnostics with direct lib contexts to preserve two TS2322 diagnostics, got: {direct_diagnostics:?}"
        );

        let diagnostics = collect_diagnostics(
            &CollectDiagnosticsInput {
                program: &program,
                options: &resolved,
                base_dir: dir.path(),
                checker_libs: &checker_libs,
                typescript_dom_replacement_globals: (false, false, false),
                has_deprecation_diagnostics: false,
                collect_compile_stats: false,
            },
            None,
            &type_cache_output,
        )
        .diagnostics;

        let ts2322_count = diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
            .count();
        assert_eq!(
            ts2322_count, 2,
            "Expected compile-inner collect_diagnostics to preserve two TS2322 diagnostics, got: {diagnostics:?}"
        );
    }

    /// TS2883: Nested `node_modules` types should be detected as non-portable.
    /// When an exported variable's inferred type references a type from a
    /// nested `node_modules` (e.g., `foo/node_modules/nested`), tsz must
    /// emit TS2883 even when the nested package lacks a `package.json`.
    #[test]
    fn test_ts2883_nested_node_modules_non_portable_type() {
        let dir = tempfile::TempDir::new().expect("temp dir");

        // Create nested node_modules structure:
        // r/node_modules/foo/node_modules/nested/index.d.ts
        let nested_dir = dir.path().join("r/node_modules/foo/node_modules/nested");
        std::fs::create_dir_all(&nested_dir).expect("create nested dir");
        std::fs::write(
            nested_dir.join("index.d.ts"),
            "export interface NestedProps {}\n",
        )
        .expect("write nested/index.d.ts");

        let foo_dir = dir.path().join("r/node_modules/foo");
        std::fs::write(
            foo_dir.join("index.d.ts"),
            r#"import { NestedProps } from "nested";
export interface SomeProps {}
export function foo(): [SomeProps, NestedProps];
"#,
        )
        .expect("write foo/index.d.ts");

        std::fs::write(
            dir.path().join("r/entry.ts"),
            r#"import { foo } from "foo";
export const x = foo();
"#,
        )
        .expect("write r/entry.ts");

        let mut resolved = resolved_options_for_es2015_strict_test();
        resolved.checker.module = ModuleKind::CommonJS;
        resolved.printer.module = ModuleKind::CommonJS;
        resolved.checker.emit_declarations = true;

        let file_paths = vec![dir.path().join("r/entry.ts")];
        let SourceReadResult {
            sources,
            dependencies: _,
            module_resolutions: _,
            type_reference_errors,
            resolution_mode_errors,
            ..
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths = super::resolve_effective_lib_paths(
            &resolved,
            &sources,
            dir.path(),
            disable_default_libs,
        )
        .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let checker_libs = load_checker_libs(&lib_files);
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());
        let diagnostics = collect_diagnostics(
            &CollectDiagnosticsInput {
                program: &program,
                options: &resolved,
                base_dir: dir.path(),
                checker_libs: &checker_libs,
                typescript_dom_replacement_globals: (false, false, false),
                has_deprecation_diagnostics: false,
                collect_compile_stats: false,
            },
            None,
            &type_cache_output,
        )
        .diagnostics;

        let ts2883_diags: Vec<_> = diagnostics
            .iter()
            .filter(|diag| diag.code == 2883)
            .collect();
        assert!(
            !ts2883_diags.is_empty(),
            "Expected at least one TS2883 diagnostic for nested node_modules type reference, got: {diagnostics:?}"
        );
        assert!(
            ts2883_diags[0].message_text.contains("NestedProps"),
            "TS2883 message should reference 'NestedProps', got: {}",
            ts2883_diags[0].message_text
        );
        assert!(
            ts2883_diags[0]
                .message_text
                .contains("foo/node_modules/nested"),
            "TS2883 message should reference 'foo/node_modules/nested', got: {}",
            ts2883_diags[0].message_text
        );
    }

    #[test]
    fn test_collect_diagnostics_keeps_unimported_external_module_type_alias_unresolved() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        std::fs::write(
            dir.path().join("Helpers.ts"),
            r#"
export type StringKeyOf<TObj> = Extract<string, keyof TObj>;
"#,
        )
        .expect("write Helpers.ts");
        std::fs::write(
            dir.path().join("FromFactor.ts"),
            r#"
export type RowToColumns<TColumns> = {
    [TName in StringKeyOf<TColumns>]: any;
};
"#,
        )
        .expect("write FromFactor.ts");

        let mut resolved = resolved_options_for_es2015_strict_test();
        resolved.checker.module = ModuleKind::CommonJS;
        resolved.printer.module = ModuleKind::CommonJS;
        resolved.checker.emit_declarations = true;

        let file_paths = vec![
            dir.path().join("Helpers.ts"),
            dir.path().join("FromFactor.ts"),
        ];
        let SourceReadResult {
            sources,
            dependencies: _,
            module_resolutions: _,
            type_reference_errors,
            resolution_mode_errors,
            ..
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths = super::resolve_effective_lib_paths(
            &resolved,
            &sources,
            dir.path(),
            disable_default_libs,
        )
        .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let checker_libs = load_checker_libs(&lib_files);
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());
        let diagnostics = collect_diagnostics(
            &CollectDiagnosticsInput {
                program: &program,
                options: &resolved,
                base_dir: dir.path(),
                checker_libs: &checker_libs,
                typescript_dom_replacement_globals: (false, false, false),
                has_deprecation_diagnostics: false,
                collect_compile_stats: false,
            },
            None,
            &type_cache_output,
        )
        .diagnostics;

        assert!(
            diagnostics.iter().any(|diag| {
                diag.code == diagnostic_codes::CANNOT_FIND_NAME
                    && diag.message_text.contains("StringKeyOf")
            }),
            "Expected TS2304 for unimported external-module type alias in collect_diagnostics, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_is_declaration_file() {
        assert!(is_declaration_file("types.d.ts"));
        assert!(is_declaration_file("index.d.mts"));
        assert!(is_declaration_file("index.d.cts"));
        assert!(is_declaration_file("native.d.node.ts"));
        assert!(is_declaration_file("/path/to/file.d.ts"));
        assert!(is_declaration_file("/path/to/file.d.mts"));
        assert!(is_declaration_file("/path/to/file.d.cts"));
        assert!(is_declaration_file("/path/to/file.d.node.ts"));

        assert!(!is_declaration_file("index.ts"));
        assert!(!is_declaration_file("index.mts"));
        assert!(!is_declaration_file("index.cts"));
        assert!(!is_declaration_file("index.js"));
    }

    #[test]
    fn test_transitive_module_export_bridge_infers_type_only_flags() {
        let a_file = parallel::parse_and_bind_single(
            "/a.ts".to_string(),
            "export class A {}\nexport class B {}".to_string(),
        );
        let b_file = parallel::parse_and_bind_single(
            "/b.ts".to_string(),
            "export type * from \"./a\";".to_string(),
        );
        let c_file = parallel::parse_and_bind_single(
            "/c.ts".to_string(),
            "export * from \"./b\";".to_string(),
        );
        let d_file = parallel::parse_and_bind_single(
            "/d.ts".to_string(),
            r#"import { A, B } from "./c";
let _: A = new A();
let __: B = new B();"#
                .to_string(),
        );

        let program = parallel::merge_bind_results(vec![a_file, b_file, c_file, d_file]);
        let d_idx = 3;
        let d_bound = &program.files[d_idx];
        let mut binder = create_binder_from_bound_file(d_bound, &program, d_idx);

        let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
        resolved_module_paths.insert((d_idx, "./c".to_string()), 2);
        resolved_module_paths.insert((2, "./b".to_string()), 1);
        resolved_module_paths.insert((1, "./a".to_string()), 0);

        let module_specifiers = collect_module_specifiers(&d_bound.arena, d_bound.source_file);
        for (specifier, _, _, _) in &module_specifiers {
            if let Some(&target_idx) = resolved_module_paths.get(&(d_idx, specifier.clone())) {
                propagate_module_export_maps(
                    &mut binder,
                    specifier,
                    target_idx,
                    &program,
                    &resolved_module_paths,
                );
            }
        }

        assert!(binder.wildcard_reexports.contains_key("./c"));
        let c_wildcards = binder
            .wildcard_reexports
            .get("./c")
            .expect("expected wildcard re-exports for ./c");
        assert_eq!(c_wildcards, &vec!["./b".to_string()]);

        let b_wildcards = binder
            .wildcard_reexports
            .get("./b")
            .expect("expected wildcard re-exports for ./b");
        assert_eq!(b_wildcards, &vec!["./a".to_string()]);

        let b_type_only = binder
            .wildcard_reexports_type_only
            .get("./b")
            .expect("expected type-only metadata for ./b");
        assert!(
            b_type_only
                .iter()
                .any(|(source, is_type_only)| source == "./a" && *is_type_only)
        );

        let exports_via_c = binder
            .resolve_import_with_reexports_type_only("./c", "A")
            .expect("expected A to resolve via wildcard chain");
        assert!(exports_via_c.1, "A should be considered type-only via ./b");
    }

    #[test]
    fn test_collect_diagnostics_suppresses_ts2307_for_local_ambient_module() {
        let diagnostics = collect_test_diagnostics(&[
            (
                "/project/demo.d.ts",
                r#"
declare namespace demoNS {
    function f(): void;
}
declare module "demoModule" {
    import alias = demoNS;
    export = alias;
}
"#,
            ),
            (
                "/project/user.ts",
                r#"
import { f } from "demoModule";
let x1: string = demoNS.f;
let x2: string = f;
"#,
            ),
        ]);

        let codes = diagnostics.iter().map(|d| d.code).collect::<Vec<_>>();
        assert!(
            !codes.contains(&2307),
            "Did not expect TS2307 when a local ambient module declaration matches the import. Diagnostics: {diagnostics:?}"
        );
        assert_eq!(
            codes.iter().filter(|&&code| code == 2322).count(),
            2,
            "Expected the import to still resolve and produce two downstream TS2322 diagnostics. Diagnostics: {diagnostics:?}"
        );
    }

    #[test]
    fn test_collect_diagnostics_preserves_source_local_duplicate_package_paths() {
        let dir = std::env::temp_dir().join("tsz_check_duplicate_package_global_merge");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::create_dir_all(dir.join("tests")).unwrap();
        fs::create_dir_all(dir.join("node_modules/@types/react")).unwrap();
        fs::create_dir_all(dir.join("tests/node_modules/@types/react")).unwrap();

        fs::write(
            dir.join("node_modules/@types/react/package.json"),
            r#"{"name":"@types/react","version":"16.4.6"}"#,
        )
        .unwrap();
        fs::write(
            dir.join("tests/node_modules/@types/react/package.json"),
            r#"{"name":"@types/react","version":"16.4.6"}"#,
        )
        .unwrap();

        let root_react_path = dir.join("node_modules/@types/react/index.d.ts");
        let tests_react_path = dir.join("tests/node_modules/@types/react/index.d.ts");
        // Both stubs must be proper external modules so that import resolution
        // sees them as valid modules rather than producing TS2306/TS2669.
        fs::write(
            &root_react_path,
            "export declare function createElement(tag: string): any;\n",
        )
        .unwrap();
        fs::write(
            &tests_react_path,
            "export declare function createElement(tag: string): any;\n",
        )
        .unwrap();

        let src_index = dir.join("src/index.ts");
        let tests_index = dir.join("tests/index.ts");

        let options = ResolvedCompilerOptions {
            checker: tsz::checker::context::CheckerOptions {
                module: ModuleKind::CommonJS,
                target: tsz_common::common::ScriptTarget::ES2015,
                ..Default::default()
            },
            printer: tsz::emitter::PrinterOptions {
                module: ModuleKind::CommonJS,
                target: tsz_common::common::ScriptTarget::ES2015,
                ..Default::default()
            },
            module_suffixes: vec![String::new()],
            ..Default::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (
                    src_index.to_str().unwrap(),
                    "import * as React from 'react';\nexport var x = 1;\n",
                ),
                (
                    tests_index.to_str().unwrap(),
                    "import * as React from 'react';\nexport var y = 2;\n",
                ),
                (
                    root_react_path.to_str().unwrap(),
                    "export declare function createElement(tag: string): any;\n",
                ),
                (
                    tests_react_path.to_str().unwrap(),
                    "export declare function createElement(tag: string): any;\n",
                ),
            ],
            &options,
            &dir,
        );

        // With valid module stubs, both imports should resolve successfully.
        // The test primarily validates that having duplicate @types/react at
        // different node_modules depths does not crash or produce spurious errors.
        // No TS2307/TS2306/TS2669 diagnostics should appear.
        let error_codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
        assert!(
            !error_codes.contains(&2307)
                && !error_codes.contains(&2306)
                && !error_codes.contains(&2669),
            "expected no module resolution errors with valid react stubs, got: {diagnostics:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_collect_diagnostics_suppresses_ts2339_after_type_only_export_equals_namespace_use() {
        let dir = std::env::temp_dir().join("tsz_check_type_only_export_equals_namespace_use");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let a_path = dir.join("a.ts");
        let b_path = dir.join("b.ts");
        let f_path = dir.join("f.ts");

        let a_source = "export class A {}\n";
        let b_source = "import type * as types from './a';\nexport = types;\n";
        let f_source = "import * as types from './b';\nnew types.A();\n";

        fs::write(&a_path, a_source).unwrap();
        fs::write(&b_path, b_source).unwrap();
        fs::write(&f_path, f_source).unwrap();

        let options = ResolvedCompilerOptions {
            checker: tsz::checker::context::CheckerOptions {
                module: ModuleKind::CommonJS,
                target: tsz_common::common::ScriptTarget::ES2015,
                ..Default::default()
            },
            printer: tsz::emitter::PrinterOptions {
                module: ModuleKind::CommonJS,
                target: tsz_common::common::ScriptTarget::ES2015,
                ..Default::default()
            },
            es_module_interop: true,
            module_suffixes: vec![String::new()],
            ..Default::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (a_path.to_str().unwrap(), a_source),
                (b_path.to_str().unwrap(), b_source),
                (f_path.to_str().unwrap(), f_source),
            ],
            &options,
            &dir,
        );

        let f_diags: Vec<_> = diagnostics
            .iter()
            .filter(|diag| Path::new(&diag.file) == f_path.as_path() && diag.code != 2318)
            .collect();

        assert!(
            f_diags.iter().any(|diag| diag.code == 1361),
            "expected TS1361 on namespace use of a type-only export= chain, got: {f_diags:?}"
        );
        assert!(
            f_diags.iter().all(|diag| diag.code != 2339),
            "did not expect follow-on TS2339 once TS1361 fired, got: {f_diags:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_collect_diagnostics_allows_checked_js_module_exports_type_only_require() {
        let dir = std::env::temp_dir().join("tsz_check_js_module_exports_type_only_require");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let importer_path = dir.join("importer.cjs");
        let exporter_path = dir.join("exporter.mts");

        let importer_source = "const Foo = require(\"./exporter.mjs\");\nnew Foo();\n";
        let exporter_source =
            "export default class Foo {}\nexport type { Foo as \"module.exports\" };\n";

        fs::write(&importer_path, importer_source).unwrap();
        fs::write(&exporter_path, exporter_source).unwrap();

        let options = ResolvedCompilerOptions {
            allow_js: true,
            check_js: true,
            module_resolution: Some(crate::config::ModuleResolutionKind::Node16),
            module_suffixes: vec![String::new()],
            printer: tsz::emitter::PrinterOptions {
                module: ModuleKind::Node18,
                target: tsz_common::common::ScriptTarget::ES2023,
                ..Default::default()
            },
            checker: tsz::checker::context::CheckerOptions {
                module: ModuleKind::Node18,
                target: tsz_common::common::ScriptTarget::ES2023,
                ..Default::default()
            },
            ..Default::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (importer_path.to_str().unwrap(), importer_source),
                (exporter_path.to_str().unwrap(), exporter_source),
            ],
            &options,
            &dir,
        );

        let importer_diags: Vec<_> = diagnostics
            .iter()
            .filter(|diag| Path::new(&diag.file) == importer_path.as_path())
            .collect();

        assert!(
            importer_diags.is_empty(),
            "expected checked CommonJS require() of a type-only \
             \"module.exports\" binding to avoid diagnostics, got: {importer_diags:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_collect_diagnostics_rejects_exports_in_cjs_file_with_esm_syntax() {
        let dir = std::env::temp_dir().join("tsz_check_js_cjs_exports_with_esm_syntax");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let two_path = dir.join("2.cjs");
        let three_path = dir.join("3.cjs");
        let four_path = dir.join("4.cjs");
        let five_path = dir.join("5.cjs");

        let two_source = "exports.foo = 0;\n";
        let three_source = "import \"foo\";\nexports.foo = {};\n";
        let four_source = ";\n";
        let five_source =
            "import two from \"./2.cjs\";\nimport three from \"./3.cjs\";\ntwo.foo;\nthree.foo;\n";

        let options = ResolvedCompilerOptions {
            allow_js: true,
            check_js: true,
            module_resolution: Some(crate::config::ModuleResolutionKind::NodeNext),
            module_suffixes: vec![String::new()],
            printer: tsz::emitter::PrinterOptions {
                module: ModuleKind::Node20,
                target: tsz_common::common::ScriptTarget::ES2022,
                ..Default::default()
            },
            checker: tsz::checker::context::CheckerOptions {
                module: ModuleKind::Node20,
                target: tsz_common::common::ScriptTarget::ES2022,
                no_types_and_symbols: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (two_path.to_str().unwrap(), two_source),
                (three_path.to_str().unwrap(), three_source),
                (four_path.to_str().unwrap(), four_source),
                (five_path.to_str().unwrap(), five_source),
            ],
            &options,
            &dir,
        );

        let three_diags: Vec<_> = diagnostics
            .iter()
            .filter(|diag| Path::new(&diag.file) == three_path.as_path())
            .collect();
        let five_diags: Vec<_> = diagnostics
            .iter()
            .filter(|diag| Path::new(&diag.file) == five_path.as_path())
            .collect();

        assert!(
            three_diags.iter().any(|diag| {
                diag.code == 2304 && diag.message_text.contains("Cannot find name 'exports'")
            }),
            "expected TS2304 for exports in a .cjs file with ESM syntax, got: {three_diags:?}"
        );
        assert!(
            five_diags
                .iter()
                .any(|diag| { diag.code == 1192 && diag.message_text.contains("Module '\"3\"'") }),
            "expected TS1192 for default import from the ESM-syntax .cjs file, got file diagnostics: {five_diags:?}; all diagnostics: {diagnostics:?}"
        );
        assert!(
            five_diags.iter().all(|diag| {
                !(diag.code == 1192 && diag.message_text.contains("Module '\"2\"'"))
            }),
            "did not expect TS1192 for default import from plain CommonJS .cjs, got: {five_diags:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_collect_diagnostics_bundler_dual_package_modes_do_not_emit_ts2305() {
        let dir = std::env::temp_dir().join("tsz_check_bundler_dual_package_modes");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("node_modules/dual")).unwrap();

        let package_json_path = dir.join("node_modules/dual/package.json");
        let index_js_path = dir.join("node_modules/dual/index.js");
        let index_d_ts_path = dir.join("node_modules/dual/index.d.ts");
        let index_cjs_path = dir.join("node_modules/dual/index.cjs");
        let index_d_cts_path = dir.join("node_modules/dual/index.d.cts");
        let main_ts_path = dir.join("main.ts");
        let main_mts_path = dir.join("main.mts");
        let main_cts_path = dir.join("main.cts");

        fs::write(
            &package_json_path,
            r#"{
  "name": "dual",
  "version": "1.0.0",
  "type": "module",
  "main": "index.cjs",
  "types": "index.d.cts",
  "exports": {
    ".": {
      "import": "./index.js",
      "require": "./index.cjs"
    }
  }
}
"#,
        )
        .unwrap();
        fs::write(&index_js_path, "export const esm = 0;\n").unwrap();
        fs::write(&index_d_ts_path, "export const esm: number;\n").unwrap();
        fs::write(&index_cjs_path, "exports.cjs = 0;\n").unwrap();
        fs::write(&index_d_cts_path, "export const cjs: number;\n").unwrap();
        fs::write(&main_ts_path, "import { esm, cjs } from \"dual\";\n").unwrap();
        fs::write(&main_mts_path, "import { esm, cjs } from \"dual\";\n").unwrap();
        fs::write(&main_cts_path, "import { esm, cjs } from \"dual\";\n").unwrap();

        let options = ResolvedCompilerOptions {
            module_resolution: Some(crate::config::ModuleResolutionKind::Bundler),
            resolve_package_json_exports: true,
            module_suffixes: vec![String::new()],
            printer: tsz::emitter::PrinterOptions {
                module: ModuleKind::Preserve,
                target: tsz_common::common::ScriptTarget::ES2015,
                ..Default::default()
            },
            checker: tsz::checker::context::CheckerOptions {
                module: ModuleKind::Preserve,
                target: tsz_common::common::ScriptTarget::ES2015,
                ..Default::default()
            },
            ..Default::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (index_js_path.to_str().unwrap(), "export const esm = 0;\n"),
                (
                    index_d_ts_path.to_str().unwrap(),
                    "export const esm: number;\n",
                ),
                (index_cjs_path.to_str().unwrap(), "exports.cjs = 0;\n"),
                (
                    index_d_cts_path.to_str().unwrap(),
                    "export const cjs: number;\n",
                ),
                (
                    main_ts_path.to_str().unwrap(),
                    "import { esm, cjs } from \"dual\";\n",
                ),
                (
                    main_mts_path.to_str().unwrap(),
                    "import { esm, cjs } from \"dual\";\n",
                ),
                (
                    main_cts_path.to_str().unwrap(),
                    "import { esm, cjs } from \"dual\";\n",
                ),
            ],
            &options,
            &dir,
        );

        let import_diags: Vec<_> = diagnostics
            .iter()
            .filter(|diag| {
                let file = Path::new(&diag.file);
                (file == main_ts_path.as_path()
                    || file == main_mts_path.as_path()
                    || file == main_cts_path.as_path())
                    && diag.code != 2318
            })
            .collect();

        assert!(
            import_diags.iter().all(|diag| diag.code != 2305),
            "expected no TS2305 for bundler dual-package import/require mode selection, got: {import_diags:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }
