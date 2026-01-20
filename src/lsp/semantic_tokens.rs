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

use crate::binder::{Symbol, symbol_flags};
use crate::lsp::position::LineMap;
use crate::parser::node::NodeArena;
use crate::parser::{NodeIndex, syntax_kind_ext};
use crate::scanner::SyntaxKind;
use crate::binder::BinderState;

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
        std::mem::replace(&mut self.builder, SemanticTokensBuilder::new()).build()
    }

    /// Visit a node and its children recursively.
    fn visit_node(&mut self, node_idx: NodeIndex) {
        let Some(node) = self.arena.get(node_idx) else {
            return;
        };

        // Handle modifiers (keywords like public, private, static, readonly)
        if self.is_modifier(node.kind) {
            self.emit_token_for_node(node_idx, SemanticTokenType::Modifier, 0);
            return;
        }

        // Handle identifiers in decorator context
        if self.in_decorator && node.kind == SyntaxKind::Identifier as u16 {
            self.emit_token_for_node(node_idx, SemanticTokenType::Decorator, 0);
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

        // Check if this declaration node has a symbol
        if let Some(sym_id) = self.binder.get_node_symbol(node_idx) {
            if let Some(symbol) = self.binder.get_symbol(sym_id) {
                // This is a declaration - emit token for its name
                self.emit_token_for_declaration(node_idx, symbol);
            }
        }

        // Recurse into children
        self.visit_children(node_idx);
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

    /// Visit all children of a node in document order.
    fn visit_children(&mut self, node_idx: NodeIndex) {
        let Some(node) = self.arena.get(node_idx) else {
            return;
        };

        // Don't recurse into identifiers - they're already handled
        if node.kind == SyntaxKind::Identifier as u16 {
            return;
        }

        match node.kind {
            k if k == syntax_kind_ext::SOURCE_FILE => {
                if let Some(sf) = self.arena.get_source_file(node) {
                    for &stmt in &sf.statements.nodes {
                        self.visit_node(stmt);
                    }
                }
            }
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = self.arena.get_block(node) {
                    for &stmt in &block.statements.nodes {
                        self.visit_node(stmt);
                    }
                }
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var) = self.arena.get_variable(node) {
                    if let Some(modifiers) = &var.modifiers {
                        for &mod_idx in &modifiers.nodes {
                            self.visit_node(mod_idx);
                        }
                    }
                    for &decl_list in &var.declarations.nodes {
                        self.visit_node(decl_list);
                    }
                }
            }
            k if k == syntax_kind_ext::VARIABLE_DECLARATION_LIST => {
                if let Some(list) = self.arena.get_variable(node) {
                    for &decl in &list.declarations.nodes {
                        self.visit_node(decl);
                    }
                }
            }
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                if let Some(decl) = self.arena.get_variable_declaration(node) {
                    self.visit_node(decl.name);
                    if !decl.type_annotation.is_none() {
                        self.visit_node(decl.type_annotation);
                    }
                    if !decl.initializer.is_none() {
                        self.visit_node(decl.initializer);
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = self.arena.get_function(node) {
                    if let Some(modifiers) = &func.modifiers {
                        for &mod_idx in &modifiers.nodes {
                            self.visit_node(mod_idx);
                        }
                    }
                    if !func.name.is_none() {
                        self.visit_node(func.name);
                    }
                    if let Some(type_params) = &func.type_parameters {
                        for &param in &type_params.nodes {
                            self.visit_node(param);
                        }
                    }
                    for &param in &func.parameters.nodes {
                        self.visit_node(param);
                    }
                    if !func.type_annotation.is_none() {
                        self.visit_node(func.type_annotation);
                    }
                    if !func.body.is_none() {
                        self.visit_node(func.body);
                    }
                }
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = self.arena.get_class(node) {
                    if let Some(modifiers) = &class.modifiers {
                        for &mod_idx in &modifiers.nodes {
                            self.visit_node(mod_idx);
                        }
                    }
                    if !class.name.is_none() {
                        self.visit_node(class.name);
                    }
                    if let Some(type_params) = &class.type_parameters {
                        for &param in &type_params.nodes {
                            self.visit_node(param);
                        }
                    }
                    if let Some(heritage) = &class.heritage_clauses {
                        for &clause in &heritage.nodes {
                            self.visit_node(clause);
                        }
                    }
                    for &member in &class.members.nodes {
                        self.visit_node(member);
                    }
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.arena.get_method_decl(node) {
                    if let Some(modifiers) = &method.modifiers {
                        for &mod_idx in &modifiers.nodes {
                            self.visit_node(mod_idx);
                        }
                    }
                    if !method.name.is_none() {
                        self.visit_node(method.name);
                    }
                    if let Some(type_params) = &method.type_parameters {
                        for &param in &type_params.nodes {
                            self.visit_node(param);
                        }
                    }
                    for &param in &method.parameters.nodes {
                        self.visit_node(param);
                    }
                    if !method.type_annotation.is_none() {
                        self.visit_node(method.type_annotation);
                    }
                    if !method.body.is_none() {
                        self.visit_node(method.body);
                    }
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(node) {
                    self.visit_node(call.expression);
                    if let Some(args) = &call.arguments {
                        for &arg in &args.nodes {
                            self.visit_node(arg);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.arena.get_binary_expr(node) {
                    self.visit_node(bin.left);
                    self.visit_node(bin.right);
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    self.visit_node(access.expression);
                    self.visit_node(access.name_or_argument);
                }
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                if let Some(iface) = self.arena.get_interface(node) {
                    if let Some(modifiers) = &iface.modifiers {
                        for &mod_idx in &modifiers.nodes {
                            self.visit_node(mod_idx);
                        }
                    }
                    if !iface.name.is_none() {
                        self.visit_node(iface.name);
                    }
                    if let Some(type_params) = &iface.type_parameters {
                        for &param in &type_params.nodes {
                            self.visit_node(param);
                        }
                    }
                    if let Some(heritage) = &iface.heritage_clauses {
                        for &clause in &heritage.nodes {
                            self.visit_node(clause);
                        }
                    }
                    for &member in &iface.members.nodes {
                        self.visit_node(member);
                    }
                }
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = self.arena.get_enum(node) {
                    if let Some(modifiers) = &enum_decl.modifiers {
                        for &mod_idx in &modifiers.nodes {
                            self.visit_node(mod_idx);
                        }
                    }
                    if !enum_decl.name.is_none() {
                        self.visit_node(enum_decl.name);
                    }
                    for &member in &enum_decl.members.nodes {
                        self.visit_node(member);
                    }
                }
            }
            k if k == syntax_kind_ext::ENUM_MEMBER => {
                if let Some(member) = self.arena.get_enum_member(node) {
                    if !member.name.is_none() {
                        self.visit_node(member.name);
                    }
                    if !member.initializer.is_none() {
                        self.visit_node(member.initializer);
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                if let Some(alias) = self.arena.get_type_alias(node) {
                    if let Some(modifiers) = &alias.modifiers {
                        for &mod_idx in &modifiers.nodes {
                            self.visit_node(mod_idx);
                        }
                    }
                    if !alias.name.is_none() {
                        self.visit_node(alias.name);
                    }
                    if let Some(type_params) = &alias.type_parameters {
                        for &param in &type_params.nodes {
                            self.visit_node(param);
                        }
                    }
                    if !alias.type_node.is_none() {
                        self.visit_node(alias.type_node);
                    }
                }
            }
            k if k == syntax_kind_ext::DECORATOR => {
                if let Some(decorator) = self.arena.get_decorator(node) {
                    self.visit_node(decorator.expression);
                }
            }
            k if k == syntax_kind_ext::TYPE_PARAMETER => {
                if let Some(param) = self.arena.get_type_parameter(node) {
                    self.visit_node(param.name);
                    if !param.constraint.is_none() {
                        self.visit_node(param.constraint);
                    }
                    if !param.default.is_none() {
                        self.visit_node(param.default);
                    }
                }
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                if let Some(prop) = self.arena.get_property_decl(node) {
                    if let Some(modifiers) = &prop.modifiers {
                        for &mod_idx in &modifiers.nodes {
                            self.visit_node(mod_idx);
                        }
                    }
                    self.visit_node(prop.name);
                    if !prop.type_annotation.is_none() {
                        self.visit_node(prop.type_annotation);
                    }
                    if !prop.initializer.is_none() {
                        self.visit_node(prop.initializer);
                    }
                }
            }
            k if k == syntax_kind_ext::PARAMETER => {
                if let Some(param) = self.arena.get_parameter(node) {
                    if let Some(modifiers) = &param.modifiers {
                        for &mod_idx in &modifiers.nodes {
                            self.visit_node(mod_idx);
                        }
                    }
                    self.visit_node(param.name);
                    if !param.type_annotation.is_none() {
                        self.visit_node(param.type_annotation);
                    }
                    if !param.initializer.is_none() {
                        self.visit_node(param.initializer);
                    }
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    if let Some(modifiers) = &accessor.modifiers {
                        for &mod_idx in &modifiers.nodes {
                            self.visit_node(mod_idx);
                        }
                    }
                    self.visit_node(accessor.name);
                    for &param in &accessor.parameters.nodes {
                        self.visit_node(param);
                    }
                    if !accessor.body.is_none() {
                        self.visit_node(accessor.body);
                    }
                }
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                if let Some(ctor) = self.arena.get_constructor(node) {
                    if let Some(modifiers) = &ctor.modifiers {
                        for &mod_idx in &modifiers.nodes {
                            self.visit_node(mod_idx);
                        }
                    }
                    for &param in &ctor.parameters.nodes {
                        self.visit_node(param);
                    }
                    if !ctor.body.is_none() {
                        self.visit_node(ctor.body);
                    }
                }
            }
            _ => {
                // For other node types, we don't need special traversal
            }
        }
    }

    /// Emit a semantic token for a declaration.
    ///
    /// Extracts the name identifier from the declaration and emits a token for it.
    fn emit_token_for_declaration(&mut self, decl_idx: NodeIndex, symbol: &Symbol) {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return;
        };

        // Extract the name identifier from the declaration
        let name_idx = match decl_node.kind {
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => self
                .arena
                .get_variable_declaration(decl_node)
                .map(|v| v.name),
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.arena.get_function(decl_node).map(|f| f.name)
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.arena.get_class(decl_node).map(|c| c.name)
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                self.arena.get_interface(decl_node).map(|i| i.name)
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.arena.get_enum(decl_node).map(|e| e.name)
            }
            k if k == syntax_kind_ext::ENUM_MEMBER => {
                self.arena.get_enum_member(decl_node).map(|m| m.name)
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                self.arena.get_type_alias(decl_node).map(|t| t.name)
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                self.arena.get_method_decl(decl_node).map(|m| m.name)
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                self.arena.get_property_decl(decl_node).map(|p| p.name)
            }
            k if k == syntax_kind_ext::PARAMETER => {
                self.arena.get_parameter(decl_node).map(|p| p.name)
            }
            k if k == syntax_kind_ext::TYPE_PARAMETER => {
                self.arena.get_type_parameter(decl_node).map(|t| t.name)
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                self.arena.get_accessor(decl_node).map(|a| a.name)
            }
            _ => None,
        };

        if let Some(name_idx) = name_idx {
            if !name_idx.is_none() {
                self.emit_token_for_name(name_idx, symbol, true);
            }
        }
    }

    /// Emit a semantic token for a name identifier.
    fn emit_token_for_name(&mut self, node_idx: NodeIndex, symbol: &Symbol, is_declaration: bool) {
        let Some(node) = self.arena.get(node_idx) else {
            return;
        };

        let pos = self.line_map.offset_to_position(node.pos, self.source_text);
        let length = node.end - node.pos;

        let (token_type, mut modifiers) = self.map_symbol_to_token(symbol);

        if is_declaration {
            modifiers |= semantic_token_modifiers::DECLARATION;
        }

        self.builder
            .push(pos.line, pos.character, length, token_type, modifiers);
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

        // Determine token type based on symbol flags
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
        } else if flags & symbol_flags::PROPERTY != 0 {
            SemanticTokenType::Property
        } else if flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0 {
            // Check if it's a parameter
            if symbol.value_declaration.is_some() {
                if let Some(decl_node) = self.arena.get(symbol.value_declaration) {
                    if decl_node.kind == syntax_kind_ext::PARAMETER {
                        return (SemanticTokenType::Parameter, modifiers);
                    }
                }
            }
            SemanticTokenType::Variable
        } else if flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0 {
            SemanticTokenType::Variable
        } else if flags & symbol_flags::NAMESPACE_MODULE != 0 {
            SemanticTokenType::Namespace
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

    #[test]
    fn test_semantic_tokens_basic() {
        let source = "const x = 1;\nfunction foo() {}\nclass Bar {}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let mut provider = SemanticTokensProvider::new(arena, &binder, &line_map, source);

        let tokens = provider.get_semantic_tokens(root);

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
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let mut provider = SemanticTokensProvider::new(arena, &binder, &line_map, source);

        let tokens = provider.get_semantic_tokens(root);

        // Should have exactly 1 token (myFunc - the function name)
        assert_eq!(tokens.len(), 5, "Should have 1 token (5 values)");

        // Check token type is Function (12)
        assert_eq!(tokens[3], SemanticTokenType::Function as u32);

        // Check DECLARATION modifier is set (bit 0)
        assert_eq!(
            tokens[4] & semantic_token_modifiers::DECLARATION,
            semantic_token_modifiers::DECLARATION
        );
    }

    #[test]
    fn test_semantic_tokens_class() {
        let source = "class MyClass {}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let mut provider = SemanticTokensProvider::new(arena, &binder, &line_map, source);

        let tokens = provider.get_semantic_tokens(root);

        // Should have exactly 1 token (MyClass)
        assert_eq!(tokens.len(), 5, "Should have 1 token (5 values)");

        // Check token type is Class (2)
        assert_eq!(tokens[3], SemanticTokenType::Class as u32);
    }

    #[test]
    fn test_semantic_tokens_delta_encoding() {
        let source = "const a = 1;\nconst b = 2;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let mut provider = SemanticTokensProvider::new(arena, &binder, &line_map, source);

        let tokens = provider.get_semantic_tokens(root);

        // Should have 2 tokens (a and b)
        assert_eq!(tokens.len(), 10, "Should have 2 tokens (10 values)");

        // First token: deltaLine=0, deltaStart=6 (position of 'a')
        assert_eq!(tokens[0], 0); // deltaLine (first token always 0)
        assert_eq!(tokens[1], 6); // deltaStart (position of 'a')

        // Second token: deltaLine=1 (next line), deltaStart=6 (position of 'b')
        assert_eq!(tokens[5], 1); // deltaLine (moved to next line)
        assert_eq!(tokens[6], 6); // deltaStart (absolute position on new line)
    }
}
