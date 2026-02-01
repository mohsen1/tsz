//! Completions implementation for LSP.
//!
//! Given a position in the source, provides completion suggestions for
//! identifiers that are visible at that position.

use rustc_hash::{FxHashMap, FxHashSet};
use std::borrow::Cow;

use crate::binder::BinderState;
use crate::binder::SymbolId;
use crate::checker::TypeCache;
use crate::checker::state::CheckerState;
use crate::lsp::jsdoc::jsdoc_for_node;
use crate::lsp::position::{LineMap, Position};
use crate::lsp::resolver::{ScopeCache, ScopeCacheStats, ScopeWalker};
use crate::lsp::utils::find_node_at_offset;
use crate::parser::NodeIndex;
use crate::parser::node::{NodeAccess, NodeArena};
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use crate::solver::{
    ApparentMemberKind, IntrinsicKind, TypeId, TypeInterner, TypeKey, apparent_primitive_members,
};

/// The kind of completion item, matching tsserver's ScriptElementKind values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CompletionItemKind {
    /// A variable or constant
    Variable,
    /// A function
    Function,
    /// A class
    Class,
    /// A method
    Method,
    /// A parameter
    Parameter,
    /// A property
    Property,
    /// A keyword
    Keyword,
    /// An interface
    Interface,
    /// An enum
    Enum,
    /// A type alias
    TypeAlias,
    /// A module or namespace
    Module,
    /// A type parameter
    TypeParameter,
    /// A constructor
    Constructor,
}

/// Sort priority categories matching tsserver's sort text conventions.
/// Lower numbers appear first in the completion list.
pub mod sort_priority {
    // Values match TypeScript's `ts.Completions.SortText` enum.
    /// Local variables, parameters, and function-scoped identifiers.
    pub const LOCAL_DECLARATION: &str = "10";
    /// Properties, methods, and other location-based completions.
    pub const LOCATION_PRIORITY: &str = "11";
    /// Optional members.
    pub const OPTIONAL_MEMBER: &str = "12";
    /// Properties and methods on a member completion.
    pub const MEMBER: &str = "11";
    /// Type-level completions (interfaces, type aliases, enums).
    pub const TYPE_DECLARATION: &str = "11";
    /// Suggested class members.
    pub const SUGGESTED_CLASS_MEMBERS: &str = "14";
    /// Global variables and keywords.
    pub const GLOBALS_OR_KEYWORDS: &str = "15";
    /// Completions from auto-import candidates.
    pub const AUTO_IMPORT: &str = "16";
    /// Legacy alias for GLOBALS_OR_KEYWORDS.
    pub const KEYWORD: &str = "15";

    /// Produce a deprecated sort text by prefixing "z" to the base sort text.
    /// This matches TypeScript's `SortText.Deprecated()` transformation.
    pub fn deprecated(base: &str) -> String {
        format!("z{}", base)
    }
}

/// Result of a completion request, matching tsserver's `CompletionInfo`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CompletionResult {
    /// Whether this is a global (non-member) completion.
    pub is_global_completion: bool,
    /// Whether this is a member completion (after a dot).
    pub is_member_completion: bool,
    /// Whether the cursor is at a location where a new identifier can be typed.
    /// When true, the editor should not auto-commit completions (the user might
    /// be typing a new name rather than selecting an existing one).
    pub is_new_identifier_location: bool,
    /// The completion entries.
    pub entries: Vec<CompletionItem>,
}

/// A completion item to be suggested to the user.
///
/// Fields align with tsserver's `CompletionEntry` protocol:
///   name, kind, kindModifiers, sortText, insertText, replacementSpan,
///   hasAction, source, sourceDisplay, data
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CompletionItem {
    /// The label to display in the completion list (tsserver: `name`)
    pub label: String,
    /// The kind of completion item (tsserver: `kind`)
    pub kind: CompletionItemKind,
    /// Optional detail text (e.g., type information)
    pub detail: Option<String>,
    /// Optional documentation
    pub documentation: Option<String>,
    /// Sort text controls ordering in the completion list (tsserver: `sortText`).
    /// Lower strings appear first. See [`sort_priority`] for categories.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_text: Option<String>,
    /// Text to insert when the completion is accepted, if different from `label`
    /// (tsserver: `insertText`). For snippets this may contain tab stops like `$1`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_text: Option<String>,
    /// Whether the completion is a snippet (contains tab-stop placeholders).
    #[serde(skip_serializing_if = "is_false")]
    pub is_snippet: bool,
    /// Whether selecting this completion triggers an additional action such as
    /// an auto-import (tsserver: `hasAction`).
    #[serde(skip_serializing_if = "is_false")]
    pub has_action: bool,
    /// Module specifier for auto-import completions (tsserver: `source`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Display label for the source module (tsserver: `sourceDisplay`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_display: Option<String>,
    /// Comma-separated modifier flags such as `export`, `declare`, `abstract`,
    /// `static`, `private`, `protected` (tsserver: `kindModifiers`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind_modifiers: Option<String>,
    /// The byte range in the source text that this completion replaces
    /// (tsserver: `replacementSpan`). `None` means the editor should use its
    /// default replacement behaviour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement_span: Option<(u32, u32)>,
}

/// Helper for serde `skip_serializing_if`.
fn is_false(v: &bool) -> bool {
    !v
}

impl CompletionItem {
    /// Create a new completion item with only the required fields.
    pub fn new(label: String, kind: CompletionItemKind) -> Self {
        Self {
            label,
            kind,
            detail: None,
            documentation: None,
            sort_text: None,
            insert_text: None,
            is_snippet: false,
            has_action: false,
            source: None,
            source_display: None,
            kind_modifiers: None,
            replacement_span: None,
        }
    }

    /// Set the detail text.
    pub fn with_detail(mut self, detail: String) -> Self {
        self.detail = Some(detail);
        self
    }

    /// Set the documentation.
    pub fn with_documentation(mut self, documentation: String) -> Self {
        self.documentation = Some(documentation);
        self
    }

    /// Set the sort text (controls ordering in the list).
    pub fn with_sort_text(mut self, sort_text: impl Into<String>) -> Self {
        self.sort_text = Some(sort_text.into());
        self
    }

    /// Set the insert text (text inserted on accept).
    pub fn with_insert_text(mut self, insert_text: String) -> Self {
        self.insert_text = Some(insert_text);
        self
    }

    /// Mark this completion as a snippet (insert text contains tab-stop placeholders).
    pub fn as_snippet(mut self) -> Self {
        self.is_snippet = true;
        self
    }

    /// Mark this completion as requiring an additional action (e.g. auto-import).
    pub fn with_has_action(mut self) -> Self {
        self.has_action = true;
        self
    }

    /// Set the module source path for auto-import completions.
    pub fn with_source(mut self, source: String) -> Self {
        self.source = Some(source);
        self
    }

    /// Set the display label for the source module.
    pub fn with_source_display(mut self, display: String) -> Self {
        self.source_display = Some(display);
        self
    }

    /// Set the kind modifiers string.
    pub fn with_kind_modifiers(mut self, modifiers: String) -> Self {
        self.kind_modifiers = Some(modifiers);
        self
    }

    /// Set the replacement span (byte offsets).
    pub fn with_replacement_span(mut self, start: u32, end: u32) -> Self {
        self.replacement_span = Some((start, end));
        self
    }

    /// Return the effective sort text: the explicitly set value, or a default
    /// derived from the completion kind.
    pub fn effective_sort_text(&self) -> &str {
        if let Some(ref s) = self.sort_text {
            s.as_str()
        } else {
            default_sort_text(self.kind)
        }
    }
}

/// Derive a default sort text from the completion kind, following tsserver
/// conventions. Most scope-visible items get LocationPriority ("11").
pub fn default_sort_text(kind: CompletionItemKind) -> &'static str {
    match kind {
        CompletionItemKind::Keyword => sort_priority::GLOBALS_OR_KEYWORDS,
        CompletionItemKind::Property | CompletionItemKind::Method => sort_priority::MEMBER,
        _ => sort_priority::LOCATION_PRIORITY,
    }
}

/// Completions provider.
///
/// This struct provides LSP "Completions" functionality by:
/// 1. Converting a position to a byte offset
/// 2. Finding the AST node at that offset
/// 3. Getting the active scope chain at that position
/// 4. Collecting all visible identifiers from the scope chain
/// 5. Returning them as completion items
pub struct Completions<'a> {
    arena: &'a NodeArena,
    binder: &'a BinderState,
    line_map: &'a LineMap,
    source_text: &'a str,
    interner: Option<&'a TypeInterner>,
    file_name: Option<String>,
    strict: bool,
}

/// JavaScript/TypeScript keywords for completion.
/// Matches tsserver's `globalKeywords` list.
const KEYWORDS: &[&str] = &[
    "abstract",
    "any",
    "as",
    "asserts",
    "async",
    "await",
    "bigint",
    "boolean",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "declare",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "implements",
    "import",
    "in",
    "infer",
    "instanceof",
    "interface",
    "keyof",
    "let",
    "module",
    "namespace",
    "never",
    "new",
    "null",
    "number",
    "object",
    "package",
    "readonly",
    "return",
    "satisfies",
    "string",
    "super",
    "switch",
    "symbol",
    "this",
    "throw",
    "true",
    "try",
    "type",
    "typeof",
    "unique",
    "unknown",
    "using",
    "var",
    "void",
    "while",
    "with",
    "yield",
];

/// Keywords valid inside a function body (subset without top-level-only keywords).
/// Matches tsserver's `globalKeywordsInsideFunction`.
const KEYWORDS_INSIDE_FUNCTION: &[&str] = &[
    "as",
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "let",
    "new",
    "null",
    "package",
    "return",
    "satisfies",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "type",
    "typeof",
    "using",
    "var",
    "void",
    "while",
    "with",
    "yield",
];

/// Global variable names from lib.d.ts that should appear in completions.
/// Matches tsserver's `globalsVars` list.
const GLOBAL_VARS: &[(&str, CompletionItemKind)] = &[
    ("Array", CompletionItemKind::Variable),
    ("ArrayBuffer", CompletionItemKind::Variable),
    ("Boolean", CompletionItemKind::Variable),
    ("DataView", CompletionItemKind::Variable),
    ("Date", CompletionItemKind::Variable),
    ("decodeURI", CompletionItemKind::Function),
    ("decodeURIComponent", CompletionItemKind::Function),
    ("encodeURI", CompletionItemKind::Function),
    ("encodeURIComponent", CompletionItemKind::Function),
    ("Error", CompletionItemKind::Variable),
    ("escape", CompletionItemKind::Function),
    ("eval", CompletionItemKind::Function),
    ("EvalError", CompletionItemKind::Variable),
    ("Float32Array", CompletionItemKind::Variable),
    ("Float64Array", CompletionItemKind::Variable),
    ("Function", CompletionItemKind::Variable),
    ("globalThis", CompletionItemKind::Module),
    ("Infinity", CompletionItemKind::Variable),
    ("Int16Array", CompletionItemKind::Variable),
    ("Int32Array", CompletionItemKind::Variable),
    ("Int8Array", CompletionItemKind::Variable),
    ("Intl", CompletionItemKind::Module),
    ("isFinite", CompletionItemKind::Function),
    ("isNaN", CompletionItemKind::Function),
    ("JSON", CompletionItemKind::Variable),
    ("Math", CompletionItemKind::Variable),
    ("NaN", CompletionItemKind::Variable),
    ("Number", CompletionItemKind::Variable),
    ("Object", CompletionItemKind::Variable),
    ("parseFloat", CompletionItemKind::Function),
    ("parseInt", CompletionItemKind::Function),
    ("RangeError", CompletionItemKind::Variable),
    ("ReferenceError", CompletionItemKind::Variable),
    ("RegExp", CompletionItemKind::Variable),
    ("String", CompletionItemKind::Variable),
    ("SyntaxError", CompletionItemKind::Variable),
    ("TypeError", CompletionItemKind::Variable),
    ("Uint16Array", CompletionItemKind::Variable),
    ("Uint32Array", CompletionItemKind::Variable),
    ("Uint8Array", CompletionItemKind::Variable),
    ("Uint8ClampedArray", CompletionItemKind::Variable),
    ("undefined", CompletionItemKind::Variable),
    ("unescape", CompletionItemKind::Function),
    ("URIError", CompletionItemKind::Variable),
];

/// Global variables that are deprecated. These get kindModifiers "deprecated,declare"
/// and sort text prefixed with "z" to push them to the end of the completion list.
const DEPRECATED_GLOBALS: &[&str] = &["escape", "unescape"];

/// Compare two strings using a case-sensitive UI sort order that matches
/// TypeScript's `compareStringsCaseSensitiveUI` (Intl.Collator with
/// sensitivity: "variant", numeric: true). Uses multi-pass comparison like
/// the Unicode Collation Algorithm: primary pass resolves case-insensitive
/// differences (with numeric segments compared as numbers), then tertiary
/// pass resolves case (lowercase before uppercase).
fn compare_case_sensitive_ui(a: &str, b: &str) -> std::cmp::Ordering {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    // Primary pass: case-insensitive with numeric comparison
    let mut ai = 0;
    let mut bi = 0;
    let mut case_diff: Option<std::cmp::Ordering> = None;
    while ai < a_chars.len() && bi < b_chars.len() {
        let ac = a_chars[ai];
        let bc = b_chars[bi];

        // If both are digits, compare numeric segments
        if ac.is_ascii_digit() && bc.is_ascii_digit() {
            let a_start = ai;
            while ai < a_chars.len() && a_chars[ai].is_ascii_digit() {
                ai += 1;
            }
            let b_start = bi;
            while bi < b_chars.len() && b_chars[bi].is_ascii_digit() {
                bi += 1;
            }
            let a_num: u64 = a_chars[a_start..ai]
                .iter()
                .collect::<String>()
                .parse()
                .unwrap_or(0);
            let b_num: u64 = b_chars[b_start..bi]
                .iter()
                .collect::<String>()
                .parse()
                .unwrap_or(0);
            if a_num != b_num {
                return a_num.cmp(&b_num);
            }
            continue;
        }

        let al = ac.to_ascii_lowercase();
        let bl = bc.to_ascii_lowercase();
        if al != bl {
            return al.cmp(&bl);
        }
        // Track first case difference for tertiary pass
        if case_diff.is_none() && ac != bc {
            if ac.is_lowercase() && bc.is_uppercase() {
                case_diff = Some(std::cmp::Ordering::Less);
            } else if ac.is_uppercase() && bc.is_lowercase() {
                case_diff = Some(std::cmp::Ordering::Greater);
            }
        }
        ai += 1;
        bi += 1;
    }

    // Length difference (shorter first)
    if ai < a_chars.len() {
        return std::cmp::Ordering::Greater;
    }
    if bi < b_chars.len() {
        return std::cmp::Ordering::Less;
    }

    // Tertiary: first case difference determines order
    case_diff.unwrap_or(std::cmp::Ordering::Equal)
}

impl<'a> Completions<'a> {
    /// Create a new Completions provider.
    pub fn new(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        line_map: &'a LineMap,
        source_text: &'a str,
    ) -> Self {
        Self {
            arena,
            binder,
            line_map,
            source_text,
            interner: None,
            file_name: None,
            strict: false,
        }
    }

    /// Create a completions provider with type-aware member completion support.
    pub fn new_with_types(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        line_map: &'a LineMap,
        interner: &'a TypeInterner,
        source_text: &'a str,
        file_name: String,
    ) -> Self {
        Self {
            arena,
            binder,
            line_map,
            source_text,
            interner: Some(interner),
            file_name: Some(file_name),
            strict: false,
        }
    }

    /// Create a completions provider with type-aware member completion support and explicit strict mode.
    pub fn with_strict(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        line_map: &'a LineMap,
        interner: &'a TypeInterner,
        source_text: &'a str,
        file_name: String,
        strict: bool,
    ) -> Self {
        Self {
            arena,
            binder,
            line_map,
            source_text,
            interner: Some(interner),
            file_name: Some(file_name),
            strict,
        }
    }

    /// Get completion suggestions at the given position.
    ///
    /// Returns a list of completion items for identifiers visible at the cursor position.
    /// Returns None if no completions are available.
    pub fn get_completions(
        &self,
        root: NodeIndex,
        position: Position,
    ) -> Option<Vec<CompletionItem>> {
        self.get_completions_internal(root, position, None, None, None)
    }

    /// Get completion suggestions at the given position with a persistent type cache.
    pub fn get_completions_with_cache(
        &self,
        root: NodeIndex,
        position: Position,
        type_cache: &mut Option<TypeCache>,
    ) -> Option<Vec<CompletionItem>> {
        self.get_completions_internal(root, position, Some(type_cache), None, None)
    }

    pub fn get_completions_with_caches(
        &self,
        root: NodeIndex,
        position: Position,
        type_cache: &mut Option<TypeCache>,
        scope_cache: &mut ScopeCache,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<CompletionItem>> {
        self.get_completions_internal(
            root,
            position,
            Some(type_cache),
            Some(scope_cache),
            scope_stats,
        )
    }

    /// Get a full completion result including metadata like `is_new_identifier_location`.
    pub fn get_completion_result(
        &self,
        root: NodeIndex,
        position: Position,
    ) -> Option<CompletionResult> {
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;
        let is_member = self.is_member_context(offset);
        let is_new_id = if is_member {
            false
        } else {
            self.compute_is_new_identifier_location(root, offset)
        };
        let items = self.get_completions_internal(root, position, None, None, None)?;
        Some(CompletionResult {
            is_global_completion: !is_member,
            is_member_completion: is_member,
            is_new_identifier_location: is_new_id,
            entries: items,
        })
    }

    /// Check if the cursor is after a dot (member completion context).
    fn is_member_context(&self, offset: u32) -> bool {
        if offset > 0 {
            self.source_text
                .as_bytes()
                .get((offset - 1) as usize)
                .copied()
                .map(|ch| ch == b'.')
                .unwrap_or(false)
        } else {
            false
        }
    }

    /// Determine `isNewIdentifierLocation` by examining the AST context at the
    /// given byte offset. This matches tsserver's `computeCommitCharactersAndIsNewIdentifier`.
    ///
    /// Returns `true` when the cursor is in a position where the user might be
    /// typing a brand-new identifier (e.g. a variable name after `const`, a
    /// parameter name, an import binding name, etc.).
    pub fn compute_is_new_identifier_location(&self, root: NodeIndex, offset: u32) -> bool {
        // TypeScript's isNewIdentifierLocation defaults to false and only returns true
        // for specific token/parent-kind combinations. Our heuristic approximates this
        // by checking AST context and text patterns.

        let node_idx = self.find_completions_node(root, offset);

        // Check if inside a class/interface body at a member declaration position
        if let Some(node) = self.arena.get(node_idx) {
            let k = node.kind;

            // Property/method declarations and signatures in class/interface bodies
            if k == syntax_kind_ext::PROPERTY_DECLARATION
                || k == syntax_kind_ext::PROPERTY_SIGNATURE
                || k == syntax_kind_ext::METHOD_SIGNATURE
                || k == syntax_kind_ext::INDEX_SIGNATURE
            {
                return true;
            }

            // Inside class/interface body at member position (after `{`)
            if k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION
                || k == syntax_kind_ext::INTERFACE_DECLARATION
            {
                let text_before = &self.source_text[..offset as usize];
                if text_before[node.pos as usize..].contains('{') {
                    return true;
                }
            }

            // TODO: More AST-based checks needed for:
            // - Object literal with index signatures
            // - Type literal positions
            // - Function call argument positions
            // - Array literal positions
            // These require careful type-checking context that we don't have yet.
        }

        // Text-based heuristic for the context token
        let text = &self.source_text[..offset as usize];
        let trimmed = text.trim_end();
        if trimmed.is_empty() {
            return false;
        }

        // Find the last word before cursor
        let last_word_start = trimmed
            .rfind(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|p| p + 1)
            .unwrap_or(0);
        let last_word = &trimmed[last_word_start..];

        // Keywords that always return true per TypeScript's implementation
        if matches!(
            last_word,
            "module" | "namespace" | "import" | "function" | "yield"
        ) {
            return true;
        }

        // Check the last non-whitespace character for common expression-start operators.
        // These match TypeScript's isNewIdentifierDefinitionLocation logic for tokens
        // that indicate the user may type a new expression (variable initializer,
        // function argument, array element, etc.).
        let last_char = trimmed.as_bytes().last().copied();
        match last_char {
            // After `=` in variable declarations and property assignments,
            // but NOT after `==`, `===`, `!=`, `>=`, `<=`
            Some(b'=') => {
                let before = &trimmed[..trimmed.len() - 1];
                let prev = before.as_bytes().last().copied();
                if prev != Some(b'=') && prev != Some(b'!') && prev != Some(b'>') && prev != Some(b'<') {
                    return true;
                }
            }
            // After `(` in function calls, constructor calls, parenthesized expressions
            Some(b'(') => return true,
            // After `,` in function arguments, array elements, object properties
            Some(b',') => return true,
            // After `[` in array literals, index signatures, computed properties
            Some(b'[') => return true,
            _ => {}
        }

        // After `${` in template literal expressions
        if trimmed.ends_with("${") {
            return true;
        }

        false
    }

    /// Check if the cursor is inside a context where completions should not be offered,
    /// such as inside string literals (non-module-specifier), comments, or regex literals.
    fn is_in_no_completion_context(&self, offset: u32) -> bool {
        // Check if we're at an identifier definition location first - this works
        // even when offset == source_text.len() (cursor at end of file).
        if self.is_at_definition_location(offset) {
            return true;
        }

        // Check for comments before the offset >= len guard, since comments at
        // end-of-file (offset == len) should still suppress completions.
        let i = offset as usize;
        if i > 0 {
            // Check for line comments: if we find // before offset on same line
            let line_start = self.source_text[..i]
                .rfind('\n')
                .map(|p| p + 1)
                .unwrap_or(0);
            let line_prefix = &self.source_text[line_start..i];
            if line_prefix.contains("//") {
                // Check that the // is not inside a string
                let comment_pos = line_prefix.find("//").unwrap();
                let before_comment = &line_prefix[..comment_pos];
                let single_quotes = before_comment.chars().filter(|&c| c == '\'').count();
                let double_quotes = before_comment.chars().filter(|&c| c == '"').count();
                let backticks = before_comment.chars().filter(|&c| c == '`').count();
                if single_quotes % 2 == 0 && double_quotes % 2 == 0 && backticks % 2 == 0 {
                    return true;
                }
            }

            // Check for block comments: scan backwards for /* without matching */
            if let Some(block_start) = self.source_text[..i].rfind("/*") {
                let after_block = &self.source_text[block_start + 2..i];
                if !after_block.contains("*/") {
                    return true;
                }
            }

            // Text-based regex literal detection: after /pattern/ or /pattern/flags
            // This catches cases where cursor is at end-of-file after a regex.
            if self.text_is_inside_regex(i) {
                return true;
            }

            // Text-based template literal detection: inside backtick strings
            if self.text_is_inside_template_literal(i) {
                return true;
            }

            // Text-based string literal detection: inside unclosed quotes
            if self.text_is_inside_string_literal(i) {
                return true;
            }
        }

        // Check if we're inside a string literal, comment, or regex by examining
        // the source text character context around the offset.
        let bytes = self.source_text.as_bytes();
        let len = bytes.len();
        if offset as usize >= len {
            return false;
        }

        // Check if we're inside a numeric literal (including BigInt suffixed with 'n')
        // No completions should appear at the end of numeric literals like `0n`, `123`, `0xff`
        if offset > 0 {
            let check_offset = (offset - 1) as usize;
            if check_offset < len {
                let prev_byte = bytes[check_offset];
                // After a digit or 'n' suffix (BigInt), check if we're in a numeric literal
                if prev_byte.is_ascii_digit()
                    || prev_byte == b'n'
                    || prev_byte == b'x'
                    || prev_byte == b'o'
                    || prev_byte == b'b'
                {
                    let node_idx_check = find_node_at_offset(self.arena, offset.saturating_sub(1));
                    if !node_idx_check.is_none() {
                        if let Some(node) = self.arena.get(node_idx_check) {
                            if node.kind == SyntaxKind::NumericLiteral as u16
                                || node.kind == SyntaxKind::BigIntLiteral as u16
                            {
                                // We're right after a numeric/BigInt literal
                                return true;
                            }
                        }
                    }
                }
            }
        }

        // Check if we're inside a string literal using the AST
        let node_idx = find_node_at_offset(self.arena, offset);
        if !node_idx.is_none() {
            if let Some(node) = self.arena.get(node_idx) {
                let kind = node.kind;
                // String literal (not inside an import/require module specifier)
                if kind == SyntaxKind::StringLiteral as u16 {
                    // Check if parent is an import declaration's module specifier
                    if let Some(ext) = self.arena.get_extended(node_idx) {
                        let parent = self.arena.get(ext.parent);
                        if let Some(p) = parent {
                            if p.kind == syntax_kind_ext::IMPORT_DECLARATION
                                || p.kind == syntax_kind_ext::EXPORT_DECLARATION
                                || p.kind == syntax_kind_ext::EXTERNAL_MODULE_REFERENCE
                            {
                                return false; // Module specifier - allow completions
                            }
                        }
                    }
                    return true; // Regular string literal - no completions
                }
                // No-substitution template literal
                if kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16 {
                    return true;
                }
                // Template head/middle/tail (inside template literal parts, not expressions)
                if kind == SyntaxKind::TemplateHead as u16
                    || kind == SyntaxKind::TemplateMiddle as u16
                    || kind == SyntaxKind::TemplateTail as u16
                {
                    return true;
                }
                // Regular expression literal
                if kind == SyntaxKind::RegularExpressionLiteral as u16 {
                    return true;
                }
            }
        }

        false
    }

    /// Check if the cursor is at a position where a new identifier is being defined.
    /// At these locations, completions should not be offered because the user is
    /// typing a new name, not referencing an existing one.
    fn is_at_definition_location(&self, offset: u32) -> bool {
        // Use the full text up to cursor (including trailing whitespace)
        let text = &self.source_text[..offset as usize];

        // Strategy: look at what's right before the cursor. We need to handle:
        // 1. "var |" - cursor after keyword + space
        // 2. "var a|" - cursor after keyword + partial identifier
        // 3. "var a, |" - cursor after comma in declaration list
        // 4. "function foo(|" - cursor at parameter position

        // First, check the untrimmed text for trailing whitespace patterns
        // (cursor is after space following a keyword)
        let trimmed = text.trim_end();
        if trimmed.is_empty() {
            return false;
        }

        // Extract the last word from trimmed text
        let last_word_start = trimmed
            .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
            .map(|p| p + 1)
            .unwrap_or(0);
        let last_word = &trimmed[last_word_start..];

        // Check if we have whitespace after the last word (before cursor)
        let has_trailing_ws = text.len() > trimmed.len();

        let definition_keywords = [
            "var",
            "let",
            "const",
            "function",
            "class",
            "interface",
            "type",
            "enum",
            "namespace",
            "module",
            "infer",
        ];

        // Helper to check whole-word boundary
        let is_whole_word = |text: &str, kw: &str| -> bool {
            if !text.ends_with(kw) {
                return false;
            }
            let kw_start = text.len() - kw.len();
            kw_start == 0 || {
                let c = text.as_bytes()[kw_start - 1];
                !c.is_ascii_alphanumeric() && c != b'_' && c != b'$'
            }
        };

        // Case 1: "keyword |" - cursor after keyword + whitespace
        if has_trailing_ws
            && definition_keywords
                .iter()
                .any(|kw| is_whole_word(trimmed, kw))
        {
            return true;
        }

        // Case 2: "keyword partialId|" - cursor while typing identifier after keyword
        if !has_trailing_ws && !last_word.is_empty() {
            let before_word = trimmed[..last_word_start].trim_end();
            if definition_keywords
                .iter()
                .any(|kw| is_whole_word(before_word, kw))
            {
                return true;
            }
            // "function* name|" - generator function name
            if before_word.ends_with('*') {
                let before_star = before_word[..before_word.len() - 1].trim_end();
                if is_whole_word(before_star, "function") {
                    return true;
                }
            }
            // "...name|" in parameter list - rest parameter
            if before_word.ends_with("...") && self.is_in_parameter_list(offset) {
                return true;
            }
        }

        // The text before the cursor (or before the partial identifier being typed)
        let check_before = if has_trailing_ws {
            trimmed
        } else {
            trimmed[..last_word_start].trim_end()
        };

        // Case 3: comma in declarations: "var a, |", "function f(a, |", "<T, |"
        if check_before.ends_with(',') {
            // Try AST-based detection first, then text-based fallback
            if self.is_in_variable_declaration_list(offset)
                || self.text_looks_like_var_declaration_list(check_before)
            {
                return true;
            }
            if self.is_in_parameter_list(offset)
                || self.text_looks_like_parameter_list(check_before)
            {
                return true;
            }
            if self.is_in_type_parameter_list(offset)
                || self.text_looks_like_type_param_list(check_before)
            {
                return true;
            }
        }

        // Case 4: function parameter names at opening paren: "function foo(|"
        if check_before.ends_with('(') {
            if self.is_in_parameter_list(offset)
                || self.text_looks_like_parameter_list(check_before)
            {
                return true;
            }
        }

        // Case 4b: "...name" in parameter list - rest parameter
        if has_trailing_ws && trimmed.ends_with("...") && self.is_in_parameter_list(offset) {
            return true;
        }

        // Case 5: catch clause: "catch (|" or "catch (x|"
        if check_before.ends_with("catch(") || check_before.ends_with("catch (") {
            return true;
        }
        if !has_trailing_ws && !last_word.is_empty() {
            let before_word_trimmed = trimmed[..last_word_start].trim_end();
            if before_word_trimmed.ends_with("catch(") || before_word_trimmed.ends_with("catch (") {
                return true;
            }
        }

        // Case 6: type parameter list opener: "class A<|", "interface B<|"
        if check_before.ends_with('<') {
            if self.is_in_type_parameter_list(offset)
                || self.text_looks_like_type_param_opener(check_before)
            {
                return true;
            }
        }

        // Case 7: enum member position
        if self.is_in_enum_member_position(offset) {
            return true;
        }

        // Case 8: destructuring binding: "let { |" or "let [|"
        if self.is_in_binding_pattern_definition(offset) {
            return true;
        }

        false
    }

    /// Text-based heuristic to detect if we're in a var/let/const declaration list
    /// after a comma. This is a fallback for when the AST-based check fails due to
    /// parser error recovery.
    fn text_looks_like_var_declaration_list(&self, text_before_comma: &str) -> bool {
        // Find the most recent var/let/const keyword by scanning backward.
        // Check that there's no statement boundary (`;`, `{`, `}`) between
        // the keyword and the comma that isn't inside a nested expression.
        let bytes = text_before_comma.as_bytes();
        let keywords: &[&str] = &["var ", "let ", "const "];

        for kw in keywords {
            // Search backward for this keyword
            let mut search_from = text_before_comma.len();
            while let Some(pos) = text_before_comma[..search_from].rfind(kw) {
                // Check word boundary before keyword
                if pos > 0 {
                    let c = bytes[pos - 1];
                    if c.is_ascii_alphanumeric() || c == b'_' || c == b'$' {
                        search_from = pos;
                        continue;
                    }
                }
                // Check no unbalanced statement boundaries between keyword and comma
                let between = &text_before_comma[pos + kw.len()..];
                let mut brace_depth: i32 = 0;
                let mut paren_depth: i32 = 0;
                let mut _bracket_depth: i32 = 0;
                let mut has_boundary = false;
                for &b in between.as_bytes() {
                    match b {
                        b'{' => brace_depth += 1,
                        b'}' => {
                            brace_depth -= 1;
                            if brace_depth < 0 {
                                has_boundary = true;
                                break;
                            }
                        }
                        b'(' => paren_depth += 1,
                        b')' => paren_depth -= 1,
                        b'[' => _bracket_depth += 1,
                        b']' => _bracket_depth -= 1,
                        b';' if brace_depth == 0 && paren_depth == 0 => {
                            has_boundary = true;
                            break;
                        }
                        _ => {}
                    }
                }
                if !has_boundary && brace_depth == 0 {
                    return true;
                }
                search_from = pos;
            }
        }
        false
    }

    /// Text-based heuristic to detect if cursor is in a function/method parameter list.
    /// Only matches clearly identifiable declaration patterns to avoid false positives
    /// with function calls.
    fn text_looks_like_parameter_list(&self, text_before: &str) -> bool {
        // Scan backward for an unmatched '('
        let mut paren_depth: i32 = 0;
        let bytes = text_before.as_bytes();
        for i in (0..bytes.len()).rev() {
            match bytes[i] {
                b')' => paren_depth += 1,
                b'(' => {
                    if paren_depth == 0 {
                        // Found unmatched '(' - check what's before it
                        let before_paren = text_before[..i].trim_end();
                        if before_paren.is_empty() {
                            return false;
                        }
                        let last_char = before_paren.as_bytes()[before_paren.len() - 1];
                        if last_char.is_ascii_alphanumeric()
                            || last_char == b'_'
                            || last_char == b'$'
                        {
                            let word_start = before_paren
                                .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
                                .map(|p| p + 1)
                                .unwrap_or(0);
                            let word = &before_paren[word_start..];
                            let before_word = before_paren[..word_start].trim_end();
                            // "function foo(" or "function* foo("
                            if before_word.ends_with("function")
                                || before_word.ends_with("function*")
                            {
                                return true;
                            }
                            // "constructor(" pattern
                            if word == "constructor" {
                                return true;
                            }
                        }
                        // Could also have type params: "function foo<T>(" or "class.method<T>( "
                        if last_char == b'>' {
                            // Scan back past the type params to find identifier
                            let mut angle_depth: i32 = 0;
                            for j in (0..before_paren.len()).rev() {
                                match before_paren.as_bytes()[j] {
                                    b'>' => angle_depth += 1,
                                    b'<' => {
                                        angle_depth -= 1;
                                        if angle_depth == 0 {
                                            let before_angle = before_paren[..j].trim_end();
                                            if !before_angle.is_empty() {
                                                let ws = before_angle
                                                    .rfind(|c: char| {
                                                        !c.is_alphanumeric() && c != '_' && c != '$'
                                                    })
                                                    .map(|p| p + 1)
                                                    .unwrap_or(0);
                                                let bw = before_angle[..ws].trim_end();
                                                if bw.ends_with("function")
                                                    || bw.ends_with("function*")
                                                {
                                                    return true;
                                                }
                                            }
                                            break;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        return false;
                    }
                    paren_depth -= 1;
                }
                b';' | b'{' | b'}' if paren_depth == 0 => return false,
                _ => {}
            }
        }
        false
    }

    /// Text-based heuristic to detect if cursor is after a comma in a type parameter list.
    /// Looks for an unmatched '<' preceded by a type-parameterizable declaration.
    fn text_looks_like_type_param_list(&self, text_before: &str) -> bool {
        // Scan backward for an unmatched '<'
        let mut angle_depth: i32 = 0;
        let bytes = text_before.as_bytes();
        for i in (0..bytes.len()).rev() {
            match bytes[i] {
                b'>' => angle_depth += 1,
                b'<' => {
                    if angle_depth == 0 {
                        // Found unmatched '<' - check if it's a type param opener
                        return Self::text_before_angle_is_type_param(&text_before[..i]);
                    }
                    angle_depth -= 1;
                }
                b';' | b'{' | b'}' => return false,
                _ => {}
            }
        }
        false
    }

    /// Text-based heuristic to detect if '<' at end of text opens a type parameter list.
    /// Pattern: "class A<", "interface B<", "function C<", "type D<", "f<" (method)
    fn text_looks_like_type_param_opener(&self, text_ending_with_angle: &str) -> bool {
        let before_angle = text_ending_with_angle[..text_ending_with_angle.len() - 1].trim_end();
        Self::text_before_angle_is_type_param(before_angle)
    }

    fn text_before_angle_is_type_param(before_angle: &str) -> bool {
        if before_angle.is_empty() {
            return false;
        }
        let last_char = before_angle.as_bytes()[before_angle.len() - 1];
        if !last_char.is_ascii_alphanumeric() && last_char != b'_' && last_char != b'$' {
            return false;
        }
        let word_start = before_angle
            .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
            .map(|p| p + 1)
            .unwrap_or(0);
        let before_word = before_angle[..word_start].trim_end();
        let type_param_keywords = ["class", "interface", "function", "type"];
        // "class A<", "interface B<", etc.
        for kw in &type_param_keywords {
            if before_word.ends_with(kw) {
                let kw_start = before_word.len() - kw.len();
                if kw_start == 0 || {
                    let c = before_word.as_bytes()[kw_start - 1];
                    !c.is_ascii_alphanumeric() && c != b'_' && c != b'$'
                } {
                    return true;
                }
            }
        }
        // Method in class body: any identifier followed by '<' could be a method
        // type parameter. Check if inside a class body by looking for '{' balance.
        // For simplicity, if we see an unbalanced '{' before the word, it could be
        // inside a class/interface body.
        let mut brace_depth: i32 = 0;
        for &b in before_word.as_bytes().iter().rev() {
            match b {
                b'}' => brace_depth += 1,
                b'{' => {
                    brace_depth -= 1;
                    if brace_depth < 0 {
                        // Inside a block - could be class body
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Check if offset is inside a destructuring binding pattern in a declaration
    fn is_in_binding_pattern_definition(&self, offset: u32) -> bool {
        let node_idx = find_node_at_offset(self.arena, offset);
        let start = if node_idx.is_none() && offset > 0 {
            find_node_at_offset(self.arena, offset - 1)
        } else {
            node_idx
        };
        let mut current = start;
        let mut in_binding_pattern = false;
        let mut depth = 0;
        while !current.is_none() && depth < 50 {
            if let Some(node) = self.arena.get(current) {
                if node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                    || node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                {
                    in_binding_pattern = true;
                }
                if in_binding_pattern
                    && (node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                        || node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                        || node.kind == syntax_kind_ext::PARAMETER)
                {
                    return true;
                }
            }
            if let Some(ext) = self.arena.get_extended(current) {
                if ext.parent == current {
                    break;
                }
                current = ext.parent;
            } else {
                break;
            }
            depth += 1;
        }
        false
    }

    /// Check if offset is inside a var/let/const declaration list (for comma detection)
    fn is_in_variable_declaration_list(&self, offset: u32) -> bool {
        let node_idx = find_node_at_offset(self.arena, offset);
        let start = if node_idx.is_none() && offset > 0 {
            find_node_at_offset(self.arena, offset - 1)
        } else {
            node_idx
        };
        let mut current = start;
        let mut depth = 0;
        while !current.is_none() && depth < 50 {
            if let Some(node) = self.arena.get(current) {
                if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                    || node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                {
                    return true;
                }
            }
            if let Some(ext) = self.arena.get_extended(current) {
                if ext.parent == current {
                    break;
                }
                current = ext.parent;
            } else {
                break;
            }
            depth += 1;
        }
        false
    }

    /// Check if offset is inside a parameter list
    fn is_in_parameter_list(&self, offset: u32) -> bool {
        let node_idx = find_node_at_offset(self.arena, offset);
        let start = if node_idx.is_none() && offset > 0 {
            find_node_at_offset(self.arena, offset - 1)
        } else {
            node_idx
        };
        let mut current = start;
        let mut depth = 0;
        while !current.is_none() && depth < 50 {
            if let Some(node) = self.arena.get(current) {
                if node.kind == syntax_kind_ext::PARAMETER {
                    return true;
                }
                // Stop at function boundary
                if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                    || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || node.kind == syntax_kind_ext::METHOD_DECLARATION
                    || node.kind == syntax_kind_ext::CONSTRUCTOR
                {
                    return false;
                }
            }
            if let Some(ext) = self.arena.get_extended(current) {
                if ext.parent == current {
                    break;
                }
                current = ext.parent;
            } else {
                break;
            }
            depth += 1;
        }
        false
    }

    /// Check if offset is in a type parameter list `<T, U>`
    fn is_in_type_parameter_list(&self, offset: u32) -> bool {
        let node_idx = find_node_at_offset(self.arena, offset);
        let start = if node_idx.is_none() && offset > 0 {
            find_node_at_offset(self.arena, offset - 1)
        } else {
            node_idx
        };
        let mut current = start;
        let mut depth = 0;
        while !current.is_none() && depth < 50 {
            if let Some(node) = self.arena.get(current) {
                if node.kind == syntax_kind_ext::TYPE_PARAMETER {
                    return true;
                }
            }
            if let Some(ext) = self.arena.get_extended(current) {
                if ext.parent == current {
                    break;
                }
                current = ext.parent;
            } else {
                break;
            }
            depth += 1;
        }
        false
    }

    /// Check if offset is at an enum member name position
    fn is_in_enum_member_position(&self, offset: u32) -> bool {
        let node_idx = find_node_at_offset(self.arena, offset);
        let start = if node_idx.is_none() && offset > 0 {
            find_node_at_offset(self.arena, offset - 1)
        } else {
            node_idx
        };
        let mut current = start;
        let mut depth = 0;
        while !current.is_none() && depth < 50 {
            if let Some(node) = self.arena.get(current) {
                if node.kind == syntax_kind_ext::ENUM_MEMBER {
                    return true;
                }
                if node.kind == syntax_kind_ext::ENUM_DECLARATION {
                    // Check if cursor is after `{` (member position)
                    let text_before = &self.source_text[node.pos as usize..offset as usize];
                    if text_before.contains('{') {
                        return true;
                    }
                    return false;
                }
            }
            if let Some(ext) = self.arena.get_extended(current) {
                if ext.parent == current {
                    break;
                }
                current = ext.parent;
            } else {
                break;
            }
            depth += 1;
        }
        false
    }

    /// Text-based check: is cursor inside a regex literal?
    /// Scans backward from `i` looking for an unmatched `/pattern/` or `/pattern/flags`.
    fn text_is_inside_regex(&self, i: usize) -> bool {
        let text = &self.source_text[..i];
        // Strategy: scan backward from i looking for a `/`.
        // A regex literal is `/pattern/flags` where flags are [gimsuy]*.
        // We need to find the closing `/` and determine if we're in the flags portion.
        let bytes = text.as_bytes();

        // First check if cursor is right after potential regex flags
        let mut pos = i;
        // Skip back over potential regex flags
        while pos > 0
            && matches!(
                bytes[pos - 1],
                b'g' | b'i' | b'm' | b's' | b'u' | b'y' | b'd'
            )
        {
            pos -= 1;
        }

        // Now check if there's a `/` right before the flags position
        if pos > 0 && bytes[pos - 1] == b'/' {
            let slash_pos = pos - 1;
            // Scan backward to find the opening `/` of the regex
            if slash_pos > 0 {
                // Look for the opening slash by scanning backward
                let mut j = slash_pos - 1;
                loop {
                    if bytes[j] == b'/' {
                        // Found potential opening slash - check if it's actually a regex
                        // The character before the opening slash should be an operator, keyword,
                        // or start of line (not an identifier character or closing paren/bracket)
                        if j == 0 {
                            return true; // Start of file
                        }
                        let before = bytes[j - 1];
                        if before == b'='
                            || before == b'('
                            || before == b','
                            || before == b':'
                            || before == b';'
                            || before == b'!'
                            || before == b'&'
                            || before == b'|'
                            || before == b'?'
                            || before == b'{'
                            || before == b'}'
                            || before == b'['
                            || before == b'\n'
                            || before == b'\r'
                            || before == b'\t'
                            || before == b' '
                            || before == b'+'
                            || before == b'-'
                            || before == b'~'
                            || before == b'^'
                        {
                            return true;
                        }
                        break;
                    }
                    if bytes[j] == b'\n' || bytes[j] == b'\r' {
                        break; // Regex can't span lines
                    }
                    if j == 0 {
                        break;
                    }
                    j -= 1;
                }
            }
        }
        false
    }

    /// Text-based check: is cursor inside a template literal (backtick string)?
    /// Counts unescaped backticks before cursor; odd count means inside template.
    fn text_is_inside_template_literal(&self, i: usize) -> bool {
        let text = &self.source_text[..i];
        let bytes = text.as_bytes();
        let mut backtick_count = 0;
        let mut j = 0;
        while j < bytes.len() {
            if bytes[j] == b'\\' {
                j += 2; // Skip escaped character
                continue;
            }
            if bytes[j] == b'`' {
                backtick_count += 1;
            }
            j += 1;
        }
        // If odd number of backticks, we're inside a template literal.
        // However, we might be inside a ${} expression within the template.
        if backtick_count % 2 == 0 {
            return false;
        }
        // We're inside a template. Check if we're inside a ${} expression.
        // Scan backward from cursor for `${` that isn't matched by `}`.
        let mut brace_depth: i32 = 0;
        let mut k = i;
        while k > 0 {
            k -= 1;
            if bytes[k] == b'\\' && k > 0 {
                k -= 1; // Skip escaped chars going backward (approximate)
                continue;
            }
            if bytes[k] == b'}' {
                brace_depth += 1;
            } else if bytes[k] == b'{' {
                if k > 0 && bytes[k - 1] == b'$' {
                    if brace_depth == 0 {
                        // We're inside a ${} expression, allow completions
                        return false;
                    }
                    brace_depth -= 1;
                    k -= 1; // Skip the $
                } else {
                    // Regular { - just balance
                    if brace_depth > 0 {
                        brace_depth -= 1;
                    }
                }
            } else if bytes[k] == b'`' {
                // Hit the opening backtick without being in an expression
                return true;
            }
        }
        true
    }

    /// Text-based check: is cursor inside a string literal (single/double quotes)?
    fn text_is_inside_string_literal(&self, i: usize) -> bool {
        let text = &self.source_text[..i];
        let bytes = text.as_bytes();
        // Track quote state by scanning from beginning
        let mut in_single = false;
        let mut in_double = false;
        let mut j = 0;
        while j < bytes.len() {
            if bytes[j] == b'\\' && (in_single || in_double) {
                j += 2; // Skip escaped character
                continue;
            }
            match bytes[j] {
                b'\'' if !in_double => in_single = !in_single,
                b'"' if !in_single => in_double = !in_double,
                b'\n' | b'\r' => {
                    // Newlines terminate string literals (unless escaped, handled above)
                    in_single = false;
                    in_double = false;
                }
                _ => {}
            }
            j += 1;
        }
        in_single || in_double
    }

    /// Check if the cursor is inside a function body (for keyword selection).
    fn is_inside_function(&self, offset: u32) -> bool {
        let node_idx = find_node_at_offset(self.arena, offset);
        let start = if node_idx.is_none() && offset > 0 {
            find_node_at_offset(self.arena, offset.saturating_sub(1))
        } else {
            node_idx
        };
        let mut current = start;
        while !current.is_none() {
            if let Some(node) = self.arena.get(current) {
                let k = node.kind;
                if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::CONSTRUCTOR
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR
                {
                    return true;
                }
            }
            if let Some(ext) = self.arena.get_extended(current) {
                if ext.parent == current {
                    break;
                }
                current = ext.parent;
            } else {
                break;
            }
        }
        false
    }

    /// Find the best node for completions at the given offset.
    /// When the cursor is in whitespace, finds the smallest containing scope node.
    fn find_completions_node(&self, root: NodeIndex, offset: u32) -> NodeIndex {
        // Try exact offset first
        let mut node_idx = find_node_at_offset(self.arena, offset);
        if !node_idx.is_none() {
            return node_idx;
        }
        // Try offset-1 (common when cursor is right after a token boundary)
        if offset > 0 {
            node_idx = find_node_at_offset(self.arena, offset - 1);
            if !node_idx.is_none() {
                return node_idx;
            }
        }
        // Fallback: find the smallest node whose range contains the offset
        // This handles whitespace inside blocks where pos <= offset < end
        let mut best = root;
        let mut best_len = u32::MAX;
        for (i, node) in self.arena.nodes.iter().enumerate() {
            if node.pos <= offset && node.end >= offset {
                let len = node.end - node.pos;
                if len < best_len {
                    best_len = len;
                    best = NodeIndex(i as u32);
                }
            }
        }
        best
    }

    fn get_completions_internal(
        &self,
        root: NodeIndex,
        position: Position,
        mut type_cache: Option<&mut Option<TypeCache>>,
        scope_cache: Option<&mut ScopeCache>,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<CompletionItem>> {
        // 1. Convert position to byte offset
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;

        // 2. Filter out positions where completions should not appear
        if self.is_in_no_completion_context(offset) {
            return Some(Vec::new());
        }

        // 3. Find the node at this offset using improved lookup
        let node_idx = self.find_completions_node(root, offset);

        // 4. Check for member completion (after a dot)
        if let Some(expr_idx) = self.member_completion_target(node_idx, offset)
            && let Some(items) = self.get_member_completions(expr_idx, type_cache.as_deref_mut())
        {
            return if items.is_empty() { None } else { Some(items) };
        }

        // 5. Check for object literal property completion (contextual completions)
        if self.interner.is_some()
            && self.file_name.is_some()
            && let Some(items) = self.get_object_literal_completions(node_idx, type_cache)
        {
            return if items.is_empty() { None } else { Some(items) };
        }

        // 6. Get the scope chain at this position
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let scope_chain = if let Some(scope_cache) = scope_cache {
            Cow::Borrowed(walker.get_scope_chain_cached(root, node_idx, scope_cache, scope_stats))
        } else {
            Cow::Owned(walker.get_scope_chain(root, node_idx))
        };

        // 7. Collect all visible identifiers from the scope chain
        let mut completions = Vec::new();
        let mut seen_names = std::collections::HashSet::new();

        // Walk scopes from innermost to outermost
        for scope in scope_chain.iter().rev() {
            for (name, symbol_id) in scope.iter() {
                if seen_names.contains(name) {
                    continue;
                }
                seen_names.insert(name.clone());

                if let Some(symbol) = self.binder.symbols.get(*symbol_id) {
                    let kind = self.determine_completion_kind(symbol);
                    let mut item = CompletionItem::new(name.clone(), kind);
                    item.sort_text = Some(default_sort_text(kind).to_string());

                    if let Some(detail) = self.get_symbol_detail(symbol) {
                        item = item.with_detail(detail);
                    }
                    if let Some(modifiers) = self.build_kind_modifiers(symbol) {
                        item.kind_modifiers = Some(modifiers);
                    }
                    if kind == CompletionItemKind::Function || kind == CompletionItemKind::Method {
                        item.insert_text = Some(format!("{}($1)", name));
                        item.is_snippet = true;
                    }

                    let decl_node = if !symbol.value_declaration.is_none() {
                        symbol.value_declaration
                    } else {
                        symbol
                            .declarations
                            .first()
                            .copied()
                            .unwrap_or(NodeIndex::NONE)
                    };
                    if !decl_node.is_none() {
                        let doc = jsdoc_for_node(self.arena, root, decl_node, self.source_text);
                        if !doc.is_empty() {
                            item = item.with_documentation(doc);
                        }
                    }

                    completions.push(item);
                }
            }
        }

        // 8. Add global variables (globalThis, undefined, Array, etc.)
        //    These are always available and match tsserver's globalsVars.
        let inside_func = self.is_inside_function(offset);
        for &(name, kind) in GLOBAL_VARS {
            if !seen_names.contains(name) {
                seen_names.insert(name.to_string());
                let mut item = CompletionItem::new(name.to_string(), kind);
                let is_deprecated = DEPRECATED_GLOBALS.contains(&name);
                if is_deprecated {
                    item.sort_text =
                        Some(sort_priority::deprecated(sort_priority::GLOBALS_OR_KEYWORDS));
                    item.kind_modifiers = Some("deprecated,declare".to_string());
                } else {
                    item.sort_text = Some(sort_priority::GLOBALS_OR_KEYWORDS.to_string());
                    // globalThis and undefined don't get "declare" modifier
                    if name != "globalThis" && name != "undefined" {
                        item.kind_modifiers = Some("declare".to_string());
                    }
                }
                if kind == CompletionItemKind::Function {
                    item.insert_text = Some(format!("{}($1)", name));
                    item.is_snippet = true;
                }
                completions.push(item);
            }
        }

        // 9. If inside a function, also add "arguments" as a local variable
        if inside_func {
            if !seen_names.contains("arguments") {
                seen_names.insert("arguments".to_string());
                let mut item =
                    CompletionItem::new("arguments".to_string(), CompletionItemKind::Variable);
                item.sort_text = Some(sort_priority::LOCAL_DECLARATION.to_string());
                completions.push(item);
            }
        }

        // 10. Add keywords for non-member completions
        let keywords = if inside_func {
            KEYWORDS_INSIDE_FUNCTION
        } else {
            KEYWORDS
        };
        for &kw in keywords {
            if !seen_names.contains(kw) {
                let mut kw_item = CompletionItem::new(kw.to_string(), CompletionItemKind::Keyword);
                kw_item.sort_text = Some(sort_priority::KEYWORD.to_string());
                completions.push(kw_item);
            }
        }

        if completions.is_empty() {
            None
        } else {
            completions.sort_by(|a, b| {
                let sa = a.effective_sort_text();
                let sb = b.effective_sort_text();
                compare_case_sensitive_ui(sa, sb)
                    .then_with(|| compare_case_sensitive_ui(&a.label, &b.label))
            });
            Some(completions)
        }
    }

    /// Determine the completion kind from a symbol.
    fn determine_completion_kind(&self, symbol: &crate::binder::Symbol) -> CompletionItemKind {
        use crate::binder::symbol_flags;

        if symbol.flags & symbol_flags::CONSTRUCTOR != 0 {
            CompletionItemKind::Constructor
        } else if symbol.flags & symbol_flags::FUNCTION != 0 {
            CompletionItemKind::Function
        } else if symbol.flags & symbol_flags::CLASS != 0 {
            CompletionItemKind::Class
        } else if symbol.flags & symbol_flags::INTERFACE != 0 {
            CompletionItemKind::Interface
        } else if symbol.flags & symbol_flags::REGULAR_ENUM != 0
            || symbol.flags & symbol_flags::CONST_ENUM != 0
        {
            CompletionItemKind::Enum
        } else if symbol.flags & symbol_flags::TYPE_ALIAS != 0 {
            CompletionItemKind::TypeAlias
        } else if symbol.flags & symbol_flags::TYPE_PARAMETER != 0 {
            CompletionItemKind::TypeParameter
        } else if symbol.flags & symbol_flags::METHOD != 0 {
            CompletionItemKind::Method
        } else if symbol.flags & symbol_flags::PROPERTY != 0 {
            CompletionItemKind::Property
        } else if symbol.flags & symbol_flags::VALUE_MODULE != 0
            || symbol.flags & symbol_flags::NAMESPACE_MODULE != 0
        {
            CompletionItemKind::Module
        } else {
            // Default to variable for const, let, var, and parameters
            CompletionItemKind::Variable
        }
    }

    /// Get detail information for a symbol (e.g., "const", "function", "class").
    fn get_symbol_detail(&self, symbol: &crate::binder::Symbol) -> Option<String> {
        use crate::binder::symbol_flags;

        if symbol.flags & symbol_flags::FUNCTION != 0 {
            Some("function".to_string())
        } else if symbol.flags & symbol_flags::CLASS != 0 {
            Some("class".to_string())
        } else if symbol.flags & symbol_flags::INTERFACE != 0 {
            Some("interface".to_string())
        } else if symbol.flags & symbol_flags::REGULAR_ENUM != 0
            || symbol.flags & symbol_flags::CONST_ENUM != 0
        {
            Some("enum".to_string())
        } else if symbol.flags & symbol_flags::TYPE_ALIAS != 0 {
            Some("type".to_string())
        } else if symbol.flags & symbol_flags::TYPE_PARAMETER != 0 {
            Some("type parameter".to_string())
        } else if symbol.flags & symbol_flags::METHOD != 0 {
            Some("method".to_string())
        } else if symbol.flags & symbol_flags::PROPERTY != 0 {
            Some("property".to_string())
        } else if symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0 {
            Some("let/const".to_string())
        } else if symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0 {
            Some("var".to_string())
        } else if symbol.flags & symbol_flags::VALUE_MODULE != 0
            || symbol.flags & symbol_flags::NAMESPACE_MODULE != 0
        {
            Some("module".to_string())
        } else {
            None
        }
    }

    /// Build a comma-separated `kindModifiers` string for a symbol, matching
    /// tsserver's convention: `"export"`, `"declare"`, `"abstract"`, `"static"`,
    /// `"private"`, `"protected"`.
    fn build_kind_modifiers(&self, symbol: &crate::binder::Symbol) -> Option<String> {
        use crate::binder::symbol_flags;

        let mut mods = Vec::new();
        if symbol.flags & symbol_flags::EXPORT_VALUE != 0 {
            mods.push("export");
        }
        if symbol.flags & symbol_flags::ABSTRACT != 0 {
            mods.push("abstract");
        }
        if symbol.flags & symbol_flags::STATIC != 0 {
            mods.push("static");
        }
        if symbol.flags & symbol_flags::PRIVATE != 0 {
            mods.push("private");
        }
        if symbol.flags & symbol_flags::PROTECTED != 0 {
            mods.push("protected");
        }
        if symbol.flags & symbol_flags::OPTIONAL != 0 {
            mods.push("optional");
        }
        if mods.is_empty() {
            None
        } else {
            Some(mods.join(","))
        }
    }

    fn member_completion_target(&self, node_idx: NodeIndex, offset: u32) -> Option<NodeIndex> {
        let mut current = node_idx;

        while !current.is_none() {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let access = self.arena.get_access_expr(node)?;
                let expr_node = self.arena.get(access.expression)?;
                if offset >= expr_node.end && offset <= node.end {
                    return Some(access.expression);
                }
            }

            let ext = self.arena.get_extended(current)?;
            current = ext.parent;
        }

        None
    }

    fn get_member_completions(
        &self,
        expr_idx: NodeIndex,
        type_cache: Option<&mut Option<TypeCache>>,
    ) -> Option<Vec<CompletionItem>> {
        let interner = self.interner?;
        let file_name = self.file_name.as_ref()?;

        let mut cache_ref = type_cache;
        let compiler_options = crate::checker::context::CheckerOptions {
            strict: self.strict,
            no_implicit_any: self.strict,
            no_implicit_returns: false,
            no_implicit_this: self.strict,
            strict_null_checks: self.strict,
            strict_function_types: self.strict,
            strict_property_initialization: self.strict,
            use_unknown_in_catch_variables: self.strict,
            isolated_modules: false,
            ..Default::default()
        };
        let mut checker = if let Some(cache) = cache_ref.as_deref_mut() {
            if let Some(cache_value) = cache.take() {
                CheckerState::with_cache(
                    self.arena,
                    self.binder,
                    interner,
                    file_name.clone(),
                    cache_value,
                    compiler_options.clone(),
                )
            } else {
                CheckerState::new(
                    self.arena,
                    self.binder,
                    interner,
                    file_name.clone(),
                    compiler_options.clone(),
                )
            }
        } else {
            CheckerState::new(
                self.arena,
                self.binder,
                interner,
                file_name.clone(),
                compiler_options,
            )
        };

        let type_id = checker.get_type_of_node(expr_idx);
        let mut visited = FxHashSet::default();
        let mut props: FxHashMap<String, PropertyCompletion> = FxHashMap::default();
        self.collect_properties_for_type(type_id, interner, &mut checker, &mut visited, &mut props);

        let mut items = Vec::new();
        for (name, info) in props {
            let kind = if info.is_method {
                CompletionItemKind::Method
            } else {
                CompletionItemKind::Property
            };
            let mut item = CompletionItem::new(name.clone(), kind);
            item = item.with_detail(checker.format_type(info.type_id));
            item.sort_text = Some(sort_priority::MEMBER.to_string());

            // Add snippet insert text for method completions
            if info.is_method {
                item.insert_text = Some(format!("{}($1)", name));
                item.is_snippet = true;
            }

            items.push(item);
        }

        items.sort_by(|a, b| a.label.cmp(&b.label));
        if let Some(cache) = cache_ref {
            *cache = Some(checker.extract_cache());
        }
        Some(items)
    }

    fn collect_properties_for_type(
        &self,
        type_id: TypeId,
        interner: &TypeInterner,
        checker: &mut CheckerState,
        visited: &mut FxHashSet<TypeId>,
        props: &mut FxHashMap<String, PropertyCompletion>,
    ) {
        if !visited.insert(type_id) {
            return;
        }

        let key = match interner.lookup(type_id) {
            Some(key) => key,
            None => return,
        };

        match key {
            TypeKey::Object(shape_id) => {
                let shape = interner.object_shape(shape_id);
                for prop in shape.properties.iter() {
                    let name = interner.resolve_atom(prop.name);
                    self.add_property_completion(
                        props,
                        interner,
                        name,
                        prop.type_id,
                        prop.is_method,
                    );
                }
            }
            TypeKey::ObjectWithIndex(shape_id) => {
                let shape = interner.object_shape(shape_id);
                for prop in shape.properties.iter() {
                    let name = interner.resolve_atom(prop.name);
                    self.add_property_completion(
                        props,
                        interner,
                        name,
                        prop.type_id,
                        prop.is_method,
                    );
                }
            }
            TypeKey::Union(members) | TypeKey::Intersection(members) => {
                let members = interner.type_list(members);
                for &member in members.iter() {
                    self.collect_properties_for_type(member, interner, checker, visited, props);
                }
            }
            TypeKey::Ref(symbol_ref) => {
                let type_id = checker.get_type_of_symbol(SymbolId(symbol_ref.0));
                self.collect_properties_for_type(type_id, interner, checker, visited, props);
            }
            TypeKey::Application(app) => {
                let app = interner.type_application(app);
                self.collect_properties_for_type(app.base, interner, checker, visited, props);
            }
            TypeKey::Literal(literal) => {
                if let Some(kind) = self.literal_intrinsic_kind(&literal) {
                    self.collect_intrinsic_members(kind, interner, props);
                }
            }
            TypeKey::TemplateLiteral(_) => {
                self.collect_intrinsic_members(IntrinsicKind::String, interner, props);
            }
            TypeKey::Intrinsic(kind) => {
                self.collect_intrinsic_members(kind, interner, props);
            }
            _ => {}
        }
    }

    fn collect_intrinsic_members(
        &self,
        kind: IntrinsicKind,
        interner: &TypeInterner,
        props: &mut FxHashMap<String, PropertyCompletion>,
    ) {
        let members = apparent_primitive_members(interner, kind);
        for member in members {
            let type_id = match member.kind {
                ApparentMemberKind::Value(type_id) => type_id,
                ApparentMemberKind::Method(type_id) => type_id,
            };
            let is_method = matches!(member.kind, ApparentMemberKind::Method(_));
            self.add_property_completion(
                props,
                interner,
                member.name.to_string(),
                type_id,
                is_method,
            );
        }
    }

    fn literal_intrinsic_kind(
        &self,
        literal: &crate::solver::LiteralValue,
    ) -> Option<IntrinsicKind> {
        match literal {
            crate::solver::LiteralValue::String(_) => Some(IntrinsicKind::String),
            crate::solver::LiteralValue::Number(_) => Some(IntrinsicKind::Number),
            crate::solver::LiteralValue::Boolean(_) => Some(IntrinsicKind::Boolean),
            crate::solver::LiteralValue::BigInt(_) => Some(IntrinsicKind::Bigint),
        }
    }

    fn add_property_completion(
        &self,
        props: &mut FxHashMap<String, PropertyCompletion>,
        interner: &TypeInterner,
        name: String,
        type_id: TypeId,
        is_method: bool,
    ) {
        if let Some(existing) = props.get_mut(&name) {
            if existing.type_id != type_id {
                existing.type_id = interner.union(vec![existing.type_id, type_id]);
            }
            existing.is_method |= is_method;
        } else {
            props.insert(name, PropertyCompletion { type_id, is_method });
        }
    }

    /// Suggest properties for object literals based on contextual type.
    /// When typing inside `{ | }`, suggests properties from the expected type.
    fn get_object_literal_completions(
        &self,
        node_idx: NodeIndex,
        type_cache: Option<&mut Option<TypeCache>>,
    ) -> Option<Vec<CompletionItem>> {
        let interner = self.interner?;
        let file_name = self.file_name.as_ref()?;

        // 1. Find the enclosing object literal
        let object_literal_idx = self.find_enclosing_object_literal(node_idx)?;

        // 2. Determine the contextual type (expected type)
        let mut cache_ref = type_cache;
        let compiler_options = crate::checker::context::CheckerOptions {
            strict: self.strict,
            no_implicit_any: self.strict,
            no_implicit_returns: false,
            no_implicit_this: self.strict,
            strict_null_checks: self.strict,
            strict_function_types: self.strict,
            strict_property_initialization: self.strict,
            use_unknown_in_catch_variables: self.strict,
            isolated_modules: false,
            ..Default::default()
        };
        let mut checker = if let Some(cache) = cache_ref.as_deref_mut() {
            if let Some(cache_value) = cache.take() {
                CheckerState::with_cache(
                    self.arena,
                    self.binder,
                    interner,
                    file_name.clone(),
                    cache_value,
                    compiler_options.clone(),
                )
            } else {
                CheckerState::new(
                    self.arena,
                    self.binder,
                    interner,
                    file_name.clone(),
                    compiler_options.clone(),
                )
            }
        } else {
            CheckerState::new(
                self.arena,
                self.binder,
                interner,
                file_name.clone(),
                compiler_options,
            )
        };

        let context_type = self.get_contextual_type(object_literal_idx, &mut checker)?;

        // 3. Find properties already defined in this literal
        let existing_props = self.get_defined_properties(object_literal_idx);

        // 4. Collect properties from the expected type
        let mut items = Vec::new();
        let mut props: FxHashMap<String, PropertyCompletion> = FxHashMap::default();
        let mut visited = FxHashSet::default();

        self.collect_properties_for_type(
            context_type,
            interner,
            &mut checker,
            &mut visited,
            &mut props,
        );

        for (name, info) in props {
            // Suggest only missing properties
            if !existing_props.contains(&name) {
                let kind = if info.is_method {
                    CompletionItemKind::Method
                } else {
                    CompletionItemKind::Property
                };

                let mut item = CompletionItem::new(name.clone(), kind);
                item = item.with_detail(checker.format_type(info.type_id));
                item.sort_text = Some(sort_priority::MEMBER.to_string());

                // Add snippet insert text for method completions in object literals
                if info.is_method {
                    item.insert_text = Some(format!("{}($1)", name));
                    item.is_snippet = true;
                }

                items.push(item);
            }
        }

        if let Some(cache) = cache_ref {
            *cache = Some(checker.extract_cache());
        }

        if items.is_empty() {
            None
        } else {
            items.sort_by(|a, b| a.label.cmp(&b.label));
            Some(items)
        }
    }

    /// Find the enclosing object literal expression for a given node.
    fn find_enclosing_object_literal(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.arena.get(node_idx)?;

        // Cursor is directly on the literal (e.g. empty {})
        if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return Some(node_idx);
        }

        // Cursor is on a child (identifier, property, etc.)
        let ext = self.arena.get_extended(node_idx)?;
        let parent = self.arena.get(ext.parent)?;

        if parent.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return Some(ext.parent);
        }

        // Cursor is deep (e.g. inside a property assignment value)
        // Handle { prop: | } or { prop }
        if parent.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
            let grand_ext = self.arena.get_extended(ext.parent)?;
            let grand_parent = self.arena.get(grand_ext.parent)?;
            if grand_parent.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                return Some(grand_ext.parent);
            }
        }

        // Also check for shorthand property assignment
        if parent.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
            let grand_ext = self.arena.get_extended(ext.parent)?;
            let grand_parent = self.arena.get(grand_ext.parent)?;
            if grand_parent.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                return Some(grand_ext.parent);
            }
        }

        None
    }

    /// Get the set of property names already defined in an object literal.
    fn get_defined_properties(&self, object_literal_idx: NodeIndex) -> FxHashSet<String> {
        let mut names = FxHashSet::default();
        let node = self.arena.get(object_literal_idx).unwrap();

        if let Some(lit) = self.arena.get_literal_expr(node) {
            for &prop_idx in &lit.elements.nodes {
                if let Some(name) = self.get_property_name(prop_idx) {
                    names.insert(name);
                }
            }
        }
        names
    }

    /// Extract the property name from a property assignment or shorthand.
    fn get_property_name(&self, prop_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(prop_idx)?;
        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_property_assignment(node)?;
                self.arena
                    .get_identifier_text(prop.name)
                    .map(|s| s.to_string())
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_shorthand_property(node)?;
                self.arena
                    .get_identifier_text(prop.name)
                    .map(|s| s.to_string())
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.arena.get_method_decl(node)?;
                self.arena
                    .get_identifier_text(method.name)
                    .map(|s| s.to_string())
            }
            _ => None,
        }
    }

    /// Walk up the AST to find the expected/contextual type for a node.
    fn get_contextual_type(
        &self,
        node_idx: NodeIndex,
        checker: &mut CheckerState,
    ) -> Option<TypeId> {
        let ext = self.arena.get_extended(node_idx)?;
        let parent_idx = ext.parent;
        let parent = self.arena.get(parent_idx)?;

        match parent.kind {
            // const x: Type = { ... }
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                let decl = self.arena.get_variable_declaration(parent)?;
                if decl.initializer == node_idx && !decl.type_annotation.is_none() {
                    return Some(checker.get_type_of_node(decl.type_annotation));
                }
            }
            // { prop: { ... } } -> Recurse to parent object
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_property_assignment(parent)?;
                if prop.initializer == node_idx {
                    let grand_parent_ext = self.arena.get_extended(parent_idx)?;
                    let grand_parent_idx = grand_parent_ext.parent;

                    // Get context of the parent object
                    let parent_context = self.get_contextual_type(grand_parent_idx, checker)?;

                    // Look up this property in the parent context
                    let prop_name = self.arena.get_identifier_text(prop.name)?;
                    return self.lookup_property_type(parent_context, prop_name, checker);
                }
            }
            // return { ... }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                let func_idx = self.find_enclosing_function(parent_idx)?;
                let func_node = self.arena.get(func_idx)?;

                // Check return type annotation
                if let Some(func) = self.arena.get_function(func_node)
                    && !func.type_annotation.is_none()
                {
                    return Some(checker.get_type_of_node(func.type_annotation));
                }
            }
            // function call argument: foo({ ... })
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                let call = self.arena.get_call_expr(parent)?;
                // Find which argument position this node is at
                let arg_index = call
                    .arguments
                    .as_ref()
                    .and_then(|args| args.nodes.iter().position(|&arg| arg == node_idx));

                if let Some(arg_idx) = arg_index {
                    // Get the function signature type
                    let func_type = checker.get_type_of_node(call.expression);
                    return self.get_parameter_type_at(func_type, arg_idx, checker);
                }
            }
            _ => {}
        }
        None
    }

    /// Find the type of a property from an object type.
    fn lookup_property_type(
        &self,
        type_id: TypeId,
        name: &str,
        checker: &mut CheckerState,
    ) -> Option<TypeId> {
        let mut props = FxHashMap::default();
        let mut visited = FxHashSet::default();
        let interner = self.interner?;

        self.collect_properties_for_type(type_id, interner, checker, &mut visited, &mut props);
        props.get(name).map(|p| p.type_id)
    }

    /// Find the enclosing function for a node (for return type lookup).
    fn find_enclosing_function(&self, start_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = start_idx;
        while !current.is_none() {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || node.kind == syntax_kind_ext::ARROW_FUNCTION
                || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            {
                return Some(current);
            }
            let ext = self.arena.get_extended(current)?;
            current = ext.parent;
        }
        None
    }

    /// Get the type of the Nth parameter of a function type.
    fn get_parameter_type_at(
        &self,
        func_type: TypeId,
        param_index: usize,
        _checker: &mut CheckerState,
    ) -> Option<TypeId> {
        let interner = self.interner?;

        // Look up the callable signature
        if let Some(key) = interner.lookup(func_type)
            && let TypeKey::Callable(callable_id) = key
        {
            let callable = interner.callable_shape(callable_id);
            // Use the first call signature
            if let Some(first_sig) = callable.call_signatures.first()
                && param_index < first_sig.params.len()
            {
                return Some(first_sig.params[param_index].type_id);
            }
        }
        None
    }
}

#[derive(Clone, Copy, Debug)]
struct PropertyCompletion {
    type_id: TypeId,
    is_method: bool,
}

#[cfg(test)]
mod completions_tests {
    use super::*;
    use crate::binder::BinderState;
    use crate::lsp::position::LineMap;
    use crate::parser::ParserState;
    use crate::solver::TypeInterner;

    #[test]
    fn test_completions_simple() {
        // const x = 1;
        // const y = 2;
        // |  <- cursor here
        let source = "const x = 1;\nconst y = 2;\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the end (line 2, column 0)
        let position = Position::new(2, 0);

        let completions = Completions::new(arena, &binder, &line_map, source);
        let items = completions.get_completions(root, position);

        assert!(items.is_some(), "Should have completions");

        if let Some(items) = items {
            // Should suggest both x and y
            assert!(items.len() >= 2, "Should have at least 2 completions");

            let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
            assert!(names.contains(&"x"), "Should suggest 'x'");
            assert!(names.contains(&"y"), "Should suggest 'y'");
        }
    }

    #[test]
    fn test_completions_with_scope() {
        // const x = 1;
        // function foo() {
        //   const y = 2;
        //   |  <- cursor here (should see both x and y)
        // }
        let source = "const x = 1;\nfunction foo() {\n  const y = 2;\n  \n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position inside the function (line 3, column 2)
        let position = Position::new(3, 2);

        let completions = Completions::new(arena, &binder, &line_map, source);
        let items = completions.get_completions(root, position);

        assert!(items.is_some(), "Should have completions");

        if let Some(items) = items {
            let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();

            // Should see both x (outer scope) and y (inner scope)
            assert!(names.contains(&"x"), "Should suggest 'x' from outer scope");
            assert!(names.contains(&"y"), "Should suggest 'y' from inner scope");
            assert!(
                names.contains(&"foo"),
                "Should suggest 'foo' (the function itself)"
            );
        }
    }

    #[test]
    fn test_completions_shadowing() {
        // const x = 1;
        // function foo() {
        //   const x = 2;
        //   |  <- cursor here (should see inner x, not outer x)
        // }
        let source = "const x = 1;\nfunction foo() {\n  const x = 2;\n  \n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position inside the function (line 3, column 2)
        let position = Position::new(3, 2);

        let completions = Completions::new(arena, &binder, &line_map, source);
        let items = completions.get_completions(root, position);

        assert!(items.is_some(), "Should have completions");

        if let Some(items) = items {
            let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();

            // Should only suggest 'x' once (the inner one shadows the outer one)
            let x_count = names.iter().filter(|&&n| n == "x").count();
            assert_eq!(
                x_count, 1,
                "Should suggest 'x' only once (inner shadows outer)"
            );
        }
    }

    #[test]
    fn test_completions_member_object_literal() {
        let source = "const obj = { foo: 1, bar: \"hi\" };\nobj.";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let interner = TypeInterner::new();
        let completions = Completions::new_with_types(
            arena,
            &binder,
            &line_map,
            &interner,
            source,
            "test.ts".to_string(),
        );

        let position = Position::new(1, 4);
        let mut cache = None;
        let items = completions.get_completions_with_cache(root, position, &mut cache);

        assert!(items.is_some(), "Should have member completions");
        let items = items.unwrap();
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();

        assert!(names.contains(&"foo"), "Should suggest object member 'foo'");
        assert!(names.contains(&"bar"), "Should suggest object member 'bar'");
    }

    #[test]
    fn test_completions_member_string_literal() {
        let source = "const s = \"hello\";\ns.";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let interner = TypeInterner::new();
        let completions = Completions::new_with_types(
            arena,
            &binder,
            &line_map,
            &interner,
            source,
            "test.ts".to_string(),
        );

        let position = Position::new(1, 2);
        let mut cache = None;
        let items = completions.get_completions_with_cache(root, position, &mut cache);

        assert!(items.is_some(), "Should have member completions");
        let items = items.unwrap();
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();

        assert!(
            names.contains(&"length"),
            "Should suggest string member 'length'"
        );
    }

    #[test]
    fn test_completions_includes_keywords() {
        let source = "const x = 1;\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the end
        let position = Position::new(1, 0);

        let completions = Completions::new(arena, &binder, &line_map, source);
        let items = completions.get_completions(root, position);

        assert!(items.is_some(), "Should have completions");

        if let Some(items) = items {
            let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();

            // Should include keywords
            assert!(
                names.contains(&"function"),
                "Should suggest keyword 'function'"
            );
            assert!(names.contains(&"const"), "Should suggest keyword 'const'");
            assert!(names.contains(&"class"), "Should suggest keyword 'class'");
        }
    }

    #[test]
    fn test_completions_jsdoc_documentation() {
        // Test that JSDoc comments are included in completion items
        let source = "/** This is a test function */\nfunction foo() {}\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the end
        let position = Position::new(2, 0);

        let completions = Completions::new(arena, &binder, &line_map, source);
        let items = completions.get_completions(root, position);

        assert!(items.is_some(), "Should have completions");

        if let Some(items) = items {
            let foo_item = items.iter().find(|i| i.label == "foo");
            assert!(foo_item.is_some(), "Should suggest 'foo'");

            if let Some(item) = foo_item {
                assert!(
                    item.documentation
                        .as_ref()
                        .is_some_and(|d| d.contains("test function")),
                    "Should include JSDoc documentation"
                );
            }
        }
    }

    // =========================================================================
    // New tests for improved tsserver-compatible completion entry format
    // =========================================================================

    #[test]
    fn test_completions_sort_text_keywords_after_identifiers() {
        // Keywords should have higher sort_text than identifiers so they
        // appear later in the completion list, matching tsserver behaviour.
        let source = "const abc = 1;\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let position = Position::new(1, 0);

        let completions = Completions::new(arena, &binder, &line_map, source);
        let items = completions.get_completions(root, position).unwrap();

        let abc_item = items.iter().find(|i| i.label == "abc").unwrap();
        let kw_item = items.iter().find(|i| i.label == "function").unwrap();

        assert!(
            abc_item.effective_sort_text() < kw_item.effective_sort_text(),
            "Identifiers (sort_text={:?}) should sort before keywords (sort_text={:?})",
            abc_item.effective_sort_text(),
            kw_item.effective_sort_text(),
        );
    }

    #[test]
    fn test_completions_sort_text_present_on_all_items() {
        // Every completion item should have an explicit sort_text value set.
        let source = "const x = 1;\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let position = Position::new(1, 0);

        let completions = Completions::new(arena, &binder, &line_map, source);
        let items = completions.get_completions(root, position).unwrap();

        for item in &items {
            assert!(
                item.sort_text.is_some(),
                "Item '{}' (kind={:?}) should have explicit sort_text",
                item.label,
                item.kind,
            );
        }
    }

    #[test]
    fn test_completions_function_has_snippet_insert_text() {
        // Function completions should have insert_text with snippet tab-stops
        // e.g. "foo($1)" so the cursor lands inside the parens.
        let source = "function greet(name: string) {}\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let position = Position::new(1, 0);

        let completions = Completions::new(arena, &binder, &line_map, source);
        let items = completions.get_completions(root, position).unwrap();

        let greet_item = items.iter().find(|i| i.label == "greet").unwrap();

        assert_eq!(
            greet_item.kind,
            CompletionItemKind::Function,
            "greet should be a Function"
        );
        assert_eq!(
            greet_item.insert_text.as_deref(),
            Some("greet($1)"),
            "Function completion should have snippet insert text"
        );
        assert!(
            greet_item.is_snippet,
            "Function completion should be marked as snippet"
        );
    }

    #[test]
    fn test_completions_variable_no_snippet() {
        // Variable completions should NOT have snippet insert_text.
        let source = "const value = 42;\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let position = Position::new(1, 0);

        let completions = Completions::new(arena, &binder, &line_map, source);
        let items = completions.get_completions(root, position).unwrap();

        let var_item = items.iter().find(|i| i.label == "value").unwrap();

        assert_eq!(
            var_item.kind,
            CompletionItemKind::Variable,
            "value should be a Variable"
        );
        assert!(
            var_item.insert_text.is_none(),
            "Variable completion should not have insert_text"
        );
        assert!(
            !var_item.is_snippet,
            "Variable completion should not be a snippet"
        );
    }

    #[test]
    fn test_completions_keyword_sort_text_value() {
        // All keyword completions should have sort_text == sort_priority::KEYWORD.
        let source = "const x = 1;\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let position = Position::new(1, 0);

        let completions = Completions::new(arena, &binder, &line_map, source);
        let items = completions.get_completions(root, position).unwrap();

        let keyword_items: Vec<_> = items
            .iter()
            .filter(|i| i.kind == CompletionItemKind::Keyword)
            .collect();

        assert!(!keyword_items.is_empty(), "Should have keyword completions");

        for kw in &keyword_items {
            assert_eq!(
                kw.sort_text.as_deref(),
                Some(sort_priority::KEYWORD),
                "Keyword '{}' should have sort_text='{}'",
                kw.label,
                sort_priority::KEYWORD,
            );
        }
    }

    #[test]
    fn test_completions_interface_kind() {
        // Interfaces should be reported as CompletionItemKind::Interface.
        let source = "interface Foo { x: number }\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let position = Position::new(1, 0);

        let completions = Completions::new(arena, &binder, &line_map, source);
        let items = completions.get_completions(root, position).unwrap();

        let foo_item = items.iter().find(|i| i.label == "Foo").unwrap();

        assert_eq!(
            foo_item.kind,
            CompletionItemKind::Interface,
            "Foo should be reported as Interface kind"
        );
        assert_eq!(
            foo_item.detail.as_deref(),
            Some("interface"),
            "Interface detail should be 'interface'"
        );
    }

    #[test]
    fn test_completions_enum_kind() {
        // Enums should be reported as CompletionItemKind::Enum.
        let source = "enum Color { Red, Green, Blue }\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let position = Position::new(1, 0);

        let completions = Completions::new(arena, &binder, &line_map, source);
        let items = completions.get_completions(root, position).unwrap();

        let color_item = items.iter().find(|i| i.label == "Color").unwrap();

        assert_eq!(
            color_item.kind,
            CompletionItemKind::Enum,
            "Color should be reported as Enum kind"
        );
        assert_eq!(
            color_item.detail.as_deref(),
            Some("enum"),
            "Enum detail should be 'enum'"
        );
    }

    #[test]
    fn test_completions_type_alias_kind() {
        // Type aliases should be reported as CompletionItemKind::TypeAlias.
        let source = "type MyStr = string;\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let position = Position::new(1, 0);

        let completions = Completions::new(arena, &binder, &line_map, source);
        let items = completions.get_completions(root, position).unwrap();

        let mystr_item = items.iter().find(|i| i.label == "MyStr").unwrap();

        assert_eq!(
            mystr_item.kind,
            CompletionItemKind::TypeAlias,
            "MyStr should be reported as TypeAlias kind"
        );
        assert_eq!(
            mystr_item.detail.as_deref(),
            Some("type"),
            "Type alias detail should be 'type'"
        );
    }

    #[test]
    fn test_completions_class_kind_preserved() {
        // Classes should still be reported as CompletionItemKind::Class.
        let source = "class Animal {}\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let position = Position::new(1, 0);

        let completions = Completions::new(arena, &binder, &line_map, source);
        let items = completions.get_completions(root, position).unwrap();

        let animal_item = items.iter().find(|i| i.label == "Animal").unwrap();

        assert_eq!(
            animal_item.kind,
            CompletionItemKind::Class,
            "Animal should be reported as Class kind"
        );
        assert_eq!(
            animal_item.detail.as_deref(),
            Some("class"),
            "Class detail should be 'class'"
        );
    }

    #[test]
    fn test_completions_member_sort_text() {
        // Member completions should all have sort_text set to the member priority.
        let source = "const obj = { foo: 1, bar: \"hi\" };\nobj.";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let interner = TypeInterner::new();
        let completions = Completions::new_with_types(
            arena,
            &binder,
            &line_map,
            &interner,
            source,
            "test.ts".to_string(),
        );

        let position = Position::new(1, 4);
        let mut cache = None;
        let items = completions
            .get_completions_with_cache(root, position, &mut cache)
            .unwrap();

        for item in &items {
            assert_eq!(
                item.sort_text.as_deref(),
                Some(sort_priority::MEMBER),
                "Member completion '{}' should have MEMBER sort priority",
                item.label,
            );
        }
    }

    #[test]
    fn test_completions_default_sort_text_function() {
        // default_sort_text should return correct categories for each kind.
        assert_eq!(
            default_sort_text(CompletionItemKind::Variable),
            sort_priority::LOCAL_DECLARATION
        );
        assert_eq!(
            default_sort_text(CompletionItemKind::Function),
            sort_priority::LOCAL_DECLARATION
        );
        assert_eq!(
            default_sort_text(CompletionItemKind::Parameter),
            sort_priority::LOCAL_DECLARATION
        );
        assert_eq!(
            default_sort_text(CompletionItemKind::Property),
            sort_priority::MEMBER
        );
        assert_eq!(
            default_sort_text(CompletionItemKind::Method),
            sort_priority::MEMBER
        );
        assert_eq!(
            default_sort_text(CompletionItemKind::Class),
            sort_priority::TYPE_DECLARATION
        );
        assert_eq!(
            default_sort_text(CompletionItemKind::Interface),
            sort_priority::TYPE_DECLARATION
        );
        assert_eq!(
            default_sort_text(CompletionItemKind::Enum),
            sort_priority::TYPE_DECLARATION
        );
        assert_eq!(
            default_sort_text(CompletionItemKind::TypeAlias),
            sort_priority::TYPE_DECLARATION
        );
        assert_eq!(
            default_sort_text(CompletionItemKind::Module),
            sort_priority::TYPE_DECLARATION
        );
        assert_eq!(
            default_sort_text(CompletionItemKind::TypeParameter),
            sort_priority::TYPE_DECLARATION
        );
        assert_eq!(
            default_sort_text(CompletionItemKind::Keyword),
            sort_priority::KEYWORD
        );
    }

    #[test]
    fn test_completions_has_action_default_false() {
        // By default, completions should have has_action = false.
        let source = "const x = 1;\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let position = Position::new(1, 0);

        let completions = Completions::new(arena, &binder, &line_map, source);
        let items = completions.get_completions(root, position).unwrap();

        for item in &items {
            assert!(
                !item.has_action,
                "Item '{}' should not have has_action set (reserved for auto-imports)",
                item.label,
            );
        }
    }

    #[test]
    fn test_completions_source_default_none() {
        // By default, source and source_display should be None
        // (they are only set for auto-import completions from the Project layer).
        let source = "const x = 1;\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let position = Position::new(1, 0);

        let completions = Completions::new(arena, &binder, &line_map, source);
        let items = completions.get_completions(root, position).unwrap();

        for item in &items {
            assert!(
                item.source.is_none(),
                "Item '{}' should not have source set (only for auto-imports)",
                item.label,
            );
            assert!(
                item.source_display.is_none(),
                "Item '{}' should not have source_display set",
                item.label,
            );
        }
    }

    #[test]
    fn test_completions_effective_sort_text_uses_explicit() {
        // When sort_text is explicitly set, effective_sort_text returns it.
        let mut item = CompletionItem::new("test".to_string(), CompletionItemKind::Variable);
        item.sort_text = Some("99".to_string());
        assert_eq!(item.effective_sort_text(), "99");
    }

    #[test]
    fn test_completions_effective_sort_text_uses_default() {
        // When sort_text is None, effective_sort_text returns the default.
        let item = CompletionItem::new("test".to_string(), CompletionItemKind::Keyword);
        assert_eq!(
            item.effective_sort_text(),
            sort_priority::KEYWORD,
            "Default sort text for keyword should be KEYWORD priority"
        );
    }

    #[test]
    fn test_completions_builder_methods() {
        // Test all the new builder methods on CompletionItem.
        let item = CompletionItem::new("foo".to_string(), CompletionItemKind::Function)
            .with_detail("function".to_string())
            .with_documentation("A foo function".to_string())
            .with_sort_text("0")
            .with_insert_text("foo($1)".to_string())
            .as_snippet()
            .with_has_action()
            .with_source("./module".to_string())
            .with_source_display("module".to_string())
            .with_kind_modifiers("export".to_string())
            .with_replacement_span(10, 13);

        assert_eq!(item.label, "foo");
        assert_eq!(item.kind, CompletionItemKind::Function);
        assert_eq!(item.detail.as_deref(), Some("function"));
        assert_eq!(item.documentation.as_deref(), Some("A foo function"));
        assert_eq!(item.sort_text.as_deref(), Some("0"));
        assert_eq!(item.insert_text.as_deref(), Some("foo($1)"));
        assert!(item.is_snippet);
        assert!(item.has_action);
        assert_eq!(item.source.as_deref(), Some("./module"));
        assert_eq!(item.source_display.as_deref(), Some("module"));
        assert_eq!(item.kind_modifiers.as_deref(), Some("export"));
        assert_eq!(item.replacement_span, Some((10, 13)));
    }

    #[test]
    fn test_completions_items_sorted_by_sort_text_then_label() {
        // Items should be ordered first by sort_text, then alphabetically
        // by label within each sort_text group.
        let source = "const banana = 1;\nfunction apple() {}\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let position = Position::new(2, 0);

        let completions = Completions::new(arena, &binder, &line_map, source);
        let items = completions.get_completions(root, position).unwrap();

        // Identifiers (apple, banana) should appear before keywords
        let ident_items: Vec<_> = items
            .iter()
            .filter(|i| i.kind != CompletionItemKind::Keyword)
            .collect();
        let kw_items: Vec<_> = items
            .iter()
            .filter(|i| i.kind == CompletionItemKind::Keyword)
            .collect();

        if let (Some(last_ident), Some(first_kw)) = (ident_items.last(), kw_items.first()) {
            let last_ident_pos = items
                .iter()
                .position(|i| i.label == last_ident.label)
                .unwrap();
            let first_kw_pos = items
                .iter()
                .position(|i| i.label == first_kw.label)
                .unwrap();
            assert!(
                last_ident_pos < first_kw_pos,
                "All identifiers should appear before all keywords in the sorted list"
            );
        }
    }

    // =========================================================================
    // Tests for isNewIdentifierLocation
    // =========================================================================

    fn make_completions_provider(
        source: &str,
    ) -> (
        crate::parser::NodeIndex,
        crate::parser::node::NodeArena,
        BinderState,
        LineMap,
        String,
    ) {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.into_arena();
        let mut binder = BinderState::new();
        binder.bind_source_file(&arena, root);
        let line_map = LineMap::build(source);
        (root, arena, binder, line_map, source.to_string())
    }

    #[test]
    fn test_is_new_identifier_location_after_const() {
        // TypeScript returns false for `const |` - it's a declaration keyword but
        // the default in TS is false unless specific AST conditions are met
        let source = "const ";
        let (root, arena, binder, line_map, src) = make_completions_provider(source);
        let completions = Completions::new(&arena, &binder, &line_map, &src);
        let offset = source.len() as u32;
        assert!(
            !completions.compute_is_new_identifier_location(root, offset),
            "Should NOT be new identifier location after 'const ' (TypeScript default is false)"
        );
    }

    #[test]
    fn test_is_new_identifier_location_after_import() {
        let source = "import ";
        let (root, arena, binder, line_map, src) = make_completions_provider(source);
        let completions = Completions::new(&arena, &binder, &line_map, &src);
        let offset = source.len() as u32;
        assert!(
            completions.compute_is_new_identifier_location(root, offset),
            "Should be new identifier location after 'import '"
        );
    }

    #[test]
    fn test_is_new_identifier_location_after_namespace() {
        let source = "namespace ";
        let (root, arena, binder, line_map, src) = make_completions_provider(source);
        let completions = Completions::new(&arena, &binder, &line_map, &src);
        let offset = source.len() as u32;
        assert!(
            completions.compute_is_new_identifier_location(root, offset),
            "Should be new identifier location after 'namespace '"
        );
    }

    #[test]
    fn test_is_new_identifier_location_after_module() {
        let source = "module ";
        let (root, arena, binder, line_map, src) = make_completions_provider(source);
        let completions = Completions::new(&arena, &binder, &line_map, &src);
        let offset = source.len() as u32;
        assert!(
            completions.compute_is_new_identifier_location(root, offset),
            "Should be new identifier location after 'module '"
        );
    }

    #[test]
    fn test_is_new_identifier_location_after_as() {
        // `x as <type>` is a type assertion - selecting existing type, not new identifier
        let source = "var y = x as ";
        let (root, arena, binder, line_map, src) = make_completions_provider(source);
        let completions = Completions::new(&arena, &binder, &line_map, &src);
        let offset = source.len() as u32;
        assert!(
            !completions.compute_is_new_identifier_location(root, offset),
            "Should NOT be new identifier location after 'as' in type assertion"
        );
    }

    #[test]
    fn test_is_new_identifier_location_not_after_return() {
        // TypeScript returns false for `return |` - it falls through to the default
        let source = "function f() { return ";
        let (root, arena, binder, line_map, src) = make_completions_provider(source);
        let completions = Completions::new(&arena, &binder, &line_map, &src);
        let offset = source.len() as u32;
        assert!(
            !completions.compute_is_new_identifier_location(root, offset),
            "Should NOT be new identifier location after 'return '"
        );
    }

    #[test]
    fn test_is_new_identifier_location_not_in_normal_expression() {
        let source = "const x = 1;\n";
        let (root, arena, binder, line_map, src) = make_completions_provider(source);
        let completions = Completions::new(&arena, &binder, &line_map, &src);
        let offset = source.len() as u32;
        assert!(
            !completions.compute_is_new_identifier_location(root, offset),
            "Should NOT be new identifier location at end of file"
        );
    }

    #[test]
    fn test_completion_result_struct_member_completion() {
        // Member completions should have is_member_completion = true and is_new_identifier_location = false
        let source = "const obj = { foo: 1 };
obj.";
        let (root, arena, binder, line_map, src) = make_completions_provider(source);
        let interner = TypeInterner::new();
        let completions = Completions::new_with_types(
            &arena,
            &binder,
            &line_map,
            &interner,
            &src,
            "test.ts".to_string(),
        );
        let position = Position::new(1, 4);
        let result = completions.get_completion_result(root, position);
        assert!(result.is_some(), "Should have completion result");
        let result = result.unwrap();
        assert!(result.is_member_completion, "Should be member completion");
        assert!(
            !result.is_global_completion,
            "Should not be global completion"
        );
        assert!(
            !result.is_new_identifier_location,
            "Member completion should not be new identifier location"
        );
    }

    #[test]
    fn test_completion_result_struct_global_completion() {
        let source = "const x = 1;
";
        let (root, arena, binder, line_map, src) = make_completions_provider(source);
        let completions = Completions::new(&arena, &binder, &line_map, &src);
        let position = Position::new(1, 0);
        let result = completions.get_completion_result(root, position);
        assert!(result.is_some(), "Should have completion result");
        let result = result.unwrap();
        assert!(result.is_global_completion, "Should be global completion");
        assert!(
            !result.is_member_completion,
            "Should not be member completion"
        );
        assert!(!result.entries.is_empty(), "Should have entries");
    }
}
