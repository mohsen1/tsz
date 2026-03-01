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

/// Check if two types are literals of the same base kind.
///
/// Returns true when both are string literals, both are number literals,
/// both are boolean literals, or both are bigint literals.
/// This implements tsc's rule: "If the contextual type is a literal type,
/// we consider this a literal context for all literals of the same base type."
pub fn are_same_base_literal_kind(db: &dyn TypeDatabase, a: TypeId, b: TypeId) -> bool {
    use LiteralTypeKind::*;
    matches!(
        (classify_literal_type(db, a), classify_literal_type(db, b)),
        (String(_), String(_))
            | (Number(_), Number(_))
            | (Boolean(_), Boolean(_))
            | (BigInt(_), BigInt(_))
    )
}

/// Get number value from a number literal type.
pub fn get_number_literal_value(db: &dyn TypeDatabase, type_id: TypeId) -> Option<f64> {
    match classify_literal_type(db, type_id) {
        LiteralTypeKind::Number(value) => Some(value),
        _ => None,
    }
}

// =============================================================================
// Index Type Classification
// =============================================================================

/// Returns the specific `TypeId` within the given type (e.g., inside a union)
/// that makes it invalid for indexing.
pub fn get_invalid_index_type_member(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    let mut visited = FxHashSet::default();
    is_invalid_index_type_inner(db, type_id, &mut visited)
}

fn is_invalid_index_type_inner(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    visited: &mut FxHashSet<TypeId>,
) -> Option<TypeId> {
    if !visited.insert(type_id) {
        return None;
    }

    if matches!(
        type_id,
        TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR | TypeId::NEVER
    ) {
        return None;
    }

    let is_invalid = match db.lookup(type_id) {
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
            | TypeData::Callable(_)
            | TypeData::Lazy(_),
        ) => true,
        Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => {
            for &member in db.type_list(list_id).iter() {
                if let Some(invalid_member) = is_invalid_index_type_inner(db, member, visited) {
                    return Some(invalid_member);
                }
            }
            false
        }
        Some(TypeData::TypeParameter(info)) => {
            if let Some(constraint) = info.constraint
                && let Some(invalid_member) = is_invalid_index_type_inner(db, constraint, visited)
            {
                return Some(invalid_member);
            }
            false
        }
        _ => false,
    };

    if is_invalid { Some(type_id) } else { None }
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

/// Extract string, numeric, enum, or unique symbol property name from a type.
pub fn get_literal_property_name(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_common::interner::Atom> {
    match db.lookup(type_id) {
        Some(TypeData::Literal(crate::types::LiteralValue::String(name))) => Some(name),
        Some(TypeData::Literal(crate::types::LiteralValue::Number(num))) => {
            // Format number exactly like TS (e.g. 1.0 -> "1")
            let s = format!("{}", num.0);
            Some(db.intern_string(&s))
        }
        Some(TypeData::UniqueSymbol(sym)) => {
            let s = format!("__unique_{}", sym.0);
            Some(db.intern_string(&s))
        }
        Some(TypeData::Enum(_, member_type)) => get_literal_property_name(db, member_type),
        _ => None,
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
        TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
            // For unions/intersections, collect call signatures from all callable members.
            // Intersections arise from merged declarations (e.g., function + namespace).
            let members = db.type_list(list_id);
            let mut call_signatures = Vec::new();

            for &member in members.iter() {
                match db.lookup(member) {
                    Some(TypeData::Callable(shape_id)) => {
                        let shape = db.callable_shape(shape_id);
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
// Ref Type Resolution
// =============================================================================

/// Classification for Lazy type resolution.
#[derive(Debug, Clone)]
pub enum LazyTypeKind {
    /// `DefId` - resolve to actual type
    Lazy(crate::def::DefId),
    /// Not a Lazy type
    NotLazy,
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
        Some(TypeData::Literal(ref lit)) => lit.primitive_type_id(),
        _ => type_id,
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
        // Homomorphic mapped types over array-like sources preserve array structure.
        // e.g., `{ [K in keyof T]: T[K] }` where `T extends readonly unknown[]`
        // is still array-like because it maps over an array/tuple.
        Some(TypeData::Mapped(mapped_id)) => {
            let mapped = db.mapped_type(mapped_id);
            if let Some(TypeData::KeyOf(source)) = db.lookup(mapped.constraint) {
                classify_array_like(db, source)
            } else {
                ArrayLikeKind::Other
            }
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
    /// Template literal type like `${number}` — a numeric string type that
    /// can index both string and number index signatures.
    NumericStringLike,
    /// Template literal type like `${string}` or `hello${string}` — a string
    /// subtype that can index string index signatures.
    TemplateLiteralString,
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
        Some(TypeData::TemplateLiteral(tl_id)) => {
            // Check if this is a "numeric string-like" template literal.
            // `${number}` (single Type(number) span, no text) is a numeric string type
            // that can index arrays and number index signatures.
            let spans = db.template_list(tl_id);
            if is_numeric_string_template(&spans) {
                IndexKeyKind::NumericStringLike
            } else {
                IndexKeyKind::TemplateLiteralString
            }
        }
        _ => IndexKeyKind::Other,
    }
}

/// Check if template literal spans represent a numeric string type.
/// A template literal is "numeric string-like" if it consists solely of
/// a single `Type(number)` span with no text prefix/suffix, i.e. `${number}`.
fn is_numeric_string_template(spans: &[crate::TemplateSpan]) -> bool {
    matches!(
        spans,
        [crate::TemplateSpan::Type(ty)] if *ty == TypeId::NUMBER
    )
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
    // Check union on the RAW type BEFORE evaluation.
    // evaluate_type can collapse unions via subtype simplification
    // (e.g. `{ a: number } | { [s: string]: number }` becomes just the indexed type),
    // which loses per-constituent indexability information needed for TS7053 checks.
    // Note: we only do this for unions, not intersections. Intersections need evaluation
    // to resolve to their structural form (e.g. conditional type inference with `infer`).
    if let Some(TypeData::Union(members_id)) = db.lookup(type_id) {
        return ElementIndexableKind::Union(db.type_list(members_id).to_vec());
    }

    // Evaluate to resolve Lazy/Application/Conditional wrappers
    // to their underlying structural form (e.g., Application(Boxified, [T]) → Mapped).
    let evaluated = crate::evaluation::evaluate::evaluate_type(db, type_id);
    match db.lookup(evaluated) {
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
        // Type parameters represent unknown types whose index signatures are deferred —
        // tsc creates T[K] types rather than reporting TS7053.
        // Treat both as having string and number index signatures.
        Some(TypeData::Enum(_, _)) | Some(TypeData::TypeParameter(_)) => {
            ElementIndexableKind::ObjectWithIndex {
                has_string: true,
                has_number: true,
            }
        }
        // Generic mapped types (e.g. `{ [K in keyof T]: V }`) act as having an implicit
        // string index signature in tsc. When the constraint can't be fully resolved (generic),
        // the mapped type remains unevaluated and should be treated as string-indexable
        // to avoid false positive TS7053 errors.
        Some(TypeData::Mapped(_)) => ElementIndexableKind::ObjectWithIndex {
            has_string: true,
            has_number: false,
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
    /// `TypeQuery` (`typeof M`) — the checker should resolve the `SymbolRef` to
    /// the underlying symbol type and re-classify.
    TypeQuery(crate::types::SymbolRef),
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
        Some(TypeData::TypeQuery(sym_ref)) => NamespaceMemberKind::TypeQuery(sym_ref),
        _ => NamespaceMemberKind::Other,
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
        TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
            PropertyAccessResolutionKind::Readonly(inner)
        }
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
