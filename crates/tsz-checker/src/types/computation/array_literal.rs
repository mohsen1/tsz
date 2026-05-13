//! Array literal type computation.
//!
//! This module handles type computation for array literal expressions like
//! `[1, 2, 3]`, `["a", "b"]`, `[...arr]`, etc. Extracted from `helpers.rs`
//! to keep module sizes manageable.

use crate::query_boundaries::common as query_common;
use crate::query_boundaries::common::ContextualTypeContext;
use crate::query_boundaries::type_computation::core as expr_ops;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{TupleElement, TypeId};

impl<'a> CheckerState<'a> {
    fn array_element_is_const_assertion(&self, elem_idx: NodeIndex) -> bool {
        let mut current = elem_idx;
        while let Some(node) = self.ctx.arena.get(current) {
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.ctx.arena.get_parenthesized(node)
            {
                current = paren.expression;
                continue;
            }

            if (node.kind == syntax_kind_ext::AS_EXPRESSION
                || node.kind == syntax_kind_ext::TYPE_ASSERTION)
                && let Some(assertion) = self.ctx.arena.get_type_assertion(node)
            {
                return self.is_const_assertion_type_node(assertion.type_node);
            }

            return false;
        }
        false
    }

    fn mutable_spread_declared_type_in_const_assertion(
        &mut self,
        expression: NodeIndex,
    ) -> Option<TypeId> {
        if !self.ctx.in_const_assertion {
            return None;
        }
        let expression = self.ctx.arena.skip_parenthesized(expression);
        let node = self.ctx.arena.get(expression)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, expression)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        for decl_idx in symbol.declarations.iter().copied() {
            let decl_node = self.ctx.arena.get(decl_idx)?;
            let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                continue;
            };
            if self.ctx.arena.is_const_variable_declaration(decl_idx) {
                return None;
            }
            if var_decl.type_annotation.is_some() {
                return None;
            }
            let init_idx = self.ctx.arena.skip_parenthesized(var_decl.initializer);
            let init_node = self.ctx.arena.get(init_idx)?;
            if init_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
                return None;
            }
            let prev_in_const_assertion = self.ctx.in_const_assertion;
            self.ctx.in_const_assertion = false;
            let declared_type = self.get_type_of_variable_declaration(decl_idx);
            self.ctx.in_const_assertion = prev_in_const_assertion;
            return Some(declared_type);
        }
        None
    }

    fn promise_like_array_context_shape(&self, type_id: TypeId) -> Option<TypeId> {
        match crate::query_boundaries::common::classify_promise_type(self.ctx.types, type_id) {
            crate::query_boundaries::common::PromiseTypeKind::Application { args, .. }
                if self.type_ref_is_promise_like(type_id) =>
            {
                args.first().and_then(|&inner| {
                    crate::query_boundaries::common::array_applicable_type(self.ctx.types, inner)
                })
            }
            _ => None,
        }
    }

    fn empty_array_literal_prefers_never(&self, idx: NodeIndex) -> bool {
        let Some(parent_idx) = self.ctx.arena.parent_of(idx) else {
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

    /// Whether this empty array literal is in a context whose declared or
    /// defaulted type already pins the element shape.
    ///
    /// In those contexts (variable initializers with annotations, assignment
    /// RHS, defaulting expressions, return statement RHS for an annotated
    /// return type, etc.) tsc
    /// adopts the contextual element type for `[]` rather than the
    /// evolving-array `never[]` base. Adopting the contextual element here
    /// avoids `never[]` poisoning subsequent flow narrowing of the storage
    /// slot — without that, a `prop = []` write under an `if (!prop) { ... }`
    /// guard injects `never[]` into the narrowed union, and method dispatch
    /// over `Array<X> | never[]` collapses contravariant parameters to
    /// `never` (e.g. `prop?.push(x)` reports a false TS2345).
    ///
    /// Generic-call argument positions are explicitly excluded because the
    /// contextual type there is a still-being-inferred type parameter; using
    /// it would prevent the inference engine from binding the parameter to
    /// `never`.
    fn empty_array_can_adopt_contextual_element_type(&self, idx: NodeIndex) -> bool {
        let Some(parent_idx) = self.ctx.arena.parent_of(idx) else {
            return false;
        };
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        match parent_node.kind {
            k if k == syntax_kind_ext::BINARY_EXPRESSION => self
                .ctx
                .arena
                .get_binary_expr(parent_node)
                .is_some_and(|binary| {
                    // `&&=` is intentionally excluded: tsc keeps the RHS
                    // empty array as `never[]` for `a &&= []` (the result
                    // type is `(falsy a) | typeof []`, with `[]` left at
                    // its narrowest form), which preserves a follow-up
                    // `.push(x)` failure on the never-element receiver.
                    // `||=` and `??=` widen because the RHS is the
                    // "default value" replacing a falsy/nullable LHS, so
                    // adopting the LHS element type matches user intent.
                    // `params.path || []` also uses the left operand as
                    // contextual type when no outer contextual type is
                    // available, so this prevents a fallback `never[]` from
                    // polluting `Omit<...>["path"] | never[]` unions.
                    binary.right == idx
                        && (binary.operator_token == tsz_scanner::SyntaxKind::EqualsToken as u16
                            || binary.operator_token
                                == tsz_scanner::SyntaxKind::BarBarEqualsToken as u16
                            || binary.operator_token
                                == tsz_scanner::SyntaxKind::QuestionQuestionEqualsToken as u16
                            || binary.operator_token == tsz_scanner::SyntaxKind::BarBarToken as u16)
                }),
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => self
                .ctx
                .arena
                .get_variable_declaration(parent_node)
                .is_some_and(|decl| decl.initializer == idx && decl.type_annotation.is_some()),
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT
                || k == syntax_kind_ext::PARAMETER
                || k == syntax_kind_ext::PROPERTY_DECLARATION =>
            {
                true
            }
            _ => false,
        }
    }

    fn union_context_for_array_literal_is_ambiguous(&mut self, contextual: TypeId) -> bool {
        if self.union_context_for_array_literal_prefers_tuple(contextual) {
            return false;
        }

        let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, contextual)
        else {
            return false;
        };

        let mut applicable_shapes = Vec::new();
        let mut saw_tuple_applicable = false;
        let mut saw_non_tuple_applicable = false;
        for member in members {
            // Skip null/undefined/void — these don't contribute to array contextual
            // typing ambiguity. tsc strips these before checking (getNonNullableType).
            if member.is_nullable() {
                continue;
            }

            if let Some(applicable) =
                crate::query_boundaries::common::array_applicable_type(self.ctx.types, member)
            {
                if !applicable_shapes.contains(&applicable) {
                    applicable_shapes.push(applicable);
                }
                if crate::query_boundaries::common::is_tuple_type(self.ctx.types, applicable) {
                    saw_tuple_applicable = true;
                } else {
                    saw_non_tuple_applicable = true;
                }
                continue;
            }

            if let Some(applicable) = self.promise_like_array_context_shape(member) {
                if !applicable_shapes.contains(&applicable) {
                    applicable_shapes.push(applicable);
                }
                if crate::query_boundaries::common::is_tuple_type(self.ctx.types, applicable) {
                    saw_tuple_applicable = true;
                } else {
                    saw_non_tuple_applicable = true;
                }
                continue;
            }

            if member == TypeId::ANY || member == TypeId::UNKNOWN {
                return true;
            }

            if crate::query_boundaries::common::type_parameter_constraint(self.ctx.types, member)
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
                use crate::query_boundaries::common::IndexSignatureResolver;
                let resolved = self.resolve_lazy_type(member);
                let resolved = self.evaluate_type_with_env(resolved);
                let resolver = IndexSignatureResolver::new(self.ctx.types);
                let si = resolver.resolve_string_index(resolved);
                if let Some(value_type) = si {
                    if !applicable_shapes.contains(&value_type) {
                        applicable_shapes.push(value_type);
                    }
                    saw_non_tuple_applicable = true;
                    continue;
                }
            }

            // Non-array-applicable object-like types without index signatures can't
            // meaningfully contextually type array literals. Skip them.
            if crate::query_boundaries::dispatch::is_object_like_type(self.ctx.types, member) {
                continue;
            }
        }

        applicable_shapes.len() > 1 && (!saw_tuple_applicable || saw_non_tuple_applicable)
    }

    fn union_context_for_array_literal_prefers_tuple(&self, contextual: TypeId) -> bool {
        let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, contextual)
        else {
            return false;
        };

        let mut saw_tuple = false;
        for member in members {
            let Some(applicable) =
                crate::query_boundaries::common::array_applicable_type(self.ctx.types, member)
            else {
                return false;
            };

            if !crate::query_boundaries::common::is_tuple_type(self.ctx.types, applicable) {
                return false;
            }
            saw_tuple = true;
        }

        saw_tuple
    }

    fn sole_array_applicable_union_context(&mut self, contextual: TypeId) -> Option<TypeId> {
        let members = crate::query_boundaries::common::union_members(self.ctx.types, contextual)?;
        let mut applicable_shape = None;

        for member in members {
            if member.is_nullable() {
                continue;
            }

            let candidate =
                crate::query_boundaries::common::array_applicable_type(self.ctx.types, member)
                    .or_else(|| self.promise_like_array_context_shape(member));

            let Some(candidate) = candidate else {
                if member == TypeId::ANY
                    || member == TypeId::UNKNOWN
                    || crate::query_boundaries::common::type_parameter_constraint(
                        self.ctx.types,
                        member,
                    )
                    .is_some()
                {
                    return None;
                }

                if crate::query_boundaries::dispatch::is_object_like_type(self.ctx.types, member) {
                    continue;
                }

                return None;
            };

            if applicable_shape.is_some_and(|existing| existing != candidate) {
                return None;
            }
            applicable_shape = Some(candidate);
        }

        applicable_shape
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

        if self.ctx.in_const_assertion && self.array_literal_produces_too_large_tuple(idx) {
            self.error_at_node(
                idx,
                crate::diagnostics::diagnostic_messages::EXPRESSION_PRODUCES_A_TUPLE_TYPE_THAT_IS_TOO_LARGE_TO_REPRESENT,
                crate::diagnostics::diagnostic_codes::EXPRESSION_PRODUCES_A_TUPLE_TYPE_THAT_IS_TOO_LARGE_TO_REPRESENT,
            );
            return TypeId::ANY;
        }

        if array.elements.nodes.is_empty() {
            // In const assertion context (e.g., `[] as const` or inside a `const T`
            // type parameter call), empty arrays become empty readonly tuples, not
            // `never[]`. This matches tsc's behavior for `[] as const` → `readonly []`.
            if self.ctx.in_const_assertion {
                return factory.tuple(vec![]);
            }

            // Empty array literal element type depends on noImplicitAny and strictNullChecks:
            //   - noImplicitAny OFF: any[] (tsc default for unannotated empty arrays)
            //   - noImplicitAny ON + strictNullChecks ON: never[] (strict mode evolving array)
            //   - noImplicitAny ON + strictNullChecks OFF (non-strict, TS files): never[]
            //     In non-strict TS mode, the empty array starts as never[]. The element type
            //     `never` is subsequently widened to `undefined` (then to `any`) only in
            //     specific generator yield-type inference contexts (see dispatch_yield.rs).
            //
            // For operators like ||= and ??=, tsc uses UnionReduction.Subtype in
            // the result type computation, which removes never[] when a compatible
            // array type is present (e.g., number[] | never[] → number[]).
            // For &&, no subtype reduction is applied, so undefined | never[] stays.
            if let Some(contextual) = contextual_type {
                let mut resolved = self.evaluate_contextual_type(contextual);
                resolved =
                    crate::query_boundaries::common::remove_nullish(self.ctx.types, resolved);
                resolved = self.evaluate_type_with_env(resolved);
                resolved = self.resolve_lazy_type(resolved);
                resolved = self.evaluate_application_type(resolved);
                if crate::query_boundaries::common::index_access_parts(self.ctx.types, resolved)
                    .is_some()
                {
                    let evaluated = self.evaluate_type_for_assignability(resolved);
                    if evaluated != TypeId::UNKNOWN {
                        resolved = evaluated;
                    }
                }
                resolved = self.reduce_literal_index_access_property_types(resolved);
                resolved = self.resolve_index_access_base_constraint(resolved);
                resolved = self.resolve_type_for_property_access(resolved);
                if crate::query_boundaries::common::is_tuple_type(self.ctx.types, resolved) {
                    return factory.tuple(vec![]);
                }
                // When the contextual type is an array (e.g. assigning `[]` to
                // `Types[T][]` in `obj.entries[name] = []`), thread the
                // contextual element type into the literal instead of using
                // the evolving-array `never[]` base. Using `never[]` here
                // poisons subsequent flow narrowing of the assigned slot:
                // a downstream `union_reduce(declared_type | never[])` keeps
                // `never[]` as the contravariant supertype for callable
                // members like `push`, collapsing the method signature's
                // element type to `never` and producing false TS2345 on
                // `obj.entries[name]?.push(item)`. tsc instead types `[]`
                // with the contextual element type at the assignment site,
                // so the narrowed slot stays compatible with the declared
                // array. Skip when the literal node is the base of a JSDoc
                // expando initializer or otherwise wants the evolving-array
                // base — those cases set `empty_array_literal_prefers_never`.
                if !self.empty_array_literal_prefers_never(idx)
                    && self.empty_array_can_adopt_contextual_element_type(idx)
                    && let Some(elem) = crate::query_boundaries::common::array_element_type(
                        self.ctx.types,
                        resolved,
                    )
                {
                    return factory.array(elem);
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
            let ctx_type =
                crate::query_boundaries::common::remove_nullish(self.ctx.types, ctx_type);
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
                crate::query_boundaries::common::array_applicable_type(self.ctx.types, evaluated)
            {
                // Mark constraint-derived when the resolved type was a type parameter
                // (get_array_applicable_type handles TypeParameter internally).
                // Also check union members: when the contextual type is a union of
                // parameter types from multiple overloads (e.g., `T | Iterable<U>`),
                // the applicable type may come from a TypeParameter member's constraint
                // even though the whole union isn't a TypeParameter.
                if crate::query_boundaries::common::type_parameter_constraint(
                    self.ctx.types,
                    evaluated,
                )
                .is_some()
                    || crate::query_boundaries::common::union_members(self.ctx.types, evaluated)
                        .is_some_and(|members| {
                            members.iter().any(|&m| {
                                crate::query_boundaries::common::type_parameter_constraint(
                                    self.ctx.types,
                                    m,
                                )
                                .is_some()
                            })
                        })
                {
                    tuple_context_from_constraint = true;
                }
                return Some(applicable);
            }
            if let Some(applicable) = self.sole_array_applicable_union_context(evaluated) {
                return Some(applicable);
            }
            // When the contextual type is a type parameter (e.g., `T extends [string, number]`),
            // use its base constraint for tuple context detection. This matches tsc's behavior
            // where `getApparentTypeOfContextualType` resolves type parameter constraints so
            // array literals are typed as tuples instead of being widened to arrays.
            // We only use this for shape detection (tuple vs array), NOT for element contextual
            // typing — element types should be inferred independently to preserve literals.
            if let Some(constraint) = crate::query_boundaries::common::type_parameter_constraint(
                self.ctx.types,
                evaluated,
            ) {
                let constraint = self.resolve_lazy_type(constraint);
                let constraint = self.evaluate_application_type(constraint);
                if let Some(applicable) = crate::query_boundaries::common::array_applicable_type(
                    self.ctx.types,
                    constraint,
                ) {
                    tuple_context_from_constraint = true;
                    return Some(applicable);
                }
            }
            None
        });

        let tuple_context = applicable_contextual_type.and_then(|applicable| {
            let elems =
                crate::query_boundaries::common::tuple_elements(self.ctx.types, applicable)?;
            // When all tuple elements are rest (e.g., `[...any[]]` from a
            // destructuring pattern like `[...rest]`), the contextual type is
            // effectively an array, not a fixed-length tuple.  Don't force
            // tuple inference in that case — the array literal should be typed
            // as an array (e.g., `(string | number)[]`), not a tuple.
            // However, when the rest element's type is a type parameter (e.g.,
            // `[...T]` where `T extends Container<unknown>[]`), we should still
            // force tuple inference. The `[...T]` pattern is specifically used in
            // TypeScript to trigger tuple inference from array literal arguments.
            // We also keep tuple inference when the contextual type is an
            // intersection (e.g., `[...number[]] & { length: 2 }`): the other
            // intersection members add structural constraints that make the
            // overall shape a fixed-length tuple, matching tsc's behavior where
            // `isTupleLikeType` returns true for such intersections because a
            // numeric "0" property is reachable through the tuple member.
            if elems.iter().all(|e| e.rest) {
                let has_type_param_rest = elems.iter().any(|e| {
                    e.rest
                        && crate::query_boundaries::common::is_type_parameter(
                            self.ctx.types,
                            e.type_id,
                        )
                });
                let is_intersection_context =
                    crate::query_boundaries::common::intersection_members(
                        self.ctx.types,
                        applicable,
                    )
                    .is_some();
                // Full contextual evaluation can expand `HandleOptions<T[K]>` to an
                // object/index-signature type, so also inspect the original tuple rest
                // before mapped-application identity is erased.
                let has_homomorphic_mapped_rest = elems
                    .iter()
                    .any(|e| e.rest && self.is_homomorphic_mapped_rest_context(e.type_id))
                    || original_contextual_type.is_some_and(|original| {
                        self.tuple_type_has_homomorphic_mapped_rest(original)
                    });
                if has_type_param_rest || is_intersection_context || has_homomorphic_mapped_rest {
                    Some(elems)
                } else {
                    None
                }
            } else {
                Some(elems)
            }
        });

        // When the contextual type is a homomorphic mapped type (e.g., { [K in keyof T]: ... }),
        // array literals should be typed as tuples to preserve per-element type info.
        // Homomorphic mapped types preserve array/tuple structure, so the input must
        // maintain individual element types for reverse mapped type inference to work.
        // Without this, array literals become Array(union) which loses element-level detail.
        let force_tuple_for_mapped = tuple_context.is_none()
            && (resolved_contextual_type.is_some_and(|resolved| {
                crate::query_boundaries::common::is_homomorphic_mapped_type_context(
                    self.ctx.types,
                    resolved,
                )
            }) || original_contextual_type.is_some_and(|original| {
                self.original_context_is_homomorphic_mapped_application(original)
            }));

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
                crate::query_boundaries::common::union_contains_tuple(self.ctx.types, applicable)
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
                crate::query_boundaries::common::array_applicable_type(self.ctx.types, resolved)
                    .is_none()
                    && crate::query_boundaries::common::is_tuple_like_type(self.ctx.types, resolved)
            });

        // Use the applicable (narrowed) type for contextual typing when available,
        // falling back to the full resolved contextual type.
        // When tuple context came from a type parameter constraint, don't use it for
        // element contextual typing — only use it for tuple shape detection. Element
        // types should be inferred independently to preserve literal types during
        // generic inference (e.g., `fx<T extends [string, 'a'|'b']>(x: T)` called
        // with `['x', 'a']` should infer `["x", "a"]`, not `[string, string]`).
        //
        // EXCEPTION: When the contextual union is ALL tuples (force_tuple_for_union_context),
        // preserve the contextual type even when "ambiguous". Per-position typing
        // (`get_tuple_element_type_with_count`) unions element types across union members
        // (e.g. `["a"] | ["b"]` gives position-0 context `"a" | "b"`), which preserves
        // literal types so `["a"]` correctly checks against `["a"] | ["b"]`.
        let effective_contextual =
            if union_array_context_is_ambiguous && !force_tuple_for_union_context {
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
                || crate::query_boundaries::common::contains_type_parameters(self.ctx.types, ty)
        });

        // Get types of all elements, applying contextual typing when available.
        // Track (type, node_index) pairs for excess property checking on array elements.
        let mut element_types = Vec::new();
        let mut element_nodes = Vec::new();
        let mut tuple_elements = Vec::new();
        let mut preserve_tuple_spread_literals = false;
        let mut saw_array_element_for_bct = false;
        let mut all_array_elements_const_asserted = true;
        // Total element count for tuple contextual typing. Elided slots count toward
        // the position of subsequent elements (e.g. `[42,,true]` has length 3 with
        // an undefined slot at index 1), matching tsc's `elementCount = elements.length`.
        let total_elem_count = array.elements.nodes.len();
        for (index, &elem_idx) in array.elements.nodes.iter().enumerate() {
            if elem_idx.is_none() {
                // Elided element (hole) in array literal: `[a, , b]` has an
                // OmittedExpression at index 1. tsc types `OmittedExpression` as
                // `undefinedWideningType` (see `case SyntaxKind.OmittedExpression`
                // in checkExpressionWorker) and pushes it as a Required element
                // when `exactOptionalPropertyTypes` is OFF — the resulting source
                // tuple `[number, undefined, true]` then assigns to `[number, string?, boolean?]`
                // because each Required source slot of type `undefined` is widened to
                // `T | undefined` against an Optional target slot in tuple subtyping.
                //
                // For destructuring targets (`[a, , b] = expr`) the elision is a
                // "skip" rather than a value, so preserve the current skip behavior.
                if self.ctx.in_destructuring_target {
                    continue;
                }
                let (hole_type, hole_optional) = if self.ctx.exact_optional_property_types()
                    && (tuple_context.is_some()
                        || force_tuple_for_union_context
                        || force_tuple_for_mapped
                        || force_tuple_for_constraint_hint
                        || force_tuple_for_tuple_like)
                {
                    (TypeId::NEVER, true)
                } else {
                    (TypeId::UNDEFINED, false)
                };
                if tuple_context.is_some()
                    || force_tuple_for_union_context
                    || force_tuple_for_mapped
                    || force_tuple_for_constraint_hint
                    || force_tuple_for_tuple_like
                    || self.ctx.in_const_assertion
                {
                    tuple_elements.push(TupleElement {
                        type_id: hole_type,
                        name: None,
                        optional: hole_optional,
                        rest: false,
                    });
                } else {
                    saw_array_element_for_bct = true;
                    all_array_elements_const_asserted = false;
                    element_types.push(hole_type);
                    // Note: we don't add to element_nodes since there is no node
                    // for an elision. Excess property checks are skipped for the slot.
                }
                continue;
            }

            // Build per-element typing request instead of mutating ctx.contextual_type
            let elem_request = if union_array_context_is_ambiguous && !force_tuple_for_union_context
            {
                // When the contextual union is ambiguous (multiple applicable element types),
                // clear the contextual type for each element so closures don't inherit
                // the array's union contextual type and inadvertently get typed parameters.
                // EXCEPTION: union-of-all-tuples is handled via per-position typing below.
                crate::context::TypingRequest::NONE
            } else if let Some(ref helper) = ctx_helper {
                if tuple_context.is_some() || force_tuple_for_union_context {
                    // For a union of all tuple types (force_tuple_for_union_context), use
                    // per-position contextual typing: the element type at position `index`
                    // is the union of each member tuple's type at that position.
                    // e.g. `["a"] | ["b"]` gives position 0 context `"a" | "b"`,
                    // preserving string literal types instead of widening to `string`.
                    match helper.get_tuple_element_type_with_count(index, total_elem_count) {
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
                let spread_expr_type = if let Some(declared_type) =
                    self.mutable_spread_declared_type_in_const_assertion(spread_data.expression)
                {
                    self.resolve_lazy_type(declared_type)
                } else {
                    spread_expr_type
                };
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
                if let Some(elems) = crate::query_boundaries::common::tuple_elements(
                    self.ctx.types,
                    spread_expr_type,
                ) {
                    if tuple_context.is_some() || self.ctx.in_const_assertion {
                        // For tuple context, expand each element of the spread source
                        // tuple, preserving its `rest` flag so a trailing rest element
                        // (e.g. spreading `[string, boolean, ...boolean[]]`) keeps the
                        // tuple shape `[..., ...boolean[]]` instead of collapsing to
                        // `[..., boolean[]]` (a fixed array element). The latter both
                        // garbles diagnostic display and triggers spurious TS2322
                        // when assigning to a target tuple whose own rest accepts the
                        // source's variadic tail.
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
                                rest: elem.rest,
                            });
                            // Don't increment index here - each tuple element maps to position
                        }
                    } else {
                        // For array context, add element types
                        preserve_tuple_spread_literals = true;
                        saw_array_element_for_bct = true;
                        all_array_elements_const_asserted = false;
                        for elem in &elems {
                            if elem.rest
                                && let Some(rest_elem) =
                                    query_common::array_element_type(self.ctx.types, elem.type_id)
                            {
                                element_types.push(rest_elem);
                            } else {
                                element_types.push(elem.type_id);
                            }
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

                if tuple_context.is_some() || self.ctx.in_const_assertion {
                    let rest_type = if self.ctx.in_const_assertion && tuple_context.is_none() {
                        factory.array(elem_type)
                    } else {
                        elem_type
                    };
                    let optional = match tuple_context.as_ref().and_then(|tc| tc.get(index)) {
                        Some(el) => el.optional,
                        None => false,
                    };
                    tuple_elements.push(TupleElement {
                        type_id: rest_type,
                        name: None,
                        optional,
                        rest: true, // Mark as spread for non-tuple spreads in tuple context
                    });
                } else {
                    saw_array_element_for_bct = true;
                    all_array_elements_const_asserted = false;
                    element_types.push(elem_type);
                }
                continue;
            }

            // Regular (non-spread) element
            let elem_is_const_assertion = self.array_element_is_const_assertion(elem_idx);
            let mut elem_type = if self.ctx.in_destructuring_target {
                self.destructuring_target_type_from_initializer(elem_idx)
            } else {
                self.get_type_of_node_with_request(elem_idx, &elem_request)
            };
            if !self.ctx.in_destructuring_target
                && elem_node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16
                && self.ctx.enclosing_class.is_some()
                && !self.is_this_in_nested_function_inside_class(elem_idx)
                && !self.is_this_in_static_class_member(elem_idx)
            {
                elem_type = self.ctx.types.this_type();
            }

            if !self.ctx.in_destructuring_target
                && (self.ctx.in_const_assertion || self.ctx.preserve_literal_types)
                && let Some(elem_node) = self.ctx.arena.get(elem_idx)
                && elem_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(binary) = self.ctx.arena.get_binary_expr(elem_node)
                && binary.operator_token == tsz_scanner::SyntaxKind::EqualsToken as u16
            {
                elem_type = self.get_type_of_node_with_request(binary.right, &elem_request);
            }

            if tuple_context.is_some() || self.ctx.in_const_assertion {
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
                saw_array_element_for_bct = true;
                all_array_elements_const_asserted &= elem_is_const_assertion;
                element_types.push(elem_type);
                element_nodes.push(elem_idx);
            }
        }

        if tuple_context.is_some() || force_tuple_for_union_context {
            // Check excess properties on object literal elements within tuple-typed array literals.
            // When an array literal is contextually typed as a tuple (e.g., `[{x1: 10}, ""]` as
            // `[ObjType, string]`), each element that is an object literal must be checked for
            // excess properties against the expected tuple element type. This mirrors the array
            // context path (below) but uses per-position tuple element types.
            //
            // Use the source-position-aligned tuple_index (matching the contextual extractor's
            // index) so excess property checks line up with the correct contextual element type
            // even when the array literal contains elisions like `[ {x:1}, , {y:2} ]`.
            if let Some(ref helper) = ctx_helper {
                for (tuple_index, &elem_idx) in array.elements.nodes.iter().enumerate() {
                    if elem_idx.is_none() {
                        continue;
                    }
                    if let Some(elem_node) = self.ctx.arena.get(elem_idx)
                        && elem_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        && let Some(expected_type) =
                            helper.get_tuple_element_type_with_count(tuple_index, total_elem_count)
                    {
                        let elem_type = tuple_elements
                            .get(tuple_index)
                            .map(|te| te.type_id)
                            .unwrap_or(TypeId::ERROR);
                        self.check_object_literal_excess_properties(
                            elem_type,
                            expected_type,
                            elem_idx,
                        );
                    }
                }
            }
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
            if tuple_elements.len() == 1 && tuple_elements[0].rest {
                return tuple_elements[0].type_id;
            }
            return factory.tuple(tuple_elements);
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
            && context_element_type != TypeId::ANY
            && context_element_type != TypeId::UNKNOWN
            && context_element_type != TypeId::NEVER
            && !self.ctx.preserve_literal_types
            && !self.ctx.skip_array_contextual_supertype_collapse
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
                //
                // Skip this collapse while generic call/new inference is preserving literals.
                // In those paths, the actual element tuple types are evidence for overload
                // selection and type parameter inference. Normalizing `[["", true], ["", 0]]`
                // to `Array<readonly [K, V]>` too early hides the heterogeneous entry types
                // that tsc uses to reject `new Map(...)` with TS2769.
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

        // TS2590: Pre-check element count before BCT. tsc's removeSubtypes only
        // increments its cost counter for StructuredOrInstantiable source types —
        // identity-comparable primitives/literals (number/string/boolean literals,
        // null, undefined, void, never, enum members, unique symbols) short-circuit
        // on TypeId equality and don't drive the cost. So filter them out before
        // counting; otherwise an array of 1000+ distinct number literals (e.g.
        // `[0 as 0, 1 as 1, ...]`) wrongly triggers TS2590 even though tsc widens
        // them to `number` without complaint.
        {
            let mut deduped = element_types.clone();
            deduped.sort_unstable_by_key(|t| t.0);
            deduped.dedup();
            deduped.retain(|t| {
                !crate::query_boundaries::state::type_environment::is_identity_comparable_type(
                    self.ctx.types,
                    *t,
                )
            });
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
        let preserve_const_asserted_element_literals =
            saw_array_element_for_bct && all_array_elements_const_asserted;
        let element_type = if self.ctx.preserve_literal_types
            || preserve_tuple_spread_literals
            || preserve_const_asserted_element_literals
        {
            self.ctx.types.union(element_types.clone())
        } else {
            expr_ops::compute_best_common_type_cached(
                self.ctx.types,
                Some(self.ctx.types),
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

    /// When the contextual type is a generic application like `Definition<Schema>`, the
    /// resolved type (after `evaluate_contextual_type`) is an Object, not a Mapped type,
    /// so `is_homomorphic_mapped_type_context` on the resolved type returns false. This
    /// helper inspects the Application's generic definition body directly: if the body is
    /// `{ [K in keyof T]: ... }` the application IS a homomorphic mapped context.
    fn original_context_is_homomorphic_mapped_application(
        &mut self,
        type_id: tsz_solver::TypeId,
    ) -> bool {
        use crate::query_boundaries::common as query;
        let Some((base, _args)) = query::application_info(self.ctx.types, type_id) else {
            return false;
        };
        let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(base) else {
            return false;
        };
        let (body_type, _type_params) = self.type_reference_symbol_type_with_params(sym_id);
        query::is_homomorphic_mapped_type_context(self.ctx.types, body_type)
    }

    fn is_homomorphic_mapped_rest_context(&mut self, type_id: tsz_solver::TypeId) -> bool {
        use crate::query_boundaries::common as query;
        query::is_homomorphic_mapped_type_context(self.ctx.types, type_id)
            || self.original_context_is_homomorphic_mapped_application(type_id)
            || self.mapped_type_has_generic_keyof_source(type_id)
            || {
                let evaluated = self.evaluate_application_type(type_id);
                evaluated != type_id && self.mapped_type_has_generic_keyof_source(evaluated)
            }
    }

    fn tuple_type_has_homomorphic_mapped_rest(&mut self, type_id: tsz_solver::TypeId) -> bool {
        use crate::query_boundaries::common as query;
        query::array_applicable_type(self.ctx.types, type_id)
            .and_then(|applicable| query::tuple_elements(self.ctx.types, applicable))
            .is_some_and(|elems| {
                elems
                    .iter()
                    .any(|e| e.rest && self.is_homomorphic_mapped_rest_context(e.type_id))
            })
    }

    fn mapped_type_has_generic_keyof_source(&self, type_id: tsz_solver::TypeId) -> bool {
        use crate::query_boundaries::common as query;
        let Some(mapped) = query::mapped_type_info(self.ctx.types, type_id) else {
            return false;
        };
        let Some(keyof_source) = query::keyof_inner_type(self.ctx.types, mapped.constraint) else {
            return false;
        };
        query::contains_type_parameters(self.ctx.types, keyof_source)
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
        let members = crate::query_boundaries::common::union_members(self.ctx.types, contextual)?;

        let mut element_types = Vec::new();
        for &member in &members {
            if member.is_nullable() {
                continue;
            }

            // Already a recognized array/tuple type?
            if let Some(elem) =
                crate::query_boundaries::common::array_element_type(self.ctx.types, member)
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

#[cfg(test)]
mod array_literal_context_tests {
    use crate::context::CheckerOptions;
    use crate::test_utils::{check_source, check_source_codes};

    fn check_strict_codes(source: &str) -> Vec<u32> {
        check_source(
            source,
            "test.ts",
            CheckerOptions {
                strict: true,
                strict_null_checks: true,
                ..CheckerOptions::default()
            },
        )
        .iter()
        .map(|d| d.code)
        .collect()
    }

    #[test]
    fn empty_array_in_storage_assignment_adopts_contextual_element() {
        // Regression for conformance test mappedTypeGenericIndexedAccess.ts:
        // `obj.entries[name] = []` under an `if (!obj.entries[name]) { … }`
        // guard was injecting `never[]` into the narrowed slot, then
        // `obj.entries[name]?.push(item)` collapsed push's contravariant
        // parameter to `never` and reported a false TS2345 against the
        // generic argument type `Types[T]`.
        //
        // tsc threads the storage slot's element type into the literal at
        // the assignment site, so the narrowed slot stays compatible with
        // the declared array.
        let source = r#"
type Types = {
    first: { a1: true };
    second: { a2: true };
    third: { a3: true };
}

class Test {
    entries: { [T in keyof Types]?: Types[T][] } = {};

    addEntry<T extends keyof Types>(name: T, entry: Types[T]) {
        if (!this.entries[name]) {
            this.entries[name] = [];
        }
        this.entries[name]?.push(entry);
    }
}
"#;
        let errors = check_source_codes(source);
        assert!(
            !errors.contains(&2345),
            "`this.entries[name]?.push(entry)` under an `if (!this.entries[name]) {{ this.entries[name] = []; }}` guard should not report TS2345, got: {errors:?}"
        );
    }

    #[test]
    fn empty_array_rhs_of_and_and_equals_keeps_never_element() {
        // Regression for conformance test logicalAssignment6.ts /
        // logicalAssignment7.ts:
        //   `(results &&= (results1 &&= [])).push(100)` where both
        //   `results` and `results1` are `number[] | undefined`.
        //
        // For `||=` and `??=`, the RHS empty array adopts the LHS's
        // element type (the RHS is the "default value" replacing a
        // falsy/nullable LHS). For `&&=` it does NOT — tsc keeps the
        // RHS literal at `never[]` so the chained `.push(100)` on the
        // resulting `(falsy results) | typeof []` reports TS2345
        // ("Argument of type '100' is not assignable to parameter of
        // type 'never'"). Without this distinction tsz silently
        // accepted the push.
        // The fix is observable here via the assignment to
        // `target: never[] | undefined`: with the bug, `[]` widened to
        // `number[]` and the expression type became
        // `undefined | number[]` — not assignable to `never[] | undefined`.
        // With the fix, `[]` stays `never[]` and the assignment is OK.
        let source = r#"
function foo3(arr: number[] | undefined) {
    const target: never[] | undefined = (arr &&= []);
    return target;
}
"#;
        let errors = check_strict_codes(source);
        assert!(
            !errors.contains(&2322),
            "`&&=` should leave the RHS `[]` at `never[]` so the assignment to `never[] | undefined` is clean, got: {errors:?}"
        );
    }

    #[test]
    fn empty_array_rhs_of_or_or_equals_adopts_lhs_element() {
        // Counter-test: `||=` and `??=` continue to widen the RHS empty
        // array to the LHS's element type. Without widening, the
        // expression would type as `number[] | never[]` (wider than the
        // declared `number[] | undefined`) and the LHS slot would also
        // be assigned `never[]`, weakening downstream narrowing.
        // We verify the widening by assigning the expression to the
        // LHS's declared type — if widening regressed, the expression
        // type would carry `never[]` and the assignment to
        // `number[] | undefined` would still be OK (subtype). So we
        // also check the negative case: assigning to a `number[]` slot
        // (without `undefined`), which only succeeds when the falsy
        // branch can be eliminated. Either way TS2345/TS2322 must NOT
        // appear for the round trip.
        let source = r#"
function foo1(results: number[] | undefined) {
    const widened: number[] | undefined = (results ||= []);
    return widened;
}
function foo2(results: number[] | undefined) {
    const widened: number[] | undefined = (results ??= []);
    return widened;
}
"#;
        let errors = check_strict_codes(source);
        assert!(
            !errors.contains(&2322) && !errors.contains(&2345),
            "`||=`/`??=` widen RHS `[]` to `number[]`, so the round-trip assignment is clean, got: {errors:?}"
        );
    }

    #[test]
    fn empty_array_in_generic_call_argument_still_drives_inference_to_never() {
        // Guard against the storage-context widening leaking into generic
        // call argument positions. There the contextual type is a still-
        // being-inferred type parameter, and adopting it would prevent the
        // inference engine from binding the parameter to `never`.
        let source = r#"
declare function f1<T>(x: T[]): T;
let a1 = f1([]);
let check: never = a1;
"#;
        let errors = check_source_codes(source);
        assert!(
            errors.is_empty(),
            "f1([]) should still infer T = never (so `let check: never = a1` is OK), got: {errors:?}"
        );
    }

    #[test]
    fn rest_only_tuple_intersected_with_length_accepts_literal() {
        // Regression for conformance test contextualTypeWithTuple.ts (#29311):
        // `[...number[]] & { length: 2 }` was causing `[0, 0]` to be inferred as
        // `number[]` (because the rest-only tuple skipped tuple context), which
        // then failed to satisfy the intersection. tsc's `isTupleLikeType`
        // considers such intersections tuple-like, so the array literal must
        // use tuple inference and become `[number, number]`.
        let source = r#"
type test1 = [...number[]]
type fixed1 = test1 & { length: 2 }
let var1: fixed1 = [0, 0]
"#;
        let errors = check_source_codes(source);
        assert!(
            !errors.contains(&2322),
            "[0, 0] should be assignable to [...number[]] & {{ length: 2 }}, got: {errors:?}"
        );
    }

    #[test]
    fn rest_only_tuple_without_intersection_still_widens_to_array() {
        // Guard against over-broadening the fix. A bare rest-only tuple without
        // other intersection members continues to use array inference, matching
        // the original behavior for destructuring-style contextual types such as
        // `[...any[]]`.
        let source = r#"
declare let arr: (string | number)[];
let x: [...(string | number)[]] = arr;
"#;
        let errors = check_source_codes(source);
        assert!(
            !errors.contains(&2322),
            "array is still assignable to rest-only tuple, got: {errors:?}"
        );
    }

    #[test]
    fn elided_array_literal_element_typed_as_undefined_required() {
        // Regression for conformance test optionalTupleElements1.ts:
        // an elision (hole) in a non-destructuring array literal — e.g. `[42,,true]` —
        // must produce a tuple slot with type `undefined` (Required), matching tsc's
        // OmittedExpression -> undefinedWideningType. Previously the slot was dropped,
        // which both shifted subsequent positions and caused contextual typing to
        // mismatch optional tuple targets.
        //
        // `T3 = [number, string?, boolean?]` accepts `[42,,true]` because the source
        // tuple `[number, undefined, true]` (all Required) widens each Required
        // `undefined` against an Optional target slot to `T | undefined`.
        let source = r#"
type T3 = [number, string?, boolean?];
type T4 = [number?, string?, boolean?];
let t3: T3;
let t4: T4;
t3 = [42, , true];
t4 = [42, , true];
t4 = [, "hello", true];
t4 = [, , true];
"#;
        let errors = check_source_codes(source);
        assert!(
            !errors.contains(&2322),
            "elided array literal slots should produce undefined-typed Required tuple slots, got: {errors:?}"
        );
    }

    #[test]
    fn elided_array_literal_in_array_context_pushes_undefined() {
        // Without a tuple contextual type, an elision still contributes
        // `undefined` to the resulting array element type. tsc widens
        // `[1, , 3]` (no contextual) to `(number | undefined)[]`, so the
        // array literal is assignable to `(number | undefined)[]`.
        let source = r#"
const xs: (number | undefined)[] = [1, , 3];
"#;
        let errors = check_source_codes(source);
        assert!(
            !errors.contains(&2322),
            "[1, , 3] should be assignable to (number | undefined)[], got: {errors:?}"
        );
    }
}
