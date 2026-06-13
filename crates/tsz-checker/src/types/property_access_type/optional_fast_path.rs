//! Optional-chain property access fast paths.

use crate::query_boundaries::common::{CachedPropertyType, TypeResolver};
use crate::query_boundaries::common::{OptionalPropertyChainKey, PropertyAccessResult};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

pub(super) struct OptionalPropertyChainFastPathRequest<'a> {
    pub(super) object_type: TypeId,
    pub(super) original_object_type: TypeId,
    pub(super) question_dot_token: bool,
    pub(super) skip_flow_narrowing: bool,
    pub(super) skip_result_flow_for_result: bool,
    pub(super) write_presence_only: bool,
    pub(super) optional_property_chain_cache_key: Option<&'a OptionalPropertyChainKey>,
}

impl<'a> CheckerState<'a> {
    pub(super) fn try_resolve_optional_property_chain_fast_path(
        &mut self,
        idx: NodeIndex,
        expression: NodeIndex,
        name_or_argument: NodeIndex,
        name_node: &tsz_parser::parser::node::Node,
        request: OptionalPropertyChainFastPathRequest<'_>,
    ) -> Option<TypeId> {
        let OptionalPropertyChainFastPathRequest {
            object_type,
            original_object_type,
            question_dot_token,
            skip_flow_narrowing,
            skip_result_flow_for_result,
            write_presence_only,
            optional_property_chain_cache_key,
        } = request;
        // Fast path for optional chaining on non-class receivers when the
        // property resolves successfully without diagnostics.
        //
        // This avoids the full property-access diagnostic pipeline for common
        // patterns like `opts?.timeout` / `opts?.retries` in hot call sites.
        if !question_dot_token
            || self
                .ctx
                .compiler_options
                .no_property_access_from_index_signature
            || self.is_super_expression(expression)
        {
            return None;
        }

        let ident = self.ctx.arena.get_identifier(name_node)?;
        let property_name = &ident.escaped_text;
        // Intern through the solver so the cache keys below share one atom
        // namespace with the other `property_cache` writers; the arena's
        // `AstAtom` for this identifier lives in a different namespace.
        let prop_atom = self.ctx.types.intern_string(property_name);

        // TOP-LEVEL CACHE: check the dedicated optional_chain_cache first.
        // This is keyed by (object_type_with_nullish, prop_atom) and stores
        // the FINAL result including undefined union. On cache hit, we skip
        // split_nullish, resolve_type, contains_type_params, property lookup,
        // and union2, eliminating repeated RefCell borrows and HashMap lookups.
        // Only used when flow narrowing is skipped (skip_result_flow_for_result),
        // which guarantees the result is context-independent.
        if skip_result_flow_for_result
            && let Some(&cached) = self
                .ctx
                .flow_shared
                .narrowing_cache
                .optional_chain_cache
                .borrow()
                .get(&(object_type, prop_atom))
        {
            return Some(cached);
        }

        let (non_nullish_base, base_nullish) = self.split_nullish_type(object_type);
        let Some(non_nullish_base) = non_nullish_base else {
            self.error_property_not_exist_at(property_name, TypeId::NEVER, name_or_argument);
            return Some(TypeId::UNDEFINED);
        };

        // Keep class/private/protected semantics on the full path.
        if self
            .resolve_class_for_access(expression, non_nullish_base)
            .is_some()
        {
            return None;
        }

        // Lazy single-member fast path: resolve only the accessed own property
        // of an eligible simple lib interface (e.g. `document.title`);
        // materialize on a miss. See `lazy_lib_member`.
        let lazy_member_fast =
            self.try_lazy_lib_member_property_access(non_nullish_base, property_name);
        let resolved_base = if lazy_member_fast.is_some() {
            non_nullish_base
        } else {
            self.resolve_property_access_base_materialized(non_nullish_base)
        };
        let resolver_generation = TypeResolver::resolver_generation(&self.ctx);
        let cache_key = |base, name| (base, resolver_generation, name);
        let effective_write_result = |type_id: TypeId, write_type: Option<TypeId>| -> TypeId {
            if skip_flow_narrowing {
                if write_presence_only {
                    TypeId::ANY
                } else {
                    write_type.unwrap_or(type_id)
                }
            } else {
                type_id
            }
        };

        let cached_property_type = self
            .ctx
            .flow_shared
            .narrowing_cache
            .property_cache
            .borrow()
            .get(&cache_key(resolved_base, prop_atom))
            .copied();
        if let Some(Some(entry)) = cached_property_type {
            let mut result_type = self.refine_expando_property_read_type(
                idx,
                expression,
                property_name,
                entry.type_id,
            );
            if base_nullish.is_some() {
                result_type = crate::query_boundaries::optional_chain::add_undefined_if_missing(
                    self.ctx.types,
                    result_type,
                );
            }
            if skip_result_flow_for_result {
                self.ctx
                    .flow_shared
                    .narrowing_cache
                    .optional_chain_cache
                    .borrow_mut()
                    .insert((object_type, prop_atom), result_type);
            }
            return Some(self.finalize_property_access_result(
                idx,
                result_type,
                skip_flow_narrowing,
                skip_result_flow_for_result,
            ));
        }

        let result = if let Some(lazy_result) = lazy_member_fast {
            lazy_result
        } else {
            let fast_result = self.ctx.types.resolve_property_access_with_options(
                resolved_base,
                property_name,
                self.ctx.compiler_options.no_unchecked_indexed_access,
            );
            self.resolve_property_access_with_env_post_query(
                resolved_base,
                property_name,
                fast_result,
            )
        };
        match result {
            PropertyAccessResult::Success {
                type_id,
                write_type,
                from_index_signature,
            } => {
                let generic_mapped_missing_named_property = from_index_signature
                    && self.generic_mapped_receiver_lacks_property_access_name(
                        original_object_type,
                        property_name,
                    );
                if from_index_signature
                    && self
                        .ctx
                        .compiler_options
                        .no_property_access_from_index_signature
                    && !self.union_has_explicit_property_member(resolved_base, property_name)
                {
                    // Preserve the optional-chain fast path for regular property
                    // reads, but fall back to the full path when TS4111 must be
                    // reported.
                    return None;
                }
                if generic_mapped_missing_named_property {
                    // Generic mapped receivers like `Record<keyof T | "x", V>`
                    // can surface a broad index signature in the fast solver path
                    // even when a specific named property is not guaranteed for
                    // every instantiation. Fall through so the full path can emit
                    // TS2339/TS2551.
                    return None;
                }

                let refined_type_id =
                    self.refine_expando_property_read_type(idx, expression, property_name, type_id);
                self.ctx
                    .flow_shared
                    .narrowing_cache
                    .property_cache
                    .borrow_mut()
                    .insert(
                        cache_key(resolved_base, prop_atom),
                        Some(CachedPropertyType::new(
                            refined_type_id,
                            from_index_signature,
                        )),
                    );
                let mut result_type = effective_write_result(refined_type_id, write_type);
                if base_nullish.is_some() {
                    result_type = crate::query_boundaries::optional_chain::add_undefined_if_missing(
                        self.ctx.types,
                        result_type,
                    );
                }
                if skip_result_flow_for_result {
                    self.ctx
                        .flow_shared
                        .narrowing_cache
                        .optional_chain_cache
                        .borrow_mut()
                        .insert((object_type, prop_atom), result_type);
                }
                if let Some(key) = optional_property_chain_cache_key {
                    self.ctx
                        .flow_shared
                        .narrowing_cache
                        .optional_property_chain_cache
                        .borrow_mut()
                        .insert(key.clone(), result_type);
                }
                Some(self.finalize_property_access_result(
                    idx,
                    result_type,
                    skip_flow_narrowing,
                    skip_result_flow_for_result,
                ))
            }
            PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                self.ctx
                    .flow_shared
                    .narrowing_cache
                    .property_cache
                    .borrow_mut()
                    .insert(
                        cache_key(resolved_base, prop_atom),
                        property_type.map(CachedPropertyType::explicit),
                    );
                let mut result_type = property_type.unwrap_or(TypeId::ERROR);
                if base_nullish.is_some() {
                    result_type = crate::query_boundaries::optional_chain::add_undefined_if_missing(
                        self.ctx.types,
                        result_type,
                    );
                }
                Some(self.finalize_property_access_result(
                    idx,
                    result_type,
                    skip_flow_narrowing,
                    false,
                ))
            }
            PropertyAccessResult::PropertyNotFound { .. } => {
                self.ctx
                    .flow_shared
                    .narrowing_cache
                    .property_cache
                    .borrow_mut()
                    .insert(cache_key(resolved_base, prop_atom), None);
                None
            }
            PropertyAccessResult::IsUnknown => None,
        }
    }
}
