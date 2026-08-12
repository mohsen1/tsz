use super::*;

pub(super) fn should_skip_type_checking_for_file(
    file_name: &str,
    options: &ResolvedCompilerOptions,
    is_default_lib: bool,
) -> bool {
    (options.skip_lib_check && is_declaration_file(file_name))
        || (options.skip_default_lib_check && is_default_lib)
}

pub(super) fn program_has_real_syntax_errors(program: &MergedProgram) -> bool {
    program
        .files
        .iter()
        .flat_map(|file| file.parse_diagnostics.iter())
        // TS2427/TS2457 as *parse* diagnostics are the hard-keyword/numeric
        // reserved-name rejections the parser owns (`void`/`null`/`123` as an
        // interface or type-alias name — soft predefined-type names are
        // checker-emitted and never reach `parse_diagnostics`). tsc treats those
        // as `hasParseDiagnostics`, short-circuiting the whole file's semantic
        // phase — e.g. `interface void {}` next to a real type error suppresses
        // the type error entirely (#16279). Counting them here reproduces that.
        .any(|diag| is_real_syntax_error(diag.code) || matches!(diag.code, 2427 | 2457))
        || program
            .files
            .iter()
            .any(file_has_jsdoc_typedef_missing_name_error)
}

/// tsc parses JSDoc comments as part of a file's syntax tree, so a nameless
/// `@typedef {Type}` tag (`TS1003`, "Identifier expected.") is a genuine
/// parse-time error there — even though tsz discovers it during the checker's
/// JSDoc pass rather than the parser. `program_has_real_syntax_errors` must
/// see it as a real syntax error too, or the whole-program semantic
/// suppression this file drives elsewhere never triggers for it.
fn file_has_jsdoc_typedef_missing_name_error(file: &BoundFile) -> bool {
    let Some(node) = file.arena.get(file.source_file) else {
        return false;
    };
    file.arena.get_source_file(node).is_some_and(|sf| {
        !tsz::checker::diagnostics::jsdoc_typedef_missing_name_anchors(sf).is_empty()
    })
}

pub(super) fn program_has_unsupported_js_root(
    program: &MergedProgram,
    options: &ResolvedCompilerOptions,
) -> bool {
    !options.allow_js
        && program
            .files
            .iter()
            .any(|file| is_js_file(Path::new(&file.file_name)))
}

/// The reserved *type-alias*-name diagnostic (TS2457) that survives a sibling
/// parse error, unlike the reserved *interface*-name diagnostic (TS2427).
///
/// tsc emits `type void = ...`'s TS2457 such that it survives alongside an
/// unrelated parse error (oracle: `type void = number` survives Direction B),
/// and tsz emits even that hard-keyword form from the checker
/// (`statement_callback_bridge`), never from the parser — and never for a soft
/// name. TS2427 is the opposite: its hard-keyword `void`/`null` form is now
/// parser-owned (`state_declarations.rs`) so it never reaches a checker gate,
/// and the soft form that does reach one is a checker `grammarErrorOnNode` that
/// tsc suppresses under a sibling parse error. So the two codes are gated
/// differently everywhere the checker-diagnostic pipeline consults them (#16279).
const fn checker_reserved_type_alias_name_survives_parse_error(code: u32) -> bool {
    code == 2457
}

pub(super) fn keep_checker_diagnostic_when_program_has_real_syntax_errors(code: u32) -> bool {
    // tsc suppresses type-level semantic diagnostics when any source file in the
    // program has a real syntax error. `code < 2000` is a proxy for "the parser
    // emitted this"; `is_checker_routed_ts1xxx_grammar` corrects the TS1xxx codes
    // tsz emits from the checker/binder that tsc routes through the semantic
    // phase. TS2457 is kept per its own rule (see the predicate); TS2427 is not
    // (it is now parser-owned for hard keywords, checker-emitted for soft names
    // which tsc suppresses here).
    if check_utils::is_checker_routed_ts1xxx_grammar(code) {
        return false;
    }
    code < 2000
        || tsz::checker::diagnostics::is_js_grammar_diagnostic(code)
        || checker_reserved_type_alias_name_survives_parse_error(code)
}

/// `TS1xxx` codes that tsc routes through `getSemanticDiagnostics`. They are in
/// the parser-grammar range numerically but are emitted from the checker, so
/// unchecked JS files (no `checkJs`, or `// @ts-nocheck`) must not see them
/// even though `code < 2000` would otherwise let them through. Issue #3693.
const fn is_semantic_ts1xxx_suppressed_in_unchecked_js(code: u32) -> bool {
    matches!(
        code,
        1192 // Module '{0}' has no default export.
        | 1259 // Module '{0}' can only be default-imported using the '{1}' flag
    )
}

pub(super) fn post_process_checker_diagnostics(
    checker_diagnostics: &mut Vec<Diagnostic>,
    file: &BoundFile,
    options: &ResolvedCompilerOptions,
    program_has_real_syntax_errors: bool,
    program_has_unsupported_js_root: bool,
    has_deprecation_diagnostics: bool,
) {
    // JSDoc type parsing can surface a structural TS1005 through the checker
    // diagnostic stream rather than `BoundFile::parse_diagnostics`. Tsc marks
    // the malformed function-type annotation erroneous and suppresses its
    // TS1064 follow-on. Do not promote this to a program-wide syntax error:
    // other malformed JSDoc tags can legitimately retain semantic diagnostics
    // such as TS2304.
    if checker_diagnostics
        .iter()
        .any(|diagnostic| is_real_syntax_error(diagnostic.code))
    {
        checker_diagnostics.retain(|diagnostic| diagnostic.code != 1064);
    }
    let is_js = is_js_file(Path::new(&file.file_name));
    let has_ts_check_pragma = js_file_has_ts_check_pragma(file);
    let has_ts_nocheck_pragma = js_file_has_ts_nocheck_pragma(file);
    let should_filter_type_errors =
        is_js && (has_ts_nocheck_pragma || (!options.check_js && !has_ts_check_pragma));

    if should_filter_type_errors {
        // Keep syntax/semantic diagnostics (< 2000) and JS grammar diagnostics
        // (TS8xxx). When `checkJs` is NOT explicitly false (the default
        // no-checkJs mode), also allow the `plainJSErrors` codes that tsc
        // surfaces even in unchecked JS files. When `checkJs: false` is
        // explicitly set, suppress ALL semantic errors.
        //
        // Issue #3693: a few TS1xxx codes are semantic checker diagnostics
        // that tsc routes through `getSemanticDiagnostics`. Their numeric
        // code is < 2000 but they must NOT survive unchecked-JS filtering,
        // because tsc doesn't surface them in that mode either.
        checker_diagnostics.retain(|diag| {
            if is_semantic_ts1xxx_suppressed_in_unchecked_js(diag.code) {
                return false;
            }
            diag.code < 2000
                || tsz::checker::diagnostics::is_js_grammar_diagnostic(diag.code)
                || (!options.explicit_check_js_false && is_plain_js_allowed_code(diag.code))
        });
    }

    // For JS files, suppress checker-emitted TS1xxx grammar codes that tsc
    // does NOT emit for JavaScript files. tsc's grammar checks (emitted via
    // grammarErrorOnNode) are suppressed for TypeScript-only constructs in JS
    // files because its parser handles them leniently. Our parser doesn't
    // distinguish JS vs TS, so checker-side grammar errors leak through.
    // Only keep TS1xxx codes that tsc is known to emit for JS files.
    if is_js {
        checker_diagnostics.retain(|diag| {
            // Some semantic checker diagnostics live in the TS1xxx range. Keep
            // them for checked JS files even though the coarse parser-grammar
            // classifier also covers TS1xxx.
            if !should_filter_type_errors
                && (matches!(diag.code, 1361 | 1362)
                    || is_semantic_ts1xxx_suppressed_in_unchecked_js(diag.code))
            {
                return true;
            }
            if tsz::checker::diagnostics::is_parser_grammar_diagnostic(diag.code) {
                return is_ts1xxx_allowed_in_js(diag.code);
            }
            // Also suppress checker-emitted grammar codes outside the 1xxx range
            // that tsc doesn't emit for JS files.
            if is_checker_grammar_code_suppressed_in_js(diag.code) {
                return false;
            }
            true
        });
    }

    if program_has_real_syntax_errors {
        checker_diagnostics
            .retain(|diag| keep_checker_diagnostic_when_program_has_real_syntax_errors(diag.code));
    }

    if program_has_unsupported_js_root && !program_has_real_syntax_errors {
        // tsc reports program-level TS6504 for explicit JS/CJS roots when
        // allowJs is disabled, then skips downstream semantic checks.
        checker_diagnostics
            .retain(|diag| keep_checker_diagnostic_when_program_has_real_syntax_errors(diag.code));
    }

    // TS2754 ("super may not use type arguments") indicates a fundamental class
    // hierarchy error. tsc suppresses all other semantic diagnostics when TS2754
    // is present. TS2754 is emitted by the parser, so check parse diagnostics.
    let has_ts2754 = file.parse_diagnostics.iter().any(|d| d.code == 2754);
    if has_ts2754 {
        checker_diagnostics.retain(|diag| diag.code < 2000);
    }

    // The `interface void {}` / `interface null {}` same-file suppression that
    // used to live here is now handled structurally: the parser owns the
    // hard-keyword TS2427 (a `ParseDiagnostic`), which makes
    // `program_has_real_syntax_errors` true and suppresses every *checker*
    // TS2427 (the soft predefined-type-name form) in the file via the keep-gate
    // above — exactly tsc's `hasParseDiagnostics` short-circuit, with no
    // message-text matching (#16279).

    // TS2499 ("An interface can only extend an identifier/qualified-name
    // with optional type arguments") is grammar-decidable at parse time for
    // a parenthesized or bracketed heritage expression
    // (`interface I extends (1 + 2) {}`), and the parser
    // (`parse_interface_heritage_type_reference`) already reports it there
    // at tsc's own position. The checker's independent, more general
    // heritage walk (`heritage.rs`) does not know the parser already
    // flagged that exact node — it structurally rejects anything that is
    // not an identifier/qualified-name chain — so it reports TS2499 again
    // for the same span. tsc emits this diagnostic exactly once; drop the
    // checker's copy wherever a parser TS2499 already covers the same
    // position, keeping any TS2499 the checker alone finds (e.g. non-paren
    // heritage shapes the parser's grammar check does not special-case).
    let parser_ts2499_positions: std::collections::HashSet<u32> = file
        .parse_diagnostics
        .iter()
        .filter(|d| d.code == 2499)
        .map(|d| d.start)
        .collect();
    if !parser_ts2499_positions.is_empty() {
        checker_diagnostics
            .retain(|diag| diag.code != 2499 || !parser_ts2499_positions.contains(&diag.start));
    }

    // TS1155 ("'{0}' declarations must be initialized.") has the identical
    // double-emission shape as TS2499 above (#17253 follow-up to #17251): the
    // parser's `report_const_or_using_uninitialized` reports it at tsc's own
    // position (the declarator name) for the plain-statement and C-style
    // `for`-header cases, and the checker's own independent
    // `check_variable_declaration_with_request` walk (which pre-dates #17251
    // and exists so the checker-only unit-test harness can exercise the rule
    // in isolation, `using_declaration_implicit_any_tests.rs`) reports it
    // again for the same declarator whenever `has_real_syntax_errors` is
    // false. tsc emits this diagnostic exactly once; drop the checker's copy
    // wherever a parser TS1155 already covers the same position.
    let parser_ts1155_positions: std::collections::HashSet<u32> = file
        .parse_diagnostics
        .iter()
        .filter(|d| d.code == 1155)
        .map(|d| d.start)
        .collect();
    if !parser_ts1155_positions.is_empty() {
        checker_diagnostics
            .retain(|diag| diag.code != 1155 || !parser_ts1155_positions.contains(&diag.start));
    }

    // When TS5107/TS5101 deprecation diagnostics are present, suppress the most
    // common type relationship errors that tsc would not emit. Parser errors
    // (<2000) are handled separately and not affected by this filter.
    if has_deprecation_diagnostics {
        // Type relationship errors to suppress when deprecation warnings are present
        const SUPPRESSED_TYPE_CODES: &[u32] = &[
            2322, // TS2322: Type not assignable
            2345, // TS2345: Argument not assignable
            2339, // TS2339: Property does not exist
            2343, // TS2343: Access modifier error
            2882, // TS2882: Cannot find module/type declarations for side-effect import
            2304, // TS2304: Cannot find name
            2307, // TS2307: Cannot find module
            7006, // TS7006: Parameter implicitly has 'any' type
            7005, // TS7005: Variable implicitly has 'any' type
            2323, // TS2323: Cannot redeclare exported variable
            2741, // TS2741: Missing properties
            2510, // TS2510: Cannot assign to read-only property
            2694, // TS2694: Namespace not found
            2531, // TS2531: Possibly null
            2532, // TS2532: Possibly undefined
            2533, // TS2533: Object is possibly null or undefined
            2564, // TS2564: Property has no initializer
            2454, // TS2454: Variable used before being assigned
            2403, // TS2403: Subsequent variable declarations must have same type
            2411, // TS2411: Property conflict
            2300, // TS2300: Duplicate identifier
        ];
        checker_diagnostics.retain(|diag| !SUPPRESSED_TYPE_CODES.contains(&diag.code));
    }

    // Suppress semantic errors that cascade from structural parse failures.
    // tsc sets per-node ThisNodeHasError flags and skips semantic checks on
    // error-recovery subtrees. We approximate this by suppressing semantic
    // diagnostics that are near a structural parse error (within a distance
    // window). Only structural parse failures (missing tokens, unexpected
    // tokens) trigger suppression — grammar checks like trailing commas or
    // strict mode violations don't cause AST malformation and shouldn't
    // suppress semantic errors.
    let structural_error_positions: Vec<u32> = file
        .parse_diagnostics
        .iter()
        .filter(|d| is_structural_parse_error(d.code))
        .map(|d| d.start)
        .collect();
    if !structural_error_positions.is_empty() {
        const MAX_CASCADE_DISTANCE: u32 = 300;
        checker_diagnostics.retain(|diag| {
            // Keep parse/grammar errors (1xxx) and JS grammar errors (8xxx)
            if diag.code < 2000 || tsz::checker::diagnostics::is_js_grammar_diagnostic(diag.code) {
                return true;
            }
            // TS2457 survives a sibling parse error (see the predicate); its
            // TS2427 sibling does not, so it is not exempted here (#16279).
            if checker_reserved_type_alias_name_survives_parse_error(diag.code) {
                return true;
            }
            // Suppress if a structural parse error is within the cascade window
            !structural_error_positions.iter().any(|&err_pos| {
                let dist = diag.start.abs_diff(err_pos);
                dist <= MAX_CASCADE_DISTANCE
            })
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{
        keep_checker_diagnostic_when_program_has_real_syntax_errors, program_has_real_syntax_errors,
    };
    use tsz::parallel;

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

    fn program_from(source: &str) -> tsz::parallel::MergedProgram {
        let bind_result =
            parallel::parse_and_bind_single("test.js".to_string(), source.to_string());
        parallel::merge_bind_results(vec![bind_result])
    }

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
}
