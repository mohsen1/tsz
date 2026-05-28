    use super::*;
    use crate::args::CliArgs;
    use std::fs;
    use std::path::PathBuf;
    use tsz_common::common::ModuleKind;

    fn collect_test_diagnostics(files: &[(&str, &str)]) -> Vec<Diagnostic> {
        let bind_results: Vec<_> = files
            .iter()
            .map(|(file_name, source)| {
                parallel::parse_and_bind_single((*file_name).to_string(), (*source).to_string())
            })
            .collect();
        let program = parallel::merge_bind_results(bind_results);
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        collect_diagnostics(
            &CollectDiagnosticsInput {
                program: &program,
                options: &ResolvedCompilerOptions::default(),
                base_dir: std::path::Path::new("/"),
                checker_libs: &CheckerLibSet::default(),
                typescript_dom_replacement_globals: (false, false, false),
                has_deprecation_diagnostics: false,
                collect_compile_stats: false,
            },
            None,
            &type_cache_output,
        )
        .diagnostics
    }
    fn collect_test_diagnostics_with_options(
        files: &[(&str, &str)],
        options: &ResolvedCompilerOptions,
        base_dir: &Path,
    ) -> Vec<Diagnostic> {
        let bind_results: Vec<_> = files
            .iter()
            .map(|(file_name, source)| {
                parallel::parse_and_bind_single((*file_name).to_string(), (*source).to_string())
            })
            .collect();
        let program = parallel::merge_bind_results(bind_results);
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        collect_diagnostics(
            &CollectDiagnosticsInput {
                program: &program,
                options,
                base_dir,
                checker_libs: &CheckerLibSet::default(),
                typescript_dom_replacement_globals: (false, false, false),
                has_deprecation_diagnostics: false,
                collect_compile_stats: false,
            },
            None,
            &type_cache_output,
        )
        .diagnostics
    }

    struct FileSessionReuseOverrideGuard;

    impl Drop for FileSessionReuseOverrideGuard {
        fn drop(&mut self) {
            FILE_SESSION_REUSE_TEST_OVERRIDE.with(|override_value| override_value.set(None));
        }
    }

    fn collect_test_diagnostics_with_file_session_reuse(
        files: &[(&str, &str)],
        enabled: bool,
    ) -> Vec<Diagnostic> {
        FILE_SESSION_REUSE_TEST_OVERRIDE.with(|override_value| override_value.set(Some(enabled)));
        let _guard = FileSessionReuseOverrideGuard;
        let options = ResolvedCompilerOptions {
            no_emit: true,
            ..ResolvedCompilerOptions::default()
        };
        collect_test_diagnostics_with_options(files, &options, std::path::Path::new("/"))
    }

    fn merged_program_from_owned_files(files: Vec<(String, String)>) -> MergedProgram {
        let bind_results: Vec<_> = files
            .into_iter()
            .map(|(file_name, source)| parallel::parse_and_bind_single(file_name, source))
            .collect();
        parallel::merge_bind_results(bind_results)
    }

    #[test]
    fn project_mode_cross_file_class_type_reference_uses_instance_type() {
        let mut options = ResolvedCompilerOptions::default();
        options.no_emit = true;
        options.checker.strict = true;
        options.checker.module = ModuleKind::ES2015;
        options.printer.module = ModuleKind::ES2015;

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (
                    "/p/base.ts",
                    r#"
export abstract class Base {
  abstract self(): Base;
}
"#,
                ),
                (
                    "/p/derived.ts",
                    r#"
import { Base } from "./base";

export class Derived extends Base {
  self(): Derived {
    return this;
  }
}
"#,
                ),
            ],
            &options,
            Path::new("/p"),
        );

        assert!(
            diagnostics.iter().all(|diagnostic| diagnostic.code != 2416),
            "project mode should resolve imported class type annotations to the instance type, got: {diagnostics:?}"
        );
    }

    #[test]
    fn project_mode_cross_file_generic_class_self_reference_uses_instance_type() {
        let mut options = ResolvedCompilerOptions::default();
        options.no_emit = true;
        options.checker.strict = true;
        options.checker.module = ModuleKind::ES2015;
        options.printer.module = ModuleKind::ES2015;

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (
                    "/p/base.ts",
                    r#"
export abstract class Box<T> {
  value!: T;
  abstract self(): Box<T>;
}
"#,
                ),
                (
                    "/p/derived.ts",
                    r#"
import { Box } from "./base";

export class StringBox extends Box<string> {
  self(): StringBox {
    return this;
  }
}
"#,
                ),
            ],
            &options,
            Path::new("/p"),
        );

        assert!(
            diagnostics.iter().all(|diagnostic| diagnostic.code != 2416),
            "project mode should resolve generic imported class self references to the instance type, got: {diagnostics:?}"
        );
    }

    #[test]
    fn project_mode_imported_class_annotation_and_typeof_keep_instance_constructor_split() {
        let mut options = ResolvedCompilerOptions::default();
        options.no_emit = true;
        options.checker.strict = true;
        options.checker.module = ModuleKind::ES2015;
        options.printer.module = ModuleKind::ES2015;

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (
                    "/p/base.ts",
                    r#"
export class Token {
  value = 1;
  static create(): Token {
    return new Token();
  }
}
"#,
                ),
                (
                    "/p/use.ts",
                    r#"
import { Token } from "./base";

let okInstance: Token = Token.create();
let okCtor: typeof Token = Token;
let badCtor: typeof Token = Token.create();
"#,
                ),
            ],
            &options,
            Path::new("/p"),
        );

        assert_eq!(
            diagnostics.len(),
            1,
            "only the typeof constructor mismatch should be reported, got: {diagnostics:?}"
        );
        assert_eq!(
            diagnostics[0].code, 2739,
            "typeof Token should remain constructor-shaped, got: {diagnostics:?}"
        );
    }

    /// Asserts the post-PR-#7521 file-session reuse env policy: OFF unless
    /// the user opts back in via `TSZ_FILE_SESSION_REUSE=1`. Before
    /// PR #7521 the default was ON (set by PRs #6870 / #6893) which
    /// regressed wall time 4-14x at 1k+ files; see
    /// `docs/architecture/LSP_PERF_EXPERIMENTS_2026-05-16.md`.
    ///
    /// Failure modes this test catches:
    ///   * someone accidentally reverts the env default-OFF policy
    ///     (`file_session_reuse_from_env(false, false)` returns true)
    ///   * `TSZ_FILE_SESSION_REUSE=1` opt-in stops working
    ///   * `TSZ_DISABLE_FILE_SESSION_REUSE=1` opt-out stops working
    ///   * the disable knob stops taking precedence over the enable knob
    #[test]
    fn file_session_reuse_env_policy_pr_7521() {
        // Default (no env vars set): reuse OFF.
        assert!(
            !file_session_reuse_from_env(false, false),
            "PR #7521: default reuse policy must be OFF (no env vars set)"
        );

        // Explicit opt-in: TSZ_FILE_SESSION_REUSE=1 turns reuse back on.
        assert!(
            file_session_reuse_from_env(false, true),
            "TSZ_FILE_SESSION_REUSE=1 must opt back in"
        );

        // Explicit opt-out: TSZ_DISABLE_FILE_SESSION_REUSE=1 forces OFF.
        assert!(
            !file_session_reuse_from_env(true, false),
            "TSZ_DISABLE_FILE_SESSION_REUSE=1 must force reuse OFF"
        );

        // Disable beats enable: both set => OFF.
        assert!(
            !file_session_reuse_from_env(true, true),
            "TSZ_DISABLE_FILE_SESSION_REUSE=1 must take precedence over TSZ_FILE_SESSION_REUSE=1"
        );
    }

    #[test]
    fn file_session_reuse_workload_policy_keeps_reuse_opt_in_for_tiny_batches() {
        assert!(
            !file_session_reuse_from_workload(false, false, 10),
            "tiny no-emit batches must not reuse by default until reuse is byte-identical"
        );
        assert!(
            !file_session_reuse_from_workload(
                false,
                false,
                FILE_SESSION_REUSE_SMALL_PROJECT_MAX_FILES
            ),
            "the documented tiny-project boundary is a reuse implementation limit, not a default-on policy"
        );
        assert!(
            !file_session_reuse_from_workload(
                false,
                false,
                FILE_SESSION_REUSE_SMALL_PROJECT_MAX_FILES + 1
            ),
            "larger batch CLI projects must keep the post-#7521 reuse-off default"
        );
        assert!(
            file_session_reuse_from_workload(
                false,
                true,
                FILE_SESSION_REUSE_SMALL_PROJECT_MAX_FILES + 1
            ),
            "TSZ_FILE_SESSION_REUSE=1 must still opt larger projects into reuse"
        );
        assert!(
            !file_session_reuse_from_workload(true, true, 10),
            "TSZ_DISABLE_FILE_SESSION_REUSE=1 must override tiny-project auto reuse"
        );
    }

    #[test]
    fn tiny_no_emit_reuse_path_covers_boxed_prime_checker() {
        assert!(
            !needs_separate_boxed_prime_checker(true, false, true, 10, true),
            "tiny no-emit reuse should prime on the reused checker, not a duplicate checker"
        );
        assert!(
            needs_separate_boxed_prime_checker(true, false, false, 10, true),
            "fresh-checker tiny runs still need the separate prime checker"
        );
        assert!(
            needs_separate_boxed_prime_checker(
                true,
                false,
                true,
                FILE_SESSION_REUSE_SMALL_PROJECT_MAX_FILES + 1,
                true,
            ),
            "large projects do not use the tiny reused-checker coverage rule"
        );
        assert!(
            needs_separate_boxed_prime_checker(true, true, true, 10, true),
            "declaration emit consumes per-file state and cannot use tiny no-emit coverage"
        );
        assert!(
            !needs_separate_boxed_prime_checker(true, false, true, 10, false),
            "projects without libs have nothing to prime"
        );
    }

    #[test]
    fn detects_large_wildcard_barrel() {
        let mut files = Vec::new();
        let mut barrel = String::new();
        for i in 0..LARGE_WILDCARD_BARREL_EXPORTS {
            files.push((format!("/p/a{i}.ts"), format!("export type A{i} = {i};")));
            barrel.push_str(&format!("export * from \"./a{i}\";\n"));
        }
        files.push(("/p/index.ts".to_string(), barrel));

        let program = merged_program_from_owned_files(files);
        let work_items: Vec<usize> = (0..program.files.len()).collect();

        assert!(has_large_wildcard_barrel(WildcardBarrelAnalysisInput {
            files: &program.files,
            wildcard_reexports: &program.wildcard_reexports,
            work_items: &work_items,
            large_export_threshold: LARGE_WILDCARD_BARREL_EXPORTS,
        }));
    }

    fn checker_lib_set_for_test(libs: &[(&str, &str)]) -> CheckerLibSet {
        let files = libs
            .iter()
            .map(|(file_name, source)| {
                std::sync::Arc::new(tsz::binder::lib_loader::LibFile::from_source(
                    (*file_name).to_string(),
                    (*source).to_string(),
                ))
            })
            .collect::<Vec<_>>();
        let contexts = files
            .iter()
            .map(|lib| LibContext {
                arena: std::sync::Arc::clone(&lib.arena),
                binder: std::sync::Arc::clone(&lib.binder),
            })
            .collect();

        CheckerLibSet {
            files,
            contexts: std::sync::Arc::new(contexts),
        }
    }

    #[test]
    fn user_only_global_interfaces_do_not_trigger_lib_recheck() {
        let checker_libs = checker_lib_set_for_test(&[(
            "lib.test.d.ts",
            r#"
interface Window {
    document: object;
}
"#,
        )]);

        let program = merged_program_from_owned_files(vec![(
            "file.ts".to_string(),
            r#"
interface Result<T> {
    value?: T;
}
"#
            .to_string(),
        )]);

        let affected = affected_lib_interface_names(&program, &checker_libs);
        assert!(
            affected.is_empty(),
            "user-only global interfaces should not request default-lib recheck, got: {affected:?}"
        );
    }

    #[test]
    fn user_global_interfaces_matching_lib_names_still_trigger_lib_recheck() {
        let checker_libs = checker_lib_set_for_test(&[(
            "lib.test.d.ts",
            r#"
interface Window {
    document: object;
}
"#,
        )]);

        let program = merged_program_from_owned_files(vec![(
            "file.ts".to_string(),
            r#"
interface Window {
    custom: string;
}
"#
            .to_string(),
        )]);

        let affected = affected_lib_interface_names(&program, &checker_libs);
        assert!(
            affected.contains("Window"),
            "lib-matching global interfaces must still request default-lib recheck, got: {affected:?}"
        );
    }

    #[test]
    fn parallel_order_sensitive_lib_detection_is_scoped_to_dom_like_globals() {
        let es_libs = checker_lib_set_for_test(&[("lib.es2018.d.ts", "interface Promise<T> {}\n")]);
        assert!(
            !has_parallel_order_sensitive_global_lib(&es_libs),
            "plain ES libs should stay eligible for parallel project checking"
        );

        let dom_libs =
            checker_lib_set_for_test(&[("lib.dom.d.ts", "interface Console { log(): void; }\n")]);
        assert!(
            has_parallel_order_sensitive_global_lib(&dom_libs),
            "DOM-style globals should use deterministic project checking"
        );
    }

    fn collect_test_diagnostics_with_checker_libs(
        files: &[(&str, &str)],
        checker_libs: &CheckerLibSet,
    ) -> Vec<Diagnostic> {
        let bind_results: Vec<_> = files
            .iter()
            .map(|(file_name, source)| {
                parallel::parse_and_bind_single((*file_name).to_string(), (*source).to_string())
            })
            .collect();
        let program = parallel::merge_bind_results(bind_results);
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        collect_diagnostics(
            &CollectDiagnosticsInput {
                program: &program,
                options: &ResolvedCompilerOptions::default(),
                base_dir: std::path::Path::new("/"),
                checker_libs,
                typescript_dom_replacement_globals: (false, false, false),
                has_deprecation_diagnostics: false,
                collect_compile_stats: false,
            },
            None,
            &type_cache_output,
        )
        .diagnostics
    }

    fn collect_test_diagnostics_with_lib_files(
        files: &[(&str, &str)],
        lib_files: &[std::sync::Arc<tsz::binder::lib_loader::LibFile>],
    ) -> Vec<Diagnostic> {
        collect_test_diagnostics_with_lib_files_and_options(
            files,
            lib_files,
            &ResolvedCompilerOptions::default(),
        )
    }

    fn collect_test_diagnostics_with_lib_files_and_options(
        files: &[(&str, &str)],
        lib_files: &[std::sync::Arc<tsz::binder::lib_loader::LibFile>],
        options: &ResolvedCompilerOptions,
    ) -> Vec<Diagnostic> {
        let compile_inputs = files
            .iter()
            .map(|(file_name, source)| ((*file_name).to_string(), (*source).to_string()))
            .collect::<Vec<_>>();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            lib_files,
        ));
        let checker_libs = load_checker_libs(lib_files);
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        collect_diagnostics(
            &CollectDiagnosticsInput {
                program: &program,
                options,
                base_dir: std::path::Path::new("/"),
                checker_libs: &checker_libs,
                typescript_dom_replacement_globals: (false, false, false),
                has_deprecation_diagnostics: false,
                collect_compile_stats: false,
            },
            None,
            &type_cache_output,
        )
        .diagnostics
    }

    fn default_cli_args_for_test() -> CliArgs {
        clap::Parser::try_parse_from(["tsz"]).expect("default args should parse")
    }

    fn resolved_options_for_es2015_strict_test() -> ResolvedCompilerOptions {
        let mut args = default_cli_args_for_test();
        args.ignore_config = true;
        args.strict = true;
        args.target = Some(crate::args::Target::Es2015);

        let mut resolved = crate::config::resolve_compiler_options(None)
            .expect("resolve default compiler options");
        crate::driver::apply_cli_overrides(&mut resolved, &args).expect("apply cli overrides");
        if matches!(resolved.printer.module, ModuleKind::None) {
            resolved.printer.module = ModuleKind::ES2015;
            resolved.checker.module = ModuleKind::ES2015;
        }
        resolved
    }

    #[test]
    fn readonly_alias_annotation_survives_consumer_first_program_check() {
        let lib_files = tsz::checker::test_utils::load_lib_files(&["es5.d.ts"]);
        assert!(
            !lib_files.is_empty(),
            "es5.d.ts must be available for this regression"
        );
        let files = [
            (
                "/p/b.ts",
                r#"
import { Factory } from "./a.js";

Factory.cloneWith("x");
"#,
            ),
            (
                "/p/a.ts",
                r#"
import { freeze } from "./object-utils.js";

type Factory = Readonly<{
  create(name: string): string;
  cloneWith(value: string): string;
}>;

export const Factory: Factory = freeze<Factory>({
  create(name) {
    return name;
  },
  cloneWith(value) {
    return value;
  },
});
"#,
            ),
            (
                "/p/object-utils.ts",
                r#"
export function freeze<T>(value: T): Readonly<T> {
  return value;
}
"#,
            ),
        ];

        let diagnostics = collect_test_diagnostics_with_lib_files(&files, &lib_files);
        let ts2339 = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == 2339)
            .collect::<Vec<_>>();

        assert!(
            ts2339.is_empty(),
            "Readonly alias annotations should not collapse to unknown in consumer-first program checks. Got: {ts2339:?}. All: {diagnostics:?}"
        );
    }

    #[test]
    fn large_project_checking_preserves_parallel_dom_globals() {
        let lib_files = tsz::checker::test_utils::load_lib_files(&["es5.d.ts", "dom.d.ts"]);
        assert!(
            lib_files.len() >= 2,
            "es5.d.ts and dom.d.ts must be available for this regression"
        );

        let owned_files = (0..40)
            .map(|idx| {
                (
                    format!("pkg{idx}/file{idx}.ts"),
                    format!("console.log(\"file{idx}\");\nconsole.warn(\"file{idx}\");\n"),
                )
            })
            .collect::<Vec<_>>();
        let files = owned_files
            .iter()
            .map(|(file_name, source)| (file_name.as_str(), source.as_str()))
            .collect::<Vec<_>>();
        let options = ResolvedCompilerOptions {
            no_emit: true,
            ..ResolvedCompilerOptions::default()
        };

        let reused_diagnostics = {
            FILE_SESSION_REUSE_TEST_OVERRIDE.with(|override_value| override_value.set(Some(true)));
            let _guard = FileSessionReuseOverrideGuard;
            collect_test_diagnostics_with_lib_files_and_options(&files, &lib_files, &options)
        };
        let disabled_diagnostics = {
            FILE_SESSION_REUSE_TEST_OVERRIDE.with(|override_value| override_value.set(Some(false)));
            let _guard = FileSessionReuseOverrideGuard;
            collect_test_diagnostics_with_lib_files_and_options(&files, &lib_files, &options)
        };
        let console_member_errors = reused_diagnostics
            .iter()
            .chain(disabled_diagnostics.iter())
            .filter(|diagnostic| diagnostic.code == 2339)
            .collect::<Vec<_>>();

        assert!(
            console_member_errors.is_empty(),
            "large-project DOM globals must not be order-dependent. TS2339: {console_member_errors:?}. Reused: {reused_diagnostics:?}. Disabled: {disabled_diagnostics:?}"
        );
    }

    #[test]
    fn file_session_reuse_preserves_multifile_diagnostics() {
        let files = [
            (
                "a.ts",
                "interface Alpha { kind: \"alpha\"; count: number }\nconst a: Alpha = { kind: \"alpha\", count: \"nope\" };\n",
            ),
            (
                "b.ts",
                "interface Beta { kind: \"beta\"; count: number }\nconst b: Beta = { kind: \"beta\", count: \"nope\" };\n",
            ),
            (
                "c.ts",
                "interface Gamma { kind: \"gamma\"; count: number }\nconst c: Gamma = { kind: \"gamma\", count: \"nope\" };\n",
            ),
        ];

        let default_diagnostics = collect_test_diagnostics_with_file_session_reuse(&files, false);
        let reused_diagnostics = collect_test_diagnostics_with_file_session_reuse(&files, true);

        assert_eq!(
            reused_diagnostics, default_diagnostics,
            "file-session reuse must preserve byte-identical diagnostics"
        );
        assert!(
            !default_diagnostics.is_empty(),
            "fixture should exercise real checker diagnostics"
        );
    }

    #[test]
    fn file_session_reuse_preserves_parallel_multifile_diagnostics() {
        let owned_files = (0..40)
            .map(|idx| {
                (
                    format!("pkg{idx}/file{idx}.ts"),
                    format!("export {{}};\nconst value{idx}: number = \"nope\";\n"),
                )
            })
            .collect::<Vec<_>>();
        let files = owned_files
            .iter()
            .map(|(file_name, source)| (file_name.as_str(), source.as_str()))
            .collect::<Vec<_>>();

        let default_diagnostics = collect_test_diagnostics_with_file_session_reuse(&files, false);
        let reused_diagnostics = collect_test_diagnostics_with_file_session_reuse(&files, true);

        assert_eq!(
            reused_diagnostics, default_diagnostics,
            "parallel file-session reuse must preserve byte-identical diagnostics"
        );
        assert_eq!(
            default_diagnostics.len(),
            owned_files.len(),
            "fixture should produce one checker diagnostic per file"
        );
    }

    #[test]
    fn no_check_collect_diagnostics_keeps_parse_errors_and_skips_type_errors() {
        let options = ResolvedCompilerOptions {
            no_check: true,
            ..ResolvedCompilerOptions::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[("file.ts", "const value: string = 1;\nconst broken = ;\n")],
            &options,
            std::path::Path::new("/"),
        );
        let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

        assert!(
            codes.contains(&1109),
            "expected --noCheck diagnostics to keep TS1109 parse error, got: {diagnostics:?}"
        );
        assert!(
            !codes.contains(&2322),
            "expected --noCheck diagnostics to skip TS2322 type error, got: {diagnostics:?}"
        );
    }

    #[test]
    fn no_check_path_emits_isolated_declarations_ts9007() {
        // Issue #3709: `--noCheck --isolatedDeclarations` previously dropped
        // TS9007/TS9011/etc. tsc still reports these because they gate
        // declaration emission, not type checking.
        let mut options = ResolvedCompilerOptions {
            no_check: true,
            ..ResolvedCompilerOptions::default()
        };
        options.checker.isolated_declarations = true;

        let diagnostics = collect_test_diagnostics_with_options(
            &[("file.ts", "export function f(x) { return x; }\n")],
            &options,
            std::path::Path::new("/"),
        );
        let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

        assert!(
            codes.contains(&9007),
            "expected --noCheck --isolatedDeclarations to surface TS9007, got: {diagnostics:?}"
        );
    }

    #[test]
    fn no_check_without_isolated_declarations_does_not_run_isolated_decl_pass() {
        // Without --isolatedDeclarations, the isolated-decl pass must not
        // fire and produce TS9007.
        let options = ResolvedCompilerOptions {
            no_check: true,
            ..ResolvedCompilerOptions::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[("file.ts", "export function f(x) { return x; }\n")],
            &options,
            std::path::Path::new("/"),
        );
        let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

        assert!(
            !codes.contains(&9007),
            "TS9007 must not fire under --noCheck without --isolatedDeclarations, got: {diagnostics:?}"
        );
    }

    #[test]
    fn no_check_with_declaration_emit_still_suppresses_type_errors() {
        // Issue #3733: under `--noCheck --declaration`, the regular checker
        // pipeline must run so declaration emit can pick up inferred types
        // (return types, contextual property types). But type-error
        // diagnostics (TS2322 etc.) must still be suppressed — `--noCheck`
        // means "don't surface type checking errors".
        let options = ResolvedCompilerOptions {
            no_check: true,
            emit_declarations: true,
            ..ResolvedCompilerOptions::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[("file.ts", "export const x: string = 1;\n")],
            &options,
            std::path::Path::new("/"),
        );
        let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

        assert!(
            !codes.contains(&2322),
            "TS2322 must not fire under --noCheck --declaration, got: {diagnostics:?}"
        );
    }

    #[test]
    fn skip_lib_check_pure_declaration_no_emit_skips_semantic_diagnostics() {
        let options = ResolvedCompilerOptions {
            no_emit: true,
            skip_lib_check: true,
            ..ResolvedCompilerOptions::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[(
                "index.d.ts",
                r#"
export type UsesMissing = Missing;
export interface Broken {
    value: ;
}
"#,
            )],
            &options,
            std::path::Path::new("/"),
        );

        assert!(
            diagnostics.iter().any(|diag| diag.code < 2000),
            "parse diagnostics must still surface under skipLibCheck: {diagnostics:?}"
        );
        assert!(
            !diagnostics.iter().any(|diag| diag.code == 2304),
            "skipLibCheck must suppress declaration-file semantic diagnostics: {diagnostics:?}"
        );
    }

    #[test]
    fn skip_lib_check_mixed_project_still_checks_source_files() {
        let options = ResolvedCompilerOptions {
            no_emit: true,
            skip_lib_check: true,
            ..ResolvedCompilerOptions::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                ("types.d.ts", "export type UsesMissing = Missing;\n"),
                ("main.ts", "const value: string = 1;\n"),
            ],
            &options,
            std::path::Path::new("/"),
        );

        assert!(
            diagnostics.iter().any(|diag| diag.code == 2322),
            "non-declaration source files must still be checked under skipLibCheck: {diagnostics:?}"
        );
        assert!(
            !diagnostics.iter().any(|diag| diag.code == 2304),
            "declaration-file semantic diagnostics must remain suppressed: {diagnostics:?}"
        );
    }

    #[test]
    fn collect_diagnostics_preserves_builtin_lib_ts2552_spelling_baseline() {
        let checker_libs = checker_lib_set_for_test(&[
            (
                "lib.esnext.intl.d.ts",
                r#"
declare namespace Intl {
    interface DateTimeFormat {
        formatToParts(): DateTimeFormatPart[];
    }
}
"#,
            ),
            (
                "lib.esnext.temporal.d.ts",
                r#"
declare namespace Temporal {
    interface Instant {}
}
"#,
            ),
        ]);

        let diagnostics = collect_test_diagnostics_with_checker_libs(
            &[("test.ts", "const value = new Intl.DateTimeFormat();\n")],
            &checker_libs,
        );
        let ts2552 = diagnostics
            .iter()
            .filter(|diag| diag.code == 2552)
            .collect::<Vec<_>>();

        assert_eq!(
            ts2552.len(),
            1,
            "expected one baseline lib TS2552 diagnostic, got: {diagnostics:?}"
        );
        assert!(
            ts2552[0]
                .message_text
                .contains("Cannot find name 'DateTimeFormatPart'. Did you mean 'DateTimeFormat'?"),
            "expected DateTimeFormatPart spelling suggestion, got: {ts2552:?}"
        );
        assert_eq!(ts2552[0].file, "lib.esnext.intl.d.ts");
    }

    #[test]
    fn collect_diagnostics_skips_builtin_lib_ts2552_without_temporal_trigger_lib() {
        let checker_libs = checker_lib_set_for_test(&[(
            "lib.esnext.intl.d.ts",
            r#"
declare namespace Intl {
    interface DateTimeFormat {
        formatToParts(): DateTimeFormatPart[];
    }
}
"#,
        )]);

        let diagnostics = collect_test_diagnostics_with_checker_libs(
            &[("test.ts", "const value = new Intl.DateTimeFormat();\n")],
            &checker_libs,
        );

        assert!(
            diagnostics.iter().all(|diag| diag.code != 2552),
            "expected DateTimeFormatPart baseline to require Temporal/Date libs, got: {diagnostics:?}"
        );
    }

    #[test]
    fn collect_diagnostics_ignores_unrelated_builtin_lib_ts2552_spelling_baseline() {
        let checker_libs = checker_lib_set_for_test(&[(
            "lib.esnext.intl.d.ts",
            r#"
declare namespace Intl {
    interface DateTimeFormatPart {}
    interface DateTimeFormat {
        formatToParts(): DateTimeFormatParts[];
    }
}
"#,
        )]);

        let diagnostics = collect_test_diagnostics_with_checker_libs(
            &[("test.ts", "const value = new Intl.DateTimeFormat();\n")],
            &checker_libs,
        );

        assert!(
            diagnostics.iter().all(|diag| diag.code != 2552),
            "expected unrelated baseline lib TS2552 diagnostics to stay filtered, got: {diagnostics:?}"
        );
    }

    fn collect_es2015_default_lib_diagnostics(source: &str) -> Vec<Diagnostic> {
        collect_es2015_default_lib_diagnostics_with_options(source, |_: &mut _| {})
    }

    fn collect_es2015_default_lib_diagnostics_with_options(
        source: &str,
        configure: impl FnOnce(&mut ResolvedCompilerOptions),
    ) -> Vec<Diagnostic> {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(&file_path, source).expect("write source");

        let mut resolved = resolved_options_for_es2015_strict_test();
        configure(&mut resolved);
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

        collect_diagnostics(
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
        .diagnostics
    }

    #[test]
    fn cloned_checker_libs_preserve_strict_builtin_iterator_return() {
        let diagnostics = collect_es2015_default_lib_diagnostics(
            r#"
declare const map: Map<string, number>;
const value: number = map.values().next().value;
interface Next<A> {
    readonly done?: boolean;
    readonly value: A;
}
const result: Next<number> = map.values().next();
"#,
        );
        let ts2322_count = diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
            .count();
        assert_eq!(
            ts2322_count, 2,
            "expected cloned checker libs to preserve strict built-in iterator return diagnostics, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn es2015_local_interface_t_shadows_lib_heritage_type_parameters() {
        let diagnostics = collect_es2015_default_lib_diagnostics(
            r#"
interface T { f(x: number): void }
declare var t: T;
t.f("s");
"#,
        );

        assert!(
            diagnostics.iter().any(|diag| diag.code == 2345),
            "expected TS2345 for T.f argument type, got: {diagnostics:?}"
        );
        assert!(
            diagnostics.iter().all(|diag| diag.code != 2339),
            "did not expect TS2339 from a stale local T shape, got: {diagnostics:?}"
        );
    }

    #[test]
    fn es2015_destructuring_reduce_concat_reports_overload_and_iterability() {
        let diagnostics = collect_es2015_default_lib_diagnostics(
            r#"
declare var tuple: [boolean, number, ...string[]];

const [a, b, c, ...rest] = tuple;

declare var receiver: typeof tuple;

[...receiver] = tuple;

const [oops1] = [1, 2, 3].reduce((accu, el) => accu.concat(el), []);
"#,
        );
        let codes: Vec<u32> = diagnostics.iter().map(|diag| diag.code).collect();

        assert!(
            codes.contains(&2488),
            "expected TS2488 for destructuring the failed reduce result, got: {diagnostics:?}"
        );
        assert!(
            codes.contains(&2769),
            "expected TS2769 for the nested reduce/concat overload failure, got: {diagnostics:?}"
        );
    }

    fn mapped_type_indexed_access_constraint_repro() -> &'static str {
        r#"type Identity<T> = { [K in keyof T]: T[K] };

type M0 = { a: 1, b: 2 };

type M1 = { [K in keyof Partial<M0>]: M0[K] };

type M2 = { [K in keyof Required<M1>]: M1[K] };

type M3 = { [K in keyof Identity<Partial<M0>>]: M0[K] };

function foo<K extends keyof M0>(m1: M1[K], m2: M2[K], m3: M3[K]) {
    m1.toString();
    m1?.toString();
    m2.toString();
    m2?.toString();
    m3.toString();
    m3?.toString();
}

type Obj = {
    a: 1,
    b: 2
};

const mapped: { [K in keyof Partial<Obj>]: Obj[K] } = {};

const resolveMapped = <K extends keyof typeof mapped>(key: K) => mapped[key].toString();

const arr = ["foo", "12", 42] as const;

type Mappings = { foo: boolean, "12": number, 42: string };

type MapperArgs<K extends (typeof arr)[number]> = {
    v: K,
    i: number
};

type SetOptional<T, K extends keyof T> = Omit<T, K> & Partial<Pick<T, K>>;

type PartMappings = SetOptional<Mappings, "foo">;

const mapper: { [K in keyof PartMappings]: (o: MapperArgs<K>) => PartMappings[K] } = {
    foo: ({ v, i }) => v.length + i > 4,
    "12": ({ v, i }) => Number(v) + i,
    42: ({ v, i }) => `${v}${i}`,
};

const resolveMapper1 = <K extends keyof typeof mapper>(
    key: K, o: MapperArgs<K>) => mapper[key](o);

const resolveMapper2 = <K extends keyof typeof mapper>(
    key: K, o: MapperArgs<K>) => mapper[key]?.(o);
"#
    }

    #[test]
    fn jsx_attribute_comma_expression_survives_into_bind_results() {
        let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        [s: string]: any;
    }
}

const class1 = "foo";
const class2 = "bar";
const elem = <div className={class1, class2}/>;
"#;

        let result = parallel::parse_and_bind_single("file.tsx".to_string(), source.to_string());
        let codes: Vec<u32> = result.parse_diagnostics.iter().map(|d| d.code).collect();

        assert!(
            codes.contains(&18007),
            "expected TS18007 in bind-result parse diagnostics, got: {codes:?}"
        );
    }

    #[test]
    fn jsx_attribute_comma_expression_reports_ts18007_in_cli_diagnostics() {
        let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        [s: string]: any;
    }
}

const class1 = "foo";
const class2 = "bar";
const elem = <div className={class1, class2}/>;
"#;

        let diagnostics = collect_test_diagnostics(&[("file.tsx", source)]);
        let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

        assert!(
            codes.contains(&18007),
            "expected CLI diagnostics to include TS18007, got: {diagnostics:?}"
        );
        assert!(
            codes.contains(&2695),
            "expected CLI diagnostics to include TS2695, got: {diagnostics:?}"
        );
    }

    #[test]
    fn jsx_invalid_namespace_start_keeps_colon_ts1109_in_bind_results() {
        let source = "declare var React: any;\nvar x = <:a attr={\"value\"} />;\n";
        let result = parallel::parse_and_bind_single("file.tsx".to_string(), source.to_string());
        let less_than_pos = source.find('<').expect("opening angle") as u32;
        let colon_pos = source[less_than_pos as usize + 1..]
            .find(':')
            .map(|offset| less_than_pos + 1 + offset as u32)
            .expect("colon");
        let expr_expected_positions: Vec<u32> = result
            .parse_diagnostics
            .iter()
            .filter(|diag| diag.code == 1109)
            .map(|diag| diag.start)
            .collect();

        assert!(
            expr_expected_positions.contains(&less_than_pos),
            "expected TS1109 at '<', got: {expr_expected_positions:?}"
        );
        assert!(
            expr_expected_positions.contains(&colon_pos),
            "expected TS1109 at ':', got: {expr_expected_positions:?}"
        );
    }

    #[test]
    fn jsx_invalid_namespace_start_keeps_colon_ts1109_in_cli_diagnostics() {
        let source = "declare var React: any;\nvar x = <:a attr={\"value\"} />;\n";
        let diagnostics = collect_test_diagnostics(&[("file.tsx", source)]);
        let less_than_pos = source.find('<').expect("opening angle") as u32;
        let colon_pos = source[less_than_pos as usize + 1..]
            .find(':')
            .map(|offset| less_than_pos + 1 + offset as u32)
            .expect("colon");
        let expr_expected_positions: Vec<u32> = diagnostics
            .iter()
            .filter(|diag| diag.code == 1109)
            .map(|diag| diag.start)
            .collect();

        assert!(
            expr_expected_positions.contains(&less_than_pos),
            "expected CLI TS1109 at '<', got: {diagnostics:?}"
        );
        assert!(
            expr_expected_positions.contains(&colon_pos),
            "expected CLI TS1109 at ':', got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_collect_diagnostics_preserves_mapped_type_nullish_indexed_reads() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(&file_path, mapped_type_indexed_access_constraint_repro())
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
        let ts18048_count = diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::IS_POSSIBLY_UNDEFINED)
            .count();
        let ts2532_count = diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::OBJECT_IS_POSSIBLY_UNDEFINED)
            .count();
        let ts2722_count = diagnostics
            .iter()
            .filter(|diag| {
                diag.code == diagnostic_codes::CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_UNDEFINED
            })
            .count();
        let ts2349_count = diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::THIS_EXPRESSION_IS_NOT_CALLABLE)
            .count();

        assert_eq!(
            ts18048_count, 3,
            "Expected collect_diagnostics to preserve three TS18048 diagnostics, got: {diagnostics:?}"
        );
        assert_eq!(
            ts2532_count, 1,
            "Expected one TS2532 for mapped[key].toString(), got: {diagnostics:?}"
        );
        assert_eq!(
            ts2722_count, 1,
            "Expected one TS2722 for mapper[key](o), got: {diagnostics:?}"
        );
        assert_eq!(
            ts2349_count, 0,
            "Did not expect TS2349 for mapper[key](o), got: {diagnostics:?}"
        );
    }
