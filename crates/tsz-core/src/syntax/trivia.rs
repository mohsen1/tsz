use crate::source::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommentKind {
    Line,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommentClass {
    Ordinary,
    Pinned,
    DetachedPinned,
    TripleSlashReference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommentPlacement {
    Leading,
    Trailing,
}

/// Whether the scanner encountered a comment before any source token. Unlike
/// statement-relative placement, this is the source-file leading-comment run
/// consumed by TypeScript's pragma parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommentSourcePosition {
    SourceLeading,
    AfterToken,
}

/// Scanner-owned comment identity and token adjacency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CommentTrivia {
    pub span: Span,
    pub preceding_token_end: Option<u32>,
    pub preceding_token_kind: Option<super::TokenKind>,
    pub kind: CommentKind,
    pub class: CommentClass,
    /// The scanner saw the authored JSDoc opener `/**` (excluding `/**/`).
    /// Association with a declaration remains parser-owned.
    pub jsdoc: bool,
    pub placement: CommentPlacement,
    pub source_position: CommentSourcePosition,
    pub has_trailing_line_break: bool,
    pub plain: bool,
}

/// The last TypeScript check-control pragma in the source-leading single-line
/// comment run. This is scanner-authored syntax provenance; program capability
/// analysis decides how the directive affects semantic products.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SourceCheckDirective {
    pub(crate) kind: SourceCheckDirectiveKind,
    pub(crate) span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceCheckDirectiveKind {
    Check,
    NoCheck,
}

/// Match TypeScript's source-leading single-line pragma grammar:
/// `//` or `///`, optional whitespace, a case-insensitive directive name, and
/// then either end-of-comment, whitespace, or `:` followed by arbitrary text.
pub(crate) fn parse_source_check_directive(
    comment: &str,
    span: Span,
) -> Option<SourceCheckDirective> {
    let mut body = comment.strip_prefix("//")?;
    if let Some(after_third_slash) = body.strip_prefix('/') {
        body = after_third_slash;
    }
    body = body.trim_start_matches(char::is_whitespace);
    let directive = body.strip_prefix('@')?;
    let name_end = directive
        .find(|character: char| character.is_whitespace() || character == ':')
        .unwrap_or(directive.len());
    let kind = match &directive[..name_end] {
        name if name.eq_ignore_ascii_case("ts-check") => SourceCheckDirectiveKind::Check,
        name if name.eq_ignore_ascii_case("ts-nocheck") => SourceCheckDirectiveKind::NoCheck,
        _ => return None,
    };
    Some(SourceCheckDirective { kind, span })
}

pub(crate) const fn is_single_line_whitespace(character: char) -> bool {
    matches!(
        character,
        ' ' | '\t' | '\u{000b}' | '\u{000c}' | '\u{0085}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200b}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}
