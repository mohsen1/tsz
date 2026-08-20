//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/bin/tsz/diagnostics_report.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN d9074899871ec440720595288394b867cd256b76fcb7c47f88dabb80400e479a 661 basic_output_contains_required_fields
    #[test]
    fn basic_output_contains_required_fields() {
        let report = basic_report();
        let out = render_diagnostics_report(&report, false);
        assert!(
            out.contains("Files:                         3"),
            "files count"
        );
        assert!(
            out.contains("Lines of Library:              100"),
            "library lines"
        );
        assert!(
            out.contains("Lines of TypeScript:           200"),
            "typescript lines"
        );
        assert!(out.contains("Errors:                        2"), "errors");
        assert!(out.contains("Total time:                    1.23s"), "time");
    }
// TSZ_INLINE_TEST_END d9074899871ec440720595288394b867cd256b76fcb7c47f88dabb80400e479a

// TSZ_INLINE_TEST_BEGIN b03ad4011724431ef692c17169e834186de822bcf10eebe517b3a7e5207794a0 681 basic_output_excludes_extended_fields
    #[test]
    fn basic_output_excludes_extended_fields() {
        let report = basic_report();
        let out = render_diagnostics_report(&report, false);
        assert!(
            !out.contains("Request cache"),
            "no cache stats in basic mode"
        );
        assert!(!out.contains("Memory used"), "no memory in basic mode");
    }
// TSZ_INLINE_TEST_END b03ad4011724431ef692c17169e834186de822bcf10eebe517b3a7e5207794a0

// TSZ_INLINE_TEST_BEGIN aeda3dc792d69d8cc833506c794ee8132eca5d7b8c5507f4e2392ac2701f02ec 692 extended_output_includes_cache_stats
    #[test]
    fn extended_output_includes_cache_stats() {
        let report = DiagnosticsReport {
            files_count: 1,
            total_secs: 0.5,
            request_cache_hits: 80,
            request_cache_misses: 20,
            has_query_cache: true,
            subtype_entries: 10,
            subtype_hits: 8,
            subtype_misses: 2,
            assignability_entries: 5,
            assignability_hits: 3,
            assignability_misses: 2,
            memory_used_kb: 4096,
            ..DiagnosticsReport::default()
        };
        let out = render_diagnostics_report(&report, true);
        assert!(
            out.contains("Request cache hit rate:        80.0%"),
            "hit rate: {out}"
        );
        assert!(out.contains("Subtype cache:"), "subtype cache");
        assert!(
            out.contains("Memory used:                   4096K"),
            "memory"
        );
    }
// TSZ_INLINE_TEST_END aeda3dc792d69d8cc833506c794ee8132eca5d7b8c5507f4e2392ac2701f02ec

// TSZ_INLINE_TEST_BEGIN 9038bdff13b651633eb5b4a61bae8a8b2cca8f2c843b9e2754e8aa2cd020f20f 721 phase_timings_only_shown_when_present
    #[test]
    fn phase_timings_only_shown_when_present() {
        let without = DiagnosticsReport {
            has_phase_timings: false,
            total_secs: 0.1,
            ..DiagnosticsReport::default()
        };
        let with_timings = DiagnosticsReport {
            has_phase_timings: true,
            io_read_secs: 0.05,
            parse_bind_secs: 0.03,
            check_secs: 0.02,
            emit_secs: 0.01,
            total_secs: 0.1,
            ..DiagnosticsReport::default()
        };
        let out_without = render_diagnostics_report(&without, false);
        let out_with = render_diagnostics_report(&with_timings, false);
        assert!(
            !out_without.contains("I/O Read:"),
            "no timings without flag"
        );
        assert!(
            out_with.contains("I/O Read:                      0.05s"),
            "has timings"
        );
    }
// TSZ_INLINE_TEST_END 9038bdff13b651633eb5b4a61bae8a8b2cca8f2c843b9e2754e8aa2cd020f20f

// TSZ_INLINE_TEST_BEGIN a42a016be270e5191a6f8d4ad6027433e83b22033c2b1ec9f2c6a2a17e96f6c7 749 collect_file_lines_categorizes_correctly
    #[test]
    fn collect_file_lines_categorizes_correctly() {
        // Build a temp dir with files of different types to verify categorization.
        let dir = std::env::temp_dir().join("tsz_test_file_lines");
        let _ = std::fs::create_dir_all(&dir);

        let lib_d_ts = dir.join("lib.es5.d.ts");
        let user_ts = dir.join("user.ts");
        let js_file = dir.join("helper.js");

        std::fs::write(&lib_d_ts, "line1\nline2\nline3\n").unwrap();
        std::fs::write(&user_ts, "line1\nline2\n").unwrap();
        std::fs::write(&js_file, "line1\n").unwrap();

        let stats = collect_file_lines(&[lib_d_ts, user_ts, js_file]);

        assert_eq!(stats.library, 3, "lib.d.ts lines");
        assert_eq!(stats.typescript, 2, "ts lines");
        assert_eq!(stats.javascript, 1, "js lines");
        assert_eq!(stats.definitions, 0);
        assert_eq!(stats.json, 0);
        assert_eq!(stats.other, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }
// TSZ_INLINE_TEST_END a42a016be270e5191a6f8d4ad6027433e83b22033c2b1ec9f2c6a2a17e96f6c7

// TSZ_INLINE_TEST_BEGIN f67726793b53a53fe7c33986e28dc43fc72ebb195d9ddde7b01bc11d1f3ee991 775 basic_golden_full_output
    #[test]
    fn basic_golden_full_output() {
        let report = DiagnosticsReport {
            files_count: 5,
            lines: FileLinesStats {
                library: 1000,
                definitions: 200,
                typescript: 300,
                javascript: 50,
                json: 10,
                other: 0,
            },
            error_count: 3,
            has_phase_timings: true,
            io_read_secs: 0.10,
            parse_bind_secs: 0.20,
            check_secs: 0.30,
            emit_secs: 0.05,
            total_secs: 0.65,
            ..DiagnosticsReport::default()
        };
        let out = render_diagnostics_report(&report, false);
        let expected = "\n\
Files:                         5\n\
Lines of Library:              1000\n\
Lines of Definitions:          200\n\
Lines of TypeScript:           300\n\
Lines of JavaScript:           50\n\
Lines of JSON:                 10\n\
Lines of Other:                0\n\
Errors:                        3\n\
I/O Read:                      0.10s\n\
Parse & Bind:                  0.20s\n\
Check:                         0.30s\n\
Emit:                          0.05s\n\
Total time:                    0.65s\n";
        assert_eq!(out, expected, "basic golden mismatch:\n{out}");
    }
// TSZ_INLINE_TEST_END f67726793b53a53fe7c33986e28dc43fc72ebb195d9ddde7b01bc11d1f3ee991

// TSZ_INLINE_TEST_BEGIN e10b4ceabc2012e9175a5d2e1d9177e9c5274a9b218f6968905c889c4dc93727 814 extended_golden_full_output
    #[test]
    fn extended_golden_full_output() {
        let report = DiagnosticsReport {
            files_count: 2,
            lines: FileLinesStats {
                library: 500,
                definitions: 0,
                typescript: 100,
                javascript: 0,
                json: 0,
                other: 0,
            },
            error_count: 0,
            has_phase_timings: false,
            total_secs: 1.00,
            // Extended fields
            memory_used_kb: 8192,
            emitted_files_count: 1,
            total_diagnostics: 0,
            request_cache_hits: 90,
            request_cache_misses: 10,
            contextual_cache_bypasses: 2,
            clear_type_cache_recursive_calls: 1,
            property_access_cache_hits: 45,
            property_access_cache_lookups: 50,
            interned_types_count: 0,
            interner_kb: 0.0,
            has_query_cache: false,
            has_def_store: false,
            has_residency: false,
            has_module_deps: false,
            perf_counter_dump: String::new(),
            ..DiagnosticsReport::default()
        };
        let out = render_diagnostics_report(&report, true);
        let expected = "\n\
Files:                         2\n\
Lines of Library:              500\n\
Lines of Definitions:          0\n\
Lines of TypeScript:           100\n\
Lines of JavaScript:           0\n\
Lines of JSON:                 0\n\
Lines of Other:                0\n\
Errors:                        0\n\
Total time:                    1.00s\n\
Emitted files:                 1\n\
Total diagnostics:             0\n\
Request cache hits:            90\n\
Request cache misses:          10\n\
Request cache hit rate:        90.0%\n\
Contextual cache bypasses:     2\n\
clear_type_cache_recursive:    1\n\
Access request-cache hit rate: 90.0% (45/50)\n\
Memory used:                   8192K\n";
        assert_eq!(out, expected, "extended golden mismatch:\n{out}");
    }
// TSZ_INLINE_TEST_END e10b4ceabc2012e9175a5d2e1d9177e9c5274a9b218f6968905c889c4dc93727
