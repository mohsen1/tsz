//! The #16089 nominal fast path for object-like relation targets.
//!
//! A source that reaches a class/interface `DefId` — a single instance type,
//! or one member of an intersection — can prove `Source <: Target` in O(1)
//! through the checker-verified heritage maps instead of re-walking the
//! target's full structural shape. Extracted from `objects.rs` (size
//! ratchet); the relation semantics and gate ordering live here.

use super::super::{SubtypeChecker, TypeResolver};
use crate::types::{ObjectShape, SymbolRef, TypeId};
use crate::visitor::{application_id, lazy_def_id, object_shape_id, object_with_index_shape_id};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    pub(in crate::relations::subtype) fn class_instance_extends_target_def(
        &self,
        source: &ObjectShape,
        source_receiver: Option<TypeId>,
        target_def: Option<crate::def::DefId>,
    ) -> bool {
        let Some(source_def) = source_receiver
            .and_then(|type_id| self.resolver.class_def_for_instance_type(type_id))
            .or_else(|| {
                source
                    .symbol
                    .and_then(|symbol| self.resolver.symbol_to_def_id(SymbolRef(symbol.0)))
            })
        else {
            return false;
        };
        let Some(target_def) = target_def else {
            return false;
        };
        self.def_nominally_extends_target_def(source_def, target_def)
    }

    /// O(1) check for `Intersection <: ObjectLikeTarget`: does a single
    /// intersection MEMBER's checker-verified class/interface heritage chain
    /// reach `target_def`? This is sound as an unconditional early accept
    /// regardless of the intersection's other members: subtyping is
    /// transitive, so `Member <: Target` implies `(Member & Other) <: Target`
    /// for any `Other`, whatever `Other` contributes. Lets a source like
    /// `Window & { extra: number }` skip `Window`'s full DOM-lib structural
    /// walk the same way a plain `interface W extends Window {}` source
    /// already does via `class_instance_extends_target_def` (#16089).
    ///
    /// Unlike `class_instance_extends_target_def`, `member` is a bare type
    /// reference (as stored in the intersection's member list), not a
    /// receiver/`this` instance type, so it resolves through the same chain
    /// `class_relation_target_def` uses for the target side rather than
    /// through `class_def_for_instance_type` alone.
    pub(crate) fn intersection_member_nominally_extends_target(
        &self,
        member: TypeId,
        target_receiver: TypeId,
        target_shape: Option<&ObjectShape>,
    ) -> bool {
        // Prefer a bare `Lazy(DefId)` receiver derived straight from the
        // evaluated shape's symbol over the raw `target_receiver` as stored:
        // a plain interface *reference* like `Window` used as a type
        // annotation is commonly interned as an `Application(def, args)`
        // even with an empty `args` (this is exactly what `check_object_subtype`
        // sidesteps by resolving its own `target_receiver` through
        // `receiver_type_from_shape_symbol` before ever calling
        // `class_relation_target_def`). `class_relation_target_def` bails
        // outright on an `Application` receiver, so passing the raw form
        // through unconditionally would silently defeat this fast path for
        // exactly the DOM-lib targets it exists for.
        let target_receiver = target_shape
            .and_then(|shape| self.receiver_type_from_shape_symbol(shape))
            .unwrap_or(target_receiver);
        // `class_relation_target_def` refuses an `Application` receiver
        // wholesale (its other caller passes receiver/`this` instance types,
        // where that caution is warranted). Here the target is a bare type
        // reference, and a plain interface reference is routinely interned
        // as `Application(Lazy(def), [])` with empty args — so fall back to
        // the same reference-form resolution the member side uses. This is
        // sound because `def_nominally_extends_target_def` bails on any
        // target def that has type parameters: a generic application's args
        // can never be ignored by the heritage walk, and a def with no
        // params has no args to ignore (#17332).
        let target_def = self
            .class_relation_target_def(Some(target_receiver), target_shape)
            .or_else(|| self.def_id_for_type_reference(target_receiver));
        let member_def = self.def_id_for_type_reference(member);
        tracing::trace!(
            target: "tsz::solver::nominal_fastpath",
            member = member.0,
            target_receiver = target_receiver.0,
            ?member_def,
            ?target_def,
            display_alias = ?self.interner.get_display_alias(target_receiver),
            alias_data = ?self
                .interner
                .get_display_alias(target_receiver)
                .and_then(|alias| self.interner.lookup(alias)),
            alias_alias = ?self
                .interner
                .get_display_alias(target_receiver)
                .and_then(|alias| self.interner.get_display_alias(alias)),
            shape_symbol = ?object_with_index_shape_id(self.interner, target_receiver)
                .or_else(|| object_shape_id(self.interner, target_receiver))
                .and_then(|id| self.interner.object_shape(id).symbol),
            "intersection member fast-path resolution"
        );
        let (Some(target_def), Some(member_def)) = (target_def, member_def) else {
            return false;
        };
        self.def_nominally_extends_target_def(member_def, target_def)
    }

    /// Shared heritage-chain walk behind both `class_instance_extends_target_def`
    /// and `intersection_member_nominally_extends_target`, once each has
    /// resolved its own `source_def`/`target_def`.
    fn def_nominally_extends_target_def(
        &self,
        source_def: crate::def::DefId,
        target_def: crate::def::DefId,
    ) -> bool {
        // #16137 widened this to interfaces; #16142 found the widening unsound
        // (an interface's heritage edge was trusted even when TS2430 rejected
        // it) and #16148 reverted to classes-only as a stopgap. This restores
        // the widening on the durable fix: `verified_interface_extends` below
        // is populated only when the checker's own TS2430 check passed, so an
        // interface source is now exactly as trustworthy as a class source.
        let source_kind = self.resolver.get_def_kind(source_def);
        tracing::trace!(
            target: "tsz::solver::nominal_fastpath",
            source_def = source_def.0,
            target_def = target_def.0,
            ?source_kind,
            target_has_params = self.def_has_type_params(target_def),
            equiv = self.resolver.defs_are_equivalent(source_def, target_def),
            lib_gate = self.resolver.is_actual_or_cloned_lib_def(target_def),
            "def_nominally_extends_target_def gates"
        );
        if !matches!(
            source_kind,
            Some(crate::def::DefKind::Class | crate::def::DefKind::Interface)
        ) {
            return false;
        }
        if self.def_has_type_params(target_def) {
            return false;
        }

        // Identity before the lib gate: `X & Other <: X` holds by
        // reflexivity for ANY class/interface `X`, lib or user-defined, so
        // gating it on `is_actual_or_cloned_lib_def` only loses witnesses.
        // Concretely, `TypeResolver`'s default `is_actual_or_cloned_lib_def`
        // returns `false`, so a relation running under a solver-side resolver
        // (e.g. `TypeEnvironment`) would otherwise never accept even an exact
        // `Member == Target` def match (#17332). The heritage WALK below
        // stays behind the gate: trusting extends-edges requires the
        // checker-verified maps the gate vouches for.
        if self.resolver.defs_are_equivalent(source_def, target_def) {
            return true;
        }

        if !self.resolver.is_actual_or_cloned_lib_def(target_def) {
            return false;
        }

        let mut current = source_def;
        for _ in 0..50 {
            // Classes use the checker-verified, generics-aware `class_extends`
            // map. Interfaces use `verified_interface_extends`, a single-parent
            // edge the checker registers only when
            // `check_interface_extension_compatibility` found no TS2430
            // ("incorrectly extends") for this declaration — trusting the raw
            // name-resolved heritage edge instead is unsound, since tsc's own
            // override check can reject a declared `extends` (#16142). Both
            // maps miss on a multi-parent `interface B extends A, C {}` (only
            // the first parent is tracked); a miss just returns `false` here,
            // which re-runs the always-correct structural walk in the caller.
            let parent = match self.resolver.get_def_kind(current) {
                Some(crate::def::DefKind::Class) => self.resolver.get_class_extends(current),
                Some(crate::def::DefKind::Interface) => {
                    self.resolver.get_interface_extends(current)
                }
                _ => None,
            };
            let Some(parent) = parent else {
                return false;
            };
            if self.resolver.defs_are_equivalent(parent, target_def) {
                return true;
            }
            current = parent;
        }
        false
    }

    fn def_has_type_params(&self, def_id: crate::def::DefId) -> bool {
        self.resolver
            .get_lazy_type_params(def_id)
            .is_some_and(|params| !params.is_empty())
    }

    pub(in crate::relations::subtype) fn class_relation_target_def(
        &self,
        target_receiver: Option<TypeId>,
        target: Option<&ObjectShape>,
    ) -> Option<crate::def::DefId> {
        if target_receiver.is_some_and(|type_id| self.receiver_is_application(type_id)) {
            return None;
        }

        target_receiver
            .and_then(|type_id| self.def_id_for_type_reference(type_id))
            .or_else(|| {
                target.and_then(|shape| {
                    shape
                        .symbol
                        .and_then(|symbol| self.resolver.symbol_to_def_id(SymbolRef(symbol.0)))
                })
            })
    }

    /// Resolve a `DefId` for a type used as a bare type reference (e.g. an
    /// intersection member, or the type checked against as a relation
    /// target) rather than a receiver/`this` instance type. Extracted from
    /// `class_relation_target_def` so `intersection_member_nominally_extends_target`
    /// can apply the same resolution to intersection members.
    fn def_id_for_type_reference(&self, type_id: TypeId) -> Option<crate::def::DefId> {
        lazy_def_id(self.interner, type_id)
            .or_else(|| {
                // A plain interface/class reference is commonly interned as
                // `Application(def, args)` even when `args` is empty. The
                // args are irrelevant to whether the *base* def's own
                // (arg-independent) heritage chain reaches a target def with
                // no type parameters of its own — `def_nominally_extends_target_def`
                // already requires that of the target — so unwrapping to the
                // base's `DefId` here is sound for the nominal-heritage walk
                // regardless of what the args are.
                application_id(self.interner, type_id).and_then(|app_id| {
                    lazy_def_id(self.interner, self.interner.type_application(app_id).base)
                })
            })
            .or_else(|| self.resolver.def_for_type(type_id))
            .or_else(|| self.resolver.class_def_for_instance_type(type_id))
            .or_else(|| self.def_for_receiver_shape_symbol(type_id))
            .or_else(|| {
                // Last resort: the interner's display-alias link (a
                // TypeId-to-TypeId association recorded when a structural
                // form was derived from a written reference — the same link
                // `receiver_is_application` above already consults for
                // semantic dispatch). An interface with an index signature
                // never gets a `Lazy` wrapper from the checker
                // (`type_reference_symbol_type` returns its structural
                // `ObjectWithIndex` directly), and a later `this`-substitution
                // re-mint of that shape misses `def_for_type` and carries no
                // shape symbol — the alias link back to the reference is the
                // only provenance left (#17332: `Window`, whose
                // `[index: number]: Window` keeps every `Window` annotation
                // structural). Sound at this altitude because every caller
                // still runs the full gate stack on the resolved def:
                // source-kind, target `def_has_type_params`, def equivalence,
                // and the checker-verified heritage maps — a stale alias can
                // only fail those gates, never fabricate an accept.
                let alias = self.interner.get_display_alias(type_id)?;
                lazy_def_id(self.interner, alias).or_else(|| {
                    application_id(self.interner, alias).and_then(|app_id| {
                        lazy_def_id(self.interner, self.interner.type_application(app_id).base)
                    })
                })
            })
    }

    fn receiver_is_application(&self, type_id: TypeId) -> bool {
        application_id(self.interner, type_id).is_some()
            || self
                .interner
                .get_display_alias(type_id)
                .is_some_and(|alias| application_id(self.interner, alias).is_some())
    }

    fn def_for_receiver_shape_symbol(&self, type_id: TypeId) -> Option<crate::def::DefId> {
        object_shape_id(self.interner, type_id)
            .and_then(|shape_id| self.interner.object_shape(shape_id).symbol)
            .or_else(|| {
                object_with_index_shape_id(self.interner, type_id)
                    .and_then(|shape_id| self.interner.object_shape(shape_id).symbol)
            })
            .and_then(|symbol| self.resolver.symbol_to_def_id(SymbolRef(symbol.0)))
    }
    pub(in crate::relations::subtype) fn receiver_type_from_shape_symbol(
        &self,
        shape: &ObjectShape,
    ) -> Option<TypeId> {
        let sym_id = shape.symbol?;
        let symbol_ref = crate::SymbolRef(sym_id.0);
        // Only nominalize when the resolver can produce a real DefId.
        // Falling back to `interner.reference(symbol_ref)` here would conflate
        // `SymbolId.0` with `DefId.0` (independent ID spaces) and yield a
        // Lazy(DefId) that points at an unrelated declaration.
        self.resolver
            .symbol_to_def_id(symbol_ref)
            .map(|def_id| self.interner.lazy(def_id))
    }
}

#[cfg(test)]
#[path = "../../../../tests/objects_interface_nominal_fastpath_tests.rs"]
mod objects_interface_nominal_fastpath_tests;

#[cfg(test)]
#[path = "../../../../tests/intersection_nominal_fastpath_tests.rs"]
mod intersection_nominal_fastpath_tests;
