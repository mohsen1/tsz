use crate::query_boundaries::type_computation::complex as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn application_bases_are_same_nominal_type(
        &self,
        left: TypeId,
        right: TypeId,
    ) -> bool {
        if left == right {
            return true;
        }

        if let (Some(left), Some(right)) = (
            self.application_base_symbol_id(left),
            self.application_base_symbol_id(right),
        ) {
            return left == right;
        }

        false
    }

    pub(crate) fn application_base_symbol_id(&self, base: TypeId) -> Option<tsz_binder::SymbolId> {
        if let Some(def_id) = query::lazy_def_id(self.ctx.types, base) {
            return self.ctx.def_to_symbol_id(def_id);
        }
        crate::query_boundaries::common::type_query_symbol(self.ctx.types, base)
            .map(|sym_ref| tsz_binder::SymbolId(sym_ref.0))
            .or_else(|| {
                self.ctx
                    .types
                    .get_display_alias(base)
                    .and_then(|alias| query::get_application_info(self.ctx.types, alias))
                    .and_then(|(alias_base, _)| self.application_base_symbol_id(alias_base))
            })
    }

    pub(crate) fn is_same_class_static_method_new_result(
        &self,
        new_expr_idx: NodeIndex,
        callee_expr: NodeIndex,
        contextual_type: TypeId,
    ) -> bool {
        let Some(enclosing_class) = self.ctx.enclosing_class.as_ref() else {
            return false;
        };

        if !self.is_in_static_class_method_context(new_expr_idx) {
            return false;
        }

        let Some(enclosing_sym) = self
            .ctx
            .binder
            .node_symbols
            .get(&enclosing_class.class_idx.0)
            .copied()
        else {
            return false;
        };

        let Some(target_sym) = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, callee_expr)
            .or_else(|| self.ctx.binder.get_node_symbol(callee_expr))
            .or_else(|| self.resolve_qualified_symbol(callee_expr))
        else {
            return false;
        };

        if target_sym != enclosing_sym {
            return false;
        }

        query::get_application_info(self.ctx.types, contextual_type)
            .and_then(|(ctx_base, _)| self.application_base_symbol_id(ctx_base))
            == Some(enclosing_sym)
    }

    pub(crate) fn contextual_application_directly_supplies_type_parameters(
        &self,
        result_type: TypeId,
        contextual_type: TypeId,
    ) -> bool {
        let result_app = query::get_application_info(self.ctx.types, result_type).or_else(|| {
            self.ctx
                .types
                .get_display_alias(result_type)
                .and_then(|alias| query::get_application_info(self.ctx.types, alias))
        });
        let contextual_app = query::get_application_info(self.ctx.types, contextual_type);

        let (Some((result_base, result_args)), Some((ctx_base, ctx_args))) =
            (result_app, contextual_app)
        else {
            return false;
        };

        self.application_bases_are_same_nominal_type(result_base, ctx_base)
            && result_args.len() == ctx_args.len()
            && !result_args.is_empty()
            && result_args.iter().all(|&arg| arg == TypeId::UNKNOWN)
            && ctx_args
                .iter()
                .any(|&arg| query::type_parameter_info(self.ctx.types, arg).is_some())
            && ctx_args.iter().all(|&arg| {
                arg == TypeId::UNKNOWN
                    || arg == TypeId::ERROR
                    || query::type_parameter_info(self.ctx.types, arg).is_some()
            })
    }

    fn is_in_static_class_method_context(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            CLASS_DECLARATION, CLASS_EXPRESSION, METHOD_DECLARATION,
        };

        let mut current = idx;
        let mut iterations = 0;
        while current.is_some() {
            iterations += 1;
            if iterations > 256 {
                return false;
            }

            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };

            match node.kind {
                k if k == METHOD_DECLARATION => {
                    return self
                        .ctx
                        .arena
                        .get_method_decl(node)
                        .is_some_and(|method| self.has_static_modifier(&method.modifiers));
                }
                k if k == CLASS_DECLARATION || k == CLASS_EXPRESSION => return false,
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
