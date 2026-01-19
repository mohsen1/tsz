//! Enum Value Evaluator
//!
//! Evaluates enum member values at compile time, supporting:
//! - Numeric literals
//! - String literals
//! - Binary expressions (+, -, *, /, %, <<, >>, >>>, &, |, ^)
//! - Unary expressions (+, -, ~)
//! - References to previous enum members
//! - Parenthesized expressions
//!
//! # Example
//!
//! ```typescript
//! enum E {
//!     A = 1,
//!     B = A + 1,           // evaluates to 2
//!     C = A | B,           // evaluates to 3
//!     D = ~A,              // evaluates to -2
//!     E = (A + B) * 2,     // evaluates to 6
//! }
//! ```

use crate::parser::syntax_kind_ext;
use crate::parser::thin_node::ThinNodeArena;
use crate::parser::NodeIndex;
use crate::scanner::SyntaxKind;
use rustc_hash::FxHashMap;

/// Represents an evaluated enum value
#[derive(Debug, Clone, PartialEq)]
pub enum EnumValue {
    /// Numeric value (integer)
    Number(i64),
    /// String value
    String(String),
    /// Value could not be evaluated at compile time
    Computed,
}

impl EnumValue {
    /// Check if this is a numeric value
    pub fn is_number(&self) -> bool {
        matches!(self, EnumValue::Number(_))
    }

    /// Check if this is a string value
    pub fn is_string(&self) -> bool {
        matches!(self, EnumValue::String(_))
    }

    /// Get the numeric value if available
    pub fn as_number(&self) -> Option<i64> {
        match self {
            EnumValue::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// Get the string value if available
    pub fn as_string(&self) -> Option<&str> {
        match self {
            EnumValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Convert to JavaScript literal representation
    pub fn to_js_literal(&self) -> String {
        match self {
            EnumValue::Number(n) => n.to_string(),
            EnumValue::String(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
            EnumValue::Computed => "0 /* computed */".to_string(),
        }
    }
}

/// Evaluates enum member values at compile time
pub struct EnumEvaluator<'a> {
    arena: &'a ThinNodeArena,
    /// Map of enum member names to their evaluated values within current enum
    member_values: FxHashMap<String, EnumValue>,
    /// The current enum name (for self-references like `E.A`)
    current_enum_name: Option<String>,
}

impl<'a> EnumEvaluator<'a> {
    /// Create a new enum evaluator
    pub fn new(arena: &'a ThinNodeArena) -> Self {
        EnumEvaluator {
            arena,
            member_values: FxHashMap::default(),
            current_enum_name: None,
        }
    }

    /// Set the current enum name for resolving self-references
    pub fn set_current_enum(&mut self, name: &str) {
        self.current_enum_name = Some(name.to_string());
    }

    /// Register a member value for use in subsequent member evaluations
    pub fn register_member(&mut self, name: &str, value: EnumValue) {
        self.member_values.insert(name.to_string(), value);
    }

    /// Clear all registered members (call when starting a new enum)
    pub fn clear(&mut self) {
        self.member_values.clear();
        self.current_enum_name = None;
    }

    /// Get a previously registered member value
    pub fn get_member(&self, name: &str) -> Option<&EnumValue> {
        self.member_values.get(name)
    }

    /// Evaluate all members of an enum declaration
    /// Returns a map of member name to evaluated value
    pub fn evaluate_enum(&mut self, enum_idx: NodeIndex) -> FxHashMap<String, EnumValue> {
        self.clear();

        let Some(enum_node) = self.arena.get(enum_idx) else {
            return FxHashMap::default();
        };

        let Some(enum_data) = self.arena.get_enum(enum_node) else {
            return FxHashMap::default();
        };

        // Get enum name
        if let Some(name_node) = self.arena.get(enum_data.name) {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                self.set_current_enum(&ident.escaped_text);
            }
        }

        // Track the last numeric value for auto-increment
        // None means we haven't seen any numeric value yet (start at 0)
        // Some(-1) sentinel would mean previous was non-numeric (string/computed)
        let mut last_numeric_value: Option<i64> = None;
        // Track if we've seen a non-numeric initializer (string or computed)
        let mut had_non_numeric = false;

        for &member_idx in &enum_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            let Some(member_data) = self.arena.get_enum_member(member_node) else {
                continue;
            };

            // Get member name
            let member_name = self.get_member_name(member_data.name);
            if member_name.is_empty() {
                continue;
            }

            // Evaluate the member
            let value = if member_data.initializer.is_none() {
                // Auto-increment from last numeric value
                if had_non_numeric {
                    // Previous member was non-numeric (string/computed)
                    // This is a TypeScript error - member needs explicit initializer
                    EnumValue::Computed
                } else {
                    // Either first member or previous was numeric
                    let next_val = last_numeric_value.map(|v| v + 1).unwrap_or(0);
                    last_numeric_value = Some(next_val);
                    EnumValue::Number(next_val)
                }
            } else {
                let evaluated = self.evaluate_expression(member_data.initializer);
                // Update auto-increment tracker if numeric
                if let EnumValue::Number(n) = &evaluated {
                    last_numeric_value = Some(*n);
                    had_non_numeric = false; // Reset - can auto-increment again
                } else {
                    // String or computed - can't auto-increment after
                    last_numeric_value = None;
                    had_non_numeric = true;
                }
                evaluated
            };

            self.register_member(&member_name, value);
        }

        self.member_values.clone()
    }

    /// Evaluate an expression and return its compile-time value
    pub fn evaluate_expression(&self, idx: NodeIndex) -> EnumValue {
        let Some(node) = self.arena.get(idx) else {
            return EnumValue::Computed;
        };

        match node.kind {
            // Numeric literal
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    self.parse_numeric_literal(&lit.text)
                } else {
                    EnumValue::Computed
                }
            }

            // String literal
            k if k == SyntaxKind::StringLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    EnumValue::String(lit.text.clone())
                } else {
                    EnumValue::Computed
                }
            }

            // Identifier - might be a reference to another enum member
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(ident) = self.arena.get_identifier(node) {
                    // Check if it's a reference to a known member
                    if let Some(value) = self.member_values.get(&ident.escaped_text) {
                        return value.clone();
                    }
                }
                EnumValue::Computed
            }

            // Binary expression
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.arena.get_binary_expr(node) {
                    self.evaluate_binary(bin.left, bin.operator_token, bin.right)
                } else {
                    EnumValue::Computed
                }
            }

            // Prefix unary expression
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr(node) {
                    self.evaluate_unary(unary.operator, unary.operand)
                } else {
                    EnumValue::Computed
                }
            }

            // Parenthesized expression
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.arena.get_parenthesized(node) {
                    self.evaluate_expression(paren.expression)
                } else {
                    EnumValue::Computed
                }
            }

            // Property access - E.A or SomeEnum.Member
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    self.evaluate_property_access(access.expression, access.name_or_argument)
                } else {
                    EnumValue::Computed
                }
            }

            // Template literal (no substitutions)
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    EnumValue::String(lit.text.clone())
                } else {
                    EnumValue::Computed
                }
            }

            _ => EnumValue::Computed,
        }
    }

    /// Parse a numeric literal string to i64
    fn parse_numeric_literal(&self, text: &str) -> EnumValue {
        let text = text.trim();

        // Handle hex literals
        if text.starts_with("0x") || text.starts_with("0X") {
            if let Ok(val) = i64::from_str_radix(&text[2..], 16) {
                return EnumValue::Number(val);
            }
        }
        // Handle octal literals
        else if text.starts_with("0o") || text.starts_with("0O") {
            if let Ok(val) = i64::from_str_radix(&text[2..], 8) {
                return EnumValue::Number(val);
            }
        }
        // Handle binary literals
        else if text.starts_with("0b") || text.starts_with("0B") {
            if let Ok(val) = i64::from_str_radix(&text[2..], 2) {
                return EnumValue::Number(val);
            }
        }
        // Handle legacy octal (starting with 0)
        else if text.starts_with('0') && text.len() > 1 && !text.contains('.') {
            if let Ok(val) = i64::from_str_radix(&text[1..], 8) {
                return EnumValue::Number(val);
            }
        }

        // Try parsing as decimal integer
        if let Ok(val) = text.parse::<i64>() {
            return EnumValue::Number(val);
        }

        // Try parsing as float and truncate
        if let Ok(val) = text.parse::<f64>() {
            return EnumValue::Number(val as i64);
        }

        EnumValue::Computed
    }

    /// Evaluate a binary expression
    fn evaluate_binary(&self, left: NodeIndex, op: u16, right: NodeIndex) -> EnumValue {
        let left_val = self.evaluate_expression(left);
        let right_val = self.evaluate_expression(right);

        // Both must be numbers for binary operations (except + for strings)
        match (&left_val, &right_val) {
            (EnumValue::Number(l), EnumValue::Number(r)) => {
                let result = match op {
                    k if k == SyntaxKind::PlusToken as u16 => l.wrapping_add(*r),
                    k if k == SyntaxKind::MinusToken as u16 => l.wrapping_sub(*r),
                    k if k == SyntaxKind::AsteriskToken as u16 => l.wrapping_mul(*r),
                    k if k == SyntaxKind::SlashToken as u16 => {
                        if *r == 0 {
                            return EnumValue::Computed;
                        }
                        l / r
                    }
                    k if k == SyntaxKind::PercentToken as u16 => {
                        if *r == 0 {
                            return EnumValue::Computed;
                        }
                        l % r
                    }
                    k if k == SyntaxKind::LessThanLessThanToken as u16 => {
                        l.wrapping_shl((*r as u32) & 31)
                    }
                    k if k == SyntaxKind::GreaterThanGreaterThanToken as u16 => {
                        l.wrapping_shr((*r as u32) & 31)
                    }
                    k if k == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16 => {
                        ((*l as u64).wrapping_shr((*r as u32) & 31)) as i64
                    }
                    k if k == SyntaxKind::AmpersandToken as u16 => l & r,
                    k if k == SyntaxKind::BarToken as u16 => l | r,
                    k if k == SyntaxKind::CaretToken as u16 => l ^ r,
                    k if k == SyntaxKind::AsteriskAsteriskToken as u16 => {
                        l.wrapping_pow(*r as u32)
                    }
                    _ => return EnumValue::Computed,
                };
                EnumValue::Number(result)
            }
            // String concatenation
            (EnumValue::String(l), EnumValue::String(r))
                if op == SyntaxKind::PlusToken as u16 =>
            {
                EnumValue::String(format!("{}{}", l, r))
            }
            _ => EnumValue::Computed,
        }
    }

    /// Evaluate a unary expression
    fn evaluate_unary(&self, op: u16, operand: NodeIndex) -> EnumValue {
        let operand_val = self.evaluate_expression(operand);

        match operand_val {
            EnumValue::Number(n) => {
                let result = match op {
                    k if k == SyntaxKind::PlusToken as u16 => n,
                    k if k == SyntaxKind::MinusToken as u16 => -n,
                    k if k == SyntaxKind::TildeToken as u16 => !n,
                    _ => return EnumValue::Computed,
                };
                EnumValue::Number(result)
            }
            _ => EnumValue::Computed,
        }
    }

    /// Evaluate a property access expression (E.A)
    fn evaluate_property_access(&self, obj: NodeIndex, prop: NodeIndex) -> EnumValue {
        // Get the object name
        let Some(obj_node) = self.arena.get(obj) else {
            return EnumValue::Computed;
        };

        let obj_name = if let Some(ident) = self.arena.get_identifier(obj_node) {
            ident.escaped_text.clone()
        } else {
            return EnumValue::Computed;
        };

        // Get the property name
        let Some(prop_node) = self.arena.get(prop) else {
            return EnumValue::Computed;
        };

        let prop_name = if let Some(ident) = self.arena.get_identifier(prop_node) {
            ident.escaped_text.clone()
        } else {
            return EnumValue::Computed;
        };

        // Check if this is a self-reference (E.A within enum E)
        if let Some(ref current_enum) = self.current_enum_name {
            if &obj_name == current_enum {
                if let Some(value) = self.member_values.get(&prop_name) {
                    return value.clone();
                }
            }
        }

        EnumValue::Computed
    }

    /// Get member name from a node (identifier or string literal)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thin_parser::ThinParserState;

    fn evaluate_enum(source: &str) -> FxHashMap<String, EnumValue> {
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        if let Some(root_node) = parser.arena.get(root) {
            if let Some(source_file) = parser.arena.get_source_file(root_node) {
                if let Some(&enum_idx) = source_file.statements.nodes.first() {
                    let mut evaluator = EnumEvaluator::new(&parser.arena);
                    return evaluator.evaluate_enum(enum_idx);
                }
            }
        }
        FxHashMap::default()
    }

    #[test]
    fn test_numeric_enum_auto_increment() {
        let values = evaluate_enum("enum E { A, B, C }");
        assert_eq!(values.get("A"), Some(&EnumValue::Number(0)));
        assert_eq!(values.get("B"), Some(&EnumValue::Number(1)));
        assert_eq!(values.get("C"), Some(&EnumValue::Number(2)));
    }

    #[test]
    fn test_numeric_enum_explicit_values() {
        let values = evaluate_enum("enum E { A = 10, B, C = 20, D }");
        assert_eq!(values.get("A"), Some(&EnumValue::Number(10)));
        assert_eq!(values.get("B"), Some(&EnumValue::Number(11)));
        assert_eq!(values.get("C"), Some(&EnumValue::Number(20)));
        assert_eq!(values.get("D"), Some(&EnumValue::Number(21)));
    }

    #[test]
    fn test_string_enum() {
        let values = evaluate_enum(r#"enum E { A = "alpha", B = "beta" }"#);
        assert_eq!(
            values.get("A"),
            Some(&EnumValue::String("alpha".to_string()))
        );
        assert_eq!(
            values.get("B"),
            Some(&EnumValue::String("beta".to_string()))
        );
    }

    #[test]
    fn test_computed_binary_expression() {
        let values = evaluate_enum("enum E { A = 1, B = 2, C = A + B }");
        assert_eq!(values.get("A"), Some(&EnumValue::Number(1)));
        assert_eq!(values.get("B"), Some(&EnumValue::Number(2)));
        assert_eq!(values.get("C"), Some(&EnumValue::Number(3)));
    }

    #[test]
    fn test_bitwise_operations() {
        let values = evaluate_enum("enum E { A = 1, B = 2, C = A | B, D = A & B, E = A ^ B }");
        assert_eq!(values.get("A"), Some(&EnumValue::Number(1)));
        assert_eq!(values.get("B"), Some(&EnumValue::Number(2)));
        assert_eq!(values.get("C"), Some(&EnumValue::Number(3))); // 1 | 2
        assert_eq!(values.get("D"), Some(&EnumValue::Number(0))); // 1 & 2
        assert_eq!(values.get("E"), Some(&EnumValue::Number(3))); // 1 ^ 2
    }

    #[test]
    fn test_unary_operators() {
        let values = evaluate_enum("enum E { A = 5, B = -A, C = ~A }");
        assert_eq!(values.get("A"), Some(&EnumValue::Number(5)));
        assert_eq!(values.get("B"), Some(&EnumValue::Number(-5)));
        assert_eq!(values.get("C"), Some(&EnumValue::Number(!5)));
    }

    #[test]
    fn test_shift_operators() {
        let values = evaluate_enum("enum E { A = 1 << 4, B = 16 >> 2, C = -16 >>> 2 }");
        assert_eq!(values.get("A"), Some(&EnumValue::Number(16)));
        assert_eq!(values.get("B"), Some(&EnumValue::Number(4)));
        // Unsigned right shift
        let expected_c = ((-16i64 as u64) >> 2) as i64;
        assert_eq!(values.get("C"), Some(&EnumValue::Number(expected_c)));
    }

    #[test]
    fn test_parenthesized_expression() {
        let values = evaluate_enum("enum E { A = (1 + 2) * 3 }");
        assert_eq!(values.get("A"), Some(&EnumValue::Number(9)));
    }

    #[test]
    fn test_self_reference() {
        let values = evaluate_enum("enum E { A = 1, B = E.A + 1 }");
        assert_eq!(values.get("A"), Some(&EnumValue::Number(1)));
        assert_eq!(values.get("B"), Some(&EnumValue::Number(2)));
    }

    #[test]
    fn test_hex_literal() {
        let values = evaluate_enum("enum E { A = 0xFF, B = 0x10 }");
        assert_eq!(values.get("A"), Some(&EnumValue::Number(255)));
        assert_eq!(values.get("B"), Some(&EnumValue::Number(16)));
    }

    #[test]
    fn test_mixed_string_breaks_auto_increment() {
        let values = evaluate_enum(r#"enum E { A = 1, B = "b", C }"#);
        assert_eq!(values.get("A"), Some(&EnumValue::Number(1)));
        assert_eq!(values.get("B"), Some(&EnumValue::String("b".to_string())));
        // After string, auto-increment fails - C becomes computed or error
        // In TypeScript, this is actually an error, but we produce Computed
        assert!(matches!(values.get("C"), Some(EnumValue::Computed) | None));
    }

    #[test]
    fn test_enum_value_to_js_literal() {
        assert_eq!(EnumValue::Number(42).to_js_literal(), "42");
        assert_eq!(EnumValue::Number(-5).to_js_literal(), "-5");
        assert_eq!(
            EnumValue::String("hello".to_string()).to_js_literal(),
            "\"hello\""
        );
        assert_eq!(
            EnumValue::String("say \"hi\"".to_string()).to_js_literal(),
            "\"say \\\"hi\\\"\""
        );
    }

    #[test]
    fn test_const_enum_values() {
        let values = evaluate_enum("const enum E { A = 1, B = 2, C = A | B }");
        assert_eq!(values.get("A"), Some(&EnumValue::Number(1)));
        assert_eq!(values.get("B"), Some(&EnumValue::Number(2)));
        assert_eq!(values.get("C"), Some(&EnumValue::Number(3)));
    }
}
