//! Type Query Functions — Core Implementation
//!
//! This module contains the implementation of type query functions.
//! The parent `mod.rs` re-exports everything; callers should use `type_queries::*`.

use crate::types::{IntrinsicKind, LiteralValue};
use crate::{QueryDatabase, TypeData, TypeDatabase, TypeId};

use super::classifiers::get_lazy_def_id;
use super::data::get_type_parameter_constraint;
use super::traversal::collect_property_name_atoms_for_diagnostics;

pub fn get_allowed_keys(db: &dyn TypeDatabase, type_id: TypeId) -> rustc_hash::FxHashSet<String> {
    if let Some(exact) = super::data::collect_exact_literal_property_keys(db, type_id) {
        return exact.into_iter().map(|a| db.resolve_atom(a)).collect();
    }
    let atoms = collect_property_name_atoms_for_diagnostics(db, type_id, 10);
    atoms.into_iter().map(|a| db.resolve_atom(a)).collect()
}

// =============================================================================
// Core Type Queries
// =============================================================================

/// Check if a type is a callable type (function or callable with signatures).
///
/// Returns true for `TypeData::Callable` and `TypeData::Function` types.
pub fn is_callable_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(TypeData::Callable(_) | TypeData::Function(_))
    )
}

/// Check if a type is structurally the Function interface from lib.d.ts.
///
/// The Function interface may be lowered as an `Object` (without call signatures)
/// due to cross-arena declaration splitting. This detects it by checking for the
/// characteristic properties: `apply`, `call`, and `bind`.
pub fn is_function_interface_structural(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    use crate::visitor::{object_shape_id, object_with_index_shape_id};
    let shape_id = object_shape_id(db, type_id).or_else(|| object_with_index_shape_id(db, type_id));
    let Some(shape_id) = shape_id else {
        return false;
    };
    let shape = db.object_shape(shape_id);
    // Function interface has ~8 own properties + ~7 inherited Object properties = ~15.
    // Cap at 20 to avoid false positives on large interfaces.
    if shape.properties.len() > 20 {
        return false;
    }
    let apply = db.intern_string("apply");
    let call = db.intern_string("call");
    let bind = db.intern_string("bind");
    shape.properties.iter().any(|p| p.name == apply)
        && shape.properties.iter().any(|p| p.name == call)
        && shape.properties.iter().any(|p| p.name == bind)
}

/// Get the number of elements in a fixed-length tuple type.
///
/// Returns `Some(len)` for tuple types with no rest elements, `None` otherwise
/// (arrays, non-tuples, variadic tuples with rest elements).
pub fn get_fixed_tuple_length(db: &dyn TypeDatabase, type_id: TypeId) -> Option<usize> {
    if let Some(TypeData::Tuple(tuple_list_id)) = db.lookup(type_id) {
        let elements = db.tuple_list(tuple_list_id);
        if elements.iter().all(|e| !e.rest) {
            return Some(elements.len());
        }
    }
    None
}

/// Check if a type is invokable (has call signatures, not just construct signatures).
///
/// This is more specific than `is_callable_type` - it ensures the type can be called
/// as a function (not just constructed with `new`).
///
/// # Arguments
///
/// * `db` - The type database/interner
/// * `type_id` - The type to check
///
/// # Returns
///
/// * `true` - If the type has call signatures
/// * `false` - Otherwise
///
/// # Examples
///
/// ```text
/// // Functions are invokable
/// assert!(is_invokable_type(&db, function_type));
///
/// // Callables with call signatures are invokable
/// assert!(is_invokable_type(&db, callable_with_call_sigs));
///
/// // Callables with ONLY construct signatures are NOT invokable
/// assert!(!is_invokable_type(&db, class_constructor_only));
/// ```
pub fn is_invokable_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(TypeData::Function(_)) => true,
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            // Must have at least one call signature (not just construct signatures)
            !shape.call_signatures.is_empty()
        }
        // Intersections might contain a callable
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().any(|&m| is_invokable_type(db, m))
        }
        _ => false,
    }
}

/// Check if a type is an object type (with or without index signatures).
///
/// Returns true for `TypeData::Object` and `TypeData::ObjectWithIndex`.
pub fn is_object_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(TypeData::Object(_) | TypeData::ObjectWithIndex(_))
    )
}

/// Check if a type has named properties (non-empty property list).
///
/// Returns true for object types with at least one named property.
/// Used to determine if a contextual type can provide property-level
/// type information for class expressions.
pub fn has_properties(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            !db.object_shape(shape_id).properties.is_empty()
        }
        Some(TypeData::Union(members)) => {
            // A union has properties if any non-undefined/null member does.
            let members = db.type_list(members);
            members
                .iter()
                .any(|&m| m != TypeId::UNDEFINED && m != TypeId::NULL && has_properties(db, m))
        }
        _ => false,
    }
}

/// Check if an object type has a nominal symbol (class/interface instance).
///
/// Returns true when the type is an Object or `ObjectWithIndex` with a
/// non-None `symbol` field, indicating it was created from a named class
/// or interface declaration.
pub fn has_nominal_symbol(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            db.object_shape(shape_id).symbol.is_some()
        }
        _ => false,
    }
}

/// Check if a type is a generic type application (Base<Args>).
///
/// Returns true for `TypeData::Application`.
pub fn is_generic_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeData::Application(_)))
}

/// Check if a type is a named type reference.
///
/// Returns true for `TypeData::Lazy(DefId)` (interfaces, classes, type aliases).
pub fn is_type_reference(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(TypeData::Lazy(_) | TypeData::Recursive(_) | TypeData::BoundParameter(_))
    )
}

/// Returns true for `TypeData::TypeParameter`, `TypeData::BoundParameter`,
/// and `TypeData::Infer`. `BoundParameter` is included because it represents
/// a type parameter that has been bound to a specific index in a generic
/// signature — it should still be treated as "unresolved" for purposes like
/// excess property checking and constraint validation.
///
/// Use this instead of `visitor_predicates::is_type_parameter` when you need
/// to treat bound (de Bruijn indexed) parameters as type-parameter-like.
pub fn is_type_parameter_like(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(TypeData::TypeParameter(_) | TypeData::BoundParameter(_) | TypeData::Infer(_))
    )
}

/// Check if a type is a keyof type.
///
/// Returns true for `TypeData::KeyOf`.
pub fn is_keyof_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeData::KeyOf(_)))
}

/// Check if a type is a readonly type modifier.
///
/// Returns true for `TypeData::ReadonlyType`.
pub fn is_readonly_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeData::ReadonlyType(_)))
}

/// Check if a type is the polymorphic `this` type.
///
/// `ThisType` represents `this` in class methods and needs to be resolved
/// to the concrete class type before property access.
pub fn is_this_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeData::ThisType))
}

/// Check if a type is `symbol` or a `unique symbol` type.
///
/// Returns true for the built-in `symbol` type and for `TypeData::UniqueSymbol`.
/// Check if a type is a unique symbol (not plain `symbol`).
///
/// Returns true only for `TypeData::UniqueSymbol` types, which represent
/// individual `typeof sym` types created for const symbol declarations.
pub fn is_unique_symbol_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeData::UniqueSymbol(_)))
}

pub fn is_symbol_or_unique_symbol_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::SYMBOL {
        return true;
    }
    matches!(
        db.lookup(type_id),
        Some(TypeData::UniqueSymbol(_) | TypeData::Intrinsic(crate::IntrinsicKind::Symbol))
    )
}

/// Check if a type is usable as a property name (TS1166/TS1165/TS1169).
///
/// Returns true for string literals, number literals, and unique symbol types.
/// This corresponds to TypeScript's `isTypeUsableAsPropertyName` check.
pub fn is_type_usable_as_property_name(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(
            TypeData::Literal(crate::LiteralValue::String(_))
                | TypeData::Literal(crate::LiteralValue::Number(_))
                | TypeData::UniqueSymbol(_)
        )
    )
}

/// Check if a type needs evaluation before interface merging.
///
/// Returns true for Application and Lazy types, which are meta-types that
/// may resolve to Object/Callable types when evaluated. Used before
/// `classify_for_interface_merge` to ensure that type-alias-based heritage
/// (e.g., `interface X extends TypeAlias<T>`) is properly resolved.
pub fn needs_evaluation_for_merge(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(TypeData::Application(_) | TypeData::Lazy(_))
    )
}

/// Get the return type of a function type.
///
/// Returns `TypeId::ERROR` if the type is not a Function.
pub fn get_function_return_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    match db.lookup(type_id) {
        Some(TypeData::Function(shape_id)) => db.function_shape(shape_id).return_type,
        _ => TypeId::ERROR,
    }
}

/// Get the parameter types of a function type.
///
/// Returns an empty vector if the type is not a Function.
pub fn get_function_parameter_types(db: &dyn TypeDatabase, type_id: TypeId) -> Vec<TypeId> {
    match db.lookup(type_id) {
        Some(TypeData::Function(shape_id)) => db
            .function_shape(shape_id)
            .params
            .iter()
            .map(|p| p.type_id)
            .collect(),
        _ => Vec::new(),
    }
}

// =============================================================================
// Intrinsic Type Queries
// =============================================================================
//
// These functions provide TypeData-free checking for intrinsic types.
// Checker code should use these instead of matching on TypeData::Intrinsic.
//
// ## Important Usage Notes
//
// These are TYPE IDENTITY checks, NOT compatibility checks:
//
// - Identity: `is_string_type(TypeId::STRING)` -> TRUE
// - Identity: `is_string_type(literal "hello")` -> FALSE (literal, not intrinsic)
// - Identity: `is_string_type(string & {tag: 1})` -> FALSE (intersection, not intrinsic)
//
// For assignability/compatibility checks, use Solver subtyping:
// - `solver.is_subtype_of(literal, TypeId::STRING)` -> TRUE
// - `solver.is_subtype_of(branded, TypeId::STRING)` -> TRUE (if assignable)
//
// ### When to use these helpers
// - Checking if a type annotation is explicitly the intrinsic keyword
// - Validating type constructor arguments
// - Distinguishing `void` from `undefined` in return types
//
// ### When NOT to use these helpers
// - Assignment/compatibility checks -> Use `is_subtype_of` instead
// - Type narrowing -> Use Solver's narrowing analysis
// - Checking if a value IS a string (not literal) -> Use `is_subtype_of`
//
// ## Implementation Notes
// - Shallow queries: do NOT resolve Lazy/Ref (caller's responsibility)
// - Defensive pattern: check both TypeId constants AND TypeData::Intrinsic
// - Fast-path O(1) using TypeId integer comparison

/// Generate an intrinsic type checker function that checks both the well-known
/// `TypeId` constant and the `TypeData::Intrinsic` variant.
macro_rules! define_intrinsic_check {
    ($fn_name:ident, $type_id:ident, $kind:ident) => {
        pub fn $fn_name(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
            type_id == TypeId::$type_id
                || matches!(
                    db.lookup(type_id),
                    Some(TypeData::Intrinsic(IntrinsicKind::$kind))
                )
        }
    };
}

define_intrinsic_check!(is_any_type, ANY, Any);
define_intrinsic_check!(is_unknown_type, UNKNOWN, Unknown);
define_intrinsic_check!(is_never_type, NEVER, Never);
define_intrinsic_check!(is_void_type, VOID, Void);
define_intrinsic_check!(is_undefined_type, UNDEFINED, Undefined);
define_intrinsic_check!(is_null_type, NULL, Null);
define_intrinsic_check!(is_string_type, STRING, String);
define_intrinsic_check!(is_number_type, NUMBER, Number);
define_intrinsic_check!(is_bigint_type, BIGINT, Bigint);
define_intrinsic_check!(is_boolean_type, BOOLEAN, Boolean);
define_intrinsic_check!(is_symbol_type, SYMBOL, Symbol);

/// Check if a type is `symbol` or a `unique symbol`.
pub fn is_symbol_or_unique_symbol(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::SYMBOL {
        return true;
    }
    matches!(
        db.lookup(type_id),
        Some(TypeData::UniqueSymbol(_)) | Some(TypeData::Intrinsic(crate::IntrinsicKind::Symbol))
    )
}

// =============================================================================
// Composite Type Queries
// =============================================================================

/// Check if a type is valid for object spreading (`{...x}`).
///
/// Matches tsc's `isValidSpreadType()` behavior:
/// 1. Resolve type parameters to their base constraints
/// 2. Remove definitely-falsy types (false, 0, "", null, undefined, void, never)
/// 3. Check if remaining type is an object-like/any/instantiable type
///
/// Returns `true` for types that can be spread into an object literal:
/// - `any`, `never`, `error` (always spreadable)
/// - Object types, arrays, tuples, functions, callables, mapped types
/// - `object` intrinsic (non-primitive)
/// - Type parameters whose constraint is spreadable
/// - Unions where non-falsy members are all spreadable
/// - Intersections where all members are spreadable
///
/// Returns `false` for primitive types (`number`, `string`, `boolean`, etc.),
/// literals that aren't definitely-falsy, `unknown`, and types that resolve
/// to these after constraint resolution.
pub fn is_valid_spread_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_valid_spread_type_impl(db, type_id, 0)
}

/// Check if a type is definitely falsy (always falsy at runtime).
///
/// Definitely-falsy types: `null`, `undefined`, `void`, `never`,
/// literal `false`, literal `0`/`-0`/`NaN`, literal `""`, literal `0n`.
fn is_definitely_falsy_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // null, undefined, void, never are always falsy
    if type_id.is_nullable() || type_id == TypeId::NEVER {
        return true;
    }
    match db.lookup(type_id) {
        Some(TypeData::Intrinsic(
            IntrinsicKind::Void | IntrinsicKind::Null | IntrinsicKind::Undefined,
        )) => true,
        Some(TypeData::Literal(lit)) => match lit {
            LiteralValue::Boolean(b) => !b,
            LiteralValue::Number(n) => n.0 == 0.0 || n.0.is_nan(),
            LiteralValue::String(atom) => db.resolve_atom_ref(atom).is_empty(),
            LiteralValue::BigInt(atom) => db.resolve_atom_ref(atom).as_ref() == "0",
        },
        // Intersection: if ANY member is definitely falsy, the intersection is falsy.
        // e.g., `T & undefined` is always falsy because the value must be undefined.
        Some(TypeData::Intersection(members)) => {
            let members = db.type_list(members);
            members.iter().any(|&m| is_definitely_falsy_type(db, m))
        }
        // Type parameters: check if the constraint is definitely falsy.
        // e.g., `T extends undefined` is definitely falsy.
        Some(TypeData::TypeParameter(info)) => info
            .constraint
            .is_some_and(|c| is_definitely_falsy_type(db, c)),
        _ => false,
    }
}

/// Resolve a type parameter to its base constraint, or return the type itself.
/// Matches tsc's `getBaseConstraintOrType()`.
fn get_base_constraint_or_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    get_type_parameter_constraint(db, type_id).unwrap_or(type_id)
}

fn is_valid_spread_type_impl(db: &dyn TypeDatabase, type_id: TypeId, depth: u32) -> bool {
    if depth > 20 {
        return true;
    }

    // Step 1: Resolve type parameter to its base constraint (like tsc's getBaseConstraintOrType)
    let resolved = get_base_constraint_or_type(db, type_id);

    match resolved {
        TypeId::ANY | TypeId::NEVER | TypeId::ERROR => return true,
        _ => {}
    }

    match db.lookup(resolved) {
        // Primitives, null/undefined/void, literals, template literals, string intrinsics:
        // not spreadable on their own.
        // (Definitely-falsy members are filtered out in the union branch instead.)
        Some(
            TypeData::Intrinsic(
                IntrinsicKind::String
                | IntrinsicKind::Number
                | IntrinsicKind::Boolean
                | IntrinsicKind::Bigint
                | IntrinsicKind::Symbol
                | IntrinsicKind::Unknown
                | IntrinsicKind::Void
                | IntrinsicKind::Null
                | IntrinsicKind::Undefined,
            )
            | TypeData::Literal(_)
            | TypeData::TemplateLiteral(_)
            | TypeData::StringIntrinsic { .. },
        ) => false,
        // Union: remove definitely-falsy members, then check remaining.
        // Matches tsc's removeDefinitelyFalsyTypes before checking.
        Some(TypeData::Union(members)) => {
            let members = db.type_list(members);
            // Filter out definitely-falsy types, then check if all remaining are valid
            let non_falsy: Vec<TypeId> = members
                .iter()
                .copied()
                .filter(|&m| !is_definitely_falsy_type(db, m))
                .collect();
            // If nothing remains after removing falsy types, the spread is invalid
            // (entirely falsy union like `false | null`)
            if non_falsy.is_empty() {
                return false;
            }
            non_falsy
                .iter()
                .all(|&m| is_valid_spread_type_impl(db, m, depth + 1))
        }
        // Intersection: all members must be spreadable (after constraint resolution)
        Some(TypeData::Intersection(members)) => {
            let members = db.type_list(members);
            members
                .iter()
                .all(|&m| is_valid_spread_type_impl(db, m, depth + 1))
        }
        Some(TypeData::ReadonlyType(inner)) => is_valid_spread_type_impl(db, inner, depth + 1),
        // Everything else is spreadable: object types, arrays, tuples, functions,
        // callables, mapped types, type parameters (unconstrained ones reach here
        // and are valid per tsc's InstantiableNonPrimitive), lazy refs, applications, etc.
        _ => true,
    }
}

// =============================================================================
// Constructor Type Collection Helpers
// =============================================================================

/// Result of classifying a type for constructor collection.
///
/// This enum tells the caller what kind of type this is and how to proceed
/// when collecting constructor types from a composite type structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstructorTypeKind {
    /// This is a Callable type - always a constructor type
    Callable,
    /// This is a Function type - check `is_constructor` flag on the shape
    Function(crate::types::FunctionShapeId),
    /// Recurse into these member types (Union, Intersection)
    Members(Vec<TypeId>),
    /// Recurse into the inner type (`ReadonlyType`)
    Inner(TypeId),
    /// Recurse into the constraint (`TypeParameter`, Infer)
    Constraint(Option<TypeId>),
    /// This type needs full type evaluation (Conditional, Mapped, `IndexAccess`, `KeyOf`)
    NeedsTypeEvaluation,
    /// This is a generic application that needs instantiation
    NeedsApplicationEvaluation,
    /// This is a `TypeQuery` - resolve the symbol reference to get its type
    TypeQuery(crate::types::SymbolRef),
    /// This type cannot be a constructor (primitives, literals, etc.)
    NotConstructor,
}

/// Classify a type for constructor type collection.
///
/// This function examines a `TypeData` and returns information about how to handle it
/// when collecting constructor types. The caller is responsible for:
/// - Checking the `is_constructor` flag for Function types
/// - Evaluating types when `NeedsTypeEvaluation` or `NeedsApplicationEvaluation` is returned
/// - Resolving symbol references for `TypeQuery`
/// - Recursing into members/inner types
pub fn classify_constructor_type(db: &dyn TypeDatabase, type_id: TypeId) -> ConstructorTypeKind {
    let Some(key) = db.lookup(type_id) else {
        return ConstructorTypeKind::NotConstructor;
    };

    match key {
        TypeData::Callable(_) => ConstructorTypeKind::Callable,
        TypeData::Function(shape_id) => ConstructorTypeKind::Function(shape_id),
        TypeData::Intersection(members_id) | TypeData::Union(members_id) => {
            let members = db.type_list(members_id);
            ConstructorTypeKind::Members(members.to_vec())
        }
        TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
            ConstructorTypeKind::Inner(inner)
        }
        TypeData::TypeParameter(info) | TypeData::Infer(info) => {
            ConstructorTypeKind::Constraint(info.constraint)
        }
        TypeData::Conditional(_)
        | TypeData::Mapped(_)
        | TypeData::IndexAccess(_, _)
        | TypeData::KeyOf(_) => ConstructorTypeKind::NeedsTypeEvaluation,
        TypeData::Application(_) => ConstructorTypeKind::NeedsApplicationEvaluation,
        TypeData::TypeQuery(sym_ref) => ConstructorTypeKind::TypeQuery(sym_ref),
        // All other types cannot be constructors
        TypeData::Enum(_, _)
        | TypeData::BoundParameter(_)
        | TypeData::Intrinsic(_)
        | TypeData::Literal(_)
        | TypeData::Object(_)
        | TypeData::ObjectWithIndex(_)
        | TypeData::Array(_)
        | TypeData::Tuple(_)
        | TypeData::Lazy(_)
        | TypeData::Recursive(_)
        | TypeData::TemplateLiteral(_)
        | TypeData::UniqueSymbol(_)
        | TypeData::ThisType
        | TypeData::StringIntrinsic { .. }
        | TypeData::ModuleNamespace(_)
        | TypeData::Error => ConstructorTypeKind::NotConstructor,
    }
}

// =============================================================================
// Static Property Collection Helpers
// =============================================================================

/// Result of extracting static properties from a type.
///
/// This enum allows the caller to handle recursion and type evaluation
/// while keeping the `TypeData` matching logic in the solver layer.
#[derive(Debug, Clone)]
pub enum StaticPropertySource {
    /// Direct properties from Callable, Object, or `ObjectWithIndex` types.
    Properties(Vec<crate::PropertyInfo>),
    /// Member types that should be recursively processed (Union/Intersection).
    RecurseMembers(Vec<TypeId>),
    /// Single type to recurse into (`TypeParameter` constraint, `ReadonlyType` inner).
    RecurseSingle(TypeId),
    /// Type that needs evaluation before property extraction (Conditional, Mapped, etc.).
    NeedsEvaluation,
    /// Type that needs application evaluation (Application type).
    NeedsApplicationEvaluation,
    /// No properties available (primitives, error types, etc.).
    None,
}

/// Extract static property information from a type.
///
/// This function handles the `TypeData` matching for property collection,
/// returning a `StaticPropertySource` that tells the caller how to proceed.
/// The caller is responsible for:
/// - Handling recursion for `RecurseMembers` and `RecurseSingle` cases
/// - Evaluating types for `NeedsEvaluation` and `NeedsApplicationEvaluation` cases
/// - Tracking visited types to prevent infinite loops
pub fn get_static_property_source(db: &dyn TypeDatabase, type_id: TypeId) -> StaticPropertySource {
    let Some(key) = db.lookup(type_id) else {
        return StaticPropertySource::None;
    };

    match key {
        TypeData::Callable(shape_id) => {
            let shape = db.callable_shape(shape_id);
            StaticPropertySource::Properties(shape.properties.to_vec())
        }
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = db.object_shape(shape_id);
            StaticPropertySource::Properties(shape.properties.to_vec())
        }
        TypeData::Intersection(members_id) | TypeData::Union(members_id) => {
            let members = db.type_list(members_id);
            StaticPropertySource::RecurseMembers(members.to_vec())
        }
        TypeData::TypeParameter(info) | TypeData::Infer(info) => {
            if let Some(constraint) = info.constraint {
                StaticPropertySource::RecurseSingle(constraint)
            } else {
                StaticPropertySource::None
            }
        }
        TypeData::ReadonlyType(inner) => StaticPropertySource::RecurseSingle(inner),
        TypeData::Conditional(_)
        | TypeData::Mapped(_)
        | TypeData::IndexAccess(_, _)
        | TypeData::KeyOf(_) => StaticPropertySource::NeedsEvaluation,
        TypeData::Application(_) => StaticPropertySource::NeedsApplicationEvaluation,
        _ => StaticPropertySource::None,
    }
}

// =============================================================================
// Construct Signature Queries
// =============================================================================

/// Check if a Callable type has construct signatures.
///
/// Returns true only for Callable types that have non-empty `construct_signatures`.
/// This is a direct check and does not resolve through Ref or `TypeQuery` types.
pub fn has_construct_signatures(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            !shape.construct_signatures.is_empty()
        }
        _ => false,
    }
}

// =============================================================================
// Signature Classification
// =============================================================================

/// Classification for types when extracting call/construct signatures.
#[derive(Debug, Clone)]
pub enum SignatureTypeKind {
    /// Callable type with `shape_id` - has `call_signatures` and `construct_signatures`
    Callable(crate::types::CallableShapeId),
    /// Function type with `shape_id` - has single signature
    Function(crate::types::FunctionShapeId),
    /// Union type - get signatures from each member
    Union(Vec<TypeId>),
    /// Intersection type - get signatures from each member
    Intersection(Vec<TypeId>),
    /// Readonly wrapper - unwrap and get signatures from inner type
    ReadonlyType(TypeId),
    /// Type parameter with optional constraint - may need to check constraint
    TypeParameter { constraint: Option<TypeId> },
    /// Types that need evaluation before signature extraction (Conditional, Mapped, `IndexAccess`, `KeyOf`)
    NeedsEvaluation(TypeId),
    /// Types without signatures (Intrinsic, Literal, Object without callable, etc.)
    NoSignatures,
}

/// Classify a type for signature extraction.
pub fn classify_for_signatures(db: &dyn TypeDatabase, type_id: TypeId) -> SignatureTypeKind {
    // Handle special TypeIds first
    if type_id == TypeId::ERROR || type_id == TypeId::NEVER {
        return SignatureTypeKind::NoSignatures;
    }
    if type_id == TypeId::ANY {
        // any is callable but has no concrete signatures
        return SignatureTypeKind::NoSignatures;
    }

    let Some(key) = db.lookup(type_id) else {
        return SignatureTypeKind::NoSignatures;
    };

    match key {
        // Callable types - have call_signatures and construct_signatures
        TypeData::Callable(shape_id) => SignatureTypeKind::Callable(shape_id),

        // Function types - have a single signature
        TypeData::Function(shape_id) => SignatureTypeKind::Function(shape_id),

        // Union type - get signatures from each member
        TypeData::Union(members_id) => {
            let members = db.type_list(members_id);
            SignatureTypeKind::Union(members.to_vec())
        }

        // Intersection type - get signatures from each member
        TypeData::Intersection(members_id) => {
            let members = db.type_list(members_id);
            SignatureTypeKind::Intersection(members.to_vec())
        }

        // Readonly wrapper - unwrap and recurse
        TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
            SignatureTypeKind::ReadonlyType(inner)
        }

        // Type parameter - may have constraint with signatures
        TypeData::TypeParameter(info) | TypeData::Infer(info) => SignatureTypeKind::TypeParameter {
            constraint: info.constraint,
        },

        // Complex types that need evaluation before signature extraction
        TypeData::Conditional(_)
        | TypeData::Mapped(_)
        | TypeData::IndexAccess(_, _)
        | TypeData::KeyOf(_) => SignatureTypeKind::NeedsEvaluation(type_id),

        // All other types don't have callable signatures
        TypeData::BoundParameter(_)
        | TypeData::Intrinsic(_)
        | TypeData::Literal(_)
        | TypeData::Object(_)
        | TypeData::ObjectWithIndex(_)
        | TypeData::Array(_)
        | TypeData::Tuple(_)
        | TypeData::Lazy(_)
        | TypeData::Recursive(_)
        | TypeData::Application(_)
        | TypeData::TemplateLiteral(_)
        | TypeData::UniqueSymbol(_)
        | TypeData::ThisType
        | TypeData::TypeQuery(_)
        | TypeData::StringIntrinsic { .. }
        | TypeData::ModuleNamespace(_)
        | TypeData::Enum(_, _)
        | TypeData::Error => SignatureTypeKind::NoSignatures,
    }
}

// =============================================================================
// EvaluationNeeded - Classification for types that need evaluation
// =============================================================================

/// Classification for types that need evaluation before use.
#[derive(Debug, Clone)]
pub enum EvaluationNeeded {
    /// Already resolved, no evaluation needed
    Resolved(TypeId),
    /// Symbol reference - resolve symbol first
    SymbolRef(crate::types::SymbolRef),
    /// Type query (typeof) - evaluate first
    TypeQuery(crate::types::SymbolRef),
    /// Generic application - instantiate first
    Application {
        app_id: crate::types::TypeApplicationId,
    },
    /// Index access T[K] - evaluate with environment
    IndexAccess { object: TypeId, index: TypeId },
    /// `KeyOf` type - evaluate
    KeyOf(TypeId),
    /// Mapped type - evaluate
    Mapped {
        mapped_id: crate::types::MappedTypeId,
    },
    /// Conditional type - evaluate
    Conditional {
        cond_id: crate::types::ConditionalTypeId,
    },
    /// Callable type (for contextual typing checks)
    Callable(crate::types::CallableShapeId),
    /// Function type
    Function(crate::types::FunctionShapeId),
    /// Union - may need per-member evaluation
    Union(Vec<TypeId>),
    /// Intersection - may need per-member evaluation
    Intersection(Vec<TypeId>),
    /// Type parameter with constraint
    TypeParameter { constraint: Option<TypeId> },
    /// Readonly wrapper - unwrap
    Readonly(TypeId),
}

/// Classify a type for what kind of evaluation it needs.
pub fn classify_for_evaluation(db: &dyn TypeDatabase, type_id: TypeId) -> EvaluationNeeded {
    let Some(key) = db.lookup(type_id) else {
        return EvaluationNeeded::Resolved(type_id);
    };

    match key {
        TypeData::TypeQuery(sym_ref) => EvaluationNeeded::TypeQuery(sym_ref),
        TypeData::Application(app_id) => EvaluationNeeded::Application { app_id },
        TypeData::IndexAccess(object, index) => EvaluationNeeded::IndexAccess { object, index },
        TypeData::KeyOf(inner) => EvaluationNeeded::KeyOf(inner),
        TypeData::Mapped(mapped_id) => EvaluationNeeded::Mapped { mapped_id },
        TypeData::Conditional(cond_id) => EvaluationNeeded::Conditional { cond_id },
        TypeData::Callable(shape_id) => EvaluationNeeded::Callable(shape_id),
        TypeData::Function(shape_id) => EvaluationNeeded::Function(shape_id),
        TypeData::Union(list_id) => {
            let members = db.type_list(list_id);
            EvaluationNeeded::Union(members.to_vec())
        }
        TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            EvaluationNeeded::Intersection(members.to_vec())
        }
        TypeData::TypeParameter(info) | TypeData::Infer(info) => EvaluationNeeded::TypeParameter {
            constraint: info.constraint,
        },
        TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
            EvaluationNeeded::Readonly(inner)
        }
        // Already resolved types (Lazy needs special handling when DefId lookup is implemented)
        TypeData::BoundParameter(_)
        | TypeData::Intrinsic(_)
        | TypeData::Literal(_)
        | TypeData::Object(_)
        | TypeData::ObjectWithIndex(_)
        | TypeData::Array(_)
        | TypeData::Tuple(_)
        | TypeData::Lazy(_)
        | TypeData::Recursive(_)
        | TypeData::TemplateLiteral(_)
        | TypeData::UniqueSymbol(_)
        | TypeData::ThisType
        | TypeData::StringIntrinsic { .. }
        | TypeData::ModuleNamespace(_)
        | TypeData::Enum(_, _)
        | TypeData::Error => EvaluationNeeded::Resolved(type_id),
    }
}

/// Evaluate contextual wrapper structure while delegating leaf evaluation.
///
/// Solver owns traversal over semantic type shape; caller provides the concrete
/// leaf evaluator (for example checker's judge-based environment evaluation).
pub fn evaluate_contextual_structure_with(
    db: &dyn QueryDatabase,
    type_id: TypeId,
    evaluate_leaf: &mut dyn FnMut(TypeId) -> TypeId,
) -> TypeId {
    fn visit(
        db: &dyn QueryDatabase,
        type_id: TypeId,
        evaluate_leaf: &mut dyn FnMut(TypeId) -> TypeId,
    ) -> TypeId {
        match classify_for_evaluation(db, type_id) {
            EvaluationNeeded::Union(members) => {
                let mut changed = false;
                let evaluated: Vec<TypeId> = members
                    .iter()
                    .map(|&member| {
                        let ev = visit(db, member, evaluate_leaf);
                        if ev != member {
                            changed = true;
                        }
                        ev
                    })
                    .collect();
                if changed {
                    db.factory().union(evaluated)
                } else {
                    type_id
                }
            }
            EvaluationNeeded::Intersection(members) => {
                let mut changed = false;
                let evaluated: Vec<TypeId> = members
                    .iter()
                    .map(|&member| {
                        let ev = visit(db, member, evaluate_leaf);
                        if ev != member {
                            changed = true;
                        }
                        ev
                    })
                    .collect();
                if changed {
                    db.factory().intersection(evaluated)
                } else {
                    type_id
                }
            }
            EvaluationNeeded::Application { .. }
            | EvaluationNeeded::Mapped { .. }
            | EvaluationNeeded::Conditional { .. } => {
                let evaluated = evaluate_leaf(type_id);
                if evaluated != type_id {
                    evaluated
                } else {
                    type_id
                }
            }
            _ if get_lazy_def_id(db, type_id).is_some() => {
                let evaluated = evaluate_leaf(type_id);
                if evaluated != type_id {
                    evaluated
                } else {
                    type_id
                }
            }
            _ => type_id,
        }
    }

    visit(db, type_id, evaluate_leaf)
}

// =============================================================================
// Compound Type Classification Queries
// =============================================================================

/// Check if a type is a type parameter at the top level, or is an intersection
/// that contains a type parameter member.
///
/// Used by generic call inference to determine whether excess property checking
/// should be skipped for a parameter position (because the type parameter
/// captures the full object type).
pub fn is_type_parameter_or_intersection_with_type_parameter(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    match db.lookup(type_id) {
        Some(TypeData::TypeParameter(_) | TypeData::BoundParameter(_) | TypeData::Infer(_)) => true,
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().any(|&m| {
                matches!(
                    db.lookup(m),
                    Some(
                        TypeData::TypeParameter(_)
                            | TypeData::BoundParameter(_)
                            | TypeData::Infer(_)
                    )
                )
            })
        }
        _ => false,
    }
}

/// Check if both types are application (generic instantiation) types and the
/// parameter type contains type parameters.
///
/// When true, the parameter type should be preserved without evaluation during
/// generic inference, because evaluating it would lose the type parameter
/// information needed for inference against the argument type.
pub fn should_preserve_application_for_inference(
    db: &dyn TypeDatabase,
    param_type: TypeId,
    arg_type: TypeId,
) -> bool {
    matches!(db.lookup(param_type), Some(TypeData::Application(_)))
        && matches!(db.lookup(arg_type), Some(TypeData::Application(_)))
        && super::data::contains_type_parameters_db(db, param_type)
}

/// Check if a type represents an unresolved inference result.
///
/// Returns true if the type is `error`, contains infer types, or transitively
/// references `error`. Used to detect provisional inference results from
/// Round 1 of generic call resolution that should not pollute outer inference.
pub fn is_unresolved_inference_result(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    type_id == TypeId::ERROR
        || super::data::contains_infer_types_db(db, type_id)
        || crate::visitor::collect_referenced_types(db, type_id).contains(&TypeId::ERROR)
}
