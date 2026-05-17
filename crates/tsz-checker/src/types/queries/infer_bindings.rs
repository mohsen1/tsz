//! Infer type-parameter collection helpers for conditional type checking.
//!
//! These helpers walk the `extends_type` AST of a conditional type to collect
//! `infer X` bindings and determine which ones carry an implicit `string`
//! constraint from appearing in a template-literal-type span position.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Collect every `infer` type parameter name reachable from `type_idx`,
    /// including ones nested inside another infer's `extends` constraint.
    pub(crate) fn collect_infer_type_parameters(&self, type_idx: NodeIndex) -> Vec<String> {
        let mut params = Vec::new();
        self.collect_infer_type_parameters_inner(type_idx, &mut params);
        params
    }

    /// Collect all `infer` type parameter declarations with their constraint and position info.
    /// Returns `(name, constraint_node_idx, type_parameter_node_idx)` for each `infer` declaration.
    /// Used by TS2838 validation to check that duplicate infer names have identical constraints.
    pub(crate) fn collect_infer_type_params_with_constraints(
        &self,
        type_idx: NodeIndex,
    ) -> Vec<(String, NodeIndex, NodeIndex)> {
        let mut params = Vec::new();
        self.collect_infer_params_with_constraints_inner(type_idx, &mut params);
        params
    }

    fn collect_infer_params_with_constraints_inner(
        &self,
        type_idx: NodeIndex,
        params: &mut Vec<(String, NodeIndex, NodeIndex)>,
    ) {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::INFER_TYPE => {
                if let Some(infer) = self.ctx.arena.get_infer_type(node)
                    && let Some(param_node) = self.ctx.arena.get(infer.type_parameter)
                    && let Some(param) = self.ctx.arena.get_type_parameter(param_node)
                    && let Some(name_node) = self.ctx.arena.get(param.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                {
                    params.push((
                        ident.escaped_text.clone(),
                        param.constraint,
                        infer.type_parameter,
                    ));
                    self.collect_infer_params_with_constraints_inner(infer.type_parameter, params);
                }
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.ctx.arena.get_type_ref(node)
                    && let Some(ref args) = type_ref.type_arguments
                {
                    for &arg_idx in &args.nodes {
                        self.collect_infer_params_with_constraints_inner(arg_idx, params);
                    }
                }
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                    for &member_idx in &composite.types.nodes {
                        self.collect_infer_params_with_constraints_inner(member_idx, params);
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                if let Some(func_type) = self.ctx.arena.get_function_type(node) {
                    if let Some(ref tps) = func_type.type_parameters {
                        for &tp_idx in &tps.nodes {
                            self.collect_infer_params_with_constraints_inner(tp_idx, params);
                        }
                    }
                    for &param_idx in &func_type.parameters.nodes {
                        self.collect_infer_params_with_constraints_inner(param_idx, params);
                    }
                    if func_type.type_annotation.is_some() {
                        self.collect_infer_params_with_constraints_inner(
                            func_type.type_annotation,
                            params,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(array_type) = self.ctx.arena.get_array_type(node) {
                    self.collect_infer_params_with_constraints_inner(
                        array_type.element_type,
                        params,
                    );
                }
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                if let Some(tuple_type) = self.ctx.arena.get_tuple_type(node) {
                    for &elem_idx in &tuple_type.elements.nodes {
                        self.collect_infer_params_with_constraints_inner(elem_idx, params);
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(type_lit) = self.ctx.arena.get_type_literal(node) {
                    for &member_idx in &type_lit.members.nodes {
                        self.collect_infer_params_with_constraints_inner(member_idx, params);
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                if let Some(op) = self.ctx.arena.get_type_operator(node) {
                    self.collect_infer_params_with_constraints_inner(op.type_node, params);
                }
            }
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                if let Some(indexed) = self.ctx.arena.get_indexed_access_type(node) {
                    self.collect_infer_params_with_constraints_inner(indexed.object_type, params);
                    self.collect_infer_params_with_constraints_inner(indexed.index_type, params);
                }
            }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                if let Some(mapped) = self.ctx.arena.get_mapped_type(node) {
                    self.collect_infer_params_with_constraints_inner(mapped.type_parameter, params);
                    if mapped.type_node.is_some() {
                        self.collect_infer_params_with_constraints_inner(mapped.type_node, params);
                    }
                    if mapped.name_type.is_some() {
                        self.collect_infer_params_with_constraints_inner(mapped.name_type, params);
                    }
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                if let Some(cond) = self.ctx.arena.get_conditional_type(node) {
                    self.collect_infer_params_with_constraints_inner(cond.check_type, params);
                    self.collect_infer_params_with_constraints_inner(cond.extends_type, params);
                    self.collect_infer_params_with_constraints_inner(cond.true_type, params);
                    self.collect_infer_params_with_constraints_inner(cond.false_type, params);
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                if let Some(template) = self.ctx.arena.get_template_literal_type(node) {
                    for &span_idx in &template.template_spans.nodes {
                        self.collect_infer_params_with_constraints_inner(span_idx, params);
                    }
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE_SPAN => {
                if let Some(span) = self.ctx.arena.get_template_span(node) {
                    self.collect_infer_params_with_constraints_inner(span.expression, params);
                }
            }
            k if k == syntax_kind_ext::TYPE_PREDICATE => {
                if let Some(predicate) = self.ctx.arena.get_type_predicate(node)
                    && predicate.type_node != NodeIndex::NONE
                {
                    self.collect_infer_params_with_constraints_inner(predicate.type_node, params);
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE
                || k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE =>
            {
                if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
                    self.collect_infer_params_with_constraints_inner(wrapped.type_node, params);
                }
            }
            k if k == syntax_kind_ext::NAMED_TUPLE_MEMBER => {
                if let Some(member) = self.ctx.arena.get_named_tuple_member(node) {
                    self.collect_infer_params_with_constraints_inner(member.type_node, params);
                }
            }
            k if k == syntax_kind_ext::TYPE_PARAMETER => {
                if let Some(type_param) = self.ctx.arena.get_type_parameter(node) {
                    if type_param.constraint != NodeIndex::NONE {
                        self.collect_infer_params_with_constraints_inner(
                            type_param.constraint,
                            params,
                        );
                    }
                    if type_param.default != NodeIndex::NONE {
                        self.collect_infer_params_with_constraints_inner(
                            type_param.default,
                            params,
                        );
                    }
                }
            }
            _ => {
                if let Some(sig) = self.ctx.arena.get_signature(node) {
                    if let Some(ref tps) = sig.type_parameters {
                        for &tp_idx in &tps.nodes {
                            self.collect_infer_params_with_constraints_inner(tp_idx, params);
                        }
                    }
                    if let Some(ref sig_params) = sig.parameters {
                        for &param_idx in &sig_params.nodes {
                            self.collect_infer_params_with_constraints_inner(param_idx, params);
                        }
                    }
                    if sig.type_annotation.is_some() {
                        self.collect_infer_params_with_constraints_inner(
                            sig.type_annotation,
                            params,
                        );
                    }
                } else if let Some(index_sig) = self.ctx.arena.get_index_signature(node) {
                    for &param_idx in &index_sig.parameters.nodes {
                        self.collect_infer_params_with_constraints_inner(param_idx, params);
                    }
                    if index_sig.type_annotation.is_some() {
                        self.collect_infer_params_with_constraints_inner(
                            index_sig.type_annotation,
                            params,
                        );
                    }
                } else if let Some(param) = self.ctx.arena.get_parameter(node)
                    && param.type_annotation != NodeIndex::NONE
                {
                    self.collect_infer_params_with_constraints_inner(param.type_annotation, params);
                }
            }
        }
    }

    fn collect_infer_type_parameters_inner(&self, type_idx: NodeIndex, params: &mut Vec<String>) {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::INFER_TYPE => {
                if let Some(infer) = self.ctx.arena.get_infer_type(node)
                    && let Some(param_node) = self.ctx.arena.get(infer.type_parameter)
                    && let Some(param) = self.ctx.arena.get_type_parameter(param_node)
                    && let Some(name_node) = self.ctx.arena.get(param.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                {
                    let name = ident.escaped_text.clone();
                    if !params.contains(&name) {
                        params.push(name);
                    }
                    self.collect_infer_type_parameters_inner(infer.type_parameter, params);
                }
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.ctx.arena.get_type_ref(node)
                    && let Some(ref args) = type_ref.type_arguments
                {
                    for &arg_idx in &args.nodes {
                        self.collect_infer_type_parameters_inner(arg_idx, params);
                    }
                }
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                    for &member_idx in &composite.types.nodes {
                        self.collect_infer_type_parameters_inner(member_idx, params);
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                if let Some(func_type) = self.ctx.arena.get_function_type(node) {
                    if let Some(ref tps) = func_type.type_parameters {
                        for &tp_idx in &tps.nodes {
                            self.collect_infer_type_parameters_inner(tp_idx, params);
                        }
                    }
                    for &param_idx in &func_type.parameters.nodes {
                        self.collect_infer_type_parameters_inner(param_idx, params);
                    }
                    if func_type.type_annotation.is_some() {
                        self.collect_infer_type_parameters_inner(func_type.type_annotation, params);
                    }
                }
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(array_type) = self.ctx.arena.get_array_type(node) {
                    self.collect_infer_type_parameters_inner(array_type.element_type, params);
                }
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                if let Some(tuple_type) = self.ctx.arena.get_tuple_type(node) {
                    for &elem_idx in &tuple_type.elements.nodes {
                        self.collect_infer_type_parameters_inner(elem_idx, params);
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(type_lit) = self.ctx.arena.get_type_literal(node) {
                    for &member_idx in &type_lit.members.nodes {
                        self.collect_infer_type_parameters_inner(member_idx, params);
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                if let Some(op) = self.ctx.arena.get_type_operator(node) {
                    self.collect_infer_type_parameters_inner(op.type_node, params);
                }
            }
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                if let Some(indexed) = self.ctx.arena.get_indexed_access_type(node) {
                    self.collect_infer_type_parameters_inner(indexed.object_type, params);
                    self.collect_infer_type_parameters_inner(indexed.index_type, params);
                }
            }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                if let Some(mapped) = self.ctx.arena.get_mapped_type(node) {
                    self.collect_infer_type_parameters_inner(mapped.type_parameter, params);
                    if mapped.type_node.is_some() {
                        self.collect_infer_type_parameters_inner(mapped.type_node, params);
                    }
                    if mapped.name_type.is_some() {
                        self.collect_infer_type_parameters_inner(mapped.name_type, params);
                    }
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                if let Some(cond) = self.ctx.arena.get_conditional_type(node) {
                    self.collect_infer_type_parameters_inner(cond.check_type, params);
                    self.collect_infer_type_parameters_inner(cond.extends_type, params);
                    self.collect_infer_type_parameters_inner(cond.true_type, params);
                    self.collect_infer_type_parameters_inner(cond.false_type, params);
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                if let Some(template) = self.ctx.arena.get_template_literal_type(node) {
                    for &span_idx in &template.template_spans.nodes {
                        self.collect_infer_type_parameters_inner(span_idx, params);
                    }
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE_SPAN => {
                if let Some(span) = self.ctx.arena.get_template_span(node) {
                    self.collect_infer_type_parameters_inner(span.expression, params);
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE
                || k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE =>
            {
                if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
                    self.collect_infer_type_parameters_inner(wrapped.type_node, params);
                }
            }
            k if k == syntax_kind_ext::NAMED_TUPLE_MEMBER => {
                if let Some(member) = self.ctx.arena.get_named_tuple_member(node) {
                    self.collect_infer_type_parameters_inner(member.type_node, params);
                }
            }
            k if k == syntax_kind_ext::TYPE_PREDICATE => {
                if let Some(predicate) = self.ctx.arena.get_type_predicate(node)
                    && predicate.type_node != NodeIndex::NONE
                {
                    self.collect_infer_type_parameters_inner(predicate.type_node, params);
                }
            }
            k if k == syntax_kind_ext::TYPE_PARAMETER => {
                if let Some(type_param) = self.ctx.arena.get_type_parameter(node) {
                    if type_param.constraint != NodeIndex::NONE {
                        self.collect_infer_type_parameters_inner(type_param.constraint, params);
                    }
                    if type_param.default != NodeIndex::NONE {
                        self.collect_infer_type_parameters_inner(type_param.default, params);
                    }
                }
            }
            _ => {
                if let Some(sig) = self.ctx.arena.get_signature(node) {
                    if let Some(ref tps) = sig.type_parameters {
                        for &tp_idx in &tps.nodes {
                            self.collect_infer_type_parameters_inner(tp_idx, params);
                        }
                    }
                    if let Some(ref sig_params) = sig.parameters {
                        for &param_idx in &sig_params.nodes {
                            self.collect_infer_type_parameters_inner(param_idx, params);
                        }
                    }
                    if sig.type_annotation.is_some() {
                        self.collect_infer_type_parameters_inner(sig.type_annotation, params);
                    }
                } else if let Some(index_sig) = self.ctx.arena.get_index_signature(node) {
                    for &param_idx in &index_sig.parameters.nodes {
                        self.collect_infer_type_parameters_inner(param_idx, params);
                    }
                    if index_sig.type_annotation.is_some() {
                        self.collect_infer_type_parameters_inner(index_sig.type_annotation, params);
                    }
                } else if let Some(param) = self.ctx.arena.get_parameter(node)
                    && param.type_annotation != NodeIndex::NONE
                {
                    self.collect_infer_type_parameters_inner(param.type_annotation, params);
                }
            }
        }
    }

    /// Walk `extends_type` collecting every `infer X` name together with its
    /// implicit constraint (`Some(TypeId::STRING)` when `X` is a direct span
    /// expression of a template-literal-type, `None` otherwise).
    ///
    /// Span-position `INFER_TYPE` node indices are pre-collected when the
    /// enclosing `TEMPLATE_LITERAL_TYPE` is encountered (always an ancestor),
    /// so the membership check is O(1) per infer node. Explicit descent into
    /// `infer_data.type_parameter` ensures that infer names nested inside infer
    /// constraints are also discovered with their implicit constraints.
    pub(crate) fn collect_infer_bindings_with_span_constraints(
        &self,
        extends_type: NodeIndex,
    ) -> Vec<(String, Option<TypeId>)> {
        if extends_type.is_none() {
            return Vec::new();
        }
        let mut result: Vec<(String, Option<TypeId>)> = Vec::new();
        let mut seen: rustc_hash::FxHashSet<String> = rustc_hash::FxHashSet::default();
        let mut span_infer_nodes: rustc_hash::FxHashSet<NodeIndex> =
            rustc_hash::FxHashSet::default();
        let mut stack = vec![extends_type];
        while let Some(idx) = stack.pop() {
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::TEMPLATE_LITERAL_TYPE {
                if let Some(tlt) = self.ctx.arena.get_template_literal_type(node) {
                    for &span_idx in &tlt.template_spans.nodes {
                        if let Some(span_node) = self.ctx.arena.get(span_idx)
                            && let Some(span) = self.ctx.arena.get_template_span(span_node)
                            && let Some(expr_node) = self.ctx.arena.get(span.expression)
                            && expr_node.kind == syntax_kind_ext::INFER_TYPE
                        {
                            span_infer_nodes.insert(span.expression);
                        }
                    }
                }
                // Fall through to get_children: infer constraints may contain
                // nested template literals that must also be walked.
            } else if node.kind == syntax_kind_ext::INFER_TYPE {
                if let Some(infer_data) = self.ctx.arena.get_infer_type(node) {
                    if let Some(tp_node) = self.ctx.arena.get(infer_data.type_parameter)
                        && let Some(tp_data) = self.ctx.arena.get_type_parameter(tp_node)
                        && let Some(name_node) = self.ctx.arena.get(tp_data.name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        let name = ident.escaped_text.clone();
                        if seen.insert(name.clone()) {
                            let constraint =
                                span_infer_nodes.contains(&idx).then_some(TypeId::STRING);
                            result.push((name, constraint));
                        }
                    }
                    stack.push(infer_data.type_parameter);
                }
                continue;
            }
            for child_idx in self.ctx.arena.get_children(idx) {
                stack.push(child_idx);
            }
        }
        result
    }
}
