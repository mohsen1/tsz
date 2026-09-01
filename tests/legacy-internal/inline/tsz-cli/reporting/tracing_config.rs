//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/reporting/tracing_config.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 2206e1ce154c8727ff224a546954a3a9d8fcf2f7e586b4c334fa0c3c28fd1b8e 178 log_format_from_env_defaults_to_text_and_normalizes_case
    #[test]
    fn log_format_from_env_defaults_to_text_and_normalizes_case() {
        let _guard = env_lock().lock().unwrap();
        let mut env = TestEnv::new();

        env.unset("TSZ_LOG_FORMAT");
        assert_eq!(LogFormat::from_env(), LogFormat::Text);

        env.set("TSZ_LOG_FORMAT", "TrEe");
        assert_eq!(LogFormat::from_env(), LogFormat::Tree);

        env.set("TSZ_LOG_FORMAT", "JSON");
        assert_eq!(LogFormat::from_env(), LogFormat::Json);
    }
// TSZ_INLINE_TEST_END 2206e1ce154c8727ff224a546954a3a9d8fcf2f7e586b4c334fa0c3c28fd1b8e

// TSZ_INLINE_TEST_BEGIN 4f85bf2827bf7b879d75678ec7e4e2def4ccac2ad0d6bf1387174c7884df5a2f 193 build_filter_prefers_tsz_log_over_rust_log
    #[test]
    fn build_filter_prefers_tsz_log_over_rust_log() {
        let _guard = env_lock().lock().unwrap();
        let mut env = TestEnv::new();

        env.set("TSZ_LOG", "tsz_checker=trace");
        env.set("RUST_LOG", "tsz_solver=debug");

        let filter = build_filter();
        let expected = EnvFilter::builder().parse_lossy("tsz_checker=trace");

        assert_eq!(filter.to_string(), expected.to_string());
    }
// TSZ_INLINE_TEST_END 4f85bf2827bf7b879d75678ec7e4e2def4ccac2ad0d6bf1387174c7884df5a2f

// TSZ_INLINE_TEST_BEGIN 71824fb5ca8301e41bbbd1aca8bd41e473751601cd12e908de28301fdf99018f 207 build_filter_falls_back_to_rust_log_when_tsz_log_is_missing
    #[test]
    fn build_filter_falls_back_to_rust_log_when_tsz_log_is_missing() {
        let _guard = env_lock().lock().unwrap();
        let mut env = TestEnv::new();

        env.unset("TSZ_LOG");
        env.set("RUST_LOG", "tsz_solver=debug");

        let filter = build_filter();
        let expected = EnvFilter::from_default_env();

        assert_eq!(filter.to_string(), expected.to_string());
    }
// TSZ_INLINE_TEST_END 71824fb5ca8301e41bbbd1aca8bd41e473751601cd12e908de28301fdf99018f
