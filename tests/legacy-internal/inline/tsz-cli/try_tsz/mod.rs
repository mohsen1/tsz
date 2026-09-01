//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/try_tsz/mod.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 7cd41516f8b36de6d88853e54db42f9ceac4a1c20e4d3691cb1134e7335fd51b 1613 discover_nearest_tsconfig
    #[test]
    fn discover_nearest_tsconfig() {
        let temp = TempDir::new();
        write_file(&temp.path.join("tsconfig.json"), "{}");
        fs::create_dir_all(temp.path.join("src/nested")).expect("nested dir");

        let configs = discover_configs(&temp.path.join("src/nested"), None, false)
            .expect("config should be discovered");

        assert_eq!(configs, vec![temp.path.join("tsconfig.json")]);
    }
// TSZ_INLINE_TEST_END 7cd41516f8b36de6d88853e54db42f9ceac4a1c20e4d3691cb1134e7335fd51b

// TSZ_INLINE_TEST_BEGIN c749731b2497c55973faad73b53ba8a68c19a8337fdb51c98eba14b2e02ece62 1625 explicit_project_directory_resolves_tsconfig
    #[test]
    fn explicit_project_directory_resolves_tsconfig() {
        let temp = TempDir::new();
        write_file(&temp.path.join("pkg/tsconfig.json"), "{}");

        let configs = discover_configs(&temp.path, Some(Path::new("pkg")), false)
            .expect("project dir should resolve");

        assert_eq!(configs, vec![temp.path.join("pkg/tsconfig.json")]);
    }
// TSZ_INLINE_TEST_END c749731b2497c55973faad73b53ba8a68c19a8337fdb51c98eba14b2e02ece62

// TSZ_INLINE_TEST_BEGIN 94005c2e02aa1c82e4f77f672d4913721dd4ac2d833641ae0811b79f6727e8fe 1636 all_skips_generated_directories
    #[test]
    fn all_skips_generated_directories() {
        let temp = TempDir::new();
        write_file(&temp.path.join("packages/a/tsconfig.json"), "{}");
        write_file(&temp.path.join("node_modules/pkg/tsconfig.json"), "{}");

        let configs = discover_configs(&temp.path, None, true).expect("all should find configs");

        assert_eq!(configs, vec![temp.path.join("packages/a/tsconfig.json")]);
    }
// TSZ_INLINE_TEST_END 94005c2e02aa1c82e4f77f672d4913721dd4ac2d833641ae0811b79f6727e8fe

// TSZ_INLINE_TEST_BEGIN 57d4aa06139361288202cf7472d8c1b504a0f6e5baec471164389c2c10522062 1647 typescript_oracle_preflight_accepts_hoisted_workspace_package
    #[test]
    fn typescript_oracle_preflight_accepts_hoisted_workspace_package() {
        let temp = TempDir::new();
        let package_dir = temp.path.join("packages/foo");
        let config = package_dir.join("tsconfig.json");
        write_file(&config, "{}");
        write_file(&temp.path.join(local_typescript_package_json_path()), "{}");

        ensure_typescript_oracle(&package_dir, &config)
            .expect("hoisted workspace TypeScript should satisfy preflight");
    }
// TSZ_INLINE_TEST_END 57d4aa06139361288202cf7472d8c1b504a0f6e5baec471164389c2c10522062

// TSZ_INLINE_TEST_BEGIN 407ac6e6de223f791a2da12c92ac919bd6e86fccd99483cb5b2ba25bef3d5a1f 1659 typescript_oracle_preflight_rejects_missing_tsc
    #[test]
    fn typescript_oracle_preflight_rejects_missing_tsc() {
        let temp = TempDir::new();
        let package_dir = temp.path.join("packages/foo");
        let config = package_dir.join("tsconfig.json");
        write_file(&config, "{}");

        let error = ensure_typescript_oracle(&package_dir, &config)
            .expect_err("missing local TypeScript should be rejected")
            .to_string();

        assert!(error.contains("TypeScript 7.0.2 or newer"));
        assert!(error.contains("node_modules/typescript/package.json"));
    }
// TSZ_INLINE_TEST_END 407ac6e6de223f791a2da12c92ac919bd6e86fccd99483cb5b2ba25bef3d5a1f

// TSZ_INLINE_TEST_BEGIN 681d4e38f11eb1b10f283d491f9e8d7d3565b859ace766187d4b872642f9c315 1674 tsz_timeout_env_value_must_be_positive_seconds
    #[test]
    fn tsz_timeout_env_value_must_be_positive_seconds() {
        assert_eq!(
            tsz_timeout_from_env_value(None),
            Duration::from_secs(DEFAULT_TSZ_TIMEOUT_SECS)
        );
        assert_eq!(
            tsz_timeout_from_env_value(Some("45")),
            Duration::from_secs(45)
        );
        assert_eq!(
            tsz_timeout_from_env_value(Some("0")),
            Duration::from_secs(DEFAULT_TSZ_TIMEOUT_SECS)
        );
        assert_eq!(
            tsz_timeout_from_env_value(Some("nope")),
            Duration::from_secs(DEFAULT_TSZ_TIMEOUT_SECS)
        );
    }
// TSZ_INLINE_TEST_END 681d4e38f11eb1b10f283d491f9e8d7d3565b859ace766187d4b872642f9c315

// TSZ_INLINE_TEST_BEGIN 8918dd4e13682d574e50a11cd4094aa405138d8cde437eab2ba415740dc1afe1 1694 tsconfig_context_collects_local_extends_and_references
    #[test]
    fn tsconfig_context_collects_local_extends_and_references() {
        let temp = TempDir::new();
        write_file(
            &temp.path.join("tsconfig.base.json"),
            "{ // jsonc is accepted\n  \"compilerOptions\": { \"strict\": true }\n}\n",
        );
        write_file(
            &temp.path.join("packages/shared/tsconfig.json"),
            "{ \"compilerOptions\": { \"composite\": true } }\n",
        );
        write_file(
            &temp.path.join("packages/app/tsconfig.json"),
            "{\n  \"extends\": \"../../tsconfig.base.json\",\n  \"references\": [{ \"path\": \"../shared\" }]\n}\n",
        );

        let snapshots =
            collect_tsconfig_context(&temp.path, &report_for_config("packages/app/tsconfig.json"));
        let labels = snapshots
            .iter()
            .map(|snapshot| snapshot.label.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec![
                "packages/app/tsconfig.json",
                "tsconfig.base.json",
                "packages/shared/tsconfig.json"
            ]
        );
        assert!(snapshots.iter().all(|snapshot| !snapshot.truncated));
    }
// TSZ_INLINE_TEST_END 8918dd4e13682d574e50a11cd4094aa405138d8cde437eab2ba415740dc1afe1

// TSZ_INLINE_TEST_BEGIN 91ac458b2d399e9b3121c7e30ea5a6d458e71965d3e92bd060dfad687696a65c 1728 diagnostic_diff_detects_extra_missing_and_order
    #[test]
    fn diagnostic_diff_detects_extra_missing_and_order() {
        let first = diag(2322, "a.ts", "A");
        let second = diag(2339, "b.ts", "B");

        let diff = diff_diagnostics(
            std::slice::from_ref(&first),
            &[first.clone(), second.clone()],
        );
        assert_eq!(diff.extra_tsz, vec![second.clone()]);
        assert!(diff.missing_tsc.is_empty());
        assert_eq!(diff.order_mismatches, 0);

        let diff = diff_diagnostics(&[first.clone(), second.clone()], &[second, first]);
        assert!(diff.extra_tsz.is_empty());
        assert!(diff.missing_tsc.is_empty());
        assert_eq!(diff.order_mismatches, 2);
    }
// TSZ_INLINE_TEST_END 91ac458b2d399e9b3121c7e30ea5a6d458e71965d3e92bd060dfad687696a65c

// TSZ_INLINE_TEST_BEGIN 25b2db519ba1aa97665d85f85cdd558b987ae30509cf0a7fae7e961f31a7bd36 1747 config_deprecation_diagnostics_ignore_location_for_try_tsz_diff
    #[test]
    fn config_deprecation_diagnostics_ignore_location_for_try_tsz_diff() {
        let message = concat!(
            "Option 'moduleResolution=node10' is deprecated and will stop functioning in TypeScript 7.0.",
            " Specify compilerOption '\"ignoreDeprecations\": \"6.0\"' to silence this error.",
            "\n  Visit https://aka.ms/ts6 for migration information.",
        );
        let mut tsc = ComparableDiagnostic {
            file: None,
            start: None,
            length: None,
            line: None,
            column: None,
            code: 5107,
            category: "error".to_string(),
            message: message.to_string(),
        };
        let mut tsz = diag(5107, "tsconfig.json", message);

        normalize_config_deprecation_location(&mut tsc);
        normalize_config_deprecation_location(&mut tsz);
        let diff = diff_diagnostics(&[tsc], &[tsz]);

        assert!(diff.extra_tsz.is_empty());
        assert!(diff.missing_tsc.is_empty());
        assert_eq!(diff.order_mismatches, 0);
    }
// TSZ_INLINE_TEST_END 25b2db519ba1aa97665d85f85cdd558b987ae30509cf0a7fae7e961f31a7bd36

// TSZ_INLINE_TEST_BEGIN 9f9a22b544972cac61f3c81dfd0b795841ef8852f6934fd5057cbe796058ff56 1775 line_window_returns_context_around_offset
    #[test]
    fn line_window_returns_context_around_offset() {
        let source = "one\ntwo\nthree\nfour\nfive\nsix\nseven\n";
        let snippet = enclosing_line_window(source, 14);

        assert!(snippet.contains("three"));
        assert!(snippet.contains("five"));
    }
// TSZ_INLINE_TEST_END 9f9a22b544972cac61f3c81dfd0b795841ef8852f6934fd5057cbe796058ff56
