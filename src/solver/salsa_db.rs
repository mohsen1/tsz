//! Salsa-based query database for incremental type checking.
//!
//! This module provides an alternative implementation of the TypeDatabase
//! trait using Salsa for incremental recomputation and memoization.
//!
//! The salsa implementation coexists with the legacy TypeInterner, allowing
//! for gradual migration and testing.

use crate::interner::Atom;
use crate::solver::intern::TypeInterner;
use crate::solver::types::{
    CallableShape, CallableShapeId, ConditionalType, ConditionalTypeId, FunctionShape,
    FunctionShapeId, IntrinsicKind, MappedType, MappedTypeId, ObjectShape, ObjectShapeId,
    PropertyInfo, PropertyLookup, SymbolRef, TemplateLiteralId, TemplateSpan, TupleElement,
    TupleListId, TypeApplication, TypeApplicationId, TypeId, TypeKey, TypeListId,
};
use std::sync::Arc;

// Re-export Salsa for use in the solver
pub use salsa;

/// The Salsa database structure.
///
/// This holds all the inputs and implements the query group.
#[salsa::query_group(SolverStorage)]
pub trait SolverDatabase {
    /// Get the underlying type interner (input).
    #[salsa::input]
    fn interner_ref(&self) -> Arc<TypeInterner>;

    /// Look up a type by its ID (memoized query).
    fn lookup(&self, id: TypeId) -> Option<TypeKey>;

    /// Intern a string atom (memoized query).
    fn intern_string_query(&self, s: String) -> Atom;

    /// Resolve an atom to its string value (memoized query).
    fn resolve_atom(&self, atom: Atom) -> String;

    /// Get a type list by ID (memoized query).
    fn type_list_query(&self, id: TypeListId) -> Arc<[TypeId]>;

    /// Evaluate a type (memoized query).
    fn evaluate_type(&self, type_id: TypeId) -> TypeId;

    /// Check if a type is a subtype of another (memoized query).
    fn is_subtype_of(&self, source: TypeId, target: TypeId) -> bool;
}

/// Implementation of solver queries.
///
/// Each query function uses the interner to perform its computation,
/// with Salsa providing automatic memoization and incremental recomputation.
fn lookup(db: &dyn SolverDatabase, id: TypeId) -> Option<TypeKey> {
    db.interner_ref().lookup(id)
}

fn intern_string_query(db: &dyn SolverDatabase, s: String) -> Atom {
    db.interner_ref().intern_string(&s)
}

fn resolve_atom(db: &dyn SolverDatabase, atom: Atom) -> String {
    db.interner_ref().resolve_atom(atom)
}

fn type_list_query(db: &dyn SolverDatabase, id: TypeListId) -> Arc<[TypeId]> {
    db.interner_ref().type_list(id)
}

fn evaluate_type(db: &dyn SolverDatabase, type_id: TypeId) -> TypeId {
    // Use the evaluator from the evaluate module
    // The evaluator will use the interner through the TypeDatabase trait
    crate::solver::evaluate::evaluate_type(db.interner_ref().as_ref(), type_id)
}

fn is_subtype_of(db: &dyn SolverDatabase, source: TypeId, target: TypeId) -> bool {
    crate::solver::subtype::is_subtype_of(db.interner_ref().as_ref(), source, target)
}

/// Concrete Salsa database implementation.
///
/// This struct wraps the Salsa runtime and provides the TypeDatabase trait
/// for compatibility with existing solver code.
pub struct SalsaDatabase {
    /// The underlying Salsa database runtime
    storage: salsa::DatabaseStruct<SolverDatabase>,
}

impl SalsaDatabase {
    /// Create a new Salsa database with the given interner.
    pub fn new(interner: Arc<TypeInterner>) -> Self {
        let mut storage = salsa::DatabaseStruct::default();
        storage.set_interner_ref(interner);
        SalsaDatabase { storage }
    }

    /// Get the underlying Salsa database for direct query access.
    pub fn salsa_db(&self) -> &SolverDatabase {
        &self.storage
    }

    /// Clear all cached query results and reset with a new interner.
    pub fn clear(&mut self, interner: Arc<TypeInterner>) {
        self.storage = salsa::DatabaseStruct::default();
        self.storage.set_interner_ref(interner);
    }
}

/// Implement salsa::Database for our storage.
impl salsa::Database for SalsaDatabase {
    fn salsa_storage(&self) -> &salsa::DatabaseStruct<SolverDatabase> {
        &self.storage
    }

    fn salsa_storage_mut(&mut self) -> &mut salsa::DatabaseStruct<SolverDatabase> {
        &mut self.storage
    }
}

/// Implement the TypeDatabase trait for SalsaDatabase.
///
/// This allows SalsaDatabase to be used anywhere TypeDatabase is required,
/// enabling gradual migration from the legacy TypeInterner to Salsa.
impl crate::solver::db::TypeDatabase for SalsaDatabase {
    fn intern(&self, key: TypeKey) -> TypeId {
        self.storage.interner_ref().intern(key)
    }

    fn lookup(&self, id: TypeId) -> Option<TypeKey> {
        self.storage.lookup(id)
    }

    fn intern_string(&self, s: &str) -> Atom {
        self.storage.intern_string_query(s.to_string())
    }

    fn resolve_atom(&self, atom: Atom) -> String {
        self.storage.resolve_atom(atom)
    }

    fn resolve_atom_ref(&self, atom: Atom) -> Arc<str> {
        self.storage.interner_ref().resolve_atom_ref(atom)
    }

    fn type_list(&self, id: TypeListId) -> Arc<[TypeId]> {
        self.storage.type_list_query(id)
    }

    fn tuple_list(&self, id: TupleListId) -> Arc<[TupleElement]> {
        self.storage.interner_ref().tuple_list(id)
    }

    fn template_list(&self, id: TemplateLiteralId) -> Arc<[TemplateSpan]> {
        self.storage.interner_ref().template_list(id)
    }

    fn object_shape(&self, id: ObjectShapeId) -> Arc<ObjectShape> {
        self.storage.interner_ref().object_shape(id)
    }

    fn object_property_index(&self, shape_id: ObjectShapeId, name: Atom) -> PropertyLookup {
        self.storage
            .interner_ref()
            .object_property_index(shape_id, name)
    }

    fn function_shape(&self, id: FunctionShapeId) -> Arc<FunctionShape> {
        self.storage.interner_ref().function_shape(id)
    }

    fn callable_shape(&self, id: CallableShapeId) -> Arc<CallableShape> {
        self.storage.interner_ref().callable_shape(id)
    }

    fn conditional_type(&self, id: ConditionalTypeId) -> Arc<ConditionalType> {
        self.storage.interner_ref().conditional_type(id)
    }

    fn mapped_type(&self, id: MappedTypeId) -> Arc<MappedType> {
        self.storage.interner_ref().mapped_type(id)
    }

    fn type_application(&self, id: TypeApplicationId) -> Arc<TypeApplication> {
        self.storage.interner_ref().type_application(id)
    }

    fn literal_string(&self, value: &str) -> TypeId {
        self.storage.interner_ref().literal_string(value)
    }

    fn literal_number(&self, value: f64) -> TypeId {
        self.storage.interner_ref().literal_number(value)
    }

    fn literal_boolean(&self, value: bool) -> TypeId {
        self.storage.interner_ref().literal_boolean(value)
    }

    fn literal_bigint(&self, value: &str) -> TypeId {
        self.storage.interner_ref().literal_bigint(value)
    }

    fn literal_bigint_with_sign(&self, negative: bool, digits: &str) -> TypeId {
        self.storage
            .interner_ref()
            .literal_bigint_with_sign(negative, digits)
    }

    fn union(&self, members: Vec<TypeId>) -> TypeId {
        self.storage.interner_ref().union(members)
    }

    fn union2(&self, left: TypeId, right: TypeId) -> TypeId {
        self.storage.interner_ref().union2(left, right)
    }

    fn union3(&self, first: TypeId, second: TypeId, third: TypeId) -> TypeId {
        self.storage.interner_ref().union3(first, second, third)
    }

    fn intersection(&self, members: Vec<TypeId>) -> TypeId {
        self.storage.interner_ref().intersection(members)
    }

    fn intersection2(&self, left: TypeId, right: TypeId) -> TypeId {
        self.storage.interner_ref().intersection2(left, right)
    }

    fn array(&self, element: TypeId) -> TypeId {
        self.storage.interner_ref().array(element)
    }

    fn tuple(&self, elements: Vec<TupleElement>) -> TypeId {
        self.storage.interner_ref().tuple(elements)
    }

    fn object(&self, properties: Vec<PropertyInfo>) -> TypeId {
        self.storage.interner_ref().object(properties)
    }

    fn object_with_index(&self, shape: ObjectShape) -> TypeId {
        self.storage.interner_ref().object_with_index(shape)
    }

    fn function(&self, shape: FunctionShape) -> TypeId {
        self.storage.interner_ref().function(shape)
    }

    fn callable(&self, shape: CallableShape) -> TypeId {
        self.storage.interner_ref().callable(shape)
    }

    fn template_literal(&self, spans: Vec<TemplateSpan>) -> TypeId {
        self.storage.interner_ref().template_literal(spans)
    }

    fn conditional(&self, conditional: ConditionalType) -> TypeId {
        self.storage.interner_ref().conditional(conditional)
    }

    fn mapped(&self, mapped: MappedType) -> TypeId {
        self.storage.interner_ref().mapped(mapped)
    }

    fn reference(&self, symbol: SymbolRef) -> TypeId {
        self.storage.interner_ref().reference(symbol)
    }

    fn application(&self, base: TypeId, args: Vec<TypeId>) -> TypeId {
        self.storage.interner_ref().application(base, args)
    }
}

/// Implement QueryDatabase trait for SalsaDatabase.
impl crate::solver::db::QueryDatabase for SalsaDatabase {
    fn as_type_database(&self) -> &dyn crate::solver::db::TypeDatabase {
        self
    }

    fn evaluate_type(&self, type_id: TypeId) -> TypeId {
        self.storage.evaluate_type(type_id)
    }

    fn is_subtype_of(&self, source: TypeId, target: TypeId) -> bool {
        self.storage.is_subtype_of(source, target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that SalsaDatabase can be created and basic queries work.
    #[test]
    fn test_salsa_database_creation() {
        let interner = Arc::new(TypeInterner::new());
        let db = SalsaDatabase::new(interner);

        // Test that we can look up intrinsic types
        assert_eq!(
            db.lookup(TypeId::STRING),
            Some(TypeKey::Intrinsic(IntrinsicKind::String))
        );
    }

    /// Test that query caching works - repeated calls return cached results.
    #[test]
    fn test_salsa_query_caching() {
        let interner = Arc::new(TypeInterner::new());
        let db = SalsaDatabase::new(interner);

        // First call computes the result
        let result1 = db.lookup(TypeId::NUMBER);

        // Second call should use cached result
        let result2 = db.lookup(TypeId::NUMBER);

        assert_eq!(result1, result2);
        assert_eq!(result1, Some(TypeKey::Intrinsic(IntrinsicKind::Number)));
    }

    /// Test type evaluation query.
    #[test]
    fn test_salsa_evaluate_type() {
        let interner = Arc::new(TypeInterner::new());
        let db = SalsaDatabase::new(interner);

        // Evaluating an intrinsic should return itself
        let result = db.evaluate_type(TypeId::STRING);
        assert_eq!(result, TypeId::STRING);
    }

    /// Test subtype checking query.
    #[test]
    fn test_salsa_subtype_query() {
        let interner = Arc::new(TypeInterner::new());
        let db = SalsaDatabase::new(interner);

        // string is a subtype of any
        assert!(db.is_subtype_of(TypeId::STRING, TypeId::ANY));

        // any is not a subtype of string
        assert!(!db.is_subtype_of(TypeId::ANY, TypeId::STRING));
    }

    /// Test string interning query.
    #[test]
    fn test_salsa_intern_string() {
        let interner = Arc::new(TypeInterner::new());
        let db = SalsaDatabase::new(interner);

        // Intern a string
        let atom1 = db.intern_string_query("hello".to_string());
        let atom2 = db.intern_string_query("hello".to_string());

        // Same string should return same atom
        assert_eq!(atom1, atom2);

        // Resolve the atom
        let resolved = db.resolve_atom(atom1);
        assert_eq!(resolved, "hello");
    }

    /// Test that clear() resets the database.
    #[test]
    fn test_salsa_clear() {
        let interner1 = Arc::new(TypeInterner::new());
        let mut db = SalsaDatabase::new(interner1);

        // Query something
        let result1 = db.lookup(TypeId::BOOLEAN);

        // Clear with new interner
        let interner2 = Arc::new(TypeInterner::new());
        db.clear(interner2);

        // Should still work after clear
        let result2 = db.lookup(TypeId::BOOLEAN);
        assert_eq!(result1, result2);
    }
}
