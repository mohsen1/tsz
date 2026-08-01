//! Regression tests for issue #16089 — the O(1) nominal fast path in
//! `check_object_subtype` (`class_instance_extends_target_def`) was hard-gated
//! to `DefKind::Class`, so every `interface B extends A {}` relation against
//! `A` fell through to the full structural member walk even though tsc's own
//! override-compatibility rule at declaration time already guarantees `B`'s
//! instance is `<: A` — the same premise the class fast path relies on. The
//! walk over a DOM-lib interface's ~58-receiver closure (`Window`, `Document`,
//! …) is what made `interface W2 extends Window {}` time out; see the issue
//! for the full measurement trail.
//!
//! These are white-box tests of `class_instance_extends_target_def` itself,
//! constructed with a fake `TypeResolver`, not run through the diagnostics
//! harness: the fast path and the structural walk it short-circuits are
//! required to reach the same verdict by construction, so a checker-level
//! before/after diagnostics diff cannot distinguish "took the fast path" from
//! "the slow path happened to agree" (see the coordination board's standing
//! note on `#16018`-style widenings that moved zero tests either way). Only a
//! direct call proves the new `DefKind::Interface` branch consults
//! `get_interface_extends`, walks multiple hops, and never leaks onto/from
//! the class-only `class_extends` map.
//!
//! The real DOM-lib performance win is a separate, non-unit-testable claim:
//! the fast path only fires when `is_actual_or_cloned_lib_def(target_def)` is
//! true, which no synthetic user-program def in this harness (or in the
//! checker's lib-free unit harness) ever satisfies. That half is verified by
//! timing the real `tsz` binary against the issue's own witness.

use crate::construction::TypeDatabase;
use crate::construction::TypeInterner;
use crate::def::DefId;
use crate::def::DefKind;
use crate::relations::subtype::SubtypeChecker;
use crate::relations::subtype::TypeResolver;
use crate::types::{ObjectShape, SymbolRef, TypeId};
use std::collections::HashMap;
use tsz_binder::SymbolId;

#[derive(Default)]
struct FakeHeritageResolver {
    kinds: HashMap<u32, DefKind>,
    class_extends: HashMap<u32, DefId>,
    interface_extends: HashMap<u32, DefId>,
    lib_defs: HashMap<u32, ()>,
    symbol_defs: HashMap<u32, DefId>,
}

impl FakeHeritageResolver {
    fn with_kind(mut self, def_id: DefId, kind: DefKind) -> Self {
        self.kinds.insert(def_id.0, kind);
        self
    }

    fn with_class_extends(mut self, child: DefId, parent: DefId) -> Self {
        self.class_extends.insert(child.0, parent);
        self
    }

    fn with_interface_extends(mut self, child: DefId, parent: DefId) -> Self {
        self.interface_extends.insert(child.0, parent);
        self
    }

    fn with_lib_def(mut self, def_id: DefId) -> Self {
        self.lib_defs.insert(def_id.0, ());
        self
    }

    fn with_symbol(mut self, symbol: SymbolId, def_id: DefId) -> Self {
        self.symbol_defs.insert(symbol.0, def_id);
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

    fn symbol_to_def_id(&self, symbol: SymbolRef) -> Option<DefId> {
        self.symbol_defs.get(&symbol.0).copied()
    }
}

fn shape_with_symbol(symbol: SymbolId) -> ObjectShape {
    ObjectShape {
        symbol: Some(symbol),
        ..Default::default()
    }
}

/// `W2 extends Window {}` against a lib-def `Window` target: the new
/// `DefKind::Interface` arm must consult `get_interface_extends` (the
/// class-only map is empty) and return `true` in one hop.
#[ignore = "#16142: the interface fast path is disabled until verified heritage is registered; \
            its premise fails when the `extends` was itself rejected (TS2430)"]
#[test]
fn interface_source_uses_interface_extends_map() {
    let interner = TypeInterner::new();
    let source = DefId(1);
    let target = DefId(2);
    let symbol = SymbolId(100);
    let resolver = FakeHeritageResolver::default()
        .with_kind(source, DefKind::Interface)
        .with_kind(target, DefKind::Interface)
        .with_interface_extends(source, target)
        .with_lib_def(target)
        .with_symbol(symbol, source);
    let checker = SubtypeChecker::with_resolver(&interner, &resolver);

    assert!(checker.class_instance_extends_target_def(
        &shape_with_symbol(symbol),
        None,
        Some(target),
    ));
}

/// Negative control: an interface with no recorded heritage edge must not be
/// treated as related — the caller's structural walk is the one that gets to
/// decide, not a stale/absent fast-path answer.
#[test]
fn interface_source_without_heritage_edge_declines() {
    let interner = TypeInterner::new();
    let source = DefId(1);
    let target = DefId(2);
    let symbol = SymbolId(100);
    let resolver = FakeHeritageResolver::default()
        .with_kind(source, DefKind::Interface)
        .with_lib_def(target)
        .with_symbol(symbol, source);
    let checker = SubtypeChecker::with_resolver(&interner, &resolver);

    assert!(!checker.class_instance_extends_target_def(
        &shape_with_symbol(symbol),
        None,
        Some(target),
    ));
}

/// Regression control: the pre-existing class path is untouched — a class
/// source still resolves its parent through `get_class_extends`.
#[test]
fn class_source_still_uses_class_extends_map() {
    let interner = TypeInterner::new();
    let source = DefId(1);
    let target = DefId(2);
    let symbol = SymbolId(100);
    let resolver = FakeHeritageResolver::default()
        .with_kind(source, DefKind::Class)
        .with_kind(target, DefKind::Class)
        .with_class_extends(source, target)
        .with_lib_def(target)
        .with_symbol(symbol, source);
    let checker = SubtypeChecker::with_resolver(&interner, &resolver);

    assert!(checker.class_instance_extends_target_def(
        &shape_with_symbol(symbol),
        None,
        Some(target),
    ));
}

/// A class source must never fall back to the interface map, even if it
/// happens to be populated — the two maps are strictly kind-partitioned.
#[test]
fn class_source_ignores_interface_extends_map() {
    let interner = TypeInterner::new();
    let source = DefId(1);
    let target = DefId(2);
    let symbol = SymbolId(100);
    let resolver = FakeHeritageResolver::default()
        .with_kind(source, DefKind::Class)
        .with_kind(target, DefKind::Class)
        .with_interface_extends(source, target) // wrong map, deliberately
        .with_lib_def(target)
        .with_symbol(symbol, source);
    let checker = SubtypeChecker::with_resolver(&interner, &resolver);

    assert!(!checker.class_instance_extends_target_def(
        &shape_with_symbol(symbol),
        None,
        Some(target),
    ));
}

/// Symmetric control: an interface source must never fall back to the
/// class-only map.
#[test]
fn interface_source_ignores_class_extends_map() {
    let interner = TypeInterner::new();
    let source = DefId(1);
    let target = DefId(2);
    let symbol = SymbolId(100);
    let resolver = FakeHeritageResolver::default()
        .with_kind(source, DefKind::Interface)
        .with_kind(target, DefKind::Interface)
        .with_class_extends(source, target) // wrong map, deliberately
        .with_lib_def(target)
        .with_symbol(symbol, source);
    let checker = SubtypeChecker::with_resolver(&interner, &resolver);

    assert!(!checker.class_instance_extends_target_def(
        &shape_with_symbol(symbol),
        None,
        Some(target),
    ));
}

/// Multi-hop chain: `C extends B {}`, `B extends A {}`, target `A`. Proves the
/// walk re-checks `get_def_kind` at every hop rather than only the source's
/// own kind, and follows more than one edge.
#[ignore = "#16142: the interface fast path is disabled until verified heritage is registered; \
            its premise fails when the `extends` was itself rejected (TS2430)"]
#[test]
fn interface_multi_hop_chain_walks_to_target() {
    let interner = TypeInterner::new();
    let source = DefId(1); // C
    let mid = DefId(2); // B
    let target = DefId(3); // A
    let symbol = SymbolId(100);
    let resolver = FakeHeritageResolver::default()
        .with_kind(source, DefKind::Interface)
        .with_kind(mid, DefKind::Interface)
        .with_kind(target, DefKind::Interface)
        .with_interface_extends(source, mid)
        .with_interface_extends(mid, target)
        .with_lib_def(target)
        .with_symbol(symbol, source);
    let checker = SubtypeChecker::with_resolver(&interner, &resolver);

    assert!(checker.class_instance_extends_target_def(
        &shape_with_symbol(symbol),
        None,
        Some(target),
    ));
}

/// A def kind the fast path does not recognize (e.g. a type alias standing in
/// for any non-class/interface kind) must decline immediately, unchanged from
/// before this fix.
#[test]
fn unrecognized_def_kind_declines() {
    let interner = TypeInterner::new();
    let source = DefId(1);
    let target = DefId(2);
    let symbol = SymbolId(100);
    let resolver = FakeHeritageResolver::default()
        .with_kind(source, DefKind::TypeAlias)
        .with_interface_extends(source, target)
        .with_lib_def(target)
        .with_symbol(symbol, source);
    let checker = SubtypeChecker::with_resolver(&interner, &resolver);

    assert!(!checker.class_instance_extends_target_def(
        &shape_with_symbol(symbol),
        None,
        Some(target),
    ));
}

/// The fast path must still require the target to be an actual/cloned lib
/// def — an interface heritage edge to a plain user-program interface must
/// not short-circuit (matches the pre-existing class behavior).
#[test]
fn interface_source_requires_lib_target() {
    let interner = TypeInterner::new();
    let source = DefId(1);
    let target = DefId(2);
    let symbol = SymbolId(100);
    let resolver = FakeHeritageResolver::default()
        .with_kind(source, DefKind::Interface)
        .with_kind(target, DefKind::Interface)
        .with_interface_extends(source, target)
        .with_symbol(symbol, source); // no with_lib_def(target)
    let checker = SubtypeChecker::with_resolver(&interner, &resolver);

    assert!(!checker.class_instance_extends_target_def(
        &shape_with_symbol(symbol),
        None,
        Some(target),
    ));
}
