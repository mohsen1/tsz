//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/driver/plan.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN b122facff3e6036220ab55c96bca3cdd45a4390842244d78408ee4267839d468 1399 is_valid_jsx_factory_expression_accepts_simple_identifier
    #[test]
    fn is_valid_jsx_factory_expression_accepts_simple_identifier() {
        assert!(is_valid_jsx_factory_expression("h"));
        assert!(is_valid_jsx_factory_expression("React"));
        assert!(is_valid_jsx_factory_expression("_factory"));
        assert!(is_valid_jsx_factory_expression("$createElement"));
    }
// TSZ_INLINE_TEST_END b122facff3e6036220ab55c96bca3cdd45a4390842244d78408ee4267839d468

// TSZ_INLINE_TEST_BEGIN 6d3522675b06ca2ef37accf3196a186e9176899fb94bc4a0cd654a9a28937735 1407 is_valid_jsx_factory_expression_accepts_dotted_chain
    #[test]
    fn is_valid_jsx_factory_expression_accepts_dotted_chain() {
        assert!(is_valid_jsx_factory_expression("React.createElement"));
        assert!(is_valid_jsx_factory_expression("a.b.c"));
    }
// TSZ_INLINE_TEST_END 6d3522675b06ca2ef37accf3196a186e9176899fb94bc4a0cd654a9a28937735

// TSZ_INLINE_TEST_BEGIN 480b990604150b0c91741895ddcaf56e85893f367fe235a2b549d5cf52ac6376 1413 is_valid_jsx_factory_expression_rejects_invalid
    #[test]
    fn is_valid_jsx_factory_expression_rejects_invalid() {
        assert!(!is_valid_jsx_factory_expression(""));
        assert!(!is_valid_jsx_factory_expression("234"));
        assert!(!is_valid_jsx_factory_expression("my-lib.create"));
        assert!(!is_valid_jsx_factory_expression(".leading"));
        assert!(!is_valid_jsx_factory_expression("trailing."));
    }
// TSZ_INLINE_TEST_END 480b990604150b0c91741895ddcaf56e85893f367fe235a2b549d5cf52ac6376

// TSZ_INLINE_TEST_BEGIN ebb084993848eb40b5fe3b85d9be2fc3dc4fbe1d455868fd940066ae78717b21 1422 emit_common_source_directory_single_file_uses_file_directory
    #[test]
    fn emit_common_source_directory_single_file_uses_file_directory() {
        // `tsz src/a.ts --outDir out` (no tsconfig, no rootDir): tsc lays output
        // relative to the file's directory, not the cwd, so the root is src.
        let files = vec![PathBuf::from("/proj/src/a.ts")];
        let got = emit_common_source_directory(files, Path::new("/proj"), Path::new("/proj"));
        assert_eq!(got, Some(PathBuf::from("/proj/src")));
    }
// TSZ_INLINE_TEST_END ebb084993848eb40b5fe3b85d9be2fc3dc4fbe1d455868fd940066ae78717b21

// TSZ_INLINE_TEST_BEGIN 5cb3e50601421d2f49daf0dfc9f21acee1fb105fcef00161094b056791512334 1431 emit_common_source_directory_multi_file_uses_longest_common_directory
    #[test]
    fn emit_common_source_directory_multi_file_uses_longest_common_directory() {
        let files = vec![
            PathBuf::from("/proj/src/a.ts"),
            PathBuf::from("/proj/src/sub/c.ts"),
        ];
        let got = emit_common_source_directory(files, Path::new("/proj"), Path::new("/proj"));
        assert_eq!(got, Some(PathBuf::from("/proj/src")));
    }
// TSZ_INLINE_TEST_END 5cb3e50601421d2f49daf0dfc9f21acee1fb105fcef00161094b056791512334

// TSZ_INLINE_TEST_BEGIN e1e6cd9d654ea53f1db8292a32e548e5d8ac74eb0d2edcc9fe466808917d700b 1441 emit_common_source_directory_none_when_common_equals_base_dir
    #[test]
    fn emit_common_source_directory_none_when_common_equals_base_dir() {
        // Common directory coincides with base_dir: the base_dir fallback already
        // produces the right layout, so there is nothing to override.
        let files = vec![PathBuf::from("/proj/a.ts"), PathBuf::from("/proj/b.ts")];
        let got = emit_common_source_directory(files, Path::new("/proj"), Path::new("/proj"));
        assert_eq!(got, None);
    }
// TSZ_INLINE_TEST_END e1e6cd9d654ea53f1db8292a32e548e5d8ac74eb0d2edcc9fe466808917d700b

// TSZ_INLINE_TEST_BEGIN cd7093afb7d990230e9bdff7b37c9d39e0d7eecd6b09832de22533d72228e82a 1450 emit_common_source_directory_excludes_node_modules_and_declaration_sources
    #[test]
    fn emit_common_source_directory_excludes_node_modules_and_declaration_sources() {
        // node_modules and `.d.ts` sources must not drag the common directory up.
        let files = vec![
            PathBuf::from("/proj/src/a.ts"),
            PathBuf::from("/proj/src/sub/c.ts"),
            PathBuf::from("/proj/node_modules/dep/index.ts"),
            PathBuf::from("/proj/types/global.d.ts"),
        ];
        let got = emit_common_source_directory(files, Path::new("/proj"), Path::new("/proj"));
        assert_eq!(got, Some(PathBuf::from("/proj/src")));
    }
// TSZ_INLINE_TEST_END cd7093afb7d990230e9bdff7b37c9d39e0d7eecd6b09832de22533d72228e82a

// TSZ_INLINE_TEST_BEGIN ee3a5b07467d5dd2290023a179196ab245162fbdfe5ccfbee19d9ce687fca6bd 1463 cli_ignore_deprecations_6_0_detected
    #[test]
    fn cli_ignore_deprecations_6_0_detected() {
        let args = CliArgs::try_parse_from(["tsz", "--ignoreDeprecations", "6.0"]).unwrap();
        assert!(cli_ignore_deprecations_silences_6_0(&args));
    }
// TSZ_INLINE_TEST_END ee3a5b07467d5dd2290023a179196ab245162fbdfe5ccfbee19d9ce687fca6bd

// TSZ_INLINE_TEST_BEGIN 2fa8baff22e8a3062be1076d0662bcadc7f9a727b3462f4afdd1705843da7f81 1469 cli_ignore_deprecations_5_0_not_6_0
    #[test]
    fn cli_ignore_deprecations_5_0_not_6_0() {
        let args = CliArgs::try_parse_from(["tsz", "--ignoreDeprecations", "5.0"]).unwrap();
        assert!(!cli_ignore_deprecations_silences_6_0(&args));
    }
// TSZ_INLINE_TEST_END 2fa8baff22e8a3062be1076d0662bcadc7f9a727b3462f4afdd1705843da7f81

// TSZ_INLINE_TEST_BEGIN 86eebfbf3266fc72318fcb5937ecc99fa16ee825d56e060208f820535ff3ab0f 1475 cli_ignore_deprecations_7_0_not_6_0
    #[test]
    fn cli_ignore_deprecations_7_0_not_6_0() {
        let args = CliArgs::try_parse_from(["tsz", "--ignoreDeprecations", "7.0"]).unwrap();
        assert!(!cli_ignore_deprecations_silences_6_0(&args));
    }
// TSZ_INLINE_TEST_END 86eebfbf3266fc72318fcb5937ecc99fa16ee825d56e060208f820535ff3ab0f

// TSZ_INLINE_TEST_BEGIN ebe4250e36708c6f221b3d9e0739ab514c1408c52325b64dbf4c0c5f55a17b30 1481 cli_ts7_removed_options_use_shared_ts5102_ts5108_policy
    #[test]
    fn cli_ts7_removed_options_use_shared_ts5102_ts5108_policy() {
        let cases: &[(&[&str], u32, &str)] = &[
            (&["--target", "es5"], 5108, "target=ES5"),
            (
                &["--moduleResolution", "node"],
                5108,
                "moduleResolution=node10",
            ),
            (&["--module", "amd"], 5108, "module=AMD"),
            (&["--alwaysStrict", "false"], 5108, "alwaysStrict=false"),
            (
                &["--allowSyntheticDefaultImports", "false"],
                5108,
                "allowSyntheticDefaultImports=false",
            ),
            (
                &["--__explicitly-disabled-bool-flag=esModuleInterop"],
                5108,
                "esModuleInterop=false",
            ),
            (&["--baseUrl", "."], 5102, "baseUrl"),
            (&["--outFile", "bundle.js"], 5102, "outFile"),
            (
                &["--__explicitly-disabled-bool-flag=downlevelIteration"],
                5102,
                "downlevelIteration",
            ),
        ];

        for (options, code, message_fragment) in cases {
            let args =
                CliArgs::try_parse_from(std::iter::once("tsz").chain(options.iter().copied()))
                    .unwrap();
            let diagnostics = validate_cli_compiler_option_diagnostics(&args, None).unwrap();
            assert!(
                diagnostics.iter().any(|diagnostic| {
                    diagnostic.code == *code && diagnostic.message_text.contains(message_fragment)
                }),
                "{options:?} should emit TS{code} for {message_fragment}, got {diagnostics:?}"
            );
        }
    }
// TSZ_INLINE_TEST_END ebe4250e36708c6f221b3d9e0739ab514c1408c52325b64dbf4c0c5f55a17b30

// TSZ_INLINE_TEST_BEGIN f47a894e7f394b1b6d6af45742d20085e89eb46665c81b5d96a45b18151eb73d 1525 cli_ts7_unparsed_legacy_enum_values_use_ts6046
    #[test]
    fn cli_ts7_unparsed_legacy_enum_values_use_ts6046() {
        for options in [["--target", "es3"], ["--module", "none"]] {
            let args =
                CliArgs::try_parse_from(std::iter::once("tsz").chain(options.iter().copied()))
                    .unwrap();
            let diagnostics = validate_cli_compiler_option_diagnostics(&args, None).unwrap();
            assert!(
                diagnostics.iter().any(|diagnostic| diagnostic.code == 6046),
                "{options:?} should emit TS6046, got {diagnostics:?}"
            );
            assert!(
                diagnostics.iter().all(|diagnostic| diagnostic.code != 5108),
                "{options:?} must not emit TS5108, got {diagnostics:?}"
            );
        }
    }
// TSZ_INLINE_TEST_END f47a894e7f394b1b6d6af45742d20085e89eb46665c81b5d96a45b18151eb73d

// TSZ_INLINE_TEST_BEGIN 83baa5232a2303f6b51d98b0bfdf023456ec6c2b6dee2a678d3cd4fca21af953 1543 ordered_direct_cli_parse_diagnostics_follow_side_channel
    #[test]
    fn ordered_direct_cli_parse_diagnostics_follow_side_channel() {
        let diagnostics_for = |order: [&str; 2]| {
            let mut argv = vec![
                "tsz".to_string(),
                "--target".to_string(),
                "es3".to_string(),
                "--keyofStringsOnly".to_string(),
            ];
            argv.extend(
                order
                    .into_iter()
                    .map(|name| format!("--__direct-cli-option-order={name}")),
            );
            let args = CliArgs::try_parse_from(argv).unwrap();
            ordered_direct_cli_parse_diagnostics(&args)
                .unwrap()
                .into_iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>()
        };

        assert_eq!(
            diagnostics_for(["target", "keyofStringsOnly"]),
            [6046, 5023]
        );
        assert_eq!(
            diagnostics_for(["keyofStringsOnly", "target"]),
            [5023, 6046]
        );
    }
// TSZ_INLINE_TEST_END 83baa5232a2303f6b51d98b0bfdf023456ec6c2b6dee2a678d3cd4fca21af953

// TSZ_INLINE_TEST_BEGIN 9ff451bf308679b1432085833d923c2a3357a001ecf9f303faf66b011492f6a0 1575 apply_cli_overrides_no_check_sets_option
    #[test]
    fn apply_cli_overrides_no_check_sets_option() {
        let mut options = ResolvedCompilerOptions::default();
        let args = CliArgs::try_parse_from(["tsz", "--noCheck"]).unwrap();
        apply_cli_overrides(&mut options, &args).unwrap();
        assert!(options.no_check);
    }
// TSZ_INLINE_TEST_END 9ff451bf308679b1432085833d923c2a3357a001ecf9f303faf66b011492f6a0

// TSZ_INLINE_TEST_BEGIN 66fa9e847e32f062b86fe8cd33fa21d1246dcf64b51a6611ec9b5e14d3a1507c 1583 apply_cli_overrides_types_versions_compiler_version_sets_option
    #[test]
    fn apply_cli_overrides_types_versions_compiler_version_sets_option() {
        let mut options = ResolvedCompilerOptions::default();
        let args =
            CliArgs::try_parse_from(["tsz", "--typesVersionsCompilerVersion", "5.6.1"]).unwrap();
        apply_cli_overrides(&mut options, &args).unwrap();
        assert_eq!(
            options.types_versions_compiler_version.as_deref(),
            Some("5.6.1")
        );
    }
// TSZ_INLINE_TEST_END 66fa9e847e32f062b86fe8cd33fa21d1246dcf64b51a6611ec9b5e14d3a1507c

// TSZ_INLINE_TEST_BEGIN 745c8d04aebd20fd1c18ee252f6f5de08a56bea72be6e97bf0d5d05ef1646b65 1595 apply_cli_overrides_types_versions_compiler_version_uses_env_fallback
    #[test]
    fn apply_cli_overrides_types_versions_compiler_version_uses_env_fallback() {
        let mut options = ResolvedCompilerOptions::default();
        let args = CliArgs::try_parse_from(["tsz"]).unwrap();
        crate::driver::with_types_versions_env(Some(" 5.5.4 "), || {
            apply_cli_overrides(&mut options, &args).unwrap();
        });
        assert_eq!(
            options.types_versions_compiler_version.as_deref(),
            Some("5.5.4")
        );
    }
// TSZ_INLINE_TEST_END 745c8d04aebd20fd1c18ee252f6f5de08a56bea72be6e97bf0d5d05ef1646b65

// TSZ_INLINE_TEST_BEGIN 0a59854cad8b5b931c1b5357e2ab62cf433e70dd64be6e5a34808db42d8e91e5 1608 apply_cli_overrides_strict_expands_flags
    #[test]
    fn apply_cli_overrides_strict_expands_flags() {
        let mut options = ResolvedCompilerOptions::default();
        let args = CliArgs::try_parse_from(["tsz", "--strict"]).unwrap();
        apply_cli_overrides(&mut options, &args).unwrap();
        assert!(options.checker.strict_null_checks);
        assert!(options.checker.no_implicit_any);
        assert!(options.checker.strict_function_types);
    }
// TSZ_INLINE_TEST_END 0a59854cad8b5b931c1b5357e2ab62cf433e70dd64be6e5a34808db42d8e91e5

// TSZ_INLINE_TEST_BEGIN df489937386fe5b4f1d0a728db8e89223d684503404d2bdc664e1d7cb916717f 1618 apply_cli_overrides_preserve_const_enums_sets_checker_and_printer
    #[test]
    fn apply_cli_overrides_preserve_const_enums_sets_checker_and_printer() {
        // `--preserveConstEnums` must reach the checker's copy of the option,
        // not just the printer's: the checker consults it to decide whether an
        // unreachable `const enum` still "affects control flow" for TS7027
        // (an erased const enum does not; a preserved one does), matching
        // tsc's `preserveConstEnums`-gated `ModuleInstanceState` check. Only
        // wiring `options.printer.preserve_const_enums` left the checker
        // permanently reading the default `false`, silently erasing this
        // reachability distinction for any CLI invocation (the tsconfig.json
        // path already set both fields via `resolved_options.rs`).
        let mut options = ResolvedCompilerOptions::default();
        let args = CliArgs::try_parse_from(["tsz", "--preserveConstEnums"]).unwrap();
        apply_cli_overrides(&mut options, &args).unwrap();
        assert!(options.checker.preserve_const_enums);
        assert!(options.printer.preserve_const_enums);
    }
// TSZ_INLINE_TEST_END df489937386fe5b4f1d0a728db8e89223d684503404d2bdc664e1d7cb916717f

// TSZ_INLINE_TEST_BEGIN f36ce9f345a67720c19a6c41c265e2389b0d7e2649a566817d7effde085e51b7 1636 apply_cli_overrides_no_preserve_const_enums_leaves_checker_default
    #[test]
    fn apply_cli_overrides_no_preserve_const_enums_leaves_checker_default() {
        let mut options = ResolvedCompilerOptions::default();
        let args = CliArgs::try_parse_from(["tsz"]).unwrap();
        apply_cli_overrides(&mut options, &args).unwrap();
        assert!(!options.checker.preserve_const_enums);
        assert!(!options.printer.preserve_const_enums);
    }
// TSZ_INLINE_TEST_END f36ce9f345a67720c19a6c41c265e2389b0d7e2649a566817d7effde085e51b7

// TSZ_INLINE_TEST_BEGIN 30a1a95f35edd7dcdd722b07807920ca5b811ddb60a8773c71ef277d32c3302b 1645 longest_common_directory_shared_prefix
    #[test]
    fn longest_common_directory_shared_prefix() {
        use std::path::PathBuf;
        let a = PathBuf::from("/home/user/project/src");
        let b = PathBuf::from("/home/user/project/lib");
        let common = longest_common_directory(&a, &b);
        assert_eq!(common, PathBuf::from("/home/user/project"));
    }
// TSZ_INLINE_TEST_END 30a1a95f35edd7dcdd722b07807920ca5b811ddb60a8773c71ef277d32c3302b

// TSZ_INLINE_TEST_BEGIN d8f2b4afc936f49521c628a8e900fae8e5afa2f19f665e44f0eab66677ba8dc1 1654 longest_common_directory_no_common
    #[test]
    fn longest_common_directory_no_common() {
        use std::path::PathBuf;
        let a = PathBuf::from("/usr/local");
        let b = PathBuf::from("/home/user");
        let common = longest_common_directory(&a, &b);
        // On unix, "/" is the common root
        assert!(common == Path::new("/") || common.as_os_str().is_empty());
    }
// TSZ_INLINE_TEST_END d8f2b4afc936f49521c628a8e900fae8e5afa2f19f665e44f0eab66677ba8dc1
