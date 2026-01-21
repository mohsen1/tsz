//! Enum Transform Module
//!
//! Handles transformation of TypeScript enums to JavaScript including:
//! - Const enum inlining at usage sites
//! - preserveConstEnums option support
//! - ES5 IIFE pattern generation
//! - Reverse mapping for numeric enums
//!
//! # Const Enum Inlining
//!
//! When const enums are used, their values are inlined at usage sites:
//!
//! ```typescript
//! const enum Direction { Up = 1, Down = 2 }
//! let x = Direction.Up;
//! ```
//!
//! Becomes:
//!
//! ```javascript
//! let x = 1 /* Up */;
//! ```

use crate::enums::evaluator::{EnumEvaluator, EnumValue};
use crate::parser::node::NodeArena;
use crate::parser::syntax_kind_ext;
use crate::parser::{NodeIndex, NodeList};
use crate::scanner::SyntaxKind;
use rustc_hash::FxHashMap;

/// Options for enum transformation
#[derive(Debug, Clone, Default)]
pub struct EnumTransformOptions {
    /// If true, emit const enums even when they would normally be erased
    pub preserve_const_enums: bool,
    /// If true, add comments showing original member names when inlining
    pub emit_comments: bool,
    /// Target ES version (affects IIFE pattern)
    pub target_es5: bool,
}

/// Enum transformer that handles const enum inlining and emission
pub struct EnumTransformer<'a> {
    arena: &'a NodeArena,
    options: EnumTransformOptions,
    /// Cache of enum values by enum node index
    enum_value_cache: FxHashMap<NodeIndex, FxHashMap<String, EnumValue>>,
    /// Map of enum name to their declarations (for resolving references)
    enum_declarations: FxHashMap<String, NodeIndex>,
    /// Track which enums are const enums
    const_enum_names: FxHashMap<String, bool>,
}

impl<'a> EnumTransformer<'a> {
    /// Create a new enum transformer
    pub fn new(arena: &'a NodeArena) -> Self {
        EnumTransformer {
            arena,
            options: EnumTransformOptions::default(),
            enum_value_cache: FxHashMap::default(),
            enum_declarations: FxHashMap::default(),
            const_enum_names: FxHashMap::default(),
        }
    }

    /// Create with options
    pub fn with_options(arena: &'a NodeArena, options: EnumTransformOptions) -> Self {
        EnumTransformer {
            arena,
            options,
            enum_value_cache: FxHashMap::default(),
            enum_declarations: FxHashMap::default(),
            const_enum_names: FxHashMap::default(),
        }
    }

    /// Register an enum declaration for later reference resolution
    pub fn register_enum(&mut self, enum_idx: NodeIndex) {
        let Some(enum_node) = self.arena.get(enum_idx) else {
            return;
        };

        let Some(enum_data) = self.arena.get_enum(enum_node) else {
            return;
        };

        // Get enum name
        let name = self.get_identifier_text(enum_data.name);
        if name.is_empty() {
            return;
        }

        // Check if const
        let is_const = self.is_const_enum(&enum_data.modifiers);

        self.enum_declarations.insert(name.clone(), enum_idx);
        self.const_enum_names.insert(name, is_const);
    }

    /// Evaluate and cache enum values
    pub fn evaluate_enum(&mut self, enum_idx: NodeIndex) -> &FxHashMap<String, EnumValue> {
        if !self.enum_value_cache.contains_key(&enum_idx) {
            let mut evaluator = EnumEvaluator::new(self.arena);
            let values = evaluator.evaluate_enum(enum_idx);
            self.enum_value_cache.insert(enum_idx, values);
        }
        &self.enum_value_cache[&enum_idx]
    }

    /// Get a cached enum value
    pub fn get_enum_value(&self, enum_idx: NodeIndex, member_name: &str) -> Option<&EnumValue> {
        self.enum_value_cache
            .get(&enum_idx)
            .and_then(|values| values.get(member_name))
    }

    /// Check if an enum should be erased (const enum without preserveConstEnums)
    pub fn should_erase_enum(&self, enum_idx: NodeIndex) -> bool {
        let Some(enum_node) = self.arena.get(enum_idx) else {
            return false;
        };

        let Some(enum_data) = self.arena.get_enum(enum_node) else {
            return false;
        };

        // Check for ambient (declare enum) - always erased
        if self.is_ambient_enum(&enum_data.modifiers) {
            return true;
        }

        // Check for const enum
        if self.is_const_enum(&enum_data.modifiers) {
            // Erase unless preserveConstEnums is set
            return !self.options.preserve_const_enums;
        }

        false
    }

    /// Transform a property access to a const enum member
    /// Returns Some(inlined_value) if this is a const enum access, None otherwise
    pub fn try_inline_const_enum_access(
        &mut self,
        obj_name: &str,
        member_name: &str,
    ) -> Option<String> {
        // Check if this is a const enum
        if !self
            .const_enum_names
            .get(obj_name)
            .copied()
            .unwrap_or(false)
        {
            return None;
        }

        // Get the enum declaration
        let enum_idx = self.enum_declarations.get(obj_name).copied()?;

        // Ensure values are evaluated
        self.evaluate_enum(enum_idx);

        // Get the member value
        let value = self.get_enum_value(enum_idx, member_name)?;

        // Generate inlined code
        let inlined = if self.options.emit_comments {
            format!("{} /* {} */", value.to_js_literal(), member_name)
        } else {
            value.to_js_literal()
        };

        Some(inlined)
    }

    /// Transform an element access to a const enum member
    /// E.g., ConstEnum["Member"]
    pub fn try_inline_const_enum_element_access(
        &mut self,
        obj_name: &str,
        member_name: &str,
    ) -> Option<String> {
        // Same logic as property access
        self.try_inline_const_enum_access(obj_name, member_name)
    }

    /// Emit an enum declaration in ES5 format
    pub fn emit_enum_es5(&mut self, enum_idx: NodeIndex) -> String {
        let Some(enum_node) = self.arena.get(enum_idx) else {
            return String::new();
        };

        let Some(enum_data) = self.arena.get_enum(enum_node) else {
            return String::new();
        };

        // Check if should be erased
        if self.should_erase_enum(enum_idx) && !self.options.preserve_const_enums {
            return String::new();
        }

        let name = self.get_identifier_text(enum_data.name);
        if name.is_empty() {
            return String::new();
        }

        // Ensure values are evaluated
        self.evaluate_enum(enum_idx);

        let mut output = String::with_capacity(256);

        // var E;
        output.push_str("var ");
        output.push_str(&name);
        output.push_str(";\n");

        // (function (E) { ... })(E || (E = {}));
        output.push_str("(function (");
        output.push_str(&name);
        output.push_str(") {\n");

        // Emit members
        for &member_idx in &enum_data.members.nodes {
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

            // Get evaluated value
            let value = self
                .enum_value_cache
                .get(&enum_idx)
                .and_then(|v| v.get(&member_name));

            output.push_str("    ");
            output.push_str(&name);

            match value {
                Some(EnumValue::String(s)) => {
                    // String enum: E["A"] = "val";
                    output.push_str("[\"");
                    output.push_str(&member_name);
                    output.push_str("\"] = \"");
                    // Escape the string
                    output.push_str(&s.replace('\\', "\\\\").replace('"', "\\\""));
                    output.push_str("\";\n");
                }
                Some(EnumValue::Number(n)) => {
                    // Numeric enum: E[E["A"] = 0] = "A";
                    output.push('[');
                    output.push_str(&name);
                    output.push_str("[\"");
                    output.push_str(&member_name);
                    output.push_str("\"] = ");
                    output.push_str(&n.to_string());
                    output.push_str("] = \"");
                    output.push_str(&member_name);
                    output.push_str("\";\n");
                }
                _ => {
                    // Computed or unknown - emit as expression
                    output.push_str("[\"");
                    output.push_str(&member_name);
                    output.push_str("\"] = ");
                    if !member_data.initializer.is_none() {
                        output.push_str(&self.emit_expression(member_data.initializer));
                    } else {
                        output.push('0');
                    }
                    output.push_str(";\n");
                }
            }
        }

        // Close IIFE
        output.push_str("})(");
        output.push_str(&name);
        output.push_str(" || (");
        output.push_str(&name);
        output.push_str(" = {}));\n");

        output
    }

    /// Emit an expression (for computed enum values)
    fn emit_expression(&self, idx: NodeIndex) -> String {
        let Some(node) = self.arena.get(idx) else {
            return "0".to_string();
        };

        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    lit.text.clone()
                } else {
                    "0".to_string()
                }
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    format!(
                        "\"{}\"",
                        lit.text.replace('\\', "\\\\").replace('"', "\\\"")
                    )
                } else {
                    "\"\"".to_string()
                }
            }
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(ident) = self.arena.get_identifier(node) {
                    ident.escaped_text.clone()
                } else {
                    "undefined".to_string()
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.arena.get_binary_expr(node) {
                    let left = self.emit_expression(bin.left);
                    let right = self.emit_expression(bin.right);
                    let op = self.operator_to_string(bin.operator_token);
                    format!("{} {} {}", left, op, right)
                } else {
                    "0".to_string()
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr(node) {
                    let operand = self.emit_expression(unary.operand);
                    let op = self.operator_to_string(unary.operator);
                    format!("{}{}", op, operand)
                } else {
                    "0".to_string()
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.arena.get_parenthesized(node) {
                    format!("({})", self.emit_expression(paren.expression))
                } else {
                    "(0)".to_string()
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    let obj = self.emit_expression(access.expression);
                    let prop = self.emit_expression(access.name_or_argument);
                    format!("{}.{}", obj, prop)
                } else {
                    "undefined".to_string()
                }
            }
            _ => "0".to_string(),
        }
    }

    fn operator_to_string(&self, op: u16) -> &'static str {
        match op {
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
            k if k == SyntaxKind::AsteriskAsteriskToken as u16 => "**",
            _ => "/* op */",
        }
    }

    fn is_const_enum(&self, modifiers: &Option<NodeList>) -> bool {
        if let Some(mods) = modifiers {
            for &idx in &mods.nodes {
                if let Some(node) = self.arena.get(idx)
                    && node.kind == SyntaxKind::ConstKeyword as u16
                {
                    return true;
                }
            }
        }
        false
    }

    fn is_ambient_enum(&self, modifiers: &Option<NodeList>) -> bool {
        if let Some(mods) = modifiers {
            for &idx in &mods.nodes {
                if let Some(node) = self.arena.get(idx)
                    && node.kind == SyntaxKind::DeclareKeyword as u16
                {
                    return true;
                }
            }
        }
        false
    }

    fn get_identifier_text(&self, idx: NodeIndex) -> String {
        if let Some(node) = self.arena.get(idx)
            && let Some(ident) = self.arena.get_identifier(node)
        {
            return ident.escaped_text.clone();
        }
        String::new()
    }

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

    /// Check if an identifier refers to a const enum
    pub fn is_const_enum_reference(&self, name: &str) -> bool {
        self.const_enum_names.get(name).copied().unwrap_or(false)
    }

    /// Get all registered enum names
    pub fn get_enum_names(&self) -> Vec<&String> {
        self.enum_declarations.keys().collect()
    }
}

/// Inline const enum usages in a source file
pub struct ConstEnumInliner<'a> {
    transformer: EnumTransformer<'a>,
    source_text: &'a str,
}

impl<'a> ConstEnumInliner<'a> {
    pub fn new(arena: &'a NodeArena, source_text: &'a str) -> Self {
        ConstEnumInliner {
            transformer: EnumTransformer::new(arena),
            source_text,
        }
    }

    /// Register all enums from the source file
    pub fn register_enums(&mut self, source_file_idx: NodeIndex) {
        let Some(root_node) = self.transformer.arena.get(source_file_idx) else {
            return;
        };

        let Some(source_file) = self.transformer.arena.get_source_file(root_node) else {
            return;
        };

        for &stmt_idx in &source_file.statements.nodes {
            if let Some(stmt_node) = self.transformer.arena.get(stmt_idx)
                && stmt_node.kind == syntax_kind_ext::ENUM_DECLARATION
            {
                self.transformer.register_enum(stmt_idx);
            }
        }
    }

    /// Try to inline a const enum access
    pub fn try_inline(&mut self, obj_name: &str, member_name: &str) -> Option<String> {
        self.transformer
            .try_inline_const_enum_access(obj_name, member_name)
    }

    /// Check if a name refers to a const enum
    pub fn is_const_enum(&self, name: &str) -> bool {
        self.transformer.is_const_enum_reference(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::NodeIndex;
    use crate::parser::ParserState;

    fn create_parser(source: &str) -> (ParserState, NodeIndex) {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root_idx = parser.parse_source_file();
        (parser, root_idx)
    }

    #[test]
    fn test_numeric_enum_es5() {
        let (parser, root_idx) = create_parser("enum E { A, B, C }");

        if let Some(root_node) = parser.arena.get(root_idx)
            && let Some(source_file) = parser.arena.get_source_file(root_node)
            && let Some(&enum_idx) = source_file.statements.nodes.first()
        {
            let mut transformer = EnumTransformer::new(&parser.arena);
            transformer.register_enum(enum_idx);
            let output = transformer.emit_enum_es5(enum_idx);

            assert!(output.contains("var E;"));
            assert!(output.contains("(function (E)"));
            assert!(output.contains("E[E[\"A\"] = 0] = \"A\""));
            assert!(output.contains("E[E[\"B\"] = 1] = \"B\""));
            assert!(output.contains("E[E[\"C\"] = 2] = \"C\""));
        }
    }

    #[test]
    fn test_string_enum_no_reverse_mapping() {
        let (parser, root_idx) = create_parser(r#"enum S { A = "alpha", B = "beta" }"#);

        if let Some(root_node) = parser.arena.get(root_idx)
            && let Some(source_file) = parser.arena.get_source_file(root_node)
            && let Some(&enum_idx) = source_file.statements.nodes.first()
        {
            let mut transformer = EnumTransformer::new(&parser.arena);
            transformer.register_enum(enum_idx);
            let output = transformer.emit_enum_es5(enum_idx);

            // String enums should NOT have reverse mapping
            assert!(output.contains("S[\"A\"] = \"alpha\""));
            assert!(output.contains("S[\"B\"] = \"beta\""));
            assert!(
                !output.contains("S[S["),
                "String enum should not have reverse mapping"
            );
        }
    }

    #[test]
    fn test_const_enum_erased() {
        let (parser, root_idx) = create_parser("const enum CE { A = 1, B = 2 }");

        if let Some(root_node) = parser.arena.get(root_idx)
            && let Some(source_file) = parser.arena.get_source_file(root_node)
            && let Some(&enum_idx) = source_file.statements.nodes.first()
        {
            let mut transformer = EnumTransformer::new(&parser.arena);
            transformer.register_enum(enum_idx);
            let output = transformer.emit_enum_es5(enum_idx);

            assert!(output.is_empty(), "Const enum should be erased by default");
        }
    }

    #[test]
    fn test_const_enum_preserved() {
        let (parser, root_idx) = create_parser("const enum CE { A = 1, B = 2 }");

        if let Some(root_node) = parser.arena.get(root_idx)
            && let Some(source_file) = parser.arena.get_source_file(root_node)
            && let Some(&enum_idx) = source_file.statements.nodes.first()
        {
            let options = EnumTransformOptions {
                preserve_const_enums: true,
                ..Default::default()
            };
            let mut transformer = EnumTransformer::with_options(&parser.arena, options);
            transformer.register_enum(enum_idx);
            let output = transformer.emit_enum_es5(enum_idx);

            assert!(
                !output.is_empty(),
                "Const enum should be preserved with option"
            );
            assert!(output.contains("var CE;"));
        }
    }

    #[test]
    fn test_const_enum_inlining() {
        let (parser, root_idx) = create_parser("const enum Direction { Up = 1, Down = 2 }");

        if let Some(root_node) = parser.arena.get(root_idx)
            && let Some(source_file) = parser.arena.get_source_file(root_node)
            && let Some(&enum_idx) = source_file.statements.nodes.first()
        {
            let mut transformer = EnumTransformer::new(&parser.arena);
            transformer.register_enum(enum_idx);
            transformer.evaluate_enum(enum_idx);

            // Try to inline
            let inlined_up = transformer.try_inline_const_enum_access("Direction", "Up");
            let inlined_down = transformer.try_inline_const_enum_access("Direction", "Down");

            assert_eq!(inlined_up, Some("1".to_string()));
            assert_eq!(inlined_down, Some("2".to_string()));
        }
    }

    #[test]
    fn test_const_enum_inlining_with_comments() {
        let (parser, root_idx) = create_parser("const enum Flags { None = 0, Read = 1 }");

        if let Some(root_node) = parser.arena.get(root_idx)
            && let Some(source_file) = parser.arena.get_source_file(root_node)
            && let Some(&enum_idx) = source_file.statements.nodes.first()
        {
            let options = EnumTransformOptions {
                emit_comments: true,
                ..Default::default()
            };
            let mut transformer = EnumTransformer::with_options(&parser.arena, options);
            transformer.register_enum(enum_idx);
            transformer.evaluate_enum(enum_idx);

            let inlined = transformer.try_inline_const_enum_access("Flags", "Read");
            assert_eq!(inlined, Some("1 /* Read */".to_string()));
        }
    }

    #[test]
    fn test_ambient_enum_erased() {
        let (parser, root_idx) = create_parser("declare enum E { A, B }");

        if let Some(root_node) = parser.arena.get(root_idx)
            && let Some(source_file) = parser.arena.get_source_file(root_node)
            && let Some(&enum_idx) = source_file.statements.nodes.first()
        {
            let mut transformer = EnumTransformer::new(&parser.arena);
            transformer.register_enum(enum_idx);
            let output = transformer.emit_enum_es5(enum_idx);

            assert!(output.is_empty(), "Declare enum should be erased");
        }
    }

    #[test]
    fn test_computed_enum_values() {
        let (parser, root_idx) = create_parser("enum E { A = 1 << 2, B = 3 | 4 }");

        if let Some(root_node) = parser.arena.get(root_idx)
            && let Some(source_file) = parser.arena.get_source_file(root_node)
            && let Some(&enum_idx) = source_file.statements.nodes.first()
        {
            let mut transformer = EnumTransformer::new(&parser.arena);
            transformer.register_enum(enum_idx);
            let values = transformer.evaluate_enum(enum_idx);

            assert_eq!(values.get("A"), Some(&EnumValue::Number(4))); // 1 << 2
            assert_eq!(values.get("B"), Some(&EnumValue::Number(7))); // 3 | 4
        }
    }
}
