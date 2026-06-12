//! Type assignability and excess property checking.
//! Subtype, identity, and redeclaration compatibility live in `subtype_identity_checker`.

use crate::query_boundaries::assignability::{
    AssignabilityEvalKind, classify_for_assignability_eval, contains_free_infer_types,
    get_keyof_type, get_string_literal_value, get_union_members, is_type_parameter_like,
    keyof_object_properties, map_compound_members,
};
use crate::query_boundaries::common::collect_type_queries;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Merge overflow flags into the checker context (sticky: only ever sets to `true`).
    ///
    /// Callers that need a fresh read must reset the context fields before
    /// invoking the relation.
    #[inline]
    pub(super) fn propagate_overflow_flags(&self, depth_exceeded: bool, iteration_exceeded: bool) {
        let mut overflow = self.ctx.relation_overflow.get();
        overflow.merge(depth_exceeded, iteration_exceeded);
        self.ctx.relation_overflow.set(overflow);
    }

    pub(crate) fn callable_has_own_generic_signatures(&self, type_id: TypeId) -> bool {
        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
        {
            return !shape.type_params.is_empty();
        }
        if let Some(shape) =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
        {
            return shape
                .call_signatures
                .iter()
                .any(|sig| !sig.type_params.is_empty())
                || shape
                    .construct_signatures
                    .iter()
                    .any(|sig| !sig.type_params.is_empty());
        }
        false
    }

    /// Check if a callable type's parameters contain type parameters within intersections.
    /// This distinguishes narrowed callback parameters (e.g., `(x: number & T) => void`)
    /// from callbacks with standalone enclosing-scope type parameters (e.g., `(x: T) => void`).
    pub(crate) fn callable_params_contain_type_param_intersection(&self, type_id: TypeId) -> bool {
        let params = if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
        {
            shape.params.iter().map(|p| p.type_id).collect::<Vec<_>>()
        } else if let Some(shape) =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
        {
            shape
                .call_signatures
                .iter()
                .flat_map(|sig| sig.params.iter().map(|p| p.type_id))
                .collect::<Vec<_>>()
        } else {
            return false;
        };
        params.iter().any(|&param_type| {
            if let Some(members) =
                crate::query_boundaries::common::intersection_members(self.ctx.types, param_type)
            {
                members.iter().any(|&m| {
                    crate::query_boundaries::assignability::contains_type_parameters(
                        self.ctx.types,
                        m,
                    )
                })
            } else {
                false
            }
        })
    }

    /// Check if an argument node is a callback (arrow function or function expression)
    /// with unannotated parameters that rely on contextual typing.
    pub(crate) fn arg_is_callback_with_unannotated_params(&self, arg_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(arg_idx) else {
            return false;
        };

        let is_callback = node.kind == syntax_kind_ext::ARROW_FUNCTION
            || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION;

        if !is_callback {
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.ctx.arena.get_parenthesized(node)
            {
                return self.arg_is_callback_with_unannotated_params(paren.expression);
            }
            return false;
        }

        let Some(func) = self.ctx.arena.get_function(node) else {
            return false;
        };

        func.parameters.nodes.iter().any(|&param_idx| {
            self.ctx
                .arena
                .get(param_idx)
                .and_then(|pn| self.ctx.arena.get_parameter(pn))
                .is_some_and(|p| {
                    p.type_annotation.is_none()
                        && self.ctx.arena.get(p.name).is_some_and(|name_node| {
                            name_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN
                                && name_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN
                        })
                })
        })
    }

    /// Returns the parameter count of a callback argument's function expression.
    /// Returns `None` if `arg_idx` is not an arrow/function expression (or a
    /// parenthesized one).
    fn callback_argument_param_count(&self, arg_idx: NodeIndex) -> Option<usize> {
        let node = self.ctx.arena.get(arg_idx)?;
        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            let paren = self.ctx.arena.get_parenthesized(node)?;
            return self.callback_argument_param_count(paren.expression);
        }
        if node.kind != syntax_kind_ext::ARROW_FUNCTION
            && node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return None;
        }
        let func = self.ctx.arena.get_function(node)?;
        Some(func.parameters.nodes.len())
    }

    /// Returns true when `target` exposes at least one callable signature whose
    /// parameter list can supply contextual types for every parameter of the
    /// unannotated callback at `arg_idx`.
    ///
    /// A signature can supply contextual types when it has a rest parameter, or
    /// when its fixed parameter count is at least the source callback's
    /// parameter count. When the target is not a recognizably callable type, we
    /// conservatively answer `true` so that the existing suppression behavior
    /// for non-trivial target shapes (unions, generics, etc.) is preserved —
    /// the bug we are guarding against is the concrete case where the target
    /// has *fewer* parameters than the source.
    pub(crate) fn target_can_contextually_type_callback_params(
        &self,
        arg_idx: NodeIndex,
        target: TypeId,
    ) -> bool {
        let Some(source_param_count) = self.callback_argument_param_count(arg_idx) else {
            return true;
        };
        let db = self.ctx.types;
        if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(db, target) {
            return signature_has_param_capacity(&shape.params, source_param_count);
        }
        if let Some(shape) = crate::query_boundaries::common::callable_shape_for_type(db, target) {
            let any_call_ok = shape
                .call_signatures
                .iter()
                .any(|sig| signature_has_param_capacity(&sig.params, source_param_count));
            let any_construct_ok = shape
                .construct_signatures
                .iter()
                .any(|sig| signature_has_param_capacity(&sig.params, source_param_count));
            return any_call_ok || any_construct_ok;
        }
        true
    }

    /// Returns true when a callback-like function type still has unresolved
    /// `any`/`unknown` parameter types, meaning contextual typing did not
    /// concretely bind its parameters yet.
    pub(crate) fn callback_type_params_are_unresolved(&self, arg_type: TypeId) -> bool {
        if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(
            self.ctx.types.as_type_database(),
            arg_type,
        ) {
            shape.params.is_empty()
                || shape
                    .params
                    .iter()
                    .all(|p| matches!(p.type_id, TypeId::ANY | TypeId::UNKNOWN))
        } else {
            false
        }
    }

    fn normalize_nested_type_for_assignability(&mut self, type_id: TypeId) -> TypeId {
        // Depth guard: prevents stack overflow from mutually recursive types
        // (e.g., Foo<T> ↔ Bar<T>) where each fresh visited set misses cross-function cycles.
        thread_local! { static DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) }; }
        let depth = DEPTH.with(|d| {
            let v = d.get();
            d.set(v + 1);
            v
        });
        if depth >= 10 {
            DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
            return type_id;
        }
        let mut visited = FxHashSet::default();
        let result = self.normalize_nested_type_for_assignability_inner(type_id, &mut visited);
        DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
        result
    }

    fn normalize_nested_type_for_assignability_inner(
        &mut self,
        type_id: TypeId,
        visited: &mut FxHashSet<TypeId>,
    ) -> TypeId {
        if !visited.insert(type_id) {
            return type_id;
        }

        let resolved = self.resolve_type_query_type(type_id);
        let evaluated = if crate::query_boundaries::common::type_application(
            self.ctx.types,
            resolved,
        )
        .is_some()
        {
            self.evaluate_type_for_assignability(resolved)
        } else {
            self.evaluate_type_with_env(resolved)
        };
        let type_id = if evaluated == TypeId::UNKNOWN && resolved != TypeId::UNKNOWN {
            resolved
        } else if evaluated != resolved {
            evaluated
        } else {
            resolved
        };

        if let Some(inner) =
            crate::query_boundaries::common::get_readonly_inner(self.ctx.types, type_id)
        {
            let normalized = self.normalize_nested_type_for_assignability_inner(inner, visited);
            if normalized != inner {
                self.ctx.types.readonly_type(normalized)
            } else {
                type_id
            }
        } else if let Some(inner) =
            crate::query_boundaries::common::get_noinfer_inner(self.ctx.types, type_id)
        {
            let normalized = self.normalize_nested_type_for_assignability_inner(inner, visited);
            if normalized != inner {
                self.ctx.types.no_infer(normalized)
            } else {
                type_id
            }
        } else if let Some(elem) =
            crate::query_boundaries::common::array_element_type(self.ctx.types, type_id)
        {
            if crate::query_boundaries::common::is_array_type(self.ctx.types, type_id) {
                let normalized = self.normalize_nested_type_for_assignability_inner(elem, visited);
                if normalized != elem {
                    self.ctx.types.array(normalized)
                } else {
                    type_id
                }
            } else {
                type_id
            }
        } else if let Some(elements) =
            crate::query_boundaries::common::tuple_elements(self.ctx.types, type_id)
        {
            if crate::query_boundaries::common::is_tuple_type(self.ctx.types, type_id) {
                let mut changed = false;
                let normalized_elements: Vec<_> = elements
                    .iter()
                    .map(|elem| {
                        let normalized = self
                            .normalize_nested_type_for_assignability_inner(elem.type_id, visited);
                        if normalized != elem.type_id {
                            changed = true;
                        }
                        tsz_solver::TupleElement {
                            type_id: normalized,
                            name: elem.name,
                            optional: elem.optional,
                            rest: elem.rest,
                        }
                    })
                    .collect();
                if changed {
                    self.ctx.types.factory().tuple(normalized_elements)
                } else {
                    type_id
                }
            } else {
                type_id
            }
        } else if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, type_id)
        {
            let mut changed = false;
            let normalized_members: Vec<_> = members
                .iter()
                .map(|&member| {
                    let normalized =
                        self.normalize_nested_type_for_assignability_inner(member, visited);
                    if normalized != member {
                        changed = true;
                    }
                    normalized
                })
                .collect();
            if changed {
                self.ctx.types.factory().union(normalized_members)
            } else {
                type_id
            }
        } else if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
        {
            let mut changed = false;
            let normalized_members: Vec<_> = members
                .iter()
                .map(|&member| {
                    let normalized =
                        self.normalize_nested_type_for_assignability_inner(member, visited);
                    if normalized != member {
                        changed = true;
                    }
                    normalized
                })
                .collect();
            if changed {
                self.ctx.types.factory().intersection(normalized_members)
            } else {
                type_id
            }
        } else {
            type_id
        }
    }

    fn normalize_function_shape_for_assignability(
        &mut self,
        shape: &tsz_solver::FunctionShape,
    ) -> Option<tsz_solver::FunctionShape> {
        let own_tp_names: Vec<_> = shape.type_params.iter().map(|tp| tp.name).collect();

        let mut changed = false;
        let params = shape
            .params
            .iter()
            .map(|param| {
                let skip = !own_tp_names.is_empty()
                    && own_tp_names.iter().any(|&name| {
                        crate::query_boundaries::common::contains_type_parameter_named(
                            self.ctx.types,
                            param.type_id,
                            name,
                        )
                    });
                let evaluated = if skip {
                    param.type_id
                } else {
                    self.normalize_nested_type_for_assignability(param.type_id)
                };
                if evaluated != param.type_id {
                    changed = true;
                }
                tsz_solver::ParamInfo {
                    name: param.name,
                    type_id: evaluated,
                    optional: param.optional,
                    rest: param.rest,
                }
            })
            .collect();
        let this_type = shape.this_type.map(|this_type| {
            let evaluated = self.normalize_nested_type_for_assignability(this_type);
            if evaluated != this_type {
                changed = true;
            }
            evaluated
        });
        let return_type = {
            let skip_for_type_params = !own_tp_names.is_empty()
                && own_tp_names.iter().any(|&name| {
                    crate::query_boundaries::common::contains_type_parameter_named(
                        self.ctx.types,
                        shape.return_type,
                        name,
                    )
                });
            let skip_for_type_query = crate::query_boundaries::common::is_type_query_type(
                self.ctx.types,
                shape.return_type,
            );
            let skip_for_conditional = crate::query_boundaries::common::is_conditional_type(
                self.ctx.types,
                shape.return_type,
            );
            let skip = skip_for_type_params || skip_for_type_query || skip_for_conditional;
            let evaluated = if skip {
                shape.return_type
            } else {
                self.normalize_nested_type_for_assignability(shape.return_type)
            };
            if evaluated != shape.return_type {
                changed = true;
            }
            evaluated
        };
        let type_predicate = shape.type_predicate.as_ref().map(|predicate| {
            let type_id = predicate.type_id.map(|type_id| {
                let evaluated = self.normalize_nested_type_for_assignability(type_id);
                if evaluated != type_id {
                    changed = true;
                }
                evaluated
            });
            tsz_solver::TypePredicate {
                asserts: predicate.asserts,
                target: predicate.target,
                type_id,
                parameter_index: predicate.parameter_index,
            }
        });

        changed.then_some(tsz_solver::FunctionShape {
            type_params: shape.type_params.clone(),
            params,
            this_type,
            return_type,
            type_predicate,
            is_constructor: shape.is_constructor,
            is_method: shape.is_method,
        })
    }

    fn normalize_callable_type_for_assignability(&mut self, type_id: TypeId) -> TypeId {
        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
        {
            let result = self
                .normalize_function_shape_for_assignability(&shape)
                .map(|shape| self.ctx.types.factory().function(shape))
                .unwrap_or(type_id);
            return result;
        }
        if let Some(shape) =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
        {
            let mut changed = false;
            let call_signatures: Vec<_> = shape
                .call_signatures
                .iter()
                .map(|sig| {
                    let normalized = self.normalize_function_shape_for_assignability(
                        &tsz_solver::FunctionShape {
                            type_params: sig.type_params.clone(),
                            params: sig.params.clone(),
                            this_type: sig.this_type,
                            return_type: sig.return_type,
                            type_predicate: sig.type_predicate,
                            is_constructor: false,
                            is_method: false,
                        },
                    );
                    if normalized.is_some() {
                        changed = true;
                    }
                    normalized.map_or_else(
                        || sig.clone(),
                        |shape| tsz_solver::CallSignature {
                            type_params: shape.type_params,
                            params: shape.params,
                            this_type: shape.this_type,
                            return_type: shape.return_type,
                            type_predicate: shape.type_predicate,
                            is_method: sig.is_method,
                        },
                    )
                })
                .collect();
            let construct_signatures: Vec<_> = shape
                .construct_signatures
                .iter()
                .map(|sig| {
                    let normalized = self.normalize_function_shape_for_assignability(
                        &tsz_solver::FunctionShape {
                            type_params: sig.type_params.clone(),
                            params: sig.params.clone(),
                            this_type: sig.this_type,
                            return_type: sig.return_type,
                            type_predicate: sig.type_predicate,
                            is_constructor: true,
                            is_method: false,
                        },
                    );
                    if normalized.is_some() {
                        changed = true;
                    }
                    normalized.map_or_else(
                        || sig.clone(),
                        |shape| tsz_solver::CallSignature {
                            type_params: shape.type_params,
                            params: shape.params,
                            this_type: shape.this_type,
                            return_type: shape.return_type,
                            type_predicate: shape.type_predicate,
                            is_method: sig.is_method,
                        },
                    )
                })
                .collect();

            if changed {
                self.ctx
                    .types
                    .factory()
                    .callable(tsz_solver::CallableShape {
                        call_signatures,
                        construct_signatures,
                        properties: shape.properties.clone(),
                        string_index: shape.string_index,
                        number_index: shape.number_index,
                        symbol: shape.symbol,
                        is_abstract: shape.is_abstract,
                    })
            } else {
                type_id
            }
        } else {
            type_id
        }
    }

    pub(crate) fn get_keyof_type_keys(
        &mut self,
        type_id: TypeId,
        db: &dyn tsz_solver::construction::TypeDatabase,
    ) -> FxHashSet<Atom> {
        if let Some(keyof_type) = get_keyof_type(db, type_id)
            && let Some(key_type) = keyof_object_properties(db, keyof_type)
            && let Some(members) = get_union_members(db, key_type)
        {
            return members
                .into_iter()
                .filter_map(|m| {
                    if let Some(str_lit) = get_string_literal_value(db, m) {
                        return Some(str_lit);
                    }
                    None
                })
                .collect();
        }
        FxHashSet::default()
    }

    /// Ensure relation preconditions (lazy refs + application symbols) for one type.
    pub(crate) fn ensure_relation_input_ready(&mut self, type_id: TypeId) {
        if type_id.is_intrinsic() {
            return;
        }
        // Do NOT gate on global_resolution_fuel_exhausted() here.  The inner
        // guards inside ensure_refs_resolved (and ensure_application_symbols_resolved)
        // already exit the materialization worklist when global fuel is exhausted,
        // bounding total work per call to O(1).  Gating the entire readiness step
        // here caused subsequent DOM/lib relation checks in the same file to skip
        // their input step entirely, silently dropping TS2322/TS2345 diagnostics
        // after the first large-lib type graph was materialized (issue #12144).
        self.ensure_refs_resolved(type_id);
        self.ensure_application_symbols_resolved(type_id);
    }

    /// Ensure relation preconditions (lazy refs + application symbols) for multiple types.
    pub(crate) fn ensure_relation_inputs_ready(&mut self, type_ids: &[TypeId]) {
        for &type_id in type_ids {
            self.ensure_relation_input_ready(type_id);
        }
    }

    /// Prepare both relation inputs, skipping expensive lazy-ref resolution
    /// when a cached result is already available.
    ///
    /// Gates on `is_relation_cacheable` first: only cacheable type pairs
    /// benefit from a cache lookup short-circuit. Non-cacheable pairs (those
    /// containing infer placeholders or `this` types) always go through the
    /// full `ensure_relation_input_ready` path.
    pub(crate) fn ensure_relation_pair_ready(&mut self, source: TypeId, target: TypeId) {
        if crate::query_boundaries::assignability::is_relation_cacheable(
            self.ctx.types,
            source,
            target,
        ) {
            let flags = self.ctx.pack_relation_flags();
            let cache_key = crate::query_boundaries::assignability::assignability_cache_key(
                source, target, flags,
            );
            if self
                .ctx
                .types
                .lookup_assignability_cache(cache_key)
                .is_some()
            {
                return;
            }
        }
        self.ensure_relation_input_ready(source);
        self.ensure_relation_input_ready(target);
    }

    /// Centralized suppression for TS2322-style assignability diagnostics.
    pub(crate) fn should_suppress_assignability_diagnostic(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let is_type_alias_application = |type_id: TypeId| {
            crate::query_boundaries::common::type_application(self.ctx.types, type_id)
                .and_then(|app| {
                    crate::query_boundaries::common::lazy_def_id(self.ctx.types, app.base)
                })
                .and_then(|def_id| self.ctx.definition_store.get(def_id))
                .is_some_and(|def| def.kind == tsz_solver::def::DefKind::TypeAlias)
        };
        if is_type_alias_application(source)
            && is_type_alias_application(target)
            && crate::query_boundaries::assignability::are_types_structurally_identical(
                self.ctx.types,
                &self.ctx,
                source,
                target,
            )
        {
            return true;
        }
        if self.recursive_conditional_path_alias_mismatch_is_tsc_bailout(source, target) {
            return true;
        }

        if crate::query_boundaries::common::keyof_inner_type(self.ctx.types, target).is_some() {
            let resolved_keyof =
                crate::query_boundaries::state::type_environment::evaluate_type_with_resolver(
                    self.ctx.types,
                    &self.ctx,
                    target,
                );
            if resolved_keyof != target
                && self
                    .keyof_diagnostic_suppression_relation_outcome(source, resolved_keyof)
                    .related
            {
                return true;
            }
            if self.keyof_interface_augmentation_literals_cover_source(source, target) {
                return true;
            }
        }

        let evaluated_target_for_invalid_mapped = self.ctx.types.evaluate_type(target);
        if self.type_contains_invalid_mapped_key_type(target)
            || self.type_contains_invalid_mapped_key_type(evaluated_target_for_invalid_mapped)
        {
            return true;
        }

        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, target)
        {
            let has_indexed_access = members.iter().any(|&member| {
                crate::query_boundaries::common::is_index_access_type(self.ctx.types, member)
            });
            if has_indexed_access {
                let indexed_access_has_errors = members.iter().any(|&member| {
                    if crate::query_boundaries::common::is_index_access_type(self.ctx.types, member)
                    {
                        Self::type_contains_error_application(self.ctx.types, member)
                    } else {
                        false
                    }
                });
                let union_has_errors =
                    Self::type_contains_error_application(self.ctx.types, target);
                if !indexed_access_has_errors && !union_has_errors {
                    return false;
                }
            }
        }

        // Check if a type contains an error application (e.g., error<any>)
        // This happens when type resolution fails for qualified names like React.ReactElement
        // in function return type positions. Suppress the false positive TS2322.
        let contains_error_application =
            |type_id: TypeId| Self::type_contains_error_application(self.ctx.types, type_id);
        let evaluated_target_for_infer_suppression = self.ctx.types.evaluate_type(target);
        let target_is_conditional_for_infer_suppression =
            crate::query_boundaries::common::is_conditional_type(self.ctx.types, target)
                || crate::query_boundaries::common::is_conditional_type(
                    self.ctx.types,
                    evaluated_target_for_infer_suppression,
                );

        let callable_pair_has_opaque_return_mismatch =
            if crate::query_boundaries::assignability::callable_pair_contains_type_parameters(
                self.ctx.types,
                source,
                target,
            ) {
                let callable_return_type = |type_id: TypeId| -> Option<TypeId> {
                    if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(
                        self.ctx.types,
                        type_id,
                    ) {
                        return Some(shape.return_type);
                    }
                    if let Some(shape) = crate::query_boundaries::common::callable_shape_for_type(
                        self.ctx.types,
                        type_id,
                    ) {
                        return shape.call_signatures.last().map(|sig| sig.return_type);
                    }
                    if let Some(app) =
                        crate::query_boundaries::common::type_application(self.ctx.types, type_id)
                    {
                        if let Some(shape) =
                            crate::query_boundaries::common::function_shape_for_type(
                                self.ctx.types,
                                app.base,
                            )
                        {
                            return Some(shape.return_type);
                        }
                        if let Some(shape) =
                            crate::query_boundaries::common::callable_shape_for_type(
                                self.ctx.types,
                                app.base,
                            )
                        {
                            return shape.call_signatures.last().map(|sig| sig.return_type);
                        }
                    }
                    None
                };
                match (callable_return_type(source), callable_return_type(target)) {
                    (Some(source_return), Some(target_return)) => {
                        !self
                            .no_erase_generics_relation_outcome(source_return, target_return)
                            .related
                    }
                    _ => false,
                }
            } else {
                false
            };

        // Suppress TS2322 for callable types with generic type parameters from outer
        // context. Skip the suppression when both sides have their own signature-level
        // type params — the solver handles generic-to-generic comparison correctly.
        let is_callable_or_function = |type_id: TypeId| {
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
                .is_some()
                || crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
                    .is_some()
                || crate::query_boundaries::common::type_application(self.ctx.types, type_id)
                    .is_some_and(|app| {
                        crate::query_boundaries::common::callable_shape_for_type(
                            self.ctx.types,
                            app.base,
                        )
                        .is_some()
                            || crate::query_boundaries::common::function_shape_for_type(
                                self.ctx.types,
                                app.base,
                            )
                            .is_some()
                    })
        };

        let is_constructor_like = |type_id: TypeId| -> bool {
            if crate::query_boundaries::common::has_construct_signatures(self.ctx.types, type_id) {
                return true;
            }
            if let Some(shape) =
                crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
                && shape.is_constructor
            {
                return true;
            }
            if let Some(app) =
                crate::query_boundaries::common::type_application(self.ctx.types, type_id)
            {
                if crate::query_boundaries::common::has_construct_signatures(
                    self.ctx.types,
                    app.base,
                ) {
                    return true;
                }
                if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(
                    self.ctx.types,
                    app.base,
                ) && shape.is_constructor
                {
                    return true;
                }
            }
            false
        };

        let has_own_signature_type_params = |type_id: TypeId| -> bool {
            if let Some(shape) =
                crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
            {
                return shape
                    .call_signatures
                    .iter()
                    .chain(shape.construct_signatures.iter())
                    .any(|sig| !sig.type_params.is_empty());
            }
            if let Some(shape) =
                crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
            {
                return !shape.type_params.is_empty();
            }
            false
        };

        let contains_type_parameters = |type_id: TypeId| {
            crate::query_boundaries::common::contains_type_parameters(self.ctx.types, type_id)
        };

        let is_structural_target_that_must_not_be_suppressed = |type_id: TypeId| {
            let has_structural_mismatch_shape = |candidate: TypeId| {
                crate::query_boundaries::assignability::has_deferred_conditional_member(
                    self.ctx.types,
                    candidate,
                ) || crate::query_boundaries::common::is_conditional_type(self.ctx.types, candidate)
                    || crate::query_boundaries::common::is_string_intrinsic_type(
                        self.ctx.types,
                        candidate,
                    )
                    || crate::query_boundaries::common::is_mapped_type(self.ctx.types, candidate)
                    || crate::query_boundaries::common::intersection_members(
                        self.ctx.types,
                        candidate,
                    )
                    .is_some()
            };

            let evaluated = self.ctx.types.evaluate_type(type_id);
            let application_evaluated =
                if crate::query_boundaries::state::type_environment::application_info(
                    self.ctx.types,
                    type_id,
                )
                .is_some()
                {
                    crate::query_boundaries::state::type_environment::evaluate_type_with_resolver(
                        self.ctx.types,
                        &self.ctx,
                        type_id,
                    )
                } else {
                    type_id
                };
            has_structural_mismatch_shape(type_id)
                || (evaluated != type_id && has_structural_mismatch_shape(evaluated))
                || (application_evaluated != type_id
                    && has_structural_mismatch_shape(application_evaluated))
        };

        // Suppress TS2322 for types that contain recursive constraints or error conditions
        // that would lead to false positive diagnostics. These include:
        // - Types with type parameters that might cause recursive constraint issues
        let should_suppress_for_complex_type = |type_id: TypeId| -> bool {
            if crate::query_boundaries::common::is_type_parameter(self.ctx.types, type_id)
                || is_callable_or_function(type_id)
                || is_structural_target_that_must_not_be_suppressed(type_id)
            {
                return false;
            }
            // Also check for union types containing indexed access types.
            // For example, `(S & State<T>)["a"] | undefined` is a union where
            // one member is an indexed access type. We should not suppress TS2322
            // for these cases because the indexed access may resolve to a type
            // that is not assignable from the source.
            //
            // However, if the indexed access types contain error applications
            // (e.g., when type resolution fails), we should still allow suppression
            // to avoid false positives on unresolved types.
            if let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, type_id)
            {
                if members.iter().any(|&member| {
                    crate::query_boundaries::common::is_type_parameter(self.ctx.types, member)
                }) {
                    return false;
                }

                let has_indexed_access = members.iter().any(|&member| {
                    crate::query_boundaries::common::is_index_access_type(self.ctx.types, member)
                });
                if has_indexed_access {
                    // Check if any indexed access type contains error applications
                    let indexed_access_has_errors = members.iter().any(|&member| {
                        if crate::query_boundaries::common::is_index_access_type(
                            self.ctx.types,
                            member,
                        ) {
                            Self::type_contains_error_application(self.ctx.types, member)
                        } else {
                            false
                        }
                    });
                    // Also check if the union itself contains error applications
                    let union_has_errors =
                        Self::type_contains_error_application(self.ctx.types, type_id);
                    // Only prevent suppression if there are indexed access types AND no errors
                    if !indexed_access_has_errors && !union_has_errors {
                        return false; // Don't suppress for unions containing indexed access types without errors
                    }
                }
            }
            // Keep the generic false-positive suppression for genuinely complex
            // generic shapes, but do not suppress plain `T`/`U` relations.
            // tsc reports TS2322 for distinct type parameters even when they
            // share the same constraint.
            crate::query_boundaries::assignability::has_recursive_type_parameter_constraint(
                self.ctx.types,
                type_id,
            ) || (crate::query_boundaries::common::contains_type_parameters(
                self.ctx.types,
                type_id,
            ) && !is_type_parameter_like(self.ctx.types, type_id))
        };

        // Check if both source and target are simple generic Applications with the same base.
        // In this case, don't suppress - let the variance check or structural comparison
        // handle it. This fixes cases like `Foo<T>` vs `Foo<U>` where T and U are different
        // unconstrained type parameters that should produce TS2322.
        let are_simple_generic_applications = |s: TypeId, t: TypeId| -> bool {
            if let (Some(s_app), Some(t_app)) = (
                crate::query_boundaries::common::type_application(self.ctx.types, s),
                crate::query_boundaries::common::type_application(self.ctx.types, t),
            ) {
                // Same base type, both contain type parameters
                return s_app.base == t_app.base
                    && crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        s,
                    )
                    && crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        t,
                    );
            }
            false
        };

        if are_simple_generic_applications(source, target) {
            return false; // Don't suppress - let the actual assignability check run
        }

        // Don't suppress for generic Applications with type parameters.
        // This fixes false TS2769 errors when passing generic return types
        // (e.g., IterableIterator<T> from values()) to overloads.
        let is_generic_application_with_type_params = |ty: TypeId| -> bool {
            if let Some(app) = crate::query_boundaries::common::type_application(self.ctx.types, ty)
                && app.args.iter().any(|&arg| {
                    crate::query_boundaries::common::contains_type_parameters(self.ctx.types, arg)
                })
            {
                return true;
            }
            false
        };

        // Check if target contains indexed access type - these should NOT be suppressed
        // even when source has type parameters, because indexed access may resolve
        // to incompatible types (e.g., (S & State<T>)["a"] may not accept T)
        let target_contains_indexed_access = || -> bool {
            if crate::query_boundaries::common::is_index_access_type(self.ctx.types, target) {
                return true;
            }
            // Check union members for indexed access types
            if let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, target)
            {
                return members.iter().any(|&member| {
                    crate::query_boundaries::common::is_index_access_type(self.ctx.types, member)
                });
            }
            false
        };

        // Check if target is an index signature type (e.g., { [s: string]: A })
        // These should prefer TS2741 for missing properties over TS2322 suppression
        let target_is_index_signature = || -> bool {
            if let Some(shape) =
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, target)
            {
                return shape.string_index.is_some() || shape.number_index.is_some();
            }
            false
        };

        if is_generic_application_with_type_params(source)
            || is_generic_application_with_type_params(target)
        {
            return false; // Don't suppress - let the actual assignability check run
        }

        let is_single_type_param_spread_tuple = |ty: TypeId| {
            crate::query_boundaries::common::tuple_elements(self.ctx.types, ty).is_some_and(
                |elements| {
                    elements.len() == 1
                        && elements[0].rest
                        && crate::query_boundaries::common::type_param_info(
                            self.ctx.types,
                            elements[0].type_id,
                        )
                        .is_some()
                },
            )
        };
        if is_single_type_param_spread_tuple(source) || is_single_type_param_spread_tuple(target) {
            return false;
        }

        let evaluated_source = self.ctx.types.evaluate_type(source);
        let evaluated_target = self.ctx.types.evaluate_type(target);
        if let (Some(source_elem), Some(target_elem)) = (
            crate::query_boundaries::common::array_element_type(self.ctx.types, evaluated_source),
            crate::query_boundaries::common::array_element_type(self.ctx.types, evaluated_target),
        ) && crate::query_boundaries::common::is_mapped_type(self.ctx.types, source_elem)
            && is_type_parameter_like(self.ctx.types, target_elem)
        {
            return false;
        }

        // Structural targets (mapped/intersection/conditional/string-intrinsic) require
        // property-level checking; they must not take the complex-generic suppression
        // early-exit below — the solver decides those relations directly.
        let target_is_structural = is_structural_target_that_must_not_be_suppressed(target);
        let target_is_template_literal_from_bare_type_param =
            crate::query_boundaries::common::is_template_literal_type(self.ctx.types, target)
                && crate::query_boundaries::common::is_type_parameter(self.ctx.types, source);
        let target_allows_complex_generic_suppression = !target_is_structural
            && should_suppress_for_complex_type(target)
            && contains_type_parameters(source)
            && !is_callable_or_function(target)
            && !target_contains_indexed_access()
            && !target_is_template_literal_from_bare_type_param;

        matches!(source, TypeId::ERROR)
            || matches!(target, TypeId::ERROR | TypeId::ANY)
            || contains_error_application(target)
            // any is assignable to everything except never — tsc reports TS2322 for any→never
            || (source == TypeId::ANY && target != TypeId::NEVER)
            // Inference placeholders are transient solver state. Emitting TS2322/TS2345
            // while they are still present creates contextual false positives.
            || contains_free_infer_types(self.ctx.types, self.ctx.types.evaluate_type(source))
            || (contains_free_infer_types(self.ctx.types, evaluated_target_for_infer_suppression)
                && !target_is_conditional_for_infer_suppression)
            // Suppress TS2322 for non-callable types with type parameters that may
            // cause false positives due to complex generic constraints
            // (e.g., T extends { [P in T]: number }). Callable/generic signature
            // targets have their own suppression rules below, and suppressing them
            // here hides real TS2322s like templateLiteralTypes7.
            // Also keep mainline behavior that only suppresses while the source is
            // still generic/unresolved too; once the source has reduced to a concrete
            // type, tsc surfaces the mismatch even if the target still mentions an
            // outer type parameter (for example Assign<T, U> receiving a concrete U).
            // EXCEPTION: Don't suppress when target contains indexed access types - these
            // may resolve to incompatible concrete types that should produce TS2322.
            // Don't suppress when target is a template-literal pattern and the
            // source is a bare type parameter. The pattern `${T}` is *not*
            // trivially assignable from a bare T: T's instantiation could be
            // a literal subtype ("a") that does not structurally match the
            // template's pattern. tsc emits TS2322 for these cases (see
            // templateLiteralTypes5.ts:14:11 — `const test1: \`${T3}\` = x`).
            // Restrict the carve-out to bare type-parameter sources so that
            // template-vs-template generic comparisons (e.g.
            // `\`...${Uppercase<T>}.4\`` vs `\`...${Uppercase<T>}.3\``) keep
            // their existing suppression — tsc tolerates those under generic
            // constraint relationships.
            || target_allows_complex_generic_suppression
            // Suppress TS2322 for callable types where the source contains generic type
            // parameters that may not have been fully inferred from context. When both
            // source and target contain type parameters that are COMPLETELY disjoint
            // at the signature level (e.g., () => T vs () => U from an outer `<T, U>`
            // scope), the incompatibility is real and must NOT be suppressed.
            // Skip when both sides have their own signature-level type parameters —
            // the solver handles generic-to-generic comparison correctly via alpha-renaming.
            // Also skip when only the source has type parameters and target is concrete —
            // this is a real mismatch (e.g., <T>(x: T) => T vs (x: string) => boolean).
            // Additionally skip when source has outer-context type params and target is concrete
            // (e.g., JSDoc @template types that should emit errors for concrete mismatches).
            || (!self.ctx.skip_callable_type_param_suppression.get()
                && is_callable_or_function(source)
                && is_callable_or_function(target)
                && contains_type_parameters(source)
                && !self.callable_types_have_disjoint_type_parameters(source, target)
                // A genuine return mismatch confirmed by the solver while holding
                // shared/outer type parameters opaque (no-erase-generics) must not be
                // suppressed. Keep this return-scoped: tsc still accepts generic rest
                // parameter comparisons whose return types agree, even when an opaque
                // whole-callable probe cannot relate the parameter tuples.
                && !callable_pair_has_opaque_return_mismatch
                && !(has_own_signature_type_params(source)
                    && has_own_signature_type_params(target))
                && !(has_own_signature_type_params(source)
                    && !has_own_signature_type_params(target)
                    && !contains_type_parameters(target))
                && !(!has_own_signature_type_params(source)
                    && contains_type_parameters(source)
                    && !contains_type_parameters(target))
                && !is_constructor_like(source)
                && !is_constructor_like(target)
                && !target_is_index_signature())
    }

    /// Targeted suppression for member type compatibility checks (TS2416/TS2430).
    ///
    /// Unlike `should_suppress_assignability_diagnostic`, this does NOT suppress
    /// callable types whose source contains type parameters from an outer context.
    /// For implements/extends member checking, class-level type parameters are fully
    /// declared and their constraints must be checked eagerly — suppressing them
    /// causes false negatives where incompatible member/property signatures are accepted.
    pub(crate) fn should_suppress_member_assignability(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let contains_error_application =
            |type_id: TypeId| Self::type_contains_error_application(self.ctx.types, type_id);

        matches!(source, TypeId::ERROR)
            || matches!(target, TypeId::ERROR | TypeId::ANY)
            || contains_error_application(target)
            || (source == TypeId::ANY && target != TypeId::NEVER)
            || contains_free_infer_types(self.ctx.types, self.ctx.types.evaluate_type(source))
            || contains_free_infer_types(self.ctx.types, self.ctx.types.evaluate_type(target))
    }

    /// Check if two callable types have completely disjoint outer type parameters
    /// at their immediate signature level (parameters and return type only).
    ///
    /// Returns true when both source and target function shapes directly reference
    /// type parameters in their parameter/return positions and those type parameters
    /// are entirely different. This is a conservative check that only looks at the
    /// shallow signature level to avoid false positives from type parameters buried
    /// in generic utility types.
    fn callable_types_have_disjoint_type_parameters(&self, source: TypeId, target: TypeId) -> bool {
        let get_direct_type_params = |type_id: TypeId| -> Vec<TypeId> {
            let mut params = Vec::new();
            let mut current = type_id;
            // Walk through nested function return types to find type parameters
            // at any depth (e.g., () => (item: any) => T has T in the nested return)
            for _ in 0..4 {
                if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(
                    self.ctx.types,
                    current,
                ) {
                    for p in &shape.params {
                        if crate::query_boundaries::common::is_type_parameter(
                            self.ctx.types,
                            p.type_id,
                        ) {
                            params.push(p.type_id);
                        }
                    }
                    if crate::query_boundaries::common::is_type_parameter(
                        self.ctx.types,
                        shape.return_type,
                    ) {
                        params.push(shape.return_type);
                        break;
                    }
                    // If return type is another function, recurse into it
                    current = shape.return_type;
                } else {
                    break;
                }
            }
            params
        };

        let source_params = get_direct_type_params(source);
        let target_params = get_direct_type_params(target);

        // Both must have direct type params for them to be disjoint
        if source_params.is_empty() || target_params.is_empty() {
            return false;
        }

        // Disjoint = no overlap at all
        !source_params.iter().any(|s| target_params.contains(s))
    }

    /// Check if a type contains an error application (recursively).
    fn type_contains_error_application(
        db: &dyn tsz_solver::construction::TypeDatabase,
        type_id: TypeId,
    ) -> bool {
        // Check if it's a direct error application
        if let Some(app) = crate::query_boundaries::common::type_application(db, type_id)
            && app.base == TypeId::ERROR
        {
            return true;
        }

        // Check if it's a union type containing an error application
        if let Some(members) = crate::query_boundaries::common::union_members(db, type_id) {
            for member in members {
                if Self::type_contains_error_application(db, member) {
                    return true;
                }
            }
        }

        // Check if it's an intersection type containing an error application
        if let Some(members) = crate::query_boundaries::common::intersection_members(db, type_id) {
            for member in members {
                if Self::type_contains_error_application(db, member) {
                    return true;
                }
            }
        }

        // Check if it's a function type with error return
        if let Some(fn_shape) =
            crate::query_boundaries::common::function_shape_for_type(db, type_id)
            && Self::type_contains_error_application(db, fn_shape.return_type)
        {
            return true;
        }

        // Check if it's a callable type with error return
        if let Some(callable) =
            crate::query_boundaries::common::callable_shape_for_type(db, type_id)
        {
            for sig in &callable.call_signatures {
                if Self::type_contains_error_application(db, sig.return_type) {
                    return true;
                }
            }
        }

        false
    }

    fn recursive_conditional_path_alias_mismatch_is_tsc_bailout(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some((source_base, source_args)) = self.application_info_or_display_alias(source)
        else {
            return false;
        };
        let Some((target_base, target_args)) = self.application_info_or_display_alias(target)
        else {
            return false;
        };
        if source_base != target_base
            || source_args.len() != target_args.len()
            || source_args == target_args
            || !self.ctx.types.is_conditional_alias_base(source_base)
        {
            return false;
        }
        source_args
            .iter()
            .zip(target_args.iter())
            .any(|(&source_arg, &target_arg)| {
                source_arg == target_arg
                    && crate::query_boundaries::common::string_literal_value(
                        self.ctx.types,
                        source_arg,
                    )
                    .is_some_and(|atom| {
                        let path = self.ctx.types.resolve_atom_ref(atom);
                        // Each dot is one recursive nesting level in a
                        // path-splitting conditional alias.  tsc's
                        // `getRecursionIdentity` mechanism assumes compatible
                        // (`Ternary.Maybe`) at depth ≥ 4 path segments (3 dots).
                        // Suppress only when the path is deep enough for that
                        // bailout; shallower paths reach the leaf and must
                        // produce TS2322 on a genuine mismatch.
                        path.chars().filter(|&c| c == '.').count() >= 3
                    })
            })
    }

    /// Suppress assignability diagnostics for parser-recovery artifacts.
    pub(crate) fn should_suppress_assignability_for_parse_recovery(
        &self,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
    ) -> bool {
        if !self.has_syntax_parse_errors() {
            return false;
        }

        if self.ctx.syntax_parse_error_positions.is_empty() {
            return false;
        }

        self.is_parse_recovery_anchor_node(source_idx)
            || self.is_parse_recovery_anchor_node(diag_idx)
    }

    /// Detect nodes that look like parser-recovery artifacts (empty text, near errors).
    fn is_parse_recovery_anchor_node(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        // Missing-expression placeholders used by parser recovery.
        if self
            .ctx
            .arena
            .get_identifier_text(idx)
            .is_some_and(str::is_empty)
        {
            return true;
        }

        // Also suppress diagnostics anchored very near a syntax parse error.
        const DIAG_PARSE_DISTANCE: u32 = 16;
        for &err_pos in &self.ctx.syntax_parse_error_positions {
            let before = err_pos.saturating_sub(DIAG_PARSE_DISTANCE);
            let after = err_pos.saturating_add(DIAG_PARSE_DISTANCE);
            if (node.pos >= before && node.pos <= after)
                || (node.end >= before && node.end <= after)
            {
                return true;
            }
        }

        let mut current = idx;
        let mut walk_guard = 0;
        while current.is_some() {
            walk_guard += 1;
            if walk_guard > 512 {
                break;
            }

            if let Some(current_node) = self.ctx.arena.get(current) {
                if current_node.this_node_has_error() || current_node.this_or_subtree_has_error() {
                    return true;
                }
            } else {
                break;
            }

            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        false
    }

    // =========================================================================
    // Type Evaluation for Assignability
    // =========================================================================

    /// Ensure all Lazy/Ref types in a type are resolved into the type environment.
    pub(crate) fn ensure_refs_resolved(&mut self, type_id: TypeId) {
        use crate::state_domain::type_environment::lazy::{
            enter_refs_resolution_scope, exit_refs_resolution_scope,
            global_resolution_fuel_exhausted, increment_global_resolution_fuel,
            increment_refs_resolution_fuel, refs_resolution_fuel_exhausted,
        };

        if self.ctx.refs_resolved.contains(&type_id) {
            return;
        }

        let is_outermost = enter_refs_resolution_scope();

        let mut visited_types = FxHashSet::default();
        let mut visited_def_ids = FxHashSet::default();
        let mut worklist = vec![type_id];

        while let Some(current) = worklist.pop() {
            if refs_resolution_fuel_exhausted() {
                break;
            }

            if !visited_types.insert(current) {
                continue;
            }

            for symbol_ref in collect_type_queries(self.ctx.types, current) {
                let sym_id = tsz_binder::SymbolId(symbol_ref.0);
                let _ = self.get_type_of_symbol(sym_id);
                // Populate type_env with the VALUE type (constructor for classes) so that
                // TypeEvaluator::visit_type_query can resolve via TypeEnvironment::resolve_ref.
                // Without this, resolve_ref returns None and the fallback resolve_lazy returns
                // the INSTANCE type for classes, causing false TS2345 on `typeof ClassName` args.
                if let Some(&value_type) = self.ctx.symbol_types.get(&sym_id)
                    && let Ok(mut env) = self.ctx.type_env.try_borrow_mut()
                {
                    env.insert(tsz_solver::SymbolRef(sym_id.0), value_type);
                }
            }

            for &def_id in self.ctx.collect_lazy_def_ids_cached(current).iter() {
                if refs_resolution_fuel_exhausted() {
                    break;
                }
                if !visited_def_ids.insert(def_id) {
                    continue;
                }
                increment_refs_resolution_fuel();
                increment_global_resolution_fuel();
                let at_fuel_limit = global_resolution_fuel_exhausted();
                // Always call resolve_and_insert_def_type even when global fuel is
                // exhausted: the call is typically a fast cache hit for lib types that
                // were computed during type-environment building, and the resolver needs
                // the TypeEnvironment entry to evaluate a Lazy(def_id) during
                // assignability checks.  Without this, exhausted-fuel calls silently
                // leave subsequent DOM/lib type refs unresolvable, causing the relation
                // checker to treat unresolved Lazy types as compatible (issue #12144).
                // When at the fuel limit we still resolve the direct def_id but skip
                // adding its result to the worklist so transitive work stays bounded.
                if let Some(result) = self.resolve_and_insert_def_type(def_id)
                    && result != TypeId::ERROR
                    && result != TypeId::ANY
                    && !at_fuel_limit
                {
                    worklist.push(result);
                }
                if at_fuel_limit {
                    break;
                }
            }
        }
        self.ctx.refs_resolved.insert(type_id);

        if is_outermost {
            exit_refs_resolution_scope();
        }
    }

    /// Session-state stamp for the [`crate::context::AssignabilityEvalMemo`]
    /// and the [`crate::context::AssignabilityFailureMemo`].
    ///
    /// `None` when either type environment is currently mutably borrowed; the
    /// memos are skipped entirely for such re-entrant calls.
    pub(crate) fn assignability_eval_memo_stamp(
        &self,
    ) -> Option<crate::context::AssignabilityEvalStamp> {
        let env_generation = self.ctx.type_env.try_borrow().ok()?.generation();
        let environment_generation = self.ctx.type_environment.try_borrow().ok()?.generation();
        Some((
            env_generation,
            environment_generation,
            self.ctx.symbol_types.version(),
            self.ctx.symbol_instance_types.version(),
        ))
    }

    /// Evaluate a type for assignability checking.
    ///
    /// Determines if the type needs evaluation (applications, env-dependent types)
    /// and performs the appropriate evaluation.
    ///
    /// Outermost calls are memoized per checker session: the recursive
    /// normalization below is deterministic while the type environments and
    /// symbol-type caches are unchanged, and constraint validation re-requests
    /// the same `TypeId`s heavily (~94% repeated outermost calls on the
    /// ts-toolbelt project row, issue #8356). Nested calls are never served
    /// from or written to the memo so the cycle-guard semantics (re-entered
    /// types evaluate to themselves) are preserved exactly.
    pub(crate) fn evaluate_type_for_assignability(&mut self, type_id: TypeId) -> TypeId {
        use crate::state_domain::type_environment::lazy::{
            global_resolution_fuel_exhausted, refs_resolution_fuel_exhausted,
        };

        if type_id.is_intrinsic() {
            return type_id;
        }

        // Inside a diagnostic display-budget scope, evaluation results are
        // memoized and total evaluation work is fuel-bounded (issue #13040).
        // Self-expanding application chains intern fresh types per
        // evaluation, so the cycle set below never converges on them; the
        // fuel guarantees rendering one type does bounded work. Outside a
        // scope (relation/semantic paths) both are inert.
        if let Some(cached) = crate::error_reporter::display_budget::cached_eval(type_id) {
            return cached;
        }

        thread_local! {
            static ASSIGNABILITY_EVAL_VISITING: std::cell::RefCell<FxHashSet<TypeId>> =
                Default::default();
        }

        let outermost = ASSIGNABILITY_EVAL_VISITING.with(|visiting| visiting.borrow().is_empty());
        if outermost
            && let Some(stamp) = self.assignability_eval_memo_stamp()
            && let Some(memoized) = self
                .ctx
                .type_reference_validation_caches
                .assignability_eval_memo
                .get(stamp, type_id)
        {
            return memoized;
        }

        let entered =
            ASSIGNABILITY_EVAL_VISITING.with(|visiting| visiting.borrow_mut().insert(type_id));
        if !entered {
            return type_id;
        }

        if !crate::error_reporter::display_budget::try_consume_eval_fuel() {
            ASSIGNABILITY_EVAL_VISITING.with(|visiting| visiting.borrow_mut().remove(&type_id));
            return type_id;
        }

        let result = self.evaluate_type_for_assignability_inner(type_id);
        ASSIGNABILITY_EVAL_VISITING.with(|visiting| visiting.borrow_mut().remove(&type_id));
        // Cycle-truncated returns above are never recorded — only complete
        // results are safe to replay for later calls in this scope.
        crate::error_reporter::display_budget::record_eval(type_id, result);

        // Memoize only clean completions: fuel-exhausted or depth-clamped
        // evaluations are degraded forms a fresher evaluation must improve on.
        // The stamp is recomputed on purpose: evaluation grows the type
        // environments, and the result is valid for that *post*-evaluation
        // state; the lookup-time stamp would file the entry as already stale.
        if outermost
            && result != TypeId::ERROR
            && !refs_resolution_fuel_exhausted()
            && !global_resolution_fuel_exhausted()
            && !self.ctx.depth_exceeded.get()
            && let Some(stamp) = self.assignability_eval_memo_stamp()
        {
            self.ctx
                .type_reference_validation_caches
                .assignability_eval_memo
                .insert(stamp, type_id, result);
        }
        result
    }

    pub(super) fn evaluate_type_for_assignability_inner(&mut self, type_id: TypeId) -> TypeId {
        if let Some(evaluated) = self.evaluate_lazy_alias_for_assignability(type_id) {
            return evaluated;
        }
        if let Some(distributed) = self.distribute_intersection_union_for_assignability(type_id) {
            return distributed;
        }

        let kind = classify_for_assignability_eval(self.ctx.types, type_id);
        let mut evaluated = match kind {
            AssignabilityEvalKind::Application => {
                let result = self.evaluate_type_with_resolution(type_id);
                // Guard: if evaluation degraded a valid type to ERROR (e.g., due to
                // stack overflow protection tripping during deep recursive type
                // resolution), preserve the original type. ERROR is treated as
                // assignable to/from everything by the subtype checker, which would
                // silently suppress real type errors like TS2322. Keeping the original
                // Lazy type allows the compat checker's resolver to resolve it from the
                // type environment (populated during earlier successful resolution).
                if result == TypeId::ERROR && type_id != TypeId::ERROR {
                    return type_id;
                }
                result
            }
            AssignabilityEvalKind::NeedsEnvEval => {
                // For TypeQuery (typeof), resolve the value type directly from
                // get_type_of_symbol. The TypeEnvironment's types map may contain
                // the instance type for class symbols (stored by type-position
                // resolution paths like resolve_lazy_def_for_type_env), but
                // TypeQuery needs the value-position type (constructor for classes).
                if let Some(symbol_ref) = crate::query_boundaries::common::type_query_symbol(
                    self.ctx.types.as_type_database(),
                    type_id,
                ) {
                    let sym_id = tsz_binder::SymbolId(symbol_ref.0);
                    // For merged TYPE_ALIAS + VARIABLE symbols (e.g.,
                    // `type Input = Static<typeof Input>` + `const Input = ...`),
                    // get_type_of_symbol may return the type alias's circular
                    // Lazy(DefId) instead of the value's concrete type. Since
                    // TypeQuery always refers to the value side, resolve directly
                    // from the value declaration to avoid TS2344 false positives.
                    let flags = self
                        .ctx
                        .binder
                        .get_symbol(sym_id)
                        .map(|s| s.flags)
                        .unwrap_or(0);
                    if (flags & tsz_binder::symbol_flags::TYPE_ALIAS) != 0
                        && (flags & tsz_binder::symbol_flags::VARIABLE) != 0
                    {
                        let value_decl = self
                            .ctx
                            .binder
                            .get_symbol(sym_id)
                            .map(|s| s.value_declaration)
                            .unwrap_or(tsz_parser::NodeIndex::NONE);
                        self.type_of_value_declaration_for_symbol(sym_id, value_decl)
                    } else {
                        self.get_type_of_symbol(sym_id)
                    }
                } else {
                    self.evaluate_type_with_env(type_id)
                }
            }
            AssignabilityEvalKind::Resolved => type_id,
        };

        if evaluated != type_id && evaluated != TypeId::ERROR && evaluated != TypeId::ANY {
            let further = self.evaluate_type_for_assignability(evaluated);
            if further != TypeId::ERROR && further != TypeId::ANY {
                evaluated = further;
            }
        }

        // Distribution pass: normalize compound types so mixed representations do not
        // leak into relation checks (for example, `Lazy(Class)` + resolved class object).
        if let Some(distributed) = self.distribute_intersection_union_for_assignability(evaluated) {
            evaluated = distributed;
        } else if let Some(distributed) =
            map_compound_members(self.ctx.types, evaluated, |member| {
                self.evaluate_type_for_assignability(member)
            })
        {
            evaluated = distributed;
        }

        // tsc expands homomorphic mapped type applications (e.g. `PassThrough<A|B>`)
        // before structural comparison; mirror that for tuple elements.
        if let Some(elements) =
            crate::query_boundaries::common::tuple_elements(self.ctx.types, evaluated)
        {
            let mut any_changed = false;
            let new_elements: Vec<tsz_solver::TupleElement> = elements
                .iter()
                .map(|elem| {
                    if matches!(
                        classify_for_assignability_eval(self.ctx.types, elem.type_id),
                        AssignabilityEvalKind::Resolved
                    ) {
                        return *elem;
                    }
                    let elem_eval = self.evaluate_type_for_assignability(elem.type_id);
                    if elem_eval != elem.type_id {
                        any_changed = true;
                    }
                    tsz_solver::TupleElement {
                        type_id: elem_eval,
                        ..*elem
                    }
                })
                .collect();
            if any_changed {
                evaluated = self.ctx.types.as_type_database().tuple(new_elements);
            }
        }

        if crate::query_boundaries::assignability::remapped_mapped_type_has_no_outer_type_params(
            self.ctx.types,
            evaluated,
        ) {
            let concrete = self.evaluate_concrete_remapped_mapped_type_with_resolution(evaluated);
            if concrete != evaluated {
                evaluated = concrete;
            }
        }

        evaluated = self.evaluate_awaited_application_for_assignability(evaluated);

        evaluated = self.normalize_callable_type_for_assignability(evaluated);

        evaluated
    }

    fn distribute_intersection_union_for_assignability(
        &mut self,
        type_id: TypeId,
    ) -> Option<TypeId> {
        let members =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)?;
        let mut evaluated_members = Vec::with_capacity(members.len());
        let mut union_member_index = None;

        for member in members {
            let evaluated = self.evaluate_type_for_assignability(member);
            if union_member_index.is_none() && self.object_union_has_branch_only_keys(evaluated) {
                union_member_index = Some(evaluated_members.len());
            }
            evaluated_members.push(evaluated);
        }

        let union_member_index = union_member_index?;
        let union_members = crate::query_boundaries::common::union_members(
            self.ctx.types,
            evaluated_members[union_member_index],
        )?;
        let mut distributed = Vec::with_capacity(union_members.len());
        for branch in union_members {
            let mut branch_members = evaluated_members.clone();
            branch_members[union_member_index] = branch;
            distributed.push(self.ctx.types.factory().intersection(branch_members));
        }

        Some(self.ctx.types.factory().union_preserve_members(distributed))
    }

    fn object_union_has_branch_only_keys(&self, type_id: TypeId) -> bool {
        let Some(members) = crate::query_boundaries::common::union_members(self.ctx.types, type_id)
        else {
            return false;
        };
        if members.len() < 2 {
            return false;
        }

        let mut first_keys = None;
        for member in members {
            let Some(shape_id) =
                crate::query_boundaries::common::object_shape_id(self.ctx.types, member)
            else {
                return false;
            };
            let keys: FxHashSet<_> = self
                .ctx
                .types
                .object_shape(shape_id)
                .properties
                .iter()
                .map(|prop| prop.name)
                .collect();
            match &first_keys {
                Some(first) if first != &keys => return true,
                None => first_keys = Some(keys),
                _ => {}
            }
        }
        false
    }

    pub(super) fn concrete_remapped_mapped_assignability_target(
        &mut self,
        target: TypeId,
    ) -> Option<TypeId> {
        let resolved = self.evaluate_type_with_resolution(target);
        let mapped_id = crate::query_boundaries::common::mapped_type_id(self.ctx.types, resolved)?;
        let mapped = self.ctx.types.mapped_type(mapped_id);
        mapped.name_type?;
        let concrete = self.evaluate_concrete_remapped_mapped_type_with_resolution(resolved);
        (concrete != resolved).then_some(concrete)
    }

    /// Recursively evaluate Lazy property types within an Object type so that
    /// the solver's `types_are_comparable_for_assertion` sees concrete types
    /// instead of opaque `Lazy(DefId)` references.
    ///
    /// Recurses up to `max_depth` levels into nested Object types whose
    /// properties are Lazy.  Returns the original type unchanged if it is not
    /// an object or has no Lazy property types.
    pub(crate) fn deep_evaluate_object_properties(&mut self, type_id: TypeId) -> TypeId {
        self.deep_evaluate_object_properties_inner(type_id, 0)
    }

    fn deep_evaluate_object_properties_inner(&mut self, type_id: TypeId, depth: u32) -> TypeId {
        const MAX_DEPTH: u32 = 3;
        if depth >= MAX_DEPTH {
            return type_id;
        }

        // Tuples carry their element types directly (not via Object shape),
        // so the property-shape walk below would skip them. Resolve each
        // tuple element first so downstream comparable-for-assertion checks
        // (e.g. tuple-to-tuple element-wise overlap in
        // `types_are_comparable_for_assertion`) see concrete types instead
        // of unresolved `Lazy(DefId)` class refs — those refs short-circuit
        // the solver's depth>0 Lazy heuristic to "comparable", masking real
        // mismatches like `[C, D] as [A, I]`.
        if let Some(elements) =
            crate::query_boundaries::common::tuple_elements(self.ctx.types, type_id)
        {
            let mut any_changed = false;
            let new_elements: Vec<tsz_solver::TupleElement> = elements
                .iter()
                .map(|elem| {
                    let mut eval_ty = elem.type_id;
                    if crate::query_boundaries::common::is_lazy_type(
                        self.ctx.types.as_type_database(),
                        eval_ty,
                    ) {
                        let resolved = self.evaluate_type_for_assignability(eval_ty);
                        if resolved != eval_ty {
                            any_changed = true;
                            eval_ty = resolved;
                        }
                    }
                    let deep = self.deep_evaluate_object_properties_inner(eval_ty, depth + 1);
                    if deep != eval_ty {
                        any_changed = true;
                        eval_ty = deep;
                    }
                    tsz_solver::TupleElement {
                        type_id: eval_ty,
                        ..*elem
                    }
                })
                .collect();
            if any_changed {
                return self.ctx.types.as_type_database().tuple(new_elements);
            }
            return type_id;
        }

        let db = self.ctx.types.as_type_database();
        // Use solver query API to get the shape id (handles Object and ObjectWithIndex)
        let shape_id = match crate::query_boundaries::common::object_shape_id(db, type_id) {
            Some(sid) => sid,
            None => return type_id,
        };

        let shape = db.object_shape(shape_id);
        let mut any_changed = false;
        let new_props: Vec<tsz_solver::PropertyInfo> = shape
            .properties
            .iter()
            .map(|p| {
                let mut eval_ty = p.type_id;
                // Resolve Lazy references (interface/type alias names)
                if crate::query_boundaries::common::is_lazy_type(
                    self.ctx.types.as_type_database(),
                    eval_ty,
                ) {
                    let resolved = self.evaluate_type_for_assignability(eval_ty);
                    if resolved != eval_ty {
                        any_changed = true;
                        eval_ty = resolved;
                    }
                }
                // Recurse into resolved Object types to resolve their properties too
                let deep = self.deep_evaluate_object_properties_inner(eval_ty, depth + 1);
                if deep != eval_ty {
                    any_changed = true;
                    eval_ty = deep;
                }

                let mut eval_write = p.write_type;
                if crate::query_boundaries::common::is_lazy_type(
                    self.ctx.types.as_type_database(),
                    eval_write,
                ) {
                    let resolved = self.evaluate_type_for_assignability(eval_write);
                    if resolved != eval_write {
                        any_changed = true;
                        eval_write = resolved;
                    }
                }

                tsz_solver::PropertyInfo {
                    type_id: eval_ty,
                    write_type: eval_write,
                    ..*p
                }
            })
            .collect();

        if !any_changed {
            return type_id;
        }

        // Re-intern the object with resolved property types
        self.ctx.types.as_type_database().object(new_props)
    }

    /// Resolve a deferred Mapped type by pre-resolving its constraint's Applications.
    ///
    /// When evaluation produces a deferred Mapped type (e.g., from Omit/Pick where
    /// the constraint contains Application types like `Exclude<keyof T, K>`), the
    /// solver's `TypeEvaluator` may have failed because lib type `DefIds` weren't
    /// registered in the `TypeEnvironment`. This method resolves the constraint through
    /// the checker's evaluation path and retries the Mapped type evaluation.
    pub(crate) fn resolve_deferred_mapped_type(&mut self, type_id: TypeId) -> TypeId {
        let Some(mapped_id) = crate::query_boundaries::state::type_environment::mapped_type_id(
            self.ctx.types.as_type_database(),
            type_id,
        ) else {
            return type_id;
        };
        let mapped = self.ctx.types.mapped_type(mapped_id);
        let constraint = mapped.constraint;
        let resolved_constraint = self.evaluate_mapped_constraint_with_resolution(constraint);
        if resolved_constraint != constraint {
            self.ctx
                .cache_env_eval_result_if_absent(constraint, resolved_constraint, false);
            let retry = self.evaluate_type_with_env_uncached(type_id);
            if retry != type_id {
                return retry;
            }
        }
        type_id
    }

    // =========================================================================
    // Main Assignability Check
    // =========================================================================

    /// Substitute `ThisType` in a type with the enclosing class instance type.
    ///
    /// When inside a class body, `ThisType` represents the polymorphic `this` type
    /// (a type parameter bounded by the class). Since the `this` expression evaluates
    /// to the concrete class instance type, we must substitute `ThisType` → class
    /// instance type before assignability checks. This matches tsc's behavior where
    /// `return this`, `f(this)`, etc. succeed when the target type is `this`.
    pub(super) fn substitute_this_type_if_needed(&mut self, type_id: TypeId) -> TypeId {
        // Fast path: intrinsic types can't contain ThisType
        if type_id.is_intrinsic() {
            return type_id;
        }

        let needs_substitution =
            crate::query_boundaries::common::contains_this_type(self.ctx.types, type_id);

        if !needs_substitution {
            return type_id;
        }

        let Some(class_info) = &self.ctx.enclosing_class else {
            return type_id;
        };
        let class_idx = class_info.class_idx;

        let Some(node) = self.ctx.arena.get(class_idx) else {
            return type_id;
        };
        let Some(class_data) = self.ctx.arena.get_class(node) else {
            return type_id;
        };

        let instance_type = self.get_class_instance_type(class_idx, class_data);

        if crate::query_boundaries::common::is_this_type(self.ctx.types, type_id) {
            // Substitute bare `ThisType` with the concrete class instance type so
            // that `return this` / `f(this)` assignability succeeds by identity check.
            instance_type
        } else if crate::query_boundaries::common::index_access_types(self.ctx.types, type_id)
            .is_some()
        {
            // A direct indexed access like `this["x"]` is still anchored in the
            // current class context. Resolve it to the concrete property type for
            // assignment/call-argument checks, while leaving more complex wrappers
            // such as `Unwrap<this["x"]>` deferred below.
            let substituted = crate::query_boundaries::common::substitute_this_type(
                self.ctx.types,
                type_id,
                instance_type,
            );
            self.evaluate_type_with_env_uncached(substituted)
        } else {
            // Do NOT substitute complex types that merely contain `ThisType` in nested
            // positions (e.g. `Builder_instance` whose methods return `this`).  The
            // solver's `bind_property_receiver_this` already substitutes `this` during
            // property comparison using the object shape's receiver symbol.
            // Pre-substituting here creates a new TypeId (Builder_instance_subst) with no
            // symbol, so the subsequent `bind_property_receiver_this` call on the *target*
            // produces a Lazy/ref TypeId while the source stays as the concrete TypeId,
            // causing spurious TS2322 errors for fluent/builder patterns.
            type_id
        }
    }
}

/// A target signature can supply contextual types for `source_param_count`
/// callback parameters when it has a rest parameter (which absorbs any
/// trailing positions) or its fixed parameter list is at least that long.
fn signature_has_param_capacity(
    params: &[tsz_solver::ParamInfo],
    source_param_count: usize,
) -> bool {
    if params.iter().any(|p| p.rest) {
        return true;
    }
    params.len() >= source_param_count
}
