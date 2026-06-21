//! Type-predicate (`p is X`) assignability checking (TS2677).
//!
//! Split out of `type_node.rs` to keep that module under the 2000-line
//! checker file boundary. Operates on [`TypeNodeChecker`].

use super::type_node::TypeNodeChecker;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl TypeNodeChecker<'_, '_> {
    /// TS2677: A type predicate's type must be assignable to its parameter's type.
    pub(super) fn check_type_predicate_assignability(
        &mut self,
        function_type_idx: NodeIndex,
        type_annotation: NodeIndex,
        lowered_type: TypeId,
    ) {
        if type_annotation.is_none() {
            return;
        }
        let predicate_node_idx = match self.find_type_predicate_in_type(type_annotation) {
            Some(idx) => idx,
            None => return,
        };
        let Some(pred_node) = self.ctx.arena.get(predicate_node_idx) else {
            return;
        };
        let Some(pred_data) = self.ctx.arena.get_type_predicate(pred_node) else {
            return;
        };
        if pred_data.type_node.is_none() {
            return;
        }
        let Some(predicate_name) = self.ctx.arena.get_identifier_text(pred_data.parameter_name)
        else {
            return;
        };

        // A signature's value parameters are in scope for every type position
        // of that signature, including a type-predicate's asserted type.
        let predicate_param_shape =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, lowered_type);
        if let Some(shape) = &predicate_param_shape {
            for param in &shape.params {
                if let Some(atom) = param.name {
                    self.ctx
                        .typeof_param_scope
                        .insert(self.ctx.types.resolve_atom(atom), param.type_id);
                }
            }
        }

        let mut predicate_type = self.check(pred_data.type_node);

        // When the predicate type was parsed from `?T` (prefix ?), the parser recovers
        // just `T` but tsc semantically treats it as `T | null | undefined`. Detect this
        // by checking if the type node's position matches a nullable-type parse error.
        // Only `?`-related errors (TS17019/TS17020) trigger widening; `!`-related errors
        // should not widen since the recovered type is already correct.
        if let Some(type_node) = self.ctx.arena.get(pred_data.type_node) {
            let type_pos = type_node.pos;
            if self
                .ctx
                .nullable_type_parse_error_positions
                .contains(&type_pos)
            {
                // Widen predicate type to T | null | undefined to match tsc behavior
                predicate_type = self.ctx.types.factory().union(vec![
                    predicate_type,
                    TypeId::NULL,
                    TypeId::UNDEFINED,
                ]);
            }
        }

        let mut param_type = None;

        if let Some(function_node) = self.ctx.arena.get(function_type_idx)
            && let Some(function_data) = self.ctx.arena.get_function_type(function_node)
        {
            for &param_idx in &function_data.parameters.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param_data) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                if self.ctx.arena.get_identifier_text(param_data.name) == Some(predicate_name) {
                    param_type = (param_data.type_annotation.is_some())
                        .then(|| self.check(param_data.type_annotation));
                    break;
                }
            }
        }

        if let Some(shape) = &predicate_param_shape {
            for param in &shape.params {
                if let Some(atom) = param.name {
                    self.ctx
                        .typeof_param_scope
                        .remove(&*self.ctx.types.resolve_atom_ref(atom));
                }
            }
        }

        let (predicate_type, param_type) = if let Some(param_type) = param_type {
            (predicate_type, param_type)
        } else {
            let Some(shape) = &predicate_param_shape else {
                return;
            };
            let Some(ref predicate) = shape.type_predicate else {
                return;
            };
            let Some(predicate_type) = predicate.type_id else {
                return;
            };
            let Some(param_index) = predicate.parameter_index else {
                return;
            };
            let Some(param) = shape.params.get(param_index) else {
                return;
            };
            (predicate_type, param.type_id)
        };
        // Resolve type aliases on both sides before any further analysis. A
        // function-type-node predicate whose asserted or parameter type goes
        // through a type alias (`A = string`, or a generic-alias `Application`
        // like `Alias<T> = keyof T` / `To<T> = T`) must be compared on the
        // resolved bodies — the relation query below runs with a non-resolving
        // resolver, and resolving here (rather than after the type-parameter
        // normalization) keeps both sides symmetric so an alias that resolves to
        // a type parameter is normalized the same way the bare parameter is
        // (#14231).
        let predicate_type = self.resolve_predicate_alias(predicate_type);
        let param_type = self.resolve_predicate_alias(param_type);
        // Skip the check when the predicate type is an unevaluable Application
        // (e.g., NonNullable<T> where T is a free type parameter). Our evaluator
        // can't resolve all lib.d.ts type aliases yet, so the Application stays
        // opaque and fails the assignability check even when it's structurally sound
        // (e.g., NonNullable<T> = T & {} which is always assignable to T).
        // TSC resolves these and succeeds; we defer to avoid false TS2677 errors.
        if self.predicate_type_contains_unevaluable_application(predicate_type) {
            return;
        }
        // TSC checks: checkTypeAssignableTo(predicateType, paramType).
        // For type parameters with an explicit constraint (`T extends X`), the
        // constraint is by definition assignable to the param type when the param
        // type IS that constraint. Skip the check for constrained type parameters
        // to avoid false positives from TypeId dedup issues with recursive types.
        // For unconstrained type parameters, use `unknown` as the implicit constraint.
        let resolved_predicate = if crate::query_boundaries::common::is_type_parameter_like(
            self.ctx.types,
            predicate_type,
        ) {
            match crate::query_boundaries::common::type_param_info(self.ctx.types, predicate_type)
                .and_then(|info| info.constraint)
            {
                Some(_) => return, // Constrained type param: always assignable to its constraint
                None => TypeId::UNKNOWN,
            }
        } else {
            predicate_type
        };
        let resolved_param = if crate::query_boundaries::common::is_type_parameter_like(
            self.ctx.types,
            param_type,
        ) {
            match crate::query_boundaries::common::type_param_info(self.ctx.types, param_type)
                .and_then(|info| info.constraint)
            {
                Some(c) => c,
                None => TypeId::UNKNOWN,
            }
        } else {
            param_type
        };

        // Resolve type aliases (`Lazy(DefId)` heads and generic-alias
        // `Application`s like `Alias<T> = keyof T`) to their bodies before the
        // relation. The predicate-relation query runs with a non-resolving
        // resolver, so an alias-typed asserted or parameter type would otherwise
        // stay opaque and spuriously fail TS2677 (#14231). tsc compares the
        // resolved forms.
        let types = self.ctx.types;
        if !crate::query_boundaries::type_predicates::type_predicate_type_assignability_outcome(
            types,
            resolved_predicate,
            resolved_param,
        )
        .related
            && let Some(type_node) = self.ctx.arena.get(pred_data.type_node)
        {
            self.ctx.error(
                type_node.pos,
                type_node.end - type_node.pos,
                "A type predicate's type must be assignable to its parameter's type.".to_string(),
                2677,
            );
        }
    }

    /// Resolve a type alias (`Lazy(DefId)` head or generic-alias `Application`
    /// like `Alias<T> = keyof T`) to its body before the type-predicate relation.
    /// The relation query runs with a non-resolving resolver, so an alias-typed
    /// asserted/parameter type would otherwise stay opaque and spuriously fail
    /// TS2677 (#14231). Uses the env-aware evaluator so registered `DefId`s
    /// resolve; intrinsics/errors are returned as-is.
    fn resolve_predicate_alias(&mut self, type_id: TypeId) -> TypeId {
        if type_id.is_intrinsic() || type_id == TypeId::ERROR {
            return type_id;
        }
        crate::query_boundaries::state::type_environment::evaluate_type_with_cache(
            self.ctx.types,
            &*self.ctx,
            type_id,
            std::iter::empty(),
            false,
            crate::query_boundaries::state::type_environment::EvaluateTypeWithCacheOptions {
                expand_application_display_alias_args: false,
                query_db: Some(self.ctx.types),
                authoritative: true,
                cache_entry_collection:
                    crate::query_boundaries::state::type_environment::CacheEntryCollection::Skip,
            },
        )
        .result
    }

    /// Check if a type contains an Application that can't be evaluated (e.g., `NonNullable<T>`
    /// where the resolver doesn't know about the base type's definition). In such cases,
    /// the Application stays opaque and assignability checks may give incorrect results.
    fn predicate_type_contains_unevaluable_application(&self, type_id: TypeId) -> bool {
        if crate::query_boundaries::common::application_info(self.ctx.types, type_id).is_some() {
            // If evaluate_type returns the same TypeId, the Application couldn't be resolved
            let evaluated = self.ctx.types.evaluate_type(type_id);
            return evaluated == type_id;
        }
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
        {
            return members
                .iter()
                .any(|&m| self.predicate_type_contains_unevaluable_application(m));
        }
        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, type_id)
        {
            return members
                .iter()
                .any(|&m| self.predicate_type_contains_unevaluable_application(m));
        }
        false
    }

    fn find_type_predicate_in_type(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(node_idx)?;
        match node.kind {
            k if k == syntax_kind_ext::TYPE_PREDICATE => Some(node_idx),
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
                let wrapped = self.ctx.arena.get_wrapped_type(node)?;
                self.find_type_predicate_in_type(wrapped.type_node)
            }
            k if k == syntax_kind_ext::INTERSECTION_TYPE => {
                let composite = self.ctx.arena.get_composite_type(node)?;
                for &member in &composite.types.nodes {
                    if let Some(found) = self.find_type_predicate_in_type(member) {
                        return Some(found);
                    }
                }
                None
            }
            _ => None,
        }
    }
}
