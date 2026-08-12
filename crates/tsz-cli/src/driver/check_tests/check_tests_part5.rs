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

    // ------------------------------------------------------------------
    // #16279: the reserved interface/type-alias-name family (TS2427/TS2457)
    // is split by tsc's emission site. Hard keywords (`void`/`null`) and
    // numeric names are parser diagnostics (they short-circuit the file's
    // semantic phase); soft predefined-type names (`string`, `number`, ...)
    // are checker `grammarErrorOnNode` diagnostics (suppressed by a sibling
    // parse error, but otherwise coexisting with unrelated grammar errors).
    // Before the fix tsz emitted the soft-name TS2427 from the parser, which
    // both double-emitted and, being outside `is_parser_grammar_code`, counted
    // as a suppressing "real parse error" that silently deleted sibling grammar
    // diagnostics in the same file.

    #[test]
    fn soft_reserved_interface_name_does_not_delete_sibling_grammar_diagnostic() {
        // The headline family-deletion witness. tsc reports BOTH the soft-name
        // TS2427 and the sibling accessor grammar error (TS1054); tsz used to
        // drop the TS1054 because the parser-emitted TS2427 was treated as a
        // suppressing parse error.
        for soft in ["string", "number", "object"] {
            let src = format!(
                "interface {soft} {{}}\nclass C {{ get x(a: number) {{ return a; }} }}\n"
            );
            let diagnostics = collect_test_diagnostics(&[("/a.ts", src.as_str())]);
            assert!(
                diagnostics.iter().any(|d| d.code == 2427),
                "expected TS2427 for `interface {soft}`, got: {diagnostics:?}"
            );
            assert!(
                diagnostics.iter().any(|d| d.code == 1054),
                "sibling TS1054 (get accessor with parameters) must survive \
                 alongside the soft-name TS2427 for `interface {soft}`, got: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn soft_reserved_interface_name_is_single_emission() {
        // No parser/checker double-emission for the soft-name form.
        for soft in ["string", "number", "boolean"] {
            let src = format!("interface {soft} {{}}\n");
            let diagnostics = collect_test_diagnostics(&[("/a.ts", src.as_str())]);
            assert_eq!(
                diagnostics.iter().filter(|d| d.code == 2427).count(),
                1,
                "exactly one TS2427 expected for `interface {soft}`, got: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn soft_reserved_interface_name_suppressed_by_sibling_parse_error() {
        // Direction B: a genuine syntax error elsewhere in the file drops the
        // soft-name (checker-emitted) TS2427, matching tsc's hasParseDiagnostics.
        let diagnostics =
            collect_test_diagnostics(&[("/a.ts", "interface string {}\nlet zzz: = 1;\n")]);
        assert!(
            diagnostics.iter().any(|d| d.code == 1110),
            "the real syntax error (TS1110) must be reported, got: {diagnostics:?}"
        );
        assert!(
            !diagnostics.iter().any(|d| d.code == 2427),
            "soft-name TS2427 must be suppressed by a sibling parse error, got: {diagnostics:?}"
        );
    }

    #[test]
    fn hard_keyword_interface_name_short_circuits_semantic_phase() {
        // `interface void {}` is a parser diagnostic (`void` cannot be an
        // identifier), so tsc suppresses the file's unrelated type error just
        // as it would for any parse error. The TS2427 itself survives.
        let diagnostics = collect_test_diagnostics(&[(
            "/a.ts",
            "interface void {}\nconst x: number = \"s\";\n",
        )]);
        assert!(
            diagnostics.iter().any(|d| d.code == 2427),
            "hard-keyword TS2427 for `interface void` must be reported, got: {diagnostics:?}"
        );
        assert!(
            !diagnostics.iter().any(|d| d.code == 2322),
            "the unrelated type error must be short-circuited by the hard-keyword \
             parse diagnostic, got: {diagnostics:?}"
        );
    }

    #[test]
    fn hard_keyword_interface_name_suppresses_soft_sibling_same_file() {
        // `interface void {}` + `interface string {}` in one file: tsc reports
        // only the `void` TS2427. This falls out of the general
        // hasParseDiagnostics mechanism (the parser `void`-TS2427 makes
        // `program_has_real_syntax_errors` true, suppressing the checker's
        // soft-name TS2427), not a message-text special case.
        let diagnostics =
            collect_test_diagnostics(&[("/a.ts", "interface void {}\ninterface string {}\n")]);
        let ts2427: Vec<_> = diagnostics.iter().filter(|d| d.code == 2427).collect();
        assert_eq!(
            ts2427.len(),
            1,
            "only the hard-keyword TS2427 should survive, got: {diagnostics:?}"
        );
        assert!(
            ts2427[0].message_text.contains("void"),
            "the surviving TS2427 must be the `void` one, got: {:?}",
            ts2427[0]
        );
    }

    #[test]
    fn numeric_and_hard_keyword_interface_names_both_reported() {
        // Numeric names are parser diagnostics too, and tsc keeps both a numeric
        // and a hard-keyword TS2427 in the same file (neither suppresses the
        // other) — the branch the removed bespoke filter would have collapsed.
        let diagnostics =
            collect_test_diagnostics(&[("/a.ts", "interface void {}\ninterface 123 {}\n")]);
        assert_eq!(
            diagnostics.iter().filter(|d| d.code == 2427).count(),
            2,
            "both the `void` and numeric TS2427 must survive, got: {diagnostics:?}"
        );
    }

    /// End-to-end guard for the property-name span fix in
    /// `parse_property_name_impl`: a parser grammar diagnostic and a checker
    /// diagnostic anchored on the *same* accessor name must interleave by code,
    /// exactly as `tsc`'s `compareDiagnostics` orders them (start, then length,
    /// then code). `TS1054` ("A 'get' accessor cannot have parameters.") and
    /// `TS2378` ("A 'get' accessor must return a value.") both anchor on the `x`
    /// name of `get x(v: number): string`; `tsc` emits `TS1054` first because
    /// `1054 < 2378` at an equal span.
    ///
    /// The property-name node used to overshoot its `end` by the width of the
    /// following `(`, so `TS1054` carried a longer span than `TS2378`; since
    /// `compare` breaks ties on length before code, the longer diagnostic sorted
    /// *after* the shorter one, inverting the pair relative to `tsc`. This asserts
    /// the corrected, code-ordered interleaving through the full CLI sort path.
    #[test]
    fn accessor_grammar_and_checker_diagnostics_interleave_by_code_at_one_name() {
        let mut diagnostics =
            collect_es2015_default_lib_diagnostics("class C { get x(v: number): string {} }\n");
        diagnostics.sort_by(|a, b| a.compare(b));

        let ts1054 = diagnostics
            .iter()
            .position(|d| d.code == 1054)
            .expect("TS1054 (get accessor cannot have parameters) must be reported");
        let ts2378 = diagnostics
            .iter()
            .position(|d| d.code == 2378)
            .expect("TS2378 (get accessor must return a value) must be reported");

        assert_eq!(
            diagnostics[ts1054].start, diagnostics[ts2378].start,
            "both diagnostics must anchor on the same accessor name"
        );
        assert!(
            ts1054 < ts2378,
            "TS1054 must sort before TS2378 at an equal span (code order), matching tsc; \
             got TS1054 at {ts1054} and TS2378 at {ts2378}: {diagnostics:?}"
        );
    }

    /// #17253 regression: end-to-end parity through the real CLI pipeline for
    /// the TS1155 grammar-code reclassification.
    ///
    /// `#17251` moved the "'{0}' declarations must be initialized." check into
    /// the parser but left TS1155 out of `is_parser_grammar_code` and, worse,
    /// listed in `is_real_syntax_error`/`is_structural_parse_error`. So an
    /// uninitialized const counted as a suppressing "real parse error" and
    /// deleted every co-occurring sibling (`constDeclarations-errors` lost its
    /// TS2588). tsc reports BOTH: `const a; a = 1;` is a must-initialize TS1155
    /// AND an assign-to-constant TS2588. The sibling must survive.
    #[test]
    fn ts1155_does_not_delete_the_assign_to_const_sibling() {
        let diagnostics = collect_test_diagnostics(&[("/a.ts", "const a;\na = 1;\n")]);
        let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&1155),
            "the uninitialized const must still report TS1155, got: {codes:?}"
        );
        assert!(
            codes.contains(&2588),
            "the co-occurring TS2588 (cannot assign to a constant) must survive \
             alongside TS1155, not be deleted by it, got: {codes:?}"
        );
    }

    /// The other half of #17253: TS1155 is a grammar check on a well-formed
    /// AST, so a genuine unrelated parse error in the same file must suppress
    /// it (tsc's Direction B). `let b: = 1;` is a real syntax error (TS1110,
    /// "Type expected"); tsc drops the file's TS1155 next to it. Before the fix
    /// tsz kept TS1155 because it was absent from `is_parser_grammar_code`.
    #[test]
    fn ts1155_is_suppressed_by_an_unrelated_real_parse_error() {
        let diagnostics = collect_test_diagnostics(&[("/a.ts", "const a;\nlet b: = 1;\n")]);
        let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&1110),
            "the real parse error TS1110 (type expected) must be reported, got: {codes:?}"
        );
        assert!(
            !codes.contains(&1155),
            "TS1155 must be suppressed alongside a real parse error (Direction B), got: {codes:?}"
        );
    }
