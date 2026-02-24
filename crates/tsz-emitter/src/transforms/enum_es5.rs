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

use std::collections::{HashMap, HashSet};

use crate::transforms::ir::{IRNode, IRParam};
use crate::transforms::ir_printer::IRPrinter;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

/// Enum ES5 transformer - produces IR for enum declarations
pub struct EnumES5Transformer<'a> {
    arena: &'a NodeArena,
    /// Track last numeric value for auto-incrementing
    last_value: Option<i64>,
    /// Source text for extracting raw expressions
    source_text: Option<&'a str>,
    /// Names of all enum members declared so far (for qualifying self-references)
    member_names: HashSet<String>,
    /// Names of enum members with string-valued initializers (no reverse mapping)
    string_members: HashSet<String>,
    /// Evaluated numeric values of enum members (for constant folding in subsequent member initializers)
    member_values: HashMap<String, i64>,
    /// The enum parameter name used inside the IIFE (for qualifying self-references)
    current_enum_name: String,
    /// When true, emit const enums instead of erasing them
    preserve_const_enums: bool,
}

impl<'a> EnumES5Transformer<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        EnumES5Transformer {
            arena,
            last_value: None,
            source_text: None,
            member_names: HashSet::new(),
            string_members: HashSet::new(),
            member_values: HashMap::new(),
            current_enum_name: String::new(),
            preserve_const_enums: false,
        }
    }

    pub const fn set_preserve_const_enums(&mut self, value: bool) {
        self.preserve_const_enums = value;
    }

    /// Set source text for raw expression extraction
    pub const fn set_source_text(&mut self, text: &'a str) {
        self.source_text = Some(text);
    }

    /// Transform an enum declaration to IR
    /// Returns None for const enums (they are erased)
    pub fn transform_enum(&mut self, enum_idx: NodeIndex) -> Option<IRNode> {
        self.last_value = Some(-1); // Start at -1 so first increment is 0

        let enum_node = self.arena.get(enum_idx)?;

        let enum_data = self.arena.get_enum(enum_node)?;

        // Const enums are erased unless preserveConstEnums is set
        if self
            .arena
            .has_modifier(&enum_data.modifiers, SyntaxKind::ConstKeyword)
            && !self.preserve_const_enums
        {
            return None;
        }

        let name =
            crate::transforms::emit_utils::identifier_text_or_empty(self.arena, enum_data.name);
        if name.is_empty() {
            return None;
        }

        // Build IR for: var E; (function (E) { ... })(E || (E = {}));
        let mut statements = Vec::new();

        // var E;
        statements.push(IRNode::VarDecl {
            name: name.clone(),
            initializer: None,
        });

        // Build IIFE body (enum member assignments)
        let body = self.transform_members(&enum_data.members, &name);

        // Build IIFE argument: E || (E = {})
        let iife_arg = IRNode::LogicalOr {
            left: Box::new(IRNode::Identifier(name.clone())),
            right: Box::new(IRNode::BinaryExpr {
                left: Box::new(IRNode::Identifier(name.clone())),
                operator: "=".to_string(),
                right: Box::new(IRNode::empty_object()),
            }),
        };

        // (function (E) { body })(arg)
        let iife = IRNode::CallExpr {
            callee: Box::new(IRNode::FunctionExpr {
                name: None, // IIFEs are anonymous functions
                parameters: vec![IRParam::new(&name)],
                body,
                is_expression_body: false,
                body_source_range: None,
            }),
            arguments: vec![iife_arg],
        };

        statements.push(IRNode::ExpressionStatement(Box::new(iife)));

        Some(IRNode::Sequence(statements))
    }

    /// Get the enum name without transforming
    pub fn get_enum_name(&self, enum_idx: NodeIndex) -> String {
        let Some(enum_node) = self.arena.get(enum_idx) else {
            return String::new();
        };
        let Some(enum_data) = self.arena.get_enum(enum_node) else {
            return String::new();
        };
        crate::transforms::emit_utils::identifier_text_or_empty(self.arena, enum_data.name)
    }

    /// Check if enum is a const enum
    pub fn is_const_enum_by_idx(&self, enum_idx: NodeIndex) -> bool {
        let Some(enum_node) = self.arena.get(enum_idx) else {
            return false;
        };
        let Some(enum_data) = self.arena.get_enum(enum_node) else {
            return false;
        };
        self.arena
            .has_modifier(&enum_data.modifiers, SyntaxKind::ConstKeyword)
    }

    /// Transform enum members to IR statements
    fn transform_members(&mut self, members: &NodeList, enum_name: &str) -> Vec<IRNode> {
        let mut statements = Vec::new();
        // Reset per-enum tracking state
        self.member_names.clear();
        self.string_members.clear();
        self.member_values.clear();
        self.current_enum_name = enum_name.to_string();

        for &member_idx in &members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let Some(member_data) = self.arena.get_enum_member(member_node) else {
                continue;
            };

            let member_name =
                crate::transforms::emit_utils::enum_member_name(self.arena, member_data.name);
            let has_initializer = member_data.initializer.is_some();

            let stmt = if has_initializer {
                if self.is_syntactically_string(member_data.initializer) {
                    // String enum: E["A"] = "val";
                    // No reverse mapping for string enums
                    self.string_members.insert(member_name.clone());
                    let assign = IRNode::BinaryExpr {
                        left: Box::new(IRNode::ElementAccess {
                            object: Box::new(IRNode::Identifier(enum_name.to_string())),
                            index: Box::new(IRNode::StringLiteral(member_name.clone())),
                        }),
                        operator: "=".to_string(),
                        right: Box::new(self.transform_expression(member_data.initializer)),
                    };
                    self.last_value = None; // Reset auto-increment
                    IRNode::ExpressionStatement(Box::new(assign))
                } else {
                    // Numeric/Computed: E[E["A"] = val] = "A";
                    // Try to evaluate the constant expression for auto-increment tracking
                    // and constant folding (tsc emits evaluated values, not source expressions)
                    let evaluated = self.evaluate_constant_expression(member_data.initializer);
                    if let Some(val) = evaluated {
                        self.last_value = Some(val);
                    } else {
                        self.last_value = None; // Can't evaluate, reset auto-increment
                    }
                    // Use the evaluated value if available, otherwise emit the source expression
                    let inner_value = if let Some(val) = evaluated {
                        Self::format_numeric_literal(val)
                    } else {
                        self.transform_expression(member_data.initializer)
                    };
                    let inner_assign = IRNode::BinaryExpr {
                        left: Box::new(IRNode::ElementAccess {
                            object: Box::new(IRNode::Identifier(enum_name.to_string())),
                            index: Box::new(IRNode::StringLiteral(member_name.clone())),
                        }),
                        operator: "=".to_string(),
                        right: Box::new(inner_value),
                    };
                    let outer_assign = IRNode::BinaryExpr {
                        left: Box::new(IRNode::ElementAccess {
                            object: Box::new(IRNode::Identifier(enum_name.to_string())),
                            index: Box::new(inner_assign),
                        }),
                        operator: "=".to_string(),
                        right: Box::new(IRNode::StringLiteral(member_name.clone())),
                    };
                    IRNode::ExpressionStatement(Box::new(outer_assign))
                }
            } else {
                // Auto-increment: E[E["A"] = 0] = "A";
                let next_val = self.last_value.map_or(0, |v| v + 1);
                self.last_value = Some(next_val);

                let inner_assign = IRNode::BinaryExpr {
                    left: Box::new(IRNode::ElementAccess {
                        object: Box::new(IRNode::Identifier(enum_name.to_string())),
                        index: Box::new(IRNode::StringLiteral(member_name.clone())),
                    }),
                    operator: "=".to_string(),
                    right: Box::new(IRNode::NumericLiteral(next_val.to_string())),
                };
                let outer_assign = IRNode::BinaryExpr {
                    left: Box::new(IRNode::ElementAccess {
                        object: Box::new(IRNode::Identifier(enum_name.to_string())),
                        index: Box::new(inner_assign),
                    }),
                    operator: "=".to_string(),
                    right: Box::new(IRNode::StringLiteral(member_name.clone())),
                };
                IRNode::ExpressionStatement(Box::new(outer_assign))
            };

            // Track the evaluated value for use in subsequent member initializers
            if let Some(val) = self.last_value {
                self.member_values.insert(member_name.clone(), val);
            }
            self.member_names.insert(member_name);
            statements.push(stmt);
        }

        statements
    }

    /// Transform an expression node to IR
    fn transform_expression(&self, idx: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(idx) else {
            return IRNode::NumericLiteral("0".to_string());
        };

        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    IRNode::NumericLiteral(lit.text.clone())
                } else {
                    IRNode::NumericLiteral("0".to_string())
                }
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    IRNode::StringLiteral(lit.text.clone())
                } else {
                    IRNode::StringLiteral(String::new())
                }
            }
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(id) = self.arena.get_identifier(node) {
                    // Inside enum IIFE, references to sibling enum members must be
                    // qualified with the enum parameter name: `a` -> `Foo.a`
                    if !self.current_enum_name.is_empty()
                        && self.member_names.contains(id.escaped_text.as_str())
                    {
                        IRNode::PropertyAccess {
                            object: Box::new(IRNode::Identifier(self.current_enum_name.clone())),
                            property: id.escaped_text.clone(),
                        }
                    } else {
                        IRNode::Identifier(id.escaped_text.clone())
                    }
                } else {
                    IRNode::Identifier("unknown".to_string())
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.arena.get_binary_expr(node) {
                    IRNode::BinaryExpr {
                        left: Box::new(self.transform_expression(bin.left)),
                        operator: self.emit_operator(bin.operator_token),
                        right: Box::new(self.transform_expression(bin.right)),
                    }
                } else {
                    IRNode::NumericLiteral("0".to_string())
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr(node) {
                    IRNode::PrefixUnaryExpr {
                        operator: self.emit_operator(unary.operator),
                        operand: Box::new(self.transform_expression(unary.operand)),
                    }
                } else {
                    IRNode::NumericLiteral("0".to_string())
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.arena.get_parenthesized(node) {
                    IRNode::Parenthesized(Box::new(self.transform_expression(paren.expression)))
                } else {
                    IRNode::NumericLiteral("0".to_string())
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                // E.A reference inside enum
                if let Some(access) = self.arena.get_access_expr(node) {
                    let obj = self.transform_expression(access.expression);
                    let prop = if let Some(prop_node) = self.arena.get(access.name_or_argument) {
                        if let Some(ident) = self.arena.get_identifier(prop_node) {
                            ident.escaped_text.clone()
                        } else if let Some(lit) = self.arena.get_literal(prop_node) {
                            lit.text.clone()
                        } else {
                            "unknown".to_string()
                        }
                    } else {
                        "unknown".to_string()
                    };
                    IRNode::PropertyAccess {
                        object: Box::new(obj),
                        property: prop,
                    }
                } else {
                    IRNode::NumericLiteral("0".to_string())
                }
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    IRNode::ElementAccess {
                        object: Box::new(self.transform_expression(access.expression)),
                        index: Box::new(self.transform_expression(access.name_or_argument)),
                    }
                } else {
                    IRNode::NumericLiteral("0".to_string())
                }
            }
            // `this` keyword
            k if k == SyntaxKind::ThisKeyword as u16 => IRNode::This { captured: false },

            // Call expression: fn(args)
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(node) {
                    let callee = self.transform_expression(call.expression);
                    let args: Vec<_> = call
                        .arguments
                        .as_ref()
                        .map_or(&[][..], |nl| &nl.nodes)
                        .iter()
                        .map(|&arg| self.transform_expression(arg))
                        .collect();
                    IRNode::CallExpr {
                        callee: Box::new(callee),
                        arguments: args,
                    }
                } else {
                    IRNode::NumericLiteral("0".to_string())
                }
            }

            // Arrow function / function expression: use raw source text
            k if k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION =>
            {
                if let Some(text) = self.source_text {
                    let start = node.pos as usize;
                    // Use body end as a tighter bound - node.end may extend
                    // past closing delimiters of parent expressions
                    let body_end = self
                        .arena
                        .get_function(node)
                        .map(|f| self.arena.get(f.body).map_or(node.end, |b| b.end))
                        .unwrap_or(node.end);
                    let end = body_end as usize;
                    if start < end && end <= text.len() {
                        let raw = text[start..end].trim();
                        // Trim trailing comma (element separator that bleeds into
                        // the node's span)
                        let raw = raw.trim_end_matches(',').trim_end();
                        if !raw.is_empty() {
                            return IRNode::Raw(raw.to_string());
                        }
                    }
                }
                IRNode::NumericLiteral("0".to_string())
            }

            _ => {
                // Fallback: emit the source text verbatim for other unrecognized
                // expressions (template expressions, tagged templates, etc.)
                if let Some(text) = self.source_text {
                    let start = node.pos as usize;
                    let end = node.end as usize;
                    if start < end && end <= text.len() {
                        let raw = text[start..end].trim();
                        let raw = raw.trim_end_matches(',').trim_end();
                        if !raw.is_empty() {
                            return IRNode::Raw(raw.to_string());
                        }
                    }
                }
                IRNode::NumericLiteral("0".to_string())
            }
        }
    }

    fn emit_operator(&self, op: u16) -> String {
        crate::transforms::emit_utils::operator_to_str(op).to_string()
    }

    /// Format an i64 value as an `IRNode` numeric literal, matching tsc's output format.
    fn format_numeric_literal(val: i64) -> IRNode {
        IRNode::NumericLiteral(val.to_string())
    }

    /// Try to evaluate a constant expression to its numeric value.
    /// Handles numeric literals, binary/unary expressions, parenthesized expressions,
    /// and references to previously evaluated enum members (both bare identifiers and
    /// `EnumName.Member` property accesses).
    /// Returns None if the expression can't be statically evaluated.
    fn evaluate_constant_expression(&self, idx: NodeIndex) -> Option<i64> {
        let node = self.arena.get(idx)?;

        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.arena.get_literal(node)?;
                lit.text.parse().ok()
            }
            k if k == SyntaxKind::Identifier as u16 => {
                // Resolve references to previously evaluated enum members
                let id = self.arena.get_identifier(node)?;
                self.member_values.get(id.escaped_text.as_str()).copied()
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                // Resolve E.Member references (same enum self-references)
                let access = self.arena.get_access_expr(node)?;
                let obj_node = self.arena.get(access.expression)?;
                if obj_node.kind == SyntaxKind::Identifier as u16
                    && let Some(obj_id) = self.arena.get_identifier(obj_node)
                    && obj_id.escaped_text == self.current_enum_name
                {
                    let prop_node = self.arena.get(access.name_or_argument)?;
                    if let Some(prop_id) = self.arena.get_identifier(prop_node) {
                        return self
                            .member_values
                            .get(prop_id.escaped_text.as_str())
                            .copied();
                    }
                }
                None
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let bin = self.arena.get_binary_expr(node)?;
                let left = self.evaluate_constant_expression(bin.left)?;
                let right = self.evaluate_constant_expression(bin.right)?;
                let op = bin.operator_token;

                match op {
                    o if o == SyntaxKind::PlusToken as u16 => left.checked_add(right),
                    o if o == SyntaxKind::MinusToken as u16 => left.checked_sub(right),
                    o if o == SyntaxKind::AsteriskToken as u16 => left.checked_mul(right),
                    o if o == SyntaxKind::SlashToken as u16 => (right != 0).then(|| left / right),
                    o if o == SyntaxKind::PercentToken as u16 => (right != 0).then(|| left % right),
                    o if o == SyntaxKind::LessThanLessThanToken as u16 => {
                        Some(left.wrapping_shl(right as u32))
                    }
                    o if o == SyntaxKind::GreaterThanGreaterThanToken as u16 => {
                        Some(left.wrapping_shr(right as u32))
                    }
                    o if o == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16 => {
                        Some((left as u64).wrapping_shr(right as u32) as i64)
                    }
                    o if o == SyntaxKind::AmpersandToken as u16 => Some(left & right),
                    o if o == SyntaxKind::BarToken as u16 => Some(left | right),
                    o if o == SyntaxKind::CaretToken as u16 => Some(left ^ right),
                    _ => None,
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.arena.get_unary_expr(node)?;
                let operand = self.evaluate_constant_expression(unary.operand)?;
                let op = unary.operator;
                match op {
                    o if o == SyntaxKind::MinusToken as u16 => Some(operand.checked_neg()?),
                    o if o == SyntaxKind::TildeToken as u16 => Some(!operand),
                    o if o == SyntaxKind::ExclamationToken as u16 => Some(i64::from(operand == 0)),
                    o if o == SyntaxKind::PlusToken as u16 => Some(operand),
                    _ => None,
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let paren = self.arena.get_parenthesized(node)?;
                self.evaluate_constant_expression(paren.expression)
            }
            _ => None,
        }
    }

    /// Check if an expression is syntactically string-valued per tsc's rules.
    /// String-valued enum members do NOT get reverse mappings.
    /// Handles: string literals, template literals, string concatenation (`"x" + expr`),
    /// references to other string-valued enum members, and parenthesized wrappers.
    fn is_syntactically_string(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16 => true,
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => true,
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => true,
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                // Unwrap parens: (`${BAR}`) is still syntactically string
                if let Some(paren) = self.arena.get_parenthesized(node) {
                    self.is_syntactically_string(paren.expression)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                // String concatenation: "x" + expr is syntactically string
                if let Some(bin) = self.arena.get_binary_expr(node) {
                    let is_plus = bin.operator_token == SyntaxKind::PlusToken as u16;
                    if is_plus {
                        self.is_syntactically_string(bin.left)
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                // E.A where A is a known string member — syntactically string
                if let Some(access) = self.arena.get_access_expr(node) {
                    // Check if the object is the enum parameter name
                    let obj_node = self.arena.get(access.expression);
                    let obj_is_enum = obj_node.is_some_and(|n| {
                        n.kind == SyntaxKind::Identifier as u16
                            && self
                                .arena
                                .get_identifier(n)
                                .is_some_and(|id| id.escaped_text == self.current_enum_name)
                    });
                    if obj_is_enum {
                        // Check if the property name is a known string member
                        let prop_name = self
                            .arena
                            .get(access.name_or_argument)
                            .and_then(|n| self.arena.get_identifier(n))
                            .map(|id| id.escaped_text.as_str());
                        if let Some(name) = prop_name {
                            return self.string_members.contains(name);
                        }
                    }
                    false
                } else {
                    false
                }
            }
            k if k == SyntaxKind::Identifier as u16 => {
                // Bare identifier that matches a known string member
                if let Some(id) = self.arena.get_identifier(node) {
                    self.string_members.contains(id.escaped_text.as_str())
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}

/// Enum ES5 emitter wrapping `EnumES5Transformer` + `IRPrinter`
pub struct EnumES5Emitter<'a> {
    indent_level: u32,
    transformer: EnumES5Transformer<'a>,
}

impl<'a> EnumES5Emitter<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        EnumES5Emitter {
            indent_level: 0,
            transformer: EnumES5Transformer::new(arena),
        }
    }

    pub const fn set_indent_level(&mut self, level: u32) {
        self.indent_level = level;
    }

    /// Set source text for raw expression extraction
    pub const fn set_source_text(&mut self, text: &'a str) {
        self.transformer.set_source_text(text);
    }

    /// Emit an enum declaration
    /// Returns empty string for const enums (they are erased)
    pub fn emit_enum(&mut self, enum_idx: NodeIndex) -> String {
        let ir = self.transformer.transform_enum(enum_idx);
        let ir = match ir {
            Some(ir) => ir,
            None => return String::new(),
        };

        let mut printer = IRPrinter::new();
        printer.set_indent_level(self.indent_level);
        let result = printer.emit(&ir);
        result.to_string()
    }

    /// Get the enum name without emitting anything
    pub fn get_enum_name(&self, enum_idx: NodeIndex) -> String {
        self.transformer.get_enum_name(enum_idx)
    }

    /// Check if enum is a const enum
    pub fn is_const_enum_by_idx(&self, enum_idx: NodeIndex) -> bool {
        self.transformer.is_const_enum_by_idx(enum_idx)
    }
}

#[cfg(test)]
#[path = "../../tests/enum_es5.rs"]
mod tests;
