use crate::source::{SourceText, Span};

use super::Statement;
use super::template_literal::statements_form_no_substitution_template_expression_file;

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

pub(crate) fn comments_form_no_substitution_template_expression_file(
    source: &SourceText,
    statements: &[Statement],
    comments: &[CommentTrivia],
) -> bool {
    if comments.is_empty() {
        return true;
    }
    let [comment] = comments else {
        return false;
    };
    let [statement] = statements else {
        return false;
    };
    statements_form_no_substitution_template_expression_file(statements)
        && comment.plain
        && comment.kind == CommentKind::Line
        && comment.placement == CommentPlacement::Leading
        && comment.has_trailing_line_break
        && is_column_zero(source, comment.span.start)
        && !source
            .slice(comment.span)
            .chars()
            .last()
            .is_some_and(is_single_line_whitespace)
        && has_one_line_break(source, comment.span.end, statement.span.start)
}

pub(crate) fn comments_form_contiguous_plain_leading_run(
    source: &SourceText,
    statement: &Statement,
    comments: &[CommentTrivia],
) -> bool {
    if comments.is_empty() {
        return false;
    }
    comments.iter().enumerate().all(|(index, comment)| {
        let next_start = comments
            .get(index + 1)
            .map_or(statement.span.start, |next| next.span.start);
        comment.plain
            && comment.kind == CommentKind::Line
            && comment.placement == CommentPlacement::Leading
            && comment.has_trailing_line_break
            && is_column_zero(source, comment.span.start)
            && !source
                .slice(comment.span)
                .chars()
                .last()
                .is_some_and(is_single_line_whitespace)
            && comment.span.end <= next_start
            && has_one_line_break(source, comment.span.end, next_start)
    })
}

pub(crate) fn source_is_ascii_outside_comments(
    source: &SourceText,
    comments: &[CommentTrivia],
) -> bool {
    let mut cursor = 0_usize;
    for comment in comments {
        let start = comment.span.start as usize;
        let end = comment.span.end as usize;
        if start < cursor
            || end < start
            || end > source.text.len()
            || !source.text.is_char_boundary(start)
            || !source.text.is_char_boundary(end)
            || !source.text[cursor..start].is_ascii()
        {
            return false;
        }
        cursor = end;
    }
    source.text[cursor..].is_ascii()
}

pub(crate) fn source_uses_supported_line_breaks(source: &SourceText) -> bool {
    let bytes = source.text.as_bytes();
    !source
        .text
        .chars()
        .any(|character| matches!(character, '\u{2028}' | '\u{2029}'))
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| *byte != b'\r' || bytes.get(index + 1) == Some(&b'\n'))
}

pub(crate) fn statement_starts_at_supported_column(
    source: &SourceText,
    statements: &[Statement],
) -> bool {
    let [statement] = statements else {
        return false;
    };
    statement.span.start == 0
        || source
            .text
            .as_bytes()
            .get(statement.span.start as usize - 1)
            == Some(&b'\n')
}

fn has_one_line_break(source: &SourceText, start: u32, end: u32) -> bool {
    let bytes = source.text.as_bytes();
    let Some(gap) = bytes.get(start as usize..end as usize) else {
        return false;
    };
    let mut index = 0;
    match gap.get(index) {
        Some(b'\r') => {
            index += 1;
            if gap.get(index) == Some(&b'\n') {
                index += 1;
            }
        }
        Some(b'\n') => index += 1,
        _ => return false,
    }
    index == gap.len()
}

fn is_column_zero(source: &SourceText, offset: u32) -> bool {
    offset == 0
        || source
            .text
            .as_bytes()
            .get(offset as usize - 1)
            .is_some_and(|byte| matches!(byte, b'\r' | b'\n'))
}

pub(crate) const fn is_single_line_whitespace(character: char) -> bool {
    matches!(
        character,
        ' ' | '\t' | '\u{000b}' | '\u{000c}' | '\u{0085}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200b}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}
