use crate::relations::subtype::TypeResolver;
use crate::type_queries::{UnionMembersKind, classify_for_union_members};
use crate::types::{FunctionShape, LiteralValue, ParamInfo, TypeData, TypeId};
use crate::utils::{TypeIdExt, union_or_single};
use crate::visitor::{
    index_access_parts, intersection_list_id, is_function_type_through_type_constraints,
    is_object_like_type_through_type_constraints, lazy_def_id, literal_value, object_shape_id,
    object_with_index_shape_id, template_literal_id, type_param_info, union_list_id,
};
use crate::{QueryDatabase, TypeDatabase};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::sync::Arc;
use tracing::{Level, span, trace};
use tsz_common::interner::Atom;

/// Describes whether a type guard should be applied in its positive (truthy)
/// or negative (falsy) sense.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardSense {
    /// The guard condition is true (e.g., `typeof x === "string"`).
    Positive,
    /// The guard condition is false (e.g., `typeof x !== "string"`).
    Negative,
}

impl From<bool> for GuardSense {
    fn from(value: bool) -> Self {
        if value {
            GuardSense::Positive
        } else {
            GuardSense::Negative
        }
    }
}

type SplitNullishParts = (Option<TypeId>, Option<TypeId>);

/// The result of a `typeof` expression, restricted to the 8 standard JavaScript types.
///
/// Using an enum instead of `String` eliminates heap allocation per typeof guard.
/// TypeScript's `typeof` operator only returns these 8 values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TypeofKind {
    String,
    Number,
    Boolean,
    BigInt,
    Symbol,
    Undefined,
    Object,
    Function,
}

impl TypeofKind {
    /// Parse a typeof result string into a `TypeofKind`.
    /// Returns None for non-standard typeof strings (which don't narrow).
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "string" => Some(Self::String),
            "number" => Some(Self::Number),
            "boolean" => Some(Self::Boolean),
            "bigint" => Some(Self::BigInt),
            "symbol" => Some(Self::Symbol),
            "undefined" => Some(Self::Undefined),
            "object" => Some(Self::Object),
            "function" => Some(Self::Function),
            _ => None,
        }
    }

    /// Get the string representation of this typeof kind.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::BigInt => "bigint",
            Self::Symbol => "symbol",
            Self::Undefined => "undefined",
            Self::Object => "object",
            Self::Function => "function",
        }
    }
}

/// AST-agnostic representation of a type narrowing condition.
///
/// This enum represents various guards that can narrow a type, without
/// depending on AST nodes like `NodeIndex` or `SyntaxKind`.
///
/// # Examples
/// ```typescript
/// typeof x === "string"     -> TypeGuard::Typeof(TypeofKind::String)
/// x instanceof MyClass      -> TypeGuard::Instanceof(MyClass_type)
/// x === null                -> TypeGuard::NullishEquality
/// x                         -> TypeGuard::Truthy
/// x.kind === "circle"       -> TypeGuard::Discriminant { property: "kind", value: "circle" }
/// ```
#[derive(Clone, Debug, PartialEq)]
pub enum TypeGuard {
    /// `typeof x === "typename"`
    ///
    /// Narrows a union to only members matching the typeof result.
    /// For example, narrowing `string | number` with `Typeof(TypeofKind::String)` yields `string`.
    Typeof(TypeofKind),

    /// `x instanceof Class`
    ///
    /// Narrows to the class type or its subtypes.
    /// The boolean flag indicates whether the constructor was an explicit global
    /// name like `Object` or `Function` (true) vs. a resolved/fallback type (false).
    /// This distinction matters for the false branch: only explicit global constructors
    /// trigger aggressive narrowing (e.g., excluding all non-primitives for `instanceof Object`).
    Instanceof(TypeId, bool),

    /// `x === literal` or `x !== literal`
    ///
    /// Narrows to exactly that literal type (for equality) or excludes it (for inequality).
    LiteralEquality(TypeId),

    /// `x == null` or `x != null` (checks both null and undefined)
    ///
    /// JavaScript/TypeScript treats `== null` as matching both `null` and `undefined`.
    NullishEquality,

    /// `x` (truthiness check in a conditional)
    ///
    /// Removes falsy types from a union: `null`, `undefined`, `false`, `0`, `""`, `NaN`.
    Truthy,

    /// `x.prop === literal` or `x.payload.type === "value"` (Discriminated Union narrowing)
    ///
    /// Narrows a union of object types based on a discriminant property.
    ///
    /// # Examples
    /// - Top-level: `{ kind: "A" } | { kind: "B" }` with `path: ["kind"]` yields `{ kind: "A" }`
    /// - Nested: `{ payload: { type: "user" } } | { payload: { type: "product" } }`
    ///   with `path: ["payload", "type"]` yields `{ payload: { type: "user" } }`
    Discriminant {
        /// Property path from base to discriminant (e.g., ["payload", "type"])
        property_path: Vec<Atom>,
        /// The literal value to match against
        value_type: TypeId,
    },

    /// `prop in x`
    ///
    /// Narrows to types that have the specified property.
    InProperty(Atom),

    /// `x is T` or `asserts x is T` (User-Defined Type Guard)
    ///
    /// Narrows a type based on a user-defined type predicate function.
    ///
    /// # Examples
    /// ```typescript
    /// function isString(x: any): x is string { ... }
    /// function assertDefined(x: any): asserts x is Date { ... }
    ///
    /// if (isString(x)) { x; // string }
    /// assertDefined(x); x; // Date
    /// ```
    ///
    /// - `type_id: Some(T)`: The type to narrow to (e.g., `string` or `Date`)
    /// - `type_id: None`: Truthiness assertion (`asserts x`), behaves like `Truthy`
    /// - `asserts: true`: This is an assertion (throws if false), affects control flow
    Predicate {
        type_id: Option<TypeId>,
        asserts: bool,
    },

    /// `Array.isArray(x)`
    ///
    /// Narrows a type to only array-like types (arrays, tuples, readonly arrays).
    ///
    /// # Examples
    /// ```typescript
    /// function process(x: string[] | number | { length: number }) {
    ///   if (Array.isArray(x)) {
    ///     x; // string[] (not number or the object)
    ///   }
    /// }
    /// ```
    ///
    /// This preserves element types - `string[] | number[]` stays as `string[] | number[]`,
    /// it doesn't collapse to `any[]`.
    Array,

    /// `array.every(predicate)` where predicate has type predicate
    ///
    /// Narrows an array's element type based on a type predicate.
    ///
    /// # Examples
    /// ```typescript
    /// const arr: (number | string)[] = ['aaa'];
    /// const isString = (x: unknown): x is string => typeof x === 'string';
    /// if (arr.every(isString)) {
    ///   arr; // string[] (element type narrowed from number | string to string)
    /// }
    /// ```
    ///
    /// This only applies to arrays. For non-array types, the type is unchanged.
    ArrayElementPredicate {
        /// The type to narrow array elements to
        element_type: TypeId,
    },

    /// `x.constructor === SomeClass`
    ///
    /// Narrows based on constructor identity (exact class match).
    /// Unlike `instanceof` which includes subclasses, constructor equality
    /// only matches the exact class whose constructor function is compared.
    /// For example, `C2 | string` narrowed by `Constructor(C1)` yields `never`
    /// because C2.constructor !== C1 (even though C2 extends C1).
    Constructor(TypeId),
}

#[inline]
pub(crate) fn union_or_single_preserve(db: &dyn TypeDatabase, types: Vec<TypeId>) -> TypeId {
    match types.len() {
        0 => TypeId::NEVER,
        1 => types[0],
        _ => db.union_from_sorted_vec(types),
    }
}

/// Create a union from an already-sorted slice, excluding a single member.
///
/// This avoids allocating a Vec when removing one member from an existing union.
/// For the common case of discriminant exclusion in if-chains (where one member
/// is removed at a time), this eliminates an O(N) Vec allocation per branch.
pub(crate) fn union_excluding_one(
    db: &dyn TypeDatabase,
    members: &[TypeId],
    excluded_idx: usize,
) -> TypeId {
    debug_assert!(excluded_idx < members.len());
    let new_len = members.len() - 1;
    if new_len == 0 {
        return TypeId::NEVER;
    }
    if new_len == 1 {
        // Return the single remaining member
        return if excluded_idx == 0 {
            members[1]
        } else {
            members[0]
        };
    }
    // Build the result without the excluded member
    let mut result = Vec::with_capacity(new_len);
    result.extend_from_slice(&members[..excluded_idx]);
    result.extend_from_slice(&members[excluded_idx + 1..]);
    db.union_from_sorted_vec(result)
}

/// Result of a narrowing operation.
///
/// Represents the types in both branches of a condition.
#[derive(Clone, Debug)]
pub struct NarrowingResult {
    /// The type in the "true" branch of the condition
    pub true_type: TypeId,
    /// The type in the "false" branch of the condition
    pub false_type: TypeId,
}

/// Result of finding discriminant properties in a union.
#[derive(Clone, Debug)]
pub struct DiscriminantInfo {
    /// The name of the discriminant property
    pub property_name: Atom,
    /// Map from literal value to the union member type
    pub variants: Vec<(TypeId, TypeId)>, // (literal_type, member_type)
}

type DiscriminantMembers = FxHashMap<TypeId, Vec<TypeId>>;
type DiscriminantIndex = FxHashMap<(TypeId, Atom), Arc<DiscriminantMembers>>;

/// Narrowing context for type guards and control flow analysis.
/// Shared across multiple narrowing contexts to persist resolution results.
#[derive(Default, Clone, Debug)]
pub struct NarrowingCache {
    /// Cache for type resolution (Lazy/App/Template -> Structural)
    pub resolve_cache: RefCell<FxHashMap<TypeId, TypeId>>,
    /// Cache for top-level property type lookups (TypeId, `PropName`) -> `PropType`
    pub property_cache: RefCell<FxHashMap<(TypeId, Atom), Option<TypeId>>>,
    /// Cache for split-nullish decomposition (TypeId -> (`non_nullish`, nullish)).
    /// Reused by checker optional-chain/property-access hot paths.
    pub split_nullish_cache: RefCell<FxHashMap<TypeId, SplitNullishParts>>,
    /// Cache for "type contains type parameters" checks.
    pub contains_type_parameters_cache: RefCell<FxHashMap<TypeId, bool>>,
    /// Cache for optional chain property access results.
    /// Keyed by `(object_type_with_nullish, property_atom)` → final result TypeId.
    /// Unlike `property_cache` which is keyed by resolved (non-nullish) base type,
    /// this caches the COMPLETE result including nullish union and undefined addition.
    /// This skips `split_nullish`, `resolve_type`, `contains_type_params`, and property
    /// lookup on cache hits — eliminating 4+ `RefCell` borrows per repeated access.
    pub optional_chain_cache: RefCell<FxHashMap<(TypeId, Atom), TypeId>>,
    /// Cache for contextual type resolution in object literal property typing.
    /// Maps raw contextual TypeId -> fully resolved TypeId after the
    /// evaluate/resolve/lazy/application chain. Avoids repeating the expensive
    /// chain for each property of the same object literal.
    pub contextual_resolve_cache: RefCell<FxHashMap<TypeId, TypeId>>,
    /// Discriminant index for fast switch-case narrowing.
    /// Key: (`union_type`, `discriminant_property`) → Map of `literal_value` → matching members.
    /// Built once per (union, property) pair, then O(1) lookup per case clause.
    /// Without this, each case clause iterates ALL union members (O(N) per case = O(N²) total).
    pub discriminant_index: RefCell<DiscriminantIndex>,
}

impl NarrowingCache {
    pub fn new() -> Self {
        Self {
            resolve_cache: RefCell::new(FxHashMap::with_capacity_and_hasher(
                1024,
                Default::default(),
            )),
            property_cache: RefCell::new(FxHashMap::with_capacity_and_hasher(
                512,
                Default::default(),
            )),
            split_nullish_cache: RefCell::new(FxHashMap::with_capacity_and_hasher(
                512,
                Default::default(),
            )),
            contains_type_parameters_cache: RefCell::new(FxHashMap::with_capacity_and_hasher(
                1024,
                Default::default(),
            )),
            optional_chain_cache: RefCell::new(FxHashMap::with_capacity_and_hasher(
                512,
                Default::default(),
            )),
            contextual_resolve_cache: RefCell::new(FxHashMap::with_capacity_and_hasher(
                256,
                Default::default(),
            )),
            discriminant_index: RefCell::new(FxHashMap::default()),
        }
    }
}

/// Narrowing context for type guards and control flow analysis.
pub struct NarrowingContext<'a> {
    pub(crate) db: &'a dyn QueryDatabase,
    /// Optional `TypeResolver` for resolving Lazy types (e.g., type aliases).
    /// When present, this enables proper narrowing of type aliases like `type Shape = Circle | Square`.
    pub(crate) resolver: Option<&'a dyn TypeResolver>,
    /// Cache for narrowing operations.
    /// If provided, uses the shared cache; otherwise uses a local ephemeral cache.
    pub(crate) cache: std::borrow::Cow<'a, NarrowingCache>,
}

mod context;
