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

use crate::transform_context::TransformContext;
use crate::transforms::async_es5_ir::AsyncES5Transformer;
use crate::transforms::ir::{
    IRCatchClause, IRMethodName, IRNode, IRParam, IRProperty, IRPropertyDescriptor, IRPropertyKey,
    IRPropertyKind, IRSwitchCase,
};
use crate::transforms::private_fields_es5::{
    PrivateAccessorInfo, PrivateFieldInfo, collect_private_accessors, collect_private_fields,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::Cell;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_parser::syntax::transform_utils::contains_this_reference;
use tsz_parser::syntax::transform_utils::is_private_identifier;
use tsz_scanner::SyntaxKind;

/// Context for ES5 class transformation
pub struct ES5ClassTransformer<'a> {
    arena: &'a NodeArena,
    class_name: String,
    has_extends: bool,
    private_fields: Vec<PrivateFieldInfo>,
    private_accessors: Vec<PrivateAccessorInfo>,
    /// Transform directives from `LoweringPass`
    transforms: Option<TransformContext>,
    /// Source text for extracting comments
    source_text: Option<&'a str>,
}

impl<'a> ES5ClassTransformer<'a> {
    pub const fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            class_name: String::new(),
            has_extends: false,
            private_fields: Vec::new(),
            private_accessors: Vec::new(),
            transforms: None,
            source_text: None,
        }
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
                body.push(IRNode::Raw(trimmed.to_string()));
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
    /// Returns the comment text including the /** ... */ delimiters.
    fn extract_leading_comment(&self, node: &tsz_parser::parser::node::Node) -> Option<String> {
        let source_text = self.source_text?;

        // Scan backwards from node.pos to find the start of this member's section
        // We need to go back far enough to include any JSDoc comments
        let bytes = source_text.as_bytes();
        let mut search_pos = node.pos as usize;

        // Scan back to find opening brace or end of previous member
        let mut brace_depth = 0;
        while search_pos > 0 {
            let ch = bytes[search_pos - 1];

            // Track brace depth (we're scanning backwards, so braces are reversed)
            if ch == b'}' {
                brace_depth += 1;
            } else if ch == b'{' {
                if brace_depth == 0 {
                    // Found the opening brace of the class body - stop here
                    break;
                }
                brace_depth -= 1;
            }

            // Also stop at semicolons at depth 0 (end of previous member)
            if ch == b';' && brace_depth == 0 {
                break;
            }

            search_pos -= 1;

            // Safety limit: don't scan back more than 1000 chars
            if node.pos as usize - search_pos > 1000 {
                search_pos = node.pos.saturating_sub(1000) as usize;
                break;
            }
        }

        // Get comments starting from the beginning of this member's "section"
        let comments = crate::emitter::get_leading_comment_ranges(source_text, search_pos);

        // Find the last JSDoc-style comment that ends before or at node.pos
        for comment in comments.iter().rev() {
            // Only consider comments that end before or at the node's start
            if comment.end > node.pos {
                continue;
            }

            let comment_text = &source_text[comment.pos as usize..comment.end as usize];
            // Check if it's a JSDoc comment (starts with /** not just /*)
            if comment_text.starts_with("/**") && !comment_text.starts_with("/***") {
                return Some(comment_text.to_string());
            }
            // Also accept regular block comments for now
            if comment_text.starts_with("/*") && !comment_text.starts_with("/**") {
                return Some(comment_text.to_string());
            }
        }

        None
    }

    /// Extract trailing comment on the same line as a class method declaration.
    fn extract_trailing_comment_for_method(&self, body_idx: NodeIndex) -> Option<String> {
        let source_text = self.source_text?;
        let body_node = self.arena.get(body_idx)?;
        let bytes = source_text.as_bytes();
        let start = body_node.pos as usize;
        let end = (body_node.end as usize).min(bytes.len());
        if start >= end {
            return None;
        }
        let mut trailing = None;
        for (offset, &byte) in bytes[start..end].iter().enumerate() {
            if byte == b'}'
                && let Some(comment) =
                    crate::emitter::get_trailing_comment_ranges(source_text, (start + offset) + 1)
                        .first()
            {
                trailing =
                    Some(source_text[comment.pos as usize..comment.end as usize].to_string());
            }
        }
        trailing
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

    /// Convert a block body to IR statements
    fn convert_block_body(&self, block_idx: NodeIndex) -> Vec<IRNode> {
        self.convert_block_body_with_alias(block_idx, None)
    }

    /// Convert a block body to IR statements, optionally prepending a class alias declaration
    fn convert_block_body_with_alias(
        &self,
        block_idx: NodeIndex,
        class_alias: Option<String>,
    ) -> Vec<IRNode> {
        let mut stmts = if let Some(block_node) = self.arena.get(block_idx)
            && let Some(block) = self.arena.get_block(block_node)
        {
            block
                .statements
                .nodes
                .iter()
                .map(|&s| self.convert_statement(s))
                .collect()
        } else {
            vec![]
        };

        // If we have a class_alias, prepend the alias declaration: `var <alias> = this;`
        if let Some(alias) = class_alias {
            stmts.insert(
                0,
                IRNode::VarDecl {
                    name: alias,
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
        if has_declare_modifier(self.arena, &class_data.modifiers) {
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

        // Check for extends clause
        let base_class = self.get_extends_class(&class_data.heritage_clauses);
        self.has_extends = base_class.is_some();

        // Build IIFE body
        let mut body = Vec::new();

        // __extends(ClassName, _super);
        if self.has_extends {
            body.push(IRNode::ExtendsHelper {
                class_name: self.class_name.clone(),
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

        // return ClassName;
        body.push(IRNode::ret(Some(IRNode::id(&self.class_name))));

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

        Some(IRNode::ES5ClassIIFE {
            name: self.class_name.clone(),
            base_class: base_class.map(Box::new),
            body,
            weakmap_decls,
            weakmap_inits,
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
                if has_static_modifier(self.arena, &prop_data.modifiers) {
                    return None;
                }
                // Skip abstract properties (they don't exist at runtime)
                if has_abstract_modifier(self.arena, &prop_data.modifiers) {
                    return None;
                }
                // Skip private fields (they use WeakMap pattern)
                if is_private_identifier(self.arena, prop_data.name) {
                    return None;
                }
                // Include if has initializer
                (!prop_data.initializer.is_none()).then_some(member_idx)
            })
            .collect();

        // Find constructor implementation
        let mut constructor_data = None;
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::CONSTRUCTOR {
                let Some(ctor_data) = self.arena.get_constructor(member_node) else {
                    continue;
                };
                // Only use constructor with body (not overload signatures)
                if !ctor_data.body.is_none() {
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
        let has_private_fields = self.private_fields.iter().any(|f| !f.is_static);

        if let Some(ctor) = constructor_data {
            // Extract parameters
            params = self.extract_parameters(&ctor.parameters);
            trailing_comment = self.extract_trailing_comment_for_method(ctor.body);
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
            if self.has_extends {
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

                // Instance property initializations
                for &prop_idx in &instance_props {
                    if let Some(ir) = self.emit_property_initializer_ir(prop_idx, false) {
                        ctor_body.push(ir);
                    }
                }
            }
        }

        let ctor_fn = IRNode::FunctionDecl {
            name: self.class_name.clone(),
            parameters: params,
            body: ctor_body,
            body_source_range,
        };

        if let Some(comment) = trailing_comment {
            Some(IRNode::Sequence(vec![
                ctor_fn,
                IRNode::TrailingComment(comment),
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
                    IRNode::prop(receiver, &param_name),
                    IRNode::id(&param_name),
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
                weakmap_name: field.weakmap_name.clone(),
                key: Box::new(key.clone()),
                value: Box::new(IRNode::Undefined),
            }));

            // If has initializer: __classPrivateFieldSet(this, _ClassName_field, value, "f");
            if field.has_initializer && !field.initializer.is_none() {
                body.push(IRNode::expr_stmt(IRNode::PrivateFieldSet {
                    receiver: Box::new(key.clone()),
                    weakmap_name: field.weakmap_name.clone(),
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
                    weakmap_name: get_var.clone(),
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
                    weakmap_name: set_var.clone(),
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
            if !param.initializer.is_none() {
                ir_param.default_value = Some(Box::new(self.convert_expression(param.initializer)));
            }

            result.push(ir_param);
        }

        result
    }

    /// Get the extends clause base class
    fn get_extends_class(&self, heritage_clauses: &Option<NodeList>) -> Option<IRNode> {
        let clauses = heritage_clauses.as_ref()?;

        for &clause_idx in &clauses.nodes {
            let clause_node = self.arena.get(clause_idx)?;
            let heritage_data = self.arena.get_heritage(clause_node)?;

            // Check if this is an extends clause (not implements)
            if heritage_data.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            // Get the first type in the extends clause (the base class)
            let first_type_idx = heritage_data.types.nodes.first()?;
            let type_node = self.arena.get(*first_type_idx)?;

            // The type could be:
            // 1. A simple identifier (B in `extends B`)
            // 2. An ExpressionWithTypeArguments (B<T> in `extends B<T>`)
            // 3. A PropertyAccessExpression (A.B in `extends A.B`)

            // Try as simple identifier first
            if let Some(ident) = self.arena.get_identifier(type_node) {
                return Some(IRNode::id(&ident.escaped_text));
            }

            // Try as ExpressionWithTypeArguments (for generics)
            if let Some(expr_data) = self.arena.get_expr_type_args(type_node) {
                // Return the expression converted to IR
                return Some(self.convert_expression(expr_data.expression));
            }
        }

        None
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
                    && let crate::transform_context::TransformDirective::ES5ArrowFunction {
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
                if let Some(crate::transform_context::TransformDirective::ES5ArrowFunction {
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
                    if let Some(crate::transform_context::TransformDirective::ES5ArrowFunction {
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
            if !func.body.is_none() {
                self.collect_arrow_functions_in_node(func.body, arrows);
            }
        }
        // For variable declarations, check initializer
        else if let Some(var_decl) = self.arena.get_variable_declaration(node) {
            if !var_decl.initializer.is_none() {
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
            if !ret_stmt.expression.is_none() {
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

    /// Emit prototype methods as IR
    fn emit_methods_ir(&self, body: &mut Vec<IRNode>, class_idx: NodeIndex) {
        let Some(class_node) = self.arena.get(class_idx) else {
            return;
        };
        let Some(class_data) = self.arena.get_class(class_node) else {
            return;
        };

        // First pass: collect instance accessors by name to combine getter/setter pairs
        let accessor_map = collect_accessor_pairs(self.arena, &class_data.members, false);

        // Track which accessor names we've emitted
        let mut emitted_accessors: FxHashSet<String> = FxHashSet::default();

        // Second pass: emit methods and accessors in source order
        for (member_i, &member_idx) in class_data.members.nodes.iter().enumerate() {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                let Some(method_data) = self.arena.get_method_decl(member_node) else {
                    continue;
                };

                // Skip static methods
                if has_static_modifier(self.arena, &method_data.modifiers) {
                    continue;
                }

                // Skip if no body
                if method_data.body.is_none() {
                    continue;
                }

                let method_name = self.get_method_name_ir(method_data.name);
                let params = self.extract_parameters(&method_data.parameters);

                // Check if async method (not generator)
                let is_async = has_async_modifier(self.arena, &method_data.modifiers)
                    && !method_data.asterisk_token;

                // Capture body source range for single-line detection
                let body_source_range = self
                    .arena
                    .get(method_data.body)
                    .map(|body_node| (body_node.pos, body_node.end));

                let method_body = if is_async {
                    // Async method: use async transformer to build proper generator body
                    let mut async_transformer = AsyncES5Transformer::new(self.arena);
                    let has_await = async_transformer.body_contains_await(method_data.body);
                    let generator_body =
                        async_transformer.transform_generator_body(method_data.body, has_await);
                    vec![IRNode::AwaiterCall {
                        this_arg: Box::new(IRNode::this()),
                        generator_body: Box::new(generator_body),
                    }]
                } else {
                    let mut method_body = self.convert_block_body(method_data.body);

                    // Check if method needs `var _this = this;` capture
                    let needs_this_capture = self.constructor_needs_this_capture(method_data.body);
                    if needs_this_capture {
                        // Insert `var _this = this;` at the start of method body
                        method_body.insert(0, IRNode::var_decl("_this", Some(IRNode::this())));
                    }

                    method_body
                };

                // Extract leading JSDoc comment
                let leading_comment = self.extract_leading_comment(member_node);
                let trailing_comment = self.extract_trailing_comment_for_method(method_data.body);

                // ClassName.prototype.methodName = function () { body };
                body.push(IRNode::PrototypeMethod {
                    class_name: self.class_name.clone(),
                    method_name,
                    function: Box::new(IRNode::FunctionExpr {
                        name: None,
                        parameters: params,
                        body: method_body,
                        is_expression_body: false,
                        body_source_range,
                    }),
                    leading_comment,
                    trailing_comment,
                });
            } else if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                || member_node.kind == syntax_kind_ext::SET_ACCESSOR
            {
                // Handle accessor (getter/setter) - combine pairs
                if let Some(accessor_data) = self.arena.get_accessor(member_node) {
                    // Skip static/abstract/private (already filtered in first pass)
                    if has_static_modifier(self.arena, &accessor_data.modifiers)
                        || has_abstract_modifier(self.arena, &accessor_data.modifiers)
                        || is_private_identifier(self.arena, accessor_data.name)
                    {
                        continue;
                    }

                    let accessor_name =
                        get_identifier_text(self.arena, accessor_data.name).unwrap_or_default();

                    // Skip if already emitted
                    if emitted_accessors.contains(&accessor_name) {
                        continue;
                    }

                    // Emit combined getter/setter
                    if let Some(&(getter_idx, setter_idx)) = accessor_map.get(&accessor_name) {
                        let get_fn = if let Some(getter_idx) = getter_idx {
                            self.build_getter_function_ir(getter_idx)
                        } else {
                            None
                        };

                        let set_fn = if let Some(setter_idx) = setter_idx {
                            self.build_setter_function_ir(setter_idx)
                        } else {
                            None
                        };

                        body.push(IRNode::DefineProperty {
                            target: Box::new(IRNode::prop(
                                IRNode::id(&self.class_name),
                                "prototype",
                            )),
                            property_name: self.get_method_name_ir(accessor_data.name),
                            descriptor: IRPropertyDescriptor {
                                get: get_fn.map(Box::new),
                                set: set_fn.map(Box::new),
                                enumerable: false,
                                configurable: true,
                            },
                        });

                        let has_explicit_semicolon_member = class_data
                            .members
                            .nodes
                            .get(member_i + 1)
                            .and_then(|&idx| self.arena.get(idx))
                            .is_some_and(|n| n.kind == syntax_kind_ext::SEMICOLON_CLASS_ELEMENT);
                        if !has_explicit_semicolon_member {
                            let accessor_end = [getter_idx, setter_idx]
                                .into_iter()
                                .flatten()
                                .filter_map(|idx| self.arena.get(idx))
                                .map(|n| n.end)
                                .max()
                                .unwrap_or(member_node.end);
                            let next_pos = class_data
                                .members
                                .nodes
                                .get(member_i + 1)
                                .and_then(|&idx| self.arena.get(idx))
                                .map_or(member_node.end, |n| n.pos);
                            if self.source_has_semicolon_between(accessor_end, next_pos) {
                                body.push(IRNode::EmptyStatement);
                            }
                        }
                        if self.source_text.is_some_and(|text| {
                            let start = std::cmp::min(member_node.pos as usize, text.len());
                            let end = std::cmp::min(member_node.end as usize, text.len());
                            start < end && text[start..end].trim_end().ends_with(';')
                        }) {
                            body.push(IRNode::EmptyStatement);
                        }

                        emitted_accessors.insert(accessor_name);
                    }
                }
            } else if member_node.kind == syntax_kind_ext::SEMICOLON_CLASS_ELEMENT {
                body.push(IRNode::EmptyStatement);
            }
        }
    }

    /// Build a getter function IR from an accessor node
    fn build_getter_function_ir(&self, accessor_idx: NodeIndex) -> Option<IRNode> {
        let accessor_node = self.arena.get(accessor_idx)?;
        let accessor_data = self.arena.get_accessor(accessor_node)?;

        let body_source_range = self.arena.get(accessor_data.body).map(|n| (n.pos, n.end));

        let body = if accessor_data.body.is_none() {
            vec![]
        } else {
            let mut body = self.convert_block_body(accessor_data.body);

            // Check if getter needs `var _this = this;` capture
            let needs_this_capture = self.constructor_needs_this_capture(accessor_data.body);
            if needs_this_capture {
                // Insert `var _this = this;` at the start of getter body
                body.insert(0, IRNode::var_decl("_this", Some(IRNode::this())));
            }

            body
        };

        Some(IRNode::FunctionExpr {
            name: None,
            parameters: vec![],
            body,
            is_expression_body: false,
            body_source_range,
        })
    }

    /// Build a setter function IR from an accessor node
    fn build_setter_function_ir(&self, accessor_idx: NodeIndex) -> Option<IRNode> {
        let accessor_node = self.arena.get(accessor_idx)?;
        let accessor_data = self.arena.get_accessor(accessor_node)?;

        let mut params = self.extract_parameters(&accessor_data.parameters);

        let body_source_range = self.arena.get(accessor_data.body).map(|n| (n.pos, n.end));

        let mut body = if accessor_data.body.is_none() {
            vec![]
        } else {
            let mut body = self.convert_block_body(accessor_data.body);

            // Check if setter needs `var _this = this;` capture
            let needs_this_capture = self.constructor_needs_this_capture(accessor_data.body);
            if needs_this_capture {
                // Insert `var _this = this;` at the start of setter body
                body.insert(0, IRNode::var_decl("_this", Some(IRNode::this())));
            }

            body
        };

        self.lower_rest_parameter_for_es5(&mut params, &mut body);

        Some(IRNode::FunctionExpr {
            name: None,
            parameters: params,
            body,
            is_expression_body: false,
            body_source_range,
        })
    }

    /// Lower a rest parameter into ES5 `arguments` collection statements.
    /// Example: `(...v)` -> `() { var v = []; for (var _i = 0; _i < arguments.length; _i++) { ... } }`
    fn lower_rest_parameter_for_es5(&self, params: &mut Vec<IRParam>, body: &mut Vec<IRNode>) {
        let Some(rest_index) = params.iter().position(|param| param.rest) else {
            return;
        };

        let rest_name = params[rest_index].name.clone();
        params.truncate(rest_index);

        let loop_var = "_i";
        let start_index = rest_index.to_string();

        let target_index = if rest_index == 0 {
            IRNode::id(loop_var)
        } else {
            IRNode::binary(
                IRNode::id(loop_var),
                "-",
                IRNode::number(start_index.clone()),
            )
        };

        let assignment = IRNode::expr_stmt(IRNode::assign(
            IRNode::elem(IRNode::id(rest_name.clone()), target_index),
            IRNode::elem(IRNode::id("arguments"), IRNode::id(loop_var)),
        ));

        let collect_rest = IRNode::ForStatement {
            initializer: Some(Box::new(IRNode::Raw(format!(
                "var {loop_var} = {start_index}"
            )))),
            condition: Some(Box::new(IRNode::binary(
                IRNode::id(loop_var),
                "<",
                IRNode::prop(IRNode::id("arguments"), "length"),
            ))),
            incrementor: Some(Box::new(IRNode::PostfixUnaryExpr {
                operand: Box::new(IRNode::id(loop_var)),
                operator: "++".to_string(),
            })),
            body: Box::new(IRNode::block(vec![assignment])),
        };

        body.insert(0, collect_rest);
        body.insert(0, IRNode::var_decl(rest_name, Some(IRNode::empty_array())));
    }

    /// Emit static members as IR.
    /// Returns deferred static block IIFEs (for classes with no non-block static members).
    fn emit_static_members_ir(&self, body: &mut Vec<IRNode>, class_idx: NodeIndex) -> Vec<IRNode> {
        let Some(class_node) = self.arena.get(class_idx) else {
            return Vec::new();
        };
        let Some(class_data) = self.arena.get_class(class_node) else {
            return Vec::new();
        };

        // Check if class has non-block static members (properties, accessors, methods with bodies)
        // This determines whether static blocks go inline or deferred
        let has_static_props = class_data.members.nodes.iter().any(|&m_idx| {
            let Some(m_node) = self.arena.get(m_idx) else {
                return false;
            };
            if m_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                if let Some(prop_data) = self.arena.get_property_decl(m_node) {
                    return has_static_modifier(self.arena, &prop_data.modifiers)
                        && !has_abstract_modifier(self.arena, &prop_data.modifiers)
                        && !is_private_identifier(self.arena, prop_data.name)
                        && prop_data.initializer.is_some();
                }
            } else if (m_node.kind == syntax_kind_ext::GET_ACCESSOR
                || m_node.kind == syntax_kind_ext::SET_ACCESSOR)
                && let Some(acc_data) = self.arena.get_accessor(m_node)
            {
                return has_static_modifier(self.arena, &acc_data.modifiers)
                    && !has_abstract_modifier(self.arena, &acc_data.modifiers)
                    && !is_private_identifier(self.arena, acc_data.name);
            }
            false
        });

        let mut deferred_static_blocks = Vec::new();

        // First pass: collect static accessors by name to combine getter/setter pairs
        let static_accessor_map = collect_accessor_pairs(self.arena, &class_data.members, true);

        // Track which static accessor names we've emitted
        let mut emitted_static_accessors: FxHashSet<String> = FxHashSet::default();

        // Second pass: emit static members in source order
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                let Some(method_data) = self.arena.get_method_decl(member_node) else {
                    continue;
                };

                // Only static methods
                if !has_static_modifier(self.arena, &method_data.modifiers) {
                    continue;
                }

                // Skip if no body
                if method_data.body.is_none() {
                    continue;
                }

                let method_name = self.get_method_name_ir(method_data.name);
                let params = self.extract_parameters(&method_data.parameters);

                // Check if async method (not generator)
                let is_async = has_async_modifier(self.arena, &method_data.modifiers)
                    && !method_data.asterisk_token;

                let method_body = if is_async {
                    // Async method: use async transformer to build proper generator body
                    let mut async_transformer = AsyncES5Transformer::new(self.arena);
                    let has_await = async_transformer.body_contains_await(method_data.body);
                    let generator_body =
                        async_transformer.transform_generator_body(method_data.body, has_await);
                    vec![IRNode::AwaiterCall {
                        this_arg: Box::new(IRNode::this()),
                        generator_body: Box::new(generator_body),
                    }]
                } else {
                    // Check if this static method has arrow functions with class_alias
                    let class_alias = self.get_class_alias_for_static_method(method_data.body);
                    self.convert_block_body_with_alias(method_data.body, class_alias)
                };

                // Capture body source range for single-line detection
                let body_source_range = self
                    .arena
                    .get(method_data.body)
                    .map(|body_node| (body_node.pos, body_node.end));

                // Extract leading JSDoc comment
                let leading_comment = self.extract_leading_comment(member_node);
                let trailing_comment = self.extract_trailing_comment_for_method(method_data.body);

                // ClassName.methodName = function () { body };
                body.push(IRNode::StaticMethod {
                    class_name: self.class_name.clone(),
                    method_name,
                    function: Box::new(IRNode::FunctionExpr {
                        name: None,
                        parameters: params,
                        body: method_body,
                        is_expression_body: false,
                        body_source_range,
                    }),
                    leading_comment,
                    trailing_comment,
                });
            } else if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                let Some(prop_data) = self.arena.get_property_decl(member_node) else {
                    continue;
                };

                // Only static properties
                if !has_static_modifier(self.arena, &prop_data.modifiers) {
                    continue;
                }

                // Skip abstract properties (they don't exist at runtime)
                if has_abstract_modifier(self.arena, &prop_data.modifiers) {
                    continue;
                }

                // Skip private
                if is_private_identifier(self.arena, prop_data.name) {
                    continue;
                }

                // Skip if no initializer
                if prop_data.initializer.is_none() {
                    continue;
                }

                if let Some(prop_name) = self.get_property_name_ir(prop_data.name) {
                    let target = match &prop_name {
                        PropertyNameIR::Identifier(n) => {
                            IRNode::prop(IRNode::id(&self.class_name), n)
                        }
                        PropertyNameIR::StringLiteral(s) => {
                            IRNode::elem(IRNode::id(&self.class_name), IRNode::string(s))
                        }
                        PropertyNameIR::NumericLiteral(n) => {
                            IRNode::elem(IRNode::id(&self.class_name), IRNode::number(n))
                        }
                        PropertyNameIR::Computed(expr_idx) => IRNode::elem(
                            IRNode::id(&self.class_name),
                            self.convert_expression(*expr_idx),
                        ),
                    };

                    // ClassName.prop = value;
                    body.push(IRNode::expr_stmt(IRNode::assign(
                        target,
                        self.convert_expression(prop_data.initializer),
                    )));
                }
            } else if member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                // Static block: wrap in IIFE to preserve block scoping
                if let Some(block_data) = self.arena.get_block(member_node) {
                    let statements: Vec<IRNode> = block_data
                        .statements
                        .nodes
                        .iter()
                        .map(|&stmt_idx| self.convert_statement(stmt_idx))
                        .collect();

                    let iife = IRNode::StaticBlockIIFE { statements };
                    if has_static_props {
                        // Inline: maintain initialization order with other static members
                        body.push(iife);
                    } else {
                        // Deferred: emit after the class IIFE
                        deferred_static_blocks.push(iife);
                    }
                }
            } else if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                || member_node.kind == syntax_kind_ext::SET_ACCESSOR
            {
                // Handle static accessor - combine pairs
                if let Some(accessor_data) = self.arena.get_accessor(member_node)
                    && has_static_modifier(self.arena, &accessor_data.modifiers)
                {
                    // Skip abstract/private
                    if has_abstract_modifier(self.arena, &accessor_data.modifiers)
                        || is_private_identifier(self.arena, accessor_data.name)
                    {
                        continue;
                    }

                    let accessor_name =
                        get_identifier_text(self.arena, accessor_data.name).unwrap_or_default();

                    // Skip if already emitted
                    if emitted_static_accessors.contains(&accessor_name) {
                        continue;
                    }

                    // Emit combined getter/setter
                    if let Some(&(getter_idx, setter_idx)) = static_accessor_map.get(&accessor_name)
                    {
                        let get_fn = if let Some(getter_idx) = getter_idx {
                            self.build_getter_function_ir(getter_idx)
                        } else {
                            None
                        };

                        let set_fn = if let Some(setter_idx) = setter_idx {
                            self.build_setter_function_ir(setter_idx)
                        } else {
                            None
                        };

                        body.push(IRNode::DefineProperty {
                            target: Box::new(IRNode::id(&self.class_name)),
                            property_name: self.get_method_name_ir(accessor_data.name),
                            descriptor: IRPropertyDescriptor {
                                get: get_fn.map(Box::new),
                                set: set_fn.map(Box::new),
                                enumerable: false,
                                configurable: true,
                            },
                        });

                        emitted_static_accessors.insert(accessor_name);
                    }
                }
            }
        }

        deferred_static_blocks
    }

    /// Get method name as IR representation
    fn get_method_name_ir(&self, name_idx: NodeIndex) -> IRMethodName {
        let Some(name_node) = self.arena.get(name_idx) else {
            return IRMethodName::Identifier(String::new());
        };

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            if let Some(computed) = self.arena.get_computed_property(name_node) {
                return IRMethodName::Computed(Box::new(
                    self.convert_expression(computed.expression),
                ));
            }
        } else if name_node.kind == SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return IRMethodName::Identifier(ident.escaped_text.clone());
            }
        } else if name_node.kind == SyntaxKind::StringLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(name_node) {
                return IRMethodName::StringLiteral(lit.text.clone());
            }
        } else if name_node.kind == SyntaxKind::NumericLiteral as u16
            && let Some(lit) = self.arena.get_literal(name_node)
        {
            return IRMethodName::NumericLiteral(lit.text.clone());
        }

        IRMethodName::Identifier(String::new())
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
    let node = arena.get(idx)?;
    if node.kind == SyntaxKind::Identifier as u16 {
        arena.get_identifier(node).map(|id| id.escaped_text.clone())
    } else if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
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

fn has_modifier(arena: &NodeArena, modifiers: &Option<NodeList>, kind: u16) -> bool {
    if let Some(mods) = modifiers {
        for &mod_idx in &mods.nodes {
            if let Some(mod_node) = arena.get(mod_idx)
                && mod_node.kind == kind
            {
                return true;
            }
        }
    }
    false
}

fn has_declare_modifier(arena: &NodeArena, modifiers: &Option<NodeList>) -> bool {
    has_modifier(arena, modifiers, SyntaxKind::DeclareKeyword as u16)
}

fn has_static_modifier(arena: &NodeArena, modifiers: &Option<NodeList>) -> bool {
    has_modifier(arena, modifiers, SyntaxKind::StaticKeyword as u16)
}

fn has_abstract_modifier(arena: &NodeArena, modifiers: &Option<NodeList>) -> bool {
    has_modifier(arena, modifiers, SyntaxKind::AbstractKeyword as u16)
}

fn has_async_modifier(arena: &NodeArena, modifiers: &Option<NodeList>) -> bool {
    has_modifier(arena, modifiers, SyntaxKind::AsyncKeyword as u16)
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
            let is_static = has_static_modifier(arena, &accessor_data.modifiers);
            if is_static != collect_static {
                continue;
            }
            // Skip abstract
            if has_abstract_modifier(arena, &accessor_data.modifiers) {
                continue;
            }
            // Skip private
            if is_private_identifier(arena, accessor_data.name) {
                continue;
            }

            let name = get_identifier_text(arena, accessor_data.name).unwrap_or_default();
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

fn has_parameter_property_modifier(arena: &NodeArena, modifiers: &Option<NodeList>) -> bool {
    if let Some(mods) = modifiers {
        for &mod_idx in &mods.nodes {
            if let Some(mod_node) = arena.get(mod_idx) {
                match mod_node.kind {
                    k if k == SyntaxKind::PublicKeyword as u16
                        || k == SyntaxKind::PrivateKeyword as u16
                        || k == SyntaxKind::ProtectedKeyword as u16
                        || k == SyntaxKind::ReadonlyKeyword as u16 =>
                    {
                        return true;
                    }
                    _ => {}
                }
            }
        }
    }
    false
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[path = "../../tests/class_es5_ir.rs"]
mod tests;
