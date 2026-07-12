//! Diagnostic display provenance for the type interner.
//!
//! These methods record and consult the side tables that let the printer show
//! types the way the user wrote them: pre-widened fresh-literal properties,
//! evaluated-application alias names, conditional-alias bases, and as-written
//! union member order. None of this affects type identity or semantics; it is
//! purely the provenance the formatter reads to match `tsc` display.

use super::TypeInterner;
use crate::types::{LiteralValue, PropertyInfo, TypeData, TypeId};
use std::sync::Arc;

impl TypeInterner {
    /// Store display-only properties for a fresh object literal.
    ///
    /// These are the pre-widened property types shown in error messages.
    /// The `shape_id` is the widened (interned) shape; `props` contains
    /// the original literal types from the source code.
    pub fn store_display_properties(&self, type_id: TypeId, props: Vec<PropertyInfo>) {
        self.display_properties.insert(type_id, Arc::new(props));
    }

    /// Retrieve display-only properties for a fresh object literal.
    ///
    /// Returns `None` if no display properties were stored (i.e., the
    /// object type was not a fresh literal or had no widened properties).
    pub fn get_display_properties(&self, type_id: TypeId) -> Option<Arc<Vec<PropertyInfo>>> {
        self.display_properties.get(&type_id).map(|r| r.clone())
    }

    /// The empty object type `{}` is a shared display sentinel: every widened
    /// empty object literal in the program interns to the same `TypeId`, so a
    /// display alias recorded on it would repaint every unrelated empty object.
    /// Like intrinsics, it must never carry an alias. A shape with an index
    /// signature is a distinct, meaningful surface and is not the bare-`{}`
    /// sentinel. Mirrors `visitors::visitor_predicates::is_empty_object_type`.
    fn is_empty_object_display_sentinel(&self, type_id: TypeId) -> bool {
        match self.lookup(type_id) {
            Some(TypeData::Object(shape_id)) => self.object_shape(shape_id).properties.is_empty(),
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.object_shape(shape_id);
                shape.properties.is_empty()
                    && shape.string_index.is_none()
                    && shape.number_index.is_none()
            }
            _ => false,
        }
    }

    /// Store a reverse mapping from an evaluated Application result to its
    /// original Application TypeId for diagnostic display.
    ///
    /// Called during Application evaluation so that the formatter can recover
    /// the named form (e.g., `Dictionary<string>`) when it encounters the
    /// evaluated type (e.g., `{ [index: string]: string }`).
    pub fn store_display_alias(&self, evaluated: TypeId, application: TypeId) {
        // Only store if the evaluated type differs from the application
        // (i.e., evaluation actually produced a different type).
        if evaluated == application {
            return;
        }
        // A generic alias can evaluate to a bare type parameter (for example
        // `type Id<T> = T` or a conditional alias branch that returns `T`).
        // Type parameter ids are scoped semantic identities, not fresh display
        // surfaces for one alias application. Recording `T -> Alias<...>` here
        // repaints unrelated uses of `T` in later diagnostics.
        if matches!(self.lookup(evaluated), Some(TypeData::TypeParameter(_))) {
            return;
        }
        // Only alias types produced by this evaluation. Generic helper aliases
        // can otherwise repaint unrelated earlier structural types that happen
        // to intern to the same shape. Concrete applications remain eligible so
        // named library/interface instantiations can display their nominal form.
        let application_is_alias =
            matches!(self.lookup(application), Some(TypeData::Application(_)));
        let application_has_generic_args = application_is_alias
            && self
                .lookup(application)
                .and_then(|data| match data {
                    TypeData::Application(app_id) => Some(self.type_application(app_id)),
                    _ => None,
                })
                .is_some_and(|app| {
                    app.args.iter().any(|&arg| {
                        crate::type_queries::contains_generic_type_parameters_db(self, arg)
                    })
                });
        let evaluated_is_mapped = matches!(self.lookup(evaluated), Some(TypeData::Mapped(_)));
        let evaluated_precedes_application = match (
            self.lookup_alloc_order(evaluated),
            self.lookup_alloc_order(application),
        ) {
            (Some(evaluated_order), Some(application_order)) => {
                evaluated_order <= application_order
            }
            _ => evaluated.0 <= application.0,
        };
        if application_is_alias
            && application_has_generic_args
            && evaluated_precedes_application
            && !evaluated_is_mapped
        {
            let existing_is_application =
                self.display_alias.get(&evaluated).is_some_and(|existing| {
                    matches!(self.lookup(*existing), Some(TypeData::Application(_)))
                });
            if !existing_is_application {
                return;
            }
        }
        // Never alias intrinsic types (string, number, any, etc.) — they are
        // shared sentinels and aliasing them would make ALL occurrences display
        // as whatever alias happened to be stored last.
        if evaluated.is_intrinsic() {
            return;
        }
        // The empty object type `{}` is a shared structural sentinel for the
        // same reason: every widened empty object literal in the program
        // interns to it. Recording `{} -> Alias<...>` — e.g. `ThisType<any>`,
        // whose empty interface body is evaluated when the checker resolves the
        // `PropertyDescriptor & ThisType<any>` parameter of `Object.defineProperty`
        // — repaints EVERY empty object in the file. tsc always renders an empty
        // object as `{}`, never the alias.
        if self.is_empty_object_display_sentinel(evaluated) {
            return;
        }
        // Guard against self-referential cycles: if the Application's args
        // contain the evaluated type itself, storing this alias would create
        // a formatting cycle (e.g., `Wrap<T> = T | T[]` where evaluating
        // `Wrap<{x?: "ok"}>` produces a union, and a later re-application
        // creates `Wrap<union>` whose arg IS the union). Skip storage in
        // that case to prevent infinite `Wrap<Wrap<Wrap<...>>>` in diagnostics.
        if let Some(TypeData::Application(app_id)) = self.lookup(application) {
            let app = self.type_application(app_id);
            if app.args.contains(&evaluated) {
                return;
            }
        }
        if application_is_alias
            && let Some(existing) = self.display_alias.get(&evaluated).map(|alias| *alias)
            && !matches!(self.lookup(existing), Some(TypeData::Application(_)))
        {
            return;
        }
        self.display_alias.insert(evaluated, application);
    }

    /// Prefer a concrete Application display alias over structural provenance
    /// recorded while evaluating the alias body.
    pub fn store_display_alias_preferring_application(
        &self,
        evaluated: TypeId,
        application: TypeId,
    ) {
        self.store_display_alias(evaluated, application);
        if self.get_display_alias(evaluated) == Some(application) {
            return;
        }
        if evaluated == application || evaluated.is_intrinsic() {
            return;
        }
        // See `store_display_alias`: the empty object type is a shared display
        // sentinel and must never be repainted by a named application.
        if self.is_empty_object_display_sentinel(evaluated) {
            return;
        }
        if matches!(self.lookup(evaluated), Some(TypeData::TypeParameter(_))) {
            return;
        }
        let Some(TypeData::Application(app_id)) = self.lookup(application) else {
            return;
        };
        let app = self.type_application(app_id);
        if app.args.contains(&evaluated) {
            return;
        }
        let preserves_conditional_branch_alias = self.is_conditional_alias_base(app.base)
            && self.get_display_alias(evaluated).is_some_and(|existing| {
                matches!(self.lookup(existing), Some(TypeData::Intersection(_)))
            });
        if preserves_conditional_branch_alias {
            return;
        }
        let application_has_generic_args = app
            .args
            .iter()
            .any(|&arg| crate::type_queries::contains_generic_type_parameters_db(self, arg));
        let evaluated_precedes_application = match (
            self.lookup_alloc_order(evaluated),
            self.lookup_alloc_order(application),
        ) {
            (Some(evaluated_order), Some(application_order)) => {
                evaluated_order <= application_order
            }
            _ => evaluated.0 <= application.0,
        };
        let evaluated_is_mapped = matches!(self.lookup(evaluated), Some(TypeData::Mapped(_)));
        if application_has_generic_args && evaluated_precedes_application && !evaluated_is_mapped {
            return;
        }
        if evaluated_precedes_application
            && !evaluated_is_mapped
            && self.get_display_alias(evaluated).is_some_and(|existing| {
                matches!(self.lookup(existing), Some(TypeData::Application(_)))
            })
        {
            return;
        }
        self.display_alias.insert(evaluated, application);
    }

    /// Look up the original Application TypeId for a type that was produced
    /// by evaluating an Application.
    ///
    /// Returns `None` if this type was not produced from an Application evaluation.
    pub fn get_display_alias(&self, type_id: TypeId) -> Option<TypeId> {
        self.display_alias.get(&type_id).map(|r| *r)
    }

    /// Record semantic provenance from an evaluated structural result back to
    /// the `Application` it was produced from.
    ///
    /// Unlike [`Self::store_display_alias`], this carries no display-repaint
    /// heuristics: it is recorded unconditionally for nominal application
    /// evaluations and consumed only by the relation layer's accept-only
    /// variance recovery (never by the printer). First write wins so a stable
    /// origin survives later re-evaluations through other instantiations.
    pub fn record_application_eval_origin(&self, evaluated: TypeId, application: TypeId) {
        if evaluated == application || evaluated.is_intrinsic() {
            return;
        }
        if !matches!(self.lookup(application), Some(TypeData::Application(_))) {
            return;
        }
        // Guard against self-referential cycles (result appearing in its own
        // application arguments).
        if let Some(TypeData::Application(app_id)) = self.lookup(application) {
            let app = self.type_application(app_id);
            if app.args.contains(&evaluated) {
                return;
            }
        }
        self.application_eval_origin
            .entry(evaluated)
            .or_insert(application);
    }

    /// Look up the semantic application origin of an evaluated structural
    /// result. Returns `None` when the type was not recorded as the
    /// evaluation of a nominal `Application`.
    pub fn get_application_eval_origin(&self, type_id: TypeId) -> Option<TypeId> {
        self.application_eval_origin.get(&type_id).map(|r| *r)
    }

    /// Record that `merged` (an object synthesized by merging the object
    /// members of `intersection`, carrying `ObjectFlags::INTERSECTION_MERGED`)
    /// originated from `intersection`. Written once at merge time and never
    /// repainted, so it survives any later alias/application the merged object
    /// flows through. Consumed only by diagnostics to elaborate an intersection
    /// target member-by-member. First write wins.
    pub fn store_merged_intersection_origin(&self, merged: TypeId, intersection: TypeId) {
        if merged == intersection || merged.is_intrinsic() {
            return;
        }
        if !matches!(self.lookup(intersection), Some(TypeData::Intersection(_))) {
            return;
        }
        self.merged_intersection_origin
            .entry(merged)
            .or_insert(intersection);
    }

    /// Look up the original `Intersection` TypeId a merged object was
    /// synthesized from. Returns `None` when the type is not a recorded merge.
    pub fn get_merged_intersection_origin(&self, type_id: TypeId) -> Option<TypeId> {
        self.merged_intersection_origin.get(&type_id).map(|r| *r)
    }

    /// Record that an application base belongs to a type alias whose body is a
    /// conditional type. This is diagnostic-only provenance.
    pub fn mark_conditional_alias_base(&self, base: TypeId) {
        self.conditional_alias_bases.insert(base, ());
    }

    pub fn is_conditional_alias_base(&self, base: TypeId) -> bool {
        self.conditional_alias_bases.contains_key(&base)
    }

    /// Record that `type_id` is the synthetic `typeof globalThis` surface object
    /// so the formatter renders it as `typeof globalThis` rather than its full
    /// member body. Display-only provenance.
    pub fn mark_global_this_surface_display(&self, type_id: TypeId) {
        self.global_this_surface_display.insert(type_id, ());
    }

    /// Whether `type_id` is a recorded synthetic `typeof globalThis` surface.
    pub fn is_global_this_surface_display(&self, type_id: TypeId) -> bool {
        self.global_this_surface_display.contains_key(&type_id)
    }

    /// Record that `type_id` is an object type the user wrote directly as an
    /// object-type literal annotation (`{ ... }`). The printer consults this to
    /// refuse repainting such an annotation with an unrelated utility-type
    /// (`Application`) display alias that happens to share the same content-
    /// interned object id. Display-only provenance.
    pub fn mark_literal_object_annotation(&self, type_id: TypeId) {
        if type_id.is_intrinsic() {
            return;
        }
        self.literal_object_annotations.insert(type_id, ());
    }

    /// Whether `type_id` is a recorded hand-written object-type literal
    /// annotation.
    pub fn is_literal_object_annotation(&self, type_id: TypeId) -> bool {
        self.literal_object_annotations.contains_key(&type_id)
    }

    /// Record the as-written origin members for a flattened Union TypeId.
    ///
    /// Mirrors tsc's `UnionType.origin` mechanism: when `T | null` is built and
    /// `T` is itself a union alias, normalization flattens the result, but the
    /// printer needs the unflattened input list to display `T | null` instead
    /// of the structural expansion.
    ///
    /// We store the origin when normalization changed the visible order:
    /// - Flattening occurred (resulting Union has strictly more members), OR
    /// - The Union contains anonymous Object members whose canonical sort
    ///   (by `ShapeId`) doesn't match tsc's display order. tsc displays
    ///   anonymous objects in source/declaration order, but our interner
    ///   sorts by `ShapeId` (allocation order), which can reorder e.g.
    ///   `{} | { a: number }` to `{ a: number; } | {}` when the empty
    ///   shape was interned later than `{ a: number }`.
    ///
    /// We DO NOT store origin for canonical sort that matches tsc for non-
    /// anonymous-object cases (e.g., user wrote `"foo" | Refrigerator` but
    /// the interner sorted to `Refrigerator | "foo"` — tsc does the same).
    pub fn store_union_origin(&self, union_type_id: TypeId, origin_members: Vec<TypeId>) {
        if origin_members.len() < 2 {
            return;
        }
        let Some(TypeData::Union(list_id)) = self.lookup(union_type_id) else {
            // Only store origins for actual Union TypeIds; other shapes have
            // their own display paths.
            return;
        };
        let current = self.type_list(list_id);
        let flattened = current.len() > origin_members.len();
        if !flattened {
            // No flattening — only store if the canonical sort reordered
            // members whose source order tsc preserves verbatim.
            let needs_origin = self.union_origin_overrides_canonical_anon_object_sort(
                current.as_ref(),
                &origin_members,
            ) || self.union_origin_overrides_canonical_number_literal_sort(
                current.as_ref(),
                &origin_members,
            ) || self
                .union_origin_overrides_canonical_keyof_sort(current.as_ref(), &origin_members)
                || self.union_origin_overrides_canonical_application_sort(
                    current.as_ref(),
                    &origin_members,
                )
                || self.union_origin_overrides_canonical_array_pair_sort(
                    current.as_ref(),
                    &origin_members,
                )
                || self
                    .union_origin_overrides_canonical_tuple_sort(current.as_ref(), &origin_members)
                || self.union_origin_overrides_canonical_keyof_literal_sort(
                    current.as_ref(),
                    &origin_members,
                )
                || self.union_origin_overrides_canonical_type_param_sort(
                    current.as_ref(),
                    &origin_members,
                )
                || self.union_origin_overrides_canonical_generic_display_sort(
                    current.as_ref(),
                    &origin_members,
                );
            if !needs_origin {
                return;
            }
        }
        // First writer wins so deterministic display order is preserved when
        // the same flattened union is reached from multiple annotation sites.
        self.display_union_origin
            .entry(union_type_id)
            .or_insert_with(|| Arc::new(origin_members));
    }

    /// Replace the display origin for a union whose diagnostic context has a
    /// more specific tsc-compatible member order than the source union.
    pub fn replace_union_origin_for_display(
        &self,
        union_type_id: TypeId,
        origin_members: Vec<TypeId>,
    ) {
        if origin_members.len() < 2 {
            return;
        }
        let Some(TypeData::Union(_)) = self.lookup(union_type_id) else {
            return;
        };
        self.display_union_origin
            .insert(union_type_id, Arc::new(origin_members));
    }

    /// Decide whether storing the as-written origin is needed even when no
    /// flattening occurred — i.e. the union contains anonymous Object members
    /// and our canonical sort reordered them relative to the input.
    ///
    /// tsc displays anonymous (symbol-less) object union members in source/
    /// declaration order. Our interner sorts by `ShapeId` (allocation order),
    /// which can reorder them when the same shape is reached from earlier
    /// annotations in the file. Returns true only when (a) at least one
    /// member of the resulting union is an anonymous Object/ObjectWithIndex
    /// and (b) the resulting member order differs from the input.
    fn union_origin_overrides_canonical_anon_object_sort(
        &self,
        current: &[TypeId],
        origin: &[TypeId],
    ) -> bool {
        if current.len() != origin.len() {
            return false;
        }
        let mut has_anon_object = false;
        for &id in current {
            if let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) =
                self.lookup(id)
            {
                let shape = self.object_shape(shape_id);
                if shape.symbol.is_none() {
                    has_anon_object = true;
                    break;
                }
            }
        }
        if !has_anon_object {
            return false;
        }
        current != origin
    }

    /// Decide whether storing the as-written origin is needed for a union
    /// whose members are all number literals.
    ///
    /// The canonical comparator only special-cases the `0` literal and falls
    /// back to allocation order for other number literals. Allocation order
    /// is global and depends on which literals tsz happens to intern earlier
    /// (e.g., from lib processing or unrelated code), so the canonical sort
    /// can reorder source-written `0 | 1 | 2` into `0 | 2 | 1` even though
    /// tsc preserves the as-written order. When the canonical sort changed
    /// the order of a literal-only union, persist the origin so the printer
    /// can render the source-written form.
    fn union_origin_overrides_canonical_number_literal_sort(
        &self,
        current: &[TypeId],
        origin: &[TypeId],
    ) -> bool {
        if current.len() != origin.len() || current == origin {
            return false;
        }
        current.iter().all(|&id| {
            matches!(
                self.lookup(id),
                Some(TypeData::Literal(LiteralValue::Number(_)))
            )
        })
    }

    /// Decide whether storing the as-written origin is needed for a union that
    /// contains a `keyof` member. TypeScript preserves source order for
    /// displays such as `keyof Shape | "knownLiteralKey"`, while the semantic
    /// union comparator can place the literal first.
    fn union_origin_overrides_canonical_keyof_sort(
        &self,
        current: &[TypeId],
        origin: &[TypeId],
    ) -> bool {
        if current.len() != origin.len() || current == origin {
            return false;
        }
        current
            .iter()
            .chain(origin.iter())
            .any(|&id| matches!(self.lookup(id), Some(TypeData::KeyOf(_))))
    }

    /// Decide whether storing origin is needed for a generated union of
    /// same-base generic applications.
    ///
    /// Distributive conditional alias display builds unions such as
    /// `ChannelOfType<T, TextChannel> | ChannelOfType<T, EmailChannel>`.
    /// The semantic union comparator sorts same-base applications by their
    /// type arguments, which can invert the branch order that tsc displays from
    /// the source union being distributed. Preserve that origin only when the
    /// canonical union contains exactly the same same-base applications.
    fn union_origin_overrides_canonical_application_sort(
        &self,
        current: &[TypeId],
        origin: &[TypeId],
    ) -> bool {
        if current.len() != origin.len() || current == origin {
            return false;
        }

        let mut current_sorted = current.to_vec();
        let mut origin_sorted = origin.to_vec();
        current_sorted.sort_unstable_by_key(|id| id.0);
        origin_sorted.sort_unstable_by_key(|id| id.0);
        if current_sorted != origin_sorted {
            return false;
        }

        fn same_application_base(
            interner: &TypeInterner,
            ids: &[TypeId],
            expected_base: &mut Option<TypeId>,
        ) -> bool {
            for &id in ids {
                let Some(TypeData::Application(app_id)) = interner.lookup(id) else {
                    return false;
                };
                let app = interner.type_application(app_id);
                match expected_base {
                    Some(base) if *base != app.base => return false,
                    Some(_) => {}
                    None => *expected_base = Some(app.base),
                }
            }
            true
        }

        let mut expected_base = None;
        same_application_base(self, origin, &mut expected_base)
            && same_application_base(self, current, &mut expected_base)
            && expected_base.is_some()
    }

    fn union_origin_overrides_canonical_array_pair_sort(
        &self,
        current: &[TypeId],
        origin: &[TypeId],
    ) -> bool {
        if current.len() != 2 || origin.len() != 2 || current == origin {
            return false;
        }

        let mut current_sorted = current.to_vec();
        let mut origin_sorted = origin.to_vec();
        current_sorted.sort_unstable_by_key(|id| id.0);
        origin_sorted.sort_unstable_by_key(|id| id.0);
        if current_sorted != origin_sorted {
            return false;
        }

        fn is_array_of(interner: &TypeInterner, array: TypeId, element: TypeId) -> bool {
            matches!(interner.lookup(array), Some(TypeData::Array(inner)) if inner == element)
        }

        is_array_of(self, origin[0], origin[1]) || is_array_of(self, origin[1], origin[0])
    }

    /// Preserve source-written order for unions of tuple types.
    ///
    /// Tuple type IDs are allocated through normal structural interning, so
    /// unrelated eager/lib prewarming can change the canonical order of a
    /// source union like `[] | [number, string]`. `tsc` keeps the
    /// as-written tuple-branch order in diagnostics.
    fn union_origin_overrides_canonical_tuple_sort(
        &self,
        current: &[TypeId],
        origin: &[TypeId],
    ) -> bool {
        if current.len() != origin.len() || current == origin {
            return false;
        }

        let mut current_sorted = current.to_vec();
        let mut origin_sorted = origin.to_vec();
        current_sorted.sort_unstable_by_key(|id| id.0);
        origin_sorted.sort_unstable_by_key(|id| id.0);
        if current_sorted != origin_sorted {
            return false;
        }

        origin
            .iter()
            .all(|&id| matches!(self.lookup(id), Some(TypeData::Tuple(_))))
    }

    fn union_origin_overrides_canonical_keyof_literal_sort(
        &self,
        current: &[TypeId],
        origin: &[TypeId],
    ) -> bool {
        if current.len() != 2 || origin.len() != 2 || current == origin {
            return false;
        }

        let mut current_sorted = current.to_vec();
        let mut origin_sorted = origin.to_vec();
        current_sorted.sort_unstable_by_key(|id| id.0);
        origin_sorted.sort_unstable_by_key(|id| id.0);
        if current_sorted != origin_sorted {
            return false;
        }

        let is_keyof = |id| matches!(self.lookup(id), Some(TypeData::KeyOf(_)));
        let is_literal = |id| matches!(self.lookup(id), Some(TypeData::Literal(_)));
        (is_keyof(origin[0]) && is_literal(origin[1]))
            || (is_literal(origin[0]) && is_keyof(origin[1]))
    }

    /// Preserve source-written order for unions that contain type parameters.
    ///
    /// Declaration-scoped type parameters are interned fresh, and their
    /// allocation order can differ from declaration order when a later
    /// constrained refinement (`T extends U`) replaces the first-pass
    /// unconstrained placeholder. TypeScript still displays a source union
    /// such as `T | U` in source order, so when a caller records an origin
    /// with the same members in a different order, keep that origin for
    /// diagnostics.
    fn union_origin_overrides_canonical_type_param_sort(
        &self,
        current: &[TypeId],
        origin: &[TypeId],
    ) -> bool {
        if current.len() != origin.len() || current == origin {
            return false;
        }

        let mut current_sorted = current.to_vec();
        let mut origin_sorted = origin.to_vec();
        current_sorted.sort_unstable_by_key(|id| id.0);
        origin_sorted.sort_unstable_by_key(|id| id.0);
        if current_sorted != origin_sorted {
            return false;
        }

        origin
            .iter()
            .any(|&id| matches!(self.lookup(id), Some(TypeData::TypeParameter(_))))
    }

    /// Preserve source-written order for generic display unions whose canonical
    /// member sort is not the order tsc prints in diagnostics.
    ///
    /// Pure-literal unions are excluded: when every member is a literal,
    /// our canonical alloc-order sort already matches tsc's
    /// registration-order display (e.g. for instantiation-time unions like
    /// `T | U` with `T="World", U="Hello"` where `"Hello"` was registered
    /// first, tsc displays `"Hello" | "World"`).
    fn union_origin_overrides_canonical_generic_display_sort(
        &self,
        current: &[TypeId],
        origin: &[TypeId],
    ) -> bool {
        if current.len() != origin.len() || current == origin {
            return false;
        }

        let mut current_sorted = current.to_vec();
        let mut origin_sorted = origin.to_vec();
        current_sorted.sort_unstable_by_key(|id| id.0);
        origin_sorted.sort_unstable_by_key(|id| id.0);
        if current_sorted != origin_sorted {
            return false;
        }

        let has_complex_generic = origin.iter().any(|&id| {
            matches!(
                self.lookup(id),
                Some(TypeData::Application(_) | TypeData::KeyOf(_) | TypeData::IndexAccess(_, _))
            )
        });
        if has_complex_generic {
            return true;
        }

        // Mixed unions where literals coexist with non-literal complex types
        // still need origin preservation, but pure-literal unions don't.
        let all_literals = origin
            .iter()
            .all(|&id| matches!(self.lookup(id), Some(TypeData::Literal(_))));
        if all_literals {
            return false;
        }

        origin
            .iter()
            .any(|&id| matches!(self.lookup(id), Some(TypeData::Literal(_))))
    }

    /// Look up the as-written origin members for a flattened Union TypeId.
    pub fn get_union_origin(&self, type_id: TypeId) -> Option<Arc<Vec<TypeId>>> {
        self.display_union_origin.get(&type_id).map(|r| r.clone())
    }
}
