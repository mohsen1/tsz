//! Type representation for the structural solver.
//!
//! Types are represented as lightweight `TypeId` handles that point into
//! an interning table. The actual structure is stored in `TypeKey`.

use crate::interner::Atom;
use serde::Serialize;

/// A lightweight handle to an interned type.
/// Equality check is O(1) - just compare the u32 values.
///
/// # Sentinel Value Semantics
///
/// The following sentinel values have specific semantics for error handling and type inference:
///
/// ## `TypeId::ERROR`
/// Used when type resolution **fails** due to an actual error:
/// - Missing AST nodes or invalid syntax
/// - Type annotation that cannot be resolved
/// - Failed type inference with no fallback
///
/// **Error propagation**: ERROR is "contagious" - operations on ERROR types return ERROR.
/// This prevents cascading errors from a single root cause. Property access on ERROR
/// returns ERROR silently (no additional diagnostics emitted).
///
/// **Example uses:**
/// - Missing type annotation: `let x;` -> ERROR (prevents "any poisoning")
/// - Failed generic inference with no constraint/default
/// - Invalid type syntax or unresolved type references
///
/// ## `TypeId::UNKNOWN`
/// The TypeScript `unknown` type - a type-safe alternative to `any`.
/// Use when the type is genuinely unknown at compile time, but should be
/// checked before use.
///
/// **Strict behavior**: Property access on UNKNOWN returns `IsUnknown` result,
/// which the checker reports as TS2571 "Object is of type 'unknown'".
///
/// **Example uses:**
/// - Explicit `unknown` type annotation
/// - Return type of functions that could return anything
/// - Missing `this` parameter type (stricter than `any`)
///
/// ## `TypeId::ANY`
/// The TypeScript `any` type - opts out of type checking entirely.
/// Use for intentional any-typed values or interop with untyped code.
///
/// **Permissive behavior**: Property access on ANY succeeds and returns ANY.
/// No type errors are produced for any-typed expressions.
///
/// **Example uses:**
/// - Explicit `any` type annotation
/// - Arrays with no element type context: `[]` defaults to `any[]`
/// - Interop with JavaScript libraries without type definitions
///
/// ## `TypeId::NEVER`
/// The bottom type - represents values that can never exist.
/// Used for exhaustive checking and functions that never return.
///
/// **Example uses:**
/// - Function that always throws or loops forever
/// - Exhaustive switch/if narrowing (remaining type after all cases)
/// - Intersection of incompatible types
///
/// ## Summary: When to Use Each
///
/// | Scenario                          | Use           |
/// |-----------------------------------|---------------|
/// | Type resolution failed            | `ERROR`       |
/// | Missing required type annotation  | `ERROR`       |
/// | Failed inference (no fallback)    | `ERROR`       |
/// | Explicit `unknown` annotation     | `UNKNOWN`     |
/// | Missing `this` parameter type     | `UNKNOWN`     |
/// | Explicit `any` annotation         | `ANY`         |
/// | Empty array literal `[]`          | `any[]`       |
/// | Function never returns            | `NEVER`       |
/// | Exhaustive narrowing remainder    | `NEVER`       |
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Default)]
pub struct TypeId(pub u32);

impl TypeId {
    /// Internal placeholder - no valid type.
    pub const NONE: TypeId = TypeId(0);

    /// Error sentinel - type resolution failed.
    /// Propagates through operations to prevent cascading errors.
    /// See struct-level docs for detailed semantics.
    pub const ERROR: TypeId = TypeId(1);

    /// The bottom type - represents values that can never exist.
    /// Used for exhaustive checks and functions that never return.
    pub const NEVER: TypeId = TypeId(2);

    /// TypeScript's `unknown` type - type-safe top type.
    /// Requires type narrowing before use. See struct-level docs.
    pub const UNKNOWN: TypeId = TypeId(3);

    /// TypeScript's `any` type - opts out of type checking.
    /// All operations succeed, returning `any`. See struct-level docs.
    pub const ANY: TypeId = TypeId(4);

    /// The `void` type - used for functions with no meaningful return.
    pub const VOID: TypeId = TypeId(5);

    /// The `undefined` type - represents the undefined value.
    pub const UNDEFINED: TypeId = TypeId(6);

    /// The `null` type - represents the null value.
    pub const NULL: TypeId = TypeId(7);

    /// The `boolean` type - union of true | false.
    pub const BOOLEAN: TypeId = TypeId(8);

    /// The `number` type - all numeric values.
    pub const NUMBER: TypeId = TypeId(9);

    /// The `string` type - all string values.
    pub const STRING: TypeId = TypeId(10);

    /// The `bigint` type - arbitrary precision integers.
    pub const BIGINT: TypeId = TypeId(11);

    /// The `symbol` type - unique symbol values.
    pub const SYMBOL: TypeId = TypeId(12);

    /// The `object` type - any non-primitive value.
    pub const OBJECT: TypeId = TypeId(13);

    /// The literal type `true`.
    pub const BOOLEAN_TRUE: TypeId = TypeId(14);

    /// The literal type `false`.
    pub const BOOLEAN_FALSE: TypeId = TypeId(15);

    /// The `Function` type - any callable.
    pub const FUNCTION: TypeId = TypeId(16);

    /// Synthetic Promise base type for Promise<T> when Promise symbol is not resolved.
    /// Used to allow promise_like_return_type_argument to extract T from await expressions.
    pub const PROMISE_BASE: TypeId = TypeId(17);

    /// First user-defined type ID (after built-in intrinsics)
    pub const FIRST_USER: u32 = 100;

    pub fn is_intrinsic(self) -> bool {
        self.0 < Self::FIRST_USER
    }

    pub fn is_error(self) -> bool {
        self == Self::ERROR
    }

    pub fn is_any(self) -> bool {
        self == Self::ANY
    }

    pub fn is_unknown(self) -> bool {
        self == Self::UNKNOWN
    }

    pub fn is_never(self) -> bool {
        self == Self::NEVER
    }
}

/// Interned list of TypeId values (e.g., unions/intersections).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TypeListId(pub u32);

/// Interned object shape (properties + index signatures).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ObjectShapeId(pub u32);

/// Interned tuple element list.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TupleListId(pub u32);

/// Interned function shape.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FunctionShapeId(pub u32);

/// Interned callable shape.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct CallableShapeId(pub u32);

/// Interned type application (Base<Args>).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TypeApplicationId(pub u32);

/// Interned template literal span list.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TemplateLiteralId(pub u32);

/// Interned conditional type.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConditionalTypeId(pub u32);

/// Interned mapped type.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct MappedTypeId(pub u32);

/// Well-known Symbol property keys used in the iterator protocol.
/// These are used to represent `[Symbol.iterator]` and `[Symbol.asyncIterator]` property names.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum WellKnownSymbolKey {
    /// Symbol.iterator - used for sync iterables
    Iterator,
    /// Symbol.asyncIterator - used for async iterables
    AsyncIterator,
    /// Symbol.hasInstance - used for instanceof checks
    HasInstance,
    /// Symbol.isConcatSpreadable - used for array concat behavior
    IsConcatSpreadable,
    /// Symbol.match - used for String.match
    Match,
    /// Symbol.matchAll - used for String.matchAll
    MatchAll,
    /// Symbol.replace - used for String.replace
    Replace,
    /// Symbol.search - used for String.search
    Search,
    /// Symbol.split - used for String.split
    Split,
    /// Symbol.species - used for derived constructors
    Species,
    /// Symbol.toPrimitive - used for type coercion
    ToPrimitive,
    /// Symbol.toStringTag - used for Object.prototype.toString
    ToStringTag,
    /// Symbol.unscopables - used for with statement
    Unscopables,
    /// Symbol.dispose - used for using declarations
    Dispose,
    /// Symbol.asyncDispose - used for async using declarations
    AsyncDispose,
}

impl WellKnownSymbolKey {
    /// Returns the conventional string property name for this well-known symbol.
    /// This follows the convention of using `"[Symbol.iterator]"` etc. as property names.
    pub fn as_property_name(&self) -> &'static str {
        match self {
            WellKnownSymbolKey::Iterator => "[Symbol.iterator]",
            WellKnownSymbolKey::AsyncIterator => "[Symbol.asyncIterator]",
            WellKnownSymbolKey::HasInstance => "[Symbol.hasInstance]",
            WellKnownSymbolKey::IsConcatSpreadable => "[Symbol.isConcatSpreadable]",
            WellKnownSymbolKey::Match => "[Symbol.match]",
            WellKnownSymbolKey::MatchAll => "[Symbol.matchAll]",
            WellKnownSymbolKey::Replace => "[Symbol.replace]",
            WellKnownSymbolKey::Search => "[Symbol.search]",
            WellKnownSymbolKey::Split => "[Symbol.split]",
            WellKnownSymbolKey::Species => "[Symbol.species]",
            WellKnownSymbolKey::ToPrimitive => "[Symbol.toPrimitive]",
            WellKnownSymbolKey::ToStringTag => "[Symbol.toStringTag]",
            WellKnownSymbolKey::Unscopables => "[Symbol.unscopables]",
            WellKnownSymbolKey::Dispose => "[Symbol.dispose]",
            WellKnownSymbolKey::AsyncDispose => "[Symbol.asyncDispose]",
        }
    }

    /// Parses a property name string into a well-known symbol key.
    /// Returns `None` if the string is not a well-known symbol property name.
    pub fn from_property_name(name: &str) -> Option<Self> {
        match name {
            "[Symbol.iterator]" => Some(WellKnownSymbolKey::Iterator),
            "[Symbol.asyncIterator]" => Some(WellKnownSymbolKey::AsyncIterator),
            "[Symbol.hasInstance]" => Some(WellKnownSymbolKey::HasInstance),
            "[Symbol.isConcatSpreadable]" => Some(WellKnownSymbolKey::IsConcatSpreadable),
            "[Symbol.match]" => Some(WellKnownSymbolKey::Match),
            "[Symbol.matchAll]" => Some(WellKnownSymbolKey::MatchAll),
            "[Symbol.replace]" => Some(WellKnownSymbolKey::Replace),
            "[Symbol.search]" => Some(WellKnownSymbolKey::Search),
            "[Symbol.split]" => Some(WellKnownSymbolKey::Split),
            "[Symbol.species]" => Some(WellKnownSymbolKey::Species),
            "[Symbol.toPrimitive]" => Some(WellKnownSymbolKey::ToPrimitive),
            "[Symbol.toStringTag]" => Some(WellKnownSymbolKey::ToStringTag),
            "[Symbol.unscopables]" => Some(WellKnownSymbolKey::Unscopables),
            "[Symbol.dispose]" => Some(WellKnownSymbolKey::Dispose),
            "[Symbol.asyncDispose]" => Some(WellKnownSymbolKey::AsyncDispose),
            _ => None,
        }
    }
}

/// The structural "shape" of a type.
/// This is the key used for interning - structurally identical types
/// will have the same TypeKey and therefore the same TypeId.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TypeKey {
    /// Intrinsic types (any, unknown, never, void, null, undefined, boolean, number, string, bigint, symbol, object)
    Intrinsic(IntrinsicKind),

    /// Literal types ("hello", 42, true, 123n)
    Literal(LiteralValue),

    /// Object type with sorted property list for structural identity
    Object(ObjectShapeId),

    /// Object type with index signatures
    /// For objects like { [key: string]: number, foo: string }
    ObjectWithIndex(ObjectShapeId),

    /// Union type (A | B | C)
    Union(TypeListId),

    /// Intersection type (A & B & C)
    Intersection(TypeListId),

    /// Array type
    Array(TypeId),

    /// Tuple type
    Tuple(TupleListId),

    /// Function type
    Function(FunctionShapeId),

    /// Callable type with overloaded signatures
    /// For interfaces with call/construct signatures
    Callable(CallableShapeId),

    /// Type parameter (generic)
    TypeParameter(TypeParamInfo),

    /// Reference to a named type (interface, class, type alias)
    /// Uses SymbolId to break infinite recursion
    Ref(SymbolRef),

    /// Generic type application (Base<Args>)
    Application(TypeApplicationId),

    /// Conditional type (T extends U ? X : Y)
    Conditional(ConditionalTypeId),

    /// Mapped type ({ [K in Keys]: ValueType })
    Mapped(MappedTypeId),

    /// Index access type (T[K])
    IndexAccess(TypeId, TypeId),

    /// Template literal type (`hello${string}world`)
    TemplateLiteral(TemplateLiteralId),

    /// Type query (typeof expression in type position)
    TypeQuery(SymbolRef),

    /// KeyOf type operator (keyof T)
    KeyOf(TypeId),

    /// Readonly type modifier (readonly T[])
    ReadonlyType(TypeId),

    /// Unique symbol type
    UniqueSymbol(SymbolRef),

    /// Infer type (infer R in conditional types)
    Infer(TypeParamInfo),

    /// This type (polymorphic this)
    ThisType,

    /// String manipulation intrinsic types
    /// Uppercase<T>, Lowercase<T>, Capitalize<T>, Uncapitalize<T>
    StringIntrinsic {
        kind: StringIntrinsicKind,
        type_arg: TypeId,
    },

    /// Error type for recovery
    Error,
}

/// Generic type application (Base<Args>)
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TypeApplication {
    pub base: TypeId,
    pub args: Vec<TypeId>,
}

/// Intrinsic type kinds
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum IntrinsicKind {
    Any,
    Unknown,
    Never,
    Void,
    Null,
    Undefined,
    Boolean,
    Number,
    String,
    Bigint,
    Symbol,
    Object,
    Function,
}

/// String manipulation intrinsic kinds
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum StringIntrinsicKind {
    Uppercase,
    Lowercase,
    Capitalize,
    Uncapitalize,
}

impl IntrinsicKind {
    pub fn to_type_id(self) -> TypeId {
        match self {
            IntrinsicKind::Any => TypeId::ANY,
            IntrinsicKind::Unknown => TypeId::UNKNOWN,
            IntrinsicKind::Never => TypeId::NEVER,
            IntrinsicKind::Void => TypeId::VOID,
            IntrinsicKind::Null => TypeId::NULL,
            IntrinsicKind::Undefined => TypeId::UNDEFINED,
            IntrinsicKind::Boolean => TypeId::BOOLEAN,
            IntrinsicKind::Number => TypeId::NUMBER,
            IntrinsicKind::String => TypeId::STRING,
            IntrinsicKind::Bigint => TypeId::BIGINT,
            IntrinsicKind::Symbol => TypeId::SYMBOL,
            IntrinsicKind::Object => TypeId::OBJECT,
            IntrinsicKind::Function => TypeId::FUNCTION,
        }
    }
}

/// Literal values (for literal types)
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LiteralValue {
    String(Atom),
    Number(OrderedFloat),
    BigInt(Atom),
    Boolean(bool),
}

/// Wrapper for f64 that implements Eq and Hash for use in TypeKey
#[derive(Clone, Copy, Debug)]
pub struct OrderedFloat(pub f64);

impl PartialEq for OrderedFloat {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for OrderedFloat {}

impl std::hash::Hash for OrderedFloat {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

/// Property information for object types
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct PropertyInfo {
    pub name: Atom,
    /// Read type (getter/lookup).
    pub type_id: TypeId,
    /// Write type (setter/assignment).
    pub write_type: TypeId,
    pub optional: bool,
    pub readonly: bool,
    pub is_method: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PropertyLookup {
    Found(usize),
    NotFound,
    Uncached,
}

/// Index signature information for object types
/// Represents `{ [key: string]: ValueType }` or `{ [key: number]: ValueType }`
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct IndexSignature {
    /// The key type (usually string or number)
    pub key_type: TypeId,
    /// The value type for all indexed properties
    pub value_type: TypeId,
    /// Whether the index signature is readonly
    pub readonly: bool,
}

/// Combined index signature information for a type
/// Provides convenient access to both string and number index signatures
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct IndexInfo {
    /// String index signature: { [key: string]: T }
    pub string_index: Option<IndexSignature>,
    /// Number index signature: { [key: number]: T }
    pub number_index: Option<IndexSignature>,
}

/// Object type with properties and optional index signatures
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ObjectShape {
    /// Named properties (sorted by name for consistent hashing)
    pub properties: Vec<PropertyInfo>,
    /// String index signature: { [key: string]: T }
    pub string_index: Option<IndexSignature>,
    /// Number index signature: { [key: number]: T }
    pub number_index: Option<IndexSignature>,
}

/// Tuple element information
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TupleElement {
    pub type_id: TypeId,
    pub name: Option<Atom>,
    pub optional: bool,
    pub rest: bool,
}

/// Type predicate information (x is T / asserts x is T).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TypePredicate {
    pub asserts: bool,
    pub target: TypePredicateTarget,
    pub type_id: Option<TypeId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TypePredicateTarget {
    This,
    Identifier(Atom),
}

/// Function shape for function types
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FunctionShape {
    pub type_params: Vec<TypeParamInfo>,
    pub params: Vec<ParamInfo>,
    pub this_type: Option<TypeId>,
    pub return_type: TypeId,
    pub type_predicate: Option<TypePredicate>,
    pub is_constructor: bool,
    /// Whether this function is a method (bivariant parameters) vs a standalone function (contravariant when strictFunctionTypes)
    pub is_method: bool,
}

/// Call signature for overloaded functions
/// Represents a single call signature in an overloaded type
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CallSignature {
    pub type_params: Vec<TypeParamInfo>,
    pub params: Vec<ParamInfo>,
    pub this_type: Option<TypeId>,
    pub return_type: TypeId,
    pub type_predicate: Option<TypePredicate>,
    /// Whether this call signature is from a method (uses bivariant parameter checking).
    /// Methods in TypeScript are intentionally bivariant for compatibility reasons.
    pub is_method: bool,
}

/// Callable type with multiple overloaded call signatures
/// Represents types like:
/// ```typescript
/// interface Overloaded {
///   (x: string): number;
///   (x: number): string;
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct CallableShape {
    /// Call signatures (order matters for overload resolution)
    pub call_signatures: Vec<CallSignature>,
    /// Constructor signatures
    pub construct_signatures: Vec<CallSignature>,
    /// Optional properties on the callable (e.g., Function.prototype)
    pub properties: Vec<PropertyInfo>,
    /// String index signature (for static index signatures on classes)
    pub string_index: Option<IndexSignature>,
    /// Number index signature (for static index signatures on classes)
    pub number_index: Option<IndexSignature>,
}

/// Parameter information
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct ParamInfo {
    pub name: Option<Atom>,
    pub type_id: TypeId,
    pub optional: bool,
    pub rest: bool,
}

/// Type parameter information
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TypeParamInfo {
    pub name: Atom,
    pub constraint: Option<TypeId>,
    pub default: Option<TypeId>,
}

/// Reference to a symbol (for named types)
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SymbolRef(pub u32);

/// Conditional type structure
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConditionalType {
    pub check_type: TypeId,
    pub extends_type: TypeId,
    pub true_type: TypeId,
    pub false_type: TypeId,
    pub is_distributive: bool,
}

/// Mapped type structure
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MappedType {
    pub type_param: TypeParamInfo,
    pub constraint: TypeId,
    pub name_type: Option<TypeId>,
    pub template: TypeId,
    pub readonly_modifier: Option<MappedModifier>,
    pub optional_modifier: Option<MappedModifier>,
}

/// Mapped type modifier (+/-)
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum MappedModifier {
    Add,
    Remove,
}

/// Template literal span
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TemplateSpan {
    Text(Atom),
    Type(TypeId),
}

impl TemplateSpan {
    /// Check if this span is a text span
    pub fn is_text(&self) -> bool {
        matches!(self, TemplateSpan::Text(_))
    }

    /// Check if this span is a type interpolation
    pub fn is_type(&self) -> bool {
        matches!(self, TemplateSpan::Type(_))
    }

    /// Get the text content if this is a text span
    pub fn as_text(&self) -> Option<Atom> {
        match self {
            TemplateSpan::Text(atom) => Some(*atom),
            _ => None,
        }
    }

    /// Get the type ID if this is a type span
    pub fn as_type(&self) -> Option<TypeId> {
        match self {
            TemplateSpan::Type(type_id) => Some(*type_id),
            _ => None,
        }
    }

    /// Create a type span
    pub fn type_from_id(type_id: TypeId) -> Self {
        TemplateSpan::Type(type_id)
    }
}

/// Process escape sequences in a template literal string
/// Handles: \${, \\, \n, \r, \t, \b, \f, \v, \0, \xXX, \uXXXX, \u{X...}
pub fn process_template_escape_sequences(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars();
    let mut last_was_backslash = false;

    while let Some(c) = chars.next() {
        if last_was_backslash {
            last_was_backslash = false;
            match c {
                '$' => {
                    // \$${ becomes $ (not an interpolation)
                    result.push('$');
                }
                '\\' => result.push('\\'),
                'n' => result.push('\n'),
                'r' => result.push('\r'),
                't' => result.push('\t'),
                'b' => result.push('\x08'),
                'f' => result.push('\x0c'),
                'v' => result.push('\x0b'),
                '0' => result.push('\0'),
                'x' => {
                    // \xXX - exactly 2 hex digits
                    let hex1 = chars.next().unwrap_or('0');
                    let hex2 = chars.next().unwrap_or('0');
                    let code = u8::from_str_radix(&format!("{}{}", hex1, hex2), 16).unwrap_or(0);
                    result.push(code as char);
                }
                'u' => {
                    // \uXXXX or \u{X...}
                    if let Some('{') = chars.next() {
                        // \u{X...} - Unicode code point
                        let mut code_str = String::new();
                        while let Some(nc) = chars.next() {
                            if nc == '}' {
                                break;
                            }
                            code_str.push(nc);
                        }
                        if let Ok(code) = u32::from_str_radix(&code_str, 16) {
                            if let Some(c) = char::from_u32(code) {
                                result.push(c);
                            }
                        }
                    } else {
                        // \uXXXX - exactly 4 hex digits
                        let mut code_str = String::new();
                        for _ in 0..4 {
                            if let Some(nc) = chars.next() {
                                code_str.push(nc);
                            }
                        }
                        if let Ok(code) = u16::from_str_radix(&code_str, 16) {
                            if let Some(c) = char::from_u32(code as u32) {
                                result.push(c);
                            }
                        }
                    }
                }
                _ => {
                    // Unknown escape - preserve the backslash and character
                    result.push('\\');
                    result.push(c);
                }
            }
        } else if c == '\\' {
            last_was_backslash = true;
        } else {
            result.push(c);
        }
    }

    // Handle trailing backslash
    if last_was_backslash {
        result.push('\\');
    }

    result
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
