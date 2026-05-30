pub mod data;

/// True when `code` is in the TypeScript parser/grammar diagnostic range
/// (1000–1999) — syntactic errors and grammar rule violations.
pub fn is_parser_grammar_diagnostic(code: u32) -> bool {
    (1000..2000).contains(&code)
}

/// True when `code` is in the TypeScript JavaScript grammar diagnostic range
/// (8000–8999) — JS-specific parser errors emitted for `.js`/`.jsx` sources.
pub fn is_js_grammar_diagnostic(code: u32) -> bool {
    (8000..9000).contains(&code)
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DiagnosticCategory {
    Warning,
    Error,
    Suggestion,
    Message,
}

impl DiagnosticCategory {
    /// Map a cached numeric category (as serialized by the incremental cache)
    /// back to a [`DiagnosticCategory`]; unknown values fall back to `Message`.
    pub const fn from_cache_index(index: u8) -> Self {
        match index {
            0 => Self::Warning,
            1 => Self::Error,
            2 => Self::Suggestion,
            _ => Self::Message,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticMessage {
    pub code: u32,
    pub category: DiagnosticCategory,
    pub message: &'static str,
}

pub mod diagnostic_messages {
    pub use super::data::diagnostic_messages::*;
}

pub mod diagnostic_codes {
    pub use super::data::diagnostic_codes::*;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticRelatedInformation {
    pub category: DiagnosticCategory,
    pub code: u32,
    pub file: String,
    pub start: u32,
    pub length: u32,
    pub message_text: String,
    /// Nesting depth of this entry within a diagnostic's elaboration message
    /// chain, used to render `tsc`-style progressive indentation in plain
    /// output. `0` is the first elaboration level (rendered at 2 spaces); each
    /// deeper level adds 2 more spaces. Genuine cross-location related
    /// information (not part of an elaboration chain) stays at `0`.
    pub depth: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub category: DiagnosticCategory,
    pub code: u32,
    pub file: String,
    pub start: u32,
    pub length: u32,
    pub message_text: String,
    pub related_information: Vec<DiagnosticRelatedInformation>,
}

impl Diagnostic {
    pub fn error(
        file: impl Into<String>,
        start: u32,
        length: u32,
        message: impl Into<String>,
        code: u32,
    ) -> Self {
        Self {
            category: DiagnosticCategory::Error,
            message_text: message.into(),
            code,
            file: file.into(),
            start,
            length,
            related_information: Vec::new(),
        }
    }

    /// Create a diagnostic by looking up the message template and category from
    /// the diagnostic code. The template's `{0}`, `{1}`, ... placeholders are
    /// replaced with the provided `args`.
    ///
    /// Panics (debug) if the code is not found in the generated diagnostic table.
    pub fn from_code(
        code: u32,
        file: impl Into<String>,
        start: u32,
        length: u32,
        args: &[&str],
    ) -> Self {
        let info = lookup_diagnostic(code).unwrap_or(DiagnosticMessage {
            code,
            category: DiagnosticCategory::Error,
            message: "Unknown diagnostic",
        });
        debug_assert!(
            lookup_diagnostic(code).is_some(),
            "diagnostic code {code} not found in generated table"
        );
        Self {
            category: info.category,
            code,
            file: file.into(),
            start,
            length,
            message_text: format_message(info.message, args),
            related_information: Vec::new(),
        }
    }

    /// Build a `Message`-category related entry at the first elaboration depth
    /// (rendered at 2 spaces). Use this for elaboration/related lines that are
    /// not part of a deeper chain so the `depth` field need not be spelled out
    /// at every call site.
    pub fn related_message(
        code: u32,
        file: impl Into<String>,
        start: u32,
        length: u32,
        message_text: impl Into<String>,
    ) -> DiagnosticRelatedInformation {
        DiagnosticRelatedInformation {
            category: DiagnosticCategory::Message,
            code,
            file: file.into(),
            start,
            length,
            message_text: message_text.into(),
            depth: 0,
        }
    }

    pub fn with_related(
        mut self,
        file: impl Into<String>,
        start: u32,
        length: u32,
        message: impl Into<String>,
    ) -> Self {
        self.related_information.push(DiagnosticRelatedInformation {
            category: DiagnosticCategory::Message,
            code: 0,
            file: file.into(),
            start,
            length,
            message_text: message.into(),
            depth: 0,
        });
        self
    }

    /// `Span`-based variant of [`Diagnostic::error`]. Converts the
    /// half-open `[start, end)` `Span` to the diagnostic's
    /// start+length representation. Equivalent to
    /// `Diagnostic::error(file, span.start, span.len(), message, code)`.
    pub fn error_with_span(
        file: impl Into<String>,
        span: crate::span::Span,
        message: impl Into<String>,
        code: u32,
    ) -> Self {
        Self::error(file, span.start, span.len(), message, code)
    }

    /// `Span`-based variant of [`Diagnostic::from_code`].
    pub fn from_code_with_span(
        code: u32,
        file: impl Into<String>,
        span: crate::span::Span,
        args: &[&str],
    ) -> Self {
        Self::from_code(code, file, span.start, span.len(), args)
    }

    /// `Span`-based variant of [`Diagnostic::with_related`].
    pub fn with_related_span(
        self,
        file: impl Into<String>,
        span: crate::span::Span,
        message: impl Into<String>,
    ) -> Self {
        self.with_related(file, span.start, span.len(), message)
    }

    /// View this diagnostic's location as a `Span`. Reconstructs the
    /// half-open `[start, start + length)` interval from the stored
    /// start+length pair.
    pub const fn span(&self) -> crate::span::Span {
        crate::span::Span::from_len(self.start, self.length)
    }

    /// Canonical total ordering for diagnostics, mirroring the TypeScript
    /// compiler's `compareDiagnostics`: by file, then start, then length, then
    /// code, then message text, then related information.
    ///
    /// This is a *total* order over the observable fields, so two diagnostics
    /// that share a location and code still have a stable, reproducible
    /// relative order. That is what keeps reported diagnostic order
    /// deterministic across equivalent relations, regardless of the
    /// (potentially parallel or hash-map-driven) order in which the
    /// diagnostics were produced. Every site that emits the final diagnostic
    /// list must sort through this comparator rather than an ad-hoc partial
    /// key, otherwise diagnostics that tie on the partial key fall back to
    /// nondeterministic production order.
    pub fn compare(&self, other: &Self) -> std::cmp::Ordering {
        self.compare_skip_related_information(other).then_with(|| {
            compare_related_information(&self.related_information, &other.related_information)
        })
    }

    /// The location/code/message portion of [`Diagnostic::compare`], mirroring
    /// tsc's `compareDiagnosticsSkipRelatedInformation`. Orders by file, then
    /// start, then length, then code, then message text.
    pub fn compare_skip_related_information(&self, other: &Self) -> std::cmp::Ordering {
        self.file
            .cmp(&other.file)
            .then_with(|| self.start.cmp(&other.start))
            .then_with(|| self.length.cmp(&other.length))
            .then_with(|| self.code.cmp(&other.code))
            .then_with(|| self.message_text.cmp(&other.message_text))
    }
}

/// Order two related-information lists, mirroring tsc's
/// `compareRelatedInformation`: shorter lists sort first, then the lists are
/// compared element-by-element on file, start, length, code, and message text.
fn compare_related_information(
    a: &[DiagnosticRelatedInformation],
    b: &[DiagnosticRelatedInformation],
) -> std::cmp::Ordering {
    a.len().cmp(&b.len()).then_with(|| {
        a.iter()
            .zip(b.iter())
            .map(|(left, right)| {
                left.file
                    .cmp(&right.file)
                    .then_with(|| left.start.cmp(&right.start))
                    .then_with(|| left.length.cmp(&right.length))
                    .then_with(|| left.code.cmp(&right.code))
                    .then_with(|| left.message_text.cmp(&right.message_text))
            })
            .find(|ordering| ordering.is_ne())
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}

/// Look up a `DiagnosticMessage` (code + category + template) by numeric code.
/// Uses binary search over the sorted generated table — O(log n).
pub fn lookup_diagnostic(code: u32) -> Option<DiagnosticMessage> {
    self::data::DIAGNOSTIC_MESSAGE_SECTIONS
        .iter()
        .find_map(|section| {
            section
                .binary_search_by_key(&code, |m| m.code)
                .ok()
                .map(|idx| section[idx])
        })
}

pub fn get_message_template(code: u32) -> Option<&'static str> {
    lookup_diagnostic(code).map(|m| m.message)
}

pub fn format_message(message: &str, args: &[&str]) -> String {
    fn normalize_template_placeholder_spacing(arg: &str) -> String {
        if !arg.contains("${") {
            return arg.to_string();
        }

        let chars: Vec<char> = arg.chars().collect();
        let mut out = String::with_capacity(arg.len());
        let mut i = 0usize;

        while i < chars.len() {
            if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '{' {
                out.push('$');
                out.push('{');
                i += 2;

                while i < chars.len() && chars[i].is_whitespace() {
                    i += 1;
                }

                let mut depth = 1usize;
                let mut inner = String::new();
                while i < chars.len() {
                    let ch = chars[i];
                    i += 1;
                    if ch == '{' {
                        depth += 1;
                        inner.push(ch);
                        continue;
                    }
                    if ch == '}' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        inner.push(ch);
                        continue;
                    }
                    inner.push(ch);
                }

                out.push_str(inner.trim_end());
                out.push('}');
                continue;
            }

            out.push(chars[i]);
            i += 1;
        }

        out
    }

    let mut result = message.to_string();
    for (i, arg) in args.iter().enumerate() {
        let normalized = normalize_template_placeholder_spacing(arg);
        result = result.replace(&format!("{{{i}}}"), &normalized);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_diagnostic_finds_known_code_and_rejects_unknown_code() {
        let known = data::iter_diagnostic_messages()
            .next()
            .expect("generated diagnostic table should not be empty");

        let lookup = lookup_diagnostic(known.code).expect("known code should resolve");
        assert_eq!(lookup, known);
        assert!(lookup_diagnostic(u32::MAX).is_none());
    }

    #[test]
    fn get_message_template_matches_lookup_and_returns_none_for_unknown_code() {
        let known = data::iter_diagnostic_messages()
            .next()
            .expect("generated diagnostic table should not be empty");

        assert_eq!(get_message_template(known.code), Some(known.message));
        assert_eq!(get_message_template(u32::MAX), None);
    }

    #[test]
    fn format_message_replaces_placeholders_and_leaves_missing_ones_intact() {
        let formatted = format_message("{0} + {1} + {0} + {2}", &["a", "b"]);
        assert_eq!(formatted, "a + b + a + {2}");
    }

    #[test]
    fn diagnostic_from_code_uses_table_entry_for_known_code() {
        let known = data::iter_diagnostic_messages()
            .next()
            .expect("generated diagnostic table should not be empty");
        let args = ["left", "right", "extra"];
        let expected_message = format_message(known.message, &args);

        let diagnostic = Diagnostic::from_code(known.code, "test.ts", 4, 8, &args);

        assert_eq!(diagnostic.category, known.category);
        assert_eq!(diagnostic.code, known.code);
        assert_eq!(diagnostic.file, "test.ts");
        assert_eq!(diagnostic.start, 4);
        assert_eq!(diagnostic.length, 8);
        assert_eq!(diagnostic.message_text, expected_message);
        assert!(diagnostic.related_information.is_empty());
    }

    #[test]
    fn diagnostic_from_code_uses_unknown_fallback_for_missing_code() {
        let result = std::panic::catch_unwind(|| {
            Diagnostic::from_code(u32::MAX, "missing.ts", 1, 2, &["ignored"])
        });

        if cfg!(debug_assertions) {
            assert!(
                result.is_err(),
                "debug builds should trip the diagnostic lookup assertion"
            );
        } else {
            let diagnostic = result.expect("release builds should return the fallback diagnostic");
            assert_eq!(diagnostic.category, DiagnosticCategory::Error);
            assert_eq!(diagnostic.code, u32::MAX);
            assert_eq!(diagnostic.file, "missing.ts");
            assert_eq!(diagnostic.start, 1);
            assert_eq!(diagnostic.length, 2);
            assert_eq!(diagnostic.message_text, "Unknown diagnostic");
            assert!(diagnostic.related_information.is_empty());
        }
    }

    // =========================================================================
    // Diagnostic::compare — canonical, total, deterministic ordering
    //
    // These lock the rule: when diagnostics tie on a partial key (e.g. same
    // file+start), the relative order is still fully determined by the
    // remaining fields (length, code, message, related info) in tsc's
    // `compareDiagnostics` order — never by production/insertion order. Each
    // test scrambles insertion order and asserts the same canonical sequence.
    // =========================================================================

    fn diag(file: &str, start: u32, length: u32, code: u32, message: &str) -> Diagnostic {
        Diagnostic::error(file, start, length, message, code)
    }

    /// Sorting through the canonical comparator yields the same order no matter
    /// how the input was permuted — the property that makes reported diagnostic
    /// order deterministic across equivalent relations.
    fn assert_canonical_order_is_permutation_invariant(canonical: &[Diagnostic]) {
        // The canonical slice must already be sorted by `compare`.
        for window in canonical.windows(2) {
            assert_ne!(
                window[0].compare(&window[1]),
                std::cmp::Ordering::Greater,
                "input slice is expected to be in canonical order"
            );
        }
        // Every permutation collapses back to the same canonical order
        // (`Diagnostic` derives `PartialEq`, so compare the whole slice).
        let mut reversed = canonical.to_vec();
        reversed.reverse();
        let mut rotated = canonical.to_vec();
        rotated.rotate_left(canonical.len() / 2);
        for mut permutation in [reversed, rotated] {
            permutation.sort_by(|a, b| a.compare(b));
            assert_eq!(permutation, canonical);
        }
    }

    #[test]
    fn compare_orders_by_file_then_start_then_length_then_code_then_message() {
        // Canonical tsc order: file, then start, then length, then code, then
        // message text. Several pairs deliberately tie on the earlier keys so
        // the later tiebreakers are exercised.
        let canonical = vec![
            diag("a.ts", 0, 5, 2304, "alpha"),
            // same file+start as next, shorter length sorts first
            diag("a.ts", 10, 2, 9999, "zzz"),
            diag("a.ts", 10, 4, 1000, "aaa"),
            // same file+start+length, lower code first
            diag("a.ts", 20, 3, 2322, "msg"),
            diag("a.ts", 20, 3, 2345, "msg"),
            // same file+start+length+code, message breaks the tie
            diag("a.ts", 30, 1, 2304, "aaa"),
            diag("a.ts", 30, 1, 2304, "bbb"),
            // file name is the highest-priority key
            diag("b.ts", 0, 1, 1000, "anything"),
        ];

        assert_canonical_order_is_permutation_invariant(&canonical);
    }

    #[test]
    fn compare_breaks_ties_on_related_information() {
        // Two diagnostics identical on every primary field differ only in
        // related information; the shorter related list sorts first, then the
        // lists compare element-by-element.
        let bare = diag("a.ts", 0, 1, 2304, "msg");
        let with_one = diag("a.ts", 0, 1, 2304, "msg").with_related("a.ts", 5, 1, "see a");
        let with_two = diag("a.ts", 0, 1, 2304, "msg")
            .with_related("a.ts", 5, 1, "see a")
            .with_related("a.ts", 9, 1, "see b");

        let canonical = vec![bare, with_one, with_two];
        assert_canonical_order_is_permutation_invariant(&canonical);
    }

    #[test]
    fn compare_is_a_total_order_consistent_with_equality() {
        let a = diag("a.ts", 0, 1, 2304, "msg");
        let b = a.clone();
        assert_eq!(a.compare(&b), std::cmp::Ordering::Equal);

        let c = diag("a.ts", 0, 1, 2304, "msg2");
        // Antisymmetry: a < c implies c > a.
        assert_eq!(a.compare(&c), std::cmp::Ordering::Less);
        assert_eq!(c.compare(&a), std::cmp::Ordering::Greater);
    }

    #[test]
    fn diagnostic_with_related_appends_message_information() {
        let diagnostic = Diagnostic::error("file.ts", 10, 3, "message", 1234)
            .with_related("other.ts", 20, 5, "see also");

        assert_eq!(diagnostic.related_information.len(), 1);
        let related = &diagnostic.related_information[0];
        assert_eq!(related.category, DiagnosticCategory::Message);
        assert_eq!(related.code, 0);
        assert_eq!(related.file, "other.ts");
        assert_eq!(related.start, 20);
        assert_eq!(related.length, 5);
        assert_eq!(related.message_text, "see also");
    }

    // =========================================================================
    // is_parser_grammar_diagnostic / is_js_grammar_diagnostic
    //
    // Lock the half-open ranges (1000..2000) and (8000..9000) — these are the
    // exact boundaries the CLI driver and LSP rely on to bucket parser/grammar
    // diagnostics for emit policy and stderr formatting.
    // =========================================================================

    #[test]
    fn is_parser_grammar_diagnostic_covers_inclusive_lower_bound() {
        assert!(is_parser_grammar_diagnostic(1000));
        assert!(is_parser_grammar_diagnostic(1001));
    }

    #[test]
    fn is_parser_grammar_diagnostic_covers_typical_codes() {
        // TS1005 ("X expected"), TS1109 ("Expression expected"), TS1128, TS1434.
        assert!(is_parser_grammar_diagnostic(1005));
        assert!(is_parser_grammar_diagnostic(1109));
        assert!(is_parser_grammar_diagnostic(1128));
        assert!(is_parser_grammar_diagnostic(1434));
        assert!(is_parser_grammar_diagnostic(1999));
    }

    #[test]
    fn is_parser_grammar_diagnostic_excludes_exclusive_upper_bound() {
        assert!(!is_parser_grammar_diagnostic(2000));
        assert!(!is_parser_grammar_diagnostic(2001));
    }

    #[test]
    fn is_parser_grammar_diagnostic_excludes_codes_below_range() {
        assert!(!is_parser_grammar_diagnostic(0));
        assert!(!is_parser_grammar_diagnostic(999));
    }

    #[test]
    fn is_parser_grammar_diagnostic_excludes_semantic_and_js_grammar_codes() {
        // Semantic (TS2xxx-TS7xxx) and JS-grammar (TS8xxx) codes are out of range.
        assert!(!is_parser_grammar_diagnostic(2322)); // assignability
        assert!(!is_parser_grammar_diagnostic(2345)); // call argument mismatch
        assert!(!is_parser_grammar_diagnostic(7053)); // implicit any index
        assert!(!is_parser_grammar_diagnostic(8000));
        assert!(!is_parser_grammar_diagnostic(9000));
        assert!(!is_parser_grammar_diagnostic(u32::MAX));
    }

    #[test]
    fn is_js_grammar_diagnostic_covers_inclusive_lower_bound() {
        assert!(is_js_grammar_diagnostic(8000));
        assert!(is_js_grammar_diagnostic(8001));
    }

    #[test]
    fn is_js_grammar_diagnostic_covers_typical_codes() {
        // TS8002, TS8005, TS8006 are emitted for JS-only syntactic constructs.
        assert!(is_js_grammar_diagnostic(8002));
        assert!(is_js_grammar_diagnostic(8005));
        assert!(is_js_grammar_diagnostic(8500));
        assert!(is_js_grammar_diagnostic(8999));
    }

    #[test]
    fn is_js_grammar_diagnostic_excludes_exclusive_upper_bound() {
        assert!(!is_js_grammar_diagnostic(9000));
        assert!(!is_js_grammar_diagnostic(9001));
    }

    #[test]
    fn is_js_grammar_diagnostic_excludes_codes_below_range() {
        assert!(!is_js_grammar_diagnostic(0));
        assert!(!is_js_grammar_diagnostic(7999));
    }

    #[test]
    fn is_js_grammar_diagnostic_excludes_parser_and_semantic_codes() {
        // The two helpers MUST be disjoint — a parser-grammar code is never a
        // JS-grammar code (and vice versa). Lock that contract.
        assert!(!is_js_grammar_diagnostic(1005));
        assert!(!is_js_grammar_diagnostic(1999));
        assert!(!is_js_grammar_diagnostic(2322));
        assert!(!is_js_grammar_diagnostic(u32::MAX));
        assert!(!is_parser_grammar_diagnostic(8005));
    }

    // =========================================================================
    // Diagnostic::error simple constructor
    //
    // Locks the basic field initialization for the convenience constructor.
    // =========================================================================

    #[test]
    fn diagnostic_error_constructor_sets_fields_and_empty_related() {
        let diagnostic = Diagnostic::error("file.ts", 7, 4, "boom", 9001);
        assert_eq!(diagnostic.category, DiagnosticCategory::Error);
        assert_eq!(diagnostic.code, 9001);
        assert_eq!(diagnostic.file, "file.ts");
        assert_eq!(diagnostic.start, 7);
        assert_eq!(diagnostic.length, 4);
        assert_eq!(diagnostic.message_text, "boom");
        assert!(diagnostic.related_information.is_empty());
    }

    #[test]
    fn diagnostic_error_constructor_accepts_string_and_str_via_into() {
        // The `impl Into<String>` arms accept both `&str` and `String` callers
        // — verify both work without surprises.
        let from_str = Diagnostic::error("file.ts", 0, 1, "literal", 1);
        assert_eq!(from_str.message_text, "literal");

        let from_string = Diagnostic::error(
            String::from("owned.ts"),
            0,
            1,
            String::from("owned message"),
            2,
        );
        assert_eq!(from_string.file, "owned.ts");
        assert_eq!(from_string.message_text, "owned message");
    }

    // =========================================================================
    // format_message: ${...} placeholder normalization
    //
    // Lock the inner `normalize_template_placeholder_spacing` behaviour: when
    // an arg contains a TS-style template-literal placeholder `${ ... }`,
    // surrounding whitespace inside the braces is stripped so the substituted
    // type display matches tsc's compact rendering. Args without `${` are
    // pass-through; nested braces are tracked by depth.
    // =========================================================================

    #[test]
    fn format_message_passes_through_arg_without_template_placeholder() {
        // No `${` -> arg is substituted byte-for-byte (including its own braces).
        let formatted = format_message("got {0}", &["plain {value}"]);
        assert_eq!(formatted, "got plain {value}");
    }

    #[test]
    fn format_message_strips_whitespace_inside_template_placeholder() {
        let formatted = format_message("got {0}", &["${  number  }"]);
        assert_eq!(formatted, "got ${number}");
    }

    #[test]
    fn format_message_strips_only_outer_whitespace_in_template_placeholder() {
        // Internal whitespace between tokens is preserved; only leading after
        // `${` and trailing before `}` are stripped.
        let formatted = format_message("got {0}", &["${  string | number  }"]);
        assert_eq!(formatted, "got ${string | number}");
    }

    #[test]
    fn format_message_preserves_nested_braces_in_template_placeholder() {
        // `${ {a: number} }` should yield `${{a: number}}` — the inner `{...}`
        // is balanced by depth counting and not mistaken for the placeholder
        // close.
        let formatted = format_message("got {0}", &["${ {a: number} }"]);
        assert_eq!(formatted, "got ${{a: number}}");
    }

    #[test]
    fn format_message_handles_multiple_template_placeholders_in_one_arg() {
        let formatted = format_message("x: {0}", &["before ${ first } middle ${  second  } after"]);
        assert_eq!(formatted, "x: before ${first} middle ${second} after");
    }

    #[test]
    fn format_message_normalizes_each_arg_independently() {
        let formatted = format_message("{0} -> {1}", &["${  source  }", "plain {x}"]);
        assert_eq!(formatted, "${source} -> plain {x}");
    }

    #[test]
    fn format_message_handles_unterminated_template_placeholder_gracefully() {
        // No closing `}` — function consumes to end without panicking and
        // emits the (trimmed) inner content followed by a synthesized `}`.
        let formatted = format_message("got {0}", &["prefix ${ unterminated"]);
        assert_eq!(formatted, "got prefix ${unterminated}");
    }

    #[test]
    fn format_message_handles_empty_template_placeholder() {
        let formatted = format_message("got {0}", &["${}"]);
        assert_eq!(formatted, "got ${}");
    }

    #[test]
    fn format_message_dollar_without_brace_is_literal() {
        // A bare `$` not followed by `{` is passed through as-is.
        let formatted = format_message("got {0}", &["price: $5"]);
        assert_eq!(formatted, "got price: $5");
    }

    #[test]
    fn error_with_span_matches_error_start_length() {
        // `error_with_span(file, Span::new(start, end), msg, code)` must
        // produce the same diagnostic as `error(file, start, end-start, msg, code)`.
        // The span uses half-open `[start, end)` semantics.
        let span = crate::span::Span::new(10, 17);
        let lhs = Diagnostic::error_with_span("a.ts", span, "hello", 2322);
        let rhs = Diagnostic::error("a.ts", 10, 7, "hello", 2322);
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn span_accessor_round_trips_with_error_with_span() {
        // `Diagnostic::span()` reconstructs the half-open `Span` that
        // `error_with_span` stored — round-trip identity.
        let span = crate::span::Span::new(100, 105);
        let diag = Diagnostic::error_with_span("a.ts", span, "x", 2322);
        assert_eq!(diag.span(), span);
        assert_eq!(diag.span().len(), 5);
    }

    #[test]
    fn from_code_with_span_matches_from_code_start_length() {
        // Same equivalence, for `from_code` / `from_code_with_span`.
        let span = crate::span::Span::new(0, 4);
        // Use a known-existing code so format-message lookup behaves the
        // same on both sides.
        let code = 2322;
        let lhs = Diagnostic::from_code_with_span(code, "a.ts", span, &["string", "number"]);
        let rhs = Diagnostic::from_code(code, "a.ts", 0, 4, &["string", "number"]);
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn with_related_span_matches_with_related_start_length() {
        let main_span = crate::span::Span::new(0, 3);
        let related_span = crate::span::Span::new(20, 25);
        let lhs = Diagnostic::error_with_span("a.ts", main_span, "x", 2322).with_related_span(
            "b.ts",
            related_span,
            "see here",
        );
        let rhs =
            Diagnostic::error("a.ts", 0, 3, "x", 2322).with_related("b.ts", 20, 5, "see here");
        assert_eq!(lhs, rhs);
    }
}
