use super::super::Printer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn erased_statement_has_recovered_import_type_tail(
        &self,
        stmt_node: &Node,
    ) -> bool {
        if stmt_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
            return self.type_alias_has_recovered_import_type_tail(stmt_node);
        }

        if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
            && let Some(export) = self.arena.get_export_decl(stmt_node)
            && let Some(inner_node) = self.arena.get(export.export_clause)
            && inner_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
        {
            return self.type_alias_has_recovered_import_type_tail(inner_node);
        }

        false
    }

    fn type_alias_has_recovered_import_type_tail(&self, alias_node: &Node) -> bool {
        let Some(alias) = self.arena.get_type_alias(alias_node) else {
            return false;
        };
        let Some(type_node) = self.arena.get(alias.type_node) else {
            return false;
        };
        if type_node.kind != syntax_kind_ext::TYPE_QUERY {
            return false;
        }
        let Some(type_query) = self.arena.get_type_query(type_node) else {
            return false;
        };
        self.type_query_import_call_has_recovered_tail(type_node, type_query.expr_name)
    }

    fn type_query_import_call_has_recovered_tail(
        &self,
        type_query_node: &Node,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION
            || expr_node.end <= type_query_node.end
        {
            return false;
        }
        let Some(call) = self.arena.get_call_expr(expr_node) else {
            return false;
        };
        let Some(callee) = self.arena.get(call.expression) else {
            return false;
        };
        callee.kind == SyntaxKind::ImportKeyword as u16
            && call
                .arguments
                .as_ref()
                .is_some_and(|args| args.nodes.len() >= 2)
    }
}
