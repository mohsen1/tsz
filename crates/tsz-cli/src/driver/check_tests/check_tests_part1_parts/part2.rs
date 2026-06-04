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
