use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    pub(super) fn has_explicit_any_generic_variable_annotation(&self, diag_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(diag_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return false;
        }
        let Some(decl) = self.ctx.arena.get_variable_declaration(node) else {
            return false;
        };
        if decl.type_annotation == NodeIndex::NONE {
            return false;
        }
        self.type_annotation_contains_explicit_any_type_argument(decl.type_annotation)
    }

    fn type_annotation_contains_explicit_any_type_argument(
        &self,
        type_annotation: NodeIndex,
    ) -> bool {
        let mut stack = vec![type_annotation];
        while let Some(current) = stack.pop() {
            let Some(node) = self.ctx.arena.get(current) else {
                continue;
            };
            if node.kind == syntax_kind_ext::TYPE_REFERENCE
                && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
                && let Some(type_args) = &type_ref.type_arguments
                && type_args
                    .nodes
                    .iter()
                    .any(|&arg| self.type_node_contains_any_keyword(arg))
            {
                return true;
            }
            stack.extend(self.ctx.arena.get_children(current));
        }
        false
    }

    fn type_node_contains_any_keyword(&self, type_node: NodeIndex) -> bool {
        let mut stack = vec![type_node];
        while let Some(current) = stack.pop() {
            let Some(node) = self.ctx.arena.get(current) else {
                continue;
            };
            if node.kind == SyntaxKind::AnyKeyword as u16
                || self.is_bare_any_keyword_type_reference(node)
            {
                return true;
            }
            stack.extend(self.ctx.arena.get_children(current));
        }
        false
    }

    fn is_bare_any_keyword_type_reference(&self, node: &tsz_parser::parser::node::Node) -> bool {
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return false;
        };
        if type_ref
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty())
        {
            return false;
        }
        let Some(name_node) = self.ctx.arena.get(type_ref.type_name) else {
            return false;
        };
        if name_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        self.ctx
            .arena
            .get_identifier(name_node)
            .is_some_and(|ident| ident.escaped_text == "any")
    }
}
