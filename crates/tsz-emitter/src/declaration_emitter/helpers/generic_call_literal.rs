//! Literal inference helpers for generic call expression declaration emit.

use super::super::DeclarationEmitter;
use tsz_parser::parser::node::{FunctionData, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn call_expression_reused_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        self.imported_static_method_declared_return_type_text(expr_idx)
            .or_else(|| self.call_expression_returned_local_class_constructor_text(expr_idx, false))
            .or_else(|| {
                self.super_method_call_return_type_text(expr_idx)
                    .or_else(|| self.call_expression_source_return_type_text(expr_idx))
                    .or_else(|| self.generic_call_literal_type_text(expr_idx))
                    .or_else(|| self.call_expression_declared_return_type_text(expr_idx))
            })
            .map(Self::normalize_constructor_arrow_return_object_text)
    }

    fn normalize_constructor_arrow_return_object_text(type_text: String) -> String {
        let Some(arrow_pos) = type_text.find("=> {") else {
            return type_text;
        };
        let object_start = arrow_pos + "=> ".len();
        let Some(close_rel) = type_text[object_start + 1..].find('}') else {
            return type_text;
        };
        let object_end = object_start + 1 + close_rel;
        let member_text = type_text[object_start + 1..object_end].trim();
        if member_text.is_empty() || member_text.contains('\n') || !member_text.contains(':') {
            return type_text;
        }

        let member_text = member_text.trim_end_matches(';').trim();
        let replacement = format!("{{\n    {member_text};\n}}");
        let mut normalized = String::new();
        normalized.push_str(&type_text[..object_start]);
        normalized.push_str(&replacement);
        normalized.push_str(&type_text[object_end + 1..]);
        normalized
    }

    pub(in crate::declaration_emitter) fn generic_call_literal_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        if !self.call_expression_has_generic_callee(expr_idx) {
            return None;
        }

        if let Some(type_text) = self.generic_call_object_property_literal_type_text(expr_idx) {
            return Some(type_text);
        }

        let type_id = self.get_node_type_or_names(&[expr_idx])?;
        if type_id == tsz_solver::types::TypeId::ANY || type_id == tsz_solver::types::TypeId::ERROR
        {
            return None;
        }

        let interner = self.type_interner?;
        tsz_solver::type_queries::is_literal_or_literal_union_type(interner, type_id)
            .then(|| self.print_type_id_for_inferred_declaration(type_id))
    }

    fn generic_call_object_property_literal_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let call = self.arena.get_call_expr(expr_node)?;
        let arguments = call.arguments.as_ref()?;

        if self.function_expression_has_type_parameters(call.expression) {
            let callee_idx = self.skip_parenthesized_expression(call.expression)?;
            let callee_node = self.arena.get(callee_idx)?;
            let func = self.arena.get_function(callee_node)?;
            return self.generic_call_object_property_literal_type_text_for_function(
                self.arena, func, arguments,
            );
        }

        let sym_id = self.value_reference_symbol(call.expression)?;
        let binder = self.binder?;
        let sym_id = self
            .resolve_portability_import_alias(sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));
        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let func = callable_function_from_symbol_decl(source_arena, decl_idx)?;
            self.generic_call_object_property_literal_type_text_for_function(
                source_arena,
                func,
                arguments,
            )
        })
    }

    fn generic_call_object_property_literal_type_text_for_function(
        &self,
        source_arena: &NodeArena,
        func: &FunctionData,
        arguments: &NodeList,
    ) -> Option<String> {
        let return_type_param =
            function_return_type_parameter_name(source_arena, func).filter(|type_param| {
                func.type_parameters.as_ref().is_some_and(|type_params| {
                    type_params.nodes.iter().copied().any(|param_idx| {
                        source_arena
                            .get(param_idx)
                            .and_then(|node| source_arena.get_type_parameter(node))
                            .and_then(|param| identifier_text(source_arena, param.name))
                            .is_some_and(|name| name == *type_param)
                    })
                })
            })?;
        func.parameters
            .nodes
            .iter()
            .copied()
            .zip(arguments.nodes.iter().copied())
            .find_map(|(param_idx, arg_idx)| {
                let param_node = source_arena.get(param_idx)?;
                let param = source_arena.get_parameter(param_node)?;
                parameter_type_has_property_type_parameter(
                    source_arena,
                    param.type_annotation,
                    "type",
                    &return_type_param,
                )
                .then(|| self.object_literal_property_literal_type_text(arg_idx, "type"))
                .flatten()
            })
    }

    fn call_expression_has_generic_callee(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        let Some(call) = self.arena.get_call_expr(expr_node) else {
            return false;
        };
        if self.function_expression_has_type_parameters(call.expression) {
            return true;
        }

        let Some(sym_id) = self.value_reference_symbol(call.expression) else {
            return false;
        };
        let Some(binder) = self.binder else {
            return false;
        };
        let sym_id = self
            .resolve_portability_import_alias(sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));
        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let func = callable_function_from_symbol_decl(source_arena, decl_idx)?;
            func.type_parameters
                .as_ref()
                .is_some_and(|params| !params.nodes.is_empty())
                .then_some(())
        })
        .is_some()
    }

    fn function_expression_has_type_parameters(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_idx) = self.skip_parenthesized_expression(expr_idx) else {
            return false;
        };
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && expr_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return false;
        }
        self.arena
            .get_function(expr_node)
            .and_then(|func| func.type_parameters.as_ref())
            .is_some_and(|params| !params.nodes.is_empty())
    }
}

fn function_return_type_parameter_name(
    source_arena: &NodeArena,
    func: &FunctionData,
) -> Option<String> {
    type_reference_identifier_name(source_arena, func.type_annotation)
}

fn parameter_type_has_property_type_parameter(
    source_arena: &NodeArena,
    type_idx: NodeIndex,
    property_name: &str,
    type_param_name: &str,
) -> bool {
    let Some(type_node) = source_arena.get(type_idx) else {
        return false;
    };
    match type_node.kind {
        k if k == syntax_kind_ext::TYPE_LITERAL => source_arena
            .get_type_literal(type_node)
            .is_some_and(|literal| {
                literal.members.nodes.iter().copied().any(|member_idx| {
                    let Some(member_node) = source_arena.get(member_idx) else {
                        return false;
                    };
                    if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
                        return false;
                    }
                    let Some(signature) = source_arena.get_signature(member_node) else {
                        return false;
                    };
                    identifier_text(source_arena, signature.name).as_deref() == Some(property_name)
                        && type_reference_identifier_name(source_arena, signature.type_annotation)
                            .as_deref()
                            == Some(type_param_name)
                })
            }),
        k if k == syntax_kind_ext::INTERSECTION_TYPE || k == syntax_kind_ext::UNION_TYPE => {
            source_arena
                .get_composite_type(type_node)
                .is_some_and(|composite| {
                    composite.types.nodes.iter().copied().any(|part_idx| {
                        parameter_type_has_property_type_parameter(
                            source_arena,
                            part_idx,
                            property_name,
                            type_param_name,
                        )
                    })
                })
        }
        k if k == syntax_kind_ext::PARENTHESIZED_TYPE => source_arena
            .get_wrapped_type(type_node)
            .is_some_and(|wrapped| {
                parameter_type_has_property_type_parameter(
                    source_arena,
                    wrapped.type_node,
                    property_name,
                    type_param_name,
                )
            }),
        _ => false,
    }
}

fn type_reference_identifier_name(source_arena: &NodeArena, type_idx: NodeIndex) -> Option<String> {
    let type_node = source_arena.get(type_idx)?;
    if type_node.kind == SyntaxKind::Identifier as u16 {
        return identifier_text(source_arena, type_idx);
    }
    let type_ref = source_arena.get_type_ref(type_node)?;
    identifier_text(source_arena, type_ref.type_name)
}

fn identifier_text(source_arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    source_arena
        .get(idx)
        .and_then(|node| source_arena.get_identifier(node))
        .map(|ident| ident.escaped_text.clone())
}

fn callable_function_from_symbol_decl(
    source_arena: &NodeArena,
    decl_idx: NodeIndex,
) -> Option<&FunctionData> {
    if let Some(func) = source_arena
        .get(decl_idx)
        .and_then(|node| source_arena.get_function(node))
    {
        return Some(func);
    }

    let mut current = decl_idx;
    for _ in 0..8 {
        let node = source_arena.get(current)?;
        if let Some(var_decl) = source_arena.get_variable_declaration(node) {
            let initializer_node = source_arena.get(var_decl.initializer)?;
            if initializer_node.kind == syntax_kind_ext::ARROW_FUNCTION
                || initializer_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            {
                return source_arena.get_function(initializer_node);
            }
        }
        current = source_arena.parent_of(current)?;
    }

    None
}
