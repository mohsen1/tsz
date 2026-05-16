//! Syntax-guided constraint instantiation helpers.

use crate::query_boundaries::checkers::generic as query;
use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver::{TypeId, TypeParamInfo};

impl<'a> CheckerState<'a> {
    pub(super) fn type_arg_is_unknown_keyword(&self, type_arg_idx: NodeIndex) -> bool {
        self.node_text(type_arg_idx)
            .is_some_and(|text| text.trim() == "unknown")
            || self
                .type_arg_identifier_name(type_arg_idx)
                .is_some_and(|name| name == "unknown")
            || self
                .ctx
                .arena
                .get(type_arg_idx)
                .is_some_and(|node| node.kind == SyntaxKind::UnknownKeyword as u16)
    }

    pub(super) fn syntax_instantiated_type_arg_satisfies_constraint(
        &mut self,
        type_arg: TypeId,
        type_arg_idx: NodeIndex,
        type_params: &[TypeParamInfo],
        type_args: &[TypeId],
        constraint: TypeId,
    ) -> bool {
        let constraint_resolved = self.resolve_lazy_type(constraint);
        let inst_constraint =
            self.instantiate_constraint_with_type_args(constraint_resolved, type_params, type_args);
        if matches!(
            inst_constraint,
            TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR
        ) || query::contains_type_parameters(self.ctx.types, inst_constraint)
        {
            return false;
        }
        let db = self.ctx.types.as_type_database();
        let constraint_is_callable = query::is_callable_type(db, inst_constraint)
            || self.is_function_constraint(constraint_resolved)
            || self.is_function_constraint(inst_constraint);

        let resolved_type_arg = self.resolve_lazy_type(type_arg);
        if matches!(resolved_type_arg, TypeId::ANY | TypeId::ERROR) {
            return false;
        }

        let Some(instantiated_type_arg) =
            self.instantiate_type_ref_argument_from_syntax(type_arg, type_arg_idx)
        else {
            return false;
        };
        if instantiated_type_arg == type_arg
            || matches!(instantiated_type_arg, TypeId::ANY | TypeId::ERROR)
            || query::contains_type_parameters(self.ctx.types, instantiated_type_arg)
        {
            return false;
        }

        let syntax_instantiated_type_arg =
            self.evaluate_type_for_assignability(instantiated_type_arg);

        if matches!(syntax_instantiated_type_arg, TypeId::ANY | TypeId::ERROR)
            || query::contains_type_parameters(self.ctx.types, syntax_instantiated_type_arg)
        {
            return false;
        }

        if constraint_is_callable {
            let db = self.ctx.types.as_type_database();
            return query::is_callable_type(db, syntax_instantiated_type_arg)
                || query::callable_shape_for_type(db, syntax_instantiated_type_arg).is_some()
                || self.is_assignable_to(syntax_instantiated_type_arg, inst_constraint);
        }

        self.is_assignable_to(syntax_instantiated_type_arg, inst_constraint)
            || self.base_union_members_satisfy_constraint(
                syntax_instantiated_type_arg,
                inst_constraint,
            )
            || self.satisfies_array_like_constraint(syntax_instantiated_type_arg, inst_constraint)
    }

    pub(crate) fn instantiate_type_ref_argument_from_syntax(
        &mut self,
        type_arg: TypeId,
        type_arg_idx: NodeIndex,
    ) -> Option<TypeId> {
        let cache_key = (
            self.ctx.current_file_idx,
            type_arg_idx.0,
            type_arg,
            self.type_reference_arg_validation_scope_key(),
        );
        if let Some(cached) = self
            .ctx
            .type_reference_validation_caches
            .syntax_instantiation
            .get(&cache_key)
            .copied()
        {
            return cached;
        }
        let result =
            self.instantiate_type_ref_argument_from_syntax_inner(type_arg, type_arg_idx, 0);
        self.ctx
            .type_reference_validation_caches
            .syntax_instantiation
            .insert(cache_key, result);
        result
    }

    fn instantiate_type_ref_argument_from_syntax_inner(
        &mut self,
        type_arg: TypeId,
        type_arg_idx: NodeIndex,
        depth: usize,
    ) -> Option<TypeId> {
        if depth > 4 {
            return None;
        }
        let node = self.ctx.arena.get(type_arg_idx)?;
        let type_ref = self.ctx.arena.get_type_ref(node)?;

        let mut sym_id = match self.resolve_identifier_symbol_in_type_position(type_ref.type_name) {
            TypeSymbolResolution::Type(sym_id) => Some(sym_id),
            _ => match self.resolve_qualified_symbol_in_type_position(type_ref.type_name) {
                TypeSymbolResolution::Type(sym_id) => Some(sym_id),
                _ => None,
            },
        }?;
        let mut visited = crate::symbols_domain::alias_cycle::AliasCycleTracker::new();
        if let Some(target_sym_id) = self.resolve_alias_symbol(sym_id, &mut visited) {
            sym_id = target_sym_id;
        }
        if self
            .get_cross_file_symbol(sym_id)
            .is_some_and(|symbol| symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS))
        {
            let imported_target = self.get_cross_file_symbol(sym_id).and_then(|symbol| {
                let module_name = symbol.import_module.as_ref()?;
                let import_name = symbol
                    .import_name
                    .as_deref()
                    .unwrap_or(&symbol.escaped_name);
                let source_file_idx = (symbol.decl_file_idx != u32::MAX)
                    .then_some(symbol.decl_file_idx as usize)
                    .or_else(|| self.ctx.resolve_symbol_file_index(sym_id))
                    .unwrap_or(self.ctx.current_file_idx);
                self.resolve_cross_file_export_from_file(
                    module_name,
                    import_name,
                    Some(source_file_idx),
                )
            });
            if let Some(target_sym_id) = imported_target {
                sym_id = target_sym_id;
            }
        }

        let args = type_ref.type_arguments.as_ref();
        if args.is_none_or(|args| args.nodes.is_empty()) {
            let symbol = self.get_cross_file_symbol(sym_id)?;
            if !symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS) {
                return None;
            }
            let body_node = symbol.declarations.iter().find_map(|&decl_idx| {
                self.ctx
                    .arena
                    .get(decl_idx)
                    .and_then(|node| self.ctx.arena.get_type_alias(node))
                    .map(|alias| alias.type_node)
            })?;
            if body_node == type_arg_idx {
                return None;
            }
            return self.instantiate_type_ref_argument_from_syntax_inner(
                type_arg,
                body_node,
                depth + 1,
            );
        }
        let args = args?;

        let (body_type, mut params) = if self
            .get_cross_file_symbol(sym_id)
            .is_some_and(|symbol| symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS))
            && self
                .ctx
                .resolve_symbol_file_index(sym_id)
                .is_some_and(|file_idx| file_idx != self.ctx.current_file_idx)
        {
            self.delegate_cross_arena_symbol_resolution(sym_id)
                .unwrap_or_else(|| self.type_reference_symbol_type_with_params(sym_id))
        } else {
            self.type_reference_symbol_type_with_params(sym_id)
        };
        if params.is_empty() {
            params = self.get_display_type_params_for_symbol(sym_id);
        }
        if params.is_empty() || matches!(body_type, TypeId::ANY | TypeId::ERROR) {
            return None;
        }

        let mut substitution = crate::query_boundaries::common::TypeSubstitution::new();
        for (param, &arg_idx) in params.iter().zip(args.nodes.iter()) {
            let arg = self.get_type_from_type_node(arg_idx);
            if matches!(arg, TypeId::ERROR | TypeId::UNKNOWN) {
                continue;
            }
            substitution.insert(param.name, arg);
        }
        if substitution.is_empty() {
            return None;
        }

        let instantiated = crate::query_boundaries::common::instantiate_type(
            self.ctx.types,
            body_type,
            &substitution,
        );
        (instantiated != type_arg).then_some(instantiated)
    }
}
