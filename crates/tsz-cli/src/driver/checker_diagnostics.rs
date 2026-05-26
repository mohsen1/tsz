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
        .any(|diag| is_real_syntax_error(diag.code))
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

const fn is_reserved_type_name_declaration_diagnostic(code: u32) -> bool {
    matches!(code, 2427 | 2457)
}

/// Returns true if a TS2427 diagnostic message refers to a hard reserved
/// keyword that triggers a parser error in tsc (`void` or `null`). When such
/// an interface declaration is present in a source file, tsc only surfaces
/// the TS2427 for that hard-keyword interface and suppresses TS2427 for any
/// other reserved-name interfaces in the same file. This mirrors tsc's
/// behavior in `interfacesWithPredefinedTypesAsNames.ts` and similar tests.
fn is_hard_keyword_interface_name_2427(diag: &Diagnostic) -> bool {
    if diag.code != 2427 {
        return false;
    }
    diag.message_text == "Interface name cannot be 'void'."
        || diag.message_text == "Interface name cannot be 'null'."
}

pub(super) fn keep_checker_diagnostic_when_program_has_real_syntax_errors(code: u32) -> bool {
    // tsc suppresses type-level semantic diagnostics when any source file in the
    // program has a real syntax error, but it still reports declaration-name
    // diagnostics such as TS2427/TS2457 alongside parse errors because the parser
    // accepts those names and defers validation to the checker.
    if code == 1315 {
        return false;
    }
    code < 2000
        || tsz::checker::diagnostics::is_js_grammar_diagnostic(code)
        || is_reserved_type_name_declaration_diagnostic(code)
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

    // When the file contains an `interface void {}` or `interface null {}`
    // declaration, tsc only emits TS2427 for that hard-keyword interface and
    // suppresses TS2427 for ANY other interfaces in the same file (including
    // ones with predefined-type names like `any`, `number`, etc.). This is
    // because tsc's parser produces a parse error for hard-keyword names,
    // which prevents the lazy diagnostic queue from running for the other
    // interface declarations. We don't currently emit a parse error in our
    // parser for `void`/`null` as interface names, so we model the same
    // suppression by filtering out non-hard-keyword TS2427 when a
    // hard-keyword TS2427 is present.
    let has_hard_keyword_ts2427 = checker_diagnostics
        .iter()
        .any(is_hard_keyword_interface_name_2427);
    if has_hard_keyword_ts2427 {
        checker_diagnostics.retain(|diag| {
            // Keep all non-TS2427 diagnostics untouched.
            if diag.code != 2427 {
                return true;
            }
            // Among TS2427, keep only the hard-keyword (`void`/`null`) ones.
            is_hard_keyword_interface_name_2427(diag)
        });
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
            // Some semantic errors are deliberately emitted alongside
            // structural parse errors and must not be suppressed.
            // TS2427 / TS2457 are checker-side validation for reserved type names
            // in interface/type-alias declarations. TSC keeps them even when the
            // surrounding file also has structural parse errors.
            if is_reserved_type_name_declaration_diagnostic(diag.code) {
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
