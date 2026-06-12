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

    // --- topological_file_order tests ---

    #[test]
    fn topo_order_empty() {
        let result = topological_file_order(&[], &FxHashMap::default());
        assert!(result.is_empty());
    }

    #[test]
    fn topo_order_single_file() {
        let result = topological_file_order(&[0], &FxHashMap::default());
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn topo_order_no_deps() {
        // Three files with no dependencies — output should be sorted by index
        let result = topological_file_order(&[2, 0, 1], &FxHashMap::default());
        assert_eq!(result, vec![0, 1, 2]);
    }

    #[test]
    fn topo_order_linear_chain() {
        // File 0 imports file 1, file 1 imports file 2
        // Expected order: 2 (no deps), then 1, then 0
        let mut deps = FxHashMap::default();
        deps.insert((0, "./b".to_string()), 1);
        deps.insert((1, "./c".to_string()), 2);

        let result = topological_file_order(&[0, 1, 2], &deps);
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

        let result = topological_file_order(&[0, 1, 2, 3], &deps);
        assert_eq!(result, vec![3, 1, 2, 0]);
    }

    #[test]
    fn topo_order_cycle() {
        // Circular: 0 -> 1 -> 0
        // Both participate in a cycle; should still include both files
        let mut deps = FxHashMap::default();
        deps.insert((0, "./b".to_string()), 1);
        deps.insert((1, "./a".to_string()), 0);

        let result = topological_file_order(&[0, 1], &deps);
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

        let result = topological_file_order(&[0, 1, 2], &deps);
        assert_eq!(result[0], 2, "dependency-free file should come first");
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn topo_order_ignores_external_deps() {
        // File 0 depends on file 5, but 5 is not in file_indices — should be ignored
        let mut deps = FxHashMap::default();
        deps.insert((0, "./ext".to_string()), 5);

        let result = topological_file_order(&[0, 1], &deps);
        assert_eq!(result.len(), 2);
        // Both have no in-set dependencies, so sorted order
        assert_eq!(result, vec![0, 1]);
    }

    #[test]
    fn topo_order_self_import_ignored() {
        // File 0 imports itself — self-loops should be ignored
        let mut deps = FxHashMap::default();
        deps.insert((0, "./self".to_string()), 0);

        let result = topological_file_order(&[0, 1], &deps);
        assert_eq!(result, vec![0, 1]);
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

    /// Build a program containing one barrel file with `wildcard_count`
    /// `export * from` declarations plus the leaf modules it re-exports.
    fn program_with_wildcard_barrel(wildcard_count: usize) -> MergedProgram {
        let mut files = Vec::new();
        let mut barrel = String::new();
        for i in 0..wildcard_count {
            files.push((
                format!("/proj/leafmod{i}.ts"),
                format!("export const widget{i} = {i};"),
            ));
            barrel.push_str(&format!("export * from \"./leafmod{i}\";\n"));
        }
        files.push(("/proj/barrel.ts".to_string(), barrel));
        merged_program_from_owned_files(files)
    }

    fn program_has_large_wildcard_barrel(program: &MergedProgram) -> bool {
        let work_items: Vec<usize> = (0..program.files.len()).collect();
        has_large_wildcard_barrel(WildcardBarrelAnalysisInput {
            files: &program.files,
            wildcard_reexports: &program.wildcard_reexports,
            work_items: &work_items,
            large_export_threshold: LARGE_WILDCARD_BARREL_EXPORTS,
        })
    }

    /// Threshold boundary for wildcard-barrel detection: 31 wildcard
    /// re-exports stay below the large-barrel threshold, 32 and far above (the
    /// 201-re-export barrel shape from the large-ts-repo row) are reported as
    /// large. The scheduling fallback was lifted in #13244; this still guards
    /// the analysis input used by diagnostics and future targeted policies.
    #[test]
    fn wildcard_barrel_threshold_boundary_31_32_201() {
        assert!(
            !program_has_large_wildcard_barrel(&program_with_wildcard_barrel(
                LARGE_WILDCARD_BARREL_EXPORTS - 1
            )),
            "31 wildcard re-exports must stay below the sequential-fallback threshold"
        );
        assert!(
            program_has_large_wildcard_barrel(&program_with_wildcard_barrel(
                LARGE_WILDCARD_BARREL_EXPORTS
            )),
            "32 wildcard re-exports must be reported as a large wildcard barrel"
        );
        assert!(
            program_has_large_wildcard_barrel(&program_with_wildcard_barrel(201)),
            "201 wildcard re-exports (large-ts-repo heavy barrel) must be reported as large"
        );
    }

    /// The detection is per-file, not transitive: a re-export chain whose
    /// files each stay below the threshold must not trip the gate even when
    /// the transitive closure exceeds it.
    #[test]
    fn wildcard_barrel_chain_counts_per_file_not_transitively() {
        let per_file = LARGE_WILDCARD_BARREL_EXPORTS / 2;
        let mut files = Vec::new();
        // Three chained tiers (hub -> mid -> leaves), renamed binders per tier.
        let mut hub = String::new();
        for m in 0..per_file {
            let mut mid = String::new();
            for l in 0..per_file {
                files.push((
                    format!("/proj/tierleaf_{m}_{l}.ts"),
                    format!("export interface Gadget{m}x{l} {{ tag: \"g{m}-{l}\"; }}"),
                ));
                mid.push_str(&format!("export * from \"./tierleaf_{m}_{l}\";\n"));
            }
            files.push((format!("/proj/tiermid{m}.ts"), mid));
            hub.push_str(&format!("export * from \"./tiermid{m}\";\n"));
        }
        files.push(("/proj/tierhub.ts".to_string(), hub));

        let program = merged_program_from_owned_files(files);
        assert!(
            !program_has_large_wildcard_barrel(&program),
            "chained sub-threshold barrels must not trip the per-file gate"
        );
    }

    /// Cyclic wildcard re-exports: detection is a per-file count lookup, so a
    /// cycle below the threshold stays parallel-eligible and a cycle at the
    /// threshold trips it — without hanging on the cycle.
    #[test]
    fn cyclic_wildcard_reexports_respect_threshold() {
        let make_cycle = |extra: usize| {
            let mut files = Vec::new();
            for (name, peer) in [("ring_a", "ring_b"), ("ring_b", "ring_a")] {
                let mut src = format!("export * from \"./{peer}\";\n");
                for i in 0..extra {
                    files.push((
                        format!("/proj/{name}_dep{i}.ts"),
                        format!("export type Spoke{i} = {i};"),
                    ));
                    src.push_str(&format!("export * from \"./{name}_dep{i}\";\n"));
                }
                files.push((format!("/proj/{name}.ts"), src));
            }
            merged_program_from_owned_files(files)
        };

        // Each cycle member has 1 (peer) + extra wildcard re-exports.
        let below = make_cycle(LARGE_WILDCARD_BARREL_EXPORTS - 2);
        assert!(
            !program_has_large_wildcard_barrel(&below),
            "cyclic re-exports below the threshold must stay parallel-eligible"
        );
        let at = make_cycle(LARGE_WILDCARD_BARREL_EXPORTS - 1);
        assert!(
            program_has_large_wildcard_barrel(&at),
            "cyclic re-exports at the threshold must trip the fallback"
        );
    }

    /// Only `export * from` counts: namespace (`export * as ns from`) and
    /// named (`export { a as b } from`) re-exports carry an export clause and
    /// must not count toward the wildcard-barrel threshold.
    #[test]
    fn clause_reexports_do_not_count_toward_wildcard_barrel() {
        let mut files = Vec::new();
        let mut barrel = String::new();
        for i in 0..LARGE_WILDCARD_BARREL_EXPORTS {
            files.push((
                format!("/proj/clausemod{i}.ts"),
                format!("export const item{i} = {i};"),
            ));
            barrel.push_str(&format!("export * as bundle{i} from \"./clausemod{i}\";\n"));
            barrel.push_str(&format!(
                "export {{ item{i} as renamed{i} }} from \"./clausemod{i}\";\n"
            ));
        }
        files.push(("/proj/clausebarrel.ts".to_string(), barrel));

        let program = merged_program_from_owned_files(files);
        assert!(
            !program_has_large_wildcard_barrel(&program),
            "namespace/named re-exports must not count toward the wildcard-barrel gate"
        );
    }

    /// The gate only inspects scheduled work items: a large barrel outside the
    /// work set (e.g. an unchecked dependency) must not serialize the batch.
    #[test]
    fn wildcard_barrel_outside_work_items_does_not_trip_gate() {
        let program = program_with_wildcard_barrel(LARGE_WILDCARD_BARREL_EXPORTS);
        let barrel_idx = program
            .files
            .iter()
            .position(|file| file.file_name.ends_with("/barrel.ts"))
            .expect("barrel file present");
        let work_items: Vec<usize> = (0..program.files.len())
            .filter(|&idx| idx != barrel_idx)
            .collect();
        assert!(
            !has_large_wildcard_barrel(WildcardBarrelAnalysisInput {
                files: &program.files,
                wildcard_reexports: &program.wildcard_reexports,
                work_items: &work_items,
                large_export_threshold: LARGE_WILDCARD_BARREL_EXPORTS,
            }),
            "a barrel outside the scheduled work items must not serialize the batch"
        );
    }
