//! Utility functions for the compilation driver's checking phase:
//! export hash computation, tslib helper detection, binder construction,
//! parse diagnostic conversion, and pragma detection.

use super::*;
use tsz_common::position::LineMap;

#[path = "check_utils/tslib_helpers.rs"]
mod tslib_helpers;

pub(super) fn detect_missing_tslib_helper_diagnostics(
    program: &MergedProgram,
    options: &ResolvedCompilerOptions,
    base_dir: &Path,
    file_is_esm_map: &rustc_hash::FxHashMap<String, bool>,
) -> Vec<Diagnostic> {
    tslib_helpers::detect_missing_tslib_helper_diagnostics(
        program,
        options,
        base_dir,
        file_is_esm_map,
    )
}

#[cfg(test)]
fn required_helpers(
    file: &BoundFile,
    target: tsz_common::ScriptTarget,
    es_module_interop: bool,
    is_esm: bool,
    experimental_decorators: bool,
) -> Vec<(&'static str, u32, u32)> {
    tslib_helpers::required_helpers(
        file,
        target,
        es_module_interop,
        is_esm,
        experimental_decorators,
    )
}

/// Compute the unified export signature for a file from the merged program.
///
/// This uses the same `ExportSignatureInput` -> `ExportSignature` pipeline as the
/// LSP, ensuring both systems produce identical hashes for the same public API
/// surface. The signature is binder-level (names, flags, re-exports, augmentations)
/// and does not include checker-inferred types.
pub(super) fn compute_export_signature(
    program: &MergedProgram,
    file: &BoundFile,
    file_idx: usize,
) -> tsz_lsp::export_signature::ExportSignature {
    let input = build_export_signature_input(program, file, file_idx);
    tsz_lsp::export_signature::ExportSignature::from_input(&input)
}

/// Build an `ExportSignatureInput` from the merged program's per-file data.
///
/// This extracts the same data that the LSP's `ExportSignatureInput::from_binder`
/// extracts from a `BinderState`, but reads from the post-merge program structures.
fn build_export_signature_input(
    program: &MergedProgram,
    file: &BoundFile,
    file_idx: usize,
) -> tsz_lsp::export_signature::ExportSignatureInput {
    let mut input = tsz_lsp::export_signature::ExportSignatureInput::default();
    let file_name = &file.file_name;

    // 1. Direct exports from module_exports
    if let Some(exports) = program.module_exports.get(file_name) {
        let mut entries: Vec<_> = exports.iter().collect();
        entries.sort_by_key(|(name, _)| *name);

        for (name, sym_id) in entries {
            if let Some(symbol) = program.symbols.get(*sym_id) {
                input
                    .exports
                    .push((name.clone(), symbol.flags, symbol.is_type_only));
            }
        }
    }

    // 2. Named re-exports
    if let Some(reexports) = program.reexports.get(file_name) {
        let mut entries: Vec<_> = reexports.iter().collect();
        entries.sort_by_key(|(name, _)| *name);

        for (export_name, (source_module, original_name)) in entries {
            input.named_reexports.push((
                export_name.clone(),
                source_module.clone(),
                original_name.clone(),
            ));
        }
    }

    // 3. Wildcard re-exports (with type_only provenance)
    if let Some(wildcards) = program.wildcard_reexports.get(file_name) {
        let mut entries: Vec<(String, bool)> = wildcards.to_vec();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        input.wildcard_reexports = entries;
    }

    // 4. Global augmentations (per-file)
    {
        let mut names: Vec<&String> = file.global_augmentations.keys().collect();
        names.sort();
        for name in names {
            let count = file
                .global_augmentations
                .get(name.as_str())
                .map_or(0, Vec::len);
            input.global_augmentations.push((name.clone(), count));
        }
    }

    // 5. Module augmentations (per-file)
    {
        let mut modules: Vec<&String> = file.module_augmentations.keys().collect();
        modules.sort();
        for module in modules {
            let mut aug_names: Vec<String> = file
                .module_augmentations
                .get(module.as_str())
                .map_or_else(Vec::new, |augs| {
                    augs.iter().map(|a| a.name.clone()).collect()
                });
            aug_names.sort();
            input.module_augmentations.push((module.clone(), aug_names));
        }
    }

    // 6. Exported file-local symbols
    if let Some(file_locals) = program.file_locals.get(file_idx) {
        let mut exported_locals: Vec<_> = file_locals
            .iter()
            .filter(|(_, sym_id)| program.symbols.get(**sym_id).is_some_and(|s| s.is_exported))
            .collect();
        exported_locals.sort_by_key(|(name, _)| *name);

        for (name, sym_id) in exported_locals {
            if let Some(symbol) = program.symbols.get(*sym_id) {
                input
                    .exported_locals
                    .push((name.clone(), symbol.flags, symbol.is_type_only));
            }
        }
    }

    input
}

pub(super) fn js_file_has_ts_check_pragma(file: &BoundFile) -> bool {
    let Some(source) = file.arena.get_source_file_at(file.source_file) else {
        return false;
    };
    let text: &str = source.text.as_ref();
    // When both directives are present in leading trivia, the last one wins.
    let ts_check_pos =
        tsz_common::comments::last_ts_directive_offset_in_leading_trivia(text, "@ts-check");
    let ts_nocheck_pos =
        tsz_common::comments::last_ts_directive_offset_in_leading_trivia(text, "@ts-nocheck");
    match (ts_check_pos, ts_nocheck_pos) {
        (Some(check), Some(nocheck)) => check > nocheck,
        (Some(_), None) => true,
        _ => false,
    }
}

pub(super) fn js_file_has_ts_nocheck_pragma(file: &BoundFile) -> bool {
    let Some(source) = file.arena.get_source_file_at(file.source_file) else {
        return false;
    };
    let text: &str = source.text.as_ref();
    tsz_common::comments::source_has_ts_nocheck_directive(text)
}

/// Convert specific parser diagnostics to `TS8xxx` equivalents for JS files.
/// tsc's parser is lenient with TypeScript-only syntax in JS files, so some
/// parser errors should be converted to `TS8xxx` checker equivalents rather
/// than being suppressed entirely.
pub(super) fn convert_js_parse_diagnostics_to_ts8xxx(
    parse_diagnostics: &[ParseDiagnostic],
    file_name: &str,
    out: &mut Vec<Diagnostic>,
    source_text: Option<&str>,
) {
    for diag in parse_diagnostics {
        // TS1162 ("An object member cannot be declared optional.") ->
        // TS8009 ("The '?' modifier can only be used in TypeScript files.")
        // tsc's parser accepts `?` on object members in JS files; the checker
        // emits TS8009 only for method-like optionals (e.g., `m?()`), not for
        // property optionals (e.g., `prop?: val`). We distinguish by checking
        // if `(` follows the `?`.
        if diag.code == 1162 {
            let is_method_optional = source_text.is_some_and(|src| {
                let after_q = (diag.start + diag.length) as usize;
                // Skip whitespace after `?` and check for `(`
                src.get(after_q..).is_some_and(|s| {
                    let s = s.trim_start();
                    s.starts_with('(') || s.starts_with('<')
                })
            });
            if is_method_optional {
                out.push(Diagnostic::error(
                    file_name.to_string(),
                    diag.start,
                    diag.length,
                    "The '?' modifier can only be used in TypeScript files.".to_string(),
                    8009,
                ));
            }
        }
        // All other parser diagnostics are suppressed for JS files.
    }
}

pub(super) fn parse_diagnostic_to_checker(
    file_name: &str,
    diagnostic: &ParseDiagnostic,
) -> Diagnostic {
    let mut result = Diagnostic::error(
        file_name.to_string(),
        diagnostic.start,
        diagnostic.length,
        diagnostic.message.clone(),
        diagnostic.code,
    );
    if let Some(related) = &diagnostic.related {
        result.related_information.push(Diagnostic::related_pointer(
            related.code,
            file_name.to_string(),
            related.start,
            related.length,
            related.message.clone(),
        ));
    }
    result
}

pub(super) fn collect_no_check_parse_diagnostics_for_file(
    file_name: &str,
    arena: &NodeArena,
    source_file: NodeIndex,
    parse_diagnostics: &[ParseDiagnostic],
    options: &ResolvedCompilerOptions,
    program_has_real_syntax_errors: bool,
) -> Vec<Diagnostic> {
    let filtered_parse_diagnostics =
        filtered_parse_diagnostics(parse_diagnostics, program_has_real_syntax_errors);
    let is_js = is_js_file(Path::new(file_name));

    let mut file_diagnostics: Vec<Diagnostic> = if is_js {
        let source_text = arena
            .get_source_file_at(source_file)
            .map(|sf| sf.text.as_ref());
        let mut diags = Vec::new();
        convert_js_parse_diagnostics_to_ts8xxx(
            parse_diagnostics,
            file_name,
            &mut diags,
            source_text,
        );
        for parse_diagnostic in &filtered_parse_diagnostics {
            if is_ts1xxx_allowed_in_js(parse_diagnostic.code) {
                diags.push(parse_diagnostic_to_checker(file_name, parse_diagnostic));
            }
        }
        // tsc reports the JS-only TS8xxx grammar diagnostics from its parser,
        // so they must surface even in `--noCheck` mode where tsz otherwise
        // skips the regular checker pass (#3692). Run a minimal binder + checker
        // grammar-only walk for each JS source so type annotations, modifiers,
        // and other TypeScript-only constructs still produce TS8xxx errors.
        diags.extend(collect_js_grammar_diagnostics(
            file_name,
            arena,
            source_file,
            options,
        ));
        diags
    } else {
        filtered_parse_diagnostics
            .into_iter()
            .map(|d| parse_diagnostic_to_checker(file_name, d))
            .collect()
    };

    if is_js {
        file_diagnostics.retain(|d| !is_checker_grammar_code_suppressed_in_js(d.code));
    }

    // `@ts-expect-error` suppression applies only to semantic diagnostics; all
    // diagnostics here are syntactic, so directive suppression must not run.

    file_diagnostics
}

/// Run the checker's JS grammar pass on a parsed JS source file. The pass
/// surfaces the `TS8xxx` diagnostics tsc emits for TypeScript-only constructs in
/// JS files. Used by the `--noCheck` parse-only path to align with tsc, which
/// reports these from its parser regardless of `--noCheck`.
fn collect_js_grammar_diagnostics(
    file_name: &str,
    arena: &NodeArena,
    source_file: NodeIndex,
    options: &ResolvedCompilerOptions,
) -> Vec<Diagnostic> {
    let mut binder = tsz_binder::state::BinderState::new();
    binder.bind_source_file(arena, source_file);
    tsz_checker::run_js_grammar_pass(
        arena,
        &binder,
        source_file,
        file_name.to_string(),
        options.checker.clone(),
    )
}

pub(super) fn filtered_parse_diagnostics(
    parse_diagnostics: &[ParseDiagnostic],
    program_has_real_syntax_errors: bool,
) -> Vec<&ParseDiagnostic> {
    let has_real_syntax_error = parse_diagnostics
        .iter()
        .any(|diagnostic| is_real_syntax_error(diagnostic.code));

    // tsc emits many grammar codes via grammarErrorOnNode in the checker, which
    // suppresses them when `hasParseDiagnostics(sourceFile)` is true. tsz emits
    // them from the parser instead, so we post-filter here to match. A grammar
    // code is suppressed only when the file also carries a genuinely *suppressing*
    // parse error — one that is NOT `is_non_suppressing_parse_error`. That is the
    // single canonical model of tsc's `hasParseDiagnostics`; the checker gate sets
    // `ctx.has_syntax_parse_errors` from the identical call (`check.rs`,
    // `check_file.rs`), so this trigger cannot drift from it.
    //
    // This replaced a hand-kept complement that #16279 showed was load-bearing in
    // both directions (an unnamed non-suppressing code silently deleted every
    // grammar sibling in its file). The unclassified-code default stays
    // "suppressing" on purpose: TS1260 is neither structural nor a grammar code
    // yet tsc still suppresses siblings for it, so it must never join
    // `is_non_suppressing_parse_error`. See `filter_trigger_unification_tests` for
    // the oracle-pinned witnesses and the full history.
    let has_non_grammar_parse_error = parse_diagnostics
        .iter()
        .any(|d| !is_non_suppressing_parse_error(d.code));

    // TS1359 for `await` is parser-emitted in tsz. Keep it alongside unrelated
    // parse diagnostics (tsc does this in plain JS binder errors), but suppress
    // it for expression-recovery cases where TS1109 is the primary diagnostic.
    let has_expression_expected_parse_error = parse_diagnostics.iter().any(|d| d.code == 1109);
    parse_diagnostics
        .iter()
        .filter(|diagnostic| {
            // Existing: suppress TS1184 when real syntax errors exist
            if has_real_syntax_error && diagnostic.code == 1184 {
                return false;
            }
            // Suppress parser-emitted grammar codes that tsc would emit via
            // grammarErrorOnNode (checker-side, suppressed by hasParseDiagnostics).
            // This applies both per-file (when the current file has non-grammar errors)
            // and program-wide (when any file in the program has real syntax errors).
            // tsc's grammarErrorOnNode calls hasParseDiagnostics(sourceFile) which
            // covers program-level parse errors; we mirror that behavior here.
            if (has_non_grammar_parse_error || program_has_real_syntax_errors)
                && is_parser_grammar_code(diagnostic.code)
            {
                return false;
            }
            // Suppress TS1359 for 'await' when expression recovery already
            // reported TS1109 at the construct.
            if diagnostic.code == 1359
                && diagnostic.message.contains("'await'")
                && has_expression_expected_parse_error
            {
                return false;
            }
            true
        })
        .collect()
}

/// Parser-emitted rest-parameter grammar codes that belong to tsc's single
/// `checkGrammarParameterList` early-return chain: TS1014 (a rest parameter
/// must be last), TS1047 (a rest parameter cannot be optional), and TS1048 (a
/// rest parameter cannot have an initializer). `is_parser_grammar_code` and
/// `is_non_suppressing_parse_error` also list these three (as `const fn` match
/// arms that cannot reference a slice), so this constant is the single source
/// for *this* pass rather than a global unification of the family.
const PARAMETER_LIST_REST_GRAMMAR_CODES: [u32; 3] = [1014, 1047, 1048];

fn is_parameter_grammar_rest_code(code: u32) -> bool {
    PARAMETER_LIST_REST_GRAMMAR_CODES.contains(&code)
}

/// Drop parser-emitted rest-parameter grammar diagnostics that lost tsc's
/// one-diagnostic-per-parameter-list rule.
///
/// `suppress_spans` are half-open `[pos, boundary)` ranges recorded by the
/// checker (`check_parameter_ordering`) for each rest parameter that follows an
/// earlier checker-owned grammar error (TS1015/TS1016) in the same list. tsc's
/// `checkGrammarParameterList` returns at that earlier parameter and never
/// evaluates the rest parameter, so its TS1014/TS1047/TS1048 must not surface.
/// `boundary` stops before the parameter's type annotation and default value,
/// so a nested function's own rest-grammar diagnostic (which anchors inside
/// those subtrees) is never caught here.
pub(super) fn suppress_parameter_grammar_losers(
    diagnostics: &mut Vec<Diagnostic>,
    suppress_spans: &[(u32, u32)],
) {
    if suppress_spans.is_empty() {
        return;
    }
    diagnostics.retain(|diagnostic| {
        if !is_parameter_grammar_rest_code(diagnostic.code) {
            return true;
        }
        !suppress_spans
            .iter()
            .any(|&(start, end)| diagnostic.start >= start && diagnostic.start < end)
    });
}

#[path = "check_utils/parser_grammar_code.rs"]
mod parser_grammar_code;
use parser_grammar_code::is_parser_grammar_code;

/// Parse-error codes that tsc is known to emit for JavaScript files.
/// tsc's parser is lenient with TypeScript-only syntax in JS files and its
/// checker grammar checks (`grammarErrorOnNode`) are suppressed for TS-only
/// constructs. Only these `TS1xxx` codes are legitimately emitted for JS.
pub(super) const fn is_ts1xxx_allowed_in_js(code: u32) -> bool {
    matches!(
        code,
        1002 // Unterminated string literal
        | 1340 // Module '{0}' does not refer to a type, but is used as a type here
        | 1003 // Identifier expected
        | 1005 // "{0}" expected (missing token)
        | 1014 // A rest parameter must be last in a parameter list
        | 1016 // A required parameter cannot follow an optional parameter
        | 1064 // The return type of an async function must be 'void' or 'Promise<T>'
        | 1069 // Unexpected token; expected type parameter
        | 1092 // Type parameters cannot appear on a constructor declaration
        | 1093 // Type annotation cannot appear on a constructor declaration
        | 1098 // Type parameter list cannot be empty
        | 1100 // Invalid use of 'arguments' in strict mode
        | 1101 // 'with' statements are not allowed in strict mode
        | 1102 // SyntaxError (strict mode binding)
        | 1104 // A 'continue' statement can only be used within an enclosing iteration statement
        | 1105 // A 'break' statement can only be used within an enclosing iteration statement
        | 1107 // Jump target cannot cross function boundary
        | 1109 // Expression expected
        | 1110 // Type expected
        | 1111 // Private field must be declared in an enclosing class
        | 1139 // Can not use 'JSDoc' type in TS
        | 1141 // String literal expected
        | 1163 // A 'yield' expression is only allowed in a generator body
        // Note: TS1192 ("Module has no default export") is intentionally
        // excluded — it is a semantic checker diagnostic that tsc routes
        // through getSemanticDiagnostics, so unchecked JS files never see
        // it (issue #3693).
        | 1196 // Catch clause variable type annotation
        | 1206 // Decorators are not valid here
        | 8038 // Decorators may not appear after 'export' if they also appear before 'export'
        | 1210 // Code contained in a class is evaluated in strict mode
        | 1214 // Identifier expected; 'yield' is reserved in module strict mode
        | 1215 // Identifier expected; 'await' is a reserved word
        | 1223 // Constructor implementation is missing
        | 1228 // A type predicate is only allowed in return type position
        | 1262 // Identifier expected; 'await' at top level
        | 1273 // '@typedef' tag should either have a type annotation or be followed by '@property' or '@member' tags
        | 1274 // JSDoc '@typedef' tag should either have a type annotation or be followed by '@property' or '@member' tags
        | 1277 // 'JSDoc' types may only appear in type positions
        | 1308 // 'await' expressions are only allowed within async functions
        | 1343 | 1344 // 1343 import.meta module support; 1344 unreachable code
        | 1359 // Identifier expected; 'await' is reserved in async
        | 1360 // '@satisfies' types can only be used in type positions
        | 1382 // Unexpected token
        | 1464 // Import assertion/attribute
        | 1470 // 'import.meta' in a file building into CommonJS output
        | 1473 // Module declaration names
        | 1479 // This syntax is only allowed when 'allowImportingTsExtensions'
        | 1489 // Duplicate identifier
        | 17014 // JSX fragment has no corresponding closing tag
        | 17002 // Expected corresponding JSX closing tag for '{0}'
        | 2657 // JSX expressions must have one parent element
        | 17008 // JSX element '{0}' has no corresponding closing tag
        | 18030 // An optional chain cannot contain private identifiers
        | 18012 // '#constructor' is a reserved word
    )
}

/// Checker-emitted grammar codes outside the `TS1xxx` range that should be
/// suppressed for JS files. tsc doesn't emit these for JavaScript because
/// its parser handles the constructs leniently.
pub(super) const fn is_checker_grammar_code_suppressed_in_js(code: u32) -> bool {
    matches!(
        code, 17012 // '{0}' is not a valid meta-property for keyword '{1}'
    )
}

/// JS-only-syntactic diagnostic codes — those `TS8xxx` codes that tsc emits
/// from `getJSSyntacticDiagnosticsForFile` (see `program.ts`) for TypeScript
/// syntax appearing inside JavaScript source files. tsc routes these through
/// `getSyntacticDiagnostics` and uses them to short-circuit
/// `getSemanticDiagnostics` across the whole program in
/// `emitFilesAndReportErrors`.
///
/// This list is a stricter subset of `is_js_grammar_diagnostic`. JSDoc-related
/// `TS8xxx` codes (`TS8020`–`TS8039` save for `TS8038`) come from the checker
/// and do **not** participate in the syntactic-skip-semantic gate.
pub(super) const fn is_js_only_syntactic_diagnostic(code: u32) -> bool {
    matches!(
        code,
        8002  // 'import ... =' can only be used in TypeScript files
        | 8003  // 'export =' can only be used in TypeScript files
        | 8004  // Type parameter declarations
        | 8005  // 'implements' clauses
        | 8006  // '{0}' declarations (interface, namespace, enum, import/export type)
        | 8008  // Type aliases
        | 8009  // The '{0}' modifier
        | 8010  // Type annotations
        | 8011  // Type arguments
        | 8012  // Parameter modifiers
        | 8013  // Non-null assertions
        | 8016  // Type assertion expressions
        | 8017  // Signature declarations
        | 8037  // Type satisfaction expressions
        | 8038 // Decorators may not appear after 'export'
    )
}

/// True when a diagnostic should be retained even though the program contains
/// a JS-only-syntactic diagnostic.
///
/// In tsc, `getSyntacticDiagnostics` (which contains the JS-only-syntactic
/// codes for JS files) short-circuits `getSemanticDiagnostics` program-wide
/// in `emitFilesAndReportErrors`. The only diagnostics that survive are the
/// ones tsc routes through `getSyntacticDiagnostics` itself: structural parse
/// failures, plus the codes contributed by `getJSSyntacticDiagnosticsForFile`.
///
/// tsz's emission map straddles parser and checker — many `TS1xxx` codes that
/// `is_ts1xxx_allowed_in_js` legitimately accepts in JS files are nonetheless
/// emitted from the *checker*'s grammar phase, so tsc would route them through
/// `getSemanticDiagnostics` and drop them here. We honour that by keeping the
/// broad `TS1xxx` allow-list and then explicitly excluding the checker/binder
/// grammar checks tsc treats as semantic — break/continue (`TS1104`/`TS1105`)
/// and the cross-function jump-target check (`TS1107`).
pub(super) const fn keep_diagnostic_when_js_only_syntactic_skips_semantic(code: u32) -> bool {
    if is_checker_routed_ts1xxx_grammar(code) {
        return false;
    }
    is_real_syntax_error(code)
        || is_ts1xxx_allowed_in_js(code)
        || (code >= 8000 && code < 9000)
        // Reserved declaration-name diagnostics survive the JS-only semantic
        // skip. This axis is orthogonal to #16279's parse-error suppression
        // (which distinguishes 2427 from 2457): here both are kept because a JS
        // file's reserved-name grammar error is not a suppressible semantic
        // diagnostic, regardless of which side emits it.
        || matches!(code, 2427 | 2457)
}

/// `TS1xxx` codes that tsc routes through `getSemanticDiagnostics` rather than
/// `getSyntacticDiagnostics`, despite occupying the parser/grammar numeric
/// range.
///
/// tsc's `emitFilesAndReportErrors` runs the syntactic phase first and only
/// proceeds to the semantic phase when it produced nothing, so *whenever* a
/// syntactic gate fires, every code in this list is dropped program-wide. Both
/// of tsz's gates model that same single tsc fact and must therefore consult
/// the same list:
///
/// - [`keep_diagnostic_when_js_only_syntactic_skips_semantic`] — the JS-only
///   (`TS8xxx`) trigger.
/// - `keep_checker_diagnostic_when_program_has_real_syntax_errors` (in
///   `checker_diagnostics`) — the real-parse-failure trigger.
///
/// Numeric range is not a reliable proxy for which phase emits a diagnostic:
/// tsz's emission map straddles parser and checker, so a `TS1xxx` code emitted
/// from the checker's or binder's grammar phase would otherwise survive a gate
/// that tsc applies to it. Every entry below is verified against the pinned
/// `tsc` oracle — the construct alone reports the code, and the same construct
/// in a program that also contains a parse error reports nothing but the parse
/// error.
pub(super) const fn is_checker_routed_ts1xxx_grammar(code: u32) -> bool {
    matches!(
        code,
        // Binder strict-mode checks (`checkStrictModeEvalOrArguments`,
        // `checkStrictModeWithStatement`, `checkStrictModeLabeledStatement`)
        // push onto `file.bindDiagnostics`, which tsc surfaces through
        // `getSemanticDiagnostics`.
        1100  // Invalid use of '{0}' in strict mode.
        | 1101 // 'with' statements are not allowed in strict mode.
        | 1215 // Invalid use of '{0}'. Modules are automatically in strict mode.
        | 1344 // A label is not allowed here.
        // The break/continue family — tsc's `checkBreakOrContinueStatement`
        // emits these from the type checker.
        | 1104 // A 'continue' statement can only be used within an enclosing iteration statement.
        | 1105 // A 'break' statement can only be used within an enclosing iteration or switch statement.
        | 1107 // Jump target cannot cross function boundary.
        // Semantic checker diagnostics that merely occupy the grammar range.
        | 1064 // The return type of an async function or method must be the global Promise<T> type.
        | 1539 // A 'bigint' literal cannot be used as a property name.
        // The global-module-export family — tsc's `checkNamespaceExportDeclaration`
        // is one function reporting three codes in an early-return chain, so all
        // three share its routing and must be listed together. Membership was
        // 1315-only while 1314/1316 were unwired; wiring them without extending
        // this list makes `umd-errors.ts` report TS1314 and TS1316 in a program
        // whose real syntax errors suppress their own sibling.
        | 1314 // Global module exports may only appear in module files.
        | 1315 // Global module exports may only appear in declaration files.
        | 1316 // Global module exports may only appear at top level.
        // `in`/`out` used as a class member's own modifier (`class C { in x }`,
        // as opposed to a parameter modifier — see the parser-emitted 1274 arm
        // in `is_parser_grammar_code` above for that shape). tsc's
        // `checkGrammarModifiers` reports this from the checker; tsz's
        // `check_variance_modifier_not_on_class_member_node`
        // (`class_type_param_checks.rs`) does too, but unconditionally, with
        // no `hasParseDiagnostics`-equivalent gate. #16279 audit round 9:
        // oracle-confirmed against `typescript@7.0.2` — Direction A,
        // `class C { in x = 1; }` (and `out`) alone reports TS1274 exactly
        // once; Direction B, the same line plus an unrelated real syntax
        // error (`let x: = 1;`) elsewhere in the file drops TS1274 entirely,
        // which tsz's checker-emitted copy did not.
        | 1274 // '{0}' modifier can only appear on a type parameter of a class, interface or type alias
    )
}

/// Pre-computed merged augmentation data shared across all per-file binders.
/// Computing this once avoids `O(N_files²)` iteration in [`create_binder_from_bound_file`].
pub(super) struct MergedAugmentations {
    /// Cross-file merged module augmentations.
    ///
    /// Wrapped in `Arc` so per-file binders can share the merged map via
    /// `Arc::clone` instead of deep-cloning the entire map into each binder.
    pub module_augmentations:
        std::sync::Arc<rustc_hash::FxHashMap<String, Vec<tsz::binder::ModuleAugmentation>>>,
    /// Cross-file merged augmentation target modules.
    ///
    /// Wrapped in `Arc` so per-file binders can share the merged map via
    /// `Arc::clone` instead of deep-cloning the entire map into each binder.
    pub augmentation_target_modules:
        std::sync::Arc<rustc_hash::FxHashMap<tsz::binder::SymbolId, String>>,
    /// Cross-file merged global augmentations.
    ///
    /// Wrapped in `Arc` so per-file binders can share the merged map via
    /// `Arc::clone` instead of deep-cloning the entire map into each binder.
    pub global_augmentations:
        std::sync::Arc<rustc_hash::FxHashMap<String, Vec<tsz::binder::GlobalAugmentation>>>,
}

impl MergedAugmentations {
    /// Build merged augmentations from all files in the program. Call once per compilation.
    pub fn from_program(program: &MergedProgram) -> Self {
        let module_augmentation_keys = program
            .files
            .iter()
            .map(|file| file.module_augmentations.len())
            .sum();
        let augmentation_target_count = program
            .files
            .iter()
            .map(|file| file.augmentation_target_modules.len())
            .sum();
        let global_augmentation_keys = program
            .files
            .iter()
            .map(|file| file.global_augmentations.len())
            .sum();

        let mut module_augmentations: rustc_hash::FxHashMap<
            String,
            Vec<tsz::binder::ModuleAugmentation>,
        > = rustc_hash::FxHashMap::with_capacity_and_hasher(
            module_augmentation_keys,
            Default::default(),
        );
        let mut augmentation_target_modules: rustc_hash::FxHashMap<tsz::binder::SymbolId, String> =
            rustc_hash::FxHashMap::with_capacity_and_hasher(
                augmentation_target_count,
                Default::default(),
            );
        let mut global_augmentations: rustc_hash::FxHashMap<
            String,
            Vec<tsz::binder::GlobalAugmentation>,
        > = rustc_hash::FxHashMap::with_capacity_and_hasher(
            global_augmentation_keys,
            Default::default(),
        );

        for file in &program.files {
            for (spec, augs) in file.module_augmentations.iter() {
                module_augmentations
                    .entry(spec.clone())
                    .or_insert_with(|| Vec::with_capacity(augs.len()))
                    .extend(augs.iter().map(|aug| {
                        tsz::binder::ModuleAugmentation::with_arena(
                            aug.name.clone(),
                            aug.node,
                            Arc::clone(&file.arena),
                        )
                    }));
            }
            for (&sym_id, module_spec) in file.augmentation_target_modules.iter() {
                augmentation_target_modules.insert(sym_id, module_spec.clone());
            }
            for (name, decls) in file.global_augmentations.iter() {
                global_augmentations
                    .entry(name.clone())
                    .or_insert_with(|| Vec::with_capacity(decls.len()))
                    .extend(decls.iter().map(|aug| {
                        tsz::binder::GlobalAugmentation::with_arena(
                            aug.node,
                            Arc::clone(&file.arena),
                            aug.flags,
                        )
                    }));
            }
        }

        Self {
            module_augmentations: std::sync::Arc::new(module_augmentations),
            augmentation_target_modules: std::sync::Arc::new(augmentation_target_modules),
            global_augmentations: std::sync::Arc::new(global_augmentations),
        }
    }
}

#[allow(dead_code)] // Dead in the lib build; exercised only by tests.
pub(super) fn create_binder_from_bound_file(
    file: &BoundFile,
    program: &MergedProgram,
    file_idx: usize,
) -> BinderState {
    let augmentations = MergedAugmentations::from_program(program);
    create_binder_from_bound_file_with_augmentations(file, program, file_idx, &augmentations)
}

pub(super) fn create_binder_from_bound_file_with_augmentations(
    file: &BoundFile,
    program: &MergedProgram,
    file_idx: usize,
    augmentations: &MergedAugmentations,
) -> BinderState {
    // Share the program-wide `declaration_arenas` map via `Arc::clone` — O(1)
    // instead of iterating the entire program-wide map per file and cloning
    // matching entries. The previous filter kept ~99% of entries on large
    // projects, so the per-file filtering was almost entirely wasted work:
    // on a 6086-file project with ~100K declarations this was ~600M entry
    // visits × a `SmallVec<[Arc<NodeArena>; 1]>` clone each.
    //
    // Consumers doing point lookups (~30 call sites) see the same data via
    // `binder.declaration_arenas.get(&(sym_id, decl_idx))`. The three iter
    // consumers that needed to enumerate every `NodeIndex` for a given
    // `SymbolId` were rewritten to use the `sym_to_decl_indices` secondary
    // index (point lookup) instead of a full `declaration_arenas` scan.
    let declaration_arenas = Arc::clone(&program.declaration_arenas);
    let sym_to_decl_indices = Arc::clone(&program.sym_to_decl_indices);

    // Share the program-wide symbol_arenas via Arc::clone — O(1) instead of
    // building a per-file filtered map. The previous filter dropped entries
    // where the symbol was already local (arena pointer equal to file.arena
    // and no cross-file decl), but keeping them is harmless: consumers do
    // point lookups (`binder.symbol_arenas.get(&sym_id)`), and the checker
    // has no iter consumers of this map. Drops ~O(N_files × N_symbols)
    // iteration on large repos.
    let symbol_arenas = Arc::clone(&program.symbol_arenas);

    // Merge per-file locals with program globals via the shared helper,
    // which short-circuits to an O(1) `Arc::clone` when one side is empty
    // (common for trivial declaration files with no top-level locals).
    // The slow path pre-sizes to (locals + globals) to avoid rehashing
    // during inserts.
    let file_locals = program.build_merged_file_locals(file_idx);

    let mut binder = BinderState::from_bound_state_with_scopes_and_augmentations(
        BinderOptions::default(),
        program.symbols.clone(),
        file_locals,
        // Arc::clone is O(1) (atomic refcount bump) instead of deep-cloning the
        // underlying `FxHashMap<u32, SymbolId>`. Per-file binders consume this
        // map read-only after construction (binder mutations during checking
        // are gated by `Arc::make_mut`, which copy-on-writes safely if a
        // mutation ever does fire); sharing is safe.
        Arc::clone(&file.node_symbols),
        BinderStateScopeInputs {
            scopes: file.scopes.clone(),
            node_scope_ids: file.node_scope_ids.clone(),
            global_augmentations: augmentations.global_augmentations.clone(),
            module_augmentations: augmentations.module_augmentations.clone(),
            augmentation_target_modules: augmentations.augmentation_target_modules.clone(),
            module_exports: program.module_exports.clone(),
            module_declaration_exports_publicly: file.module_declaration_exports_publicly.clone(),
            reexports: program.reexports.clone(),
            wildcard_reexports: program.wildcard_reexports.clone(),
            symbol_arenas,
            declaration_arenas,
            sym_to_decl_indices,
            // Per-binder cross_file_node_symbols left empty intentionally.
            // The program-wide outer map is stored once on ProgramContext and
            // read via `ctx.cross_file_node_symbols_for_arena`. Cloning
            // it into every per-file binder scales outer-map allocation
            // with N² — several hundred MB on large-ts-repo.
            cross_file_node_symbols: Default::default(),
            shorthand_ambient_modules: program.shorthand_ambient_modules.clone(),
            // Per-binder `flow_nodes` is an Arc clone (atomic increment)
            // instead of a deep clone of the underlying `Vec<FlowNode>`.
            // Each `FlowNode` owns a `Vec<FlowNodeId>` antecedents, so
            // the previous deep clone was allocation-heavy; on large
            // repos it was paid ~2× per file (cross-file lookup +
            // per-file checking binder).
            flow_nodes: Arc::clone(&file.flow_nodes),
            // Arc::clone is O(1); per-file binders share the same `node_flow`
            // map as the `BoundFile` instead of deep-cloning the underlying
            // `FxHashMap<u32, FlowNodeId>`. Per-file binders consume this map
            // read-only after construction (binder mutations during checking
            // are gated by `Arc::make_mut`, which copy-on-writes safely if a
            // mutation ever does fire); sharing is safe.
            node_flow: Arc::clone(&file.node_flow),
            switch_clause_to_switch: file.switch_clause_to_switch.clone(),
            expando_properties: file.expando_properties.clone(),
            // Per-binder alias_partners left empty: every checker consumer
            // routes through `ctx.alias_partner_for` /
            // `alias_partners_contains`, which prefers the project-wide
            // `program_alias_partners` Arc installed by ProgramContext::apply_to.
            alias_partners: Default::default(),
        },
    );

    // Per-binder declared_modules left empty: every checker consumer
    // routes through `ctx.declared_modules_contains`, which prefers the
    // project-wide `global_declared_modules` index built from the skeleton.
    binder.declared_modules = Default::default();
    // Restore is_external_module from BoundFile to preserve per-file state
    binder.is_external_module = file.is_external_module;
    binder.file_features = file.file_features;
    binder.lib_symbol_reverse_remap = file.lib_symbol_reverse_remap.clone();
    // Only the file-local semantic_defs are stored on the reconstructed
    // binder. The cross-file / program-wide entries live in the shared
    // `DefinitionStore` installed by `ProgramContext::apply_to`, which gates
    // every consumer of `binder.semantic_defs` (`pre_populate_def_ids_*`,
    // `resolve_cross_batch_heritage`) behind
    // `!ctx.definition_store.is_fully_populated()`. In the parallel CLI
    // path the shared store IS fully populated, so those consumers never
    // read the binder's map — copying `program.semantic_defs` into each
    // per-file binder was pure O(N · program_defs) waste (6%+ of total
    // CPU on ts-toolbelt subsets, all of it in `SemanticDefEntry::drop`).
    // Arc::clone is O(1) (atomic refcount bump) instead of deep-cloning the
    // underlying `FxHashMap<SymbolId, SemanticDefEntry>`. The previous deep
    // clone was the largest single source of memory pressure on multi-file
    // builds (e.g., 50-70 GB total virtual on the 6086-file large-ts-repo
    // benchmark, multiplied across rayon worker threads). Cross-file lookup
    // binders only read this map (post-construction), so sharing is safe.
    binder.semantic_defs = Arc::clone(&file.semantic_defs);
    if !binder.scopes.is_empty() {
        binder.current_scope_id = tsz::binder::ScopeId(0);
    }
    // Reconstructed program binders already contain lib symbols remapped into the
    // unified symbol arena, so preserve that invariant instead of falling back to
    // legacy raw-lib lookup paths.
    binder.set_lib_symbols_merged(true);
    binder.lib_binders = program.lib_binders.clone();
    // Track lib-originating symbols so unused checking can skip them
    binder.lib_symbol_ids = program.lib_symbol_ids.clone();
    binder.lib_type_namespace = Arc::new(program.build_lib_type_namespace(file_idx));

    binder
}

/// Build a binder for cross-file symbol and type resolution.
///
/// Cross-file delegation can use entries from `CheckerContext::all_binders` for
/// full semantic type computation, not just export-table lookups. Reuse the same
/// binder construction path as a normal file check so delegated child checkers
/// have access to the owning file's symbols, declaration arenas, and augmentations.
pub(super) fn create_cross_file_lookup_binder_with_augmentations(
    file: &BoundFile,
    program: &MergedProgram,
    file_idx: usize,
    augmentations: &MergedAugmentations,
) -> BinderState {
    // Cross-file lookup binders never merge program-wide globals into their
    // `file_locals`; consumers (e.g. `resolve_in_all_binders`) only walk the
    // per-file local entries. Since #1535 made `SymbolTable` internally
    // `Arc<FxHashMap<String, SymbolId>>`, plain `.clone()` is an O(1)
    // atomic refcount bump — no fresh map allocation, no per-entry
    // `String` clones. The previous manual rebuild paid `local_count` per
    // file, multiplied by the rayon-parallel per-file binder build.
    let file_locals = program
        .file_locals
        .get(file_idx)
        .cloned()
        .unwrap_or_default();

    let mut binder = BinderState::from_bound_state_with_scopes_and_augmentations(
        BinderOptions::default(),
        program.symbols.clone(),
        file_locals,
        // Arc::clone is O(1) (atomic refcount bump) instead of deep-cloning the
        // underlying `FxHashMap<u32, SymbolId>`. Per-file binders consume this
        // map read-only after construction (binder mutations during checking
        // are gated by `Arc::make_mut`, which copy-on-writes safely if a
        // mutation ever does fire); sharing is safe.
        Arc::clone(&file.node_symbols),
        BinderStateScopeInputs {
            scopes: file.scopes.clone(),
            node_scope_ids: file.node_scope_ids.clone(),
            global_augmentations: augmentations.global_augmentations.clone(),
            module_augmentations: augmentations.module_augmentations.clone(),
            augmentation_target_modules: augmentations.augmentation_target_modules.clone(),
            // Per-binder `module_exports` is left empty intentionally.
            // The program-wide merged `module_exports` lives once on
            // `ProgramContext` as `program_module_exports` and is read via
            // `ctx.module_exports_for_module`. Cross-file lookup binders
            // used to deep-clone the entire merged map (thousands of
            // entries on large repos) into every one of N per-file
            // binders.
            module_exports: Default::default(),
            module_declaration_exports_publicly: file.module_declaration_exports_publicly.clone(),
            // Per-binder re-export maps left empty intentionally. The
            // program-wide merged re-export maps are stored once on
            // `ProgramContext` and read via `ctx.reexports_for_file` /
            // `wildcard_reexports_for_file`. Cloning them into every one
            // of N cross-file lookup binders scales the per-file setup
            // cost with total re-exports across the whole project —
            // several GB on the large-ts-repo benchmark fixture.
            reexports: Default::default(),
            wildcard_reexports: Default::default(),
            // Cross-file lookup binders only need local scopes/symbol ownership plus the
            // merged export/augmentation tables. Cloning the full cross-program arena maps
            // into every file binder makes all_binders setup scale with total declarations.
            symbol_arenas: Default::default(),
            declaration_arenas: Default::default(),
            sym_to_decl_indices: Default::default(),
            // See `create_binder_from_bound_file_with_augmentations` for
            // the rationale: the program-wide map lives on ProgramContext.
            cross_file_node_symbols: Default::default(),
            shorthand_ambient_modules: program.shorthand_ambient_modules.clone(),
            // Per-binder `flow_nodes` is an Arc clone; see
            // `create_binder_from_bound_file_with_augmentations` for
            // the rationale.
            flow_nodes: Arc::clone(&file.flow_nodes),
            // Arc::clone is O(1); cross-file lookup binders share the per-file
            // `node_flow` map instead of deep-cloning the underlying
            // `FxHashMap<u32, FlowNodeId>`. Per-file binders consume this map
            // read-only after construction (binder mutations during checking
            // are gated by `Arc::make_mut`, which copy-on-writes safely if a
            // mutation ever does fire); sharing is safe.
            node_flow: Arc::clone(&file.node_flow),
            switch_clause_to_switch: file.switch_clause_to_switch.clone(),
            expando_properties: file.expando_properties.clone(),
            // See `create_binder_from_bound_file_with_augmentations`:
            // consumers go through the project-wide accessor.
            alias_partners: Default::default(),
        },
    );

    // See `create_binder_from_bound_file_with_augmentations` for rationale.
    binder.declared_modules = Default::default();
    binder.is_external_module = file.is_external_module;
    binder.file_features = file.file_features;
    binder.lib_symbol_reverse_remap = file.lib_symbol_reverse_remap.clone();
    // See `create_binder_from_bound_file_with_augmentations` for the
    // rationale: the cross-file semantic_defs live in the shared
    // `DefinitionStore`, not here.
    // Arc::clone is O(1) (atomic refcount bump) instead of deep-cloning the
    // underlying `FxHashMap<SymbolId, SemanticDefEntry>`. The previous deep
    // clone was the largest single source of memory pressure on multi-file
    // builds (e.g., 50-70 GB total virtual on the 6086-file large-ts-repo
    // benchmark, multiplied across rayon worker threads). Cross-file lookup
    // binders only read this map (post-construction), so sharing is safe.
    binder.semantic_defs = Arc::clone(&file.semantic_defs);
    if !binder.scopes.is_empty() {
        binder.current_scope_id = tsz::binder::ScopeId(0);
    }
    binder.set_lib_symbols_merged(true);
    binder.lib_binders = program.lib_binders.clone();
    binder.lib_symbol_ids = program.lib_symbol_ids.clone();
    binder.lib_type_namespace = Arc::new(program.build_lib_type_namespace(file_idx));
    // Cross-file lookup binders deliberately keep `file_locals` per-file
    // (ownership scans iterate that map), so carry the hoisted LIB globals
    // separately. Without this, a lib-global name (e.g. an `extends Request`
    // heritage base) silently fails to resolve when a file's types are
    // computed through cross-arena delegation — making check results depend
    // on root-file order. Lib-origin names only: script-file globals (e.g. a
    // program's own `JSX` namespace) must keep resolving through the
    // per-file cross-file path with their original symbol identity, or this
    // fallback shadows/re-identifies them (multi-file JSX conformance
    // regressions). `SymbolTable` is internally `Arc`-backed, so this clone
    // is an O(1) refcount bump.
    binder.program_globals = program.lib_globals.clone();

    binder
}

// --- TS directive suppression ---
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirectiveKind {
    ExpectError,
    Ignore,
}

/// Characters that can follow `@ts-expect-error` / `@ts-ignore` in a valid directive.
const fn is_directive_separator(b: u8) -> bool {
    matches!(
        b,
        b' ' | b'\t' | b'\r' | b'\n' | 0x0B | 0x0C | b':' | b'*' | b'/'
    )
}

const fn is_directive_leading_trivia_byte(b: u8) -> bool {
    matches!(b, b'/' | b' ' | b'\t' | b'\r' | b'\n' | 0x0B | 0x0C | b'*')
}

/// Check if a comment text contains `@ts-expect-error` or `@ts-ignore`.
/// Returns the directive kind and the byte offset of the directive marker
/// within the comment text.
fn find_directive_in_text(comment: &str) -> Option<(DirectiveKind, u32)> {
    let bytes = comment.as_bytes();
    let mut pos = if comment.starts_with("//") || comment.starts_with("/*") {
        2
    } else {
        0
    };

    while pos < bytes.len() && is_directive_leading_trivia_byte(bytes[pos]) {
        pos += 1;
    }

    for (kind, text) in [
        (DirectiveKind::ExpectError, "@ts-expect-error"),
        (DirectiveKind::Ignore, "@ts-ignore"),
    ] {
        if !comment[pos..].starts_with(text) {
            continue;
        }
        let after = pos + text.len();
        if after >= comment.len() || is_directive_separator(comment.as_bytes()[after]) {
            return Some((kind, pos as u32));
        }
    }
    None
}

/// A `@ts-expect-error` or `@ts-ignore` directive found in a source file comment.
struct TsDirective {
    /// True for `@ts-expect-error`, false for `@ts-ignore`.
    is_expect_error: bool,
    /// The 0-based line number that this directive suppresses (the line after the comment).
    suppressed_line: u32,
    /// Byte offset where an unused `@ts-expect-error` diagnostic should start.
    unused_diagnostic_start: u32,
    /// Byte length for an unused `@ts-expect-error` diagnostic.
    unused_diagnostic_length: u32,
}

/// Match `commentDirectiveRegExMultiLine`
/// (`/^(?:\/|\*)*\s*@(ts-expect-error|ts-ignore)/`) against a block comment's
/// last line. tsc only registers a block comment as a directive when its final
/// line begins the directive, so `/* @ts-ignore */` qualifies but
/// `/* @ts-ignore\n...*/` (directive on a non-final line) does not.
fn directive_in_block_last_line(last_line: &str) -> Option<DirectiveKind> {
    let body = last_line
        .trim_start()
        .trim_start_matches(['/', '*'])
        .trim_start();
    for (kind, text) in [
        (DirectiveKind::ExpectError, "@ts-expect-error"),
        (DirectiveKind::Ignore, "@ts-ignore"),
    ] {
        if let Some(rest) = body.strip_prefix(text)
            && (rest.is_empty() || is_directive_separator(rest.as_bytes()[0]))
        {
            return Some(kind);
        }
    }
    None
}

/// Scan source text for `@ts-expect-error` and `@ts-ignore` directives in
/// comments. `line_map` must be built from the same `text`.
///
/// tsc registers each directive on the line of the comment's end (`_tsc.js`
/// `createCommentDirectivesMap`): single-line `//`/`///` comments match the
/// whole comment; block comments are matched against their last line alone.
fn find_ts_directives(text: &str, line_map: &LineMap) -> Vec<TsDirective> {
    let mut directives = Vec::new();

    for comment in tsz_common::comments::get_comment_ranges(text) {
        let (kind, unused_diagnostic_start) = if comment.is_multi_line {
            let close_line = line_map.line_index(comment.end.saturating_sub(1)) as usize;
            let last_line_start = line_map
                .line_start(close_line)
                .unwrap_or(comment.pos)
                .max(comment.pos);
            let last_line = &text[last_line_start as usize..comment.end as usize];
            let Some(kind) = directive_in_block_last_line(last_line) else {
                continue;
            };
            (kind, last_line_start)
        } else {
            let Some((kind, _offset)) = find_directive_in_text(comment.get_text(text)) else {
                continue;
            };
            (kind, comment.pos)
        };

        let suppressed_line = line_map.line_index(comment.end.saturating_sub(1)) + 1;

        directives.push(TsDirective {
            is_expect_error: kind == DirectiveKind::ExpectError,
            suppressed_line,
            unused_diagnostic_start,
            unused_diagnostic_length: comment.end.saturating_sub(unused_diagnostic_start),
        });
    }

    directives
}

/// Mirror tsc's `markPrecedingCommentDirectiveLine` (`_tsc.js`): starting from
/// the line above `diag_line`, walk upward skipping blank lines and
/// `//`-comment lines and return the first line that carries a directive. Stop
/// (returning `None`) at the first line with non-comment content. Block-comment
/// directive lines are caught before the content check so a `/* @ts-ignore */`
/// line still suppresses across an intervening blank line.
fn preceding_directive_line(
    diag_line: u32,
    directive_lines: &rustc_hash::FxHashSet<u32>,
    text: &str,
    line_map: &LineMap,
) -> Option<u32> {
    if diag_line == 0 {
        return None;
    }
    let mut line = diag_line - 1;
    loop {
        if directive_lines.contains(&line) {
            return Some(line);
        }
        let line_text = line_trimmed(text, line_map, line);
        if !line_text.is_empty() && !line_text.starts_with("//") {
            return None;
        }
        if line == 0 {
            return None;
        }
        line -= 1;
    }
}

/// The trimmed text of a single 0-based `line` in `text`.
fn line_trimmed<'a>(text: &'a str, line_map: &LineMap, line: u32) -> &'a str {
    let Some(start) = line_map.line_start(line as usize) else {
        return "";
    };
    let start = start as usize;
    let end = line_map
        .line_start(line as usize + 1)
        .map_or(text.len(), |next| next as usize)
        .min(text.len());
    if start >= end {
        return "";
    }
    text[start..end].trim()
}

/// Apply `@ts-expect-error` and `@ts-ignore` directive suppression to diagnostics.
///
/// 1. Finds all directive comments in the source text
/// 2. Suppresses diagnostics on the line following each directive
/// 3. Emits TS2578 for unused `@ts-expect-error` directives
pub(super) fn apply_ts_directive_suppression(
    file_name: &str,
    source_text: &str,
    diagnostics: &mut Vec<Diagnostic>,
    preserve_declaration_jsdoc_name_diagnostics: bool,
) {
    let line_map = LineMap::build(source_text);
    let directives = find_ts_directives(source_text, &line_map);
    if directives.is_empty() {
        return;
    }

    // Check for @ts-nocheck — suppresses TS2578 for unused directives.
    let has_ts_nocheck =
        tsz_common::comments::has_ts_directive_in_leading_trivia(source_text, "@ts-nocheck");

    // Lines (0-based) carrying a directive. tsc keys directives by the line of
    // the comment's end; `suppressed_line` is that line + 1.
    let directive_lines: rustc_hash::FxHashSet<u32> = directives
        .iter()
        .map(|d| d.suppressed_line.saturating_sub(1))
        .collect();

    // Directive lines that suppressed (or were touched by) at least one
    // diagnostic. An unused `@ts-expect-error` line draws TS2578.
    let mut used_directive_lines: rustc_hash::FxHashSet<u32> = rustc_hash::FxHashSet::default();

    // Suppress diagnostics whose line resolves to a preceding directive.
    //
    // tsc applies `@ts-ignore` and `@ts-expect-error` uniformly across the
    // checking pipeline, including the JSDoc `@type` lookup that runs during
    // checked-JS declaration emit. An earlier carve-out kept TS2304/TS2552
    // alive on lines containing `@type {` to align a different fingerprint,
    // but issue #3996 confirmed tsc actually suppresses those diagnostics.
    // The `preserve_declaration_jsdoc_name_diagnostics` flag is now unused
    // here; callers still pass it so the public signature stays stable while
    // any deeper revisit of declaration-emit fingerprints lands.
    let _ = preserve_declaration_jsdoc_name_diagnostics;
    // tsc's `getSyntacticDiagnostics` path bypasses directive suppression, but
    // syntactic diagnostics on the target line still make `@ts-expect-error`
    // used. Keep those diagnostics while recording the directive hit.
    diagnostics.retain(|diag| {
        let diag_line = line_map.line_index(diag.start);
        match preceding_directive_line(diag_line, &directive_lines, source_text, &line_map) {
            Some(dline) => {
                used_directive_lines.insert(dline);
                is_ts_directive_unsuppressible_syntactic(diag.code)
            }
            None => true,
        }
    });

    // Emit TS2578 for unused @ts-expect-error directives.
    //
    // tsc anchors this diagnostic at the directive comment text, not at the
    // enclosing line start. Same-line directives start at the `//` or `/*`
    // opener, while directives inside multiline block comments start at the
    // line containing the directive text.
    if !has_ts_nocheck {
        for directive in &directives {
            let dline = directive.suppressed_line.saturating_sub(1);
            if directive.is_expect_error && !used_directive_lines.contains(&dline) {
                diagnostics.push(Diagnostic::error(
                    file_name.to_string(),
                    directive.unused_diagnostic_start,
                    directive.unused_diagnostic_length,
                    "Unused '@ts-expect-error' directive.".to_string(),
                    2578,
                ));
            }
        }
    }
}

const fn is_ts_directive_unsuppressible_syntactic(code: u32) -> bool {
    is_real_syntax_error(code) || is_js_only_syntactic_diagnostic(code)
}

/// Classify a parse diagnostic code as a "real" syntax error (actual parse failure)
/// vs a grammar/semantic check emitted during parsing.
///
/// Real syntax errors indicate that the parser couldn't parse the source normally
/// and had to recover. tsc propagates `ThisNodeHasError` flags from these errors
/// to suppress cascading semantic errors like TS2304.
///
/// Grammar checks (e.g., strict mode violations, decorator errors) are emitted
/// during parsing but don't indicate parse failure — tsc still emits TS2304 for
/// undeclared names in these files.
pub(super) const fn is_real_syntax_error(code: u32) -> bool {
    matches!(
        code,
        1005  // '{0}' expected
        // Note: TS1009 (Trailing comma not allowed) is intentionally excluded.
        // It does not corrupt the AST enough to suppress semantic errors like
        // TS2304. Files with only TS1009 parse errors (e.g., `extends A,`)
        // still have valid identifiers that need name resolution.
        //
        // Note: TS1014 (A rest parameter must be last) is intentionally excluded.
        // It is a grammar check, not a structural parse failure. The AST for
        // `function f(...x, y)` is valid — both parameters are parsed correctly.
        // tsc still emits TS7019/TS7006 alongside TS1014.
        //
        // Note: TS1047 (A rest parameter cannot be optional) is excluded for the
        // same reason — the parameter is syntactically valid and should be type-checked.
        | 1036 // Statements are not allowed in ambient contexts
        | 1109 // Expression expected
        | 1110 // Type expected
        | 1126 // Unexpected end of text
        | 1127 // Invalid character
        | 1128 // Declaration or statement expected
        | 1129 // '{' or ';' expected
        | 1130 // '}' expected
        | 1131 // Property assignment expected
        | 1134 // Variable declaration expected
        | 1135 // Argument expression expected
        | 1136 // Property or signature expected
        | 1137 // Expression or comma expected
        | 1138 // Parameter declaration expected
        | 1141 // Type parameter declaration expected
        | 1146 // Declaration expected
        // Note: TS1155 ('{0}' declarations must be initialized) is intentionally
        // excluded (#16279 audit round 12 / #17253). Like TS1313 in round 10, it
        // is a `checkGrammarVariableDeclaration` grammar check on a well-formed
        // AST (`const x;` parses cleanly), not a structural parse failure — tsc
        // reports it via `grammarErrorOnNode` and still runs the file's other
        // semantic checks (oracle: `const x;` reports TS1155 AND TS7005;
        // `const x` reassigned reports TS1155 AND TS2588). It moved to
        // `is_parser_grammar_code`; leaving it here made #17251's parser wiring
        // set `has_real_syntax_errors` and delete every co-occurring diagnostic.
        | 1160 // Unterminated template literal
        | 1161 // Unterminated regular expression literal
        | 1180 // Property destructuring pattern expected
        | 1002 // Unterminated string literal
        | 1003 // Identifier expected
        | 1006 // A file cannot have a reference to itself
        | 1007 // The parser expected to find a '}'
        | 1010 // 'while' expected
        | 1011 // '(' or '<' expected
        | 1012 // '{' expected
        | 1035 // Only ambient modules can use quoted names
        // Note: TS1101 ('with' statements are not allowed in strict mode) is intentionally
        // excluded. It is a grammar check, not a structural parse failure. The parser
        // accepts the with-statement and produces a valid AST; tsc still emits semantic
        // errors like TS2410 alongside TS1101.
        | 1103 // A character literal must contain exactly one character
        | 1121 // Octal literals are not allowed in strict mode
        | 1124 // Digit expected
        | 1144 // '{' or ';' expected
        | 1145 // '{' or JSX element expected
        | 1147 // Import declarations in a namespace cannot reference a module
        | 1164 // Computed property names are not allowed in enums
        | 1185 // Merge conflict marker encountered
        // Note: TS1191 (An import declaration cannot have modifiers) is intentionally
        // excluded. It is a grammar constraint error, not a structural parse failure.
        // The AST is fully valid — the import is parsed correctly. tsc still emits
        // semantic errors like TS2323 alongside TS1191.
        //
        // Note: TS1313 (The body of an 'if' statement cannot be the empty
        // statement) is intentionally excluded (#16279 follow-up). It was
        // previously listed here under an unrelated, non-existent message
        // ("'else' is not allowed after rest element" is not a tsc diagnostic
        // at all) — a stale mislabel, not a real classification decision. The
        // `then_statement` AST node tsz emits it on
        // (`state_declarations_exports.rs`) is a well-formed EMPTY_STATEMENT,
        // not an error-recovery placeholder, and oracle-verified against
        // `typescript@7.0.2`: `if (true); undeclaredName;` reports both
        // TS1313 and TS2304 — TS1313 does not suppress cascading semantic
        // diagnostics the way a genuine structural parse failure does. It now
        // lives in `is_parser_grammar_code` instead.
        | 1351 // An identifier or keyword cannot immediately follow a numeric literal
        | 1357 // A default clause cannot appear more than once
        | 1378 // Top-level 'for await' loops are only allowed...
        | 1432 // 'await' expressions are only allowed within async functions
        | 1434 // Top-level 'await' expressions are only allowed...
        | 1389 // '{0}' is not allowed as a variable declaration name
        | 1382 // Unexpected token. Did you mean `{'>'}` or `&gt;`? (JSX)
        | 1438 // Interface must be given a name (recovery creates invalid expression statements)
        | 1442 // Identifier or expression expected (TS-only construct in JS)
        | 1477 // Member must have an initializer
        // The private-identifier-in-a-value-position family. All three are tsc
        // *parser* diagnostics (`parseErrorAtRange`) that land in
        // `sourceFile.parseDiagnostics` and drive `hasParseDiagnostics()`, so tsc
        // short-circuits the program's whole semantic phase: `o?.a.#b` stops
        // surfacing the receiver's TS2532/TS18048, and a `const #x` / `function
        // f(#x)` file stops surfacing trailing TS2322/TS2304. tsz emits all three
        // from the parser too (`parse_error_at`), so they must be classified
        // together here (TS18029/TS18009 were previously missing — issue #16817).
        // Distinct from the checker-routed private-identifier grammar codes
        // TS18016/TS18024, which tsc raises via `grammarErrorOnNode` and are
        // suppressed *by* — not triggers of — a parse error, so they live in
        // `is_parser_grammar_code`. Oracle evidence: #16817 and the #16279 audit
        // round 6, which found all of these "survive Direction B" (tsc keeps them
        // alongside an unrelated real syntax error — the signature of a genuine
        // parser diagnostic rather than a checker grammar check).
        | 18009 // Private identifiers cannot be used as parameters
        | 18029 // Private identifiers are not allowed in variable declarations
        | 18030 // An optional chain cannot contain private identifiers
    )
}

/// Classify a parse diagnostic as a **structural** parse error — one that causes
/// actual AST malformation and error recovery, leading to cascading semantic errors.
///
/// This is a more restrictive subset of `is_real_syntax_error`. It excludes:
/// - Grammar checks that don't affect AST structure (strict mode, trailing commas)
/// - Contextual restrictions that don't cause parse recovery (import modifiers, etc.)
///
/// Used for the cascading suppression heuristic: semantic errors near structural
/// parse failures are likely artifacts of error recovery and should be suppressed.
pub(super) const fn is_structural_parse_error(code: u32) -> bool {
    matches!(
        code,
        1002  // Unterminated string literal
        | 1003 // Identifier expected
        | 1005 // '{0}' expected (missing token)
        | 1007 // The parser expected to find a '}'
        | 1010 // 'while' expected
        | 1011 // '(' or '<' expected
        | 1012 // '{' expected
        | 1109 // Expression expected
        | 1110 // Type expected
        | 1124 // Digit expected
        | 1126 // Unexpected end of text
        | 1127 // Invalid character
        | 1128 // Declaration or statement expected
        | 1129 // '{' or ';' expected
        | 1130 // '}' expected
        | 1131 // Property assignment expected
        | 1134 // Variable declaration expected
        | 1135 // Argument expression expected
        | 1136 // Property or signature expected
        | 1137 // Expression or comma expected
        | 1138 // Parameter declaration expected
        | 1141 // Type parameter declaration expected
        | 1144 // '{' or ';' expected
        | 1145 // '{' or JSX element expected
        | 1146 // Declaration expected
        // TS1155 is intentionally excluded — see the matching note in
        // `is_real_syntax_error` above. It is a grammar check on a well-formed
        // AST and moved to `is_parser_grammar_code` (#16279 audit round 12 /
        // #17253).
        | 1160 // Unterminated template literal
        | 1161 // Unterminated regular expression literal
        | 1180 // Property destructuring pattern expected
        | 1185 // Merge conflict marker encountered
        // TS1313 is intentionally excluded — see the matching note in
        // `is_real_syntax_error` above. It moved to `is_parser_grammar_code`.
        | 1351 // An identifier or keyword cannot immediately follow a numeric literal
        | 1382 // Unexpected token in JSX
        | 1441 // Cannot start a function call in a type annotation
        | 1442 // Identifier or expression expected
        | 1068 // Unexpected token. A constructor, method, accessor, or property was expected.
    )
}

/// Parse error codes that should NOT cause `has_syntax_parse_errors` to suppress
/// semantic diagnostics like TS7006/TS7019 (implicit any).
///
/// These are grammar/constraint errors on otherwise well-formed AST nodes:
/// - TS1009: Trailing comma not allowed
/// - TS1014: A rest parameter must be last in a parameter list
/// - TS1047: A rest parameter cannot be optional
/// - TS1048: A rest parameter cannot have an initializer
/// - TS1096: An index signature must have exactly one parameter
/// - TS1185: Merge conflict marker encountered
/// - TS1214: Identifier expected (strict mode reserved word)
/// - TS1262: 'await' at top level
/// - TS1359: 'await' in async context
///
/// tsc emits TS7006/TS7019 even in the presence of these errors because
/// the parameter identity (name) is still valid and can be type-checked.
///
/// TS1096 belongs here because tsc reports it from
/// `checkGrammarIndexSignatureParameters` at CHECK time on a well-formed index
/// signature (the multi-parameter AST parses fine), so it never participates in
/// tsc's `hasParseDiagnostics` suppression. tsz emits it during parsing instead,
/// so without this entry a stray `[a, b]` would set `has_syntax_parse_errors` and
/// suppress unrelated check-time grammar diagnostics elsewhere in the file
/// (e.g. TS1036 in an ambient namespace, and nearby TS1021).
///
/// # The regular-expression grammar family
///
/// The whole `TS1499..=TS1538` regex band belongs here, for one structural
/// reason: tsc never puts a regex grammar diagnostic in `parseDiagnostics` at
/// all. Its regex validation runs from the checker, which re-scans the literal
/// through `scanner.scanRange`, so a malformed pattern cannot participate in
/// `hasParseDiagnostics()` suppression. tsz instead validates the pattern in
/// `state_expressions_literals_regex.rs` during parsing, which places the same
/// diagnostics in `parse_diagnostics` — so every code that walk can emit has to
/// be excluded here or it silently suppresses the rest of the file.
///
/// That band is matched as a **range, not an enumeration**, and the distinction
/// is load-bearing. Enumerating it one code at a time made the predicate drift
/// out of step with this very doc comment three separate times, each time
/// landing a live whole-file suppression bug on `main`: `TS1511` (`\q` outside a
/// character class), then `TS1501`/`TS1504`/`TS1509` (subpattern modifier
/// groups), then `TS1514`/`TS1515` (capturing group names). The codes an
/// enumeration was missing were exactly `1503`, `1513`, `1514`, `1515`, `1518`,
/// `1521` and `1532` — every code the family had tripped on plus every one still
/// queued to be wired.
///
/// The range is safe to state as a range because it is upstream's own
/// allocation, not a shape tsz imposes: the diagnostics table in
/// `tsz_common::diagnostics` is generated verbatim from TypeScript's
/// `diagnosticMessages.json`, and every row from `1499` (`Unknown regular
/// expression flag.`) through `1538` (`Unicode escape sequences are only
/// available when the Unicode (u) flag …`) is a regular-expression grammar
/// message, with `1498` (`Invalid syntax in decorator.`) and `1539` (`A 'bigint'
/// literal cannot be used as a property name.`) as non-regex neighbours on
/// either side. `regex_grammar_suppression_tests` pins both the band and those
/// two boundary codes, so a future upstream sync that allocates a non-regex
/// message inside the band fails a test rather than silently widening this
/// predicate.
///
/// `TS1487` (`Octal escape sequences are not allowed…`) sits outside the band
/// and stays enumerated below: it is shared with string-literal escapes and is
/// not part of upstream's contiguous regex allocation.
///
/// That suppression is not limited to the check-time `TS1xxx` grammar family named
/// above. `has_syntax_parse_errors` is read at 30+ sites in `tsz-checker`,
/// including `error_reporter/name_resolution.rs` (TS2304 and its suggestions),
/// `error_reporter/properties.rs` (TS2339), `query_boundaries/`
/// `assignability_suppression.rs`, `checkers/jsx/orchestration/resolution.rs`,
/// and the `noImplicitAny` circularity checkers — so a single bad regex
/// literal deleted unrelated real diagnostics from the entire file.
///
/// Every code below was pinned against `typescript@7.0.2` with a fixture
/// pairing the regex literal with TS1039, TS2304, TS2322 and TS2339: tsc
/// reports all four companions alongside the regex diagnostic in every case.
/// The same fixture with a genuine structural error (`const broken = ;`,
/// TS1109) drops all four, which is what makes the probe discriminating rather
/// than vacuous.
///
/// Codes the regex validator shares with non-regex contexts (TS1005, TS1125,
/// TS1161, TS1198) are deliberately NOT here: this predicate is keyed on the
/// code, not the emitting site, and those are real parse failures elsewhere.
///
/// # Containment with `is_parser_grammar_code`
///
/// The regex band above is one instance of a general rule, and the general rule
/// is now stated directly: **every code `is_parser_grammar_code` accepts is
/// non-suppressing.** That predicate means "tsc emits this from the checker via
/// `grammarErrorOnNode`; tsz emits it from the parser instead". A checker-raised
/// diagnostic is never in tsc's `sourceFile.parseDiagnostics`, so
/// `hasParseDiagnostics(sourceFile)` stays `false` and every other checker
/// grammar check in the file still runs.
///
/// Before that containment was stated, the two lists overlapped on only five
/// codes (`1014`, `1047`, `1048`, `1096`, `1191`) out of ~70, so ~65
/// parser-emitted grammar codes set `has_syntax_parse_errors` and deleted the
/// rest of their file's checker diagnostics. Pinned against `typescript@7.0.2`
/// with each grammar witness paired against a TS1308 (`await` outside an async
/// function) companion, tsc reported the companion in **every** probed case —
/// including the structurally suspicious "list is empty" members (`1097`,
/// `1098`, `1099`, `1123`, `1182`) where a malformed AST might plausibly have
/// justified suppression. See `parser_grammar_non_suppressing_tests`.
pub(super) const fn is_non_suppressing_parse_error(code: u32) -> bool {
    // The regular-expression grammar band, matched as a range rather than an
    // enumeration. Every code from 1499 (`Unknown regular expression flag.`)
    // through 1538 (`Unicode escape sequences are only available when the
    // Unicode (u) flag …`) is a regex grammar message in upstream's own
    // diagnostics table, and tsc reports all of them from the checker. Matching
    // the band keeps a newly wired regex diagnostic from silently suppressing
    // the rest of the file before anyone remembers to add a line here.
    if matches!(code, 1499..=1538) {
        return true;
    }

    // Everything `is_parser_grammar_code` covers, by construction. That
    // predicate's contract is "tsc emits this from the checker via
    // `grammarErrorOnNode`, tsz emits it from the parser instead" — and a code
    // tsc raises from the checker is never in `sourceFile.parseDiagnostics`, so
    // `hasParseDiagnostics(sourceFile)` stays false and tsc's other checker
    // grammar checks in the same file still run. Anything in that list must
    // therefore be non-suppressing here too; the two predicates were disagreeing
    // on ~65 codes, each of which silently deleted the rest of its file's
    // checker grammar diagnostics.
    if is_parser_grammar_code(code) {
        return true;
    }

    matches!(
        code,
        1009  // Trailing comma not allowed
            | 1014 // A rest parameter must be last in a parameter list
            | 1047 // A rest parameter cannot be optional
            | 1048 // A rest parameter cannot have an initializer
            | 1096 // An index signature must have exactly one parameter (check-time grammar in tsc)
            | 1185 // Merge conflict marker
            | 1191 // An import declaration cannot have modifiers (grammar constraint, AST is valid)
            | 1214 // Identifier expected (strict mode reserved word)
            | 1262 // 'await' at top level
            | 1359 // 'await' in async context
            | 1492 // 'using' declarations may not have binding patterns (grammar constraint, AST is valid)
            | 1487 // Octal escape sequences are not allowed (regex `\0`-prefixed decimal escape,
                   // AST is valid). Outside the contiguous regex band handled above — shared
                   // with string-literal escapes — so it stays enumerated here.
            | 17019 // '?' at end of type is not valid TS syntax (parser recovers valid AST)
            | 17020 // '?' at start of type is not valid TS syntax (parser recovers valid AST)
            | 18012 // '#constructor' is a reserved word. tsc raises this from the
                    // binder/checker, so it is never in `sourceFile.parseDiagnostics`;
                    // tsz emits it from the parser instead. Oracle-pinned against
                    // `typescript@7.0.2`: `class D { #constructor = 1; }` reports
                    // TS18012 *alongside* a sibling getter's TS1054 and a semantic
                    // TS2304 in the same file (plainJSBinderErrors.ts does the same
                    // with TS1101/TS1359), so it must not set has_syntax_parse_errors.
    )
}

/// Semantic diagnostic codes (>= 2000) that tsc allows through for plain JS files.
/// Mirrors tsc's `plainJSErrors` set from `program.ts`.
pub(super) const fn is_plain_js_allowed_code(code: u32) -> bool {
    matches!(
        code,
        2451  // Cannot redeclare block-scoped variable '{0}'
        | 2492 // Cannot redeclare identifier '{0}' in catch clause
        | 2528 // A module cannot have multiple default exports
        | 2752 // The first export default is here
        | 2753 // Another export default is here
        | 2801 // This condition will always return true since this '{0}' is always defined
        | 2803 // Cannot assign to private method '{0}'. Private methods are not writable
        | 2839 // This condition will always return '{0}' since JS compares objects by reference
        | 2845 // This condition will always return '{0}'
        | 18013 // Property '{0}' is not accessible outside class '{1}' (private identifier)
    )
}

#[cfg(test)]
#[path = "check_utils/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "check_utils/heritage_clause_tests.rs"]
mod heritage_clause_tests;

#[cfg(test)]
#[path = "check_utils/rest_parameter_grammar_tests.rs"]
mod rest_parameter_grammar_tests;

#[cfg(test)]
#[path = "check_utils/regex_grammar_suppression_tests.rs"]
mod regex_grammar_suppression_tests;

#[cfg(test)]
#[path = "check_utils/for_in_of_single_declaration_grammar_tests.rs"]
mod for_in_of_single_declaration_grammar_tests;

#[cfg(test)]
#[path = "check_utils/audit_round_3_grammar_tests.rs"]
mod audit_round_3_grammar_tests;

#[cfg(test)]
#[path = "check_utils/audit_round_9_grammar_tests.rs"]
mod audit_round_9_grammar_tests;

#[cfg(test)]
#[path = "check_utils/parser_grammar_non_suppressing_tests.rs"]
mod parser_grammar_non_suppressing_tests;

#[cfg(test)]
#[path = "check_utils/for_in_using_declaration_grammar_tests.rs"]
mod for_in_using_declaration_grammar_tests;

#[cfg(test)]
#[path = "check_utils/using_declaration_binding_pattern_grammar_tests.rs"]
mod using_declaration_binding_pattern_grammar_tests;

#[cfg(test)]
#[path = "check_utils/filter_trigger_unification_tests.rs"]
mod filter_trigger_unification_tests;

#[cfg(test)]
#[path = "check_utils/class_static_block_grammar_tests.rs"]
mod class_static_block_grammar_tests;

#[cfg(test)]
#[path = "check_utils/import_call_type_arguments_grammar_tests.rs"]
mod import_call_type_arguments_grammar_tests;

#[cfg(test)]
#[path = "check_utils/meta_property_grammar_tests.rs"]
mod meta_property_grammar_tests;

#[cfg(test)]
#[path = "check_utils/jsx_comma_operator_grammar_tests.rs"]
mod jsx_comma_operator_grammar_tests;

#[cfg(test)]
#[path = "check_utils/jsdoc_star_type_grammar_tests.rs"]
mod jsdoc_star_type_grammar_tests;

#[cfg(test)]
#[path = "check_utils/private_identifier_parse_error_suppression_tests.rs"]
mod private_identifier_parse_error_suppression_tests;

#[cfg(test)]
#[path = "check_utils/variable_declaration_initializer_grammar_tests.rs"]
mod variable_declaration_initializer_grammar_tests;
