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
//! is handled by the Checker using InheritanceGraph BEFORE calling these functions.
//! This module assumes the inheritance graph is acyclic.

use crate::binder::SymbolId;
use crate::interner::Atom;
use crate::solver::TypeDatabase;
use crate::solver::types::{ObjectFlags, ObjectShape, PropertyInfo, TypeId};
use rustc_hash::FxHashMap;

/// Builder for constructing class instance types.
///
/// This is a pure type computation - it knows nothing about AST nodes.
/// It takes a base type and a list of member properties, and produces
/// the merged instance type.
pub struct ClassTypeBuilder<'a> {
    db: &'a dyn TypeDatabase,
}

impl<'a> ClassTypeBuilder<'a> {
    pub fn new(db: &'a dyn TypeDatabase) -> Self {
        Self { db }
    }

    /// Creates a class instance type by merging base class properties with own members.
    ///
    /// # Arguments
    /// * `base_type` - The TypeId of the base class (or TypeId::ANY if no base)
    /// * `own_members` - Properties/methods declared directly in this class
    /// * `symbol` - The SymbolId of the class (for creating Ref type if needed)
    ///
    /// # Returns
    /// The TypeId representing the merged instance type.
    pub fn create_instance_type(
        &self,
        base_type: TypeId,
        own_members: Vec<PropertyInfo>,
        symbol: SymbolId,
    ) -> TypeId {
        // If base is ERROR, just return own members as an object type
        // This handles the case where base class has a cycle or error
        if base_type == TypeId::ERROR {
            return self.create_object_type(own_members, symbol);
        }

        // Get base class properties
        let base_props = self.get_properties_of_type(base_type);

        // Merge: own members override base properties
        // Pass the class symbol to handle parent_id updates during merge
        let merged = self.merge_properties(base_props, own_members, symbol);

        self.create_object_type(merged, symbol)
    }

    /// Creates a class constructor type.
    ///
    /// The constructor type is an object type with:
    /// - Static properties
    /// - A construct signature that returns the instance type
    pub fn create_constructor_type(
        &self,
        static_members: Vec<PropertyInfo>,
        _instance_type: TypeId,
        _symbol: SymbolId,
    ) -> TypeId {
        // For now, we just return an object type with static members
        // In a full implementation, this would include construct signatures
        // TODO: Add construct signature support
        self.db.object(static_members)
    }

    /// Extract properties from a type.
    fn get_properties_of_type(&self, type_id: TypeId) -> Vec<PropertyInfo> {
        use crate::solver::type_queries::get_object_shape;

        match get_object_shape(self.db, type_id) {
            Some(shape) => shape.properties.clone(),
            None => Vec::new(),
        }
    }

    /// Merge base properties with own members.
    ///
    /// Requirements:
    /// - ALL properties (including private) are inherited.
    /// - Private members are inherited but not accessible (Checker handles access control).
    /// - parent_id is updated to the current class for all own/overriding members.
    fn merge_properties(
        &self,
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

    /// Create an object type from properties.
    fn create_object_type(&self, properties: Vec<PropertyInfo>, symbol: SymbolId) -> TypeId {
        // Create an object type with the class symbol for nominal discrimination
        // The symbol field affects Hash (for interning) but NOT PartialEq (for structural comparison)
        // This ensures that:
        // - Different classes with identical structures get different TypeIds (via Hash in interner)
        // - Structural type checking still works correctly (via PartialEq ignoring symbol)
        self.db.object_with_index(ObjectShape {
            flags: ObjectFlags::empty(),
            properties,
            string_index: None,
            number_index: None,
            symbol: Some(symbol),
        })
    }
}

/// Detects if adding a parent to a child would create a cycle in the inheritance graph.
///
/// This is a static check that should be performed by the Checker BEFORE
/// calling the Solver to construct class types.
///
/// # Arguments
/// * `child` - The SymbolId of the class being defined
/// * `parent` - The SymbolId of the base class
/// * `graph` - The InheritanceGraph to check against
///
/// # Returns
/// `true` if adding child->parent would create a cycle.
pub fn would_create_inheritance_cycle(
    child: SymbolId,
    parent: SymbolId,
    graph: &crate::solver::inheritance::InheritanceGraph,
) -> bool {
    // If parent is already derived from child, adding child->parent creates a cycle
    graph.is_derived_from(parent, child)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver::TypeInterner;
    use crate::solver::types::Visibility;

    #[test]
    fn test_merge_properties() {
        let interner = TypeInterner::new();
        let builder = ClassTypeBuilder::new(&interner);

        let name_atom = interner.intern_string("name");
        let age_atom = interner.intern_string("age");

        let base_props = vec![PropertyInfo {
            name: name_atom,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        }];

        let own_props = vec![
            PropertyInfo {
                name: name_atom, // Override
                type_id: TypeId::NUMBER,
                write_type: TypeId::NUMBER,
                optional: true,
                readonly: true,
                is_method: false,
                visibility: Visibility::Public,
                parent_id: None,
            },
            PropertyInfo {
                name: age_atom, // New
                type_id: TypeId::NUMBER,
                write_type: TypeId::NUMBER,
                optional: false,
                readonly: false,
                is_method: false,
                visibility: Visibility::Public,
                parent_id: None,
            },
        ];

        let dummy_symbol = SymbolId(999);
        let merged = builder.merge_properties(base_props, own_props, dummy_symbol);

        assert_eq!(merged.len(), 2);
        // name should be overridden
        let name_prop = merged.iter().find(|p| p.name == name_atom).unwrap();
        assert_eq!(name_prop.type_id, TypeId::NUMBER); // Overridden
        assert!(name_prop.optional); // Overridden
        assert!(name_prop.readonly); // Overridden
    }
}
