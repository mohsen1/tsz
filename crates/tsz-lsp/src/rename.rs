//! Rename implementation for LSP.
//!
//! Handles renaming symbols across the codebase, including validation,
//! prepare-rename info (tsserver-compatible), shorthand property expansion,
//! import specifier handling, and workspace edit generation.

use crate::references::FindReferences;
use crate::resolver::{ScopeCache, ScopeCacheStats, ScopeWalker};
use crate::utils::find_node_at_offset;
use rustc_hash::FxHashMap;
use tsz_binder::SymbolId;
use tsz_binder::symbol_flags;
use tsz_common::position::{Position, Range};
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::{NodeIndex, modifier_flags, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// A single text edit.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    /// The range to replace.
    pub range: Range,
    /// The new text.
    pub new_text: String,
}

impl TextEdit {
    /// Create a new text edit.
    pub const fn new(range: Range, new_text: String) -> Self {
        Self { range, new_text }
    }
}

/// A rich text edit used only for rename operations. Includes optional
/// `prefix_text` and `suffix_text` metadata matching tsserver's rename
/// response format. These fields tell the client that the replacement
/// involves a structural expansion (shorthand property, import alias, etc.).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameTextEdit {
    /// The range to replace.
    pub range: Range,
    /// The new text for the identifier.
    pub new_text: String,
    /// Optional prefix text (e.g. `"oldName: "` for shorthand property
    /// expansion `{ x }` -> `{ x: y }`). Matches tsserver's `prefixText`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix_text: Option<String>,
    /// Optional suffix text (e.g. `" as oldName"` for export specifier
    /// expansion `export { x }` -> `export { y as x }`).
    /// Matches tsserver's `suffixText`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix_text: Option<String>,
}

impl RenameTextEdit {
    /// Create a plain rename edit (no prefix/suffix).
    pub const fn new(range: Range, new_text: String) -> Self {
        Self {
            range,
            new_text,
            prefix_text: None,
            suffix_text: None,
        }
    }

    /// Create a rename edit with prefix text.
    pub const fn with_prefix(range: Range, new_text: String, prefix_text: String) -> Self {
        Self {
            range,
            new_text,
            prefix_text: Some(prefix_text),
            suffix_text: None,
        }
    }

    /// Create a rename edit with suffix text.
    pub const fn with_suffix(range: Range, new_text: String, suffix_text: String) -> Self {
        Self {
            range,
            new_text,
            prefix_text: None,
            suffix_text: Some(suffix_text),
        }
    }

    /// Convert to a plain `TextEdit` by folding prefix/suffix into `new_text`.
    pub fn to_text_edit(&self) -> TextEdit {
        let mut text = String::new();
        if let Some(ref prefix) = self.prefix_text {
            text.push_str(prefix);
        }
        text.push_str(&self.new_text);
        if let Some(ref suffix) = self.suffix_text {
            text.push_str(suffix);
        }
        TextEdit::new(self.range, text)
    }
}

/// A workspace edit (changes across multiple files).
#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkspaceEdit {
    /// Map of file path -> list of edits.
    pub changes: FxHashMap<String, Vec<TextEdit>>,
}

impl WorkspaceEdit {
    /// Create a new workspace edit.
    pub fn new() -> Self {
        Self {
            changes: FxHashMap::default(),
        }
    }

    /// Add an edit to the workspace edit.
    pub fn add_edit(&mut self, file_path: String, edit: TextEdit) {
        self.changes.entry(file_path).or_default().push(edit);
    }
}

impl Default for WorkspaceEdit {
    fn default() -> Self {
        Self::new()
    }
}

/// A rename-specific workspace edit that preserves prefix/suffix metadata.
/// Use `to_workspace_edit()` to convert to a standard `WorkspaceEdit`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RenameWorkspaceEdit {
    /// Map of file path -> list of rich rename edits.
    pub changes: FxHashMap<String, Vec<RenameTextEdit>>,
}

impl RenameWorkspaceEdit {
    pub fn new() -> Self {
        Self {
            changes: FxHashMap::default(),
        }
    }

    pub fn add_edit(&mut self, file_path: String, edit: RenameTextEdit) {
        self.changes.entry(file_path).or_default().push(edit);
    }

    /// Convert to a standard `WorkspaceEdit` by folding prefix/suffix into
    /// each edit's `new_text`.
    pub fn to_workspace_edit(&self) -> WorkspaceEdit {
        let mut ws = WorkspaceEdit::new();
        for (file, edits) in &self.changes {
            for edit in edits {
                ws.add_edit(file.clone(), edit.to_text_edit());
            }
        }
        ws
    }
}

impl Default for RenameWorkspaceEdit {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Prepare-rename result (tsserver-compatible)
// ---------------------------------------------------------------------------

/// The kind of a symbol for rename purposes (matches tsserver `ScriptElementKind`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum RenameSymbolKind {
    #[serde(rename = "let")]
    Let,
    #[serde(rename = "const")]
    Const,
    #[serde(rename = "var")]
    Var,
    #[serde(rename = "parameter")]
    Parameter,
    #[serde(rename = "function")]
    Function,
    #[serde(rename = "method")]
    Method,
    #[serde(rename = "property")]
    Property,
    #[serde(rename = "class")]
    Class,
    #[serde(rename = "interface")]
    Interface,
    #[serde(rename = "type")]
    TypeAlias,
    #[serde(rename = "enum")]
    Enum,
    #[serde(rename = "enum member")]
    EnumMember,
    #[serde(rename = "module")]
    Module,
    #[serde(rename = "alias")]
    Alias,
    #[serde(rename = "type parameter")]
    TypeParameter,
    #[serde(rename = "unknown")]
    Unknown,
}

/// Result of `prepare_rename`, providing tsserver-compatible information.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareRenameResult {
    /// Whether this element can be renamed.
    pub can_rename: bool,
    /// Short display name of the symbol (e.g. `"bar"`).
    pub display_name: String,
    /// Qualified display name (e.g. `"Foo.bar"` for a class member).
    pub full_display_name: String,
    /// Symbol kind (matches tsserver `ScriptElementKind`).
    pub kind: RenameSymbolKind,
    /// Comma-separated modifier keywords (e.g. `"export,declare"`).
    pub kind_modifiers: String,
    /// The range of the identifier that triggered the rename request.
    pub trigger_span: Range,
    /// If the rename is not possible, a localized error message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub localized_error_message: Option<String>,
}

impl PrepareRenameResult {
    /// Create a result for when renaming is not allowed.
    fn cannot_rename(msg: &str) -> Self {
        Self {
            can_rename: false,
            display_name: String::new(),
            full_display_name: String::new(),
            kind: RenameSymbolKind::Unknown,
            kind_modifiers: String::new(),
            trigger_span: Range::new(Position::new(0, 0), Position::new(0, 0)),
            localized_error_message: Some(msg.to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// RenameProvider
// ---------------------------------------------------------------------------

define_lsp_provider!(binder RenameProvider, "Rename provider.");

impl<'a> RenameProvider<'a> {
    // -----------------------------------------------------------------------
    // Prepare-rename (simple -- returns Range for backward compatibility)
    // -----------------------------------------------------------------------

    /// Check if the symbol at the position can be renamed.
    /// Returns the Range of the identifier if valid, or None.
    pub fn prepare_rename(&self, position: Position) -> Option<Range> {
        let node_idx = self.rename_target_node(position)?;
        let node = self.arena.get(node_idx)?;
        let start = self.line_map.offset_to_position(node.pos, self.source_text);
        let end = self.line_map.offset_to_position(node.end, self.source_text);
        Some(Range::new(start, end))
    }

    // -----------------------------------------------------------------------
    // Prepare-rename (rich -- returns PrepareRenameResult for tsserver compat)
    // -----------------------------------------------------------------------

    /// Prepare a rename and return a rich result that includes display name,
    /// kind, kind modifiers, and trigger span -- matching tsserver's format.
    pub fn prepare_rename_info(&self, root: NodeIndex, position: Position) -> PrepareRenameResult {
        let Some(node_idx) = self.rename_target_node(position) else {
            return PrepareRenameResult::cannot_rename("You cannot rename this element.");
        };
        let Some(node) = self.arena.get(node_idx) else {
            return PrepareRenameResult::cannot_rename("You cannot rename this element.");
        };

        // Extract identifier text (fall back to source slice, trimming non-ident chars)
        let display_name = match self
            .arena
            .get_identifier_text(node_idx)
            .filter(|s| !s.is_empty())
        {
            Some(s) => s.to_string(),
            None => extract_identifier_from_source(self.source_text, node.pos, node.end),
        };

        // Check for non-renamable built-in identifiers
        if is_non_renamable_builtin(&display_name) {
            return PrepareRenameResult::cannot_rename("You cannot rename this element.");
        }

        // Check if identifier lives inside node_modules (heuristic)
        if self.file_name.contains("node_modules") {
            return PrepareRenameResult::cannot_rename(
                "You cannot rename elements from external modules.",
            );
        }

        // Resolve symbol to get kind / modifiers / qualified name
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let symbol_id = walker.resolve_node(root, node_idx);

        let (kind, kind_modifiers, full_display_name) = self.symbol_info(node_idx, symbol_id);

        let start = self.line_map.offset_to_position(node.pos, self.source_text);
        let end = self.line_map.offset_to_position(node.end, self.source_text);

        PrepareRenameResult {
            can_rename: true,
            display_name,
            full_display_name,
            kind,
            kind_modifiers,
            trigger_span: Range::new(start, end),
            localized_error_message: None,
        }
    }

    // -----------------------------------------------------------------------
    // Validation helpers
    // -----------------------------------------------------------------------

    /// Validate and normalize a rename request for the symbol at the position.
    pub fn normalize_rename_at_position(
        &self,
        position: Position,
        new_name: &str,
    ) -> Result<String, String> {
        let node_idx = self
            .rename_target_node(position)
            .ok_or_else(|| "You cannot rename this element.".to_string())?;
        let node = self
            .arena
            .get(node_idx)
            .ok_or_else(|| "You cannot rename this element.".to_string())?;
        self.normalize_rename_name(node.kind, new_name)
    }

    // -----------------------------------------------------------------------
    // Provide rename edits (standard WorkspaceEdit)
    // -----------------------------------------------------------------------

    /// Perform the rename operation.
    ///
    /// Returns a `WorkspaceEdit` with all the changes needed to rename the symbol,
    /// or an error message if the rename is invalid.
    pub fn provide_rename_edits(
        &self,
        root: NodeIndex,
        position: Position,
        new_name: String,
    ) -> Result<WorkspaceEdit, String> {
        self.provide_rename_edits_internal(root, position, new_name, None, None)
    }

    pub fn provide_rename_edits_with_scope_cache(
        &self,
        root: NodeIndex,
        position: Position,
        new_name: String,
        scope_cache: &mut ScopeCache,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Result<WorkspaceEdit, String> {
        self.provide_rename_edits_internal(root, position, new_name, Some(scope_cache), scope_stats)
    }

    /// Provide rename edits when the symbol has already been resolved.
    pub fn provide_rename_edits_for_symbol(
        &self,
        root: NodeIndex,
        symbol_id: SymbolId,
        new_name: String,
    ) -> Result<WorkspaceEdit, String> {
        if symbol_id.is_none() {
            return Err("Could not find symbol to rename".to_string());
        }

        let finder = FindReferences::new(
            self.arena,
            self.binder,
            self.line_map,
            self.file_name.clone(),
            self.source_text,
        );
        let locations = finder
            .find_references_for_symbol(root, symbol_id)
            .ok_or_else(|| "Could not find symbol to rename".to_string())?;

        let mut workspace_edit = WorkspaceEdit::new();
        for loc in locations {
            workspace_edit.add_edit(loc.file_path, TextEdit::new(loc.range, new_name.clone()));
        }

        Ok(workspace_edit)
    }

    // -----------------------------------------------------------------------
    // Provide rich rename edits (RenameWorkspaceEdit with prefix/suffix)
    // -----------------------------------------------------------------------

    /// Perform rename and return a `RenameWorkspaceEdit` that preserves
    /// `prefix_text` and `suffix_text` metadata for shorthand expansions.
    pub fn provide_rich_rename_edits(
        &self,
        root: NodeIndex,
        position: Position,
        new_name: String,
    ) -> Result<RenameWorkspaceEdit, String> {
        self.provide_rich_rename_edits_internal(root, position, new_name, None, None)
    }

    /// Rich rename with scope cache.
    pub fn provide_rich_rename_edits_with_scope_cache(
        &self,
        root: NodeIndex,
        position: Position,
        new_name: String,
        scope_cache: &mut ScopeCache,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Result<RenameWorkspaceEdit, String> {
        self.provide_rich_rename_edits_internal(
            root,
            position,
            new_name,
            Some(scope_cache),
            scope_stats,
        )
    }

    // -----------------------------------------------------------------------
    // Internal implementation
    // -----------------------------------------------------------------------

    fn provide_rename_edits_internal(
        &self,
        root: NodeIndex,
        position: Position,
        new_name: String,
        scope_cache: Option<&mut ScopeCache>,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Result<WorkspaceEdit, String> {
        let rich = self.provide_rich_rename_edits_internal(
            root,
            position,
            new_name,
            scope_cache,
            scope_stats,
        )?;
        Ok(rich.to_workspace_edit())
    }

    fn provide_rich_rename_edits_internal(
        &self,
        root: NodeIndex,
        position: Position,
        new_name: String,
        scope_cache: Option<&mut ScopeCache>,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Result<RenameWorkspaceEdit, String> {
        let node_idx = self
            .rename_target_node(position)
            .ok_or_else(|| "You cannot rename this element.".to_string())?;
        let node = self
            .arena
            .get(node_idx)
            .ok_or_else(|| "You cannot rename this element.".to_string())?;

        // Get old name for shorthand / import expansion.
        // Try get_identifier_text first; fall back to source text slice.
        let old_name = match self
            .arena
            .get_identifier_text(node_idx)
            .filter(|s| !s.is_empty())
        {
            Some(s) => s.to_string(),
            None => extract_identifier_from_source(self.source_text, node.pos, node.end),
        };

        // Reject non-renamable built-in identifiers
        if is_non_renamable_builtin(&old_name) {
            return Err("You cannot rename this element.".to_string());
        }

        let normalized_name = self.normalize_rename_name(node.kind, &new_name)?;

        // Find all references (declarations + usages)
        let finder = FindReferences::new(
            self.arena,
            self.binder,
            self.line_map,
            self.file_name.clone(),
            self.source_text,
        );

        let locations = if let Some(scope_cache) = scope_cache {
            finder.find_references_with_scope_cache(root, position, scope_cache, scope_stats)
        } else {
            finder.find_references(root, position)
        }
        .ok_or_else(|| "Could not find symbol to rename".to_string())?;

        // Convert locations to RenameTextEdits, handling special contexts
        let mut workspace_edit = RenameWorkspaceEdit::new();

        for loc in &locations {
            let edit = self.build_rename_edit(loc.range, &old_name, &normalized_name);
            workspace_edit.add_edit(loc.file_path.clone(), edit);
        }

        Ok(workspace_edit)
    }

    /// Build a `RenameTextEdit` for a single reference, detecting special
    /// contexts such as shorthand property assignments and import specifiers
    /// where simple text replacement would change semantics.
    fn build_rename_edit(&self, range: Range, old_name: &str, new_name: &str) -> RenameTextEdit {
        // Determine the byte offset of the reference
        let Some(offset) = self
            .line_map
            .position_to_offset(range.start, self.source_text)
        else {
            return RenameTextEdit::new(range, new_name.to_string());
        };

        let ref_node_idx = find_node_at_offset(self.arena, offset);
        if ref_node_idx.is_none() {
            return RenameTextEdit::new(range, new_name.to_string());
        }

        // Check parent context
        if let Some(ext) = self.arena.get_extended(ref_node_idx) {
            let parent = ext.parent;
            if parent.is_some()
                && let Some(parent_node) = self.arena.get(parent)
            {
                // Shorthand property assignment: `{ x }` => when renaming
                // x to y, we need `{ x: y }` (insert old name as property
                // key prefix).
                if parent_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                    return RenameTextEdit::with_prefix(
                        range,
                        new_name.to_string(),
                        format!("{old_name}: "),
                    );
                }
                // Also handle PROPERTY_ASSIGNMENT where name == initializer
                // (legacy shorthand detection)
                if parent_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
                    && let Some(prop) = self.arena.get_property_assignment(parent_node)
                    && prop.name == prop.initializer
                {
                    return RenameTextEdit::with_prefix(
                        range,
                        new_name.to_string(),
                        format!("{old_name}: "),
                    );
                }

                // Binding element in destructuring: `const { x } = obj;`
                // When renaming local x to y, we need `const { x: y } = obj;`.
                if parent_node.kind == syntax_kind_ext::BINDING_ELEMENT
                    && let Some(binding) = self.arena.get_binding_element(parent_node)
                {
                    // Only expand when there is no explicit
                    // property_name (i.e., the shorthand form).
                    if binding.property_name.is_none() {
                        return RenameTextEdit::with_prefix(
                            range,
                            new_name.to_string(),
                            format!("{old_name}: "),
                        );
                    }
                }

                // Import specifier shorthand: `import { foo } from 'mod'`
                // When renaming foo to bar, we need
                // `import { foo as bar } from 'mod'`.
                if parent_node.kind == syntax_kind_ext::IMPORT_SPECIFIER
                    && let Some(spec) = self.arena.get_specifier(parent_node)
                    && spec.property_name.is_none()
                {
                    return RenameTextEdit::with_prefix(
                        range,
                        new_name.to_string(),
                        format!("{old_name} as "),
                    );
                }

                // Export specifier shorthand: `export { foo }`
                // When renaming local foo to bar, we need
                // `export { bar as foo }` to keep the public API stable.
                if parent_node.kind == syntax_kind_ext::EXPORT_SPECIFIER
                    && let Some(spec) = self.arena.get_specifier(parent_node)
                    && spec.property_name.is_none()
                {
                    return RenameTextEdit::with_suffix(
                        range,
                        new_name.to_string(),
                        format!(" as {old_name}"),
                    );
                }
            }
        }

        RenameTextEdit::new(range, new_name.to_string())
    }

    // -----------------------------------------------------------------------
    // Target node lookup
    // -----------------------------------------------------------------------

    fn rename_target_node(&self, position: Position) -> Option<NodeIndex> {
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;
        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return None;
        }

        let node = self.arena.get(node_idx)?;

        // Allow renaming identifiers and private identifiers
        if node.kind == SyntaxKind::Identifier as u16
            || node.kind == SyntaxKind::PrivateIdentifier as u16
        {
            return Some(node_idx);
        }

        // Allow renaming string literal property names in computed element
        // access (`obj["propName"]`) and string-keyed property assignments.
        if node.kind == SyntaxKind::StringLiteral as u16
            && let Some(ext) = self.arena.get_extended(node_idx)
        {
            let parent = ext.parent;
            if parent.is_some()
                && let Some(parent_node) = self.arena.get(parent)
                && (parent_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                    || parent_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT)
            {
                return Some(node_idx);
            }
        }

        None
    }

    // -----------------------------------------------------------------------
    // Symbol info helpers (for prepare_rename_info)
    // -----------------------------------------------------------------------

    /// Derive the symbol kind, kind modifiers, and full display name from the
    /// resolved symbol (if any) and the AST node.
    fn symbol_info(
        &self,
        node_idx: NodeIndex,
        symbol_id: Option<SymbolId>,
    ) -> (RenameSymbolKind, String, String) {
        let display_name = match self
            .arena
            .get_identifier_text(node_idx)
            .filter(|s| !s.is_empty())
        {
            Some(s) => s.to_string(),
            None => {
                if let Some(n) = self.arena.get(node_idx) {
                    extract_identifier_from_source(self.source_text, n.pos, n.end)
                } else {
                    String::new()
                }
            }
        };

        let Some(sym_id) = symbol_id else {
            return (RenameSymbolKind::Unknown, String::new(), display_name);
        };
        if sym_id.is_none() {
            return (RenameSymbolKind::Unknown, String::new(), display_name);
        }
        let Some(symbol) = self.binder.symbols.get(sym_id) else {
            return (RenameSymbolKind::Unknown, String::new(), display_name);
        };

        let flags = symbol.flags;

        // Determine kind
        let kind = if flags & symbol_flags::FUNCTION != 0 {
            RenameSymbolKind::Function
        } else if flags & symbol_flags::CLASS != 0 {
            RenameSymbolKind::Class
        } else if flags & symbol_flags::INTERFACE != 0 {
            RenameSymbolKind::Interface
        } else if flags & symbol_flags::TYPE_ALIAS != 0 {
            RenameSymbolKind::TypeAlias
        } else if flags & symbol_flags::ENUM != 0 {
            RenameSymbolKind::Enum
        } else if flags & symbol_flags::ENUM_MEMBER != 0 {
            RenameSymbolKind::EnumMember
        } else if flags & symbol_flags::MODULE != 0 {
            RenameSymbolKind::Module
        } else if flags & symbol_flags::METHOD != 0 {
            RenameSymbolKind::Method
        } else if flags & symbol_flags::PROPERTY != 0 {
            RenameSymbolKind::Property
        } else if flags & symbol_flags::TYPE_PARAMETER != 0 {
            RenameSymbolKind::TypeParameter
        } else if flags & symbol_flags::ALIAS != 0 {
            RenameSymbolKind::Alias
        } else if flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0 {
            self.let_or_const_kind(symbol)
        } else if flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0 {
            if self.is_parameter(symbol) {
                RenameSymbolKind::Parameter
            } else {
                RenameSymbolKind::Var
            }
        } else {
            RenameSymbolKind::Unknown
        };

        // Determine kind modifiers (export, declare, etc.)
        let kind_modifiers = self.kind_modifiers_for_symbol(symbol);

        // Build full display name (qualified)
        let full_display_name = self.full_display_name(symbol, &display_name);

        (kind, kind_modifiers, full_display_name)
    }

    /// Determine whether a block-scoped variable is `let` or `const`.
    fn let_or_const_kind(&self, symbol: &tsz_binder::Symbol) -> RenameSymbolKind {
        for &decl_idx in &symbol.declarations {
            if let Some(decl_node) = self.arena.get(decl_idx)
                && decl_node.flags as u32 & tsz_parser::parser::flags::node_flags::CONST != 0
            {
                return RenameSymbolKind::Const;
            }
            if let Some(ext) = self.arena.get_extended(decl_idx)
                && ext.parent.is_some()
                && let Some(parent_node) = self.arena.get(ext.parent)
                && parent_node.flags as u32 & tsz_parser::parser::flags::node_flags::CONST != 0
            {
                return RenameSymbolKind::Const;
            }
        }
        RenameSymbolKind::Let
    }

    /// Check whether a function-scoped variable is actually a parameter.
    fn is_parameter(&self, symbol: &tsz_binder::Symbol) -> bool {
        for &decl_idx in &symbol.declarations {
            if let Some(decl_node) = self.arena.get(decl_idx)
                && decl_node.kind == syntax_kind_ext::PARAMETER
            {
                return true;
            }
        }
        false
    }

    /// Compute comma-separated kind modifiers (e.g. `"export,declare"`).
    fn kind_modifiers_for_symbol(&self, symbol: &tsz_binder::Symbol) -> String {
        let mut modifiers: Vec<&str> = Vec::new();

        if symbol.is_exported {
            modifiers.push("export");
        }

        for &decl_idx in &symbol.declarations {
            if let Some(ext) = self.arena.get_extended(decl_idx) {
                let mf = ext.modifier_flags;
                if mf & modifier_flags::AMBIENT != 0 && !modifiers.contains(&"declare") {
                    modifiers.push("declare");
                }
                if mf & modifier_flags::ABSTRACT != 0 && !modifiers.contains(&"abstract") {
                    modifiers.push("abstract");
                }
                if mf & modifier_flags::ASYNC != 0 && !modifiers.contains(&"async") {
                    modifiers.push("async");
                }
                if mf & modifier_flags::STATIC != 0 && !modifiers.contains(&"static") {
                    modifiers.push("static");
                }
                if mf & modifier_flags::DEFAULT != 0 && !modifiers.contains(&"default") {
                    modifiers.push("default");
                }
            }
        }

        modifiers.join(",")
    }

    /// Build a qualified display name by walking parent symbols.
    fn full_display_name(&self, symbol: &tsz_binder::Symbol, simple_name: &str) -> String {
        let mut parts = vec![simple_name.to_string()];
        let mut current_parent = symbol.parent;

        for _ in 0..10 {
            if current_parent.is_none() {
                break;
            }
            if let Some(parent_sym) = self.binder.symbols.get(current_parent) {
                if !parent_sym.escaped_name.is_empty() && parent_sym.escaped_name != "__global" {
                    parts.push(parent_sym.escaped_name.clone());
                }
                current_parent = parent_sym.parent;
            } else {
                break;
            }
        }

        parts.reverse();
        parts.join(".")
    }

    // -----------------------------------------------------------------------
    // Identifier validation
    // -----------------------------------------------------------------------

    /// Validate that a string is a valid identifier.
    fn is_valid_identifier(&self, name: &str) -> bool {
        if name.is_empty() {
            return false;
        }

        if let Some(kind) = tsz_scanner::text_to_keyword(name)
            && (tsz_scanner::token_is_reserved_word(kind)
                || tsz_scanner::token_is_strict_mode_reserved_word(kind))
        {
            return false;
        }

        let mut chars = name.chars();

        if let Some(first) = chars.next() {
            if !is_identifier_start(first) {
                return false;
            }
        } else {
            return false;
        }

        for ch in chars {
            if !is_identifier_part(ch) {
                return false;
            }
        }

        true
    }

    fn normalize_rename_name(&self, node_kind: u16, new_name: &str) -> Result<String, String> {
        let is_private = node_kind == SyntaxKind::PrivateIdentifier as u16;
        if is_private {
            let stripped = new_name.strip_prefix('#').unwrap_or(new_name);
            if !is_valid_private_identifier(stripped) {
                return Err(format!(
                    "'{new_name}' is not a valid private identifier name"
                ));
            }
            return Ok(format!("#{stripped}"));
        }

        // For string literal property names, accept any non-empty string
        if node_kind == SyntaxKind::StringLiteral as u16 {
            if new_name.is_empty() {
                return Err("Rename target cannot be empty.".to_string());
            }
            return Ok(new_name.to_string());
        }

        if new_name.starts_with('#') || !self.is_valid_identifier(new_name) {
            return Err(format!("'{new_name}' is not a valid identifier name"));
        }

        Ok(new_name.to_string())
    }
}

// ---------------------------------------------------------------------------
// Free-standing helpers
// ---------------------------------------------------------------------------

/// Check if a character can start an identifier.
fn is_identifier_start(ch: char) -> bool {
    ch == '$' || ch == '_' || ch.is_alphabetic()
}

/// Check if a character can be part of an identifier.
fn is_identifier_part(ch: char) -> bool {
    ch == '$' || ch == '_' || ch.is_alphanumeric()
}

fn is_valid_private_identifier(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !is_identifier_start(first) {
        return false;
    }

    for ch in chars {
        if !is_identifier_part(ch) {
            return false;
        }
    }

    true
}

/// Extract an identifier name from a source text range, trimming any
/// trailing non-identifier characters (like `;` or `,`) that the parser
/// may include in the node's span.
fn extract_identifier_from_source(source: &str, pos: u32, end: u32) -> String {
    let start = pos as usize;
    let end = end as usize;
    if end <= source.len() && start < end {
        let raw = &source[start..end];
        // Trim trailing non-identifier characters
        raw.trim_end_matches(|c: char| !is_identifier_part(c) && c != '#')
            .to_string()
    } else {
        String::new()
    }
}

/// Return `true` for identifiers that should never be renamed because they
/// are built-in global names or keywords that happen to parse as identifiers.
fn is_non_renamable_builtin(name: &str) -> bool {
    matches!(
        name,
        "undefined" | "NaN" | "Infinity" | "globalThis" | "arguments"
    )
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
#[path = "../tests/rename_tests.rs"]
mod rename_tests;
