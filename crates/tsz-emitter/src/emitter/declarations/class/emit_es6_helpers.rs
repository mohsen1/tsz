use super::super::super::Printer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::base::NodeList;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn class_expression_is_in_loop_body(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        while let Some(ext) = self.arena.get_extended(current) {
            let parent = ext.parent;
            if parent.is_none() {
                return false;
            }

            let Some(current_node) = self.arena.get(current) else {
                return false;
            };
            let Some(parent_node) = self.arena.get(parent) else {
                return false;
            };

            if current_node.kind == syntax_kind_ext::BLOCK
                && (parent_node.kind == syntax_kind_ext::FOR_STATEMENT
                    || parent_node.kind == syntax_kind_ext::FOR_IN_STATEMENT
                    || parent_node.kind == syntax_kind_ext::FOR_OF_STATEMENT
                    || parent_node.kind == syntax_kind_ext::WHILE_STATEMENT
                    || parent_node.kind == syntax_kind_ext::DO_STATEMENT)
            {
                return true;
            }

            if parent_node.kind == syntax_kind_ext::SOURCE_FILE
                || parent_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || parent_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || parent_node.kind == syntax_kind_ext::ARROW_FUNCTION
            {
                return false;
            }

            current = parent;
        }

        false
    }

    pub(super) fn is_reserved_private_constructor_name(name: &str) -> bool {
        name == "constructor"
    }

    pub(in crate::emitter) fn emit_class_es6(&mut self, node: &Node, idx: NodeIndex) {
        self.emit_class_es6_with_options(node, idx, false, None, None, None, false);
    }

    pub(super) fn emit_recovered_top_level_accessor_class_modifier(
        &mut self,
        modifiers: &Option<NodeList>,
        suppress_modifiers: bool,
    ) {
        if !suppress_modifiers
            && self.ctx.options.target == crate::emitter::ScriptTarget::ESNext
            && self
                .arena
                .has_modifier(modifiers, SyntaxKind::AccessorKeyword)
        {
            self.write("accessor ");
        }
    }
}
