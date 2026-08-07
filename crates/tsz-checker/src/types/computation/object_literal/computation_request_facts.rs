//! Pre-scan of object-literal contextual-typing facts.
//!
//! Collects the `ObjectLiteralRequestFacts` (contextual type after nullish
//! stripping / union-discriminant narrowing, `ThisType` marker, getter names,
//! partial-initializer stack slot) that `get_type_of_object_literal_with_request`
//! consumes. Extracted verbatim from `computation.rs` to keep that shard under
//! the size limit.

use crate::context::{PartialObjectLiteralInitializer, TypingRequest};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

pub(super) struct ObjectLiteralRequestFacts {
    pub(super) contextual_type: Option<TypeId>,
    pub(super) original_contextual_type: Option<TypeId>,
    pub(super) all_properties_context_sensitive: bool,
    pub(super) obj_getter_names: rustc_hash::FxHashSet<String>,
    pub(super) marker_this_type: Option<TypeId>,
    pub(super) contextual_receiver_this_type: Option<TypeId>,
    pub(super) base_request: TypingRequest,
    pub(super) partial_initializer_stack_index: Option<usize>,
}

impl<'a> CheckerState<'a> {
    pub(super) fn collect_object_literal_request_facts(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
        obj_elements: &[NodeIndex],
    ) -> ObjectLiteralRequestFacts {
        let mut contextual_type = request.contextual_type;

        // Strip nullish types from contextual type for object literals.
        // When a parameter is optional (e.g., `options?: Opts`), the contextual type
        // includes `undefined`. Since an object literal can never be `undefined` or
        // `null`, using nullish types as contextual type causes incorrect `this` typing
        // (e.g., `this` becomes `undefined` inside method bodies) and breaks ThisType
        // marker extraction from intersection types like `Opts & ThisType<T>`.
        if let Some(ctx) = contextual_type {
            if ctx == TypeId::UNDEFINED || ctx == TypeId::NULL || ctx == TypeId::VOID {
                contextual_type = None;
            } else {
                let (non_nullish, _) = crate::query_boundaries::diagnostics::split_nullish_type(
                    self.ctx.types.as_type_database(),
                    ctx,
                );
                if let Some(non_nullish) = non_nullish
                    && non_nullish != ctx
                {
                    contextual_type = Some(non_nullish);
                }
            }
        }

        // Reduce an inline instantiated generic type-alias application whose body
        // is a computed (conditional/mapped) type to its structural form through
        // the authoritative resolver before per-property contextual types are
        // extracted. See `contextual_type_requires_authoritative_evaluation` for
        // why this is gated narrowly (#13618).
        if let Some(ctx) = contextual_type
            && self.contextual_type_requires_authoritative_evaluation(ctx)
        {
            let evaluated = self.evaluate_contextual_type(ctx);
            if evaluated != ctx {
                contextual_type = Some(evaluated);
            }
        }

        if let Some(ctx_ty) = contextual_type {
            // Keep the last real contextual object target we saw for this literal.
            // The same node can be recomputed later under TypingRequest::NONE during
            // diagnostic elaboration, and clearing the side table there loses the
            // richer surface we want to report.
            self.ctx
                .object_literal_tracking
                .contextual_targets
                .insert(idx, ctx_ty);
        }

        tracing::trace!(
            idx = idx.0,
            contextual_type = ?contextual_type.map(|t| t.0),
            contextual_type_display = ?contextual_type.map(|t| self.format_type(t)),
            "get_type_of_object_literal: entry"
        );

        let all_properties_context_sensitive = !obj_elements.is_empty()
            && obj_elements.iter().all(|&element_idx| {
                let Some(element) = self.ctx.arena.get(element_idx) else {
                    return false;
                };

                if let Some(prop) = self.ctx.arena.get_property_assignment(element) {
                    return super::super::contextual::is_contextually_sensitive(
                        self,
                        prop.initializer,
                    );
                }

                if element.kind == syntax_kind_ext::METHOD_DECLARATION {
                    return super::super::contextual::is_contextually_sensitive(self, element_idx);
                }

                element.kind == syntax_kind_ext::GET_ACCESSOR
                    || element.kind == syntax_kind_ext::SET_ACCESSOR
            });

        // Pre-scan: collect getter property names so setter TS7006 checks can
        // detect paired getters regardless of declaration order.
        let obj_getter_names: rustc_hash::FxHashSet<String> = obj_elements
            .iter()
            .filter_map(|&elem_idx| {
                let elem_node = self.ctx.arena.get(elem_idx)?;
                if elem_node.kind != syntax_kind_ext::GET_ACCESSOR {
                    return None;
                }
                let accessor = self.ctx.arena.get_accessor(elem_node)?;
                self.get_property_name_resolved(accessor.name)
            })
            .collect();

        // Pre-scan: narrow union contextual type via discriminant properties.
        // When the contextual type is a union (e.g. `A | B`) and the object literal
        // has literal-valued properties that discriminate the union, narrow to the
        // matching member(s) so other properties get precise contextual types.
        // Save original for TS7006 checks (must use pre-narrowed union to detect
        // primitive members like `string` in `string | FullRule`).
        let original_contextual_type = contextual_type;
        if let Some(ctx_type) = contextual_type {
            let narrowed = self
                .narrow_contextual_union_via_object_literal_discriminants(ctx_type, obj_elements);
            if narrowed != ctx_type {
                contextual_type = Some(narrowed);
            }
        }
        // Check for ThisType<T> marker in contextual type (Vue 2 / Options API
        // pattern) after union narrowing so discriminated object literals choose
        // the matching union member's marker.
        let marker_this_type: Option<TypeId> = contextual_type
            .and_then(|ctx_type| self.contextual_this_type_from_marker(ctx_type))
            .or_else(|| self.enclosing_object_literal_this_type_marker(idx));

        // Push this type onto stack if found (methods will pick it up)
        if let Some(mut this_type) = marker_this_type {
            // The ThisType<T> marker may contain unresolved type parameters
            // (e.g., `Data & Readonly<Props> & Instance` before inference completes)
            // or unresolved Lazy references to generic interfaces that need their
            // default type arguments applied (e.g., `ThisType<T & Comp>` where
            // `Comp<U = any>` appears as bare `Lazy(DefId)` without an Application
            // wrapper). Evaluate through the type environment to resolve both
            // cases, ensuring property access on `this` inside method bodies
            // works correctly.
            if crate::query_boundaries::diagnostics::contains_type_parameters(
                self.ctx.types,
                this_type,
            ) || crate::query_boundaries::diagnostics::contains_lazy_or_recursive(
                self.ctx.types,
                this_type,
            ) {
                this_type = self.evaluate_type_with_env(this_type);
            }
            self.ctx.this_type_stack.push(this_type);
        }
        // TypeScript 7 dropped JS constructor-function inference, so an object
        // literal assigned to `M.prototype` no longer borrows `M`'s synthesized
        // instance type as the `this` of its methods. Such a method is an
        // ordinary object-literal method: its `this` is the literal itself, the
        // same as `var o = { m() { ... } }`. tsc 7.0.2 reports
        // `Property '_map' does not exist on type '{ get(key: any): any; }'`
        // for `M.prototype = { get(key) { return this._map[key] } }`, naming the
        // literal rather than `M`.
        let contextual_receiver_this_type =
            self.contextual_object_receiver_this_type(contextual_type, marker_this_type);
        let base_request = request.contextual_opt(contextual_type);
        let partial_initializer_stack_index = self
            .object_literal_variable_initializer_symbol(idx)
            .map(|variable_symbol| {
                self.ctx
                    .object_literal_tracking
                    .partial_initializers
                    .push(PartialObjectLiteralInitializer::new(variable_symbol, idx));
                self.ctx.object_literal_tracking.partial_initializers.len() - 1
            });

        ObjectLiteralRequestFacts {
            contextual_type,
            original_contextual_type,
            all_properties_context_sensitive,
            obj_getter_names,
            marker_this_type,
            contextual_receiver_this_type,
            base_request,
            partial_initializer_stack_index,
        }
    }

    /// The `ThisType[ T ]` marker owned by an *enclosing* object literal, found
    /// by walking outward from `literal_idx` through property assignments.
    ///
    /// `tsc`'s `getContextualThisParameter` does not stop at the literal that
    /// directly contains the member. When that literal's own contextual type
    /// carries no marker it re-reads the contextual type of the enclosing
    /// literal, and keeps climbing for as long as the current literal sits in a
    /// `PropertyAssignment`:
    ///
    /// ```text
    /// while (type) {
    ///     const thisType = getThisTypeFromContextualType(type);
    ///     if (thisType) { return ...; }
    ///     if (literal.parent.kind !== SyntaxKind.PropertyAssignment) break;
    ///     literal = literal.parent.parent;
    ///     type = getApparentTypeOfContextualType(literal);
    /// }
    /// ```
    ///
    /// That climb is what makes the Vue options shape work: in
    /// `new Vue({ methods: { f() { return this.x; } } })` with
    /// `VueOptions[ D, M, P ] = ThisType[ D & M & P ] & { methods?: M; ... }`,
    /// the inner `methods` literal is contextually typed by bare `M` — the
    /// marker lives one level out, on the options literal. The same applies to
    /// the `Object.defineProperties` shape, where
    /// `PropDescMap[ U ] & ThisType[ T ]` types the outer descriptor map and
    /// each per-key descriptor literal only sees `PropDesc[ U ]`.
    ///
    /// The enclosing literal is always computed before its property
    /// initializers, so its (nullish-stripped, discriminant-narrowed)
    /// contextual type is already recorded in
    /// `object_literal_tracking.contextual_targets`; an ancestor with no
    /// recorded contextual type ends the walk, mirroring the `while (type)`
    /// condition.
    fn enclosing_object_literal_this_type_marker(
        &mut self,
        literal_idx: NodeIndex,
    ) -> Option<TypeId> {
        let mut literal_idx = literal_idx;
        loop {
            let property_idx = self.ctx.arena.get_extended(literal_idx)?.parent;
            if self.ctx.arena.get(property_idx)?.kind != syntax_kind_ext::PROPERTY_ASSIGNMENT {
                return None;
            }
            let enclosing_idx = self.ctx.arena.get_extended(property_idx)?.parent;
            if self.ctx.arena.get(enclosing_idx)?.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            {
                return None;
            }
            let enclosing_contextual_type = *self
                .ctx
                .object_literal_tracking
                .contextual_targets
                .get(&enclosing_idx)?;
            if let Some(marker) = self.contextual_this_type_from_marker(enclosing_contextual_type) {
                return Some(marker);
            }
            literal_idx = enclosing_idx;
        }
    }
}
