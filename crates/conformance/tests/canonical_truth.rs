use std::collections::HashMap;
use std::path::Path;
use tsz_conformance::tsz_wrapper::{parse_batch_output, parse_tsz_output, SemanticCompletion};

fn process_output(code: i32, stdout: &[u8], stderr: &[u8]) -> std::process::Output {
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;
    #[cfg(windows)]
    use std::os::windows::process::ExitStatusExt;

    std::process::Output {
        status: {
            #[cfg(unix)]
            {
                std::process::ExitStatus::from_raw(code << 8)
            }
            #[cfg(windows)]
            {
                std::process::ExitStatus::from_raw(code as u32)
            }
        },
        stdout: stdout.to_vec(),
        stderr: stderr.to_vec(),
    }
}

#[test]
fn canonical_tsz_boundary_has_no_result_repair_hooks() {
    let sources = [
        ("runner.rs", include_str!("../src/runner.rs")),
        ("runner/plan.rs", include_str!("../src/runner/plan.rs")),
        (
            "runner/helpers.rs",
            include_str!("../src/runner/helpers.rs"),
        ),
        ("tsz_wrapper.rs", include_str!("../src/tsz_wrapper.rs")),
        ("cache.rs", include_str!("../src/cache.rs")),
        ("corpus.rs", include_str!("../src/corpus.rs")),
        ("integrity.rs", include_str!("../src/integrity.rs")),
        ("jsonc.rs", include_str!("../src/jsonc.rs")),
        ("oracle.rs", include_str!("../src/oracle.rs")),
        ("cli.rs", include_str!("../src/cli.rs")),
        (
            "compiler_options.rs",
            include_str!("../src/compiler_options.rs"),
        ),
        ("lib.rs", include_str!("../src/lib.rs")),
        ("main.rs", include_str!("../src/main.rs")),
        (
            "test_directives.rs",
            include_str!("../src/test_directives.rs"),
        ),
        ("test_filter.rs", include_str!("../src/test_filter.rs")),
        ("test_parser.rs", include_str!("../src/test_parser.rs")),
        ("text_decode.rs", include_str!("../src/text_decode.rs")),
        ("tsc_results.rs", include_str!("../src/tsc_results.rs")),
        (
            "bin/generate-tsc-cache.rs",
            include_str!("../src/bin/generate-tsc-cache.rs"),
        ),
        (
            "scripts/conformance/conformance.sh",
            include_str!("../../../scripts/conformance/conformance.sh"),
        ),
        (
            "scripts/conformance/oracle.sh",
            include_str!("../../../scripts/conformance/oracle.sh"),
        ),
        (
            "scripts/conformance/lib/results.py",
            include_str!("../../../scripts/conformance/lib/results.py"),
        ),
        (
            "scripts/conformance/lib/cache_domain.py",
            include_str!("../../../scripts/conformance/lib/cache_domain.py"),
        ),
        (
            "scripts/conformance/build-snapshot-detail.py",
            include_str!("../../../scripts/conformance/build-snapshot-detail.py"),
        ),
        (
            "scripts/conformance/validate-cache-domain.py",
            include_str!("../../../scripts/conformance/validate-cache-domain.py"),
        ),
        (
            "scripts/conformance/extract-baseline.py",
            include_str!("../../../scripts/conformance/extract-baseline.py"),
        ),
        (
            "scripts/conformance/build-manifest.py",
            include_str!("../../../scripts/conformance/build-manifest.py"),
        ),
        (
            "scripts/conformance/snapshot-provenance.py",
            include_str!("../../../scripts/conformance/snapshot-provenance.py"),
        ),
        (
            "scripts/conformance/validate-runner-output.py",
            include_str!("../../../scripts/conformance/validate-runner-output.py"),
        ),
    ];
    let forbidden = [
        "fn is_extra_",
        "filter_lib_diagnostics_tsz",
        "filter_lib_diagnostics_tsc",
        "filter_extra_typescript",
        "is_lib_diagnostic",
        "suppress_tsz",
        "use_fingerprint_compare",
        "tsc_expects_",
        "tsc_has_",
        "expected_error_codes",
        "compile_result.error_codes.retain",
        "compile_result.diagnostic_fingerprints.retain",
        "diagnostic_fingerprints.dedup",
        "error_codes.dedup",
        "compile_with_subprocess",
        "saturating_mul(2).max(60)",
        "_legacy_unused_codes",
        "normalize_message_key",
        "normalize_builtin_iterator_return_message",
        "serialized retry",
        "↻ Retrying",
        "sort_diagnostic_fingerprints",
        "normalize_file_not_found_message_key",
        "normalize_temp_directory_paths",
        "normalize_ts2883_node_modules_message",
        "filter_map(std::result::Result::ok)",
        "hasDiagnostics",
        "TSZ_CI_TRUST_DIST_FAST_CACHE",
        "unwrap_or_else(|_| serde_json::json!({}))",
    ];

    let mut violations = Vec::new();
    for (path, source) in sources {
        for needle in forbidden {
            if source.contains(needle) {
                violations.push(format!("{path}: {needle}"));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "canonical TSZ output must reach comparison without oracle-conditioned repair: {violations:?}"
    );
}

#[test]
fn canonical_runner_retains_only_the_fresh_process_contract() {
    let runner = include_str!("../src/runner.rs");
    let production = runner.split("#[cfg(test)]").next().unwrap_or(runner);
    let cli = include_str!("../src/cli.rs");
    let source_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert!(!source_dir.join("server_pool.rs").exists());
    assert!(!source_dir.join("batch_pool.rs").exists());
    assert!(!source_dir.join("options_convert.rs").exists());
    assert!(!source_dir.join("process_rss.rs").exists());
    assert!(!production.contains("ProcessPool"));
    assert!(!production.contains("ServerPool"));
    assert!(!production.contains("parse_batch_output"));
    assert!(production.contains("tokio::process::Command::new(tsz_binary)"));
    assert!(production.contains("child.wait_with_output()"));
    assert!(cli.contains("Fresh"));
    assert!(cli.contains("noncanonical performance transport and cannot score conformance"));
}

#[test]
fn verbose_identity_output_cannot_hide_passing_rows() {
    let runner = include_str!("../src/runner.rs");
    let production = runner.split("#[cfg(test)]").next().unwrap_or(runner);
    assert!(!production.contains("print_test && !verbose"));
    assert!(production.contains("if print_test {\n                                        writeln!(buf, \"PASS {}\", rel_path)"));
}

#[test]
fn final_summary_is_emitted_only_after_fallible_completion_checks() {
    let runner = include_str!("../src/runner.rs");
    let production = runner.split("#[cfg(test)]").next().unwrap_or(runner);
    let final_results = production.find("FINAL RESULTS:").expect("final summary");
    assert!(
        production
            .find("fatal worker errors")
            .expect("worker infrastructure failure check")
            < final_results
    );
    assert!(
        production
            .find("if !summary.has_result_bijection()")
            .expect("bijection check")
            < final_results
    );
    assert!(
        production
            .find("failed to write timings file")
            .expect("fallible timings write")
            < final_results
    );
    assert!(!production.contains("println!(\"FAIL {} (ERROR: {})\""));
}

#[test]
fn corpus_git_identity_disables_routing_and_replace_objects() {
    let corpus = include_str!("../src/corpus.rs");
    assert!(corpus.contains("GIT_REPLACE_REF_BASE"));
    assert!(corpus.contains("GIT_NO_REPLACE_OBJECTS"));
    assert!(corpus.contains("tests/lib"));
}

#[test]
fn utf8_and_utf16_share_one_selector_and_fresh_variant_executor() {
    let runner = include_str!("../src/runner.rs");
    let production = runner.split("#[cfg(test)]").next().unwrap_or(runner);
    assert_eq!(
        production.matches("Self::compile_text_variants(").count(),
        2
    );
    assert_eq!(
        production
            .matches("select_ts7_oracle_configurations(directives)")
            .count(),
        1
    );
    assert!(production.contains("for variant in option_variants"));
    assert!(production.contains("tokio::process::Command::new(tsz_binary)"));
}

#[test]
fn cache_generator_has_no_unverified_oracle_fallback() {
    let generator = include_str!("../src/bin/generate-tsc-cache.rs");
    assert!(generator.contains("resolve_verified_oracle"));
    assert!(generator.contains("Command::new(tsc_path)"));
    for forbidden in [
        "resolve_tsc_path",
        "resolve_tsc_version",
        "require.resolve",
        "Command::new(\"which\")",
        "Command::new(\"npx\")",
        "npx:tsc",
        "unknown\".to_string()",
    ] {
        assert!(
            !generator.contains(forbidden),
            "unverified oracle fallback survived: {forbidden}"
        );
    }
}

#[test]
fn trace_resolution_products_are_terminal_nonclaims_before_oracle_execution() {
    let parser = include_str!("../src/test_parser.rs");
    let generator = include_str!("../src/bin/generate-tsc-cache.rs");
    let runner = include_str!("../src/runner.rs");

    assert!(parser.contains("TraceResolutionOutputNotCompared"));
    assert!(parser.contains("crate::jsonc::parse_jsonc(content)"));
    assert!(generator.contains("TestDisposition::Unsupported(reason)"));
    assert!(generator.contains("ProcessOutcome::Unsupported"));
    assert!(runner.contains("TestDisposition::Unsupported(reason)"));
}

#[test]
fn manual_oracle_uses_the_same_verified_native_resolver() {
    let oracle = include_str!("../../../scripts/conformance/oracle.sh");
    assert!(oracle.contains("scripts/emit/resolve-oracle.mjs"));
    assert!(oracle.contains("exec \"$TSC_BIN\""));
    assert!(oracle.contains("ensure-pinned-typescript.sh\" \"$REPO_ROOT/scripts\" >&2"));
    assert!(oracle.contains("ORACLE_JSON=\"$(node --experimental-strip-types"));
    assert!(oracle.contains("echo \"# oracle: typescript@$PINNED_VERSION"));
    assert!(oracle.contains("${EXTRA_FLAGS[*]:-}\" >&2"));
    for forbidden in [
        "typescript/lib/tsc.js",
        "npm install",
        "require(process.argv",
        "TSZ_ORACLE_CACHE_DIR",
        "exec node \"$TSC",
    ] {
        assert!(
            !oracle.contains(forbidden),
            "manual oracle fallback survived: {forbidden}"
        );
    }
}

#[test]
fn oracle_cache_requires_first_run_grouped_block_evidence() {
    let generator = include_str!("../src/bin/generate-tsc-cache.rs");
    let wrapper = include_str!("../../../scripts/conformance/conformance.sh");
    assert!(generator.contains("diagnostic_blocks_complete: true"));
    assert!(generator.contains("ordinary_exit_statuses"));
    assert!(generator.contains("result.crashed || !result.semantic_completion.is_complete()"));
    assert!(wrapper.contains("entry.diagnostic_blocks_complete !== true"));
    assert!(wrapper.contains("entry.ordinary_exit_statuses"));
    assert!(wrapper.contains("cache lacks complete grouped diagnostic-block evidence"));
}

#[test]
fn panic_exit_101_is_a_crash_not_complete_empty() {
    let output = process_output(101, &[], b"thread 'main' panicked at internal invariant\n");
    let result = parse_tsz_output(&output, Path::new("/tmp/project"), HashMap::new());

    assert!(result.crashed);
    assert_eq!(result.semantic_completion, SemanticCompletion::Incomplete);
    assert!(result.diagnostic_fingerprints.is_empty());
}

#[test]
fn internal_nonzero_output_is_a_crash_not_complete_empty() {
    let output = process_output(1, &[], b"internal compiler error\n");
    let result = parse_tsz_output(&output, Path::new("/tmp/project"), HashMap::new());

    assert!(result.crashed);
    assert_eq!(result.semantic_completion, SemanticCompletion::Incomplete);
    assert!(result.diagnostic_fingerprints.is_empty());
}

#[test]
fn batch_internal_text_is_a_crash_not_complete_empty() {
    let result = parse_batch_output(
        "internal compiler error\n---TSZ-SEMANTIC-COMPLETION:complete---\n",
        Path::new("/tmp/project"),
        HashMap::new(),
    );

    assert!(result.crashed);
    assert_eq!(result.semantic_completion, SemanticCompletion::Incomplete);
}

#[test]
fn batch_parser_preserves_identical_diagnostic_multiplicity() {
    let diagnostic = "test.ts(1,1): error TS2304: Cannot find name 'missing'.\n";
    let result = parse_batch_output(
        &format!("{diagnostic}{diagnostic}---TSZ-SEMANTIC-COMPLETION:complete---\n"),
        Path::new("/tmp/project"),
        HashMap::new(),
    );

    assert!(!result.crashed);
    assert_eq!(result.error_codes, vec![2304, 2304]);
    assert_eq!(result.diagnostic_fingerprints.len(), 2);
    assert_eq!(
        result.diagnostic_fingerprints[0],
        result.diagnostic_fingerprints[1]
    );
}

#[test]
fn fresh_mixed_diagnostic_and_unparsed_text_is_a_crash() {
    let diagnostic = b"test.ts(1,1): error TS2304: Cannot find name 'missing'.\n";
    for extra in [
        "thread 'main' panicked at internal invariant\n",
        "internal compiler error\n",
        "unrecognized transport garbage\n",
    ] {
        let mut stdout = diagnostic.to_vec();
        stdout.extend_from_slice(extra.as_bytes());
        let result = parse_tsz_output(
            &process_output(1, &stdout, &[]),
            Path::new("/tmp/project"),
            HashMap::new(),
        );
        assert!(result.crashed, "mixed output passed for {extra:?}");
        assert_eq!(result.semantic_completion, SemanticCompletion::Incomplete);
        assert_eq!(result.error_codes, [2304]);
        assert_eq!(result.diagnostic_fingerprints.len(), 1);
    }
}

#[test]
fn batch_mixed_diagnostic_and_unparsed_text_is_a_crash() {
    let diagnostic = "test.ts(1,1): error TS2304: Cannot find name 'missing'.\n";
    for extra in [
        "thread 'main' panicked at internal invariant\n",
        "internal compiler error\n",
        "unrecognized transport garbage\n",
    ] {
        let output = format!("{diagnostic}{extra}---TSZ-SEMANTIC-COMPLETION:complete---\n");
        let result = parse_batch_output(&output, Path::new("/tmp/project"), HashMap::new());
        assert!(result.crashed, "mixed output passed for {extra:?}");
        assert_eq!(result.semantic_completion, SemanticCompletion::Incomplete);
        assert_eq!(result.error_codes, [2304]);
        assert_eq!(result.diagnostic_fingerprints.len(), 1);
    }
}

#[test]
fn diagnostic_continuation_mutation_changes_its_owner_fingerprint() {
    let first = parse_batch_output(
        "test.ts(1,1): error TS2322: Primary message.\n  Related reason one.\n---TSZ-SEMANTIC-COMPLETION:complete---\n",
        Path::new("/tmp/project"),
        HashMap::new(),
    );
    let second = parse_batch_output(
        "test.ts(1,1): error TS2322: Primary message.\n  Related reason two.\n---TSZ-SEMANTIC-COMPLETION:complete---\n",
        Path::new("/tmp/project"),
        HashMap::new(),
    );

    assert!(!first.crashed && !second.crashed);
    assert_eq!(first.semantic_completion, SemanticCompletion::Complete);
    assert_ne!(
        first.diagnostic_fingerprints,
        second.diagnostic_fingerprints
    );
    assert_eq!(
        first.diagnostic_fingerprints[0].continuations,
        ["  Related reason one."]
    );
}

#[test]
fn swapped_continuations_cannot_move_between_primary_diagnostics() {
    let expected = parse_batch_output(
        "a.ts(1,1): error TS2322: First.\n  First owner.\nb.ts(2,1): error TS2345: Second.\n  Second owner.\n---TSZ-SEMANTIC-COMPLETION:complete---\n",
        Path::new("/tmp/project"),
        HashMap::new(),
    );
    let swapped = parse_batch_output(
        "a.ts(1,1): error TS2322: First.\n  Second owner.\nb.ts(2,1): error TS2345: Second.\n  First owner.\n---TSZ-SEMANTIC-COMPLETION:complete---\n",
        Path::new("/tmp/project"),
        HashMap::new(),
    );

    assert!(!expected.crashed && !swapped.crashed);
    assert_ne!(
        expected.diagnostic_fingerprints,
        swapped.diagnostic_fingerprints
    );
}

#[test]
fn one_vs_two_user_message_spaces_are_not_normalized() {
    let one = parse_batch_output(
        "test.ts(1,1): error TS6053: File 'one space.ts' not found.\n---TSZ-SEMANTIC-COMPLETION:complete---\n",
        Path::new("/tmp/project"),
        HashMap::new(),
    );
    let two = parse_batch_output(
        "test.ts(1,1): error TS6053: File 'one  space.ts' not found.\n---TSZ-SEMANTIC-COMPLETION:complete---\n",
        Path::new("/tmp/project"),
        HashMap::new(),
    );

    assert_ne!(one.diagnostic_fingerprints, two.diagnostic_fingerprints);
    assert_eq!(
        two.diagnostic_fingerprints[0].message_key,
        "File 'one  space.ts' not found."
    );
}

#[test]
fn blank_and_space_only_continuations_are_byte_distinct() {
    let blank = parse_batch_output(
        "test.ts(1,1): error TS2322: Primary.\n\n---TSZ-SEMANTIC-COMPLETION:complete---\n",
        Path::new("/tmp/project"),
        HashMap::new(),
    );
    let one_space = parse_batch_output(
        "test.ts(1,1): error TS2322: Primary.\n \n---TSZ-SEMANTIC-COMPLETION:complete---\n",
        Path::new("/tmp/project"),
        HashMap::new(),
    );
    assert!(!blank.crashed && !one_space.crashed);
    assert_eq!(blank.diagnostic_fingerprints[0].continuations, [""]);
    assert_eq!(one_space.diagnostic_fingerprints[0].continuations, [" "]);
    assert_ne!(
        blank.diagnostic_fingerprints,
        one_space.diagnostic_fingerprints
    );
}

#[test]
fn bare_carriage_return_remains_diagnostic_payload() {
    let bare = parse_tsz_output(
        &process_output(1, b"test.ts(1,1): error TS2322: Primary.\n related\r", &[]),
        Path::new("/tmp/project"),
        HashMap::new(),
    );
    let crlf = parse_tsz_output(
        &process_output(
            1,
            b"test.ts(1,1): error TS2322: Primary.\r\n related\r\n",
            &[],
        ),
        Path::new("/tmp/project"),
        HashMap::new(),
    );

    assert!(!bare.crashed && !crlf.crashed);
    assert_eq!(
        bare.diagnostic_fingerprints[0].continuations,
        [" related\r"]
    );
    assert_eq!(crlf.diagnostic_fingerprints[0].continuations, [" related"]);
    assert_ne!(bare.diagnostic_fingerprints, crlf.diagnostic_fingerprints);
}

#[test]
fn unowned_blank_or_space_only_output_is_not_complete() {
    for prefix in ["\n", " \n"] {
        let fresh = parse_tsz_output(
            &process_output(
                1,
                format!("{prefix}test.ts(1,1): error TS2322: Primary.\n").as_bytes(),
                &[],
            ),
            Path::new("/tmp/project"),
            HashMap::new(),
        );
        assert!(fresh.crashed, "unowned whitespace passed: {prefix:?}");
    }
}

#[test]
fn exact_grouped_diagnostic_blocks_are_complete_and_equal() {
    let output = "test.ts(1,1): error TS2322: Primary.\n  Related detail.\nFound 1 error.\n---TSZ-SEMANTIC-COMPLETION:complete---\n";
    let first = parse_batch_output(output, Path::new("/tmp/project"), HashMap::new());
    let second = parse_batch_output(output, Path::new("/tmp/project"), HashMap::new());

    assert!(!first.crashed);
    assert_eq!(first.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        first.diagnostic_fingerprints,
        second.diagnostic_fingerprints
    );
}

#[test]
fn fresh_exact_grouped_diagnostic_block_is_complete() {
    let output = process_output(
        1,
        b"test.ts(1,1): error TS2322: Primary.\n  Related detail.\nFound 1 error.\n",
        &[],
    );
    let result = parse_tsz_output(&output, Path::new("/tmp/project"), HashMap::new());

    assert!(!result.crashed);
    assert_eq!(result.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(result.error_codes, [2322]);
    assert_eq!(result.ordinary_exit_statuses, [1]);
    assert_eq!(
        result.diagnostic_fingerprints[0].continuations,
        ["  Related detail."]
    );
}

#[test]
fn fresh_diagnostic_with_exit_zero_preserves_the_wrong_exit_for_comparison() {
    let output = process_output(0, b"test.ts(1,1): error TS2322: Primary.\n", &[]);
    let result = parse_tsz_output(&output, Path::new("/tmp/project"), HashMap::new());

    assert!(!result.crashed);
    assert_eq!(result.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(result.error_codes, [2322]);
    assert_eq!(result.ordinary_exit_statuses, [0]);
}

#[test]
fn diagnostics_on_both_process_streams_are_nonclaim_not_elected_order() {
    let diagnostic = b"test.ts(1,1): error TS2304: Cannot find name 'x'.\n";
    let result = parse_tsz_output(
        &process_output(1, diagnostic, diagnostic),
        Path::new("/tmp/project"),
        HashMap::new(),
    );
    assert!(result.crashed);
    assert_eq!(result.semantic_completion, SemanticCompletion::Incomplete);
}

#[test]
fn arbitrary_path_spellings_and_ts5057_payloads_remain_distinct() {
    let root = Path::new("/tmp/owned-project");
    let absolute = parse_batch_output(
        "error TS6053: File '/src/a.ts' not found.\n---TSZ-SEMANTIC-COMPLETION:complete---\n",
        root,
        HashMap::new(),
    );
    let parent = parse_batch_output(
        "error TS6053: File '../src/a.ts' not found.\n---TSZ-SEMANTIC-COMPLETION:complete---\n",
        root,
        HashMap::new(),
    );
    let first_dir = parse_batch_output(
        "error TS5057: Cannot find a tsconfig.json file at the specified directory: 'one'.\n---TSZ-SEMANTIC-COMPLETION:complete---\n",
        root,
        HashMap::new(),
    );
    let second_dir = parse_batch_output(
        "error TS5057: Cannot find a tsconfig.json file at the specified directory: 'two'.\n---TSZ-SEMANTIC-COMPLETION:complete---\n",
        root,
        HashMap::new(),
    );
    assert_ne!(
        absolute.diagnostic_fingerprints,
        parent.diagnostic_fingerprints
    );
    assert_ne!(
        first_dir.diagnostic_fingerprints,
        second_dir.diagnostic_fingerprints
    );
}

#[test]
fn parser_preserves_top_level_diagnostic_order() {
    let result = parse_batch_output(
        "b.ts(2,1): error TS2322: second\na.ts(1,1): error TS2304: first\n---TSZ-SEMANTIC-COMPLETION:complete---\n",
        Path::new("/tmp/project"),
        HashMap::new(),
    );
    assert_eq!(result.error_codes, [2322, 2304]);
    assert_eq!(result.diagnostic_fingerprints[0].file, "b.ts");
    assert_eq!(result.diagnostic_fingerprints[1].file, "a.ts");
}

#[test]
fn relative_backslash_path_is_not_elected_equivalent_to_slash_path() {
    let root = Path::new("/tmp/project");
    let backslash = parse_batch_output(
        "src\\file.ts(1,1): error TS2322: mismatch\n---TSZ-SEMANTIC-COMPLETION:complete---\n",
        root,
        HashMap::new(),
    );
    let slash = parse_batch_output(
        "src/file.ts(1,1): error TS2322: mismatch\n---TSZ-SEMANTIC-COMPLETION:complete---\n",
        root,
        HashMap::new(),
    );
    assert_ne!(
        backslash.diagnostic_fingerprints,
        slash.diagnostic_fingerprints
    );
}
