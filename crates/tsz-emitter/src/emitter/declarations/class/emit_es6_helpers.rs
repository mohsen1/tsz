use super::super::super::Printer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::base::NodeList;
use tsz_parser::parser::node::{ClassData, Node};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::syntax::transform_utils::{contains_super_reference, contains_this_reference};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn class_computed_property_names_contain_static_context_reference(
        &self,
        class: &ClassData,
    ) -> bool {
        class.members.nodes.iter().any(|&member_idx| {
            let Some(member_node) = self.arena.get(member_idx) else {
                return false;
            };
            let name_idx = match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                    .arena
                    .get_property_decl(member_node)
                    .map(|prop| prop.name),
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .arena
                    .get_method_decl(member_node)
                    .map(|method| method.name),
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    self.arena
                        .get_accessor(member_node)
                        .map(|accessor| accessor.name)
                }
                _ => None,
            };
            let Some(name_idx) = name_idx else {
                return false;
            };
            let Some(name_node) = self.arena.get(name_idx) else {
                return false;
            };
            if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                return false;
            }
            self.arena
                .get_computed_property(name_node)
                .is_some_and(|computed| {
                    contains_this_reference(self.arena, computed.expression)
                        || contains_super_reference(self.arena, computed.expression)
                })
        })
    }

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
