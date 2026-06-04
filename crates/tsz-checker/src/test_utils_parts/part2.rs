#[cfg(test)]
mod tests {
    //! Self-tests for the `test_utils` helpers themselves.
    //!
    //! These pin the contracts that 100s of checker tests rely on:
    //! - `check_source_diagnostics` ≡ `check_source(source, "test.ts", default)`.
    //! - `check_source_codes` is a code-only projection of `check_source_diagnostics`.
    //! - `diagnostic_code_messages` is a `(code, message)` projection of diagnostics.
    //! - `check_source_code_messages` projects to (code, message) pairs.
    //! - `check_js_source_diagnostics` uses `test.js` + `check_js: true`.
    //! - `check_js_source_code_messages_with_options` uses checked-JS options.
    //! - `check_source_codes_experimental_decorators` enables the decorator flag.
    //! - `check_source_no_unused_params` / `_no_unused_locals` enable the
    //!   matching unused-detection flag.
    //! - `check_with_options` ≡ `check_source(source, "test.ts", options)`.
    use super::*;

    #[test]
    fn check_source_diagnostics_matches_explicit_default_options() {
        // The convenience wrapper must produce the same diagnostics as the
        // 3-arg `check_source` with `"test.ts"` + default options.
        let source = "interface I {} const x = new I();";
        let lhs = check_source_diagnostics(source);
        let rhs = check_source(source, "test.ts", CheckerOptions::default());
        assert_eq!(lhs.len(), rhs.len());
        let lhs_codes: Vec<u32> = lhs.iter().map(|d| d.code).collect();
        let rhs_codes: Vec<u32> = rhs.iter().map(|d| d.code).collect();
        assert_eq!(lhs_codes, rhs_codes);
    }

    #[test]
    fn check_source_codes_is_code_projection_of_diagnostics() {
        let source = "interface I {} const x = new I();";
        let diags = check_source_diagnostics(source);
        let codes = check_source_codes(source);
        let projected: Vec<u32> = diags.iter().map(|d| d.code).collect();
        assert_eq!(codes, projected);
    }

    #[test]
    fn diagnostic_code_messages_projects_owned_diagnostics() {
        let source = "interface I {} const x = new I();";
        let diags = check_source_diagnostics(source);
        let projected: Vec<(u32, String)> = diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect();
        assert_eq!(diagnostic_code_messages(diags), projected);
    }

    #[test]
    fn check_source_code_messages_projects_pairs() {
        let source = "interface I {} const x = new I();";
        let pairs = check_source_code_messages(source);
        let diags = check_source_diagnostics(source);
        assert_eq!(pairs.len(), diags.len());
        for (i, (code, msg)) in pairs.iter().enumerate() {
            assert_eq!(*code, diags[i].code);
            assert_eq!(*msg, diags[i].message_text);
        }
    }

    #[test]
    fn check_source_diagnostics_returns_empty_for_clean_source() {
        let codes = check_source_codes("const x: number = 1;");
        assert!(
            codes.is_empty(),
            "expected no diagnostics for `const x: number = 1;`, got: {codes:?}"
        );
    }

    #[test]
    fn check_source_diagnostics_emits_ts2693_for_interface_as_value() {
        let codes = check_source_codes("interface I {} const x = new I();");
        assert!(
            codes.contains(&2693),
            "expected TS2693 for interface used as value, got: {codes:?}"
        );
    }

    #[test]
    fn check_js_source_diagnostics_uses_check_js_flag() {
        // A JS-specific diagnostic that requires `check_js: true` is the
        // simplest contract test. `function Foo(){ this.x = 1 }; new Foo()`
        // is well-typed under check_js but produces TS7006/TS7041 etc. when
        // an undeclared identifier is used. Use a source with an obvious
        // type error and confirm we see SOME diagnostics under check_js.
        let source = "var x: number = 'hi';";
        let diags = check_js_source_diagnostics(source);
        // Should NOT emit TS2322 — type annotations are syntax errors in JS
        // and the parser path produces TS8010/TS8009 instead. We just want
        // to confirm `check_js: true` was applied (the diagnostics differ
        // from the default-TS path).
        let ts_diags = check_source_diagnostics(source);
        // The two helpers have different filename + check_js flag, so the
        // diagnostic SETS should not be identical for a TS-syntax-in-JS
        // source.
        let js_codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
        let ts_codes: Vec<u32> = ts_diags.iter().map(|d| d.code).collect();
        assert_ne!(
            js_codes, ts_codes,
            "JS source with TS syntax should emit different diagnostics than TS path"
        );
    }

    #[test]
    fn check_js_source_code_messages_with_options_matches_checked_js_projection() {
        let source = "var x: number = 'hi';";
        let opts = CheckerOptions {
            no_implicit_any: true,
            ..CheckerOptions::default()
        };
        let pairs = check_js_source_code_messages_with_options(source, "custom.js", opts.clone());
        let explicit = check_source(
            source,
            "custom.js",
            CheckerOptions {
                allow_js: true,
                check_js: true,
                ..opts
            },
        );
        assert_eq!(pairs, diagnostic_code_messages(explicit));
    }

    #[test]
    fn check_source_no_unused_params_emits_ts6133() {
        let source = "function f(unused: number) {}";
        let diags = check_source_no_unused_params(source);
        let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&6133),
            "expected TS6133 for unused parameter, got: {codes:?}"
        );
    }

    #[test]
    fn check_source_no_unused_locals_emits_ts6133() {
        let source = "function f() { var unused: number = 1; }";
        let diags = check_source_no_unused_locals(source);
        let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&6133),
            "expected TS6133 for unused local, got: {codes:?}"
        );
    }

    #[test]
    fn check_with_options_matches_check_source_with_test_ts() {
        // `check_with_options(source, opts)` is exactly
        // `check_source(source, "test.ts", opts)` — pin that.
        let opts = CheckerOptions {
            no_unused_parameters: true,
            ..Default::default()
        };
        let source = "function f(unused: number) {}";
        let lhs = check_with_options(source, opts.clone());
        let rhs = check_source(source, "test.ts", opts);
        let lhs_codes: Vec<u32> = lhs.iter().map(|d| d.code).collect();
        let rhs_codes: Vec<u32> = rhs.iter().map(|d| d.code).collect();
        assert_eq!(lhs_codes, rhs_codes);
    }

    #[test]
    fn check_source_codes_experimental_decorators_clean_decorator_compiles() {
        // With `experimental_decorators` enabled, a well-typed decorator
        // application must not produce diagnostics. This pins that the flag
        // gets propagated through `CheckerOptions` to the checker.
        let source = r#"
function dec(target: any) { return target; }
@dec
class C {}
"#;
        let codes = check_source_codes_experimental_decorators(source);
        // No TS1219 ("Experimental decorator") gate.
        assert!(
            !codes.contains(&1219),
            "experimental_decorators flag should suppress TS1219, got: {codes:?}"
        );
    }

    #[test]
    fn strict_checker_options_sets_canonical_triple() {
        let opts = strict_checker_options();
        assert!(opts.strict, "strict_checker_options must set strict");
        assert!(
            opts.strict_null_checks,
            "strict_checker_options must set strict_null_checks"
        );
        assert!(
            opts.no_implicit_any,
            "strict_checker_options must set no_implicit_any"
        );
        // Other fields are explicit defaults — the factory must not silently
        // turn them on (callers rely on overlay-by-spread).
        let defaults = CheckerOptions::default();
        assert_eq!(opts.strict_function_types, defaults.strict_function_types);
        assert_eq!(
            opts.exact_optional_property_types,
            defaults.exact_optional_property_types
        );
    }

    #[test]
    fn check_source_strict_matches_explicit_strict_options() {
        let source = "let s: string = 1;";
        let lhs = check_source_strict(source);
        let rhs = check_with_options(source, strict_checker_options());
        let lhs_codes: Vec<u32> = lhs.iter().map(|d| d.code).collect();
        let rhs_codes: Vec<u32> = rhs.iter().map(|d| d.code).collect();
        assert_eq!(lhs_codes, rhs_codes);
    }

    #[test]
    fn check_with_options_code_messages_projects_custom_option_diagnostics() {
        let source = "function f() { return this; }";
        let opts = CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_this: true,
            ..CheckerOptions::default()
        };
        let pairs = check_with_options_code_messages(source, opts.clone());
        let diags = check_with_options(source, opts);
        assert_eq!(pairs.len(), diags.len());
        assert!(
            pairs.iter().any(|(code, _)| *code == 2683),
            "expected custom noImplicitThis options to report TS2683, got {pairs:?}"
        );
        for (i, pair) in pairs.iter().enumerate() {
            assert_eq!(pair.0, diags[i].code);
            assert_eq!(pair.1, diags[i].message_text);
        }
    }

    #[test]
    fn check_source_strict_codes_and_messages_project_strict_diagnostics() {
        let source = "let s: string = 1;";
        let codes = check_source_strict_codes(source);
        let pairs = check_source_strict_messages(source);
        let diags = check_source_strict(source);
        assert_eq!(codes.len(), diags.len());
        assert_eq!(pairs.len(), diags.len());
        for (i, code) in codes.iter().enumerate() {
            assert_eq!(*code, diags[i].code);
            assert_eq!(pairs[i].0, diags[i].code);
            assert_eq!(pairs[i].1, diags[i].message_text);
        }
    }

    #[test]
    fn check_source_strict_emits_ts2322_for_implicit_string_to_number() {
        // strict + strictNullChecks + noImplicitAny is enough to surface the
        // TS2322 mismatch on `let s: string = 1;`.
        let codes = check_source_strict_codes("let s: string = 1;");
        assert!(
            codes.contains(&2322),
            "expected TS2322 under strict_checker_options, got: {codes:?}"
        );
    }

    #[test]
    fn check_source_lib_contexts_are_empty_no_ts2318() {
        // The wrapper's `set_lib_contexts(Vec::new())` step prevents
        // spurious TS2318 ("Cannot find global type") errors that would
        // otherwise fire for built-in types like Promise/Array. Pin that
        // a source that uses `Promise` does NOT emit TS2318.
        let source = "let p: Promise<number>;";
        let codes = check_source_codes(source);
        assert!(
            !codes.contains(&2318),
            "set_lib_contexts(empty) must prevent TS2318 for Promise, got: {codes:?}"
        );
    }

    #[test]
    fn load_default_lib_files_finds_es5_and_es2015_promise() {
        // The DEFAULT_LIB_NAMES bundle must resolve at least the core
        // typings every checker test relies on. If the bundled
        // `lib-assets-stripped/` ever loses one of these the checker
        // tests that use Promise/Array will silently lose lib coverage.
        let libs = load_default_lib_files();
        let names: Vec<&str> = libs.iter().map(|l| l.file_name.as_str()).collect();
        assert!(
            names.contains(&"es5.d.ts"),
            "DEFAULT_LIB_NAMES must resolve es5.d.ts in some root, got: {names:?}"
        );
        assert!(
            names.contains(&"es2015.promise.d.ts"),
            "DEFAULT_LIB_NAMES must resolve es2015.promise.d.ts, got: {names:?}"
        );
    }

    #[test]
    fn load_lib_files_dedupes_and_skips_missing() {
        // Duplicates in the input must not produce duplicate LibFiles.
        // Names that don't exist in any root must be silently dropped.
        let libs = load_lib_files(&["es5.d.ts", "es5.d.ts", "definitely_missing_lib.d.ts"]);
        let names: Vec<&str> = libs.iter().map(|l| l.file_name.as_str()).collect();
        assert_eq!(names.iter().filter(|n| **n == "es5.d.ts").count(), 1);
        assert!(!names.contains(&"definitely_missing_lib.d.ts"));
    }

    #[test]
    fn check_source_with_libs_resolves_promise_no_ts2318() {
        // With libs loaded, `Promise<number>` is a known global type, so
        // checking this source must not emit TS2318. (Without libs, the
        // empty-lib wrapper avoids TS2318 by suppressing global lookups
        // entirely; with libs, the global lookup must succeed.)
        let libs = load_default_lib_files();
        assert!(!libs.is_empty(), "expected default libs to load");
        let diags = check_source_with_libs(
            "let p: Promise<number>;",
            "test.ts",
            CheckerOptions::default(),
            &libs,
        );
        let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
        assert!(
            !codes.contains(&2318),
            "Promise must resolve via loaded libs, got: {codes:?}"
        );
    }

    /// Synthetic `lib.*.d.ts` exercising namespaced lib interfaces whose
    /// `extends` clause names a base interface declared in the same namespace.
    /// Mirrors the shape of `Temporal.RoundingOptionsWithLargestUnit` /
    /// `Temporal.DurationRoundingOptions` without depending on the full lib.
    /// `param_name` varies the bound type-parameter spelling so the fix cannot
    /// be hardcoded to a particular identifier.
    fn namespaced_heritage_lib(param_name: &str) -> Vec<Arc<LibFile>> {
        let source = format!(
            "declare namespace NS {{\n\
                 type Unit = \"x\" | \"y\";\n\
                 type Plural<{param_name} extends Unit> = {param_name} | {{ x: \"xs\"; y: \"ys\" }}[{param_name}];\n\
                 interface RoundOpts<{param_name} extends Unit> {{\n\
                     small?: Plural<{param_name}> | undefined;\n\
                     mode?: \"a\" | \"b\" | undefined;\n\
                 }}\n\
                 interface RoundOptsLargest<{param_name} extends Unit> extends RoundOpts<{param_name}> {{\n\
                     large?: \"auto\" | Plural<{param_name}> | undefined;\n\
                 }}\n\
                 interface RelativeOpts {{ relative?: string | undefined; }}\n\
                 interface DurationOpts extends RelativeOpts, RoundOptsLargest<Unit> {{}}\n\
             }}\n"
        );
        vec![Arc::new(LibFile::from_source(
            "lib.es2099.synthetic.d.ts".to_string(),
            source,
        ))]
    }

    #[test]
    fn namespaced_lib_interface_inherits_base_members() {
        // A namespaced interface that `extends` another namespaced interface
        // must expose the base's members (single inheritance level).
        for param in ["U", "T", "K"] {
            let libs = namespaced_heritage_lib(param);
            let codes: Vec<u32> = check_source_with_libs(
                "declare const o: NS.RoundOptsLargest<NS.Unit>;\n\
                 const a = o.small;\n\
                 const b = o.large;\n\
                 const c = o.mode;\n",
                "test.ts",
                CheckerOptions::default(),
                &libs,
            )
            .iter()
            .map(|d| d.code)
            .collect();
            assert!(
                !codes.contains(&2339),
                "inherited member access on NS.RoundOptsLargest (param {param}) must not emit TS2339, got: {codes:?}"
            );
        }
    }

    #[test]
    fn namespaced_lib_interface_excess_property_uses_inherited_members() {
        // Inherited optional members must count as known properties so the
        // object literal is not flagged with a spurious TS2353, while a truly
        // unknown property still is.
        let libs = namespaced_heritage_lib("U");
        let ok_codes: Vec<u32> = check_source_with_libs(
            "declare function f(o?: NS.RoundOptsLargest<NS.Unit>): void;\n\
             f({ large: \"x\", small: \"x\" });\n",
            "test.ts",
            CheckerOptions::default(),
            &libs,
        )
        .iter()
        .map(|d| d.code)
        .collect();
        assert!(
            !ok_codes.contains(&2353),
            "object literal using inherited `small` must not emit TS2353, got: {ok_codes:?}"
        );

        let bad_codes: Vec<u32> = check_source_with_libs(
            "declare function f(o?: NS.RoundOptsLargest<NS.Unit>): void;\n\
             f({ large: \"x\", bogus: 1 });\n",
            "test.ts",
            CheckerOptions::default(),
            &libs,
        )
        .iter()
        .map(|d| d.code)
        .collect();
        assert!(
            bad_codes.contains(&2353),
            "a genuinely unknown property must still emit TS2353, got: {bad_codes:?}"
        );
    }

    #[test]
    fn namespaced_lib_interface_inherits_transitive_members_after_base_resolved() {
        // `DurationOpts extends RelativeOpts, RoundOptsLargest<Unit>` inherits
        // `small` transitively (RoundOptsLargest -> RoundOpts). Resolving the
        // intermediate `RoundOptsLargest` first must not poison the cache and
        // strip the transitive member from `DurationOpts`.
        let libs = namespaced_heritage_lib("U");
        let codes: Vec<u32> = check_source_with_libs(
            "declare const m: NS.RoundOptsLargest<NS.Unit>;\n\
             const pre = m.small;\n\
             declare const d: NS.DurationOpts;\n\
             const a = d.small;\n\
             const b = d.large;\n\
             const c = d.relative;\n",
            "test.ts",
            CheckerOptions::default(),
            &libs,
        )
        .iter()
        .map(|d| d.code)
        .collect();
        assert!(
            !codes.contains(&2339),
            "transitively inherited members on NS.DurationOpts must resolve even after the intermediate base is resolved first, got: {codes:?}"
        );
    }

    #[test]
    fn check_source_with_libs_code_messages_projects_diagnostics() {
        let pairs = check_source_with_libs_code_messages(
            "const x: string = 1;",
            "test.ts",
            CheckerOptions::default(),
            &[],
        );
        assert!(
            pairs
                .iter()
                .any(|(code, message)| *code == 2322 && message.contains("number")),
            "expected TS2322 code/message projection, got: {pairs:?}"
        );
    }

    #[test]
    fn check_source_with_libs_empty_matches_check_source() {
        // Calling `check_source_with_libs` with an empty slice must
        // produce the exact same diagnostics as `check_source`. This
        // pins the no-lib code path as a strict superset of the lib
        // path and guards against drift between the two helpers.
        let source = "interface I {} const x = new I();";
        let lhs = check_source_with_libs(source, "test.ts", CheckerOptions::default(), &[]);
        let rhs = check_source(source, "test.ts", CheckerOptions::default());
        let lhs_codes: Vec<u32> = lhs.iter().map(|d| d.code).collect();
        let rhs_codes: Vec<u32> = rhs.iter().map(|d| d.code).collect();
        assert_eq!(lhs_codes, rhs_codes);
    }

    #[test]
    fn load_compiled_lib_files_preserves_lib_prefix_naming() {
        // Tests that depend on the `source.file_name.starts_with("lib.")`
        // gate at lib_resolution.rs:983 (or assert against
        // `Diagnostic.file == "lib.es5.d.ts"`) require the LibFile name
        // to retain the `lib.` prefix — load_compiled_lib_files must
        // store names verbatim. We can't assume the compiled lib roots
        // are populated in every dev environment (npm install ts under
        // scripts/, or `git submodule update` for TypeScript/lib), so
        // only assert on the *naming* if at least one file resolved.
        let libs = load_compiled_lib_files(&["lib.es5.d.ts"]);
        if let Some(lib) = libs.first() {
            assert_eq!(
                lib.file_name, "lib.es5.d.ts",
                "load_compiled_lib_files must store names with the `lib.` prefix verbatim"
            );
        }
        // Dedup contract holds even when nothing resolves.
        let dup = load_compiled_lib_files(&[
            "lib.es5.d.ts",
            "lib.es5.d.ts",
            "lib.definitely_missing.d.ts",
        ]);
        assert!(dup.len() <= 1);
    }

    #[test]
    fn load_compiled_lib_files_resolves_when_only_primary_has_node_modules() {
        // When run from a worktree under `<primary>/.worktrees/<name>/`,
        // the worktree-relative `../../scripts/node_modules/...` paths
        // resolve into the worktree's empty scripts/ tree. This test
        // ensures the helper's walk-up fallback finds the primary
        // checkout's scripts/node_modules/typescript/lib/ when at
        // least one of the standard `npm install` directories has been
        // populated above the worktree.
        //
        // Skipped silently in environments without any compiled libs.
        let libs = load_compiled_lib_files(&["lib.es5.d.ts"]);
        // No assertion when the env is missing all three install dirs;
        // this is the same robustness pattern the test above uses.
        // When the helper does find a file, it must have the `lib.`
        // prefix and be readable.
        if let Some(lib) = libs.first() {
            assert!(
                !lib.arena.source_files.is_empty(),
                "loaded LibFile must have a parsed source file"
            );
            assert!(lib.file_name.starts_with("lib."));
        }
    }

    // =========================================================================
    // line_column_for_offset / diagnostic_line_column / DiagnosticShape
    //
    // Lock the location-aware diagnostic assertion helpers added for issue
    // #8488. The two key correctness contracts are:
    //   - 1-indexed line/column with UTF-16 column units (matches tsc/LSP).
    //   - Panic messages on near-miss surface the actual `(line, column)`
    //     and reason — the information `assert!(codes.contains(&NNNN), ..)`
    //     swallows.
    // =========================================================================

    #[test]
    fn line_column_for_offset_returns_one_indexed_for_start_of_source() {
        assert_eq!(line_column_for_offset("abc", 0), (1, 1));
        assert_eq!(line_column_for_offset("", 0), (1, 1));
    }

    #[test]
    fn line_column_for_offset_advances_columns_within_a_line() {
        let source = "let s: string = 1;";
        assert_eq!(line_column_for_offset(source, 0), (1, 1));
        assert_eq!(line_column_for_offset(source, 4), (1, 5));
        // Offset just past the last char clamps to end of line.
        let end = u32::try_from(source.len()).unwrap();
        let (line, _) = line_column_for_offset(source, end);
        assert_eq!(line, 1);
    }

    #[test]
    fn line_column_for_offset_advances_lines_across_newlines() {
        // "\nfoo\nbar": offset 0='\n' -> (1,1); offset 1='f' -> (2,1);
        // offset 5='b' -> (3,1).
        let source = "\nfoo\nbar";
        assert_eq!(line_column_for_offset(source, 0), (1, 1));
        assert_eq!(line_column_for_offset(source, 1), (2, 1));
        assert_eq!(line_column_for_offset(source, 5), (3, 1));
    }

    #[test]
    fn line_column_for_offset_counts_utf16_units_for_bmp_characters() {
        // 'é' is a single UTF-16 code unit (2 bytes in UTF-8). Column after
        // 'é' must be 2 (1-indexed UTF-16 units = 1 unit past start).
        let source = "éX"; // bytes: [c3 a9, 58], offset 2 = start of 'X'
        assert_eq!(line_column_for_offset(source, 2), (1, 2));
    }

    #[test]
    fn diagnostic_line_column_uses_diagnostic_start_offset() {
        // `let s: string = 1;` -> TS2322 anchors on the offending `1`.
        let source = "let s: string = 1;";
        let diags = check_source_strict(source);
        let ts2322 = diags
            .iter()
            .find(|d| d.code == 2322)
            .expect("expected TS2322 for string = number");
        let (line, column) = diagnostic_line_column(source, ts2322);
        // The diagnostic should be on line 1, and the column must equal
        // `start + 1` (UTF-16 == byte for ASCII).
        assert_eq!(line, 1);
        assert_eq!(column, ts2322.start + 1);
    }

    #[test]
    fn diagnostic_shape_builder_pins_fields_independently() {
        let shape = DiagnosticShape::code(2322)
            .at(3, 5)
            .with_message_fragment("not assignable")
            .with_related_min(1);
        assert_eq!(shape.code, 2322);
        assert_eq!(shape.line, Some(3));
        assert_eq!(shape.column, Some(5));
        assert_eq!(shape.message_fragment, Some("not assignable"));
        assert_eq!(shape.related_min, Some(1));
    }

    #[test]
    fn assert_diagnostic_shape_matches_code_line_column_and_fragment() {
        let source = "let s: string = 1;";
        let diags = check_source_strict(source);
        let ts2322 = diags
            .iter()
            .find(|d| d.code == 2322)
            .expect("expected TS2322");
        let (line, column) = diagnostic_line_column(source, ts2322);
        let matched = assert_diagnostic_shape(
            source,
            &diags,
            &DiagnosticShape::code(2322)
                .at(line, column)
                .with_message_fragment("is not assignable to type"),
        );
        assert_eq!(matched.code, 2322);
    }

    #[test]
    fn assert_diagnostic_shape_panics_with_near_miss_detail_on_wrong_line() {
        // Deliberately wrong line. The panic message must surface the
        // near-miss with the actual emitted location so the test author
        // can see the diagnostic moved, rather than guessing what went
        // wrong. Capture the panic payload and assert on the structure
        // rather than `#[should_panic]` so we can verify the rich content.
        let source = "let s: string = 1;";
        let diags = check_source_strict(source);
        let payload = std::panic::catch_unwind(|| {
            assert_diagnostic_shape(source, &diags, &DiagnosticShape::code(2322).at(999, 1));
        })
        .expect_err("near-miss assertion must panic");
        let message = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&'static str>().copied())
            .unwrap_or("<non-string panic>");
        assert!(
            message.contains("Expected diagnostic shape"),
            "panic must mention the shape that failed; got: {message}"
        );
        assert!(
            message.contains("Emitted codes: [2322]"),
            "panic must list the emitted codes for triage; got: {message}"
        );
        assert!(
            message.contains("expected 999"),
            "panic must mention the expected line value so the author can see it was wrong; got: {message}"
        );
    }

    #[test]
    #[should_panic(expected = "no diagnostic with the expected code")]
    fn assert_diagnostic_shape_panics_when_no_diagnostic_has_the_code() {
        let source = "let s: string = 1;";
        let diags = check_source_strict(source);
        // A code that this source can never emit. The panic must say "no
        // diagnostic with the expected code was emitted" rather than
        // listing irrelevant near-misses.
        assert_diagnostic_shape(source, &diags, &DiagnosticShape::code(9999));
    }

    #[test]
    fn assert_diagnostic_shape_is_rename_agnostic_when_fragment_is_structural() {
        // Anti-hardcoding (§25): the same DiagnosticShape with a structural
        // message fragment matches two fixtures that differ only in
        // user-chosen identifier names. If a future change made the matcher
        // depend on identifier spelling, one of these two asserts would fail.
        let shape = DiagnosticShape::code(2322).with_message_fragment("is not assignable to type");
        let lhs = check_source_strict("let alpha: string = 1;");
        let rhs = check_source_strict("let beta: string = 2;");
        assert_diagnostic_shape("let alpha: string = 1;", &lhs, &shape);
        assert_diagnostic_shape("let beta: string = 2;", &rhs, &shape);
    }

    #[test]
    fn assert_diagnostic_shapes_passes_when_every_shape_has_a_match() {
        // Two distinct diagnostics from one source. The presence-based
        // helper accepts the set as long as every shape matches. We pin
        // the `(line, column)` from the actual emitted diagnostics rather
        // than from a hardcoded guess at the anchor offset, so this test
        // stays a contract over the matcher, not over the checker's
        // current anchor policy.
        let source = "let s: string = 1; let t: number = '';";
        let diags = check_source_strict(source);
        let pairs: Vec<(u32, u32)> = diags
            .iter()
            .filter(|d| d.code == 2322)
            .map(|d| diagnostic_line_column(source, d))
            .collect();
        assert!(
            pairs.len() >= 2,
            "fixture must emit at least two TS2322s, got: {pairs:?} from {diags:#?}"
        );
        let shapes = [
            DiagnosticShape::code(2322).at(pairs[0].0, pairs[0].1),
            DiagnosticShape::code(2322).at(pairs[1].0, pairs[1].1),
        ];
        assert_diagnostic_shapes(source, &diags, &shapes);
    }

    #[test]
    fn assert_diagnostic_shapes_exactly_rejects_extra_unmatched_diagnostics() {
        // Two TS2322 are emitted but the test only declares one shape.
        // `_exactly` must reject the extra diagnostic.
        let source = "let s: string = 1; let t: number = '';";
        let diags = check_source_strict(source);
        let first = diags
            .iter()
            .find(|d| d.code == 2322)
            .expect("fixture must emit at least one TS2322");
        let (line, column) = diagnostic_line_column(source, first);
        let single_shape = [DiagnosticShape::code(2322).at(line, column)];
        let panicked = std::panic::catch_unwind(|| {
            assert_diagnostic_shapes_exactly(source, &diags, &single_shape);
        });
        assert!(
            panicked.is_err(),
            "_exactly must panic when an emitted diagnostic has no matching shape"
        );
    }

    #[test]
    fn assert_diagnostic_shapes_exactly_rejects_missing_expected_shapes() {
        // The expected shape is for a diagnostic that the source does not
        // emit. `_exactly` must panic listing the missing shape.
        let diags: Vec<Diagnostic> = Vec::new();
        let shape = [DiagnosticShape::code(2322).at(1, 1)];
        let panicked =
            std::panic::catch_unwind(|| assert_diagnostic_shapes_exactly("", &diags, &shape));
        assert!(
            panicked.is_err(),
            "_exactly must panic when a declared shape never matched any diagnostic"
        );
    }
}
