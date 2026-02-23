//! Completions implementation for LSP.
//!
//! Given a position in the source, provides completion suggestions for
//! identifiers that are visible at that position.

use rustc_hash::{FxHashMap, FxHashSet};
use std::borrow::Cow;

use crate::jsdoc::jsdoc_for_node;
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
    /// Legacy alias for `GLOBALS_OR_KEYWORDS`.
    pub const KEYWORD: &str = "15";

    /// Produce a deprecated sort text by prefixing "z" to the base sort text.
    /// This matches TypeScript's `SortText.Deprecated()` transformation.
    pub fn deprecated(base: &str) -> String {
        format!("z{base}")
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
        | CompletionItemKind::Function
        | CompletionItemKind::Parameter
        | CompletionItemKind::Constructor => sort_priority::LOCATION_PRIORITY,
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
    arena: &'a NodeArena,
    binder: &'a BinderState,
    line_map: &'a LineMap,
    source_text: &'a str,
    interner: Option<&'a TypeInterner>,
    file_name: Option<String>,
    strict: bool,
}

mod context;
mod filters;
mod member;
mod render;
mod string_literals;
mod symbols;
use member::PropertyCompletion;
use render::compare_case_sensitive_ui;
use symbols::{DEPRECATED_GLOBALS, GLOBAL_VARS, KEYWORDS, KEYWORDS_INSIDE_FUNCTION};

impl<'a> Completions<'a> {
    /// Create a new Completions provider.
    pub const fn new(
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
    pub const fn new_with_types(
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
    pub const fn with_strict(
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

    /// Collect inherited class members as completion candidates for class member snippets.
    pub fn get_class_member_snippet_candidates(
        &self,
        root: NodeIndex,
        position: Position,
    ) -> Vec<CompletionItem> {
        let Some(offset) = self.line_map.position_to_offset(position, self.source_text) else {
            return Vec::new();
        };
        let node_idx = self.find_completions_node(root, offset);
        let Some(class_idx) = self.find_enclosing_class_declaration(node_idx) else {
            return Vec::new();
        };
        let Some(base_expr) = self.class_extends_expression(class_idx) else {
            return Vec::new();
        };
        let mut candidates = self
            .get_member_completions(base_expr, None)
            .unwrap_or_default();
        if candidates.is_empty() {
            return candidates;
        }

        let declared_members = self.class_declared_member_names(class_idx);
        candidates.retain(|item| {
            (item.kind == CompletionItemKind::Method || item.kind == CompletionItemKind::Property)
                && !declared_members.contains(&item.label)
        });

        for item in &mut candidates {
            item.sort_text = Some(sort_priority::SUGGESTED_CLASS_MEMBERS.to_string());
        }

        candidates.sort_by(|a, b| a.label.cmp(&b.label));
        candidates
    }

    /// Check if the cursor is after a dot (member completion context).
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

        // 2. Find the node at this offset using improved lookup
        let node_idx = self.find_completions_node(root, offset);

        // 3. Contextual string-literal completions for call arguments.
        // This path intentionally runs before no-completion suppression, because
        // ordinary string literals are suppressed by default.
        if self.interner.is_some()
            && self.file_name.is_some()
            && let Some(items) =
                self.get_string_literal_completions(node_idx, offset, type_cache.as_deref_mut())
        {
            return if items.is_empty() { None } else { Some(items) };
        }

        // 4. Filter out positions where completions should not appear
        if self.is_in_no_completion_context(offset) {
            return Some(Vec::new());
        }

        // 5. Check for member completion (after a dot)
        if let Some(expr_idx) = self
            .member_completion_target(node_idx, offset)
            .or_else(|| self.marker_comment_member_completion_target(offset))
            && let Some(items) = self.get_member_completions(expr_idx, type_cache.as_deref_mut())
        {
            return if items.is_empty() { None } else { Some(items) };
        }

        // 6. Check for object literal property completion (contextual completions)
        if self.interner.is_some()
            && self.file_name.is_some()
            && let Some(items) = self.get_object_literal_completions(node_idx, type_cache)
        {
            return if items.is_empty() { None } else { Some(items) };
        }

        // 7. Get the scope chain at this position
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let scope_chain = if let Some(scope_cache) = scope_cache {
            Cow::Borrowed(walker.get_scope_chain_cached(root, node_idx, scope_cache, scope_stats))
        } else {
            Cow::Owned(walker.get_scope_chain(root, node_idx))
        };

        // 8. Collect all visible identifiers from the scope chain
        let mut completions = Vec::new();
        let mut seen_names = FxHashSet::default();

        // Walk scopes from innermost to outermost
        for scope in scope_chain.iter().rev() {
            for (name, symbol_id) in scope.iter() {
                if seen_names.contains(name) {
                    continue;
                }

                if let Some(symbol) = self.binder.symbols.get(*symbol_id) {
                    // Synthetic CommonJS helpers should not appear in globals-style completion lists.
                    // Keep user-declared symbols with these names by requiring no declarations.
                    if matches!(
                        name.as_str(),
                        "exports" | "require" | "module" | "__dirname" | "__filename"
                    ) && symbol.declarations.is_empty()
                        && symbol.value_declaration.is_none()
                    {
                        continue;
                    }

                    seen_names.insert(name.clone());
                    let mut kind = self.determine_completion_kind(symbol);
                    if kind == CompletionItemKind::Variable && self.symbol_is_parameter(symbol) {
                        kind = CompletionItemKind::Parameter;
                    }
                    let mut item = CompletionItem::new(name.clone(), kind);
                    item.sort_text = Some(default_sort_text(kind).to_string());

                    if kind == CompletionItemKind::Parameter
                        && let Some(param_type) = self.parameter_annotation_text(symbol)
                    {
                        item = item.with_detail(param_type);
                    } else if let Some(detail) = self.get_symbol_detail(symbol) {
                        item = item.with_detail(detail);
                    }
                    if let Some(modifiers) = self.build_kind_modifiers(symbol) {
                        item.kind_modifiers = Some(modifiers);
                    }
                    if kind == CompletionItemKind::Function || kind == CompletionItemKind::Method {
                        item.insert_text = Some(format!("{name}($1)"));
                        item.is_snippet = true;
                    }

                    let decl_node = if symbol.value_declaration.is_some() {
                        symbol.value_declaration
                    } else {
                        symbol
                            .declarations
                            .first()
                            .copied()
                            .unwrap_or(NodeIndex::NONE)
                    };
                    if decl_node.is_some() {
                        let doc = jsdoc_for_node(self.arena, root, decl_node, self.source_text);
                        if !doc.is_empty() {
                            item = item.with_documentation(doc);
                        }
                    }

                    completions.push(item);
                }
            }
        }

        // 9. Add global variables (globalThis, Array, etc.)
        //    These are always available and match fourslash globalsVars order.
        let inside_func = self.is_inside_function(offset);
        if !seen_names.contains("globalThis") {
            seen_names.insert("globalThis".to_string());
            let mut item =
                CompletionItem::new("globalThis".to_string(), CompletionItemKind::Module);
            item.sort_text = Some(sort_priority::GLOBALS_OR_KEYWORDS.to_string());
            completions.push(item);
        }

        for &(name, kind) in GLOBAL_VARS {
            if !seen_names.contains(name) {
                seen_names.insert(name.to_string());
                let mut item = CompletionItem::new(name.to_string(), kind);
                let is_deprecated = DEPRECATED_GLOBALS.contains(&name);
                if is_deprecated {
                    item.sort_text = Some(sort_priority::deprecated(
                        sort_priority::GLOBALS_OR_KEYWORDS,
                    ));
                    item.kind_modifiers = Some("deprecated,declare".to_string());
                } else {
                    item.sort_text = Some(sort_priority::GLOBALS_OR_KEYWORDS.to_string());
                    item.kind_modifiers = Some("declare".to_string());
                }
                if kind == CompletionItemKind::Function {
                    item.insert_text = Some(format!("{name}($1)"));
                    item.is_snippet = true;
                }
                completions.push(item);
            }
        }

        if !seen_names.contains("undefined") {
            seen_names.insert("undefined".to_string());
            let mut item =
                CompletionItem::new("undefined".to_string(), CompletionItemKind::Variable);
            item.sort_text = Some(sort_priority::GLOBALS_OR_KEYWORDS.to_string());
            completions.push(item);
        }

        // 10. If inside a function, also add "arguments" as a local variable
        if inside_func && !seen_names.contains("arguments") {
            seen_names.insert("arguments".to_string());
            let mut item =
                CompletionItem::new("arguments".to_string(), CompletionItemKind::Variable);
            item.sort_text = Some(sort_priority::LOCAL_DECLARATION.to_string());
            completions.push(item);
        }

        // 11. Add keywords for non-member completions
        let keywords = if inside_func {
            KEYWORDS_INSIDE_FUNCTION
        } else {
            KEYWORDS
        };
        for kw in keywords.iter().copied() {
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
    const fn determine_completion_kind(&self, symbol: &tsz_binder::Symbol) -> CompletionItemKind {
        use tsz_binder::symbol_flags;

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

    fn symbol_is_parameter(&self, symbol: &tsz_binder::Symbol) -> bool {
        let decl = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else if let Some(&first) = symbol.declarations.first() {
            first
        } else {
            return false;
        };
        self.arena
            .get(decl)
            .is_some_and(|node| node.kind == syntax_kind_ext::PARAMETER)
    }

    fn parameter_annotation_text(&self, symbol: &tsz_binder::Symbol) -> Option<String> {
        let decl = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let node = self.arena.get(decl)?;
        if node.kind != syntax_kind_ext::PARAMETER {
            return None;
        }
        let param = self.arena.get_parameter(node)?;
        if !param.type_annotation.is_some() {
            return None;
        }
        let type_node = self.arena.get(param.type_annotation)?;
        let start = type_node.pos as usize;
        let end = type_node.end.min(self.source_text.len() as u32) as usize;
        (start < end).then(|| {
            let mut text = self.source_text[start..end].trim().to_string();
            while text.ends_with(',') || text.ends_with(';') {
                text.pop();
                text = text.trim_end().to_string();
            }
            while text.ends_with(')') {
                let opens = text.chars().filter(|&c| c == '(').count();
                let closes = text.chars().filter(|&c| c == ')').count();
                if closes > opens {
                    text.pop();
                    text = text.trim_end().to_string();
                } else {
                    break;
                }
            }
            text
        })
    }

    /// Get detail information for a symbol (e.g., "const", "function", "class").
    fn member_completion_target(&self, node_idx: NodeIndex, offset: u32) -> Option<NodeIndex> {
        let mut current = node_idx;

        while current.is_some() {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let access = self.arena.get_access_expr(node)?;
                let expr_node = self.arena.get(access.expression)?;
                if offset >= expr_node.end && offset <= node.end {
                    return Some(access.expression);
                }
            }
            if node.kind == syntax_kind_ext::QUALIFIED_NAME {
                let qualified = self.arena.get_qualified_name(node)?;
                let left_node = self.arena.get(qualified.left)?;
                if offset >= left_node.end && offset <= node.end {
                    return Some(qualified.left);
                }
            }

            let ext = self.arena.get_extended(current)?;
            current = ext.parent;
        }

        None
    }

    fn marker_comment_member_completion_target(&self, offset: u32) -> Option<NodeIndex> {
        let bytes = self.source_text.as_bytes();
        let len = bytes.len() as u32;
        if len == 0 {
            return None;
        }
        let mut cursor = offset.min(len);

        loop {
            while cursor > 0 && bytes[(cursor - 1) as usize].is_ascii_whitespace() {
                cursor -= 1;
            }

            if cursor >= 2
                && bytes[(cursor - 2) as usize] == b'*'
                && bytes[(cursor - 1) as usize] == b'/'
            {
                cursor -= 2;
                while cursor >= 2 {
                    if bytes[(cursor - 2) as usize] == b'/' && bytes[(cursor - 1) as usize] == b'*'
                    {
                        cursor -= 2;
                        break;
                    }
                    cursor -= 1;
                }
                continue;
            }

            break;
        }

        if cursor == 0 || bytes[(cursor - 1) as usize] != b'.' {
            return None;
        }

        let dot = cursor - 1;
        let mut ident_end = dot;
        while ident_end > 0 && bytes[(ident_end - 1) as usize].is_ascii_whitespace() {
            ident_end -= 1;
        }
        let mut ident_start = ident_end;
        while ident_start > 0 {
            let ch = bytes[(ident_start - 1) as usize];
            if ch == b'_' || ch == b'$' || ch.is_ascii_alphanumeric() {
                ident_start -= 1;
            } else {
                break;
            }
        }
        if ident_start >= ident_end {
            return None;
        }

        let mut current = find_node_at_offset(self.arena, ident_end.saturating_sub(1));
        while current.is_some() {
            let node = self.arena.get(current)?;
            if node.kind == SyntaxKind::Identifier as u16
                && node.pos <= ident_start
                && node.end >= ident_end
            {
                return Some(current);
            }
            let ext = self.arena.get_extended(current)?;
            current = ext.parent;
        }

        None
    }
}

#[cfg(test)]
#[path = "../tests/completions_tests.rs"]
mod completions_tests;
