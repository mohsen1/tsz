//! Completions implementation for LSP.
//!
//! Given a position in the source, provides completion suggestions for
//! identifiers that are visible at that position.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::jsdoc::jsdoc_for_node;
use crate::provider_macro::FullProviderOptions;
use crate::resolver::{ScopeCache, ScopeCacheStats, ScopeWalker};
use crate::utils::find_node_at_offset;
use tsz_binder::BinderState;
use tsz_checker::TypeCache;
use tsz_checker::state::CheckerState;
use tsz_common::position::{LineMap, Position};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{
    ApparentMemberKind, IntrinsicKind, TypeId, TypeInterner, Visibility,
    apparent_primitive_members, visitor,
};

/// The kind of completion item, matching tsserver's `ScriptElementKind` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CompletionItemKind {
    /// A variable declared with `var`
    Variable,
    /// A constant declared with `const`
    Const,
    /// A variable declared with `let`
    Let,
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
    /// An imported alias (import binding)
    Alias,
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
    /// Legacy alias for `GLOBALS_OR_KEYWORDS`.
    pub const KEYWORD: &str = "15";

    /// Produce a deprecated sort text by prefixing "z" to the base sort text.
    /// This matches TypeScript's `SortText.Deprecated()` transformation.
    pub fn deprecated(base: &str) -> String {
        format!("z{base}")
    }

    /// Produce an object literal property sort text by appending the property
    /// name as a null-byte-separated tiebreaker.
    /// This matches TypeScript's `SortText.ObjectLiteralProperty()`.
    pub fn object_literal_property(base: &str, name: &str) -> String {
        format!("{base}\0{name}\0")
    }

    /// Produce a sort text that sorts below the given base sort text.
    /// This matches TypeScript's `SortText.SortBelow()`.
    pub fn sort_below(base: &str) -> String {
        format!("{base}1")
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
    /// tsserver-style default commit characters for this completion session.
    /// Omitted in new-identifier locations.
    #[serde(
        rename = "defaultCommitCharacters",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_commit_characters: Option<Vec<String>>,
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
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_snippet: bool,
    /// Whether selecting this completion triggers an additional action such as
    /// an auto-import (tsserver: `hasAction`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub has_action: bool,
    /// Module specifier for auto-import completions (tsserver: `source`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Display label for the source module (tsserver: `sourceDisplay`).
    #[serde(rename = "sourceDisplay", skip_serializing_if = "Option::is_none")]
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
    /// Additional text edits to apply when accepting this completion (LSP:
    /// `additionalTextEdits`). Used for auto-import to insert the import statement.
    #[serde(
        rename = "additionalTextEdits",
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_additional_edits::deserialize"
    )]
    pub additional_text_edits: Option<Vec<crate::rename::TextEdit>>,
    /// Opaque data preserved between completion and resolve requests.
    /// Contains the file name and label so the server can look up documentation on demand.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<CompletionItemData>,
    /// Whether this auto-import source corresponds to a package listed in
    /// the project's package.json dependencies (tsserver:
    /// `isPackageJsonImport`). Serialized as `true` when set; omitted otherwise.
    #[serde(
        rename = "isPackageJsonImport",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_package_json_import: Option<bool>,
}

/// Data attached to a completion item for lazy resolve.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CompletionItemData {
    /// The file where this completion was requested.
    pub file_name: String,
    /// The original position of the completion request.
    pub position: tsz_common::position::Position,
}

/// Custom deserializer that always returns None for `additional_text_edits`.
/// Since `CompletionItem` is only sent from server to client, we never
/// deserialize this field from the client.
mod deserialize_additional_edits {
    use crate::rename::TextEdit;
    use serde::Deserializer;

    pub fn deserialize<'de, D>(_deserializer: D) -> Result<Option<Vec<TextEdit>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Skip deserialization and always return None
        Ok(None)
    }
}

impl CompletionItem {
    /// Create a new completion item with only the required fields.
    pub const fn new(label: String, kind: CompletionItemKind) -> Self {
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
            additional_text_edits: None,
            data: None,
            is_package_json_import: None,
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
    pub const fn as_snippet(mut self) -> Self {
        self.is_snippet = true;
        self
    }

    /// Mark this completion as requiring an additional action (e.g. auto-import).
    pub const fn with_has_action(mut self) -> Self {
        self.has_action = true;
        self
    }

    /// Mark this completion as corresponding to a package listed in the
    /// project's `package.json` dependencies.
    pub const fn with_is_package_json_import(mut self) -> Self {
        self.is_package_json_import = Some(true);
        self
    }

    /// Set the module source path for auto-import completions.
    pub fn with_source(mut self, source: String) -> Self {
        self.source = Some(source);
        self
    }

    /// Set additional text edits to apply when accepting this completion.
    /// Used for auto-import to insert the import statement.
    pub fn with_additional_edits(mut self, edits: Vec<crate::rename::TextEdit>) -> Self {
        self.additional_text_edits = Some(edits);
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
    pub const fn with_replacement_span(mut self, start: u32, end: u32) -> Self {
        self.replacement_span = Some((start, end));
        self
    }

    /// Return the effective sort text: the explicitly set value, or a default
    /// derived from the completion kind.
    pub const fn effective_sort_text(&self) -> &str {
        if let Some(ref s) = self.sort_text {
            s.as_str()
        } else {
            default_sort_text(self.kind)
        }
    }
}

/// Derive a default sort text from the completion kind, following tsserver
/// conventions.
pub const fn default_sort_text(kind: CompletionItemKind) -> &'static str {
    match kind {
        // Scope-level declarations: variables, functions, parameters
        // TypeScript uses LocationPriority ("11") for most items in scope.
        // LocalDeclarationPriority ("10") is only for immediate block-scope locals.
        CompletionItemKind::Variable
        | CompletionItemKind::Const
        | CompletionItemKind::Let
        | CompletionItemKind::Function
        | CompletionItemKind::Parameter
        | CompletionItemKind::Constructor
        | CompletionItemKind::Alias => sort_priority::LOCATION_PRIORITY,
        // Member completions: properties and methods
        CompletionItemKind::Property | CompletionItemKind::Method => sort_priority::MEMBER,
        // Type declarations: classes, interfaces, enums, type aliases, modules, type params
        CompletionItemKind::Class
        | CompletionItemKind::Interface
        | CompletionItemKind::Enum
        | CompletionItemKind::TypeAlias
        | CompletionItemKind::Module
        | CompletionItemKind::TypeParameter => sort_priority::TYPE_DECLARATION,
        // Keywords and globals
        CompletionItemKind::Keyword => sort_priority::GLOBALS_OR_KEYWORDS,
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
    pub(super) arena: &'a NodeArena,
    pub(super) binder: &'a BinderState,
    pub(super) line_map: &'a LineMap,
    pub(super) source_text: &'a str,
    pub(super) interner: Option<&'a TypeInterner>,
    pub(super) file_name: Option<String>,
    pub(super) strict: bool,
    pub(super) sound_mode: bool,
    pub(super) lib_contexts: &'a [tsz_checker::context::LibContext],
}

mod context;
mod core;
mod filters;
pub mod import_paths;
mod member;
pub mod postfix;
mod render;
mod string_literals;
mod symbols;
use member::PropertyCompletion;
use render::compare_case_sensitive_ui;
use symbols::{DEPRECATED_GLOBALS, GLOBAL_VARS, KEYWORDS, KEYWORDS_INSIDE_FUNCTION};

#[cfg(test)]
#[path = "../../tests/completions_tests.rs"]
mod completions_tests;
