//! Helpers for TS2344 constraint validation against conditional `infer`
//! variables and concrete callable type arguments.
//!
//! Extracted from `constraint_validation.rs` to keep that file under the
//! checker per-file size guard. Behavior is unchanged; only the physical
//! location of these helpers moved.

use crate::query_boundaries::checkers::generic as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn instantiate_constraint_with_type_args(
        &mut self,
        constraint: TypeId,
        type_params: &[tsz_solver::TypeParamInfo],
        type_args: &[TypeId],
    ) -> TypeId {
        let mut subst = crate::query_boundaries::common::TypeSubstitution::new();
        for (param, &arg) in type_params.iter().zip(type_args.iter()) {
            subst.insert(param.name, arg);
        }
        if subst.is_empty() {
            constraint
        } else {
            crate::query_boundaries::common::instantiate_type(self.ctx.types, constraint, &subst)
        }
    }

    pub(super) fn concrete_function_type_arg_violates_callable_constraint(
        &self,
        type_arg: TypeId,
        constraint: TypeId,
    ) -> bool {
        let Some(source_shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_arg)
        else {
            return false;
        };
        let Some(target_shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, constraint)
        else {
            return false;
        };

        let source_required = source_shape
            .params
            .iter()
            .filter(|param| !param.optional && !param.rest)
            .count();
        let target_param_count = target_shape
            .params
            .iter()
            .filter(|param| !param.rest)
            .count();
        let target_has_rest = target_shape.params.iter().any(|param| param.rest);

        !target_has_rest && source_required > target_param_count
    }

    pub(super) fn hidden_conditional_infer_constraint_type(
        &mut self,
        arg_idx: tsz_parser::parser::NodeIndex,
    ) -> Option<TypeId> {
        use tsz_parser::parser::syntax_kind_ext;

        let name = self.type_arg_identifier_name(arg_idx)?;
        let arg_node = self.ctx.arena.get(arg_idx)?;
        let mut current = arg_idx;
        for _ in 0..30 {
            let parent = self
                .ctx
                .arena
                .get_extended(current)
                .map_or(NodeIndex::NONE, |ext| ext.parent);
            if parent.is_none() {
                return None;
            }
            if let Some(parent_node) = self.ctx.arena.get(parent) {
                if let Some(cond) = self.ctx.arena.get_conditional_type(parent_node)
                    && let Some(true_node) = self.ctx.arena.get(cond.true_type)
                    && arg_node.pos >= true_node.pos
                    && arg_node.end <= true_node.end
                {
                    let mut constraints = Vec::new();
                    self.collect_infer_constraints_from_extends_type(
                        cond.extends_type,
                        &name,
                        &mut constraints,
                    );
                    constraints.retain(|&constraint| {
                        constraint != TypeId::UNKNOWN
                            && constraint != TypeId::ANY
                            && !query::contains_type_parameters(self.ctx.types, constraint)
                    });
                    let first = constraints.first().copied()?;
                    return constraints
                        .iter()
                        .all(|&constraint| constraint == first)
                        .then_some(first);
                }
                if parent_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || parent_node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                    || parent_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                {
                    return None;
                }
            }
            current = parent;
        }
        None
    }

    pub(super) fn collect_infer_constraints_from_extends_type(
        &mut self,
        node_idx: tsz_parser::parser::NodeIndex,
        name: &str,
        constraints: &mut Vec<TypeId>,
    ) {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::INFER_TYPE
            && let Some(infer_data) = self.ctx.arena.get_infer_type(node)
            && self.infer_type_param_has_name_for_constraint_probe(infer_data, name)
            && let Some(tp_node) = self.ctx.arena.get(infer_data.type_parameter)
            && let Some(tp_data) = self.ctx.arena.get_type_parameter(tp_node)
            && tp_data.constraint != NodeIndex::NONE
        {
            constraints.push(self.get_type_from_type_node(tp_data.constraint));
            return;
        }

        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.ctx.arena.get_type_ref(node).cloned()
        {
            if let Some(type_args) = &type_ref.type_arguments
                && let Some(sym_id) = self.resolve_type_symbol_for_lowering(type_ref.type_name)
            {
                let sym_id = tsz_binder::SymbolId(sym_id);
                let lib_binders = self.get_lib_binders();
                let base_name = self
                    .ctx
                    .binder
                    .get_symbol_with_libs(sym_id, &lib_binders)
                    .map_or_else(
                        || "<unknown>".to_string(),
                        |symbol| symbol.escaped_name.clone(),
                    );
                let type_params = self.get_reference_type_params_for_symbol(sym_id, &base_name);
                for (i, &arg_idx) in type_args.nodes.iter().enumerate() {
                    if self.type_node_contains_infer_named(arg_idx, name)
                        && let Some(constraint) =
                            type_params.get(i).and_then(|param| param.constraint)
                    {
                        constraints.push(self.resolve_lazy_type(constraint));
                    }
                }
            }
            if let Some(type_args) = &type_ref.type_arguments {
                for &arg_idx in &type_args.nodes {
                    self.collect_infer_constraints_from_extends_type(arg_idx, name, constraints);
                }
            }
            return;
        }

        if let Some(tuple) = self.ctx.arena.get_tuple_type(node).cloned() {
            for &elem_idx in &tuple.elements.nodes {
                self.collect_infer_constraints_from_extends_type(elem_idx, name, constraints);
            }
        }
        if let Some(named_member) = self.ctx.arena.get_named_tuple_member(node) {
            self.collect_infer_constraints_from_extends_type(
                named_member.type_node,
                name,
                constraints,
            );
        }
        if (node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            || node.kind == syntax_kind_ext::OPTIONAL_TYPE
            || node.kind == syntax_kind_ext::REST_TYPE)
            && let Some(wrapped) = self.ctx.arena.get_wrapped_type(node)
        {
            self.collect_infer_constraints_from_extends_type(wrapped.type_node, name, constraints);
        }
        if (node.kind == syntax_kind_ext::UNION_TYPE
            || node.kind == syntax_kind_ext::INTERSECTION_TYPE)
            && let Some(composite) = self.ctx.arena.get_composite_type(node).cloned()
        {
            for &member_idx in &composite.types.nodes {
                self.collect_infer_constraints_from_extends_type(member_idx, name, constraints);
            }
        }
        // Recurse into function/constructor types: parameters and return type.
        // Collect NodeIndexes first (before any &mut self calls) to avoid borrow
        // conflicts between the arena reference and the recursive mutable calls.
        if node.kind == syntax_kind_ext::FUNCTION_TYPE
            || node.kind == syntax_kind_ext::CONSTRUCTOR_TYPE
        {
            let (param_annotations, return_annotation) = if let Some(func_type) =
                self.ctx.arena.get_function_type(node)
            {
                let param_annots: Vec<NodeIndex> = func_type
                    .parameters
                    .nodes
                    .iter()
                    .filter_map(|&param_idx| {
                        let param_node = self.ctx.arena.get(param_idx)?;
                        let param = self.ctx.arena.get_parameter(param_node)?;
                        (param.type_annotation != NodeIndex::NONE).then_some(param.type_annotation)
                    })
                    .collect();
                let ret = func_type.type_annotation;
                (param_annots, ret)
            } else {
                (Vec::new(), NodeIndex::NONE)
            };
            for annotation in param_annotations {
                self.collect_infer_constraints_from_extends_type(annotation, name, constraints);
            }
            if return_annotation != NodeIndex::NONE {
                self.collect_infer_constraints_from_extends_type(
                    return_annotation,
                    name,
                    constraints,
                );
            }
        }
    }

    pub(super) fn type_node_contains_infer_named(
        &self,
        node_idx: tsz_parser::parser::NodeIndex,
        name: &str,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::INFER_TYPE {
            return self
                .ctx
                .arena
                .get_infer_type(node)
                .is_some_and(|infer_data| {
                    self.infer_type_param_has_name_for_constraint_probe(infer_data, name)
                });
        }
        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
            && let Some(type_args) = &type_ref.type_arguments
        {
            return type_args
                .nodes
                .iter()
                .any(|&arg_idx| self.type_node_contains_infer_named(arg_idx, name));
        }
        if let Some(tuple) = self.ctx.arena.get_tuple_type(node) {
            return tuple
                .elements
                .nodes
                .iter()
                .any(|&elem_idx| self.type_node_contains_infer_named(elem_idx, name));
        }
        if let Some(named_member) = self.ctx.arena.get_named_tuple_member(node) {
            return self.type_node_contains_infer_named(named_member.type_node, name);
        }
        if (node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            || node.kind == syntax_kind_ext::OPTIONAL_TYPE
            || node.kind == syntax_kind_ext::REST_TYPE)
            && let Some(wrapped) = self.ctx.arena.get_wrapped_type(node)
        {
            return self.type_node_contains_infer_named(wrapped.type_node, name);
        }
        false
    }

    pub(super) fn infer_type_param_has_name_for_constraint_probe(
        &self,
        infer_data: &tsz_parser::parser::node::InferTypeData,
        name: &str,
    ) -> bool {
        self.ctx
            .arena
            .get(infer_data.type_parameter)
            .and_then(|tp_node| self.ctx.arena.get_type_parameter(tp_node))
            .and_then(|tp_data| self.ctx.arena.get(tp_data.name))
            .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
            .is_some_and(|ident| ident.escaped_text == name)
    }
}
