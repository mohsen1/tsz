//! TypeScript Enums
//!
//! Exports TypeScript-compatible enum values via wasm-bindgen.
//! These match TypeScript's enum definitions exactly.

use wasm_bindgen::prelude::*;

/// SyntaxKind enum values
/// Matches TypeScript's SyntaxKind exactly
#[allow(non_snake_case)]
pub mod SyntaxKind {
    pub const UNKNOWN: u16 = 0;
    pub const END_OF_FILE_TOKEN: u16 = 1;
    pub const SINGLE_LINE_COMMENT_TRIVIA: u16 = 2;
    pub const MULTI_LINE_COMMENT_TRIVIA: u16 = 3;
    pub const NEW_LINE_TRIVIA: u16 = 4;
    pub const WHITESPACE_TRIVIA: u16 = 5;
    pub const NUMERIC_LITERAL: u16 = 9;
    pub const BIG_INT_LITERAL: u16 = 10;
    pub const STRING_LITERAL: u16 = 11;
    pub const IDENTIFIER: u16 = 80;
    pub const PRIVATE_IDENTIFIER: u16 = 81;

    // Keywords
    pub const BREAK_KEYWORD: u16 = 83;
    pub const CASE_KEYWORD: u16 = 84;
    pub const CLASS_KEYWORD: u16 = 86;
    pub const CONST_KEYWORD: u16 = 87;
    pub const FUNCTION_KEYWORD: u16 = 100;
    pub const IF_KEYWORD: u16 = 101;
    pub const IMPORT_KEYWORD: u16 = 102;
    pub const LET_KEYWORD: u16 = 121;
    pub const RETURN_KEYWORD: u16 = 107;
    pub const VAR_KEYWORD: u16 = 115;

    // Type keywords
    pub const ANY_KEYWORD: u16 = 133;
    pub const BOOLEAN_KEYWORD: u16 = 136;
    pub const NEVER_KEYWORD: u16 = 146;
    pub const NUMBER_KEYWORD: u16 = 150;
    pub const OBJECT_KEYWORD: u16 = 151;
    pub const STRING_KEYWORD: u16 = 154;
    pub const UNKNOWN_KEYWORD: u16 = 159;
    pub const VOID_KEYWORD: u16 = 116;

    // Declarations
    pub const TYPE_PARAMETER: u16 = 168;
    pub const PARAMETER: u16 = 169;
    pub const PROPERTY_SIGNATURE: u16 = 171;
    pub const PROPERTY_DECLARATION: u16 = 172;
    pub const METHOD_SIGNATURE: u16 = 173;
    pub const METHOD_DECLARATION: u16 = 174;
    pub const CONSTRUCTOR: u16 = 176;
    pub const GET_ACCESSOR: u16 = 177;
    pub const SET_ACCESSOR: u16 = 178;
    pub const CALL_SIGNATURE: u16 = 179;
    pub const CONSTRUCT_SIGNATURE: u16 = 180;
    pub const INDEX_SIGNATURE: u16 = 181;

    // Types
    pub const TYPE_PREDICATE: u16 = 182;
    pub const TYPE_REFERENCE: u16 = 183;
    pub const FUNCTION_TYPE: u16 = 184;
    pub const CONSTRUCTOR_TYPE: u16 = 185;
    pub const TYPE_QUERY: u16 = 186;
    pub const TYPE_LITERAL: u16 = 187;
    pub const ARRAY_TYPE: u16 = 188;
    pub const TUPLE_TYPE: u16 = 189;
    pub const UNION_TYPE: u16 = 192;
    pub const INTERSECTION_TYPE: u16 = 193;
    pub const CONDITIONAL_TYPE: u16 = 194;
    pub const INFER_TYPE: u16 = 195;
    pub const MAPPED_TYPE: u16 = 200;
    pub const LITERAL_TYPE: u16 = 201;
    pub const TEMPLATE_LITERAL_TYPE: u16 = 203;

    // Binding patterns
    pub const OBJECT_BINDING_PATTERN: u16 = 206;
    pub const ARRAY_BINDING_PATTERN: u16 = 207;
    pub const BINDING_ELEMENT: u16 = 208;

    // Expressions
    pub const ARRAY_LITERAL_EXPRESSION: u16 = 209;
    pub const OBJECT_LITERAL_EXPRESSION: u16 = 210;
    pub const PROPERTY_ACCESS_EXPRESSION: u16 = 211;
    pub const ELEMENT_ACCESS_EXPRESSION: u16 = 212;
    pub const CALL_EXPRESSION: u16 = 213;
    pub const NEW_EXPRESSION: u16 = 214;
    pub const FUNCTION_EXPRESSION: u16 = 218;
    pub const ARROW_FUNCTION: u16 = 219;
    pub const BINARY_EXPRESSION: u16 = 226;
    pub const CONDITIONAL_EXPRESSION: u16 = 227;
    pub const AS_EXPRESSION: u16 = 234;

    // Statements
    pub const BLOCK: u16 = 241;
    pub const VARIABLE_STATEMENT: u16 = 243;
    pub const EXPRESSION_STATEMENT: u16 = 244;
    pub const IF_STATEMENT: u16 = 245;
    pub const FOR_STATEMENT: u16 = 248;
    pub const FOR_IN_STATEMENT: u16 = 249;
    pub const FOR_OF_STATEMENT: u16 = 250;
    pub const RETURN_STATEMENT: u16 = 253;
    pub const SWITCH_STATEMENT: u16 = 255;
    pub const TRY_STATEMENT: u16 = 258;

    // Declarations
    pub const VARIABLE_DECLARATION: u16 = 260;
    pub const VARIABLE_DECLARATION_LIST: u16 = 261;
    pub const FUNCTION_DECLARATION: u16 = 262;
    pub const CLASS_DECLARATION: u16 = 263;
    pub const INTERFACE_DECLARATION: u16 = 264;
    pub const TYPE_ALIAS_DECLARATION: u16 = 265;
    pub const ENUM_DECLARATION: u16 = 266;
    pub const MODULE_DECLARATION: u16 = 267;
    pub const IMPORT_DECLARATION: u16 = 272;
    pub const EXPORT_DECLARATION: u16 = 278;
    pub const EXPORT_ASSIGNMENT: u16 = 277;

    // Source file
    pub const SOURCE_FILE: u16 = 312;
}

/// DiagnosticCategory enum
#[wasm_bindgen]
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticCategory {
    Warning = 0,
    Error = 1,
    Suggestion = 2,
    Message = 3,
}

/// ScriptTarget enum (ES version)
#[wasm_bindgen]
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ScriptTarget {
    ES3 = 0,
    ES5 = 1,
    ES2015 = 2,
    ES2016 = 3,
    ES2017 = 4,
    ES2018 = 5,
    ES2019 = 6,
    ES2020 = 7,
    ES2021 = 8,
    ES2022 = 9,
    ES2023 = 10,
    #[default]
    ESNext = 99,
    JSON = 100,
}

/// ModuleKind enum
#[wasm_bindgen]
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ModuleKind {
    #[default]
    None = 0,
    CommonJS = 1,
    AMD = 2,
    UMD = 3,
    System = 4,
    ES2015 = 5,
    ES2020 = 6,
    ES2022 = 7,
    ESNext = 99,
    Node16 = 100,
    NodeNext = 199,
    Preserve = 200,
}

/// ScriptKind enum (file type)
#[wasm_bindgen]
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ScriptKind {
    Unknown = 0,
    JS = 1,
    JSX = 2,
    #[default]
    TS = 3,
    TSX = 4,
    External = 5,
    JSON = 6,
    Deferred = 7,
}

/// TypeFlags enum
#[allow(non_snake_case)]
pub mod TypeFlags {
    pub const ANY: u32 = 1 << 0;
    pub const UNKNOWN: u32 = 1 << 1;
    pub const STRING: u32 = 1 << 2;
    pub const NUMBER: u32 = 1 << 3;
    pub const BOOLEAN: u32 = 1 << 4;
    pub const ENUM: u32 = 1 << 5;
    pub const BIG_INT: u32 = 1 << 6;
    pub const STRING_LITERAL: u32 = 1 << 7;
    pub const NUMBER_LITERAL: u32 = 1 << 8;
    pub const BOOLEAN_LITERAL: u32 = 1 << 9;
    pub const ENUM_LITERAL: u32 = 1 << 10;
    pub const BIG_INT_LITERAL: u32 = 1 << 11;
    pub const ES_SYMBOL: u32 = 1 << 12;
    pub const UNIQUE_ES_SYMBOL: u32 = 1 << 13;
    pub const VOID: u32 = 1 << 14;
    pub const UNDEFINED: u32 = 1 << 15;
    pub const NULL: u32 = 1 << 16;
    pub const NEVER: u32 = 1 << 17;
    pub const TYPE_PARAMETER: u32 = 1 << 18;
    pub const OBJECT: u32 = 1 << 19;
    pub const UNION: u32 = 1 << 20;
    pub const INTERSECTION: u32 = 1 << 21;
    pub const INDEX: u32 = 1 << 22;
    pub const INDEXED_ACCESS: u32 = 1 << 23;
    pub const CONDITIONAL: u32 = 1 << 24;
    pub const SUBSTITUTION: u32 = 1 << 25;
    pub const NON_PRIMITIVE: u32 = 1 << 26;
    pub const TEMPLATE_LITERAL: u32 = 1 << 27;
    pub const STRING_MAPPING: u32 = 1 << 28;
}

/// SymbolFlags enum
#[allow(non_snake_case)]
pub mod SymbolFlags {
    pub const NONE: u32 = 0;
    pub const FUNCTION_SCOPED_VARIABLE: u32 = 1 << 0;
    pub const BLOCK_SCOPED_VARIABLE: u32 = 1 << 1;
    pub const PROPERTY: u32 = 1 << 2;
    pub const ENUM_MEMBER: u32 = 1 << 3;
    pub const FUNCTION: u32 = 1 << 4;
    pub const CLASS: u32 = 1 << 5;
    pub const INTERFACE: u32 = 1 << 6;
    pub const CONST_ENUM: u32 = 1 << 7;
    pub const REGULAR_ENUM: u32 = 1 << 8;
    pub const VALUE_MODULE: u32 = 1 << 9;
    pub const NAMESPACE_MODULE: u32 = 1 << 10;
    pub const TYPE_LITERAL: u32 = 1 << 11;
    pub const OBJECT_LITERAL: u32 = 1 << 12;
    pub const METHOD: u32 = 1 << 13;
    pub const CONSTRUCTOR: u32 = 1 << 14;
    pub const GET_ACCESSOR: u32 = 1 << 15;
    pub const SET_ACCESSOR: u32 = 1 << 16;
    pub const SIGNATURE: u32 = 1 << 17;
    pub const TYPE_PARAMETER: u32 = 1 << 18;
    pub const TYPE_ALIAS: u32 = 1 << 19;
    pub const EXPORT_VALUE: u32 = 1 << 20;
    pub const ALIAS: u32 = 1 << 21;
    pub const PROTOTYPE: u32 = 1 << 22;
    pub const EXPORT_STAR: u32 = 1 << 23;
    pub const OPTIONAL: u32 = 1 << 24;
    pub const TRANSIENT: u32 = 1 << 25;

    pub const VARIABLE: u32 = FUNCTION_SCOPED_VARIABLE | BLOCK_SCOPED_VARIABLE;
    pub const VALUE: u32 = VARIABLE | PROPERTY | ENUM_MEMBER | OBJECT_LITERAL | FUNCTION | CLASS;
    pub const TYPE: u32 =
        CLASS | INTERFACE | REGULAR_ENUM | CONST_ENUM | TYPE_LITERAL | TYPE_PARAMETER | TYPE_ALIAS;
    pub const NAMESPACE: u32 = VALUE_MODULE | NAMESPACE_MODULE | REGULAR_ENUM | CONST_ENUM;
    pub const MODULE: u32 = VALUE_MODULE | NAMESPACE_MODULE;
}

/// SignatureKind enum
#[wasm_bindgen]
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignatureKind {
    Call = 0,
    Construct = 1,
}
