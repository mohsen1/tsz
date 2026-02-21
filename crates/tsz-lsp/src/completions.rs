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
    ApparentMemberKind, IntrinsicKind, TypeId, TypeInterner, apparent_primitive_members, visitor,
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
mod render;
mod symbols;
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
        let mut seen_names = FxHashSet::default();

        // Walk scopes from innermost to outermost
        for scope in scope_chain.iter().rev() {
            for (name, symbol_id) in scope.iter() {
                if seen_names.contains(name) {
                    continue;
                }
                seen_names.insert(name.clone());

                if let Some(symbol) = self.binder.symbols.get(*symbol_id) {
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

        // 8. Add global variables (globalThis, undefined, Array, etc.)
        //    These are always available and match tsserver's globalsVars.
        let inside_func = self.is_inside_function(offset);
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
                    // globalThis and undefined don't get "declare" modifier
                    if name != "globalThis" && name != "undefined" {
                        item.kind_modifiers = Some("declare".to_string());
                    }
                }
                if kind == CompletionItemKind::Function {
                    item.insert_text = Some(format!("{name}($1)"));
                    item.is_snippet = true;
                }
                completions.push(item);
            }
        }

        // 9. If inside a function, also add "arguments" as a local variable
        if inside_func && !seen_names.contains("arguments") {
            seen_names.insert("arguments".to_string());
            let mut item =
                CompletionItem::new("arguments".to_string(), CompletionItemKind::Variable);
            item.sort_text = Some(sort_priority::LOCAL_DECLARATION.to_string());
            completions.push(item);
        }

        // 10. Add keywords for non-member completions
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

    fn get_member_completions(
        &self,
        expr_idx: NodeIndex,
        type_cache: Option<&mut Option<TypeCache>>,
    ) -> Option<Vec<CompletionItem>> {
        let interner = self.interner?;
        let file_name = self.file_name.as_ref()?;

        let mut cache_ref = type_cache;
        let compiler_options = tsz_checker::context::CheckerOptions {
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
                    compiler_options,
                )
            } else {
                CheckerState::new(
                    self.arena,
                    self.binder,
                    interner,
                    file_name.clone(),
                    compiler_options,
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

        let mut items = Vec::new();
        let mut seen_names = FxHashSet::default();

        // Type-qualified member access (`A.B`) should prefer namespace/module exports
        // instead of instance/member shape properties.
        let qualified_name_target = self.is_qualified_name_member_target(expr_idx);
        if !qualified_name_target {
            let type_id = checker.get_type_of_node(expr_idx);
            let mut visited = FxHashSet::default();
            let mut props: FxHashMap<String, PropertyCompletion> = FxHashMap::default();
            self.collect_properties_for_type(
                type_id,
                interner,
                &mut checker,
                &mut visited,
                &mut props,
            );

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
                    item.insert_text = Some(format!("{name}($1)"));
                    item.is_snippet = true;
                }

                seen_names.insert(name);
                items.push(item);
            }
        }

        if items.is_empty()
            && let Some(sym_id) = self.resolve_member_target_symbol(expr_idx)
            && let Some(type_annotation) = self.variable_type_annotation_node(sym_id)
        {
            let mut visited = FxHashSet::default();
            let mut props: FxHashMap<String, PropertyCompletion> = FxHashMap::default();
            let declared_type = checker.get_type_of_node(type_annotation);
            self.collect_properties_for_type(
                declared_type,
                interner,
                &mut checker,
                &mut visited,
                &mut props,
            );
            if props.is_empty()
                && let Some(type_annotation_node) = self.arena.get(type_annotation)
                && type_annotation_node.kind == syntax_kind_ext::TYPE_REFERENCE
                && let Some(type_ref) = self.arena.get_type_ref(type_annotation_node)
                && let Some(type_symbol_id) = self.resolve_member_target_symbol(type_ref.type_name)
            {
                let annotation_symbol_type = checker.get_type_of_symbol(type_symbol_id);
                self.collect_properties_for_type(
                    annotation_symbol_type,
                    interner,
                    &mut checker,
                    &mut visited,
                    &mut props,
                );
            }
            for (name, info) in props {
                let kind = if info.is_method {
                    CompletionItemKind::Method
                } else {
                    CompletionItemKind::Property
                };
                let mut item = CompletionItem::new(name.clone(), kind);
                item = item.with_detail(checker.format_type(info.type_id));
                item.sort_text = Some(sort_priority::MEMBER.to_string());
                if info.is_method {
                    item.insert_text = Some(format!("{name}($1)"));
                    item.is_snippet = true;
                }
                seen_names.insert(name);
                items.push(item);
            }
        }

        if let Some(target_symbol_id) = self.resolve_member_target_symbol(expr_idx) {
            self.append_namespace_export_member_completions(
                target_symbol_id,
                &mut checker,
                !qualified_name_target,
                &mut seen_names,
                &mut items,
            );
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
        _checker: &mut CheckerState,
        visited: &mut FxHashSet<TypeId>,
        props: &mut FxHashMap<String, PropertyCompletion>,
    ) {
        if !visited.insert(type_id) {
            return;
        }

        if let Some(shape_id) = visitor::object_shape_id(interner, type_id)
            .or_else(|| visitor::object_with_index_shape_id(interner, type_id))
        {
            let shape = interner.object_shape(shape_id);
            for prop in &shape.properties {
                let name = interner.resolve_atom(prop.name);
                self.add_property_completion(props, interner, name, prop.type_id, prop.is_method);
            }
            return;
        }

        if let Some(members) = visitor::union_list_id(interner, type_id)
            .or_else(|| visitor::intersection_list_id(interner, type_id))
        {
            let members = interner.type_list(members);
            for &member in members.iter() {
                self.collect_properties_for_type(member, interner, _checker, visited, props);
            }
            return;
        }

        if let Some(app) = visitor::application_id(interner, type_id) {
            let app = interner.type_application(app);
            self.collect_properties_for_type(app.base, interner, _checker, visited, props);
            return;
        }

        if let Some(literal) = visitor::literal_value(interner, type_id) {
            if let Some(kind) = self.literal_intrinsic_kind(&literal) {
                self.collect_intrinsic_members(kind, interner, props);
            }
            return;
        }

        if visitor::template_literal_id(interner, type_id).is_some() {
            self.collect_intrinsic_members(IntrinsicKind::String, interner, props);
            return;
        }

        if let Some(kind) = visitor::intrinsic_kind(interner, type_id) {
            self.collect_intrinsic_members(kind, interner, props);
        }
    }

    fn variable_type_annotation_node(&self, sym_id: tsz_binder::SymbolId) -> Option<NodeIndex> {
        let symbol = self.binder.symbols.get(sym_id)?;
        let decl = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let node = self.arena.get(decl)?;
        if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let var_decl = self.arena.get_variable_declaration(node)?;
        var_decl
            .type_annotation
            .is_some()
            .then_some(var_decl.type_annotation)
    }

    fn is_qualified_name_member_target(&self, expr_idx: NodeIndex) -> bool {
        let Some(ext) = self.arena.get_extended(expr_idx) else {
            return false;
        };
        let Some(parent) = self.arena.get(ext.parent) else {
            return false;
        };
        if parent.kind != syntax_kind_ext::QUALIFIED_NAME {
            return false;
        }
        self.arena
            .get_qualified_name(parent)
            .is_some_and(|qualified| qualified.left == expr_idx)
    }

    fn resolve_member_target_symbol(&self, expr_idx: NodeIndex) -> Option<tsz_binder::SymbolId> {
        if let Some(sym_id) = self.binder.node_symbols.get(&expr_idx.0).copied() {
            return Some(sym_id);
        }

        let node = self.arena.get(expr_idx)?;
        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                self.binder.resolve_identifier(self.arena, expr_idx)
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr(node)?;
                let left = self.resolve_member_target_symbol(access.expression)?;
                let name = self.arena.get_identifier_text(access.name_or_argument)?;
                self.resolve_exported_member_symbol(left, name)
            }
            k if k == syntax_kind_ext::QUALIFIED_NAME => {
                let qualified = self.arena.get_qualified_name(node)?;
                let left = self.resolve_member_target_symbol(qualified.left)?;
                let name = self.arena.get_identifier_text(qualified.right)?;
                self.resolve_exported_member_symbol(left, name)
            }
            _ => self.binder.resolve_identifier(self.arena, expr_idx),
        }
    }

    fn resolve_exported_member_symbol(
        &self,
        container: tsz_binder::SymbolId,
        member_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        let container_symbol = self.binder.symbols.get(container)?;
        if let Some(exports) = container_symbol.exports.as_ref()
            && let Some(member) = exports.get(member_name)
        {
            return Some(member);
        }
        if let Some(members) = container_symbol.members.as_ref()
            && let Some(member) = members.get(member_name)
        {
            return Some(member);
        }
        None
    }

    fn append_namespace_export_member_completions(
        &self,
        symbol_id: tsz_binder::SymbolId,
        checker: &mut CheckerState,
        allow_class_prototype: bool,
        seen_names: &mut FxHashSet<String>,
        items: &mut Vec<CompletionItem>,
    ) {
        use tsz_binder::symbol_flags;

        let Some(symbol) = self.binder.symbols.get(symbol_id) else {
            return;
        };

        let symbol_name = symbol.escaped_name.clone();
        let is_class = (symbol.flags & symbol_flags::CLASS) != 0;

        let export_entries: Vec<(String, tsz_binder::SymbolId)> = symbol
            .exports
            .as_ref()
            .map(|exports| {
                exports
                    .iter()
                    .map(|(name, id)| (name.clone(), *id))
                    .collect()
            })
            .unwrap_or_default();

        for (name, export_id) in export_entries {
            if seen_names.contains(&name) {
                continue;
            }
            let Some(export_symbol) = self.binder.symbols.get(export_id) else {
                continue;
            };

            let kind = self.determine_completion_kind(export_symbol);
            let mut item = CompletionItem::new(name.clone(), kind);
            item.sort_text = Some(sort_priority::MEMBER.to_string());

            let export_type = checker.get_type_of_symbol(export_id);
            let detail = checker.format_type(export_type);
            if !detail.is_empty() {
                item = item.with_detail(detail);
            } else if let Some(detail) = self.get_symbol_detail(export_symbol) {
                item = item.with_detail(detail);
            }

            if let Some(modifiers) = self.build_kind_modifiers(export_symbol) {
                item.kind_modifiers = Some(modifiers);
            }

            if kind == CompletionItemKind::Function || kind == CompletionItemKind::Method {
                item.insert_text = Some(format!("{name}($1)"));
                item.is_snippet = true;
            }

            seen_names.insert(name);
            items.push(item);
        }

        if allow_class_prototype && is_class && !seen_names.contains("prototype") {
            let mut item =
                CompletionItem::new("prototype".to_string(), CompletionItemKind::Property);
            item.sort_text = Some(sort_priority::MEMBER.to_string());
            item = item.with_detail(symbol_name);
            seen_names.insert("prototype".to_string());
            items.push(item);
        }
    }

    pub fn get_member_completion_parent_type_name(
        &self,
        root: NodeIndex,
        position: Position,
    ) -> Option<String> {
        let interner = self.interner?;
        let file_name = self.file_name.as_ref()?;
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;
        let node_idx = self.find_completions_node(root, offset);
        let expr_idx = self.member_completion_target(node_idx, offset)?;

        let compiler_options = tsz_checker::context::CheckerOptions {
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
        let mut checker = CheckerState::new(
            self.arena,
            self.binder,
            interner,
            file_name.clone(),
            compiler_options,
        );
        let type_id = checker.get_type_of_node(expr_idx);
        let type_text = checker.format_type(type_id);
        if let Some(parent) = Self::normalize_member_parent_type_name(&type_text) {
            return Some(parent);
        }
        self.resolve_member_target_symbol(expr_idx)
            .and_then(|sym_id| self.binder.symbols.get(sym_id))
            .and_then(|symbol| {
                use tsz_binder::symbol_flags;
                ((symbol.flags & (symbol_flags::CLASS | symbol_flags::FUNCTION)) != 0)
                    .then(|| symbol.escaped_name.clone())
            })
    }

    fn normalize_member_parent_type_name(type_text: &str) -> Option<String> {
        let mut normalized = type_text.trim();
        if let Some(stripped) = normalized.strip_prefix("typeof ") {
            normalized = stripped.trim();
        }
        if normalized.is_empty() {
            return None;
        }
        let mut chars = normalized.chars();
        let first = chars.next()?;
        if !(first == '_' || first == '$' || first.is_ascii_alphabetic()) {
            return None;
        }
        if chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()) {
            Some(normalized.to_string())
        } else {
            None
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
                ApparentMemberKind::Value(type_id) | ApparentMemberKind::Method(type_id) => type_id,
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

    const fn literal_intrinsic_kind(
        &self,
        literal: &tsz_solver::LiteralValue,
    ) -> Option<IntrinsicKind> {
        match literal {
            tsz_solver::LiteralValue::String(_) => Some(IntrinsicKind::String),
            tsz_solver::LiteralValue::Number(_) => Some(IntrinsicKind::Number),
            tsz_solver::LiteralValue::Boolean(_) => Some(IntrinsicKind::Boolean),
            tsz_solver::LiteralValue::BigInt(_) => Some(IntrinsicKind::Bigint),
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
        let compiler_options = tsz_checker::context::CheckerOptions {
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
                    compiler_options,
                )
            } else {
                CheckerState::new(
                    self.arena,
                    self.binder,
                    interner,
                    file_name.clone(),
                    compiler_options,
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
                    item.insert_text = Some(format!("{name}($1)"));
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
                    .map(std::string::ToString::to_string)
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_shorthand_property(node)?;
                self.arena
                    .get_identifier_text(prop.name)
                    .map(std::string::ToString::to_string)
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.arena.get_method_decl(node)?;
                self.arena
                    .get_identifier_text(method.name)
                    .map(std::string::ToString::to_string)
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
                if decl.initializer == node_idx && decl.type_annotation.is_some() {
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
                    && func.type_annotation.is_some()
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
        while current.is_some() {
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
        if let Some(callable_id) = visitor::callable_shape_id(interner, func_type) {
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
#[path = "../tests/completions_tests.rs"]
mod completions_tests;
