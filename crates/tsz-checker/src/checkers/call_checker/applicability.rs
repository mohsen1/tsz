//! Adapter methods for routing call/new resolution through the solver.

use super::CheckerCallAssignabilityAdapter;
use crate::query_boundaries::checkers::call::{resolve_call, resolve_new};
use crate::query_boundaries::common::CallResult;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn callable_context_can_type_function_argument_despite_unresolved(
        &self,
        arg_idx: NodeIndex,
        expected_context_type: Option<TypeId>,
    ) -> bool {
        let Some(expected_context_type) = expected_context_type else {
            return false;
        };
        if !self.is_callback_like_argument(arg_idx) {
            return false;
        }

        if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(
            self.ctx.types,
            expected_context_type,
        ) {
            return shape
                .params
                .iter()
                .all(|param| param.type_id != TypeId::UNKNOWN && param.type_id != TypeId::ERROR);
        }

        if let Some(shape) = crate::query_boundaries::common::callable_shape_for_type(
            self.ctx.types,
            expected_context_type,
        ) {
            return shape.call_signatures.iter().all(|sig| {
                sig.params
                    .iter()
                    .all(|param| param.type_id != TypeId::UNKNOWN && param.type_id != TypeId::ERROR)
            });
        }

        false
    }

    pub(super) fn normalized_spread_argument_type(&mut self, expr: NodeIndex) -> TypeId {
        let spread_type = self.get_type_of_node(expr);
        let spread_type = self.resolve_type_for_property_access(spread_type);
        let spread_type = self.resolve_lazy_type(spread_type);
        let spread_type = self.evaluate_type_with_env(spread_type);
        let spread_type = self.resolve_type_for_property_access(spread_type);
        let spread_type = self.resolve_lazy_type(spread_type);
        self.evaluate_application_type(spread_type)
    }

    /// Const object/array literal bindings do not benefit from flow narrowing at
    /// call sites. Skipping flow narrowing for these stable identifiers avoids
    /// repeated CFG traversals on large argument objects.
    pub(super) fn can_skip_flow_narrowing_for_argument(&self, idx: NodeIndex) -> bool {
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }

        let Some(sym_id) = self
            .ctx
            .binder
            .get_node_symbol(idx)
            .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, idx))
        else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let value_decl = symbol.value_declaration;
        if value_decl.is_none() || !self.is_const_variable_declaration(value_decl) {
            return false;
        }

        let Some(decl_node) = self.ctx.arena.get(value_decl) else {
            return false;
        };
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        if var_decl.type_annotation.is_some() || var_decl.initializer.is_none() {
            return false;
        }

        let Some(init_node) = self.ctx.arena.get(var_decl.initializer) else {
            return false;
        };
        init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            || init_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
    }

    pub(crate) fn resolve_call_with_checker_adapter(
        &mut self,
        func_type: TypeId,
        arg_types: &[TypeId],
        force_bivariant_callbacks: bool,
        contextual_type: Option<TypeId>,
        actual_this_type: Option<TypeId>,
    ) -> tsz_solver::operations::CallWithCheckerResult {
        self.ensure_relation_input_ready(func_type);
        self.ensure_relation_inputs_ready(arg_types);

        let db = self.ctx.types;
        let mut checker = CheckerCallAssignabilityAdapter { state: self };
        resolve_call(
            db,
            &mut checker,
            func_type,
            arg_types,
            force_bivariant_callbacks,
            contextual_type,
            actual_this_type,
        )
    }

    pub(crate) fn resolve_new_with_checker_adapter(
        &mut self,
        type_id: TypeId,
        arg_types: &[TypeId],
        force_bivariant_callbacks: bool,
        contextual_type: Option<TypeId>,
    ) -> CallResult {
        self.ensure_relation_input_ready(type_id);
        self.ensure_relation_inputs_ready(arg_types);

        let db = self.ctx.types;
        let mut checker = CheckerCallAssignabilityAdapter { state: self };
        resolve_new(
            db,
            &mut checker,
            type_id,
            arg_types,
            force_bivariant_callbacks,
            contextual_type,
        )
    }
}
