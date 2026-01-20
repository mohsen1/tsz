//! ES5 Enum Transform
//!
//! Transforms TypeScript enums to ES5 IIFE patterns.
//!
//! # Patterns
//!
//! ## Numeric Enum (with Reverse Mapping)
//! ```typescript
//! enum E { A, B = 2 }
//! ```
//! Becomes:
//! ```javascript
//! var E;
//! (function (E) {
//!     E[E["A"] = 0] = "A";
//!     E[E["B"] = 2] = "B";
//! })(E || (E = {}));
//! ```
//!
//! ## String Enum (No Reverse Mapping)
//! ```typescript
//! enum S { A = "a" }
//! ```
//! Becomes:
//! ```javascript
//! var S;
//! (function (S) {
//!     S["A"] = "a";
//! })(S || (S = {}));
//! ```
//!
//! ## Const Enum (Erased by Default)
//! ```typescript
//! const enum CE { A = 0 }
//! // usages are inlined
//! ```

use crate::parser::syntax_kind_ext;
use crate::parser::node::NodeArena;
use crate::parser::{NodeIndex, NodeList};
use crate::scanner::SyntaxKind;
use crate::transforms::emit_utils;

/// Enum ES5 emitter
pub struct EnumES5Emitter<'a> {
    arena: &'a NodeArena,
    output: String,
    indent_level: u32,
    /// Track last numeric value for auto-incrementing
    last_value: Option<i64>,
}

impl<'a> EnumES5Emitter<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        EnumES5Emitter {
            arena,
            output: String::with_capacity(1024),
            indent_level: 0,
            last_value: None,
        }
    }

    pub fn set_indent_level(&mut self, level: u32) {
        self.indent_level = level;
    }

    /// Emit an enum declaration
    /// Returns empty string for const enums (they are erased)
    pub fn emit_enum(&mut self, enum_idx: NodeIndex) -> String {
        self.output.clear();
        self.last_value = Some(-1); // Start at -1 so first increment is 0

        let Some(enum_node) = self.arena.get(enum_idx) else {
            return String::new();
        };

        let Some(enum_data) = self.arena.get_enum(enum_node) else {
            return String::new();
        };

        // Check for const enum - erase by default (preserveConstEnums not yet supported)
        if self.is_const_enum(&enum_data.modifiers) {
            return String::new();
        }

        let name = self.get_identifier_text(enum_data.name);
        if name.is_empty() {
            return String::new();
        }

        // var E;
        self.write_indent();
        self.write("var ");
        self.write(&name);
        self.write(";");
        self.write_line();

        // (function (E) { ... })(E || (E = {}));
        self.write_indent();
        self.write("(function (");
        self.write(&name);
        self.write(") {");
        self.write_line();
        self.increase_indent();

        // Emit members
        self.emit_members(&enum_data.members, &name);

        // Close IIFE
        self.decrease_indent();
        self.write_indent();
        self.write("})(");
        self.write(&name);
        self.write(" || (");
        self.write(&name);
        self.write(" = {}));");

        std::mem::take(&mut self.output)
    }

    /// Get the enum name without emitting anything
    pub fn get_enum_name(&self, enum_idx: NodeIndex) -> String {
        let Some(enum_node) = self.arena.get(enum_idx) else {
            return String::new();
        };
        let Some(enum_data) = self.arena.get_enum(enum_node) else {
            return String::new();
        };
        self.get_identifier_text(enum_data.name)
    }

    /// Check if enum is a const enum
    pub fn is_const_enum_by_idx(&self, enum_idx: NodeIndex) -> bool {
        let Some(enum_node) = self.arena.get(enum_idx) else {
            return false;
        };
        let Some(enum_data) = self.arena.get_enum(enum_node) else {
            return false;
        };
        self.is_const_enum(&enum_data.modifiers)
    }

    fn emit_members(&mut self, members: &NodeList, enum_name: &str) {
        for &member_idx in &members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let Some(member_data) = self.arena.get_enum_member(member_node) else {
                continue;
            };

            let member_name = self.get_member_name(member_data.name);
            let has_initializer = !member_data.initializer.is_none();

            self.write_indent();
            self.write(enum_name);

            if has_initializer {
                if self.is_string_literal(member_data.initializer) {
                    // String enum: E["A"] = "val";
                    // No reverse mapping for string enums
                    self.write("[\"");
                    self.write(&member_name);
                    self.write("\"] = ");
                    self.emit_expression(member_data.initializer);
                    self.write(";");

                    // Reset auto-increment - can't continue after string
                    self.last_value = None;
                } else {
                    // Numeric/Computed: E[E["A"] = val] = "A";
                    self.write("[");
                    self.write(enum_name);
                    self.write("[\"");
                    self.write(&member_name);
                    self.write("\"] = ");
                    self.emit_expression(member_data.initializer);
                    self.write("] = \"");
                    self.write(&member_name);
                    self.write("\";");

                    // Try to track value for next member
                    self.update_last_value_from_expr(member_data.initializer);
                }
            } else {
                // Auto-increment: E[E["A"] = 0] = "A";
                let next_val = self.last_value.map(|v| v + 1).unwrap_or(0);
                self.last_value = Some(next_val);

                self.write("[");
                self.write(enum_name);
                self.write("[\"");
                self.write(&member_name);
                self.write("\"] = ");
                self.write_i64(next_val);
                self.write("] = \"");
                self.write(&member_name);
                self.write("\";");
            }
            self.write_line();
        }
    }

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

    fn is_string_literal(&self, idx: NodeIndex) -> bool {
        if let Some(node) = self.arena.get(idx) {
            return node.kind == SyntaxKind::StringLiteral as u16;
        }
        false
    }

    fn update_last_value_from_expr(&mut self, idx: NodeIndex) {
        if let Some(node) = self.arena.get(idx) {
            if node.kind == SyntaxKind::NumericLiteral as u16 {
                if let Some(lit) = self.arena.get_literal(node) {
                    if let Ok(val) = lit.text.parse::<i64>() {
                        self.last_value = Some(val);
                        return;
                    }
                }
            }
        }
        // Complex expression - lose track
        self.last_value = None;
    }

    fn get_identifier_text(&self, idx: NodeIndex) -> String {
        if let Some(node) = self.arena.get(idx) {
            if let Some(ident) = self.arena.get_identifier(node) {
                return ident.escaped_text.clone();
            }
        }
        String::new()
    }

    fn get_member_name(&self, idx: NodeIndex) -> String {
        if let Some(node) = self.arena.get(idx) {
            // Can be identifier or string literal for computed names
            if let Some(ident) = self.arena.get_identifier(node) {
                return ident.escaped_text.clone();
            }
            if let Some(lit) = self.arena.get_literal(node) {
                return lit.text.clone();
            }
        }
        String::new()
    }

    fn emit_expression(&mut self, idx: NodeIndex) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    self.write(&lit.text);
                }
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    self.write("\"");
                    self.write(&lit.text);
                    self.write("\"");
                }
            }
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(id) = self.arena.get_identifier(node) {
                    self.write(&id.escaped_text);
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.arena.get_binary_expr(node) {
                    self.emit_expression(bin.left);
                    self.write(" ");
                    self.emit_operator(bin.operator_token);
                    self.write(" ");
                    self.emit_expression(bin.right);
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr(node) {
                    self.emit_operator(unary.operator);
                    self.emit_expression(unary.operand);
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.arena.get_parenthesized(node) {
                    self.write("(");
                    self.emit_expression(paren.expression);
                    self.write(")");
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                // E.A reference inside enum
                if let Some(access) = self.arena.get_access_expr(node) {
                    self.emit_expression(access.expression);
                    self.write(".");
                    self.emit_expression(access.name_or_argument);
                }
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    self.emit_expression(access.expression);
                    self.write("[");
                    self.emit_expression(access.name_or_argument);
                    self.write("]");
                }
            }
            _ => {
                // Fallback - write placeholder
                self.write("0 /* complex */");
            }
        }
    }

    fn emit_operator(&mut self, op: u16) {
        let op_str = match op {
            k if k == SyntaxKind::PlusToken as u16 => "+",
            k if k == SyntaxKind::MinusToken as u16 => "-",
            k if k == SyntaxKind::AsteriskToken as u16 => "*",
            k if k == SyntaxKind::SlashToken as u16 => "/",
            k if k == SyntaxKind::PercentToken as u16 => "%",
            k if k == SyntaxKind::LessThanLessThanToken as u16 => "<<",
            k if k == SyntaxKind::GreaterThanGreaterThanToken as u16 => ">>",
            k if k == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16 => ">>>",
            k if k == SyntaxKind::AmpersandToken as u16 => "&",
            k if k == SyntaxKind::BarToken as u16 => "|",
            k if k == SyntaxKind::CaretToken as u16 => "^",
            k if k == SyntaxKind::TildeToken as u16 => "~",
            k if k == SyntaxKind::ExclamationToken as u16 => "!",
            _ => "/* op */",
        };
        self.write(op_str);
    }

    fn write(&mut self, s: &str) {
        self.output.push_str(s);
    }

    fn write_i64(&mut self, value: i64) {
        emit_utils::push_i64(&mut self.output, value);
    }

    fn write_line(&mut self) {
        self.output.push('\n');
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent_level {
            self.output.push_str("    ");
        }
    }

    fn increase_indent(&mut self) {
        self.indent_level += 1;
    }

    fn decrease_indent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ParserState;

    fn emit_enum(source: &str) -> String {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        if let Some(root_node) = parser.arena.get(root) {
            if let Some(source_file) = parser.arena.get_source_file(root_node) {
                if let Some(&enum_idx) = source_file.statements.nodes.first() {
                    let mut emitter = EnumES5Emitter::new(&parser.arena);
                    return emitter.emit_enum(enum_idx);
                }
            }
        }
        String::new()
    }

    #[test]
    fn test_numeric_enum() {
        let output = emit_enum("enum E { A, B, C }");
        assert!(output.contains("var E;"), "Should declare var E");
        assert!(output.contains("(function (E)"), "Should have IIFE");
        assert!(
            output.contains("E[E[\"A\"] = 0] = \"A\""),
            "Should have reverse mapping for A"
        );
        assert!(
            output.contains("E[E[\"B\"] = 1] = \"B\""),
            "Should have reverse mapping for B"
        );
        assert!(
            output.contains("E[E[\"C\"] = 2] = \"C\""),
            "Should auto-increment C"
        );
    }

    #[test]
    fn test_enum_with_initializer() {
        let output = emit_enum("enum E { A = 10, B, C = 20 }");
        assert!(
            output.contains("E[E[\"A\"] = 10] = \"A\""),
            "A should be 10"
        );
        assert!(
            output.contains("E[E[\"B\"] = 11] = \"B\""),
            "B should be 11 (auto-increment)"
        );
        assert!(
            output.contains("E[E[\"C\"] = 20] = \"C\""),
            "C should be 20"
        );
    }

    #[test]
    fn test_string_enum() {
        let output = emit_enum("enum S { A = \"alpha\", B = \"beta\" }");
        assert!(output.contains("var S;"), "Should declare var S");
        assert!(
            output.contains("S[\"A\"] = \"alpha\";"),
            "String enum no reverse mapping"
        );
        assert!(
            output.contains("S[\"B\"] = \"beta\";"),
            "String enum no reverse mapping"
        );
        // Should NOT contain reverse mapping pattern
        assert!(
            !output.contains("S[S["),
            "String enums should not have reverse mapping"
        );
    }

    #[test]
    fn test_const_enum_erased() {
        let output = emit_enum("const enum CE { A = 0 }");
        assert!(
            output.trim().is_empty(),
            "Const enums should be erased: {}",
            output
        );
    }
}
