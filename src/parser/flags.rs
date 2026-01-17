//! Node flags and modifier flags for AST nodes.

/// Node flags indicating various properties of AST nodes.
/// Matches TypeScript's NodeFlags enum exactly.
/// NOTE: wasm_bindgen doesn't support bit-shift expressions, so these are
/// stored as a u32 bitfield in NodeBase. Use the constants below for flag operations.
pub mod node_flags {
    pub const NONE: u32 = 0;
    pub const LET: u32 = 1; // 1 << 0
    pub const CONST: u32 = 2; // 1 << 1
    pub const USING: u32 = 4; // 1 << 2
    pub const AWAIT_USING: u32 = 6; // Const | Using
    pub const NESTED_NAMESPACE: u32 = 8; // 1 << 3
    pub const SYNTHESIZED: u32 = 16; // 1 << 4
    pub const NAMESPACE: u32 = 32; // 1 << 5
    pub const OPTIONAL_CHAIN: u32 = 64; // 1 << 6
    pub const EXPORT_CONTEXT: u32 = 128; // 1 << 7
    pub const CONTAINS_THIS: u32 = 256; // 1 << 8
    pub const HAS_IMPLICIT_RETURN: u32 = 512; // 1 << 9
    pub const HAS_EXPLICIT_RETURN: u32 = 1024; // 1 << 10
    pub const GLOBAL_AUGMENTATION: u32 = 2048; // 1 << 11
    pub const HAS_ASYNC_FUNCTIONS: u32 = 4096; // 1 << 12
    pub const DISALLOW_IN_CONTEXT: u32 = 8192; // 1 << 13
    pub const YIELD_CONTEXT: u32 = 16384; // 1 << 14
    pub const DECORATOR_CONTEXT: u32 = 32768; // 1 << 15
    pub const AWAIT_CONTEXT: u32 = 65536; // 1 << 16
    pub const DISALLOW_CONDITIONAL_TYPES_CONTEXT: u32 = 131072; // 1 << 17
    pub const THIS_NODE_HAS_ERROR: u32 = 262144; // 1 << 18
    pub const JAVASCRIPT_FILE: u32 = 524288; // 1 << 19
    pub const THIS_NODE_OR_ANY_SUB_NODES_HAS_ERROR: u32 = 1048576; // 1 << 20
    pub const HAS_AGGREGATED_CHILD_DATA: u32 = 2097152; // 1 << 21
    pub const POSSIBLY_CONTAINS_DYNAMIC_IMPORT: u32 = 4194304; // 1 << 22
    pub const POSSIBLY_CONTAINS_IMPORT_META: u32 = 8388608; // 1 << 23
    pub const JSDOC: u32 = 16777216; // 1 << 24
    pub const AMBIENT: u32 = 33554432; // 1 << 25
    pub const IN_WITH_STATEMENT: u32 = 67108864; // 1 << 26
    pub const JSON_FILE: u32 = 134217728; // 1 << 27
    pub const TYPE_CACHED: u32 = 268435456; // 1 << 28
    pub const DEPRECATED: u32 = 536870912; // 1 << 29

    // Type-only imports/exports
    pub const TYPE_ONLY: u32 = 1073741824; // 1 << 30
}

/// Modifier flags for declarations.
/// Matches TypeScript's ModifierFlags enum exactly.
pub mod modifier_flags {
    pub const NONE: u32 = 0;

    // Syntactic/JSDoc modifiers
    pub const PUBLIC: u32 = 1; // 1 << 0
    pub const PRIVATE: u32 = 2; // 1 << 1
    pub const PROTECTED: u32 = 4; // 1 << 2
    pub const READONLY: u32 = 8; // 1 << 3
    pub const OVERRIDE: u32 = 16; // 1 << 4

    // Syntactic-only modifiers
    pub const EXPORT: u32 = 32; // 1 << 5
    pub const ABSTRACT: u32 = 64; // 1 << 6
    pub const AMBIENT: u32 = 128; // 1 << 7
    pub const STATIC: u32 = 256; // 1 << 8
    pub const ACCESSOR: u32 = 512; // 1 << 9
    pub const ASYNC: u32 = 1024; // 1 << 10
    pub const DEFAULT: u32 = 2048; // 1 << 11
    pub const CONST: u32 = 4096; // 1 << 12
    pub const IN: u32 = 8192; // 1 << 13
    pub const OUT: u32 = 16384; // 1 << 14
    pub const DECORATOR: u32 = 32768; // 1 << 15

    // JSDoc-only modifiers
    pub const DEPRECATED: u32 = 65536; // 1 << 16
}

/// Transform flags indicate which transformations are needed for emit.
/// Matches TypeScript's TransformFlags enum.
pub mod transform_flags {
    pub const NONE: u32 = 0;

    // Facts about the node
    pub const CONTAINS_TYPESCRIPT: u32 = 1; // 1 << 0
    pub const CONTAINS_JSX: u32 = 2; // 1 << 1
    pub const CONTAINS_ESNEXT: u32 = 4; // 1 << 2
    pub const CONTAINS_ES2022: u32 = 8; // 1 << 3
    pub const CONTAINS_ES2021: u32 = 16; // 1 << 4
    pub const CONTAINS_ES2020: u32 = 32; // 1 << 5
    pub const CONTAINS_ES2019: u32 = 64; // 1 << 6
    pub const CONTAINS_ES2018: u32 = 128; // 1 << 7
    pub const CONTAINS_ES2017: u32 = 256; // 1 << 8
    pub const CONTAINS_ES2016: u32 = 512; // 1 << 9
    pub const CONTAINS_ES2015: u32 = 1024; // 1 << 10
    pub const CONTAINS_GENERATOR: u32 = 2048; // 1 << 11
    pub const CONTAINS_DESTRUCTURING_ASSIGNMENT: u32 = 4096; // 1 << 12

    // Markers
    pub const CONTAINS_TYPESCRIPT_CLASS_SYNTAX: u32 = 8192; // 1 << 13
    pub const CONTAINS_LEXICAL_THIS: u32 = 16384; // 1 << 14
    pub const CONTAINS_REST_OR_SPREAD: u32 = 32768; // 1 << 15
    pub const CONTAINS_OBJECT_REST_OR_SPREAD: u32 = 65536; // 1 << 16
    pub const CONTAINS_COMPUTED_PROPERTY_NAME: u32 = 131072; // 1 << 17
    pub const CONTAINS_BLOCK_SCOPED_BINDING: u32 = 262144; // 1 << 18
    pub const CONTAINS_BINDING_PATTERN: u32 = 524288; // 1 << 19
    pub const CONTAINS_YIELD: u32 = 1048576; // 1 << 20
    pub const CONTAINS_AWAIT: u32 = 2097152; // 1 << 21
    pub const CONTAINS_HOISTED_DECLARATION_OR_COMPLETION: u32 = 4194304; // 1 << 22
    pub const CONTAINS_DYNAMIC_IMPORT: u32 = 8388608; // 1 << 23
    pub const CONTAINS_CLASS_FIELDS: u32 = 16777216; // 1 << 24
    pub const CONTAINS_DECORATORS: u32 = 33554432; // 1 << 25
    pub const CONTAINS_POSSIBLE_TOP_LEVEL_AWAIT: u32 = 67108864; // 1 << 26
    pub const CONTAINS_LEXICAL_SUPER: u32 = 134217728; // 1 << 27
    pub const CONTAINS_UPDATE_EXPRESSION_FOR_IDENTIFIER: u32 = 268435456; // 1 << 28
    pub const CONTAINS_PRIVATE_IDENTIFIER_IN_EXPRESSION: u32 = 536870912; // 1 << 29

    pub const HAS_COMPUTED_FLAGS: u32 = 2147483648; // 1 << 31
}
