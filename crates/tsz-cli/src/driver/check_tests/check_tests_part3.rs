    #[cfg(unix)]
    #[test]
    fn test_collect_diagnostics_preserve_symlinks_keeps_original_target_error() {
        use std::fs;
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("linked")).unwrap();
        fs::create_dir_all(dir.path().join("app/node_modules/real")).unwrap();
        fs::create_dir_all(dir.path().join("app/node_modules/linked")).unwrap();
        fs::create_dir_all(dir.path().join("app/node_modules/linked2")).unwrap();

        fs::write(
            dir.path().join("linked/index.d.ts"),
            "export { real } from \"real\";\nexport class C { private x; }\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("app/node_modules/real/index.d.ts"),
            "export const real: string;\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("app/app.ts"),
            "/// <reference types=\"linked\" />\nimport { C as C1 } from \"linked\";\nimport { C as C2 } from \"linked2\";\nlet x = new C1();\nx = new C2();\n",
        )
        .unwrap();
        symlink(
            dir.path().join("linked/index.d.ts"),
            dir.path().join("app/node_modules/linked/index.d.ts"),
        )
        .unwrap();
        symlink(
            dir.path().join("linked/index.d.ts"),
            dir.path().join("app/node_modules/linked2/index.d.ts"),
        )
        .unwrap();

        let resolved = ResolvedCompilerOptions {
            module_resolution: Some(crate::config::ModuleResolutionKind::Bundler),
            preserve_symlinks: true,
            module_suffixes: vec![String::new()],
            printer: tsz::emitter::PrinterOptions {
                module: ModuleKind::ES2015,
                ..Default::default()
            },
            checker: tsz::checker::context::CheckerOptions {
                module: ModuleKind::ES2015,
                ..Default::default()
            },
            ..Default::default()
        };

        let file_paths = vec![
            dir.path().join("linked/index.d.ts"),
            dir.path().join("app/app.ts"),
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

        let source_paths: FxHashSet<PathBuf> =
            sources.iter().map(|source| source.path.clone()).collect();
        assert!(source_paths.contains(&dir.path().join("linked/index.d.ts")));
        assert!(source_paths.contains(&dir.path().join("app/node_modules/linked/index.d.ts")));
        assert!(source_paths.contains(&dir.path().join("app/node_modules/linked2/index.d.ts")));

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
            &[],
        ));
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        let diagnostics = collect_diagnostics(
            &CollectDiagnosticsInput {
                program: &program,
                options: &resolved,
                base_dir: dir.path(),
                reference_path_current_directory: None,
                checker_libs: &CheckerLibSet::default(),
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
                diag.code
                    == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
                    && diag.file.contains("linked/index.d.ts")
            }),
            "expected TS2307 for original linked target, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_collect_diagnostics_preserves_mapped_type_generic_indexed_access_context() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"// repro from #49242

type Types = {
    [key: string]: object;
};

type Filled<T extends Types> = {
    [K in keyof T]: [T[K]];
}

class Test<Types extends {
    [key: string]: object;
}> {
    entries: {
        [T in keyof Types]?: Types[T][];
    } = {}

    get<T extends keyof Types>(name: T): Filled<Pick<Types, T>> {
        let entry = this.entries[name];
        if (entry) return { [name]: [entry[0]] } as Filled<Pick<Types, T>>;
        throw new Error("Entry not found");
    }
}

// repro from #49338

type TypesMap = {
    0: {
        foo: string,
    };
    1: {
        a: number,
    };
}
type P<T extends keyof TypesMap> = {
    t: T;
} & TypesMap[T];
type Handlers = { [M in keyof TypesMap]?: (p: P<M>) => void };
const typeHandlers: Handlers = {
    [0]: (p) => console.log(p.foo),
    [1]: (p) => console.log(p.a),
};

const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) => typeHandlers[p.t]?.(p);
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
                reference_path_current_directory: None,
                checker_libs: &checker_libs,
                typescript_dom_replacement_globals: (false, false, false),
                has_deprecation_diagnostics: false,
                collect_compile_stats: false,
            },
            None,
            &type_cache_output,
        )
        .diagnostics;

        let relevant = diagnostics
            .iter()
            .filter(|diag| {
                matches!(
                    diag.code,
                    diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT
                        | diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
                )
            })
            .collect::<Vec<_>>();

        assert!(
            relevant.is_empty(),
            "Expected mapped generic indexed access repro to keep context in collect_diagnostics, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_collect_diagnostics_preserves_recursive_mapped_type_callback_context() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"type MorphTuple = [string, "|>", any]

type validateMorph<def extends MorphTuple> = def[1] extends "|>"
    ? [validateDefinition<def[0]>, "|>", (In: def[0]) => unknown]
    : def

type validateDefinition<def> = def extends MorphTuple
    ? validateMorph<def>
    : {
          [k in keyof def]: validateDefinition<def[k]>
      }

declare function type<def>(def: validateDefinition<def>): def

const shallow = type(["ark", "|>", (x) => x.length])
const objectLiteral = type({ a: ["ark", "|>", (x) => x.length] })
const nestedTuple = type([["ark", "|>", (x) => x.length]])
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
                reference_path_current_directory: None,
                checker_libs: &checker_libs,
                typescript_dom_replacement_globals: (false, false, false),
                has_deprecation_diagnostics: false,
                collect_compile_stats: false,
            },
            None,
            &type_cache_output,
        )
        .diagnostics;

        let relevant = diagnostics
            .iter()
            .filter(|diag| {
                matches!(
                    diag.code,
                    diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
                )
            })
            .collect::<Vec<_>>();

        assert!(
            relevant.is_empty(),
            "Expected recursive mapped-type callback repro to keep context in collect_diagnostics, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_collect_diagnostics_preserves_union_array_method_alias_callback_context() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"interface Fizz { id: number; fizz: string }
interface Buzz { id: number; buzz: string }
interface Arr<T> {
  filter<S extends T>(pred: (value: T) => value is S): S[];
  filter(pred: (value: T) => unknown): T[];
}
declare const m: Arr<Fizz>["filter"] | Arr<Buzz>["filter"];
m(item => item.id < 5);
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
                reference_path_current_directory: None,
                checker_libs: &checker_libs,
                typescript_dom_replacement_globals: (false, false, false),
                has_deprecation_diagnostics: false,
                collect_compile_stats: false,
            },
            None,
            &type_cache_output,
        )
        .diagnostics;

        let relevant = diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE)
            .collect::<Vec<_>>();

        assert!(
            relevant.is_empty(),
            "Expected union overloaded array method alias repro to keep context in collect_diagnostics, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_collect_diagnostics_preserves_union_builtin_array_method_callback_context() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"interface Fizz { id: number; fizz: string }
interface Buzz { id: number; buzz: string }

([] as Fizz[] | Buzz[]).filter(item => item.id < 5);
([] as Fizz[] | readonly Buzz[]).filter(item => item.id < 5);
([] as Fizz[] | Buzz[]).find(item => item);
([] as Fizz[] | Buzz[]).every(item => item.id < 5);
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
                reference_path_current_directory: None,
                checker_libs: &checker_libs,
                typescript_dom_replacement_globals: (false, false, false),
                has_deprecation_diagnostics: false,
                collect_compile_stats: false,
            },
            None,
            &type_cache_output,
        )
        .diagnostics;

        let relevant = diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE)
            .collect::<Vec<_>>();

        assert!(
            relevant.is_empty(),
            "Expected union built-in array method repro to keep context in collect_diagnostics, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_collect_diagnostics_reports_implicit_any_for_primitive_union_property_callback() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"type Validate = (text: string, pos: number, self: Rule) => number | boolean;
interface FullRule {
  validate: string | RegExp | Validate;
  normalize?: (match: {x: string}) => void;
}

type Rule = string | FullRule;

const obj: {field: Rule} = {
  field: {
    validate: (_t, _p, _s) => false,
    normalize: match => match.x,
  }
};
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
                reference_path_current_directory: None,
                checker_libs: &checker_libs,
                typescript_dom_replacement_globals: (false, false, false),
                has_deprecation_diagnostics: false,
                collect_compile_stats: false,
            },
            None,
            &type_cache_output,
        )
        .diagnostics;

        let relevant = diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE)
            .collect::<Vec<_>>();

        assert_eq!(
            relevant.len(),
            1,
            "Expected exactly one TS7006 for the primitive-union normalize callback, got: {diagnostics:?}"
        );
    }

    #[test]
    fn real_syntax_errors_suppress_cross_file_type_diagnostics() {
        let diagnostics = collect_test_diagnostics(&[
            ("/a.ts", "const x =\n"),
            ("/b.ts", "const y: number = \"s\";\n"),
        ]);

        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.file == "/a.ts" && diag.code == 1109),
            "expected the real syntax error to remain: {diagnostics:?}"
        );
        assert!(
            !diagnostics
                .iter()
                .any(|diag| diag.file == "/b.ts" && diag.code == 2322),
            "did not expect TS2322 when another file has a real syntax error: {diagnostics:?}"
        );
    }

    #[test]
    fn collect_diagnostics_reports_default_lib_breakage_from_global_node_merge() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"
const enum SyntaxKind {
    Modifier,
    Decorator,
}

interface Node {
    kind: SyntaxKind;
}

interface Modifier extends Node { kind: SyntaxKind.Modifier; }
interface Decorator extends Node { kind: SyntaxKind.Decorator; }

declare function isModifier(node: Node): node is Modifier;
declare function isDecorator(node: Node): node is Decorator;

declare function every<T, U extends T>(array: readonly T[], callback: (element: T) => element is U): array is readonly U[];

declare const modifiers: readonly Decorator[] | readonly Modifier[];

function foo() {
    every(modifiers, isModifier);
    every(modifiers, isDecorator);
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
                reference_path_current_directory: None,
                checker_libs: &checker_libs,
                typescript_dom_replacement_globals: (false, false, false),
                has_deprecation_diagnostics: false,
                collect_compile_stats: false,
            },
            None,
            &type_cache_output,
        )
        .diagnostics;

        let lib_dom_diagnostics = diagnostics
            .iter()
            .filter(|diag| diag.file.ends_with("lib.dom.d.ts"))
            .collect::<Vec<_>>();
        let ts2344_count = lib_dom_diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT)
            .count();
        let ts2430_count = lib_dom_diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE)
            .count();

        // tsc reports three TS2344 diagnostics here: the apparent
        // `HTMLElementTagNameMap[K]` value union includes `HTMLTrackElement`,
        // whose existing `kind: string` property conflicts with the merged
        // `Node.kind: SyntaxKind` property.
        assert_eq!(
            ts2344_count, 3,
            "Expected three TS2344 diagnostics from lib.dom.d.ts after merging Node.kind, got: {diagnostics:?}"
        );
        assert_eq!(
            ts2430_count, 1,
            "Expected one TS2430 diagnostic from lib.dom.d.ts after merging Node.kind, got: {diagnostics:?}"
        );
    }

    #[test]
    fn default_lib_validation_ignores_unresolved_overload_cascades_after_global_merge() {
        let diagnostics = collect_es2015_default_lib_diagnostics(
            r#"
interface HTMLElement {
    type: string;
}
"#,
        );

        assert!(
            !diagnostics.iter().any(|diag| {
                diag.file.ends_with("lib.dom.d.ts")
                    && diag.code == diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE
            }),
            "Did not expect default-lib TS2430 diagnostics from unrelated unresolved overload parameters, got: {diagnostics:?}"
        );
    }

    #[test]
    fn skip_lib_check_skips_default_lib_recheck_after_global_merge() {
        let diagnostics = collect_es2015_default_lib_diagnostics_with_options(
            r#"
const enum SyntaxKind {
    Modifier,
    Decorator,
}

interface Node {
    kind: SyntaxKind;
}

interface Modifier extends Node { kind: SyntaxKind.Modifier; }
interface Decorator extends Node { kind: SyntaxKind.Decorator; }

declare function isModifier(node: Node): node is Modifier;
declare function isDecorator(node: Node): node is Decorator;

declare function every<T, U extends T>(array: readonly T[], callback: (element: T) => element is U): array is readonly U[];

declare const modifiers: readonly Decorator[] | readonly Modifier[];

function foo() {
    every(modifiers, isModifier);
    every(modifiers, isDecorator);
}
"#,
            |resolved| {
                resolved.skip_lib_check = true;
            },
        );

        assert!(
            !diagnostics.iter().any(|diag| {
                diag.file.ends_with("lib.dom.d.ts")
                    && matches!(
                        diag.code,
                        diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT
                            | diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE
                    )
            }),
            "Did not expect lib.dom.d.ts TS2344/TS2430 diagnostics when skipLibCheck is enabled, got: {diagnostics:?}"
        );
    }

    #[test]
    fn default_lib_validation_keeps_select_option_index_compatible_after_html_element_merge() {
        let diagnostics = collect_es2015_default_lib_diagnostics(
            r#"
declare global {
    interface ElementTagNameMap {
        [index: number]: HTMLElement
    }

    interface HTMLElement {
        [index: number]: HTMLElement;
    }
}

export {};
"#,
        );

        let lib_ts2430 = diagnostics
            .iter()
            .filter(|diag| {
                diag.file.ends_with("lib.dom.d.ts")
                    && diag.code == diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE
            })
            .collect::<Vec<_>>();

        assert!(
            lib_ts2430
                .iter()
                .any(|diag| diag.message_text.contains("HTMLFormElement")),
            "Expected the real HTMLFormElement numeric-index incompatibility, got: {diagnostics:?}"
        );
        assert!(
            !lib_ts2430
                .iter()
                .any(|diag| diag.message_text.contains("HTMLSelectElement")),
            "Did not expect HTMLSelectElement to fail: its option/group index values inherit HTMLElement. Got: {diagnostics:?}"
        );
    }

    #[test]
    fn default_lib_validation_normalizes_cross_arena_method_members_after_global_merge() {
        let diagnostics = collect_es2015_default_lib_diagnostics(
            r#"
interface HTMLElement {
    clientWidth: number;
    isDisabled: boolean;
}

declare var document: Document;
interface Document {
    getElementById(elementId: string): HTMLElement;
}
"#,
        );

        assert!(
            !diagnostics.iter().any(|diag| {
                diag.file.ends_with("lib.dom.d.ts")
                    && diag.code == diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE
            }),
            "Did not expect default-lib TS2430 diagnostics when a cross-arena method override is compatible, got: {diagnostics:?}"
        );
    }

    /// Regression for #17641: the membership-monotone guard on the lib
    /// `symbol_types` write (#17631) rejected a thin re-derivation but still
    /// returned and name-cached it, splitting the merged `HTMLElement` into two
    /// `TypeId`s. The two identities then met in `elements.map(...)`'s
    /// contextual-parameter relation and mis-fired a TS2345 whose argument and
    /// parameter types render identically. The full
    /// `compiler/genericMethodOverspecialization.ts` fixture must stay clean
    /// (tsc reports nothing: the user-arena `getElementById` overload wins and
    /// every callback checks against one `HTMLElement` identity).
    #[test]
    fn global_lib_merge_keeps_one_element_identity_for_array_callbacks() {
        let diagnostics = collect_es2015_default_lib_diagnostics(
            r#"
var names = ["list", "table1", "table2", "table3", "summary"];

interface HTMLElement {
    clientWidth: number;
    isDisabled: boolean;
}

declare var document: Document;
interface Document {
    getElementById(elementId: string): HTMLElement;
}

var elements = names.map(function (name) {
    return document.getElementById(name);
});


var xxx = elements.filter(function (e) {
    return !e.isDisabled;
});

var widths: number[] = elements.map(function (e) {
    return e.clientWidth;
});
"#,
        );

        assert!(
            !diagnostics.iter().any(|diag| {
                diag.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
            }),
            "Did not expect a TS2345 from a split lib-interface identity, got: {diagnostics:?}"
        );
        assert!(
            diagnostics.is_empty(),
            "Expected the merged-lib-interface fixture to be clean like tsc, got: {diagnostics:?}"
        );
    }

    /// Characterization guard for #16308: a *user* interface that extends a lib
    /// generic (`interface IObservableArray<T> extends Array<T>`), reached from a
    /// *different* file through a re-export barrel cycle (mobx's `internal.ts`
    /// shape), must keep every inherited `Array` member visible at the cross-file
    /// use site — `tsc` is clean. The underlying defect is a raw-`SymbolId`
    /// `symbol_types` collision across arena binders (#14344/#14921): a homonym
    /// arena's degraded, member-less `Array` is handed to the interface's
    /// heritage merge, dropping `map`/`slice`/`length`/`forEach` (false TS2339)
    /// while the interface's own members still resolve. The fix resolves a
    /// lib-global heritage base through the name-keyed canonical lib path.
    ///
    /// The *deterministic* witness that fails pre-fix is the mobx project row
    /// (10 diagnostics -> 0); the exact `SymbolId` collision is order/scale
    /// dependent and does not reduce to a unit-test-sized program (the issue's
    /// own reduction attempts confirmed this), so this multi-arena case is kept
    /// as a coarse guard: it holds on both sides today but catches any broader
    /// regression that drops inherited lib-generic members across files. Uses the
    /// multi-file (real multi-arena) driver — the in-process single-checker path
    /// shares binders and would not exercise the cross-arena delegation at all.
    #[test]
    fn user_interface_extends_lib_generic_keeps_inherited_members_across_barrel_cycle() {
        // Two user interfaces (distinct binder names, per the anti-hardcoding
        // gate) that each extend a lib generic, declared in separate files and
        // reached only through the `internal.ts` re-export barrel — plus a fan
        // of decoy modules and consumers so the program allocates enough
        // cross-arena symbols to exercise the raw-`SymbolId` `symbol_types`
        // collision (#14344/#14921) that dropped the inherited members.
        let mut files: Vec<(String, String)> = vec![(
            "helper.ts".to_string(),
            "export function shared_helper(): unknown { return {}; }\n".to_string(),
        )];
        let mut barrel = String::from("export * from \"./helper\";\n");

        let interfaces = [
            ("IObservableArray", "observablearray", "mx"),
            ("BespokeArray", "bespokearray", "zz"),
        ];
        for (decl_name, file_stem, prefix) in interfaces {
            files.push((
                format!("{file_stem}.ts"),
                format!(
                    "import {{ shared_helper }} from \"./internal\";\n\
                     export interface {decl_name}<T = any> extends Array<T> {{\n\
                     \x20   {prefix}_extra(): T;\n\
                     }}\n\
                     export function {prefix}_make<T>(): {decl_name}<T> {{ return shared_helper() as any; }}\n"
                ),
            ));
            barrel.push_str(&format!("export * from \"./{file_stem}\";\n"));

            // Several consuming files per interface, each in its own arena,
            // exercising the inherited members after a cross-file import.
            for tag in ["one", "two", "three"] {
                let consumer = format!("consumer_{prefix}_{tag}");
                files.push((
                    format!("{consumer}.ts"),
                    format!(
                        "import {{ {decl_name} }} from \"./internal\";\n\
                         export function {consumer}(a: {decl_name}<number>): number {{\n\
                         \x20   const mapped = a.map((x) => x + 1);\n\
                         \x20   a.slice(0, 1);\n\
                         \x20   a.forEach((x) => void x);\n\
                         \x20   return a.length + mapped.length;\n\
                         }}\n"
                    ),
                ));
                barrel.push_str(&format!("export * from \"./{consumer}\";\n"));
            }
        }

        // Decoy modules that also cycle back through the barrel. The count is
        // load-bearing: the raw-`SymbolId` collision only surfaces once the
        // program allocates enough cross-arena symbols, so keep it generous.
        const DECOY_MODULES: usize = 24;
        for idx in 0..DECOY_MODULES {
            let name = format!("mod_{idx}");
            files.push((
                format!("{name}.ts"),
                format!(
                    "import {{ shared_helper }} from \"./internal\";\n\
                     export const val_{idx} = {idx};\n\
                     export interface If_{idx} {{ m_{idx}(): number; }}\n\
                     export function fn_{idx}() {{ return shared_helper(); }}\n"
                ),
            ));
            barrel.push_str(&format!("export * from \"./{name}\";\n"));
        }
        files.push(("internal.ts".to_string(), barrel));

        let file_refs: Vec<(&str, &str)> = files
            .iter()
            .map(|(name, src)| (name.as_str(), src.as_str()))
            .collect();
        let diagnostics = collect_es2015_default_lib_diagnostics_multifile(&file_refs);

        let inherited_member_failures: Vec<_> = diagnostics
            .iter()
            .filter(|diag| {
                diag.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE
                    && (diag.message_text.contains("IObservableArray")
                        || diag.message_text.contains("BespokeArray"))
                    && ["map", "slice", "forEach", "length"]
                        .iter()
                        .any(|member| diag.message_text.contains(&format!("Property '{member}'")))
            })
            .collect();

        assert!(
            inherited_member_failures.is_empty(),
            "inherited Array members must resolve cross-file through the barrel cycle (#16308), \
             got: {inherited_member_failures:?}; all: {diagnostics:?}"
        );
    }

    #[test]
    fn collect_diagnostics_respects_skip_default_lib_check_for_global_node_merge() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(
            &file_path,
            r#"
const enum SyntaxKind {
    Modifier,
    Decorator,
}

interface Node {
    kind: SyntaxKind;
}
"#,
        )
        .expect("write source");

        let mut resolved = resolved_options_for_es2015_strict_test();
        resolved.skip_default_lib_check = true;
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
                reference_path_current_directory: None,
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
            !diagnostics.iter().any(|diag| {
                diag.file.ends_with("lib.dom.d.ts")
                    && matches!(
                        diag.code,
                        diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT
                            | diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE
                    )
            }),
            "Did not expect lib.dom.d.ts TS2344/TS2430 diagnostics when skipDefaultLibCheck is enabled, got: {diagnostics:?}"
        );
    }

    #[test]
    fn real_syntax_errors_preserve_checker_grammar_diagnostics() {
        // Without `declare`, the parser falls through to expression-statement
        // parsing and never produces a `TypeAliasDeclaration`, so TS2457 would
        // not be emitted and this test would vacuously pass.
        let diagnostics = collect_test_diagnostics(&[
            ("/a.ts", "const x =\n"),
            ("/b.ts", "declare type void = string;\n"),
        ]);

        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.file == "/b.ts" && diag.code == 2457),
            "expected TS2457 to survive program-level syntax suppression: {diagnostics:?}"
        );
    }

    #[test]
    fn tarjan_scc_no_edges() {
        let adj = vec![vec![], vec![], vec![]];
        let sccs = tarjan_scc(3, &adj);
        // Each node is its own SCC
        assert_eq!(sccs.len(), 3);
        for scc in &sccs {
            assert_eq!(scc.len(), 1);
        }
    }

    #[test]
    fn tarjan_scc_linear_chain() {
        // 0 -> 1 -> 2 (no cycles)
        let adj = vec![vec![1], vec![2], vec![]];
        let sccs = tarjan_scc(3, &adj);
        assert_eq!(sccs.len(), 3);
        for scc in &sccs {
            assert_eq!(scc.len(), 1);
        }
    }

    #[test]
    fn tarjan_scc_simple_cycle() {
        // 0 -> 1 -> 0 (one cycle of size 2)
        let adj = vec![vec![1], vec![0]];
        let sccs = tarjan_scc(2, &adj);
        assert_eq!(sccs.len(), 1);
        assert_eq!(sccs[0].len(), 2);
    }

    #[test]
    fn tarjan_scc_triangle_cycle() {
        // 0 -> 1 -> 2 -> 0 (one cycle of size 3)
        let adj = vec![vec![1], vec![2], vec![0]];
        let sccs = tarjan_scc(3, &adj);
        assert_eq!(sccs.len(), 1);
        assert_eq!(sccs[0].len(), 3);
    }

    #[test]
    fn tarjan_scc_mixed() {
        // 0 -> 1 -> 2 -> 1 (cycle {1,2}), 3 standalone
        let adj = vec![vec![1], vec![2], vec![1], vec![]];
        let sccs = tarjan_scc(4, &adj);
        let cycles: Vec<_> = sccs.iter().filter(|s| s.len() > 1).collect();
        assert_eq!(cycles.len(), 1, "expected exactly one cycle");
        assert_eq!(cycles[0].len(), 2, "cycle should have 2 nodes");
    }

    #[test]
    fn real_syntax_errors_preserve_reserved_interface_name_diagnostics() {
        let diagnostics = collect_test_diagnostics(&[
            ("/a.ts", "const x =\n"),
            ("/b.ts", "function function() {}\ninterface void {}\n"),
        ]);

        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.file == "/b.ts" && diag.code == 2427),
            "expected TS2427 to survive parse-error suppression: {diagnostics:?}"
        );
    }

    // TS2499 ("An interface can only extend an identifier/qualified-name with
    // optional type arguments") is owned solely by the checker's generic
    // heritage walk (`heritage.rs`), which rejects every non-identifier/
    // qualified-name heritage node. The parser used to also report it for the
    // parenthesized/bracketed shape, producing a parser+checker double-report
    // that a position-keyed dedup in `post_process_checker_diagnostics` then
    // had to strip; the parser no longer emits it, so the checker is the
    // single owner. tsc emits it exactly once (oracle: `typescript@7.0.2`);
    // these pin the single emission (Direction A) and the checker keep-gate's
    // Direction-B suppression under a real syntax error.

    fn ts2499_count(diagnostics: &[Diagnostic], file: &str) -> usize {
        diagnostics
            .iter()
            .filter(|diag| diag.file == file && diag.code == 2499)
            .count()
    }

    #[test]
    fn interface_extends_parenthesized_expression_reports_ts2499_once() {
        let diagnostics =
            collect_test_diagnostics(&[("/a.ts", "interface I extends (1 + 2) {}\n")]);
        assert_eq!(
            ts2499_count(&diagnostics, "/a.ts"),
            1,
            "expected exactly one TS2499, not a parser+checker double-report: {diagnostics:?}"
        );
    }

    #[test]
    fn interface_extends_bracketed_expression_reports_ts2499_once() {
        let diagnostics =
            collect_test_diagnostics(&[("/a.ts", "interface I extends [1, 2] {}\n")]);
        assert_eq!(
            ts2499_count(&diagnostics, "/a.ts"),
            1,
            "expected exactly one TS2499, not a parser+checker double-report: {diagnostics:?}"
        );
    }

    #[test]
    fn interface_extends_class_expression_reports_ts2499_once() {
        // The parenthesized operand is a class expression rather than a
        // binary/array literal; the checker's generic heritage walk owns the
        // single TS2499 for every parenthesized shape.
        let diagnostics =
            collect_test_diagnostics(&[("/a.ts", "interface I extends (class {}) {}\n")]);
        assert_eq!(
            ts2499_count(&diagnostics, "/a.ts"),
            1,
            "expected exactly one TS2499: {diagnostics:?}"
        );
    }

    #[test]
    fn interface_extends_call_expression_reports_ts2499_once() {
        // A call-expression heritage operand (`foo()`) is a shape the parser's
        // old open-paren/bracket special-case never covered — only the
        // checker's generic heritage walk rejects it. With the checker as the
        // single owner this must still report exactly one TS2499, proving the
        // fix does not depend on the removed parser special-case.
        let diagnostics = collect_test_diagnostics(&[(
            "/a.ts",
            "declare function foo(): number;\ninterface I extends foo() {}\n",
        )]);
        assert_eq!(
            ts2499_count(&diagnostics, "/a.ts"),
            1,
            "expected exactly one TS2499 for a call-expression heritage operand: {diagnostics:?}"
        );
    }

    // Direction-B suppression is shape-agnostic — the checker keep-gate
    // (`keep_checker_diagnostic_when_program_has_real_syntax_errors`) keys on
    // `code >= 2000`, not the heritage node shape — so
    // `real_syntax_error_suppresses_parenthesized_interface_heritage_ts2499`
    // below already covers the call-expression shape's Direction B too.

    #[test]
    fn interface_extends_renamed_binder_reports_ts2499_once() {
        // Anti-hardcoding: a differently-named interface/binder must not
        // change the dedup outcome.
        let diagnostics =
            collect_test_diagnostics(&[("/a.ts", "interface Zeta extends (99 - 1) {}\n")]);
        assert_eq!(
            ts2499_count(&diagnostics, "/a.ts"),
            1,
            "expected exactly one TS2499: {diagnostics:?}"
        );
    }

    #[test]
    fn two_invalid_interface_heritage_clauses_each_report_ts2499_once() {
        // Distinct nodes at distinct positions must not cross-cancel each
        // other in the position-keyed dedup.
        let diagnostics = collect_test_diagnostics(&[(
            "/a.ts",
            "interface A extends (1 + 2) {}\ninterface B extends [3, 4] {}\n",
        )]);
        assert_eq!(
            ts2499_count(&diagnostics, "/a.ts"),
            2,
            "expected one TS2499 per interface, not merged or duplicated: {diagnostics:?}"
        );
    }

    #[test]
    fn real_syntax_error_suppresses_parenthesized_interface_heritage_ts2499() {
        // Direction B: tsc's grammar-error suppression (hasParseDiagnostics)
        // drops TS2499 program-wide when a real, unrelated syntax error is
        // also present — matching the already-listed grammar codes' behavior.
        let diagnostics = collect_test_diagnostics(&[(
            "/a.ts",
            "interface I extends (1 + 2) {}\nlet x: = 1;\n",
        )]);
        assert_eq!(
            ts2499_count(&diagnostics, "/a.ts"),
            0,
            "expected TS2499 to be suppressed alongside a real syntax error: {diagnostics:?}"
        );
    }

    #[test]
    fn interface_extends_valid_qualified_name_reports_no_ts2499() {
        // Negative control: a genuinely valid heritage clause (a namespaced
        // qualified name) must not trip either emission site.
        let diagnostics = collect_test_diagnostics(&[(
            "/a.ts",
            "declare namespace N { interface Base {} }\ninterface I extends N.Base {}\n",
        )]);
        assert_eq!(
            ts2499_count(&diagnostics, "/a.ts"),
            0,
            "expected no TS2499 for a valid qualified-name heritage clause: {diagnostics:?}"
        );
    }

    #[test]
    fn class_implements_parenthesized_expression_still_reports_ts2500_once() {
        // Negative control for the fix's blast radius: TS2500 (class
        // `implements`) is checker-owned like TS2499, and was never part of
        // the parser special-case, so making the checker the single owner of
        // TS2499 must leave TS2500 reporting exactly once.
        let diagnostics =
            collect_test_diagnostics(&[("/a.ts", "class C implements (1 as any) {}\n")]);
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diag| diag.file == "/a.ts" && diag.code == 2500)
                .count(),
            1,
            "expected exactly one TS2500: {diagnostics:?}"
        );
    }

    // --- topological_file_order tests ---

    #[test]
    fn topo_order_empty() {
        let result = topological_file_order(&[], &FxHashMap::default(), &FxHashMap::default());
        assert!(result.is_empty());
    }

    #[test]
    fn topo_order_single_file() {
        let result = topological_file_order(&[0], &FxHashMap::default(), &FxHashMap::default());
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn topo_order_no_deps() {
        // Three files with no dependencies — output should be sorted by index
        let result = topological_file_order(&[2, 0, 1], &FxHashMap::default(), &FxHashMap::default());
        assert_eq!(result, vec![0, 1, 2]);
    }

    #[test]
    fn topo_order_linear_chain() {
        // File 0 imports file 1, file 1 imports file 2
        // Expected order: 2 (no deps), then 1, then 0
        let mut deps = FxHashMap::default();
        deps.insert((0, "./b".to_string()), 1);
        deps.insert((1, "./c".to_string()), 2);

        let result = topological_file_order(&[0, 1, 2], &deps, &FxHashMap::default());
        assert_eq!(result, vec![2, 1, 0]);
    }

    #[test]
    fn topo_order_diamond() {
        // File 0 imports 1 and 2; both 1 and 2 import 3
        // Expected: 3 first, then 1 and 2 (sorted), then 0
        let mut deps = FxHashMap::default();
        deps.insert((0, "./a".to_string()), 1);
        deps.insert((0, "./b".to_string()), 2);
        deps.insert((1, "./c".to_string()), 3);
        deps.insert((2, "./c".to_string()), 3);

        let result = topological_file_order(&[0, 1, 2, 3], &deps, &FxHashMap::default());
        assert_eq!(result, vec![3, 1, 2, 0]);
    }

    #[test]
    fn topo_order_cycle() {
        // Circular: 0 -> 1 -> 0
        // Both participate in a cycle; should still include both files
        let mut deps = FxHashMap::default();
        deps.insert((0, "./b".to_string()), 1);
        deps.insert((1, "./a".to_string()), 0);

        let result = topological_file_order(&[0, 1], &deps, &FxHashMap::default());
        assert_eq!(result.len(), 2);
        assert!(result.contains(&0));
        assert!(result.contains(&1));
    }

    #[test]
    fn topo_order_partial_cycle() {
        // File 2 has no deps; files 0 and 1 form a cycle
        // Expected: 2 first (no deps), then 0, 1 (cycle participants appended)
        let mut deps = FxHashMap::default();
        deps.insert((0, "./b".to_string()), 1);
        deps.insert((1, "./a".to_string()), 0);

        let result = topological_file_order(&[0, 1, 2], &deps, &FxHashMap::default());
        assert_eq!(result[0], 2, "dependency-free file should come first");
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn topo_order_ignores_external_deps() {
        // File 0 depends on file 5, but 5 is not in file_indices — should be ignored
        let mut deps = FxHashMap::default();
        deps.insert((0, "./ext".to_string()), 5);

        let result = topological_file_order(&[0, 1], &deps, &FxHashMap::default());
        assert_eq!(result.len(), 2);
        // Both have no in-set dependencies, so sorted order
        assert_eq!(result, vec![0, 1]);
    }

    #[test]
    fn topo_order_self_import_ignored() {
        // File 0 imports itself — self-loops should be ignored
        let mut deps = FxHashMap::default();
        deps.insert((0, "./self".to_string()), 0);

        let result = topological_file_order(&[0, 1], &deps, &FxHashMap::default());
        assert_eq!(result, vec![0, 1]);
    }

    /// Order the same four-file import cycle twice, under two different index
    /// assignments, and return the resulting *name* sequences.
    ///
    /// Program file indices are assigned in root-discovery order, so changing
    /// which file the `tsconfig` `include` glob names first hands the same file
    /// a different index. Both runs below describe the identical module graph
    /// (the edges are written in terms of names) and differ only in that
    /// assignment — mirroring zod's `types.ts` / `helpers/partialUtil.ts` cycle
    /// in issue #16036.
    fn cycle_order_under_two_root_discovery_orders(
        stable_key: bool,
    ) -> (Vec<&'static str>, Vec<&'static str>) {
        const CYCLE_EDGES: [(&str, &str); 4] = [
            ("types.ts", "partialUtil.ts"),
            ("partialUtil.ts", "index.ts"),
            ("index.ts", "external.ts"),
            ("external.ts", "types.ts"),
        ];

        let order_of = |names: [&'static str; 4]| -> Vec<&'static str> {
            let position = |name: &str| names.iter().position(|n| *n == name).unwrap();
            let mut deps = FxHashMap::default();
            for (from, to) in CYCLE_EDGES {
                deps.insert((position(from), to.to_string()), position(to));
            }

            let indices: Vec<usize> = (0..names.len()).collect();
            // Mirrors `stable_file_order_key`: rank by file name, which is a
            // property of the file set and not of the root list.
            let key: FxHashMap<usize, u32> = if stable_key {
                let mut by_name = indices.clone();
                by_name.sort_unstable_by_key(|&idx| names[idx]);
                by_name
                    .into_iter()
                    .enumerate()
                    .map(|(rank, idx)| (idx, rank as u32))
                    .collect()
            } else {
                FxHashMap::default()
            };

            topological_file_order(&indices, &deps, &key)
                .into_iter()
                .map(|idx| names[idx])
                .collect()
        };

        (
            order_of(["types.ts", "index.ts", "external.ts", "partialUtil.ts"]),
            order_of(["partialUtil.ts", "index.ts", "external.ts", "types.ts"]),
        )
    }

    #[test]
    fn topo_order_of_a_cycle_is_independent_of_root_discovery_order() {
        // Regression for #16036. A cycle admits no topological order at all, so
        // Kahn's algorithm drains nothing and the entire component is ordered by
        // the tie-break. With a name-derived key the two root configurations
        // must check the identical file set in the identical order.
        let (from_types_root, from_partial_util_root) =
            cycle_order_under_two_root_discovery_orders(true);
        assert_eq!(
            from_types_root, from_partial_util_root,
            "the same file set must check in the same order regardless of which \
             of its files the tsconfig root list names"
        );
    }

    #[test]
    fn topo_order_of_a_cycle_without_a_stable_key_is_root_dependent() {
        // Negative control: the same scenario with no stable key falls back to
        // the raw file index, which is exactly the root-discovery-order
        // dependence #16036 reports. Without this the positive test above could
        // pass on a graph that never had a tie to break.
        let (from_types_root, from_partial_util_root) =
            cycle_order_under_two_root_discovery_orders(false);
        assert_ne!(
            from_types_root, from_partial_util_root,
            "index tie-breaking is expected to be root-dependent; if this ever \
             holds, the fixture stopped exercising the cycle-append path"
        );
    }

    #[test]
    fn ts6504_emitted_for_js_root_when_allow_js_disabled() {
        // When allowJs is not set, an explicit JS root must produce TS6504.
        // tsc includes the file in the program but reports the error and skips
        // semantic checks for that file.
        let options = ResolvedCompilerOptions {
            allow_js: false,
            ..ResolvedCompilerOptions::default()
        };
        let diagnostics = collect_test_diagnostics_with_options(
            &[("/main.js", "const n = 1;\n")],
            &options,
            std::path::Path::new("/"),
        );

        assert!(
            diagnostics.iter().any(|d| d.code == 6504),
            "expected TS6504 for JS root without allowJs, got: {diagnostics:?}"
        );

        let ts6504 = diagnostics.iter().find(|d| d.code == 6504).unwrap();
        assert!(
            ts6504.message_text.contains("main.js"),
            "TS6504 message should include the JS file path: {}",
            ts6504.message_text
        );
        assert!(
            ts6504.related_information.len() >= 2,
            "TS6504 should have related info explaining why the file is in the program"
        );
    }

    #[test]
    fn ts6504_not_emitted_when_allow_js_enabled() {
        // When allowJs is enabled, JS root files are accepted without TS6504.
        let options = ResolvedCompilerOptions {
            allow_js: true,
            ..ResolvedCompilerOptions::default()
        };
        let diagnostics = collect_test_diagnostics_with_options(
            &[("/main.js", "const n = 1;\n")],
            &options,
            std::path::Path::new("/"),
        );

        assert!(
            !diagnostics.iter().any(|d| d.code == 6504),
            "expected no TS6504 when allowJs is enabled, got: {diagnostics:?}"
        );
    }

    /// Regression for #12299: in program mode (the lib is checked as a source
    /// file — the default es2015 lib set includes `lib.dom.d.ts`), a DOM
    /// interface that extends `Node` both directly and through
    /// `ChildNode`/`ParentNode` (the `Element`/`HTMLElement` diamond) was built
    /// without any `Node` members when a heritage base was dropped while it was
    /// itself mid-resolution. That produced false TS2339 on inherited methods
    /// (`appendChild`, `cloneNode`, ...) and false TS2740 for
    /// `Element`-is-not-assignable-to-`Node`.
    ///
    /// This drives the program-file interface-lowering path
    /// (`compute_type_of_symbol` -> `merge_interface_heritage_types`), which the
    /// `LibContext`-based `lib_heritage_cycle_dom_tests` harness cannot reach.
    ///
    /// Fixed by draining cycle-incomplete lib names at the outermost
    /// `resolve_lib_type_by_name` boundary: once the mutual `Element` ↔ `Node` ↔
    /// `HTMLElement` cycle fully unwinds, each interface left `Incomplete`
    /// (because a base was dropped while itself mid-resolution) is re-resolved
    /// against its now-cached bases and its body is rewritten flat. The
    /// flattened (not intersection) shape is why generic inference over DOM
    /// types is unaffected.
    #[test]
    fn dom_element_inherits_node_members_in_program_mode_12299() {
        // Vary the receiver binder name and element type so a fix keyed to a
        // single identifier or resolution order would not satisfy this.
        for (recv, ty) in [
            ("el", "Element"),
            ("widget", "HTMLElement"),
            ("vec", "SVGElement"),
        ] {
            let src =
                format!("declare const {recv}: {ty};\n{recv}.appendChild({recv});\n{recv}.cloneNode();\n");
            let diagnostics = collect_es2015_default_lib_diagnostics(&src);
            assert!(
                !diagnostics.iter().any(|diag| diag.code == 2339),
                "{ty}.appendChild/cloneNode must resolve through Node heritage in program mode: {diagnostics:?}"
            );
        }

        // Element is assignable to Node (it extends Node directly and via
        // ChildNode/ParentNode); the incomplete body previously dropped that.
        let assignable = collect_es2015_default_lib_diagnostics(
            "declare const e: Element;\nconst n: Node = e;\n",
        );
        assert!(
            !assignable
                .iter()
                .any(|diag| diag.code == 2740 || diag.code == 2322),
            "Element must be assignable to Node in program mode: {assignable:?}"
        );

        // Guard against over-correction: a genuinely missing member still errors.
        let bogus = collect_es2015_default_lib_diagnostics(
            "declare const el: Element;\nel.totallyBogusMember();\n",
        );
        assert!(
            bogus.iter().any(|diag| diag.code == 2339),
            "a genuinely missing member must still report TS2339: {bogus:?}"
        );
    }

    /// A generic class declared in one module that `extends` a global lib
    /// interface whose own body inherits from a further lib base
    /// (`class FetchResponse<T> extends Response`, where
    /// `interface Response extends Body`) must surface the transitive base
    /// members (`json`, `text`, `body`, ...) when the class is used from a
    /// DIFFERENT module.
    ///
    /// The class instance type is built in a transient cross-arena delegation
    /// child (`delegate_cross_arena_class_instance_type`) running against the
    /// declaring file's binder, which lacks the merged standard-lib globals.
    /// `merge_lib_interface_heritage` resolved the lib base symbol through that
    /// binder and silently bailed when it was absent, dropping the lib base's
    /// own `extends` and leaving an own-members-only `Response`. Property access
    /// and assignability then mis-reported the inherited members as missing
    /// (false TS2339 / TS2740). The same-file and non-generic forms were
    /// unaffected because they resolve the lib base in the top-level checker
    /// whose binder carries the globals; this is the class analog of the
    /// interface-path fix #13767 / the lib-interface drain #12299.
    ///
    /// Vary the class name, type parameter spelling, and lib base so a fix keyed
    /// to a single identifier would not satisfy this.
    #[test]
    fn cross_file_generic_class_inherits_transitive_lib_base_members() {
        for (class_name, type_param, lib_base, inherited_member) in [
            ("FetchResponse", "T", "Response", "json"),
            ("Wrapped", "TValue", "Response", "text"),
        ] {
            let base_src = format!(
                "export class {class_name}<{type_param}> extends {lib_base} {{ parsed?: {type_param}; }}\n"
            );
            let use_src = format!(
                "import {{ {class_name} }} from './base';\nfunction take(x: {class_name}<number>) {{ x.{inherited_member}(); }}\n"
            );
            let diagnostics = collect_es2015_default_lib_diagnostics_multifile(&[
                ("base.ts", base_src.as_str()),
                ("use.ts", use_src.as_str()),
            ]);
            assert!(
                !diagnostics.iter().any(|diag| diag.code == 2339),
                "{class_name}<{type_param}> extends {lib_base}: inherited `{inherited_member}` must resolve cross-file: {diagnostics:?}"
            );
        }

        // Guard against over-correction: a genuinely missing member still errors
        // on the cross-file generic class.
        let bogus = collect_es2015_default_lib_diagnostics_multifile(&[
            (
                "base.ts",
                "export class FetchResponse<T> extends Response { parsed?: T; }\n",
            ),
            (
                "use.ts",
                "import { FetchResponse } from './base';\nfunction take(x: FetchResponse<number>) { x.totallyBogusMember(); }\n",
            ),
        ]);
        assert!(
            bogus.iter().any(|diag| diag.code == 2339),
            "a genuinely missing member must still report TS2339 cross-file: {bogus:?}"
        );
    }

