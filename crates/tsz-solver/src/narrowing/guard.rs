use crate::types::TypeId;
use tsz_common::interner::Atom;

/// Describes whether a type guard should be applied in its positive (truthy)
/// or negative (falsy) sense.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TypeGuard {
    /// `typeof x === "typename"`
    ///
    /// Narrows a union to only members matching the typeof result.
    /// For example, narrowing `string | number` with `TypeGuard::Typeof(TypeofKind::String)` yields `string`.
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
