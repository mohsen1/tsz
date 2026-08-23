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
        let has_authored_no_substitution_template =
            self.finish_no_substitution_template_source(&statements);
        let has_authored_extended_unicode_string =
            self.finish_extended_unicode_string_source(&statements);
        let has_authored_regular_expression = self.finish_regular_expression_source(&statements);
        let has_authored_numeric_recovery = self.finish_numeric_recovery_source(&statements);
        let has_authored_numeric_separator = self.finish_numeric_separator_source();
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
                function_products_supported: self.product_capabilities.functions_supported,
                class_products_supported: self.product_capabilities.classes_supported,
                declaration_products_supported: self.product_capabilities.declarations_supported,
                declaration_hosts_supported: self.product_capabilities.declaration_hosts_supported,
                default_export_hosts_supported: self
                    .product_capabilities
                    .default_export_hosts_supported,
                expression_products_supported: self
                    .product_capabilities
                    .expression_products_supported,
                comments: self.comments,
                has_unicode_line_comment_terminator: self.has_unicode_line_comment_terminator,
                has_authored_no_substitution_template,
                template_products_supported: self.product_capabilities.template_products_supported,
                has_authored_extended_unicode_string,
                extended_unicode_string_products_supported: self
                    .product_capabilities
                    .extended_unicode_string_products_supported,
                has_authored_regular_expression,
                regular_expression_products_supported: self
                    .product_capabilities
                    .regular_expression_products_supported,
                has_authored_numeric_recovery,
                numeric_recovery_products_supported: self
                    .product_capabilities
                    .numeric_recovery_products_supported,
                has_authored_numeric_separator,
                numeric_separator_products_supported: self
                    .product_capabilities
                    .numeric_separator_products_supported,
                commonjs_class_products_supported: self
                    .product_capabilities
                    .commonjs_classes_supported(),
            },
            diagnostics: self.diagnostics,
        }
    }
}
