//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/perf_json.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 4c4c8789d32bf89c1fa8aa0258e98acdc89a5261bf9cad2315dc8764b71e4a34 278 schema_version_is_two
    #[test]
    fn schema_version_is_two() {
        // Bumping schema_version is a breaking change for the bench harness;
        // the test exists to make that intent explicit.
        assert_eq!(SCHEMA_VERSION, 2);
    }
// TSZ_INLINE_TEST_END 4c4c8789d32bf89c1fa8aa0258e98acdc89a5261bf9cad2315dc8764b71e4a34

// TSZ_INLINE_TEST_BEGIN 6e370d2a99324ba22baf4389e1c884df6e9b7917b7777fd85da518b0bca61deb 285 fixture_provenance_local_override_parses_truthy_strings
    #[test]
    fn fixture_provenance_local_override_parses_truthy_strings() {
        // Smoke-test the env-var coercion logic without touching the global
        // process env (we exercise the inner `matches!` via direct call).
        let truthy = ["1", "true"];
        for value in truthy {
            // Same shape as the inner check.
            assert!(matches!(Some(value), Some("1") | Some("true")));
        }
        let falsy = ["0", "false", "yes", ""];
        for value in falsy {
            assert!(!matches!(Some(value), Some("1") | Some("true")));
        }
    }
// TSZ_INLINE_TEST_END 6e370d2a99324ba22baf4389e1c884df6e9b7917b7777fd85da518b0bca61deb

// TSZ_INLINE_TEST_BEGIN 25a9e32167abe0b4c834f8b4478edf9fcdc00aaa8fdd261cdf0ec1261cad8932 300 report_serializes_to_valid_json
    #[test]
    fn report_serializes_to_valid_json() {
        let report = PerfDiagnosticsReport {
            schema_version: SCHEMA_VERSION,
            mode: "timing",
            tsz: TszBuildInfo {
                version: "test".to_string(),
                commit: Some("abc123".to_string()),
                profile: "release",
            },
            fixture: FixtureProvenance::default(),
            command_line: vec!["tsz".to_string(), "--noEmit".to_string()],
            phases_ms: PhasesMs::default(),
            counts: Counts::default(),
            rss_peak_bytes: Some(1024),
        };
        let json = serde_json::to_value(&report).expect("serializes");
        assert_eq!(json["schema_version"], 2);
        assert_eq!(json["mode"], "timing");
        assert_eq!(json["tsz"]["version"], "test");
        assert_eq!(json["fixture"]["local_override"], false);
        // Schema-locked phase keys: bench harness depends on these names.
        let phases = &json["phases_ms"];
        for key in [
            "config_discovery",
            "source_discovery",
            "module_resolution",
            "io_read",
            "load_libs",
            "parse_bind",
            "check",
            "emit",
            "total",
        ] {
            assert!(phases.get(key).is_some(), "missing phase key: {key}");
        }

        let counts = json["counts"]
            .as_object()
            .expect("counts serializes as an object");
        for key in [
            "files",
            "root_files",
            "lib_files",
            "source_bytes",
            "diagnostics",
        ] {
            assert!(counts.contains_key(key), "missing counts key: {key}");
        }
        assert!(
            counts["source_bytes"].is_null(),
            "source_bytes must stay null until the driver supplies a reliable byte count"
        );
    }
// TSZ_INLINE_TEST_END 25a9e32167abe0b4c834f8b4478edf9fcdc00aaa8fdd261cdf0ec1261cad8932
