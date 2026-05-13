use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    pub(super) fn super_accessor_error(
        &mut self,
        object_expr: NodeIndex,
        property_name: &str,
        error_node: NodeIndex,
        class_idx: NodeIndex,
        is_static: bool,
    ) -> bool {
        if !self.is_super_expression(object_expr)
            || self.is_in_static_class_member_context(error_node)
            || !self.is_inside_accessor_declaration(error_node)
            || !self.class_chain_member_is_accessor(class_idx, property_name, is_static)
            || self.is_property_access_write_context(error_node)
        {
            return false;
        }

        self.error_at_node(
            error_node,
            diagnostic_messages::ONLY_PUBLIC_AND_PROTECTED_METHODS_OF_THE_BASE_CLASS_ARE_ACCESSIBLE_VIA_THE_SUPER,
            diagnostic_codes::ONLY_PUBLIC_AND_PROTECTED_METHODS_OF_THE_BASE_CLASS_ARE_ACCESSIBLE_VIA_THE_SUPER,
        );
        true
    }

    fn is_inside_accessor_declaration(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        loop {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent_idx = ext.parent;
            let Some(parent) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            if matches!(
                parent.kind,
                syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR
            ) {
                return true;
            }
            if matches!(
                parent.kind,
                syntax_kind_ext::METHOD_DECLARATION
                    | syntax_kind_ext::CONSTRUCTOR
                    | syntax_kind_ext::PROPERTY_DECLARATION
                    | syntax_kind_ext::CLASS_DECLARATION
                    | syntax_kind_ext::CLASS_EXPRESSION
            ) {
                return false;
            }
            current = parent_idx;
        }
    }
}
