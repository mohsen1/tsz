//! Extended Type Query Functions
//!
//! This module contains additional type classification and query functions
//! that complement the core type queries in `type_queries.rs`.
//!
//! These functions provide structured classification enums for various
//! type-checking scenarios, allowing the checker layer to handle types
//! without directly matching on `TypeData`.

use crate::def::DefId;
use crate::{TypeData, TypeDatabase, TypeId};
use rustc_hash::FxHashSet;

// =============================================================================
// Full Literal Type Classification (includes boolean)
// =============================================================================

/// Classification for all literal types including boolean.
/// Used by `literal_type.rs` for comprehensive literal handling.
#[derive(Debug, Clone)]
pub enum LiteralTypeKind {
    /// String literal type with the atom for the string value
    String(tsz_common::interner::Atom),
    /// Number literal type with the numeric value
    Number(f64),
    /// `BigInt` literal type with the atom for the bigint value
    BigInt(tsz_common::interner::Atom),
    /// Boolean literal type with the boolean value
    Boolean(bool),
    /// Not a literal type
    NotLiteral,
}

/// Classify a type for literal type handling.
///
/// This function examines a type and returns information about what kind
/// of literal it is. Used for:
/// - Detecting string/number/boolean literals
/// - Extracting literal values
/// - Literal type comparison
pub fn classify_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> LiteralTypeKind {
    let Some(key) = db.lookup(type_id) else {
        return LiteralTypeKind::NotLiteral;
    };

    match key {
        TypeData::Literal(crate::LiteralValue::String(atom)) => LiteralTypeKind::String(atom),
        TypeData::Literal(crate::LiteralValue::Number(ordered_float)) => {
            LiteralTypeKind::Number(ordered_float.0)
        }
        TypeData::Literal(crate::LiteralValue::BigInt(atom)) => LiteralTypeKind::BigInt(atom),
        TypeData::Literal(crate::LiteralValue::Boolean(value)) => LiteralTypeKind::Boolean(value),
        _ => LiteralTypeKind::NotLiteral,
    }
}

/// Check if a type is a string literal type.
pub fn is_string_literal(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        classify_literal_type(db, type_id),
        LiteralTypeKind::String(_)
    )
}

/// Check if a type is a number literal type.
pub fn is_number_literal(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        classify_literal_type(db, type_id),
        LiteralTypeKind::Number(_)
    )
}

/// Check if a type is a boolean literal type.
pub fn is_boolean_literal(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        classify_literal_type(db, type_id),
        LiteralTypeKind::Boolean(_)
    )
}

/// Get string atom from a string literal type.
pub fn get_string_literal_atom(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_common::interner::Atom> {
    match classify_literal_type(db, type_id) {
        LiteralTypeKind::String(atom) => Some(atom),
        _ => None,
    }
}

/// Get number value from a number literal type.
pub fn get_number_literal_value(db: &dyn TypeDatabase, type_id: TypeId) -> Option<f64> {
    match classify_literal_type(db, type_id) {
        LiteralTypeKind::Number(value) => Some(value),
        _ => None,
    }
}

/// Get boolean value from a boolean literal type.
pub fn get_boolean_literal_value(db: &dyn TypeDatabase, type_id: TypeId) -> Option<bool> {
    match classify_literal_type(db, type_id) {
        LiteralTypeKind::Boolean(value) => Some(value),
        _ => None,
    }
}

// =============================================================================
// Index Type Classification
// =============================================================================

/// Check if a type cannot be used as an index type (TS2538).
///
/// A valid index type must be string/number/symbol (or compatible literal forms).
pub fn is_invalid_index_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let mut visited = FxHashSet::default();
    is_invalid_index_type_inner(db, type_id, &mut visited)
}

fn is_invalid_index_type_inner(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    visited: &mut FxHashSet<TypeId>,
) -> bool {
    if !visited.insert(type_id) {
        return false;
    }

    if matches!(
        type_id,
        TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR | TypeId::NEVER
    ) {
        return false;
    }

    match db.lookup(type_id) {
        Some(TypeData::Intrinsic(kind)) => matches!(
            kind,
            crate::IntrinsicKind::Void
                | crate::IntrinsicKind::Null
                | crate::IntrinsicKind::Undefined
                | crate::IntrinsicKind::Boolean
                | crate::IntrinsicKind::Bigint
                | crate::IntrinsicKind::Object
                | crate::IntrinsicKind::Function
        ),
        Some(TypeData::Literal(value)) => matches!(
            value,
            crate::LiteralValue::Boolean(_) | crate::LiteralValue::BigInt(_)
        ),
        Some(
            TypeData::Array(_)
            | TypeData::Tuple(_)
            | TypeData::Object(_)
            | TypeData::ObjectWithIndex(_)
            | TypeData::Function(_)
            | TypeData::Callable(_),
        ) => true,
        Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => db
            .type_list(list_id)
            .iter()
            .any(|&member| is_invalid_index_type_inner(db, member, visited)),
        Some(TypeData::TypeParameter(info)) => info
            .constraint
            .is_some_and(|constraint| is_invalid_index_type_inner(db, constraint, visited)),
        _ => false,
    }
}

// =============================================================================
// Spread Type Classification
// =============================================================================

/// Classification for spread operations.
///
/// This enum provides a structured way to handle spread types without
/// directly matching on `TypeData` in the checker layer.
#[derive(Debug, Clone)]
pub enum SpreadTypeKind {
    /// Array type - element type for spread
    Array(TypeId),
    /// Tuple type - can expand individual elements
    Tuple(crate::types::TupleListId),
    /// Object type - properties can be spread
    Object(crate::types::ObjectShapeId),
    /// Object with index signature
    ObjectWithIndex(crate::types::ObjectShapeId),
    /// String literal - can be spread as characters
    StringLiteral(tsz_common::interner::Atom),
    /// Lazy reference (`DefId`) - needs resolution to actual spreadable type
    Lazy(DefId),
    /// Type that needs further checks for iterability
    Other,
    /// Type that cannot be spread
    NotSpreadable,
}

/// Classify a type for spread operations.
///
/// This function examines a type and returns information about how to handle it
/// when used in a spread context.
pub fn classify_spread_type(db: &dyn TypeDatabase, type_id: TypeId) -> SpreadTypeKind {
    // Handle intrinsic types first
    if type_id.is_any() || type_id == TypeId::STRING {
        return SpreadTypeKind::Other;
    }
    if type_id.is_unknown() {
        return SpreadTypeKind::NotSpreadable;
    }

    let Some(key) = db.lookup(type_id) else {
        return SpreadTypeKind::NotSpreadable;
    };

    match key {
        TypeData::Array(element_type) => SpreadTypeKind::Array(element_type),
        TypeData::Tuple(tuple_id) => SpreadTypeKind::Tuple(tuple_id),
        TypeData::Object(shape_id) => SpreadTypeKind::Object(shape_id),
        TypeData::ObjectWithIndex(shape_id) => SpreadTypeKind::ObjectWithIndex(shape_id),
        TypeData::Literal(crate::LiteralValue::String(atom)) => SpreadTypeKind::StringLiteral(atom),
        TypeData::Lazy(def_id) => SpreadTypeKind::Lazy(def_id),
        _ => SpreadTypeKind::Other,
    }
}

/// Check if a type has Symbol.iterator or is otherwise iterable.
///
/// This is a helper for checking iterability without matching on `TypeData`.
pub fn is_iterable_type_kind(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // Handle intrinsic string type
    if type_id == TypeId::STRING {
        return true;
    }

    let Some(key) = db.lookup(type_id) else {
        return false;
    };

    match key {
        TypeData::Array(_)
        | TypeData::Tuple(_)
        | TypeData::Literal(crate::LiteralValue::String(_)) => true,
        TypeData::Object(shape_id) => {
            // Check for [Symbol.iterator] method
            let shape = db.object_shape(shape_id);
            shape.properties.iter().any(|prop| {
                let prop_name = db.resolve_atom_ref(prop.name);
                (prop_name.as_ref() == "[Symbol.iterator]" || prop_name.as_ref() == "next")
                    && prop.is_method
            })
        }
        _ => false,
    }
}

/// Get the iterable element type for a type if it's iterable.
pub fn get_iterable_element_type_from_db(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    // Handle intrinsic string type
    if type_id == TypeId::STRING {
        return Some(TypeId::STRING);
    }

    let key = db.lookup(type_id)?;

    match key {
        TypeData::Array(elem_type) => Some(elem_type),
        TypeData::Tuple(tuple_list_id) => {
            let elements = db.tuple_list(tuple_list_id);
            if elements.is_empty() {
                Some(TypeId::NEVER)
            } else {
                let types: Vec<TypeId> = elements.iter().map(|e| e.type_id).collect();
                Some(db.union(types))
            }
        }
        TypeData::Literal(crate::LiteralValue::String(_)) => Some(TypeId::STRING),
        TypeData::Object(shape_id) => {
            // For objects with [Symbol.iterator], we'd need to infer the element type
            // from the iterator's return type. For now, return Any as a fallback.
            let shape = db.object_shape(shape_id);
            let has_iterator = shape.properties.iter().any(|prop| {
                let prop_name = db.resolve_atom_ref(prop.name);
                (prop_name.as_ref() == "[Symbol.iterator]" || prop_name.as_ref() == "next")
                    && prop.is_method
            });
            has_iterator.then_some(TypeId::ANY)
        }
        _ => None,
    }
}

// =============================================================================
// Type Parameter Classification (Extended)
// =============================================================================

/// Classification for type parameter types.
///
/// This enum provides a structured way to handle type parameters without
/// directly matching on `TypeData` in the checker layer.
#[derive(Debug, Clone)]
pub enum TypeParameterKind {
    /// Type parameter with info
    TypeParameter(crate::types::TypeParamInfo),
    /// Infer type with info
    Infer(crate::types::TypeParamInfo),
    /// Type application - may contain type parameters
    Application(crate::types::TypeApplicationId),
    /// Union - may contain type parameters in members
    Union(Vec<TypeId>),
    /// Intersection - may contain type parameters in members
    Intersection(Vec<TypeId>),
    /// Callable - may have type parameters
    Callable(crate::types::CallableShapeId),
    /// Not a type parameter or type containing type parameters
    NotTypeParameter,
}

/// Classify a type for type parameter handling.
///
/// Returns detailed information about type parameter types.
pub fn classify_type_parameter(db: &dyn TypeDatabase, type_id: TypeId) -> TypeParameterKind {
    let Some(key) = db.lookup(type_id) else {
        return TypeParameterKind::NotTypeParameter;
    };

    match key {
        TypeData::TypeParameter(info) => TypeParameterKind::TypeParameter(info),
        TypeData::Infer(info) => TypeParameterKind::Infer(info),
        TypeData::Application(app_id) => TypeParameterKind::Application(app_id),
        TypeData::Union(list_id) => {
            let members = db.type_list(list_id);
            TypeParameterKind::Union(members.to_vec())
        }
        TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            TypeParameterKind::Intersection(members.to_vec())
        }
        TypeData::Callable(shape_id) => TypeParameterKind::Callable(shape_id),
        _ => TypeParameterKind::NotTypeParameter,
    }
}

/// Check if a type is directly a type parameter (`TypeParameter` or Infer).
pub fn is_direct_type_parameter(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        classify_type_parameter(db, type_id),
        TypeParameterKind::TypeParameter(_) | TypeParameterKind::Infer(_)
    )
}

/// Get the type parameter default if this is a type parameter.
pub fn get_type_param_default(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info.default,
        _ => None,
    }
}

/// Get the callable type parameter count.
pub fn get_callable_type_param_count(db: &dyn TypeDatabase, type_id: TypeId) -> usize {
    match db.lookup(type_id) {
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            shape
                .call_signatures
                .iter()
                .map(|sig| sig.type_params.len())
                .max()
                .unwrap_or(0)
        }
        _ => 0,
    }
}

// =============================================================================
// Promise Type Classification
// =============================================================================

/// Classification for promise-like types.
///
/// This enum provides a structured way to handle promise types without
/// directly matching on `TypeData` in the checker layer.
#[derive(Debug, Clone)]
pub enum PromiseTypeKind {
    /// Type application (like Promise<T>) - contains base and args
    Application {
        app_id: crate::types::TypeApplicationId,
        base: TypeId,
        args: Vec<TypeId>,
    },
    /// Lazy reference (`DefId`) - needs resolution to check if it's Promise
    Lazy(crate::def::DefId),
    /// Object type (might be Promise interface from lib)
    Object(crate::types::ObjectShapeId),
    /// Union type - check each member
    Union(Vec<TypeId>),
    /// Not a promise type
    NotPromise,
}

/// Classify a type for promise handling.
///
/// This function examines a type and returns information about how to handle it
/// when checking for promise-like types.
pub fn classify_promise_type(db: &dyn TypeDatabase, type_id: TypeId) -> PromiseTypeKind {
    let Some(key) = db.lookup(type_id) else {
        return PromiseTypeKind::NotPromise;
    };

    match key {
        TypeData::Application(app_id) => {
            let app = db.type_application(app_id);
            PromiseTypeKind::Application {
                app_id,
                base: app.base,
                args: app.args.clone(),
            }
        }
        TypeData::Lazy(def_id) => PromiseTypeKind::Lazy(def_id),
        TypeData::Object(shape_id) => PromiseTypeKind::Object(shape_id),
        TypeData::Union(list_id) => {
            let members = db.type_list(list_id);
            PromiseTypeKind::Union(members.to_vec())
        }
        _ => PromiseTypeKind::NotPromise,
    }
}

// =============================================================================
// New Expression Type Classification
// =============================================================================

/// Classification for types in `new` expressions.
#[derive(Debug, Clone)]
pub enum NewExpressionTypeKind {
    /// Callable type - check for construct signatures
    Callable(crate::types::CallableShapeId),
    /// Function type - always constructable
    Function(crate::types::FunctionShapeId),
    /// `TypeQuery` (typeof X) - needs symbol resolution
    TypeQuery(crate::types::SymbolRef),
    /// Intersection type - check all members for construct signatures
    Intersection(Vec<TypeId>),
    /// Union type - all members must be constructable
    Union(Vec<TypeId>),
    /// Type parameter with constraint
    TypeParameter { constraint: Option<TypeId> },
    /// Not constructable
    NotConstructable,
}

/// Classify a type for new expression handling.
pub fn classify_for_new_expression(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> NewExpressionTypeKind {
    let Some(key) = db.lookup(type_id) else {
        return NewExpressionTypeKind::NotConstructable;
    };

    match key {
        TypeData::Callable(shape_id) => NewExpressionTypeKind::Callable(shape_id),
        TypeData::Function(shape_id) => NewExpressionTypeKind::Function(shape_id),
        TypeData::TypeQuery(sym_ref) => NewExpressionTypeKind::TypeQuery(sym_ref),
        TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            NewExpressionTypeKind::Intersection(members.to_vec())
        }
        TypeData::Union(list_id) => {
            let members = db.type_list(list_id);
            NewExpressionTypeKind::Union(members.to_vec())
        }
        TypeData::TypeParameter(info) | TypeData::Infer(info) => {
            NewExpressionTypeKind::TypeParameter {
                constraint: info.constraint,
            }
        }
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            // Objects might contain callable properties that represent construct signatures
            // Check if the object has a "new" property or if any property is callable with construct signatures
            let shape = db.object_shape(shape_id);
            for prop in &shape.properties {
                // Check if this property is a callable type with construct signatures
                if let Some(TypeData::Callable(callable_shape_id)) = db.lookup(prop.type_id) {
                    let callable_shape = db.callable_shape(callable_shape_id);
                    if !callable_shape.construct_signatures.is_empty() {
                        // Found a callable property with construct signatures
                        return NewExpressionTypeKind::Callable(callable_shape_id);
                    }
                }
            }
            NewExpressionTypeKind::NotConstructable
        }
        _ => NewExpressionTypeKind::NotConstructable,
    }
}

// =============================================================================
// Abstract Class Type Classification
// =============================================================================

/// Classification for checking if a type contains abstract classes.
#[derive(Debug, Clone)]
pub enum AbstractClassCheckKind {
    /// `TypeQuery` - check if symbol is abstract
    TypeQuery(crate::types::SymbolRef),
    /// Union - check if any member is abstract
    Union(Vec<TypeId>),
    /// Intersection - check if any member is abstract
    Intersection(Vec<TypeId>),
    /// Other type - not an abstract class
    NotAbstract,
}

/// Classify a type for abstract class checking.
pub fn classify_for_abstract_check(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> AbstractClassCheckKind {
    let Some(key) = db.lookup(type_id) else {
        return AbstractClassCheckKind::NotAbstract;
    };

    match key {
        TypeData::TypeQuery(sym_ref) => AbstractClassCheckKind::TypeQuery(sym_ref),
        TypeData::Union(list_id) => {
            let members = db.type_list(list_id);
            AbstractClassCheckKind::Union(members.to_vec())
        }
        TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            AbstractClassCheckKind::Intersection(members.to_vec())
        }
        _ => AbstractClassCheckKind::NotAbstract,
    }
}

// =============================================================================
// Construct Signature Return Type Classification
// =============================================================================

/// Classification for extracting construct signature return types.
#[derive(Debug, Clone)]
pub enum ConstructSignatureKind {
    /// Callable type with potential construct signatures
    Callable(crate::types::CallableShapeId),
    /// Lazy reference (`DefId`) - resolve and check
    Lazy(crate::def::DefId),
    /// Symbol reference - may be a class (deprecated)
    #[deprecated(note = "Lazy types don't use SymbolRef")]
    Ref(crate::types::SymbolRef),
    /// `TypeQuery` (typeof X) - check if class
    TypeQuery(crate::types::SymbolRef),
    /// Application type - needs evaluation
    Application(crate::types::TypeApplicationId),
    /// Union - all members must have construct signatures
    Union(Vec<TypeId>),
    /// Intersection - any member with construct signature is sufficient
    Intersection(Vec<TypeId>),
    /// Type parameter with constraint
    TypeParameter { constraint: Option<TypeId> },
    /// Function type - check `is_constructor` flag
    Function(crate::types::FunctionShapeId),
    /// No construct signatures available
    NoConstruct,
}

/// Classify a type for construct signature extraction.
pub fn classify_for_construct_signature(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ConstructSignatureKind {
    let Some(key) = db.lookup(type_id) else {
        return ConstructSignatureKind::NoConstruct;
    };

    match key {
        TypeData::Callable(shape_id) => ConstructSignatureKind::Callable(shape_id),
        TypeData::Lazy(def_id) => ConstructSignatureKind::Lazy(def_id),
        TypeData::TypeQuery(sym_ref) => ConstructSignatureKind::TypeQuery(sym_ref),
        TypeData::Application(app_id) => ConstructSignatureKind::Application(app_id),
        TypeData::Union(list_id) => {
            let members = db.type_list(list_id);
            ConstructSignatureKind::Union(members.to_vec())
        }
        TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            ConstructSignatureKind::Intersection(members.to_vec())
        }
        TypeData::TypeParameter(info) | TypeData::Infer(info) => {
            ConstructSignatureKind::TypeParameter {
                constraint: info.constraint,
            }
        }
        TypeData::Function(shape_id) => ConstructSignatureKind::Function(shape_id),
        _ => ConstructSignatureKind::NoConstruct,
    }
}

// =============================================================================
// KeyOf Type Classification
// =============================================================================

/// Classification for computing keyof types.
#[derive(Debug, Clone)]
pub enum KeyOfTypeKind {
    /// Object type with properties
    Object(crate::types::ObjectShapeId),
    /// No keys available
    NoKeys,
}

/// Classify a type for keyof computation.
pub fn classify_for_keyof(db: &dyn TypeDatabase, type_id: TypeId) -> KeyOfTypeKind {
    let Some(key) = db.lookup(type_id) else {
        return KeyOfTypeKind::NoKeys;
    };

    match key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            KeyOfTypeKind::Object(shape_id)
        }
        _ => KeyOfTypeKind::NoKeys,
    }
}

// =============================================================================
// String Literal Key Extraction
// =============================================================================

/// Classification for extracting string literal keys.
#[derive(Debug, Clone)]
pub enum StringLiteralKeyKind {
    /// Single string literal
    SingleString(tsz_common::interner::Atom),
    /// Union of types - check each member
    Union(Vec<TypeId>),
    /// Not a string literal
    NotStringLiteral,
}

/// Classify a type for string literal key extraction.
pub fn classify_for_string_literal_keys(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> StringLiteralKeyKind {
    let Some(key) = db.lookup(type_id) else {
        return StringLiteralKeyKind::NotStringLiteral;
    };

    match key {
        TypeData::Literal(crate::types::LiteralValue::String(name)) => {
            StringLiteralKeyKind::SingleString(name)
        }
        TypeData::Union(list_id) => {
            let members = db.type_list(list_id);
            StringLiteralKeyKind::Union(members.to_vec())
        }
        _ => StringLiteralKeyKind::NotStringLiteral,
    }
}

/// Extract string literal from a Literal type.
/// Returns None if not a string literal.
pub fn get_string_literal_value(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_common::interner::Atom> {
    match db.lookup(type_id) {
        Some(TypeData::Literal(crate::types::LiteralValue::String(name))) => Some(name),
        _ => None,
    }
}

// =============================================================================
// Class Declaration from Type
// =============================================================================

/// Classification for extracting class declarations from types.
#[derive(Debug, Clone)]
pub enum ClassDeclTypeKind {
    /// Object type with properties (may have brand)
    Object(crate::types::ObjectShapeId),
    /// Union/Intersection - check all members
    Members(Vec<TypeId>),
    /// Not an object type
    NotObject,
}

/// Classify a type for class declaration extraction.
pub fn classify_for_class_decl(db: &dyn TypeDatabase, type_id: TypeId) -> ClassDeclTypeKind {
    let Some(key) = db.lookup(type_id) else {
        return ClassDeclTypeKind::NotObject;
    };

    match key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            ClassDeclTypeKind::Object(shape_id)
        }
        TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            ClassDeclTypeKind::Members(members.to_vec())
        }
        _ => ClassDeclTypeKind::NotObject,
    }
}

// =============================================================================
// Call Expression Overload Classification
// =============================================================================

/// Classification for extracting call signatures from a type.
#[derive(Debug, Clone)]
pub enum CallSignaturesKind {
    /// Callable type with signatures
    Callable(crate::types::CallableShapeId),
    /// Multiple call signatures (e.g., from union of callables)
    MultipleSignatures(Vec<crate::CallSignature>),
    /// Other type - no call signatures
    NoSignatures,
}

/// Classify a type for call signature extraction.
pub fn classify_for_call_signatures(db: &dyn TypeDatabase, type_id: TypeId) -> CallSignaturesKind {
    let Some(key) = db.lookup(type_id) else {
        return CallSignaturesKind::NoSignatures;
    };

    match key {
        TypeData::Callable(shape_id) => CallSignaturesKind::Callable(shape_id),
        TypeData::Union(list_id) => {
            // For unions, collect call signatures from all callable members
            let members = db.type_list(list_id);
            let mut call_signatures = Vec::new();

            for &member in members.iter() {
                match db.lookup(member) {
                    Some(TypeData::Callable(shape_id)) => {
                        let shape = db.callable_shape(shape_id);
                        // Extend with call signatures from this member
                        call_signatures.extend(shape.call_signatures.iter().cloned());
                    }
                    _ => continue,
                }
            }

            if call_signatures.is_empty() {
                CallSignaturesKind::NoSignatures
            } else {
                CallSignaturesKind::MultipleSignatures(call_signatures)
            }
        }
        _ => CallSignaturesKind::NoSignatures,
    }
}

// =============================================================================
// Generic Application Type Extraction
// =============================================================================

/// Get the base and args from an Application type.
/// Returns None if not an Application.
pub fn get_application_info(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, Vec<TypeId>)> {
    match db.lookup(type_id) {
        Some(TypeData::Application(app_id)) => {
            let app = db.type_application(app_id);
            Some((app.base, app.args.clone()))
        }
        _ => None,
    }
}

// =============================================================================
// Type Parameter Content Classification
// =============================================================================

/// Classification for types when checking for type parameters.
#[derive(Debug, Clone)]
pub enum TypeParameterContentKind {
    /// Is a type parameter or infer type
    IsTypeParameter,
    /// Array - check element type
    Array(TypeId),
    /// Tuple - check element types
    Tuple(crate::types::TupleListId),
    /// Union - check all members
    Union(Vec<TypeId>),
    /// Intersection - check all members
    Intersection(Vec<TypeId>),
    /// Application - check base and args
    Application { base: TypeId, args: Vec<TypeId> },
    /// Not a type parameter and no nested types to check
    NotTypeParameter,
}

/// Classify a type for type parameter checking.
pub fn classify_for_type_parameter_content(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeParameterContentKind {
    let Some(key) = db.lookup(type_id) else {
        return TypeParameterContentKind::NotTypeParameter;
    };

    match key {
        TypeData::TypeParameter(_) | TypeData::Infer(_) => {
            TypeParameterContentKind::IsTypeParameter
        }
        TypeData::Array(elem) => TypeParameterContentKind::Array(elem),
        TypeData::Tuple(list_id) => TypeParameterContentKind::Tuple(list_id),
        TypeData::Union(list_id) => {
            let members = db.type_list(list_id);
            TypeParameterContentKind::Union(members.to_vec())
        }
        TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            TypeParameterContentKind::Intersection(members.to_vec())
        }
        TypeData::Application(app_id) => {
            let app = db.type_application(app_id);
            TypeParameterContentKind::Application {
                base: app.base,
                args: app.args.clone(),
            }
        }
        _ => TypeParameterContentKind::NotTypeParameter,
    }
}

// =============================================================================
// Type Depth Classification
// =============================================================================

/// Classification for computing type depth.
#[derive(Debug, Clone)]
pub enum TypeDepthKind {
    /// Array - depth = 1 + element depth
    Array(TypeId),
    /// Tuple - depth = 1 + max element depth
    Tuple(crate::types::TupleListId),
    /// Union or Intersection - depth = 1 + max member depth
    Members(Vec<TypeId>),
    /// Application - depth = 1 + max(base depth, arg depths)
    Application { base: TypeId, args: Vec<TypeId> },
    /// Terminal type - depth = 1
    Terminal,
}

/// Classify a type for depth computation.
pub fn classify_for_type_depth(db: &dyn TypeDatabase, type_id: TypeId) -> TypeDepthKind {
    let Some(key) = db.lookup(type_id) else {
        return TypeDepthKind::Terminal;
    };

    match key {
        TypeData::Array(elem) => TypeDepthKind::Array(elem),
        TypeData::Tuple(list_id) => TypeDepthKind::Tuple(list_id),
        TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            TypeDepthKind::Members(members.to_vec())
        }
        TypeData::Application(app_id) => {
            let app = db.type_application(app_id);
            TypeDepthKind::Application {
                base: app.base,
                args: app.args.clone(),
            }
        }
        _ => TypeDepthKind::Terminal,
    }
}

// =============================================================================
// Object Spread Property Classification
// =============================================================================

/// Classification for collecting properties from spread expressions.
#[derive(Debug, Clone)]
pub enum SpreadPropertyKind {
    /// Object type with properties
    Object(crate::types::ObjectShapeId),
    /// Callable type with properties
    Callable(crate::types::CallableShapeId),
    /// Intersection - collect from all members
    Intersection(Vec<TypeId>),
    /// No properties to spread
    NoProperties,
}

/// Classify a type for spread property collection.
pub fn classify_for_spread_properties(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> SpreadPropertyKind {
    let Some(key) = db.lookup(type_id) else {
        return SpreadPropertyKind::NoProperties;
    };

    match key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            SpreadPropertyKind::Object(shape_id)
        }
        TypeData::Callable(shape_id) => SpreadPropertyKind::Callable(shape_id),
        TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            SpreadPropertyKind::Intersection(members.to_vec())
        }
        _ => SpreadPropertyKind::NoProperties,
    }
}

// =============================================================================
// Ref Type Resolution
// =============================================================================

/// Classification for Lazy type resolution.
#[derive(Debug, Clone)]
pub enum LazyTypeKind {
    /// `DefId` - resolve to actual type
    Lazy(crate::def::DefId),
    /// Not a Lazy type
    NotLazy,
    /// Deprecated: `SymbolRef` - use Lazy instead
    #[deprecated(note = "Use Lazy instead")]
    Ref(crate::def::DefId),
    /// Deprecated: `NotRef` - use `NotLazy` instead
    #[deprecated(note = "Use NotLazy instead")]
    NotRef,
}

/// Classify a type for Lazy resolution.
pub fn classify_for_lazy_resolution(db: &dyn TypeDatabase, type_id: TypeId) -> LazyTypeKind {
    let Some(key) = db.lookup(type_id) else {
        return LazyTypeKind::NotLazy;
    };

    match key {
        TypeData::Lazy(def_id) => LazyTypeKind::Lazy(def_id),
        _ => LazyTypeKind::NotLazy,
    }
}

/// Compatibility alias for `RefTypeKind`.
#[deprecated(note = "Use LazyTypeKind instead")]
pub type RefTypeKind = LazyTypeKind;

/// Compatibility alias for `classify_for_lazy_resolution`.
#[deprecated(note = "Use classify_for_lazy_resolution instead")]
pub fn classify_for_ref_resolution(db: &dyn TypeDatabase, type_id: TypeId) -> LazyTypeKind {
    classify_for_lazy_resolution(db, type_id)
}

// =============================================================================
// Constructor Check Classification (for is_constructor_type)
// =============================================================================

/// Classification for checking if a type is a constructor type.
#[derive(Debug, Clone)]
pub enum ConstructorCheckKind {
    /// Type parameter with optional constraint - recurse into constraint
    TypeParameter { constraint: Option<TypeId> },
    /// Intersection type - check if any member is a constructor
    Intersection(Vec<TypeId>),
    /// Union type - check if all members are constructors
    Union(Vec<TypeId>),
    /// Application type - extract base and check
    Application { base: TypeId },
    /// Lazy reference (`DefId`) - resolve to check if it's a class/interface
    Lazy(crate::def::DefId),
    /// `TypeQuery` (typeof) - check referenced symbol
    TypeQuery(crate::types::SymbolRef),
    /// Not a constructor type or needs special handling
    Other,
}

/// Classify a type for constructor type checking.
pub fn classify_for_constructor_check(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ConstructorCheckKind {
    let Some(key) = db.lookup(type_id) else {
        return ConstructorCheckKind::Other;
    };

    match key {
        TypeData::TypeParameter(info) | TypeData::Infer(info) => {
            ConstructorCheckKind::TypeParameter {
                constraint: info.constraint,
            }
        }
        TypeData::Intersection(members_id) => {
            let members = db.type_list(members_id);
            ConstructorCheckKind::Intersection(members.to_vec())
        }
        TypeData::Union(members_id) => {
            let members = db.type_list(members_id);
            ConstructorCheckKind::Union(members.to_vec())
        }
        TypeData::Application(app_id) => {
            let app = db.type_application(app_id);
            ConstructorCheckKind::Application { base: app.base }
        }
        TypeData::Lazy(def_id) => ConstructorCheckKind::Lazy(def_id),
        TypeData::TypeQuery(sym_ref) => ConstructorCheckKind::TypeQuery(sym_ref),
        _ => ConstructorCheckKind::Other,
    }
}

/// Check if a type is narrowable (union or type parameter).
pub fn is_narrowable_type_key(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // CRITICAL FIX: Lazy types are also narrowable!
    // Lazy types are DefId references that need to be resolved, but they represent
    // types that can be narrowed (unions, type parameters, etc.)
    // Without this, discriminant narrowing fails for types stored as lazy references
    matches!(
        db.lookup(type_id),
        Some(
            TypeData::Union(_)
                | TypeData::TypeParameter(_)
                | TypeData::Infer(_)
                | TypeData::Lazy(_)
        )
    )
}

// =============================================================================
// Private Brand Classification (for get_private_brand)
// =============================================================================

/// Classification for types when extracting private brands.
#[derive(Debug, Clone)]
pub enum PrivateBrandKind {
    /// Object type with `shape_id` - check properties for brand
    Object(crate::types::ObjectShapeId),
    /// Callable type with `shape_id` - check properties for brand
    Callable(crate::types::CallableShapeId),
    /// No private brand possible
    None,
}

/// Classify a type for private brand extraction.
pub fn classify_for_private_brand(db: &dyn TypeDatabase, type_id: TypeId) -> PrivateBrandKind {
    match db.lookup(type_id) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            PrivateBrandKind::Object(shape_id)
        }
        Some(TypeData::Callable(shape_id)) => PrivateBrandKind::Callable(shape_id),
        _ => PrivateBrandKind::None,
    }
}

/// Get the widened type for a literal type.
pub fn get_widened_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeData::Literal(crate::LiteralValue::String(_))) => Some(TypeId::STRING),
        Some(TypeData::Literal(crate::LiteralValue::Number(_))) => Some(TypeId::NUMBER),
        Some(TypeData::Literal(crate::LiteralValue::BigInt(_))) => Some(TypeId::BIGINT),
        Some(TypeData::Literal(crate::LiteralValue::Boolean(_))) => Some(TypeId::BOOLEAN),
        _ => None,
    }
}

/// Get tuple elements list ID if the type is a tuple.
pub fn get_tuple_list_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::types::TupleListId> {
    match db.lookup(type_id) {
        Some(TypeData::Tuple(list_id)) => Some(list_id),
        _ => None,
    }
}

/// Get the base type of an application type.
pub fn get_application_base(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeData::Application(app_id)) => Some(db.type_application(app_id).base),
        _ => None,
    }
}

// =============================================================================
// Literal Key Classification (for get_literal_key_union_from_type)
// =============================================================================

/// Classification for literal key extraction from types.
#[derive(Debug, Clone)]
pub enum LiteralKeyKind {
    StringLiteral(tsz_common::interner::Atom),
    NumberLiteral(f64),
    Union(Vec<TypeId>),
    Other,
}

/// Classify a type for literal key extraction.
pub fn classify_literal_key(db: &dyn TypeDatabase, type_id: TypeId) -> LiteralKeyKind {
    match db.lookup(type_id) {
        Some(TypeData::Literal(crate::LiteralValue::String(atom))) => {
            LiteralKeyKind::StringLiteral(atom)
        }
        Some(TypeData::Literal(crate::LiteralValue::Number(num))) => {
            LiteralKeyKind::NumberLiteral(num.0)
        }
        Some(TypeData::Union(members_id)) => {
            LiteralKeyKind::Union(db.type_list(members_id).to_vec())
        }
        _ => LiteralKeyKind::Other,
    }
}

/// Get literal value from a type if it's a literal.
pub fn get_literal_value(db: &dyn TypeDatabase, type_id: TypeId) -> Option<crate::LiteralValue> {
    match db.lookup(type_id) {
        Some(TypeData::Literal(value)) => Some(value),
        _ => None,
    }
}

/// Widen a literal type to its corresponding primitive type.
///
/// - `1` -> `number`
/// - `"hello"` -> `string`
/// - `true` -> `boolean`
/// - `1n` -> `bigint`
///
/// Non-literal types are returned unchanged.
pub fn widen_literal_to_primitive(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    match db.lookup(type_id) {
        Some(TypeData::Literal(ref lit)) => match lit {
            crate::LiteralValue::String(_) => TypeId::STRING,
            crate::LiteralValue::Number(_) => TypeId::NUMBER,
            crate::LiteralValue::Boolean(_) => TypeId::BOOLEAN,
            crate::LiteralValue::BigInt(_) => TypeId::BIGINT,
        },
        _ => type_id,
    }
}

/// Get the type of a named property from an object type.
///
/// Returns `None` if the type is not an object or the property doesn't exist.
pub fn get_object_property_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    property_name: &str,
) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            for prop in &shape.properties {
                let prop_name = db.resolve_atom_ref(prop.name);
                if prop_name.as_ref() == property_name {
                    return Some(prop.type_id);
                }
            }
            None
        }
        _ => None,
    }
}

/// Check if a type is a Function (not Callable) and get its return type.
///
/// Used for checking if a function's return type is promise-like in async iterator contexts.
pub fn get_function_return_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeData::Function(shape_id)) => {
            let shape = db.function_shape(shape_id);
            Some(shape.return_type)
        }
        _ => None,
    }
}

/// Check if a type is specifically an object type with index signatures.
///
/// Returns true only for `TypeData::ObjectWithIndex`, not for `TypeData::Object`.
pub fn is_object_with_index_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeData::ObjectWithIndex(_)))
}

// =============================================================================
// Array-Like Type Classification (for is_array_like_type)
// =============================================================================

/// Classification for array-like types.
#[derive(Debug, Clone)]
pub enum ArrayLikeKind {
    Array(TypeId),
    Tuple,
    Readonly(TypeId),
    Union(Vec<TypeId>),
    Intersection(Vec<TypeId>),
    Other,
}

/// Classify a type for array-like checking.
pub fn classify_array_like(db: &dyn TypeDatabase, type_id: TypeId) -> ArrayLikeKind {
    match db.lookup(type_id) {
        Some(TypeData::Array(elem)) => ArrayLikeKind::Array(elem),
        Some(TypeData::Tuple(_)) => ArrayLikeKind::Tuple,
        Some(TypeData::ReadonlyType(inner)) => ArrayLikeKind::Readonly(inner),
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
            info.constraint.map_or(ArrayLikeKind::Other, |constraint| {
                classify_array_like(db, constraint)
            })
        }
        Some(TypeData::Union(members_id)) => {
            ArrayLikeKind::Union(db.type_list(members_id).to_vec())
        }
        Some(TypeData::Intersection(members_id)) => {
            ArrayLikeKind::Intersection(db.type_list(members_id).to_vec())
        }
        _ => ArrayLikeKind::Other,
    }
}

// =============================================================================
// Index Key Classification (for get_index_key_kind)
// =============================================================================

/// Classification for index key types.
#[derive(Debug, Clone)]
pub enum IndexKeyKind {
    String,
    Number,
    StringLiteral,
    NumberLiteral,
    Union(Vec<TypeId>),
    Other,
}

/// Classify a type for index key checking.
pub fn classify_index_key(db: &dyn TypeDatabase, type_id: TypeId) -> IndexKeyKind {
    match db.lookup(type_id) {
        Some(TypeData::Intrinsic(crate::IntrinsicKind::String)) => IndexKeyKind::String,
        Some(TypeData::Intrinsic(crate::IntrinsicKind::Number)) => IndexKeyKind::Number,
        Some(TypeData::Literal(crate::LiteralValue::String(_))) => IndexKeyKind::StringLiteral,
        Some(TypeData::Literal(crate::LiteralValue::Number(_))) => IndexKeyKind::NumberLiteral,
        Some(TypeData::Union(members_id)) => IndexKeyKind::Union(db.type_list(members_id).to_vec()),
        _ => IndexKeyKind::Other,
    }
}

// =============================================================================
// Element Indexable Classification (for is_element_indexable_key)
// =============================================================================

/// Classification for element indexable types.
#[derive(Debug, Clone)]
pub enum ElementIndexableKind {
    Array,
    Tuple,
    ObjectWithIndex { has_string: bool, has_number: bool },
    Union(Vec<TypeId>),
    Intersection(Vec<TypeId>),
    StringLike,
    Other,
}

/// Classify a type for element indexing capability.
pub fn classify_element_indexable(db: &dyn TypeDatabase, type_id: TypeId) -> ElementIndexableKind {
    match db.lookup(type_id) {
        Some(TypeData::Array(_)) => ElementIndexableKind::Array,
        Some(TypeData::Tuple(_)) => ElementIndexableKind::Tuple,
        Some(TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            ElementIndexableKind::ObjectWithIndex {
                has_string: shape.string_index.is_some(),
                has_number: shape.number_index.is_some(),
            }
        }
        Some(TypeData::Union(members_id)) => {
            ElementIndexableKind::Union(db.type_list(members_id).to_vec())
        }
        Some(TypeData::Intersection(members_id)) => {
            ElementIndexableKind::Intersection(db.type_list(members_id).to_vec())
        }
        Some(TypeData::Literal(crate::LiteralValue::String(_)))
        | Some(TypeData::Intrinsic(crate::IntrinsicKind::String)) => {
            ElementIndexableKind::StringLike
        }
        // Enums support reverse mapping: E[value] returns the name, E["name"] returns the value.
        // Treat them as having both string and number index signatures.
        Some(TypeData::Enum(_, _)) => ElementIndexableKind::ObjectWithIndex {
            has_string: true,
            has_number: true,
        },
        _ => ElementIndexableKind::Other,
    }
}

// =============================================================================
// Type Query Classification (for resolve_type_query_type)
// =============================================================================

/// Classification for type query resolution.
#[derive(Debug, Clone)]
pub enum TypeQueryKind {
    TypeQuery(crate::types::SymbolRef),
    ApplicationWithTypeQuery {
        base_sym_ref: crate::types::SymbolRef,
        args: Vec<TypeId>,
    },
    Application {
        app_id: crate::types::TypeApplicationId,
    },
    Other,
}

/// Classify a type for type query resolution.
pub fn classify_type_query(db: &dyn TypeDatabase, type_id: TypeId) -> TypeQueryKind {
    match db.lookup(type_id) {
        Some(TypeData::TypeQuery(sym_ref)) => TypeQueryKind::TypeQuery(sym_ref),
        Some(TypeData::Application(app_id)) => {
            let app = db.type_application(app_id);
            match db.lookup(app.base) {
                Some(TypeData::TypeQuery(base_sym_ref)) => {
                    TypeQueryKind::ApplicationWithTypeQuery {
                        base_sym_ref,
                        args: app.args.clone(),
                    }
                }
                _ => TypeQueryKind::Application { app_id },
            }
        }
        _ => TypeQueryKind::Other,
    }
}

// =============================================================================
// Symbol Reference Classification (for enum_symbol_from_value_type)
// =============================================================================

/// Classification for symbol reference types.
#[derive(Debug, Clone)]
pub enum SymbolRefKind {
    /// Lazy reference (`DefId`)
    Lazy(crate::def::DefId),
    #[deprecated(note = "Lazy types don't use SymbolRef")]
    Ref(crate::types::SymbolRef),
    TypeQuery(crate::types::SymbolRef),
    Other,
}

/// Classify a type as a symbol reference.
pub fn classify_symbol_ref(db: &dyn TypeDatabase, type_id: TypeId) -> SymbolRefKind {
    match db.lookup(type_id) {
        Some(TypeData::Lazy(def_id)) => SymbolRefKind::Lazy(def_id),
        Some(TypeData::TypeQuery(sym_ref)) => SymbolRefKind::TypeQuery(sym_ref),
        _ => SymbolRefKind::Other,
    }
}

// =============================================================================
// Type Contains Classification (for type_contains_any_inner)
// =============================================================================

/// Classification for recursive type traversal.
#[derive(Debug, Clone)]
pub enum TypeContainsKind {
    Array(TypeId),
    Tuple(crate::types::TupleListId),
    Members(Vec<TypeId>),
    Object(crate::types::ObjectShapeId),
    Function(crate::types::FunctionShapeId),
    Callable(crate::types::CallableShapeId),
    Application(crate::types::TypeApplicationId),
    Conditional(crate::types::ConditionalTypeId),
    Mapped(crate::types::MappedTypeId),
    IndexAccess {
        base: TypeId,
        index: TypeId,
    },
    TemplateLiteral(crate::types::TemplateLiteralId),
    Inner(TypeId),
    TypeParam {
        constraint: Option<TypeId>,
        default: Option<TypeId>,
    },
    Terminal,
}

/// Classify a type for recursive traversal.
pub fn classify_for_contains_traversal(db: &dyn TypeDatabase, type_id: TypeId) -> TypeContainsKind {
    match db.lookup(type_id) {
        Some(TypeData::Array(elem)) => TypeContainsKind::Array(elem),
        Some(TypeData::Tuple(list_id)) => TypeContainsKind::Tuple(list_id),
        Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => {
            TypeContainsKind::Members(db.type_list(list_id).to_vec())
        }
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            TypeContainsKind::Object(shape_id)
        }
        Some(TypeData::Function(shape_id)) => TypeContainsKind::Function(shape_id),
        Some(TypeData::Callable(shape_id)) => TypeContainsKind::Callable(shape_id),
        Some(TypeData::Application(app_id)) => TypeContainsKind::Application(app_id),
        Some(TypeData::Conditional(cond_id)) => TypeContainsKind::Conditional(cond_id),
        Some(TypeData::Mapped(mapped_id)) => TypeContainsKind::Mapped(mapped_id),
        Some(TypeData::IndexAccess(base, index)) => TypeContainsKind::IndexAccess { base, index },
        Some(TypeData::TemplateLiteral(template_id)) => {
            TypeContainsKind::TemplateLiteral(template_id)
        }
        Some(TypeData::KeyOf(inner) | TypeData::ReadonlyType(inner)) => {
            TypeContainsKind::Inner(inner)
        }
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
            TypeContainsKind::TypeParam {
                constraint: info.constraint,
                default: info.default,
            }
        }
        _ => TypeContainsKind::Terminal,
    }
}

// =============================================================================
// Namespace Member Classification (for resolve_namespace_value_member)
// =============================================================================

/// Classification for namespace member resolution.
#[derive(Debug, Clone)]
pub enum NamespaceMemberKind {
    Lazy(DefId),
    ModuleNamespace(crate::types::SymbolRef),
    Callable(crate::types::CallableShapeId),
    // TSZ-4: Added Enum variant to handle enum member property access (E.A)
    Enum(DefId),
    Other,
}

/// Classify a type for namespace member resolution.
pub fn classify_namespace_member(db: &dyn TypeDatabase, type_id: TypeId) -> NamespaceMemberKind {
    match db.lookup(type_id) {
        Some(TypeData::Callable(shape_id)) => NamespaceMemberKind::Callable(shape_id),
        Some(TypeData::Lazy(def_id)) => NamespaceMemberKind::Lazy(def_id),
        Some(TypeData::ModuleNamespace(sym_ref)) => NamespaceMemberKind::ModuleNamespace(sym_ref),
        // TSZ-4: Handle TypeData::Enum for enum member property access (E.A)
        Some(TypeData::Enum(def_id, _)) => NamespaceMemberKind::Enum(def_id),
        _ => NamespaceMemberKind::Other,
    }
}

/// Unwrap readonly type wrapper if present.
pub fn unwrap_readonly_for_lookup(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    match db.lookup(type_id) {
        Some(TypeData::ReadonlyType(inner)) => inner,
        _ => type_id,
    }
}

// =============================================================================
// Literal Type Creation Helpers
// =============================================================================

/// Create a string literal type from a string value.
///
/// This abstracts away the `TypeData` construction from the checker layer.
pub fn create_string_literal_type(db: &dyn TypeDatabase, value: &str) -> TypeId {
    let atom = db.intern_string(value);
    db.literal_string_atom(atom)
}

/// Create a number literal type from a numeric value.
///
/// This abstracts away the `TypeData` construction from the checker layer.
pub fn create_number_literal_type(db: &dyn TypeDatabase, value: f64) -> TypeId {
    db.literal_number(value)
}

/// Create a boolean literal type.
///
/// This abstracts away the `TypeData` construction from the checker layer.
pub fn create_boolean_literal_type(db: &dyn TypeDatabase, value: bool) -> TypeId {
    db.literal_boolean(value)
}

// =============================================================================
// Instance Type from Constructor Classification
// =============================================================================

/// Classification for extracting instance types from constructor types.
#[derive(Debug, Clone)]
pub enum InstanceTypeKind {
    /// Callable type - extract from `construct_signatures` return types
    Callable(crate::types::CallableShapeId),
    /// Function type - check `is_constructor` flag
    Function(crate::types::FunctionShapeId),
    /// Intersection type - recursively extract instance types from members
    Intersection(Vec<TypeId>),
    /// Union type - recursively extract instance types from members
    Union(Vec<TypeId>),
    /// `ReadonlyType` - unwrap and recurse
    Readonly(TypeId),
    /// Type parameter with constraint - follow constraint
    TypeParameter { constraint: Option<TypeId> },
    /// Symbol reference (Ref or `TypeQuery`) - needs resolution to class instance type
    SymbolRef(crate::types::SymbolRef),
    /// Complex types (Conditional, Mapped, `IndexAccess`, `KeyOf`) - need evaluation
    NeedsEvaluation,
    /// Not a constructor type
    NotConstructor,
}

/// Classify a type for instance type extraction.
pub fn classify_for_instance_type(db: &dyn TypeDatabase, type_id: TypeId) -> InstanceTypeKind {
    let Some(key) = db.lookup(type_id) else {
        return InstanceTypeKind::NotConstructor;
    };

    match key {
        TypeData::Callable(shape_id) => InstanceTypeKind::Callable(shape_id),
        TypeData::Function(shape_id) => InstanceTypeKind::Function(shape_id),
        TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            InstanceTypeKind::Intersection(members.to_vec())
        }
        TypeData::Union(list_id) => {
            let members = db.type_list(list_id);
            InstanceTypeKind::Union(members.to_vec())
        }
        TypeData::ReadonlyType(inner) => InstanceTypeKind::Readonly(inner),
        TypeData::TypeParameter(info) | TypeData::Infer(info) => InstanceTypeKind::TypeParameter {
            constraint: info.constraint,
        },
        // TypeQuery (typeof expressions) needs resolution to instance type
        TypeData::TypeQuery(sym_ref) => InstanceTypeKind::SymbolRef(sym_ref),
        TypeData::Conditional(_)
        | TypeData::Mapped(_)
        | TypeData::IndexAccess(_, _)
        | TypeData::KeyOf(_)
        | TypeData::Application(_) => InstanceTypeKind::NeedsEvaluation,
        _ => InstanceTypeKind::NotConstructor,
    }
}

// =============================================================================
// Constructor Return Merge Classification
// =============================================================================

/// Classification for merging base instance into constructor return.
#[derive(Debug, Clone)]
pub enum ConstructorReturnMergeKind {
    /// Callable type - update `construct_signatures`
    Callable(crate::types::CallableShapeId),
    /// Function type - check `is_constructor` flag
    Function(crate::types::FunctionShapeId),
    /// Intersection type - update all members
    Intersection(Vec<TypeId>),
    /// Not mergeable
    Other,
}

/// Classify a type for constructor return merging.
pub fn classify_for_constructor_return_merge(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ConstructorReturnMergeKind {
    let Some(key) = db.lookup(type_id) else {
        return ConstructorReturnMergeKind::Other;
    };

    match key {
        TypeData::Callable(shape_id) => ConstructorReturnMergeKind::Callable(shape_id),
        TypeData::Function(shape_id) => ConstructorReturnMergeKind::Function(shape_id),
        TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            ConstructorReturnMergeKind::Intersection(members.to_vec())
        }
        _ => ConstructorReturnMergeKind::Other,
    }
}

// =============================================================================
// Abstract Constructor Type Classification
// =============================================================================

/// Classification for checking if a type is an abstract constructor type.
#[derive(Debug, Clone)]
pub enum AbstractConstructorKind {
    /// `TypeQuery` (typeof `AbstractClass`) - check if symbol is abstract
    TypeQuery(crate::types::SymbolRef),
    /// Ref - resolve and check (deprecated)
    #[deprecated(note = "Lazy types don't use SymbolRef")]
    Ref(crate::types::SymbolRef),
    /// Callable - check if marked as abstract
    Callable(crate::types::CallableShapeId),
    /// Application - check base type
    Application(crate::types::TypeApplicationId),
    /// Not an abstract constructor type
    NotAbstract,
}

/// Classify a type for abstract constructor checking.
pub fn classify_for_abstract_constructor(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> AbstractConstructorKind {
    let Some(key) = db.lookup(type_id) else {
        return AbstractConstructorKind::NotAbstract;
    };

    match key {
        TypeData::TypeQuery(sym_ref) => AbstractConstructorKind::TypeQuery(sym_ref),
        TypeData::Callable(shape_id) => AbstractConstructorKind::Callable(shape_id),
        TypeData::Application(app_id) => AbstractConstructorKind::Application(app_id),
        _ => AbstractConstructorKind::NotAbstract,
    }
}

// =============================================================================
// Property Access Resolution Classification
// =============================================================================

/// Classification for resolving types for property access.
#[derive(Debug, Clone)]
pub enum PropertyAccessResolutionKind {
    /// Lazy type (`DefId`) - needs resolution to actual type
    Lazy(DefId),
    /// `TypeQuery` (typeof) - resolve the symbol
    TypeQuery(crate::types::SymbolRef),
    /// Application - needs evaluation
    Application(crate::types::TypeApplicationId),
    /// Type parameter - follow constraint
    TypeParameter { constraint: Option<TypeId> },
    /// Complex types that need evaluation
    NeedsEvaluation,
    /// Union - resolve each member
    Union(Vec<TypeId>),
    /// Intersection - resolve each member
    Intersection(Vec<TypeId>),
    /// Readonly wrapper - unwrap
    Readonly(TypeId),
    /// Function or Callable - may need Function interface
    FunctionLike,
    /// Already resolved
    Resolved,
}

/// Classify a type for property access resolution.
pub fn classify_for_property_access_resolution(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> PropertyAccessResolutionKind {
    let Some(key) = db.lookup(type_id) else {
        return PropertyAccessResolutionKind::Resolved;
    };

    match key {
        TypeData::TypeQuery(sym_ref) => PropertyAccessResolutionKind::TypeQuery(sym_ref),
        TypeData::Lazy(def_id) => PropertyAccessResolutionKind::Lazy(def_id),
        TypeData::Application(app_id) => PropertyAccessResolutionKind::Application(app_id),
        TypeData::TypeParameter(info) | TypeData::Infer(info) => {
            PropertyAccessResolutionKind::TypeParameter {
                constraint: info.constraint,
            }
        }
        TypeData::Conditional(_)
        | TypeData::Mapped(_)
        | TypeData::IndexAccess(_, _)
        | TypeData::KeyOf(_) => PropertyAccessResolutionKind::NeedsEvaluation,
        TypeData::Union(list_id) => {
            let members = db.type_list(list_id);
            PropertyAccessResolutionKind::Union(members.to_vec())
        }
        TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            PropertyAccessResolutionKind::Intersection(members.to_vec())
        }
        TypeData::ReadonlyType(inner) => PropertyAccessResolutionKind::Readonly(inner),
        TypeData::Function(_) | TypeData::Callable(_) => PropertyAccessResolutionKind::FunctionLike,
        _ => PropertyAccessResolutionKind::Resolved,
    }
}

// =============================================================================
// Contextual Type Literal Allow Classification
// =============================================================================

/// Classification for checking if contextual type allows literals.
#[derive(Debug, Clone)]
pub enum ContextualLiteralAllowKind {
    /// Union or Intersection - check all members
    Members(Vec<TypeId>),
    /// Type parameter - check constraint
    TypeParameter { constraint: Option<TypeId> },
    /// Application - needs evaluation
    Application,
    /// Mapped type - needs evaluation
    Mapped,
    /// Template literal type - always allows string literals (pattern matching check
    /// happens later during assignability). This prevents premature widening of string
    /// literals like `"*hello*"` to `string` when the target is `` `*${string}*` ``.
    TemplateLiteral,
    /// Does not allow literal
    NotAllowed,
}

/// Classify a type for contextual literal checking.
pub fn classify_for_contextual_literal(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ContextualLiteralAllowKind {
    let Some(key) = db.lookup(type_id) else {
        return ContextualLiteralAllowKind::NotAllowed;
    };

    match key {
        TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            ContextualLiteralAllowKind::Members(members.to_vec())
        }
        TypeData::TypeParameter(info) | TypeData::Infer(info) => {
            ContextualLiteralAllowKind::TypeParameter {
                constraint: info.constraint,
            }
        }
        TypeData::Application(_) => ContextualLiteralAllowKind::Application,
        TypeData::Mapped(_) => ContextualLiteralAllowKind::Mapped,
        TypeData::TemplateLiteral(_) => ContextualLiteralAllowKind::TemplateLiteral,
        _ => ContextualLiteralAllowKind::NotAllowed,
    }
}

// =============================================================================
// Mapped Constraint Resolution Classification
// =============================================================================

/// Classification for evaluating mapped type constraints.
#[derive(Debug, Clone)]
pub enum MappedConstraintKind {
    /// `KeyOf` type - evaluate operand
    KeyOf(TypeId),
    /// Union or Literal - return as-is
    Resolved,
    /// Other type - return as-is
    Other,
}

/// Classify a constraint type for mapped type evaluation.
pub fn classify_mapped_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> MappedConstraintKind {
    let Some(key) = db.lookup(type_id) else {
        return MappedConstraintKind::Other;
    };

    match key {
        TypeData::KeyOf(operand) => MappedConstraintKind::KeyOf(operand),
        TypeData::Union(_) | TypeData::Literal(_) => MappedConstraintKind::Resolved,
        _ => MappedConstraintKind::Other,
    }
}

// =============================================================================
// Type Resolution Classification
// =============================================================================

/// Classification for evaluating types with symbol resolution.
#[derive(Debug, Clone)]
pub enum TypeResolutionKind {
    /// Lazy - resolve to symbol type via `DefId`
    Lazy(DefId),
    /// Application - evaluate the application
    Application,
    /// Already resolved
    Resolved,
}

/// Classify a type for resolution.
pub fn classify_for_type_resolution(db: &dyn TypeDatabase, type_id: TypeId) -> TypeResolutionKind {
    let Some(key) = db.lookup(type_id) else {
        return TypeResolutionKind::Resolved;
    };

    match key {
        TypeData::Lazy(def_id) => TypeResolutionKind::Lazy(def_id),
        TypeData::Application(_) => TypeResolutionKind::Application,
        _ => TypeResolutionKind::Resolved,
    }
}

// =============================================================================
// Type Argument Extraction Classification
// =============================================================================

/// Classification for extracting type parameters from a type for instantiation.
#[derive(Debug, Clone)]
pub enum TypeArgumentExtractionKind {
    /// Function type with type params
    Function(crate::types::FunctionShapeId),
    /// Callable type with signatures potentially having type params
    Callable(crate::types::CallableShapeId),
    /// Not applicable
    Other,
}

/// Classify a type for type argument extraction.
pub fn classify_for_type_argument_extraction(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeArgumentExtractionKind {
    let Some(key) = db.lookup(type_id) else {
        return TypeArgumentExtractionKind::Other;
    };

    match key {
        TypeData::Function(shape_id) => TypeArgumentExtractionKind::Function(shape_id),
        TypeData::Callable(shape_id) => TypeArgumentExtractionKind::Callable(shape_id),
        _ => TypeArgumentExtractionKind::Other,
    }
}

// =============================================================================
// Base Instance Properties Merge Classification
// =============================================================================

/// Classification for merging base instance properties.
#[derive(Debug, Clone)]
pub enum BaseInstanceMergeKind {
    /// Object type with shape
    Object(crate::types::ObjectShapeId),
    /// Intersection - merge all members
    Intersection(Vec<TypeId>),
    /// Union - find common properties
    Union(Vec<TypeId>),
    /// Not mergeable
    Other,
}

/// Classify a type for base instance property merging.
pub fn classify_for_base_instance_merge(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> BaseInstanceMergeKind {
    let Some(key) = db.lookup(type_id) else {
        return BaseInstanceMergeKind::Other;
    };

    match key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            BaseInstanceMergeKind::Object(shape_id)
        }
        TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            BaseInstanceMergeKind::Intersection(members.to_vec())
        }
        TypeData::Union(list_id) => {
            let members = db.type_list(list_id);
            BaseInstanceMergeKind::Union(members.to_vec())
        }
        _ => BaseInstanceMergeKind::Other,
    }
}
