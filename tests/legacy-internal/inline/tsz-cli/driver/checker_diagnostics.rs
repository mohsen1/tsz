//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/driver/checker_diagnostics.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 6cd80439aa7fa65e1a0ef205c35b91de0930310ec93a3d65878a36f949325e16 288 real_syntax_errors_suppress_semantic_ts1xxx_but_keep_parse_diagnostics
    #[test]
    fn real_syntax_errors_suppress_semantic_ts1xxx_but_keep_parse_diagnostics() {
        assert!(!keep_checker_diagnostic_when_program_has_real_syntax_errors(1064));
        // The global-module-export family is one tsc function reporting three
        // codes, so the three must agree here. `umd-errors.ts` is the corpus
        // witness: it pairs all three shapes with real syntax errors in a
        // sibling file, and tsc reports none of them.
        assert!(!keep_checker_diagnostic_when_program_has_real_syntax_errors(1314));
        assert!(!keep_checker_diagnostic_when_program_has_real_syntax_errors(1315));
        assert!(!keep_checker_diagnostic_when_program_has_real_syntax_errors(1316));
        // #16279 audit round 9: `in`/`out` as a class member's own modifier
        // (`class C { in x }`) is checker-emitted
        // (`check_variance_modifier_not_on_class_member_node`); tsc's oracle
        // suppresses it alongside an unrelated real syntax error.
        assert!(!keep_checker_diagnostic_when_program_has_real_syntax_errors(1274));
        assert!(keep_checker_diagnostic_when_program_has_real_syntax_errors(
            1005
        ));
        // #16279: the reserved interface-name TS2427 is now parser-owned for the
        // hard keywords `void`/`null` (a `ParseDiagnostic` that never reaches this
        // checker-diagnostic gate); the only TS2427 that reaches here is the soft
        // predefined-type-name form, which tsc suppresses under a sibling parse
        // error — so it must NOT be kept. The reserved type-alias-name TS2457,
        // whose hard-keyword `void` form tsz emits from the checker, IS kept
        // (tsc keeps `type void = ...`'s TS2457 alongside a parse error).
        assert!(!keep_checker_diagnostic_when_program_has_real_syntax_errors(2427));
        assert!(keep_checker_diagnostic_when_program_has_real_syntax_errors(
            2457
        ));
        assert!(!keep_checker_diagnostic_when_program_has_real_syntax_errors(2322));
    }
// TSZ_INLINE_TEST_END 6cd80439aa7fa65e1a0ef205c35b91de0930310ec93a3d65878a36f949325e16

// TSZ_INLINE_TEST_BEGIN 81de65491051359823965f303d6d2391b9e9573d5447176e863229e9afe11c05 336 nameless_jsdoc_typedef_is_a_real_syntax_error
    /// tsc parses JSDoc as part of a file's syntax tree, so a nameless
    /// `@typedef {Type}` tag (`TS1003`) is a genuine parse-time error there —
    /// verified against the pinned tsc@7.0.2 oracle: `f(1, 2, 3)` against a
    /// single-`@param` JS function normally reports `TS2554`, but that
    /// diagnostic (and every other semantic diagnostic in the program)
    /// disappears once a nameless `@typedef` is anywhere in the program,
    /// leaving only the `TS1003`. tsz discovers this during the checker's
    /// JSDoc pass rather than the parser, so `program_has_real_syntax_errors`
    /// must fold it in explicitly or the whole-program suppression never
    /// triggers for it.
    #[test]
    fn nameless_jsdoc_typedef_is_a_real_syntax_error() {
        let program =
            program_from("var exports = {};\n/** @typedef {string} */\nexports.SomeName;\n");
        assert!(program_has_real_syntax_errors(&program));
    }
// TSZ_INLINE_TEST_END 81de65491051359823965f303d6d2391b9e9573d5447176e863229e9afe11c05

// TSZ_INLINE_TEST_BEGIN 069c615f90053aa48c1e7d2964ba9e210fa9f40906f67f7c46d985ce307025e5 347 named_jsdoc_typedef_is_not_a_real_syntax_error
    /// Negative control: a properly-named `@typedef {Type} Name` tag is valid
    /// JSDoc grammar in tsc — no `TS1003`, so it must not trip the real-syntax
    /// -error gate (verified against the oracle: `exports.SomeName;` alone
    /// still reports an ordinary `TS2339` there, it is not suppressed).
    #[test]
    fn named_jsdoc_typedef_is_not_a_real_syntax_error() {
        let program = program_from(
            "var exports = {};\n/** @typedef {string} SomeName */\nexports.SomeName;\n",
        );
        assert!(!program_has_real_syntax_errors(&program));
    }
// TSZ_INLINE_TEST_END 069c615f90053aa48c1e7d2964ba9e210fa9f40906f67f7c46d985ce307025e5
