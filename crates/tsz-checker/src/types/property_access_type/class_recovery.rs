//! Recovery helpers for property access during class construction.

use crate::classes_domain::class_summary::ClassChainSummary;
use crate::state::CheckerState;
use std::rc::Rc;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn resolve_class_access_with_current_member_initializer_recovery(
        &mut self,
        expression: NodeIndex,
        receiver_type: TypeId,
    ) -> (Option<(NodeIndex, bool)>, bool) {
        let mut resolved = self.resolve_class_for_access(expression, receiver_type);
        let recovery = self.resolve_current_class_member_initializer_access_for_recovery(
            expression,
            receiver_type,
        );
        if resolved.is_none() {
            resolved = recovery;
        }
        let is_current = recovery.is_some()
            || self.property_access_is_current_class_member_initializer_receiver(
                expression,
                receiver_type,
            );
        (resolved, is_current)
    }

    pub(crate) fn resolve_current_class_member_initializer_access_for_recovery(
        &mut self,
        expression: NodeIndex,
        receiver_type: TypeId,
    ) -> Option<(NodeIndex, bool)> {
        let (class_idx, class_sym) = self.current_class_member_initializer_context(expression)?;
        if self.property_access_receiver_symbol(receiver_type) != Some(class_sym)
            && !self.asserted_this_receiver_targets_current_class(expression, class_sym)
        {
            return None;
        }

        let is_static_access = self.find_enclosing_static_block(expression).is_some()
            || self.is_in_static_class_member_context(expression)
            || self.is_constructor_type(receiver_type);
        Some((class_idx, is_static_access))
    }

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
        if !self.ctx.class_instance_resolution_set.contains(&class_sym) {
            return false;
        }
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

    fn current_class_member_initializer_context(
        &self,
        expression: NodeIndex,
    ) -> Option<(NodeIndex, tsz_binder::SymbolId)> {
        let class_idx = self.nearest_enclosing_class(expression)?;
        let class_sym = self.ctx.binder.get_node_symbol(class_idx)?;
        if self.ctx.checking_computed_property_name.is_none()
            && !self.property_access_is_in_class_property_initializer(expression)
        {
            return None;
        }
        Some((class_idx, class_sym))
    }

    fn asserted_this_receiver_targets_current_class(
        &mut self,
        expression: NodeIndex,
        class_sym: tsz_binder::SymbolId,
    ) -> bool {
        let mut current = expression;
        let mut saw_assertion = false;
        let mut guard = 0;
        while current.is_some() {
            guard += 1;
            if guard > 64 {
                return false;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };
            match node.kind {
                syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    let Some(paren) = self.ctx.arena.get_parenthesized(node) else {
                        return false;
                    };
                    current = paren.expression;
                }
                syntax_kind_ext::AS_EXPRESSION | syntax_kind_ext::TYPE_ASSERTION => {
                    let Some(assertion) = self.ctx.arena.get_type_assertion(node) else {
                        return false;
                    };
                    let assertion_expression = assertion.expression;
                    let assertion_type_node = assertion.type_node;
                    let assertion_type = self.get_type_from_type_node(assertion_type_node);
                    if self.property_access_receiver_symbol(assertion_type) != Some(class_sym) {
                        return false;
                    }
                    saw_assertion = true;
                    current = assertion_expression;
                }
                _ => return saw_assertion && self.is_this_expression(current),
            }
        }
        false
    }

    pub(super) fn recover_property_from_class_chain_summary(
        &mut self,
        is_current_class_member_initializer_receiver: bool,
        resolved_class_access: Option<(NodeIndex, bool)>,
        summary: &mut Option<Rc<ClassChainSummary>>,
        property_name: &str,
    ) -> Option<TypeId> {
        if !is_current_class_member_initializer_receiver {
            return None;
        }
        let (class_idx, is_static_access) = resolved_class_access?;
        if summary.is_none() {
            *summary = Some(self.summarize_class_chain(class_idx));
        }
        summary
            .as_ref()?
            .member_info(property_name, is_static_access, true)
            .map(|member| member.type_id)
    }

    pub(super) fn recover_direct_this_class_chain_member(
        &mut self,
        direct_class_this_receiver: bool,
        used_class_chain_method_type: bool,
        receiver_expr: NodeIndex,
        property_name: &str,
        prop_type: TypeId,
        object_type_for_access: TypeId,
        original_object_type: TypeId,
    ) -> Option<(TypeId, bool)> {
        if used_class_chain_method_type
            || !direct_class_this_receiver
            || object_type_for_access != original_object_type
            || self.enclosing_class_declares_member(property_name)
        {
            return None;
        }

        let summary = self.summarize_class_chain(self.nearest_enclosing_class(receiver_expr)?);
        let member = summary.member_info(property_name, false, true)?;
        if member.from_interface
            || matches!(
                member.type_id,
                TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR
            )
            || member.type_id == prop_type
        {
            return None;
        }

        Some((member.type_id, member.is_method || member.is_accessor))
    }

    fn enclosing_class_declares_member(&self, property_name: &str) -> bool {
        self.ctx.enclosing_class.as_ref().is_some_and(|class_info| {
            class_info.member_nodes.iter().any(|&member_idx| {
                self.get_member_name(member_idx).as_deref() == Some(property_name)
            })
        })
    }

    pub(super) fn substitute_direct_this_property_shape_type(
        &self,
        direct_class_this_receiver: bool,
        used_class_chain_method_type: bool,
        object_type_for_access: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        if used_class_chain_method_type || !direct_class_this_receiver {
            return None;
        }

        let shape = crate::query_boundaries::common::object_shape_for_type(
            self.ctx.types,
            object_type_for_access,
        )?;
        let raw_prop = shape
            .properties
            .iter()
            .find(|prop| self.ctx.types.resolve_atom_ref(prop.name).as_ref() == property_name)?;
        crate::query_boundaries::common::contains_this_type(self.ctx.types, raw_prop.type_id).then(
            || {
                crate::query_boundaries::common::substitute_this_type(
                    self.ctx.types,
                    raw_prop.type_id,
                    self.ctx.types.this_type(),
                )
            },
        )
    }

    pub(super) fn has_recoverable_current_class_member(
        &mut self,
        is_current_class_member_initializer_receiver: bool,
        resolved_class_access: Option<(NodeIndex, bool)>,
        summary: &mut Option<Rc<ClassChainSummary>>,
        property_name: &str,
    ) -> bool {
        if !is_current_class_member_initializer_receiver {
            return false;
        }
        let Some((class_idx, is_static_access)) = resolved_class_access else {
            return false;
        };
        if summary.is_none() {
            *summary = Some(self.summarize_class_chain(class_idx));
        }
        summary.as_ref().is_some_and(|summary| {
            summary
                .member_info(property_name, is_static_access, true)
                .is_some()
        })
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
