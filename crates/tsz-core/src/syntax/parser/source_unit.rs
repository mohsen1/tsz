use crate::source::Span;

use super::{ParseOutput, Parser};
use crate::syntax::{
    CommentKind, CommentSourcePosition, SourceUnit, TokenKind, parse_source_check_directive,
};

impl Parser<'_> {
    pub(super) fn parse(mut self) -> ParseOutput {
        let mut statements = Vec::new();
        while !self.at(TokenKind::EndOfFile) {
            let before = self.index;
            statements.push(self.parse_statement_at_current_depth());
            if self.index == before {
                self.bump();
            }
        }
        self.finish_no_substitution_template_source(&statements);
        self.finish_extended_unicode_string_source(&statements);
        self.finish_regular_expression_source(&statements);
        self.finish_numeric_recovery_source(&statements);
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
                authored_literal_facts,
                parser_recovery_facts,
                unmodeled_declaration_hosts: self.unmodeled_declaration_hosts,
                source_check_directive,
                source_syntax_facts: self.source_syntax_facts.into_iter().collect(),
                comments: self.comments,
                has_unicode_line_comment_terminator: self.has_unicode_line_comment_terminator,
            },
            diagnostics: self.diagnostics,
        }
    }
}
