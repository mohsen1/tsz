//! Thin `Node` Architecture for Cache-Efficient AST
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
use super::node_pools::for_each_node_pool;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
pub use tsz_common::interner::IdentText;
use tsz_common::interner::{AstAtom, Interner};

/// A thin 16-byte node header for cache-efficient AST storage.
///
/// Layout (16 bytes total):
/// - `kind`: 2 bytes (`SyntaxKind` value, supports 0-65535)
/// - `flags`: 2 bytes (packed `NodeFlags`)
/// - `pos`: 4 bytes (start position in source)
/// - `end`: 4 bytes (end position in source)
/// - `data_index`: 4 bytes (index into type-specific pool, `u32::MAX` = no data)
#[repr(C)]
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Node {
    /// `SyntaxKind` value (u16 to support extended kinds up to 400+)
    pub kind: u16,
    /// Packed node flags (subset of `NodeFlags` that fits in u16)
    pub flags: u16,
    /// Start position in source (character index)
    pub pos: u32,
    /// End position in source (character index)
    pub end: u32,
    /// Index into the type-specific storage pool (`u32::MAX` = no data)
    pub data_index: u32,
}

impl Node {
    pub const NO_DATA: u32 = u32::MAX;

    /// Create a new thin node with no associated data
    #[inline]
    #[must_use]
    pub const fn new(kind: u16, pos: u32, end: u32) -> Self {
        Self {
            kind,
            flags: 0,
            pos,
            end,
            data_index: Self::NO_DATA,
        }
    }

    /// Create a new thin node with data index
    #[inline]
    #[must_use]
    pub const fn with_data(kind: u16, pos: u32, end: u32, data_index: u32) -> Self {
        Self {
            kind,
            flags: 0,
            pos,
            end,
            data_index,
        }
    }

    /// Create a new thin node with data index and flags
    #[inline]
    #[must_use]
    pub const fn with_data_and_flags(
        kind: u16,
        pos: u32,
        end: u32,
        data_index: u32,
        flags: u16,
    ) -> Self {
        Self {
            kind,
            flags,
            pos,
            end,
            data_index,
        }
    }

    /// Check if this node has associated data
    #[inline]
    #[must_use]
    pub const fn has_data(&self) -> bool {
        self.data_index != Self::NO_DATA
    }

    /// Return `true` if any of the given `NodeFlags` bits are set.
    ///
    /// Only flags whose bit fits in the u16 `flags` field are observable via
    /// this method; higher-bit `NodeFlags` values are stored elsewhere and
    /// will always return `false` here.
    #[inline]
    #[must_use]
    pub const fn has_any_node_flags(&self, mask: u32) -> bool {
        (self.flags as u32 & mask) != 0
    }

    /// `true` if this node carries the `OPTIONAL_CHAIN` flag.
    #[inline]
    #[must_use]
    pub const fn is_optional_chain(&self) -> bool {
        self.has_any_node_flags(super::flags::node_flags::OPTIONAL_CHAIN)
    }

    /// `true` if this node carries the `GLOBAL_AUGMENTATION` flag.
    #[inline]
    #[must_use]
    pub const fn is_global_augmentation(&self) -> bool {
        self.has_any_node_flags(super::flags::node_flags::GLOBAL_AUGMENTATION)
    }

    /// `true` if this node carries the `NAMESPACE` flag.
    #[inline]
    #[must_use]
    pub const fn has_namespace_flag(&self) -> bool {
        self.has_any_node_flags(super::flags::node_flags::NAMESPACE)
    }

    /// `true` if this node carries the `THIS_NODE_HAS_ERROR` flag.
    #[inline]
    #[must_use]
    pub const fn this_node_has_error(&self) -> bool {
        self.has_any_node_flags(super::flags::node_flags::THIS_NODE_HAS_ERROR)
    }

    /// `true` if this node or any of its sub-nodes has an error.
    #[inline]
    #[must_use]
    pub const fn this_or_subtree_has_error(&self) -> bool {
        self.has_any_node_flags(super::flags::node_flags::THIS_NODE_OR_ANY_SUB_NODES_HAS_ERROR)
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
    /// Type nodes (`TypeReference`, `UnionType`, etc.)
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

/// Declare a family of AST node-data structs that share the standard
/// `#[derive(Clone, Debug, Serialize, Deserialize)]` surface, spelling those
/// four derives in exactly one place instead of on every struct.
///
/// Each wrapped struct keeps its own doc comments, container attributes, and
/// field-level `#[serde(...)]` attributes verbatim, so the generated output is
/// byte-identical to writing the derive on each struct. Node-data variants that
/// need a different derive set (for example the `Copy` headers or the `Default`
/// arenas) stay outside this macro with their own explicit derive.
macro_rules! node_data_structs {
    (
        $(
            $(#[$meta:meta])*
            $vis:vis struct $name:ident { $($body:tt)* }
        )+
    ) => {
        $(
            $(#[$meta])*
            #[derive(Clone, Debug, Serialize, Deserialize)]
            $vis struct $name { $($body)* }
        )+
    };
}

node_data_structs! {
/// Data for identifier nodes (`Identifier`, `PrivateIdentifier`)
pub struct IdentifierData {
    /// Interned atom for O(1) comparison (`OPTIMIZATION`: use this instead of `escaped_text`).
    /// `AstAtom` indices are stable within a single arena because they are
    /// allocated by the arena's per-arena `Interner`. Round-tripping the
    /// arena (parser snapshot pipeline, see
    /// `docs/plan/PERFORMANCE_PLAN.md`) requires the atom to
    /// survive — otherwise identifier resolution silently breaks. The
    /// `AstAtom::none` `default` is retained for backward-compatible
    /// JSON inputs that omit the field (e.g. snapshots produced before
    /// the field became serialised).
    #[serde(default = "AstAtom::none")]
    pub atom: AstAtom,
    /// The identifier's cooked text as a shared handle into the arena
    /// interner's string table: all occurrences of the same identifier in a
    /// file share one allocation instead of each node owning a `String` copy.
    pub escaped_text: IdentText,
    /// The original source spelling when it differs from the cooked text
    /// (identifiers written with unicode escapes); `None` otherwise.
    pub original_text: Option<IdentText>,
}

/// Data for string literals (`StringLiteral`, template parts)
pub struct LiteralData {
    pub text: String,
    pub raw_text: Option<String>,
    /// For numeric literals only
    pub value: Option<f64>,
    /// `serde(default)` keeps deserialise back-compat with older outputs
    /// that elided this field. We dropped `skip_serializing_if` because
    /// the lib-snapshot pipeline serializes `NodeArena, NodeArenaInner` via bincode, and
    /// bincode's positional format desyncs on conditionally-elided
    /// fields. Always emitting a 1-byte bool adds <0.1% to JSON IPC
    /// payloads and is invisible in binary. See
    /// `crates/tsz-core/src/parallel/lib_snapshot.rs`.
    #[serde(default)]
    pub has_invalid_escape: bool,
}

/// Data for binary expressions
pub struct BinaryExprData {
    pub left: NodeIndex,
    pub operator_token: u16, // SyntaxKind
    pub right: NodeIndex,
}

/// Data for unary expressions (prefix/postfix)
pub struct UnaryExprData {
    pub operator: u16, // SyntaxKind
    pub operand: NodeIndex,
}

/// Data for call/new expressions
pub struct CallExprData {
    pub expression: NodeIndex,
    pub type_arguments: Option<NodeList>,
    pub arguments: Option<NodeList>,
    /// `true` when the call itself is written with `?.` before its argument
    /// list (`f?.()`, `o.m?.<T>()`), mirroring tsc's `questionDotToken` on
    /// `CallChain`. A call that merely continues a chain (`o?.f()`) carries
    /// the `OPTIONAL_CHAIN` node flag but leaves this `false`; the distinction
    /// decides whether `?.` guards the invoked value itself.
    pub question_dot_token: bool,
}

/// Data for property/element access
pub struct AccessExprData {
    pub expression: NodeIndex,
    pub name_or_argument: NodeIndex,
    pub question_dot_token: bool,
}

/// Data for function declarations/expressions/arrows
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
pub struct ClassData {
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub type_parameters: Option<NodeList>,
    pub heritage_clauses: Option<NodeList>,
    pub members: NodeList,
}

/// Data for if statements
pub struct IfStatementData {
    pub expression: NodeIndex,
    pub then_statement: NodeIndex,
    pub else_statement: NodeIndex,
}

/// Data for for/while/do loops
pub struct LoopData {
    pub initializer: NodeIndex,
    pub condition: NodeIndex,
    pub incrementor: NodeIndex,
    pub statement: NodeIndex,
}

/// Data for block statements
pub struct BlockData {
    pub statements: NodeList,
    pub multi_line: bool,
}
}

/// Data for expression statements
#[derive(Clone, Copy, Debug)]
pub struct ExpressionStatementData {
    pub expression: NodeIndex,
}

node_data_structs! {
/// Parser-owned recovery fact for malformed `''.typeof(expr)` declaration tails.
pub struct RecoveredTypeofMemberCallData {
    pub argument_pos: u32,
    pub argument_end: u32,
}

/// Data for variable declarations
pub struct VariableData {
    pub modifiers: Option<NodeList>,
    pub declarations: NodeList,
    pub recovered_typeof_member_calls: Vec<RecoveredTypeofMemberCallData>,
}

/// Data for type references
pub struct TypeRefData {
    pub type_name: NodeIndex,
    pub type_arguments: Option<NodeList>,
}

/// Data for union/intersection types
pub struct CompositeTypeData {
    pub types: NodeList,
}

/// Data for conditional expressions (a ? b : c)
pub struct ConditionalExprData {
    pub condition: NodeIndex,
    pub when_true: NodeIndex,
    pub when_false: NodeIndex,
}

/// Data for object/array literals
pub struct LiteralExprData {
    pub elements: NodeList,
    pub multi_line: bool,
}

/// Data for parenthesized expressions
pub struct ParenthesizedData {
    pub expression: NodeIndex,
}

/// Data for spread/await/yield expressions
pub struct UnaryExprDataEx {
    pub expression: NodeIndex,
    pub asterisk_token: bool, // For yield*
}

/// Data for as/satisfies/type assertion expressions
pub struct TypeAssertionData {
    pub expression: NodeIndex,
    pub type_node: NodeIndex,
    /// Position of the `as` or `satisfies` keyword token (used for TS1360 diagnostic spans).
    pub keyword_pos: u32,
}

/// Data for return/throw statements
pub struct ReturnData {
    pub expression: NodeIndex,
}

/// Data for expression statements
pub struct ExprStatementData {
    pub expression: NodeIndex,
}

/// Data for switch statements
pub struct SwitchData {
    pub expression: NodeIndex,
    pub case_block: NodeIndex,
}

/// Data for case/default clauses
pub struct CaseClauseData {
    pub expression: NodeIndex, // NONE for default clause
    pub statements: NodeList,
}

/// Data for try statements
pub struct TryData {
    pub try_block: NodeIndex,
    pub catch_clause: NodeIndex,
    pub finally_block: NodeIndex,
}

/// Data for catch clauses
pub struct CatchClauseData {
    pub variable_declaration: NodeIndex,
    pub block: NodeIndex,
}

/// Data for labeled statements
pub struct LabeledData {
    pub label: NodeIndex,
    pub statement: NodeIndex,
}

/// Data for break/continue statements
pub struct JumpData {
    pub label: NodeIndex,
}

/// Data for with statements
pub struct WithData {
    pub expression: NodeIndex,
    pub statement: NodeIndex,
}

/// Data for interface declarations
pub struct InterfaceData {
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub type_parameters: Option<NodeList>,
    pub heritage_clauses: Option<NodeList>,
    pub members: NodeList,
}

/// Data for type alias declarations
pub struct TypeAliasData {
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub type_parameters: Option<NodeList>,
    pub type_node: NodeIndex,
}

/// Data for enum declarations
pub struct EnumData {
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub members: NodeList,
}

/// Data for enum members
pub struct EnumMemberData {
    pub name: NodeIndex,
    pub initializer: NodeIndex,
}

/// Data for module/namespace declarations
pub struct ModuleData {
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub body: NodeIndex,
}

/// Data for module blocks: { statements }
pub struct ModuleBlockData {
    pub statements: Option<NodeList>,
}

/// Data for property/method signatures
pub struct SignatureData {
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub question_token: bool,
    pub type_parameters: Option<NodeList>,
    pub parameters: Option<NodeList>,
    pub type_annotation: NodeIndex,
}

/// Data for index signatures
pub struct IndexSignatureData {
    pub modifiers: Option<NodeList>,
    pub parameters: NodeList,
    pub type_annotation: NodeIndex,
    /// True when the parser already reported a parameter-arity error (TS1096,
    /// "An index signature must have exactly one parameter.") for an empty
    /// `[]` or a multi-parameter `[a, b]` signature. The multi-parameter form
    /// is recovered down to a single parameter node, so `parameters.len()`
    /// alone cannot distinguish it; the checker consults this flag to replicate
    /// tsc's early return (TS1096 suppresses the later TS1021).
    pub had_parameter_arity_error: bool,
}

/// Data for property declarations
pub struct PropertyDeclData {
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub question_token: bool,
    pub exclamation_token: bool,
    pub type_annotation: NodeIndex,
    pub initializer: NodeIndex,
}

/// Data for method declarations (class methods)
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
pub struct ConstructorData {
    pub modifiers: Option<NodeList>,
    pub type_parameters: Option<NodeList>,
    pub parameters: NodeList,
    pub body: NodeIndex,
}

/// Data for accessor declarations (get/set)
pub struct AccessorData {
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub type_parameters: Option<NodeList>,
    pub parameters: NodeList,
    pub type_annotation: NodeIndex,
    pub body: NodeIndex,
}

/// Data for parameter declarations
pub struct ParameterData {
    pub modifiers: Option<NodeList>,
    pub dot_dot_dot_token: bool,
    pub name: NodeIndex,
    pub question_token: bool,
    pub type_annotation: NodeIndex,
    pub initializer: NodeIndex,
}

/// Data for type parameter declarations
pub struct TypeParameterData {
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub constraint: NodeIndex,
    pub default: NodeIndex,
}

/// Data for decorator nodes
pub struct DecoratorData {
    pub expression: NodeIndex,
}

/// Data for heritage clauses
pub struct HeritageData {
    pub token: u16, // ExtendsKeyword or ImplementsKeyword
    pub types: NodeList,
}

/// Data for expression with type arguments
pub struct ExprWithTypeArgsData {
    pub expression: NodeIndex,
    pub type_arguments: Option<NodeList>,
}

/// Data for import declarations
pub struct ImportDeclData {
    pub modifiers: Option<NodeList>,
    /// For `import type X = require(...)` (import-equals only): true when the `type` keyword was present.
    /// For regular import declarations, this is always false (type-only info lives in `ImportClauseData`).
    pub is_type_only: bool,
    pub import_clause: NodeIndex,
    pub module_specifier: NodeIndex,
    pub attributes: NodeIndex,
}

/// Data for import clauses
pub struct ImportClauseData {
    pub is_type_only: bool,
    pub is_deferred: bool,
    pub name: NodeIndex,
    pub named_bindings: NodeIndex,
}

/// Data for namespace/named imports
pub struct NamedImportsData {
    pub name: NodeIndex,    // For namespace import
    pub elements: NodeList, // For named imports
}

/// Data for import/export specifiers
pub struct SpecifierData {
    pub is_type_only: bool,
    pub property_name: NodeIndex,
    pub name: NodeIndex,
}

/// Data for export declarations
pub struct ExportDeclData {
    pub modifiers: Option<NodeList>,
    pub is_type_only: bool,
    /// True if this is `export default ...`
    pub is_default_export: bool,
    /// Position of the `default` keyword token (used for TS1319 diagnostic spans).
    /// Only set when `is_default_export` is true.
    pub default_keyword_pos: Option<u32>,
    pub export_clause: NodeIndex,
    pub module_specifier: NodeIndex,
    pub attributes: NodeIndex,
}

/// Data for export assignments
pub struct ExportAssignmentData {
    pub modifiers: Option<NodeList>,
    pub is_export_equals: bool,
    pub expression: NodeIndex,
}

/// Data for import attributes
pub struct ImportAttributesData {
    pub token: u16,
    pub elements: NodeList,
    pub multi_line: bool,
}

/// Data for import attribute
pub struct ImportAttributeData {
    pub name: NodeIndex,
    pub value: NodeIndex,
}

/// Data for binding patterns
pub struct BindingPatternData {
    pub elements: NodeList,
}

/// Data for binding elements
pub struct BindingElementData {
    pub dot_dot_dot_token: bool,
    pub property_name: NodeIndex,
    pub name: NodeIndex,
    pub initializer: NodeIndex,
}

/// Data for property assignments
pub struct PropertyAssignmentData {
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub initializer: NodeIndex,
}

/// Data for shorthand property assignments
pub struct ShorthandPropertyData {
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub equals_token: bool,
    /// Position of the `=` token in a cover-initialized shorthand (`{ x = expr }`).
    /// 0 means no equals token was present.
    pub equals_token_pos: u32,
    /// Position of a `!` (definite assignment assertion) that was parsed and skipped.
    /// 0 means no exclamation token was present.
    pub exclamation_token_pos: u32,
    /// Position of a `?` (optional marker) that was parsed and skipped.
    /// This is a grammar error (TS1162), but tsc still infers an optional property
    /// for the object type. 0 means no question token was present.
    pub question_token_pos: u32,
    pub object_assignment_initializer: NodeIndex,
}

/// Data for spread assignments
pub struct SpreadData {
    pub expression: NodeIndex,
}

/// Data for variable declarations (individual)
pub struct VariableDeclarationData {
    pub name: NodeIndex,            // Identifier or BindingPattern
    pub exclamation_token: bool,    // Definite assignment assertion
    pub type_annotation: NodeIndex, // TypeNode (optional)
    pub initializer: NodeIndex,     // Expression (optional)
}

/// Data for for-in/for-of statements
pub struct ForInOfData {
    pub await_modifier: bool,   // For for-await-of
    pub initializer: NodeIndex, // Variable declaration or expression
    pub expression: NodeIndex,  // The iterable expression
    pub statement: NodeIndex,   // The loop body
}

/// Data for debugger/empty statements (no data needed, use token)
///
/// Data for template expressions
pub struct TemplateExprData {
    pub head: NodeIndex,
    pub template_spans: NodeList,
}

/// Data for template spans
pub struct TemplateSpanData {
    pub expression: NodeIndex,
    pub literal: NodeIndex,
}

/// Data for tagged template expressions
pub struct TaggedTemplateData {
    pub tag: NodeIndex,
    pub type_arguments: Option<NodeList>,
    pub template: NodeIndex,
}

/// Data for qualified names
pub struct QualifiedNameData {
    pub left: NodeIndex,
    pub right: NodeIndex,
}

/// Data for computed property names
pub struct ComputedPropertyData {
    pub expression: NodeIndex,
}

/// Data for type nodes (function type, constructor type)
pub struct FunctionTypeData {
    pub type_parameters: Option<NodeList>,
    pub parameters: NodeList,
    pub type_annotation: NodeIndex,
    /// True if this is an abstract constructor type: `abstract new () => T`.
    /// `skip_serializing_if` removed for bincode round-trip compatibility
    /// (see `LiteralData::has_invalid_escape` for the same rationale).
    #[serde(default)]
    pub is_abstract: bool,
}

/// Data for type query (typeof)
pub struct TypeQueryData {
    pub expr_name: NodeIndex,
    pub type_arguments: Option<NodeList>,
}

/// Data for type literal
pub struct TypeLiteralData {
    pub members: NodeList,
}

/// Data for array type
pub struct ArrayTypeData {
    pub element_type: NodeIndex,
}

/// Data for tuple type
pub struct TupleTypeData {
    pub elements: NodeList,
}

/// Data for optional/rest types
pub struct WrappedTypeData {
    pub type_node: NodeIndex,
}

/// Data for conditional types
pub struct ConditionalTypeData {
    pub check_type: NodeIndex,
    pub extends_type: NodeIndex,
    pub true_type: NodeIndex,
    pub false_type: NodeIndex,
}

/// Data for infer type
pub struct InferTypeData {
    pub type_parameter: NodeIndex,
}

/// Data for type operator (keyof, unique, readonly)
pub struct TypeOperatorData {
    pub operator: u16,
    pub type_node: NodeIndex,
}

/// Data for indexed access type
pub struct IndexedAccessTypeData {
    pub object_type: NodeIndex,
    pub index_type: NodeIndex,
}

/// Data for mapped type
pub struct MappedTypeData {
    pub readonly_token: NodeIndex,
    pub type_parameter: NodeIndex,
    pub name_type: NodeIndex,
    pub question_token: NodeIndex,
    pub type_node: NodeIndex,
    pub members: Option<NodeList>,
}

/// Data for literal types
pub struct LiteralTypeData {
    pub literal: NodeIndex,
}

/// Data for template literal types
pub struct TemplateLiteralTypeData {
    pub head: NodeIndex,
    pub template_spans: NodeList,
}

/// Data for named tuple member
pub struct NamedTupleMemberData {
    pub dot_dot_dot_token: bool,
    pub name: NodeIndex,
    pub question_token: bool,
    pub type_node: NodeIndex,
}

/// Data for type predicate
pub struct TypePredicateData {
    pub asserts_modifier: bool,
    pub parameter_name: NodeIndex,
    pub type_node: NodeIndex,
}

/// Data for JSX elements
pub struct JsxElementData {
    pub opening_element: NodeIndex,
    pub children: NodeList,
    pub closing_element: NodeIndex,
}

/// Data for JSX self-closing/opening elements
pub struct JsxOpeningData {
    pub tag_name: NodeIndex,
    pub type_arguments: Option<NodeList>,
    pub attributes: NodeIndex,
}

/// Data for JSX closing elements
pub struct JsxClosingData {
    pub tag_name: NodeIndex,
}

/// Data for JSX fragments
pub struct JsxFragmentData {
    pub opening_fragment: NodeIndex,
    pub children: NodeList,
    pub closing_fragment: NodeIndex,
}

/// Data for JSX attributes
pub struct JsxAttributesData {
    pub properties: NodeList,
}

/// Data for JSX attribute
pub struct JsxAttributeData {
    pub name: NodeIndex,
    pub initializer: NodeIndex,
}

/// Data for JSX spread attribute
pub struct JsxSpreadAttributeData {
    pub expression: NodeIndex,
}

/// Data for JSX expression
pub struct JsxExpressionData {
    pub dot_dot_dot_token: bool,
    pub expression: NodeIndex,
}

/// Data for JSX text
pub struct JsxTextData {
    pub text: String,
    pub contains_only_trivia_white_spaces: bool,
}

/// Data for JSX namespaced name
pub struct JsxNamespacedNameData {
    pub namespace: NodeIndex,
    pub name: NodeIndex,
}

/// Data for source files
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
    pub comments: Vec<tsz_common::comments::CommentRange>,
    // Extended node info (parent, id, modifiers, transform_flags)
    pub parent: NodeIndex,
    pub id: u32,
    pub modifier_flags: u32,
    pub transform_flags: u32,
}
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
// Arena pool checkpoint
// =============================================================================

/// Generate [`NodeArenaPoolLengths`] from the canonical pool registry in
/// [`super::node_pools`], so the snapshot struct can never drift from the
/// [`NodeArenaInner`] pool fields or the checkpoint methods.
macro_rules! declare_pool_lengths {
    ($($pool:ident => $elem:ty),+ $(,)?) => {
        /// Lengths of every typed data pool in a [`NodeArena`] at a point in time.
        ///
        /// The speculation machinery in `speculation.rs` captures this snapshot before
        /// a speculative `parse_*` call and restores it on rollback. Without this,
        /// failed speculations leave orphaned entries in every typed pool (identifiers,
        /// `type_refs`, etc.) even though the corresponding node headers are truncated.
        /// The orphaned data inflates peak memory and degrades cache efficiency,
        /// causing super-linear slowdowns on files with many complex recursive types
        /// such as the `utility-types-project` benchmark row.
        ///
        /// Every field is a `usize` (the pool's `Vec::len()`). The struct is cheap to
        /// construct (`O(1)` field reads) and cheap to restore (`truncate` on each
        /// pool, which is `O(dropped)` but the drop cost is paid at the moment of
        /// rollback rather than deferred to the arena's `clear()` call).
        #[derive(Default)]
        pub(crate) struct NodeArenaPoolLengths {
            $(pub $pool: usize,)+
        }
    };
}
for_each_node_pool!(declare_pool_lengths);

// =============================================================================
// Thin Node Arena
// =============================================================================

node_data_structs! {
/// A class-body member that parser error recovery dropped after it matched
/// tsc's `var <name>() { }` statement-level recovery shape: a `var` keyword
/// treated as an invalid member modifier, a plain identifier name, an empty
/// parameter list, no type parameters or return type, and an empty `{ }`
/// body.
///
/// tsc parses this construct as trailing statements (`var <name>;` plus an
/// arrow function recovered from `() { }`), so the class emitters append the
/// equivalent tail after the class output. The parser records the recovery
/// here so emit consumes parser-owned AST data instead of re-scanning raw
/// source text.
pub struct ClassBodyVarFnRecovery {
    /// Start position of the dropped member (the `var` keyword, or the first
    /// modifier when other modifiers precede it). Always inside the span of
    /// the class whose body the member appeared in.
    pub pos: u32,
    /// Recovered declaration name, preserving any original escape spelling.
    pub name: String,
}
}

/// Generate [`NodeArenaInner`] with one `Vec<ElementType>` field per entry of
/// the canonical pool registry in [`super::node_pools`].
///
/// The typed pool fields exist only through that table, so a new pool cannot
/// be added without registering it — and every other generated surface
/// (checkpoints, size accounting, clearing) picks it up automatically.
macro_rules! declare_node_arena_inner {
    ($($pool:ident => $elem:ty),+ $(,)?) => {
        /// Arena for thin nodes with typed data pools.
        /// Provides O(1) allocation and cache-efficient storage.
        ///
        /// The typed data pool fields (everything except `nodes`, `interner`,
        /// and `extended_info`) are generated from the canonical pool registry
        /// in [`super::node_pools`], in registry (and thus serde) order.
        #[derive(Clone, Debug, Default, Serialize, Deserialize)]
        pub struct NodeArenaInner {
            /// The thin node headers (16 bytes each)
            pub nodes: Vec<Node>,

            /// String interner for resolving identifier atoms
            /// This is populated from the scanner after parsing completes.
            /// Round-tripped via `Interner`'s custom `Serialize`/`Deserialize`
            /// (which writes only the `strings` Vec; the lookup map is rebuilt
            /// on load). This is required for snapshot round-trip to preserve
            /// identifier text — node `IdentifierData` references atoms by
            /// index, and stripping the interner would leave them unresolvable.
            pub interner: Interner,

            // Typed data pools — generated from `node_pools::for_each_node_pool!`.
            $(#[serde(default)] pub $pool: Vec<$elem>,)+

            /// Extended node info (for nodes that need parent, id, full flags)
            pub extended_info: Vec<ExtendedNodeInfo>,
        }
    };
}
for_each_node_pool!(declare_node_arena_inner);

/// Cheap-to-clone wrapper around the parse-immutable node arena.
///
/// The parser builds the arena through `DerefMut` (`Arc::make_mut`, free at
/// refcount 1 during construction). Once built and shared — e.g. deep-cloned
/// per checker by `clone_lib_files_for_checker` — the heavy typed pools are
/// `Arc`-shared read-only, so cloning a `NodeArena` is an `Arc` bump instead of
/// a deep copy of every pool. The distinct outer `Arc<NodeArenaInner>` identity
/// is preserved per clone, so arena-pointer comparisons (lib-origin / current-
/// file discriminators) behave exactly as with the prior deep clone.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NodeArena {
    inner: std::sync::Arc<NodeArenaInner>,
}

impl std::ops::Deref for NodeArena {
    type Target = NodeArenaInner;
    #[inline]
    fn deref(&self) -> &NodeArenaInner {
        &self.inner
    }
}

impl std::ops::DerefMut for NodeArena {
    #[inline]
    fn deref_mut(&mut self) -> &mut NodeArenaInner {
        std::sync::Arc::make_mut(&mut self.inner)
    }
}

impl NodeArena {
    /// Construct an empty arena.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: std::sync::Arc::new(NodeArenaInner::new()),
        }
    }

    /// Stable identity key for `AstAtom`s minted by this arena's interner.
    ///
    /// `AstAtom` raw values are only meaningful within one `NodeArena`.
    /// Callers that build atom-keyed side indexes must include this owner key
    /// with the atom so a same-number atom from another file cannot collide.
    #[inline]
    #[must_use]
    pub fn atom_owner_key(&self) -> usize {
        Arc::as_ptr(&self.inner) as usize
    }

    /// Whether `self` and `other` share the same underlying node storage
    /// (`Arc<NodeArenaInner>`).
    ///
    /// Two `NodeArena` values that are cheap clones of the same parsed source
    /// share storage even though their outer wrapper addresses differ (cloning
    /// bumps the inner `Arc` rather than deep-copying the pools); two
    /// independently parsed arenas never do. Use this — not `std::ptr::eq` on
    /// the wrapper — when the question is "is this the same parsed file?"
    /// across clones, e.g. confirming a merged lib declaration's provenance
    /// arena is the one currently being walked (issue #15687 follow-up).
    #[inline]
    #[must_use]
    pub fn shares_node_storage_with(&self, other: &NodeArena) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// Construct an arena with pool capacity pre-reserved for `capacity` nodes.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: std::sync::Arc::new(NodeArenaInner::with_capacity(capacity)),
        }
    }
}

/// Generate inherent `NodeArena` allocation wrappers that delegate to the inner
/// arena via `Arc::make_mut`.
///
/// These shadow the `Deref`-reachable `NodeArenaInner` methods of the same name
/// so that the parser's `self.arena.add_x(self.token_end())` calls resolve to a
/// real inherent method (a two-phase-borrow receiver) rather than an explicit
/// `DerefMut::deref_mut` call — preserving the two-phase borrow that lets an
/// argument read `self` while the receiver is mutably borrowed. `make_mut` is
/// free at refcount 1 (the parse-build case).
macro_rules! delegate_arena_alloc {
    ($($name:ident($($arg:ident: $ty:ty),* $(,)?));+ $(;)?) => {
        impl NodeArena {
            $(
                #[inline]
                pub fn $name(&mut self, $($arg: $ty),*) -> NodeIndex {
                    std::sync::Arc::make_mut(&mut self.inner).$name($($arg),*)
                }
            )+
        }
    };
}

delegate_arena_alloc! {
    add_token(kind: u16, pos: u32, end: u32);
    add_block(kind: u16, pos: u32, end: u32, data: BlockData);
    add_type_ref(kind: u16, pos: u32, end: u32, data: TypeRefData);
    add_source_file(pos: u32, end: u32, data: SourceFileData);
    add_binary_expr(kind: u16, pos: u32, end: u32, data: BinaryExprData);
    add_jsx_opening(kind: u16, pos: u32, end: u32, data: JsxOpeningData);
    add_expr_statement(kind: u16, pos: u32, end: u32, data: ExprStatementData);
    add_variable_declaration(kind: u16, pos: u32, end: u32, data: VariableDeclarationData);
    add_identifier(kind: u16, pos: u32, end: u32, data: IdentifierData);
    add_expr_with_type_args(kind: u16, pos: u32, end: u32, data: ExprWithTypeArgsData);
    add_computed_property(kind: u16, pos: u32, end: u32, data: ComputedPropertyData);
    add_call_expr(kind: u16, pos: u32, end: u32, data: CallExprData);
    create_modifier(kind: tsz_scanner::SyntaxKind, pos: u32);
}

/// Generate `pool_checkpoint` and `restore_pool_checkpoint` on
/// [`NodeArenaInner`] from the canonical pool registry in
/// [`super::node_pools`].
macro_rules! impl_pool_checkpoints {
    ($($pool:ident => $elem:ty),+ $(,)?) => {
        impl NodeArenaInner {
            /// Capture the current length of every typed data pool.
            ///
            /// Paired with [`Self::restore_pool_checkpoint`] in the speculation system
            /// to reclaim orphaned pool entries when a speculative parse is rolled back.
            #[must_use]
            pub(crate) const fn pool_checkpoint(&self) -> NodeArenaPoolLengths {
                NodeArenaPoolLengths { $($pool: self.$pool.len(),)+ }
            }

            /// Truncate every typed data pool back to the lengths captured by
            /// [`Self::pool_checkpoint`].
            ///
            /// This reclaims any pool entries allocated during a failed speculation,
            /// preventing unbounded memory growth in files with many speculative parses
            /// (e.g. complex generic types, arrow function lookaheads).
            pub(crate) fn restore_pool_checkpoint(&mut self, c: &NodeArenaPoolLengths) {
                $(self.$pool.truncate(c.$pool);)+
            }
        }
    };
}
for_each_node_pool!(impl_pool_checkpoints);

/// Generate the per-pool capacity accounting used by
/// [`NodeArenaInner::estimated_size_bytes`] from the canonical pool registry
/// in [`super::node_pools`].
macro_rules! impl_pool_capacity_bytes {
    ($($pool:ident => $elem:ty),+ $(,)?) => {
        impl NodeArenaInner {
            /// Sum of `capacity * size_of::<Element>()` over every typed data
            /// pool (fixed-size element storage only; heap allocations inside
            /// elements are accounted separately by
            /// [`Self::estimated_size_bytes`]).
            #[must_use]
            const fn pool_capacity_bytes(&self) -> usize {
                let mut size = 0usize;
                $(size += self.$pool.capacity() * std::mem::size_of::<$elem>();)+
                size
            }
        }
    };
}
for_each_node_pool!(impl_pool_capacity_bytes);

impl NodeArenaInner {
    /// Estimate the total heap memory footprint of this arena in bytes.
    ///
    /// Accounts for the struct itself, all typed data pool capacities,
    /// heap-allocated strings inside pool elements (identifiers, literals,
    /// JSX text, source file data), the interner's string table, and
    /// the source text `Arc<str>`. This gives an accurate picture for
    /// memory-pressure tracking and LSP eviction budgeting.
    #[must_use]
    pub fn estimated_size_bytes(&self) -> usize {
        use std::mem::size_of;

        let mut size = size_of::<Self>();

        // ---- Interner ----
        // FxHashMap<Arc<str>, Atom>: each entry is (Arc overhead + string data + Atom)
        // Vec<Arc<str>>: capacity * pointer-size
        // We approximate string data by iterating the interner's resolve table.
        size += self.interner.len() * (size_of::<Arc<str>>() + size_of::<u32>());
        // HashMap overhead (~56 bytes per bucket on average with FxHashMap)
        size += self.interner.len() * 56;

        // ---- Node headers ----
        size += self.nodes.capacity() * size_of::<Node>();

        // ---- Typed data pools (fixed-size elements) ----
        // Generated from the canonical pool registry: for each `Vec<T>`,
        // adds `capacity * size_of::<T>()`. Heap allocations inside elements
        // (identifier/literal/JSX/source-file strings) are handled below.
        size += self.pool_capacity_bytes();

        // Extended info
        size += self.extended_info.capacity() * size_of::<ExtendedNodeInfo>();

        // ---- Heap strings inside pool elements ----

        // IdentifierData: escaped_text is a shared handle into the interner's
        // string table (counted by the interner's own estimate below), so only
        // original_text — a standalone shared string — adds heap here
        // (16-byte Arc header + string bytes).
        for id in &self.identifiers {
            if let Some(ref s) = id.original_text {
                size += 16 + s.len();
            }
        }

        // LiteralData: text + raw_text
        for lit in &self.literals {
            size += lit.text.capacity();
            if let Some(ref s) = lit.raw_text {
                size += s.capacity();
            }
        }

        // JsxTextData: text
        for jt in &self.jsx_text {
            size += jt.text.capacity();
        }

        // SourceFileData: file_name + text (Arc<str>) + comments
        for sf in &self.source_files {
            size += sf.file_name.capacity();
            size += sf.text.len(); // Arc<str> heap data
            size += sf.comments.capacity() * size_of::<tsz_common::comments::CommentRange>();
        }

        size
    }
}

node_data_structs! {
/// Extended node info for nodes that need more than what fits in Node
pub struct ExtendedNodeInfo {
    pub parent: NodeIndex,
    pub id: u32,
    pub modifier_flags: u32,
    pub transform_flags: u32,
}
}

impl Default for ExtendedNodeInfo {
    fn default() -> Self {
        Self {
            parent: NodeIndex::NONE,
            id: 0,
            modifier_flags: 0,
            transform_flags: 0,
        }
    }
}

// Re-export types from node_view module
pub use super::node_view::{NodeAccess, NodeInfo, NodeView};

#[cfg(test)]
#[path = "../../tests/node_tests.rs"]
mod node_tests;
