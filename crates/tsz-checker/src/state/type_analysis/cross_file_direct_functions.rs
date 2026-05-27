//! Direct source-file function declaration fast paths.

use crate::state::CheckerState;
use tsz_binder::{BinderState, SymbolId, symbol_flags};
use tsz_lowering::TypeLowering;
use tsz_parser::NodeIndex;
use tsz_parser::parser::base::NodeList;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{FunctionShape, ParamInfo, TypeId, TypePredicate, TypePredicateTarget};

use super::cross_file_direct::is_direct_lowering_source_file_arena;

impl<'a> CheckerState<'a> {
    pub(super) fn direct_source_file_function_declaration_type(
        &self,
        sym_id: SymbolId,
        delegate_binder: &BinderState,
        symbol_arena: &NodeArena,
        allow_source_file_arena: bool,
    ) -> Option<TypeId> {
        if !allow_source_file_arena || !is_direct_lowering_source_file_arena(symbol_arena) {
            return None;
        }
        let symbol = delegate_binder
            .get_symbol(sym_id)
            .or_else(|| self.get_cross_file_symbol(sym_id))?;
        if symbol.flags & symbol_flags::FUNCTION == 0
            || symbol.flags & (symbol_flags::MODULE | symbol_flags::ALIAS) != 0
            || symbol.declarations.len() != 1
        {
            return None;
        }

        let decl_idx = symbol.declarations[0];
        let decl_node = symbol_arena.get(decl_idx)?;
        let function = symbol_arena.get_function(decl_node)?;
        if decl_node.kind != syntax_kind_ext::FUNCTION_DECLARATION
            || function.type_annotation == NodeIndex::NONE
            || function
                .type_parameters
                .as_ref()
                .is_some_and(|params| !params.nodes.is_empty())
            || function.parameters.nodes.iter().copied().any(|param_idx| {
                symbol_arena
                    .get(param_idx)
                    .and_then(|param_node| symbol_arena.get_parameter(param_node))
                    .is_none_or(|param| param.type_annotation == NodeIndex::NONE)
            })
        {
            return None;
        }
        let mut seen_type_names = Vec::new();
        if !Self::source_file_type_node_is_option_bag_lowerable(
            symbol_arena,
            delegate_binder,
            function.type_annotation,
            &mut seen_type_names,
        ) {
            return None;
        }
        for param_idx in function.parameters.nodes.iter().copied() {
            let param = symbol_arena
                .get(param_idx)
                .and_then(|param_node| symbol_arena.get_parameter(param_node))?;
            if !Self::source_file_type_node_is_option_bag_lowerable(
                symbol_arena,
                delegate_binder,
                param.type_annotation,
                &mut seen_type_names,
            ) {
                return None;
            }
        }

        let name_resolver = |type_name: &str| -> Option<tsz_solver::def::DefId> {
            self.source_file_local_name_def_id_for_lowering(
                delegate_binder,
                symbol_arena,
                type_name,
            )
        };
        let no_type_symbol = |_node_idx: NodeIndex| -> Option<u32> { None };
        let no_def_id = |_node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> { None };
        let no_value_symbol = |_node_idx: NodeIndex| -> Option<u32> { None };
        let lowering = TypeLowering::with_hybrid_resolver(
            symbol_arena,
            self.ctx.types,
            &no_type_symbol,
            &no_def_id,
            &no_value_symbol,
        )
        .with_builtin_iterator_return_type(self.builtin_iterator_return_intrinsic_type())
        .with_name_def_id_resolver(&name_resolver)
        .prefer_name_def_id_resolution();
        let ty = lowering.lower_signature_from_declaration(decl_idx, None);
        (ty != TypeId::UNKNOWN && ty != TypeId::ERROR).then_some(ty)
    }

    pub(super) fn direct_source_file_function_declaration_result(
        &self,
        sym_id: SymbolId,
        direct_target: Option<(&NodeArena, &BinderState, Option<usize>)>,
    ) -> Option<TypeId> {
        let (symbol_arena, delegate_binder, _) = direct_target?;
        self.direct_source_file_function_declaration_type(
            sym_id,
            delegate_binder,
            symbol_arena,
            true,
        )
    }

    pub(super) fn direct_source_file_variable_or_function_annotation_result(
        &self,
        sym_id: SymbolId,
        direct_target: Option<(&NodeArena, &BinderState, Option<usize>)>,
        allow_source_file_arena: bool,
    ) -> Option<TypeId> {
        self.direct_source_file_variable_annotation_result(
            sym_id,
            direct_target,
            allow_source_file_arena,
        )
        .or_else(|| {
            let (symbol_arena, delegate_binder, _) = direct_target?;
            self.direct_source_file_variable_function_initializer_type(
                sym_id,
                delegate_binder,
                symbol_arena,
                allow_source_file_arena,
            )
        })
    }

    fn direct_source_file_variable_function_initializer_type(
        &self,
        sym_id: SymbolId,
        delegate_binder: &BinderState,
        symbol_arena: &NodeArena,
        allow_source_file_arena: bool,
    ) -> Option<TypeId> {
        if !allow_source_file_arena || !is_direct_lowering_source_file_arena(symbol_arena) {
            return None;
        }
        let symbol = delegate_binder
            .get_symbol(sym_id)
            .or_else(|| self.get_cross_file_symbol(sym_id))?;
        if symbol.flags & symbol_flags::VARIABLE == 0
            || symbol.flags & (symbol_flags::MODULE | symbol_flags::ALIAS) != 0
            || symbol.declarations.len() != 1
        {
            return None;
        }

        let decl_idx = Self::source_file_variable_declaration_for_symbol(
            symbol_arena,
            &symbol.escaped_name,
            symbol,
        )?;
        let variable = symbol_arena
            .get(decl_idx)
            .and_then(|node| symbol_arena.get_variable_declaration(node))?;
        if variable.type_annotation.is_some() {
            return None;
        }
        let initializer_node = symbol_arena.get(variable.initializer)?;
        if initializer_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && initializer_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return None;
        }
        let function = symbol_arena.get_function(initializer_node)?;
        if function.type_annotation == NodeIndex::NONE
            || function
                .type_parameters
                .as_ref()
                .is_some_and(|params| !params.nodes.is_empty())
            || function.parameters.nodes.iter().copied().any(|param_idx| {
                symbol_arena
                    .get(param_idx)
                    .and_then(|param_node| symbol_arena.get_parameter(param_node))
                    .is_none_or(|param| param.type_annotation == NodeIndex::NONE)
            })
        {
            return None;
        }

        let mut seen_type_names = Vec::new();
        for param_idx in function.parameters.nodes.iter().copied() {
            let param = symbol_arena
                .get(param_idx)
                .and_then(|param_node| symbol_arena.get_parameter(param_node))?;
            if !Self::source_file_type_node_is_option_bag_lowerable(
                symbol_arena,
                delegate_binder,
                param.type_annotation,
                &mut seen_type_names,
            ) {
                return None;
            }
        }
        if !Self::source_file_return_annotation_is_lowerable(
            symbol_arena,
            delegate_binder,
            function.type_annotation,
            &mut seen_type_names,
        ) {
            return None;
        }

        let name_resolver = |type_name: &str| -> Option<tsz_solver::def::DefId> {
            self.source_file_local_name_def_id_for_lowering(
                delegate_binder,
                symbol_arena,
                type_name,
            )
        };
        let no_type_symbol = |_node_idx: NodeIndex| -> Option<u32> { None };
        let no_def_id = |_node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> { None };
        let no_value_symbol = |_node_idx: NodeIndex| -> Option<u32> { None };
        let lowering = TypeLowering::with_hybrid_resolver(
            symbol_arena,
            self.ctx.types,
            &no_type_symbol,
            &no_def_id,
            &no_value_symbol,
        )
        .with_builtin_iterator_return_type(self.builtin_iterator_return_intrinsic_type())
        .with_name_def_id_resolver(&name_resolver)
        .prefer_name_def_id_resolution();

        let params =
            self.lower_direct_source_file_params(symbol_arena, &lowering, &function.parameters)?;
        let (return_type, type_predicate) = self.lower_direct_source_file_return_annotation(
            symbol_arena,
            &lowering,
            function.type_annotation,
            &params,
        )?;
        let ty = self.ctx.types.factory().function(FunctionShape {
            type_params: Vec::new(),
            params,
            this_type: None,
            return_type,
            type_predicate,
            is_constructor: false,
            is_method: false,
        });
        Some(ty)
    }

    fn source_file_variable_declaration_for_symbol(
        arena: &NodeArena,
        escaped_name: &str,
        symbol: &tsz_binder::Symbol,
    ) -> Option<NodeIndex> {
        symbol
            .declarations
            .iter()
            .copied()
            .find_map(|decl_idx| {
                Self::source_file_variable_declaration_named(arena, decl_idx, escaped_name)
            })
            .or_else(|| {
                Self::source_file_variable_declaration_named(
                    arena,
                    symbol.value_declaration,
                    escaped_name,
                )
            })
    }

    fn source_file_variable_declaration_named(
        arena: &NodeArena,
        node_idx: NodeIndex,
        escaped_name: &str,
    ) -> Option<NodeIndex> {
        if node_idx.is_none() {
            return None;
        }
        let node = arena.get(node_idx)?;
        if let Some(variable) = arena.get_variable_declaration(node) {
            let name = arena
                .get(variable.name)
                .and_then(|name_node| arena.get_identifier(name_node))?;
            return (name.escaped_text == escaped_name).then_some(node_idx);
        }
        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            && let Some(list) = arena.get_variable(node)
        {
            return list
                .declarations
                .nodes
                .iter()
                .copied()
                .find_map(|decl_idx| {
                    Self::source_file_variable_declaration_named(arena, decl_idx, escaped_name)
                });
        }
        if node.kind == syntax_kind_ext::VARIABLE_STATEMENT
            && let Some(statement) = arena.get_variable(node)
        {
            return statement
                .declarations
                .nodes
                .iter()
                .copied()
                .find_map(|list_idx| {
                    Self::source_file_variable_declaration_named(arena, list_idx, escaped_name)
                });
        }
        None
    }

    fn source_file_return_annotation_is_lowerable<'b>(
        arena: &'b NodeArena,
        binder: &BinderState,
        annotation: NodeIndex,
        seen_type_names: &mut Vec<&'b str>,
    ) -> bool {
        let Some(node) = arena.get(annotation) else {
            return false;
        };
        if node.kind == syntax_kind_ext::TYPE_PREDICATE {
            let Some(predicate) = arena.get_type_predicate(node) else {
                return false;
            };
            return predicate.type_node == NodeIndex::NONE
                || Self::source_file_type_node_is_option_bag_lowerable(
                    arena,
                    binder,
                    predicate.type_node,
                    seen_type_names,
                );
        }
        Self::source_file_type_node_is_option_bag_lowerable(
            arena,
            binder,
            annotation,
            seen_type_names,
        )
    }

    fn lower_direct_source_file_params(
        &self,
        arena: &NodeArena,
        lowering: &TypeLowering<'_>,
        parameters: &NodeList,
    ) -> Option<Vec<ParamInfo>> {
        let mut params = Vec::new();
        for param_idx in parameters.nodes.iter().copied() {
            let param = arena
                .get(param_idx)
                .and_then(|param_node| arena.get_parameter(param_node))?;
            let name = arena
                .get(param.name)
                .and_then(|name_node| arena.get_identifier(name_node))
                .map(|ident| self.ctx.types.intern_string(&ident.escaped_text));
            let mut type_id = lowering.lower_type(param.type_annotation);
            if param.question_token
                && type_id != TypeId::ANY
                && type_id != TypeId::ERROR
                && !tsz_solver::narrowing::type_contains_undefined(self.ctx.types, type_id)
            {
                type_id = self.ctx.types.factory().union2(type_id, TypeId::UNDEFINED);
            }
            if type_id == TypeId::ERROR {
                return None;
            }
            params.push(ParamInfo {
                name,
                type_id,
                optional: param.question_token || param.initializer != NodeIndex::NONE,
                rest: param.dot_dot_dot_token,
            });
        }
        Some(params)
    }

    fn lower_direct_source_file_return_annotation(
        &self,
        arena: &NodeArena,
        lowering: &TypeLowering<'_>,
        annotation: NodeIndex,
        params: &[ParamInfo],
    ) -> Option<(TypeId, Option<TypePredicate>)> {
        let node = arena.get(annotation)?;
        if node.kind != syntax_kind_ext::TYPE_PREDICATE {
            let return_type = lowering.lower_type(annotation);
            return (return_type != TypeId::ERROR).then_some((return_type, None));
        }

        let predicate = arena.get_type_predicate(node)?;
        let return_type = if predicate.asserts_modifier {
            TypeId::VOID
        } else {
            TypeId::BOOLEAN
        };
        let target_node = arena.get(predicate.parameter_name)?;
        let target = if target_node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16
            || target_node.kind == syntax_kind_ext::THIS_TYPE
        {
            TypePredicateTarget::This
        } else {
            let name = arena.get_identifier(target_node)?;
            TypePredicateTarget::Identifier(self.ctx.types.intern_string(&name.escaped_text))
        };
        let type_id = if predicate.type_node == NodeIndex::NONE {
            None
        } else {
            let ty = lowering.lower_type(predicate.type_node);
            if ty == TypeId::ERROR {
                return None;
            }
            Some(ty)
        };
        let parameter_index = if let TypePredicateTarget::Identifier(name) = target {
            params.iter().position(|param| param.name == Some(name))
        } else {
            None
        };
        Some((
            return_type,
            Some(TypePredicate {
                asserts: predicate.asserts_modifier,
                target,
                type_id,
                parameter_index,
            }),
        ))
    }
}

#[cfg(test)]
#[path = "cross_file_direct_functions_tests.rs"]
mod tests;
