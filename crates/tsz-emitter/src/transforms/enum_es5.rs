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

use crate::transforms::emit_utils::is_valid_identifier_name;
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
    /// Names of all members in same-name enum declarations in the current source file.
    /// This lets forward-reference detection see later merged enum blocks.
    merged_member_names: HashSet<String>,
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
    /// Source file containing the enum currently being transformed.
    /// Used to resolve top-level `const` initializers in enum constant expressions.
    current_source_file: Option<NodeIndex>,
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
    /// Previously-evaluated string enum member values from other enums.
    /// Keyed by `enum_name` → `member_name` → value.
    prior_string_values: HashMap<String, HashMap<String, String>>,
    /// Whether this enum should emit its own `var E;` declaration.
    emit_var_declaration: bool,
    /// Structured module export fold for the enum IIFE tail.
    export_fold: Option<EnumExportFold>,
}

#[derive(Clone, Debug)]
enum EnumExportFold {
    /// Source-ordered list of CJS export aliases for the enum's local name.
    /// The emitter chains them so the local-name assignment is right-most:
    /// `["E", "EE"]` produces `(E || (exports.EE = exports.E = E = {}))`.
    CommonJs {
        export_names: Vec<String>,
    },
    System {
        export_names: Vec<String>,
    },
}

fn commonjs_export_access(export_name: &str) -> IRNode {
    let exports = IRNode::Identifier("exports".into());
    if is_valid_identifier_name(export_name) {
        IRNode::PropertyAccess {
            object: Box::new(exports),
            property: export_name.to_string().into(),
        }
    } else {
        IRNode::ElementAccess {
            object: Box::new(exports),
            index: Box::new(IRNode::StringLiteral(export_name.to_string().into())),
        }
    }
}

impl<'a> EnumES5Transformer<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        EnumES5Transformer {
            arena,
            last_value: None,
            last_float_value: None,
            source_text: None,
            member_names: HashSet::new(),
            merged_member_names: HashSet::new(),
            string_members: HashSet::new(),
            processed_members: HashSet::new(),
            current_member_name: String::new(),
            member_values: HashMap::new(),
            string_member_values: HashMap::new(),
            current_source_file: None,
            current_enum_name: String::new(),
            preserve_const_enums: false,
            prior_enum_values: HashMap::new(),
            prior_string_members: HashMap::new(),
            prior_string_values: HashMap::new(),
            emit_var_declaration: true,
            export_fold: None,
        }
    }

    pub const fn set_preserve_const_enums(&mut self, value: bool) {
        self.preserve_const_enums = value;
    }

    pub const fn set_emit_var_declaration(&mut self, value: bool) {
        self.emit_var_declaration = value;
    }

    pub fn set_commonjs_export_fold(&mut self, export_name: &str) {
        self.set_commonjs_export_folds([export_name]);
    }

    /// Fold one or more CommonJS export bindings into the enum IIFE tail.
    ///
    /// `export_names` must be in **source order**: the directly-exported name
    /// first, followed by any later `export { local as alias }` re-exports.
    /// The emitter inverts this list when building the chain so the local
    /// assignment is the right-most node (e.g. `["E", "EE"]` →
    /// `exports.EE = exports.E = E = {}`).
    pub fn set_commonjs_export_folds<'b>(
        &mut self,
        export_names: impl IntoIterator<Item = &'b str>,
    ) {
        let mut collected: Vec<String> = Vec::new();
        for name in export_names {
            if name.is_empty() {
                continue;
            }
            if collected.iter().any(|existing| existing == name) {
                continue;
            }
            collected.push(name.to_string());
        }
        if collected.is_empty() {
            self.export_fold = None;
        } else {
            self.export_fold = Some(EnumExportFold::CommonJs {
                export_names: collected,
            });
        }
    }

    pub fn set_system_export_fold(&mut self, export_name: &str) {
        self.set_system_export_folds([export_name]);
    }

    pub fn set_system_export_folds<'b>(&mut self, export_names: impl IntoIterator<Item = &'b str>) {
        self.export_fold = Some(EnumExportFold::System {
            export_names: export_names
                .into_iter()
                .filter(|name| !name.is_empty())
                .map(ToOwned::to_owned)
                .collect(),
        });
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

    /// Set previously-evaluated string enum member values for cross-enum folding.
    pub fn set_prior_string_values(
        &mut self,
        values: &rustc_hash::FxHashMap<String, rustc_hash::FxHashMap<String, String>>,
    ) {
        self.prior_string_values = values
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    v.iter().map(|(mk, mv)| (mk.clone(), mv.clone())).collect(),
                )
            })
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

    /// Get evaluated string member values for this enum.
    pub const fn get_string_member_values(&self) -> &HashMap<String, String> {
        &self.string_member_values
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
        self.current_source_file = self.containing_source_file(enum_idx);
        self.merged_member_names = self.collect_merged_enum_member_names(enum_idx, &name);

        // Build IR for: var E; (function (E) { ... })(E || (E = {}));
        let mut statements = Vec::new();

        // var E;
        if self.emit_var_declaration {
            statements.push(IRNode::VarDecl {
                name: name.clone().into(),
                initializer: None,
            });
        }

        let body_open_pos = self.find_enum_body_open_pos(enum_node);
        let body = self.transform_members(&enum_data.members, &name, body_open_pos);

        let iife_arg = self.enum_iife_argument(&name);

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

    fn enum_iife_argument(&self, enum_name: &str) -> IRNode {
        let plain_assignment = || IRNode::BinaryExpr {
            left: Box::new(IRNode::Identifier(enum_name.to_string().into())),
            operator: "=".into(),
            right: Box::new(IRNode::empty_object()),
        };

        let right = match &self.export_fold {
            None => plain_assignment(),
            Some(EnumExportFold::CommonJs { export_names }) => {
                // Inside-out: forward iteration places the source-latest alias
                // outermost so the chain reads `exports.LastAlias = ... = E = {}`.
                let mut folded = plain_assignment();
                for export_name in export_names {
                    folded = IRNode::BinaryExpr {
                        left: Box::new(commonjs_export_access(export_name)),
                        operator: "=".into(),
                        right: Box::new(folded),
                    };
                }
                folded
            }
            Some(EnumExportFold::System { export_names }) => {
                let mut folded = plain_assignment();
                for export_name in export_names {
                    folded = IRNode::CallExpr {
                        callee: Box::new(IRNode::Identifier("exports_1".into())),
                        arguments: vec![IRNode::StringLiteral(export_name.clone().into()), folded],
                    };
                }
                IRNode::Parenthesized(Box::new(folded))
            }
        };

        IRNode::LogicalOr {
            left: Box::new(IRNode::Identifier(enum_name.to_string().into())),
            right: Box::new(right),
        }
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

    /// Find the source-text position immediately after the enum body's
    /// opening `{`. Returns `None` when the source is unavailable or the
    /// brace cannot be located.
    fn find_enum_body_open_pos(&self, enum_node: &tsz_parser::parser::node::Node) -> Option<u32> {
        let source_text = self.source_text?;
        let bytes = source_text.as_bytes();
        let start = enum_node.pos as usize;
        let end = (enum_node.end as usize).min(bytes.len());
        let mut i = start;
        while i < end {
            if bytes[i] == b'{' {
                return Some((i + 1) as u32);
            }
            i += 1;
        }
        None
    }

    /// Collect every leading comment (line or block) that appears between
    /// `scan_start` and `member_pos`.
    ///
    /// Mirrors tsc's `getLeadingCommentRanges`: comments are attached to the
    /// next member only once scanning has crossed a line break from the enum
    /// body's `{` or the previous member's `,`. Same-line block comments
    /// immediately after those boundaries are trailing trivia and are not
    /// emitted before the next synthesized assignment.
    fn extract_leading_comments_between(&self, scan_start: u32, member_pos: u32) -> Vec<String> {
        let Some(source_text) = self.source_text else {
            return Vec::new();
        };
        if scan_start >= member_pos {
            return Vec::new();
        }
        let mut comments = Vec::new();
        for range in crate::emitter::get_leading_comment_ranges(source_text, scan_start as usize) {
            if range.end > member_pos {
                break;
            }
            if !source_text[scan_start as usize..range.pos as usize].contains('\n') {
                continue;
            }
            let text = &source_text[range.pos as usize..range.end as usize];
            comments.push(text.to_string());
        }
        comments
    }

    /// Scan past the comma (and any trailing same-line whitespace) following an
    /// enum member, so subsequent leading-comment scans don't see the comma as
    /// a boundary. Returns the position to start scanning from for the next
    /// member's leading comments.
    fn scan_past_member_terminator(&self, after_member: u32) -> u32 {
        let Some(source_text) = self.source_text else {
            return after_member;
        };
        let bytes = source_text.as_bytes();
        let len = bytes.len();
        let mut i = after_member as usize;
        // Skip same-line whitespace, then take an optional comma.
        while i < len && matches!(bytes[i], b' ' | b'\t') {
            i += 1;
        }
        if i < len && bytes[i] == b',' {
            i += 1;
        }
        i as u32
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
    fn transform_members(
        &mut self,
        members: &NodeList,
        enum_name: &str,
        body_open_pos: Option<u32>,
    ) -> Vec<IRNode> {
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

        // Position where we should start scanning for the *next* member's
        // leading comments. Initially set just past the enum body's opening
        // brace; after each member it advances past that member's trailing
        // comma so comments between members are seen exactly once.
        let mut comment_scan_pos = body_open_pos;

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

            // Extract every leading comment (line or block) that sits between
            // the previous member terminator (or `{`) and this member's start.
            // tsc preserves both kinds of comments in the lowered IIFE body, so
            // a single trailing-`*/` lookup is not enough.
            let leading_comments = match comment_scan_pos {
                Some(start) => self.extract_leading_comments_between(start, member_node.pos),
                None => Vec::new(),
            };
            // Extract trailing inline comment after the enum member before its comma
            // (e.g. `Cornflower /* blue */,`). Block comments after the comma are
            // boundary-adjacent trailing trivia in tsc and are not preserved.
            let name_or_init_end = if let Some(init_node) = self.arena.get(member_data.initializer)
            {
                init_node.end
            } else {
                self.arena
                    .get(member_data.name)
                    .map_or(member_node.end, |n| n.end)
            };
            let trailing_comment = self.extract_trailing_comment_at(name_or_init_end);

            for text in &leading_comments {
                let is_block = text.starts_with("/*");
                let inner = if is_block {
                    text[2..text.len().saturating_sub(2)].to_string()
                } else if let Some(rest) = text.strip_prefix("//") {
                    rest.trim_start_matches(' ').to_string()
                } else {
                    text.clone()
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

            comment_scan_pos = Some(self.scan_past_member_terminator(member_node.end));
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
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                if let Some(assertion) = self.arena.get_type_assertion(node) {
                    self.transform_expression(assertion.expression)
                } else {
                    IRNode::NumericLiteral("0".to_string().into())
                }
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr_ex(node) {
                    self.transform_expression(unary.expression)
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

            // Arrow functions need normal AST printing so parser-recovered
            // arrows are emitted in canonical form instead of preserving an
            // illegal source line break before `=>`.
            k if k == syntax_kind_ext::ARROW_FUNCTION => IRNode::ASTRef(idx),

            // Function expression: use raw source text
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
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
        if node.is_numeric_literal()
            && let Some(lit) = self.arena.get_literal(node)
            && let Some(val) = tsz_common::numeric::parse_numeric_literal_value(&lit.text)
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
        if node.is_string_literal()
            && let Some(source_text) = self.source_text
        {
            let start = node.pos as usize;
            let end = node.end as usize;
            if end <= source_text.len() {
                let raw = &source_text[start..end];
                let is_single_quoted = raw.starts_with('\'');
                let inner = raw
                    .strip_prefix('"')
                    .or_else(|| raw.strip_prefix('\''))
                    .and_then(|s| s.strip_suffix('"').or_else(|| s.strip_suffix('\'')))
                    .unwrap_or(raw);
                if is_single_quoted {
                    let mut converted = String::with_capacity(inner.len());
                    let mut chars = inner.chars().peekable();
                    while let Some(c) = chars.next() {
                        if c == '\\' {
                            if let Some(&next) = chars.peek() {
                                if next == '\'' {
                                    converted.push('\'');
                                    chars.next();
                                } else {
                                    converted.push('\\');
                                    converted.push(next);
                                    chars.next();
                                }
                            } else {
                                converted.push('\\');
                            }
                        } else if c == '"' {
                            converted.push('\\');
                            converted.push('"');
                        } else {
                            converted.push(c);
                        }
                    }
                    return IRNode::RawStringLiteral(converted.into());
                }
                return IRNode::RawStringLiteral(inner.to_string().into());
            }
        }
        IRNode::StringLiteral(member_name.to_string().into())
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
                if let Some(val) = self.resolve_top_level_const(id.escaped_text.as_str()) {
                    return val.map(|n| n as f64);
                }
                match id.escaped_text.as_str() {
                    "NaN" => Some(f64::NAN),
                    "Infinity" => Some(f64::INFINITY),
                    _ => None,
                }
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
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                let assertion = self.arena.get_type_assertion(node)?;
                self.evaluate_constant_float_expression(assertion.expression)
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                let unary = self.arena.get_unary_expr_ex(node)?;
                self.evaluate_constant_float_expression(unary.expression)
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
                    self.is_known_current_enum_member(name)
                        && !self.processed_members.contains(name)
                        && !self.has_prior_current_enum_member(name)
                        && name != self.current_member_name
                } else {
                    false
                }
            }
            // Property access: `E1.Y` where E1 is the current enum
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node)
                    && let Some(obj_node) = self.arena.get(access.expression)
                    && obj_node.is_identifier()
                    && let Some(obj_id) = self.arena.get_identifier(obj_node)
                    && obj_id.escaped_text == self.current_enum_name
                    && let Some(prop_node) = self.arena.get(access.name_or_argument)
                    && prop_node.is_identifier()
                    && let Some(prop_id) = self.arena.get_identifier(prop_node)
                {
                    let name = prop_id.escaped_text.as_str();
                    self.is_known_current_enum_member(name)
                        && !self.processed_members.contains(name)
                        && !self.has_prior_current_enum_member(name)
                        && name != self.current_member_name
                } else {
                    false
                }
            }
            // Element access: `E1["Y"]` where E1 is the current enum
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node)
                    && let Some(obj_node) = self.arena.get(access.expression)
                    && obj_node.is_identifier()
                    && let Some(obj_id) = self.arena.get_identifier(obj_node)
                    && obj_id.escaped_text == self.current_enum_name
                    && let Some(index_node) = self.arena.get(access.name_or_argument)
                    && index_node.is_string_literal()
                    && let Some(lit) = self.arena.get_literal(index_node)
                {
                    let name = lit.text.as_str();
                    self.is_known_current_enum_member(name)
                        && !self.processed_members.contains(name)
                        && !self.has_prior_current_enum_member(name)
                        && name != self.current_member_name
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn is_known_current_enum_member(&self, name: &str) -> bool {
        self.member_names.contains(name) || self.merged_member_names.contains(name)
    }

    fn has_prior_current_enum_member(&self, name: &str) -> bool {
        self.prior_enum_values
            .get(&self.current_enum_name)
            .is_some_and(|members| members.contains_key(name))
            || self
                .prior_string_values
                .get(&self.current_enum_name)
                .is_some_and(|members| members.contains_key(name))
            || self
                .prior_string_members
                .get(&self.current_enum_name)
                .is_some_and(|members| members.contains(name))
    }

    fn collect_merged_enum_member_names(
        &self,
        enum_idx: NodeIndex,
        enum_name: &str,
    ) -> HashSet<String> {
        let mut names = HashSet::new();
        if enum_name.is_empty() {
            return names;
        }

        let Some(source_file_idx) = self.containing_source_file(enum_idx) else {
            return names;
        };
        let Some(source_file_node) = self.arena.get(source_file_idx) else {
            return names;
        };

        for node in &self.arena.nodes {
            if node.kind != syntax_kind_ext::ENUM_DECLARATION
                || node.pos < source_file_node.pos
                || node.end > source_file_node.end
            {
                continue;
            }
            let Some(enum_data) = self.arena.get_enum(node) else {
                continue;
            };
            let candidate_name =
                crate::transforms::emit_utils::identifier_text_or_empty(self.arena, enum_data.name);
            if candidate_name != enum_name {
                continue;
            }
            for &member_idx in &enum_data.members.nodes {
                let Some(member_node) = self.arena.get(member_idx) else {
                    continue;
                };
                let Some(member_data) = self.arena.get_enum_member(member_node) else {
                    continue;
                };
                names.insert(crate::transforms::emit_utils::enum_member_name(
                    self.arena,
                    member_data.name,
                ));
            }
        }

        names
    }

    /// Build a dotted path from a (possibly nested) property-access expression
    /// or bare identifier. Used to resolve namespace-qualified enum references
    /// like `M.N.E1` to a key in `prior_enum_values`.
    fn build_dotted_path(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        if node.is_identifier() {
            let id = self.arena.get_identifier(node)?;
            return Some(id.escaped_text.to_string());
        }
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(node)?;
            let left = self.build_dotted_path(access.expression)?;
            let right_node = self.arena.get(access.name_or_argument)?;
            let right_id = self.arena.get_identifier(right_node)?;
            return Some(format!("{left}.{}", right_id.escaped_text));
        }
        None
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

    fn resolve_top_level_const(&self, name: &str) -> Option<Option<i64>> {
        let source_file_idx = self.current_source_file?;
        let source_file_node = self.arena.get(source_file_idx)?;
        let source_file = self.arena.get_source_file(source_file_node)?;

        for &stmt_idx in &source_file.statements.nodes {
            let stmt_node = self.arena.get(stmt_idx)?;
            if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                continue;
            }
            let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
                continue;
            };
            for &decl_list_idx in &var_stmt.declarations.nodes {
                let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                    continue;
                };
                let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                    continue;
                };
                for &decl_idx in &decl_list.declarations.nodes {
                    if !self.arena.is_const_variable_declaration(decl_idx) {
                        continue;
                    }
                    let Some(decl_node) = self.arena.get(decl_idx) else {
                        continue;
                    };
                    let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                        continue;
                    };
                    let Some(decl_name) = self.arena.get(decl.name) else {
                        continue;
                    };
                    let Some(ident) = self.arena.get_identifier(decl_name) else {
                        continue;
                    };
                    if ident.escaped_text == name {
                        return Some(
                            decl.initializer
                                .is_some()
                                .then(|| self.evaluate_constant_expression(decl.initializer))
                                .flatten(),
                        );
                    }
                }
            }
        }

        None
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
                if let Some(val) = self.resolve_top_level_const(id.escaped_text.as_str()) {
                    return val;
                }
                None
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                // Resolve E.Member references
                let access = self.arena.get_access_expr(node)?;
                let prop_node = self.arena.get(access.name_or_argument)?;
                let prop_id = self.arena.get_identifier(prop_node)?;
                let obj_node = self.arena.get(access.expression)?;
                if obj_node.is_identifier()
                    && let Some(obj_id) = self.arena.get_identifier(obj_node)
                {
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
                    return None;
                }
                // Multi-level namespace-qualified reference (e.g. `M.N.E1.a`).
                // tsc inlines the constant value at emit time so the JS output
                // does not depend on the namespace IIFE having been evaluated.
                if obj_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    && let Some(qualified) = self.build_dotted_path(access.expression)
                {
                    // Try the full qualified name first (`M.N.E1`).
                    if let Some(enum_vals) = self.prior_enum_values.get(&qualified)
                        && let Some(&val) = enum_vals.get(prop_id.escaped_text.as_str())
                    {
                        return Some(val);
                    }
                    // Fall back to the trailing segment so simple-name keys
                    // (`E1` in `prior_enum_values`) still resolve when the
                    // emitter has not yet recorded a fully-qualified key.
                    if let Some(last) = qualified.rsplit('.').next()
                        && let Some(enum_vals) = self.prior_enum_values.get(last)
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
                if obj_node.is_identifier()
                    && let Some(obj_id) = self.arena.get_identifier(obj_node)
                {
                    // Get the string key from the index expression
                    let index_node = self.arena.get(access.name_or_argument)?;
                    let member_name = if index_node.is_string_literal() {
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
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                let assertion = self.arena.get_type_assertion(node)?;
                self.evaluate_constant_expression(assertion.expression)
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                let unary = self.arena.get_unary_expr_ex(node)?;
                self.evaluate_constant_expression(unary.expression)
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
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                let assertion = self.arena.get_type_assertion(node)?;
                self.evaluate_string_expression(assertion.expression)
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                let unary = self.arena.get_unary_expr_ex(node)?;
                self.evaluate_string_expression(unary.expression)
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
                if let Some(prior) = self.prior_string_values.get(&self.current_enum_name)
                    && let Some(s) = prior.get(id.escaped_text.as_str())
                {
                    return Some(s.clone());
                }
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
                if !obj_node.is_identifier() {
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
                    if let Some(prior) = self.prior_string_values.get(&self.current_enum_name)
                        && let Some(s) = prior.get(prop_id.escaped_text.as_str())
                    {
                        return Some(s.clone());
                    }
                    if let Some(&n) = self.member_values.get(prop_id.escaped_text.as_str()) {
                        return Some(n.to_string());
                    }
                }
                // Cross-enum reference
                if let Some(prior) = self.prior_string_values.get(obj_id.escaped_text.as_str())
                    && let Some(value) = prior.get(prop_id.escaped_text.as_str())
                {
                    return Some(value.clone());
                }
                if let Some(prior) = self.prior_enum_values.get(obj_id.escaped_text.as_str())
                    && let Some(&n) = prior.get(prop_id.escaped_text.as_str())
                {
                    return Some(n.to_string());
                }
                None
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr(node)?;
                let obj_node = self.arena.get(access.expression)?;
                if !obj_node.is_identifier() {
                    return None;
                }
                let obj_id = self.arena.get_identifier(obj_node)?;
                let member_name = self.string_literal_key(access.name_or_argument)?;

                if obj_id.escaped_text == self.current_enum_name {
                    if let Some(s) = self.string_member_values.get(member_name.as_str()) {
                        return Some(s.clone());
                    }
                    if let Some(prior) = self.prior_string_values.get(&self.current_enum_name)
                        && let Some(s) = prior.get(member_name.as_str())
                    {
                        return Some(s.clone());
                    }
                    if let Some(&n) = self.member_values.get(member_name.as_str()) {
                        return Some(n.to_string());
                    }
                }
                if let Some(prior) = self.prior_string_values.get(obj_id.escaped_text.as_str())
                    && let Some(value) = prior.get(member_name.as_str())
                {
                    return Some(value.clone());
                }
                if let Some(prior) = self.prior_enum_values.get(obj_id.escaped_text.as_str())
                    && let Some(&n) = prior.get(member_name.as_str())
                {
                    return Some(n.to_string());
                }
                None
            }
            _ => None,
        }
    }

    fn string_literal_key(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        if node.kind == SyntaxKind::StringLiteral as u16
            || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            return self.arena.get_literal(node).map(|lit| lit.text.clone());
        }
        None
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
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                if let Some(assertion) = self.arena.get_type_assertion(node) {
                    self.is_syntactically_string(assertion.expression)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr_ex(node) {
                    self.is_syntactically_string(unary.expression)
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
                        n.is_identifier()
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
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    let obj_node = self.arena.get(access.expression);
                    let obj_name = obj_node
                        .and_then(|n| self.arena.get_identifier(n))
                        .map(|id| id.escaped_text.as_str());
                    let Some(member_name) = self.string_literal_key(access.name_or_argument) else {
                        return false;
                    };

                    if obj_name == Some(self.current_enum_name.as_str()) {
                        return self.string_members.contains(member_name.as_str());
                    }
                    if let Some(obj_name) = obj_name
                        && let Some(prior) = self.prior_string_members.get(obj_name)
                    {
                        return prior.contains(member_name.as_str());
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

    /// Set whether the enum should emit its own `var E;` declaration.
    pub const fn set_emit_var_declaration(&mut self, value: bool) {
        self.transformer.set_emit_var_declaration(value);
    }

    /// Fold a CommonJS export binding into the enum IIFE tail.
    pub fn set_commonjs_export_fold(&mut self, export_name: &str) {
        self.transformer.set_commonjs_export_fold(export_name);
    }

    pub fn set_commonjs_export_folds<'b>(
        &mut self,
        export_names: impl IntoIterator<Item = &'b str>,
    ) {
        self.transformer.set_commonjs_export_folds(export_names);
    }

    /// Fold a System export call into the enum IIFE tail.
    pub fn set_system_export_fold(&mut self, export_name: &str) {
        self.transformer.set_system_export_fold(export_name);
    }

    /// Fold multiple System export calls into the enum IIFE tail.
    pub fn set_system_export_folds<'b>(&mut self, export_names: impl IntoIterator<Item = &'b str>) {
        self.transformer.set_system_export_folds(export_names);
    }

    /// Emit an enum declaration
    /// Returns empty string for const enums (they are erased)
    pub fn emit_enum(&mut self, enum_idx: NodeIndex) -> String {
        let ir = self.transformer.transform_enum(enum_idx);
        let ir = match ir {
            Some(ir) => ir,
            None => return String::new(),
        };

        // ASTRef nodes (used for string literals to preserve source quote
        // style) require both arena and source text to print; without them
        // the printer falls back to "undefined".
        let arena = self.transformer.arena;
        let mut printer = match self.transformer.source_text {
            Some(text) => IRPrinter::with_arena_and_source(arena, text),
            None => IRPrinter::with_arena(arena),
        };
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
