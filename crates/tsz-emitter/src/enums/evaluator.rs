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

use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Represents an evaluated enum value
#[derive(Debug, Clone, PartialEq)]
pub enum EnumValue {
    /// Numeric value (integer)
    Number(i64),
    /// Floating-point numeric value
    Float(f64),
    /// String value
    String(String),
    /// Value could not be evaluated at compile time
    Computed,
}

impl EnumValue {
    /// Convert to JavaScript literal representation
    pub fn to_js_literal(&self) -> String {
        match self {
            Self::Number(n) => n.to_string(),
            Self::Float(f) => {
                // Format float to match tsc output: 0.5, -1.5, etc.
                // Remove trailing zeros after decimal but keep at least one decimal digit
                let s = format!("{f}");
                s
            }
            Self::String(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
            Self::Computed => "0 /* computed */".to_string(),
        }
    }

    /// Returns true if this is a negative numeric value (integer or float).
    /// Used to determine if parentheses are needed (e.g., `(-1).toString()`).
    pub fn is_negative(&self) -> bool {
        match self {
            Self::Number(n) => *n < 0,
            Self::Float(f) => *f < 0.0,
            _ => false,
        }
    }

    /// Returns true if this is a non-negative integer value.
    /// Used to determine if double-dot is needed for property access
    /// (e.g., `100..toString()` instead of `100.toString()`).
    pub const fn needs_double_dot(&self) -> bool {
        matches!(self, Self::Number(n) if *n >= 0)
    }

    /// Returns the numeric value as f64, if this is a number type.
    pub const fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(n) => Some(*n as f64),
            Self::Float(f) => Some(*f),
            _ => None,
        }
    }
}

/// Evaluates enum member values at compile time
pub struct EnumEvaluator<'a> {
    arena: &'a NodeArena,
    /// Map of enum member names to their evaluated values within current enum
    member_values: FxHashMap<String, EnumValue>,
    /// The current enum name (for self-references like `E.A`)
    current_enum_name: Option<String>,
    /// The source file currently being evaluated, for resolving top-level const bindings.
    current_source_file: Option<NodeIndex>,
    /// Accumulated enum values from all previously-evaluated enums.
    /// Keyed by enum name → member name → value.
    /// Used to resolve cross-enum references like `Foo.a` in `enum Bar { B = Foo.a }`.
    all_enum_values: FxHashMap<String, FxHashMap<String, EnumValue>>,
}

impl<'a> EnumEvaluator<'a> {
    /// Create a new enum evaluator
    pub fn new(arena: &'a NodeArena) -> Self {
        EnumEvaluator {
            arena,
            member_values: FxHashMap::default(),
            current_enum_name: None,
            current_source_file: None,
            all_enum_values: FxHashMap::default(),
        }
    }

    /// Create a new evaluator seeded with accumulated enum values from prior evaluations.
    /// This enables cross-enum reference resolution (e.g., `enum B { Y = A.X }`).
    pub fn with_prior_values(
        arena: &'a NodeArena,
        prior: FxHashMap<String, FxHashMap<String, EnumValue>>,
    ) -> Self {
        EnumEvaluator {
            arena,
            member_values: FxHashMap::default(),
            current_enum_name: None,
            current_source_file: None,
            all_enum_values: prior,
        }
    }

    /// Return the accumulated enum values (for persisting across evaluations).
    pub fn take_all_enum_values(self) -> FxHashMap<String, FxHashMap<String, EnumValue>> {
        self.all_enum_values
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
        self.current_source_file = None;
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
        if let Some(name_node) = self.arena.get(enum_data.name)
            && let Some(ident) = self.arena.get_identifier(name_node)
        {
            self.set_current_enum(&ident.escaped_text);
        }
        self.current_source_file = self.containing_source_file(enum_idx);

        // Track the last numeric value for auto-increment.
        // tsc uses f64 arithmetic internally, so float values auto-increment correctly:
        //   enum E { a = 0.1, b } → b = 1.1
        let mut last_numeric_value: Option<i64> = None;
        let mut last_float_value: Option<f64> = None;
        let mut had_non_numeric = false;

        for &member_idx in &enum_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            let Some(member_data) = self.arena.get_enum_member(member_node) else {
                continue;
            };

            // Get member name
            let member_name =
                crate::transforms::emit_utils::enum_member_name(self.arena, member_data.name);
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
                } else if let Some(f) = last_float_value {
                    // Previous was a float - auto-increment in f64 space
                    let next = f + 1.0;
                    last_float_value = Some(next);
                    // Check if result is an exact integer
                    if next == (next as i64) as f64
                        && next >= i64::MIN as f64
                        && next <= i64::MAX as f64
                    {
                        last_float_value = None;
                        last_numeric_value = Some(next as i64);
                        EnumValue::Number(next as i64)
                    } else {
                        EnumValue::Float(next)
                    }
                } else {
                    // Either first member or previous was integer
                    let next_val = last_numeric_value.map_or(0, |v| v + 1);
                    last_numeric_value = Some(next_val);
                    EnumValue::Number(next_val)
                }
            } else {
                let evaluated = self.evaluate_expression(member_data.initializer);
                // Update auto-increment tracker if numeric
                match &evaluated {
                    EnumValue::Number(n) => {
                        last_numeric_value = Some(*n);
                        last_float_value = None;
                        had_non_numeric = false;
                    }
                    EnumValue::Float(f) => {
                        last_float_value = Some(*f);
                        last_numeric_value = None;
                        had_non_numeric = false;
                    }
                    _ => {
                        // String or computed - can't auto-increment after
                        last_numeric_value = None;
                        last_float_value = None;
                        had_non_numeric = true;
                    }
                }
                evaluated
            };

            self.register_member(&member_name, value);
        }

        let result = self.member_values.clone();

        // Accumulate values for cross-enum reference resolution
        if let Some(ref name) = self.current_enum_name {
            let entry = self.all_enum_values.entry(name.clone()).or_default();
            entry.extend(result.iter().map(|(k, v)| (k.clone(), v.clone())));
        }

        result
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
                    // Check current enum members first
                    if let Some(value) = self.member_values.get(&ident.escaped_text) {
                        return value.clone();
                    }
                    // Check prior blocks of the same merged enum (e.g.,
                    // `enum Animals { CatDog = Cat | Dog }` where Cat/Dog
                    // are from earlier `enum Animals` blocks)
                    if let Some(ref enum_name) = self.current_enum_name
                        && let Some(prior) = self.all_enum_values.get(enum_name)
                        && let Some(value) = prior.get(&ident.escaped_text)
                    {
                        return value.clone();
                    }
                    if let Some(value) = self.resolve_top_level_const(&ident.escaped_text) {
                        return value;
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

            // Element access - Enum["Member"]
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node)
                    && let Some(obj_node) = self.arena.get(access.expression)
                    && obj_node.kind == SyntaxKind::Identifier as u16
                    && let Some(index_node) = self.arena.get(access.name_or_argument)
                    && index_node.kind == SyntaxKind::StringLiteral as u16
                {
                    // Reuse evaluate_property_access — it now handles string literal keys
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

            // Template expression with substitutions: `prefix${expr}suffix`
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                self.evaluate_template_expression(idx)
            }

            _ => EnumValue::Computed,
        }
    }

    fn containing_source_file(&self, mut idx: NodeIndex) -> Option<NodeIndex> {
        for _ in 0..100 {
            let ext = self.arena.get_extended(idx)?;
            if ext.parent.is_none() {
                let node = self.arena.get(idx)?;
                return (node.kind == syntax_kind_ext::SOURCE_FILE).then_some(idx);
            }
            idx = ext.parent;
        }
        None
    }

    fn resolve_top_level_const(&self, name: &str) -> Option<EnumValue> {
        let source_file_idx = self.current_source_file?;
        let source_file_node = self.arena.get(source_file_idx)?;
        let source_file = self.arena.get_source_file(source_file_node)?;

        for &stmt_idx in &source_file.statements.nodes {
            let stmt_node = self.arena.get(stmt_idx)?;
            let var_stmt = self.arena.get_variable(stmt_node)?;
            if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                continue;
            }

            for &decl_list_idx in &var_stmt.declarations.nodes {
                let decl_list_node = self.arena.get(decl_list_idx)?;
                let decl_list = self.arena.get_variable(decl_list_node)?;
                for &decl_idx in &decl_list.declarations.nodes {
                    if !self.arena.is_const_variable_declaration(decl_idx) {
                        continue;
                    }
                    let decl_node = self.arena.get(decl_idx)?;
                    let decl = self.arena.get_variable_declaration(decl_node)?;
                    let decl_name = self.arena.get(decl.name)?;
                    let ident = self.arena.get_identifier(decl_name)?;
                    if ident.escaped_text == name && decl.initializer.is_some() {
                        return Some(self.evaluate_expression(decl.initializer));
                    }
                }
            }
        }

        None
    }

    /// Evaluate a template expression with substitutions: `` `head${expr}middle${expr}tail` ``
    /// Each substitution expression is recursively evaluated; if all parts resolve to
    /// strings (or stringifiable numbers), the result is concatenated into a single string.
    fn evaluate_template_expression(&self, idx: NodeIndex) -> EnumValue {
        let Some(node) = self.arena.get(idx) else {
            return EnumValue::Computed;
        };
        let Some(template) = self.arena.get_template_expr(node) else {
            return EnumValue::Computed;
        };

        // Start with the head text
        let Some(head_node) = self.arena.get(template.head) else {
            return EnumValue::Computed;
        };
        let Some(head_lit) = self.arena.get_literal(head_node) else {
            return EnumValue::Computed;
        };
        let mut result = head_lit.text.clone();

        // Process each template span: expression + literal (middle or tail)
        for &span_idx in &template.template_spans.nodes {
            let Some(span_node) = self.arena.get(span_idx) else {
                return EnumValue::Computed;
            };
            let Some(span) = self.arena.get_template_span(span_node) else {
                return EnumValue::Computed;
            };

            // Evaluate the interpolated expression
            let expr_value = self.evaluate_expression(span.expression);
            match &expr_value {
                EnumValue::String(s) => result.push_str(s),
                EnumValue::Number(n) => result.push_str(&n.to_string()),
                EnumValue::Float(f) => result.push_str(&f.to_string()),
                EnumValue::Computed => return EnumValue::Computed,
            }

            // Append the literal part (middle or tail)
            let Some(lit_node) = self.arena.get(span.literal) else {
                return EnumValue::Computed;
            };
            let Some(lit) = self.arena.get_literal(lit_node) else {
                return EnumValue::Computed;
            };
            result.push_str(&lit.text);
        }

        EnumValue::String(result)
    }

    /// Parse a numeric literal string to i64
    fn parse_numeric_literal(&self, text: &str) -> EnumValue {
        let text = text.trim();
        // Strip numeric separators (underscores) before parsing
        let owned;
        let text = if text.contains('_') {
            owned = text.replace('_', "");
            &owned
        } else {
            text
        };

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
        else if text.starts_with('0')
            && text.len() > 1
            && !text.contains('.')
            && let Ok(val) = i64::from_str_radix(&text[1..], 8)
        {
            return EnumValue::Number(val);
        }

        // Try parsing as decimal integer
        if let Ok(val) = text.parse::<i64>() {
            return EnumValue::Number(val);
        }

        // Try parsing as float (e.g., 0.5, 2., -1.5)
        if let Ok(val) = text.parse::<f64>() {
            // Check if this is actually an integer value (e.g., "2." parses as 2.0)
            if val.fract() == 0.0 && val >= i64::MIN as f64 && val <= i64::MAX as f64 {
                return EnumValue::Number(val as i64);
            }
            return EnumValue::Float(val);
        }

        EnumValue::Computed
    }

    /// Evaluate a binary expression
    fn evaluate_binary(&self, left: NodeIndex, op: u16, right: NodeIndex) -> EnumValue {
        let left_val = self.evaluate_expression(left);
        let right_val = self.evaluate_expression(right);

        // Both must be numbers for binary operations (except + for strings)
        // If either operand is a float, promote to float arithmetic
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
                    k if k == SyntaxKind::AsteriskAsteriskToken as u16 => l.wrapping_pow(*r as u32),
                    _ => return EnumValue::Computed,
                };
                EnumValue::Number(result)
            }
            // Float arithmetic: if either side is float, use f64
            (l_val, r_val)
                if matches!(l_val, EnumValue::Number(_) | EnumValue::Float(_))
                    && matches!(r_val, EnumValue::Number(_) | EnumValue::Float(_)) =>
            {
                let l = l_val
                    .as_f64()
                    .expect("pattern guard ensures Number or Float variant");
                let r = r_val
                    .as_f64()
                    .expect("pattern guard ensures Number or Float variant");
                let result = match op {
                    k if k == SyntaxKind::PlusToken as u16 => l + r,
                    k if k == SyntaxKind::MinusToken as u16 => l - r,
                    k if k == SyntaxKind::AsteriskToken as u16 => l * r,
                    k if k == SyntaxKind::SlashToken as u16 => {
                        if r == 0.0 {
                            return EnumValue::Computed;
                        }
                        l / r
                    }
                    k if k == SyntaxKind::PercentToken as u16 => {
                        if r == 0.0 {
                            return EnumValue::Computed;
                        }
                        l % r
                    }
                    k if k == SyntaxKind::AsteriskAsteriskToken as u16 => l.powf(r),
                    _ => return EnumValue::Computed,
                };
                if result.fract() == 0.0 && result >= i64::MIN as f64 && result <= i64::MAX as f64 {
                    EnumValue::Number(result as i64)
                } else {
                    EnumValue::Float(result)
                }
            }
            // String concatenation
            (EnumValue::String(l), EnumValue::String(r)) if op == SyntaxKind::PlusToken as u16 => {
                EnumValue::String(format!("{l}{r}"))
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
            EnumValue::Float(f) => {
                let result = match op {
                    k if k == SyntaxKind::PlusToken as u16 => f,
                    k if k == SyntaxKind::MinusToken as u16 => -f,
                    k if k == SyntaxKind::TildeToken as u16 => {
                        // ~f truncates to integer first
                        return EnumValue::Number(!(f as i64));
                    }
                    _ => return EnumValue::Computed,
                };
                EnumValue::Float(result)
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
        } else if prop_node.kind == SyntaxKind::StringLiteral as u16
            && let Some(lit) = self.arena.get_literal(prop_node)
        {
            lit.text.clone()
        } else {
            return EnumValue::Computed;
        };

        // Check if this is a self-reference (E.A within enum E)
        if let Some(ref current_enum) = self.current_enum_name
            && &obj_name == current_enum
            && let Some(value) = self.member_values.get(&prop_name)
        {
            return value.clone();
        }

        // Check cross-enum references (Foo.A from within enum Bar)
        if let Some(enum_members) = self.all_enum_values.get(&obj_name)
            && let Some(value) = enum_members.get(&prop_name)
        {
            return value.clone();
        }

        EnumValue::Computed
    }
}

#[cfg(test)]
#[path = "../../tests/evaluator.rs"]
mod tests;
