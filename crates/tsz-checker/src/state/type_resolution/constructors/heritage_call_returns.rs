use crate::query_boundaries::common::{TypeSubstitution, call_signatures_for_type};
use crate::state::CheckerState;
use tsz_parser::parser::node::CallExprData;
use tsz_parser::parser::{NodeIndex, NodeList, syntax_kind_ext};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn heritage_call_return_type_for_base_constructor(
        &mut self,
        call_expr: &CallExprData,
        type_arguments: Option<&NodeList>,
        cached_expr_type: TypeId,
    ) -> TypeId {
        let callee_expr_type_args = self
            .ctx
            .arena
            .get(call_expr.expression)
            .and_then(|callee_node| self.ctx.arena.get_expr_type_args(callee_node));
        let callee_idx = callee_expr_type_args
            .map(|expr_type_args| expr_type_args.expression)
            .unwrap_or(call_expr.expression);
        let call_type_args = call_expr
            .type_arguments
            .as_ref()
            .or_else(|| {
                callee_expr_type_args
                    .and_then(|expr_type_args| expr_type_args.type_arguments.as_ref())
            })
            .or(type_arguments);
        let Some(type_args) = call_type_args else {
            return cached_expr_type;
        };

        let callee_type = self.get_type_of_node(callee_idx);
        let invoked_type = self.apply_type_arguments_to_callable_type(callee_type, Some(type_args));
        let signature_return = call_signatures_for_type(self.ctx.types, invoked_type)
            .or_else(|| {
                let value_type = self.value_type_for_heritage_call_callee(callee_idx)?;
                let invoked_value_type =
                    self.apply_type_arguments_to_callable_type(value_type, Some(type_args));
                call_signatures_for_type(self.ctx.types, invoked_value_type)
            })
            .and_then(|call_signatures| call_signatures.first().map(|sig| sig.return_type));
        let same_named_return =
            self.same_named_heritage_function_return_type(callee_idx, type_args);
        match (signature_return, same_named_return) {
            (_, Some(alias_return)) => alias_return,
            (Some(signature_return), _) => signature_return,
            (None, None) => cached_expr_type,
        }
    }

    fn value_type_for_heritage_call_callee(&mut self, callee_idx: NodeIndex) -> Option<TypeId> {
        let callee_node = self.ctx.arena.get(callee_idx)?;
        let ident = self.ctx.arena.get_identifier(callee_node)?;
        let value_type = self.type_of_value_symbol_by_name(&ident.escaped_text);
        if !matches!(value_type, TypeId::ERROR | TypeId::UNKNOWN)
            && call_signatures_for_type(self.ctx.types, value_type).is_some()
        {
            return Some(value_type);
        }

        let function_declarations = self
            .ctx
            .arena
            .source_files
            .first()
            .map(|source_file| {
                source_file
                    .statements
                    .nodes
                    .iter()
                    .copied()
                    .filter_map(|stmt_idx| {
                        let stmt_node = self.ctx.arena.get(stmt_idx)?;
                        let func = self.ctx.arena.get_function(stmt_node)?;
                        let name_node = self.ctx.arena.get(func.name)?;
                        let name = self.ctx.arena.get_identifier(name_node)?;
                        (name.escaped_text == ident.escaped_text).then(|| {
                            self.ctx
                                .binder
                                .get_node_symbol(stmt_idx)
                                .or_else(|| self.ctx.binder.get_node_symbol(func.name))
                                .map(|sym_id| (sym_id, stmt_idx))
                        })?
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (sym_id, decl_idx) in function_declarations {
            let value_type = self.type_of_value_declaration_for_symbol(sym_id, decl_idx);
            if !matches!(value_type, TypeId::ERROR | TypeId::UNKNOWN)
                && call_signatures_for_type(self.ctx.types, value_type).is_some()
            {
                return Some(value_type);
            }
        }

        let sym_id = self.resolve_identifier_symbol(callee_idx)?;
        let (value_decl, declarations) = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .or_else(|| self.get_cross_file_symbol(sym_id))
            .map(|symbol| (symbol.value_declaration, symbol.declarations.clone()))?;
        for decl_idx in std::iter::once(value_decl).chain(declarations) {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if decl_node.kind != syntax_kind_ext::FUNCTION_DECLARATION && decl_idx != value_decl {
                continue;
            }
            let value_type = self.type_of_value_declaration_for_symbol(sym_id, decl_idx);
            if !matches!(value_type, TypeId::ERROR | TypeId::UNKNOWN)
                && call_signatures_for_type(self.ctx.types, value_type).is_some()
            {
                return Some(value_type);
            }
        }
        None
    }

    fn same_named_heritage_function_return_type(
        &mut self,
        callee_idx: NodeIndex,
        type_arguments: &NodeList,
    ) -> Option<TypeId> {
        let ident = self.ctx.arena.get_identifier_at(callee_idx)?;
        let callee_name = ident.escaped_text.as_str();
        let candidates =
            self.ctx
                .arena
                .source_files
                .first()
                .map(|source_file| {
                    source_file
                        .statements
                        .nodes
                        .iter()
                        .copied()
                        .filter_map(|stmt_idx| {
                            let stmt_node = self.ctx.arena.get(stmt_idx)?;
                            let func = self.ctx.arena.get_function(stmt_node)?;
                            let function_name =
                                self.ctx.arena.get(func.name).and_then(|name_node| {
                                    self.ctx.arena.get_identifier(name_node)
                                })?;
                            if function_name.escaped_text != callee_name {
                                return None;
                            }
                            let return_node = self.ctx.arena.get(func.type_annotation)?;
                            let returns_callee_alias = self
                                .ctx
                                .arena
                                .get_type_ref(return_node)
                                .and_then(|type_ref| self.entity_name_text(type_ref.type_name))
                                .is_some_and(|return_name| return_name == callee_name);
                            returns_callee_alias
                                .then(|| (func.type_parameters.clone(), func.type_annotation))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
        for (type_parameters, return_annotation) in candidates {
            let (params, updates) = self.push_type_parameters(&type_parameters);
            let return_type = self.get_type_from_type_node(return_annotation);
            self.pop_type_parameters(updates);
            if matches!(return_type, TypeId::ERROR | TypeId::UNKNOWN) {
                continue;
            }
            if params.is_empty() || type_arguments.nodes.is_empty() {
                return Some(return_type);
            }

            let mut type_args = Vec::with_capacity(type_arguments.nodes.len());
            for &arg_idx in &type_arguments.nodes {
                type_args.push(self.get_type_from_type_node(arg_idx));
            }
            if type_args.len() > params.len() {
                type_args.truncate(params.len());
            }
            let substitution =
                TypeSubstitution::from_signature_args(self.ctx.types, &params, &type_args);
            return Some(crate::query_boundaries::common::instantiate_type(
                self.ctx.types,
                return_type,
                &substitution,
            ));
        }

        None
    }
}
