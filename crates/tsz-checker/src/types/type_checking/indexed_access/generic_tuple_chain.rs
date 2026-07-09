//! Tuple-specific indexed-access key-space analysis.
//!
//! This module keeps concrete tuple element-domain checks and tuple-rooted
//! generic indexed-access chains in one place. Semantic operations continue
//! to route through query boundaries.

use crate::query_boundaries::type_checking as type_checking_query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

/// Classification of a type node as a generic indexed-access chain rooted at a
/// concrete tuple (`Table[D1]`, `AddDigitTable[Carry][T]`, …).
#[derive(Clone, Copy, PartialEq, Eq)]
enum GenericTupleChainVerdict {
    /// A tuple-rooted chain whose every index stays in the element-index domain;
    /// carries the chain's element-value union.
    Value(TypeId),
    /// A tuple-rooted chain with an index outside the element-index domain, so
    /// the chain stays deferred and exposes no key space.
    Escapes,
    /// Not a tuple-rooted chain; this analysis has no opinion.
    NotTupleRooted,
}

impl<'a> CheckerState<'a> {
    /// The number of fixed (non-rest) elements of a concrete tuple value, and
    /// whether it has an open rest tail (`[A, ...B[]]`). Returns `None` when
    /// `tuple_value` is not a concrete tuple (or a union of concrete tuples).
    ///
    /// For a union of tuples the fixed length is the maximum across members (an
    /// index in any member's range is acceptable) and the rest flag is the OR,
    /// mirroring `tsc`'s apparent-type behavior where `(A | B)[I]` is valid for
    /// any index in either operand's range.
    pub(super) fn tuple_fixed_length(&mut self, tuple_value: TypeId) -> Option<(usize, bool)> {
        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, tuple_value)
        {
            let members: Vec<TypeId> = members.iter().copied().collect();
            let mut max_len = 0usize;
            let mut any_rest = false;
            let mut saw_tuple = false;
            for member in members {
                let (len, rest) = self.tuple_fixed_length(member)?;
                saw_tuple = true;
                max_len = max_len.max(len);
                any_rest |= rest;
            }
            return saw_tuple.then_some((max_len, any_rest));
        }

        let elements =
            crate::query_boundaries::common::tuple_elements(self.ctx.types, tuple_value)?;
        let mut fixed_len = 0usize;
        let mut has_rest = false;
        for element in &elements {
            if element.rest {
                has_rest = true;
            } else {
                fixed_len += 1;
            }
        }
        Some((fixed_len, has_rest))
    }

    /// Whether the index constraint `index_for_check` stays within a concrete
    /// tuple's element-index domain: every union member must be either the
    /// abstract `number` primitive (which `tsc` resolves to the element value)
    /// or a numeric literal `0 <= n < len`. An out-of-range literal, a string
    /// key, or `keyof Base` (which includes `length`/method names) escapes the
    /// domain, so the chain no longer resolves to an element value and the
    /// caller keeps the genuine `TS2536`. With an open rest tail any numeric
    /// literal is in range.
    fn index_within_tuple_element_domain(
        &mut self,
        index_for_check: TypeId,
        tuple_value: TypeId,
    ) -> bool {
        let Some((len, has_rest)) = self.tuple_fixed_length(tuple_value) else {
            return false;
        };
        let members: Vec<TypeId> =
            crate::query_boundaries::common::union_members(self.ctx.types, index_for_check)
                .map(|members| members.iter().copied().collect())
                .unwrap_or_else(|| vec![index_for_check]);
        if members.is_empty() {
            return false;
        }
        members.into_iter().all(|member| {
            if member == TypeId::NUMBER {
                return true;
            }
            if let Some(value) =
                crate::query_boundaries::common::number_literal_value(self.ctx.types, member)
            {
                return value.fract() == 0.0
                    && value >= 0.0
                    && (has_rest || (value as usize) < len);
            }
            // A string-literal key that spells a canonical array index (`'0'`,
            // `'1'`, …) indexes a tuple/array element exactly like its numeric
            // form. Accept it within the same numeric domain so a generic index
            // constrained to `'0' | '1'` stays a tuple element access rather than
            // spuriously escaping to `TS2536`.
            if let Some(index) = self.string_literal_numeric_index(member) {
                return has_rest || index < len;
            }
            false
        })
    }

    /// Resolve the apparent value-type union of a generic indexed-access chain
    /// `Base[I]`, `Base[I][J]`, … whose innermost base is a concrete tuple, when
    /// every intermediate index stays within the tuple element-index domain.
    ///
    /// This mirrors `tsc`: indexing a concrete tuple `Base` with a generic index
    /// `I extends C` where `C` lies in the tuple's numeric index space yields the
    /// element-value union `Base[number]`; a further index `Base[I][J]` then
    /// indexes that union. When an intermediate index constraint escapes the
    /// numeric index domain (e.g. an out-of-range literal, `keyof Base`, or a
    /// string key), the chain no longer resolves to an element value and the
    /// helper returns `None`, so the caller keeps the genuine `TS2536`.
    ///
    /// Returns `None` when `node_idx` is not such a chain. Bounded by chain depth
    /// and tuple length; no unbounded instantiation.
    fn generic_tuple_chain_value_type(&mut self, node_idx: NodeIndex) -> Option<TypeId> {
        match self.generic_tuple_chain_verdict(node_idx) {
            GenericTupleChainVerdict::Value(value) => Some(value),
            GenericTupleChainVerdict::Escapes | GenericTupleChainVerdict::NotTupleRooted => None,
        }
    }

    /// Classify `node_idx` as a generic indexed-access chain rooted at a concrete
    /// tuple, distinguishing "not such a chain" from "such a chain whose index
    /// escapes the tuple's element-index domain". Only the latter licenses a
    /// `TS2536` at the enclosing access: `tsc` leaves `Table[D1]` deferred when
    /// `D1 extends 0 | 1 | 2` over a 2-tuple, so the chain has no known key space
    /// and `Table[D1][0]` errors — even though evaluating the inner access here
    /// yields a clean element union that would accept `0`.
    ///
    /// Bounded by chain depth and tuple length; no unbounded instantiation.
    fn generic_tuple_chain_verdict(&mut self, node_idx: NodeIndex) -> GenericTupleChainVerdict {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return GenericTupleChainVerdict::NotTupleRooted;
        };
        let Some(indexed) = self.ctx.arena.get_indexed_access_type(node) else {
            return GenericTupleChainVerdict::NotTupleRooted;
        };
        let inner_node_idx = indexed.object_type;
        let index_node_idx = indexed.index_type;

        // Resolve the base value union: either a concrete tuple directly, or a
        // nested generic chain that itself bottoms out at a concrete tuple.
        let base_value = match self.generic_tuple_chain_verdict(inner_node_idx) {
            GenericTupleChainVerdict::Value(parent) => parent,
            GenericTupleChainVerdict::Escapes => return GenericTupleChainVerdict::Escapes,
            GenericTupleChainVerdict::NotTupleRooted => {
                let resolved = self.get_type_from_type_node(inner_node_idx);
                let resolved = self.evaluate_type_with_env(resolved);
                if resolved == TypeId::ERROR || self.tuple_fixed_length(resolved).is_none() {
                    return GenericTupleChainVerdict::NotTupleRooted;
                }
                resolved
            }
        };

        // This level indexes `base_value`, so it is a tuple-element index only when
        // `base_value` is itself tuple-like. A parent chain whose element union is,
        // say, an object (`typeof x[0]` over `[{ tags: [...] }]`) makes the next key
        // an ordinary property lookup, which this analysis has no opinion about.
        if self.tuple_fixed_length(base_value).is_none() {
            return GenericTupleChainVerdict::NotTupleRooted;
        }

        // The index at this level must stay within the tuple element-index domain.
        let mut index_constraint = crate::query_boundaries::common::type_parameter_constraint(
            self.ctx.types,
            self.get_type_from_type_node(index_node_idx),
        );
        if index_constraint.is_none()
            && crate::query_boundaries::common::is_type_parameter_like(
                self.ctx.types,
                self.get_type_from_type_node(index_node_idx),
            )
        {
            index_constraint =
                self.resolve_index_constraint_from_declaration(index_node_idx, inner_node_idx);
        }
        let index_for_check =
            index_constraint.unwrap_or_else(|| self.get_type_from_type_node(index_node_idx));
        let index_for_check = self.evaluate_type_with_env(index_for_check);
        if !self.index_within_tuple_element_domain(index_for_check, base_value) {
            return GenericTupleChainVerdict::Escapes;
        }

        // The resolved value is the tuple's element-value union (`base[number]`).
        let element_union = type_checking_query::type_checking_index_access(
            self.ctx.types,
            base_value,
            TypeId::NUMBER,
        );
        let element_union = self.evaluate_type_with_env(element_union);
        if matches!(element_union, TypeId::ERROR | TypeId::UNDEFINED) {
            // The element union is unusable, so this analysis has nothing to say;
            // leave the decision to the general recovery paths.
            return GenericTupleChainVerdict::NotTupleRooted;
        }
        GenericTupleChainVerdict::Value(element_union)
    }

    /// Whether `Chain[J]` is a genuine `TS2536` that the general key-space
    /// recovery would miss, because `Chain` is a tuple-rooted generic
    /// indexed-access chain that `tsc` keeps deferred. Either an intermediate
    /// index escapes the tuple element-index domain, or `J` lies outside the
    /// chain's element-value key space. Returns `false` when `Chain` is not such
    /// a chain, leaving the existing recovery paths in charge.
    pub(super) fn generic_tuple_chain_index_access_rejects(
        &mut self,
        object_type_node_idx: NodeIndex,
        outer_index_node_idx: NodeIndex,
        outer_index_type: TypeId,
    ) -> bool {
        match self.generic_tuple_chain_verdict(object_type_node_idx) {
            GenericTupleChainVerdict::NotTupleRooted => false,
            GenericTupleChainVerdict::Escapes => true,
            GenericTupleChainVerdict::Value(value_union) => !self
                .outer_index_within_chain_value_key_space(
                    value_union,
                    object_type_node_idx,
                    outer_index_node_idx,
                    outer_index_type,
                ),
        }
    }

    fn outer_index_within_chain_value_key_space(
        &mut self,
        value_union: TypeId,
        object_type_node_idx: NodeIndex,
        outer_index_node_idx: NodeIndex,
        outer_index_type: TypeId,
    ) -> bool {
        let value_keyof = self.indexed_access_keyof_with_env(value_union);

        let mut outer_index_constraint = crate::query_boundaries::common::type_parameter_constraint(
            self.ctx.types,
            outer_index_type,
        );
        if outer_index_constraint.is_none()
            && crate::query_boundaries::common::is_type_parameter_like(
                self.ctx.types,
                outer_index_type,
            )
        {
            outer_index_constraint = self.resolve_index_constraint_from_declaration(
                outer_index_node_idx,
                object_type_node_idx,
            );
        }
        let outer_for_check = outer_index_constraint.unwrap_or(outer_index_type);
        let outer_for_check = self.evaluate_type_with_env(outer_for_check);
        self.indexed_access_key_space_relation_outcome(outer_for_check, value_keyof)
            .related
    }

    /// Whether `Chain[J]` is a valid indexed access where `Chain` is a generic
    /// indexed-access chain rooted at a concrete tuple base (e.g.
    /// `Table[D1][0]`, `AddDigitTable[Carry][T][U]`). The chain's apparent value
    /// type is the element-value union of the tuple; the outer index `J` must lie
    /// in `keyof` of that union. Returns `false` (keeping any genuine `TS2536`)
    /// when the chain is not tuple-rooted or `J` is out of the value key-space.
    pub(super) fn generic_tuple_chain_index_access_allows_index(
        &mut self,
        object_type_node_idx: NodeIndex,
        outer_index_node_idx: NodeIndex,
        outer_index_type: TypeId,
    ) -> bool {
        let Some(value_union) = self.generic_tuple_chain_value_type(object_type_node_idx) else {
            return false;
        };
        self.outer_index_within_chain_value_key_space(
            value_union,
            object_type_node_idx,
            outer_index_node_idx,
            outer_index_type,
        )
    }
}
