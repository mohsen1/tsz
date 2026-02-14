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

use crate::TypeDatabase;
use crate::types::{CallSignature, CallableShape, ObjectFlags, ObjectShape, PropertyInfo, TypeId};
use rustc_hash::FxHashMap;
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;

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
    /// The constructor type is a callable type with:
    /// - A construct signature that returns the instance type
    /// - Static properties as own properties
    /// - Optional nominal identity via the class symbol
    pub fn create_constructor_type(
        &self,
        static_members: Vec<PropertyInfo>,
        instance_type: TypeId,
        constructor_params: Vec<crate::types::ParamInfo>,
        type_params: Vec<crate::types::TypeParamInfo>,
        symbol: SymbolId,
    ) -> TypeId {
        let construct_sig = CallSignature {
            type_params,
            params: constructor_params,
            this_type: None,
            return_type: instance_type,
            type_predicate: None,
            is_method: false,
        };

        self.db.callable(CallableShape {
            call_signatures: Vec::new(),
            construct_signatures: vec![construct_sig],
            properties: static_members,
            string_index: None,
            number_index: None,
            symbol: Some(symbol),
        })
    }

    /// Extract properties from a type.
    fn get_properties_of_type(&self, type_id: TypeId) -> Vec<PropertyInfo> {
        use crate::type_queries::get_object_shape;

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
    graph: &crate::inheritance::InheritanceGraph,
) -> bool {
    // If parent is already derived from child, adding child->parent creates a cycle
    graph.is_derived_from(parent, child)
}

#[cfg(test)]
#[path = "tests/class_hierarchy_tests.rs"]
mod tests;
