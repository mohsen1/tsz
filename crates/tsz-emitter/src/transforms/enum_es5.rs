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
    /// Track last numeric value for auto-incrementing (integer path)
    last_value: Option<i64>,
    /// Track last float value for auto-incrementing (float path, e.g., 0.1 → 1.1)
    last_float_value: Option<f64>,
    /// Source text for extracting raw expressions
    source_text: Option<&'a str>,
    /// Names of all enum members declared so far (for qualifying self-references)
    member_names: HashSet<String>,
    /// Names of enum members that have been processed (had their IR emitted).
    /// Used to distinguish forward references (not yet processed → resolve to 0)
    /// from self-references and back-references (already processed → keep expression).
    processed_members: HashSet<String>,
    /// The name of the member currently being processed (for detecting self-references)
    current_member_name: String,
    /// Names of enum members with string-valued initializers (no reverse mapping)
    string_members: HashSet<String>,
    /// Evaluated numeric values of enum members (for constant folding in subsequent member initializers)
    member_values: HashMap<String, i64>,
    /// Evaluated string values of enum members (for constant folding in string concatenation)
    string_member_values: HashMap<String, String>,
    /// The enum parameter name used inside the IIFE (for qualifying self-references)
    current_enum_name: String,
    /// When true, emit const enums instead of erasing them
    preserve_const_enums: bool,
    /// Previously-evaluated enum member values from other enums.
    /// Keyed by `enum_name` → `member_name` → value.
    prior_enum_values: HashMap<String, HashMap<String, i64>>,
    /// Previously-evaluated string enum member names from other enums.
    /// Keyed by `enum_name` → set of member names that have string values.
    prior_string_members: HashMap<String, HashSet<String>>,
}

impl<'a> EnumES5Transformer<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        EnumES5Transformer {
            arena,
            last_value: None,
            last_float_value: None,
            source_text: None,
            member_names: HashSet::new(),
            string_members: HashSet::new(),
            processed_members: HashSet::new(),
            current_member_name: String::new(),
            member_values: HashMap::new(),
            string_member_values: HashMap::new(),
            current_enum_name: String::new(),
            preserve_const_enums: false,
            prior_enum_values: HashMap::new(),
            prior_string_members: HashMap::new(),
        }
    }

    pub const fn set_preserve_const_enums(&mut self, value: bool) {
        self.preserve_const_enums = value;
    }

    /// Set source text for raw expression extraction
    pub const fn set_source_text(&mut self, text: &'a str) {
        self.source_text = Some(text);
    }

    /// Set previously-evaluated enum values for cross-enum reference resolution.
    pub fn set_prior_enum_values(
        &mut self,
        values: &rustc_hash::FxHashMap<String, rustc_hash::FxHashMap<String, i64>>,
    ) {
        self.prior_enum_values = values
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    v.iter().map(|(mk, mv)| (mk.clone(), *mv)).collect(),
                )
            })
            .collect();
    }

    /// Set previously-known string enum member names for cross-enum detection.
    pub fn set_prior_string_members(
        &mut self,
        members: &rustc_hash::FxHashMap<String, rustc_hash::FxHashSet<String>>,
    ) {
        self.prior_string_members = members
            .iter()
            .map(|(k, v)| (k.clone(), v.iter().cloned().collect()))
            .collect();
    }

    /// Get the accumulated member values for this enum (for persisting across declarations).
    pub const fn get_member_values(&self) -> &HashMap<String, i64> {
        &self.member_values
    }

    /// Get the string member names for this enum.
    pub const fn get_string_members(&self) -> &HashSet<String> {
        &self.string_members
    }

    /// Get the current enum name (from the last `transform_enum` call).
    pub fn current_enum_name_ref(&self) -> &str {
        &self.current_enum_name
    }

    /// Transform an enum declaration to IR
    /// Returns None for const enums (they are erased)
    pub fn transform_enum(&mut self, enum_idx: NodeIndex) -> Option<IRNode> {
        self.last_value = Some(-1); // Start at -1 so first increment is 0
        self.last_float_value = None;

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
            name: name.clone().into(),
            initializer: None,
        });

        // Build IIFE body (enum member assignments)
        let body = self.transform_members(&enum_data.members, &name);

        // Build IIFE argument: E || (E = {})
        let iife_arg = IRNode::LogicalOr {
            left: Box::new(IRNode::Identifier(name.clone().into())),
            right: Box::new(IRNode::BinaryExpr {
                left: Box::new(IRNode::Identifier(name.clone().into())),
                operator: "=".to_string().into(),
                right: Box::new(IRNode::empty_object()),
            }),
        };

        // (function (E) { body })(arg)
        let iife = IRNode::CallExpr {
            callee: Box::new(IRNode::FunctionExpr {
                name: None, // IIFEs are anonymous functions
                parameters: vec![IRParam::new(name.clone())],
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

    /// Extract a leading block/JSDoc comment that appears immediately before `pos`.
    ///
    /// Scans backward from `pos` skipping whitespace/newlines.  If we land on `*/`
    /// we scan further back for the matching `/*` and return the comment text.
    fn extract_leading_comment_at(&self, pos: u32) -> Option<String> {
        let source_text = self.source_text?;
        let bytes = source_text.as_bytes();
        let pos = pos as usize;
        if pos == 0 {
            return None;
        }
        let mut i = pos;
        // Skip trailing whitespace/newlines before the token
        while i > 0 && matches!(bytes[i - 1], b' ' | b'\t' | b'\r' | b'\n') {
            i -= 1;
        }
        // Check if we landed on `*/` (end of a block comment)
        if i >= 2 && bytes[i - 1] == b'/' && bytes[i - 2] == b'*' {
            let comment_end = i;
            let mut j = i - 2;
            loop {
                if j < 2 {
                    break;
                }
                if bytes[j - 1] == b'/' && bytes[j] == b'*' {
                    let comment_start = j - 1;
                    let comment_text = &source_text[comment_start..comment_end];
                    if comment_text.starts_with("/**") && !comment_text.starts_with("/***") {
                        return Some(comment_text.to_string());
                    }
                    if comment_text.starts_with("/*") {
                        return Some(comment_text.to_string());
                    }
                    break;
                }
                j -= 1;
            }
        }
        None
    }

    /// Extract trailing inline comment from right after the member name end.
    ///
    /// Handles `/* block */` and `// line` comments on the same line.
    fn extract_trailing_comment_at(&self, end: u32) -> Option<String> {
        let source_text = self.source_text?;
        for comment in crate::emitter::get_trailing_comment_ranges(source_text, end as usize) {
            let text = &source_text[comment.pos as usize..comment.end as usize];
            if text.starts_with("//") || text.starts_with("/*") {
                return Some(text.to_string());
            }
        }
        None
    }

    /// Transform enum members to IR statements
    fn transform_members(&mut self, members: &NodeList, enum_name: &str) -> Vec<IRNode> {
        let mut statements = Vec::new();
        // Reset per-enum tracking state
        self.member_names.clear();
        self.processed_members.clear();
        self.current_member_name.clear();
        self.string_members.clear();
        self.member_values.clear();
        self.string_member_values.clear();
        self.current_enum_name = enum_name.to_string();

        // Pre-populate member_names with ALL member names so that forward
        // and self-references in initializers get qualified with the enum name.
        // E.g., `enum E { A = A }` → `E[E["A"] = E.A] = "A"` (not `= A`).
        for &member_idx in &members.nodes {
            if let Some(member_node) = self.arena.get(member_idx)
                && let Some(member_data) = self.arena.get_enum_member(member_node)
            {
                let name =
                    crate::transforms::emit_utils::enum_member_name(self.arena, member_data.name);
                self.member_names.insert(name);
            }
        }

        for &member_idx in &members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let Some(member_data) = self.arena.get_enum_member(member_node) else {
                continue;
            };

            let name_idx = member_data.name;
            let member_name = crate::transforms::emit_utils::enum_member_name(self.arena, name_idx);
            self.current_member_name = member_name.clone();
            let has_initializer = member_data.initializer.is_some();

            // Check if this is a computed property name: enum E { [expr] = value }
            let is_computed = self
                .arena
                .get(member_data.name)
                .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME);

            // For computed property names, get the expression as an IR node
            let computed_key = if is_computed {
                self.arena
                    .get(member_data.name)
                    .and_then(|n| self.arena.get_computed_property(n))
                    .map(|cp| self.transform_expression(cp.expression))
            } else {
                None
            };

            let stmt = if let Some(key_expr) = computed_key {
                // Computed property: E[E[expr] = value] = expr;
                let value = if has_initializer {
                    self.transform_expression(member_data.initializer)
                } else {
                    let next_val = self.last_value.map_or(0, |v| v + 1);
                    self.last_value = Some(next_val);
                    IRNode::NumericLiteral(next_val.to_string().into())
                };
                self.last_value = None; // Can't auto-increment after computed
                self.last_float_value = None;
                let inner_assign = IRNode::BinaryExpr {
                    left: Box::new(IRNode::ElementAccess {
                        object: Box::new(IRNode::Identifier(enum_name.to_string().into())),
                        index: Box::new(key_expr.clone()),
                    }),
                    operator: "=".to_string().into(),
                    right: Box::new(value),
                };
                let outer_assign = IRNode::BinaryExpr {
                    left: Box::new(IRNode::ElementAccess {
                        object: Box::new(IRNode::Identifier(enum_name.to_string().into())),
                        index: Box::new(inner_assign),
                    }),
                    operator: "=".to_string().into(),
                    right: Box::new(key_expr),
                };
                IRNode::ExpressionStatement(Box::new(outer_assign))
            } else if has_initializer {
                if self.is_syntactically_string(member_data.initializer) {
                    // String enum: E["A"] = "val";
                    // No reverse mapping for string enums
                    self.string_members.insert(member_name.clone());
                    // Try to constant-fold the string expression (e.g. "1" + "2" -> "12"),
                    // but preserve enum member references like `H = A` as `Foo.A` in the
                    // emitted output while still recording their folded value for later uses.
                    let folded_string = self.evaluate_string_expression(member_data.initializer);
                    if let Some(ref folded) = folded_string {
                        self.string_member_values
                            .insert(member_name.clone(), folded.clone());
                    }
                    let value_ir = if let Some(folded) = folded_string {
                        IRNode::StringLiteral(folded.into())
                    } else {
                        self.transform_expression(member_data.initializer)
                    };
                    let assign = IRNode::BinaryExpr {
                        left: Box::new(IRNode::ElementAccess {
                            object: Box::new(IRNode::Identifier(enum_name.to_string().into())),
                            index: Box::new(self.member_name_ir_node(name_idx, &member_name)),
                        }),
                        operator: "=".to_string().into(),
                        right: Box::new(value_ir),
                    };
                    self.last_value = None; // Reset auto-increment
                    self.last_float_value = None;
                    IRNode::ExpressionStatement(Box::new(assign))
                } else {
                    // Numeric/Computed: E[E["A"] = val] = "A";
                    // Try to evaluate the constant expression for auto-increment tracking
                    // and constant folding (tsc emits evaluated values, not source expressions)
                    let evaluated = self.evaluate_constant_expression(member_data.initializer);
                    let evaluated_float = if evaluated.is_none() {
                        self.evaluate_constant_float_expression(member_data.initializer)
                    } else {
                        None
                    };
                    if let Some(val) = evaluated {
                        self.last_value = Some(val);
                        self.last_float_value = None;
                    } else if let Some(fval) = evaluated_float {
                        self.last_float_value = Some(fval);
                        self.last_value = None;
                    } else {
                        self.last_value = None;
                        self.last_float_value = None;
                    }
                    // Use the evaluated value if available, otherwise emit the source expression.
                    // TSC resolves forward references within the same enum to 0.
                    let inner_value = if let Some(val) = evaluated {
                        Self::format_numeric_literal(val)
                    } else if let Some(fval) = evaluated_float {
                        Self::format_float_literal(fval)
                    } else if self.is_forward_enum_reference(member_data.initializer) {
                        Self::format_numeric_literal(0)
                    } else {
                        self.transform_expression(member_data.initializer)
                    };
                    let inner_assign = IRNode::BinaryExpr {
                        left: Box::new(IRNode::ElementAccess {
                            object: Box::new(IRNode::Identifier(enum_name.to_string().into())),
                            index: Box::new(self.member_name_ir_node(name_idx, &member_name)),
                        }),
                        operator: "=".to_string().into(),
                        right: Box::new(inner_value),
                    };
                    let outer_assign = IRNode::BinaryExpr {
                        left: Box::new(IRNode::ElementAccess {
                            object: Box::new(IRNode::Identifier(enum_name.to_string().into())),
                            index: Box::new(inner_assign),
                        }),
                        operator: "=".to_string().into(),
                        right: Box::new(self.member_name_ir_node(name_idx, &member_name)),
                    };
                    IRNode::ExpressionStatement(Box::new(outer_assign))
                }
            } else if let Some(fval) = self.last_float_value {
                // Float auto-increment: E[E["b"] = 1.1] = "b";
                let next = fval + 1.0;
                // Check if result became an exact integer
                if next == (next as i64) as f64
                    && next >= i64::MIN as f64
                    && next <= i64::MAX as f64
                {
                    self.last_float_value = None;
                    self.last_value = Some(next as i64);
                } else {
                    self.last_float_value = Some(next);
                }
                let value_node = if self.last_float_value.is_some() {
                    Self::format_float_literal(next)
                } else {
                    Self::format_numeric_literal(next as i64)
                };
                let inner_assign = IRNode::BinaryExpr {
                    left: Box::new(IRNode::ElementAccess {
                        object: Box::new(IRNode::Identifier(enum_name.to_string().into())),
                        index: Box::new(self.member_name_ir_node(name_idx, &member_name)),
                    }),
                    operator: "=".to_string().into(),
                    right: Box::new(value_node),
                };
                let outer_assign = IRNode::BinaryExpr {
                    left: Box::new(IRNode::ElementAccess {
                        object: Box::new(IRNode::Identifier(enum_name.to_string().into())),
                        index: Box::new(inner_assign),
                    }),
                    operator: "=".to_string().into(),
                    right: Box::new(self.member_name_ir_node(name_idx, &member_name)),
                };
                IRNode::ExpressionStatement(Box::new(outer_assign))
            } else {
                // Auto-increment: E[E["A"] = 0] = "A";
                // When last_value is None, auto-increment was broken by a computed
                // initializer (e.g. `X = "".length`), so emit `void 0` like tsc.
                let value_node = if let Some(v) = self.last_value {
                    let next_val = v + 1;
                    self.last_value = Some(next_val);
                    IRNode::NumericLiteral(next_val.to_string().into())
                } else {
                    // Can't auto-increment: emit void 0
                    IRNode::Undefined
                };

                let inner_assign = IRNode::BinaryExpr {
                    left: Box::new(IRNode::ElementAccess {
                        object: Box::new(IRNode::Identifier(enum_name.to_string().into())),
                        index: Box::new(self.member_name_ir_node(name_idx, &member_name)),
                    }),
                    operator: "=".to_string().into(),
                    right: Box::new(value_node),
                };
                let outer_assign = IRNode::BinaryExpr {
                    left: Box::new(IRNode::ElementAccess {
                        object: Box::new(IRNode::Identifier(enum_name.to_string().into())),
                        index: Box::new(inner_assign),
                    }),
                    operator: "=".to_string().into(),
                    right: Box::new(self.member_name_ir_node(name_idx, &member_name)),
                };
                IRNode::ExpressionStatement(Box::new(outer_assign))
            };

            // Extract leading comment (JSDoc/block comment before the member name)
            let leading_comment = self.extract_leading_comment_at(member_node.pos);
            // Extract trailing inline comment after the enum member (e.g., `/* blue */`)
            // Search from the name end or initializer end, then also from the comma position.
            // We check multiple positions because the comment can appear at different spots:
            // `Cornflower, /* blue */` — comment is after the comma
            // `Cornflower = 0, /* comment */` — comment is after the comma
            // `Cornflower /* comment */,` — comment is after the name
            let name_or_init_end = if let Some(init_node) = self.arena.get(member_data.initializer)
            {
                init_node.end
            } else {
                self.arena
                    .get(member_data.name)
                    .map_or(member_node.end, |n| n.end)
            };
            // Try from name/init end first, then from after the comma (scan for comma in source)
            let trailing_comment =
                self.extract_trailing_comment_at(name_or_init_end)
                    .or_else(|| {
                        // Scan forward from name_or_init_end to find the comma, then check after it
                        if let Some(source_text) = self.source_text {
                            let bytes = source_text.as_bytes();
                            let mut pos = name_or_init_end as usize;
                            while pos < bytes.len() && bytes[pos] != b',' && bytes[pos] != b'}' {
                                if bytes[pos] == b'\n' {
                                    return None; // Stop at newline
                                }
                                pos += 1;
                            }
                            if pos < bytes.len() && bytes[pos] == b',' {
                                return self.extract_trailing_comment_at((pos + 1) as u32);
                            }
                        }
                        None
                    });

            // Insert leading comment before the member statement
            if let Some(text) = leading_comment {
                let is_block = text.starts_with("/*");
                // Strip the `/*` / `/**` prefix and `*/` suffix for the Comment node text
                let inner = if is_block {
                    text[2..text.len().saturating_sub(2)].to_string()
                } else {
                    text
                };
                statements.push(IRNode::Comment {
                    text: inner.into(),
                    is_block,
                });
            }

            // Track the evaluated value for use in subsequent member initializers
            if let Some(val) = self.last_value {
                self.member_values.insert(member_name.clone(), val);
            }
            self.processed_members.insert(member_name.clone());
            self.member_names.insert(member_name);
            statements.push(stmt);

            // Insert trailing comment after the member statement (same line).
            // Only preserve block comments (/* ... */); tsc strips line comments (//) from
            // enum members during the transform phase.
            if let Some(text) = trailing_comment
                && text.starts_with("/*")
            {
                statements.push(IRNode::TrailingComment(text.into()));
            }
        }

        statements
    }

    /// Transform an expression node to IR
    fn transform_expression(&self, idx: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(idx) else {
            return IRNode::NumericLiteral("0".to_string().into());
        };

        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    // tsc evaluates numeric literals and emits their JS representation.
                    // E.g., 1e999 → Infinity (not the source text "1e999").
                    if lit.text.parse::<i64>().is_err()
                        && let Ok(val) = lit.text.parse::<f64>()
                        && val.is_infinite()
                    {
                        return IRNode::NumericLiteral("Infinity".to_string().into());
                    }
                    IRNode::NumericLiteral(lit.text.clone().into())
                } else {
                    IRNode::NumericLiteral("0".to_string().into())
                }
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                // Use ASTRef to preserve original quote style from source text.
                // IRNode::StringLiteral always emits double quotes, but source may
                // use single quotes (e.g., `'foo'.length` inside an enum initializer).
                if self.source_text.is_some() {
                    IRNode::ASTRef(idx)
                } else if let Some(lit) = self.arena.get_literal(node) {
                    IRNode::StringLiteral(lit.text.clone().into())
                } else {
                    IRNode::StringLiteral(String::new().into())
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
                            object: Box::new(IRNode::Identifier(
                                self.current_enum_name.clone().into(),
                            )),
                            property: id.escaped_text.clone().into(),
                        }
                    } else {
                        IRNode::Identifier(id.escaped_text.clone().into())
                    }
                } else {
                    IRNode::Identifier("unknown".to_string().into())
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.arena.get_binary_expr(node) {
                    IRNode::BinaryExpr {
                        left: Box::new(self.transform_expression(bin.left)),
                        operator: self.emit_operator(bin.operator_token).into(),
                        right: Box::new(self.transform_expression(bin.right)),
                    }
                } else {
                    IRNode::NumericLiteral("0".to_string().into())
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr(node) {
                    IRNode::PrefixUnaryExpr {
                        operator: self.emit_operator(unary.operator).into(),
                        operand: Box::new(self.transform_expression(unary.operand)),
                    }
                } else {
                    IRNode::NumericLiteral("0".to_string().into())
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.arena.get_parenthesized(node) {
                    IRNode::Parenthesized(Box::new(self.transform_expression(paren.expression)))
                } else {
                    IRNode::NumericLiteral("0".to_string().into())
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
                        property: prop.into(),
                    }
                } else {
                    IRNode::NumericLiteral("0".to_string().into())
                }
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    IRNode::ElementAccess {
                        object: Box::new(self.transform_expression(access.expression)),
                        index: Box::new(self.transform_expression(access.name_or_argument)),
                    }
                } else {
                    IRNode::NumericLiteral("0".to_string().into())
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
                    IRNode::NumericLiteral("0".to_string().into())
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
                            return IRNode::Raw(raw.to_string().into());
                        }
                    }
                }
                IRNode::NumericLiteral("0".to_string().into())
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
                            return IRNode::Raw(raw.to_string().into());
                        }
                    }
                }
                IRNode::NumericLiteral("0".to_string().into())
            }
        }
    }

    fn emit_operator(&self, op: u16) -> String {
        crate::transforms::emit_utils::operator_to_str(op).to_string()
    }

    /// Format an i64 value as an `IRNode` numeric literal, matching tsc's output format.
    fn format_numeric_literal(val: i64) -> IRNode {
        IRNode::NumericLiteral(val.to_string().into())
    }

    /// Check if an enum member name node is a numeric literal, and if so,
    /// return the appropriate `IRNode::NumericLiteral` with the parsed value.
    /// Otherwise return `IRNode::StringLiteral` with the `member_name` text.
    ///
    /// tsc normalizes numeric literal member names:
    ///   `1.0` → `1`, `0xF00D` → `61453`, `11e-1` → `1.1`
    fn member_name_ir_node(&self, name_idx: NodeIndex, member_name: &str) -> IRNode {
        let Some(node) = self.arena.get(name_idx) else {
            return IRNode::StringLiteral(member_name.to_string().into());
        };
        // Private identifiers (#name) in enum members are parse errors.
        // tsc's TS transformer replaces them with factory.createIdentifier(""),
        // which emits as an empty identifier (no quotes), not an empty string literal.
        if node.kind == SyntaxKind::PrivateIdentifier as u16 {
            return IRNode::Identifier(String::new().into());
        }
        if node.kind == SyntaxKind::NumericLiteral as u16
            && let Some(lit) = self.arena.get_literal(node)
            && let Some(val) = Self::parse_numeric_literal_text(&lit.text)
        {
            // If the value is an exact integer, emit as integer
            if val == val.floor() && val.is_finite() && val.abs() < (i64::MAX as f64) {
                return IRNode::NumericLiteral((val as i64).to_string().into());
            }
            // Otherwise emit as float
            return IRNode::NumericLiteral(format!("{val}").into());
        }
        if node.kind == SyntaxKind::BigIntLiteral as u16 {
            // BigInt literal names like `0n` are emitted as-is (no quotes)
            return IRNode::NumericLiteral(member_name.to_string().into());
        }
        // For string literal member names, use source text to preserve Unicode escapes
        // (e.g., "gold \u2730" must stay as-is, not be decoded to the literal char).
        if node.kind == SyntaxKind::StringLiteral as u16 {
            if let Some(source_text) = self.source_text {
                let start = node.pos as usize;
                let end = node.end as usize;
                if end <= source_text.len() {
                    let raw = &source_text[start..end];
                    // Strip surrounding quotes from source text to get inner content
                    let inner = raw
                        .strip_prefix('"')
                        .or_else(|| raw.strip_prefix('\''))
                        .and_then(|s| s.strip_suffix('"').or_else(|| s.strip_suffix('\'')))
                        .unwrap_or(raw);
                    return IRNode::RawStringLiteral(inner.to_string().into());
                }
            }
        }
        IRNode::StringLiteral(member_name.to_string().into())
    }

    /// Parse a numeric literal source text to its f64 value.
    /// Handles decimal, hex (0x), binary (0b), octal (0o), and scientific notation.
    fn parse_numeric_literal_text(text: &str) -> Option<f64> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        if text.starts_with("0x") || text.starts_with("0X") {
            u64::from_str_radix(&text[2..], 16).ok().map(|v| v as f64)
        } else if text.starts_with("0b") || text.starts_with("0B") {
            u64::from_str_radix(&text[2..], 2).ok().map(|v| v as f64)
        } else if text.starts_with("0o") || text.starts_with("0O") {
            u64::from_str_radix(&text[2..], 8).ok().map(|v| v as f64)
        } else {
            text.parse::<f64>().ok()
        }
    }

    /// Format a float value as an IR node.
    /// Handles special values: Infinity, -Infinity, NaN.
    fn format_float_literal(val: f64) -> IRNode {
        if val.is_nan() {
            IRNode::Identifier("NaN".to_string().into())
        } else if val.is_infinite() {
            if val.is_sign_positive() {
                IRNode::Identifier("Infinity".to_string().into())
            } else {
                IRNode::PrefixUnaryExpr {
                    operator: "-".to_string().into(),
                    operand: Box::new(IRNode::Identifier("Infinity".to_string().into())),
                }
            }
        } else {
            // Format with enough precision to round-trip
            let s = format!("{val}");
            IRNode::NumericLiteral(s.into())
        }
    }

    /// Try to evaluate a constant expression as a float value.
    /// Used as fallback when integer evaluation fails (e.g., 0.1, 1/0).
    fn evaluate_constant_float_expression(&self, idx: NodeIndex) -> Option<f64> {
        let node = self.arena.get(idx)?;

        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.arena.get_literal(node)?;
                lit.text.parse::<f64>().ok()
            }
            k if k == SyntaxKind::Identifier as u16 => {
                let id = self.arena.get_identifier(node)?;
                // Check integer members first, promote to f64
                if let Some(&val) = self.member_values.get(id.escaped_text.as_str()) {
                    return Some(val as f64);
                }
                None
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let bin = self.arena.get_binary_expr(node)?;
                let left = self.evaluate_constant_float_expression(bin.left)?;
                let right = self.evaluate_constant_float_expression(bin.right)?;
                let op = bin.operator_token;
                match op {
                    o if o == SyntaxKind::PlusToken as u16 => Some(left + right),
                    o if o == SyntaxKind::MinusToken as u16 => Some(left - right),
                    o if o == SyntaxKind::AsteriskToken as u16 => Some(left * right),
                    o if o == SyntaxKind::SlashToken as u16 => Some(left / right),
                    o if o == SyntaxKind::PercentToken as u16 => Some(left % right),
                    _ => None,
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.arena.get_unary_expr(node)?;
                let operand = self.evaluate_constant_float_expression(unary.operand)?;
                match unary.operator {
                    o if o == SyntaxKind::MinusToken as u16 => Some(-operand),
                    o if o == SyntaxKind::PlusToken as u16 => Some(operand),
                    o if o == SyntaxKind::TildeToken as u16 => Some(!(operand as i64) as f64),
                    _ => None,
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let paren = self.arena.get_parenthesized(node)?;
                self.evaluate_constant_float_expression(paren.expression)
            }
            _ => None,
        }
    }

    /// Check if an expression is a forward reference to a member of the current enum
    /// that hasn't been processed yet. TSC resolves such references to 0.
    ///
    /// A forward reference is when member X references member Y where Y appears later
    /// in the enum body and hasn't been emitted yet. Self-references (A = E.A) and
    /// references to already-processed members with non-numeric values are NOT forward
    /// references — they should keep their original expression form.
    fn is_forward_enum_reference(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        match node.kind {
            // Bare identifier: `Y` where Y is a member of the current enum
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(id) = self.arena.get_identifier(node) {
                    let name = id.escaped_text.as_str();
                    self.member_names.contains(name)
                        && !self.processed_members.contains(name)
                        && name != self.current_member_name
                } else {
                    false
                }
            }
            // Property access: `E1.Y` where E1 is the current enum
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node)
                    && let Some(obj_node) = self.arena.get(access.expression)
                    && obj_node.kind == SyntaxKind::Identifier as u16
                    && let Some(obj_id) = self.arena.get_identifier(obj_node)
                    && obj_id.escaped_text == self.current_enum_name
                    && let Some(prop_node) = self.arena.get(access.name_or_argument)
                    && prop_node.kind == SyntaxKind::Identifier as u16
                    && let Some(prop_id) = self.arena.get_identifier(prop_node)
                {
                    let name = prop_id.escaped_text.as_str();
                    self.member_names.contains(name)
                        && !self.processed_members.contains(name)
                        && name != self.current_member_name
                } else {
                    false
                }
            }
            // Element access: `E1["Y"]` where E1 is the current enum
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node)
                    && let Some(obj_node) = self.arena.get(access.expression)
                    && obj_node.kind == SyntaxKind::Identifier as u16
                    && let Some(obj_id) = self.arena.get_identifier(obj_node)
                    && obj_id.escaped_text == self.current_enum_name
                    && let Some(index_node) = self.arena.get(access.name_or_argument)
                    && index_node.kind == SyntaxKind::StringLiteral as u16
                    && let Some(lit) = self.arena.get_literal(index_node)
                {
                    let name = lit.text.as_str();
                    self.member_names.contains(name)
                        && !self.processed_members.contains(name)
                        && name != self.current_member_name
                } else {
                    false
                }
            }
            _ => false,
        }
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
                // Check current enum members first
                if let Some(&val) = self.member_values.get(id.escaped_text.as_str()) {
                    return Some(val);
                }
                // Check prior blocks of the same merged enum
                if let Some(prior) = self.prior_enum_values.get(&self.current_enum_name)
                    && let Some(&val) = prior.get(id.escaped_text.as_str())
                {
                    return Some(val);
                }
                None
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                // Resolve E.Member references
                let access = self.arena.get_access_expr(node)?;
                let obj_node = self.arena.get(access.expression)?;
                if obj_node.kind == SyntaxKind::Identifier as u16
                    && let Some(obj_id) = self.arena.get_identifier(obj_node)
                {
                    let prop_node = self.arena.get(access.name_or_argument)?;
                    let prop_id = self.arena.get_identifier(prop_node)?;

                    // Same enum self-reference
                    if obj_id.escaped_text == self.current_enum_name
                        && let Some(&val) = self.member_values.get(prop_id.escaped_text.as_str())
                    {
                        return Some(val);
                    }
                    // Cross-enum reference (Foo.A from within enum Bar)
                    if let Some(enum_vals) =
                        self.prior_enum_values.get(obj_id.escaped_text.as_str())
                        && let Some(&val) = enum_vals.get(prop_id.escaped_text.as_str())
                    {
                        return Some(val);
                    }
                }
                None
            }
            // Element access: Enum["Member"] → resolve like property access
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr(node)?;
                let obj_node = self.arena.get(access.expression)?;
                if obj_node.kind == SyntaxKind::Identifier as u16
                    && let Some(obj_id) = self.arena.get_identifier(obj_node)
                {
                    // Get the string key from the index expression
                    let index_node = self.arena.get(access.name_or_argument)?;
                    let member_name = if index_node.kind == SyntaxKind::StringLiteral as u16 {
                        self.arena
                            .get_literal(index_node)
                            .map(|lit| lit.text.as_str())
                    } else {
                        None
                    };
                    if let Some(member_name) = member_name {
                        // Same enum self-reference
                        if obj_id.escaped_text == self.current_enum_name
                            && let Some(&val) = self.member_values.get(member_name)
                        {
                            return Some(val);
                        }
                        // Cross-enum reference
                        if let Some(enum_vals) =
                            self.prior_enum_values.get(obj_id.escaped_text.as_str())
                            && let Some(&val) = enum_vals.get(member_name)
                        {
                            return Some(val);
                        }
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

    /// Evaluate a string enum member initializer at compile time.
    /// Returns `Some(folded_string)` when the expression can be constant-folded.
    /// Handles: string literals, string concatenation (`"a" + "b"` → `"ab"`),
    /// mixed string+numeric (`"a" + 1` → `"a1"`), and references to
    /// previously evaluated string or numeric enum members.
    fn evaluate_string_expression(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                let lit = self.arena.get_literal(node)?;
                Some(lit.text.clone())
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.arena.get_literal(node)?;
                // Parse and format to match tsc behavior
                if let Ok(n) = lit.text.parse::<i64>() {
                    Some(n.to_string())
                } else if let Ok(f) = lit.text.parse::<f64>() {
                    Some(f.to_string())
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                // Template expression with substitutions: `head${expr}middle${expr}tail`
                let tmpl = self.arena.get_template_expr(node)?;
                let head_node = self.arena.get(tmpl.head)?;
                let head_lit = self.arena.get_literal(head_node)?;
                let mut result = head_lit.text.clone();
                for &span_idx in &tmpl.template_spans.nodes {
                    let span_node = self.arena.get(span_idx)?;
                    let span = self.arena.get_template_span(span_node)?;
                    // Evaluate the expression part
                    let expr_val = self.evaluate_string_expression(span.expression)?;
                    result.push_str(&expr_val);
                    // Get the literal tail part
                    let lit_node = self.arena.get(span.literal)?;
                    let lit = self.arena.get_literal(lit_node)?;
                    result.push_str(&lit.text);
                }
                Some(result)
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let bin = self.arena.get_binary_expr(node)?;
                if bin.operator_token != SyntaxKind::PlusToken as u16 {
                    return None;
                }
                let left = self.evaluate_string_expression(bin.left)?;
                let right = self.evaluate_string_expression(bin.right)?;
                Some(format!("{left}{right}"))
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let paren = self.arena.get_parenthesized(node)?;
                self.evaluate_string_expression(paren.expression)
            }
            k if k == SyntaxKind::Identifier as u16 => {
                let id = self.arena.get_identifier(node)?;
                // Check current enum members first
                if let Some(s) = self.string_member_values.get(id.escaped_text.as_str()) {
                    return Some(s.clone());
                }
                if let Some(&n) = self.member_values.get(id.escaped_text.as_str()) {
                    return Some(n.to_string());
                }
                // Check prior blocks of the same merged enum
                if let Some(prior) = self.prior_enum_values.get(&self.current_enum_name)
                    && let Some(&n) = prior.get(id.escaped_text.as_str())
                {
                    return Some(n.to_string());
                }
                None
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr(node)?;
                let obj_node = self.arena.get(access.expression)?;
                if obj_node.kind != SyntaxKind::Identifier as u16 {
                    return None;
                }
                let obj_id = self.arena.get_identifier(obj_node)?;
                let prop_node = self.arena.get(access.name_or_argument)?;
                let prop_id = self.arena.get_identifier(prop_node)?;

                // Same enum self-reference
                if obj_id.escaped_text == self.current_enum_name {
                    if let Some(s) = self.string_member_values.get(prop_id.escaped_text.as_str()) {
                        return Some(s.clone());
                    }
                    if let Some(&n) = self.member_values.get(prop_id.escaped_text.as_str()) {
                        return Some(n.to_string());
                    }
                }
                // Cross-enum reference
                if let Some(prior) = self.prior_enum_values.get(obj_id.escaped_text.as_str())
                    && let Some(&n) = prior.get(prop_id.escaped_text.as_str())
                {
                    return Some(n.to_string());
                }
                None
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
                    let prop_name = self
                        .arena
                        .get(access.name_or_argument)
                        .and_then(|n| self.arena.get_identifier(n))
                        .map(|id| id.escaped_text.as_str());

                    if obj_is_enum && let Some(name) = prop_name {
                        return self.string_members.contains(name);
                    }

                    // Cross-enum reference: check prior enum string members
                    if let Some(obj_name) = obj_node
                        .and_then(|n| self.arena.get_identifier(n))
                        .map(|id| id.escaped_text.as_str())
                        && let Some(prior) = self.prior_string_members.get(obj_name)
                        && let Some(name) = prop_name
                    {
                        return prior.contains(name);
                    }
                    false
                } else {
                    false
                }
            }
            k if k == SyntaxKind::Identifier as u16 => {
                // Bare identifier that matches a known string member
                if let Some(id) = self.arena.get_identifier(node) {
                    if self.string_members.contains(id.escaped_text.as_str()) {
                        return true;
                    }
                    // Check prior blocks of the same merged enum
                    if let Some(prior) = self.prior_string_members.get(&self.current_enum_name) {
                        return prior.contains(id.escaped_text.as_str());
                    }
                }
                false
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

    /// Set whether const enums should be preserved (emitted instead of erased)
    pub const fn set_preserve_const_enums(&mut self, value: bool) {
        self.transformer.set_preserve_const_enums(value);
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
