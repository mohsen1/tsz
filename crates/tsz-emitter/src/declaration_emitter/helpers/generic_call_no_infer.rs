//! Declaration emit helpers for generic calls constrained by builtin `NoInfer`.

use super::super::DeclarationEmitter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::parser::syntax_kind_ext;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn call_expression_uses_no_infer_return_block(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        let Some(call) = self.arena.get_call_expr(expr_node) else {
            return false;
        };
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(raw_sym_id) = self.value_reference_symbol(call.expression) else {
            return false;
        };
        let sym_id = self
            .resolve_portability_import_alias(raw_sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(raw_sym_id, binder));

        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let decl_node = source_arena.get(decl_idx)?;
            let callable = Self::callable_decl_parts_from_node(source_arena, decl_node)?;
            if !callable.type_annotation.is_some()
                || !self.function_signature_accepts_call_arguments(
                    source_arena,
                    callable.parameters,
                    call,
                )
            {
                return None;
            }
            let type_param_names = callable
                .type_parameters?
                .nodes
                .iter()
                .copied()
                .filter_map(|param_idx| {
                    let param_node = source_arena.get(param_idx)?;
                    let param = source_arena.get_type_parameter(param_node)?;
                    self.identifier_text_from_arena(source_arena, param.name)
                })
                .collect::<Vec<_>>();
            for &param_idx in &callable.parameters.nodes {
                let Some(param_node) = source_arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = source_arena.get_parameter(param_node) else {
                    continue;
                };
                if !self
                    .no_infer_blocked_return_type_params_from_function_type(
                        source_arena,
                        param.type_annotation,
                        &type_param_names,
                    )
                    .is_empty()
                {
                    return Some(true);
                }
            }
            None
        })
        .unwrap_or(false)
    }

    pub(in crate::declaration_emitter) fn no_infer_blocked_return_type_params_from_function_type(
        &self,
        source_arena: &NodeArena,
        type_idx: NodeIndex,
        type_param_names: &[String],
    ) -> Vec<String> {
        let type_idx = source_arena.skip_parenthesized(type_idx);
        let Some(type_node) = source_arena.get(type_idx) else {
            return Vec::new();
        };
        if type_node.kind != syntax_kind_ext::FUNCTION_TYPE {
            return Vec::new();
        }
        let Some(function_type) = source_arena.get_function_type(type_node) else {
            return Vec::new();
        };
        let Some(return_param) =
            self.simple_type_node_name_from_arena(source_arena, function_type.type_annotation)
        else {
            return Vec::new();
        };
        if !type_param_names
            .iter()
            .any(|name| name.as_str() == return_param)
        {
            return Vec::new();
        }
        let blocked = function_type
            .parameters
            .nodes
            .iter()
            .copied()
            .any(|param_idx| {
                let Some(param_node) = source_arena.get(param_idx) else {
                    return false;
                };
                let Some(param) = source_arena.get_parameter(param_node) else {
                    return false;
                };
                self.type_node_contains_builtin_no_infer_of_type_param(
                    source_arena,
                    param.type_annotation,
                    &return_param,
                    0,
                )
            });
        if blocked {
            vec![return_param]
        } else {
            Vec::new()
        }
    }

    fn type_node_contains_builtin_no_infer_of_type_param(
        &self,
        source_arena: &NodeArena,
        type_idx: NodeIndex,
        type_param_name: &str,
        depth: u8,
    ) -> bool {
        if depth > 64 || type_idx.is_none() {
            return false;
        }
        let type_idx = source_arena.skip_parenthesized(type_idx);
        if self.type_node_is_builtin_no_infer_of_type_param(source_arena, type_idx, type_param_name)
        {
            return true;
        }
        source_arena
            .get_children(type_idx)
            .into_iter()
            .any(|child| {
                self.type_node_contains_builtin_no_infer_of_type_param(
                    source_arena,
                    child,
                    type_param_name,
                    depth + 1,
                )
            })
    }

    fn type_node_is_builtin_no_infer_of_type_param(
        &self,
        source_arena: &NodeArena,
        type_idx: NodeIndex,
        type_param_name: &str,
    ) -> bool {
        let type_idx = source_arena.skip_parenthesized(type_idx);
        let Some(type_node) = source_arena.get(type_idx) else {
            return false;
        };
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = source_arena.get_type_ref(type_node) else {
            return false;
        };
        let Some(type_args) = type_ref.type_arguments.as_ref() else {
            return false;
        };
        let [arg_idx] = type_args.nodes.as_slice() else {
            return false;
        };
        if self
            .simple_type_node_name_from_arena(source_arena, *arg_idx)
            .as_deref()
            != Some(type_param_name)
        {
            return false;
        }
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(global_no_infer) = binder.get_global_type("NoInfer") else {
            return false;
        };
        let Some(sym_id) = self.declaration_type_symbol_from_type_node(source_arena, type_idx)
        else {
            return false;
        };
        sym_id == global_no_infer && binder.lib_symbol_ids.contains(&sym_id)
    }
}
