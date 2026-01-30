//! Thin Node Architecture for Cache-Efficient AST
//!
//! This module implements a cache-optimized AST representation where each node
//! is exactly 16 bytes (4 nodes per 64-byte cache line), compared to the
//! previous 208-byte Node enum (0.31 nodes per cache line).
//!
//! # Architecture
//!
//! Instead of a single large enum, we use:
//! 1. `Node` - A 16-byte header containing kind, flags, position, and a data index
//! 2. Typed storage pools - Separate Vec<T> for each node category
//!
//! The `data_index` field points into the appropriate pool based on `kind`.
//!
//! # Performance Impact
//!
//! - **Before**: 208 bytes/node = 0.31 nodes/cache-line
//! - **After**: 16 bytes/node = 4 nodes/cache-line
//! - **Improvement**: 13x better cache locality for AST traversal
//!
//! # Design Principles
//!
//! 1. **Common data inline**: kind, flags, pos, end are accessed constantly
//! 2. **Rare data indirect**: modifiers, type parameters, etc. via index
//! 3. **No heap allocation per node**: All storage in arena vectors
//! 4. **O(1) node access**: Direct index into typed pool

use super::base::{NodeIndex, NodeList};
use crate::interner::{Atom, Interner};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// A thin 16-byte node header for cache-efficient AST storage.
///
/// Layout (16 bytes total):
/// - `kind`: 2 bytes (SyntaxKind value, supports 0-65535)
/// - `flags`: 2 bytes (packed NodeFlags)
/// - `pos`: 4 bytes (start position in source)
/// - `end`: 4 bytes (end position in source)
/// - `data_index`: 4 bytes (index into type-specific pool, u32::MAX = no data)
#[repr(C)]
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Node {
    /// SyntaxKind value (u16 to support extended kinds up to 400+)
    pub kind: u16,
    /// Packed node flags (subset of NodeFlags that fits in u16)
    pub flags: u16,
    /// Start position in source (character index)
    pub pos: u32,
    /// End position in source (character index)
    pub end: u32,
    /// Index into the type-specific storage pool (u32::MAX = no data)
    pub data_index: u32,
}

impl Node {
    pub const NO_DATA: u32 = u32::MAX;

    /// Create a new thin node with no associated data
    #[inline]
    pub fn new(kind: u16, pos: u32, end: u32) -> Node {
        Node {
            kind,
            flags: 0,
            pos,
            end,
            data_index: Self::NO_DATA,
        }
    }

    /// Create a new thin node with data index
    #[inline]
    pub fn with_data(kind: u16, pos: u32, end: u32, data_index: u32) -> Node {
        Node {
            kind,
            flags: 0,
            pos,
            end,
            data_index,
        }
    }

    /// Create a new thin node with data index and flags
    #[inline]
    pub fn with_data_and_flags(kind: u16, pos: u32, end: u32, data_index: u32, flags: u16) -> Node {
        Node {
            kind,
            flags,
            pos,
            end,
            data_index,
        }
    }

    /// Check if this node has associated data
    #[inline]
    pub fn has_data(&self) -> bool {
        self.data_index != Self::NO_DATA
    }
}

// =============================================================================
// Node Category Classification
// =============================================================================

/// Categories of nodes that share storage pools.
/// Nodes in the same category have similar data layouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeCategory {
    /// Simple tokens with no additional data (keywords, operators, etc.)
    Token,
    /// Identifiers with text data
    Identifier,
    /// String/numeric/regex literals with text
    Literal,
    /// Binary, unary, conditional expressions
    Expression,
    /// Function declarations and expressions
    Function,
    /// Class declarations
    Class,
    /// Statements (if, for, while, etc.)
    Statement,
    /// Type nodes (TypeReference, UnionType, etc.)
    TypeNode,
    /// Import/export declarations
    Module,
    /// JSX elements
    Jsx,
    /// Source file (only one per parse)
    SourceFile,
}

// =============================================================================
// Typed Data Pools
// =============================================================================

/// Data for identifier nodes (Identifier, PrivateIdentifier)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IdentifierData {
    /// Interned atom for O(1) comparison (OPTIMIZATION: use this instead of escaped_text)
    #[serde(skip, default = "Atom::none")]
    pub atom: Atom,
    /// The identifier text (DEPRECATED: kept for backward compatibility during migration)
    pub escaped_text: String,
    pub original_text: Option<String>,
    pub type_arguments: Option<NodeList>,
}

/// Data for string literals (StringLiteral, template parts)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LiteralData {
    pub text: String,
    pub raw_text: Option<String>,
    /// For numeric literals only
    pub value: Option<f64>,
}

/// Data for binary expressions
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BinaryExprData {
    pub left: NodeIndex,
    pub operator_token: u16, // SyntaxKind
    pub right: NodeIndex,
}

/// Data for unary expressions (prefix/postfix)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnaryExprData {
    pub operator: u16, // SyntaxKind
    pub operand: NodeIndex,
}

/// Data for call/new expressions
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CallExprData {
    pub expression: NodeIndex,
    pub type_arguments: Option<NodeList>,
    pub arguments: Option<NodeList>,
}

/// Data for property/element access
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccessExprData {
    pub expression: NodeIndex,
    pub name_or_argument: NodeIndex,
    pub question_dot_token: bool,
}

/// Data for function declarations/expressions/arrows
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunctionData {
    pub modifiers: Option<NodeList>,
    pub is_async: bool,       // Async function
    pub asterisk_token: bool, // Generator function
    pub name: NodeIndex,
    pub type_parameters: Option<NodeList>,
    pub parameters: NodeList,
    pub type_annotation: NodeIndex,
    pub body: NodeIndex,
    pub equals_greater_than_token: bool, // For arrows
}

/// Data for class declarations
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClassData {
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub type_parameters: Option<NodeList>,
    pub heritage_clauses: Option<NodeList>,
    pub members: NodeList,
}

/// Data for if statements
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IfStatementData {
    pub expression: NodeIndex,
    pub then_statement: NodeIndex,
    pub else_statement: NodeIndex,
}

/// Data for for/while/do loops
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoopData {
    pub initializer: NodeIndex,
    pub condition: NodeIndex,
    pub incrementor: NodeIndex,
    pub statement: NodeIndex,
}

/// Data for block statements
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockData {
    pub statements: NodeList,
    pub multi_line: bool,
}

/// Data for expression statements
#[derive(Clone, Copy, Debug)]
pub struct ExpressionStatementData {
    pub expression: NodeIndex,
}

/// Data for variable declarations
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VariableData {
    pub modifiers: Option<NodeList>,
    pub declarations: NodeList,
}

/// Data for type references
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TypeRefData {
    pub type_name: NodeIndex,
    pub type_arguments: Option<NodeList>,
}

/// Data for union/intersection types
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompositeTypeData {
    pub types: NodeList,
}

/// Data for conditional expressions (a ? b : c)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConditionalExprData {
    pub condition: NodeIndex,
    pub when_true: NodeIndex,
    pub when_false: NodeIndex,
}

/// Data for object/array literals
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LiteralExprData {
    pub elements: NodeList,
    pub multi_line: bool,
}

/// Data for parenthesized expressions
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParenthesizedData {
    pub expression: NodeIndex,
}

/// Data for spread/await/yield expressions
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnaryExprDataEx {
    pub expression: NodeIndex,
    pub asterisk_token: bool, // For yield*
}

/// Data for as/satisfies/type assertion expressions
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TypeAssertionData {
    pub expression: NodeIndex,
    pub type_node: NodeIndex,
}

/// Data for return/throw statements
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReturnData {
    pub expression: NodeIndex,
}

/// Data for expression statements
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExprStatementData {
    pub expression: NodeIndex,
}

/// Data for switch statements
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SwitchData {
    pub expression: NodeIndex,
    pub case_block: NodeIndex,
}

/// Data for case/default clauses
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CaseClauseData {
    pub expression: NodeIndex, // NONE for default clause
    pub statements: NodeList,
}

/// Data for try statements
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TryData {
    pub try_block: NodeIndex,
    pub catch_clause: NodeIndex,
    pub finally_block: NodeIndex,
}

/// Data for catch clauses
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CatchClauseData {
    pub variable_declaration: NodeIndex,
    pub block: NodeIndex,
}

/// Data for labeled statements
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LabeledData {
    pub label: NodeIndex,
    pub statement: NodeIndex,
}

/// Data for break/continue statements
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JumpData {
    pub label: NodeIndex,
}

/// Data for with statements
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WithData {
    pub expression: NodeIndex,
    pub statement: NodeIndex,
}

/// Data for interface declarations
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InterfaceData {
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub type_parameters: Option<NodeList>,
    pub heritage_clauses: Option<NodeList>,
    pub members: NodeList,
}

/// Data for type alias declarations
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TypeAliasData {
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub type_parameters: Option<NodeList>,
    pub type_node: NodeIndex,
}

/// Data for enum declarations
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnumData {
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub members: NodeList,
}

/// Data for enum members
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnumMemberData {
    pub name: NodeIndex,
    pub initializer: NodeIndex,
}

/// Data for module/namespace declarations
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModuleData {
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub body: NodeIndex,
}

/// Data for module blocks: { statements }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModuleBlockData {
    pub statements: Option<NodeList>,
}

/// Data for property/method signatures
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignatureData {
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub question_token: bool,
    pub type_parameters: Option<NodeList>,
    pub parameters: Option<NodeList>,
    pub type_annotation: NodeIndex,
}

/// Data for index signatures
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexSignatureData {
    pub modifiers: Option<NodeList>,
    pub parameters: NodeList,
    pub type_annotation: NodeIndex,
}

/// Data for property declarations
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PropertyDeclData {
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub question_token: bool,
    pub exclamation_token: bool,
    pub type_annotation: NodeIndex,
    pub initializer: NodeIndex,
}

/// Data for method declarations (class methods)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MethodDeclData {
    pub modifiers: Option<NodeList>,
    pub asterisk_token: bool,
    pub name: NodeIndex,
    pub question_token: bool,
    pub type_parameters: Option<NodeList>,
    pub parameters: NodeList,
    pub type_annotation: NodeIndex,
    pub body: NodeIndex,
}

/// Data for constructor declarations
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConstructorData {
    pub modifiers: Option<NodeList>,
    pub type_parameters: Option<NodeList>,
    pub parameters: NodeList,
    pub body: NodeIndex,
}

/// Data for accessor declarations (get/set)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccessorData {
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub type_parameters: Option<NodeList>,
    pub parameters: NodeList,
    pub type_annotation: NodeIndex,
    pub body: NodeIndex,
}

/// Data for parameter declarations
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParameterData {
    pub modifiers: Option<NodeList>,
    pub dot_dot_dot_token: bool,
    pub name: NodeIndex,
    pub question_token: bool,
    pub type_annotation: NodeIndex,
    pub initializer: NodeIndex,
}

/// Data for type parameter declarations
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TypeParameterData {
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub constraint: NodeIndex,
    pub default: NodeIndex,
}

/// Data for decorator nodes
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecoratorData {
    pub expression: NodeIndex,
}

/// Data for heritage clauses
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HeritageData {
    pub token: u16, // ExtendsKeyword or ImplementsKeyword
    pub types: NodeList,
}

/// Data for expression with type arguments
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExprWithTypeArgsData {
    pub expression: NodeIndex,
    pub type_arguments: Option<NodeList>,
}

/// Data for import declarations
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportDeclData {
    pub modifiers: Option<NodeList>,
    pub import_clause: NodeIndex,
    pub module_specifier: NodeIndex,
    pub attributes: NodeIndex,
}

/// Data for import clauses
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportClauseData {
    pub is_type_only: bool,
    pub name: NodeIndex,
    pub named_bindings: NodeIndex,
}

/// Data for namespace/named imports
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NamedImportsData {
    pub name: NodeIndex,    // For namespace import
    pub elements: NodeList, // For named imports
}

/// Data for import/export specifiers
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpecifierData {
    pub is_type_only: bool,
    pub property_name: NodeIndex,
    pub name: NodeIndex,
}

/// Data for export declarations
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExportDeclData {
    pub modifiers: Option<NodeList>,
    pub is_type_only: bool,
    /// True if this is `export default ...`
    pub is_default_export: bool,
    pub export_clause: NodeIndex,
    pub module_specifier: NodeIndex,
    pub attributes: NodeIndex,
}

/// Data for export assignments
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExportAssignmentData {
    pub modifiers: Option<NodeList>,
    pub is_export_equals: bool,
    pub expression: NodeIndex,
}

/// Data for import attributes
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportAttributesData {
    pub token: u16,
    pub elements: NodeList,
    pub multi_line: bool,
}

/// Data for import attribute
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportAttributeData {
    pub name: NodeIndex,
    pub value: NodeIndex,
}

/// Data for binding patterns
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BindingPatternData {
    pub elements: NodeList,
}

/// Data for binding elements
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BindingElementData {
    pub dot_dot_dot_token: bool,
    pub property_name: NodeIndex,
    pub name: NodeIndex,
    pub initializer: NodeIndex,
}

/// Data for property assignments
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PropertyAssignmentData {
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub initializer: NodeIndex,
}

/// Data for shorthand property assignments
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShorthandPropertyData {
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub equals_token: bool,
    pub object_assignment_initializer: NodeIndex,
}

/// Data for spread assignments
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpreadData {
    pub expression: NodeIndex,
}

/// Data for variable declarations (individual)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VariableDeclarationData {
    pub name: NodeIndex,            // Identifier or BindingPattern
    pub exclamation_token: bool,    // Definite assignment assertion
    pub type_annotation: NodeIndex, // TypeNode (optional)
    pub initializer: NodeIndex,     // Expression (optional)
}

/// Data for for-in/for-of statements
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ForInOfData {
    pub await_modifier: bool,   // For for-await-of
    pub initializer: NodeIndex, // Variable declaration or expression
    pub expression: NodeIndex,  // The iterable expression
    pub statement: NodeIndex,   // The loop body
}

/// Data for debugger/empty statements (no data needed, use token)

/// Data for template expressions
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemplateExprData {
    pub head: NodeIndex,
    pub template_spans: NodeList,
}

/// Data for template spans
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemplateSpanData {
    pub expression: NodeIndex,
    pub literal: NodeIndex,
}

/// Data for tagged template expressions
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaggedTemplateData {
    pub tag: NodeIndex,
    pub type_arguments: Option<NodeList>,
    pub template: NodeIndex,
}

/// Data for qualified names
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QualifiedNameData {
    pub left: NodeIndex,
    pub right: NodeIndex,
}

/// Data for computed property names
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComputedPropertyData {
    pub expression: NodeIndex,
}

/// Data for type nodes (function type, constructor type)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunctionTypeData {
    pub type_parameters: Option<NodeList>,
    pub parameters: NodeList,
    pub type_annotation: NodeIndex,
    /// True if this is an abstract constructor type: `abstract new () => T`
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_abstract: bool,
}

/// Data for type query (typeof)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TypeQueryData {
    pub expr_name: NodeIndex,
    pub type_arguments: Option<NodeList>,
}

/// Data for type literal
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TypeLiteralData {
    pub members: NodeList,
}

/// Data for array type
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArrayTypeData {
    pub element_type: NodeIndex,
}

/// Data for tuple type
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TupleTypeData {
    pub elements: NodeList,
}

/// Data for optional/rest types
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WrappedTypeData {
    pub type_node: NodeIndex,
}

/// Data for conditional types
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConditionalTypeData {
    pub check_type: NodeIndex,
    pub extends_type: NodeIndex,
    pub true_type: NodeIndex,
    pub false_type: NodeIndex,
}

/// Data for infer type
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InferTypeData {
    pub type_parameter: NodeIndex,
}

/// Data for type operator (keyof, unique, readonly)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TypeOperatorData {
    pub operator: u16,
    pub type_node: NodeIndex,
}

/// Data for indexed access type
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexedAccessTypeData {
    pub object_type: NodeIndex,
    pub index_type: NodeIndex,
}

/// Data for mapped type
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MappedTypeData {
    pub readonly_token: NodeIndex,
    pub type_parameter: NodeIndex,
    pub name_type: NodeIndex,
    pub question_token: NodeIndex,
    pub type_node: NodeIndex,
    pub members: Option<NodeList>,
}

/// Data for literal types
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LiteralTypeData {
    pub literal: NodeIndex,
}

/// Data for template literal types
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemplateLiteralTypeData {
    pub head: NodeIndex,
    pub template_spans: NodeList,
}

/// Data for named tuple member
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NamedTupleMemberData {
    pub dot_dot_dot_token: bool,
    pub name: NodeIndex,
    pub question_token: bool,
    pub type_node: NodeIndex,
}

/// Data for type predicate
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TypePredicateData {
    pub asserts_modifier: bool,
    pub parameter_name: NodeIndex,
    pub type_node: NodeIndex,
}

/// Data for JSX elements
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsxElementData {
    pub opening_element: NodeIndex,
    pub children: NodeList,
    pub closing_element: NodeIndex,
}

/// Data for JSX self-closing/opening elements
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsxOpeningData {
    pub tag_name: NodeIndex,
    pub type_arguments: Option<NodeList>,
    pub attributes: NodeIndex,
}

/// Data for JSX closing elements
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsxClosingData {
    pub tag_name: NodeIndex,
}

/// Data for JSX fragments
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsxFragmentData {
    pub opening_fragment: NodeIndex,
    pub children: NodeList,
    pub closing_fragment: NodeIndex,
}

/// Data for JSX attributes
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsxAttributesData {
    pub properties: NodeList,
}

/// Data for JSX attribute
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsxAttributeData {
    pub name: NodeIndex,
    pub initializer: NodeIndex,
}

/// Data for JSX spread attribute
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsxSpreadAttributeData {
    pub expression: NodeIndex,
}

/// Data for JSX expression
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsxExpressionData {
    pub dot_dot_dot_token: bool,
    pub expression: NodeIndex,
}

/// Data for JSX text
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsxTextData {
    pub text: String,
    pub contains_only_trivia_white_spaces: bool,
}

/// Data for JSX namespaced name
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsxNamespacedNameData {
    pub namespace: NodeIndex,
    pub name: NodeIndex,
}

/// Data for source files
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceFileData {
    pub statements: NodeList,
    pub end_of_file_token: NodeIndex,
    pub file_name: String,
    /// Source text. Uses custom serialization to handle Arc<str> properly.
    #[serde(
        serialize_with = "serialize_arc_str",
        deserialize_with = "deserialize_arc_str"
    )]
    pub text: Arc<str>,
    pub language_version: u32,
    pub language_variant: u32,
    pub script_kind: u32,
    pub is_declaration_file: bool,
    pub has_no_default_lib: bool,
    /// Cached comment ranges for the entire file (computed once during parsing).
    /// This avoids O(N) rescanning on every hover/documentation request.
    pub comments: Vec<crate::comments::CommentRange>,
    // Extended node info (parent, id, modifiers, transform_flags)
    pub parent: NodeIndex,
    pub id: u32,
    pub modifier_flags: u32,
    pub transform_flags: u32,
}

/// Serialize Arc<str> as a regular string
fn serialize_arc_str<S>(arc: &Arc<str>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(arc)
}

/// Deserialize Arc<str> from a string
fn deserialize_arc_str<'de, D>(deserializer: D) -> Result<Arc<str>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    let s = String::deserialize(deserializer)?;
    Ok(Arc::from(s))
}

// =============================================================================
// Thin Node Arena
// =============================================================================

/// Arena for thin nodes with typed data pools.
/// Provides O(1) allocation and cache-efficient storage.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NodeArena {
    /// The thin node headers (16 bytes each)
    pub nodes: Vec<Node>,

    /// String interner for resolving identifier atoms
    /// This is populated from the scanner after parsing completes
    #[serde(skip)]
    pub interner: Interner,

    // ==========================================================================
    // Typed data pools - organized by category
    // ==========================================================================

    // Names and identifiers
    pub identifiers: Vec<IdentifierData>,
    pub qualified_names: Vec<QualifiedNameData>,
    pub computed_properties: Vec<ComputedPropertyData>,

    // Literals
    pub literals: Vec<LiteralData>,

    // Expressions
    pub binary_exprs: Vec<BinaryExprData>,
    pub unary_exprs: Vec<UnaryExprData>,
    pub call_exprs: Vec<CallExprData>,
    pub access_exprs: Vec<AccessExprData>,
    pub conditional_exprs: Vec<ConditionalExprData>,
    pub literal_exprs: Vec<LiteralExprData>,
    pub parenthesized: Vec<ParenthesizedData>,
    pub unary_exprs_ex: Vec<UnaryExprDataEx>,
    pub type_assertions: Vec<TypeAssertionData>,
    pub template_exprs: Vec<TemplateExprData>,
    pub template_spans: Vec<TemplateSpanData>,
    pub tagged_templates: Vec<TaggedTemplateData>,

    // Functions and classes
    pub functions: Vec<FunctionData>,
    pub classes: Vec<ClassData>,
    pub interfaces: Vec<InterfaceData>,
    pub type_aliases: Vec<TypeAliasData>,
    pub enums: Vec<EnumData>,
    pub enum_members: Vec<EnumMemberData>,
    pub modules: Vec<ModuleData>,
    pub module_blocks: Vec<ModuleBlockData>,

    // Signatures and members
    pub signatures: Vec<SignatureData>,
    pub index_signatures: Vec<IndexSignatureData>,
    pub property_decls: Vec<PropertyDeclData>,
    pub method_decls: Vec<MethodDeclData>,
    pub constructors: Vec<ConstructorData>,
    pub accessors: Vec<AccessorData>,
    pub parameters: Vec<ParameterData>,
    pub type_parameters: Vec<TypeParameterData>,
    pub decorators: Vec<DecoratorData>,
    pub heritage_clauses: Vec<HeritageData>,
    pub expr_with_type_args: Vec<ExprWithTypeArgsData>,

    // Statements
    pub if_statements: Vec<IfStatementData>,
    pub loops: Vec<LoopData>,
    pub blocks: Vec<BlockData>,
    pub variables: Vec<VariableData>,
    pub return_data: Vec<ReturnData>,
    pub expr_statements: Vec<ExprStatementData>,
    pub switch_data: Vec<SwitchData>,
    pub case_clauses: Vec<CaseClauseData>,
    pub try_data: Vec<TryData>,
    pub catch_clauses: Vec<CatchClauseData>,
    pub labeled_data: Vec<LabeledData>,
    pub jump_data: Vec<JumpData>,
    pub with_data: Vec<WithData>,

    // Types
    pub type_refs: Vec<TypeRefData>,
    pub composite_types: Vec<CompositeTypeData>,
    pub function_types: Vec<FunctionTypeData>,
    pub type_queries: Vec<TypeQueryData>,
    pub type_literals: Vec<TypeLiteralData>,
    pub array_types: Vec<ArrayTypeData>,
    pub tuple_types: Vec<TupleTypeData>,
    pub wrapped_types: Vec<WrappedTypeData>,
    pub conditional_types: Vec<ConditionalTypeData>,
    pub infer_types: Vec<InferTypeData>,
    pub type_operators: Vec<TypeOperatorData>,
    pub indexed_access_types: Vec<IndexedAccessTypeData>,
    pub mapped_types: Vec<MappedTypeData>,
    pub literal_types: Vec<LiteralTypeData>,
    pub template_literal_types: Vec<TemplateLiteralTypeData>,
    pub named_tuple_members: Vec<NamedTupleMemberData>,
    pub type_predicates: Vec<TypePredicateData>,

    // Import/export
    pub import_decls: Vec<ImportDeclData>,
    pub import_clauses: Vec<ImportClauseData>,
    pub named_imports: Vec<NamedImportsData>,
    pub specifiers: Vec<SpecifierData>,
    pub export_decls: Vec<ExportDeclData>,
    pub export_assignments: Vec<ExportAssignmentData>,
    pub import_attributes: Vec<ImportAttributesData>,
    pub import_attribute: Vec<ImportAttributeData>,

    // Binding patterns
    pub binding_patterns: Vec<BindingPatternData>,
    pub binding_elements: Vec<BindingElementData>,

    // Object literal members
    pub property_assignments: Vec<PropertyAssignmentData>,
    pub shorthand_properties: Vec<ShorthandPropertyData>,
    pub spread_data: Vec<SpreadData>,

    // Variable declarations (individual)
    pub variable_declarations: Vec<VariableDeclarationData>,

    // For-in/for-of
    pub for_in_of: Vec<ForInOfData>,

    // JSX
    pub jsx_elements: Vec<JsxElementData>,
    pub jsx_opening: Vec<JsxOpeningData>,
    pub jsx_closing: Vec<JsxClosingData>,
    pub jsx_fragments: Vec<JsxFragmentData>,
    pub jsx_attributes: Vec<JsxAttributesData>,
    pub jsx_attribute: Vec<JsxAttributeData>,
    pub jsx_spread_attributes: Vec<JsxSpreadAttributeData>,
    pub jsx_expressions: Vec<JsxExpressionData>,
    pub jsx_text: Vec<JsxTextData>,
    pub jsx_namespaced_names: Vec<JsxNamespacedNameData>,

    // Source file
    pub source_files: Vec<SourceFileData>,

    // Extended node info (for nodes that need parent, id, full flags)
    pub extended_info: Vec<ExtendedNodeInfo>,
}

/// Extended node info for nodes that need more than what fits in Node
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtendedNodeInfo {
    pub parent: NodeIndex,
    pub id: u32,
    pub modifier_flags: u32,
    pub transform_flags: u32,
}

impl Default for ExtendedNodeInfo {
    fn default() -> Self {
        ExtendedNodeInfo {
            parent: NodeIndex::NONE,
            id: 0,
            modifier_flags: 0,
            transform_flags: 0,
        }
    }
}

// Re-export types from node_access module for backward compatibility
pub use super::node_access::{NodeAccess, NodeInfo, NodeView};

#[cfg(test)]
#[path = "node_tests.rs"]
mod node_tests;
