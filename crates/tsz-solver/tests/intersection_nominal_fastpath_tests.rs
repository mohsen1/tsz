//! Regression tests for issue #16089 — the O(1) nominal fast path that
//! `class_instance_extends_target_def` provides for a plain `interface W
//! extends Window {}` source never fired for an INTERSECTION source like
//! `Window & { extra: number }`: `check_object_subtype`'s short-circuit only
//! runs on a single evaluated `ObjectShape`, and an intersection source is
//! merged into one property set before that point, losing the per-member def
//! identity the fast path needs. So a source that merely mixes in an extra
//! property with a DOM-lib interface still paid the full structural walk
//! (and could time out) even though the underlying interface member alone
//! nominally proves the relation.
//!
//! `intersection_member_nominally_extends_target` is the fix: a per-member
//! check run in `visit_intersection` before the merged-property structural
//! path. It is sound as an unconditional early accept — subtyping is
//! transitive, so `Member <: Target` implies `(Member & Other) <: Target`
//! for any `Other` — which is exactly why checking one member in isolation,
//! ignoring the rest of the intersection, is safe here (unlike the general
//! "any member is a subtype" shortcut `visit_intersection` deliberately
//! disables for object-like targets to avoid accepting genuinely conflicting
//! intersections through the slower structural comparison).
//!
//! These are white-box tests of `intersection_member_nominally_extends_target`
//! itself, constructed with a fake `TypeResolver`, for the same reason
//! `objects_interface_nominal_fastpath_tests.rs` is: the fast path and the
//! structural walk it short-circuits must reach the same verdict by
//! construction, so a checker-level diagnostics diff cannot distinguish
//! "took the fast path" from "the slow path happened to agree." The real
//! DOM-lib performance win is verified separately by timing the real `tsz`
//! binary against the issue's own `Window & {..}` witnesses.

use crate::construction::TypeDatabase;
use crate::construction::TypeInterner;
use crate::def::DefId;
use crate::def::DefKind;
use crate::relations::subtype::SubtypeChecker;
use crate::relations::subtype::TypeResolver;
use crate::types::{PropertyInfo, SymbolRef, TypeId};
use std::collections::HashMap;

#[derive(Default)]
struct FakeHeritageResolver {
    kinds: HashMap<u32, DefKind>,
    class_extends: HashMap<u32, DefId>,
    interface_extends: HashMap<u32, DefId>,
    lib_defs: HashMap<u32, ()>,
}

impl FakeHeritageResolver {
    fn with_kind(mut self, def_id: DefId, kind: DefKind) -> Self {
        self.kinds.insert(def_id.0, kind);
        self
    }

    fn with_interface_extends(mut self, child: DefId, parent: DefId) -> Self {
        self.interface_extends.insert(child.0, parent);
        self
    }

    fn with_class_extends(mut self, child: DefId, parent: DefId) -> Self {
        self.class_extends.insert(child.0, parent);
        self
    }

    fn with_lib_def(mut self, def_id: DefId) -> Self {
        self.lib_defs.insert(def_id.0, ());
        self
    }
}

impl TypeResolver for FakeHeritageResolver {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn get_def_kind(&self, def_id: DefId) -> Option<DefKind> {
        self.kinds.get(&def_id.0).copied()
    }

    fn get_class_extends(&self, def_id: DefId) -> Option<DefId> {
        self.class_extends.get(&def_id.0).copied()
    }

    fn get_interface_extends(&self, def_id: DefId) -> Option<DefId> {
        self.interface_extends.get(&def_id.0).copied()
    }

    fn is_actual_or_cloned_lib_def(&self, def_id: DefId) -> bool {
        self.lib_defs.contains_key(&def_id.0)
    }
}

/// `Window & { extra: number }` (`member` = `Window`) against a lib-def
/// `Window` target: the member alone must resolve through `Lazy(DefId)`
/// identity (no receiver/shape needed) and accept in one hop.
#[test]
fn intersection_member_extends_lib_interface_target() {
    let interner = TypeInterner::new();
    let member_def = DefId(1); // e.g. an `interface W extends Window {}` member
    let target_def = DefId(2); // `Window`
    let resolver = FakeHeritageResolver::default()
        .with_kind(member_def, DefKind::Interface)
        .with_kind(target_def, DefKind::Interface)
        .with_interface_extends(member_def, target_def)
        .with_lib_def(target_def);
    let checker = SubtypeChecker::with_resolver(&interner, &resolver);

    let member = interner.lazy(member_def);
    let target_receiver = interner.lazy(target_def);

    assert!(checker.intersection_member_nominally_extends_target(member, target_receiver, None,));
}

/// A source member that is itself `Window` (not merely a heritage
/// descendant) must also short-circuit — `defs_are_equivalent` covers the
/// zero-hop identity case, not just multi-hop inheritance.
#[test]
fn intersection_member_identical_to_lib_interface_target() {
    let interner = TypeInterner::new();
    let target_def = DefId(1);
    let resolver = FakeHeritageResolver::default()
        .with_kind(target_def, DefKind::Interface)
        .with_lib_def(target_def);
    let checker = SubtypeChecker::with_resolver(&interner, &resolver);

    let member = interner.lazy(target_def);
    let target_receiver = interner.lazy(target_def);

    assert!(checker.intersection_member_nominally_extends_target(member, target_receiver, None,));
}

/// A class member (not just interfaces) must also reach the fast path
/// through this entry point — the underlying walk is kind-agnostic.
#[test]
fn intersection_member_class_extends_lib_class_target() {
    let interner = TypeInterner::new();
    let member_def = DefId(1);
    let target_def = DefId(2);
    let resolver = FakeHeritageResolver::default()
        .with_kind(member_def, DefKind::Class)
        .with_kind(target_def, DefKind::Class)
        .with_class_extends(member_def, target_def)
        .with_lib_def(target_def);
    let checker = SubtypeChecker::with_resolver(&interner, &resolver);

    let member = interner.lazy(member_def);
    let target_receiver = interner.lazy(target_def);

    assert!(checker.intersection_member_nominally_extends_target(member, target_receiver, None,));
}

/// Negative control: a member with no recorded heritage edge to the target
/// must decline, leaving the caller's merged-property structural walk to
/// decide (as it does today) rather than a stale/absent fast-path answer.
#[test]
fn intersection_member_without_heritage_edge_declines() {
    let interner = TypeInterner::new();
    let member_def = DefId(1);
    let target_def = DefId(2);
    let resolver = FakeHeritageResolver::default()
        .with_kind(member_def, DefKind::Interface)
        .with_kind(target_def, DefKind::Interface)
        .with_lib_def(target_def); // no with_interface_extends
    let checker = SubtypeChecker::with_resolver(&interner, &resolver);

    let member = interner.lazy(member_def);
    let target_receiver = interner.lazy(target_def);

    assert!(!checker.intersection_member_nominally_extends_target(member, target_receiver, None,));
}

/// A plain object-literal member (e.g. the `{ extra: number }` half of
/// `Window & { extra: number }`) is not a `Lazy(DefId)` reference at all and
/// must decline gracefully rather than panicking or matching spuriously.
#[test]
fn intersection_member_without_def_identity_declines() {
    let interner = TypeInterner::new();
    let target_def = DefId(1);
    let resolver = FakeHeritageResolver::default()
        .with_kind(target_def, DefKind::Interface)
        .with_lib_def(target_def);
    let checker = SubtypeChecker::with_resolver(&interner, &resolver);

    let member = interner.object(vec![]); // plain `{}`-shaped member, no def
    let target_receiver = interner.lazy(target_def);

    assert!(!checker.intersection_member_nominally_extends_target(member, target_receiver, None,));
}

/// The fast path must still require the target to be an actual/cloned lib
/// def — an interface heritage edge to a plain user-program interface must
/// not short-circuit (matches the non-intersection fast path's behavior).
#[test]
fn intersection_member_requires_lib_target() {
    let interner = TypeInterner::new();
    let member_def = DefId(1);
    let target_def = DefId(2);
    let resolver = FakeHeritageResolver::default()
        .with_kind(member_def, DefKind::Interface)
        .with_kind(target_def, DefKind::Interface)
        .with_interface_extends(member_def, target_def); // no with_lib_def(target_def)
    let checker = SubtypeChecker::with_resolver(&interner, &resolver);

    let member = interner.lazy(member_def);
    let target_receiver = interner.lazy(target_def);

    assert!(!checker.intersection_member_nominally_extends_target(member, target_receiver, None,));
}

/// Multi-hop chain: `C extends B {}`, `B extends Window {}`, member `C`,
/// target `Window`. Proves the walk re-checks `get_def_kind` at every hop
/// when reached through the intersection-member entry point too.
#[test]
fn intersection_member_multi_hop_chain_walks_to_target() {
    let interner = TypeInterner::new();
    let member_def = DefId(1); // C
    let mid = DefId(2); // B
    let target_def = DefId(3); // Window
    let resolver = FakeHeritageResolver::default()
        .with_kind(member_def, DefKind::Interface)
        .with_kind(mid, DefKind::Interface)
        .with_kind(target_def, DefKind::Interface)
        .with_interface_extends(member_def, mid)
        .with_interface_extends(mid, target_def)
        .with_lib_def(target_def);
    let checker = SubtypeChecker::with_resolver(&interner, &resolver);

    let member = interner.lazy(member_def);
    let target_receiver = interner.lazy(target_def);

    assert!(checker.intersection_member_nominally_extends_target(member, target_receiver, None,));
}

/// Zero-hop identity must NOT require the target to be a lib def: `X & Other
/// <: X` holds by reflexivity for a user-program interface exactly as it does
/// for `Window` (#17332). Only the heritage WALK stays behind the lib gate —
/// trusting extends-edges needs the checker-verified maps the gate vouches
/// for; trusting `X == X` needs nothing.
#[test]
fn intersection_member_identity_accepts_without_lib_gate() {
    let interner = TypeInterner::new();
    let target_def = DefId(1);
    let resolver = FakeHeritageResolver::default().with_kind(target_def, DefKind::Interface);
    // deliberately NO with_lib_def(target_def)
    let checker = SubtypeChecker::with_resolver(&interner, &resolver);

    let member = interner.lazy(target_def);
    let target_receiver = interner.lazy(target_def);

    assert!(checker.intersection_member_nominally_extends_target(member, target_receiver, None,));
}

/// The identity acceptance above must not leak into the heritage walk: a
/// verified extends-edge to a NON-lib target still declines (this pins the
/// gate's position between the identity check and the walk).
#[test]
fn intersection_member_heritage_edge_still_requires_lib_target() {
    let interner = TypeInterner::new();
    let member_def = DefId(1);
    let target_def = DefId(2);
    let resolver = FakeHeritageResolver::default()
        .with_kind(member_def, DefKind::Class)
        .with_kind(target_def, DefKind::Class)
        .with_class_extends(member_def, target_def);
    // deliberately NO with_lib_def(target_def)
    let checker = SubtypeChecker::with_resolver(&interner, &resolver);

    let member = interner.lazy(member_def);
    let target_receiver = interner.lazy(target_def);

    assert!(!checker.intersection_member_nominally_extends_target(member, target_receiver, None,));
}

/// A plain interface reference used as a relation target is routinely
/// interned as `Application(Lazy(def), [])` with empty args. The fast path
/// must resolve that form to its base def rather than declining (declining
/// re-runs the full structural walk, which for DOM-lib targets is exactly
/// the #17332 relation-budget blowup). Safe because the walk itself refuses
/// any target def that has type parameters.
#[test]
fn intersection_member_identity_accepts_application_wrapped_target() {
    let interner = TypeInterner::new();
    let target_def = DefId(1);
    let resolver = FakeHeritageResolver::default()
        .with_kind(target_def, DefKind::Interface)
        .with_lib_def(target_def);
    let checker = SubtypeChecker::with_resolver(&interner, &resolver);

    let member = interner.lazy(target_def);
    let target_receiver = interner.application(interner.lazy(target_def), vec![]);

    assert!(checker.intersection_member_nominally_extends_target(member, target_receiver, None,));
}

/// A target that is an anonymous structural re-mint (no `Lazy` form, no
/// `def_for_type` registration, no shape symbol) whose only remaining
/// provenance is the interner's display-alias link back to the reference it
/// was derived from must still resolve for the fast path. This is the
/// checker's shape for lib interfaces that never get a `Lazy` wrapper
/// (`type_reference_symbol_type` returns index-signature interfaces
/// structurally) and then get re-minted by `this`-substitution (#17332).
#[test]
fn intersection_member_identity_accepts_display_alias_provenance_target() {
    let interner = TypeInterner::new();
    let target_def = DefId(1);
    let resolver = FakeHeritageResolver::default()
        .with_kind(target_def, DefKind::Interface)
        .with_lib_def(target_def);
    let checker = SubtypeChecker::with_resolver(&interner, &resolver);

    let member = interner.lazy(target_def);
    let structural = interner.object(vec![PropertyInfo::new(
        interner.intern_string("document"),
        TypeId::STRING,
    )]);
    interner.store_display_alias(structural, interner.lazy(target_def));

    assert!(checker.intersection_member_nominally_extends_target(member, structural, None,));
}
