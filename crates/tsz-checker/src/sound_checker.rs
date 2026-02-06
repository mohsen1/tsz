//! Sound Mode checker-side helpers (sticky freshness tracking).

use rustc_hash::{FxHashMap, FxHashSet};
use tsz_binder::SymbolId;
use tsz_solver::freshness;
use tsz_solver::type_queries;
use tsz_solver::{SoundDiagnostic, SoundDiagnosticCode, TypeDatabase, TypeId};

/// Tracks "sticky freshness" for Sound Mode excess property checking.
///
/// In TypeScript, object literals lose freshness once assigned to a variable.
/// Sound Mode's Sticky Freshness preserves freshness through inferred types,
/// only consuming it at explicit type annotations.
#[derive(Debug, Default)]
pub struct StickyFreshnessTracker {
    /// Variables that hold sticky-fresh values.
    /// Key: SymbolId of the variable
    /// Value: The source TypeId (for error messages)
    fresh_bindings: FxHashMap<SymbolId, TypeId>,

    /// Properties accessed from fresh objects that are also considered fresh.
    /// This handles destructuring and property access patterns.
    fresh_property_accesses: FxHashSet<(SymbolId, u32)>, // (object_symbol, property_hash)
}

impl StickyFreshnessTracker {
    /// Create a new tracker.
    pub fn new() -> Self {
        StickyFreshnessTracker {
            fresh_bindings: FxHashMap::default(),
            fresh_property_accesses: FxHashSet::default(),
        }
    }

    /// Mark a variable binding as holding a sticky-fresh value.
    ///
    /// Called when:
    /// - Variable is initialized with an object literal
    /// - Variable is assigned a fresh value from another fresh binding
    pub fn mark_binding_fresh(&mut self, symbol: SymbolId, source_type: TypeId) {
        self.fresh_bindings.insert(symbol, source_type);
    }

    /// Consume freshness from a binding.
    ///
    /// Called when:
    /// - Variable is assigned to an explicitly typed target
    /// - Variable is cast via `as` expression
    pub fn consume_freshness(&mut self, symbol: SymbolId) {
        self.fresh_bindings.remove(&symbol);
    }

    /// Check if a binding is sticky-fresh.
    pub fn is_binding_fresh(&self, symbol: SymbolId) -> bool {
        self.fresh_bindings.contains_key(&symbol)
    }

    /// Get the source type for a fresh binding (for error messages).
    pub fn get_fresh_source_type(&self, symbol: SymbolId) -> Option<TypeId> {
        self.fresh_bindings.get(&symbol).copied()
    }

    /// Mark a property access from a fresh object as fresh.
    ///
    /// Called when destructuring or accessing properties from fresh objects:
    /// ```typescript
    /// const obj = { a: 1, b: { c: 2, d: 3 } };
    /// const { b } = obj;  // b is also sticky-fresh
    /// ```
    pub fn mark_property_fresh(&mut self, object_symbol: SymbolId, property_hash: u32) {
        self.fresh_property_accesses
            .insert((object_symbol, property_hash));
    }

    /// Check if a property access is fresh.
    pub fn is_property_fresh(&self, object_symbol: SymbolId, property_hash: u32) -> bool {
        self.fresh_property_accesses
            .contains(&(object_symbol, property_hash))
    }

    /// Clear all freshness tracking (e.g., when entering a new function scope).
    pub fn clear(&mut self) {
        self.fresh_bindings.clear();
        self.fresh_property_accesses.clear();
    }

    /// Transfer freshness from one binding to another.
    ///
    /// Called when:
    /// - Variable is initialized from another variable
    /// - Destructuring assigns to new variables
    pub fn transfer_freshness(&mut self, from: SymbolId, to: SymbolId) {
        if let Some(source_type) = self.fresh_bindings.get(&from).copied() {
            self.fresh_bindings.insert(to, source_type);
        }
    }

    /// Number of fresh bindings (for testing).
    pub fn fresh_binding_count(&self) -> usize {
        self.fresh_bindings.len()
    }
}

/// Checker-side sound mode flow analyzer (sticky freshness, binding-level).
pub struct SoundFlowAnalyzer<'a> {
    db: &'a dyn TypeDatabase,
    freshness: StickyFreshnessTracker,
}

impl<'a> SoundFlowAnalyzer<'a> {
    pub fn new(db: &'a dyn TypeDatabase) -> Self {
        SoundFlowAnalyzer {
            db,
            freshness: StickyFreshnessTracker::new(),
        }
    }

    /// Check for excess properties with sticky freshness.
    ///
    /// This is the core of Sound Mode's excess property checking improvement.
    pub fn check_excess_properties(
        &self,
        source: TypeId,
        target: TypeId,
        source_symbol: Option<SymbolId>,
    ) -> Option<SoundDiagnostic> {
        let is_fresh = source_symbol
            .map(|s| self.freshness.is_binding_fresh(s))
            .unwrap_or(false)
            || freshness::is_fresh_object_type(self.db, source);

        if !is_fresh {
            return None;
        }

        let source_shape = type_queries::get_object_shape(self.db, source)?;
        let target_shape = type_queries::get_object_shape(self.db, target)?;

        // If target has an index signature, it accepts any extra properties.
        if target_shape.string_index.is_some() || target_shape.number_index.is_some() {
            return None;
        }

        for s_prop in source_shape.properties.iter() {
            let found = target_shape
                .properties
                .iter()
                .any(|t_prop| t_prop.name == s_prop.name);
            if !found {
                let prop_name = self.db.resolve_atom(s_prop.name);
                return Some(
                    SoundDiagnostic::new(SoundDiagnosticCode::ExcessPropertyStickyFreshness)
                        .with_arg(prop_name)
                        .with_arg(format!("{:?}", target)),
                );
            }
        }

        None
    }

    /// Get mutable access to the freshness tracker.
    pub fn freshness_mut(&mut self) -> &mut StickyFreshnessTracker {
        &mut self.freshness
    }

    /// Get read-only access to the freshness tracker.
    pub fn freshness(&self) -> &StickyFreshnessTracker {
        &self.freshness
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sticky_freshness_basic() {
        let mut tracker = StickyFreshnessTracker::new();

        let sym = SymbolId(1);
        assert!(!tracker.is_binding_fresh(sym));

        tracker.mark_binding_fresh(sym, TypeId::OBJECT);
        assert!(tracker.is_binding_fresh(sym));
        assert_eq!(tracker.get_fresh_source_type(sym), Some(TypeId::OBJECT));

        tracker.consume_freshness(sym);
        assert!(!tracker.is_binding_fresh(sym));
    }

    #[test]
    fn test_sticky_freshness_transfer() {
        let mut tracker = StickyFreshnessTracker::new();

        let from = SymbolId(1);
        let to = SymbolId(2);

        tracker.mark_binding_fresh(from, TypeId::OBJECT);
        tracker.transfer_freshness(from, to);

        assert!(tracker.is_binding_fresh(from));
        assert!(tracker.is_binding_fresh(to));
    }

    #[test]
    fn test_sticky_freshness_property() {
        let mut tracker = StickyFreshnessTracker::new();

        let sym = SymbolId(1);
        let prop_hash = 12345u32;

        assert!(!tracker.is_property_fresh(sym, prop_hash));

        tracker.mark_property_fresh(sym, prop_hash);
        assert!(tracker.is_property_fresh(sym, prop_hash));
        assert!(!tracker.is_property_fresh(sym, 99999));
    }
}
