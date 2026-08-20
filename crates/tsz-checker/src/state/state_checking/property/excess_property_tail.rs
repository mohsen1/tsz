//! Excess-property checking helpers for union and nested-literal targets.
//!
//! Split out of the parent module to satisfy the source-file line cap.

use super::*;

impl<'a> CheckerState<'a> {
    pub(super) fn excess_property_union_target(
        &self,
        target: TypeId,
        resolved_target: TypeId,
    ) -> TypeId {
        if query::union_members(self.ctx.types, resolved_target).is_some() {
            return resolved_target;
        }

        let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, target)
        else {
            return resolved_target;
        };
        let Some(def) = self.ctx.definition_store.get(def_id) else {
            return resolved_target;
        };
        if def.kind != tsz_solver::def::DefKind::TypeAlias || !def.type_params.is_empty() {
            return resolved_target;
        }
        let Some(body) = self.ctx.get_semantic_def_body(def_id).or(def.body) else {
            return resolved_target;
        };
        if query::union_members(self.ctx.types, body).is_some() {
            body
        } else {
            resolved_target
        }
    }

    /// `true` when `lazy_candidate` is a `Lazy(DefId)` whose definition is one of
    /// `outer_members`. Three lookups cover the different TypeId forms intersection
    /// members can take (Lazy, resolved body, or registered canonical TypeId).
    pub(super) fn lazy_def_is_recursive_member(
        &self,
        lazy_candidate: TypeId,
        outer_members: &[TypeId],
    ) -> bool {
        let Some(def_id) =
            crate::query_boundaries::common::lazy_def_id(self.ctx.types, lazy_candidate)
        else {
            return false;
        };
        if outer_members.contains(&lazy_candidate) {
            return true;
        }
        let body = self.ctx.get_semantic_def_body(def_id);
        outer_members.iter().any(|&m| {
            body == Some(m)
                || self.ctx.definition_store.find_def_for_type(m) == Some(def_id)
                || self.ctx.definition_store.find_type_alias_by_body(m) == Some(def_id)
        })
    }

    /// `true` when `candidate` is the resolved body of a Lazy intersection member.
    /// Property-access resolution can substitute `Recursive(0)` with the concrete body,
    /// so we must check resolved forms too; only Lazy members are scanned to avoid
    /// treating the literal extra-properties arm as a recursive self-reference.
    pub(super) fn resolved_lazy_is_recursive_member(
        &mut self,
        candidate: TypeId,
        outer_members: &[TypeId],
    ) -> bool {
        outer_members.iter().any(|&m| {
            crate::query_boundaries::common::lazy_def_id(self.ctx.types, m).is_some()
                && self.resolve_type_for_property_access(m) == candidate
        })
    }

    /// `true` when `candidate` is a recursive self-reference: a `Lazy(DefId)` member,
    /// a De Bruijn `Recursive(n)` (type aliases only; interfaces keep `Lazy`),
    /// or the resolved body of a Lazy member.
    pub(super) fn is_recursive_self_reference(
        &mut self,
        candidate: TypeId,
        outer_members: &[TypeId],
    ) -> bool {
        self.lazy_def_is_recursive_member(candidate, outer_members)
            || crate::query_boundaries::type_predicates::is_recursive_type_reference(
                self.ctx.types,
                candidate,
            )
            || self.resolved_lazy_is_recursive_member(candidate, outer_members)
    }

    /// Widen `nested_target` before a nested excess-property check, using
    /// two complementary paths:
    /// 1. If `target` (or its Lazy body) is an intersection, widen to the full
    ///    intersection when `nested_target` is a recursive self-reference.
    /// 2. Otherwise (`normalize_intersection` already merged the intersection into
    ///    `effective_target`), widen when `nested_target` is a `Lazy(DefId)` whose
    ///    body is a strict sub-shape of `effective_target`.
    pub(super) fn widen_nested_target_for_property(
        &mut self,
        nested_target: TypeId,
        target: TypeId,
        effective_target: TypeId,
    ) -> TypeId {
        if let Some(outer) = self.recover_outer_intersection_from_target(target) {
            self.widen_nested_target_if_recursive(nested_target, outer)
        } else {
            self.widen_nested_target_if_sub_shape(nested_target, effective_target)
        }
    }

    /// Widen `nested_target` to `outer_intersection` when `nested_target` is a
    /// recursive self-reference inside the intersection.
    ///
    /// Structural rule: for `Rec & Extra`, a fresh object literal assigned to a
    /// recursive property of `Rec` (type `Lazy(Rec_DefId)`) must be validated
    /// against the whole intersection, suppressing false TS2353 errors for properties
    /// contributed by `Extra`. Also widens `Lazy | undefined` to
    /// `outer_intersection | undefined` for optional recursive properties.
    pub(super) fn widen_nested_target_if_recursive(
        &mut self,
        nested_target: TypeId,
        outer_intersection: TypeId,
    ) -> TypeId {
        let Some(outer_members) = query::intersection_members(self.ctx.types, outer_intersection)
        else {
            return nested_target;
        };

        if self.is_recursive_self_reference(nested_target, &outer_members) {
            return outer_intersection;
        }

        // Optional recursive property: `Union([Lazy(Rec_DefId), undefined])`.
        if let Some((candidate, true)) = self.single_non_undefined_member(nested_target)
            && self.is_recursive_self_reference(candidate, &outer_members)
        {
            return query::optional_excess_property_nested_target(
                self.ctx.types,
                outer_intersection,
            );
        }

        nested_target
    }

    pub(super) fn single_non_undefined_member(&self, type_id: TypeId) -> Option<(TypeId, bool)> {
        let Some(members) = query::union_members(self.ctx.types, type_id) else {
            return (type_id != TypeId::UNDEFINED).then_some((type_id, false));
        };
        let has_undefined = members.contains(&TypeId::UNDEFINED);
        let mut non_undef = members.into_iter().filter(|&m| m != TypeId::UNDEFINED);
        match (non_undef.next(), non_undef.next()) {
            (Some(member), None) => Some((member, has_undefined)),
            _ => None,
        }
    }

    /// Returns the `TypeId` of an enclosing intersection reachable from `target`:
    /// the type itself, its `Lazy` body, or an intersection member of a union wrapper.
    pub(super) fn recover_outer_intersection_from_target(&self, target: TypeId) -> Option<TypeId> {
        let is_intersection = |t: TypeId| query::intersection_members(self.ctx.types, t).is_some();
        if is_intersection(target) {
            return Some(target);
        }
        if let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, target)
            && let Some(body) = self.ctx.get_semantic_def_body(def_id)
            && is_intersection(body)
        {
            return Some(body);
        }
        query::union_members(self.ctx.types, target)?
            .into_iter()
            .find(|&member| is_intersection(member))
    }

    /// Widen `nested_target` to `outer_object` when `normalize_intersection` merged a
    /// `Rec & Extra` type alias into a single Object but the recursive property still
    /// carries `Lazy(Rec_DefId)`. Only fires when `Lazy`'s body is self-referential
    /// (the body itself contains a property that refers back to `Rec_DefId`), to avoid
    /// suppressing valid TS2353 errors for non-recursive types whose properties happen
    /// to be a name-subset of the outer object.
    pub(super) fn widen_nested_target_if_sub_shape(
        &mut self,
        nested_target: TypeId,
        outer_object: TypeId,
    ) -> TypeId {
        let Some(_outer_shape) = query::object_shape(self.ctx.types, outer_object) else {
            return nested_target;
        };

        let Some((candidate, has_undef)) = self.single_non_undefined_member(nested_target) else {
            return nested_target;
        };

        let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, candidate)
        else {
            return nested_target;
        };

        let Some(body) = self.ctx.get_semantic_def_body(def_id) else {
            return nested_target;
        };

        let Some(body_shape) = query::object_shape(self.ctx.types, body) else {
            return nested_target;
        };

        // Only widen for genuinely recursive types: the body must contain at least
        // one property whose type directly references `def_id` (as `Lazy(def_id)`,
        // `Recursive(n)`, or a union including those).  Non-recursive aliases whose
        // property names happen to be a subset of `outer_object` must NOT be widened,
        // or we suppress valid TS2353 errors across hundreds of unrelated tests.
        let is_recursive = body_shape
            .properties
            .iter()
            .any(|prop| self.type_directly_references_def(prop.type_id, def_id));
        if !is_recursive {
            return nested_target;
        }

        let outer_props = _outer_shape.properties.as_slice();
        let body_props = body_shape.properties.as_slice();

        // Require strict subset: body has fewer props AND all body props exist in outer.
        if body_props.len() >= outer_props.len() {
            return nested_target;
        }
        if !body_props
            .iter()
            .all(|bp| outer_props.iter().any(|op| op.name == bp.name))
        {
            return nested_target;
        }

        if has_undef {
            query::optional_excess_property_nested_target(self.ctx.types, outer_object)
        } else {
            outer_object
        }
    }

    /// `true` when `type_id` directly encodes a reference back to `target_def_id`:
    /// a `Lazy(target_def_id)`, a De Bruijn `Recursive(n)` index (used by structural
    /// type-alias self-references), or a union/optional wrapper of either.
    pub(super) fn type_directly_references_def(
        &self,
        type_id: TypeId,
        target_def_id: tsz_solver::def::DefId,
    ) -> bool {
        if let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, type_id)
        {
            return def_id == target_def_id;
        }
        if crate::query_boundaries::type_predicates::is_recursive_type_reference(
            self.ctx.types,
            type_id,
        ) {
            return true;
        }
        if let Some(members) = query::union_members(self.ctx.types, type_id) {
            return members
                .iter()
                .any(|&m| self.type_directly_references_def(m, target_def_id));
        }
        false
    }

    pub(crate) fn excess_property_target_from_type_annotation(
        &mut self,
        type_node: NodeIndex,
    ) -> Option<TypeId> {
        let mut visited = HashSet::new();
        self.excess_property_annotation_union_type(type_node, &mut visited)
            .filter(|&ty| query::union_members(self.ctx.types, ty).is_some())
    }

    pub(super) fn excess_property_annotation_union_type(
        &mut self,
        type_node: NodeIndex,
        visited: &mut HashSet<NodeIndex>,
    ) -> Option<TypeId> {
        if !visited.insert(type_node) {
            return None;
        }

        let node = self.ctx.arena.get(type_node)?;
        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE {
            let wrapped = self.ctx.arena.get_wrapped_type(node)?;
            return self.excess_property_annotation_union_type(wrapped.type_node, visited);
        }

        if node.kind == syntax_kind_ext::UNION_TYPE {
            let composite = self.ctx.arena.get_composite_type(node)?;
            let contains_intersection_member = composite
                .types
                .nodes
                .iter()
                .any(|&member| self.annotation_node_contains_intersection(member));
            if !contains_intersection_member {
                return None;
            }
            let member_types = composite
                .types
                .nodes
                .iter()
                .map(|&member| self.excess_property_annotation_component_type(member, visited))
                .collect::<Vec<_>>();
            return Some(query::excess_property_display_target_union(
                self.ctx.types,
                member_types,
            ));
        }

        if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            let alias_body = self.local_non_generic_type_alias_body_for_reference(type_node)?;
            return self.excess_property_annotation_union_type(alias_body, visited);
        }

        None
    }

    pub(super) fn excess_property_annotation_component_type(
        &mut self,
        type_node: NodeIndex,
        visited: &mut HashSet<NodeIndex>,
    ) -> TypeId {
        let Some(node) = self.ctx.arena.get(type_node) else {
            return TypeId::ERROR;
        };

        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            && let Some(wrapped) = self.ctx.arena.get_wrapped_type(node)
        {
            return self.excess_property_annotation_component_type(wrapped.type_node, visited);
        }

        if node.kind == syntax_kind_ext::INTERSECTION_TYPE
            && let Some(composite) = self.ctx.arena.get_composite_type(node)
        {
            let member_types = composite
                .types
                .nodes
                .iter()
                .map(|&member| self.excess_property_annotation_component_type(member, visited))
                .collect::<Vec<_>>();
            return query::excess_property_annotation_intersection_or_single(
                self.ctx.types,
                member_types,
            );
        }

        if let Some(union_type) = self.excess_property_annotation_union_type(type_node, visited) {
            return union_type;
        }

        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(lazy_type) = self.named_type_reference_lazy_type(type_node)
        {
            return lazy_type;
        }

        self.get_type_from_type_node(type_node)
    }

    pub(super) fn annotation_node_contains_intersection(&self, type_node: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(type_node) else {
            return false;
        };
        if node.kind == syntax_kind_ext::INTERSECTION_TYPE {
            return true;
        }
        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            && let Some(wrapped) = self.ctx.arena.get_wrapped_type(node)
        {
            return self.annotation_node_contains_intersection(wrapped.type_node);
        }
        false
    }

    pub(super) fn named_type_reference_lazy_type(
        &mut self,
        type_node: NodeIndex,
    ) -> Option<TypeId> {
        let node = self.ctx.arena.get(type_node)?;
        let type_ref = self.ctx.arena.get_type_ref(node)?;
        if type_ref.type_arguments.is_some() {
            return None;
        }
        let TypeSymbolResolution::Type(sym_id) =
            self.resolve_identifier_symbol_in_type_position_without_tracking(type_ref.type_name)
        else {
            return None;
        };
        let def_id = self.ctx.get_or_create_def_id(sym_id);
        let _ = self.get_type_from_type_node(type_node);
        Some(self.ctx.types.lazy(def_id))
    }

    pub(crate) fn local_non_generic_type_alias_body_for_reference(
        &self,
        type_node: NodeIndex,
    ) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(type_node)?;
        let type_ref = self.ctx.arena.get_type_ref(node)?;
        if type_ref.type_arguments.is_some() {
            return None;
        }
        let TypeSymbolResolution::Type(sym_id) =
            self.resolve_identifier_symbol_in_type_position_without_tracking(type_ref.type_name)
        else {
            return None;
        };
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS) {
            return None;
        }
        symbol.declarations.iter().copied().find_map(|decl_idx| {
            let decl_node = self.ctx.arena.get(decl_idx)?;
            if decl_node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                return None;
            }
            let type_alias = self.ctx.arena.get_type_alias(decl_node)?;
            if type_alias.type_parameters.is_some() {
                return None;
            }
            Some(type_alias.type_node)
        })
    }

    /// Boolean query: does a fresh source have any excess properties relative to
    /// the target?
    ///
    /// Delegates to the canonical `classify_object_properties` boundary function
    /// For fresh object literals assigned to discriminated union targets, detect
    /// the discriminant member and emit TS2353 for excess properties against that
    /// specific member. Returns `true` if excess properties were found and
    /// reported, meaning the caller should skip the regular assignability error.
    ///
    /// This matches tsc behavior: `{ kind: "sq", x: 12 }` assigned to
    /// `Square | Rectangle` where Square = { kind: "sq", size: number }
    /// reports "'x' does not exist in type 'Square'" (not a generic TS2322).
    pub(crate) fn try_discriminated_union_excess_check(
        &mut self,
        source: TypeId,
        target: TypeId,
        obj_literal_idx: NodeIndex,
    ) -> bool {
        use crate::query_boundaries::common as freshness_query;

        let is_fresh_source = freshness_query::is_fresh_object_type(self.ctx.types, source);
        let explicit_property_names = if is_fresh_source {
            None
        } else {
            self.explicit_object_literal_property_names_for_spread(obj_literal_idx)
        };

        if !is_fresh_source && explicit_property_names.is_none() {
            return false;
        }

        let Some(source_shape) = query::object_shape(self.ctx.types, source) else {
            return false;
        };

        let resolved_target = self.resolve_type_for_property_access(target);
        let resolved_target = self.excess_property_union_target(target, resolved_target);
        let Some(members) = query::union_members(self.ctx.types, resolved_target) else {
            return false;
        };

        // Collect resolved shapes for each union member, along with the original
        // TypeId (for error message formatting) which preserves type alias names.
        //
        // IMPORTANT: Use `member` directly as display_id, not `original_members[i]`.
        // Resolution can change the order of union members (e.g., during normalization),
        // so index-based lookup into original_members would cause misalignment between
        // display names and shapes. Using `member` preserves the Lazy reference if the
        // union member is a type alias, giving proper display names like "Int" vs "{ type: ... }".
        //
        // When a union member is an intersection of object types
        // (e.g. `Common & A` or `BaseAttribute<string> & { type: 'string' }`),
        // `query::object_shape` returns None because intersections are not
        // backed by a single shape. Fall back to a solver-flattened apparent
        // shape so the discriminant narrowing and excess-property check can
        // see the merged property set, matching tsc's apparent-type behavior.
        let mut member_shapes: Vec<(TypeId, std::sync::Arc<tsz_solver::ObjectShape>)> = Vec::new();
        for &member in members.iter() {
            let resolved = self.resolve_type_for_property_access(member);
            if let Some(shape) = query::object_shape(self.ctx.types, resolved) {
                // Use member directly - it preserves Lazy wrappers for named types
                member_shapes.push((member, shape));
            } else if let Some(shape) = self.apparent_intersection_object_shape(resolved) {
                member_shapes.push((member, shape));
            }
        }

        if member_shapes.is_empty() {
            return false;
        }

        // Find a source property with a unit type that matches exactly one member
        let source_props = source_shape.properties.as_slice();
        let union_shapes: Vec<_> = member_shapes
            .iter()
            .map(|(_, shape)| shape.clone())
            .collect();
        let matching_indices = self
            .discriminant_matching_union_member_indices(
                obj_literal_idx,
                source_props,
                &union_shapes,
                explicit_property_names.as_ref(),
            )
            .unwrap_or_default();

        if matching_indices.is_empty() {
            return false;
        }

        // Display the union of *all* members the discriminator narrowed to (tsc
        // shows e.g. `'StringAttribute | OneToOneAttribute'`, not just the first
        // matching member). The excess-property existence check still considers
        // every narrowed member so we don't false-positive on properties that
        // belong to one of the matches.
        let narrowed_member_types: Vec<TypeId> = matching_indices
            .iter()
            .map(|&i| member_shapes[i].0)
            .collect();
        let display_target = if narrowed_member_types.len() == 1 {
            narrowed_member_types[0]
        } else {
            query::excess_property_display_target_union(
                self.ctx.types,
                narrowed_member_types.clone(),
            )
        };
        let narrowed_shapes: Vec<&std::sync::Arc<tsz_solver::ObjectShape>> = matching_indices
            .iter()
            .map(|&i| &member_shapes[i].1)
            .collect();

        // Collect excess properties (not in any narrowed member) with their AST
        // positions. tsc reports only the first excess property in source order.
        let mut excess_candidates: Vec<(tsz_common::interner::Atom, NodeIndex, u32)> = Vec::new();
        for source_prop in source_props {
            if explicit_property_names.is_some()
                && !explicit_property_names
                    .as_ref()
                    .is_some_and(|names| names.contains(&source_prop.name))
            {
                continue;
            }

            let exists_in_narrowed = narrowed_shapes
                .iter()
                .any(|shape| shape.properties.iter().any(|p| p.name == source_prop.name));

            if !exists_in_narrowed {
                let report_idx = self
                    .find_object_literal_property_element(obj_literal_idx, source_prop.name)
                    .unwrap_or(obj_literal_idx);
                let pos = self
                    .ctx
                    .arena
                    .get(report_idx)
                    .map(|n| n.pos)
                    .unwrap_or(u32::MAX);
                excess_candidates.push((source_prop.name, report_idx, pos));
            }
        }

        // Report the first excess property by source position (earliest in file)
        if let Some(earliest) = excess_candidates.iter().min_by_key(|c| c.2) {
            let prop_name = self.object_literal_property_display_name(
                earliest.1,
                self.ctx.types.resolve_atom(earliest.0).as_ref(),
            );
            // Use the multi-member union only for the diagnostic message text
            // (display_target). For implicit-any initializer checking we still
            // pass the first-matching member to keep contextual typing on the
            // same path as the single-narrowed case.
            self.error_excess_property_at(&prop_name, display_target, earliest.1);
            self.check_excess_property_initializer_implicit_any(
                earliest.1,
                narrowed_member_types[0],
            );
            true
        } else {
            false
        }
    }

    pub(super) fn discriminant_matching_union_member_indices(
        &mut self,
        obj_literal_idx: NodeIndex,
        source_props: &[tsz_solver::PropertyInfo],
        union_shapes: &[std::sync::Arc<tsz_solver::ObjectShape>],
        explicit_property_names: Option<&HashSet<Atom>>,
    ) -> Option<Vec<usize>> {
        let direct_discriminants =
            self.object_literal_direct_unit_discriminants(obj_literal_idx, explicit_property_names);

        // Apply all discriminants sequentially to progressively narrow the set of
        // matching union members. This matches tsc's behavior for objects with
        // multiple discriminant properties like `{ p1: 'left', p2: false }` against
        // `{ p1: 'left'; p2: true; p3: number } | { p1: 'right'; p2: false; p4: string } | { p1: 'left'; p2: boolean }`:
        // - `p1: 'left'` narrows to members [0, 2]
        // - `p2: false` further narrows [0, 2] to [2] (member 0 has p2: true, not assignable)
        // Result: only member 2 is applicable, and excess property check uses that
        // narrowed set for the error message.
        //
        // A source property is treated as a discriminator when the *target union*
        // exposes the property as a tsc-style discriminant property
        // (`CheckFlags.Discriminant` = `HasLiteralType | HasNonUniformType`):
        //   - it must occur in at least one member,
        //   - the collected property types must contain at least one unit type, and
        //   - the collected property types must differ across members.
        // Once that holds, members that lack the property are filtered out — this
        // mirrors tsc's `discriminateTypeByDiscriminableItems` where
        // `getTypeOfPropertyOfType` returns undefined for missing properties and
        // the member is dropped from the candidate set.
        let mut active_indices: Vec<usize> = (0..union_shapes.len()).collect();
        let mut did_narrow = false;

        for (prop_name, prop_type) in direct_discriminants {
            if source_props.iter().all(|prop| prop.name != prop_name) {
                continue;
            }

            // Collect (member_index, target_prop_type) for the FULL union, so the
            // discriminator decision is based on the union shape rather than the
            // currently-narrowed set (which can shrink to a single member during
            // iteration but should still treat the original union as the reference
            // for "is this a discriminant property?").
            let mut full_members_with_prop: Vec<(usize, TypeId)> = Vec::new();
            for (i, shape) in union_shapes.iter().enumerate() {
                if let Some(target_prop) = shape.properties.iter().find(|p| p.name == prop_name) {
                    full_members_with_prop.push((i, target_prop.type_id));
                }
            }

            if full_members_with_prop.is_empty() {
                continue;
            }

            // tsc's discriminant requires at least one unit/literal type and
            // non-uniform types across the occurrences. Without those, narrowing
            // by the property would risk over-eliminating union members whose
            // shape only happened to differ in non-discriminator slots (for
            // example `{ a: 1, first: string } | { a: 2, second: string }` where
            // `first` is not a discriminant, only `a` is).
            // Evaluate target property types so intersections like
            // `(string | undefined) & 'string'` simplify to the unit literal
            // before the discriminator decision. Without this, a target type
            // like `BaseAttribute<string> & { type: 'string' }` keeps the raw
            // intersection shape on its `type` property and `is_unit_type`
            // returns false, even though the property is in fact a unit
            // literal — matching tsc's apparent-type behavior on intersections.
            let evaluated_full_members_with_prop: Vec<(usize, TypeId)> = full_members_with_prop
                .iter()
                .map(|&(i, ty)| (i, self.evaluate_type_with_env(ty)))
                .collect();
            let any_unit = evaluated_full_members_with_prop
                .iter()
                .any(|(_, ty)| query::is_unit_type(self.ctx.types, *ty));
            if !any_unit {
                continue;
            }
            let first_ty = evaluated_full_members_with_prop[0].1;
            let non_uniform = evaluated_full_members_with_prop
                .iter()
                .any(|(_, ty)| *ty != first_ty);
            if !non_uniform {
                continue;
            }

            // Filter active members: drop members whose target type does not
            // accept the source unit value. Members that *lack* the property
            // are kept as candidates — tsc's `discriminateTypeByDiscriminableItems`
            // treats missing properties on union members as compatible at the
            // discriminator step (the missing property becomes an excess
            // candidate downstream). Previously this filter dropped lacking
            // members too, which caused TS2353 to be missed for object literals
            // like `{ subkind: 1, kind: "b" }` against
            // `{ kind: "a"; subkind: 0|1; … } | { kind: "b" }` — kind narrows
            // to member 2, but tsz also dropped member 2 on the `subkind`
            // pass because it lacks subkind, leaving no narrowed member and
            // skipping excess emission entirely. See `compiler/missingDiscriminants*.ts`.
            // A discriminator only participates when its source value
            // positively matches at least one still-active member that
            // declares the property. tsc's `discriminateTypeByDiscriminableItems`
            // tracks a per-discriminator `matched` flag and REVERTS the pass
            // when no constituent matched (`Ternary.Maybe` back to included):
            // a failing unit literal that names no arm must not narrow the
            // union — it is an ordinary property-type mismatch for the
            // relation to report, not a discriminant. Without this revert,
            // `{ p: 1, q: 8 }` against `{ p: 1; q: 4 } | { p: 2; q: 8 } | Box`
            // lets the failing `q: 8` pass drop the `p: 1`-matched arm and pin
            // `Box` alone, misrouting a plain TS2345/TS2322 into a TS2353
            // ("'p' does not exist in type 'Box'").
            let mut any_positive_match = false;
            let candidate: Vec<usize> = active_indices
                .iter()
                .copied()
                .filter(|&i| {
                    evaluated_full_members_with_prop
                        .iter()
                        .find(|(idx, _)| *idx == i)
                        .is_none_or(|(_, target_ty)| {
                            let related = self
                                .diagnostic_subtype_outcome(prop_type, *target_ty)
                                .related;
                            any_positive_match |= related;
                            related
                        })
                })
                .collect();

            // Mirror tsc's `if (!candidate.length) break;` in
            // `discriminateTypeByDiscriminableItems`: when this discriminator
            // produces an empty candidate set, abandon discriminator-based
            // narrowing entirely. The previous `continue` (keeping the
            // pre-filter set and trying the next discriminator) lets a later
            // discriminator narrow over the *unfiltered* `active_indices` and
            // over-narrow the result. Example: source `{a:3, c:10, ...}`
            // against `{a:1,c:10,...}|{a:1,c:20,...}|{a:2,c:30,...}` —
            // `a=3` produces an empty candidate set, then `c=10` would
            // falsely pin the first member alone (TS2353 false positive).
            if candidate.is_empty() {
                return None;
            }

            // The revert itself: some active member survived (it lacks the
            // property), but no member positively matched the source value —
            // skip this discriminator instead of letting the lacking members
            // alone define the narrowed set.
            if !any_positive_match {
                continue;
            }

            if candidate.len() < active_indices.len() {
                did_narrow = true;
            }
            active_indices = candidate;
        }

        if did_narrow && active_indices.len() < union_shapes.len() {
            Some(active_indices)
        } else {
            None
        }
    }

    /// Build a flat object shape for an intersection of object types by
    /// collecting all member properties via the solver's intersection
    /// property collector. Returns `None` for non-intersection types or
    /// when no object-like properties can be collected.
    ///
    /// This is the boundary that lets discriminated-union excess-property
    /// checking see intersection union members (e.g. `Common & A`,
    /// `BaseAttribute<string> & { type: 'string' }`) as if they were
    /// flat object shapes, matching tsc's apparent-type behavior.
    pub(super) fn apparent_intersection_object_shape(
        &self,
        type_id: TypeId,
    ) -> Option<std::sync::Arc<tsz_solver::ObjectShape>> {
        // Only fall back for intersections; other shapes are handled by the
        // direct `query::object_shape` path on the call site.
        query::intersection_members(self.ctx.types, type_id)?;
        crate::query_boundaries::intersection_display::collected_properties_object_shape(
            self.ctx.types,
            &self.ctx,
            type_id,
        )
    }

    pub(super) fn try_emit_nested_discriminated_union_assignability_error(
        &mut self,
        outer_source: TypeId,
        outer_target: TypeId,
        obj_literal_idx: NodeIndex,
        prop_name: Atom,
        nested_target: TypeId,
    ) -> bool {
        if !self.is_object_like_nested_target(nested_target) {
            return false;
        }

        let literal_idx = if self
            .ctx
            .arena
            .get(obj_literal_idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
        {
            obj_literal_idx
        } else {
            let Some(literal_idx) = self.find_rhs_object_literal(obj_literal_idx) else {
                return false;
            };
            literal_idx
        };

        let Some((report_idx, value_idx)) =
            self.object_literal_property_name_and_value(literal_idx, prop_name)
        else {
            return false;
        };
        let effective_value_idx = self.ctx.arena.skip_parenthesized(value_idx);
        let Some(value_node) = self.ctx.arena.get(effective_value_idx) else {
            return false;
        };
        if value_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }

        let Some(rejected_property) =
            self.nested_literal_rejected_fresh_property(effective_value_idx, nested_target)
        else {
            return false;
        };

        let source_str = self.format_type(outer_source);
        let target_str = self.format_type(outer_target);
        let message = crate::diagnostics::format_message(
            crate::diagnostics::diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, &target_str],
        );
        if let Some((start, length)) =
            self.find_excess_property_anchor(effective_value_idx, rejected_property)
        {
            self.error(
                start,
                length,
                message,
                crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        } else {
            self.error_at_anchor(
                report_idx,
                crate::error_reporter::DiagnosticAnchorKind::PropertyToken,
                &message,
                crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }
        true
    }

    pub(super) fn is_object_like_nested_target(&mut self, nested_target: TypeId) -> bool {
        let nested_target = self.evaluate_type_with_env(nested_target);
        let nested_target = self.resolve_type_for_property_access(nested_target);

        if query::object_shape(self.ctx.types, nested_target).is_some() {
            return true;
        }

        let resolved_target = self.prune_impossible_object_union_members_with_env(nested_target);
        query::union_members(self.ctx.types, resolved_target).is_some_and(|members| {
            members.iter().any(|member| {
                let resolved_member = self.resolve_type_for_property_access(*member);
                query::object_shape(self.ctx.types, resolved_member).is_some()
            })
        })
    }

    pub(super) fn nested_literal_rejected_fresh_property(
        &mut self,
        nested_literal_idx: NodeIndex,
        nested_target: TypeId,
    ) -> Option<Atom> {
        let nested_target = self.evaluate_type_with_env(nested_target);
        let nested_target = self.resolve_type_for_property_access(nested_target);
        let resolved_target = self.prune_impossible_object_union_members_with_env(nested_target);
        let members = query::union_members(self.ctx.types, resolved_target)?;

        let mut target_shapes = Vec::new();
        for &member in &members {
            let resolved_member = self.resolve_type_for_property_access(member);
            let shape = query::object_shape(self.ctx.types, resolved_member)?;
            target_shapes.push(shape);
        }
        if target_shapes.is_empty() {
            return None;
        }

        let nested_node = self.ctx.arena.get(nested_literal_idx)?;
        let nested_literal = self.ctx.arena.get_literal_expr(nested_node)?;
        for &elem_idx in &nested_literal.elements.nodes {
            let elem_node = self.ctx.arena.get(elem_idx)?;
            let prop_name = match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let prop = self.ctx.arena.get_property_assignment(elem_node)?;
                    self.get_property_name(prop.name)
                }
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    let prop = self.ctx.arena.get_shorthand_property(elem_node)?;
                    self.get_property_name(prop.name)
                }
                _ => None,
            };
            let Some(prop_name) = prop_name.map(|name| self.ctx.types.intern_string(&name)) else {
                continue;
            };
            let accepted_by_target = target_shapes.iter().any(|shape| {
                shape
                    .properties
                    .iter()
                    .any(|target_prop| target_prop.name == prop_name)
                    || shape.string_index.is_some()
                    || (shape.number_index.is_some()
                        && tsz_solver::utils::is_numeric_literal_name(
                            &self.ctx.types.resolve_atom(prop_name),
                        ))
            });
            if !accepted_by_target {
                return Some(prop_name);
            }
        }

        None
    }

    pub(in crate::state_domain::state_checking) fn object_literal_property_name_and_value(
        &self,
        obj_literal_idx: NodeIndex,
        prop_name: Atom,
    ) -> Option<(NodeIndex, NodeIndex)> {
        let obj_node = self.ctx.arena.get(obj_literal_idx)?;
        let obj_lit = self.ctx.arena.get_literal_expr(obj_node)?;

        for &elem_idx in obj_lit.elements.nodes.iter().rev() {
            let elem_node = self.ctx.arena.get(elem_idx)?;
            match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let prop = self.ctx.arena.get_property_assignment(elem_node)?;
                    let elem_prop_name = self
                        .get_property_name(prop.name)
                        .map(|name| self.ctx.types.intern_string(&name));
                    if elem_prop_name == Some(prop_name) {
                        return Some((prop.name, prop.initializer));
                    }
                }
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    let prop = self.ctx.arena.get_shorthand_property(elem_node)?;
                    let elem_prop_name = self
                        .get_property_name(prop.name)
                        .map(|name| self.ctx.types.intern_string(&name));
                    if elem_prop_name == Some(prop_name) {
                        return Some((prop.name, prop.name));
                    }
                }
                _ => {}
            }
        }

        None
    }

    pub(super) fn object_literal_direct_unit_discriminants(
        &mut self,
        obj_literal_idx: NodeIndex,
        explicit_property_names: Option<&HashSet<Atom>>,
    ) -> Vec<(Atom, TypeId)> {
        let Some(obj_node) = self.ctx.arena.get(obj_literal_idx) else {
            return Vec::new();
        };
        let Some(obj_lit) = self.ctx.arena.get_literal_expr(obj_node) else {
            return Vec::new();
        };

        let mut discriminants = Vec::new();
        for &elem_idx in &obj_lit.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) else {
                continue;
            };
            let Some(prop_name) = self.get_property_name_resolved(prop.name) else {
                continue;
            };
            let prop_atom = self.ctx.types.intern_string(&prop_name);
            if explicit_property_names.is_some_and(|names| !names.contains(&prop_atom)) {
                continue;
            }
            // Prefer the purely-syntactic literal value (string/number/bigint/
            // boolean/`undefined`/const-assertion). When that misses — most
            // commonly an enum member reference like `Kind.A`, a
            // PropertyAccessExpression the syntactic query does not resolve —
            // fall back to the node's computed type, which yields the *nominal*
            // enum-literal unit type (`Kind.A`) that matches the target union
            // member's discriminant. Both paths are gated on `is_unit_type`, so
            // non-discriminant initializers (e.g. `obj.x: number`) are ignored
            // and the precise-excess path is not entered for them.
            let lit_type = self
                .literal_type_from_initializer(prop.initializer)
                .filter(|&t| query::is_unit_type(self.ctx.types, t))
                .or_else(|| {
                    let computed = self.get_type_of_node(prop.initializer);
                    query::is_unit_type(self.ctx.types, computed).then_some(computed)
                });
            let Some(lit_type) = lit_type else {
                continue;
            };
            discriminants.push((prop_atom, lit_type));
        }

        discriminants
    }

    pub(super) fn narrow_union_target_by_object_literal_discriminants(
        &mut self,
        union_type: TypeId,
        obj_literal_idx: NodeIndex,
        explicit_property_names: Option<&HashSet<Atom>>,
    ) -> TypeId {
        let Some(members) = query::union_members(self.ctx.types, union_type) else {
            return union_type;
        };

        let direct_discriminants =
            self.object_literal_direct_unit_discriminants(obj_literal_idx, explicit_property_names);
        if direct_discriminants.is_empty() {
            return union_type;
        }

        for (prop_name, prop_type) in direct_discriminants {
            let mut matching_members = Vec::new();
            let mut fully_discriminated = true;

            for &member in &members {
                let resolved_member = self.resolve_type_for_property_access(member);
                let Some(shape) = query::object_shape(self.ctx.types, resolved_member) else {
                    fully_discriminated = false;
                    break;
                };
                let Some(prop) = shape.properties.iter().find(|prop| prop.name == prop_name) else {
                    fully_discriminated = false;
                    break;
                };
                if !query::is_unit_type(self.ctx.types, prop.type_id) {
                    fully_discriminated = false;
                    break;
                }
                if self
                    .diagnostic_subtype_outcome(prop_type, prop.type_id)
                    .related
                {
                    matching_members.push(member);
                }
            }

            if fully_discriminated
                && !matching_members.is_empty()
                && matching_members.len() < members.len()
            {
                return query::excess_property_target_union(self.ctx.types, matching_members);
            }
        }

        union_type
    }

    /// Detect whether an object shape represents the global `Object` or `Function`
    /// interface (or similar built-in prototypes).
    ///
    /// Delegates to the canonical boundary function
    /// `query_boundaries::assignability::is_global_object_or_function_shape`.
    pub(super) fn is_global_object_or_function_shape(
        &self,
        shape: &tsz_solver::ObjectShape,
    ) -> bool {
        crate::query_boundaries::assignability::is_global_object_or_function_shape_boundary(
            self.ctx.types,
            shape,
        )
    }

    pub(super) fn explicit_object_literal_property_names_for_spread(
        &self,
        obj_literal_idx: NodeIndex,
    ) -> Option<HashSet<Atom>> {
        let obj_node = self.ctx.arena.get(obj_literal_idx)?;
        if obj_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }

        let obj_lit = self.ctx.arena.get_literal_expr(obj_node)?;

        let has_spread = obj_lit.elements.nodes.iter().any(|&elem_idx| {
            self.ctx.arena.get(elem_idx).is_some_and(|elem_node| {
                elem_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT
                    || elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT
            })
        });
        if !has_spread {
            return None;
        }

        self.explicit_object_literal_property_names(obj_literal_idx)
    }

    pub(super) fn explicit_object_literal_property_names(
        &self,
        obj_literal_idx: NodeIndex,
    ) -> Option<HashSet<Atom>> {
        let obj_node = self.ctx.arena.get(obj_literal_idx)?;
        if obj_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }

        let obj_lit = self.ctx.arena.get_literal_expr(obj_node)?;

        let mut explicit_names = HashSet::new();
        for &elem_idx in &obj_lit.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            let elem_prop_name = match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_property_assignment(elem_node)
                    .and_then(|prop| self.get_property_name(prop.name)),
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_shorthand_property(elem_node)
                    .and_then(|prop| self.get_property_name(prop.name)),
                syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(elem_node)
                    .and_then(|method| self.get_property_name(method.name)),
                syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => self
                    .ctx
                    .arena
                    .get_accessor(elem_node)
                    .and_then(|accessor| self.get_property_name(accessor.name)),
                _ => None,
            };

            if let Some(name) = elem_prop_name {
                explicit_names.insert(self.ctx.types.intern_string(&name));
            }
        }

        Some(explicit_names)
    }

    pub(super) fn const_assertion_object_literal_expression(
        &self,
        idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let idx = self.ctx.arena.skip_parenthesized(idx);
        let node = self.ctx.arena.get(idx)?;
        if node.kind != syntax_kind_ext::AS_EXPRESSION
            && node.kind != syntax_kind_ext::TYPE_ASSERTION
        {
            return None;
        }
        let assertion = self.ctx.arena.get_type_assertion(node)?;
        if !self.is_const_assertion_type_node(assertion.type_node) {
            return None;
        }
        let expression_idx = self.ctx.arena.skip_parenthesized(assertion.expression);
        let expression = self.ctx.arena.get(expression_idx)?;
        if expression.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return Some(expression_idx);
        }
        None
    }

    /// Resolve an object-literal member's written name to the atom that the
    /// member's own *type* carries, so the nested excess-property walk can match
    /// an element by the same name its caller was handed.
    ///
    /// The syntactic resolver declines for a late-bound computed key (`[K]`
    /// where `const K = "door"`, `[E.A]`, `[S]`) — by design, since the written
    /// text names a variable, not a property. But the source type's member list
    /// carries the *resolved* name, and that list is where the caller's
    /// `prop_name` comes from, so matching on syntax alone silently skips every
    /// computed member and the walk never descends into its value.
    fn object_literal_member_name_atom(&mut self, name_idx: NodeIndex) -> Option<Atom> {
        if let Some(name) = self.get_property_name(name_idx) {
            return Some(self.ctx.types.intern_string(&name));
        }
        // Only a computed key can be late-bound; anything else that the
        // syntactic resolver declined has no name to recover.
        let name_node = self.ctx.arena.get(name_idx)?;
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }
        let resolved = self.get_property_name_resolved(name_idx)?;
        Some(self.ctx.types.intern_string(&resolved))
    }

    /// Check nested object literal properties for excess properties.
    ///
    /// This implements recursive excess property checking for nested object literals.
    /// For example, in `const p: { x: { y: number } } = { x: { y: 1, z: 2 } }`,
    /// the nested object literal `{ y: 1, z: 2 }` should be checked for excess property `z`.
    pub(super) fn check_nested_object_literal_excess_properties(
        &mut self,
        prop_name: tsz_common::interner::Atom,
        target_prop_type: Option<TypeId>,
        obj_literal_idx: NodeIndex,
    ) -> bool {
        let diagnostics_before = self.ctx.diagnostics.len();
        // Get the AST node for the object literal
        let Some(obj_node) = self.ctx.arena.get(obj_literal_idx) else {
            return false;
        };

        let Some(obj_lit) = self.ctx.arena.get_literal_expr(obj_node) else {
            return false;
        };

        // =============================================================
        // CRITICAL FIX: Iterate in reverse to handle duplicate properties
        // =============================================================
        // JavaScript/TypeScript behavior is "last property wins".
        // Example: const o = { x: { a: 1 }, x: { b: 1 } }
        // The runtime value of o.x is { b: 1 }, so we must check the last assignment.
        // =============================================================
        for &elem_idx in obj_lit.elements.nodes.iter().rev() {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Get the property name from this element
            let elem_prop_name = match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_property_assignment(elem_node)
                    .map(|prop| prop.name)
                    .and_then(|name_idx| self.object_literal_member_name_atom(name_idx)),
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_shorthand_property(elem_node)
                    .and_then(|prop| {
                        self.get_property_name(prop.name)
                            .map(|name| self.ctx.types.intern_string(&name))
                    }),
                _ => None,
            };

            // Skip if this property doesn't match the one we're looking for
            if elem_prop_name != Some(prop_name) {
                continue;
            }

            // Get the value expression for this property
            let value_idx = match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_property_assignment(elem_node)
                    .map(|prop| prop.initializer),
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    // For shorthand properties, the value expression is the same as the property name expression
                    self.ctx
                        .arena
                        .get_shorthand_property(elem_node)
                        .map(|prop| prop.name)
                }
                _ => None,
            };

            let Some(value_idx) = value_idx else {
                continue;
            };

            // =============================================================
            // CRITICAL FIX: Handle parenthesized expressions
            // =============================================================
            // TypeScript treats parenthesized object literals as fresh.
            // Example: x: ({ a: 1 }) should be checked for excess properties.
            // We need to unwrap parentheses before checking the kind.
            // =============================================================
            let effective_value_idx = self.ctx.arena.skip_parenthesized(value_idx);
            let Some(value_node) = self.ctx.arena.get(effective_value_idx) else {
                continue;
            };

            if value_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                // Check if we have a target type for this property
                if let Some(nested_target_type) = target_prop_type {
                    // Preserve the derived contextual target when retyping nested object
                    // literals for recursive excess-property checks.
                    let nested_request =
                        crate::context::TypingRequest::with_contextual_type(nested_target_type);
                    let nested_source_type =
                        self.get_type_of_node_with_request(effective_value_idx, &nested_request);

                    // Recursively check the nested object literal for excess properties
                    self.check_object_literal_excess_properties(
                        nested_source_type,
                        nested_target_type,
                        effective_value_idx,
                    );
                }

                return self.ctx.diagnostics.len() > diagnostics_before;
            }
        }
        false
    }

    /// Find the property element node in an object literal by interned property name.
    pub(crate) fn find_object_literal_property_element(
        &self,
        obj_literal_idx: NodeIndex,
        prop_name: tsz_common::interner::Atom,
    ) -> Option<NodeIndex> {
        let obj_node = self.ctx.arena.get(obj_literal_idx)?;
        let obj_lit = self.ctx.arena.get_literal_expr(obj_node)?;
        for &elem_idx in &obj_lit.elements.nodes {
            let elem_node = self.ctx.arena.get(elem_idx)?;
            let elem_prop_atom = match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_property_assignment(elem_node)
                    .and_then(|prop| self.get_property_name(prop.name))
                    .map(|name| self.ctx.types.intern_string(&name)),
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_shorthand_property(elem_node)
                    .and_then(|prop| self.get_identifier_text_from_idx(prop.name))
                    .map(|name| self.ctx.types.intern_string(&name)),
                _ => None,
            };
            if elem_prop_atom == Some(prop_name) {
                return Some(elem_idx);
            }
        }
        None
    }

    pub(super) fn object_literal_property_initializer(
        &self,
        prop_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let prop_node = self.ctx.arena.get(prop_idx)?;
        match prop_node.kind {
            syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                .ctx
                .arena
                .get_property_assignment(prop_node)
                .map(|prop| prop.initializer),
            syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                .ctx
                .arena
                .get_shorthand_property(prop_node)
                .map(|prop| prop.name),
            _ => None,
        }
    }

    pub(crate) fn object_literal_property_display_name(
        &self,
        elem_idx: NodeIndex,
        fallback_name: &str,
    ) -> String {
        let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
            return fallback_name.to_string();
        };

        match elem_node.kind {
            syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                .ctx
                .arena
                .get_property_assignment(elem_node)
                .and_then(|prop| {
                    self.ctx.arena.get(prop.name).and_then(|name_node| {
                        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                            self.computed_property_display_name(prop.name)
                        } else {
                            self.get_property_name(prop.name)
                        }
                    })
                })
                .unwrap_or_else(|| fallback_name.to_string()),
            syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                .ctx
                .arena
                .get_shorthand_property(elem_node)
                .and_then(|prop| self.get_identifier_text_from_idx(prop.name))
                .unwrap_or_else(|| fallback_name.to_string()),
            _ => fallback_name.to_string(),
        }
    }

    /// TS2353 guard for object destructuring from object literals with computed keys.
    ///
    /// TypeScript reports excess-property errors for computed properties in object
    /// literal initializers when the binding pattern contains only explicit keys.
    pub(crate) fn check_destructuring_object_literal_computed_excess_properties(
        &mut self,
        pattern_idx: NodeIndex,
        initializer_idx: NodeIndex,
        target_type: TypeId,
        use_pattern_target_type: bool,
    ) {
        if initializer_idx.is_none() || target_type == TypeId::ERROR {
            return;
        }

        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };
        if pattern_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN {
            return;
        }
        let Some(pattern) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        // Keep this narrow: if the pattern has rest, leave behavior to
        // the general relation path.
        for &element_idx in &pattern.elements.nodes {
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            let Some(element) = self.ctx.arena.get_binding_element(element_node) else {
                continue;
            };
            if element.dot_dot_dot_token {
                return;
            }
        }

        let effective_init = self.ctx.arena.skip_parenthesized(initializer_idx);
        let Some(init_node) = self.ctx.arena.get(effective_init) else {
            return;
        };
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return;
        };
        let Some(init_lit) = self.ctx.arena.get_literal_expr(init_node) else {
            return;
        };

        // Use explicit pattern keys only when the pattern is the source of truth (no
        // explicit annotation). This avoids overriding annotated target types like
        // `{x: string}` with a synthetic `{x: any}` shape.
        let effective_target_type = if use_pattern_target_type {
            self.object_binding_pattern_target_type_for_excess_checks(pattern_idx)
                .unwrap_or(target_type)
        } else {
            target_type
        };

        if effective_target_type == TypeId::ANY {
            return;
        }

        // Get the properties of the target type
        let Some(target_shape) = query::object_shape(self.ctx.types, effective_target_type) else {
            return;
        };
        let target_props = target_shape.properties.as_slice();

        for &elem_idx in &init_lit.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Get the property name from this element
            let prop_name = match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_property_assignment(elem_node)
                    .and_then(|prop| {
                        if let Some(name_node) = self.ctx.arena.get(prop.name)
                            && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                        {
                            return self.computed_property_display_name(prop.name);
                        }

                        self.get_property_name(prop.name)
                    }),
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_shorthand_property(elem_node)
                    .and_then(|prop| self.get_property_name(prop.name)),
                _ => None,
            };

            let prop_name = if let Some(pn) = prop_name {
                pn
            } else if elem_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
                && let Some(prop) = self.ctx.arena.get_property_assignment(elem_node)
                && let Some(name_node) = self.ctx.arena.get(prop.name)
                && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            {
                self.computed_property_display_name(prop.name)
                    .unwrap_or("[computed property]".to_string())
            } else {
                continue;
            };

            let prop_atom = self.ctx.types.intern_string(&prop_name);

            // Check if the property exists in the target type
            let target_prop = target_props.iter().find(|p| p.name == prop_atom);
            if target_prop.is_none() {
                self.error_excess_property_at_no_suggestion(
                    &prop_name,
                    effective_target_type,
                    elem_idx,
                );
                self.check_excess_property_initializer_implicit_any(
                    elem_idx,
                    effective_target_type,
                );
            }
        }
    }

    pub(super) fn object_binding_pattern_target_type_for_excess_checks(
        &mut self,
        pattern_idx: NodeIndex,
    ) -> Option<TypeId> {
        use tsz_common::interner::Atom;

        let pattern_node = self.ctx.arena.get(pattern_idx)?;
        if pattern_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN {
            return None;
        }
        let pattern = self.ctx.arena.get_binding_pattern(pattern_node)?;

        // Empty binding pattern `var {} = ...` — no properties are being destructured,
        // so no excess property check is needed. tsc treats `{}` as "ignore all properties".
        if pattern.elements.nodes.is_empty() {
            return None;
        }

        let mut prop_names: Vec<Atom> = Vec::new();
        for &element_idx in &pattern.elements.nodes {
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            let Some(element) = self.ctx.arena.get_binding_element(element_node) else {
                continue;
            };

            if element.dot_dot_dot_token {
                return None;
            }

            let property_names: Vec<Atom> = if element.property_name.is_some() {
                let Some(property_name_node) = self.ctx.arena.get(element.property_name) else {
                    continue;
                };
                if property_name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    let computed = self.ctx.arena.get_computed_property(property_name_node)?;
                    let key_type = self.get_type_of_node(computed.expression);
                    let (string_keys, number_keys) =
                        self.get_literal_key_union_from_type(key_type)?;
                    if string_keys.is_empty() && number_keys.is_empty() {
                        return None;
                    }

                    let mut names = Vec::with_capacity(string_keys.len() + number_keys.len());
                    names.extend(string_keys);
                    names.extend(number_keys.into_iter().map(|num| {
                        self.ctx
                            .types
                            .intern_string(&tsz_solver::utils::js_number_to_string(num))
                    }));
                    names
                } else {
                    vec![
                        self.ctx
                            .types
                            .intern_string(&self.get_property_name(element.property_name)?),
                    ]
                }
            } else {
                vec![
                    self.ctx
                        .types
                        .intern_string(&self.get_identifier_text_from_idx(element.name)?),
                ]
            };

            prop_names.extend(property_names);
        }

        query::excess_property_any_object_type_from_names(self.ctx.types, prop_names)
    }
}
