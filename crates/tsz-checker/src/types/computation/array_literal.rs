//! Array literal type computation.
//!
//! This module handles type computation for array literal expressions like
//! `[1, 2, 3]`, `["a", "b"]`, `[...arr]`, etc. Extracted from `helpers.rs`
//! to keep module sizes manageable.

use crate::query_boundaries::common as query_common;
use crate::query_boundaries::type_computation::core as expr_ops;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{ContextualTypeContext, TupleElement, TypeId};

impl<'a> CheckerState<'a> {
    fn promise_like_array_context_shape(&self, type_id: TypeId) -> Option<TypeId> {
        match tsz_solver::type_queries::classify_promise_type(self.ctx.types, type_id) {
            tsz_solver::type_queries::PromiseTypeKind::Application { args, .. }
                if self.type_ref_is_promise_like(type_id) =>
            {
                args.first().and_then(|&inner| {
                    tsz_solver::type_queries::get_array_applicable_type(self.ctx.types, inner)
                })
            }
            _ => None,
        }
    }

    fn empty_array_literal_prefers_never(&self, idx: NodeIndex) -> bool {
        let Some(parent_idx) = self.ctx.arena.get_extended(idx).map(|ext| ext.parent) else {
            return false;
        };
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };

        match parent_node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                self.ctx
                    .arena
                    .get_access_expr(parent_node)
                    .is_some_and(|access| access.expression == idx)
            }
            _ => false,
        }
    }

    fn union_context_for_array_literal_is_ambiguous(&mut self, contextual: TypeId) -> bool {
        let Some(members) = tsz_solver::type_queries::get_union_members(self.ctx.types, contextual)
        else {
            return false;
        };

        let mut applicable_shapes = Vec::new();
        for member in members {
            // Skip null/undefined/void — these don't contribute to array contextual
            // typing ambiguity. tsc strips these before checking (getNonNullableType).
            if member.is_nullable() {
                continue;
            }

            if let Some(applicable) =
                tsz_solver::type_queries::get_array_applicable_type(self.ctx.types, member)
            {
                if !applicable_shapes.contains(&applicable) {
                    applicable_shapes.push(applicable);
                }
                continue;
            }

            if let Some(applicable) = self.promise_like_array_context_shape(member) {
                if !applicable_shapes.contains(&applicable) {
                    applicable_shapes.push(applicable);
                }
                continue;
            }

            if member == TypeId::ANY || member == TypeId::UNKNOWN {
                return true;
            }

            if tsz_solver::type_queries::get_type_parameter_constraint(self.ctx.types, member)
                .is_some()
            {
                return true;
            }

            // Types with a string index signature (e.g. Record<string, T>, including lazy
            // types that resolve to such objects) can contextually type array elements via
            // element access. Treat their index value type as an applicable shape — making
            // the union ambiguous when combined with an array-typed member. This matches
            // tsc's behavior where
            // Record<string, (arg: string) => void> | Array<(arg: number) => void>
            // is treated as ambiguous, emitting TS7006 for implicit-any parameters.
            // NOTE: Resolve through Lazy(DefId) first so Record<string,T> becomes an
            // ObjectWithIndex shape that the IndexSignatureResolver can inspect.
            {
                use tsz_solver::IndexSignatureResolver;
                let resolved = self.resolve_lazy_type(member);
                let resolved = self.evaluate_type_with_env(resolved);
                let resolver = IndexSignatureResolver::new(self.ctx.types);
                let si = resolver.resolve_string_index(resolved);
                if let Some(value_type) = si {
                    if !applicable_shapes.contains(&value_type) {
                        applicable_shapes.push(value_type);
                    }
                    continue;
                }
            }

            // Non-array-applicable object-like types without index signatures can't
            // meaningfully contextually type array literals. Skip them.
            if tsz_solver::type_queries::is_object_like_type(self.ctx.types, member) {
                continue;
            }
        }

        applicable_shapes.len() > 1
    }

    fn union_context_for_array_literal_prefers_tuple(&self, contextual: TypeId) -> bool {
        let Some(members) = tsz_solver::type_queries::get_union_members(self.ctx.types, contextual)
        else {
            return false;
        };

        let mut saw_tuple = false;
        for member in members {
            let Some(applicable) =
                tsz_solver::type_queries::get_array_applicable_type(self.ctx.types, member)
            else {
                return false;
            };

            if !tsz_solver::type_queries::is_tuple_type(self.ctx.types, applicable) {
                return false;
            }
            saw_tuple = true;
        }

        saw_tuple
    }

    /// Get type of array literal.
    ///
    /// Computes the type of array literals like `[1, 2, 3]` or `["a", "b"]`.
    /// Handles:
    /// - Empty arrays (infer from context or use never[])
    /// - Tuple contexts (e.g., `[string, number]`)
    /// - Spread elements (`[...arr]`)
    /// - Common type inference for mixed elements
    #[allow(dead_code)]
    pub(crate) fn get_type_of_array_literal(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_array_literal_with_request(idx, &crate::context::TypingRequest::NONE)
    }

    pub(crate) fn get_type_of_array_literal_with_request(
        &mut self,
        idx: NodeIndex,
        request: &crate::context::TypingRequest,
    ) -> TypeId {
        let factory = self.ctx.types.factory();
        let contextual_type = request.contextual_type;
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(array) = self.ctx.arena.get_literal_expr(node) else {
            return TypeId::ERROR;
        };

        if array.elements.nodes.is_empty() {
            // Empty array literal: infer from context or use never[]/any[]
            // TypeScript uses "evolving array types" where [] starts as never[] and widens
            // via control flow.

            // When [] is the receiver of a property/element access (e.g., `[].map(...)`),
            // always use `never[]` regardless of any leaked contextual type. Without this,
            // nested generic calls like `[].map(() => [].map(p => ({ X: p })))` propagate
            // the outer callback's inferred return type into the inner [], causing false
            // TS2322 errors.
            if self.empty_array_literal_prefers_never(idx) {
                return factory.array(TypeId::NEVER);
            }

            if let Some(contextual) = contextual_type {
                let resolved = self.resolve_type_for_property_access(contextual);
                let resolved = self.resolve_lazy_type(resolved);
                if tsz_solver::type_queries::is_tuple_type(self.ctx.types, resolved) {
                    return factory.tuple(vec![]);
                } else if let Some(t_elem) =
                    tsz_solver::type_queries::get_array_element_type(self.ctx.types, resolved)
                {
                    // When the contextual element type is a TypeParameter, unknown, or any,
                    // it carries no useful type information for an empty array (typically
                    // happens when `[]` is an argument for a generic parameter like `T[]`).
                    // Use never[] instead, matching tsc behavior where empty arrays always
                    // start as never[] regardless of contextual type. This prevents
                    // inference from being polluted with unknown/any from the contextual
                    // type parameter's constraint.
                    if t_elem == TypeId::UNKNOWN
                        || t_elem == TypeId::ANY
                        || tsz_solver::type_queries::is_type_parameter(self.ctx.types, t_elem)
                    {
                        // Fall through to never[]/any[] below
                    } else {
                        return factory.array(t_elem);
                    }
                }
            }

            // When noImplicitAny is off, empty array literals without contextual type
            // are typed as any[] (matching tsc behavior). With noImplicitAny on, use never[]
            // which is the "evolving array type" starting point.
            if !self.ctx.no_implicit_any() && !self.empty_array_literal_prefers_never(idx) {
                return factory.array(TypeId::ANY);
            }
            return factory.array(TypeId::NEVER);
        }

        // Resolve lazy type aliases once and reuse for both tuple_context and ctx_helper
        // This ensures type aliases (e.g., type Tup = [string, number]) are expanded
        // before checking for tuple elements and providing contextual typing
        //
        // We also save the ORIGINAL (unevaluated) contextual type for iterable fallback.
        // When the contextual type is an Application like `Iterable<readonly [K, V]>`,
        // the full evaluation chain (evaluate_type_with_env + evaluate_application_type)
        // expands it to an Object type (with [Symbol.iterator]). The Object form loses
        // the type argument information needed to provide element contextual types for
        // array literals. The original Application retains the type args so the solver's
        // get_array_element_type can use the app.args[0] heuristic to extract the element
        // type (e.g., readonly [K, V] from Iterable<readonly [K, V]>).
        let original_contextual_type = contextual_type;
        let resolved_contextual_type = contextual_type.map(|ctx_type| {
            let ctx_type = self.evaluate_contextual_type(ctx_type);
            // Strip null/undefined before evaluation — matches tsc's getNonNullableType
            // in getApparentTypeOfContextualType. Without this, `Iterable<T> | null`
            // evaluates to `Object{...} | null` which triggers union ambiguity.
            let ctx_type = tsz_solver::remove_nullish(self.ctx.types, ctx_type);
            let ctx_type = self.evaluate_type_with_env(ctx_type);
            let ctx_type = self.resolve_lazy_type(ctx_type);
            self.evaluate_application_type(ctx_type)
        });

        // When the contextual type is a union like `[number] | string`, narrow it to
        // only the array/tuple constituents applicable to an array literal. This ensures
        // `[1]` with contextual type `[number] | string` is typed as `[number]` not `number[]`.
        let mut tuple_context_from_constraint = false;
        let union_array_context_is_ambiguous = resolved_contextual_type
            .is_some_and(|resolved| self.union_context_for_array_literal_is_ambiguous(resolved));
        let applicable_contextual_type = resolved_contextual_type.and_then(|resolved| {
            if union_array_context_is_ambiguous {
                return None;
            }
            let evaluated = self.evaluate_application_type(resolved);
            // Try the type directly first
            if let Some(applicable) =
                tsz_solver::type_queries::get_array_applicable_type(self.ctx.types, evaluated)
            {
                // Mark constraint-derived when the resolved type was a type parameter
                // (get_array_applicable_type handles TypeParameter internally)
                if tsz_solver::type_queries::get_type_parameter_constraint(
                    self.ctx.types,
                    evaluated,
                )
                .is_some()
                {
                    tuple_context_from_constraint = true;
                }
                return Some(applicable);
            }
            // When the contextual type is a type parameter (e.g., `T extends [string, number]`),
            // use its base constraint for tuple context detection. This matches tsc's behavior
            // where `getApparentTypeOfContextualType` resolves type parameter constraints so
            // array literals are typed as tuples instead of being widened to arrays.
            // We only use this for shape detection (tuple vs array), NOT for element contextual
            // typing — element types should be inferred independently to preserve literals.
            if let Some(constraint) =
                tsz_solver::type_queries::get_type_parameter_constraint(self.ctx.types, evaluated)
            {
                let constraint = self.resolve_lazy_type(constraint);
                let constraint = self.evaluate_application_type(constraint);
                if let Some(applicable) =
                    tsz_solver::type_queries::get_array_applicable_type(self.ctx.types, constraint)
                {
                    tuple_context_from_constraint = true;
                    return Some(applicable);
                }
            }
            None
        });

        let tuple_context = applicable_contextual_type.and_then(|applicable| {
            tsz_solver::type_queries::get_tuple_elements(self.ctx.types, applicable)
        });

        // When the contextual type is a homomorphic mapped type (e.g., { [K in keyof T]: ... }),
        // array literals should be typed as tuples to preserve per-element type info.
        // Homomorphic mapped types preserve array/tuple structure, so the input must
        // maintain individual element types for reverse mapped type inference to work.
        // Without this, array literals become Array(union) which loses element-level detail.
        let force_tuple_for_mapped = tuple_context.is_none()
            && resolved_contextual_type.is_some_and(|resolved| {
                tsz_solver::type_queries::is_homomorphic_mapped_type_context(
                    self.ctx.types,
                    resolved,
                )
            });

        let force_tuple_for_union_context = tuple_context.is_none()
            && !force_tuple_for_mapped
            && resolved_contextual_type.is_some_and(|resolved| {
                self.union_context_for_array_literal_prefers_tuple(resolved)
            });

        // When a type parameter constraint is a union containing a tuple member
        // (the `T extends readonly unknown[] | []` pattern), force tuple inference.
        // The `| []` is a deliberate hint in TypeScript to infer tuple types from
        // array literals. This pattern is used by Promise.all, Promise.allSettled,
        // and many other APIs. Without this, array literals are typed as arrays
        // instead of tuples, causing incorrect generic inference.
        let force_tuple_for_constraint_hint = tuple_context.is_none()
            && !force_tuple_for_mapped
            && tuple_context_from_constraint
            && applicable_contextual_type.is_some_and(|applicable| {
                tsz_solver::type_queries::union_contains_tuple(self.ctx.types, applicable)
            });

        // When the contextual type is an object with numeric-string properties
        // (e.g., `{ "0": (p1: number) => number }`), force tuple typing.
        // This matches tsc's `isTupleLikeType` which treats any type with a "0"
        // property as tuple-like for array literal contextual typing purposes.
        let force_tuple_for_tuple_like = tuple_context.is_none()
            && !force_tuple_for_mapped
            && !force_tuple_for_union_context
            && !force_tuple_for_constraint_hint
            && resolved_contextual_type.is_some_and(|resolved| {
                // Only force tuple for object types that have a "0" property but
                // aren't already handled by get_array_applicable_type.
                tsz_solver::type_queries::get_array_applicable_type(self.ctx.types, resolved)
                    .is_none()
                    && tsz_solver::type_queries::is_tuple_like_type(self.ctx.types, resolved)
            });

        // Use the applicable (narrowed) type for contextual typing when available,
        // falling back to the full resolved contextual type.
        // When tuple context came from a type parameter constraint, don't use it for
        // element contextual typing — only use it for tuple shape detection. Element
        // types should be inferred independently to preserve literal types during
        // generic inference (e.g., `fx<T extends [string, 'a'|'b']>(x: T)` called
        // with `['x', 'a']` should infer `["x", "a"]`, not `[string, string]`).
        let effective_contextual = if union_array_context_is_ambiguous {
            None
        } else if tuple_context_from_constraint {
            resolved_contextual_type
        } else {
            applicable_contextual_type.or(resolved_contextual_type)
        };
        let ctx_helper = match effective_contextual {
            Some(resolved) => Some(ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                resolved,
                self.ctx.compiler_options.no_implicit_any,
            )),
            None => None,
        };
        // Fallback context from the ORIGINAL (unevaluated) contextual type.
        // When the contextual type is an Application like Iterable<readonly [K, V]>,
        // the evaluation chain expands it to an Object, losing the type arg info.
        // The original Application retains its type args so get_array_element_type
        // can extract the iterable's element type (e.g., readonly [K, V]) via args[0].
        let iterable_element_ctx_helper = if applicable_contextual_type.is_none() {
            original_contextual_type.and_then(|orig| {
                // Only use the fallback if the original differs from the resolved form
                // (meaning evaluation actually changed something, e.g., Application → Object)
                if resolved_contextual_type != Some(orig) {
                    Some(ContextualTypeContext::with_expected_and_options(
                        self.ctx.types,
                        orig,
                        self.ctx.compiler_options.no_implicit_any,
                    ))
                } else {
                    None
                }
            })
        } else {
            None
        };
        let fallback_unknown_array_element_context = effective_contextual.is_some_and(|ty| {
            ty == TypeId::UNKNOWN
                || tsz_solver::type_queries::contains_type_parameters_db(self.ctx.types, ty)
        });

        // Get types of all elements, applying contextual typing when available.
        // Track (type, node_index) pairs for excess property checking on array elements.
        let mut element_types = Vec::new();
        let mut element_nodes = Vec::new();
        let mut tuple_elements = Vec::new();
        for (index, &elem_idx) in array.elements.nodes.iter().enumerate() {
            if elem_idx.is_none() {
                continue;
            }

            // Build per-element typing request instead of mutating ctx.contextual_type
            let elem_request = if union_array_context_is_ambiguous {
                // When the contextual union is ambiguous (multiple applicable element types),
                // clear the contextual type for each element so closures don't inherit
                // the array's union contextual type and inadvertently get typed parameters.
                crate::context::TypingRequest::NONE
            } else if let Some(ref helper) = ctx_helper {
                if tuple_context.is_some() {
                    let elem_count = array.elements.nodes.iter().filter(|n| n.is_some()).count();
                    match helper.get_tuple_element_type_with_count(index, elem_count) {
                        Some(ty) => request.read().contextual(ty),
                        None => crate::context::TypingRequest::NONE,
                    }
                } else {
                    let elem_ctx_type = helper
                        .get_array_element_type()
                        .filter(|&ty| ty != TypeId::NEVER)
                        .or_else(|| {
                            // Fallback: try the pre-Application-evaluation form for
                            // iterable types like Iterable<readonly [K, V]>. The fully-
                            // evaluated Object form loses type argument info, but the
                            // original Application retains it for element extraction.
                            iterable_element_ctx_helper
                                .as_ref()
                                .and_then(|h| h.get_array_element_type())
                        })
                        .or_else(|| {
                            // Fallback: when the contextual type is an object with
                            // numeric-string properties (e.g., { "0": (p1: number) => number }),
                            // look up the property by index string. This matches tsc's
                            // getContextualTypeForElementExpression which uses
                            // getIndexedAccessType(type, numericLiteral(index)).
                            let index_str = index.to_string();
                            helper.get_property_type(&index_str)
                        })
                        .or_else(|| {
                            fallback_unknown_array_element_context.then_some(TypeId::UNKNOWN)
                        });
                    match elem_ctx_type {
                        Some(ty) => request.read().contextual(ty),
                        None => crate::context::TypingRequest::NONE,
                    }
                }
            } else {
                crate::context::TypingRequest::NONE
            };

            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            let elem_is_spread = elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT;

            // Handle spread elements - expand tuple types
            if elem_is_spread && let Some(spread_data) = self.ctx.arena.get_spread(elem_node) {
                let spread_expr_type = if self.ctx.in_destructuring_target {
                    self.get_type_of_assignment_target(spread_data.expression)
                } else {
                    self.get_type_of_node_with_request(spread_data.expression, &elem_request)
                };
                let spread_expr_type = self.resolve_lazy_type(spread_expr_type);
                // Check if spread argument is iterable, emit TS2488 if not.
                // Skip this check when the array is a destructuring target
                // (e.g., `[...c] = expr`), since the spread element is an assignment
                // target, not a value being spread into a new array.
                if !self.ctx.in_destructuring_target {
                    self.check_spread_iterability(spread_expr_type, spread_data.expression);
                }

                // TS2779: Array spread target in destructuring may not be an optional chain.
                // E.g. `[...obj?.a] = []` — obj?.a is the assignment target.
                if self.ctx.in_destructuring_target
                    && self.is_optional_chain_access(spread_data.expression)
                {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.error_at_node(
                        spread_data.expression,
                        diagnostic_messages::THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_A,
                        diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_A,
                    );
                }

                // If it's a tuple type, expand its elements
                if let Some(elems) =
                    tsz_solver::type_queries::get_tuple_elements(self.ctx.types, spread_expr_type)
                {
                    if let Some(ref _expected) = tuple_context {
                        // For tuple context, add each element with spread flag
                        for elem in &elems {
                            let optional = match tuple_context.as_ref().and_then(|tc| tc.get(index))
                            {
                                Some(el) => el.optional,
                                None => false,
                            };
                            tuple_elements.push(TupleElement {
                                type_id: elem.type_id,
                                name: None,
                                optional,
                                rest: false, // Individual tuple elements are not spreads
                            });
                            // Don't increment index here - each tuple element maps to position
                        }
                    } else {
                        // For array context, add element types
                        for elem in &elems {
                            element_types.push(elem.type_id);
                        }
                    }
                    continue;
                }

                // For non-tuple spreads in array context, use element type
                // For tuple context, use the spread type itself
                let elem_type = if tuple_context.is_some() {
                    spread_expr_type
                } else {
                    self.for_of_element_type(spread_expr_type, false)
                };

                if let Some(ref _expected) = tuple_context {
                    let optional = match tuple_context.as_ref().and_then(|tc| tc.get(index)) {
                        Some(el) => el.optional,
                        None => false,
                    };
                    tuple_elements.push(TupleElement {
                        type_id: elem_type,
                        name: None,
                        optional,
                        rest: true, // Mark as spread for non-tuple spreads in tuple context
                    });
                } else {
                    element_types.push(elem_type);
                }
                continue;
            }

            // Regular (non-spread) element
            let elem_type = if self.ctx.in_destructuring_target {
                self.destructuring_target_type_from_initializer(elem_idx)
            } else {
                self.get_type_of_node_with_request(elem_idx, &elem_request)
            };

            if let Some(ref _expected) = tuple_context {
                let optional = match tuple_context.as_ref().and_then(|tc| tc.get(index)) {
                    Some(el) => el.optional,
                    None => false,
                };
                tuple_elements.push(TupleElement {
                    type_id: elem_type,
                    name: None,
                    optional,
                    rest: false,
                });
            } else if force_tuple_for_union_context {
                tuple_elements.push(TupleElement {
                    type_id: elem_type,
                    name: None,
                    optional: false,
                    rest: false,
                });
            } else {
                element_types.push(elem_type);
                element_nodes.push(elem_idx);
            }
        }

        if tuple_context.is_some() || force_tuple_for_union_context {
            return factory.tuple(tuple_elements);
        }

        // When contextual type is a homomorphic mapped type, force tuple typing.
        // This preserves per-element types for reverse mapped type inference.
        if force_tuple_for_mapped || force_tuple_for_constraint_hint || force_tuple_for_tuple_like {
            let mapped_tuple_elements: Vec<tsz_solver::TupleElement> = element_types
                .iter()
                .map(|&type_id| tsz_solver::TupleElement {
                    type_id,
                    name: None,
                    optional: false,
                    rest: false,
                })
                .collect();
            return factory.tuple(mapped_tuple_elements);
        }

        // When in a const assertion context, array literals become tuples (not arrays)
        // This allows [1, 2, 3] as const to become readonly [1, 2, 3] instead of readonly Array<number>
        if self.ctx.in_const_assertion {
            // Convert element_types to tuple_elements
            let const_tuple_elements: Vec<tsz_solver::TupleElement> = element_types
                .iter()
                .map(|&type_id| tsz_solver::TupleElement {
                    type_id,
                    name: None,
                    optional: false,
                    rest: false,
                })
                .collect();
            return factory.tuple(const_tuple_elements);
        }

        // Use contextual element type when available for better inference
        let solver_element_type = if !request.origin.is_assertion() {
            ctx_helper.as_ref().and_then(|h| h.get_array_element_type())
        } else {
            None
        };
        // Fallback: when the solver can't extract element types (e.g., union members are
        // Lazy interface types extending Array that haven't been resolved to structural form),
        // resolve each member via the checker's type environment and check number index
        // signatures. This handles cases like `type Style = StyleBase | StyleArray` where
        // `StyleArray extends Array<Style>`.
        let context_element_type = solver_element_type.or_else(|| {
            if request.origin.is_assertion() {
                return None;
            }
            let contextual = effective_contextual?;
            self.resolve_array_element_type_from_union_members(contextual)
        });
        if let Some(context_element_type) = context_element_type
            && context_element_type != TypeId::UNKNOWN
            && context_element_type != TypeId::NEVER
        {
            let context_requires_assignability_overrides = {
                let resolved = self.resolve_ref_type(context_element_type);
                query_common::has_construct_signatures(self.ctx.types, resolved)
                    || self.type_contains_abstract_class(resolved)
            };

            let element_requires_assignability_overrides: Vec<bool> = element_types
                .iter()
                .map(|&ty| {
                    let resolved = self.resolve_ref_type(ty);
                    query_common::has_construct_signatures(self.ctx.types, resolved)
                        || self.type_contains_abstract_class(resolved)
                })
                .collect();

            // Check if all elements are structurally compatible with the contextual type.
            // IMPORTANT: Use is_subtype_of (structural check) instead of is_assignable_to
            // because is_assignable_to includes excess property checking which would
            // reject fresh object literals like `{a: 1, b: 2}` against `Foo {a: number}`.
            // Excess properties should be checked separately, not block contextual typing.
            if element_types
                .iter()
                .zip(element_requires_assignability_overrides.iter())
                .all(|(&elem_type, &elem_requires_assignability_overrides)| {
                    if elem_requires_assignability_overrides
                        || context_requires_assignability_overrides
                    {
                        self.is_assignable_to(elem_type, context_element_type)
                    } else {
                        self.is_subtype_of(elem_type, context_element_type)
                    }
                })
            {
                // Check excess properties on each element before collapsing to contextual type.
                // Fresh object literal types would be lost after returning Array<ContextualType>,
                // so we must check excess properties here while the fresh types are still available.
                for (elem_type, elem_node) in element_types.iter().zip(element_nodes.iter()) {
                    self.check_object_literal_excess_properties(
                        *elem_type,
                        context_element_type,
                        *elem_node,
                    );
                }
                return factory.array(context_element_type);
            }
        }

        // TS2590: Pre-check element count before BCT. tsc's removeSubtypes counts
        // pairwise iterations on the original (un-widened) types and bails at 1,000,000.
        // tsc counts ALL pairwise comparisons regardless of type kind (including
        // identity-comparable primitives/literals).
        {
            let mut deduped = element_types.clone();
            deduped.sort_unstable_by_key(|t| t.0);
            deduped.dedup();
            let distinct_count = deduped.len() as u64;
            let pairwise = distinct_count * distinct_count.saturating_sub(1);
            if pairwise >= 1_000_000 {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    idx,
                    diagnostic_messages::EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT,
                    diagnostic_codes::EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT,
                );
                return factory.array(TypeId::ERROR);
            }
        }

        // Use Solver API for Best Common Type computation (Solver-First architecture)
        // When preserve_literal_types is set (e.g., inside generic call argument collection),
        // skip BCT's literal widening by computing the union directly. This preserves
        // literal types like "foo" | "bar" instead of widening to string, enabling
        // correct type parameter inference (e.g., K inferred as "foo" | "bar" not string).
        let element_type = if self.ctx.preserve_literal_types {
            self.ctx.types.union(element_types.clone())
        } else {
            expr_ops::compute_best_common_type(
                self.ctx.types,
                &element_types,
                Some(&self.ctx), // Pass TypeResolver for class hierarchy BCT
            )
        };

        // TS2590: Also check the solver's flag in case the union was constructed
        // through a different path (e.g., preserve_literal_types or internal union ops).
        if self.ctx.types.take_union_too_complex() {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                idx,
                diagnostic_messages::EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT,
                diagnostic_codes::EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT,
            );
            return factory.array(TypeId::ERROR);
        }

        factory.array(element_type)
    }

    /// Resolve array element type from union members by checking number index signatures.
    ///
    /// When the contextual type is a union like `StyleBase | StyleArray` where `StyleArray`
    /// is a Lazy interface extending `Array<Style>`, the solver's `get_array_element_type`
    /// can't extract the element type because Lazy interface types don't evaluate to
    /// array type data. This method resolves each union member via the checker's type
    /// environment and checks for number index signatures (which Array types have).
    fn resolve_array_element_type_from_union_members(
        &mut self,
        contextual: TypeId,
    ) -> Option<TypeId> {
        let members = tsz_solver::type_queries::get_union_members(self.ctx.types, contextual)?;

        let mut element_types = Vec::new();
        for &member in &members {
            if member.is_nullable() {
                continue;
            }

            // Already a recognized array/tuple type?
            if let Some(elem) =
                tsz_solver::type_queries::get_array_element_type(self.ctx.types, member)
            {
                element_types.push(elem);
                continue;
            }

            // Resolve Lazy interface types to their structural form via the checker's
            // type environment, then check for a number index signature.
            let resolved = self.resolve_lazy_type(member);
            let resolved = self.evaluate_type_with_env(resolved);
            let resolved = self.resolve_type_for_property_access(resolved);

            // Check if the resolved type has a number index signature (array-like)
            let resolver = tsz_solver::IndexSignatureResolver::new(self.ctx.types);
            if let Some(elem) = resolver.resolve_number_index(resolved) {
                element_types.push(elem);
            }
        }

        if element_types.is_empty() {
            None
        } else if element_types.len() == 1 {
            Some(element_types[0])
        } else {
            Some(self.ctx.types.union(element_types))
        }
    }
}
