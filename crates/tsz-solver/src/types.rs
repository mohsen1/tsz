//! Type representation for the structural solver.
//!
//! Types are represented as lightweight `TypeId` handles that point into
//! an interning table. The actual structure is stored in `TypeData`.

use crate::def::DefId;
use serde::Serialize;
use tsz_binder::SymbolId;
use tsz_common::define_id;
use tsz_common::interner::Atom;

/// Hand-maintained `PartialEq`/`Eq`/`Hash` impls for the interned shape types
/// (`PropertyInfo`, `IndexSignature`, `ObjectShape`, `CallableShape`), with
/// per-field identity decisions made explicit via exhaustive destructuring.
mod relation_cache;
mod shape_identity;

pub use relation_cache::{
    CachedAnyMode, RelationCacheConfig, RelationCacheKey, RelationCacheKind, RelationCacheValue,
    RelationFlags,
};

define_id! {
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
pub struct TypeId; derive: Serialize, Default
}

impl TypeId {
    /// Internal placeholder - no valid type.
    pub const NONE: Self = Self(0);

    /// Error sentinel - type resolution failed.
    /// Propagates through operations to prevent cascading errors.
    /// See struct-level docs for detailed semantics.
    pub const ERROR: Self = Self(1);

    /// The bottom type - represents values that can never exist.
    /// Used for exhaustive checks and functions that never return.
    pub const NEVER: Self = Self(2);

    /// TypeScript's `unknown` type - type-safe top type.
    /// Requires type narrowing before use. See struct-level docs.
    pub const UNKNOWN: Self = Self(3);

    /// TypeScript's `any` type - opts out of type checking.
    /// All operations succeed, returning `any`. See struct-level docs.
    pub const ANY: Self = Self(4);

    /// The `void` type - used for functions with no meaningful return.
    pub const VOID: Self = Self(5);

    /// The `undefined` type - represents the undefined value.
    pub const UNDEFINED: Self = Self(6);

    /// The `null` type - represents the null value.
    pub const NULL: Self = Self(7);

    /// The `boolean` type - union of true | false.
    pub const BOOLEAN: Self = Self(8);

    /// The `number` type - all numeric values.
    pub const NUMBER: Self = Self(9);

    /// The `string` type - all string values.
    pub const STRING: Self = Self(10);

    /// The `bigint` type - arbitrary precision integers.
    pub const BIGINT: Self = Self(11);

    /// The `symbol` type - unique symbol values.
    pub const SYMBOL: Self = Self(12);

    /// The `object` type - any non-primitive value.
    pub const OBJECT: Self = Self(13);

    /// The literal type `true`.
    pub const BOOLEAN_TRUE: Self = Self(14);

    /// The literal type `false`.
    pub const BOOLEAN_FALSE: Self = Self(15);

    /// The `Function` type - any callable.
    pub const FUNCTION: Self = Self(16);

    /// Synthetic Promise base type for Promise<T> when Promise symbol is not resolved.
    /// Used to allow `promise_like_return_type_argument` to extract T from await expressions.
    pub const PROMISE_BASE: Self = Self(17);

    /// Internal sentinel used to represent 'any' in strict mode (North Star Fix).
    /// Behaves like 'any' but does NOT silence structural mismatches.
    pub const STRICT_ANY: Self = Self(19);

    /// First user-defined type ID (after built-in intrinsics)
    pub const FIRST_USER: u32 = 100;

    #[inline]
    pub const fn is_intrinsic(self) -> bool {
        self.0 < Self::FIRST_USER
    }

    #[inline]
    pub fn is_error(self) -> bool {
        self == Self::ERROR
    }

    #[inline]
    pub fn is_any(self) -> bool {
        self == Self::ANY
    }

    #[inline]
    pub fn is_unknown(self) -> bool {
        self == Self::UNKNOWN
    }

    #[inline]
    pub fn is_never(self) -> bool {
        self == Self::NEVER
    }

    /// Returns true if this type is nullish (null or undefined).
    /// Useful for strict null checking logic.
    #[inline]
    pub fn is_nullish(self) -> bool {
        self == Self::NULL || self == Self::UNDEFINED
    }

    /// Returns true if this type is nullable (null, undefined, or void).
    /// VOID is considered nullable because it represents undefined in some contexts.
    #[inline]
    pub fn is_nullable(self) -> bool {
        self == Self::NULL || self == Self::UNDEFINED || self == Self::VOID
    }

    /// Returns true if this type is a top type (any or unknown).
    /// Top types are assignable from all other types.
    #[inline]
    pub fn is_top_type(self) -> bool {
        self == Self::ANY || self == Self::UNKNOWN
    }

    /// Returns true if this type is any or unknown (types that accept anything).
    /// Alias for `is_top_type` for clarity in some contexts.
    #[inline]
    pub fn is_any_or_unknown(self) -> bool {
        self.is_top_type()
    }

    /// Returns true if this type is `UNKNOWN` or `ERROR` — the "unresolved"
    /// intrinsics that typically indicate a failed resolution or a propagated
    /// error, regardless of display.
    #[inline]
    pub const fn is_unknown_or_error(self) -> bool {
        matches!(self, Self::UNKNOWN | Self::ERROR)
    }

    /// Returns true if this type is `ANY`, `UNKNOWN`, or `ERROR` — the
    /// "information-less" intrinsics callers typically treat as wildcards
    /// when deciding whether a type carries useful structural information.
    #[inline]
    pub const fn is_any_unknown_or_error(self) -> bool {
        matches!(self, Self::ANY | Self::UNKNOWN | Self::ERROR)
    }

    // =========================================================================
    // Local/Global Partitioning (for ScopedTypeInterner GC)
    // =========================================================================

    /// Mask for the local bit (MSB of u32).
    ///
    /// Local IDs have MSB=1 (0x80000000+), Global IDs have MSB=0 (0x7FFFFFFF-).
    /// This partitioning allows `ScopedTypeInterner` to create ephemeral types
    /// that don't pollute the global `TypeId` space.
    pub const LOCAL_MASK: u32 = 0x80000000;

    /// Check if this `TypeId` is a local (ephemeral) type.
    ///
    /// Local types are created by `ScopedTypeInterner` and are only valid
    /// for the current operation/request. They are automatically freed
    /// when the `ScopedTypeInterner` is dropped.
    ///
    /// Returns `true` if MSB is set (0x80000000+).
    pub const fn is_local(self) -> bool {
        (self.0 & Self::LOCAL_MASK) != 0
    }

    /// Check if this `TypeId` is a global (persistent) type.
    ///
    /// Global types are managed by `TypeInterner` and persist for the lifetime
    /// of the project/server. These include declarations and intrinsics.
    ///
    /// Returns `true` if MSB is clear (0x7FFFFFFF-).
    pub const fn is_global(self) -> bool {
        !self.is_local()
    }
}

/// Priority levels for generic type inference constraints.
///
/// TypeScript uses a multi-pass inference algorithm where constraints are processed
/// in priority order. Higher priority constraints (like explicit type annotations) are
/// processed first, then lower priority constraints (like contextual types from return
/// position) are processed in subsequent passes.
///
/// This prevents circular dependencies and `any` leakage in complex generic scenarios
/// like `Array.prototype.map` or `Promise.then`.
///
/// ## Priority Order (Highest to Lowest)
///
/// 1. **`NakedTypeVariable`** - Direct type parameter with no constraints (highest)
/// 2. **`HomomorphicMappedType`** - Mapped types that preserve structure
/// 3. **`PartialHomomorphicMappedType`** - Partially homomorphic mapped types
/// 4. **`MappedType`** - Generic mapped types
/// 5. **`ContravariantConditional`** - Conditional types in contravariant position
/// 6. **`ReturnType`** - Contextual type from return position (low priority)
/// 7. **`LowPriority`** - Fallback inference (lowest)
/// 8. **Circular** - Detected circular dependency (prevents infinite loops)
///
/// ## Example
///
/// ```typescript
/// function map<U>(arr: T[], fn: (x: T) => U): U[];
/// // When calling map(x => x.toString()):
/// // 1. T is inferred from array element type (NakedTypeVariable)
/// // 2. U is inferred from return type contextual type (ReturnType)
/// // Processing T first prevents circular T <-> U dependency
/// ```
///
/// Part of the Priority-Based Contextual Inference implementation.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum InferencePriority {
    /// Naked type variable with no constraints (highest priority).
    /// Example: `<T>` where T appears directly in parameter types.
    NakedTypeVariable = 1 << 0,

    /// Mapped type that preserves array/tuple structure.
    /// Example: `Partial<T[]>` preserves array structure.
    HomomorphicMappedType = 1 << 1,

    /// Partially homomorphic mapped type.
    /// Example: Mapped types with some mixed properties.
    PartialHomomorphicMappedType = 1 << 2,

    /// Generic mapped type.
    /// Example: `{ [K in keyof T]: U }`
    MappedType = 1 << 3,

    /// Conditional type in contravariant position.
    /// Example: Inference from function parameter types in conditional types.
    ContravariantConditional = 1 << 4,

    /// Contextual type from return position.
    /// Example: `const x: number = fn()` where fn is generic.
    ReturnType = 1 << 5,

    /// Low priority fallback inference.
    LowPriority = 1 << 6,

    /// Reverse inference from a string/number literal to `keyof T`.
    /// Example: `f<T>(x: keyof T)` called with `f("a")` synthesises
    /// `{ a: any }` as a contra-candidate for T at this priority.
    ///
    /// Strictly worse than every "real" inference site so that a naked
    /// `obj: T` argument always wins over a synthetic key-derived shape.
    /// Mirrors tsc's `InferencePriority.LiteralKeyof` (1 << 8 in tsc).
    LiteralKeyof = 1 << 7,

    /// Detected circular dependency (prevents infinite loops).
    /// Set when a type parameter depends on itself through constraints.
    Circular = 1 << 8,
}

impl InferencePriority {
    /// Check if this priority level should be processed in a given pass.
    ///
    /// Multi-pass inference processes constraints in increasing priority order.
    /// Returns true if this priority matches or is lower than the current pass level.
    pub fn should_process_in_pass(&self, current_pass: Self) -> bool {
        *self >= current_pass && *self != Self::Circular
    }

    /// Get the next priority level for multi-pass inference.
    pub const fn next_level(&self) -> Option<Self> {
        match self {
            Self::NakedTypeVariable => Some(Self::HomomorphicMappedType),
            Self::HomomorphicMappedType => Some(Self::PartialHomomorphicMappedType),
            Self::PartialHomomorphicMappedType => Some(Self::MappedType),
            Self::MappedType => Some(Self::ContravariantConditional),
            Self::ContravariantConditional => Some(Self::ReturnType),
            Self::ReturnType => Some(Self::LowPriority),
            Self::LowPriority => Some(Self::LiteralKeyof),
            Self::LiteralKeyof | Self::Circular => None,
        }
    }

    /// Default priority for normal constraint collection.
    pub const NORMAL: Self = Self::ReturnType;

    /// Highest priority for explicit type annotations.
    pub const HIGHEST: Self = Self::NakedTypeVariable;

    /// Lowest priority for fallback inference.
    pub const LOWEST: Self = Self::LowPriority;
}

define_id!(/// Interned list of `TypeId` values (e.g., unions/intersections).
    pub struct TypeListId);

define_id!(/// Interned object shape (properties + index signatures).
    pub struct ObjectShapeId);

define_id!(/// Interned tuple element list.
    pub struct TupleListId);

define_id!(/// Interned function shape.
    pub struct FunctionShapeId);

define_id!(/// Interned callable shape.
    pub struct CallableShapeId);

define_id!(/// Interned type application (Base<Args>).
    pub struct TypeApplicationId);

define_id!(/// Interned template literal span list.
    pub struct TemplateLiteralId);

define_id!(/// Interned conditional type.
    pub struct ConditionalTypeId);

define_id!(/// Interned mapped type.
    pub struct MappedTypeId);
/// The structural "shape" of a type.
/// This is the key used for interning - structurally identical types
/// will have the same `TypeData` and therefore the same `TypeId`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TypeData {
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

    /// Bound type parameter using De Bruijn index for alpha-equivalence.
    ///
    /// Represents a type parameter relative to the current binding scope.
    /// Used by the Canonicalizer to achieve alpha-equivalence, where
    /// `type F<T> = T` and `type G<U> = U` are considered identical.
    ///
    /// ## Alpha-Equivalence (Task #32)
    ///
    /// When canonicalizing generic types, we replace named type parameters
    /// with positional indices to achieve structural identity.
    ///
    /// ### Example
    ///
    /// ```typescript
    /// type F<T> = { value: T };  // canonicalizes to Object({ value: BoundParameter(0) })
    /// type G<U> = { value: U };  // also canonicalizes to Object({ value: BoundParameter(0) })
    /// // Both get the same TypeId because they're structurally identical
    /// ```
    ///
    /// ## De Bruijn Index Semantics
    ///
    /// - `BoundParameter(0)` = the most recently bound type parameter
    /// - `BoundParameter(1)` = the second-most recently bound type parameter
    /// - `BoundParameter(n)` = the (n+1)th-most recently bound type parameter
    BoundParameter(u32),

    /// Reference to a named type (interface, class, type alias)
    /// Uses `SymbolId` to break infinite recursion
    /// DEPRECATED: Use `Lazy(DefId)` for new code. This is kept for backward compatibility
    /// during the migration from `SymbolRef` to `DefId`.
    /// PHASE 4.2: REMOVED - Migration complete, all types now use Lazy(DefId)
    // Ref(SymbolRef),

    /// Lazy reference to a type definition.
    ///
    /// Unlike `Ref(SymbolRef)` which references Binder symbols, `Lazy(DefId)` uses
    /// Solver-owned identifiers that:
    /// - Don't require Binder context
    /// - Support content-addressed hashing for LSP stability
    /// - Enable Salsa integration for incremental compilation
    ///
    /// The type is evaluated lazily when first accessed, resolving to the actual
    /// type stored in the `DefinitionStore`.
    ///
    /// ## Migration
    ///
    /// Eventually all `Ref(SymbolRef)` usages will be replaced with `Lazy(DefId)`.
    Lazy(DefId),

    /// Recursive type reference using De Bruijn index.
    ///
    /// Represents a back-reference to a type N levels up the nesting path.
    /// This is used for canonicalizing recursive types to achieve O(1) equality.
    ///
    /// ## Graph Isomorphism (Task #32)
    ///
    /// When canonicalizing recursive type aliases, we replace cycles with
    /// relative De Bruijn indices instead of absolute Lazy references.
    ///
    /// ### Example
    ///
    /// ```typescript
    /// type A = { x: A };  // canonicalizes to Object({ x: Recursive(0) })
    /// type B = { x: B };  // also canonicalizes to Object({ x: Recursive(0) })
    /// // Both get the same TypeId because they're structurally identical
    /// ```
    ///
    /// ## De Bruijn Index Semantics
    ///
    /// - `Recursive(0)` = the current type itself (immediate recursion)
    /// - `Recursive(1)` = one level up (parent in the nesting chain)
    /// - `Recursive(n)` = n levels up
    ///
    /// ## Nominal vs Structural
    ///
    /// This is ONLY used for structural types (type aliases). Nominal types
    /// (classes, interfaces) preserve their Lazy(DefId) for nominal identity.
    Recursive(u32),

    /// Enum type with nominal identity and structural member types.
    ///
    /// Enums are nominal types - two different enums with the same member types
    /// are NOT compatible (e.g., `enum E1 { A, B }` is not assignable to `enum E2 { A, B }`).
    ///
    /// - `DefId`: The unique identity of the enum (for E1 vs E2 nominal checking)
    /// - `TypeId`: The structural union of member types (e.g., 0 | 1 for numeric enums),
    ///   used for assignability to primitives (e.g., E1 assignable to number)
    Enum(DefId, TypeId),

    /// Generic type application (Base<Args>)
    Application(TypeApplicationId),

    /// Conditional type (T extends U ? X : Y)
    Conditional(ConditionalTypeId),

    /// Mapped type ({ [K in Keys]: `ValueType` })
    Mapped(MappedTypeId),

    /// Index access type (T[K])
    IndexAccess(TypeId, TypeId),

    /// Template literal type (`hello${string}world`)
    TemplateLiteral(TemplateLiteralId),

    /// Type query (typeof expression in type position)
    TypeQuery(SymbolRef),

    /// `KeyOf` type operator (keyof T)
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

    /// Module namespace type (import * as ns from "module")
    /// Uses `SymbolRef` for lazy evaluation to avoid circular dependency issues
    ModuleNamespace(SymbolRef),

    /// `NoInfer`<T> utility type (TypeScript 5.4+)
    /// Prevents inference from flowing through this type position.
    /// During inference, this blocks inference. During evaluation/subtyping,
    /// it evaluates to the inner type (transparent).
    NoInfer(TypeId),

    /// Substitution type: a type variable narrowed by an implied constraint.
    ///
    /// Mirrors `tsc`'s `SubstitutionType`. `base_type` is the underlying type
    /// variable (the surface identity used by printing and inference);
    /// `constraint` is the implied constraint from an enclosing conditional's
    /// `extends` type. The substitution intersection `base_type & constraint` is
    /// what relations and base-constraint computation observe. See
    /// `getConditionalFlowTypeOfType` / `getSubstitutionType` in `tsc`.
    Substitution {
        base_type: TypeId,
        constraint: TypeId,
    },

    /// Error type for recovery
    Error,

    /// Error-like unresolved type name with diagnostic display provenance.
    ///
    /// TypeScript keeps the source spelling of unresolved annotation types in
    /// downstream messages (for example `fs.Stats`) even though the semantic
    /// type is an error. This leaf preserves that display surface without
    /// collapsing every occurrence into the global `TypeId::ERROR` sentinel.
    UnresolvedTypeName(Atom),
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
    pub const fn to_type_id(self) -> TypeId {
        match self {
            Self::Any => TypeId::ANY,
            Self::Unknown => TypeId::UNKNOWN,
            Self::Never => TypeId::NEVER,
            Self::Void => TypeId::VOID,
            Self::Null => TypeId::NULL,
            Self::Undefined => TypeId::UNDEFINED,
            Self::Boolean => TypeId::BOOLEAN,
            Self::Number => TypeId::NUMBER,
            Self::String => TypeId::STRING,
            Self::Bigint => TypeId::BIGINT,
            Self::Symbol => TypeId::SYMBOL,
            Self::Object => TypeId::OBJECT,
            Self::Function => TypeId::FUNCTION,
        }
    }
}

/// Literal values (for literal types)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LiteralValue {
    String(Atom),
    Number(OrderedFloat),
    BigInt(Atom),
    Boolean(bool),
}

impl LiteralValue {
    /// Returns the primitive `TypeId` that this literal widens to.
    ///
    /// - `String(_)` → `TypeId::STRING`
    /// - `Number(_)` → `TypeId::NUMBER`
    /// - `Boolean(_)` → `TypeId::BOOLEAN`
    /// - `BigInt(_)` → `TypeId::BIGINT`
    pub const fn primitive_type_id(&self) -> TypeId {
        match self {
            Self::String(_) => TypeId::STRING,
            Self::Number(_) => TypeId::NUMBER,
            Self::Boolean(_) => TypeId::BOOLEAN,
            Self::BigInt(_) => TypeId::BIGINT,
        }
    }
}

/// Wrapper for f64 that implements Eq and Hash for use in `TypeData`.
///
/// Identity is `SameValueZero`, matching how tsc keys number literal types
/// (a JS `Map`): `-0` and `+0` are one value and every NaN bit pattern is one
/// value. `literal_number` also normalizes the stored representative, but the
/// `Eq`/`Hash` impls make the collapse unbypassable for direct constructions.
#[derive(Clone, Copy, Debug)]
pub struct OrderedFloat(pub f64);

impl OrderedFloat {
    /// The `SameValueZero` representative of a value: `-0` folds into `+0`
    /// and every NaN bit pattern collapses to the canonical NaN. Single owner
    /// of the keying rule, shared by `Eq`/`Hash` and `literal_number`'s
    /// stored-representative normalization.
    pub(crate) fn same_value_zero_canonical(value: f64) -> f64 {
        if value == 0.0 {
            0.0
        } else if value.is_nan() {
            f64::NAN
        } else {
            value
        }
    }

    fn same_value_zero_bits(self) -> u64 {
        Self::same_value_zero_canonical(self.0).to_bits()
    }
}

impl PartialEq for OrderedFloat {
    fn eq(&self, other: &Self) -> bool {
        self.same_value_zero_bits() == other.same_value_zero_bits()
    }
}

impl Eq for OrderedFloat {}

impl std::hash::Hash for OrderedFloat {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.same_value_zero_bits().hash(state);
    }
}

// Visibility is defined in tsz-common for cross-crate sharing (parser, solver, checker, lowering)
pub use tsz_common::Visibility;

/// Property information for object types
#[derive(Clone, Debug, Default)]
pub struct PropertyInfo {
    pub name: Atom,
    /// Read type (getter/lookup).
    pub type_id: TypeId,
    /// Write type (setter/assignment).
    pub write_type: TypeId,
    pub optional: bool,
    pub readonly: bool,
    pub is_method: bool,
    /// Whether this property is a class prototype member (method or accessor).
    /// Prototype properties are excluded from spread types (`{ ...classInstance }`).
    /// Excluded from PartialEq/Hash since it's declaration-site metadata, not structural.
    pub is_class_prototype: bool,
    /// Visibility modifier for nominal subtyping
    pub visibility: Visibility,
    /// Symbol that declared this property (for nominal identity checks)
    pub parent_id: Option<SymbolId>,
    /// Declaration order for preserving source ordering in emit (excluded from
    /// equality/hash). 0 means "unset": interner constructors backfill it from
    /// insertion order.
    pub declaration_order: u32,
    /// Whether this property was declared with a string key that looks numeric
    /// (e.g. `"404"` vs `404`). Included in PartialEq/Hash because `"100"` and
    /// `100` are semantically different property keys in TypeScript.
    pub is_string_named: bool,
    /// Whether this property key came from a symbol-valued computed name.
    ///
    /// Unique-symbol keys are encoded as `__unique_N` atoms internally, but a
    /// user-authored string property can also have that exact text. This flag
    /// keeps the two cases distinct.
    pub is_symbol_named: bool,
    /// Whether the string property name was written with single quotes in source.
    /// Declaration emit preserves this for reconstructed mapped properties.
    /// Excluded from PartialEq/Hash since it's purely cosmetic (controls quote
    /// style in `.d.ts` output) — `{ 'foo': string }` and `{ "foo": string }`
    /// are semantically identical in TypeScript and must intern to the same
    /// `TypeId`.
    pub single_quoted_name: bool,
    /// Whether this property's literal type is a *non-widening* (regular)
    /// literal that must be preserved through later widening passes.
    ///
    /// This mirrors tsc's fresh-vs-regular literal-type duality at the property
    /// level. tsc widens a fresh object literal's properties via
    /// `getWidenedTypeOfObjectLiteral`, but each property only widens when its
    /// own type carries `ContainsWideningType` (a *fresh* literal). A property
    /// whose value came from a non-widening source — an `as const` assertion, a
    /// plain `as T` / `<T>expr` type assertion, an identifier with a
    /// non-widening declared type, or a literal index access — holds a *regular*
    /// literal that `getWidenedType` leaves untouched.
    ///
    /// tsz has no fresh/regular split on the literal type itself (one interned
    /// `true`), so the checker records the decision here when it preserves such
    /// a property at object-literal construction. The solver's widening passes
    /// honour it so an inferred `{ single: true }` from
    /// `{ single: true as const }` is not re-widened to `{ single: boolean }`.
    ///
    /// Identity-bearing: a preserved-because-`as const` `{ a: 1 }` and a
    /// preserved-because-deferred `{ a: 1 }` (a plain literal awaiting widening
    /// under an object-typed contextual parameter) are otherwise structurally
    /// identical and would intern to the same `TypeId`; the first must not widen
    /// while the second must. See `crates/tsz-solver/src/types/shape_identity.rs`.
    pub non_widening: bool,
}

impl PropertyInfo {
    /// Create a property with default settings (non-optional, non-readonly, public).
    /// Sets `write_type` equal to `type_id`.
    pub const fn new(name: Atom, type_id: TypeId) -> Self {
        Self {
            name,
            type_id,
            write_type: type_id,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
            non_widening: false,
        }
    }

    /// Create a method property with default settings.
    pub const fn method(name: Atom, type_id: TypeId) -> Self {
        Self {
            is_method: true,
            ..Self::new(name, type_id)
        }
    }

    /// Create an optional property with default settings.
    pub const fn opt(name: Atom, type_id: TypeId) -> Self {
        Self {
            optional: true,
            ..Self::new(name, type_id)
        }
    }

    /// Create a readonly property with default settings.
    pub const fn readonly(name: Atom, type_id: TypeId) -> Self {
        Self {
            readonly: true,
            ..Self::new(name, type_id)
        }
    }

    /// Returns true when getter and setter have distinct types (TypeScript 4.3+
    /// split accessor). A non-NONE `write_type` that differs from the read type
    /// is the canonical encoding: `write_type == NONE` means readonly (lowering
    /// pass), and `write_type == type_id` means a uniform read/write property.
    pub fn has_split_accessor(&self) -> bool {
        self.write_type != TypeId::NONE && self.write_type != self.type_id
    }

    /// Find a property by name in a slice of properties.
    pub fn find_in_slice(props: &[Self], name: Atom) -> Option<&Self> {
        props.iter().find(|p| p.name == name)
    }
}

/// Normalize display-only properties for diagnostic storage.
///
/// Unlike canonical object properties, display properties should preserve
/// source-facing member order rather than name order. Explicit declaration
/// orders come first; any unordered trailing members keep their existing
/// sequence after the ordered segment. The final dense renumbering gives later
/// display-time merges a stable ordering signal.
pub fn normalize_display_property_order(props: &mut [PropertyInfo]) {
    props.sort_by(
        |a, b| match (a.declaration_order > 0, b.declaration_order > 0) {
            (true, true) => a.declaration_order.cmp(&b.declaration_order),
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            (false, false) => std::cmp::Ordering::Equal,
        },
    );

    for (idx, prop) in props.iter_mut().enumerate() {
        prop.declaration_order = idx as u32 + 1;
    }
}

/// Merge display-only properties from multiple intersection members while
/// preserving first-seen source order.
pub fn merge_display_properties_for_intersection(
    db: &dyn crate::construction::TypeDatabase,
    members: &[TypeId],
) -> Vec<PropertyInfo> {
    let mut merged: Vec<PropertyInfo> = Vec::new();
    let mut prop_indices: rustc_hash::FxHashMap<Atom, usize> = rustc_hash::FxHashMap::default();

    for &member in members {
        let Some(props) = db.get_display_properties(member) else {
            continue;
        };

        for prop in props.as_ref() {
            if let Some(&idx) = prop_indices.get(&prop.name) {
                let existing: &mut PropertyInfo = &mut merged[idx];
                if existing.type_id != prop.type_id {
                    existing.type_id = db.intersect_types_raw2(existing.type_id, prop.type_id);
                }
                if existing.write_type != prop.write_type {
                    existing.write_type =
                        db.intersect_types_raw2(existing.write_type, prop.write_type);
                }
            } else {
                prop_indices.insert(prop.name, merged.len());
                merged.push(prop.clone());
            }
        }
    }

    for (idx, prop) in merged.iter_mut().enumerate() {
        prop.declaration_order = idx as u32 + 1;
    }
    merged
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PropertyLookup {
    Found(usize),
    NotFound,
    Uncached,
}

/// Index signature information for object types
/// Represents `{ [key: string]: ValueType }` or `{ [key: number]: ValueType }`
#[derive(Clone, Copy, Debug)]
pub struct IndexSignature {
    /// The key type (usually string or number)
    pub key_type: TypeId,
    /// The value type for all indexed properties
    pub value_type: TypeId,
    /// Whether the index signature is readonly
    pub readonly: bool,
    /// Original parameter name from source (cosmetic, excluded from equality/hash).
    /// E.g., for `[x: string]: T`, this is `Some(atom("x"))`.
    pub param_name: Option<Atom>,
}

/// Combined index signature information for a type.
/// Provides convenient access to string, number, and symbol index signatures.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct IndexInfo {
    /// String index signature: { [key: string]: T }
    pub string_index: Option<IndexSignature>,
    /// Number index signature: { [key: number]: T }
    pub number_index: Option<IndexSignature>,
    /// Symbol index signature: { [key: symbol]: T }
    pub symbol_index: Option<IndexSignature>,
}

bitflags::bitflags! {
    #[derive(Default, Clone, Copy, PartialEq, Eq, Hash, Debug)]
    pub struct ObjectFlags: u32 {
        const FRESH_LITERAL = 1 << 0;
        /// The type has members with computed property names whose key types
        /// could not be resolved to a single string/number literal (e.g.,
        /// union keys like `"a" | "b"`, template literals like `` `prefix-${string}` ``).
        /// tsc treats such types as implicitly string-indexable for TS7053 purposes.
        const HAS_LATE_BOUND_MEMBERS = 1 << 1;
        /// This object represents a const enum namespace.
        /// Const enums have no runtime object, so Object.prototype members
        /// (constructor, hasOwnProperty, etc.) must not be accessible.
        const CONST_ENUM = 1 << 2;
        /// This object represents an enum namespace type (`typeof E`).
        /// Enum namespaces get implicit index signatures for inference and
        /// subtype checking, unlike regular named interfaces/classes.
        const ENUM_NAMESPACE = 1 << 3;
        /// This object came from the declaration-only UMD global surface of an
        /// exported class. It must not later pick up module augmentation members
        /// through name-based fallback lookup.
        const NO_MODULE_AUGMENTATION_LOOKUP = 1 << 4;
        /// Synthetic `Record<K, unknown>` produced by `K in value` flow narrowing.
        /// This is semantically structural, but diagnostics should keep tsc's
        /// `Record<..., unknown>` display surface.
        const IN_OPERATOR_RECORD = 1 << 5;
        /// Fresh object literal whose own properties all require contextual typing.
        /// Generic-call inference must defer these literals to Round 2; otherwise
        /// already-contextualized callback properties can feed back into Round 1.
        const ALL_PROPERTIES_CONTEXT_SENSITIVE = 1 << 6;
        /// Anonymous object whose declaration order is semantically irrelevant
        /// for assignability, but must remain distinct in the interner so
        /// diagnostics can print source/display order after widening.
        const PRESERVE_DECLARATION_ORDER = 1 << 7;
        /// This object is the materialized form of a *non-homomorphic* mapped
        /// type (`{ [P in C]: V }`, `Record<C, V>`), whose index signatures are
        /// derived from the mapped *constraint* `C`. `keyof` of such an object
        /// is the constraint key space, so a string-keyed slot contributes
        /// exactly `string` — never the implicit `number` that a genuine
        /// `[k: string]` index signature carries. Homomorphic mapped types
        /// (`{ [K in keyof T]: V }`) are NOT flagged: their `keyof` is `keyof T`
        /// and must keep the source's full key space.
        const MAPPED_CONSTRAINT_KEYS = 1 << 8;

        /// The object was synthesized by merging the object members of an
        /// object-only intersection (`{ a } & { b }` -> `{ a; b }`) during
        /// intersection normalization. tsc never merges `A & B` into one
        /// object; tsz does so for O(1) member lookup, recording a display alias
        /// back to the original `Intersection` so the `&` form can still be
        /// printed. Because a merged object interns to the *same* shape as a
        /// plain object literal of that shape, the display alias alone cannot
        /// tell the two apart. This flag makes the merged object a distinct
        /// `TypeId`, so diagnostics can reliably recover that a target really is
        /// an intersection (`TS2322` elaboration) without misfiring on a plain
        /// object of the same shape. It is identity-only: structural subtyping
        /// ignores it (a merged `A & B` and a plain `{ a; b }` still relate).
        const INTERSECTION_MERGED = 1 << 9;

        /// The `string_index` signature is *optional* (`[k: string]?: V`), as
        /// produced by an optional mapped type such as `Partial<Record<string, V>>`.
        /// tsc has no `optional` slot on an `IndexInfo`; the `?` modifier instead
        /// makes the index signature impose no requirement on a property-less
        /// source, so an `object`-typed value (or an `object`-constrained generic)
        /// is assignable to it even though it is rejected by a *required* index
        /// signature. The value type already carries `| undefined` (added when the
        /// mapped type is evaluated); this flag records the relaxation needed by
        /// the assignability relation and keeps the optional form a distinct
        /// interned shape from the required `Record<string, V>`.
        const STRING_INDEX_OPTIONAL = 1 << 10;

        /// The `number_index` signature is *optional* (`[k: number]?: V`). The
        /// numeric mirror of [`Self::STRING_INDEX_OPTIONAL`].
        const NUMBER_INDEX_OPTIONAL = 1 << 11;

        /// The `symbol_index` signature is *optional* (`[k: symbol]?: V`, e.g.
        /// from `Partial<Record<symbol, V>>`). The symbol mirror of
        /// [`Self::STRING_INDEX_OPTIONAL`].
        const SYMBOL_INDEX_OPTIONAL = 1 << 12;

        /// This object is the materialized `typeof globalThis` surface: a
        /// concrete object whose properties are the in-scope value globals.
        /// Structurally it behaves like its property set (relations ignore this
        /// flag), but diagnostics must print it as `typeof globalThis` rather
        /// than expanding the hundreds of global members. The flag makes the
        /// surface a distinct interned `TypeId` so the formatter can recover the
        /// `typeof globalThis` display even when the surface appears as a member
        /// of an intersection such as `Window & typeof globalThis`.
        const GLOBAL_THIS_SURFACE = 1 << 13;
    }
}

/// Object type with properties and optional index signatures
///
/// NOTE: The `symbol` field affects BOTH Hash and `PartialEq` for nominal discrimination.
/// This ensures that different classes get different `TypeIds` in the interner.
/// Structural subtyping is computed explicitly in the Solver, not via `PartialEq`.
#[derive(Clone, Debug, Default)]
pub struct ObjectShape {
    /// Object-level flags (e.g. fresh literal tracking).
    pub flags: ObjectFlags,
    /// Named properties (sorted by name for consistent hashing)
    pub properties: Vec<PropertyInfo>,
    /// String index signature: { [key: string]: T }
    pub string_index: Option<IndexSignature>,
    /// Number index signature: { [key: number]: T }
    pub number_index: Option<IndexSignature>,
    /// Symbol index signature: { [key: symbol]: T }.
    ///
    /// A type can carry a `symbol` index signature *simultaneously* with a
    /// `string` (and/or `number`) index — e.g. `Record<PropertyKey, V>` /
    /// `{ [K in string | number | symbol]: V }`. The bare `symbol` intrinsic key
    /// therefore needs its own slot; it cannot share `string_index` (whose
    /// `key_type` historically doubled as a string/symbol discriminator) without
    /// the two colliding. The contained [`IndexSignature::key_type`] is always
    /// [`TypeId::SYMBOL`].
    pub symbol_index: Option<IndexSignature>,
    /// Nominal identity for class instance types (prevents structural interning of distinct classes)
    pub symbol: Option<tsz_binder::SymbolId>,
}

impl ObjectShape {
    /// Mark this shape as a fresh object literal.
    ///
    /// Fresh literals are subject to excess property checking. Use this
    /// instead of importing `ObjectFlags::FRESH_LITERAL` directly.
    pub fn mark_fresh_literal(&mut self) {
        self.flags |= ObjectFlags::FRESH_LITERAL;
    }

    /// Return true if this shape is a fresh object literal.
    pub const fn is_fresh_literal(&self) -> bool {
        self.flags.contains(ObjectFlags::FRESH_LITERAL)
    }

    /// Mark this fresh literal as requiring full contextual Round 2 inference.
    ///
    /// Use this instead of importing `ObjectFlags::ALL_PROPERTIES_CONTEXT_SENSITIVE`
    /// directly outside the solver.
    pub fn mark_all_properties_context_sensitive(&mut self) {
        self.flags |= ObjectFlags::ALL_PROPERTIES_CONTEXT_SENSITIVE;
    }

    /// Return true if every own property of this fresh literal needed contextual typing.
    pub const fn all_properties_context_sensitive(&self) -> bool {
        self.flags
            .contains(ObjectFlags::ALL_PROPERTIES_CONTEXT_SENSITIVE)
    }

    /// Mark this anonymous object as requiring display-order-sensitive interning.
    pub fn mark_preserve_declaration_order(&mut self) {
        self.flags |= ObjectFlags::PRESERVE_DECLARATION_ORDER;
    }

    /// Mark this shape as having late-bound (computed) members.
    ///
    /// Use this instead of importing `ObjectFlags::HAS_LATE_BOUND_MEMBERS` directly.
    pub fn mark_has_late_bound_members(&mut self) {
        self.flags |= ObjectFlags::HAS_LATE_BOUND_MEMBERS;
    }

    /// Mark this class-instance shape as opting out of module-augmentation lookup.
    ///
    /// Use this instead of importing `ObjectFlags::NO_MODULE_AUGMENTATION_LOOKUP`
    /// directly outside the solver.
    pub fn mark_no_module_augmentation_lookup(&mut self) {
        self.flags |= ObjectFlags::NO_MODULE_AUGMENTATION_LOOKUP;
    }

    /// Return true if this shape's `string_index` signature is optional
    /// (`[k: string]?: V`, e.g. from `Partial<Record<string, V>>`).
    pub const fn string_index_is_optional(&self) -> bool {
        self.flags.contains(ObjectFlags::STRING_INDEX_OPTIONAL)
    }

    /// Return true if this shape's `number_index` signature is optional
    /// (`[k: number]?: V`, e.g. from `Partial<Record<number, V>>`).
    pub const fn number_index_is_optional(&self) -> bool {
        self.flags.contains(ObjectFlags::NUMBER_INDEX_OPTIONAL)
    }

    /// Return true if this shape's `symbol_index` signature is optional
    /// (`[k: symbol]?: V`, e.g. from `Partial<Record<symbol, V>>`).
    pub const fn symbol_index_is_optional(&self) -> bool {
        self.flags.contains(ObjectFlags::SYMBOL_INDEX_OPTIONAL)
    }

    /// The `string` index signature (`[k: string]: V`), if any.
    ///
    /// Defensively excludes a `symbol`-keyed signature: the symbol index lives
    /// in its own [`Self::symbol_index`] slot, but historically a symbol index
    /// was encoded in `string_index` with `key_type == SYMBOL`, so this filter
    /// keeps callers correct even if a legacy path still routes one here.
    pub fn string_index_signature(&self) -> Option<&IndexSignature> {
        self.string_index
            .as_ref()
            .filter(|idx| idx.key_type != TypeId::SYMBOL)
    }

    /// The `symbol` index signature (`[k: symbol]: V`), if any.
    pub fn symbol_index_signature(&self) -> Option<&IndexSignature> {
        self.symbol_index.as_ref().or_else(|| {
            self.string_index
                .as_ref()
                .filter(|idx| idx.key_type == TypeId::SYMBOL)
        })
    }
}

/// Tuple element information
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TupleElement {
    pub type_id: TypeId,
    pub name: Option<Atom>,
    pub optional: bool,
    pub rest: bool,
}

impl TupleElement {
    /// Returns `true` if this element is required (non-optional, non-rest).
    pub const fn is_required(&self) -> bool {
        !self.optional && !self.rest
    }

    /// Create an anonymous, non-optional, non-rest element — the most common
    /// shape when constructing proxy or synthetic tuples.
    pub const fn fixed(type_id: TypeId) -> Self {
        Self {
            type_id,
            name: None,
            optional: false,
            rest: false,
        }
    }

    /// Create an anonymous rest element (`...type_id`).
    pub const fn rest(type_id: TypeId) -> Self {
        Self {
            type_id,
            name: None,
            optional: false,
            rest: true,
        }
    }
}

/// Type predicate information (x is T / asserts x is T).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TypePredicate {
    pub asserts: bool,
    pub target: TypePredicateTarget,
    pub type_id: Option<TypeId>,
    pub parameter_index: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

impl FunctionShape {
    /// Create a simple function shape with params and return type.
    /// No type params, no this, no predicate, not a constructor or method.
    pub const fn new(params: Vec<ParamInfo>, return_type: TypeId) -> Self {
        Self {
            type_params: Vec::new(),
            params,
            this_type: None,
            return_type,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        }
    }

    /// Return a copy of this shape with the params replaced.
    /// Preserves `type_params`, `this_type`, `return_type`, `type_predicate`,
    /// `is_constructor`, and `is_method` from the original.
    pub fn with_replaced_params(&self, params: Vec<ParamInfo>) -> Self {
        Self {
            type_params: self.type_params.clone(),
            params,
            this_type: self.this_type,
            return_type: self.return_type,
            type_predicate: self.type_predicate,
            is_constructor: self.is_constructor,
            is_method: self.is_method,
        }
    }
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

impl CallSignature {
    /// Create a simple call signature with params and return type.
    pub const fn new(params: Vec<ParamInfo>, return_type: TypeId) -> Self {
        Self {
            type_params: Vec::new(),
            params,
            this_type: None,
            return_type,
            type_predicate: None,
            is_method: false,
        }
    }
}

/// Callable type with multiple overloaded call signatures
/// Represents types like:
/// ```typescript
/// interface Overloaded {
///   (x: string): number;
///   (x: number): string;
/// }
/// ```
/// NOTE: The `symbol` field affects BOTH Hash and `PartialEq` for nominal discrimination.
/// This ensures that different classes get different `TypeIds` in the interner.
/// Structural subtyping is computed explicitly in the Solver, not via `PartialEq`.
#[derive(Clone, Debug, Default)]
pub struct CallableShape {
    /// Call signatures (order matters for overload resolution)
    pub call_signatures: Vec<CallSignature>,
    /// Constructor signatures
    pub construct_signatures: Vec<CallSignature>,
    /// Optional properties on the callable (e.g., Function.prototype)
    pub properties: Vec<PropertyInfo>,
    /// String index signature (for static index signatures on classes).
    ///
    /// Unlike [`ObjectShape`], a `CallableShape` keeps the single-slot index
    /// convention: a `symbol` index signature rides here too, discriminated by
    /// `key_type == SYMBOL`. A static `string` + `symbol` index collision on a
    /// callable is vanishingly rare, so no dedicated `symbol_index` slot is
    /// carried; producers merge symbol into this slot via `string_index.or(..)`.
    pub string_index: Option<IndexSignature>,
    /// Number index signature (for static index signatures on classes)
    pub number_index: Option<IndexSignature>,
    /// Nominal identity for class constructors (prevents structural interning of distinct classes)
    pub symbol: Option<tsz_binder::SymbolId>,
    /// Whether this callable represents an abstract constructor type.
    /// Set for both abstract class constructors and `abstract new (...)` construct signature types.
    pub is_abstract: bool,
}

impl CallableShape {
    /// Return the last signature for a construct or call pattern, or `None` if absent.
    ///
    /// Callables like `DateConstructor` carry both kinds; callers select by the
    /// pattern's kind rather than assuming only one signature type is present.
    #[inline]
    pub fn last_sig_for(&self, is_construct: bool) -> Option<&CallSignature> {
        if is_construct {
            self.construct_signatures.last()
        } else {
            self.call_signatures.last()
        }
    }
}

/// Parameter information
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ParamInfo {
    pub name: Option<Atom>,
    pub type_id: TypeId,
    pub optional: bool,
    pub rest: bool,
    /// Optional *only* for JS call-arity leniency; renders/emits as required
    /// (see `is_display_optional`) to match `tsc`. Never set for genuine `?`.
    pub arity_only_optional: bool,
}

impl ParamInfo {
    /// Returns `true` if this parameter is required (non-optional, non-rest).
    pub const fn is_required(&self) -> bool {
        !self.optional && !self.rest
    }

    /// Create a required parameter.
    pub const fn required(name: Atom, type_id: TypeId) -> Self {
        Self {
            name: Some(name),
            type_id,
            optional: false,
            rest: false,
            arity_only_optional: false,
        }
    }

    /// Create an optional parameter.
    pub const fn optional(name: Atom, type_id: TypeId) -> Self {
        Self {
            optional: true,
            ..Self::required(name, type_id)
        }
    }

    /// Create a rest parameter.
    pub const fn rest(name: Atom, type_id: TypeId) -> Self {
        Self {
            rest: true,
            ..Self::required(name, type_id)
        }
    }

    /// Create an unnamed required parameter.
    pub const fn unnamed(type_id: TypeId) -> Self {
        Self {
            name: None,
            type_id,
            optional: false,
            rest: false,
            arity_only_optional: false,
        }
    }
}

/// Origin/kind discriminant for a [`TypeParamInfo`].
///
/// Inference machinery synthesizes type parameters as placeholders during
/// generic-call inference. Historically the placeholder *kind* was encoded into
/// the `name` atom text (`__infer_{id}`, `__infer_src_{id}#{origin}`) and
/// recovered by string scans (`starts_with("__infer_")`,
/// `decode_src_placeholder_origin`). That put atom resolution and string parsing
/// on memoized walk hot paths and risked colliding with a user type parameter
/// literally named `__infer_0` (legal TS). This field makes the classification
/// an O(1) structured read; the `name` atom is display-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum TypeParamOrigin {
    /// A type parameter written in user source (or any non-inference synthetic).
    #[default]
    User,
    /// #14344 STEP-B: a user-written type parameter stamped with its declaration
    /// site `(file, name_node)`, so two distinct declarations sharing an
    /// identical surface `TypeParamInfo` intern to DISTINCT ids (the bare info
    /// now differs per-decl) — fixing the self-ref-guard over-collapse. Stamped
    /// at the dominant lowering construction path (`collect_type_parameters`)
    /// under `TSZ_TYPEPARAM_DECL_IDENTITY`; flag-OFF keeps `User` (byte-parity).
    /// Behaves as `User` for all classification (NOT an inference placeholder).
    DeclScoped { file: Atom, node: u32 },
    /// A call-local inference placeholder minted for a generic call's own type
    /// parameter (`__infer_{id}` historically). `id` keeps it program-unique.
    InferPlaceholder { id: u64 },
    /// A higher-order *source* inference placeholder minted for a generic
    /// function argument's type parameter (`__infer_src_{id}#{origin}`
    /// historically, or the legacy index-named `__infer_src_ctx_{index}` form,
    /// which carries no origin name).
    ///
    /// `origin_name` records the source type parameter's original name so that
    /// re-generalized return types display with their source names; `None` for
    /// the legacy `__infer_src_ctx_*` form.
    InferSource { id: u64, origin_name: Option<Atom> },
    /// An overload signature's own type parameter that collided by name with a
    /// foreign (caller-scope) type parameter in the arguments, so
    /// `overload_signature_for_inference` renamed it to a program-unique
    /// `__overload_sig_{sig}_tp_{i}` atom for name-keyed inference. This is NOT
    /// an inference placeholder — it behaves as `User` for all classification —
    /// but the synthetic `name` must never reach diagnostics. `display_name`
    /// records the original declared name so the printer renders it instead.
    OverloadRenamed { display_name: Atom },
    /// A user-written type parameter declared by a standalone JSDoc comment.
    ///
    /// JSDoc `@overload`, `@typedef`, and `@callback` declarations have no AST
    /// name node. Their source position is stable when an incremental parse
    /// appends arena nodes. This dedicated variant's discriminant keeps the
    /// identity distinct from every real `NodeIndex`, even when the numeric
    /// payloads match. The payload matches [`Self::DeclScoped`], so this remains
    /// size-neutral.
    JsdocCommentScoped { file: Atom, pos: u32 },
    /// A user-written type parameter declared by JSDoc attached to an AST
    /// owner, such as a class or function.
    ///
    /// Owner-attached JSDoc parameters have no individual name nodes, so every
    /// sibling in one `@template T, U` list shares `(file, node)`. The origin
    /// discriminant keeps this namespace separate from syntax name nodes and
    /// standalone-comment positions; the parameter name completes the binder
    /// identity for sibling JSDoc parameters.
    JsdocOwnerScoped { file: Atom, node: u32 },
}

/// Stable identity of a user-declared type-parameter binder.
///
/// The declaration origin identifies the owning syntax node or JSDoc comment,
/// while the declared name distinguishes sibling JSDoc parameters that share
/// that owner. Syntax type parameters already have one name node per binder,
/// so including the name is redundant there but keeps every authoritative
/// identity consumer on one collision-free representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct TypeParamBinderKey {
    pub(crate) origin: TypeParamOrigin,
    pub(crate) name: Atom,
}

impl TypeParamOrigin {
    /// Any inference placeholder (call-local or higher-order source).
    ///
    /// Replaces the historical `starts_with("__infer_")` scan.
    #[inline]
    pub const fn is_infer_placeholder(self) -> bool {
        // Declaration-scoped origins are user-written params, NOT inference
        // placeholders — classify them with `User` (#14344 STEP-B).
        // `OverloadRenamed` is a declared overload type param that was renamed
        // for name-keyed inference; it too behaves as `User` (the rename does
        // not turn it into a HOFI/call-local placeholder).
        !matches!(
            self,
            Self::User
                | Self::DeclScoped { .. }
                | Self::JsdocCommentScoped { .. }
                | Self::JsdocOwnerScoped { .. }
                | Self::OverloadRenamed { .. }
        )
    }

    /// Whether this origin carries authoritative user-declaration identity.
    #[inline]
    pub const fn is_decl_scoped(self) -> bool {
        matches!(
            self,
            Self::DeclScoped { .. }
                | Self::JsdocCommentScoped { .. }
                | Self::JsdocOwnerScoped { .. }
        )
    }

    /// Higher-order *source* inference placeholder only.
    ///
    /// Replaces the historical `starts_with("__infer_src_")` scan.
    #[inline]
    pub const fn is_infer_source(self) -> bool {
        matches!(self, TypeParamOrigin::InferSource { .. })
    }

    /// Call-local inference placeholder only (excludes higher-order source).
    ///
    /// Replaces the historical
    /// `starts_with("__infer_") && !starts_with("__infer_src_")` scan.
    #[inline]
    pub const fn is_current_infer_placeholder(self) -> bool {
        matches!(self, TypeParamOrigin::InferPlaceholder { .. })
    }

    /// Origin type-parameter name for a higher-order source placeholder, if any.
    ///
    /// Replaces `decode_src_placeholder_origin`.
    #[inline]
    pub const fn infer_source_origin_name(self) -> Option<Atom> {
        match self {
            TypeParamOrigin::InferSource { origin_name, .. } => origin_name,
            _ => None,
        }
    }

    /// The original declared name for an overload-renamed type parameter, if
    /// any. Used by the printer to render the source name instead of the
    /// synthetic `__overload_sig_*` atom.
    #[inline]
    pub const fn overload_rename_display_name(self) -> Option<Atom> {
        match self {
            TypeParamOrigin::OverloadRenamed { display_name } => Some(display_name),
            _ => None,
        }
    }
}

/// Type parameter information
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TypeParamInfo {
    pub name: Atom,
    pub constraint: Option<TypeId>,
    pub default: Option<TypeId>,
    /// Whether this is a const type parameter (TS 5.0+)
    /// Const type parameters preserve literal types and infer readonly modifiers
    pub is_const: bool,
    /// Origin/kind discriminant. `User` for source-written and ordinary
    /// synthetic type parameters; the inference variants mark placeholders so
    /// classification is a field read, not a name-string scan.
    pub origin: TypeParamOrigin,
}

impl TypeParamInfo {
    /// Unconstrained, non-const, user-origin type parameter with no default.
    pub const fn simple(name: Atom) -> Self {
        Self {
            name,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::User,
        }
    }

    /// Authoritative declaration identity, when this parameter has one.
    #[inline]
    pub(crate) const fn declaration_binder_key(self) -> Option<TypeParamBinderKey> {
        if self.origin.is_decl_scoped() {
            Some(TypeParamBinderKey {
                origin: self.origin,
                name: self.name,
            })
        } else {
            None
        }
    }

    /// Whether two occurrences denote the same logical binder for lexical
    /// shadowing and ownership checks.
    ///
    /// Declaration-scoped parameters carry authoritative identity in their
    /// origin. Once either side has that identity, a same-spelled parameter
    /// from another declaration is foreign. Unstamped parameters retain the
    /// historical name-keyed behavior.
    #[inline]
    pub fn is_same_binder(self, other: Self) -> bool {
        match (
            self.declaration_binder_key(),
            other.declaration_binder_key(),
        ) {
            (Some(left), Some(right)) => left.origin == right.origin && left.name.0 == right.name.0,
            (Some(_), None) | (None, Some(_)) => false,
            (None, None) => self.name.0 == other.name.0,
        }
    }

    /// Whether this type parameter is any inference placeholder.
    #[inline]
    pub const fn is_infer_placeholder(&self) -> bool {
        self.origin.is_infer_placeholder()
    }

    /// Whether this type parameter is a higher-order source inference placeholder.
    #[inline]
    pub const fn is_infer_source(&self) -> bool {
        self.origin.is_infer_source()
    }

    /// Whether this type parameter is a call-local inference placeholder.
    #[inline]
    pub const fn is_current_infer_placeholder(&self) -> bool {
        self.origin.is_current_infer_placeholder()
    }
}

define_id!(/// Reference to a symbol (for named types)
    pub struct SymbolRef);

/// Conditional type structure
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ConditionalType {
    pub check_type: TypeId,
    pub extends_type: TypeId,
    pub true_type: TypeId,
    pub false_type: TypeId,
    pub is_distributive: bool,
}

/// Mapped type structure
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MappedType {
    pub type_param: TypeParamInfo,
    pub constraint: TypeId,
    pub name_type: Option<TypeId>,
    pub template: TypeId,
    pub readonly_modifier: Option<MappedModifier>,
    pub optional_modifier: Option<MappedModifier>,
}

impl MappedType {
    /// Resolve the result `readonly`-ness of a homomorphic mapped type applied
    /// to an array/tuple source.
    ///
    /// `+readonly` always makes the result readonly, `-readonly` always makes
    /// it mutable, and an absent modifier copies the source's readonly-ness
    /// (the homomorphic "copy modifiers" behavior `tsc` uses for arrays/tuples,
    /// mirroring per-property modifier copying for object sources).
    pub const fn resolve_readonly(&self, source_readonly: bool) -> bool {
        match self.readonly_modifier {
            Some(MappedModifier::Add) => true,
            Some(MappedModifier::Remove) => false,
            None => source_readonly,
        }
    }
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
    pub const fn is_text(&self) -> bool {
        matches!(self, Self::Text(_))
    }

    /// Check if this span is a type interpolation
    pub const fn is_type(&self) -> bool {
        matches!(self, Self::Type(_))
    }

    /// Get the text content if this is a text span
    pub const fn as_text(&self) -> Option<Atom> {
        match self {
            Self::Text(atom) => Some(*atom),
            _ => None,
        }
    }

    /// Get the type ID if this is a type span
    pub const fn as_type(&self) -> Option<TypeId> {
        match self {
            Self::Type(type_id) => Some(*type_id),
            _ => None,
        }
    }

    /// Create a type span
    pub const fn type_from_id(type_id: TypeId) -> Self {
        Self::Type(type_id)
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
                    let code = u8::from_str_radix(&format!("{hex1}{hex2}"), 16).unwrap_or(0);
                    result.push(code as char);
                }
                'u' => {
                    // \uXXXX or \u{X...}
                    if let Some('{') = chars.next() {
                        // \u{X...} - Unicode code point
                        let mut code_str = String::new();
                        for nc in chars.by_ref() {
                            if nc == '}' {
                                break;
                            }
                            code_str.push(nc);
                        }
                        if let Ok(code) = u32::from_str_radix(&code_str, 16)
                            && let Some(c) = char::from_u32(code)
                        {
                            result.push(c);
                        }
                    } else {
                        // \uXXXX - exactly 4 hex digits
                        let mut code_str = String::new();
                        for _ in 0..4 {
                            if let Some(nc) = chars.next() {
                                code_str.push(nc);
                            }
                        }
                        if let Ok(code) = u16::from_str_radix(&code_str, 16)
                            && let Some(c) = char::from_u32(code as u32)
                        {
                            result.push(c);
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

/// Returns true if the type name corresponds to a built-in type that should
/// be represented structurally or intrinsically, rather than by reference.
///
/// ## Built-in vs Referenced Types
///
/// **Built-in types** (managed by the compiler) are represented directly by their
/// structure (e.g., `TypeData::Array`) rather than by symbol reference (`TypeData::Ref`).
/// This ensures canonicalization: `Array<number>` and `number[]` resolve to the same type.
///
/// **Referenced types** (user-defined and lib types) are represented as `TypeData::Ref(symbol_id)`
/// and resolved lazily during type checking through the `TypeEnvironment`.
///
/// ## Examples
///
/// - `Array<T>` → `TypeData::Array(T)` (structural, not `Ref`)
/// - `Uppercase<S>` → `TypeData::StringIntrinsic { kind: Uppercase, ... }`
/// - `MyInterface` → `TypeData::Ref(SymbolRef(sym_id))`
///
/// ## When to Add Types
///
/// Add a type to this list if:
/// 1. It has special structural representation in `TypeData` (e.g., `Array`)
/// 2. It is a compiler intrinsic (e.g., `Uppercase`, `Lowercase`)
/// 3. It needs canonicalization with alternative syntax (e.g., `T[]` vs `Array<T>`)
///
/// **DO NOT** add:
/// - Regular lib types like `Promise`, `Map`, `Set` (these use `Ref`)
/// - User-defined interfaces or type aliases
pub fn is_compiler_managed_type(name: &str) -> bool {
    matches!(
        name,
        "Array" |          // Canonicalizes with T[] syntax
        "ReadonlyArray" |   // Built-in readonly array type
        "Uppercase" |       // String intrinsic
        "Lowercase" |       // String intrinsic
        "Capitalize" |      // String intrinsic
        "Uncapitalize" // String intrinsic
    )
}

// =============================================================================
// Variance Types (Task #41)
// =============================================================================

bitflags::bitflags! {
    /// Variance of a type parameter in a generic type.
    ///
    /// Variance determines how subtyping of generic types relates to subtyping
    /// of their type arguments. This is critical for O(1) generic assignability.
    ///
    /// ## Variance Kinds
    ///
    /// - **Covariant** (COVARIANT): T<U> <: T<V> iff U <: V
    ///   - Example: `Array`, `ReadonlyArray`, `Promise`
    /// - Most common for immutable containers
    ///
    /// - **Contravariant** (CONTRAVARIANT): T<U> <: T<V> iff V <: U (reversed)
    ///   - Example: Function parameters (in strict mode)
    /// - Rare in practice, mostly for function types
    ///
    /// - **Invariant** (COVARIANT | CONTRAVARIANT): T<U> <: T<V> iff U === V
    ///   - Example: Mutable properties, `Box<T>` with read/write
    /// - Requires both directions to hold
    ///
    /// - **Independent** (empty): Type parameter not used in variance position
    ///   - Example: Type parameter only used in non-variance positions
    /// - Can be skipped in subtype checks (always compatible)
    ///
    /// ## Examples
    ///
    /// ```typescript
    /// // Covariant: Array< Dog > <: Array< Animal >
    /// type Covariant<T> = { readonly get(): T };
    ///
    /// // Contravariant: Writer< Animal > <: Writer< Dog >
    /// type Contravariant<T> = { write(x: T): void };
    ///
    /// // Invariant: Box<Dog> NOT <: Box<Animal> (mutable!)
    /// type Invariant<T> = { get(): T; set(x: T): void };
    /// ```
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
    pub struct Variance: u8 {
        /// Covariant position (e.g., function return types)
        const COVARIANT = 1 << 0;
        /// Contravariant position (e.g., function parameters)
        const CONTRAVARIANT = 1 << 1;
        /// Variance may be unreliable due to mapped type modifiers (-?/+?/-readonly/+readonly).
        /// When set, the variance shortcut must fall through to structural comparison
        /// because modifiers can transform mutually-assignable type arguments into
        /// structurally incompatible results (e.g., Required<{a?}> vs Required<{b?}>).
        const NEEDS_STRUCTURAL_FALLBACK = 1 << 2;
        /// Variance-based REJECTION is unreliable. Different type arguments can
        /// produce structurally equivalent instantiations through indexed access
        /// types and intersection normalization. When set, a variance failure
        /// should fall through to structural comparison instead of conclusively
        /// rejecting. Example: `DT<{base: Base, new: New}>` vs
        /// `DT<{base: Base, new: New & Base}>` where `S["base"] & S["new"]`
        /// normalizes to the same type for both.
        const REJECTION_UNRELIABLE = 1 << 3;
        /// The type parameter was found in a direct (non-mapped-type) position,
        /// such as a function parameter, return type, or property type. When set
        /// alongside NEEDS_STRUCTURAL_FALLBACK, the variance rejection can be
        /// trusted because the direct usage provides a reliable variance signal
        /// that dominates over the unreliable mapped-type contribution.
        const DIRECT_USAGE = 1 << 4;
        /// The parameter occurs below a method/constructor parameter or a
        /// callback parameter governed by non-strict function checking. This
        /// is not true independence: unrelated arguments must still compare
        /// structurally, while the directional `any`/`never` exception accepts
        /// either orientation.
        const BIVARIANT_USAGE = 1 << 5;
    }
}

impl Variance {
    /// Check if this is an independent type parameter (not used in variance position).
    pub const fn is_independent(&self) -> bool {
        !self.contains(Self::COVARIANT)
            && !self.contains(Self::CONTRAVARIANT)
            && !self.contains(Self::BIVARIANT_USAGE)
    }

    /// Check if this is covariant only.
    pub const fn is_covariant(&self) -> bool {
        self.contains(Self::COVARIANT) && !self.contains(Self::CONTRAVARIANT)
    }

    /// Check if this is contravariant only.
    pub const fn is_contravariant(&self) -> bool {
        self.contains(Self::CONTRAVARIANT) && !self.contains(Self::COVARIANT)
    }

    /// Check if this is invariant (both covariant and contravariant).
    pub fn is_invariant(&self) -> bool {
        self.contains(Self::COVARIANT | Self::CONTRAVARIANT)
    }

    /// Check if variance requires structural fallback (unreliable due to mapped type modifiers).
    pub const fn needs_structural_fallback(&self) -> bool {
        self.contains(Self::NEEDS_STRUCTURAL_FALLBACK) || self.contains(Self::BIVARIANT_USAGE)
    }

    /// Check if variance-based rejection is unreliable. When true, a variance
    /// failure should fall through to structural comparison because indexed
    /// access types and intersections can normalize away differences between
    /// type arguments, producing structurally equivalent instantiations.
    pub const fn rejection_unreliable(&self) -> bool {
        self.contains(Self::REJECTION_UNRELIABLE)
    }

    /// Check if the type parameter was found in a direct (non-mapped-type) position.
    /// When true alongside `needs_structural_fallback()`, the variance rejection
    /// is still reliable because the direct usage provides a trustworthy signal.
    pub const fn has_direct_usage(&self) -> bool {
        self.contains(Self::DIRECT_USAGE)
    }

    /// Whether the effective variance walk found a bivariant parameter edge.
    pub const fn has_bivariant_usage(&self) -> bool {
        self.contains(Self::BIVARIANT_USAGE)
    }

    /// Whether bivariance is the only semantic occurrence in this mask.
    pub const fn is_pure_bivariant_usage(&self) -> bool {
        self.contains(Self::BIVARIANT_USAGE)
            && !self.contains(Self::COVARIANT)
            && !self.contains(Self::CONTRAVARIANT)
            && !self.contains(Self::NEEDS_STRUCTURAL_FALLBACK)
            && !self.contains(Self::REJECTION_UNRELIABLE)
    }

    /// Compose an outer variance (`self`) with a nested parameter variance.
    ///
    /// Rules:
    /// - Independent × anything = Independent
    /// - Covariant × Covariant = Covariant
    /// - Covariant × Contravariant = Contravariant
    /// - Contravariant × Covariant = Contravariant
    /// - Contravariant × Contravariant = Covariant
    /// - Outer bivariant × inner invariant = Invariant
    /// - Outer invariant × inner bivariant = Bivariant structural fallback
    /// - Bivariant × directional = Bivariant structural fallback
    pub fn compose(&self, other: Self) -> Self {
        if self.is_independent() || other.is_independent() {
            return Self::empty();
        }
        if other.is_invariant() {
            return Self::COVARIANT | Self::CONTRAVARIANT;
        }
        if other.has_bivariant_usage() {
            return Self::BIVARIANT_USAGE;
        }
        if self.has_bivariant_usage() {
            return Self::BIVARIANT_USAGE;
        }
        if self.is_invariant() {
            return Self::COVARIANT | Self::CONTRAVARIANT;
        }

        // XOR for covariance composition
        let is_covariant = self.is_covariant() == other.is_covariant();
        let is_contravariant = !is_covariant;

        let mut result = Self::empty();
        if is_covariant {
            result |= Self::COVARIANT;
        }
        if is_contravariant {
            result |= Self::CONTRAVARIANT;
        }
        result
    }
}

#[cfg(test)]
#[path = "../tests/types_tests.rs"]
mod tests;
