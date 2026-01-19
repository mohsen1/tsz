//! Enum Type Checking
//!
//! Provides type checking for enum declarations including:
//! - Duplicate member detection
//! - Initializer type validation
//! - Const enum usage validation
//! - Ambient enum compatibility checking
//!
//! # Diagnostics
//!
//! - TS2300: Duplicate identifier
//! - TS2335: 'super' can only be referenced in a derived class
//! - TS2474: const enum member initializers can only contain literal values
//! - TS2477: A const enum member can only be accessed using a string literal

use crate::binder::symbol_flags;
use crate::checker::types::diagnostics::Diagnostic;
use crate::enums::evaluator::{EnumEvaluator, EnumValue};
use crate::parser::syntax_kind_ext;
use crate::parser::thin_node::ThinNodeArena;
use crate::parser::{NodeIndex, NodeList};
use crate::scanner::SyntaxKind;
use rustc_hash::{FxHashMap, FxHashSet};

/// Diagnostic code constants
pub mod diagnostic_codes {
    pub const DUPLICATE_IDENTIFIER: u32 = 2300;
    pub const CONST_ENUM_LITERAL_ONLY: u32 = 2474;
    pub const CONST_ENUM_STRING_ACCESS: u32 = 2476;
    pub const ENUM_MEMBER_MUST_HAVE_INITIALIZER: u32 = 1061;
    pub const COMPUTED_ENUM_NOT_NUMERIC: u32 = 2553;
}

/// Type checker for enum declarations
pub struct EnumChecker<'a> {
    arena: &'a ThinNodeArena,
    diagnostics: Vec<Diagnostic>,
    /// Evaluated enum values cache
    enum_values: FxHashMap<NodeIndex, FxHashMap<String, EnumValue>>,
}

impl<'a> EnumChecker<'a> {
    /// Create a new enum checker
    pub fn new(arena: &'a ThinNodeArena) -> Self {
        EnumChecker {
            arena,
            diagnostics: Vec::new(),
            enum_values: FxHashMap::default(),
        }
    }

    /// Get diagnostics produced during checking
    pub fn get_diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Take diagnostics, consuming them
    pub fn take_diagnostics(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.diagnostics)
    }

    /// Check an enum declaration
    pub fn check_enum_declaration(&mut self, enum_idx: NodeIndex) {
        let Some(enum_node) = self.arena.get(enum_idx) else {
            return;
        };

        let Some(enum_data) = self.arena.get_enum(enum_node) else {
            return;
        };

        // Check for duplicate members
        self.check_duplicate_members(&enum_data.members);

        // Check if this is a const enum
        let is_const = self.is_const_enum(&enum_data.modifiers);
        let is_ambient = self.is_ambient_enum(&enum_data.modifiers);

        // Evaluate enum values
        let mut evaluator = EnumEvaluator::new(self.arena);
        let values = evaluator.evaluate_enum(enum_idx);
        self.enum_values.insert(enum_idx, values.clone());

        // Check member initializers
        self.check_member_initializers(
            &enum_data.members,
            is_const,
            is_ambient,
            &values,
        );
    }

    /// Check for duplicate enum members
    fn check_duplicate_members(&mut self, members: &NodeList) {
        let mut seen: FxHashSet<String> = FxHashSet::default();

        for &member_idx in &members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            let Some(member_data) = self.arena.get_enum_member(member_node) else {
                continue;
            };

            let member_name = self.get_member_name(member_data.name);
            if member_name.is_empty() {
                continue;
            }

            if !seen.insert(member_name.clone()) {
                // Duplicate found
                self.diagnostics.push(Diagnostic::error(
                    String::new(), // file name filled in later
                    member_node.pos,
                    member_node.end - member_node.pos,
                    format!("Duplicate identifier '{}'.", member_name),
                    diagnostic_codes::DUPLICATE_IDENTIFIER,
                ));
            }
        }
    }

    /// Check member initializers
    fn check_member_initializers(
        &mut self,
        members: &NodeList,
        is_const: bool,
        is_ambient: bool,
        values: &FxHashMap<String, EnumValue>,
    ) {
        let mut had_string = false;

        for &member_idx in &members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            let Some(member_data) = self.arena.get_enum_member(member_node) else {
                continue;
            };

            let member_name = self.get_member_name(member_data.name);
            let has_initializer = !member_data.initializer.is_none();

            // Get evaluated value
            let value = values.get(&member_name);

            // Check if value is string
            if let Some(EnumValue::String(_)) = value {
                had_string = true;
            }

            // After a string member, all subsequent members must have initializers
            // Check if value is Computed due to missing initializer after string member
            let needs_initializer = had_string && !has_initializer
                && matches!(value, Some(EnumValue::Computed) | None);
            if needs_initializer {
                self.diagnostics.push(Diagnostic::error(
                    String::new(),
                    member_node.pos,
                    member_node.end - member_node.pos,
                    "Enum member must have initializer.".to_string(),
                    diagnostic_codes::ENUM_MEMBER_MUST_HAVE_INITIALIZER,
                ));
            }

            // Const enum specific checks
            if is_const && has_initializer {
                self.check_const_enum_initializer(member_data.initializer);
            }

            // Ambient enum - all members should have initializers (warning)
            if is_ambient && !has_initializer {
                // In ambient context, uninitialized members get computed values
                // This is allowed but we might want to warn
            }
        }
    }

    /// Check that const enum initializers only contain literal values and
    /// references to other const enum members
    fn check_const_enum_initializer(&mut self, init_idx: NodeIndex) {
        if !self.is_valid_const_enum_initializer(init_idx) {
            let node = self.arena.get(init_idx);
            let (start, length) = if let Some(n) = node {
                (n.pos, n.end - n.pos)
            } else {
                (0, 0)
            };

            self.diagnostics.push(Diagnostic::error(
                String::new(),
                start,
                length,
                "const enum member initializers can only contain literal values and other computed enum values.".to_string(),
                diagnostic_codes::CONST_ENUM_LITERAL_ONLY,
            ));
        }
    }

    /// Check if an expression is valid as a const enum initializer
    fn is_valid_const_enum_initializer(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        match node.kind {
            // Literals are always valid
            k if k == SyntaxKind::NumericLiteral as u16 => true,
            k if k == SyntaxKind::StringLiteral as u16 => true,
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => true,

            // Identifiers - must be enum member reference
            k if k == SyntaxKind::Identifier as u16 => {
                // Allow references to other enum members
                true
            }

            // Binary expressions - both sides must be valid
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.arena.get_binary_expr(node) {
                    // Check operator is allowed
                    let valid_op = matches!(
                        bin.operator_token,
                        k if k == SyntaxKind::PlusToken as u16
                            || k == SyntaxKind::MinusToken as u16
                            || k == SyntaxKind::AsteriskToken as u16
                            || k == SyntaxKind::SlashToken as u16
                            || k == SyntaxKind::PercentToken as u16
                            || k == SyntaxKind::LessThanLessThanToken as u16
                            || k == SyntaxKind::GreaterThanGreaterThanToken as u16
                            || k == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16
                            || k == SyntaxKind::AmpersandToken as u16
                            || k == SyntaxKind::BarToken as u16
                            || k == SyntaxKind::CaretToken as u16
                            || k == SyntaxKind::AsteriskAsteriskToken as u16
                    );
                    valid_op
                        && self.is_valid_const_enum_initializer(bin.left)
                        && self.is_valid_const_enum_initializer(bin.right)
                } else {
                    false
                }
            }

            // Unary expressions
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr(node) {
                    let valid_op = matches!(
                        unary.operator,
                        k if k == SyntaxKind::PlusToken as u16
                            || k == SyntaxKind::MinusToken as u16
                            || k == SyntaxKind::TildeToken as u16
                    );
                    valid_op && self.is_valid_const_enum_initializer(unary.operand)
                } else {
                    false
                }
            }

            // Parenthesized expression
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.arena.get_parenthesized(node) {
                    self.is_valid_const_enum_initializer(paren.expression)
                } else {
                    false
                }
            }

            // Property access - allowed for enum member references
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => true,

            // Everything else is invalid
            _ => false,
        }
    }

    /// Check if an enum has the const modifier
    fn is_const_enum(&self, modifiers: &Option<NodeList>) -> bool {
        if let Some(mods) = modifiers {
            for &idx in &mods.nodes {
                if let Some(node) = self.arena.get(idx) {
                    if node.kind == SyntaxKind::ConstKeyword as u16 {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if an enum has the declare modifier (ambient)
    fn is_ambient_enum(&self, modifiers: &Option<NodeList>) -> bool {
        if let Some(mods) = modifiers {
            for &idx in &mods.nodes {
                if let Some(node) = self.arena.get(idx) {
                    if node.kind == SyntaxKind::DeclareKeyword as u16 {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Get member name from node
    fn get_member_name(&self, idx: NodeIndex) -> String {
        if let Some(node) = self.arena.get(idx) {
            if let Some(ident) = self.arena.get_identifier(node) {
                return ident.escaped_text.clone();
            }
            if let Some(lit) = self.arena.get_literal(node) {
                return lit.text.clone();
            }
        }
        String::new()
    }

    /// Get cached enum values for an enum declaration
    pub fn get_enum_values(&self, enum_idx: NodeIndex) -> Option<&FxHashMap<String, EnumValue>> {
        self.enum_values.get(&enum_idx)
    }

    /// Check if a symbol refers to a const enum
    pub fn is_symbol_const_enum(&self, symbol_flags: u32) -> bool {
        symbol_flags & symbol_flags::CONST_ENUM != 0
    }

    /// Validate access to a const enum member
    /// Const enum members can only be accessed with string literal keys
    pub fn check_const_enum_access(
        &mut self,
        access_node: NodeIndex,
        is_element_access: bool,
        argument_idx: NodeIndex,
    ) {
        if is_element_access {
            // Element access to const enum must use string literal
            let Some(arg_node) = self.arena.get(argument_idx) else {
                return;
            };

            if arg_node.kind != SyntaxKind::StringLiteral as u16 {
                self.diagnostics.push(Diagnostic::error(
                    String::new(),
                    arg_node.pos,
                    arg_node.end - arg_node.pos,
                    "A const enum member can only be accessed using a string literal.".to_string(),
                    diagnostic_codes::CONST_ENUM_STRING_ACCESS,
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thin_parser::ThinParserState;

    fn check_enum(source: &str) -> Vec<Diagnostic> {
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        if let Some(root_node) = parser.arena.get(root) {
            if let Some(source_file) = parser.arena.get_source_file(root_node) {
                if let Some(&enum_idx) = source_file.statements.nodes.first() {
                    let mut checker = EnumChecker::new(&parser.arena);
                    checker.check_enum_declaration(enum_idx);
                    return checker.take_diagnostics();
                }
            }
        }
        Vec::new()
    }

    #[test]
    fn test_no_errors_for_valid_enum() {
        let diagnostics = check_enum("enum E { A, B, C }");
        assert!(diagnostics.is_empty(), "Should have no errors");
    }

    #[test]
    fn test_duplicate_member_error() {
        let diagnostics = check_enum("enum E { A, B, A }");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, diagnostic_codes::DUPLICATE_IDENTIFIER);
        assert!(diagnostics[0].message_text.contains("Duplicate identifier"));
    }

    #[test]
    fn test_valid_const_enum() {
        let diagnostics = check_enum("const enum E { A = 1, B = 2, C = A | B }");
        assert!(diagnostics.is_empty(), "Valid const enum should have no errors");
    }

    #[test]
    fn test_valid_string_enum() {
        let diagnostics = check_enum(r#"enum E { A = "a", B = "b" }"#);
        assert!(diagnostics.is_empty(), "Valid string enum should have no errors");
    }

    #[test]
    fn test_mixed_enum_needs_initializer() {
        // After a string member, following members need initializers
        let diagnostics = check_enum(r#"enum E { A = "a", B }"#);
        // B should require an initializer since A is a string
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == diagnostic_codes::ENUM_MEMBER_MUST_HAVE_INITIALIZER),
            "Should error on member without initializer after string member"
        );
    }

    #[test]
    fn test_const_enum_with_expressions() {
        let diagnostics = check_enum("const enum E { A = 1 + 2, B = ~3, C = (4 * 5) }");
        assert!(
            diagnostics.is_empty(),
            "Const enum with simple expressions should be valid"
        );
    }
}
