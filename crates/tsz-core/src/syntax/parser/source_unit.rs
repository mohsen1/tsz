use crate::source::{SourceText, Span};

use super::{ParseOutput, Parser};
use crate::syntax::{
    CommentKind, CommentSourcePosition, CommentTrivia, JavaScriptJSDocCastKind, SourceUnit, Token,
    TokenKind, parse_source_check_directive,
};

pub(super) fn source_is_external_module(source: &SourceText, tokens: &[Token]) -> bool {
    let implicit_module = source
        .path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "mts" | "cts" | "mjs" | "cjs"
            )
        });
    if implicit_module {
        return true;
    }
    let mut brace_depth = 0_u32;
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::LeftBrace => brace_depth += 1,
            TokenKind::RightBrace => brace_depth = brace_depth.saturating_sub(1),
            TokenKind::Export if brace_depth == 0 => return true,
            TokenKind::Import
                if brace_depth == 0
                    && tokens.get(index + 1).map(|token| token.kind)
                        != Some(TokenKind::LeftParen) =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}

impl Parser<'_> {
    /// Whether the current token's trivia range contains an authored JSDoc
    /// comment. Token boundaries, rather than declaration spelling, establish
    /// the association.
    pub(super) fn current_has_leading_jsdoc(&self) -> bool {
        self.current_leading_jsdoc().is_some()
    }

    pub(super) fn current_leading_jsdoc_cast_kind(&self) -> Option<JavaScriptJSDocCastKind> {
        self.current_leading_jsdoc()
            .and_then(|comment| jsdoc_cast_kind(self.source.slice(comment.span)))
    }

    fn current_leading_jsdoc(&self) -> Option<&CommentTrivia> {
        let start = self
            .index
            .checked_sub(1)
            .and_then(|index| self.tokens.get(index))
            .map_or(0, |token| token.span.end);
        let end = self.current().span.start;
        let first = self
            .comments
            .partition_point(|comment| comment.span.end <= start);
        let comment = self.comments[first..]
            .iter()
            .take_while(|comment| comment.span.start < end)
            .last()?;
        let end_line = self.source.position(end)?.0;
        let comment_end_line = self.source.position(comment.span.end)?.0;
        (comment.jsdoc && end_line <= comment_end_line.saturating_add(1)).then_some(comment)
    }

    pub(super) fn tokens_are_on_same_line(&self, left: usize, right: usize) -> bool {
        let Some(left) = self.tokens.get(left) else {
            return false;
        };
        let Some(right) = self.tokens.get(right) else {
            return false;
        };
        !self
            .source
            .slice(Span::new(
                self.source.id,
                left.span.end as usize,
                right.span.start as usize,
            ))
            .chars()
            .any(|character| matches!(character, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
    }

    pub(super) fn parse(mut self) -> ParseOutput {
        let mut statements = Vec::new();
        while !self.at(TokenKind::EndOfFile) {
            let before = self.index;
            statements.push(self.parse_statement_at_current_depth());
            if self.index == before {
                self.bump();
            }
        }
        self.finish_regular_expression_source();
        self.finish_numeric_recovery_source();
        self.finish_numeric_separator_source();
        let parser_recovery_facts = self.finish_parser_recovery_facts(&statements);
        let authored_literal_facts =
            self.authored_literal_facts(&statements, &parser_recovery_facts);
        let source_check_directive = self
            .comments
            .iter()
            .filter(|comment| {
                comment.kind == CommentKind::Line
                    && comment.source_position == CommentSourcePosition::SourceLeading
            })
            .filter_map(|comment| {
                parse_source_check_directive(self.source.slice(comment.span), comment.span)
            })
            .next_back();
        let end = self.source.text.len();
        ParseOutput {
            unit: SourceUnit {
                statements,
                span: Span::new(self.source.id, 0, end),
                identifier_token_spans: self
                    .tokens
                    .iter()
                    .filter(|token| token.kind.is_identifier())
                    .map(|token| token.span)
                    .collect(),
                authored_literal_facts,
                parser_recovery_facts,
                unmodeled_declaration_hosts: self.unmodeled_declaration_hosts,
                source_check_directive,
                source_syntax_facts: self.source_syntax_facts.into_iter().collect(),
                contextual_grammar_facts: self.contextual_grammar_facts,
                comments: self.comments,
                has_unicode_line_comment_terminator: self.has_unicode_line_comment_terminator,
            },
            diagnostics: self.diagnostics,
        }
    }
}

fn jsdoc_cast_kind(comment: &str) -> Option<JavaScriptJSDocCastKind> {
    let body = comment
        .strip_prefix("/**")
        .and_then(|body| body.strip_suffix("*/"))
        .unwrap_or(comment);
    let mut in_backticks = false;
    let mut line_prefix = true;
    let mut previous_whitespace = true;
    for (index, character) in body.char_indices() {
        if character == '`' {
            in_backticks = !in_backticks;
        } else if character == '@'
            && !in_backticks
            && (previous_whitespace || line_prefix)
            && let Some(kind) = jsdoc_cast_tag(&body[index + 1..])
        {
            return Some(kind);
        }
        if matches!(character, '\n' | '\r' | '\u{2028}' | '\u{2029}') {
            line_prefix = true;
        } else if !character.is_whitespace() && !(line_prefix && character == '*') {
            line_prefix = false;
        }
        previous_whitespace = character.is_whitespace();
    }
    None
}

fn jsdoc_cast_tag(text: &str) -> Option<JavaScriptJSDocCastKind> {
    [
        ("type", JavaScriptJSDocCastKind::Type),
        ("satisfies", JavaScriptJSDocCastKind::Satisfies),
    ]
    .into_iter()
    .find_map(|(name, kind)| {
        text.strip_prefix(name)
            .filter(|tail| {
                tail.chars().next().is_none_or(|character| {
                    !character.is_alphanumeric() && !matches!(character, '_' | '$')
                })
            })
            .map(|_| kind)
    })
}
