//! Semantic Tokens implementation for LSP.
//!
//! Provides semantic syntax highlighting by classifying identifiers based on
//! symbol information from the binder. This allows editors to distinguish
//! between different types of identifiers (variables, functions, classes, etc.)
//! with better coloring and styling.
//!
//! # Encoding
//! Returns a flat list of integers in delta-encoded format:
//! `[deltaLine, deltaStartChar, length, tokenType, tokenModifiers]`
//!
//! Each token is encoded relative to the previous token for efficiency.

use crate::binder::BinderState;
use crate::binder::{Symbol, symbol_flags};
use crate::lsp::position::LineMap;
use crate::parser::node::{NodeAccess, NodeArena};
use crate::parser::{NodeIndex, node_flags, syntax_kind_ext};
use crate::scanner::SyntaxKind;

/// LSP Semantic Token Types (mapped to indices 0-N).
/// These match the standard LSP semantic token types.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticTokenType {
    Namespace = 0,
    Type = 1,
    Class = 2,
    Enum = 3,
    Interface = 4,
    Struct = 5,
    TypeParameter = 6,
    Parameter = 7,
    Variable = 8,
    Property = 9,
    EnumMember = 10,
    Event = 11,
    Function = 12,
    Method = 13,
    Macro = 14,
    Keyword = 15,
    Modifier = 16,
    Comment = 17,
    String = 18,
    Number = 19,
    Regexp = 20,
    Operator = 21,
    Decorator = 22,
}

/// LSP Semantic Token Modifiers (bit flags).
pub mod semantic_token_modifiers {
    pub const DECLARATION: u32 = 1 << 0;
    pub const DEFINITION: u32 = 1 << 1;
    pub const READONLY: u32 = 1 << 2;
    pub const STATIC: u32 = 1 << 3;
    pub const DEPRECATED: u32 = 1 << 4;
    pub const ABSTRACT: u32 = 1 << 5;
    pub const ASYNC: u32 = 1 << 6;
    pub const MODIFICATION: u32 = 1 << 7;
    pub const DOCUMENTATION: u32 = 1 << 8;
    pub const DEFAULT_LIBRARY: u32 = 1 << 9;
}

/// Builder for LSP semantic tokens response (delta encoding).
///
/// Tokens must be pushed in order of appearance in the file (by line, then by column).
pub struct SemanticTokensBuilder {
    data: Vec<u32>,
    prev_line: u32,
    prev_char: u32,
}

impl SemanticTokensBuilder {
    /// Create a new semantic tokens builder.
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            prev_line: 0,
            prev_char: 0,
        }
    }

    /// Push a token. Tokens MUST be pushed in order of appearance in the file.
    pub fn push(
        &mut self,
        line: u32,
        start_char: u32,
        length: u32,
        token_type: SemanticTokenType,
        modifiers: u32,
    ) {
        // Delta encoding: encode relative to previous token
        let delta_line = line - self.prev_line;
        let delta_start = if delta_line == 0 {
            start_char - self.prev_char
        } else {
            start_char // New line, so absolute position
        };

        self.data.push(delta_line);
        self.data.push(delta_start);
        self.data.push(length);
        self.data.push(token_type as u32);
        self.data.push(modifiers);

        self.prev_line = line;
        self.prev_char = start_char;
    }

    /// Build the final token array.
    pub fn build(self) -> Vec<u32> {
        self.data
    }
}

impl Default for SemanticTokensBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Provider for Semantic Tokens.
pub struct SemanticTokensProvider<'a> {
    arena: &'a NodeArena,
    binder: &'a BinderState,
    line_map: &'a LineMap,
    source_text: &'a str,
    builder: SemanticTokensBuilder,
    in_decorator: bool,
}

impl<'a> SemanticTokensProvider<'a> {
    /// Create a new semantic tokens provider.
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
            builder: SemanticTokensBuilder::new(),
            in_decorator: false,
        }
    }

    /// Compute semantic tokens for the entire file.
    ///
    /// Returns a delta-encoded array of integers representing all semantic tokens.
    pub fn get_semantic_tokens(&mut self, root: NodeIndex) -> Vec<u32> {
        // Traverse the AST in document order and emit tokens
        self.visit_node(root);

        // Take the builder and return the encoded data
        std::mem::take(&mut self.builder).build()
    }

    /// Visit a node and its children recursively.
    fn visit_node(&mut self, node_idx: NodeIndex) {
        if node_idx.is_none() {
            return;
        }

        let Some(node) = self.arena.get(node_idx) else {
            return;
        };

        // Handle modifiers (keywords like public, private, static, readonly)
        if self.is_modifier(node.kind) {
            self.emit_token_for_node(node_idx, SemanticTokenType::Modifier, 0);
            return;
        }

        // Handle Decorator node wrapper
        if node.kind == syntax_kind_ext::DECORATOR {
            let prev_in_decorator = self.in_decorator;
            self.in_decorator = true;
            self.visit_children(node_idx);
            self.in_decorator = prev_in_decorator;
            return;
        }

        // Handle identifiers - both declarations and references
        if node.kind == SyntaxKind::Identifier as u16 {
            self.handle_identifier(node_idx);
            return;
        }

        // For all other nodes, just recurse into children
        self.visit_children(node_idx);
    }

    /// Handle an identifier node by resolving it to a symbol and emitting a token.
    fn handle_identifier(&mut self, node_idx: NodeIndex) {
        // Check if in decorator context
        if self.in_decorator {
            self.emit_token_for_node(node_idx, SemanticTokenType::Decorator, 0);
            return;
        }

        // Check if this identifier is the name of a type parameter (not always bound in binder)
        if self.is_type_parameter_name(node_idx) {
            self.emit_token_at(
                node_idx,
                SemanticTokenType::TypeParameter,
                semantic_token_modifiers::DECLARATION,
            );
            return;
        }

        // First check if this is a declaration (has a symbol directly bound to its parent)
        if let Some(sym_id) = self.find_declaration_symbol(node_idx) {
            if let Some(symbol) = self.binder.get_symbol(sym_id) {
                let (token_type, mut modifiers) = self.map_symbol_to_token(symbol);
                modifiers |= semantic_token_modifiers::DECLARATION;

                // Check for const variable (readonly modifier)
                modifiers |= self.get_contextual_modifiers(node_idx, symbol);

                self.emit_token_at(node_idx, token_type, modifiers);
                return;
            }
        }

        // Try to resolve this identifier as a reference
        if let Some(sym_id) = self.binder.resolve_identifier(self.arena, node_idx) {
            if let Some(symbol) = self.binder.get_symbol(sym_id) {
                let (token_type, mut modifiers) = self.map_symbol_to_token(symbol);
                modifiers |= self.get_contextual_modifiers_for_ref(symbol);
                self.emit_token_at(node_idx, token_type, modifiers);
                return;
            }
        }

        // Check if this identifier is used as a type reference to a type parameter
        if self.is_type_parameter_reference(node_idx) {
            self.emit_token_at(node_idx, SemanticTokenType::TypeParameter, 0);
            return;
        }

        // Unresolved identifier - don't emit a token (let editor use default highlighting)
    }

    /// Find the symbol for a declaration that this identifier is the name of.
    ///
    /// Walks up to the parent declaration node and checks if it has a bound symbol.
    fn find_declaration_symbol(&self, ident_idx: NodeIndex) -> Option<crate::binder::SymbolId> {
        // Check each possible parent declaration type
        // The binder maps declaration nodes (not their name identifiers) to symbols
        let ext = self.arena.get_extended(ident_idx)?;
        let parent_idx = ext.parent;
        if parent_idx.is_none() {
            return None;
        }
        let parent = self.arena.get(parent_idx)?;

        // Check if the parent is a declaration and this identifier is its name
        let is_name_of_decl = match parent.kind {
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                self.arena.get_variable_declaration(parent).map(|d| d.name) == Some(ident_idx)
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION =>
            {
                self.arena
                    .get_function(parent)
                    .and_then(|f| {
                        if f.name == ident_idx {
                            Some(true)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(false)
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION =>
            {
                self.arena.get_class(parent).map(|c| c.name) == Some(ident_idx)
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                self.arena.get_interface(parent).map(|i| i.name) == Some(ident_idx)
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.arena.get_enum(parent).map(|e| e.name) == Some(ident_idx)
            }
            k if k == syntax_kind_ext::ENUM_MEMBER => {
                self.arena.get_enum_member(parent).map(|m| m.name) == Some(ident_idx)
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                self.arena.get_type_alias(parent).map(|t| t.name) == Some(ident_idx)
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                self.arena.get_method_decl(parent).map(|m| m.name) == Some(ident_idx)
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                self.arena.get_property_decl(parent).map(|p| p.name) == Some(ident_idx)
            }
            k if k == syntax_kind_ext::PROPERTY_SIGNATURE => {
                self.arena.get_signature(parent).map(|s| s.name) == Some(ident_idx)
            }
            k if k == syntax_kind_ext::METHOD_SIGNATURE => {
                self.arena.get_signature(parent).map(|s| s.name) == Some(ident_idx)
            }
            k if k == syntax_kind_ext::PARAMETER => {
                self.arena.get_parameter(parent).map(|p| p.name) == Some(ident_idx)
            }
            k if k == syntax_kind_ext::TYPE_PARAMETER => {
                self.arena.get_type_parameter(parent).map(|t| t.name) == Some(ident_idx)
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                self.arena.get_accessor(parent).map(|a| a.name) == Some(ident_idx)
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.arena.get_module(parent).map(|m| m.name) == Some(ident_idx)
            }
            k if k == syntax_kind_ext::IMPORT_SPECIFIER
                || k == syntax_kind_ext::EXPORT_SPECIFIER =>
            {
                self.arena.get_specifier(parent).map(|s| s.name) == Some(ident_idx)
            }
            k if k == syntax_kind_ext::IMPORT_CLAUSE => {
                self.arena.get_import_clause(parent).map(|c| c.name) == Some(ident_idx)
            }
            k if k == syntax_kind_ext::NAMESPACE_IMPORT
                || k == syntax_kind_ext::NAMESPACE_EXPORT =>
            {
                self.arena.get_named_imports(parent).map(|n| n.name) == Some(ident_idx)
            }
            _ => false,
        };

        if is_name_of_decl {
            self.binder.get_node_symbol(parent_idx)
        } else {
            None
        }
    }

    /// Get contextual modifiers based on the declaration context (e.g., const -> readonly).
    fn get_contextual_modifiers(&self, ident_idx: NodeIndex, symbol: &Symbol) -> u32 {
        let mut modifiers = 0u32;

        // Check for const variable -> READONLY modifier
        if symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0 {
            if self.is_const_variable(ident_idx) {
                modifiers |= semantic_token_modifiers::READONLY;
            }
        }

        // Check for exported symbol
        if symbol.is_exported || symbol.flags & symbol_flags::EXPORT_VALUE != 0 {
            modifiers |= semantic_token_modifiers::DEFAULT_LIBRARY; // Using DEFAULT_LIBRARY as export indicator
        }

        // Check for async function/method via extended node modifier flags
        if let Some(ext) = self.arena.get_extended(ident_idx) {
            let parent_idx = ext.parent;
            if let Some(parent_ext) = self.arena.get_extended(parent_idx) {
                let mf = parent_ext.modifier_flags;
                if mf & crate::parser::flags::modifier_flags::ASYNC != 0 {
                    modifiers |= semantic_token_modifiers::ASYNC;
                }
                if mf & crate::parser::flags::modifier_flags::DEPRECATED != 0 {
                    modifiers |= semantic_token_modifiers::DEPRECATED;
                }
            }
        }

        modifiers
    }

    /// Get contextual modifiers for a reference (not declaration).
    fn get_contextual_modifiers_for_ref(&self, symbol: &Symbol) -> u32 {
        let mut modifiers = 0u32;

        // Check for const variable -> READONLY modifier
        if symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0 {
            // Check the declaration to see if it's const
            if let Some(decl_idx) = symbol.declarations.first() {
                if let Some(decl_node) = self.arena.get(*decl_idx) {
                    if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                        // Walk up to the variable declaration list to check for CONST flag
                        if let Some(ext) = self.arena.get_extended(*decl_idx) {
                            let parent_idx = ext.parent;
                            if let Some(parent_node) = self.arena.get(parent_idx) {
                                if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                                    if (parent_node.flags as u32 & node_flags::CONST) != 0 {
                                        modifiers |= semantic_token_modifiers::READONLY;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        modifiers
    }

    /// Check if the identifier is in a `const` declaration.
    fn is_const_variable(&self, ident_idx: NodeIndex) -> bool {
        // ident -> VariableDeclaration -> VariableDeclarationList
        let Some(ext) = self.arena.get_extended(ident_idx) else {
            return false;
        };
        let var_decl_idx = ext.parent;
        let Some(var_decl_ext) = self.arena.get_extended(var_decl_idx) else {
            return false;
        };
        let decl_list_idx = var_decl_ext.parent;
        let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
            return false;
        };
        if decl_list_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return (decl_list_node.flags as u32 & node_flags::CONST) != 0;
        }
        false
    }

    /// Check if an identifier is the name child of a TYPE_PARAMETER node.
    fn is_type_parameter_name(&self, ident_idx: NodeIndex) -> bool {
        let Some(ext) = self.arena.get_extended(ident_idx) else {
            return false;
        };
        let parent_idx = ext.parent;
        let Some(parent) = self.arena.get(parent_idx) else {
            return false;
        };
        if parent.kind == syntax_kind_ext::TYPE_PARAMETER {
            if let Some(tp) = self.arena.get_type_parameter(parent) {
                return tp.name == ident_idx;
            }
        }
        false
    }

    /// Check if an identifier is a type reference to a type parameter.
    /// This checks if the identifier text matches any type parameter in scope.
    fn is_type_parameter_reference(&self, ident_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(ident_idx) else {
            return false;
        };
        let Some(ident_data) = self.arena.get_identifier(node) else {
            return false;
        };
        let name = &ident_data.escaped_text;

        // Walk up the tree to find enclosing function/class/interface with type parameters
        let mut current = ident_idx;
        loop {
            let Some(ext) = self.arena.get_extended(current) else {
                break;
            };
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let Some(parent) = self.arena.get(parent_idx) else {
                break;
            };

            // Check if parent has type parameters matching this name
            let type_params = match parent.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION =>
                {
                    self.arena
                        .get_function(parent)
                        .and_then(|f| f.type_parameters.as_ref())
                }
                k if k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION =>
                {
                    self.arena
                        .get_class(parent)
                        .and_then(|c| c.type_parameters.as_ref())
                }
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => self
                    .arena
                    .get_interface(parent)
                    .and_then(|i| i.type_parameters.as_ref()),
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => self
                    .arena
                    .get_type_alias(parent)
                    .and_then(|t| t.type_parameters.as_ref()),
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .arena
                    .get_method_decl(parent)
                    .and_then(|m| m.type_parameters.as_ref()),
                _ => None,
            };

            if let Some(tp_list) = type_params {
                for &tp_idx in &tp_list.nodes {
                    if let Some(tp_node) = self.arena.get(tp_idx) {
                        if let Some(tp_data) = self.arena.get_type_parameter(tp_node) {
                            if let Some(tp_name_node) = self.arena.get(tp_data.name) {
                                if let Some(tp_ident) = self.arena.get_identifier(tp_name_node) {
                                    if tp_ident.escaped_text == *name {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            current = parent_idx;
        }
        false
    }

    /// Check if a node kind is a modifier keyword.
    fn is_modifier(&self, kind: u16) -> bool {
        matches!(
            SyntaxKind::try_from_u16(kind),
            Some(
                SyntaxKind::PublicKeyword
                    | SyntaxKind::PrivateKeyword
                    | SyntaxKind::ProtectedKeyword
                    | SyntaxKind::StaticKeyword
                    | SyntaxKind::ReadonlyKeyword
                    | SyntaxKind::AbstractKeyword
                    | SyntaxKind::AsyncKeyword
                    | SyntaxKind::ExportKeyword
                    | SyntaxKind::DefaultKeyword
                    | SyntaxKind::ConstKeyword
                    | SyntaxKind::DeclareKeyword
                    | SyntaxKind::OverrideKeyword
            )
        )
    }

    /// Emit a semantic token for a specific node.
    fn emit_token_for_node(
        &mut self,
        node_idx: NodeIndex,
        token_type: SemanticTokenType,
        modifiers: u32,
    ) {
        let Some(node) = self.arena.get(node_idx) else {
            return;
        };
        let pos = self.line_map.offset_to_position(node.pos, self.source_text);
        let length = node.end - node.pos;
        self.builder
            .push(pos.line, pos.character, length, token_type, modifiers);
    }

    /// Emit a semantic token at a node's position.
    fn emit_token_at(
        &mut self,
        node_idx: NodeIndex,
        token_type: SemanticTokenType,
        modifiers: u32,
    ) {
        self.emit_token_for_node(node_idx, token_type, modifiers);
    }

    /// Visit all children of a node using the generic get_children traversal.
    fn visit_children(&mut self, node_idx: NodeIndex) {
        let children = self.arena.get_children(node_idx);
        for child in children {
            self.visit_node(child);
        }
    }

    /// Map a symbol to a semantic token type and modifiers.
    fn map_symbol_to_token(&self, symbol: &Symbol) -> (SemanticTokenType, u32) {
        let flags = symbol.flags;
        let mut modifiers = 0;

        // Add modifiers based on symbol flags
        if flags & symbol_flags::STATIC != 0 {
            modifiers |= semantic_token_modifiers::STATIC;
        }
        if flags & symbol_flags::ABSTRACT != 0 {
            modifiers |= semantic_token_modifiers::ABSTRACT;
        }

        // Determine token type based on symbol flags (ordered by specificity)
        let token_type = if flags & symbol_flags::CLASS != 0 {
            SemanticTokenType::Class
        } else if flags & symbol_flags::INTERFACE != 0 {
            SemanticTokenType::Interface
        } else if flags & symbol_flags::ENUM != 0 {
            SemanticTokenType::Enum
        } else if flags & symbol_flags::ENUM_MEMBER != 0 {
            SemanticTokenType::EnumMember
        } else if flags & symbol_flags::TYPE_ALIAS != 0 {
            SemanticTokenType::Type
        } else if flags & symbol_flags::TYPE_PARAMETER != 0 {
            SemanticTokenType::TypeParameter
        } else if flags & symbol_flags::FUNCTION != 0 {
            SemanticTokenType::Function
        } else if flags & symbol_flags::METHOD != 0 {
            SemanticTokenType::Method
        } else if flags & symbol_flags::GET_ACCESSOR != 0 || flags & symbol_flags::SET_ACCESSOR != 0
        {
            SemanticTokenType::Property
        } else if flags & symbol_flags::PROPERTY != 0 {
            SemanticTokenType::Property
        } else if flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0 {
            // Check if it's a parameter
            if let Some(decl_idx) = symbol.declarations.first() {
                if let Some(decl_node) = self.arena.get(*decl_idx) {
                    if decl_node.kind == syntax_kind_ext::PARAMETER {
                        return (SemanticTokenType::Parameter, modifiers);
                    }
                }
            }
            SemanticTokenType::Variable
        } else if flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0 {
            SemanticTokenType::Variable
        } else if flags & symbol_flags::VALUE_MODULE != 0
            || flags & symbol_flags::NAMESPACE_MODULE != 0
        {
            SemanticTokenType::Namespace
        } else if flags & symbol_flags::ALIAS != 0 {
            // Import alias - try to determine the underlying type from the name
            // For now, classify as Variable (the editor can refine via type info)
            SemanticTokenType::Variable
        } else {
            // Default to variable for unknown types
            SemanticTokenType::Variable
        };

        (token_type, modifiers)
    }
}

#[cfg(test)]
mod semantic_tokens_tests {
    use super::*;
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    /// Helper to parse source, bind, and compute semantic tokens.
    fn get_tokens(source: &str) -> Vec<u32> {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let mut provider = SemanticTokensProvider::new(arena, &binder, &line_map, source);
        provider.get_semantic_tokens(root)
    }

    /// Helper to decode delta-encoded tokens into absolute (line, col, len, type, modifiers).
    fn decode_tokens(data: &[u32]) -> Vec<(u32, u32, u32, u32, u32)> {
        let mut result = Vec::new();
        let mut line = 0u32;
        let mut col = 0u32;
        for chunk in data.chunks_exact(5) {
            let delta_line = chunk[0];
            let delta_col = chunk[1];
            let length = chunk[2];
            let token_type = chunk[3];
            let modifiers = chunk[4];

            if delta_line > 0 {
                line += delta_line;
                col = delta_col;
            } else {
                col += delta_col;
            }
            result.push((line, col, length, token_type, modifiers));
        }
        result
    }

    /// Find a token by its position (line, col). Returns (type, modifiers).
    fn find_token_at(
        tokens: &[(u32, u32, u32, u32, u32)],
        line: u32,
        col: u32,
    ) -> Option<(u32, u32)> {
        tokens
            .iter()
            .find(|t| t.0 == line && t.1 == col)
            .map(|t| (t.3, t.4))
    }

    #[test]
    fn test_semantic_tokens_basic() {
        let source = "const x = 1;\nfunction foo() {}\nclass Bar {}";
        let tokens = get_tokens(source);

        // Should have tokens (5 values per token)
        assert!(!tokens.is_empty(), "Should have semantic tokens");
        assert_eq!(tokens.len() % 5, 0, "Token array should be divisible by 5");

        // Should have at least 3 tokens (x, foo, Bar)
        assert!(
            tokens.len() >= 15,
            "Should have at least 3 tokens (15 values)"
        );
    }

    #[test]
    fn test_semantic_tokens_function() {
        let source = "function myFunc() {}";
        let tokens = get_tokens(source);
        let decoded = decode_tokens(&tokens);

        // Find the function name token (at column 9, "myFunc")
        let func_token = find_token_at(&decoded, 0, 9);
        assert!(func_token.is_some(), "Should have token for myFunc");
        let (token_type, modifiers) = func_token.unwrap();
        assert_eq!(token_type, SemanticTokenType::Function as u32);
        assert_ne!(
            modifiers & semantic_token_modifiers::DECLARATION,
            0,
            "Should have DECLARATION modifier"
        );
    }

    #[test]
    fn test_semantic_tokens_class() {
        let source = "class MyClass {}";
        let tokens = get_tokens(source);
        let decoded = decode_tokens(&tokens);

        // Find the class name token (at column 6, "MyClass")
        let class_token = find_token_at(&decoded, 0, 6);
        assert!(class_token.is_some(), "Should have token for MyClass");
        let (token_type, modifiers) = class_token.unwrap();
        assert_eq!(token_type, SemanticTokenType::Class as u32);
        assert_ne!(modifiers & semantic_token_modifiers::DECLARATION, 0);
    }

    #[test]
    fn test_semantic_tokens_delta_encoding() {
        let source = "const a = 1;\nconst b = 2;";
        let tokens = get_tokens(source);

        // Should have at least 2 tokens (a and b)
        assert!(
            tokens.len() >= 10,
            "Should have at least 2 tokens (10 values)"
        );

        // First token: deltaLine=0, deltaStart=6 (position of 'a')
        assert_eq!(tokens[0], 0); // deltaLine (first token always 0)
        assert_eq!(tokens[1], 6); // deltaStart (position of 'a')

        // Second token: deltaLine=1 (next line), deltaStart=6 (position of 'b')
        assert_eq!(tokens[5], 1); // deltaLine (moved to next line)
        assert_eq!(tokens[6], 6); // deltaStart (absolute position on new line)
    }

    #[test]
    fn test_semantic_tokens_interface() {
        let source = "interface IFoo { bar: string; }";
        let tokens = get_tokens(source);
        let decoded = decode_tokens(&tokens);

        // Interface name at col 10
        let iface_token = find_token_at(&decoded, 0, 10);
        assert!(iface_token.is_some(), "Should have token for IFoo");
        let (token_type, _) = iface_token.unwrap();
        assert_eq!(token_type, SemanticTokenType::Interface as u32);
    }

    #[test]
    fn test_semantic_tokens_enum() {
        let source = "enum Color { Red, Green, Blue }";
        let tokens = get_tokens(source);
        let decoded = decode_tokens(&tokens);

        // Enum name "Color" at col 5
        let enum_token = find_token_at(&decoded, 0, 5);
        assert!(enum_token.is_some(), "Should have token for Color");
        let (token_type, _) = enum_token.unwrap();
        assert_eq!(token_type, SemanticTokenType::Enum as u32);

        // Enum members should be EnumMember
        let red_token = find_token_at(&decoded, 0, 13);
        assert!(red_token.is_some(), "Should have token for Red");
        let (token_type, _) = red_token.unwrap();
        assert_eq!(token_type, SemanticTokenType::EnumMember as u32);
    }

    #[test]
    fn test_semantic_tokens_type_alias() {
        let source = "type MyType = string | number;";
        let tokens = get_tokens(source);
        let decoded = decode_tokens(&tokens);

        // Type alias name "MyType" at col 5
        let type_token = find_token_at(&decoded, 0, 5);
        assert!(type_token.is_some(), "Should have token for MyType");
        let (token_type, modifiers) = type_token.unwrap();
        assert_eq!(token_type, SemanticTokenType::Type as u32);
        assert_ne!(modifiers & semantic_token_modifiers::DECLARATION, 0);
    }

    #[test]
    fn test_semantic_tokens_parameter() {
        let source = "function greet(name: string) {}";
        let tokens = get_tokens(source);
        let decoded = decode_tokens(&tokens);

        // Parameter "name" at col 15
        let param_token = find_token_at(&decoded, 0, 15);
        assert!(
            param_token.is_some(),
            "Should have token for parameter 'name'"
        );
        let (token_type, modifiers) = param_token.unwrap();
        assert_eq!(token_type, SemanticTokenType::Parameter as u32);
        assert_ne!(modifiers & semantic_token_modifiers::DECLARATION, 0);
    }

    #[test]
    fn test_semantic_tokens_type_parameter() {
        let source = "function identity<T>(x: T): T { return x; }";
        let tokens = get_tokens(source);
        let decoded = decode_tokens(&tokens);

        // Type parameter "T" at col 18
        let tp_token = find_token_at(&decoded, 0, 18);
        assert!(tp_token.is_some(), "Should have token for type parameter T");
        let (token_type, _) = tp_token.unwrap();
        assert_eq!(token_type, SemanticTokenType::TypeParameter as u32);
    }

    #[test]
    fn test_semantic_tokens_const_readonly_modifier() {
        let source = "const PI = 3.14;";
        let tokens = get_tokens(source);
        let decoded = decode_tokens(&tokens);

        // Variable "PI" at col 6
        let var_token = find_token_at(&decoded, 0, 6);
        assert!(var_token.is_some(), "Should have token for PI");
        let (token_type, modifiers) = var_token.unwrap();
        assert_eq!(token_type, SemanticTokenType::Variable as u32);
        assert_ne!(
            modifiers & semantic_token_modifiers::READONLY,
            0,
            "const variable should have READONLY modifier"
        );
        assert_ne!(modifiers & semantic_token_modifiers::DECLARATION, 0);
    }

    #[test]
    fn test_semantic_tokens_let_variable_no_readonly() {
        let source = "let mutable = 1;";
        let tokens = get_tokens(source);
        let decoded = decode_tokens(&tokens);

        // Variable "mutable" at col 4
        let var_token = find_token_at(&decoded, 0, 4);
        assert!(var_token.is_some(), "Should have token for 'mutable'");
        let (token_type, modifiers) = var_token.unwrap();
        assert_eq!(token_type, SemanticTokenType::Variable as u32);
        assert_eq!(
            modifiers & semantic_token_modifiers::READONLY,
            0,
            "let variable should NOT have READONLY modifier"
        );
    }

    #[test]
    fn test_semantic_tokens_namespace() {
        let source = "namespace MyNS { export const x = 1; }";
        let tokens = get_tokens(source);
        let decoded = decode_tokens(&tokens);

        // Namespace "MyNS" at col 10
        let ns_token = find_token_at(&decoded, 0, 10);
        assert!(ns_token.is_some(), "Should have token for MyNS");
        let (token_type, _) = ns_token.unwrap();
        assert_eq!(token_type, SemanticTokenType::Namespace as u32);
    }

    #[test]
    fn test_semantic_tokens_variable_reference() {
        let source = "const x = 1;\nconst y = x;";
        let tokens = get_tokens(source);
        let decoded = decode_tokens(&tokens);

        // 'x' declaration at (0, 6)
        let x_decl = find_token_at(&decoded, 0, 6);
        assert!(x_decl.is_some(), "Should have declaration token for x");
        let (tt, m) = x_decl.unwrap();
        assert_eq!(tt, SemanticTokenType::Variable as u32);
        assert_ne!(m & semantic_token_modifiers::DECLARATION, 0);

        // 'y' declaration at (1, 6)
        let y_decl = find_token_at(&decoded, 1, 6);
        assert!(y_decl.is_some(), "Should have declaration token for y");

        // 'x' reference at (1, 10) - used in initializer
        let x_ref = find_token_at(&decoded, 1, 10);
        assert!(x_ref.is_some(), "Should have reference token for x");
        let (tt, m) = x_ref.unwrap();
        assert_eq!(tt, SemanticTokenType::Variable as u32);
        // Reference should NOT have DECLARATION modifier
        assert_eq!(
            m & semantic_token_modifiers::DECLARATION,
            0,
            "Reference should not have DECLARATION modifier"
        );
    }

    #[test]
    fn test_semantic_tokens_function_call_reference() {
        let source = "function add(a: number, b: number) { return a + b; }\nadd(1, 2);";
        let tokens = get_tokens(source);
        let decoded = decode_tokens(&tokens);

        // 'add' declaration at (0, 9)
        let add_decl = find_token_at(&decoded, 0, 9);
        assert!(add_decl.is_some(), "Should have declaration token for add");
        let (tt, m) = add_decl.unwrap();
        assert_eq!(tt, SemanticTokenType::Function as u32);
        assert_ne!(m & semantic_token_modifiers::DECLARATION, 0);

        // 'add' reference at (1, 0) in call expression
        let add_ref = find_token_at(&decoded, 1, 0);
        assert!(
            add_ref.is_some(),
            "Should have reference token for add call"
        );
        let (tt, _m) = add_ref.unwrap();
        assert_eq!(tt, SemanticTokenType::Function as u32);
    }

    #[test]
    fn test_semantic_tokens_class_method_property() {
        let source = "class Foo {\n  bar: number;\n  baz() {}\n}";
        let tokens = get_tokens(source);
        let decoded = decode_tokens(&tokens);

        // Class name "Foo" at (0, 6)
        let class_token = find_token_at(&decoded, 0, 6);
        assert!(class_token.is_some(), "Should have token for Foo");
        assert_eq!(class_token.unwrap().0, SemanticTokenType::Class as u32);

        // Property "bar" at (1, 2)
        let prop_token = find_token_at(&decoded, 1, 2);
        assert!(prop_token.is_some(), "Should have token for property bar");
        assert_eq!(prop_token.unwrap().0, SemanticTokenType::Property as u32);

        // Method "baz" at (2, 2)
        let method_token = find_token_at(&decoded, 2, 2);
        assert!(method_token.is_some(), "Should have token for method baz");
        assert_eq!(method_token.unwrap().0, SemanticTokenType::Method as u32);
    }

    #[test]
    fn test_semantic_tokens_multiple_declarations_same_line() {
        let source = "let a = 1, b = 2;";
        let tokens = get_tokens(source);
        let decoded = decode_tokens(&tokens);

        // 'a' at (0, 4)
        let a_token = find_token_at(&decoded, 0, 4);
        assert!(a_token.is_some(), "Should have token for a");
        assert_eq!(a_token.unwrap().0, SemanticTokenType::Variable as u32);

        // 'b' at (0, 11)
        let b_token = find_token_at(&decoded, 0, 11);
        assert!(b_token.is_some(), "Should have token for b");
        assert_eq!(b_token.unwrap().0, SemanticTokenType::Variable as u32);
    }

    #[test]
    fn test_semantic_tokens_expression_statement_reference() {
        let source = "const x = 1;\nx;";
        let tokens = get_tokens(source);
        let decoded = decode_tokens(&tokens);

        // 'x' reference at (1, 0)
        let x_ref = find_token_at(&decoded, 1, 0);
        assert!(
            x_ref.is_some(),
            "Should have reference token for x in expression statement"
        );
        assert_eq!(x_ref.unwrap().0, SemanticTokenType::Variable as u32);
    }

    #[test]
    fn test_semantic_tokens_parameter_reference_in_body() {
        let source = "function f(x: number) { return x; }";
        let tokens = get_tokens(source);
        let decoded = decode_tokens(&tokens);

        // 'x' parameter declaration at (0, 11)
        let x_decl = find_token_at(&decoded, 0, 11);
        assert!(
            x_decl.is_some(),
            "Should have declaration token for parameter x"
        );
        let (tt, m) = x_decl.unwrap();
        assert_eq!(tt, SemanticTokenType::Parameter as u32);
        assert_ne!(m & semantic_token_modifiers::DECLARATION, 0);

        // 'x' reference at (0, 31) in return statement
        let x_ref = find_token_at(&decoded, 0, 31);
        assert!(
            x_ref.is_some(),
            "Should have reference token for x in return"
        );
        assert_eq!(x_ref.unwrap().0, SemanticTokenType::Parameter as u32);
    }

    #[test]
    fn test_semantic_tokens_static_modifier() {
        let source = "class C {\n  static count = 0;\n}";
        let tokens = get_tokens(source);
        let decoded = decode_tokens(&tokens);

        // "static" keyword as Modifier token
        let static_token = find_token_at(&decoded, 1, 2);
        assert!(
            static_token.is_some(),
            "Should have token for static keyword"
        );
        assert_eq!(static_token.unwrap().0, SemanticTokenType::Modifier as u32);

        // "count" property with STATIC modifier
        let count_token = find_token_at(&decoded, 1, 9);
        assert!(count_token.is_some(), "Should have token for count");
        let (tt, m) = count_token.unwrap();
        assert_eq!(tt, SemanticTokenType::Property as u32);
        assert_ne!(
            m & semantic_token_modifiers::STATIC,
            0,
            "Should have STATIC modifier"
        );
    }

    #[test]
    fn test_semantic_tokens_enum_with_values() {
        let source = "enum Direction {\n  Up = 1,\n  Down = 2,\n}";
        let tokens = get_tokens(source);
        let decoded = decode_tokens(&tokens);

        // Enum "Direction" at (0, 5)
        let dir_token = find_token_at(&decoded, 0, 5);
        assert!(dir_token.is_some(), "Should have token for Direction");
        assert_eq!(dir_token.unwrap().0, SemanticTokenType::Enum as u32);

        // Enum member "Up" at (1, 2)
        let up_token = find_token_at(&decoded, 1, 2);
        assert!(up_token.is_some(), "Should have token for Up");
        assert_eq!(up_token.unwrap().0, SemanticTokenType::EnumMember as u32);

        // Enum member "Down" at (2, 2)
        let down_token = find_token_at(&decoded, 2, 2);
        assert!(down_token.is_some(), "Should have token for Down");
        assert_eq!(down_token.unwrap().0, SemanticTokenType::EnumMember as u32);
    }
}
