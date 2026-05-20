use super::super::Printer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn expression_statement_consumed_invalid_backslash_semicolon(
        &self,
        node: &Node,
        expression: NodeIndex,
    ) -> bool {
        let (Some(source_text), Some(expr_node)) = (self.source_text, self.arena.get(expression))
        else {
            return false;
        };
        if matches!(
            expr_node.kind,
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == syntax_kind_ext::TEMPLATE_EXPRESSION
                || k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
        ) {
            return false;
        }
        let start = expr_node.end as usize;
        let end = node.end as usize;
        start < end
            && end <= source_text.len()
            && source_text[start..end].contains('\\')
            && source_text[start..end].contains(';')
    }
}
