//! Core rename implementation logic.
//!
//! Contains the main `RenameProvider` methods for preparing renames,
//! providing rename edits, and building rich rename edits with
//! prefix/suffix metadata for shorthand expansions.

use super::{
    PrepareRenameResult, RenameProvider, RenameSymbolKind, RenameTextEdit, RenameWorkspaceEdit,
    TextEdit, WorkspaceEdit,
};
use crate::navigation::references::FindReferences;
use crate::resolver::{ScopeCache, ScopeCacheStats, ScopeWalker};
use crate::utils::find_node_at_offset;
use tsz_binder::SymbolId;
use tsz_binder::symbol_flags;
use tsz_common::position::{Position, Range};
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::{NodeIndex, modifier_flags, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

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

        // `default` cannot be renamed when used as a declaration name
        // (parameter, variable, function, class), but CAN be renamed as a
        // property name in an object literal.
        if display_name == "default" {
            let is_property_name = self
                .arena
                .get_extended(node_idx)
                .and_then(|ext| self.arena.get(ext.parent))
                .is_some_and(|parent| {
                    parent.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
                        || parent.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
                        || parent.kind == syntax_kind_ext::METHOD_DECLARATION
                });
            if !is_property_name {
                return PrepareRenameResult::cannot_rename("You cannot rename this element.");
            }
        }

        // Check if identifier lives inside node_modules (heuristic)
        if self.file_name.contains("node_modules") {
            return PrepareRenameResult::cannot_rename(
                "You cannot rename elements from external modules.",
            );
        }

        // Resolve symbol to get kind / modifiers / qualified name
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let mut symbol_id = walker.resolve_node(root, node_idx);

        // If direct resolution failed, try resolving as a property access member
        // (e.g., `e.thirdMember` where thirdMember is an enum member).
        if symbol_id.map_or(true, |id| id.is_none()) {
            if let Some(member_sym_id) =
                self.resolve_property_access_member(&mut walker, root, node_idx, &display_name)
            {
                symbol_id = Some(member_sym_id);
            }
        }

        let (kind, kind_modifiers, full_display_name) = self.symbol_info(node_idx, symbol_id);

        let start = self.line_map.offset_to_position(node.pos, self.source_text);
        // Use the display_name length for the trigger span end, not node.end.
        // Some nodes (e.g., DefaultKeyword in property assignments) have an end
        // that extends past the identifier into trailing punctuation like `:`.
        let trigger_end = node.pos + display_name.len() as u32;
        let end = self
            .line_map
            .offset_to_position(trigger_end.min(node.end), self.source_text);

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

    pub(super) fn rename_target_node(&self, position: Position) -> Option<NodeIndex> {
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
                    || parent_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
                    || parent_node.kind == syntax_kind_ext::IMPORT_SPECIFIER
                    || parent_node.kind == syntax_kind_ext::EXPORT_SPECIFIER)
            {
                return Some(node_idx);
            }
        }

        None
    }

    // -----------------------------------------------------------------------
    // Property access member resolution
    // -----------------------------------------------------------------------

    /// When the cursor is on the `name` part of a PropertyAccessExpression
    /// (e.g., `thirdMember` in `e.thirdMember`), try to resolve the expression
    /// part to a symbol, then look up the member name in that symbol's exports
    /// or members table.
    fn resolve_property_access_member(
        &self,
        walker: &mut ScopeWalker<'_>,
        root: NodeIndex,
        name_node: NodeIndex,
        member_name: &str,
    ) -> Option<SymbolId> {
        let ext = self.arena.get_extended(name_node)?;
        let parent_node = self.arena.get(ext.parent)?;
        if parent_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access_data = self.arena.get_access_expr(parent_node)?;
        // Only proceed if this node is the name part, not the expression part
        if access_data.name_or_argument != name_node {
            return None;
        }
        // Resolve the expression (e.g., `e`) to a symbol
        let expr_sym_id = walker.resolve_node(root, access_data.expression)?;
        if expr_sym_id.is_none() {
            return None;
        }
        let expr_sym = self.binder.symbols.get(expr_sym_id)?;
        // Look in exports first (enum members are stored as exports)
        if let Some(exports) = &expr_sym.exports {
            if let Some(member_id) = exports.get(member_name) {
                return Some(member_id);
            }
        }
        // Then try members (for class/interface members)
        if let Some(members) = &expr_sym.members {
            if let Some(member_id) = members.get(member_name) {
                return Some(member_id);
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
        let mut is_top_level = false;

        for _ in 0..10 {
            if current_parent.is_none() {
                is_top_level = true;
                break;
            }
            if let Some(parent_sym) = self.binder.symbols.get(current_parent) {
                let name = &parent_sym.escaped_name;
                if name == "__global" || name == "__export" {
                    is_top_level = true;
                    break;
                }
                // Source file symbols have the file path as name
                if name.starts_with('/') || name.starts_with("\\\\") {
                    is_top_level = true;
                    break;
                }
                if !name.is_empty() {
                    parts.push(name.clone());
                }
                current_parent = parent_sym.parent;
            } else {
                is_top_level = true;
                break;
            }
        }

        parts.reverse();

        // For top-level symbols with the EXPORT_VALUE flag (directly declared
        // with `export` keyword, e.g. `export class Foo`, `export default class Foo`),
        // prefix with the quoted module path: '"/path/to/module".SymbolName'.
        //
        // Do NOT qualify symbols that are only `is_exported` — this includes
        // local names re-exported via `export { x as y }` or `export default f`.
        if is_top_level && symbol.flags & symbol_flags::EXPORT_VALUE != 0 {
            let module_name = self
                .file_name
                .strip_suffix(".d.ts")
                .or_else(|| self.file_name.strip_suffix(".ts"))
                .or_else(|| self.file_name.strip_suffix(".tsx"))
                .or_else(|| self.file_name.strip_suffix(".js"))
                .or_else(|| self.file_name.strip_suffix(".jsx"))
                .unwrap_or(&self.file_name);
            return format!("\"{}\".{}", module_name, parts.join("."));
        }

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
pub(super) fn extract_identifier_from_source(source: &str, pos: u32, end: u32) -> String {
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
pub(super) fn is_non_renamable_builtin(name: &str) -> bool {
    matches!(
        name,
        "undefined" | "NaN" | "Infinity" | "globalThis" | "arguments"
    )
}
