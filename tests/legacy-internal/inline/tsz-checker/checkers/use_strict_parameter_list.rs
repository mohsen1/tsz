//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/checkers/use_strict_parameter_list.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN a2123428fc9d3cbe638b11a0d473e9acaf62eaea78d81a423d9c16cf685195a0 300 use_strict_non_simple_parameter_list_reports_ts1346_ts1347
    #[test]
    fn use_strict_non_simple_parameter_list_reports_ts1346_ts1347() {
        // A default initializer, a rest element, and a binding pattern each make
        // the parameter list non-simple. Vary binder names to keep the check
        // structural, not name-scoped.
        for source in [
            "function widget(size = 1) { \"use strict\"; }",
            "function collect(...items) { \"use strict\"; }",
            "function unpack({ label }) { \"use strict\"; }",
            "const handler = (opt = 2) => { \"use strict\"; };",
            "class Store { method(seed = 3) { \"use strict\"; } }",
        ] {
            let codes = checker_codes_at_target(source, tsz_common::common::ScriptTarget::ES2016);
            assert!(
                codes.contains(&1346) && codes.contains(&1347),
                "expected TS1346+TS1347 for `{source}`: {codes:?}"
            );
        }
    }
// TSZ_INLINE_TEST_END a2123428fc9d3cbe638b11a0d473e9acaf62eaea78d81a423d9c16cf685195a0

// TSZ_INLINE_TEST_BEGIN 042a5fb0e9757dc1e964228abac0c5218127e04fbd702fed3a2011446a5d0dba 322 ts1346_carries_one_ts1349_pointing_at_the_directive
    /// TS1346 points forward at the directive with a single TS1349 entry, for
    /// every non-simple parameter shape and every function-like carrier.
    #[test]
    fn ts1346_carries_one_ts1349_pointing_at_the_directive() {
        for source in [
            "function widget(size = 1) { \"use strict\"; }",
            "function collect(...items) { \"use strict\"; }",
            "function unpack({ label }) { \"use strict\"; }",
            "const handler = (opt = 2) => { \"use strict\"; };",
            "class Store { method(seed = 3) { \"use strict\"; } }",
            "class Widget { set label({ text }) { \"use strict\"; } }",
        ] {
            let related = related_of(source, 1346, tsz_common::common::ScriptTarget::ES2016)
                .unwrap_or_else(|| panic!("no TS1346 reported for `{source}`"));
            assert_eq!(
                related,
                vec![(1349, "'use strict' directive used here.".to_string())],
                "TS1346 related information for `{source}`"
            );
            // The related entry must land on the directive, not on the
            // parameter TS1346 is already anchored at.
            assert_eq!(
                related_start(source, 1346, 1349, tsz_common::common::ScriptTarget::ES2016),
                Some(
                    source
                        .find("\"use strict\"")
                        .expect("witness contains a \"use strict\" directive")
                        as u32
                ),
                "TS1349 anchor for `{source}`"
            );
        }
    }
// TSZ_INLINE_TEST_END 042a5fb0e9757dc1e964228abac0c5218127e04fbd702fed3a2011446a5d0dba

// TSZ_INLINE_TEST_BEGIN 8f9f7a2c4075bc85f69ff49ba86f2f50eea9421be0b4fd1d616d273dfb9e3953 356 ts1347_carries_ts1348_then_and_here_per_extra_parameter
    /// TS1347 points back at every offending parameter: TS1348 names the first,
    /// TS6204 (`and here.`) elides each subsequent one, in source order.
    #[test]
    fn ts1347_carries_ts1348_then_and_here_per_extra_parameter() {
        let single = "function widget(size = 1) { \"use strict\"; }";
        assert_eq!(
            related_of(single, 1347, tsz_common::common::ScriptTarget::ES2016),
            Some(vec![(
                1348,
                "Non-simple parameter declared here.".to_string()
            )]),
            "one non-simple parameter must produce exactly one TS1348 and no `and here.`"
        );

        // Three parameters, two of them non-simple, with a simple parameter
        // interleaved — the related list must skip the simple one and stay in
        // source order.
        let multi = "function render(size = 1, plain, ...rest) { \"use strict\"; }";
        assert_eq!(
            related_of(multi, 1347, tsz_common::common::ScriptTarget::ES2016),
            Some(vec![
                (1348, "Non-simple parameter declared here.".to_string()),
                (6204, "and here.".to_string()),
            ]),
            "TS1347 related information for `{multi}`"
        );
        assert_eq!(
            related_start(multi, 1347, 1348, tsz_common::common::ScriptTarget::ES2016),
            Some(
                multi
                    .find("size = 1")
                    .expect("witness contains the first non-simple parameter")
                    as u32
            ),
            "TS1348 must anchor on the first non-simple parameter"
        );
        assert_eq!(
            related_start(multi, 1347, 6204, tsz_common::common::ScriptTarget::ES2016),
            Some(
                multi
                    .find("...rest")
                    .expect("witness contains the rest parameter") as u32
            ),
            "`and here.` must anchor on the rest parameter including its `...`"
        );
    }
// TSZ_INLINE_TEST_END 8f9f7a2c4075bc85f69ff49ba86f2f50eea9421be0b4fd1d616d273dfb9e3953

// TSZ_INLINE_TEST_BEGIN a26cb09a22abc3917292329ea9c406035f952f1a69c4aa34e7c0279a65a6167d 403 related_codes_do_not_leak_when_the_check_does_not_fire
    /// The negative side: when the grammar check does not fire, neither related
    /// code may appear anywhere in the file's diagnostics.
    #[test]
    fn related_codes_do_not_leak_when_the_check_does_not_fire() {
        for (source, target) in [
            (
                "function plain(first, second) { \"use strict\"; }",
                tsz_common::common::ScriptTarget::ES2016,
            ),
            (
                "function widget(size = 1) { \"use strict\"; }",
                tsz_common::common::ScriptTarget::ES2015,
            ),
            (
                "function widget(size = 1) { const c = 1; \"use strict\"; }",
                tsz_common::common::ScriptTarget::ES2016,
            ),
        ] {
            let diagnostics = checker_diagnostics_at_target(source, target);
            for diag in &diagnostics {
                for rel in &diag.related_information {
                    assert!(
                        !matches!(rel.code, 1348 | 1349),
                        "TS{} leaked as related information on TS{} for `{source}`",
                        rel.code,
                        diag.code
                    );
                }
            }
        }
    }
// TSZ_INLINE_TEST_END a26cb09a22abc3917292329ea9c406035f952f1a69c4aa34e7c0279a65a6167d

// TSZ_INLINE_TEST_BEGIN 6016377f9b263f37e84edaddc89536e6d0547c107ade0e0dbbabec8a4549eba9 433 use_strict_simple_parameter_list_is_clean
    #[test]
    fn use_strict_simple_parameter_list_is_clean() {
        let codes = checker_codes_at_target(
            "function plain(first, second) { \"use strict\"; }",
            tsz_common::common::ScriptTarget::ES2016,
        );
        assert!(
            !codes.contains(&1346) && !codes.contains(&1347),
            "simple parameter list must not report TS1346/TS1347: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 6016377f9b263f37e84edaddc89536e6d0547c107ade0e0dbbabec8a4549eba9

// TSZ_INLINE_TEST_BEGIN cf2a52f4db6461fc97ce24ad610bec73030f8819a35b3683f9207648b8623fbd 445 use_strict_non_simple_parameter_list_gated_below_es2016
    #[test]
    fn use_strict_non_simple_parameter_list_gated_below_es2016() {
        // tsc's checkGrammarForUseStrictSimpleParameterList only runs at ES2016+.
        let codes = checker_codes_at_target(
            "function widget(size = 1) { \"use strict\"; }",
            tsz_common::common::ScriptTarget::ES2015,
        );
        assert!(
            !codes.contains(&1346) && !codes.contains(&1347),
            "below ES2016 must not report TS1346/TS1347: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END cf2a52f4db6461fc97ce24ad610bec73030f8819a35b3683f9207648b8623fbd

// TSZ_INLINE_TEST_BEGIN 6e278c3a3fac4af5ec9a4f1c9353579fe19896464854b481ac429f0fd1819231 458 use_strict_after_non_directive_statement_is_clean
    #[test]
    fn use_strict_after_non_directive_statement_is_clean() {
        // `"use strict"` is only a directive in the leading prologue; once a
        // non-directive statement precedes it, it is an ordinary expression and
        // the grammar check does not apply.
        let codes = checker_codes_at_target(
            "function widget(size = 1) { const c = 1; \"use strict\"; }",
            tsz_common::common::ScriptTarget::ES2016,
        );
        assert!(
            !codes.contains(&1346) && !codes.contains(&1347),
            "non-prologue 'use strict' must not report TS1346/TS1347: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 6e278c3a3fac4af5ec9a4f1c9353579fe19896464854b481ac429f0fd1819231

// TSZ_INLINE_TEST_BEGIN 12d3646f29d43a08bd9545a6d2313961f9ada2d41c91b909e8eec8f78fb38e1b 473 set_accessor_use_strict_non_simple_param_reports_ts1346_ts1347
    #[test]
    fn set_accessor_use_strict_non_simple_param_reports_ts1346_ts1347() {
        // Set-accessor parameters route through the accessor grammar path, not
        // the shared per-function-like param check; the use-strict check is
        // wired in there too. Vary binder names to keep the check structural.
        for source in [
            "class Store { set value(seed = 1) { \"use strict\"; } }",
            "class Widget { set label({ text }) { \"use strict\"; } }",
        ] {
            let codes = checker_codes_at_target(source, tsz_common::common::ScriptTarget::ES2016);
            assert!(
                codes.contains(&1346) && codes.contains(&1347),
                "expected TS1346+TS1347 for `{source}`: {codes:?}"
            );
        }
    }
// TSZ_INLINE_TEST_END 12d3646f29d43a08bd9545a6d2313961f9ada2d41c91b909e8eec8f78fb38e1b

// TSZ_INLINE_TEST_BEGIN b3e06df3784891cddd9580496ad6a65cf9891d88087373d3569cf47b2885d8f6 490 class_accessor_without_body_reports_ts1005
    #[test]
    fn class_accessor_without_body_reports_ts1005() {
        // A non-ambient, non-abstract class accessor without a brace body is
        // TS1005, emitted at check time so it coexists with semantic diagnostics.
        for source in [
            "class Store { get value(): string; }",
            "class Widget { set label(v: string); }",
        ] {
            let codes = checker_codes_at_target(source, tsz_common::common::ScriptTarget::ES2016);
            assert!(
                codes.contains(&1005),
                "expected TS1005 for `{source}`: {codes:?}"
            );
        }
    }
// TSZ_INLINE_TEST_END b3e06df3784891cddd9580496ad6a65cf9891d88087373d3569cf47b2885d8f6

// TSZ_INLINE_TEST_BEGIN aac1affd8419c7bef2a1fa8e16a0dd424cf599e071e297815c1c7e5511b8f3a8 506 abstract_or_ambient_accessor_without_body_is_clean
    #[test]
    fn abstract_or_ambient_accessor_without_body_is_clean() {
        // Abstract and ambient (`declare class`) accessors are legitimately
        // body-less and must not report TS1005.
        for source in [
            "abstract class Store { abstract get value(): string; }",
            "declare class Widget { get label(): string; }",
        ] {
            let codes = checker_codes_at_target(source, tsz_common::common::ScriptTarget::ES2016);
            assert!(
                !codes.contains(&1005),
                "unexpected TS1005 for `{source}`: {codes:?}"
            );
        }
    }
// TSZ_INLINE_TEST_END aac1affd8419c7bef2a1fa8e16a0dd424cf599e071e297815c1c7e5511b8f3a8
