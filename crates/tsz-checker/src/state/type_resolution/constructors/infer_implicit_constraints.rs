//! Implicit-constraint resolution for `infer X` bindings inside a conditional
//! type's `extends` clause.
//!
//! When walking the true branch of a conditional type, the checker pushes each
//! `infer X` name into the type-parameter scope so identifier resolution does
//! not emit TS2304 for the inferred name. The TypeId pushed into scope is
//! later substituted (with its declared constraint) by code that needs a
//! concrete witness for the parameter — for example,
//! `scoped_type_param_substituted_form` in `mapped_constraint_helpers.rs`
//! collapses `F<X>` to `F<constraint-of-X>` so TS2344 can compare the
//! evaluated form to the required constraint.
//!
//! Pushing `constraint: None` (i.e. `unknown`) drops information that tsc
//! preserves: `infer X` declared inside a template-literal slot is constrained
//! to `string`; declared after a rest token in a tuple or as a rest parameter,
//! to `unknown[]`; declared as `infer X extends C`, to `C`. Without those
//! constraints the substituted witness collapses to `unknown`, which then
//! fails TS2344 against any non-trivial constraint — manifesting as
//! false-positive TS2344 on recursive template-literal aliases like
//! `CamelCase`, `KebabToCamel`, and friends (#6748).
//!
//! This module computes those implicit/explicit constraints from the AST.

use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Push every `infer X` declared in `extends_type` into the type-parameter
    /// scope, recording each parameter's implicit/explicit constraint so the
    /// pushed `TypeParameter` carries the right shape for later substitution.
    /// Returns the list of `(name, previous)` bindings to feed into
    /// [`Self::pop_infer_bindings_for_missing_names`].
    pub(crate) fn push_infer_bindings_for_missing_names(
        &mut self,
        extends_type: NodeIndex,
    ) -> Vec<(String, Option<TypeId>)> {
        if extends_type.is_none() {
            return Vec::new();
        }
        let mut collected: FxHashMap<String, Option<TypeId>> = FxHashMap::default();
        self.collect_infer_constraints_into(extends_type, None, &mut collected);
        if collected.is_empty() {
            return Vec::new();
        }
        let mut bindings = Vec::with_capacity(collected.len());
        for (name, constraint) in collected {
            let atom = self.ctx.types.intern_string(&name);
            let type_id = self
                .ctx
                .types
                .factory()
                .type_param(tsz_solver::TypeParamInfo {
                    name: atom,
                    constraint,
                    default: None,
                    is_const: false,
                });
            let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
            bindings.push((name, previous));
        }
        bindings
    }

    /// Reverse a [`Self::push_infer_bindings_for_missing_names`] push.
    pub(crate) fn pop_infer_bindings_for_missing_names(
        &mut self,
        bindings: Vec<(String, Option<TypeId>)>,
    ) {
        for (name, previous) in bindings.into_iter().rev() {
            if let Some(prev_type) = previous {
                self.ctx.type_parameter_scope.insert(name, prev_type);
            } else {
                self.ctx.type_parameter_scope.remove(&name);
            }
        }
    }

    /// Walk `node_idx` once, recording every `infer X` with the strongest
    /// available constraint:
    /// - explicit `extends C` overrides any implicit ctx,
    /// - else `implicit_ctx` if the surrounding position carries one
    ///   (`${...}` → `string`, rest position → `unknown[]`),
    /// - else `None`.
    ///
    /// Specific node kinds get bespoke handling so `implicit_ctx` is set
    /// correctly when crossing into a context that carries one (template
    /// literal slot, rest parameter); all other kinds descend generically via
    /// [`NodeAccess::get_children`] so kinds like `TYPE_PREDICATE`,
    /// `INDEXED_ACCESS_TYPE`, `MAPPED_TYPE`, and `TYPE_QUERY` still surface
    /// any nested `infer X`. Duplicate names are merged via
    /// [`Self::merge_infer_constraint`].
    fn collect_infer_constraints_into(
        &mut self,
        node_idx: NodeIndex,
        implicit_ctx: Option<TypeId>,
        out: &mut FxHashMap<String, Option<TypeId>>,
    ) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::INFER_TYPE => {
                if let Some(infer_data) = self.ctx.arena.get_infer_type(node) {
                    self.record_infer_binding(infer_data, implicit_ctx, out);
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                if let Some(tlt) = self.ctx.arena.get_template_literal_type(node) {
                    let span_nodes: Vec<NodeIndex> = tlt.template_spans.nodes.clone();
                    for span_idx in span_nodes {
                        if let Some(span_node) = self.ctx.arena.get(span_idx)
                            && let Some(span_data) = self.ctx.arena.get_template_span(span_node)
                        {
                            let expr = span_data.expression;
                            self.collect_infer_constraints_into(expr, Some(TypeId::STRING), out);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::REST_TYPE => {
                if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
                    let inner = wrapped.type_node;
                    let ctx = self.ctx.types.factory().array(TypeId::UNKNOWN);
                    self.collect_infer_constraints_into(inner, Some(ctx), out);
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE
                || k == syntax_kind_ext::OPTIONAL_TYPE =>
            {
                if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
                    let inner = wrapped.type_node;
                    self.collect_infer_constraints_into(inner, implicit_ctx, out);
                }
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                if let Some(func_type) = self.ctx.arena.get_function_type(node) {
                    let param_nodes: Vec<NodeIndex> = func_type.parameters.nodes.clone();
                    let return_type = func_type.type_annotation;
                    for param_idx in param_nodes {
                        self.collect_function_param_infer_constraints(param_idx, out);
                    }
                    if !return_type.is_none() {
                        self.collect_infer_constraints_into(return_type, None, out);
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(type_lit) = self.ctx.arena.get_type_literal(node) {
                    let member_nodes: Vec<NodeIndex> = type_lit.members.nodes.clone();
                    for member_idx in member_nodes {
                        self.collect_type_literal_member_constraint(member_idx, out);
                    }
                }
            }
            _ => {
                // Generic descent for any other kind (TUPLE_TYPE, TYPE_REFERENCE,
                // UNION_TYPE, INTERSECTION_TYPE, ARRAY_TYPE, CONDITIONAL_TYPE,
                // TYPE_PREDICATE, INDEXED_ACCESS_TYPE, MAPPED_TYPE, TYPE_QUERY,
                // TYPE_OPERATOR, NAMED_TUPLE_MEMBER, …). Crossing one of these
                // boundaries does not carry forward an implicit constraint, so
                // children are walked with `implicit_ctx = None`.
                let children: Vec<NodeIndex> = self.ctx.arena.get_children(node_idx);
                for child in children {
                    self.collect_infer_constraints_into(child, None, out);
                }
            }
        }
    }

    fn record_infer_binding(
        &mut self,
        infer_data: &tsz_parser::parser::node::InferTypeData,
        implicit_ctx: Option<TypeId>,
        out: &mut FxHashMap<String, Option<TypeId>>,
    ) {
        let Some(tp_node) = self.ctx.arena.get(infer_data.type_parameter) else {
            return;
        };
        let Some(tp_data) = self.ctx.arena.get_type_parameter(tp_node) else {
            return;
        };
        let Some(name_node) = self.ctx.arena.get(tp_data.name) else {
            return;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return;
        };
        let name = ident.escaped_text.clone();
        // Explicit `infer X extends C` is intentionally NOT resolved here.
        // Resolving `C` via `get_type_from_type_node` would re-emit any
        // diagnostics already reported when walking the extends clause for
        // missing names, and the solver's conditional evaluator picks up the
        // explicit constraint directly when it instantiates the conditional.
        // The scope push exists only to satisfy identifier lookup and to give
        // `scoped_type_param_substituted_form` a witness for implicit cases.
        let candidate = if tp_data.constraint.is_none() {
            implicit_ctx
        } else {
            None
        };

        match (out.get(&name).copied(), candidate) {
            (None, c) => {
                out.insert(name, c);
            }
            (Some(_), None) => {}
            (Some(existing), Some(_)) => {
                let merged = Self::merge_infer_constraint(existing, candidate);
                out.insert(name, merged);
            }
        }
    }

    fn collect_function_param_infer_constraints(
        &mut self,
        param_idx: NodeIndex,
        out: &mut FxHashMap<String, Option<TypeId>>,
    ) {
        let Some(param_node) = self.ctx.arena.get(param_idx) else {
            return;
        };
        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
            return;
        };
        if param.type_annotation == NodeIndex::NONE {
            return;
        }
        let annotation = param.type_annotation;
        let ctx = if param.dot_dot_dot_token {
            Some(self.ctx.types.factory().array(TypeId::UNKNOWN))
        } else {
            None
        };
        self.collect_infer_constraints_into(annotation, ctx, out);
    }

    fn collect_type_literal_member_constraint(
        &mut self,
        member_idx: NodeIndex,
        out: &mut FxHashMap<String, Option<TypeId>>,
    ) {
        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return;
        };
        if let Some(prop) = self.ctx.arena.get_property_decl(member_node)
            && !prop.type_annotation.is_none()
        {
            let annotation = prop.type_annotation;
            self.collect_infer_constraints_into(annotation, None, out);
            return;
        }
        if let Some(sig) = self.ctx.arena.get_signature(member_node) {
            let return_type = sig.type_annotation;
            let param_nodes: Vec<NodeIndex> = sig
                .parameters
                .as_ref()
                .map_or_else(Vec::new, |p| p.nodes.clone());
            if !return_type.is_none() {
                self.collect_infer_constraints_into(return_type, None, out);
            }
            for param_idx in param_nodes {
                self.collect_function_param_infer_constraints(param_idx, out);
            }
        }
    }

    /// Conflicting implicit constraints collapse to `None`: a name inferred
    /// in two positions whose contexts disagree (e.g. one slot says `string`
    /// and another says `unknown[]`) should fall back to `unknown` rather
    /// than pick the wrong primitive.
    fn merge_infer_constraint(
        existing: Option<TypeId>,
        candidate: Option<TypeId>,
    ) -> Option<TypeId> {
        match (existing, candidate) {
            (None, c) | (c, None) => c,
            (Some(a), Some(b)) if a == b => Some(a),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_infer_constraint_conflict_drops_to_none() {
        let s = Some(TypeId::STRING);
        let n = Some(TypeId::NUMBER);
        assert_eq!(
            CheckerState::merge_infer_constraint(None, s),
            s,
            "absent + Some keeps Some"
        );
        assert_eq!(
            CheckerState::merge_infer_constraint(s, None),
            s,
            "Some + absent keeps Some"
        );
        assert_eq!(
            CheckerState::merge_infer_constraint(s, s),
            s,
            "matching candidates agree"
        );
        assert_eq!(
            CheckerState::merge_infer_constraint(s, n),
            None,
            "conflicting candidates collapse to None"
        );
    }
}
