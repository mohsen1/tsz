//! Public flow-type query entry points.
//!
//! `get_flow_type` is the boundary the rest of the checker calls to ask "what
//! is this reference's narrowed type at this flow node?". It layers three
//! concerns on top of the core `check_flow` traversal:
//! - an ERROR short-circuit so suppressed errors are not narrowed into
//!   concrete (false-positive-producing) types,
//! - a re-entrant nesting bound (`MAX_FLOW_QUERY_DEPTH`, mirroring tsc's
//!   `flowDepth` guard) so mutually-dependent reference queries cannot
//!   overflow the native stack, and
//! - correlated destructured-binding narrowing, which refines a `const`
//!   destructured binding by what its sibling bindings' narrowing implies
//!   about the shared source union.

use super::{FlowAnalyzer, resolve_tuple_binding_type, symbol_first_identifier_ref};
use crate::query_boundaries::flow_analysis::{tuple_elements_for_type, union_members_for_type};
use crate::query_boundaries::state::checking::find_property_in_object_by_str;
use tsz_binder::{FlowNodeId, SymbolId};
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

/// Maximum nesting depth of re-entrant flow-type queries before narrowing bails
/// to the un-narrowed declared type. Matches tsc's `flowDepth` ceiling of 2000
/// in `getFlowTypeOfReference`: deep enough that no realistic narrowing chain is
/// truncated, low enough that the native stack cannot overflow (each level adds
/// one `check_flow` frame; 2000 frames stay well within the 128MB worker stack).
const MAX_FLOW_QUERY_DEPTH: u32 = 2000;

impl<'a> FlowAnalyzer<'a> {
    /// Get the narrowed type of a symbol at a specific flow node.
    ///
    /// This walks backwards through the flow graph, applying narrowing operations
    /// when it encounters condition nodes.
    pub fn get_flow_type(
        &self,
        reference: NodeIndex,
        initial_type: TypeId,
        flow_node: FlowNodeId,
    ) -> TypeId {
        // Short-circuit for error types: flow narrowing must not transform ERROR
        // into a concrete type. When the declared/initial type is ERROR (e.g.,
        // property access on an unresolved type), condition narrowing handlers
        // like `== null` can produce `null | undefined` regardless of the input
        // type, turning a suppressed error into a false positive diagnostic.
        if initial_type == TypeId::ERROR {
            return initial_type;
        }
        let narrowed = self.get_flow_type_uncorrelated(reference, initial_type, flow_node);
        self.apply_correlated_destructured_narrowing(reference, initial_type, narrowed, flow_node)
    }

    fn get_flow_type_uncorrelated(
        &self,
        reference: NodeIndex,
        initial_type: TypeId,
        flow_node: FlowNodeId,
    ) -> TypeId {
        if flow_node.is_none() {
            return initial_type;
        }

        // Bound re-entrant flow-query nesting. Each nested `get_flow_type`
        // resolution adds a `check_flow` frame; without a ceiling, deeply
        // nested narrowing in large modules overflows the native stack. tsc
        // applies the same guard (`flowDepth === 2000` in
        // `getFlowTypeOfReference`), returning the un-narrowed declared type
        // rather than narrowing further. We mirror that: bail to `initial_type`.
        let depth = self.flow_query_depth.get();
        if depth >= MAX_FLOW_QUERY_DEPTH {
            return initial_type;
        }
        self.flow_query_depth.set(depth + 1);
        let result = self.get_flow_type_uncorrelated_inner(reference, initial_type, flow_node);
        self.flow_query_depth.set(depth);
        result
    }

    fn get_flow_type_uncorrelated_inner(
        &self,
        reference: NodeIndex,
        initial_type: TypeId,
        flow_node: FlowNodeId,
    ) -> TypeId {
        // Resolve symbol for caching purposes.
        //
        // Member-like references (`a.b`, `a[b]`, `this.x`) must NOT key by a bare
        // member `SymbolId`: the binder links some member accesses (notably
        // `this.x` on a class field) to the field symbol, which both aliases
        // distinct receivers (`x.foo` vs `this.foo` share the field symbol) and
        // defeats occurrence sharing for everything else. `check_flow` keys such
        // references by their structural path instead (`flow_reference_path_symbol`),
        // which is occurrence-stable and receiver-disjoint. Only resolve a symbol
        // for plain identifier / `this` / `super` roots.
        let symbol_id = self
            .binder
            .resolve_identifier(self.arena, reference)
            .or_else(|| {
                if self.is_member_like_reference(reference) {
                    None
                } else {
                    self.reference_symbol(reference)
                }
            });

        // Non-narrowable reference short-circuit (mirrors tsc's
        // `getFlowTypeOfReference`, which only walks the flow graph for
        // narrowable references). When the reference carries no binder symbol AND
        // is not a narrowable member access — its receiver chain bottoms out at a
        // call result / non-narrowable expression (e.g. `readIndexed('p').a.b`,
        // the indexed-access hotspot) — no flow node can `is_matching_reference`-
        // match it, so the backward walk provably returns the declared type
        // unchanged at every node. Skipping it is byte-identical and removes the
        // O(N^2) per-antecedent enumeration the worklist would otherwise perform
        // over preceding statements (each call/condition/assignment node re-runs
        // `is_matching_reference` against the call-rooted reference). Bare
        // identifiers, `this`/`super`, and any narrowable member path still walk.
        if symbol_id.is_none()
            && self.is_member_like_reference(reference)
            && self.reference_root_symbol(reference).is_none()
            && !self.is_narrowable_member_reference(reference)
        {
            return initial_type;
        }

        self.check_flow(
            reference,
            initial_type,
            flow_node,
            &mut Vec::new(),
            symbol_id,
        )
    }

    fn apply_correlated_destructured_narrowing(
        &self,
        reference: NodeIndex,
        _initial_type: TypeId,
        narrowed_type: TypeId,
        flow_node: FlowNodeId,
    ) -> TypeId {
        let Some(bindings) = self.destructured_bindings else {
            return narrowed_type;
        };
        let Some(sym_id) = self
            .binder
            .resolve_identifier(self.arena, reference)
            .or_else(|| self.reference_symbol(reference))
        else {
            return narrowed_type;
        };
        let Some(info) = bindings.get(&sym_id) else {
            return narrowed_type;
        };
        if !info.is_const {
            return narrowed_type;
        }

        let Some(source_members) = union_members_for_type(self.interner, info.source_type) else {
            return narrowed_type;
        };

        let siblings: Vec<_> = bindings
            .iter()
            .filter(|(other_sym, other_info)| {
                **other_sym != sym_id && other_info.group_id == info.group_id && other_info.is_const
            })
            .map(|(other_sym, other_info)| (*other_sym, other_info))
            .collect();
        if siblings.is_empty() {
            return narrowed_type;
        }

        let mut remaining_members = source_members.to_vec();
        let original_member_count = remaining_members.len();

        for (sib_sym, sib_info) in siblings {
            let Some(sib_ref) = self.symbol_identifier_ref(sib_sym) else {
                continue;
            };
            let Some(sib_initial) =
                self.derive_binding_type_from_members(&source_members, sib_info)
            else {
                continue;
            };

            let sib_narrowed = self.get_flow_type_uncorrelated(sib_ref, sib_initial, flow_node);
            if sib_narrowed == sib_initial {
                continue;
            }

            remaining_members.retain(|&member| {
                self.binding_type_from_member(member, sib_info)
                    .is_none_or(|member_ty| self.types_overlap(member_ty, sib_narrowed))
            });
        }

        if remaining_members.len() == original_member_count {
            return narrowed_type;
        }
        if remaining_members.is_empty() {
            return TypeId::NEVER;
        }

        let Some(correlated) = self.derive_binding_type_from_members(&remaining_members, info)
        else {
            return narrowed_type;
        };

        if correlated == narrowed_type {
            return correlated;
        }

        self.intersect_types(correlated, narrowed_type)
            .unwrap_or(correlated)
    }

    fn symbol_identifier_ref(&self, sym: SymbolId) -> Option<NodeIndex> {
        symbol_first_identifier_ref(
            self.arena,
            self.binder,
            self.shared_symbol_first_identifier_ref(),
            sym,
        )
    }

    fn binding_type_from_member(
        &self,
        member: TypeId,
        info: &crate::context::DestructuredBindingInfo,
    ) -> Option<TypeId> {
        if !info.property_name.is_empty() {
            let mut current = member;
            for segment in info.property_name.split('.') {
                let prop = find_property_in_object_by_str(self.interner, current, segment)?;
                current = prop.type_id;
            }
            Some(current)
        } else if let Some(elements) = tuple_elements_for_type(self.interner, member) {
            resolve_tuple_binding_type(
                self.interner,
                &elements,
                info.element_index as usize,
                info.is_rest,
            )
        } else {
            None
        }
    }

    fn derive_binding_type_from_members(
        &self,
        members: &[TypeId],
        info: &crate::context::DestructuredBindingInfo,
    ) -> Option<TypeId> {
        let mut result_types = Vec::new();
        for &member in members {
            if let Some(member_ty) = self.binding_type_from_member(member, info) {
                result_types.push(member_ty);
            }
        }
        if result_types.is_empty() {
            None
        } else {
            Some(tsz_solver::utils::union_or_single(
                self.interner,
                result_types,
            ))
        }
    }

    fn types_overlap(&self, left: TypeId, right: TypeId) -> bool {
        left == right
            || self.flow_assignability_related(left, right)
            || self.flow_assignability_related(right, left)
    }

    fn intersect_types(&self, left: TypeId, right: TypeId) -> Option<TypeId> {
        let left_members = union_members_for_type(self.interner, left);
        let right_members = union_members_for_type(self.interner, right);

        match (left_members, right_members) {
            (Some(left_members), Some(right_members)) => {
                let filtered: Vec<_> = left_members
                    .iter()
                    .filter(|member| right_members.contains(member))
                    .copied()
                    .collect();
                if filtered.is_empty() {
                    None
                } else {
                    Some(tsz_solver::utils::union_or_single(self.interner, filtered))
                }
            }
            (Some(left_members), None) => left_members.contains(&right).then_some(right),
            (None, Some(right_members)) => right_members.contains(&left).then_some(left),
            (None, None) => (left == right).then_some(left),
        }
    }
}
