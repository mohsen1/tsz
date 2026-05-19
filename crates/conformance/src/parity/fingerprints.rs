//! Structured catalog of known tsz↔tsc diagnostic parity divergences.
//!
//! The conformance wrapper currently has to drop or rewrite a handful of
//! diagnostic fingerprints so the comparison against tsc matches. Historically
//! those filters were implemented as bespoke `is_extra_*` boolean predicates
//! scattered across `tsz_wrapper.rs`, each one hardcoding a line, a column,
//! and a rendered TypeScript error message — the textbook anti-pattern from
//! `.claude/CLAUDE.md` §25 (no line/col/identifier-shaped suppression in
//! compiler-adjacent code).
//!
//! This module turns that scatter into one named data table. Each entry
//! carries the structural rule it suppresses, the parity issue tracking the
//! underlying tsz bug, and the action (`Drop` or `Remap`) the wrapper should
//! take when an output diagnostic matches it. Adding a new entry forces the
//! author to link a parity issue and write a one-sentence structural rule;
//! removing an entry is the goal once the underlying issue is fixed.

/// How a rendered message is matched against a diagnostic line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MessageMatch {
    /// Exact equality after `normalize_message_paths`.
    Exact,
    /// Substring match against the raw diagnostic line (used by
    /// `parse_error_codes_from_text`, which sees the original text).
    Contains,
}

/// What the conformance wrapper does when an output diagnostic matches a
/// catalog entry.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ParityAction {
    /// Drop the fingerprint and error code from the comparison output.
    Drop,
    /// Replace the matched fingerprint with the given canonical shape.
    ///
    /// Used only when tsz emits the wrong error code for an otherwise
    /// well-understood divergence (e.g. circular instantiation surfacing
    /// as TS2322 instead of TS2589). Reserved for future entries; the
    /// pattern-match arms in `tsz_wrapper` are already wired up.
    #[allow(dead_code)]
    Remap(ParityRemap),
}

/// The canonical (code, position, message) tsz output is rewritten to when a
/// `ParityAction::Remap` entry matches.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ParityRemap {
    pub(crate) code: u32,
    pub(crate) line: u32,
    pub(crate) column: u32,
    pub(crate) message: &'static str,
}

/// One entry in the parity divergence catalog. Each entry must reference an
/// existing parity issue so the divergence can be tracked outside the
/// conformance wrapper.
///
/// `reason` and `parity_issue` are consumed by the catalog's unit tests and
/// will be surfaced in future fingerprint-classification reporting; they
/// carry no runtime behavior today, so `dead_code` is suppressed for now.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ParityFingerprintRule {
    pub(crate) code: u32,
    pub(crate) message: &'static str,
    pub(crate) message_match: MessageMatch,
    /// One-sentence structural rule (`WHEN …, tsc …; tsz currently differs`)
    /// per CLAUDE.md §26. Keeps the *why* alongside the entry instead of
    /// only in the linked issue, so future cleanup PRs don't need to leave
    /// the file to know what they're removing.
    #[allow(dead_code)]
    pub(crate) reason: &'static str,
    /// Tsz GitHub issue tracking the underlying parity bug.
    #[allow(dead_code)]
    pub(crate) parity_issue: ParityIssue,
    pub(crate) action: ParityAction,
}

/// Newtype for a tsz GitHub parity issue. Wraps the issue number so the
/// catalog can't drift to an arbitrary URL by mistake; the canonical URL is
/// reconstructed via `Display`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ParityIssue(pub(crate) u32);

impl ParityIssue {
    #[allow(dead_code)]
    pub(crate) const fn number(self) -> u32 {
        self.0
    }
}

impl std::fmt::Display for ParityIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "https://github.com/mohsen1/tsz/issues/{}", self.0)
    }
}

/// Catalog of known parity divergences. Every entry is a tracked debt: when
/// the linked parity issue is fixed and verified by CI, the entry should be
/// removed.
///
/// The catalog is the only sanctioned place to hardcode a fingerprint shape
/// in `crates/conformance/src/`. New ad-hoc `is_extra_*` predicates in
/// `tsz_wrapper.rs` are rejected by the
/// `tsz_wrapper_has_no_ad_hoc_extra_fingerprint_helpers` architecture test.
pub(crate) const KNOWN_PARITY_FINGERPRINTS: &[ParityFingerprintRule] = &[
    // #8423 — recursive alias display: tsz over-expands one level of the
    // recursive alias before printing the TS2322 source type.
    ParityFingerprintRule {
        code: 2322,
        message: "Type '(number | (ValueOrArray<number>)[] | (number | (ValueOrArray<number>)[])[])[]' is not assignable to type 'ValueOrArray<number>'.",
        message_match: MessageMatch::Exact,
        reason: "When an excess-property TS2322 is rendered against a recursive type alias, tsc keeps the alias display intact; tsz unrolls the alias one level before printing.",
        parity_issue: ParityIssue(8423),
        action: ParityAction::Drop,
    },
    // #8424 — discriminated-union narrowing under a generic key parameter:
    // tsz fails to prove the residual is never.
    ParityFingerprintRule {
        code: 2345,
        message: "Argument of type '({ kind: K; } & OptionOne) | ({ kind: K; } & OptionTwo)' is not assignable to parameter of type 'never'.",
        message_match: MessageMatch::Contains,
        reason: "When a generic function narrows a discriminated-union argument by an exhaustive switch on a key parameter, tsc collapses the residual intersection to never; tsz still surfaces the residual intersection in the TS2345 message.",
        parity_issue: ParityIssue(8424),
        action: ParityAction::Drop,
    },
    // #8424 — same divergence, alternative tsz display form when the
    // intersection is left un-distributed.
    ParityFingerprintRule {
        code: 2345,
        message: "Argument of type 'Options & { kind: K; }' is not assignable",
        message_match: MessageMatch::Contains,
        reason: "Alternative tsz display form for the same narrowing failure: the intersection is reported un-distributed instead of the per-member union shape.",
        parity_issue: ParityIssue(8424),
        action: ParityAction::Drop,
    },
];

/// Scope a classification query against the parity catalog uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MatchScope {
    /// Match against a normalized diagnostic message. Both `Exact` and
    /// `Contains` entries are eligible.
    NormalizedMessage,
    /// Match against a raw diagnostic line (`<file>(<l>,<c>): error TS…: <msg>`).
    /// Only `Contains` entries are eligible — `Exact` requires the normalized
    /// message without the position prefix.
    RawLine,
}

/// Classify a piece of diagnostic text against the parity catalog.
///
/// `text` is either the normalized message or the raw diagnostic line,
/// depending on `scope`.
pub(crate) fn classify_parity(
    code: u32,
    text: &str,
    scope: MatchScope,
) -> Option<&'static ParityFingerprintRule> {
    KNOWN_PARITY_FINGERPRINTS
        .iter()
        .find(|rule| rule.code == code && rule_matches(rule, text, scope))
}

fn rule_matches(rule: &ParityFingerprintRule, text: &str, scope: MatchScope) -> bool {
    match (scope, rule.message_match) {
        (MatchScope::NormalizedMessage, MessageMatch::Exact) => rule.message == text,
        (_, MessageMatch::Contains) => text.contains(rule.message),
        // A raw line still carries the `<file>(<l>,<c>): error TS…: ` prefix,
        // so equality against the bare rendered message is by construction
        // impossible — reject without spending the comparison.
        (MatchScope::RawLine, MessageMatch::Exact) => false,
    }
}
