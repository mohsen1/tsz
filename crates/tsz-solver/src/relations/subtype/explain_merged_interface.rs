//! Failure explanation for merged-interface *reference* targets.
//!
//! A reference to a multi-declaration (merged) `interface` — e.g. the lib's
//! `Map<K, V>`, declared across `es2015.collection` / `es2015.iterable` /
//! `es2015.symbol.wellknown` — structurally evaluates to an intersection of
//! its per-declaration shapes, but tsc never relates or reports such a target
//! as an intersection: the merged interface is one named surface, and a
//! failed relation reports the flat missing-property reason over ALL
//! declarations' members (`Type '{}' is missing the following properties from
//! type 'Map<string, number>': clear, delete, forEach, get, and 8 more.`).
//! Only a *written* intersection target keeps the generic TS2322 +
//! per-constituent framing.

use crate::TypeId;
use crate::def::resolver::TypeResolver;
use crate::diagnostics::SubtypeFailureReason;
use crate::relations::subtype::SubtypeChecker;
use crate::visitor::{object_shape_id, object_with_index_shape_id};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Intersection-*target* arm of `explain_failure_inner`, checked BEFORE
    /// `evaluate_type` (which may merge intersection members into a single
    /// object, losing the intersection information). A merged
    /// multi-declaration `interface` reference reports the flat
    /// missing-property reason over its full member surface; any other
    /// (written) intersection target keeps the generic `TypeMismatch`
    /// downgrade, so the checker emits TS2322 — intersections combine
    /// constraints from multiple sources and tsc does not drill into member
    /// properties for them. Returns `None` when the target is not an
    /// intersection.
    pub(in crate::relations::subtype) fn explain_intersection_target(
        &mut self,
        source: TypeId,
        target: TypeId,
        resolved_source: TypeId,
        resolved_target: TypeId,
    ) -> Option<SubtypeFailureReason> {
        crate::visitor::intersection_list_id(self.interner, resolved_target)?;
        if let Some(reason) = self.explain_merged_interface_reference_failure(
            source,
            target,
            resolved_source,
            resolved_target,
        ) {
            return Some(reason);
        }
        Some(SubtypeFailureReason::TypeMismatch {
            source_type: source,
            target_type: target,
        })
    }

    /// The `DefId` behind a target that is (or instantiates) a reference to an
    /// `interface` definition: a `Lazy(DefId)` or an
    /// `Application(Lazy(DefId), args)` whose definition kind is
    /// `DefKind::Interface`. A type-alias reference (including a written
    /// `A & B` alias body) is NOT an interface reference and returns `None`.
    fn interface_reference_def_id(&self, target: TypeId) -> Option<crate::def::DefId> {
        let base = crate::visitor::application_id(self.interner, target)
            .map_or(target, |app_id| self.interner.type_application(app_id).base);
        let def_id = crate::visitor::lazy_def_id(self.interner, base)?;
        (self.resolver.get_def_kind(def_id)? == crate::def::DefKind::Interface).then_some(def_id)
    }

    /// Flat missing-property reason for an interface-reference target whose
    /// merged form evaluated to an intersection. Collects the full merged
    /// member surface across every constituent declaration shape and runs the
    /// ordinary object-failure walk against the ORIGINAL `target` id, so the
    /// rendered reason names the reference (`Map<string, number>`), not the
    /// structural intersection. Returns `None` (caller falls back to the
    /// written-intersection `TypeMismatch` downgrade) when the target is not
    /// an interface reference, the source has no object shape to compare, or
    /// the constituent surface cannot be collected.
    pub(in crate::relations::subtype) fn explain_merged_interface_reference_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
        resolved_source: TypeId,
        resolved_target: TypeId,
    ) -> Option<SubtypeFailureReason> {
        self.interface_reference_def_id(target)?;
        let s_sid = object_shape_id(self.interner, resolved_source)
            .or_else(|| object_with_index_shape_id(self.interner, resolved_source))?;
        let crate::objects::PropertyCollectionResult::Properties {
            properties: t_props,
            ..
        } = crate::objects::collect_properties_cached(
            resolved_target,
            self.interner,
            self.resolver,
            self.query_db,
        )
        else {
            return None;
        };
        let s_props = self.collect_source_properties(resolved_source);
        self.explain_object_failure(source, target, &s_props, Some(s_sid), &t_props)
    }
}
