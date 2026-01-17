//! Parser types - AST node definitions for TypeScript.
//!
//! This module defines the AST node types that match TypeScript's parser output.
//! The goal is to produce an identical AST structure that can be serialized and
//! consumed by the TypeScript type checker.
//!
//! DESIGN NOTES:
//! - We use arena allocation (indices) rather than Box/Rc for node references
//! - All nodes have common fields: kind, flags, pos, end
//! - Node-specific data is stored in enum variants
//! - This design allows efficient serialization to/from JavaScript
//!
//! PERFORMANCE NOTES:
//! - The `thin_node` module provides a cache-optimized 16-byte node representation
//! - Current `Node` enum is 208 bytes (0.31 nodes/cache-line)
//! - ThinNode is 16 bytes (4 nodes/cache-line) - 13x better cache locality

pub mod arena;
pub mod ast;
pub mod flags;
pub mod thin_node;

#[cfg(test)]
mod tests;

// Re-export flags
pub use flags::{modifier_flags, node_flags, transform_flags};

// Re-export AST types
pub use ast::*;

// Re-export arena
pub use arena::NodeArena;
pub use thin_node::ThinNodeArena;

/// Extended SyntaxKind values for AST nodes that are not tokens.
/// These match TypeScript's SyntaxKind enum values exactly.
pub mod syntax_kind_ext {
    // First AST node kinds (after tokens, starting at 167)
    pub const QUALIFIED_NAME: u16 = 167;
    pub const COMPUTED_PROPERTY_NAME: u16 = 168;
    pub const TYPE_PARAMETER: u16 = 169;
    pub const PARAMETER: u16 = 170;
    pub const DECORATOR: u16 = 171;
    pub const PROPERTY_SIGNATURE: u16 = 172;
    pub const PROPERTY_DECLARATION: u16 = 173;
    pub const METHOD_SIGNATURE: u16 = 174;
    pub const METHOD_DECLARATION: u16 = 175;
    pub const CLASS_STATIC_BLOCK_DECLARATION: u16 = 176;
    pub const CONSTRUCTOR: u16 = 177;
    pub const GET_ACCESSOR: u16 = 178;
    pub const SET_ACCESSOR: u16 = 179;
    pub const CALL_SIGNATURE: u16 = 180;
    pub const CONSTRUCT_SIGNATURE: u16 = 181;
    pub const INDEX_SIGNATURE: u16 = 182;

    // Type nodes
    pub const TYPE_PREDICATE: u16 = 183;
    pub const TYPE_REFERENCE: u16 = 184;
    pub const FUNCTION_TYPE: u16 = 185;
    pub const CONSTRUCTOR_TYPE: u16 = 186;
    pub const TYPE_QUERY: u16 = 187;
    pub const TYPE_LITERAL: u16 = 188;
    pub const ARRAY_TYPE: u16 = 189;
    pub const TUPLE_TYPE: u16 = 190;
    pub const OPTIONAL_TYPE: u16 = 191;
    pub const REST_TYPE: u16 = 192;
    pub const UNION_TYPE: u16 = 193;
    pub const INTERSECTION_TYPE: u16 = 194;
    pub const CONDITIONAL_TYPE: u16 = 195;
    pub const INFER_TYPE: u16 = 196;
    pub const PARENTHESIZED_TYPE: u16 = 197;
    pub const THIS_TYPE: u16 = 198;
    pub const TYPE_OPERATOR: u16 = 199;
    pub const INDEXED_ACCESS_TYPE: u16 = 200;
    pub const MAPPED_TYPE: u16 = 201;
    pub const LITERAL_TYPE: u16 = 202;
    pub const NAMED_TUPLE_MEMBER: u16 = 203;
    pub const TEMPLATE_LITERAL_TYPE: u16 = 204;
    pub const TEMPLATE_LITERAL_TYPE_SPAN: u16 = 205;
    pub const IMPORT_TYPE: u16 = 206;

    // Binding patterns
    pub const OBJECT_BINDING_PATTERN: u16 = 207;
    pub const ARRAY_BINDING_PATTERN: u16 = 208;
    pub const BINDING_ELEMENT: u16 = 209;

    // Expression
    pub const ARRAY_LITERAL_EXPRESSION: u16 = 210;
    pub const OBJECT_LITERAL_EXPRESSION: u16 = 211;
    pub const PROPERTY_ACCESS_EXPRESSION: u16 = 212;
    pub const ELEMENT_ACCESS_EXPRESSION: u16 = 213;
    pub const CALL_EXPRESSION: u16 = 214;
    pub const NEW_EXPRESSION: u16 = 215;
    pub const TAGGED_TEMPLATE_EXPRESSION: u16 = 216;
    pub const TYPE_ASSERTION: u16 = 217;
    pub const PARENTHESIZED_EXPRESSION: u16 = 218;
    pub const FUNCTION_EXPRESSION: u16 = 219;
    pub const ARROW_FUNCTION: u16 = 220;
    pub const DELETE_EXPRESSION: u16 = 221;
    pub const TYPE_OF_EXPRESSION: u16 = 222;
    pub const VOID_EXPRESSION: u16 = 223;
    pub const AWAIT_EXPRESSION: u16 = 224;
    pub const PREFIX_UNARY_EXPRESSION: u16 = 225;
    pub const POSTFIX_UNARY_EXPRESSION: u16 = 226;
    pub const BINARY_EXPRESSION: u16 = 227;
    pub const CONDITIONAL_EXPRESSION: u16 = 228;
    pub const TEMPLATE_EXPRESSION: u16 = 229;
    pub const YIELD_EXPRESSION: u16 = 230;
    pub const SPREAD_ELEMENT: u16 = 231;
    pub const CLASS_EXPRESSION: u16 = 232;
    pub const OMITTED_EXPRESSION: u16 = 233;
    pub const EXPRESSION_WITH_TYPE_ARGUMENTS: u16 = 234;
    pub const AS_EXPRESSION: u16 = 235;
    pub const NON_NULL_EXPRESSION: u16 = 236;
    pub const META_PROPERTY: u16 = 237;
    pub const SYNTHETIC_EXPRESSION: u16 = 238;
    pub const SATISFIES_EXPRESSION: u16 = 239;

    // Misc
    pub const TEMPLATE_SPAN: u16 = 240;
    pub const SEMICOLON_CLASS_ELEMENT: u16 = 241;

    // Statements
    pub const BLOCK: u16 = 242;
    pub const EMPTY_STATEMENT: u16 = 243;
    pub const VARIABLE_STATEMENT: u16 = 244;
    pub const EXPRESSION_STATEMENT: u16 = 245;
    pub const IF_STATEMENT: u16 = 246;
    pub const DO_STATEMENT: u16 = 247;
    pub const WHILE_STATEMENT: u16 = 248;
    pub const FOR_STATEMENT: u16 = 249;
    pub const FOR_IN_STATEMENT: u16 = 250;
    pub const FOR_OF_STATEMENT: u16 = 251;
    pub const CONTINUE_STATEMENT: u16 = 252;
    pub const BREAK_STATEMENT: u16 = 253;
    pub const RETURN_STATEMENT: u16 = 254;
    pub const WITH_STATEMENT: u16 = 255;
    pub const SWITCH_STATEMENT: u16 = 256;
    pub const LABELED_STATEMENT: u16 = 257;
    pub const THROW_STATEMENT: u16 = 258;
    pub const TRY_STATEMENT: u16 = 259;
    pub const DEBUGGER_STATEMENT: u16 = 260;

    // Declarations
    pub const VARIABLE_DECLARATION: u16 = 261;
    pub const VARIABLE_DECLARATION_LIST: u16 = 262;
    pub const FUNCTION_DECLARATION: u16 = 263;
    pub const CLASS_DECLARATION: u16 = 264;
    pub const INTERFACE_DECLARATION: u16 = 265;
    pub const TYPE_ALIAS_DECLARATION: u16 = 266;
    pub const ENUM_DECLARATION: u16 = 267;
    pub const MODULE_DECLARATION: u16 = 268;
    pub const MODULE_BLOCK: u16 = 269;
    pub const CASE_BLOCK: u16 = 270;
    pub const NAMESPACE_EXPORT_DECLARATION: u16 = 271;
    pub const IMPORT_EQUALS_DECLARATION: u16 = 272;
    pub const IMPORT_DECLARATION: u16 = 273;
    pub const IMPORT_CLAUSE: u16 = 274;
    pub const NAMESPACE_IMPORT: u16 = 275;
    pub const NAMED_IMPORTS: u16 = 276;
    pub const IMPORT_SPECIFIER: u16 = 277;
    pub const EXPORT_ASSIGNMENT: u16 = 278;
    pub const EXPORT_DECLARATION: u16 = 279;
    pub const NAMED_EXPORTS: u16 = 280;
    pub const NAMESPACE_EXPORT: u16 = 281;
    pub const EXPORT_SPECIFIER: u16 = 282;
    pub const MISSING_DECLARATION: u16 = 283;

    // Module references
    pub const EXTERNAL_MODULE_REFERENCE: u16 = 284;

    // JSX
    pub const JSX_ELEMENT: u16 = 285;
    pub const JSX_SELF_CLOSING_ELEMENT: u16 = 286;
    pub const JSX_OPENING_ELEMENT: u16 = 287;
    pub const JSX_CLOSING_ELEMENT: u16 = 288;
    pub const JSX_FRAGMENT: u16 = 289;
    pub const JSX_OPENING_FRAGMENT: u16 = 290;
    pub const JSX_CLOSING_FRAGMENT: u16 = 291;
    pub const JSX_ATTRIBUTE: u16 = 292;
    pub const JSX_ATTRIBUTES: u16 = 293;
    pub const JSX_SPREAD_ATTRIBUTE: u16 = 294;
    pub const JSX_EXPRESSION: u16 = 295;
    pub const JSX_NAMESPACED_NAME: u16 = 296;

    // Clauses
    pub const CASE_CLAUSE: u16 = 297;
    pub const DEFAULT_CLAUSE: u16 = 298;
    pub const HERITAGE_CLAUSE: u16 = 299;
    pub const CATCH_CLAUSE: u16 = 300;
    pub const IMPORT_ATTRIBUTES: u16 = 301;
    pub const IMPORT_ATTRIBUTE: u16 = 302;

    // Property assignments
    pub const PROPERTY_ASSIGNMENT: u16 = 303;
    pub const SHORTHAND_PROPERTY_ASSIGNMENT: u16 = 304;
    pub const SPREAD_ASSIGNMENT: u16 = 305;

    // Enum
    pub const ENUM_MEMBER: u16 = 306;

    // Unparsed (for incremental)
    pub const UNPARSED_PROLOGUE: u16 = 307;

    // Top-level nodes
    pub const SOURCE_FILE: u16 = 308;
    pub const BUNDLE: u16 = 309;

    // First JSDoc node (310) ... we'll add these as needed
}
