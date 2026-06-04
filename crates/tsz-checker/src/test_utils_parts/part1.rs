use crate::context::{CheckerOptions, LibContext};

use crate::diagnostics::Diagnostic;

use crate::query_boundaries::common::TypeInterner;

use crate::state::CheckerState;

use rustc_hash::FxHashSet;

use std::path::{Path, PathBuf};

use std::sync::Arc;

use tsz_binder::BinderState;

use tsz_binder::lib_loader::LibFile;

use tsz_common::position::LineMap;

use tsz_parser::parser::ParserState;

#[cfg(test)]
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

/// Parse, bind, and type-check a TypeScript source string, returning all diagnostics.
///
/// Uses the given `CheckerOptions` and file name. Calls `set_lib_contexts(Vec::new())`
/// so tests run without lib definitions (preventing spurious TS2318 errors).
pub fn check_source(source: &str, file_name: &str, options: CheckerOptions) -> Vec<Diagnostic> {
    check_source_with_file_is_esm(source, file_name, options, None)
}

/// Parse, bind, and type-check a TypeScript source string, returning every
/// recovery fallback site recorded by [`crate::context::CheckerContext::recover_any`]
/// during the check. Each entry is `(node_index, reason)`, sorted by node index.
pub fn check_source_recovery_sites(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
) -> Vec<(u32, crate::recovery::RecoveryReason)> {
    with_checked_source(source, file_name, options, None, |checker| {
        let mut snapshot: Vec<_> = checker
            .ctx
            .recovery_sites_snapshot()
            .into_iter()
            .map(|(idx, reason)| (idx.0, reason))
            .collect();
        snapshot.sort_by_key(|(idx, _)| *idx);
        snapshot
    })
}

/// Parse, bind, and type-check a source string, then return type-node
/// resolution entry counts for type literals that contain computed members.
#[cfg(test)]
pub fn check_computed_type_argument_resolution_counts(source: &str) -> Vec<u32> {
    with_checked_source(
        source,
        "test.ts",
        CheckerOptions::default(),
        None,
        |checker| {
            checker
                .ctx
                .arena
                .nodes
                .iter()
                .enumerate()
                .filter_map(|(raw_idx, node)| {
                    if node.kind == syntax_kind_ext::TYPE_LITERAL
                        && type_literal_has_computed_member(checker, node)
                    {
                        let idx = NodeIndex(raw_idx as u32);
                        Some(checker.ctx.type_node_resolution_count_for_test(idx))
                    } else {
                        None
                    }
                })
                .collect()
        },
    )
}

#[cfg(test)]
fn type_literal_has_computed_member(
    checker: &CheckerState<'_>,
    node: &tsz_parser::parser::node::Node,
) -> bool {
    checker
        .ctx
        .arena
        .get_type_literal(node)
        .is_some_and(|literal| {
            literal.members.nodes.iter().any(|&member_idx| {
                checker
                    .ctx
                    .arena
                    .get(member_idx)
                    .and_then(|member| checker.ctx.arena.get_signature(member))
                    .and_then(|signature| checker.ctx.arena.get(signature.name))
                    .is_some_and(|name| name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
            })
        })
}

/// Parse, bind, and type-check a source string with no lib contexts, source
/// file test pragmas enabled, and an explicit Node module file-format
/// classification.
pub fn check_source_with_file_is_esm(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
    file_is_esm: Option<bool>,
) -> Vec<Diagnostic> {
    with_checked_source(source, file_name, options, file_is_esm, |checker| {
        checker.ctx.diagnostics.clone()
    })
}

/// Run the canonical test parse → bind → check pipeline and hand the
/// post-check `CheckerState` to `extract`. Used by the public test helpers
/// to share one pipeline body so any change to setup (default options,
/// pragmas, lib contexts) applies uniformly.
fn with_checked_source<R>(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
    file_is_esm: Option<bool>,
    extract: impl FnOnce(&CheckerState<'_>) -> R,
) -> R {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let source_file = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), source_file);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );
    checker.enable_source_file_test_pragmas();
    checker.ctx.set_lib_contexts(Vec::new());
    checker.ctx.file_is_esm = file_is_esm;
    #[cfg(test)]
    checker.ctx.reset_type_node_resolution_counts_for_test();
    checker.check_source_file(source_file);
    extract(&checker)
}

/// Parse, bind, and type-check a TypeScript source string with default options.
///
/// Convenience wrapper around [`check_source`] using `"test.ts"` and default options.
pub fn check_source_diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source(source, "test.ts", CheckerOptions::default())
}

/// Parse, bind, and type-check a JavaScript source string.
///
/// Uses `"test.js"` filename and enables `check_js`.
pub fn check_js_source_diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            ..CheckerOptions::default()
        },
    )
}

/// Types that expose a diagnostic code for code-only test assertions.
pub trait HasDiagnosticCode {
    fn diagnostic_code(&self) -> u32;
}

impl HasDiagnosticCode for Diagnostic {
    fn diagnostic_code(&self) -> u32 {
        self.code
    }
}

impl<T: HasDiagnosticCode + ?Sized> HasDiagnosticCode for &T {
    fn diagnostic_code(&self) -> u32 {
        (*self).diagnostic_code()
    }
}

impl<T> HasDiagnosticCode for (u32, T) {
    fn diagnostic_code(&self) -> u32 {
        self.0
    }
}

/// Types that expose both diagnostic code and message text.
pub trait HasDiagnosticMessage: HasDiagnosticCode {
    fn diagnostic_message(&self) -> &str;
}

impl HasDiagnosticMessage for Diagnostic {
    fn diagnostic_message(&self) -> &str {
        &self.message_text
    }
}

impl<T: HasDiagnosticMessage + ?Sized> HasDiagnosticMessage for &T {
    fn diagnostic_message(&self) -> &str {
        (*self).diagnostic_message()
    }
}

impl HasDiagnosticMessage for (u32, String) {
    fn diagnostic_message(&self) -> &str {
        &self.1
    }
}

impl HasDiagnosticMessage for (u32, &str) {
    fn diagnostic_message(&self) -> &str {
        self.1
    }
}

/// Types that expose a diagnostic start byte offset for location-aware
/// assertions.
pub trait HasDiagnosticStart {
    fn diagnostic_start(&self) -> u32;
}

impl HasDiagnosticStart for Diagnostic {
    fn diagnostic_start(&self) -> u32 {
        self.start
    }
}

impl<T: HasDiagnosticStart + ?Sized> HasDiagnosticStart for &T {
    fn diagnostic_start(&self) -> u32 {
        (*self).diagnostic_start()
    }
}

/// Compute the 1-indexed (line, column) of a byte offset in `source`.
///
/// Lines and columns are 1-indexed; the column count is in UTF-16 code units
/// to match the tsc / LSP fingerprint convention. Built on
/// [`tsz_common::position::LineMap`], so callers do not need to roll their
/// own offset → line/column conversion in tests.
///
/// Offsets past the end of `source` clamp to the last position (same
/// semantics as `LineMap::offset_to_position`).
#[must_use]
pub fn line_column_for_offset(source: &str, offset: u32) -> (u32, u32) {
    let map = LineMap::build(source);
    let pos = map.offset_to_position(offset, source);
    (pos.line.saturating_add(1), pos.character.saturating_add(1))
}

/// 1-indexed `(line, column)` of a diagnostic's start position in `source`.
///
/// Convenience wrapper around [`line_column_for_offset`]; accepts any value
/// that exposes a diagnostic start offset via [`HasDiagnosticStart`].
#[must_use]
pub fn diagnostic_line_column<T: HasDiagnosticStart>(source: &str, diagnostic: &T) -> (u32, u32) {
    line_column_for_offset(source, diagnostic.diagnostic_start())
}

/// Structural fingerprint for a single diagnostic: a code plus optional
/// 1-indexed location, structural message fragment, and minimum
/// `related_information` arity.
///
/// Used by [`assert_diagnostic_shape`] / [`assert_diagnostic_shapes`] to
/// upgrade tests from "this code appears somewhere" to "this code appears
/// at this `(line, column)` with this structural message fragment and at
/// least this many related notes."
///
/// **Anti-hardcoding (CLAUDE.md §25):** prefer template fragments
/// (e.g. `" is not assignable to the same property in base type "`) over
/// fragments that include user-chosen identifier names, alias names, or
/// rendered identifier spellings. The matcher does not enforce this — the
/// test author owns the choice — but tests that fingerprint user-chosen
/// names will lock onto the spelling of a single repro and regress when an
/// equivalent shape uses different names.
#[derive(Debug, Clone)]
pub struct DiagnosticShape {
    /// Required diagnostic code (e.g. `2416`).
    pub code: u32,
    /// Optional 1-indexed line of the diagnostic start.
    pub line: Option<u32>,
    /// Optional 1-indexed column (UTF-16 code units) of the diagnostic start.
    pub column: Option<u32>,
    /// Optional structural fragment of the message text. The matcher uses
    /// `str::contains`; pass message-template fragments rather than full
    /// rendered messages.
    pub message_fragment: Option<&'static str>,
    /// Optional minimum number of `related_information` entries.
    pub related_min: Option<usize>,
}

impl DiagnosticShape {
    /// Begin a shape that requires only the diagnostic `code`.
    #[must_use]
    pub const fn code(code: u32) -> Self {
        Self {
            code,
            line: None,
            column: None,
            message_fragment: None,
            related_min: None,
        }
    }

    /// Pin the diagnostic start to 1-indexed `(line, column)`.
    #[must_use]
    pub const fn at(mut self, line: u32, column: u32) -> Self {
        self.line = Some(line);
        self.column = Some(column);
        self
    }

    /// Require the message text to contain `fragment`. Prefer
    /// template-shaped fragments over user-chosen identifier names.
    #[must_use]
    pub const fn with_message_fragment(mut self, fragment: &'static str) -> Self {
        self.message_fragment = Some(fragment);
        self
    }

    /// Require at least `n` `related_information` entries.
    #[must_use]
    pub const fn with_related_min(mut self, n: usize) -> Self {
        self.related_min = Some(n);
        self
    }
}

/// Check whether `diagnostic` matches `shape`. Returns `Ok(())` on a full
/// match, or `Err(reason)` describing the first failing constraint (used
/// directly in panic messages to triage near-misses).
fn shape_match(
    source: &str,
    diagnostic: &Diagnostic,
    shape: &DiagnosticShape,
) -> Result<(), String> {
    if diagnostic.code != shape.code {
        return Err(format!(
            "code TS{} (expected TS{})",
            diagnostic.code, shape.code
        ));
    }
    if shape.line.is_some() || shape.column.is_some() {
        let (line, column) = diagnostic_line_column(source, diagnostic);
        if let Some(expected) = shape.line
            && expected != line
        {
            return Err(format!("line {line} (expected {expected})"));
        }
        if let Some(expected) = shape.column
            && expected != column
        {
            return Err(format!("column {column} (expected {expected})"));
        }
    }
    if let Some(fragment) = shape.message_fragment
        && !diagnostic.message_text.contains(fragment)
    {
        return Err(format!("message missing fragment {fragment:?}"));
    }
    if let Some(expected) = shape.related_min
        && diagnostic.related_information.len() < expected
    {
        return Err(format!(
            "related_information.len() = {} (expected >= {expected})",
            diagnostic.related_information.len(),
        ));
    }
    Ok(())
}

fn format_diagnostic_for_panic(source: &str, diagnostic: &Diagnostic) -> String {
    let (line, column) = diagnostic_line_column(source, diagnostic);
    format!(
        "TS{} at {}:{} ({}+{}) in {:?}: {:?} [related={}]",
        diagnostic.code,
        line,
        column,
        diagnostic.start,
        diagnostic.length,
        diagnostic.file,
        diagnostic.message_text,
        diagnostic.related_information.len(),
    )
}

/// Assert that at least one diagnostic in `diagnostics` matches `shape`.
///
/// Returns a reference to the first matching diagnostic so callers can do
/// follow-up assertions if needed. On failure, the panic message lists every
/// diagnostic with the same code and the precise reason each was rejected
/// (wrong line, wrong column, missing message fragment, …), which is the
/// information that `assert!(codes.contains(&NNNN), ...)` swallows.
pub fn assert_diagnostic_shape<'a>(
    source: &str,
    diagnostics: &'a [Diagnostic],
    shape: &DiagnosticShape,
) -> &'a Diagnostic {
    let mut near_misses: Vec<(&Diagnostic, String)> = Vec::new();
    for diagnostic in diagnostics {
        let reason = match shape_match(source, diagnostic, shape) {
            Ok(()) => return diagnostic,
            Err(reason) => reason,
        };
        if diagnostic.code == shape.code {
            near_misses.push((diagnostic, reason));
        }
    }

    let all_codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    let detail = if near_misses.is_empty() {
        "    (no diagnostic with the expected code was emitted)".to_string()
    } else {
        near_misses
            .iter()
            .map(|(d, reason)| {
                format!(
                    "    - {}\n      reason: {reason}",
                    format_diagnostic_for_panic(source, d),
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    panic!(
        "Expected diagnostic shape {shape:?} to match at least one diagnostic.\n\
         Emitted codes: {all_codes:?}\n\
         Candidates with the expected code:\n{detail}",
    );
}

/// Assert that every shape in `shapes` matches at least one diagnostic.
///
/// Extra diagnostics are allowed; this is a structural-presence check, not
/// an exact-list match. Use [`assert_diagnostic_shapes_exactly`] when the
/// test wants to pin the full diagnostic set.
pub fn assert_diagnostic_shapes(
    source: &str,
    diagnostics: &[Diagnostic],
    shapes: &[DiagnosticShape],
) {
    for shape in shapes {
        assert_diagnostic_shape(source, diagnostics, shape);
    }
}

/// Assert that `diagnostics` contains *exactly* one match per shape and no
/// other diagnostics. Order-insensitive: each diagnostic is matched against
/// the first unsatisfied shape it fits.
///
/// Use this for tests that want to lock the full emitted set, not just
/// presence of a few key diagnostics.
pub fn assert_diagnostic_shapes_exactly(
    source: &str,
    diagnostics: &[Diagnostic],
    shapes: &[DiagnosticShape],
) {
    let mut consumed = vec![false; shapes.len()];
    let mut unmatched: Vec<&Diagnostic> = Vec::new();
    for diagnostic in diagnostics {
        let slot = shapes.iter().enumerate().position(|(idx, shape)| {
            !consumed[idx] && shape_match(source, diagnostic, shape).is_ok()
        });
        match slot {
            Some(idx) => consumed[idx] = true,
            None => unmatched.push(diagnostic),
        }
    }
    let missing: Vec<&DiagnosticShape> = shapes
        .iter()
        .zip(&consumed)
        .filter_map(|(shape, used)| (!used).then_some(shape))
        .collect();
    if !missing.is_empty() || !unmatched.is_empty() {
        let unmatched_lines: Vec<String> = unmatched
            .iter()
            .map(|d| format!("    - {}", format_diagnostic_for_panic(source, d)))
            .collect();
        let missing_lines: Vec<String> = missing.iter().map(|s| format!("    - {s:?}")).collect();
        panic!(
            "Diagnostic set did not match the expected shapes exactly.\n\
             Unmatched shapes:\n{}\n\
             Unmatched diagnostics:\n{}\n",
            missing_lines.join("\n"),
            unmatched_lines.join("\n"),
        );
    }
}

/// Project diagnostic-like values to their diagnostic codes.
pub fn diagnostic_codes<T: HasDiagnosticCode>(diagnostics: &[T]) -> Vec<u32> {
    diagnostics
        .iter()
        .map(HasDiagnosticCode::diagnostic_code)
        .collect()
}

/// Count diagnostics with the given diagnostic code.
pub fn diagnostic_count<T: HasDiagnosticCode>(diagnostics: &[T], code: u32) -> usize {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.diagnostic_code() == code)
        .count()
}

/// Count diagnostics whose code matches the supplied predicate.
pub fn diagnostic_count_where<T: HasDiagnosticCode>(
    diagnostics: &[T],
    mut matches: impl FnMut(u32) -> bool,
) -> usize {
    diagnostics
        .iter()
        .filter(|diagnostic| matches(diagnostic.diagnostic_code()))
        .count()
}

/// Borrow diagnostics with the given diagnostic code.
pub fn diagnostics_with_code<T: HasDiagnosticCode>(diagnostics: &[T], code: u32) -> Vec<&T> {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.diagnostic_code() == code)
        .collect()
}

/// Borrow diagnostics whose code matches the supplied predicate.
pub fn diagnostics_where<T: HasDiagnosticCode>(
    diagnostics: &[T],
    mut matches: impl FnMut(u32) -> bool,
) -> Vec<&T> {
    diagnostics
        .iter()
        .filter(|diagnostic| matches(diagnostic.diagnostic_code()))
        .collect()
}

/// Borrow diagnostics with any of the supplied diagnostic codes.
pub fn diagnostics_with_any_code<'a, T: HasDiagnosticCode>(
    diagnostics: &'a [T],
    codes: &[u32],
) -> Vec<&'a T> {
    diagnostics
        .iter()
        .filter(|diagnostic| codes.contains(&diagnostic.diagnostic_code()))
        .collect()
}

/// Borrow diagnostics excluding the supplied diagnostic codes.
pub fn diagnostics_without_codes<'a, T: HasDiagnosticCode>(
    diagnostics: &'a [T],
    excluded_codes: &[u32],
) -> Vec<&'a T> {
    diagnostics
        .iter()
        .filter(|diagnostic| !excluded_codes.contains(&diagnostic.diagnostic_code()))
        .collect()
}

/// Return whether any diagnostic has the given code.
pub fn has_diagnostic_code<T: HasDiagnosticCode>(diagnostics: &[T], code: u32) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.diagnostic_code() == code)
}

/// Return whether any diagnostic code matches the supplied predicate.
pub fn has_diagnostic_code_where<T: HasDiagnosticCode>(
    diagnostics: &[T],
    mut matches: impl FnMut(u32) -> bool,
) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| matches(diagnostic.diagnostic_code()))
}

/// Return whether any diagnostic has one of the supplied diagnostic codes.
pub fn has_any_diagnostic_code<T: HasDiagnosticCode>(diagnostics: &[T], codes: &[u32]) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| codes.contains(&diagnostic.diagnostic_code()))
}

/// Return whether any diagnostic matches an arbitrary predicate.
pub fn has_diagnostic_where<T>(diagnostics: &[T], matches: impl FnMut(&T) -> bool) -> bool {
    diagnostics.iter().any(matches)
}

/// Project diagnostics to `(code, message_text)` pairs.
pub fn diagnostic_code_messages(
    diagnostics: impl IntoIterator<Item = Diagnostic>,
) -> Vec<(u32, String)> {
    diagnostics
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

/// Borrow diagnostics as `(code, message_text)` pairs.
pub fn diagnostic_code_message_refs(diagnostics: &[Diagnostic]) -> Vec<(u32, &str)> {
    diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.as_str()))
        .collect()
}

/// Borrow diagnostics with the given code as `(code, message_text)` pairs.
pub fn diagnostic_code_message_refs_with_code(
    diagnostics: &[Diagnostic],
    code: u32,
) -> Vec<(u32, &str)> {
    diagnostics_with_code(diagnostics, code)
        .into_iter()
        .map(|d| (d.code, d.message_text.as_str()))
        .collect()
}

/// Borrow diagnostic messages for diagnostics with the given code.
pub fn diagnostic_messages_with_code(diagnostics: &[Diagnostic], code: u32) -> Vec<&str> {
    diagnostics_with_code(diagnostics, code)
        .into_iter()
        .map(|d| d.message_text.as_str())
        .collect()
}

/// Return whether any diagnostic has the given code and message fragment.
pub fn has_diagnostic_code_message<T: HasDiagnosticMessage>(
    diagnostics: &[T],
    code: u32,
    message_fragment: &str,
) -> bool {
    diagnostics.iter().any(|diagnostic| {
        diagnostic.diagnostic_code() == code
            && diagnostic.diagnostic_message().contains(message_fragment)
    })
}

/// Return whether any diagnostic message contains the supplied text.
pub fn has_diagnostic_message<T: HasDiagnosticMessage>(
    diagnostics: &[T],
    message_fragment: &str,
) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.diagnostic_message().contains(message_fragment))
}

/// Borrow diagnostics with the given code and message text.
pub fn diagnostics_with_code_message<'a, T: HasDiagnosticMessage>(
    diagnostics: &'a [T],
    code: u32,
    message_fragment: &str,
) -> Vec<&'a T> {
    diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.diagnostic_code() == code
                && diagnostic.diagnostic_message().contains(message_fragment)
        })
        .collect()
}

/// Borrow diagnostics with the given code and any message text.
pub fn diagnostics_with_code_any_message<'a, T: HasDiagnosticMessage>(
    diagnostics: &'a [T],
    code: u32,
    message_fragments: &[&str],
) -> Vec<&'a T> {
    diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.diagnostic_code() == code
                && message_fragments
                    .iter()
                    .any(|fragment| diagnostic.diagnostic_message().contains(fragment))
        })
        .collect()
}

/// Parse, bind, and type-check JavaScript source, returning only diagnostic codes.
///
/// The caller supplies the test file name and any additional checker options.
/// This enables both `check_js` and `allow_js` for tests that want to model a
/// checked JavaScript file even when the surrounding options are TS-oriented.
pub fn check_js_source_codes_with_options(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
) -> Vec<u32> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..options
    };
    diagnostic_codes(&check_source(source, file_name, options))
}

/// Parse, bind, and type-check JavaScript source, returning `(code, message_text)` pairs.
pub fn check_js_source_code_messages_with_options(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..options
    };
    diagnostic_code_messages(check_source(source, file_name, options))
}

/// Parse, bind, and type-check JavaScript source, returning `(code, message_text)` pairs.
pub fn check_js_source_code_messages(source: &str) -> Vec<(u32, String)> {
    check_js_source_code_messages_with_options(source, "test.js", CheckerOptions::default())
}

/// Parse, bind, and type-check source, returning only diagnostic codes.
///
/// Convenience wrapper for tests that only inspect error codes.
pub fn check_source_codes(source: &str) -> Vec<u32> {
    diagnostic_codes(&check_source_diagnostics(source))
}

/// Parse, bind, and type-check a named TypeScript source string, returning only diagnostic codes.
pub fn check_source_codes_named(source: &str, file_name: &str) -> Vec<u32> {
    diagnostic_codes(&check_source(source, file_name, CheckerOptions::default()))
}

/// Parse, bind, and type-check source, returning `(code, message_text)` pairs.
///
/// Convenience wrapper for tests that inspect both error codes and message text.
pub fn check_source_code_messages(source: &str) -> Vec<(u32, String)> {
    diagnostic_code_messages(check_source_diagnostics(source))
}

/// Parse, bind, and type-check source with `experimental_decorators` enabled, returning codes.
pub fn check_source_codes_experimental_decorators(source: &str) -> Vec<u32> {
    diagnostic_codes(&check_source(
        source,
        "test.ts",
        CheckerOptions {
            experimental_decorators: true,
            ..CheckerOptions::default()
        },
    ))
}

/// Parse, bind, and type-check source with `no_unused_parameters` enabled.
pub fn check_source_no_unused_params(source: &str) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            no_unused_parameters: true,
            ..Default::default()
        },
    )
}

/// Parse, bind, and type-check source with `no_unused_locals` enabled.
pub fn check_source_no_unused_locals(source: &str) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            no_unused_locals: true,
            ..Default::default()
        },
    )
}

/// Parse, bind, and type-check a TypeScript source string with the given options.
///
/// Uses `"test.ts"` as the file name. Convenience wrapper for tests that need
/// custom options but not a custom file name.
pub fn check_with_options(source: &str, options: CheckerOptions) -> Vec<Diagnostic> {
    check_source(source, "test.ts", options)
}

/// `(code, message_text)` projection of [`check_with_options`].
pub fn check_with_options_code_messages(
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    diagnostic_code_messages(check_with_options(source, options))
}

/// Canonical "strict" `CheckerOptions` for tests that opt into the
/// `strict` + `strictNullChecks` + `noImplicitAny` combo.
///
/// Many checker tests need this exact triple. The shared factory keeps a
/// single source of truth; per-test overlays should clone this and tweak
/// the fields they actually care about.
pub fn strict_checker_options() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        strict_null_checks: true,
        no_implicit_any: true,
        ..CheckerOptions::default()
    }
}

/// Parse, bind, and type-check `source` under [`strict_checker_options`].
///
/// Returns full [`Diagnostic`]s; tests that only need codes or
/// `(code, message)` pairs should use the `_codes` / `_messages` projections.
pub fn check_source_strict(source: &str) -> Vec<Diagnostic> {
    check_with_options(source, strict_checker_options())
}

/// Code-only projection of [`check_source_strict`].
pub fn check_source_strict_codes(source: &str) -> Vec<u32> {
    diagnostic_codes(&check_source_strict(source))
}

/// `(code, message_text)` projection of [`check_source_strict`].
pub fn check_source_strict_messages(source: &str) -> Vec<(u32, String)> {
    check_with_options_code_messages(source, strict_checker_options())
}

/// Strict `(code, message_text)` diagnostics excluding TS2318 missing-default-lib noise.
pub fn check_source_strict_messages_without_missing_libs(source: &str) -> Vec<(u32, String)> {
    diagnostic_code_messages(
        check_source_strict(source)
            .into_iter()
            .filter(|d| d.code != 2318),
    )
}

/// Standard `lib.d.ts` source roots probed by checker tests, ordered by
/// preference: bundled stripped assets first (smallest, fastest to parse),
/// then the full bundled assets, then the TypeScript submodule's
/// `src/lib/` directory as a final fallback.
fn lib_test_roots() -> Vec<PathBuf> {
    let m = Path::new(env!("CARGO_MANIFEST_DIR"));
    vec![
        m.join("../tsz-core/src/lib-assets-stripped"),
        m.join("../tsz-core/src/lib-assets"),
        m.join("../../TypeScript/src/lib"),
    ]
}

/// Lib basenames that broadly cover `Promise` / `Iterable` / `Symbol` /
/// `AsyncGenerator` / `AsyncIterableIterator` / DOM / esnext typings used by
/// checker tests. The set mirrors what `tsc --target ESNext` loads by default
/// (see `default_libs_for_target("esnext")` in `crates/conformance/src/options_convert.rs`).
///
/// ES2016–ES2019 files are included so that async generator syntax and types
/// (`AsyncGenerator<T,U,V>`, `AsyncIterableIterator`, `Symbol.asyncIterator`)
/// resolve correctly in any test that uses [`load_default_lib_files`].
/// Tests that need a smaller or differently-shaped set should call
/// [`load_lib_files`] with an explicit slice.
pub const DEFAULT_LIB_NAMES: &[&str] = &[
    "es5.d.ts",
    "es2015.d.ts",
    "es2015.core.d.ts",
    "es2015.collection.d.ts",
    "es2015.iterable.d.ts",
    "es2015.generator.d.ts",
    "es2015.promise.d.ts",
    "es2015.proxy.d.ts",
    "es2015.reflect.d.ts",
    "es2015.symbol.d.ts",
    "es2015.symbol.wellknown.d.ts",
    "es2016.array.include.d.ts",
    "es2017.arraybuffer.d.ts",
    "es2017.date.d.ts",
    "es2017.object.d.ts",
    "es2017.sharedmemory.d.ts",
    "es2017.string.d.ts",
    "es2017.typedarrays.d.ts",
    "es2018.asynciterable.d.ts",
    "es2018.asyncgenerator.d.ts",
    "es2018.promise.d.ts",
    "es2018.regexp.d.ts",
    "es2019.array.d.ts",
    "es2019.object.d.ts",
    "es2019.string.d.ts",
    "es2019.symbol.d.ts",
    "dom.d.ts",
    "dom.iterable.d.ts",
    "esnext.d.ts",
];

/// Load `LibFile`s for the given basenames by probing [`lib_test_roots`]
/// in order. Names not found in any root are silently skipped — callers
/// that strictly require a particular lib should assert presence
/// themselves. Duplicates in `names` are deduped.
pub fn load_lib_files(names: &[&str]) -> Vec<Arc<LibFile>> {
    let roots = lib_test_roots();
    let mut out = Vec::new();
    let mut seen: FxHashSet<&str> = FxHashSet::default();
    for &name in names {
        if !seen.insert(name) {
            continue;
        }
        for root in &roots {
            let p = root.join(name);
            if p.exists()
                && let Ok(content) = std::fs::read_to_string(&p)
            {
                out.push(Arc::new(LibFile::from_source(name.to_string(), content)));
                break;
            }
        }
    }
    out
}

/// Convenience: load the [`DEFAULT_LIB_NAMES`] bundle.
pub fn load_default_lib_files() -> Vec<Arc<LibFile>> {
    load_lib_files(DEFAULT_LIB_NAMES)
}

/// Roots probed by [`load_compiled_lib_files`], ordered by preference.
/// These point at directories where TypeScript's own compiled lib files
/// (with the `lib.` prefix preserved, e.g. `lib.es5.d.ts`) live.
///
/// Includes paths relative to the worktree's `CARGO_MANIFEST_DIR` AND a
/// walk-up fallback to the primary checkout. `npm install` only
/// populates `scripts/node_modules/` in the primary checkout; worktrees
/// (e.g. under `<primary>/.worktrees/<name>/`) have a fresh `scripts/`
/// without `node_modules`, so the worktree-relative roots return nothing
/// and we'd fall through to the primary checkout's roots.
fn compiled_lib_test_roots() -> Vec<PathBuf> {
    let m = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut roots = vec![
        m.join("../../TypeScript/lib"),
        m.join("../tsz-website/src/lib"),
        m.join("../../scripts/conformance/node_modules/typescript/lib"),
        m.join("../../scripts/emit/node_modules/typescript/lib"),
        m.join("../../scripts/node_modules/typescript/lib"),
    ];

    // Walk up parent directories from CARGO_MANIFEST_DIR looking for any
    // ancestor that contains `scripts/node_modules/typescript/lib/`. The
    // first hit is treated as the primary checkout. 8 levels is enough to
    // cover both `<primary>/.worktrees/<name>/crates/tsz-checker` (4
    // levels) and other reasonable layouts (`<primary>/foo/bar/...`).
    let mut ancestor: Option<&Path> = Some(m);
    let marker = Path::new("scripts/node_modules/typescript/lib");
    for _ in 0..8 {
        let Some(dir) = ancestor else { break };
        let candidate = dir.join(marker);
        if candidate.exists() {
            roots.push(candidate);
            // Also expose the conformance/emit variants that may live
            // alongside the same primary's scripts/.
            roots.push(dir.join("scripts/conformance/node_modules/typescript/lib"));
            roots.push(dir.join("scripts/emit/node_modules/typescript/lib"));
            break;
        }
        ancestor = dir.parent();
    }

    roots
}

/// Load `LibFile`s using the **compiled** TypeScript lib naming
/// (`lib.<name>.d.ts`). Pass names with the `lib.` prefix already
/// included, e.g. `&["lib.es5.d.ts", "lib.es2015.symbol.d.ts"]`.
///
/// Use this helper when a test depends on the diagnostic output anchoring
/// to the compiled lib filenames — e.g. tests that assert on
/// `Diagnostic.file == "lib.es5.d.ts"` or that exercise the
/// `source.file_name.starts_with("lib.")` gate in
/// `crates/tsz-checker/src/types/queries/lib_resolution.rs`. Most tests
/// don't need this and should use [`load_lib_files`] /
/// [`load_default_lib_files`] instead — those produce smaller `LibFile`s
/// from the bundled stripped assets.
///
/// Names not found in any root are silently skipped; duplicates are
/// deduped. The resulting `LibFile.file_name` matches the input name
/// verbatim, preserving the `lib.` prefix.
pub fn load_compiled_lib_files(names: &[&str]) -> Vec<Arc<LibFile>> {
    let roots = compiled_lib_test_roots();
    let mut out = Vec::new();
    let mut seen: FxHashSet<&str> = FxHashSet::default();
    for &name in names {
        if !seen.insert(name) {
            continue;
        }
        for root in &roots {
            let p = root.join(name);
            if p.exists()
                && let Ok(content) = std::fs::read_to_string(&p)
            {
                out.push(Arc::new(LibFile::from_source(name.to_string(), content)));
                break;
            }
        }
    }
    out
}

/// Parse, bind, and type-check `source` with the given `lib_files` wired
/// into the binder and checker.
///
/// Mirrors [`check_source`] but routes through
/// [`tsz_binder::BinderState::bind_source_file_with_libs`] and
/// `Context::set_lib_contexts` / `set_actual_lib_file_count`. Use this
/// when tests rely on built-in types (`Promise`, `Array`, `Symbol`,
/// DOM, …); for tests that don't need libs, prefer [`check_source`]
/// which is faster.
///
/// Like [`check_source`], calls `enable_source_file_test_pragmas()` so
/// `// @ts-expect-error`-style pragmas are honored.
pub fn check_source_with_libs(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
    lib_files: &[Arc<LibFile>],
) -> Vec<Diagnostic> {
    if lib_files.is_empty() {
        return with_checked_source(source, file_name, options, None, |checker| {
            checker.ctx.diagnostics.clone()
        });
    }

    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let source_file = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), source_file, lib_files);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );
    checker.enable_source_file_test_pragmas();

    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());

    checker.check_source_file(source_file);
    checker.ctx.diagnostics.clone()
}

/// `(code, message_text)` projection of [`check_source_with_libs`].
pub fn check_source_with_libs_code_messages(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
    lib_files: &[Arc<LibFile>],
) -> Vec<(u32, String)> {
    diagnostic_code_messages(check_source_with_libs(
        source, file_name, options, lib_files,
    ))
}

/// Multi-file project pipeline helpers. Extracted to [`multi_file`] to keep
/// this module under the file-size cap (§19); re-exported here so existing
/// `test_utils::check_multi_file*` call sites are unchanged.
pub use multi_file::{
    check_all_multi_file_with_global_index, check_multi_file, check_multi_file_with_global_index,
    check_multi_file_with_libs, check_multi_file_with_type_params_cache,
};
