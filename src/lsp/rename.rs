//! Rename implementation for LSP.
//!
//! Handles renaming symbols across the codebase, including validation
//! and workspace edit generation.

use crate::binder::SymbolId;
use crate::lsp::position::{LineMap, Position, Range};
use crate::lsp::references::FindReferences;
use crate::lsp::resolver::{ScopeCache, ScopeCacheStats};
use crate::lsp::utils::find_node_at_offset;
use crate::parser::NodeIndex;
use crate::parser::thin_node::ThinNodeArena;
use crate::scanner::{self, SyntaxKind};
use crate::thin_binder::ThinBinderState;
use std::collections::HashMap;

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
    pub fn new(range: Range, new_text: String) -> Self {
        Self { range, new_text }
    }
}

/// A workspace edit (changes across multiple files).
#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkspaceEdit {
    /// Map of file path -> list of edits.
    pub changes: HashMap<String, Vec<TextEdit>>,
}

impl WorkspaceEdit {
    /// Create a new workspace edit.
    pub fn new() -> Self {
        Self {
            changes: HashMap::new(),
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

/// Provider for Rename functionality.
pub struct RenameProvider<'a> {
    arena: &'a ThinNodeArena,
    binder: &'a ThinBinderState,
    line_map: &'a LineMap,
    file_name: String,
    source_text: &'a str,
}

impl<'a> RenameProvider<'a> {
    /// Create a new rename provider.
    pub fn new(
        arena: &'a ThinNodeArena,
        binder: &'a ThinBinderState,
        line_map: &'a LineMap,
        file_name: String,
        source_text: &'a str,
    ) -> Self {
        Self {
            arena,
            binder,
            line_map,
            file_name,
            source_text,
        }
    }

    /// Check if the symbol at the position can be renamed.
    /// Returns the Range of the identifier if valid, or None.
    pub fn prepare_rename(&self, position: Position) -> Option<Range> {
        let node_idx = self.rename_target_node(position)?;
        let node = self.arena.get(node_idx)?;
        let start = self.line_map.offset_to_position(node.pos, self.source_text);
        let end = self.line_map.offset_to_position(node.end, self.source_text);
        Some(Range::new(start, end))
    }

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

    /// Perform the rename operation.
    ///
    /// Returns a WorkspaceEdit with all the changes needed to rename the symbol,
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

    fn provide_rename_edits_internal(
        &self,
        root: NodeIndex,
        position: Position,
        new_name: String,
        scope_cache: Option<&mut ScopeCache>,
        mut scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Result<WorkspaceEdit, String> {
        let node_idx = self
            .rename_target_node(position)
            .ok_or_else(|| "You cannot rename this element.".to_string())?;
        let node = self
            .arena
            .get(node_idx)
            .ok_or_else(|| "You cannot rename this element.".to_string())?;
        let normalized_name = self.normalize_rename_name(node.kind, &new_name)?;

        // 3. Find all references (declarations + usages)
        // We reuse the existing FindReferences logic to ensure consistency
        let finder = FindReferences::new(
            self.arena,
            self.binder,
            self.line_map,
            self.file_name.clone(),
            self.source_text,
        );

        // We use find_references which includes the definition
        let locations = if let Some(scope_cache) = scope_cache {
            finder.find_references_with_scope_cache(
                root,
                position,
                scope_cache,
                scope_stats.as_deref_mut(),
            )
        } else {
            finder.find_references(root, position)
        }
        .ok_or_else(|| "Could not find symbol to rename".to_string())?;

        // 4. Convert locations to TextEdits
        let mut workspace_edit = WorkspaceEdit::new();

        for loc in locations {
            workspace_edit.add_edit(
                loc.file_path,
                TextEdit::new(loc.range, normalized_name.clone()),
            );
        }

        Ok(workspace_edit)
    }

    fn rename_target_node(&self, position: Position) -> Option<NodeIndex> {
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;
        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return None;
        }

        let node = self.arena.get(node_idx)?;
        if node.kind == SyntaxKind::Identifier as u16
            || node.kind == SyntaxKind::PrivateIdentifier as u16
        {
            return Some(node_idx);
        }

        None
    }

    /// Validate that a string is a valid identifier.
    ///
    /// Checks that the name:
    /// - Is not empty
    /// - Is not a reserved keyword (but allows contextual keywords like 'string', 'type', etc.)
    /// - Starts with a valid identifier start character (letter, _, $)
    /// - Contains only valid identifier characters (letters, digits, _, $)
    fn is_valid_identifier(&self, name: &str) -> bool {
        if name.is_empty() {
            return false;
        }

        // Check if it's a reserved word or strict mode reserved word
        // Allow contextual keywords (async, await, type, string, number, etc.)
        if let Some(kind) = scanner::text_to_keyword(name) {
            if scanner::token_is_reserved_word(kind)
                || scanner::token_is_strict_mode_reserved_word(kind)
            {
                return false;
            }
        }

        // Manual char check
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
                    "'{}' is not a valid private identifier name",
                    new_name
                ));
            }
            return Ok(format!("#{}", stripped));
        }

        if new_name.starts_with('#') || !self.is_valid_identifier(new_name) {
            return Err(format!("'{}' is not a valid identifier name", new_name));
        }

        Ok(new_name.to_string())
    }
}

// Helpers for identifier validation (mirrors scanner logic)

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

#[cfg(test)]
mod rename_tests {
    use super::*;
    use crate::lsp::position::LineMap;
    use crate::lsp::resolver::ScopeCache;
    use crate::thin_binder::ThinBinderState;
    use crate::thin_parser::ThinParserState;

    #[test]
    fn test_rename_variable() {
        // let oldName = 1; const b = oldName + 1;
        let source = "let oldName = 1; const b = oldName + 1;";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let rename_provider =
            RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        // Rename 'oldName' at declaration (0, 4)
        let pos = Position::new(0, 4);

        // 1. Check prepare
        let range = rename_provider.prepare_rename(pos);
        assert!(range.is_some(), "Should be able to prepare rename");

        // 2. Perform rename
        let result = rename_provider.provide_rename_edits(root, pos, "newName".to_string());
        assert!(result.is_ok(), "Rename should succeed");

        let workspace_edit = result.unwrap();
        let edits = workspace_edit.changes.get("test.ts").unwrap();

        // Should have at least 2 edits: the declaration and the usage
        assert!(
            edits.len() >= 2,
            "Should have at least 2 edits (declaration + usage)"
        );

        // Check all texts are newName
        for edit in edits {
            assert_eq!(edit.new_text, "newName");
        }
    }

    #[test]
    fn test_rename_uses_scope_cache() {
        let source = "let value = 1;\nvalue;";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let rename_provider =
            RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        let mut scope_cache = ScopeCache::default();
        let pos = Position::new(1, 0);

        let result = rename_provider.provide_rename_edits_with_scope_cache(
            root,
            pos,
            "next".to_string(),
            &mut scope_cache,
            None,
        );
        assert!(result.is_ok(), "Rename should succeed with scope cache");
        assert!(
            !scope_cache.is_empty(),
            "Expected scope cache to populate for rename"
        );
    }

    #[test]
    fn test_rename_invalid_keyword() {
        let source = "let x = 1;";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);
        let line_map = LineMap::build(source);
        let rename_provider =
            RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        let pos = Position::new(0, 4);

        // Try renaming to a keyword
        let result = rename_provider.provide_rename_edits(root, pos, "class".to_string());
        assert!(result.is_err(), "Should not allow renaming to keyword");
    }

    #[test]
    fn test_rename_invalid_chars() {
        let source = "let x = 1;";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);
        let line_map = LineMap::build(source);
        let rename_provider =
            RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        let pos = Position::new(0, 4);

        // Try renaming to invalid identifier
        let result = rename_provider.provide_rename_edits(root, pos, "123var".to_string());
        assert!(result.is_err(), "Should not allow invalid identifier");
    }

    #[test]
    fn test_rename_function() {
        // function foo() {} foo();
        let source = "function foo() {}\nfoo();";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let rename_provider =
            RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        // Rename 'foo' at the call site (1, 0)
        let pos = Position::new(1, 0);

        let result = rename_provider.provide_rename_edits(root, pos, "bar".to_string());
        assert!(result.is_ok(), "Rename should succeed");

        let workspace_edit = result.unwrap();
        let edits = workspace_edit.changes.get("test.ts").unwrap();

        // Should have at least 2 edits: the declaration and the call
        assert!(edits.len() >= 2, "Should have at least 2 edits");

        // Check all texts are bar
        for edit in edits {
            assert_eq!(edit.new_text, "bar");
        }
    }

    #[test]
    fn test_rename_private_identifier() {
        let source = "class Foo {\n  #bar = 1;\n  method() {\n    this.#bar;\n  }\n}\n";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let rename_provider =
            RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        let pos = Position::new(3, 9); // on '#bar'
        let result = rename_provider.provide_rename_edits(root, pos, "baz".to_string());
        assert!(
            result.is_ok(),
            "Rename should succeed for private identifier"
        );

        let workspace_edit = result.unwrap();
        let edits = workspace_edit.changes.get("test.ts").unwrap();
        assert!(edits.len() >= 2, "Should rename declaration and usage");
        for edit in edits {
            assert_eq!(edit.new_text, "#baz");
        }
    }

    #[test]
    fn test_rename_private_identifier_with_hash() {
        let source = "class Foo {\n  #bar = 1;\n  method() {\n    this.#bar;\n  }\n}\n";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let rename_provider =
            RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        let pos = Position::new(3, 9); // on '#bar'
        let result = rename_provider.provide_rename_edits(root, pos, "#qux".to_string());
        assert!(
            result.is_ok(),
            "Rename should accept '#qux' for private identifier"
        );

        let workspace_edit = result.unwrap();
        let edits = workspace_edit.changes.get("test.ts").unwrap();
        for edit in edits {
            assert_eq!(edit.new_text, "#qux");
        }
    }

    #[test]
    fn test_prepare_rename_invalid_position() {
        let source = "let x = 1;";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);
        let line_map = LineMap::build(source);
        let rename_provider =
            RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        // Position on the number literal '1', not an identifier
        let pos = Position::new(0, 8);

        let range = rename_provider.prepare_rename(pos);
        assert!(
            range.is_none(),
            "Should not be able to rename non-identifier"
        );
    }

    #[test]
    fn test_rename_rejects_private_name_for_identifier() {
        let source = "let x = 1;";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);
        let line_map = LineMap::build(source);
        let rename_provider =
            RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        let pos = Position::new(0, 4);
        let result = rename_provider.provide_rename_edits(root, pos, "#foo".to_string());
        assert!(
            result.is_err(),
            "Should not allow private names for identifiers"
        );
    }

    #[test]
    fn test_rename_to_contextual_keyword() {
        // Test that we can rename to contextual keywords like 'string', 'type', etc.
        let source = "let x = 1;";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        let mut binder = ThinBinderState::new();
        binder.bind_source_file(arena, root);
        let line_map = LineMap::build(source);
        let rename_provider =
            RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        let pos = Position::new(0, 4);

        // Should allow renaming to contextual keywords
        let result = rename_provider.provide_rename_edits(root, pos, "string".to_string());
        assert!(
            result.is_ok(),
            "Should allow renaming to 'string' (contextual keyword)"
        );

        let result = rename_provider.provide_rename_edits(root, pos, "type".to_string());
        assert!(
            result.is_ok(),
            "Should allow renaming to 'type' (contextual keyword)"
        );

        let result = rename_provider.provide_rename_edits(root, pos, "async".to_string());
        assert!(
            result.is_ok(),
            "Should allow renaming to 'async' (contextual keyword)"
        );
    }
}
