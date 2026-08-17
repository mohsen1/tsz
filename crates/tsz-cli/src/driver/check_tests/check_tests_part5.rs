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

    // #16279's own round-8/9 comments flagged TS18016 ("Private identifiers are
    // not allowed outside class bodies") as a deferred double-emission risk:
    // tsc reports it from one checker function
    // (`checkGrammarPrivateIdentifierExpression`), but tsz has five PARSER
    // sites (an interface/type-literal member name in `state_declarations.rs`,
    // and four object-literal member-name shapes in
    // `state_expressions_literals/object_members.rs`) plus four CHECKER sites
    // (a standalone-expression position and the `in`-operator LHS, both in
    // `types/type_checking/core.rs`; a private member-access receiver in
    // `state/type_analysis/computed_helpers_private.rs`; and a JS
    // `X.prototype.#y = …` assignment target in
    // `assignability/assignment_checker/assignment_ops.rs`). 18016 was later
    // added to `is_parser_grammar_code` (audit round 8), which closes the
    // Direction-B suppression gap for the parser copies; these tests close
    // the still-open other half — whether any of those nine sites can ever
    // fire twice for the same source position. They cannot: each owns a
    // syntactically disjoint node shape (a plain declaration name is never
    // also dispatched as a standalone expression), so this suite pins that
    // invariant with a witness at every position rather than leaving it as
    // an unverified risk note.
    //
    // Oracle-verified against `typescript@7.0.2`: each position below reports
    // TS18016 exactly once (Direction A), and an unrelated real syntax error
    // elsewhere in the file suppresses every one of them, leaving only the
    // real error (Direction B) — the checker copies are already suppressed by
    // the generic `code < 2000` rule in
    // `keep_checker_diagnostic_when_program_has_real_syntax_errors` (18016
    // fails it), independent of `is_parser_grammar_code`, which only screens
    // parser diagnostics.
    fn ts18016_all_positions_source() -> &'static str {
        "const o = { #x: 1 };\n\
         interface I { #y: string }\n\
         #z;\n\
         const inCheck = #w in {};\n\
         declare const obj: any;\n\
         obj.#v;\n"
    }

    fn ts18016_count(diagnostics: &[Diagnostic], file: &str) -> usize {
        diagnostics
            .iter()
            .filter(|diag| diag.file == file && diag.code == 18016)
            .count()
    }

    #[test]
    fn private_identifier_outside_class_reports_ts18016_once_per_position() {
        // Five distinct positions (object-literal key, interface/type-literal
        // key, standalone expression, `in`-LHS, member-access receiver) in one
        // file: five separate TS18016s, never zero and never doubled at any
        // one of them.
        let diagnostics =
            collect_test_diagnostics(&[("/a.ts", ts18016_all_positions_source())]);
        assert_eq!(
            ts18016_count(&diagnostics, "/a.ts"),
            5,
            "expected exactly one TS18016 per position (object-literal key, \
             interface key, standalone expression, `in`-LHS, member access), \
             not a parser+checker double-report at any of them: {diagnostics:?}"
        );
    }

    #[test]
    fn private_identifier_outside_class_all_positions_suppressed_by_real_parse_error() {
        let mut source = ts18016_all_positions_source().to_string();
        source.push_str("let zzz: = 1;\n");
        let diagnostics = collect_test_diagnostics(&[("/a.ts", source.as_str())]);
        let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(
            ts18016_count(&diagnostics, "/a.ts"),
            0,
            "every TS18016 (parser- and checker-emitted alike) must be \
             suppressed alongside a real syntax error (Direction B), got: {diagnostics:?}"
        );
        assert!(
            codes.contains(&1110),
            "the real parse error TS1110 (type expected) must survive, got: {diagnostics:?}"
        );
    }

    #[test]
    fn private_identifier_object_literal_accessor_and_method_shapes_each_report_once() {
        // The remaining two of the four object-literal member-name parser
        // sites not covered by the plain-property witness above: `get`/`set`
        // accessors and a method name.
        let diagnostics = collect_test_diagnostics(&[(
            "/a.ts",
            "const o = { get #x() { return 1; }, set #y(v: number) {}, #z() {} };\n",
        )]);
        assert_eq!(
            ts18016_count(&diagnostics, "/a.ts"),
            3,
            "expected exactly one TS18016 per member (get accessor, set \
             accessor, method), not merged or duplicated: {diagnostics:?}"
        );
    }

    #[test]
    fn private_identifier_renamed_receiver_does_not_change_ts18016_count() {
        // Anti-hardcoding: a differently-named receiver/binder must not
        // change the single-emission outcome.
        let diagnostics = collect_test_diagnostics(&[(
            "/a.ts",
            "declare const receiver: any;\nreceiver.#field;\n",
        )]);
        assert_eq!(
            ts18016_count(&diagnostics, "/a.ts"),
            1,
            "expected exactly one TS18016: {diagnostics:?}"
        );
    }

    // ------------------------------------------------------------------
    // #17570: `export default class` + a static/instance function-expression
    // property whose own generic signature is constrained by the enclosing
    // class collapses to a spurious TS2344.
    //
    // Structural rule: a type-parameter constraint that names the enclosing
    // class, referenced from inside that class's own property-initializer
    // expression, must resolve against the class's (not-yet-published)
    // instance type the same way tsc's checker does — deferring to the
    // class's normal member-check pass rather than substituting whatever
    // partial/constructor-side type happens to be cached mid-build. tsz's
    // constructor-shape builder (`class_type::constructor`) fully checks a
    // static property initializer eagerly, before the class's own instance
    // type is published, so a self-referential constraint reads a stale
    // answer and wrongly emits TS2344; the diagnostic then survives
    // `push_diagnostic`'s first-wins dedup even though the class's later,
    // authoritative member check would not have raised it.
    // ------------------------------------------------------------------

    #[test]
    fn export_default_class_static_arrow_property_self_referential_constraint_is_clean() {
        let options = resolved_options_for_es2015_strict_test();
        let diagnostics = collect_test_diagnostics_with_options(
            &[(
                "/a.ts",
                r#"
type MarkOf<R extends { readonly mark: unknown }> = R["mark"];
export default class Schema<T> {
  readonly mark!: T;
  static make = <R extends Schema<any>>(build: (x: number) => MarkOf<R>): R =>
    null as any;
}
"#,
            )],
            &options,
            std::path::Path::new("/"),
        );
        assert!(
            diagnostics.is_empty(),
            "expected a fully clean (tsc-parity) check; got: {diagnostics:#?}"
        );
    }

    #[test]
    fn export_default_class_instance_arrow_property_self_referential_constraint_is_clean() {
        let options = resolved_options_for_es2015_strict_test();
        let diagnostics = collect_test_diagnostics_with_options(
            &[(
                "/a.ts",
                r#"
type MarkOf<R extends { readonly mark: unknown }> = R["mark"];
export default class Schema<T> {
  readonly mark!: T;
  go = <R extends Schema<any>>(build: (x: number) => MarkOf<R>): R =>
    null as any;
}
"#,
            )],
            &options,
            std::path::Path::new("/"),
        );
        assert!(
            diagnostics.is_empty(),
            "expected a fully clean (tsc-parity) check; got: {diagnostics:#?}"
        );
    }

    #[test]
    fn export_default_class_static_property_renamed_binders_self_referential_constraint_is_clean()
    {
        // Anti-hardcoding: a differently-named class, type parameter, and
        // constrained member must not change the outcome.
        let options = resolved_options_for_es2015_strict_test();
        let diagnostics = collect_test_diagnostics_with_options(
            &[(
                "/a.ts",
                r#"
type Foo<Q extends { readonly bar: unknown }> = Q["bar"];
export default class Widget<U> {
  readonly bar!: U;
  static create = <P extends Widget<any>>(build: (x: number) => Foo<P>): P =>
    null as any;
}
"#,
            )],
            &options,
            std::path::Path::new("/"),
        );
        assert!(
            diagnostics.is_empty(),
            "expected a fully clean (tsc-parity) check; got: {diagnostics:#?}"
        );
    }

    #[test]
    fn export_default_class_static_property_genuine_constraint_violation_still_reports_ts2344() {
        // Negative control: when the enclosing class genuinely lacks the
        // member the alias's constraint requires, TS2344 must still fire —
        // the fix must not turn this into a blanket suppression.
        let options = resolved_options_for_es2015_strict_test();
        let diagnostics = collect_test_diagnostics_with_options(
            &[(
                "/a.ts",
                r#"
type MarkOf<R extends { readonly mark: unknown }> = R["mark"];
export default class Schema<T> {
  readonly notmark!: T;
  static make = <R extends Schema<any>>(build: (x: number) => MarkOf<R>): R =>
    null as any;
}
"#,
            )],
            &options,
            std::path::Path::new("/"),
        );
        let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(
            codes,
            vec![2344],
            "expected exactly one genuine TS2344 (Schema lacks `mark`): {diagnostics:?}"
        );
    }

    // ------------------------------------------------------------------
    // #17585: an instance method whose declared return type re-instantiates
    // the enclosing generic class through a self-referential type-parameter
    // constraint (zod's `ZodObject.merge<Incoming extends AnyZodObject>`
    // shape, `AnyZodObject` aliasing `ZodObject<any, any, any>`) collapses to
    // a spurious TS2536 ("... cannot be used to index type ...").
    //
    // Structural rule: while the class's OWN rough-instance-type summary is
    // being built (`class_type::constructor`'s member scan, used only as the
    // return type of the class's rough construct signatures), a method's
    // declared return-type annotation is fully resolved to compute the
    // summary's callable shape. For a method like `merge` whose return type
    // re-instantiates the enclosing class via a self-referential type
    // parameter constraint, that resolution needs the enclosing class's own
    // instance type — which is not yet published, because this rough scan
    // IS the pass that (approximately) builds it. The first of several
    // redundant `push_type_parameters` materializations for the method's own
    // type parameter therefore resolves the constraint against a
    // still-incomplete answer and spuriously fails an indexed-access check;
    // later materializations, run during the class's real member-check pass
    // once the authoritative instance type is published, resolve correctly.
    // tsc never validates this during shape-building — tsz's method-shape
    // builder needs to defer exactly like the sibling property-initializer
    // fix in #17589.
    // ------------------------------------------------------------------

    #[test]
    fn generic_method_return_type_self_referential_class_constraint_is_clean() {
        let options = resolved_options_for_es2015_strict_test();
        let diagnostics = collect_test_diagnostics_with_options(
            &[(
                "/a.ts",
                r#"
type ZodRawShape = { [key: string]: unknown };

class ZodType<Output = any, Def = any, Input = Output> {}

type extendShape<A extends ZodRawShape, B extends ZodRawShape> = A & B;

class ZodObject<T extends ZodRawShape, UnknownKeys = any, Catchall = any> extends ZodType<
  any,
  any,
  any
> {
  readonly _shape!: T;

  merge<Incoming extends AnyZodObject>(
    merging: Incoming
  ): ZodObject<extendShape<T, Incoming["_shape"]>, UnknownKeys, Catchall> {
    return {} as any;
  }
}

type AnyZodObject = ZodObject<any, any, any>;
"#,
            )],
            &options,
            std::path::Path::new("/"),
        );
        assert!(
            diagnostics.is_empty(),
            "expected a fully clean (tsc-parity) check; got: {diagnostics:#?}"
        );
    }

    #[test]
    fn generic_method_return_type_self_referential_class_constraint_renamed_binders_is_clean() {
        // Anti-hardcoding: a differently-named class, alias, and method/type
        // parameter must not change the outcome.
        let options = resolved_options_for_es2015_strict_test();
        let diagnostics = collect_test_diagnostics_with_options(
            &[(
                "/a.ts",
                r#"
type BoxShape = { [key: string]: unknown };

class Base<Output = any, Def = any, Input = Output> {}

type widenShape<A extends BoxShape, B extends BoxShape> = A & B;

class Box<T extends BoxShape, Extra = any, Other = any> extends Base<any, any, any> {
  readonly payload!: T;

  combine<Other2 extends AnyBox>(
    that: Other2
  ): Box<widenShape<T, Other2["payload"]>, Extra, Other> {
    return {} as any;
  }
}

type AnyBox = Box<any, any, any>;
"#,
            )],
            &options,
            std::path::Path::new("/"),
        );
        assert!(
            diagnostics.is_empty(),
            "expected a fully clean (tsc-parity) check; got: {diagnostics:#?}"
        );
    }

    #[test]
    fn generic_method_return_type_self_referential_class_constraint_genuine_violation_still_reports_ts2536()
     {
        // Negative control: when the enclosing class genuinely lacks the
        // member the indexed access names, TS2536 must still fire — the fix
        // must not turn this into a blanket suppression of the rough-pass
        // method-return-type check.
        let options = resolved_options_for_es2015_strict_test();
        let diagnostics = collect_test_diagnostics_with_options(
            &[(
                "/a.ts",
                r#"
type ZodRawShape = { [key: string]: unknown };

class ZodType<Output = any, Def = any, Input = Output> {}

type extendShape<A extends ZodRawShape, B extends ZodRawShape> = A & B;

class ZodObject<T extends ZodRawShape, UnknownKeys = any, Catchall = any> extends ZodType<
  any,
  any,
  any
> {
  readonly _wrongName!: T;

  merge<Incoming extends AnyZodObject>(
    merging: Incoming
  ): ZodObject<extendShape<T, Incoming["_shape"]>, UnknownKeys, Catchall> {
    return {} as any;
  }
}

type AnyZodObject = ZodObject<any, any, any>;
"#,
            )],
            &options,
            std::path::Path::new("/"),
        );
        let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&2536),
            "expected a genuine TS2536 (ZodObject lacks `_shape`): {diagnostics:?}"
        );
    }
