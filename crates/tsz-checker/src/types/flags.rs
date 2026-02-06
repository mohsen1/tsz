//! Type flags and related flag constants.
//!
//! This module contains all flag constants used by the type checker.

/// Flags that describe the kind of a type.
/// Matches TypeScript's TypeFlags enum in src/compiler/types.ts
pub mod type_flags {
    // Primitive types
    pub const ANY: u32 = 1 << 0;
    pub const UNKNOWN: u32 = 1 << 1;
    pub const STRING: u32 = 1 << 2;
    pub const NUMBER: u32 = 1 << 3;
    pub const BOOLEAN: u32 = 1 << 4;
    pub const ENUM: u32 = 1 << 5;
    pub const BIG_INT: u32 = 1 << 6;

    // Literal types
    pub const STRING_LITERAL: u32 = 1 << 7;
    pub const NUMBER_LITERAL: u32 = 1 << 8;
    pub const BOOLEAN_LITERAL: u32 = 1 << 9;
    pub const ENUM_LITERAL: u32 = 1 << 10;
    pub const BIG_INT_LITERAL: u32 = 1 << 11;

    // Symbol types
    pub const ES_SYMBOL: u32 = 1 << 12;
    pub const UNIQUE_ES_SYMBOL: u32 = 1 << 13;

    // Special types
    pub const VOID: u32 = 1 << 14;
    pub const UNDEFINED: u32 = 1 << 15;
    pub const NULL: u32 = 1 << 16;
    pub const NEVER: u32 = 1 << 17;

    // Compound types
    pub const TYPE_PARAMETER: u32 = 1 << 18;
    pub const OBJECT: u32 = 1 << 19;
    pub const UNION: u32 = 1 << 20;
    pub const INTERSECTION: u32 = 1 << 21;

    // Type operators
    pub const INDEX: u32 = 1 << 22; // keyof T
    pub const INDEXED_ACCESS: u32 = 1 << 23; // T[K]
    pub const CONDITIONAL: u32 = 1 << 24; // T extends U ? X : Y
    pub const SUBSTITUTION: u32 = 1 << 25;

    // Other
    pub const NON_PRIMITIVE: u32 = 1 << 26; // object
    pub const TEMPLATE_LITERAL: u32 = 1 << 27;
    pub const STRING_MAPPING: u32 = 1 << 28; // Uppercase<T>

    // Composite flags
    pub const ANY_OR_UNKNOWN: u32 = ANY | UNKNOWN;
    pub const NULLABLE: u32 = UNDEFINED | NULL;
    pub const LITERAL: u32 = STRING_LITERAL | NUMBER_LITERAL | BIG_INT_LITERAL | BOOLEAN_LITERAL;
    pub const UNIT: u32 = ENUM | LITERAL | UNIQUE_ES_SYMBOL | NULLABLE;
    pub const STRING_OR_NUMBER_LITERAL: u32 = STRING_LITERAL | NUMBER_LITERAL;
    pub const STRING_LIKE: u32 = STRING | STRING_LITERAL | TEMPLATE_LITERAL | STRING_MAPPING;
    pub const NUMBER_LIKE: u32 = NUMBER | NUMBER_LITERAL | ENUM;
    pub const BIG_INT_LIKE: u32 = BIG_INT | BIG_INT_LITERAL;
    pub const BOOLEAN_LIKE: u32 = BOOLEAN | BOOLEAN_LITERAL;
    pub const ENUM_LIKE: u32 = ENUM | ENUM_LITERAL;
    pub const ES_SYMBOL_LIKE: u32 = ES_SYMBOL | UNIQUE_ES_SYMBOL;
    pub const VOID_LIKE: u32 = VOID | UNDEFINED;
    pub const PRIMITIVE: u32 = STRING_LIKE
        | NUMBER_LIKE
        | BIG_INT_LIKE
        | BOOLEAN_LIKE
        | ENUM_LIKE
        | ES_SYMBOL_LIKE
        | VOID_LIKE
        | NULL;
    pub const UNION_OR_INTERSECTION: u32 = UNION | INTERSECTION;
    pub const STRUCTURED_TYPE: u32 = OBJECT | UNION | INTERSECTION;
    pub const TYPE_VARIABLE: u32 = TYPE_PARAMETER | INDEXED_ACCESS;
    pub const INSTANTIABLE_NON_PRIMITIVE: u32 = TYPE_VARIABLE | CONDITIONAL | SUBSTITUTION;
    pub const INSTANTIABLE_PRIMITIVE: u32 = INDEX | TEMPLATE_LITERAL | STRING_MAPPING;
    pub const INSTANTIABLE: u32 = INSTANTIABLE_NON_PRIMITIVE | INSTANTIABLE_PRIMITIVE;
    pub const STRUCTURED_OR_INSTANTIABLE: u32 = STRUCTURED_TYPE | INSTANTIABLE;
    pub const NARROWABLE: u32 = ANY
        | UNKNOWN
        | STRUCTURED_OR_INSTANTIABLE
        | STRING_LIKE
        | NUMBER_LIKE
        | BIG_INT_LIKE
        | BOOLEAN_LIKE
        | ES_SYMBOL
        | UNIQUE_ES_SYMBOL
        | NON_PRIMITIVE;
}

/// Additional flags for object types.
/// Matches TypeScript's ObjectFlags enum in src/compiler/types.ts
pub mod object_flags {
    pub const CLASS: u32 = 1 << 0;
    pub const INTERFACE: u32 = 1 << 1;
    pub const REFERENCE: u32 = 1 << 2;
    pub const TUPLE: u32 = 1 << 3;
    pub const ANONYMOUS: u32 = 1 << 4;
    pub const MAPPED: u32 = 1 << 5;
    pub const INSTANTIATED: u32 = 1 << 6;
    pub const OBJECT_LITERAL: u32 = 1 << 7;
    pub const EVOLVING_ARRAY: u32 = 1 << 8;
    pub const OBJECT_LITERAL_PATTERN: u32 = 1 << 9;
    pub const FRESH_LITERAL: u32 = 1 << 10;
    pub const ARRAY_LITERAL: u32 = 1 << 11;
    pub const PRIMITIVE_UNION: u32 = 1 << 12;
    pub const CONTAINS_SPREAD: u32 = 1 << 13;
    pub const REVERSE_MAPPED: u32 = 1 << 14;
    pub const JSX_ATTRIBUTES: u32 = 1 << 15;
    pub const MARKER: u32 = 1 << 16;
    /// ThisType<T> marker - specifies the type of 'this' in object literal methods
    pub const IS_THIS_TYPE: u32 = 1 << 17;
    pub const CLASS_OR_INTERFACE: u32 = CLASS | INTERFACE;
}

/// Flags for function signatures.
pub mod signature_flags {
    pub const NONE: u32 = 0;
    pub const HAS_REST_PARAMETER: u32 = 1 << 0;
    pub const HAS_LITERAL_TYPES: u32 = 1 << 1;
    pub const IS_INNER_CALL_CHAIN: u32 = 1 << 2;
    pub const IS_OUTER_CALL_CHAIN: u32 = 1 << 3;
    pub const IS_UNTERMINATED_CALL_CHAIN: u32 = 1 << 4;
    pub const OPTIONAL_CALL_CHAIN: u32 = IS_INNER_CALL_CHAIN | IS_OUTER_CALL_CHAIN;
    pub const CALL_CHAIN_FLAGS: u32 = OPTIONAL_CALL_CHAIN | IS_UNTERMINATED_CALL_CHAIN;
}
