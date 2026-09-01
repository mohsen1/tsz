//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/bin/tsz/arg_preprocess.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 6715bdc5bac5147451e9d8403edf339bfb78ac3fac8e7424d7ab3f81865c8ef7 816 preprocesses_tsc_compat_rewrites_from_case_table
    #[test]
    fn preprocesses_tsc_compat_rewrites_from_case_table() {
        struct Case {
            name: &'static str,
            input: &'static [&'static str],
            expected: &'static [&'static str],
        }

        let cases = [
            Case {
                name: "case-insensitive camel and kebab flags",
                input: &["tsz", "--No-Emit", "--types-versions", "5.7", "file.ts"],
                expected: &["tsz", "--noEmit", "--typesVersions", "5.7", "file.ts"],
            },
            Case {
                name: "build-mode short flags",
                input: &["tsz", "--build", "-v", "-d", "-f"],
                expected: &["tsz", "--build", "--build-verbose", "--dry", "--force"],
            },
            Case {
                name: "plain boolean false side channel",
                input: &["tsz", "--strict", "false", "file.ts"],
                expected: &["tsz", "--__explicitly-disabled-bool-flag=strict", "file.ts"],
            },
            Case {
                name: "option boolean defaults to true before file",
                input: &["tsz", "--strictNullChecks", "file.ts"],
                expected: &["tsz", "--strictNullChecks=true", "file.ts"],
            },
            Case {
                name: "duplicate valued flag keeps last value",
                input: &["tsz", "--target", "ES2020", "--target", "ES2022", "file.ts"],
                expected: &["tsz", "--target", "ES2022", "file.ts"],
            },
        ];

        for case in cases {
            assert_eq!(preprocess_strs(case.input), case.expected, "{}", case.name);
        }
    }
// TSZ_INLINE_TEST_END 6715bdc5bac5147451e9d8403edf339bfb78ac3fac8e7424d7ab3f81865c8ef7

// TSZ_INLINE_TEST_BEGIN 9a5a5d107eef471f8b8166e2cc7a62fd2c5c656bf3002b31fd13c86ec7ebc950 857 split_response_line_respects_single_and_double_quotes
    #[test]
    fn split_response_line_respects_single_and_double_quotes() {
        let cases = [
            (
                r#"--outDir "my output" --rootDir 'src root'"#,
                vec!["--outDir", "my output", "--rootDir", "src root"],
            ),
            (r#"foo"bar"baz"#, vec!["foobarbaz"]),
            (
                r#""file one.ts" "file two.ts""#,
                vec!["file one.ts", "file two.ts"],
            ),
        ];

        for (input, expected) in cases {
            assert_eq!(split_response_line(input), expected);
        }
    }
// TSZ_INLINE_TEST_END 9a5a5d107eef471f8b8166e2cc7a62fd2c5c656bf3002b31fd13c86ec7ebc950

// TSZ_INLINE_TEST_BEGIN 2a7049421e11a47a36543c44fcd96a291e1c711c3f8514e3e37aea7b768a3041 893 version_flags_exit_zero_with_version_banner
    #[test]
    fn version_flags_exit_zero_with_version_banner() {
        let expected = format!("Version {TSC_VERSION}");
        for input in [
            &["tsz", "--version"][..],
            &["tsz", "-V"][..],
            &["tsz", "-v"][..],
            &["tsz", "file.ts", "--version"][..],
        ] {
            let exit = early_exit(input);
            assert_eq!(exit.code, 0, "{input:?}");
            assert_eq!(exit.message, expected, "{input:?}");
        }
    }
// TSZ_INLINE_TEST_END 2a7049421e11a47a36543c44fcd96a291e1c711c3f8514e3e37aea7b768a3041

// TSZ_INLINE_TEST_BEGIN 558e44c32a6c3d229f50a0d38409a63c4b5b3debd58b9a8d02df87b76dca2e43 908 help_and_all_flags_exit_zero_with_nonempty_banner
    #[test]
    fn help_and_all_flags_exit_zero_with_nonempty_banner() {
        for input in [
            &["tsz", "--help"][..],
            &["tsz", "-h"][..],
            &["tsz", "-?"][..],
            &["tsz", "--all"][..],
        ] {
            let exit = early_exit(input);
            assert_eq!(exit.code, 0, "{input:?}");
            assert!(!exit.message.trim().is_empty(), "{input:?}");
        }
    }
// TSZ_INLINE_TEST_END 558e44c32a6c3d229f50a0d38409a63c4b5b3debd58b9a8d02df87b76dca2e43

// TSZ_INLINE_TEST_BEGIN cca97942713cdf7cb7259fa14f79a7a9e5b6f20535d47d58eaef1b929571a15b 922 all_takes_precedence_over_help
    #[test]
    fn all_takes_precedence_over_help() {
        // `--all` and `--help` together must render the all-options banner,
        // preserving the original precedence order.
        let combined = early_exit(&["tsz", "--all", "--help"]);
        let all_only = early_exit(&["tsz", "--all"]);
        assert_eq!(combined.message, all_only.message);
        assert_eq!(combined.code, 0);
    }
// TSZ_INLINE_TEST_END cca97942713cdf7cb7259fa14f79a7a9e5b6f20535d47d58eaef1b929571a15b

// TSZ_INLINE_TEST_BEGIN cb2f3b739c42bb6de4b17e78f69a45187a7687423658fd46b90dc29f94b235a8 932 dash_v_is_build_verbose_not_version_in_build_mode
    #[test]
    fn dash_v_is_build_verbose_not_version_in_build_mode() {
        // In build mode `-v` means --build-verbose, so it must NOT early-exit
        // as a version request; it stays in the normalized args.
        assert!(is_continue(&["tsz", "--build", "-v"]));
        let normalized = preprocess_strs(&["tsz", "--build", "-v"]);
        assert!(normalized.iter().any(|a| a == "--build-verbose"));
        assert!(!normalized.iter().any(|a| a == "--version"));
    }
// TSZ_INLINE_TEST_END cb2f3b739c42bb6de4b17e78f69a45187a7687423658fd46b90dc29f94b235a8

// TSZ_INLINE_TEST_BEGIN 446e30ea6768d0088cbdef2cd421597ba04e28e0928997f7ef5396d8938a7313 942 unknown_bare_dashes_reject_with_ts5023
    #[test]
    fn unknown_bare_dashes_reject_with_ts5023() {
        for opt in ["--", "-"] {
            let exit = early_exit(&["tsz", opt]);
            assert_eq!(exit.code, 1, "{opt}");
            assert_eq!(
                exit.message,
                format!("error TS5023: Unknown compiler option '{opt}'."),
            );
        }
    }
// TSZ_INLINE_TEST_END 446e30ea6768d0088cbdef2cd421597ba04e28e0928997f7ef5396d8938a7313

// TSZ_INLINE_TEST_BEGIN 731d084a67d534beb847f562f55240847ec971205390d2c21fe80a0b0c8b973f 954 boolean_flag_with_equals_value_rejects_with_ts5023
    #[test]
    fn boolean_flag_with_equals_value_rejects_with_ts5023() {
        // tsc treats `--noEmit=true` (a boolean flag in `--flag=value` form) as
        // an unknown option, reported verbatim with the whole token.
        let exit = early_exit(&["tsz", "--noEmit=true", "file.ts"]);
        assert_eq!(exit.code, 1);
        assert_eq!(
            exit.message,
            "error TS5023: Unknown compiler option '--noEmit=true'."
        );
    }
// TSZ_INLINE_TEST_END 731d084a67d534beb847f562f55240847ec971205390d2c21fe80a0b0c8b973f

// TSZ_INLINE_TEST_BEGIN b830201110a0c353f102af920aae084a4628365f999245ecc5e8466973090e42 966 build_must_be_the_first_argument
    #[test]
    fn build_must_be_the_first_argument() {
        // `--build` not first → TS6369; `-b` not first → TS5023; first → continue.
        let long = early_exit(&["tsz", "file.ts", "--build"]);
        assert_eq!(long.code, 1);
        assert_eq!(
            long.message,
            "error TS6369: Option '--build' must be the first command line argument."
        );

        let short = early_exit(&["tsz", "file.ts", "-b"]);
        assert_eq!(short.code, 1);
        assert_eq!(short.message, "error TS5023: Unknown compiler option '-b'.");

        assert!(is_continue(&["tsz", "--build", "file.ts"]));
    }
// TSZ_INLINE_TEST_END b830201110a0c353f102af920aae084a4628365f999245ecc5e8466973090e42

// TSZ_INLINE_TEST_BEGIN 53c5622f4caf913a1313b18a11d1a1b80831c930de9f127c164ddacfa7c12a32 983 ordinary_invocation_returns_continue_byte_stable
    #[test]
    fn ordinary_invocation_returns_continue_byte_stable() {
        // A normal compile invocation triggers no early exit and needs no
        // rewrite: it must come back as Continue with the argv unchanged.
        let input = &["tsz", "--noEmit", "src/main.ts"];
        assert!(is_continue(input));
        assert_eq!(preprocess_strs(input), input.to_vec());
    }
// TSZ_INLINE_TEST_END 53c5622f4caf913a1313b18a11d1a1b80831c930de9f127c164ddacfa7c12a32
