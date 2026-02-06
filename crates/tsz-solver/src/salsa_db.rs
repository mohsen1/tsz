//! Salsa-based query database for incremental type checking (proof-of-concept).
//!
//! This module provides an alternative implementation of the TypeDatabase
//! and QueryDatabase traits using Salsa for incremental recomputation and
//! memoization. It coexists with the production `QueryCache` in `db.rs`.
//!
//! **Status**: Proof-of-concept / test bed. The production memoization layer
//! is `QueryCache` (see `src/solver/db.rs`). Full Salsa routing was evaluated
//! and deferred — see Appendix F in `docs/architecture/SOLVER_REFACTORING_PROPOSAL.md`
//! for the architectural decision record.
//!
//! Key reason: The solver's SubtypeChecker and TypeEvaluator use mutable
//! internal state (`in_progress` sets, recursion depth, `&mut self`) which
//! is incompatible with Salsa's pure-function query model.

use crate::compat::CompatChecker;
use crate::intern::TypeInterner;
use crate::subtype::TypeEnvironment;
use crate::types::{
    CallableShape, CallableShapeId, ConditionalType, ConditionalTypeId, FunctionShape,
    FunctionShapeId, MappedType, MappedTypeId, ObjectFlags, ObjectShape, ObjectShapeId,
    PropertyInfo, PropertyLookup, SymbolRef, TemplateLiteralId, TemplateSpan, TupleElement,
    TupleListId, TypeApplication, TypeApplicationId, TypeId, TypeKey, TypeListId,
};
use std::sync::Arc;
use tsz_binder::SymbolId;
use tsz_common::CheckerOptions;
use tsz_common::interner::Atom;

/// Configuration for subtype checking, stored as a Salsa input.
///
/// This allows Salsa to track when configuration changes and invalidate
/// cached subtype results accordingly.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SubtypeConfig {
    pub strict_function_types: bool,
    pub strict_null_checks: bool,
    pub exact_optional_property_types: bool,
    pub no_unchecked_indexed_access: bool,
}

impl Default for SubtypeConfig {
    fn default() -> Self {
        SubtypeConfig {
            strict_function_types: false,
            strict_null_checks: true,
            exact_optional_property_types: false,
            no_unchecked_indexed_access: false,
        }
    }
}

impl SubtypeConfig {
    /// Create a SubtypeConfig from CheckerOptions.
    pub fn from_checker_options(options: &CheckerOptions) -> Self {
        SubtypeConfig {
            strict_function_types: options.strict_function_types,
            strict_null_checks: options.strict_null_checks,
            exact_optional_property_types: options.exact_optional_property_types,
            no_unchecked_indexed_access: options.no_unchecked_indexed_access,
        }
    }
}

// Re-export Salsa for use in the solver
pub use salsa;

/// The Salsa query group for the solver.
///
/// This holds all the inputs and defines the memoized queries.
#[salsa::query_group(SolverStorage)]
pub trait SolverDatabase: salsa::Database {
    /// Get the underlying type interner (input).
    #[salsa::input]
    fn interner_ref(&self) -> Arc<TypeInterner>;

    /// Get the type environment for resolving Ref/Lazy types (input).
    #[salsa::input]
    fn type_environment(&self) -> Arc<TypeEnvironment>;

    /// Get the subtype checking configuration (input).
    #[salsa::input]
    fn subtype_config(&self) -> SubtypeConfig;

    /// Look up a type by its ID (memoized query).
    fn lookup_query(&self, id: TypeId) -> Option<TypeKey>;

    /// Intern a string atom (memoized query).
    fn intern_string_query(&self, s: String) -> Atom;

    /// Resolve an atom to its string value (memoized query).
    fn resolve_atom_query(&self, atom: Atom) -> String;

    /// Get a type list by ID (memoized query).
    fn type_list_query(&self, id: TypeListId) -> Arc<[TypeId]>;

    /// Evaluate a type (memoized query with cycle recovery).
    #[salsa::cycle(evaluate_type_recover)]
    fn evaluate_type_query(&self, type_id: TypeId) -> TypeId;

    /// Check if a type is a subtype of another (memoized query with cycle recovery).
    #[salsa::cycle(is_subtype_of_recover)]
    fn is_subtype_of_query(&self, source: TypeId, target: TypeId) -> bool;
}

// =============================================================================
// Query implementations
// =============================================================================

fn lookup_query(db: &dyn SolverDatabase, id: TypeId) -> Option<TypeKey> {
    db.interner_ref().lookup(id)
}

fn intern_string_query(db: &dyn SolverDatabase, s: String) -> Atom {
    db.interner_ref().intern_string(&s)
}

fn resolve_atom_query(db: &dyn SolverDatabase, atom: Atom) -> String {
    db.interner_ref().resolve_atom(atom)
}

fn type_list_query(db: &dyn SolverDatabase, id: TypeListId) -> Arc<[TypeId]> {
    db.interner_ref().type_list(id)
}

fn evaluate_type_query(db: &dyn SolverDatabase, type_id: TypeId) -> TypeId {
    let interner = db.interner_ref();
    let env = db.type_environment();
    let mut evaluator =
        crate::evaluate::TypeEvaluator::with_resolver(interner.as_ref(), env.as_ref());
    evaluator.evaluate(type_id)
}

fn is_subtype_of_query(db: &dyn SolverDatabase, source: TypeId, target: TypeId) -> bool {
    let interner = db.interner_ref();
    let env = db.type_environment();
    let config = db.subtype_config();
    let mut checker =
        crate::subtype::SubtypeChecker::with_resolver(interner.as_ref(), env.as_ref());
    checker.strict_function_types = config.strict_function_types;
    checker.strict_null_checks = config.strict_null_checks;
    checker.exact_optional_property_types = config.exact_optional_property_types;
    checker.no_unchecked_indexed_access = config.no_unchecked_indexed_access;
    checker.is_subtype_of(source, target)
}

// =============================================================================
// Cycle recovery functions
// =============================================================================

/// Cycle recovery for subtype checking: coinductive/greatest fixed point.
fn is_subtype_of_recover(
    _db: &dyn SolverDatabase,
    _cycle: &[String],
    _source: &TypeId,
    _target: &TypeId,
) -> bool {
    true
}

/// Cycle recovery for type evaluation: identity.
fn evaluate_type_recover(_db: &dyn SolverDatabase, _cycle: &[String], type_id: &TypeId) -> TypeId {
    *type_id
}

/// Concrete Salsa database implementation.
///
/// This struct wraps the Salsa runtime and provides the TypeDatabase trait
/// for compatibility with existing solver code.
#[salsa::database(SolverStorage)]
pub struct SalsaDatabase {
    /// The underlying Salsa storage
    storage: salsa::Storage<SalsaDatabase>,
}

impl SalsaDatabase {
    /// Create a new Salsa database with the given interner.
    pub fn new(interner: Arc<TypeInterner>) -> Self {
        let mut db = SalsaDatabase {
            storage: Default::default(),
        };
        db.set_interner_ref(interner);
        db.set_type_environment(Arc::new(TypeEnvironment::default()));
        db.set_subtype_config(SubtypeConfig::default());
        db
    }

    /// Clear all cached query results and reset with a new interner.
    pub fn clear(&mut self, interner: Arc<TypeInterner>) {
        self.storage = Default::default();
        self.set_interner_ref(interner);
        self.set_type_environment(Arc::new(TypeEnvironment::default()));
        self.set_subtype_config(SubtypeConfig::default());
    }
}

impl salsa::Database for SalsaDatabase {}

/// Implement the TypeDatabase trait for SalsaDatabase.
///
/// This allows SalsaDatabase to be used anywhere TypeDatabase is required,
/// enabling gradual migration from the legacy TypeInterner to Salsa.
impl crate::db::TypeDatabase for SalsaDatabase {
    fn intern(&self, key: TypeKey) -> TypeId {
        self.interner_ref().intern(key)
    }

    fn lookup(&self, id: TypeId) -> Option<TypeKey> {
        self.lookup_query(id)
    }

    fn intern_string(&self, s: &str) -> Atom {
        self.intern_string_query(s.to_string())
    }

    fn resolve_atom(&self, atom: Atom) -> String {
        self.resolve_atom_query(atom)
    }

    fn resolve_atom_ref(&self, atom: Atom) -> Arc<str> {
        self.interner_ref().resolve_atom_ref(atom)
    }

    fn type_list(&self, id: TypeListId) -> Arc<[TypeId]> {
        self.type_list_query(id)
    }

    fn tuple_list(&self, id: TupleListId) -> Arc<[TupleElement]> {
        self.interner_ref().tuple_list(id)
    }

    fn template_list(&self, id: TemplateLiteralId) -> Arc<[TemplateSpan]> {
        self.interner_ref().template_list(id)
    }

    fn object_shape(&self, id: ObjectShapeId) -> Arc<ObjectShape> {
        self.interner_ref().object_shape(id)
    }

    fn object_property_index(&self, shape_id: ObjectShapeId, name: Atom) -> PropertyLookup {
        self.interner_ref().object_property_index(shape_id, name)
    }

    fn function_shape(&self, id: FunctionShapeId) -> Arc<FunctionShape> {
        self.interner_ref().function_shape(id)
    }

    fn callable_shape(&self, id: CallableShapeId) -> Arc<CallableShape> {
        self.interner_ref().callable_shape(id)
    }

    fn conditional_type(&self, id: ConditionalTypeId) -> Arc<ConditionalType> {
        self.interner_ref().conditional_type(id)
    }

    fn mapped_type(&self, id: MappedTypeId) -> Arc<MappedType> {
        self.interner_ref().mapped_type(id)
    }

    fn type_application(&self, id: TypeApplicationId) -> Arc<TypeApplication> {
        self.interner_ref().type_application(id)
    }

    fn literal_string(&self, value: &str) -> TypeId {
        self.interner_ref().literal_string(value)
    }

    fn literal_number(&self, value: f64) -> TypeId {
        self.interner_ref().literal_number(value)
    }

    fn literal_boolean(&self, value: bool) -> TypeId {
        self.interner_ref().literal_boolean(value)
    }

    fn literal_bigint(&self, value: &str) -> TypeId {
        self.interner_ref().literal_bigint(value)
    }

    fn literal_bigint_with_sign(&self, negative: bool, digits: &str) -> TypeId {
        self.interner_ref()
            .literal_bigint_with_sign(negative, digits)
    }

    fn union(&self, members: Vec<TypeId>) -> TypeId {
        self.interner_ref().union(members)
    }

    fn union2(&self, left: TypeId, right: TypeId) -> TypeId {
        self.interner_ref().union2(left, right)
    }

    fn union3(&self, first: TypeId, second: TypeId, third: TypeId) -> TypeId {
        self.interner_ref().union3(first, second, third)
    }

    fn intersection(&self, members: Vec<TypeId>) -> TypeId {
        self.interner_ref().intersection(members)
    }

    fn intersection2(&self, left: TypeId, right: TypeId) -> TypeId {
        self.interner_ref().intersection2(left, right)
    }

    fn array(&self, element: TypeId) -> TypeId {
        self.interner_ref().array(element)
    }

    fn tuple(&self, elements: Vec<TupleElement>) -> TypeId {
        self.interner_ref().tuple(elements)
    }

    fn object(&self, properties: Vec<PropertyInfo>) -> TypeId {
        self.interner_ref().object(properties)
    }

    fn object_with_index(&self, shape: ObjectShape) -> TypeId {
        self.interner_ref().object_with_index(shape)
    }

    fn function(&self, shape: FunctionShape) -> TypeId {
        self.interner_ref().function(shape)
    }

    fn callable(&self, shape: CallableShape) -> TypeId {
        self.interner_ref().callable(shape)
    }

    fn template_literal(&self, spans: Vec<TemplateSpan>) -> TypeId {
        self.interner_ref().template_literal(spans)
    }

    fn conditional(&self, conditional: ConditionalType) -> TypeId {
        self.interner_ref().conditional(conditional)
    }

    fn mapped(&self, mapped: MappedType) -> TypeId {
        self.interner_ref().mapped(mapped)
    }

    fn reference(&self, symbol: SymbolRef) -> TypeId {
        self.interner_ref().reference(symbol)
    }

    fn application(&self, base: TypeId, args: Vec<TypeId>) -> TypeId {
        self.interner_ref().application(base, args)
    }

    fn literal_string_atom(&self, atom: tsz_common::interner::Atom) -> TypeId {
        self.interner_ref().literal_string_atom(atom)
    }

    fn union_preserve_members(&self, members: Vec<TypeId>) -> TypeId {
        self.interner_ref().union_preserve_members(members)
    }

    fn readonly_type(&self, inner: TypeId) -> TypeId {
        self.interner_ref().readonly_type(inner)
    }

    fn intersect_types_raw2(&self, left: TypeId, right: TypeId) -> TypeId {
        self.interner_ref().intersect_types_raw2(left, right)
    }

    fn object_with_flags(&self, properties: Vec<PropertyInfo>, flags: ObjectFlags) -> TypeId {
        self.interner_ref().object_with_flags(properties, flags)
    }

    fn get_class_base_type(&self, _symbol_id: SymbolId) -> Option<TypeId> {
        // SalsaDatabase doesn't have access to the Binder, so it can't resolve base classes.
        // The Checker will override this to provide the actual implementation.
        None
    }

    fn is_unit_type(&self, type_id: TypeId) -> bool {
        self.interner_ref().is_unit_type(type_id)
    }
}

/// Implement QueryDatabase trait for SalsaDatabase.
impl crate::db::QueryDatabase for SalsaDatabase {
    fn as_type_database(&self) -> &dyn crate::db::TypeDatabase {
        self
    }

    fn evaluate_type(&self, type_id: TypeId) -> TypeId {
        self.evaluate_type_query(type_id)
    }

    fn is_subtype_of(&self, source: TypeId, target: TypeId) -> bool {
        self.is_subtype_of_query(source, target)
    }

    fn get_index_signatures(&self, type_id: TypeId) -> crate::IndexInfo {
        self.interner_ref().get_index_signatures(type_id)
    }

    fn is_nullish_type(&self, type_id: TypeId) -> bool {
        crate::narrowing::is_nullish_type(self.interner_ref().as_ref(), type_id)
    }

    fn remove_nullish(&self, type_id: TypeId) -> TypeId {
        crate::narrowing::remove_nullish(self.interner_ref().as_ref(), type_id)
    }

    fn is_assignable_to(&self, source: TypeId, target: TypeId) -> bool {
        // Use CompatChecker with all compatibility rules
        let mut checker = CompatChecker::new(self);
        checker.is_assignable(source, target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{QueryDatabase, TypeDatabase};
    use crate::types::IntrinsicKind;

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

        // any is assignable to string (TypeScript: any is both top and bottom)
        assert!(db.is_subtype_of(TypeId::ANY, TypeId::STRING));

        // number is not a subtype of string
        assert!(!db.is_subtype_of(TypeId::NUMBER, TypeId::STRING));
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
