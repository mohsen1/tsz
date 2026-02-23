//! Class Hierarchy Type Construction
//!
//! This module implements class type construction in the Solver,
//! following the Solver-First Architecture.
//!
//! Responsibilities:
//! - Merge base class properties with derived class members
//! - Handle property overrides and shadowing
//! - Apply inheritance rules (nominal for classes, structural for interfaces)
//!
//! Note: Cycle detection for inheritance (class A extends B, B extends A)
//! is handled by the Checker using `InheritanceGraph` BEFORE calling these functions.
//! This module assumes the inheritance graph is acyclic.

#[cfg(test)]
mod merge {
    use crate::types::PropertyInfo;
    use rustc_hash::FxHashMap;
    use tsz_binder::SymbolId;
    use tsz_common::interner::Atom;

    /// Merge base class properties with derived class own members.
    ///
    /// Requirements:
    /// - ALL properties (including private) are inherited.
    /// - Private members are inherited but not accessible (Checker handles access control).
    /// - `parent_id` is updated to the current class for all own/overriding members.
    pub(crate) fn merge_properties(
        base: Vec<PropertyInfo>,
        own: Vec<PropertyInfo>,
        current_class: SymbolId,
    ) -> Vec<PropertyInfo> {
        let mut result_map: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();

        // 1. Add ALL base properties (private members are inherited but inaccessible)
        // This is critical for subtyping: derived class must structurally contain
        // all base class properties for assignability to work
        for prop in base {
            result_map.insert(prop.name, prop);
        }

        // 2. Override with own members (last-write-wins)
        for mut prop in own {
            // 3. Update parent_id to the current class
            // This stamps the property as belonging to the derived class
            prop.parent_id = Some(current_class);
            result_map.insert(prop.name, prop);
        }

        // Convert back to Vec
        result_map.into_values().collect()
    }
}

#[cfg(test)]
#[path = "../../tests/class_hierarchy_tests.rs"]
mod tests;
