//! Recovery helpers for property access during class construction.

use crate::classes_domain::class_summary::ClassChainSummary;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn property_access_is_current_class_construction_recovery(
        &self,
        expression: NodeIndex,
        receiver_type: TypeId,
    ) -> bool {
        if !self
            .property_access_is_current_class_member_initializer_receiver(expression, receiver_type)
        {
            return false;
        }
        let Some(class_idx) = self.nearest_enclosing_class(expression) else {
            return false;
        };
        let Some(class_sym) = self.ctx.binder.get_node_symbol(class_idx) else {
            return false;
        };
        self.ctx.class_instance_resolution_set.contains(&class_sym)
    }

    pub(crate) fn property_access_is_current_class_member_initializer_receiver(
        &self,
        expression: NodeIndex,
        receiver_type: TypeId,
    ) -> bool {
        let Some(class_idx) = self.nearest_enclosing_class(expression) else {
            return false;
        };
        let Some(class_sym) = self.ctx.binder.get_node_symbol(class_idx) else {
            return false;
        };
        if self.ctx.checking_computed_property_name.is_none()
            && !self.property_access_is_in_class_property_initializer(expression)
        {
            return false;
        }

        self.property_access_receiver_symbol(receiver_type) == Some(class_sym)
    }

    pub(crate) fn property_access_receiver_symbol(
        &self,
        type_id: TypeId,
    ) -> Option<tsz_binder::SymbolId> {
        self.ctx.resolve_type_to_symbol_id(type_id).or_else(|| {
            crate::query_boundaries::common::application_info(self.ctx.types, type_id)
                .and_then(|(base, _)| self.ctx.resolve_type_to_symbol_id(base))
        })
    }

    pub(super) fn recover_property_from_class_chain_summary(
        &self,
        expression: NodeIndex,
        receiver_type: TypeId,
        resolved_class_access: Option<(NodeIndex, bool)>,
        summary: Option<&ClassChainSummary>,
        property_name: &str,
    ) -> Option<TypeId> {
        if !self
            .property_access_is_current_class_member_initializer_receiver(expression, receiver_type)
        {
            return None;
        }
        let (_, is_static_access) = resolved_class_access?;
        summary?
            .member_info(property_name, is_static_access, true)
            .map(|member| member.type_id)
    }

    fn property_access_is_in_class_property_initializer(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        let mut iterations = 0;
        while current.is_some() {
            iterations += 1;
            if iterations > 1024 {
                return false;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };
            match node.kind {
                syntax_kind_ext::PROPERTY_DECLARATION => {
                    return self
                        .ctx
                        .arena
                        .get_property_decl(node)
                        .is_some_and(|prop| prop.initializer.is_some());
                }
                // Nested function bodies are checked with their own or lexical `this`;
                // they are not part of the initializer expression being recovered.
                syntax_kind_ext::ARROW_FUNCTION
                | syntax_kind_ext::FUNCTION_EXPRESSION
                | syntax_kind_ext::FUNCTION_DECLARATION => {
                    return false;
                }
                syntax_kind_ext::METHOD_DECLARATION
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR
                | syntax_kind_ext::CONSTRUCTOR
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::CLASS_EXPRESSION => return false,
                _ => {}
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            current = ext.parent;
        }
        false
    }
}
