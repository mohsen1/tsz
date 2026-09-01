use crate::source::Span;
use crate::syntax::{
    CommentClass::{DetachedPinned, Pinned, TripleSlashReference},
    CommentKind, CommentPlacement,
    CommentSourcePosition::SourceLeading,
    CommentTrivia, Expression, SourceUnit, Statement, TokenKind,
};

use super::Printer;

/// Source-ordered comment identities consumed once across overlapping ranges.
#[derive(Default)]
pub(super) struct CommentIndex {
    comments: Vec<CommentTrivia>,
    next: usize,
}

impl CommentIndex {
    pub(super) fn reset(
        &mut self,
        comments: &[CommentTrivia],
        preserve_comments: bool,
        omitted: Option<Span>,
    ) {
        self.comments = comments
            .iter()
            .copied()
            .filter(|comment| {
                Some(comment.span) != omitted
                    && (preserve_comments || matches!(comment.class, Pinned | DetachedPinned))
            })
            .collect();
        self.next = 0;
    }
    fn take_prefix(
        &mut self,
        mut predicate: impl FnMut(CommentTrivia) -> bool,
    ) -> Vec<CommentTrivia> {
        let start = self.next;
        while self
            .comments
            .get(self.next)
            .is_some_and(|comment| predicate(*comment))
        {
            self.next += 1;
        }
        self.comments[start..self.next].to_vec()
    }
    fn take_before(&mut self, offset: u32) -> Vec<CommentTrivia> {
        self.take_prefix(|comment| comment.span.start < offset)
    }
}

pub(super) enum GapSeparator {
    None,
    Space,
    Indent,
    Newline,
    Hanging,
}

pub(super) enum GapOwner {
    End(u32),
    Kind(TokenKind, u32),
}
impl Printer<'_> {
    pub(super) fn write_detached_source_leading_comments(&mut self, unit: &SourceUnit) {
        let first = unit
            .statements
            .first()
            .map_or(unit.span.end, |s| s.span.start);
        let mut leading = unit
            .comments()
            .iter()
            .take_while(|comment| comment.source_position == SourceLeading);
        let detached_end = leading.next().and_then(|mut last| {
            for comment in leading {
                if self.has_blank_source_line(last.span.end, comment.span.start) {
                    break;
                }
                last = comment;
            }
            self.has_blank_source_line(last.span.end, first)
                .then_some(last.span.end)
        });
        if let Some(detached_end) = detached_end {
            let comments = self
                .comment_index
                .take_prefix(|c| c.span.end <= detached_end);
            self.write_comment_sequence(&comments, false, true);
        }
        if !self.preserve_comments {
            self.comment_index.next = self.comment_index.comments.len();
        }
    }
    fn has_blank_source_line(&self, previous_end: u32, next_start: u32) -> bool {
        let line = |offset| match self.source.position(offset) {
            Some((line, _)) => line,
            None => panic!("invalid scanner trivia offset"),
        };
        line(next_start) >= line(previous_end).saturating_add(2)
    }
    pub(super) fn finish_javascript_statements(&mut self, unit: &SourceUnit) {
        let _ = self.comment_index.take_before(
            unit.statements
                .last()
                .map_or(unit.span.start, |statement| statement.span.end),
        );
        let comments = self.comment_index.take_before(unit.span.end);
        self.write_comment_sequence(&comments, false, true);
        if comments.last().is_some_and(|comment| {
            comment.kind == CommentKind::Block && !comment.has_trailing_line_break
        }) && !self.output.chars().last().is_some_and(char::is_whitespace)
        {
            // Pinned TypeScript's printer terminates a final block-comment
            // token with its ordinary separator even when the source ends at
            // `*/`. This is observable output, not retained source trivia.
            self.output.push_str(" \n");
        }
    }
    pub(super) fn write_declaration_comments_before_node(&mut self, span: Span, emitted: bool) {
        let mut comments = self.comment_index.take_before(span.start);
        let mut attached_start = comments.len();
        let mut next_start = span.start;
        for (index, comment) in comments.iter().enumerate().rev() {
            if self.has_blank_source_line(comment.span.end, next_start) {
                break;
            }
            attached_start = index;
            next_start = comment.span.start;
        }
        comments = comments
            .into_iter()
            .enumerate()
            .filter_map(|(index, comment)| {
                let attached = emitted && index >= attached_start;
                (comment.class == DetachedPinned || self.preserve_comments && attached)
                    .then_some(comment)
            })
            .collect();
        self.write_comment_sequence(&comments, true, true);
    }
    pub(super) fn write_declaration_comments_after_node(&mut self, span: Span, emitted: bool) {
        let (_, trailing) = self.take_comments_after_node(span);
        if emitted && self.preserve_comments {
            self.write_node_trailing_comments(&trailing);
        }
    }
    pub(super) fn write_comments_before_node(&mut self, span: Span, emitted: bool) {
        let mut comments = self.comment_index.take_before(span.start);
        if !emitted {
            comments.retain(|comment| {
                comment.class == DetachedPinned
                    || comment.source_position == SourceLeading
                        && comment.class == TripleSlashReference
            });
        }
        self.write_comment_sequence(&comments, true, true);
    }
    /// Defensively reject emitted nodes with a comment slot not claimed earlier.
    pub(super) fn write_comments_after_node(&mut self, span: Span, emitted: bool) {
        let (unresolved, trailing) = self.take_comments_after_node(span);
        self.javascript_supported &= !emitted || !unresolved;
        if emitted {
            self.write_node_trailing_comments(&trailing);
        }
    }
    fn take_comments_after_node(&mut self, span: Span) -> (bool, Vec<CommentTrivia>) {
        let unresolved = !self.comment_index.take_before(span.end).is_empty();
        let trailing = self.comment_index.take_prefix(|comment| {
            comment.placement == CommentPlacement::Trailing
                && comment.preceding_token_end == Some(span.end)
        });
        (unresolved, trailing)
    }
    pub(super) fn write_comments_before(&mut self, offset: u32) -> bool {
        let comments = self.comment_index.take_before(offset);
        self.write_comment_sequence(&comments, true, true)
    }
    pub(super) fn write_gap(
        &mut self,
        owner: GapOwner,
        followed_by_token: bool,
        separator: GapSeparator,
    ) -> (bool, bool) {
        let comments = self.comment_index.take_prefix(|comment| match owner {
            GapOwner::End(end) => comment.preceding_token_end == Some(end),
            GapOwner::Kind(kind, before) => {
                comment.preceding_token_kind == Some(kind) && comment.span.end <= before
            }
        });
        let broke_line = comments.iter().any(|comment| {
            comment.placement == CommentPlacement::Leading
                || comment.kind == CommentKind::Line
                || comment.has_trailing_line_break
        });
        let ended_line = self.write_comment_sequence(&comments, followed_by_token, true);
        match separator {
            GapSeparator::Space | GapSeparator::Hanging if !ended_line => self.output.push(' '),
            GapSeparator::Space | GapSeparator::Indent if ended_line => self.write_indent(),
            GapSeparator::Newline if !ended_line => self.write_newline(),
            GapSeparator::Hanging if ended_line => {
                self.indent += 1;
                self.write_indent();
                self.indent = self.indent.saturating_sub(1);
            }
            _ => {}
        }
        (ended_line, broke_line)
    }
    pub(super) fn write_comments_before_close(&mut self, end: u32) -> bool {
        let comments = self.comment_index.take_before(end);
        let followed_by_token = comments
            .last()
            .is_some_and(|comment| comment.placement == CommentPlacement::Leading);
        self.write_comment_sequence(&comments, followed_by_token, true)
    }
    pub(super) fn consume_comments_through_token(&mut self, end: u32) {
        let _ = self
            .comment_index
            .take_prefix(|c| c.preceding_token_end.is_some_and(|start| start <= end));
    }
    pub(super) fn consume_comments_before(&mut self, offset: u32) {
        let _ = self.comment_index.take_before(offset);
    }

    pub(super) fn consume_parameter_close_comments(&mut self) {
        let _ = self
            .comment_index
            .take_prefix(|comment| comment.preceding_token_kind == Some(TokenKind::Comma));
    }

    pub(super) fn write_commented_expression_statement(
        &mut self,
        statement: &Statement,
        expression: &Expression,
    ) {
        self.write_indent();
        self.write_expression_statement_expression(expression);
        if statement.span.end > expression.span.end {
            self.write_gap(
                GapOwner::End(expression.span.end),
                true,
                GapSeparator::Indent,
            );
        }
        self.output.push_str(";\n");
    }

    fn write_node_trailing_comments(&mut self, comments: &[CommentTrivia]) {
        if comments.is_empty() {
            return;
        }
        let restore_line_break = self.output.ends_with('\n');
        if restore_line_break {
            self.output.pop();
        }
        self.write_comment_sequence(comments, false, false);
        if restore_line_break && !self.output.ends_with('\n') {
            self.output.push('\n');
        }
    }

    fn write_comment_sequence(
        &mut self,
        comments: &[CommentTrivia],
        followed_by_token: bool,
        indent_at_line_start: bool,
    ) -> bool {
        let mut wrote = false;
        for (index, comment) in comments.iter().copied().enumerate() {
            wrote = true;
            let separated_before = self.output.chars().last().is_some_and(char::is_whitespace);
            if comment.placement == CommentPlacement::Leading
                && !self.output.is_empty()
                && !self.output.ends_with('\n')
            {
                self.output.push('\n');
            } else if comment.placement == CommentPlacement::Trailing
                && !self.output.chars().last().is_some_and(char::is_whitespace)
            {
                self.output.push(' ');
            }
            if indent_at_line_start && self.output.ends_with('\n') {
                self.write_indent();
            }
            self.output
                .push_str(&self.source.slice(comment.span).replace("\r\n", "\n"));
            if comment.kind == CommentKind::Line
                || comment.has_trailing_line_break
                    && (comment.placement == CommentPlacement::Leading || !followed_by_token)
            {
                if !self.output.ends_with('\n') {
                    self.output.push('\n');
                }
            } else if comment.kind == CommentKind::Block
                && (followed_by_token || index + 1 < comments.len())
                && (separated_before || comment.placement == CommentPlacement::Leading)
                && !self.output.chars().last().is_some_and(char::is_whitespace)
            {
                self.output.push(' ');
            }
        }
        wrote && self.output.ends_with('\n')
    }
}
