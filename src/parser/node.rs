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

impl NodeArena {
    /// Maximum pre-allocation to avoid capacity overflow in huge files.
    const MAX_NODE_PREALLOC: usize = 5_000_000;
    pub fn new() -> NodeArena {
        NodeArena::default()
    }

    /// Create an arena with pre-allocated capacity.
    /// Uses heuristic ratios based on typical TypeScript AST composition.
    pub fn with_capacity(capacity: usize) -> NodeArena {
        let safe_capacity = capacity.min(Self::MAX_NODE_PREALLOC);
        // Use Default for all the new pools, just set capacity for main ones
        let mut arena = NodeArena::default();

        // Pre-allocate the most commonly used pools
        arena.nodes = Vec::with_capacity(safe_capacity);
        arena.extended_info = Vec::with_capacity(safe_capacity);
        arena.identifiers = Vec::with_capacity(safe_capacity / 4); // ~25% identifiers
        arena.literals = Vec::with_capacity(safe_capacity / 8); // ~12% literals
        arena.binary_exprs = Vec::with_capacity(safe_capacity / 8); // ~12% binary
        arena.call_exprs = Vec::with_capacity(safe_capacity / 8); // ~12% calls
        arena.access_exprs = Vec::with_capacity(safe_capacity / 8); // ~12% property access
        arena.blocks = Vec::with_capacity(safe_capacity / 8); // ~12% blocks
        arena.variables = Vec::with_capacity(safe_capacity / 16); // ~6% variables
        arena.functions = Vec::with_capacity(safe_capacity / 16); // ~6% functions
        arena.type_refs = Vec::with_capacity(safe_capacity / 8); // ~12% type refs
        arena.source_files = Vec::with_capacity(1); // Usually 1

        arena
    }

    pub fn clear(&mut self) {
        macro_rules! clear_vecs {
            ($($field:ident),+ $(,)?) => {
                $(self.$field.clear();)+
            };
        }

        clear_vecs!(
            nodes,
            identifiers,
            qualified_names,
            computed_properties,
            literals,
            binary_exprs,
            unary_exprs,
            call_exprs,
            access_exprs,
            conditional_exprs,
            literal_exprs,
            parenthesized,
            unary_exprs_ex,
            type_assertions,
            template_exprs,
            template_spans,
            tagged_templates,
            functions,
            classes,
            interfaces,
            type_aliases,
            enums,
            enum_members,
            modules,
            module_blocks,
            signatures,
            index_signatures,
            property_decls,
            method_decls,
            constructors,
            accessors,
            parameters,
            type_parameters,
            decorators,
            heritage_clauses,
            expr_with_type_args,
            if_statements,
            loops,
            blocks,
            variables,
            return_data,
            expr_statements,
            switch_data,
            case_clauses,
            try_data,
            catch_clauses,
            labeled_data,
            jump_data,
            with_data,
            type_refs,
            composite_types,
            function_types,
            type_queries,
            type_literals,
            array_types,
            tuple_types,
            wrapped_types,
            conditional_types,
            infer_types,
            type_operators,
            indexed_access_types,
            mapped_types,
            literal_types,
            template_literal_types,
            named_tuple_members,
            type_predicates,
            import_decls,
            import_clauses,
            named_imports,
            specifiers,
            export_decls,
            export_assignments,
            import_attributes,
            import_attribute,
            binding_patterns,
            binding_elements,
            property_assignments,
            shorthand_properties,
            spread_data,
            variable_declarations,
            for_in_of,
            jsx_elements,
            jsx_opening,
            jsx_closing,
            jsx_fragments,
            jsx_attributes,
            jsx_attribute,
            jsx_spread_attributes,
            jsx_expressions,
            jsx_text,
            jsx_namespaced_names,
            source_files,
            extended_info,
        );
    }

    // ============================================================================
    // Parent Mapping Helpers
    // ============================================================================

    /// Set the parent for a single child node.
    /// This is called during node creation to maintain parent pointers.
    #[inline]
    fn set_parent(&mut self, child: NodeIndex, parent: NodeIndex) {
        if !child.is_none() {
            // Safety: child index is guaranteed to be valid and < current index
            // because we build bottom-up (children are created before parents).
            if let Some(info) = self.extended_info.get_mut(child.0 as usize) {
                info.parent = parent;
            }
        }
    }

    /// Set the parent for a list of children.
    #[inline]
    fn set_parent_list(&mut self, list: &NodeList, parent: NodeIndex) {
        for &child in &list.nodes {
            self.set_parent(child, parent);
        }
    }

    /// Set the parent for an optional list of children.
    #[inline]
    fn set_parent_opt_list(&mut self, list: &Option<NodeList>, parent: NodeIndex) {
        if let Some(l) = list {
            self.set_parent_list(l, parent);
        }
    }

    // ============================================================================
    // Node Creation Methods
    // ============================================================================

    /// Add a token node (no additional data)
    pub fn add_token(&mut self, kind: u16, pos: u32, end: u32) -> NodeIndex {
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::new(kind, pos, end));
        self.extended_info.push(ExtendedNodeInfo::default());
        NodeIndex(index)
    }

    /// Create a modifier token (static, public, private, etc.)
    pub fn create_modifier(&mut self, kind: crate::scanner::SyntaxKind, pos: u32) -> NodeIndex {
        // Modifiers are simple tokens, their kind IS the modifier type
        // End position is pos + keyword length
        let end = pos
            + match kind {
                crate::scanner::SyntaxKind::StaticKeyword => 6, // "static"
                crate::scanner::SyntaxKind::PublicKeyword => 6, // "public"
                crate::scanner::SyntaxKind::PrivateKeyword => 7, // "private"
                crate::scanner::SyntaxKind::ProtectedKeyword => 9, // "protected"
                crate::scanner::SyntaxKind::ReadonlyKeyword => 8, // "readonly"
                crate::scanner::SyntaxKind::AbstractKeyword => 8, // "abstract"
                crate::scanner::SyntaxKind::OverrideKeyword => 8, // "override"
                crate::scanner::SyntaxKind::AsyncKeyword => 5,  // "async"
                crate::scanner::SyntaxKind::DeclareKeyword => 7, // "declare"
                crate::scanner::SyntaxKind::ExportKeyword => 6, // "export"
                crate::scanner::SyntaxKind::DefaultKeyword => 7, // "default"
                crate::scanner::SyntaxKind::ConstKeyword => 5,  // "const"
                _ => 0,
            };
        self.add_token(kind as u16, pos, end)
    }

    /// Add an identifier node
    pub fn add_identifier(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: IdentifierData,
    ) -> NodeIndex {
        let data_index = self.identifiers.len() as u32;
        self.identifiers.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        NodeIndex(index)
    }

    /// Add a literal node
    pub fn add_literal(&mut self, kind: u16, pos: u32, end: u32, data: LiteralData) -> NodeIndex {
        let data_index = self.literals.len() as u32;
        self.literals.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        NodeIndex(index)
    }

    /// Add a binary expression
    pub fn add_binary_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: BinaryExprData,
    ) -> NodeIndex {
        let left = data.left;
        let right = data.right;

        let data_index = self.binary_exprs.len() as u32;
        self.binary_exprs.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(left, parent);
        self.set_parent(right, parent);

        parent
    }

    /// Add a call expression
    pub fn add_call_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: CallExprData,
    ) -> NodeIndex {
        let expression = data.expression;
        let type_arguments = data.type_arguments.clone();
        let arguments = data.arguments.clone();

        let data_index = self.call_exprs.len() as u32;
        self.call_exprs.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent_opt_list(&type_arguments, parent);
        self.set_parent_opt_list(&arguments, parent);

        parent
    }

    /// Add a function node
    pub fn add_function(&mut self, kind: u16, pos: u32, end: u32, data: FunctionData) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let type_parameters = data.type_parameters.clone();
        let parameters = data.parameters.clone();
        let type_annotation = data.type_annotation;
        let body = data.body;

        let data_index = self.functions.len() as u32;
        self.functions.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(&modifiers, parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(&type_parameters, parent);
        self.set_parent_list(&parameters, parent);
        self.set_parent(type_annotation, parent);
        self.set_parent(body, parent);

        parent
    }

    /// Add a class node
    pub fn add_class(&mut self, kind: u16, pos: u32, end: u32, data: ClassData) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let type_parameters = data.type_parameters.clone();
        let heritage_clauses = data.heritage_clauses.clone();
        let members = data.members.clone();

        let data_index = self.classes.len() as u32;
        self.classes.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(&modifiers, parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(&type_parameters, parent);
        self.set_parent_opt_list(&heritage_clauses, parent);
        self.set_parent_list(&members, parent);

        parent
    }

    /// Add a block node
    pub fn add_block(&mut self, kind: u16, pos: u32, end: u32, data: BlockData) -> NodeIndex {
        let statements = data.statements.clone();

        let data_index = self.blocks.len() as u32;
        self.blocks.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_list(&statements, parent);

        parent
    }

    /// Add a source file node
    pub fn add_source_file(&mut self, pos: u32, end: u32, data: SourceFileData) -> NodeIndex {
        use super::syntax_kind_ext::SOURCE_FILE;
        let statements = data.statements.clone();
        let end_of_file_token = data.end_of_file_token;

        let data_index = self.source_files.len() as u32;
        self.source_files.push(data);
        let index = self.nodes.len() as u32;
        self.nodes
            .push(Node::with_data(SOURCE_FILE, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_list(&statements, parent);
        self.set_parent(end_of_file_token, parent);

        parent
    }

    // ==========================================================================
    // Additional add_* methods for all data pools
    // ==========================================================================

    /// Add a qualified name node
    pub fn add_qualified_name(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: QualifiedNameData,
    ) -> NodeIndex {
        let left = data.left;
        let right = data.right;

        let data_index = self.qualified_names.len() as u32;
        self.qualified_names.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(left, parent);
        self.set_parent(right, parent);

        parent
    }

    /// Add a computed property name node
    pub fn add_computed_property(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ComputedPropertyData,
    ) -> NodeIndex {
        let expression = data.expression;

        let data_index = self.computed_properties.len() as u32;
        self.computed_properties.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        parent
    }

    /// Add a unary expression node
    pub fn add_unary_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: UnaryExprData,
    ) -> NodeIndex {
        let operand = data.operand;

        let data_index = self.unary_exprs.len() as u32;
        self.unary_exprs.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(operand, parent);

        parent
    }

    /// Add a property/element access expression node
    pub fn add_access_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: AccessExprData,
    ) -> NodeIndex {
        let expression = data.expression;
        let name_or_argument = data.name_or_argument;

        let data_index = self.access_exprs.len() as u32;
        self.access_exprs.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent(name_or_argument, parent);

        parent
    }

    /// Add a conditional expression node (a ? b : c)
    pub fn add_conditional_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ConditionalExprData,
    ) -> NodeIndex {
        let condition = data.condition;
        let when_true = data.when_true;
        let when_false = data.when_false;

        let data_index = self.conditional_exprs.len() as u32;
        self.conditional_exprs.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(condition, parent);
        self.set_parent(when_true, parent);
        self.set_parent(when_false, parent);
        parent
    }

    /// Add an object/array literal expression node
    pub fn add_literal_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: LiteralExprData,
    ) -> NodeIndex {
        let elements = data.elements.clone();

        let data_index = self.literal_exprs.len() as u32;
        self.literal_exprs.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_list(&elements, parent);
        parent
    }

    /// Add a parenthesized expression node
    pub fn add_parenthesized(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ParenthesizedData,
    ) -> NodeIndex {
        let expression = data.expression;
        let data_index = self.parenthesized.len() as u32;
        self.parenthesized.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        parent
    }

    /// Add a spread/await/yield expression node
    pub fn add_unary_expr_ex(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: UnaryExprDataEx,
    ) -> NodeIndex {
        let expression = data.expression;
        let data_index = self.unary_exprs_ex.len() as u32;
        self.unary_exprs_ex.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        parent
    }

    /// Add a type assertion expression node
    pub fn add_type_assertion(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypeAssertionData,
    ) -> NodeIndex {
        let data_index = self.type_assertions.len() as u32;
        self.type_assertions.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        NodeIndex(index)
    }

    /// Add a template expression node
    pub fn add_template_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TemplateExprData,
    ) -> NodeIndex {
        let head = data.head;
        let template_spans = data.template_spans.clone();

        let data_index = self.template_exprs.len() as u32;
        self.template_exprs.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(head, parent);
        self.set_parent_list(&template_spans, parent);

        parent
    }

    /// Add a template span node
    pub fn add_template_span(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TemplateSpanData,
    ) -> NodeIndex {
        let expression = data.expression;
        let literal = data.literal;

        let data_index = self.template_spans.len() as u32;
        self.template_spans.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent(literal, parent);

        parent
    }

    /// Add a tagged template expression node
    pub fn add_tagged_template(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TaggedTemplateData,
    ) -> NodeIndex {
        let tag = data.tag;
        let type_arguments = data.type_arguments.clone();
        let template = data.template;

        let data_index = self.tagged_templates.len() as u32;
        self.tagged_templates.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(tag, parent);
        self.set_parent_opt_list(&type_arguments, parent);
        self.set_parent(template, parent);

        parent
    }

    /// Add an interface declaration node
    pub fn add_interface(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: InterfaceData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let type_parameters = data.type_parameters.clone();
        let heritage_clauses = data.heritage_clauses.clone();
        let members = data.members.clone();

        let data_index = self.interfaces.len() as u32;
        self.interfaces.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(&modifiers, parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(&type_parameters, parent);
        self.set_parent_opt_list(&heritage_clauses, parent);
        self.set_parent_list(&members, parent);

        parent
    }

    /// Add a type alias declaration node
    pub fn add_type_alias(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypeAliasData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let type_parameters = data.type_parameters.clone();
        let type_node = data.type_node;

        let data_index = self.type_aliases.len() as u32;
        self.type_aliases.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(&modifiers, parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(&type_parameters, parent);
        self.set_parent(type_node, parent);

        parent
    }

    /// Add an enum declaration node
    pub fn add_enum(&mut self, kind: u16, pos: u32, end: u32, data: EnumData) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let members = data.members.clone();

        let data_index = self.enums.len() as u32;
        self.enums.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(&modifiers, parent);
        self.set_parent(name, parent);
        self.set_parent_list(&members, parent);

        parent
    }

    /// Add an enum member node
    pub fn add_enum_member(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: EnumMemberData,
    ) -> NodeIndex {
        let name = data.name;
        let initializer = data.initializer;

        let data_index = self.enum_members.len() as u32;
        self.enum_members.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(name, parent);
        self.set_parent(initializer, parent);

        parent
    }

    /// Add a module declaration node
    pub fn add_module(&mut self, kind: u16, pos: u32, end: u32, data: ModuleData) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let body = data.body;

        let data_index = self.modules.len() as u32;
        self.modules.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(&modifiers, parent);
        self.set_parent(name, parent);
        self.set_parent(body, parent);

        parent
    }

    /// Add a module block node: { statements }
    pub fn add_module_block(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ModuleBlockData,
    ) -> NodeIndex {
        let statements = data.statements.clone();

        let data_index = self.module_blocks.len() as u32;
        self.module_blocks.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(&statements, parent);

        parent
    }

    /// Add a signature node (property/method signature)
    pub fn add_signature(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: SignatureData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let type_parameters = data.type_parameters.clone();
        let parameters = data.parameters.clone();
        let type_annotation = data.type_annotation;

        let data_index = self.signatures.len() as u32;
        self.signatures.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(&modifiers, parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(&type_parameters, parent);
        self.set_parent_opt_list(&parameters, parent);
        self.set_parent(type_annotation, parent);

        parent
    }

    /// Add an index signature node
    pub fn add_index_signature(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: IndexSignatureData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let parameters = data.parameters.clone();
        let type_annotation = data.type_annotation;

        let data_index = self.index_signatures.len() as u32;
        self.index_signatures.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(&modifiers, parent);
        self.set_parent_list(&parameters, parent);
        self.set_parent(type_annotation, parent);

        parent
    }

    /// Add a property declaration node
    pub fn add_property_decl(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: PropertyDeclData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let type_annotation = data.type_annotation;
        let initializer = data.initializer;

        let data_index = self.property_decls.len() as u32;
        self.property_decls.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_opt_list(&modifiers, parent);
        self.set_parent(name, parent);
        self.set_parent(type_annotation, parent);
        self.set_parent(initializer, parent);
        parent
    }

    /// Add a method declaration node
    pub fn add_method_decl(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: MethodDeclData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let type_parameters = data.type_parameters.clone();
        let parameters = data.parameters.clone();
        let type_annotation = data.type_annotation;
        let body = data.body;

        let data_index = self.method_decls.len() as u32;
        self.method_decls.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_opt_list(&modifiers, parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(&type_parameters, parent);
        self.set_parent_list(&parameters, parent);
        self.set_parent(type_annotation, parent);
        self.set_parent(body, parent);
        parent
    }

    /// Add a constructor declaration node
    pub fn add_constructor(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ConstructorData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let type_parameters = data.type_parameters.clone();
        let parameters = data.parameters.clone();
        let body = data.body;

        let data_index = self.constructors.len() as u32;
        self.constructors.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_opt_list(&modifiers, parent);
        self.set_parent_opt_list(&type_parameters, parent);
        self.set_parent_list(&parameters, parent);
        self.set_parent(body, parent);
        parent
    }

    /// Add an accessor declaration node (get/set)
    pub fn add_accessor(&mut self, kind: u16, pos: u32, end: u32, data: AccessorData) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let type_parameters = data.type_parameters.clone();
        let parameters = data.parameters.clone();
        let type_annotation = data.type_annotation;
        let body = data.body;

        let data_index = self.accessors.len() as u32;
        self.accessors.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(&modifiers, parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(&type_parameters, parent);
        self.set_parent_list(&parameters, parent);
        self.set_parent(type_annotation, parent);
        self.set_parent(body, parent);

        parent
    }

    /// Add a parameter declaration node
    pub fn add_parameter(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ParameterData,
    ) -> NodeIndex {
        let name = data.name;
        let type_annotation = data.type_annotation;
        let initializer = data.initializer;
        let modifiers = data.modifiers.clone();
        let data_index = self.parameters.len() as u32;
        self.parameters.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        // Set parent pointers for children
        self.set_parent(name, parent);
        self.set_parent(type_annotation, parent);
        self.set_parent(initializer, parent);
        self.set_parent_opt_list(&modifiers, parent);
        parent
    }

    /// Add a type parameter declaration node
    pub fn add_type_parameter(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypeParameterData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let constraint = data.constraint;
        let default = data.default;

        let data_index = self.type_parameters.len() as u32;
        self.type_parameters.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(&modifiers, parent);
        self.set_parent(name, parent);
        self.set_parent(constraint, parent);
        self.set_parent(default, parent);

        parent
    }

    /// Add a decorator node
    pub fn add_decorator(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: DecoratorData,
    ) -> NodeIndex {
        let data_index = self.decorators.len() as u32;
        self.decorators.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        NodeIndex(index)
    }

    /// Add a heritage clause node
    pub fn add_heritage(&mut self, kind: u16, pos: u32, end: u32, data: HeritageData) -> NodeIndex {
        let types = data.types.clone();
        let data_index = self.heritage_clauses.len() as u32;
        self.heritage_clauses.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_list(&types, parent);
        parent
    }

    /// Add an expression with type arguments node
    pub fn add_expr_with_type_args(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ExprWithTypeArgsData,
    ) -> NodeIndex {
        let expression = data.expression;
        let type_arguments = data.type_arguments.clone();
        let data_index = self.expr_with_type_args.len() as u32;
        self.expr_with_type_args.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent_opt_list(&type_arguments, parent);
        parent
    }

    /// Add an if statement node
    pub fn add_if_statement(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: IfStatementData,
    ) -> NodeIndex {
        let expression = data.expression;
        let then_statement = data.then_statement;
        let else_statement = data.else_statement;

        let data_index = self.if_statements.len() as u32;
        self.if_statements.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent(then_statement, parent);
        self.set_parent(else_statement, parent);

        parent
    }

    /// Add a loop node (for/while/do)
    pub fn add_loop(&mut self, kind: u16, pos: u32, end: u32, data: LoopData) -> NodeIndex {
        let initializer = data.initializer;
        let condition = data.condition;
        let incrementor = data.incrementor;
        let statement = data.statement;
        let data_index = self.loops.len() as u32;
        self.loops.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(initializer, parent);
        self.set_parent(condition, parent);
        self.set_parent(incrementor, parent);
        self.set_parent(statement, parent);
        parent
    }

    /// Add a variable statement/declaration list node
    pub fn add_variable(&mut self, kind: u16, pos: u32, end: u32, data: VariableData) -> NodeIndex {
        self.add_variable_with_flags(kind, pos, end, data, 0)
    }

    /// Add a variable statement/declaration list node with flags
    pub fn add_variable_with_flags(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: VariableData,
        flags: u16,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let declarations = data.declarations.clone();

        let data_index = self.variables.len() as u32;
        self.variables.push(data);
        let index = self.nodes.len() as u32;
        self.nodes
            .push(Node::with_data_and_flags(kind, pos, end, data_index, flags));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(&modifiers, parent);
        self.set_parent_list(&declarations, parent);

        parent
    }

    /// Add a return/throw statement node
    pub fn add_return(&mut self, kind: u16, pos: u32, end: u32, data: ReturnData) -> NodeIndex {
        let expression = data.expression;

        let data_index = self.return_data.len() as u32;
        self.return_data.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);

        parent
    }

    /// Add an expression statement node
    pub fn add_expr_statement(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ExprStatementData,
    ) -> NodeIndex {
        let expression = data.expression;
        let data_index = self.expr_statements.len() as u32;
        self.expr_statements.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        parent
    }

    /// Add a switch statement node
    pub fn add_switch(&mut self, kind: u16, pos: u32, end: u32, data: SwitchData) -> NodeIndex {
        let expression = data.expression;
        let case_block = data.case_block;
        let data_index = self.switch_data.len() as u32;
        self.switch_data.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent(case_block, parent);
        parent
    }

    /// Add a case/default clause node
    pub fn add_case_clause(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: CaseClauseData,
    ) -> NodeIndex {
        let expression = data.expression;
        let statements = data.statements.clone();
        let data_index = self.case_clauses.len() as u32;
        self.case_clauses.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent_list(&statements, parent);
        parent
    }

    /// Add a try statement node
    pub fn add_try(&mut self, kind: u16, pos: u32, end: u32, data: TryData) -> NodeIndex {
        let data_index = self.try_data.len() as u32;
        self.try_data.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        NodeIndex(index)
    }

    /// Add a catch clause node
    pub fn add_catch_clause(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: CatchClauseData,
    ) -> NodeIndex {
        let variable_declaration = data.variable_declaration;
        let block = data.block;
        let data_index = self.catch_clauses.len() as u32;
        self.catch_clauses.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(variable_declaration, parent);
        self.set_parent(block, parent);

        parent
    }

    /// Add a labeled statement node
    pub fn add_labeled(&mut self, kind: u16, pos: u32, end: u32, data: LabeledData) -> NodeIndex {
        let data_index = self.labeled_data.len() as u32;
        self.labeled_data.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        NodeIndex(index)
    }

    /// Add a break/continue statement node
    pub fn add_jump(&mut self, kind: u16, pos: u32, end: u32, data: JumpData) -> NodeIndex {
        let data_index = self.jump_data.len() as u32;
        self.jump_data.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        NodeIndex(index)
    }

    /// Add a with statement node
    pub fn add_with(&mut self, kind: u16, pos: u32, end: u32, data: WithData) -> NodeIndex {
        let data_index = self.with_data.len() as u32;
        self.with_data.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        NodeIndex(index)
    }

    /// Add a type reference node
    pub fn add_type_ref(&mut self, kind: u16, pos: u32, end: u32, data: TypeRefData) -> NodeIndex {
        let type_name = data.type_name;
        let type_arguments = data.type_arguments.clone();
        let data_index = self.type_refs.len() as u32;
        self.type_refs.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(type_name, parent);
        self.set_parent_opt_list(&type_arguments, parent);
        parent
    }

    /// Add a union/intersection type node
    pub fn add_composite_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: CompositeTypeData,
    ) -> NodeIndex {
        let types = data.types.clone();

        let data_index = self.composite_types.len() as u32;
        self.composite_types.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_list(&types, parent);

        parent
    }

    /// Add a function/constructor type node
    pub fn add_function_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: FunctionTypeData,
    ) -> NodeIndex {
        let type_parameters = data.type_parameters.clone();
        let parameters = data.parameters.clone();
        let type_annotation = data.type_annotation;

        let data_index = self.function_types.len() as u32;
        self.function_types.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(&type_parameters, parent);
        self.set_parent_list(&parameters, parent);
        self.set_parent(type_annotation, parent);

        parent
    }

    /// Add a type query node (typeof)
    pub fn add_type_query(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypeQueryData,
    ) -> NodeIndex {
        let expr_name = data.expr_name;
        let type_arguments = data.type_arguments.clone();
        let data_index = self.type_queries.len() as u32;
        self.type_queries.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expr_name, parent);
        self.set_parent_opt_list(&type_arguments, parent);
        parent
    }

    /// Add a type literal node
    pub fn add_type_literal(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypeLiteralData,
    ) -> NodeIndex {
        let members = data.members.clone();
        let data_index = self.type_literals.len() as u32;
        self.type_literals.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_list(&members, parent);
        parent
    }

    /// Add an array type node
    pub fn add_array_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ArrayTypeData,
    ) -> NodeIndex {
        let element_type = data.element_type;
        let data_index = self.array_types.len() as u32;
        self.array_types.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(element_type, parent);
        parent
    }

    /// Add a tuple type node
    pub fn add_tuple_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TupleTypeData,
    ) -> NodeIndex {
        let elements = data.elements.clone();
        let data_index = self.tuple_types.len() as u32;
        self.tuple_types.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_list(&elements, parent);
        parent
    }

    /// Add an optional/rest type node
    pub fn add_wrapped_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: WrappedTypeData,
    ) -> NodeIndex {
        let type_node = data.type_node;
        let data_index = self.wrapped_types.len() as u32;
        self.wrapped_types.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(type_node, parent);
        parent
    }

    /// Add a conditional type node
    pub fn add_conditional_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ConditionalTypeData,
    ) -> NodeIndex {
        let check_type = data.check_type;
        let extends_type = data.extends_type;
        let true_type = data.true_type;
        let false_type = data.false_type;
        let data_index = self.conditional_types.len() as u32;
        self.conditional_types.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(check_type, parent);
        self.set_parent(extends_type, parent);
        self.set_parent(true_type, parent);
        self.set_parent(false_type, parent);
        parent
    }

    /// Add an infer type node
    pub fn add_infer_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: InferTypeData,
    ) -> NodeIndex {
        let type_parameter = data.type_parameter;
        let data_index = self.infer_types.len() as u32;
        self.infer_types.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(type_parameter, parent);
        parent
    }

    /// Add a type operator node (keyof, unique, readonly)
    pub fn add_type_operator(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypeOperatorData,
    ) -> NodeIndex {
        let type_node = data.type_node;
        let data_index = self.type_operators.len() as u32;
        self.type_operators.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(type_node, parent);
        parent
    }

    /// Add an indexed access type node
    pub fn add_indexed_access_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: IndexedAccessTypeData,
    ) -> NodeIndex {
        let object_type = data.object_type;
        let index_type = data.index_type;
        let data_index = self.indexed_access_types.len() as u32;
        self.indexed_access_types.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(object_type, parent);
        self.set_parent(index_type, parent);
        parent
    }

    /// Add a mapped type node
    pub fn add_mapped_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: MappedTypeData,
    ) -> NodeIndex {
        let readonly_token = data.readonly_token;
        let type_parameter = data.type_parameter;
        let name_type = data.name_type;
        let question_token = data.question_token;
        let type_node = data.type_node;
        let members = data.members.clone();
        let data_index = self.mapped_types.len() as u32;
        self.mapped_types.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(readonly_token, parent);
        self.set_parent(type_parameter, parent);
        self.set_parent(name_type, parent);
        self.set_parent(question_token, parent);
        self.set_parent(type_node, parent);
        self.set_parent_opt_list(&members, parent);
        parent
    }

    /// Add a literal type node
    pub fn add_literal_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: LiteralTypeData,
    ) -> NodeIndex {
        let literal = data.literal;
        let data_index = self.literal_types.len() as u32;
        self.literal_types.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(literal, parent);
        parent
    }

    /// Add a template literal type node
    pub fn add_template_literal_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TemplateLiteralTypeData,
    ) -> NodeIndex {
        let head = data.head;
        let template_spans = data.template_spans.clone();
        let data_index = self.template_literal_types.len() as u32;
        self.template_literal_types.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(head, parent);
        self.set_parent_list(&template_spans, parent);
        parent
    }

    /// Add a named tuple member node
    pub fn add_named_tuple_member(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: NamedTupleMemberData,
    ) -> NodeIndex {
        let name = data.name;
        let type_node = data.type_node;

        let data_index = self.named_tuple_members.len() as u32;
        self.named_tuple_members.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(name, parent);
        self.set_parent(type_node, parent);
        parent
    }

    /// Add a type predicate node
    pub fn add_type_predicate(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypePredicateData,
    ) -> NodeIndex {
        let parameter_name = data.parameter_name;
        let type_node = data.type_node;

        let data_index = self.type_predicates.len() as u32;
        self.type_predicates.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(parameter_name, parent);
        self.set_parent(type_node, parent);
        parent
    }

    /// Add an import declaration node
    pub fn add_import_decl(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ImportDeclData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let import_clause = data.import_clause;
        let module_specifier = data.module_specifier;
        let attributes = data.attributes;

        let data_index = self.import_decls.len() as u32;
        self.import_decls.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_opt_list(&modifiers, parent);
        self.set_parent(import_clause, parent);
        self.set_parent(module_specifier, parent);
        self.set_parent(attributes, parent);
        parent
    }

    /// Add an import clause node
    pub fn add_import_clause(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ImportClauseData,
    ) -> NodeIndex {
        let name = data.name;
        let named_bindings = data.named_bindings;

        let data_index = self.import_clauses.len() as u32;
        self.import_clauses.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(name, parent);
        self.set_parent(named_bindings, parent);
        parent
    }

    /// Add a namespace/named imports node
    pub fn add_named_imports(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: NamedImportsData,
    ) -> NodeIndex {
        let name = data.name;
        let elements = data.elements.clone();

        let data_index = self.named_imports.len() as u32;
        self.named_imports.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(name, parent);
        self.set_parent_list(&elements, parent);
        parent
    }

    /// Add an import/export specifier node
    pub fn add_specifier(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: SpecifierData,
    ) -> NodeIndex {
        let property_name = data.property_name;
        let name = data.name;

        let data_index = self.specifiers.len() as u32;
        self.specifiers.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(property_name, parent);
        self.set_parent(name, parent);
        parent
    }

    /// Add an export declaration node
    pub fn add_export_decl(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ExportDeclData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let export_clause = data.export_clause;
        let module_specifier = data.module_specifier;
        let attributes = data.attributes;

        let data_index = self.export_decls.len() as u32;
        self.export_decls.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_opt_list(&modifiers, parent);
        self.set_parent(export_clause, parent);
        self.set_parent(module_specifier, parent);
        self.set_parent(attributes, parent);
        parent
    }
    /// Add an export assignment node
    pub fn add_export_assignment(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ExportAssignmentData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let expression = data.expression;

        let data_index = self.export_assignments.len() as u32;
        self.export_assignments.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_opt_list(&modifiers, parent);
        self.set_parent(expression, parent);
        parent
    }

    /// Add an import attributes node
    pub fn add_import_attributes(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ImportAttributesData,
    ) -> NodeIndex {
        let elements = data.elements.clone();

        let data_index = self.import_attributes.len() as u32;
        self.import_attributes.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_list(&elements, parent);
        parent
    }

    /// Add an import attribute node
    pub fn add_import_attribute(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ImportAttributeData,
    ) -> NodeIndex {
        let name = data.name;
        let value = data.value;

        let data_index = self.import_attribute.len() as u32;
        self.import_attribute.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(name, parent);
        self.set_parent(value, parent);
        parent
    }

    /// Add a binding pattern node
    pub fn add_binding_pattern(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: BindingPatternData,
    ) -> NodeIndex {
        let elements = data.elements.clone();

        let data_index = self.binding_patterns.len() as u32;
        self.binding_patterns.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_list(&elements, parent);
        parent
    }

    /// Add a binding element node
    pub fn add_binding_element(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: BindingElementData,
    ) -> NodeIndex {
        let property_name = data.property_name;
        let name = data.name;
        let initializer = data.initializer;

        let data_index = self.binding_elements.len() as u32;
        self.binding_elements.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(property_name, parent);
        self.set_parent(name, parent);
        self.set_parent(initializer, parent);
        parent
    }

    /// Add a property assignment node
    pub fn add_property_assignment(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: PropertyAssignmentData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let initializer = data.initializer;

        let data_index = self.property_assignments.len() as u32;
        self.property_assignments.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_opt_list(&modifiers, parent);
        self.set_parent(name, parent);
        self.set_parent(initializer, parent);
        parent
    }

    /// Add a shorthand property assignment node
    pub fn add_shorthand_property(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ShorthandPropertyData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let object_assignment_initializer = data.object_assignment_initializer;

        let data_index = self.shorthand_properties.len() as u32;
        self.shorthand_properties.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_opt_list(&modifiers, parent);
        self.set_parent(name, parent);
        self.set_parent(object_assignment_initializer, parent);
        parent
    }

    /// Add a spread assignment node
    pub fn add_spread(&mut self, kind: u16, pos: u32, end: u32, data: SpreadData) -> NodeIndex {
        let expression = data.expression;

        let data_index = self.spread_data.len() as u32;
        self.spread_data.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        parent
    }

    /// Add a JSX element node
    pub fn add_jsx_element(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxElementData,
    ) -> NodeIndex {
        let opening_element = data.opening_element;
        let children = data.children.clone();
        let closing_element = data.closing_element;

        let data_index = self.jsx_elements.len() as u32;
        self.jsx_elements.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(opening_element, parent);
        self.set_parent_list(&children, parent);
        self.set_parent(closing_element, parent);
        parent
    }

    /// Add a JSX opening/self-closing element node
    pub fn add_jsx_opening(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxOpeningData,
    ) -> NodeIndex {
        let tag_name = data.tag_name;
        let type_arguments = data.type_arguments.clone();
        let attributes = data.attributes;

        let data_index = self.jsx_opening.len() as u32;
        self.jsx_opening.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(tag_name, parent);
        self.set_parent_opt_list(&type_arguments, parent);
        self.set_parent(attributes, parent);
        parent
    }

    /// Add a JSX closing element node
    pub fn add_jsx_closing(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxClosingData,
    ) -> NodeIndex {
        let tag_name = data.tag_name;

        let data_index = self.jsx_closing.len() as u32;
        self.jsx_closing.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(tag_name, parent);
        parent
    }

    /// Add a JSX fragment node
    pub fn add_jsx_fragment(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxFragmentData,
    ) -> NodeIndex {
        let opening_fragment = data.opening_fragment;
        let children = data.children.clone();
        let closing_fragment = data.closing_fragment;

        let data_index = self.jsx_fragments.len() as u32;
        self.jsx_fragments.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(opening_fragment, parent);
        self.set_parent_list(&children, parent);
        self.set_parent(closing_fragment, parent);
        parent
    }

    /// Add a JSX attributes node
    pub fn add_jsx_attributes(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxAttributesData,
    ) -> NodeIndex {
        let properties = data.properties.clone();

        let data_index = self.jsx_attributes.len() as u32;
        self.jsx_attributes.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_list(&properties, parent);
        parent
    }

    /// Add a JSX attribute node
    pub fn add_jsx_attribute(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxAttributeData,
    ) -> NodeIndex {
        let name = data.name;
        let initializer = data.initializer;

        let data_index = self.jsx_attribute.len() as u32;
        self.jsx_attribute.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(name, parent);
        self.set_parent(initializer, parent);
        parent
    }

    /// Add a JSX spread attribute node
    pub fn add_jsx_spread_attribute(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxSpreadAttributeData,
    ) -> NodeIndex {
        let expression = data.expression;

        let data_index = self.jsx_spread_attributes.len() as u32;
        self.jsx_spread_attributes.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        parent
    }

    /// Add a JSX expression node
    pub fn add_jsx_expression(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxExpressionData,
    ) -> NodeIndex {
        let expression = data.expression;

        let data_index = self.jsx_expressions.len() as u32;
        self.jsx_expressions.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        parent
    }

    /// Add a JSX text node
    pub fn add_jsx_text(&mut self, kind: u16, pos: u32, end: u32, data: JsxTextData) -> NodeIndex {
        let data_index = self.jsx_text.len() as u32;
        self.jsx_text.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        NodeIndex(index)
    }

    /// Add a JSX namespaced name node
    pub fn add_jsx_namespaced_name(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxNamespacedNameData,
    ) -> NodeIndex {
        let namespace = data.namespace;
        let name = data.name;

        let data_index = self.jsx_namespaced_names.len() as u32;
        self.jsx_namespaced_names.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(namespace, parent);
        self.set_parent(name, parent);
        parent
    }

    /// Add a variable declaration node (individual)
    pub fn add_variable_declaration(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: VariableDeclarationData,
    ) -> NodeIndex {
        let name = data.name;
        let type_annotation = data.type_annotation;
        let initializer = data.initializer;

        let data_index = self.variable_declarations.len() as u32;
        self.variable_declarations.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(name, parent);
        self.set_parent(type_annotation, parent);
        self.set_parent(initializer, parent);

        parent
    }

    /// Add a for-in/for-of statement node
    pub fn add_for_in_of(&mut self, kind: u16, pos: u32, end: u32, data: ForInOfData) -> NodeIndex {
        let initializer = data.initializer;
        let expression = data.expression;
        let statement = data.statement;
        let data_index = self.for_in_of.len() as u32;
        self.for_in_of.push(data);
        let index = self.nodes.len() as u32;
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(initializer, parent);
        self.set_parent(expression, parent);
        self.set_parent(statement, parent);
        parent
    }

    /// Get a thin node by index
    #[inline]
    pub fn get(&self, index: NodeIndex) -> Option<&Node> {
        if index.is_none() {
            None
        } else {
            self.nodes.get(index.0 as usize)
        }
    }

    /// Get a mutable thin node by index
    #[inline]
    pub fn get_mut(&mut self, index: NodeIndex) -> Option<&mut Node> {
        if index.is_none() {
            None
        } else {
            self.nodes.get_mut(index.0 as usize)
        }
    }

    /// Get extended info for a node
    #[inline]
    pub fn get_extended(&self, index: NodeIndex) -> Option<&ExtendedNodeInfo> {
        if index.is_none() {
            None
        } else {
            self.extended_info.get(index.0 as usize)
        }
    }

    /// Get mutable extended info for a node
    #[inline]
    pub fn get_extended_mut(&mut self, index: NodeIndex) -> Option<&mut ExtendedNodeInfo> {
        if index.is_none() {
            None
        } else {
            self.extended_info.get_mut(index.0 as usize)
        }
    }

    /// Get identifier data for a node.
    /// Returns None if node is not an identifier or has no data.
    #[inline]
    pub fn get_identifier(&self, node: &Node) -> Option<&IdentifierData> {
        use crate::scanner::SyntaxKind;
        if node.has_data()
            && (node.kind == SyntaxKind::Identifier as u16
                || node.kind == SyntaxKind::PrivateIdentifier as u16)
        {
            self.identifiers.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get literal data for a node.
    /// Returns None if node is not a literal or has no data.
    #[inline]
    pub fn get_literal(&self, node: &Node) -> Option<&LiteralData> {
        use crate::scanner::SyntaxKind;
        if node.has_data()
            && matches!(node.kind,
                k if k == SyntaxKind::StringLiteral as u16 ||
                     k == SyntaxKind::NumericLiteral as u16 ||
                     k == SyntaxKind::BigIntLiteral as u16 ||
                     k == SyntaxKind::RegularExpressionLiteral as u16 ||
                     k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 ||
                     k == SyntaxKind::TemplateHead as u16 ||
                     k == SyntaxKind::TemplateMiddle as u16 ||
                     k == SyntaxKind::TemplateTail as u16
            )
        {
            self.literals.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get binary expression data.
    /// Returns None if node is not a binary expression or has no data.
    #[inline]
    pub fn get_binary_expr(&self, node: &Node) -> Option<&BinaryExprData> {
        use super::syntax_kind_ext::BINARY_EXPRESSION;
        if node.has_data() && node.kind == BINARY_EXPRESSION {
            self.binary_exprs.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get call expression data.
    /// Returns None if node is not a call/new expression or has no data.
    #[inline]
    pub fn get_call_expr(&self, node: &Node) -> Option<&CallExprData> {
        use super::syntax_kind_ext::{CALL_EXPRESSION, NEW_EXPRESSION};
        if node.has_data() && (node.kind == CALL_EXPRESSION || node.kind == NEW_EXPRESSION) {
            self.call_exprs.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get access expression data (property access or element access).
    /// Returns None if node is not an access expression or has no data.
    #[inline]
    pub fn get_access_expr(&self, node: &Node) -> Option<&AccessExprData> {
        use super::syntax_kind_ext::{ELEMENT_ACCESS_EXPRESSION, PROPERTY_ACCESS_EXPRESSION};
        if node.has_data()
            && (node.kind == PROPERTY_ACCESS_EXPRESSION || node.kind == ELEMENT_ACCESS_EXPRESSION)
        {
            self.access_exprs.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get conditional expression data (ternary: a ? b : c).
    /// Returns None if node is not a conditional expression or has no data.
    #[inline]
    pub fn get_conditional_expr(&self, node: &Node) -> Option<&ConditionalExprData> {
        use super::syntax_kind_ext::CONDITIONAL_EXPRESSION;
        if node.has_data() && node.kind == CONDITIONAL_EXPRESSION {
            self.conditional_exprs.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get qualified name data (A.B syntax).
    /// Returns None if node is not a qualified name or has no data.
    #[inline]
    pub fn get_qualified_name(&self, node: &Node) -> Option<&QualifiedNameData> {
        use super::syntax_kind_ext::QUALIFIED_NAME;
        if node.has_data() && node.kind == QUALIFIED_NAME {
            self.qualified_names.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get literal expression data (array or object literal).
    /// Returns None if node is not a literal expression or has no data.
    #[inline]
    pub fn get_literal_expr(&self, node: &Node) -> Option<&LiteralExprData> {
        use super::syntax_kind_ext::{ARRAY_LITERAL_EXPRESSION, OBJECT_LITERAL_EXPRESSION};
        if node.has_data()
            && (node.kind == ARRAY_LITERAL_EXPRESSION || node.kind == OBJECT_LITERAL_EXPRESSION)
        {
            self.literal_exprs.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get property assignment data.
    /// Returns None if node is not a property assignment or has no data.
    #[inline]
    pub fn get_property_assignment(&self, node: &Node) -> Option<&PropertyAssignmentData> {
        use super::syntax_kind_ext::PROPERTY_ASSIGNMENT;
        if node.has_data() && node.kind == PROPERTY_ASSIGNMENT {
            self.property_assignments.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get type assertion data (as/satisfies/type assertion).
    /// Returns None if node is not a type assertion or has no data.
    #[inline]
    pub fn get_type_assertion(&self, node: &Node) -> Option<&TypeAssertionData> {
        use super::syntax_kind_ext::{AS_EXPRESSION, SATISFIES_EXPRESSION, TYPE_ASSERTION};
        if node.has_data()
            && (node.kind == TYPE_ASSERTION
                || node.kind == AS_EXPRESSION
                || node.kind == SATISFIES_EXPRESSION)
        {
            self.type_assertions.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get unary expression data (prefix or postfix).
    /// Returns None if node is not a unary expression or has no data.
    #[inline]
    pub fn get_unary_expr(&self, node: &Node) -> Option<&UnaryExprData> {
        use super::syntax_kind_ext::{POSTFIX_UNARY_EXPRESSION, PREFIX_UNARY_EXPRESSION};
        if node.has_data()
            && (node.kind == PREFIX_UNARY_EXPRESSION || node.kind == POSTFIX_UNARY_EXPRESSION)
        {
            self.unary_exprs.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get extended unary expression data (await/yield/non-null/spread).
    /// Returns None if node is not an await/yield/non-null/spread expression or has no data.
    #[inline]
    pub fn get_unary_expr_ex(&self, node: &Node) -> Option<&UnaryExprDataEx> {
        use super::syntax_kind_ext::{
            AWAIT_EXPRESSION, NON_NULL_EXPRESSION, SPREAD_ELEMENT, YIELD_EXPRESSION,
        };
        if node.has_data()
            && (node.kind == AWAIT_EXPRESSION
                || node.kind == YIELD_EXPRESSION
                || node.kind == NON_NULL_EXPRESSION
                || node.kind == SPREAD_ELEMENT)
        {
            self.unary_exprs_ex.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get function data.
    /// Returns None if node is not a function-like node or has no data.
    #[inline]
    pub fn get_function(&self, node: &Node) -> Option<&FunctionData> {
        use super::syntax_kind_ext::*;
        if node.has_data()
            && matches!(
                node.kind,
                FUNCTION_DECLARATION | FUNCTION_EXPRESSION | ARROW_FUNCTION
            )
        {
            self.functions.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get class data.
    /// Returns None if node is not a class declaration/expression or has no data.
    #[inline]
    pub fn get_class(&self, node: &Node) -> Option<&ClassData> {
        use super::syntax_kind_ext::{CLASS_DECLARATION, CLASS_EXPRESSION};
        if node.has_data() && (node.kind == CLASS_DECLARATION || node.kind == CLASS_EXPRESSION) {
            self.classes.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get block data.
    /// Returns None if node is not a block or has no data.
    #[inline]
    pub fn get_block(&self, node: &Node) -> Option<&BlockData> {
        use super::syntax_kind_ext::{BLOCK, CASE_BLOCK, CLASS_STATIC_BLOCK_DECLARATION};
        if node.has_data()
            && (node.kind == BLOCK
                || node.kind == CLASS_STATIC_BLOCK_DECLARATION
                || node.kind == CASE_BLOCK)
        {
            self.blocks.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get source file data.
    /// Returns None if node is not a source file or has no data.
    #[inline]
    pub fn get_source_file(&self, node: &Node) -> Option<&SourceFileData> {
        use super::syntax_kind_ext::SOURCE_FILE;
        if node.has_data() && node.kind == SOURCE_FILE {
            self.source_files.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get variable data (VariableStatement or VariableDeclarationList).
    #[inline]
    pub fn get_variable(&self, node: &Node) -> Option<&VariableData> {
        use super::syntax_kind_ext::{VARIABLE_DECLARATION_LIST, VARIABLE_STATEMENT};
        if node.has_data()
            && (node.kind == VARIABLE_STATEMENT || node.kind == VARIABLE_DECLARATION_LIST)
        {
            self.variables.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get variable declaration data.
    #[inline]
    pub fn get_variable_declaration(&self, node: &Node) -> Option<&VariableDeclarationData> {
        use super::syntax_kind_ext::VARIABLE_DECLARATION;
        if node.has_data() && node.kind == VARIABLE_DECLARATION {
            self.variable_declarations.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get interface data.
    #[inline]
    pub fn get_interface(&self, node: &Node) -> Option<&InterfaceData> {
        use super::syntax_kind_ext::INTERFACE_DECLARATION;
        if node.has_data() && node.kind == INTERFACE_DECLARATION {
            self.interfaces.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get type alias data.
    #[inline]
    pub fn get_type_alias(&self, node: &Node) -> Option<&TypeAliasData> {
        use super::syntax_kind_ext::TYPE_ALIAS_DECLARATION;
        if node.has_data() && node.kind == TYPE_ALIAS_DECLARATION {
            self.type_aliases.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get enum data.
    #[inline]
    pub fn get_enum(&self, node: &Node) -> Option<&EnumData> {
        use super::syntax_kind_ext::ENUM_DECLARATION;
        if node.has_data() && node.kind == ENUM_DECLARATION {
            self.enums.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get enum member data.
    #[inline]
    pub fn get_enum_member(&self, node: &Node) -> Option<&EnumMemberData> {
        use super::syntax_kind_ext::ENUM_MEMBER;
        if node.has_data() && node.kind == ENUM_MEMBER {
            self.enum_members.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get module data.
    #[inline]
    pub fn get_module(&self, node: &Node) -> Option<&ModuleData> {
        use super::syntax_kind_ext::MODULE_DECLARATION;
        if node.has_data() && node.kind == MODULE_DECLARATION {
            self.modules.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get module block data.
    #[inline]
    pub fn get_module_block(&self, node: &Node) -> Option<&ModuleBlockData> {
        use super::syntax_kind_ext::MODULE_BLOCK;
        if node.has_data() && node.kind == MODULE_BLOCK {
            self.module_blocks.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get if statement data.
    #[inline]
    pub fn get_if_statement(&self, node: &Node) -> Option<&IfStatementData> {
        use super::syntax_kind_ext::IF_STATEMENT;
        if node.has_data() && node.kind == IF_STATEMENT {
            self.if_statements.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get loop data (while, for, do-while).
    #[inline]
    pub fn get_loop(&self, node: &Node) -> Option<&LoopData> {
        use super::syntax_kind_ext::{DO_STATEMENT, FOR_STATEMENT, WHILE_STATEMENT};
        if node.has_data()
            && (node.kind == WHILE_STATEMENT
                || node.kind == DO_STATEMENT
                || node.kind == FOR_STATEMENT)
        {
            self.loops.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get for-in/for-of data.
    #[inline]
    pub fn get_for_in_of(&self, node: &Node) -> Option<&ForInOfData> {
        use super::syntax_kind_ext::{FOR_IN_STATEMENT, FOR_OF_STATEMENT};
        if node.has_data() && (node.kind == FOR_IN_STATEMENT || node.kind == FOR_OF_STATEMENT) {
            self.for_in_of.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get switch data.
    #[inline]
    pub fn get_switch(&self, node: &Node) -> Option<&SwitchData> {
        use super::syntax_kind_ext::SWITCH_STATEMENT;
        if node.has_data() && node.kind == SWITCH_STATEMENT {
            self.switch_data.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get case clause data.
    #[inline]
    pub fn get_case_clause(&self, node: &Node) -> Option<&CaseClauseData> {
        use super::syntax_kind_ext::{CASE_CLAUSE, DEFAULT_CLAUSE};
        if node.has_data() && (node.kind == CASE_CLAUSE || node.kind == DEFAULT_CLAUSE) {
            self.case_clauses.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get try data.
    #[inline]
    pub fn get_try(&self, node: &Node) -> Option<&TryData> {
        use super::syntax_kind_ext::TRY_STATEMENT;
        if node.has_data() && node.kind == TRY_STATEMENT {
            self.try_data.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get catch clause data.
    #[inline]
    pub fn get_catch_clause(&self, node: &Node) -> Option<&CatchClauseData> {
        use super::syntax_kind_ext::CATCH_CLAUSE;
        if node.has_data() && node.kind == CATCH_CLAUSE {
            self.catch_clauses.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get labeled statement data.
    #[inline]
    pub fn get_labeled_statement(&self, node: &Node) -> Option<&LabeledData> {
        use super::syntax_kind_ext::LABELED_STATEMENT;
        if node.has_data() && node.kind == LABELED_STATEMENT {
            self.labeled_data.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get jump data (break/continue statements).
    #[inline]
    pub fn get_jump_data(&self, node: &Node) -> Option<&JumpData> {
        use super::syntax_kind_ext::{BREAK_STATEMENT, CONTINUE_STATEMENT};
        if node.has_data() && (node.kind == BREAK_STATEMENT || node.kind == CONTINUE_STATEMENT) {
            self.jump_data.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get with statement data (stored in if statement pool).
    #[inline]
    pub fn get_with_statement(&self, node: &Node) -> Option<&IfStatementData> {
        use super::syntax_kind_ext::WITH_STATEMENT;
        if node.has_data() && node.kind == WITH_STATEMENT {
            self.if_statements.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get import declaration data (handles both IMPORT_DECLARATION and IMPORT_EQUALS_DECLARATION).
    #[inline]
    pub fn get_import_decl(&self, node: &Node) -> Option<&ImportDeclData> {
        use super::syntax_kind_ext::{IMPORT_DECLARATION, IMPORT_EQUALS_DECLARATION};
        if node.has_data()
            && (node.kind == IMPORT_DECLARATION || node.kind == IMPORT_EQUALS_DECLARATION)
        {
            self.import_decls.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get import clause data.
    #[inline]
    pub fn get_import_clause(&self, node: &Node) -> Option<&ImportClauseData> {
        use super::syntax_kind_ext::IMPORT_CLAUSE;
        if node.has_data() && node.kind == IMPORT_CLAUSE {
            self.import_clauses.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get named imports/exports data.
    /// Works for NAMED_IMPORTS, NAMESPACE_IMPORT, and NAMED_EXPORTS (they share the same data structure).
    #[inline]
    pub fn get_named_imports(&self, node: &Node) -> Option<&NamedImportsData> {
        use super::syntax_kind_ext::{NAMED_EXPORTS, NAMED_IMPORTS, NAMESPACE_IMPORT};
        if node.has_data()
            && (node.kind == NAMED_IMPORTS
                || node.kind == NAMED_EXPORTS
                || node.kind == NAMESPACE_IMPORT)
        {
            self.named_imports.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get import/export specifier data.
    #[inline]
    pub fn get_specifier(&self, node: &Node) -> Option<&SpecifierData> {
        use super::syntax_kind_ext::{EXPORT_SPECIFIER, IMPORT_SPECIFIER};
        if node.has_data() && (node.kind == IMPORT_SPECIFIER || node.kind == EXPORT_SPECIFIER) {
            self.specifiers.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get export declaration data.
    #[inline]
    pub fn get_export_decl(&self, node: &Node) -> Option<&ExportDeclData> {
        use super::syntax_kind_ext::EXPORT_DECLARATION;
        if node.has_data() && node.kind == EXPORT_DECLARATION {
            self.export_decls.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get export assignment data (export = expr).
    #[inline]
    pub fn get_export_assignment(&self, node: &Node) -> Option<&ExportAssignmentData> {
        use super::syntax_kind_ext::EXPORT_ASSIGNMENT;
        if node.has_data() && node.kind == EXPORT_ASSIGNMENT {
            self.export_assignments.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get parameter data.
    #[inline]
    pub fn get_parameter(&self, node: &Node) -> Option<&ParameterData> {
        use super::syntax_kind_ext::PARAMETER;
        if node.has_data() && node.kind == PARAMETER {
            self.parameters.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get property declaration data.
    #[inline]
    pub fn get_property_decl(&self, node: &Node) -> Option<&PropertyDeclData> {
        use super::syntax_kind_ext::PROPERTY_DECLARATION;
        if node.has_data() && node.kind == PROPERTY_DECLARATION {
            self.property_decls.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get method declaration data.
    #[inline]
    pub fn get_method_decl(&self, node: &Node) -> Option<&MethodDeclData> {
        use super::syntax_kind_ext::METHOD_DECLARATION;
        if node.has_data() && node.kind == METHOD_DECLARATION {
            self.method_decls.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get constructor data.
    #[inline]
    pub fn get_constructor(&self, node: &Node) -> Option<&ConstructorData> {
        use super::syntax_kind_ext::CONSTRUCTOR;
        if node.has_data() && node.kind == CONSTRUCTOR {
            self.constructors.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get accessor data (get/set accessor).
    #[inline]
    pub fn get_accessor(&self, node: &Node) -> Option<&AccessorData> {
        use super::syntax_kind_ext::{GET_ACCESSOR, SET_ACCESSOR};
        if node.has_data() && (node.kind == GET_ACCESSOR || node.kind == SET_ACCESSOR) {
            self.accessors.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get decorator data.
    #[inline]
    pub fn get_decorator(&self, node: &Node) -> Option<&DecoratorData> {
        use super::syntax_kind_ext::DECORATOR;
        if node.has_data() && node.kind == DECORATOR {
            self.decorators.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get type reference data.
    #[inline]
    pub fn get_type_ref(&self, node: &Node) -> Option<&TypeRefData> {
        use super::syntax_kind_ext::TYPE_REFERENCE;
        if node.has_data() && node.kind == TYPE_REFERENCE {
            self.type_refs.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get expression statement data (returns the expression node index).
    #[inline]
    pub fn get_expression_statement(&self, node: &Node) -> Option<&ExprStatementData> {
        use super::syntax_kind_ext::EXPRESSION_STATEMENT;
        if node.has_data() && node.kind == EXPRESSION_STATEMENT {
            self.expr_statements.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get return statement data (returns the expression node index).
    #[inline]
    pub fn get_return_statement(&self, node: &Node) -> Option<&ReturnData> {
        use super::syntax_kind_ext::{RETURN_STATEMENT, THROW_STATEMENT};
        if node.has_data() && (node.kind == RETURN_STATEMENT || node.kind == THROW_STATEMENT) {
            self.return_data.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get JSX element data.
    #[inline]
    pub fn get_jsx_element(&self, node: &Node) -> Option<&JsxElementData> {
        use super::syntax_kind_ext::JSX_ELEMENT;
        if node.has_data() && node.kind == JSX_ELEMENT {
            self.jsx_elements.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get JSX opening/self-closing element data.
    #[inline]
    pub fn get_jsx_opening(&self, node: &Node) -> Option<&JsxOpeningData> {
        use super::syntax_kind_ext::{JSX_OPENING_ELEMENT, JSX_SELF_CLOSING_ELEMENT};
        if node.has_data()
            && (node.kind == JSX_OPENING_ELEMENT || node.kind == JSX_SELF_CLOSING_ELEMENT)
        {
            self.jsx_opening.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get JSX closing element data.
    #[inline]
    pub fn get_jsx_closing(&self, node: &Node) -> Option<&JsxClosingData> {
        use super::syntax_kind_ext::JSX_CLOSING_ELEMENT;
        if node.has_data() && node.kind == JSX_CLOSING_ELEMENT {
            self.jsx_closing.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get JSX fragment data.
    #[inline]
    pub fn get_jsx_fragment(&self, node: &Node) -> Option<&JsxFragmentData> {
        use super::syntax_kind_ext::JSX_FRAGMENT;
        if node.has_data() && node.kind == JSX_FRAGMENT {
            self.jsx_fragments.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get JSX attributes data.
    #[inline]
    pub fn get_jsx_attributes(&self, node: &Node) -> Option<&JsxAttributesData> {
        use super::syntax_kind_ext::JSX_ATTRIBUTES;
        if node.has_data() && node.kind == JSX_ATTRIBUTES {
            self.jsx_attributes.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get JSX attribute data.
    #[inline]
    pub fn get_jsx_attribute(&self, node: &Node) -> Option<&JsxAttributeData> {
        use super::syntax_kind_ext::JSX_ATTRIBUTE;
        if node.has_data() && node.kind == JSX_ATTRIBUTE {
            self.jsx_attribute.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get JSX spread attribute data.
    #[inline]
    pub fn get_jsx_spread_attribute(&self, node: &Node) -> Option<&JsxSpreadAttributeData> {
        use super::syntax_kind_ext::JSX_SPREAD_ATTRIBUTE;
        if node.has_data() && node.kind == JSX_SPREAD_ATTRIBUTE {
            self.jsx_spread_attributes.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get JSX expression data.
    #[inline]
    pub fn get_jsx_expression(&self, node: &Node) -> Option<&JsxExpressionData> {
        use super::syntax_kind_ext::JSX_EXPRESSION;
        if node.has_data() && node.kind == JSX_EXPRESSION {
            self.jsx_expressions.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get JSX text data.
    #[inline]
    pub fn get_jsx_text(&self, node: &Node) -> Option<&JsxTextData> {
        use crate::scanner::SyntaxKind;
        if node.has_data() && node.kind == SyntaxKind::JsxText as u16 {
            self.jsx_text.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get JSX namespaced name data.
    #[inline]
    pub fn get_jsx_namespaced_name(&self, node: &Node) -> Option<&JsxNamespacedNameData> {
        use super::syntax_kind_ext::JSX_NAMESPACED_NAME;
        if node.has_data() && node.kind == JSX_NAMESPACED_NAME {
            self.jsx_namespaced_names.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get signature data (call, construct, method, property signatures).
    #[inline]
    pub fn get_signature(&self, node: &Node) -> Option<&SignatureData> {
        use super::syntax_kind_ext::{
            CALL_SIGNATURE, CONSTRUCT_SIGNATURE, METHOD_SIGNATURE, PROPERTY_SIGNATURE,
        };
        if node.has_data()
            && (node.kind == CALL_SIGNATURE
                || node.kind == CONSTRUCT_SIGNATURE
                || node.kind == METHOD_SIGNATURE
                || node.kind == PROPERTY_SIGNATURE)
        {
            self.signatures.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get index signature data.
    #[inline]
    pub fn get_index_signature(&self, node: &Node) -> Option<&IndexSignatureData> {
        use super::syntax_kind_ext::INDEX_SIGNATURE;
        if node.has_data() && node.kind == INDEX_SIGNATURE {
            self.index_signatures.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get heritage clause data.
    #[inline]
    pub fn get_heritage_clause(&self, node: &Node) -> Option<&HeritageData> {
        use super::syntax_kind_ext::HERITAGE_CLAUSE;
        if node.has_data() && node.kind == HERITAGE_CLAUSE {
            self.heritage_clauses.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get composite type data (union or intersection).
    #[inline]
    pub fn get_composite_type(&self, node: &Node) -> Option<&CompositeTypeData> {
        use super::syntax_kind_ext::{INTERSECTION_TYPE, UNION_TYPE};
        if node.has_data() && (node.kind == UNION_TYPE || node.kind == INTERSECTION_TYPE) {
            self.composite_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get array type data.
    #[inline]
    pub fn get_array_type(&self, node: &Node) -> Option<&ArrayTypeData> {
        use super::syntax_kind_ext::ARRAY_TYPE;
        if node.has_data() && node.kind == ARRAY_TYPE {
            self.array_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get tuple type data.
    #[inline]
    pub fn get_tuple_type(&self, node: &Node) -> Option<&TupleTypeData> {
        use super::syntax_kind_ext::TUPLE_TYPE;
        if node.has_data() && node.kind == TUPLE_TYPE {
            self.tuple_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get function type data.
    #[inline]
    pub fn get_function_type(&self, node: &Node) -> Option<&FunctionTypeData> {
        use super::syntax_kind_ext::{CONSTRUCTOR_TYPE, FUNCTION_TYPE};
        if node.has_data() && (node.kind == FUNCTION_TYPE || node.kind == CONSTRUCTOR_TYPE) {
            self.function_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get type literal data.
    #[inline]
    pub fn get_type_literal(&self, node: &Node) -> Option<&TypeLiteralData> {
        use super::syntax_kind_ext::TYPE_LITERAL;
        if node.has_data() && node.kind == TYPE_LITERAL {
            self.type_literals.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get conditional type data.
    #[inline]
    pub fn get_conditional_type(&self, node: &Node) -> Option<&ConditionalTypeData> {
        use super::syntax_kind_ext::CONDITIONAL_TYPE;
        if node.has_data() && node.kind == CONDITIONAL_TYPE {
            self.conditional_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get mapped type data.
    #[inline]
    pub fn get_mapped_type(&self, node: &Node) -> Option<&MappedTypeData> {
        use super::syntax_kind_ext::MAPPED_TYPE;
        if node.has_data() && node.kind == MAPPED_TYPE {
            self.mapped_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get indexed access type data.
    #[inline]
    pub fn get_indexed_access_type(&self, node: &Node) -> Option<&IndexedAccessTypeData> {
        use super::syntax_kind_ext::INDEXED_ACCESS_TYPE;
        if node.has_data() && node.kind == INDEXED_ACCESS_TYPE {
            self.indexed_access_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get literal type data.
    #[inline]
    pub fn get_literal_type(&self, node: &Node) -> Option<&LiteralTypeData> {
        use super::syntax_kind_ext::LITERAL_TYPE;
        if node.has_data() && node.kind == LITERAL_TYPE {
            self.literal_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get wrapped type data (parenthesized, optional, rest types).
    #[inline]
    pub fn get_wrapped_type(&self, node: &Node) -> Option<&WrappedTypeData> {
        use super::syntax_kind_ext::{OPTIONAL_TYPE, PARENTHESIZED_TYPE, REST_TYPE};
        if node.has_data()
            && (node.kind == PARENTHESIZED_TYPE
                || node.kind == OPTIONAL_TYPE
                || node.kind == REST_TYPE)
        {
            self.wrapped_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get heritage clause data.
    #[inline]
    pub fn get_heritage(&self, node: &Node) -> Option<&HeritageData> {
        use super::syntax_kind_ext::HERITAGE_CLAUSE;
        if node.has_data() && node.kind == HERITAGE_CLAUSE {
            self.heritage_clauses.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get expression with type arguments data (e.g., `extends Base<T>`).
    #[inline]
    pub fn get_expr_type_args(&self, node: &Node) -> Option<&ExprWithTypeArgsData> {
        use super::syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS;
        if node.has_data() && node.kind == EXPRESSION_WITH_TYPE_ARGUMENTS {
            self.expr_with_type_args.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get type query data (typeof in type position).
    #[inline]
    pub fn get_type_query(&self, node: &Node) -> Option<&TypeQueryData> {
        use super::syntax_kind_ext::TYPE_QUERY;
        if node.has_data() && node.kind == TYPE_QUERY {
            self.type_queries.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get type operator data (keyof, unique, readonly).
    #[inline]
    pub fn get_type_operator(&self, node: &Node) -> Option<&TypeOperatorData> {
        use super::syntax_kind_ext::TYPE_OPERATOR;
        if node.has_data() && node.kind == TYPE_OPERATOR {
            self.type_operators.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get infer type data.
    #[inline]
    pub fn get_infer_type(&self, node: &Node) -> Option<&InferTypeData> {
        use super::syntax_kind_ext::INFER_TYPE;
        if node.has_data() && node.kind == INFER_TYPE {
            self.infer_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get template literal type data.
    #[inline]
    pub fn get_template_literal_type(&self, node: &Node) -> Option<&TemplateLiteralTypeData> {
        use super::syntax_kind_ext::TEMPLATE_LITERAL_TYPE;
        if node.has_data() && node.kind == TEMPLATE_LITERAL_TYPE {
            self.template_literal_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get named tuple member data.
    #[inline]
    pub fn get_named_tuple_member(&self, node: &Node) -> Option<&NamedTupleMemberData> {
        use super::syntax_kind_ext::NAMED_TUPLE_MEMBER;
        if node.has_data() && node.kind == NAMED_TUPLE_MEMBER {
            self.named_tuple_members.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get type predicate data.
    #[inline]
    pub fn get_type_predicate(&self, node: &Node) -> Option<&TypePredicateData> {
        use super::syntax_kind_ext::TYPE_PREDICATE;
        if node.has_data() && node.kind == TYPE_PREDICATE {
            self.type_predicates.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get type parameter data.
    #[inline]
    pub fn get_type_parameter(&self, node: &Node) -> Option<&TypeParameterData> {
        use super::syntax_kind_ext::TYPE_PARAMETER;
        if node.has_data() && node.kind == TYPE_PARAMETER {
            self.type_parameters.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get parenthesized expression data.
    /// Returns None if node is not a parenthesized expression or has no data.
    #[inline]
    pub fn get_parenthesized(&self, node: &Node) -> Option<&ParenthesizedData> {
        use super::syntax_kind_ext::PARENTHESIZED_EXPRESSION;
        if node.has_data() && node.kind == PARENTHESIZED_EXPRESSION {
            self.parenthesized.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get template expression data.
    #[inline]
    pub fn get_template_expr(&self, node: &Node) -> Option<&TemplateExprData> {
        use super::syntax_kind_ext::TEMPLATE_EXPRESSION;
        if node.has_data() && node.kind == TEMPLATE_EXPRESSION {
            self.template_exprs.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get template span data.
    #[inline]
    pub fn get_template_span(&self, node: &Node) -> Option<&TemplateSpanData> {
        use super::syntax_kind_ext::TEMPLATE_SPAN;
        if node.has_data() && node.kind == TEMPLATE_SPAN {
            self.template_spans.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get tagged template expression data.
    #[inline]
    pub fn get_tagged_template(&self, node: &Node) -> Option<&TaggedTemplateData> {
        use super::syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION;
        if node.has_data() && node.kind == TAGGED_TEMPLATE_EXPRESSION {
            self.tagged_templates.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get spread element/assignment data.
    #[inline]
    pub fn get_spread(&self, node: &Node) -> Option<&SpreadData> {
        use super::syntax_kind_ext::{SPREAD_ASSIGNMENT, SPREAD_ELEMENT};
        if node.has_data() && (node.kind == SPREAD_ELEMENT || node.kind == SPREAD_ASSIGNMENT) {
            self.spread_data.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get shorthand property assignment data.
    #[inline]
    pub fn get_shorthand_property(&self, node: &Node) -> Option<&ShorthandPropertyData> {
        use super::syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT;
        if node.has_data() && node.kind == SHORTHAND_PROPERTY_ASSIGNMENT {
            self.shorthand_properties.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get binding pattern data (ObjectBindingPattern or ArrayBindingPattern).
    #[inline]
    pub fn get_binding_pattern(&self, node: &Node) -> Option<&BindingPatternData> {
        use super::syntax_kind_ext::{ARRAY_BINDING_PATTERN, OBJECT_BINDING_PATTERN};
        if node.has_data()
            && (node.kind == OBJECT_BINDING_PATTERN || node.kind == ARRAY_BINDING_PATTERN)
        {
            self.binding_patterns.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get binding element data.
    #[inline]
    pub fn get_binding_element(&self, node: &Node) -> Option<&BindingElementData> {
        use super::syntax_kind_ext::BINDING_ELEMENT;
        if node.has_data() && node.kind == BINDING_ELEMENT {
            self.binding_elements.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get computed property name data
    #[inline]
    pub fn get_computed_property(&self, node: &Node) -> Option<&ComputedPropertyData> {
        use super::syntax_kind_ext::COMPUTED_PROPERTY_NAME;
        if node.has_data() && node.kind == COMPUTED_PROPERTY_NAME {
            self.computed_properties.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Number of nodes in the arena
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if arena is empty
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

// =============================================================================
// Node View - Ergonomic wrapper for reading Nodes
// =============================================================================

/// A view into a node that provides convenient access to both the Node
/// header and its type-specific data. This avoids the need to pass the arena
/// around when working with node data.
#[derive(Clone, Copy)]
pub struct NodeView<'a> {
    pub node: &'a Node,
    pub arena: &'a NodeArena,
    pub index: NodeIndex,
}

impl<'a> NodeView<'a> {
    /// Create a new NodeView
    #[inline]
    pub fn new(arena: &'a NodeArena, index: NodeIndex) -> Option<NodeView<'a>> {
        arena.get(index).map(|node| NodeView { node, arena, index })
    }

    /// Get the SyntaxKind
    #[inline]
    pub fn kind(&self) -> u16 {
        self.node.kind
    }

    /// Get the start position
    #[inline]
    pub fn pos(&self) -> u32 {
        self.node.pos
    }

    /// Get the end position
    #[inline]
    pub fn end(&self) -> u32 {
        self.node.end
    }

    /// Get the flags
    #[inline]
    pub fn flags(&self) -> u16 {
        self.node.flags
    }

    /// Check if this node has associated data
    #[inline]
    pub fn has_data(&self) -> bool {
        self.node.has_data()
    }

    /// Get extended node info (parent, id, modifier/transform flags)
    #[inline]
    pub fn extended(&self) -> Option<&'a ExtendedNodeInfo> {
        self.arena.get_extended(self.index)
    }

    /// Get parent node index
    #[inline]
    pub fn parent(&self) -> NodeIndex {
        self.extended().map_or(NodeIndex::NONE, |e| e.parent)
    }

    /// Get node id
    #[inline]
    pub fn id(&self) -> u32 {
        self.extended().map_or(0, |e| e.id)
    }

    /// Get a child node as a NodeView
    #[inline]
    pub fn child(&self, index: NodeIndex) -> Option<NodeView<'a>> {
        NodeView::new(self.arena, index)
    }

    // Typed data accessors - return Option<&T> based on node kind

    /// Get identifier data (for Identifier, PrivateIdentifier nodes)
    #[inline]
    pub fn as_identifier(&self) -> Option<&'a IdentifierData> {
        self.arena.get_identifier(self.node)
    }

    /// Get literal data (for StringLiteral, NumericLiteral, etc.)
    #[inline]
    pub fn as_literal(&self) -> Option<&'a LiteralData> {
        self.arena.get_literal(self.node)
    }

    /// Get binary expression data
    #[inline]
    pub fn as_binary_expr(&self) -> Option<&'a BinaryExprData> {
        self.arena.get_binary_expr(self.node)
    }

    /// Get call expression data
    #[inline]
    pub fn as_call_expr(&self) -> Option<&'a CallExprData> {
        self.arena.get_call_expr(self.node)
    }

    /// Get function data
    #[inline]
    pub fn as_function(&self) -> Option<&'a FunctionData> {
        self.arena.get_function(self.node)
    }

    /// Get class data
    #[inline]
    pub fn as_class(&self) -> Option<&'a ClassData> {
        self.arena.get_class(self.node)
    }

    /// Get block data
    #[inline]
    pub fn as_block(&self) -> Option<&'a BlockData> {
        self.arena.get_block(self.node)
    }

    /// Get source file data
    #[inline]
    pub fn as_source_file(&self) -> Option<&'a SourceFileData> {
        self.arena.get_source_file(self.node)
    }
}

// =============================================================================
// Node Kind Utilities
// =============================================================================

impl Node {
    /// Check if this is an identifier node
    #[inline]
    pub fn is_identifier(&self) -> bool {
        use crate::scanner::SyntaxKind;
        self.kind == SyntaxKind::Identifier as u16
    }

    /// Check if this is a string literal
    #[inline]
    pub fn is_string_literal(&self) -> bool {
        use crate::scanner::SyntaxKind;
        self.kind == SyntaxKind::StringLiteral as u16
    }

    /// Check if this is a numeric literal
    #[inline]
    pub fn is_numeric_literal(&self) -> bool {
        use crate::scanner::SyntaxKind;
        self.kind == SyntaxKind::NumericLiteral as u16
    }

    /// Check if this is a function declaration
    #[inline]
    pub fn is_function_declaration(&self) -> bool {
        use super::syntax_kind_ext::FUNCTION_DECLARATION;
        self.kind == FUNCTION_DECLARATION
    }

    /// Check if this is a class declaration
    #[inline]
    pub fn is_class_declaration(&self) -> bool {
        use super::syntax_kind_ext::CLASS_DECLARATION;
        self.kind == CLASS_DECLARATION
    }

    /// Check if this is any kind of function-like node
    #[inline]
    pub fn is_function_like(&self) -> bool {
        use super::syntax_kind_ext::*;
        matches!(
            self.kind,
            FUNCTION_DECLARATION
                | FUNCTION_EXPRESSION
                | ARROW_FUNCTION
                | METHOD_DECLARATION
                | CONSTRUCTOR
                | GET_ACCESSOR
                | SET_ACCESSOR
        )
    }

    /// Check if this is a statement
    #[inline]
    pub fn is_statement(&self) -> bool {
        use super::syntax_kind_ext::*;
        (BLOCK..=DEBUGGER_STATEMENT).contains(&self.kind) || self.kind == VARIABLE_STATEMENT
    }

    /// Check if this is a declaration
    #[inline]
    pub fn is_declaration(&self) -> bool {
        use super::syntax_kind_ext::*;
        (VARIABLE_DECLARATION..=EXPORT_SPECIFIER).contains(&self.kind)
    }

    /// Check if this is a type node
    #[inline]
    pub fn is_type_node(&self) -> bool {
        use super::syntax_kind_ext::*;
        (TYPE_PREDICATE..=IMPORT_TYPE).contains(&self.kind)
    }
}

// =============================================================================
// Node Access Trait - Unified Interface for Arena Types
// =============================================================================

/// Common node information that both arena types can provide.
/// This struct contains the essential fields needed by most consumers.
#[derive(Clone, Debug)]
pub struct NodeInfo {
    pub kind: u16,
    pub flags: u32,
    pub modifier_flags: u32,
    pub pos: u32,
    pub end: u32,
    pub parent: NodeIndex,
    pub id: u32,
}

impl NodeInfo {
    /// Create from a Node and its extended info
    pub fn from_thin(node: &Node, ext: &ExtendedNodeInfo) -> NodeInfo {
        NodeInfo {
            kind: node.kind,
            flags: node.flags as u32,
            modifier_flags: ext.modifier_flags,
            pos: node.pos,
            end: node.end,
            parent: ext.parent,
            id: ext.id,
        }
    }
}

/// Trait for unified access to AST nodes across different arena implementations.
/// This allows consumers (binder, checker, emitter) to work with either
/// different arena implementations without code changes.
pub trait NodeAccess {
    /// Get basic node information by index
    fn node_info(&self, index: NodeIndex) -> Option<NodeInfo>;

    /// Get the syntax kind of a node
    fn kind(&self, index: NodeIndex) -> Option<u16>;

    /// Get the source position range
    fn pos_end(&self, index: NodeIndex) -> Option<(u32, u32)>;

    /// Check if a node exists
    fn exists(&self, index: NodeIndex) -> bool {
        !index.is_none() && self.kind(index).is_some()
    }

    /// Get identifier text (if this is an identifier node)
    fn get_identifier_text(&self, index: NodeIndex) -> Option<&str>;

    /// Get literal value text (if this is a literal node)
    fn get_literal_text(&self, index: NodeIndex) -> Option<&str>;

    /// Get children of a node (for traversal)
    fn get_children(&self, index: NodeIndex) -> Vec<NodeIndex>;
}

/// Implementation of NodeAccess for NodeArena
impl NodeAccess for NodeArena {
    fn node_info(&self, index: NodeIndex) -> Option<NodeInfo> {
        if index.is_none() {
            return None;
        }
        let node = self.nodes.get(index.0 as usize)?;
        let ext = self.extended_info.get(index.0 as usize)?;
        Some(NodeInfo::from_thin(node, ext))
    }

    fn kind(&self, index: NodeIndex) -> Option<u16> {
        if index.is_none() {
            return None;
        }
        self.nodes.get(index.0 as usize).map(|n| n.kind)
    }

    fn pos_end(&self, index: NodeIndex) -> Option<(u32, u32)> {
        if index.is_none() {
            return None;
        }
        self.nodes.get(index.0 as usize).map(|n| (n.pos, n.end))
    }

    fn get_identifier_text(&self, index: NodeIndex) -> Option<&str> {
        let node = self.get(index)?;
        let data = self.get_identifier(node)?;
        Some(&data.escaped_text)
    }

    fn get_literal_text(&self, index: NodeIndex) -> Option<&str> {
        let node = self.get(index)?;
        let data = self.get_literal(node)?;
        Some(&data.text)
    }

    fn get_children(&self, index: NodeIndex) -> Vec<NodeIndex> {
        if index.is_none() {
            return Vec::new();
        }

        let node = match self.nodes.get(index.0 as usize) {
            Some(n) => n,
            None => return Vec::new(),
        };

        // Helper to add optional NodeIndex (ignoring NONE)
        let add_opt = |children: &mut Vec<NodeIndex>, idx: NodeIndex| {
            if idx.is_some() {
                children.push(idx);
            }
        };

        // Helper to add NodeList (expanding to individual nodes)
        let add_list = |children: &mut Vec<NodeIndex>, list: &NodeList| {
            children.extend(list.nodes.iter().copied());
        };

        // Helper to add optional NodeList
        let add_opt_list = |children: &mut Vec<NodeIndex>, list: &Option<NodeList>| {
            if let Some(l) = list {
                children.extend(l.nodes.iter().copied());
            }
        };

        use super::syntax_kind_ext::*;

        let mut children = Vec::new();

        // Match on node kind and retrieve data from appropriate pool
        match node.kind {
            // Names
            QUALIFIED_NAME => {
                if let Some(data) = self.get_qualified_name(node) {
                    children.push(data.left);
                    children.push(data.right);
                }
            }
            COMPUTED_PROPERTY_NAME => {
                if let Some(data) = self.get_computed_property(node) {
                    children.push(data.expression);
                }
            }

            // Expressions
            BINARY_EXPRESSION => {
                if let Some(data) = self.get_binary_expr(node) {
                    children.push(data.left);
                    children.push(data.right);
                }
            }
            PREFIX_UNARY_EXPRESSION | POSTFIX_UNARY_EXPRESSION => {
                if let Some(data) = self.get_unary_expr(node) {
                    children.push(data.operand);
                }
            }
            CALL_EXPRESSION | NEW_EXPRESSION => {
                if let Some(data) = self.get_call_expr(node) {
                    children.push(data.expression);
                    add_opt_list(&mut children, &data.type_arguments);
                    add_opt_list(&mut children, &data.arguments);
                }
            }
            TAGGED_TEMPLATE_EXPRESSION => {
                if let Some(data) = self.get_tagged_template(node) {
                    children.push(data.tag);
                    add_opt_list(&mut children, &data.type_arguments);
                    children.push(data.template);
                }
            }
            TEMPLATE_EXPRESSION => {
                if let Some(data) = self.get_template_expr(node) {
                    children.push(data.head);
                    add_list(&mut children, &data.template_spans);
                }
            }
            TEMPLATE_SPAN => {
                if let Some(data) = self.get_template_span(node) {
                    children.push(data.expression);
                    children.push(data.literal);
                }
            }
            PROPERTY_ACCESS_EXPRESSION | ELEMENT_ACCESS_EXPRESSION => {
                if let Some(data) = self.get_access_expr(node) {
                    children.push(data.expression);
                    children.push(data.name_or_argument);
                }
            }
            CONDITIONAL_EXPRESSION => {
                if let Some(data) = self.get_conditional_expr(node) {
                    children.push(data.condition);
                    children.push(data.when_true);
                    children.push(data.when_false);
                }
            }
            ARROW_FUNCTION | FUNCTION_EXPRESSION => {
                if let Some(data) = self.get_function(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt_list(&mut children, &data.type_parameters);
                    add_list(&mut children, &data.parameters);
                    add_opt(&mut children, data.type_annotation);
                    children.push(data.body);
                }
            }
            ARRAY_LITERAL_EXPRESSION => {
                if let Some(data) = self.get_literal_expr(node) {
                    add_list(&mut children, &data.elements);
                }
            }
            OBJECT_LITERAL_EXPRESSION => {
                if let Some(data) = self.get_literal_expr(node) {
                    add_list(&mut children, &data.elements);
                }
            }
            PARENTHESIZED_EXPRESSION => {
                if let Some(data) = self.get_parenthesized(node) {
                    children.push(data.expression);
                }
            }
            YIELD_EXPRESSION => {
                if let Some(data) = self.get_unary_expr_ex(node) {
                    add_opt(&mut children, data.expression);
                }
            }
            AWAIT_EXPRESSION => {
                if let Some(data) = self.get_unary_expr_ex(node) {
                    children.push(data.expression);
                }
            }
            SPREAD_ELEMENT => {
                if let Some(data) = self.get_spread(node) {
                    children.push(data.expression);
                }
            }
            AS_EXPRESSION | SATISFIES_EXPRESSION => {
                if let Some(data) = self.get_type_assertion(node) {
                    children.push(data.expression);
                    children.push(data.type_node);
                }
            }
            TYPE_ASSERTION => {
                if let Some(data) = self.get_type_assertion(node) {
                    children.push(data.type_node);
                    children.push(data.expression);
                }
            }
            NON_NULL_EXPRESSION => {
                if let Some(data) = self.get_unary_expr_ex(node) {
                    children.push(data.expression);
                }
            }

            // Statements
            VARIABLE_STATEMENT => {
                if let Some(data) = self.get_variable(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    // Variable statements contain their declarations directly
                    add_list(&mut children, &data.declarations);
                }
            }
            VARIABLE_DECLARATION_LIST => {
                if let Some(data) = self.get_variable(node) {
                    add_list(&mut children, &data.declarations);
                }
            }
            VARIABLE_DECLARATION => {
                if let Some(data) = self.get_variable_declaration(node) {
                    children.push(data.name);
                    add_opt(&mut children, data.type_annotation);
                    add_opt(&mut children, data.initializer);
                }
            }
            EXPRESSION_STATEMENT => {
                if let Some(data) = self.get_expression_statement(node) {
                    children.push(data.expression);
                }
            }
            IF_STATEMENT => {
                if let Some(data) = self.get_if_statement(node) {
                    children.push(data.expression);
                    children.push(data.then_statement);
                    add_opt(&mut children, data.else_statement);
                }
            }
            WHILE_STATEMENT | DO_STATEMENT | FOR_STATEMENT => {
                if let Some(data) = self.get_loop(node) {
                    add_opt(&mut children, data.initializer);
                    add_opt(&mut children, data.condition);
                    add_opt(&mut children, data.incrementor);
                    children.push(data.statement);
                }
            }
            FOR_IN_STATEMENT | FOR_OF_STATEMENT => {
                if let Some(data) = self.get_for_in_of(node) {
                    children.push(data.initializer);
                    children.push(data.expression);
                    children.push(data.statement);
                }
            }
            SWITCH_STATEMENT => {
                if let Some(data) = self.get_switch(node) {
                    children.push(data.expression);
                    children.push(data.case_block);
                }
            }
            CASE_BLOCK => {
                if let Some(data) = self.get_block(node) {
                    add_list(&mut children, &data.statements);
                }
            }
            CASE_CLAUSE | DEFAULT_CLAUSE => {
                if let Some(data) = self.get_case_clause(node) {
                    add_opt(&mut children, data.expression);
                    add_list(&mut children, &data.statements);
                }
            }
            RETURN_STATEMENT => {
                if let Some(data) = self.get_return_statement(node) {
                    add_opt(&mut children, data.expression);
                }
            }
            THROW_STATEMENT => {
                if let Some(data) = self.get_return_statement(node) {
                    children.push(data.expression);
                }
            }
            TRY_STATEMENT => {
                if let Some(data) = self.get_try(node) {
                    children.push(data.try_block);
                    add_opt(&mut children, data.catch_clause);
                    add_opt(&mut children, data.finally_block);
                }
            }
            CATCH_CLAUSE => {
                if let Some(data) = self.get_catch_clause(node) {
                    add_opt(&mut children, data.variable_declaration);
                    children.push(data.block);
                }
            }
            LABELED_STATEMENT => {
                if let Some(data) = self.get_labeled_statement(node) {
                    children.push(data.label);
                    children.push(data.statement);
                }
            }
            BREAK_STATEMENT | CONTINUE_STATEMENT => {
                if let Some(data) = self.get_jump_data(node) {
                    add_opt(&mut children, data.label);
                }
            }
            WITH_STATEMENT => {
                if let Some(data) = self.get_with_statement(node) {
                    children.push(data.expression);
                    children.push(data.then_statement);
                }
            }
            BLOCK | CLASS_STATIC_BLOCK_DECLARATION => {
                if let Some(data) = self.get_block(node) {
                    add_list(&mut children, &data.statements);
                }
            }

            // Declarations
            FUNCTION_DECLARATION => {
                if let Some(data) = self.get_function(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    add_opt_list(&mut children, &data.type_parameters);
                    add_list(&mut children, &data.parameters);
                    add_opt(&mut children, data.type_annotation);
                    children.push(data.body);
                }
            }
            CLASS_DECLARATION | CLASS_EXPRESSION => {
                if let Some(data) = self.get_class(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    add_opt_list(&mut children, &data.type_parameters);
                    add_opt_list(&mut children, &data.heritage_clauses);
                    add_list(&mut children, &data.members);
                }
            }
            INTERFACE_DECLARATION => {
                if let Some(data) = self.get_interface(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    add_opt_list(&mut children, &data.type_parameters);
                    add_opt_list(&mut children, &data.heritage_clauses);
                    add_list(&mut children, &data.members);
                }
            }
            TYPE_ALIAS_DECLARATION => {
                if let Some(data) = self.get_type_alias(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    add_opt_list(&mut children, &data.type_parameters);
                    children.push(data.type_node);
                }
            }
            ENUM_DECLARATION => {
                if let Some(data) = self.get_enum(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    add_list(&mut children, &data.members);
                }
            }
            ENUM_MEMBER => {
                if let Some(data) = self.get_enum_member(node) {
                    add_opt(&mut children, data.name);
                    add_opt(&mut children, data.initializer);
                }
            }
            MODULE_DECLARATION => {
                if let Some(data) = self.get_module(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    add_opt(&mut children, data.body);
                }
            }
            MODULE_BLOCK => {
                if let Some(data) = self.get_module_block(node) {
                    add_opt_list(&mut children, &data.statements);
                }
            }

            // Import/Export
            IMPORT_DECLARATION | IMPORT_EQUALS_DECLARATION => {
                if let Some(data) = self.get_import_decl(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.import_clause);
                    children.push(data.module_specifier);
                    add_opt(&mut children, data.attributes);
                }
            }
            IMPORT_CLAUSE => {
                if let Some(data) = self.get_import_clause(node) {
                    add_opt(&mut children, data.name);
                    add_opt(&mut children, data.named_bindings);
                }
            }
            NAMESPACE_IMPORT | NAMESPACE_EXPORT => {
                if let Some(data) = self.get_named_imports(node) {
                    children.push(data.name);
                }
            }
            NAMED_IMPORTS | NAMED_EXPORTS => {
                if let Some(data) = self.get_named_imports(node) {
                    add_list(&mut children, &data.elements);
                }
            }
            IMPORT_SPECIFIER | EXPORT_SPECIFIER => {
                if let Some(data) = self.get_specifier(node) {
                    add_opt(&mut children, data.property_name);
                    children.push(data.name);
                }
            }
            EXPORT_DECLARATION => {
                if let Some(data) = self.get_export_decl(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.export_clause);
                    add_opt(&mut children, data.module_specifier);
                    add_opt(&mut children, data.attributes);
                }
            }
            EXPORT_ASSIGNMENT => {
                if let Some(data) = self.get_export_assignment(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    children.push(data.expression);
                }
            }

            // Type nodes
            TYPE_REFERENCE => {
                if let Some(data) = self.get_type_ref(node) {
                    children.push(data.type_name);
                    add_opt_list(&mut children, &data.type_arguments);
                }
            }
            FUNCTION_TYPE | CONSTRUCTOR_TYPE => {
                if let Some(data) = self.get_function_type(node) {
                    add_opt_list(&mut children, &data.type_parameters);
                    add_list(&mut children, &data.parameters);
                    children.push(data.type_annotation);
                }
            }
            TYPE_QUERY => {
                if let Some(data) = self.get_type_query(node) {
                    children.push(data.expr_name);
                    add_opt_list(&mut children, &data.type_arguments);
                }
            }
            TYPE_LITERAL => {
                if let Some(data) = self.get_type_literal(node) {
                    add_list(&mut children, &data.members);
                }
            }
            ARRAY_TYPE => {
                if let Some(data) = self.get_array_type(node) {
                    children.push(data.element_type);
                }
            }
            TUPLE_TYPE => {
                if let Some(data) = self.get_tuple_type(node) {
                    add_list(&mut children, &data.elements);
                }
            }
            OPTIONAL_TYPE | REST_TYPE | PARENTHESIZED_TYPE => {
                if let Some(data) = self.get_wrapped_type(node) {
                    children.push(data.type_node);
                }
            }
            UNION_TYPE | INTERSECTION_TYPE => {
                if let Some(data) = self.get_composite_type(node) {
                    add_list(&mut children, &data.types);
                }
            }
            CONDITIONAL_TYPE => {
                if let Some(data) = self.get_conditional_type(node) {
                    children.push(data.check_type);
                    children.push(data.extends_type);
                    children.push(data.true_type);
                    children.push(data.false_type);
                }
            }
            INFER_TYPE => {
                if let Some(data) = self.get_infer_type(node) {
                    children.push(data.type_parameter);
                }
            }
            TYPE_OPERATOR => {
                if let Some(data) = self.get_type_operator(node) {
                    children.push(data.type_node);
                }
            }
            INDEXED_ACCESS_TYPE => {
                if let Some(data) = self.get_indexed_access_type(node) {
                    children.push(data.object_type);
                    children.push(data.index_type);
                }
            }
            MAPPED_TYPE => {
                if let Some(data) = self.get_mapped_type(node) {
                    add_opt(&mut children, data.type_parameter);
                    add_opt(&mut children, data.name_type);
                    add_opt(&mut children, data.type_node);
                    add_opt_list(&mut children, &data.members);
                }
            }
            LITERAL_TYPE => {
                if let Some(data) = self.get_literal_type(node) {
                    add_opt(&mut children, data.literal);
                }
            }
            TEMPLATE_LITERAL_TYPE => {
                if let Some(data) = self.get_template_literal_type(node) {
                    children.push(data.head);
                    add_list(&mut children, &data.template_spans);
                }
            }
            NAMED_TUPLE_MEMBER => {
                if let Some(data) = self.get_named_tuple_member(node) {
                    children.push(data.name);
                    children.push(data.type_node);
                }
            }
            TYPE_PREDICATE => {
                if let Some(data) = self.get_type_predicate(node) {
                    children.push(data.parameter_name);
                    add_opt(&mut children, data.type_node);
                }
            }

            // Class members
            PROPERTY_DECLARATION => {
                if let Some(data) = self.get_property_decl(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    add_opt(&mut children, data.type_annotation);
                    add_opt(&mut children, data.initializer);
                }
            }
            METHOD_DECLARATION => {
                if let Some(data) = self.get_method_decl(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    add_opt_list(&mut children, &data.type_parameters);
                    add_list(&mut children, &data.parameters);
                    add_opt(&mut children, data.type_annotation);
                    children.push(data.body);
                }
            }
            CONSTRUCTOR => {
                if let Some(data) = self.get_constructor(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt_list(&mut children, &data.type_parameters);
                    add_list(&mut children, &data.parameters);
                    children.push(data.body);
                }
            }
            GET_ACCESSOR | SET_ACCESSOR => {
                if let Some(data) = self.get_accessor(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    add_opt_list(&mut children, &data.type_parameters);
                    add_list(&mut children, &data.parameters);
                    add_opt(&mut children, data.type_annotation);
                    children.push(data.body);
                }
            }
            PARAMETER => {
                if let Some(data) = self.get_parameter(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    add_opt(&mut children, data.type_annotation);
                    add_opt(&mut children, data.initializer);
                }
            }
            TYPE_PARAMETER => {
                if let Some(data) = self.get_type_parameter(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    children.push(data.name);
                    add_opt(&mut children, data.constraint);
                    add_opt(&mut children, data.default);
                }
            }
            DECORATOR => {
                if let Some(data) = self.get_decorator(node) {
                    children.push(data.expression);
                }
            }
            HERITAGE_CLAUSE => {
                if let Some(data) = self.get_heritage_clause(node) {
                    add_list(&mut children, &data.types);
                }
            }
            EXPRESSION_WITH_TYPE_ARGUMENTS => {
                if let Some(data) = self.get_expr_type_args(node) {
                    children.push(data.expression);
                    add_opt_list(&mut children, &data.type_arguments);
                }
            }

            // Binding patterns
            OBJECT_BINDING_PATTERN | ARRAY_BINDING_PATTERN => {
                if let Some(data) = self.get_binding_pattern(node) {
                    add_list(&mut children, &data.elements);
                }
            }
            BINDING_ELEMENT => {
                if let Some(data) = self.get_binding_element(node) {
                    add_opt(&mut children, data.property_name);
                    children.push(data.name);
                    add_opt(&mut children, data.initializer);
                }
            }

            // Object literal members
            PROPERTY_ASSIGNMENT => {
                if let Some(data) = self.get_property_assignment(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    children.push(data.initializer);
                }
            }
            SHORTHAND_PROPERTY_ASSIGNMENT => {
                if let Some(data) = self.get_shorthand_property(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    children.push(data.name);
                    add_opt(&mut children, data.object_assignment_initializer);
                }
            }
            SPREAD_ASSIGNMENT => {
                if let Some(data) = self.get_spread(node) {
                    children.push(data.expression);
                }
            }

            // JSX nodes
            JSX_ELEMENT => {
                if let Some(data) = self.get_jsx_element(node) {
                    children.push(data.opening_element);
                    add_list(&mut children, &data.children);
                    add_opt(&mut children, data.closing_element);
                }
            }
            JSX_SELF_CLOSING_ELEMENT | JSX_OPENING_ELEMENT => {
                if let Some(data) = self.get_jsx_opening(node) {
                    children.push(data.tag_name);
                    add_opt_list(&mut children, &data.type_arguments);
                    add_opt(&mut children, data.attributes);
                }
            }
            JSX_CLOSING_ELEMENT => {
                if let Some(data) = self.get_jsx_closing(node) {
                    children.push(data.tag_name);
                }
            }
            JSX_FRAGMENT => {
                if let Some(data) = self.get_jsx_fragment(node) {
                    children.push(data.opening_fragment);
                    add_list(&mut children, &data.children);
                    children.push(data.closing_fragment);
                }
            }
            JSX_OPENING_FRAGMENT | JSX_CLOSING_FRAGMENT => {
                // No children
            }
            JSX_ATTRIBUTES => {
                if let Some(data) = self.get_jsx_attributes(node) {
                    add_list(&mut children, &data.properties);
                }
            }
            JSX_ATTRIBUTE => {
                if let Some(data) = self.get_jsx_attribute(node) {
                    children.push(data.name);
                    add_opt(&mut children, data.initializer);
                }
            }
            JSX_SPREAD_ATTRIBUTE => {
                if let Some(data) = self.get_jsx_spread_attribute(node) {
                    children.push(data.expression);
                }
            }
            JSX_EXPRESSION => {
                if let Some(data) = self.get_jsx_expression(node) {
                    add_opt(&mut children, data.expression);
                }
            }
            JSX_NAMESPACED_NAME => {
                if let Some(data) = self.get_jsx_namespaced_name(node) {
                    children.push(data.namespace);
                    children.push(data.name);
                }
            }

            // Signatures
            CALL_SIGNATURE | CONSTRUCT_SIGNATURE => {
                if let Some(data) = self.get_signature(node) {
                    add_opt_list(&mut children, &data.type_parameters);
                    add_opt_list(&mut children, &data.parameters);
                    add_opt(&mut children, data.type_annotation);
                }
            }
            INDEX_SIGNATURE => {
                if let Some(data) = self.get_index_signature(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_list(&mut children, &data.parameters);
                    add_opt(&mut children, data.type_annotation);
                }
            }
            PROPERTY_SIGNATURE => {
                if let Some(data) = self.get_signature(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    add_opt(&mut children, data.type_annotation);
                    // Note: SignatureData doesn't have initializer, property signatures in thin nodes
                    // use the same structure as method signatures
                }
            }
            METHOD_SIGNATURE => {
                if let Some(data) = self.get_signature(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    add_opt_list(&mut children, &data.type_parameters);
                    add_opt_list(&mut children, &data.parameters);
                    add_opt(&mut children, data.type_annotation);
                }
            }

            // Source file
            SOURCE_FILE => {
                if let Some(data) = self.get_source_file(node) {
                    add_list(&mut children, &data.statements);
                    children.push(data.end_of_file_token);
                }
            }

            // Nodes with no children (tokens, identifiers, literals)
            _ => {
                // Tokens, identifiers, literals, etc. have no children
            }
        }

        children
    }
}

#[cfg(test)]
#[path = "node_tests.rs"]
mod node_tests;
