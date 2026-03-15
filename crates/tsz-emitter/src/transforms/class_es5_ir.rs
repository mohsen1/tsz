//! ES5 Class Transform (IR-based)
//!
//! Transforms ES6 classes to ES5 IIFE patterns, producing IR nodes.
//!
//! ```typescript
//! class Animal {
//!     constructor(name) { this.name = name; }
//!     speak() { console.log(this.name); }
//! }
//! ```
//!
//! Becomes IR that prints as:
//!
//! ```javascript
//! var Animal = /** @class */ (function () {
//!     function Animal(name) {
//!         this.name = name;
//!     }
//!     Animal.prototype.speak = function () {
//!         console.log(this.name);
//!     };
//!     return Animal;
//! }());
//! ```
//!
//! ## Derived Classes with `super()`
//!
//! ```typescript
//! class Dog extends Animal {
//!     constructor(name) {
//!         super(name);
//!         this.breed = "mixed";
//!     }
//! }
//! ```
//!
//! Becomes:
//!
//! ```javascript
//! var Dog = /** @class */ (function (_super) {
//!     __extends(Dog, _super);
//!     function Dog(name) {
//!         var _this = _super.call(this, name) || this;
//!         _this.breed = "mixed";
//!         return _this;
//!     }
//!     return Dog;
//! }(Animal));
//! ```
//!
//! ## Architecture
//!
//! This transformer fully converts class bodies to IR nodes using the `AstToIr` converter,
//! which handles most JavaScript statements and expressions. The thin wrapper in
//! `class_es5.rs` uses this transformer with `IRPrinter` to emit JavaScript.
//!
//! Supported features:
//! - Simple and derived classes with extends
//! - Constructors with `super()` calls
//! - Instance and static methods
//! - Instance and static properties
//! - Getters and setters (combined into Object.defineProperty)
//! - Private fields (`WeakMap` pattern)
//! - Parameter properties (public/private/protected/readonly)
//! - Async methods (__awaiter wrapper)
//! - Computed property names
//! - Static blocks
//!
//! The `AstToIr` converter handles most JavaScript constructs. For complex or edge cases
//! not yet supported, it falls back to `IRNode::ASTRef` which copies source text directly.

#[path = "class_es5_ast_to_ir.rs"]
pub mod ast_to_ir;
pub use ast_to_ir::AstToIr;

#[path = "class_es5_ir_members.rs"]
mod members;

use crate::context::transform::TransformContext;
use crate::transforms::ir::{
    IRCatchClause, IRNode, IRParam, IRProperty, IRPropertyKey, IRPropertyKind, IRSwitchCase,
};
use crate::transforms::private_fields_es5::{
    PrivateAccessorInfo, PrivateFieldInfo, collect_private_accessors, collect_private_fields,
};
use rustc_hash::FxHashMap;
use std::cell::Cell;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_parser::syntax::transform_utils::contains_this_reference;
use tsz_parser::syntax::transform_utils::is_private_identifier;
use tsz_scanner::SyntaxKind;

/// Serialize a type annotation to a metadata runtime type string.
/// Mirrors the `Printer::serialize_type_for_metadata` logic for ES5 context.
fn serialize_type_for_metadata(arena: &NodeArena, type_idx: NodeIndex) -> String {
    let Some(type_node) = arena.get(type_idx) else {
        return "Object".to_string();
    };
    let sk = |s: SyntaxKind| s as u16;
    match type_node.kind {
        k if k == sk(SyntaxKind::StringKeyword) => "String".to_string(),
        k if k == sk(SyntaxKind::NumberKeyword) => "Number".to_string(),
        k if k == sk(SyntaxKind::BooleanKeyword) => "Boolean".to_string(),
        k if k == sk(SyntaxKind::SymbolKeyword) => "Symbol".to_string(),
        k if k == sk(SyntaxKind::BigIntKeyword) => "BigInt".to_string(),
        k if k == sk(SyntaxKind::VoidKeyword)
            || k == sk(SyntaxKind::UndefinedKeyword)
            || k == sk(SyntaxKind::NullKeyword)
            || k == sk(SyntaxKind::NeverKeyword) =>
        {
            "void 0".to_string()
        }
        k if k == sk(SyntaxKind::AnyKeyword)
            || k == sk(SyntaxKind::UnknownKeyword)
            || k == sk(SyntaxKind::ObjectKeyword) =>
        {
            "Object".to_string()
        }
        k if k == syntax_kind_ext::TYPE_REFERENCE => {
            if let Some(type_ref) = arena.get_type_ref(type_node) {
                let name = get_identifier_text(arena, type_ref.type_name).unwrap_or_default();
                match name.as_str() {
                    "string" => "String".to_string(),
                    "number" => "Number".to_string(),
                    "boolean" => "Boolean".to_string(),
                    "symbol" => "Symbol".to_string(),
                    "bigint" => "BigInt".to_string(),
                    "void" | "undefined" | "null" | "never" => "void 0".to_string(),
                    _ => name,
                }
            } else {
                "Object".to_string()
            }
        }
        k if k == syntax_kind_ext::ARRAY_TYPE || k == syntax_kind_ext::TUPLE_TYPE => {
            "Array".to_string()
        }
        k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
            "Function".to_string()
        }
        k if k == syntax_kind_ext::UNION_TYPE => {
            if let Some(composite) = arena.get_composite_type(type_node) {
                let meaningful: Vec<NodeIndex> = composite
                    .types
                    .nodes
                    .iter()
                    .copied()
                    .filter(|&m_idx| {
                        let Some(m) = arena.get(m_idx) else {
                            return false;
                        };
                        if m.kind == sk(SyntaxKind::NullKeyword)
                            || m.kind == sk(SyntaxKind::UndefinedKeyword)
                            || m.kind == sk(SyntaxKind::VoidKeyword)
                            || m.kind == sk(SyntaxKind::NeverKeyword)
                        {
                            return false;
                        }
                        // Skip TypeReference to null/undefined/void/never
                        if m.kind == syntax_kind_ext::TYPE_REFERENCE
                            && let Some(type_ref) = arena.get_type_ref(m)
                        {
                            let ref_name =
                                get_identifier_text(arena, type_ref.type_name).unwrap_or_default();
                            if matches!(ref_name.as_str(), "null" | "undefined" | "void" | "never")
                            {
                                return false;
                            }
                        }
                        true
                    })
                    .collect();
                if meaningful.len() == 1 {
                    return serialize_type_for_metadata(arena, meaningful[0]);
                }
                if meaningful.len() > 1 {
                    let first = serialize_type_for_metadata(arena, meaningful[0]);
                    if first != "Object"
                        && meaningful[1..]
                            .iter()
                            .all(|&m| serialize_type_for_metadata(arena, m) == first)
                    {
                        return first;
                    }
                }
                if meaningful.is_empty() {
                    return "void 0".to_string();
                }
            }
            "Object".to_string()
        }
        k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
            if let Some(wrapped) = arena.get_wrapped_type(type_node) {
                return serialize_type_for_metadata(arena, wrapped.type_node);
            }
            "Object".to_string()
        }
        k if k == syntax_kind_ext::LITERAL_TYPE => {
            if let Some(lit) = arena.get_literal_type(type_node)
                && let Some(lit_node) = arena.get(lit.literal)
            {
                return match lit_node.kind {
                    lk if lk == sk(SyntaxKind::StringLiteral) => "String".to_string(),
                    lk if lk == sk(SyntaxKind::NumericLiteral) => "Number".to_string(),
                    lk if lk == sk(SyntaxKind::BigIntLiteral) => "BigInt".to_string(),
                    lk if lk == sk(SyntaxKind::TrueKeyword)
                        || lk == sk(SyntaxKind::FalseKeyword) =>
                    {
                        "Boolean".to_string()
                    }
                    lk if lk == sk(SyntaxKind::NullKeyword) => "void 0".to_string(),
                    lk if lk == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => "Number".to_string(),
                    _ => "Object".to_string(),
                };
            }
            "Object".to_string()
        }
        k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => "String".to_string(),
        k if k == syntax_kind_ext::TYPE_OPERATOR => {
            if let Some(type_op) = arena.get_type_operator(type_node) {
                return serialize_type_for_metadata(arena, type_op.type_node);
            }
            "Object".to_string()
        }
        k if k == syntax_kind_ext::OPTIONAL_TYPE => {
            if let Some(wrapped) = arena.get_wrapped_type(type_node) {
                return serialize_type_for_metadata(arena, wrapped.type_node);
            }
            "Object".to_string()
        }
        _ => "Object".to_string(),
    }
}

/// Serialize parameter types for `design:paramtypes` metadata.
fn serialize_param_types(arena: &NodeArena, parameters: &NodeList) -> String {
    let mut parts = Vec::new();
    for &param_idx in &parameters.nodes {
        if let Some(param_node) = arena.get(param_idx)
            && let Some(param) = arena.get_parameter(param_node)
        {
            if param.type_annotation.is_some() {
                parts.push(serialize_type_for_metadata(arena, param.type_annotation));
            } else {
                parts.push("Object".to_string());
            }
        }
    }
    parts.join(", ")
}

#[derive(Debug, Clone)]
struct AutoAccessorFieldInfo {
    member_idx: NodeIndex,
    weakmap_name: String,
    initializer: Option<NodeIndex>,
    is_static: bool,
}

/// Context for ES5 class transformation
pub struct ES5ClassTransformer<'a> {
    arena: &'a NodeArena,
    class_name: String,
    has_extends: bool,
    extends_null: bool,
    private_fields: Vec<PrivateFieldInfo>,
    private_accessors: Vec<PrivateAccessorInfo>,
    auto_accessors: Vec<AutoAccessorFieldInfo>,
    /// Transform directives from `LoweringPass`
    transforms: Option<TransformContext>,
    /// Source text for extracting comments
    source_text: Option<&'a str>,
    /// Class-level decorator `NodeIndex` list (for legacy decorator lowering)
    class_decorators: Vec<NodeIndex>,
    /// Whether to emit member decorator __decorate calls inside the IIFE
    legacy_decorators: bool,
    /// Whether to emit `__metadata` calls in `__decorate` arrays
    emit_decorator_metadata: bool,
    /// Base indent level for raw IR strings (0 for top-level, 1+ for nested contexts)
    indent_base: u32,
}

impl<'a> ES5ClassTransformer<'a> {
    pub const fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            class_name: String::new(),
            has_extends: false,
            extends_null: false,
            private_fields: Vec::new(),
            private_accessors: Vec::new(),
            auto_accessors: Vec::new(),
            transforms: None,
            source_text: None,
            class_decorators: Vec::new(),
            legacy_decorators: false,
            emit_decorator_metadata: false,
            indent_base: 0,
        }
    }

    /// Set the base indent level for nested contexts (e.g., 1 for class inside namespace)
    pub const fn set_indent_base(&mut self, level: u32) {
        self.indent_base = level;
    }

    /// Set class-level decorators to emit inside the IIFE
    pub fn set_class_decorators(&mut self, decorators: Vec<NodeIndex>) {
        self.class_decorators = decorators;
    }

    /// Enable legacy decorator lowering (emits __decorate calls for members inside the IIFE)
    pub const fn set_legacy_decorators(&mut self, enabled: bool) {
        self.legacy_decorators = enabled;
    }

    /// Enable `__metadata` emission in `__decorate` arrays
    pub const fn set_emit_decorator_metadata(&mut self, enabled: bool) {
        self.emit_decorator_metadata = enabled;
    }

    /// Set transform directives from `LoweringPass`
    pub fn set_transforms(&mut self, transforms: TransformContext) {
        self.transforms = Some(transforms);
    }

    /// Set source text for comment extraction
    pub const fn set_source_text(&mut self, source_text: &'a str) {
        self.source_text = Some(source_text);
    }

    fn emit_leading_statement_comments(
        &self,
        body: &mut Vec<IRNode>,
        prev_end: u32,
        stmt_pos: u32,
    ) {
        let Some(source_text) = self.source_text else {
            return;
        };
        let start = std::cmp::min(prev_end as usize, source_text.len());
        let end = std::cmp::min(stmt_pos as usize, source_text.len());
        if start >= end {
            return;
        }
        let segment = &source_text[start..end];
        for line in segment.lines() {
            let trimmed = line.trim_start();
            let is_comment =
                trimmed.starts_with("//") || (trimmed.starts_with("/*") && trimmed.ends_with("*/"));
            if is_comment {
                body.push(IRNode::Raw(trimmed.to_string().into()));
            }
        }
    }

    fn source_has_semicolon_between(&self, start: u32, end: u32) -> bool {
        let Some(source_text) = self.source_text else {
            return false;
        };
        let start = std::cmp::min(start as usize, source_text.len());
        let end = std::cmp::min(end as usize, source_text.len());
        start < end && source_text[start..end].contains(';')
    }

    /// Extract leading `JSDoc` comment from a node (if any).
    /// Returns the comment text including the `/** ... */` delimiters.
    ///
    /// Scans backward from `node.pos` (the token start, not including trivia)
    /// looking for an immediately adjacent block comment separated only by
    /// whitespace.  This avoids the pitfall of the old forward-scan approach
    /// which was confused when `node.end` of the previous sibling included
    /// the current member's trivia.
    fn extract_leading_comment(&self, node: &tsz_parser::parser::node::Node) -> Option<String> {
        let source_text = self.source_text?;
        let bytes = source_text.as_bytes();
        let pos = node.pos as usize;
        if pos == 0 {
            return None;
        }

        // Scan backward from `pos` skipping whitespace/newlines.
        // If we find `*/` we look further back for the matching `/*`.
        let mut i = pos;
        // Skip trailing whitespace/newlines before the token
        while i > 0 && matches!(bytes[i - 1], b' ' | b'\t' | b'\r' | b'\n') {
            i -= 1;
        }

        // Check if we landed on `*/` (end of a block comment)
        if i >= 2 && bytes[i - 1] == b'/' && bytes[i - 2] == b'*' {
            let comment_end = i; // exclusive end of comment text
            // Scan backwards to find the matching `/*`
            // We look for the LAST `/*` before this position that is a true
            // comment opener (not inside a string — simplified scan).
            let mut j = i - 2; // j points at `*` of `*/`
            loop {
                if j < 2 {
                    break;
                }
                // Look for `/*` or `/**`
                if bytes[j - 1] == b'/' && bytes[j] == b'*' {
                    // Found `/*` at j-1..j+1
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

        // Check for line comment (`// ...`).
        // At this point `i` is just past the last non-whitespace char before the node.
        // Scan backward to find the start of that line, then check for `//`.
        if i > 0 {
            let line_end = i;
            let mut line_start = i;
            while line_start > 0 && bytes[line_start - 1] != b'\n' {
                line_start -= 1;
            }
            let line = source_text[line_start..line_end].trim_start();
            if line.starts_with("//") {
                return Some(line.to_string());
            }
        }

        None
    }

    /// Extract trailing comment on the same line as a class method's closing `}`.
    ///
    /// Finds the first `}` at brace depth 0 within the body block — that is, the
    /// actual closing brace of the function body — and returns any trailing comment
    /// on the same line.  Previous code scanned the entire body range and picked the
    /// LAST `}` with a trailing comment, which could accidentally pick up the class's
    /// closing brace comment instead of the method's own comment.
    fn extract_trailing_comment_for_method(&self, body_idx: NodeIndex) -> Option<String> {
        let source_text = self.source_text?;
        let body_node = self.arena.get(body_idx)?;
        let bytes = source_text.as_bytes();
        let start = body_node.pos as usize;
        let end = (body_node.end as usize).min(bytes.len());
        if start >= end {
            return None;
        }
        // Track brace depth starting from the opening `{` of the block.
        // We skip the initial opening brace (depth stays 0 initially).
        // For each `{` after that, depth increments; for each `}`, if depth==0
        // we have found the matching closing brace of the block; otherwise decrement.
        let mut depth: usize = 0;
        let mut in_string: Option<u8> = None; // `'` or `"`
        let mut i = start;
        while i < end {
            let byte = bytes[i];
            // Rudimentary string/template literal skip to avoid counting braces inside strings
            if in_string.is_none() {
                match byte {
                    b'{' => {
                        // Skip the opening brace of the body block itself (depth stays 0)
                        if i == start {
                            // opening brace of the block — don't count
                        } else {
                            depth += 1;
                        }
                    }
                    b'}' => {
                        if depth == 0 {
                            // This is the closing brace of the block
                            let after = i + 1;
                            return crate::emitter::get_trailing_comment_ranges(source_text, after)
                                .first()
                                .map(|c| source_text[c.pos as usize..c.end as usize].to_string());
                        }
                        depth -= 1;
                    }
                    b'\'' | b'"' | b'`' => {
                        in_string = Some(byte);
                    }
                    _ => {}
                }
            } else if let Some(delim) = in_string {
                if byte == b'\\' {
                    i += 1; // skip escaped char
                } else if byte == delim {
                    in_string = None;
                }
            }
            i += 1;
        }
        None
    }

    fn extract_trailing_comment_for_node(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> Option<String> {
        let source_text = self.source_text?;
        for comment in crate::emitter::get_trailing_comment_ranges(source_text, node.end as usize) {
            let comment_text = &source_text[comment.pos as usize..comment.end as usize];
            let trimmed = comment_text.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("/*") {
                return Some(comment_text.to_string());
            }
        }

        None
    }

    /// Convert an AST statement to IR (avoids `ASTRef` when possible)
    fn convert_statement(&self, idx: NodeIndex) -> IRNode {
        let mut converter = AstToIr::new(self.arena).with_super(self.has_extends);
        if let Some(ref transforms) = self.transforms {
            converter = converter.with_transforms(transforms.clone());
        }
        converter.convert_statement(idx)
    }

    /// Convert an AST statement to IR with `this` captured as `_this`.
    /// Used in derived constructors after `super()` where `this` → `_this`.
    fn convert_statement_this_captured(&self, idx: NodeIndex) -> IRNode {
        let mut converter = AstToIr::new(self.arena)
            .with_this_captured(true)
            .with_super(self.has_extends);
        if let Some(ref transforms) = self.transforms {
            converter = converter.with_transforms(transforms.clone());
        }
        converter.convert_statement(idx)
    }

    /// Convert an AST expression to IR (avoids `ASTRef` when possible)
    fn convert_expression(&self, idx: NodeIndex) -> IRNode {
        let mut converter = AstToIr::new(self.arena).with_super(self.has_extends);
        if let Some(ref transforms) = self.transforms {
            converter = converter.with_transforms(transforms.clone());
        }
        converter.convert_expression(idx)
    }

    /// Convert an AST statement to IR in static context (super uses `_super.X` not `_super.prototype.X`)
    fn convert_statement_static(&self, idx: NodeIndex) -> IRNode {
        let mut converter = AstToIr::new(self.arena)
            .with_super(self.has_extends)
            .with_static(true);
        if let Some(ref transforms) = self.transforms {
            converter = converter.with_transforms(transforms.clone());
        }
        converter.convert_statement(idx)
    }

    /// Convert an AST statement to IR in static context with class alias for `this` substitution
    fn convert_statement_static_with_class_alias(
        &self,
        idx: NodeIndex,
        class_alias: &str,
    ) -> IRNode {
        let mut converter = AstToIr::new(self.arena)
            .with_super(self.has_extends)
            .with_static(true)
            .with_class_alias(Some(class_alias.to_string()));
        if let Some(ref transforms) = self.transforms {
            converter = converter.with_transforms(transforms.clone());
        }
        converter.convert_statement(idx)
    }

    /// Convert an AST expression to IR in static context
    fn convert_expression_static(&self, idx: NodeIndex) -> IRNode {
        let mut converter = AstToIr::new(self.arena)
            .with_super(self.has_extends)
            .with_static(true);
        if let Some(ref transforms) = self.transforms {
            converter = converter.with_transforms(transforms.clone());
        }
        converter.convert_expression(idx)
    }

    /// Convert an AST expression to IR in static context with class alias for `this` substitution
    fn convert_expression_static_with_class_alias(
        &self,
        idx: NodeIndex,
        class_alias: &str,
    ) -> IRNode {
        let mut converter = AstToIr::new(self.arena)
            .with_super(self.has_extends)
            .with_static(true)
            .with_class_alias(Some(class_alias.to_string()));
        if let Some(ref transforms) = self.transforms {
            converter = converter.with_transforms(transforms.clone());
        }
        converter.convert_expression(idx)
    }

    /// Collect decorator `NodeIndex` list from a modifier list
    fn collect_decorators_from_modifiers(&self, modifiers: &Option<NodeList>) -> Vec<NodeIndex> {
        let Some(mods) = modifiers else {
            return Vec::new();
        };
        mods.nodes
            .iter()
            .copied()
            .filter(|&mod_idx| {
                self.arena
                    .get(mod_idx)
                    .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
            })
            .collect()
    }

    /// Render decorator expressions as strings using the IR printer.
    fn render_decorator_expressions(&self, decorators: &[NodeIndex]) -> Vec<String> {
        use crate::transforms::ir_printer::IRPrinter;
        let mut result = Vec::new();
        for &dec_idx in decorators {
            if let Some(dec_node) = self.arena.get(dec_idx)
                && let Some(dec) = self.arena.get_decorator(dec_node)
            {
                let ir_expr = self.convert_expression(dec.expression);
                let mut printer = IRPrinter::with_arena(self.arena);
                if let Some(source_text) = self.source_text {
                    printer.set_source_text(source_text);
                }
                if let Some(ref transforms) = self.transforms {
                    printer.set_transforms(transforms.clone());
                }
                let rendered = printer.emit(&ir_expr).to_string();
                result.push(rendered);
            }
        }
        result
    }

    /// Emit `__decorate` calls for decorated members inside the IIFE body.
    fn emit_member_decorator_ir(&self, body: &mut Vec<IRNode>, class_idx: NodeIndex) {
        let Some(class_node) = self.arena.get(class_idx) else {
            return;
        };
        let Some(class_data) = self.arena.get_class(class_node) else {
            return;
        };

        // Track accessor names that have already been emitted so that
        // getter/setter pairs produce only one __decorate call (the first one).
        let mut emitted_accessor_names = std::collections::HashSet::<String>::new();

        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            enum MemberMeta {
                Property {
                    type_annotation: NodeIndex,
                },
                Method {
                    parameters: NodeList,
                    return_type: NodeIndex,
                },
                Accessor,
            }

            let (modifiers, name_idx, is_property, is_accessor, meta) = match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    let meta = MemberMeta::Method {
                        parameters: method.parameters.clone(),
                        return_type: method.type_annotation,
                    };
                    (&method.modifiers, method.name, false, false, meta)
                }
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    let is_auto_accessor = self
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword);
                    let meta = MemberMeta::Property {
                        type_annotation: prop.type_annotation,
                    };
                    (&prop.modifiers, prop.name, !is_auto_accessor, false, meta)
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(accessor) = self.arena.get_accessor(member_node) else {
                        continue;
                    };
                    (
                        &accessor.modifiers,
                        accessor.name,
                        false,
                        true,
                        MemberMeta::Accessor,
                    )
                }
                _ => continue,
            };

            let decorators = self.collect_decorators_from_modifiers(modifiers);
            if decorators.is_empty() {
                continue;
            }

            let is_static = self
                .arena
                .has_modifier(modifiers, SyntaxKind::StaticKeyword);

            let member_name = get_identifier_text(self.arena, name_idx);
            let Some(member_name) = member_name else {
                continue;
            };
            if member_name.is_empty() {
                continue;
            }

            // For getter/setter pairs, tsc emits only one __decorate call
            // for the first accessor that has decorators. Skip the second.
            if is_accessor && !emitted_accessor_names.insert(member_name.clone()) {
                continue;
            }

            let dec_strs = self.render_decorator_expressions(&decorators);
            let target_str = if is_static {
                self.class_name.clone()
            } else {
                format!("{}.prototype", self.class_name)
            };
            let desc_str = if is_property { "void 0" } else { "null" };

            // Collect metadata strings if emit_decorator_metadata is enabled
            let metadata_strs: Vec<String> = if self.emit_decorator_metadata && !is_accessor {
                match &meta {
                    MemberMeta::Property { type_annotation } => {
                        let serialized = serialize_type_for_metadata(self.arena, *type_annotation);
                        vec![format!("__metadata(\"design:type\", {serialized})")]
                    }
                    MemberMeta::Method {
                        parameters,
                        return_type,
                    } => {
                        let param_types = serialize_param_types(self.arena, parameters);
                        let ret_type = if return_type.is_some() {
                            serialize_type_for_metadata(self.arena, *return_type)
                        } else {
                            "void 0".to_string()
                        };
                        vec![
                            "__metadata(\"design:type\", Function)".to_string(),
                            format!("__metadata(\"design:paramtypes\", [{param_types}])"),
                            format!("__metadata(\"design:returntype\", {ret_type})"),
                        ]
                    }
                    MemberMeta::Accessor => Vec::new(),
                }
            } else {
                Vec::new()
            };

            // Format matching tsc:
            // __decorate([\n        dec1,\n        dec2\n    ], target, "name", desc)
            // Note: first line indent is handled by the body emitter's write_indent().
            // Continuation lines after \n need absolute indentation from column 0.
            // The indent_base accounts for nesting (e.g., namespace IIFE body).
            let inner_indent = "    ".repeat((self.indent_base + 2) as usize);
            let outer_indent = "    ".repeat((self.indent_base + 1) as usize);
            let total_entries = dec_strs.len() + metadata_strs.len();
            let mut raw = String::from("__decorate([");
            for (i, dec_str) in dec_strs.iter().enumerate() {
                raw.push('\n');
                raw.push_str(&inner_indent);
                raw.push_str(dec_str);
                if i + 1 < total_entries {
                    raw.push(',');
                }
            }
            for (i, meta_str) in metadata_strs.iter().enumerate() {
                raw.push('\n');
                raw.push_str(&inner_indent);
                raw.push_str(meta_str);
                if dec_strs.len() + i + 1 < total_entries {
                    raw.push(',');
                }
            }
            raw.push('\n');
            raw.push_str(&outer_indent);
            raw.push_str("], ");
            raw.push_str(&target_str);
            raw.push_str(", \"");
            raw.push_str(&member_name);
            raw.push_str("\", ");
            raw.push_str(desc_str);
            raw.push(')');

            body.push(IRNode::ExpressionStatement(Box::new(IRNode::Raw(
                raw.into(),
            ))));
        }
    }

    /// Emit `ClassName = __decorate([dec1, ...], ClassName)` for class-level decorators.
    fn emit_class_decorator_ir(&self, body: &mut Vec<IRNode>) {
        let dec_strs = self.render_decorator_expressions(&self.class_decorators);
        if dec_strs.is_empty() {
            return;
        }

        // Format matching tsc:
        // ClassName = __decorate([\n        dec1,\n        dec2\n    ], ClassName)
        let inner_indent = "    ".repeat((self.indent_base + 2) as usize);
        let outer_indent = "    ".repeat((self.indent_base + 1) as usize);
        let mut raw = String::new();
        raw.push_str(&self.class_name);
        raw.push_str(" = __decorate([");
        for (i, dec_str) in dec_strs.iter().enumerate() {
            raw.push('\n');
            raw.push_str(&inner_indent);
            raw.push_str(dec_str);
            if i + 1 < dec_strs.len() {
                raw.push(',');
            }
        }
        raw.push('\n');
        raw.push_str(&outer_indent);
        raw.push_str("], ");
        raw.push_str(&self.class_name);
        raw.push(')');

        body.push(IRNode::ExpressionStatement(Box::new(IRNode::Raw(
            raw.into(),
        ))));
    }

    /// Convert a block body to IR statements
    fn convert_block_body(&self, block_idx: NodeIndex) -> Vec<IRNode> {
        self.convert_block_body_with_alias(block_idx, None)
    }

    /// Convert a block body to IR statements in static context
    fn convert_block_body_static(&self, block_idx: NodeIndex) -> Vec<IRNode> {
        self.convert_block_body_with_alias_static(block_idx, None)
    }

    /// Convert a block body to IR statements, optionally prepending a class alias declaration
    fn convert_block_body_with_alias(
        &self,
        block_idx: NodeIndex,
        class_alias: Option<String>,
    ) -> Vec<IRNode> {
        self.convert_block_body_with_alias_impl(block_idx, class_alias, false)
    }

    /// Convert a block body to IR statements in static context
    fn convert_block_body_with_alias_static(
        &self,
        block_idx: NodeIndex,
        class_alias: Option<String>,
    ) -> Vec<IRNode> {
        self.convert_block_body_with_alias_impl(block_idx, class_alias, true)
    }

    fn convert_block_body_with_alias_impl(
        &self,
        block_idx: NodeIndex,
        class_alias: Option<String>,
        is_static: bool,
    ) -> Vec<IRNode> {
        let mut stmts = if let Some(block_node) = self.arena.get(block_idx)
            && let Some(block) = self.arena.get_block(block_node)
        {
            block
                .statements
                .nodes
                .iter()
                .map(|&s| {
                    if is_static {
                        self.convert_statement_static(s)
                    } else {
                        self.convert_statement(s)
                    }
                })
                .collect()
        } else {
            vec![]
        };

        // If we have a class_alias, prepend the alias declaration: `var <alias> = this;`
        if let Some(alias) = class_alias {
            stmts.insert(
                0,
                IRNode::VarDecl {
                    name: alias.into(),
                    initializer: Some(Box::new(IRNode::This { captured: false })),
                },
            );
        }

        stmts
    }

    /// Transform a class declaration to IR
    pub fn transform_class_to_ir(&mut self, class_idx: NodeIndex) -> Option<IRNode> {
        self.transform_class_to_ir_with_name(class_idx, None)
    }

    /// Transform a class declaration to IR with an optional override name
    pub fn transform_class_to_ir_with_name(
        &mut self,
        class_idx: NodeIndex,
        override_name: Option<&str>,
    ) -> Option<IRNode> {
        let class_node = self.arena.get(class_idx)?;
        let class_data = self.arena.get_class(class_node)?;

        // Skip ambient/declare classes
        if self
            .arena
            .has_modifier(&class_data.modifiers, SyntaxKind::DeclareKeyword)
        {
            return None;
        }

        // Get class name
        let class_name = if let Some(name) = override_name {
            name.to_string()
        } else {
            get_identifier_text(self.arena, class_data.name)?
        };

        if class_name.is_empty() {
            return None;
        }

        self.class_name = class_name;

        // Collect private fields and accessors
        self.private_fields = collect_private_fields(self.arena, class_idx, &self.class_name);
        self.private_accessors = collect_private_accessors(self.arena, class_idx, &self.class_name);
        self.auto_accessors = collect_auto_accessor_fields(self.arena, class_idx, &self.class_name);

        // Check for extends clause
        let base_class = self.get_extends_class(&class_data.heritage_clauses);
        self.has_extends = base_class.is_some();
        self.extends_null = crate::transforms::emit_utils::extends_null_literal(
            self.arena,
            &class_data.heritage_clauses,
        );

        // Build IIFE body
        let mut body = Vec::new();

        // __extends(ClassName, _super);
        if self.has_extends {
            body.push(IRNode::ExtendsHelper {
                class_name: self.class_name.clone().into(),
            });
        }

        // Constructor function
        if let Some(ctor_ir) = self.emit_constructor_ir(class_idx) {
            body.push(ctor_ir);
        }

        // Prototype methods
        self.emit_methods_ir(&mut body, class_idx);

        // Static members
        let deferred_static_blocks = self.emit_static_members_ir(&mut body, class_idx);

        // Legacy decorator __decorate calls (inside IIFE, before return)
        if self.legacy_decorators {
            self.emit_member_decorator_ir(&mut body, class_idx);
        }
        if !self.class_decorators.is_empty() {
            self.emit_class_decorator_ir(&mut body);
        }

        // return ClassName;
        body.push(IRNode::ret(Some(IRNode::id(self.class_name.clone()))));

        // Build WeakMap declarations and instantiations
        let mut weakmap_decls: Vec<String> = self
            .private_fields
            .iter()
            .map(|f| f.weakmap_name.clone())
            .collect();

        // Add private accessor WeakMap variables
        for acc in &self.private_accessors {
            if let Some(ref get_var) = acc.get_var_name {
                weakmap_decls.push(get_var.clone());
            }
            if let Some(ref set_var) = acc.set_var_name {
                weakmap_decls.push(set_var.clone());
            }
        }
        for accessor in &self.auto_accessors {
            if !accessor.is_static {
                weakmap_decls.push(accessor.weakmap_name.clone());
            }
        }

        // WeakMap instantiations for instance fields
        let mut weakmap_inits: Vec<String> = self
            .private_fields
            .iter()
            .filter(|f| !f.is_static)
            .map(|f| format!("{} = new WeakMap()", f.weakmap_name))
            .collect();

        // Add private accessor WeakMap instantiations
        for acc in &self.private_accessors {
            if !acc.is_static {
                if let Some(ref get_var) = acc.get_var_name {
                    weakmap_inits.push(format!("{get_var} = new WeakMap()"));
                }
                if let Some(ref set_var) = acc.set_var_name {
                    weakmap_inits.push(format!("{set_var} = new WeakMap()"));
                }
            }
        }
        for accessor in &self.auto_accessors {
            if !accessor.is_static {
                weakmap_inits.push(format!("{} = new WeakMap()", accessor.weakmap_name));
            }
        }

        // NOTE: We intentionally pass `None` for `leading_comment` here.
        // The statement-level comment handler (`emit_comments_before_pos`) in
        // the block/source-file loop already emits any leading comments that
        // precede the class declaration. Extracting and re-emitting the same
        // comment in the IR printer would produce duplicate output.
        Some(IRNode::ES5ClassIIFE {
            name: self.class_name.clone().into(),
            base_class: base_class.map(Box::new),
            body,
            weakmap_decls,
            weakmap_inits,
            leading_comment: None,
            deferred_static_blocks,
        })
    }

    /// Build constructor IR node
    fn emit_constructor_ir(&self, class_idx: NodeIndex) -> Option<IRNode> {
        let class_node = self.arena.get(class_idx)?;
        let class_data = self.arena.get_class(class_node)?;

        // Collect instance property initializers (non-private only)
        let instance_props: Vec<NodeIndex> = class_data
            .members
            .nodes
            .iter()
            .filter_map(|&member_idx| {
                let member_node = self.arena.get(member_idx)?;
                if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                    return None;
                }
                let prop_data = self.arena.get_property_decl(member_node)?;
                // Skip static properties
                if self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::StaticKeyword)
                {
                    return None;
                }
                // Skip abstract properties (they don't exist at runtime)
                if self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::AbstractKeyword)
                {
                    return None;
                }
                // Skip `declare` properties — ambient/type-only declarations have no runtime representation
                if self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::DeclareKeyword)
                {
                    return None;
                }
                // Skip private fields (they use WeakMap pattern)
                if is_private_identifier(self.arena, prop_data.name) {
                    return None;
                }
                // Skip accessor fields (emitted as getter/setter pair + backing storage)
                if self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::AccessorKeyword)
                {
                    return None;
                }
                // Include if has initializer
                (prop_data.initializer.is_some()).then_some(member_idx)
            })
            .collect();

        // Find constructor implementation
        let mut constructor_data = None;
        let mut constructor_member_node: Option<&tsz_parser::parser::node::Node> = None;
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::CONSTRUCTOR {
                let Some(ctor_data) = self.arena.get_constructor(member_node) else {
                    continue;
                };
                // Only use constructor with body (not overload signatures)
                if ctor_data.body.is_some() {
                    constructor_member_node = Some(member_node);
                    constructor_data = Some(ctor_data);
                    break;
                }
            }
        }

        // Build constructor body
        let mut ctor_body = Vec::new();
        let mut params = Vec::new();
        let mut body_source_range = None;
        let mut trailing_comment = None;
        let mut leading_comment = None;
        let has_private_fields = self.private_fields.iter().any(|f| !f.is_static);

        if let Some(ctor) = constructor_data {
            // Extract parameters
            params = self.extract_parameters(&ctor.parameters);
            trailing_comment = self.extract_trailing_comment_for_method(ctor.body);
            // Extract leading JSDoc/block comment from the constructor declaration.
            if let Some(member_node) = constructor_member_node {
                leading_comment = self.extract_leading_comment(member_node);
            }
            // ES5 class-lowered constructors should follow TypeScript's normalized
            // multi-line function body formatting, not original source single-line shape.
            body_source_range = None;

            if self.has_extends {
                // Derived class with explicit constructor
                self.emit_derived_constructor_body_ir(
                    &mut ctor_body,
                    ctor.body,
                    &ctor.parameters,
                    &instance_props,
                );
            } else {
                // Non-derived class with explicit constructor
                self.emit_base_constructor_body_ir(
                    &mut ctor_body,
                    ctor.body,
                    &ctor.parameters,
                    &instance_props,
                );
            }
        } else {
            // Default constructor
            if self.has_extends && !self.extends_null {
                if instance_props.is_empty() && !has_private_fields {
                    // Simple: return _super !== null && _super.apply(this, arguments) || this;
                    ctor_body.push(IRNode::ret(Some(IRNode::logical_or(
                        IRNode::logical_and(
                            IRNode::binary(IRNode::id("_super"), "!==", IRNode::NullLiteral),
                            IRNode::call(
                                IRNode::prop(IRNode::id("_super"), "apply"),
                                vec![IRNode::this(), IRNode::id("arguments")],
                            ),
                        ),
                        IRNode::this(),
                    ))));
                } else {
                    // var _this = _super !== null && _super.apply(this, arguments) || this;
                    ctor_body.push(IRNode::var_decl(
                        "_this",
                        Some(IRNode::logical_or(
                            IRNode::logical_and(
                                IRNode::binary(IRNode::id("_super"), "!==", IRNode::NullLiteral),
                                IRNode::call(
                                    IRNode::prop(IRNode::id("_super"), "apply"),
                                    vec![IRNode::this(), IRNode::id("arguments")],
                                ),
                            ),
                            IRNode::this(),
                        )),
                    ));

                    // Private field initializations
                    self.emit_private_field_initializations_ir(&mut ctor_body, true);
                    self.emit_private_accessor_initializations_ir(&mut ctor_body, true);
                    self.emit_auto_accessor_initializations_ir(&mut ctor_body, true);

                    // Instance property initializations
                    for &prop_idx in &instance_props {
                        if let Some(ir) = self.emit_property_initializer_ir(prop_idx, true) {
                            ctor_body.push(ir);
                        }
                    }

                    // return _this;
                    ctor_body.push(IRNode::ret(Some(IRNode::id("_this"))));
                }
            } else {
                // Non-derived class default constructor
                // Check if instance property initializers need _this capture
                if self.instance_props_need_this_capture(&instance_props) {
                    ctor_body.push(IRNode::var_decl("_this", Some(IRNode::this())));
                }

                // Emit private field initializations
                self.emit_private_field_initializations_ir(&mut ctor_body, false);
                self.emit_private_accessor_initializations_ir(&mut ctor_body, false);
                self.emit_auto_accessor_initializations_ir(&mut ctor_body, false);

                // Instance property initializations
                for &prop_idx in &instance_props {
                    if let Some(ir) = self.emit_property_initializer_ir(prop_idx, false) {
                        ctor_body.push(ir);
                    }
                }
            }
        }

        let ctor_fn = IRNode::FunctionDecl {
            name: self.class_name.clone().into(),
            parameters: params,
            body: ctor_body,
            body_source_range,
            leading_comment,
        };

        if let Some(comment) = trailing_comment {
            Some(IRNode::Sequence(vec![
                ctor_fn,
                IRNode::TrailingComment(comment.into()),
            ]))
        } else {
            Some(ctor_fn)
        }
    }

    /// Emit derived class constructor body with `super()` transformation
    fn emit_derived_constructor_body_ir(
        &self,
        body: &mut Vec<IRNode>,
        body_idx: NodeIndex,
        params: &NodeList,
        instance_props: &[NodeIndex],
    ) {
        let Some(body_node) = self.arena.get(body_idx) else {
            return;
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return;
        };

        // Find super() call
        let mut super_stmt_idx = None;
        let mut super_stmt_position = 0;
        for (i, &stmt_idx) in block.statements.nodes.iter().enumerate() {
            if self.is_super_call_statement(stmt_idx) {
                super_stmt_idx = Some(stmt_idx);
                super_stmt_position = i;
                break;
            }
        }

        // Check if we can use the simple `return _super.call(this, ...) || this;` form.
        // This optimization applies when the constructor body has super() as its only statement
        // and there's no additional work to do (no parameter properties, instance props,
        // private fields, or arrow functions capturing `this`).
        let has_param_props = params.nodes.iter().any(|&p| {
            self.arena
                .get(p)
                .and_then(|n| self.arena.get_parameter(n))
                .map(|param| has_parameter_property_modifier(self.arena, &param.modifiers))
                .unwrap_or(false)
        });
        let has_private_fields = self.private_fields.iter().any(|f| !f.is_static);
        let has_auto_accessors = self.auto_accessors.iter().any(|a| !a.is_static);
        let has_private_accessors = self.private_accessors.iter().any(|a| !a.is_static);
        let stmts_before_super = super_stmt_idx.map(|_| super_stmt_position).unwrap_or(0);
        let stmts_after_super = super_stmt_idx
            .map(|_| block.statements.nodes.len() - super_stmt_position - 1)
            .unwrap_or(0);
        let needs_this_capture = self.constructor_needs_this_capture(body_idx);

        let can_use_simple_return = super_stmt_idx.is_some()
            && stmts_before_super == 0
            && stmts_after_super == 0
            && instance_props.is_empty()
            && !has_param_props
            && !has_private_fields
            && !has_auto_accessors
            && !has_private_accessors
            && !needs_this_capture;

        if can_use_simple_return {
            // Simple form: return _super.call(this, args) || this;
            if let Some(super_idx) = super_stmt_idx {
                let super_return = self.emit_super_call_return_ir(super_idx);
                body.push(super_return);
            }
            return;
        }

        // Emit statements before super() unchanged
        let mut prev_stmt_end = body_node.pos;
        for (i, &stmt_idx) in block.statements.nodes.iter().enumerate() {
            if i >= super_stmt_position && super_stmt_idx.is_some() {
                break;
            }
            if let Some(stmt_node) = self.arena.get(stmt_idx) {
                self.emit_leading_statement_comments(body, prev_stmt_end, stmt_node.pos);
                prev_stmt_end = stmt_node.end;
            }
            body.push(self.convert_statement(stmt_idx));
        }

        // Emit super() as var _this = _super.call(this, args) || this;
        if let Some(super_idx) = super_stmt_idx {
            let super_call = self.emit_super_call_ir(super_idx);
            body.push(super_call);
        }

        // Emit parameter properties
        self.emit_parameter_properties_ir(body, params, true);

        // Emit private field initializations
        self.emit_private_field_initializations_ir(body, true);
        self.emit_private_accessor_initializations_ir(body, true);
        self.emit_auto_accessor_initializations_ir(body, true);

        // Emit instance property initializers
        for &prop_idx in instance_props {
            if let Some(ir) = self.emit_property_initializer_ir(prop_idx, true) {
                body.push(ir);
            }
        }

        // Emit remaining statements after super()
        // In derived constructors, `this` becomes `_this` after super() call
        if super_stmt_idx.is_some() {
            for (i, &stmt_idx) in block.statements.nodes.iter().enumerate() {
                if i <= super_stmt_position {
                    continue;
                }
                if let Some(stmt_node) = self.arena.get(stmt_idx) {
                    self.emit_leading_statement_comments(body, prev_stmt_end, stmt_node.pos);
                    prev_stmt_end = stmt_node.end;
                }
                body.push(self.convert_statement_this_captured(stmt_idx));
            }
        }

        // return _this;
        if super_stmt_idx.is_some() {
            body.push(IRNode::ret(Some(IRNode::id("_this"))));
        }
    }

    /// Emit base class constructor body
    fn emit_base_constructor_body_ir(
        &self,
        body: &mut Vec<IRNode>,
        body_idx: NodeIndex,
        params: &NodeList,
        instance_props: &[NodeIndex],
    ) {
        // Check if constructor body or instance property initializers contain
        // arrow functions that capture `this`.
        // TSC emits `var _this = this;` as the FIRST statement in the constructor.
        let needs_this_capture = self.constructor_needs_this_capture(body_idx)
            || self.instance_props_need_this_capture(instance_props);
        if needs_this_capture {
            // Emit: var _this = this;
            body.push(IRNode::var_decl("_this", Some(IRNode::this())));
        }

        // Emit private field initializations
        self.emit_private_field_initializations_ir(body, false);
        self.emit_private_accessor_initializations_ir(body, false);
        self.emit_auto_accessor_initializations_ir(body, false);

        // Emit parameter properties
        self.emit_parameter_properties_ir(body, params, false);

        // Emit instance property initializers
        for &prop_idx in instance_props {
            if let Some(ir) = self.emit_property_initializer_ir(prop_idx, false) {
                body.push(ir);
            }
        }

        // Emit original constructor body
        if let Some(block_node) = self.arena.get(body_idx)
            && let Some(block) = self.arena.get_block(block_node)
        {
            let mut prev_stmt_end = block_node.pos;
            for &stmt_idx in &block.statements.nodes {
                if let Some(stmt_node) = self.arena.get(stmt_idx) {
                    self.emit_leading_statement_comments(body, prev_stmt_end, stmt_node.pos);
                    prev_stmt_end = stmt_node.end;
                }
                body.push(self.convert_statement(stmt_idx));
            }
        }
    }

    /// Check if a statement is a `super()` call
    fn is_super_call_statement(&self, stmt_idx: NodeIndex) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };

        if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
            return false;
        }

        let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node) else {
            return false;
        };
        let Some(call_node) = self.arena.get(expr_stmt.expression) else {
            return false;
        };

        if call_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }

        let Some(call) = self.arena.get_call_expr(call_node) else {
            return false;
        };
        let Some(callee) = self.arena.get(call.expression) else {
            return false;
        };

        callee.kind == SyntaxKind::SuperKeyword as u16
    }

    /// Emit super(args) as var _this = _super.call(this, args) || this;
    fn emit_super_call_ir(&self, stmt_idx: NodeIndex) -> IRNode {
        let mut args = vec![IRNode::this()];

        if let Some(stmt_node) = self.arena.get(stmt_idx)
            && let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node)
            && let Some(call_node) = self.arena.get(expr_stmt.expression)
            && let Some(call) = self.arena.get_call_expr(call_node)
            && let Some(ref call_args) = call.arguments
        {
            for &arg_idx in &call_args.nodes {
                args.push(self.convert_expression(arg_idx));
            }
        }

        // var _this = _super.call(this, args...) || this;
        IRNode::var_decl(
            "_this",
            Some(IRNode::logical_or(
                IRNode::call(IRNode::prop(IRNode::id("_super"), "call"), args),
                IRNode::this(),
            )),
        )
    }

    /// Emit super(args) as return _super.call(this, args) || this;
    /// Used when the constructor body only contains `super()` with no other work.
    fn emit_super_call_return_ir(&self, stmt_idx: NodeIndex) -> IRNode {
        let mut args = vec![IRNode::this()];

        if let Some(stmt_node) = self.arena.get(stmt_idx)
            && let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node)
            && let Some(call_node) = self.arena.get(expr_stmt.expression)
            && let Some(call) = self.arena.get_call_expr(call_node)
            && let Some(ref call_args) = call.arguments
        {
            for &arg_idx in &call_args.nodes {
                args.push(self.convert_expression(arg_idx));
            }
        }

        // return _super.call(this, args...) || this;
        IRNode::ret(Some(IRNode::logical_or(
            IRNode::call(IRNode::prop(IRNode::id("_super"), "call"), args),
            IRNode::this(),
        )))
    }

    /// Emit parameter properties (public/private/protected/readonly params)
    fn emit_parameter_properties_ir(
        &self,
        body: &mut Vec<IRNode>,
        params: &NodeList,
        use_this: bool,
    ) {
        for &param_idx in &params.nodes {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };

            if has_parameter_property_modifier(self.arena, &param.modifiers)
                && let Some(param_name) = get_identifier_text(self.arena, param.name)
            {
                let receiver = if use_this {
                    IRNode::id("_this")
                } else {
                    IRNode::this()
                };
                // this.param = param; or _this.param = param;
                body.push(IRNode::expr_stmt(IRNode::assign(
                    IRNode::prop(receiver, param_name.clone()),
                    IRNode::id(param_name.clone()),
                )));
            }
        }
    }

    /// Emit private field initializations using `WeakMap.set()`
    fn emit_private_field_initializations_ir(&self, body: &mut Vec<IRNode>, use_this: bool) {
        let key = if use_this {
            IRNode::id("_this")
        } else {
            IRNode::this()
        };

        for field in &self.private_fields {
            if field.is_static {
                continue;
            }

            // _ClassName_field.set(this, void 0);
            body.push(IRNode::expr_stmt(IRNode::WeakMapSet {
                weakmap_name: field.weakmap_name.clone().into(),
                key: Box::new(key.clone()),
                value: Box::new(IRNode::Undefined),
            }));

            // If has initializer: __classPrivateFieldSet(this, _ClassName_field, value, "f");
            if field.has_initializer && field.initializer.is_some() {
                body.push(IRNode::expr_stmt(IRNode::PrivateFieldSet {
                    receiver: Box::new(key.clone()),
                    weakmap_name: field.weakmap_name.clone().into(),
                    value: Box::new(self.convert_expression(field.initializer)),
                }));
            }
        }
    }

    /// Emit private accessor initializations using `WeakMap.set()`
    fn emit_private_accessor_initializations_ir(&self, body: &mut Vec<IRNode>, use_this: bool) {
        let key = if use_this {
            IRNode::id("_this")
        } else {
            IRNode::this()
        };

        for acc in &self.private_accessors {
            if acc.is_static {
                continue;
            }

            // Emit getter: _ClassName_accessor_get.set(this, function() { ... });
            if let Some(ref get_var) = acc.get_var_name
                && let Some(getter_body) = acc.getter_body
            {
                body.push(IRNode::expr_stmt(IRNode::WeakMapSet {
                    weakmap_name: get_var.clone().into(),
                    key: Box::new(key.clone()),
                    value: Box::new(IRNode::FunctionExpr {
                        name: None,
                        parameters: vec![],
                        body: self.convert_block_body(getter_body),
                        is_expression_body: false,
                        body_source_range: None,
                    }),
                }));
            }

            // Emit setter: _ClassName_accessor_set.set(this, function(param) { ... });
            if let Some(ref set_var) = acc.set_var_name
                && let Some(setter_body) = acc.setter_body
            {
                let param_name = if let Some(param_idx) = acc.setter_param {
                    get_identifier_text(self.arena, param_idx)
                        .unwrap_or_else(|| "value".to_string())
                } else {
                    "value".to_string()
                };

                body.push(IRNode::expr_stmt(IRNode::WeakMapSet {
                    weakmap_name: set_var.clone().into(),
                    key: Box::new(key.clone()),
                    value: Box::new(IRNode::FunctionExpr {
                        name: None,
                        parameters: vec![IRParam::new(param_name)],
                        body: self.convert_block_body(setter_body),
                        is_expression_body: false,
                        body_source_range: None,
                    }),
                }));
            }
        }
    }

    /// Emit auto-accessor field initializations using `WeakMap.set()`
    fn emit_auto_accessor_initializations_ir(&self, body: &mut Vec<IRNode>, use_this: bool) {
        let key = if use_this {
            IRNode::id("_this")
        } else {
            IRNode::this()
        };

        for accessor in &self.auto_accessors {
            if accessor.is_static {
                continue;
            }

            // _Class_accessor_storage.set(this, void 0);
            body.push(IRNode::expr_stmt(IRNode::WeakMapSet {
                weakmap_name: accessor.weakmap_name.clone().into(),
                key: Box::new(key.clone()),
                value: Box::new(IRNode::Undefined),
            }));

            if let Some(initializer) = accessor.initializer {
                body.push(IRNode::expr_stmt(IRNode::PrivateFieldSet {
                    receiver: Box::new(key.clone()),
                    weakmap_name: accessor.weakmap_name.clone().into(),
                    value: Box::new(self.convert_expression(initializer)),
                }));
            }
        }
    }

    fn find_auto_accessor(&self, member_idx: NodeIndex) -> Option<&AutoAccessorFieldInfo> {
        self.auto_accessors
            .iter()
            .find(|acc| acc.member_idx == member_idx && !acc.is_static)
    }

    fn build_auto_accessor_getter_function(&self, weakmap_name: &str) -> IRNode {
        IRNode::FunctionExpr {
            name: None,
            parameters: vec![],
            body: vec![IRNode::ret(Some(IRNode::PrivateFieldGet {
                receiver: Box::new(IRNode::this()),
                weakmap_name: weakmap_name.to_string().into(),
            }))],
            is_expression_body: true,
            body_source_range: None,
        }
    }

    fn build_auto_accessor_setter_function(&self, weakmap_name: &str) -> IRNode {
        IRNode::FunctionExpr {
            name: None,
            parameters: vec![IRParam::new("value")],
            body: vec![IRNode::expr_stmt(IRNode::PrivateFieldSet {
                receiver: Box::new(IRNode::this()),
                weakmap_name: weakmap_name.to_string().into(),
                value: Box::new(IRNode::id("value")),
            })],
            is_expression_body: true,
            body_source_range: None,
        }
    }

    /// Emit a property initializer as an assignment
    fn emit_property_initializer_ir(&self, prop_idx: NodeIndex, use_this: bool) -> Option<IRNode> {
        let prop_node = self.arena.get(prop_idx)?;
        let prop_data = self.arena.get_property_decl(prop_node)?;

        if prop_data.initializer.is_none() {
            return None;
        }

        let receiver = if use_this {
            IRNode::id("_this")
        } else {
            IRNode::this()
        };

        let prop_name = self.get_property_name_ir(prop_data.name)?;

        Some(IRNode::expr_stmt(IRNode::assign(
            self.build_property_access(receiver, prop_name),
            self.convert_expression(prop_data.initializer),
        )))
    }

    /// Build property access node based on property name type
    fn build_property_access(&self, receiver: IRNode, name: PropertyNameIR) -> IRNode {
        match name {
            PropertyNameIR::Identifier(n) => IRNode::prop(receiver, n),
            PropertyNameIR::StringLiteral(s) => IRNode::elem(receiver, IRNode::string(s)),
            PropertyNameIR::NumericLiteral(n) => IRNode::elem(receiver, IRNode::number(n)),
            PropertyNameIR::Computed(expr_idx) => {
                IRNode::elem(receiver, self.convert_expression(expr_idx))
            }
        }
    }

    /// Get property name as IR-friendly representation
    fn get_property_name_ir(&self, name_idx: NodeIndex) -> Option<PropertyNameIR> {
        let name_node = self.arena.get(name_idx)?;

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            if let Some(computed) = self.arena.get_computed_property(name_node) {
                return Some(PropertyNameIR::Computed(computed.expression));
            }
        } else if name_node.kind == SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return Some(PropertyNameIR::Identifier(ident.escaped_text.clone()));
            }
        } else if name_node.kind == SyntaxKind::StringLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(name_node) {
                return Some(PropertyNameIR::StringLiteral(lit.text.clone()));
            }
        } else if name_node.kind == SyntaxKind::NumericLiteral as u16
            && let Some(lit) = self.arena.get_literal(name_node)
        {
            return Some(PropertyNameIR::NumericLiteral(lit.text.clone()));
        }

        None
    }

    /// Extract parameters from a parameter list
    fn extract_parameters(&self, params: &NodeList) -> Vec<IRParam> {
        let mut result = Vec::new();

        for &param_idx in &params.nodes {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };

            let name = get_identifier_text(self.arena, param.name).unwrap_or_default();
            if name.is_empty() {
                continue;
            }

            let is_rest = param.dot_dot_dot_token;
            let mut ir_param = if is_rest {
                IRParam::rest(name)
            } else {
                IRParam::new(name)
            };

            // Convert default value if present
            if param.initializer.is_some() {
                ir_param.default_value = Some(Box::new(self.convert_expression(param.initializer)));
            }

            result.push(ir_param);
        }

        result
    }

    /// Get the extends clause base class
    fn get_extends_class(&self, heritage_clauses: &Option<NodeList>) -> Option<IRNode> {
        let expr_idx = crate::transforms::emit_utils::get_extends_expression_index(
            self.arena,
            heritage_clauses,
        )?;
        Some(self.convert_expression(expr_idx))
    }

    /// Check if a static method body contains arrow functions with `class_alias`,
    /// and return the alias if found
    fn get_class_alias_for_static_method(&self, body_idx: NodeIndex) -> Option<String> {
        if let Some(ref transforms) = self.transforms {
            // Get all arrow function nodes in the method body
            let arrow_indices = self.collect_arrow_functions_in_block(body_idx);
            // Check if any arrow function has a class_alias directive
            for &arrow_idx in &arrow_indices {
                if let Some(dir) = transforms.get(arrow_idx)
                    && let crate::context::transform::TransformDirective::ES5ArrowFunction {
                        class_alias,
                        ..
                    } = dir
                    && let Some(alias) = class_alias
                {
                    return Some(alias.to_string());
                }
            }
        }
        None
    }

    /// Collect all arrow function node indices in a block
    fn collect_arrow_functions_in_block(&self, block_idx: NodeIndex) -> Vec<NodeIndex> {
        let mut arrows = Vec::new();
        if let Some(block_node) = self.arena.get(block_idx)
            && let Some(block) = self.arena.get_block(block_node)
        {
            for &stmt_idx in &block.statements.nodes {
                self.collect_arrow_functions_in_node(stmt_idx, &mut arrows);
            }
        }
        arrows
    }

    /// Check if constructor body needs `var _this = this;` capture
    /// Returns true if the body contains arrow functions that capture `this`
    fn constructor_needs_this_capture(&self, body_idx: NodeIndex) -> bool {
        let arrow_indices = self.collect_arrow_functions_in_block(body_idx);

        // Check if any arrow function captures `this`
        for &arrow_idx in &arrow_indices {
            if let Some(ref transforms) = self.transforms {
                if let Some(crate::context::transform::TransformDirective::ES5ArrowFunction {
                    captures_this,
                    ..
                }) = transforms.get(arrow_idx)
                    && *captures_this
                {
                    return true;
                }
            } else {
                // Fallback: directly check if arrow contains `this` reference
                if contains_this_reference(self.arena, arrow_idx) {
                    return true;
                }
            }
        }

        false
    }

    /// Check if instance property initializers contain arrow functions that capture `this`.
    /// Property initializers are moved into the constructor body by the ES5 transform.
    fn instance_props_need_this_capture(&self, instance_props: &[NodeIndex]) -> bool {
        for &prop_idx in instance_props {
            let Some(prop_node) = self.arena.get(prop_idx) else {
                continue;
            };
            let Some(prop_data) = self.arena.get_property_decl(prop_node) else {
                continue;
            };
            if prop_data.initializer.is_none() {
                continue;
            }
            // Check if the initializer contains arrow functions that capture `this`
            let mut arrows = Vec::new();
            self.collect_arrow_functions_in_node(prop_data.initializer, &mut arrows);
            for &arrow_idx in &arrows {
                if let Some(ref transforms) = self.transforms {
                    if let Some(crate::context::transform::TransformDirective::ES5ArrowFunction {
                        captures_this,
                        ..
                    }) = transforms.get(arrow_idx)
                        && *captures_this
                    {
                        return true;
                    }
                } else if contains_this_reference(self.arena, arrow_idx) {
                    return true;
                }
            }
        }
        false
    }

    /// Recursively collect arrow function indices starting from a node
    fn collect_arrow_functions_in_node(&self, idx: NodeIndex, arrows: &mut Vec<NodeIndex>) {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.arena.get(idx) else {
            return;
        };

        // Check if this node itself is an arrow function
        if node.kind == syntax_kind_ext::ARROW_FUNCTION {
            arrows.push(idx);
        }

        // Recursively check children based on node type
        // For blocks, check each statement
        if let Some(block) = self.arena.get_block(node) {
            for &stmt_idx in &block.statements.nodes {
                self.collect_arrow_functions_in_node(stmt_idx, arrows);
            }
        }
        // For expressions with sub-expressions, check those
        else if let Some(func) = self.arena.get_function(node) {
            // Check parameters
            for &param_idx in &func.parameters.nodes {
                self.collect_arrow_functions_in_node(param_idx, arrows);
            }
            // Check body
            if func.body.is_some() {
                self.collect_arrow_functions_in_node(func.body, arrows);
            }
        }
        // For variable declarations, check initializer
        else if let Some(var_decl) = self.arena.get_variable_declaration(node) {
            if var_decl.initializer.is_some() {
                self.collect_arrow_functions_in_node(var_decl.initializer, arrows);
            }
        }
        // For variable statements, check declarations
        else if let Some(var_stmt) = self.arena.get_variable(node) {
            for &decl_idx in &var_stmt.declarations.nodes {
                self.collect_arrow_functions_in_node(decl_idx, arrows);
            }
        }
        // For return statements, check expression
        else if let Some(ret_stmt) = self.arena.get_return_statement(node) {
            if ret_stmt.expression.is_some() {
                self.collect_arrow_functions_in_node(ret_stmt.expression, arrows);
            }
        }
        // For expression statements, check expression
        else if let Some(expr_stmt) = self.arena.get_expression_statement(node) {
            self.collect_arrow_functions_in_node(expr_stmt.expression, arrows);
        }
        // For call expressions, check callee and arguments
        else if let Some(call) = self.arena.get_call_expr(node) {
            self.collect_arrow_functions_in_node(call.expression, arrows);
            if let Some(ref args) = call.arguments {
                for &arg_idx in &args.nodes {
                    self.collect_arrow_functions_in_node(arg_idx, arrows);
                }
            }
        }
        // For binary expressions, check left and right
        else if let Some(binary) = self.arena.get_binary_expr(node) {
            self.collect_arrow_functions_in_node(binary.left, arrows);
            self.collect_arrow_functions_in_node(binary.right, arrows);
        }
        // Note: This is a simplified traversal - may miss some edge cases
    }
}

// =============================================================================
// Helper Types
// =============================================================================

/// Property name representation for IR building
enum PropertyNameIR {
    Identifier(String),
    StringLiteral(String),
    NumericLiteral(String),
    Computed(NodeIndex),
}

// =============================================================================
// Helper Functions
// =============================================================================

fn get_identifier_text(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    // Try simple identifier first
    if let Some(text) = crate::transforms::emit_utils::identifier_text(arena, idx) {
        return Some(text);
    }
    let node = arena.get(idx)?;
    if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
        // For computed property names like ["goodbye"], extract the string literal text
        if let Some(computed) = arena.get_computed_property(node)
            && let Some(expr_node) = arena.get(computed.expression)
            && expr_node.kind == SyntaxKind::StringLiteral as u16
        {
            return arena.get_literal(expr_node).map(|lit| lit.text.clone());
        }
        None
    } else if node.kind == SyntaxKind::StringLiteral as u16 {
        arena.get_literal(node).map(|lit| lit.text.clone())
    } else {
        None
    }
}

/// Collect accessor pairs (getter/setter) from class members.
/// When `collect_static` is true, collects static accessors; otherwise collects instance accessors.
fn collect_accessor_pairs(
    arena: &NodeArena,
    members: &NodeList,
    collect_static: bool,
) -> FxHashMap<String, (Option<NodeIndex>, Option<NodeIndex>)> {
    let mut accessor_map: FxHashMap<String, (Option<NodeIndex>, Option<NodeIndex>)> =
        FxHashMap::default();

    for &member_idx in &members.nodes {
        let Some(member_node) = arena.get(member_idx) else {
            continue;
        };

        if (member_node.kind == syntax_kind_ext::GET_ACCESSOR
            || member_node.kind == syntax_kind_ext::SET_ACCESSOR)
            && let Some(accessor_data) = arena.get_accessor(member_node)
        {
            // Check static modifier matches what we're collecting
            let is_static = arena.has_modifier(&accessor_data.modifiers, SyntaxKind::StaticKeyword);
            if is_static != collect_static {
                continue;
            }
            // Skip abstract
            if arena.has_modifier(&accessor_data.modifiers, SyntaxKind::AbstractKeyword) {
                continue;
            }
            // Skip private
            if is_private_identifier(arena, accessor_data.name) {
                continue;
            }

            let name = match get_identifier_text(arena, accessor_data.name) {
                Some(name) => name,
                // Non-literal computed property name (e.g., [1 << 6]) — use a unique
                // key per accessor so they are NOT merged into a single ODP call.
                // tsc emits separate Object.defineProperty for each.
                None => format!("__computed_{}", member_idx.0),
            };
            let entry = accessor_map.entry(name).or_insert((None, None));

            if member_node.kind == syntax_kind_ext::GET_ACCESSOR {
                entry.0 = Some(member_idx);
            } else {
                entry.1 = Some(member_idx);
            }
        }
    }

    accessor_map
}

fn collect_auto_accessor_fields(
    arena: &NodeArena,
    class_idx: NodeIndex,
    class_name: &str,
) -> Vec<AutoAccessorFieldInfo> {
    let mut accessors = Vec::new();

    let Some(class_node) = arena.get(class_idx) else {
        return accessors;
    };
    let Some(class_data) = arena.get_class(class_node) else {
        return accessors;
    };

    for &member_idx in &class_data.members.nodes {
        let Some(member_node) = arena.get(member_idx) else {
            continue;
        };
        if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
            continue;
        }
        let Some(prop_data) = arena.get_property_decl(member_node) else {
            continue;
        };
        if arena.has_modifier(&prop_data.modifiers, SyntaxKind::AbstractKeyword) {
            continue;
        }
        if is_private_identifier(arena, prop_data.name) {
            continue;
        }
        if arena.has_modifier(&prop_data.modifiers, SyntaxKind::StaticKeyword) {
            continue;
        }
        let has_accessor = arena.has_modifier(&prop_data.modifiers, SyntaxKind::AccessorKeyword);
        if !has_accessor {
            continue;
        }
        let Some(name_node) = arena.get(prop_data.name) else {
            continue;
        };
        if name_node.kind != SyntaxKind::Identifier as u16 {
            continue;
        }
        let Some(name) = arena
            .get_identifier(name_node)
            .map(|id| id.escaped_text.clone())
        else {
            continue;
        };

        accessors.push(AutoAccessorFieldInfo {
            member_idx,
            weakmap_name: format!("_{class_name}_{name}_accessor_storage"),
            initializer: prop_data
                .initializer
                .is_some()
                .then_some(prop_data.initializer),
            is_static: false,
        });
    }

    accessors
}

fn has_parameter_property_modifier(arena: &NodeArena, modifiers: &Option<NodeList>) -> bool {
    arena.has_modifier(modifiers, SyntaxKind::PublicKeyword)
        || arena.has_modifier(modifiers, SyntaxKind::PrivateKeyword)
        || arena.has_modifier(modifiers, SyntaxKind::ProtectedKeyword)
        || arena.has_modifier(modifiers, SyntaxKind::ReadonlyKeyword)
        || arena.has_modifier(modifiers, SyntaxKind::OverrideKeyword)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[path = "../../tests/class_es5_ir.rs"]
mod tests;
