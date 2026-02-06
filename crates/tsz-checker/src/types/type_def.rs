//! Core type definitions for the type checker.
//!
//! This module contains the main `Type` enum and all type variant structs.

use super::flags::{signature_flags, type_flags};
use serde::Serialize;
use tsz_binder::{SymbolId, SymbolTable};
use tsz_parser::parser::NodeIndex;

// =============================================================================
// Type ID
// =============================================================================

/// Unique identifier for a type in the type arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct TypeId(pub u32);

impl TypeId {
    pub const NONE: TypeId = TypeId(u32::MAX);

    pub fn is_none(&self) -> bool {
        self.0 == u32::MAX
    }
}

// =============================================================================
// Literal Values
// =============================================================================

/// A literal value for literal types.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum LiteralValue {
    String(String),
    Number(f64),
    BigInt(String), // Store as string for precision
    Boolean(bool),
}

// =============================================================================
// Signature
// =============================================================================

/// A function or method signature.
#[derive(Clone, Debug, Serialize)]
pub struct Signature {
    pub declaration: NodeIndex,
    pub type_parameters: Vec<TypeId>,
    pub parameters: Vec<SymbolId>,
    pub this_parameter: Option<SymbolId>,
    pub resolved_return_type: Option<TypeId>,
    pub min_argument_count: u32,
    pub flags: u32,
}

impl Signature {
    pub fn new(declaration: NodeIndex) -> Self {
        Signature {
            declaration,
            type_parameters: Vec::new(),
            parameters: Vec::new(),
            this_parameter: None,
            resolved_return_type: None,
            min_argument_count: 0,
            flags: signature_flags::NONE,
        }
    }
}

// =============================================================================
// Index Info
// =============================================================================

/// Information about an index signature.
#[derive(Clone, Debug, Serialize)]
pub struct IndexInfo {
    pub key_type: TypeId,
    pub value_type: TypeId,
    pub is_readonly: bool,
    pub declaration: Option<NodeIndex>,
}

// =============================================================================
// Type Variants
// =============================================================================

/// An intrinsic type (primitives like string, number, etc.)
#[derive(Clone, Debug, Serialize)]
pub struct IntrinsicType {
    pub flags: u32,
    pub intrinsic_name: String,
}

/// A literal type (specific string, number, boolean value)
#[derive(Clone, Debug, Serialize)]
pub struct LiteralType {
    pub flags: u32,
    pub value: LiteralValue,
    pub fresh_type: TypeId,   // Widening version
    pub regular_type: TypeId, // Non-widening version
}

/// An object type (class, interface, object literal, etc.)
#[derive(Clone, Debug, Serialize)]
pub struct ObjectType {
    pub flags: u32,
    pub object_flags: u32,
    pub symbol: SymbolId,
    pub members: SymbolTable,
    pub properties: Vec<SymbolId>,
    pub call_signatures: Vec<Signature>,
    pub construct_signatures: Vec<Signature>,
    pub index_infos: Vec<IndexInfo>,
}

impl ObjectType {
    pub fn new(object_flags: u32, symbol: SymbolId) -> Self {
        ObjectType {
            flags: type_flags::OBJECT,
            object_flags,
            symbol,
            members: SymbolTable::new(),
            properties: Vec::new(),
            call_signatures: Vec::new(),
            construct_signatures: Vec::new(),
            index_infos: Vec::new(),
        }
    }

    /// Check if this object type has specific object flags set.
    pub fn has_object_flags(&self, flags: u32) -> bool {
        (self.object_flags & flags) != 0
    }
}

/// A type reference (Array<T>, Map<K, V>, etc.)
#[derive(Clone, Debug, Serialize)]
pub struct TypeReference {
    pub flags: u32,
    pub object_flags: u32,
    pub target: TypeId,              // The generic type
    pub type_arguments: Vec<TypeId>, // The type arguments
    pub symbol: SymbolId,
}

/// A union type (A | B | C)
#[derive(Clone, Debug, Serialize)]
pub struct UnionType {
    pub flags: u32,
    pub object_flags: u32,
    pub types: Vec<TypeId>,
    pub origin: Option<TypeId>,
}

impl UnionType {
    pub fn new(types: Vec<TypeId>) -> Self {
        UnionType {
            flags: type_flags::UNION,
            object_flags: 0,
            types,
            origin: None,
        }
    }
}

/// An intersection type (A & B & C)
#[derive(Clone, Debug, Serialize)]
pub struct IntersectionType {
    pub flags: u32,
    pub object_flags: u32,
    pub types: Vec<TypeId>,
}

impl IntersectionType {
    pub fn new(types: Vec<TypeId>) -> Self {
        IntersectionType {
            flags: type_flags::INTERSECTION,
            object_flags: 0,
            types,
        }
    }
}

/// A type parameter (T, K extends keyof T, etc.)
#[derive(Clone, Debug, Serialize)]
pub struct TypeParameter {
    pub flags: u32,
    pub symbol: SymbolId,
    pub constraint: TypeId, // extends clause
    pub default: TypeId,    // default type
    pub target: TypeId,     // For substitution
    pub is_this_type: bool,
    /// Whether this is a `const` type parameter (TS 5.0+): function foo<const T>()
    /// Const type parameters cause literal inference (e.g., ['a', 'b'] instead of string[])
    pub is_const: bool,
}

impl TypeParameter {
    pub fn new(symbol: SymbolId) -> Self {
        TypeParameter {
            flags: type_flags::TYPE_PARAMETER,
            symbol,
            constraint: TypeId::NONE,
            default: TypeId::NONE,
            target: TypeId::NONE,
            is_this_type: false,
            is_const: false,
        }
    }
}

/// A conditional type (T extends U ? X : Y)
#[derive(Clone, Debug, Serialize)]
pub struct ConditionalType {
    pub flags: u32,
    pub check_type: TypeId,
    pub extends_type: TypeId,
    pub true_type: TypeId,
    pub false_type: TypeId,
    pub is_distributive: bool,
    pub infer_type_parameters: Vec<TypeId>,
}

/// Modifier for mapped type readonly/optional modifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum MappedTypeModifier {
    /// No modifier specified, preserve from source type
    None,
    /// + modifier (add the modifier)
    Plus,
    /// - modifier (remove the modifier)
    Minus,
}

/// A mapped type ({ [K in keyof T]: ... })
#[derive(Clone, Debug, Serialize)]
pub struct MappedType {
    pub flags: u32,
    pub object_flags: u32,
    pub declaration: NodeIndex,
    pub type_parameter: TypeId,
    pub constraint_type: TypeId,
    pub name_type: TypeId, // as clause
    pub template_type: TypeId,
    /// Readonly modifier: None (preserve), Plus (+readonly), Minus (-readonly)
    pub readonly_modifier: MappedTypeModifier,
    /// Optional modifier: None (preserve), Plus (+?), Minus (-?)
    pub optional_modifier: MappedTypeModifier,
}

/// An indexed access type (T[K])
#[derive(Clone, Debug, Serialize)]
pub struct IndexedAccessType {
    pub flags: u32,
    pub object_type: TypeId,
    pub index_type: TypeId,
    pub constraint: TypeId,
}

/// An index type (keyof T)
#[derive(Clone, Debug, Serialize)]
pub struct IndexType {
    pub flags: u32,
    pub source_type: TypeId,
}

/// A template literal type (`hello ${T}`)
#[derive(Clone, Debug, Serialize)]
pub struct TemplateLiteralType {
    pub flags: u32,
    pub texts: Vec<String>,
    pub types: Vec<TypeId>,
}

/// A function type ((x: T) => U)
#[derive(Clone, Debug, Serialize)]
pub struct FunctionType {
    pub flags: u32,
    pub object_flags: u32,
    pub declaration: NodeIndex,
    pub parameter_types: Vec<TypeId>,
    pub parameter_names: Vec<String>,
    pub return_type: TypeId,
    pub type_parameters: Vec<TypeId>,
    pub min_argument_count: u32,
    pub has_rest_parameter: bool,
    /// The type of `this` parameter if explicitly specified: `function foo(this: SomeType)`
    pub this_type: Option<TypeId>,
}

/// An array type (T[] or Array<T>).
#[derive(Clone, Debug, Serialize)]
pub struct ArrayTypeInfo {
    pub flags: u32,
    pub element_type: TypeId,
    /// Whether this is a readonly array (readonly T[] or ReadonlyArray<T>)
    pub is_readonly: bool,
}

/// Flags for individual tuple element kinds
pub mod element_flags {
    /// A required element (no modifier)
    pub const REQUIRED: u32 = 1 << 0;
    /// An optional element (name?: type)
    pub const OPTIONAL: u32 = 1 << 1;
    /// A rest element (...type)
    pub const REST: u32 = 1 << 2;
    /// A variadic element (spread of another tuple: ...T where T is a tuple type)
    pub const VARIADIC: u32 = 1 << 3;
}

/// A tuple type ([T, U, V] or [name: T, name2: U]).
#[derive(Clone, Debug, Serialize)]
pub struct TupleTypeInfo {
    pub flags: u32,
    pub element_types: Vec<TypeId>,
    /// Per-element flags (element_flags::REQUIRED, OPTIONAL, REST, VARIADIC)
    /// This allows variadic tuples like [...T, ...U] where multiple rest elements are allowed.
    pub element_flags: Vec<u32>,
    /// Element names for named tuples (e.g., [x: number, y: number]).
    /// None means all elements are unnamed.
    /// Some(vec) contains Option<String> for each position - None means unnamed, Some(name) means named.
    pub element_names: Option<Vec<Option<String>>>,
    /// Whether the tuple has optional elements
    pub has_optional_elements: bool,
    /// Whether the tuple has a rest element
    pub has_rest_element: bool,
    /// Whether this is a readonly tuple (readonly [T, U])
    pub is_readonly: bool,
}

/// Information about an enum type.
#[derive(Clone, Debug, Serialize)]
pub struct EnumTypeInfo {
    pub flags: u32,
    /// The name of the enum
    pub name: String,
    /// The member name -> value type mapping
    pub members: Vec<(String, TypeId)>,
}

/// ThisType<T> marker type - specifies the type of 'this' in object literal methods.
/// When an object literal has a contextual type that includes ThisType<T>,
/// the 'this' keyword within methods of that literal is typed as T.
#[derive(Clone, Debug, Serialize)]
pub struct ThisTypeMarker {
    pub flags: u32,
    /// The type that 'this' should be within object literal methods
    pub constraint: TypeId,
}

/// A unique symbol type.
/// `const sym: unique symbol = Symbol();` creates a unique type for that specific symbol.
/// Each unique symbol declaration creates a distinct, incompatible type.
#[derive(Clone, Debug, Serialize)]
pub struct UniqueSymbolType {
    pub flags: u32,
    /// The symbol (variable) that this unique symbol is tied to
    pub symbol: SymbolId,
    /// Name for display purposes
    pub name: String,
}

// =============================================================================
// Type Enum
// =============================================================================

/// All possible type variants.
/// Large variants are boxed to keep the enum size small (improves cache locality).
#[derive(Clone, Debug, Serialize)]
pub enum Type {
    Intrinsic(IntrinsicType),
    Literal(LiteralType),
    Object(Box<ObjectType>),
    TypeReference(Box<TypeReference>),
    Union(Box<UnionType>),
    Intersection(Box<IntersectionType>),
    TypeParameter(Box<TypeParameter>),
    Conditional(Box<ConditionalType>),
    Mapped(Box<MappedType>),
    IndexedAccess(Box<IndexedAccessType>),
    Index(Box<IndexType>),
    TemplateLiteral(Box<TemplateLiteralType>),
    Function(Box<FunctionType>),
    Array(Box<ArrayTypeInfo>),
    Tuple(Box<TupleTypeInfo>),
    Enum(Box<EnumTypeInfo>),
    /// ThisType<T> marker - specifies 'this' type in object literal methods
    ThisType(Box<ThisTypeMarker>),
    /// Unique symbol type - each declaration creates a distinct type
    UniqueSymbol(Box<UniqueSymbolType>),
}

impl Type {
    /// Get the flags for this type.
    pub fn flags(&self) -> u32 {
        match self {
            Type::Intrinsic(t) => t.flags,
            Type::Literal(t) => t.flags,
            Type::Object(t) => t.flags,
            Type::TypeReference(t) => t.flags,
            Type::Union(t) => t.flags,
            Type::Intersection(t) => t.flags,
            Type::TypeParameter(t) => t.flags,
            Type::Conditional(t) => t.flags,
            Type::Mapped(t) => t.flags,
            Type::IndexedAccess(t) => t.flags,
            Type::Index(t) => t.flags,
            Type::TemplateLiteral(t) => t.flags,
            Type::Function(t) => t.flags,
            Type::Array(t) => t.flags,
            Type::Tuple(t) => t.flags,
            Type::Enum(t) => t.flags,
            Type::ThisType(t) => t.flags,
            Type::UniqueSymbol(t) => t.flags,
        }
    }

    /// Check if type has all specified flags.
    pub fn has_flags(&self, flags: u32) -> bool {
        (self.flags() & flags) == flags
    }

    /// Check if type has any of specified flags.
    pub fn has_any_flags(&self, flags: u32) -> bool {
        (self.flags() & flags) != 0
    }
}
