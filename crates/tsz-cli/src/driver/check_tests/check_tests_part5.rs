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

    /// #16316: cross-location related information must survive the real
    /// driver, not just the checker's own diagnostic buffer.
    ///
    /// `tsc --strict` on this source reports TS1346 carrying a TS1349
    /// `'use strict' directive used here.` pointer at the directive, and
    /// TS1347 carrying a TS1348 `Non-simple parameter declared here.` pointer
    /// at the parameter. Both entries point somewhere other than their
    /// primary's own span, so a pipeline stage that rebuilt the diagnostic
    /// without the field would show up here as an empty list.
    #[test]
    fn cross_location_related_information_survives_collect_diagnostics() {
        let diagnostics =
            collect_test_diagnostics(&[("/main.ts", "function f(a = 1) { \"use strict\"; }\n")]);

        let ts1346 = diagnostics
            .iter()
            .find(|d| d.code == 1346)
            .unwrap_or_else(|| panic!("expected TS1346, got: {diagnostics:?}"));
        assert_eq!(
            ts1346
                .related_information
                .iter()
                .map(|r| (r.code, r.start, r.length))
                .collect::<Vec<_>>(),
            vec![(1349, 20, 13)],
            "TS1346 must keep its directive pointer: {ts1346:?}"
        );

        let ts1347 = diagnostics
            .iter()
            .find(|d| d.code == 1347)
            .unwrap_or_else(|| panic!("expected TS1347, got: {diagnostics:?}"));
        assert_eq!(
            ts1347
                .related_information
                .iter()
                .map(|r| (r.code, r.start, r.length))
                .collect::<Vec<_>>(),
            vec![(1348, 11, 6)],
            "TS1347 must keep its parameter pointer: {ts1347:?}"
        );

        // Both are `tsc` `relatedInformation` (cross-location pointers), not
        // `messageText` chain links, so they must survive the driver tagged as
        // such — that tag is what keeps them out of plain-mode output.
        assert!(
            ts1346.related_information.iter().all(|r| r.is_location_pointer()),
            "TS1346's pointer must survive tagged as a pointer: {ts1346:?}"
        );
        assert!(
            ts1347.related_information.iter().all(|r| r.is_location_pointer()),
            "TS1347's pointer must survive tagged as a pointer: {ts1347:?}"
        );
    }

    #[test]
    fn lib_interface_inheriting_an_augmented_index_signature_is_scheduled_for_recheck() {
        // #16474: when a user augments a lib interface with an index signature,
        // every lib interface that inherits that index through its base chain
        // must be scheduled for the post-merge TS2430 heritage recheck — even
        // though it declares no index signature of its own (it declares only
        // ordinary members like `apply`). Requiring the *derived* interface to
        // declare an index signature asked the wrong question: an index
        // signature reaching an interface through its base chain constrains
        // every member that interface declares, so the recheck must run whenever
        // it declares at least one own member.
        //
        // The binder names are deliberately non-`Function`/`CallableFunction`
        // so the schedule is driven by the structural extends-an-index-augmented
        // base relation, not by any built-in identifier string.
        let checker_libs = checker_lib_set_for_test(&[(
            "lib.test.d.ts",
            r#"
interface LibBase {
    apply(this: LibBase, thisArg: any): any;
}
interface LibCallable extends LibBase {
    apply<T, R>(this: (this: T) => R, thisArg: T): R;
}
interface LibNewable extends LibBase {
    apply<T>(this: new () => T, thisArg: T): void;
}
interface LibEmpty extends LibBase {
}
"#,
        )]);

        let program = merged_program_from_owned_files(vec![(
            "file.ts".to_string(),
            r#"
interface Bar { b: number }
interface LibBase {
    [n: number]: Bar;
}
"#
            .to_string(),
        )]);

        let affected = affected_lib_interface_names(&program, &checker_libs);
        assert!(
            affected.contains("LibCallable") && affected.contains("LibNewable"),
            "lib interfaces that declare members and inherit a user-augmented \
             index signature must be scheduled for recheck, got: {affected:?}"
        );
        assert!(
            !affected.contains("LibEmpty"),
            "a memberless derived interface cannot conflict and must stay out, got: {affected:?}"
        );

        let extension = affected_lib_extension_interface_names(&program, &checker_libs, &affected);
        assert!(
            extension.contains("LibCallable") && extension.contains("LibNewable"),
            "and must run heritage (TS2430) extension compatibility, got: {extension:?}"
        );
        assert!(
            !extension.contains("LibEmpty"),
            "a memberless derived interface must not run heritage compatibility, got: {extension:?}"
        );
    }
