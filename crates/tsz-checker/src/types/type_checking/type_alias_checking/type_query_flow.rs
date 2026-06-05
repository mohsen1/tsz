//! `typeof` flow-type precomputation for type alias bodies.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Walk a type node AST subtree to find `TYPE_QUERY` nodes (`typeof expr`)
    /// and pre-compute the flow-narrowed type of each expression.
    ///
    /// This is called during `check_type_alias_declaration` so that when the
    /// type alias body is later lowered by `ensure_type_alias_resolved`, the
    /// `TypeLowering` can use these pre-computed types instead of creating
    /// deferred `TypeQuery` types that would lose flow narrowing information.
    pub(super) fn precompute_type_query_flow_types(&mut self, node_idx: NodeIndex) {
        if node_idx == NodeIndex::NONE {
            return;
        }
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::TYPE_QUERY {
            // Found a `typeof expr` in type position; compute the flow-narrowed
            // type of the expression and store it in node_types.
            if let Some(type_query) = self.ctx.arena.get_type_query(node) {
                let expr_name = type_query.expr_name;
                if expr_name != NodeIndex::NONE && !self.ctx.node_types.contains_key(&expr_name.0) {
                    let narrowed = self.get_type_of_identifier(expr_name);
                    if narrowed != TypeId::ERROR {
                        self.ctx.node_types.insert(expr_name.0, narrowed);
                    }
                }
            }
            return;
        }

        // Recurse into child type nodes to find nested TYPE_QUERY nodes.
        match node.kind {
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(type_lit) = self.ctx.arena.get_type_literal(node) {
                    for &member_idx in &type_lit.members.nodes {
                        let Some(member) = self.ctx.arena.get(member_idx) else {
                            continue;
                        };
                        if let Some(sig) = self.ctx.arena.get_signature(member) {
                            if let Some(params) = &sig.parameters {
                                for &p in &params.nodes {
                                    if let Some(pn) = self.ctx.arena.get(p)
                                        && let Some(pd) = self.ctx.arena.get_parameter(pn)
                                    {
                                        self.precompute_type_query_flow_types(pd.type_annotation);
                                    }
                                }
                            }
                            self.precompute_type_query_flow_types(sig.type_annotation);
                        } else if let Some(prop) = self.ctx.arena.get_property_decl(member) {
                            self.precompute_type_query_flow_types(prop.type_annotation);
                        } else if let Some(idx_sig) = self.ctx.arena.get_index_signature(member) {
                            self.precompute_type_query_flow_types(idx_sig.type_annotation);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                    for &child in &composite.types.nodes {
                        self.precompute_type_query_flow_types(child);
                    }
                }
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(arr) = self.ctx.arena.get_array_type(node) {
                    self.precompute_type_query_flow_types(arr.element_type);
                }
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                if let Some(tuple) = self.ctx.arena.get_tuple_type(node) {
                    for &elem in &tuple.elements.nodes {
                        self.precompute_type_query_flow_types(elem);
                    }
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
                if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
                    self.precompute_type_query_flow_types(wrapped.type_node);
                }
            }
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                if let Some(indexed) = self.ctx.arena.get_indexed_access_type(node) {
                    self.precompute_type_query_flow_types(indexed.object_type);
                    self.precompute_type_query_flow_types(indexed.index_type);
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                if let Some(cond) = self.ctx.arena.get_conditional_type(node) {
                    self.precompute_type_query_flow_types(cond.check_type);
                    self.precompute_type_query_flow_types(cond.extends_type);
                    self.precompute_type_query_flow_types(cond.true_type);
                    self.precompute_type_query_flow_types(cond.false_type);
                }
            }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                if let Some(mapped) = self.ctx.arena.get_mapped_type(node) {
                    self.precompute_type_query_flow_types(mapped.type_node);
                }
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.ctx.arena.get_type_ref(node)
                    && let Some(args) = &type_ref.type_arguments
                {
                    for &arg in &args.nodes {
                        self.precompute_type_query_flow_types(arg);
                    }
                }
            }
            _ => {}
        }
    }
}
