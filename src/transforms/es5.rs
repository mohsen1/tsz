//! ES5 Transforms producing IR nodes
//!
//! This module provides transforms that convert ES2015+ constructs to ES5-compatible
//! IR (Intermediate Representation) nodes. The printer then handles all string emission.
//!
//! # Example
//!
//! ```ignore
//! // Input TypeScript/ES6:
//! class Point {
//!     constructor(x, y) {
//!         this.x = x;
//!         this.y = y;
//!     }
//! }
//!
//! // Transform produces IR that prints as:
//! var Point = /** @class */ (function () {
//!     function Point(x, y) {
//!         this.x = x;
//!         this.y = y;
//!     }
//!     return Point;
//! }());
//! ```

use crate::parser::syntax_kind_ext;
use crate::parser::thin_node::ThinNodeArena;
use crate::parser::{NodeIndex, NodeList};
use crate::scanner::SyntaxKind;
use crate::transforms::ir::*;
use crate::transforms::private_fields_es5::{
    collect_private_accessors, collect_private_fields, is_private_identifier,
};

/// ES5 Class Transformer - produces IR nodes for ES5 class lowering
pub struct ES5ClassTransformer<'a> {
    arena: &'a ThinNodeArena,
    /// Current class name (for WeakMap naming)
    class_name: String,
    /// Whether we're using _this capture
    use_this_capture: bool,
    /// Counter for temp variables
    temp_var_counter: u32,
}

impl<'a> ES5ClassTransformer<'a> {
    pub fn new(arena: &'a ThinNodeArena) -> Self {
        Self {
            arena,
            class_name: String::new(),
            use_this_capture: false,
            temp_var_counter: 0,
        }
    }

    /// Transform a class declaration to ES5 IR
    pub fn transform_class(&mut self, class_idx: NodeIndex) -> Option<IRNode> {
        self.transform_class_with_name(class_idx, None)
    }

    /// Transform a class declaration with an optional override name
    pub fn transform_class_with_name(
        &mut self,
        class_idx: NodeIndex,
        override_name: Option<&str>,
    ) -> Option<IRNode> {
        let class_node = self.arena.get(class_idx)?;
        let class_data = self.arena.get_class(class_node)?;

        // Skip ambient/declare classes
        if self.has_declare_modifier(&class_data.modifiers) {
            return None;
        }

        // Get class name
        let class_name = if let Some(name) = override_name {
            name.to_string()
        } else {
            self.get_identifier_text(class_data.name)
        };
        self.class_name = class_name.clone();

        // Collect private fields and accessors
        let private_fields = collect_private_fields(self.arena, class_idx, &class_name);
        let private_accessors = collect_private_accessors(self.arena, class_idx, &class_name);

        // Get base class
        let base_class = self.get_extends_class(&class_data.heritage_clauses);
        let has_extends = base_class.is_some();

        // Build IIFE body
        let mut body = Vec::new();

        // Add __extends if has base class
        if has_extends {
            body.push(IRNode::ExtendsHelper {
                class_name: class_name.clone(),
            });
        }

        // Add constructor
        let constructor_ir = self.transform_constructor(&class_name, class_data, has_extends);
        body.push(constructor_ir);

        // Add prototype methods
        let methods = self.transform_methods(&class_name, class_data);
        body.extend(methods);

        // Add static members
        let statics = self.transform_static_members(&class_name, class_data);
        body.extend(statics);

        // Add return statement
        body.push(IRNode::ret(Some(IRNode::id(&class_name))));

        // Collect WeakMap declarations as IR nodes
        let weakmap_decls: Vec<IRNode> = private_fields
            .iter()
            .map(|f| IRNode::var_decl(&f.weakmap_name, None))
            .chain(
                private_accessors
                    .iter()
                    .filter_map(|a| a.get_var_name.as_ref().map(|name| IRNode::var_decl(name, None))),
            )
            .chain(
                private_accessors
                    .iter()
                    .filter_map(|a| a.set_var_name.as_ref().map(|name| IRNode::var_decl(name, None))),
            )
            .collect();

        // Collect WeakMap instantiations as IR nodes
        let weakmap_inits: Vec<IRNode> = private_fields
            .iter()
            .filter(|f| !f.is_static)
            .map(|f| {
                IRNode::expr_stmt(IRNode::assign(
                    IRNode::id(&f.weakmap_name),
                    IRNode::call(IRNode::id("WeakMap"), vec![]),
                ))
            })
            .chain(
                private_accessors
                    .iter()
                    .filter(|a| !a.is_static)
                    .flat_map(|a| {
                        let mut inits = Vec::new();
                        if let Some(ref name) = a.get_var_name {
                            inits.push(IRNode::expr_stmt(IRNode::assign(
                                IRNode::id(name),
                                IRNode::call(IRNode::id("WeakMap"), vec![]),
                            )));
                        }
                        if let Some(ref name) = a.set_var_name {
                            inits.push(IRNode::expr_stmt(IRNode::assign(
                                IRNode::id(name),
                                IRNode::call(IRNode::id("WeakMap"), vec![]),
                            )));
                        }
                        inits
                    }),
            )
            .collect();

        Some(IRNode::ES5ClassIIFE {
            name: class_name,
            base_class: base_class.map(Box::new),
            body,
            weakmap_decls,
            weakmap_inits,
        })
    }

    /// Transform class constructor to IR
    fn transform_constructor(
        &mut self,
        class_name: &str,
        class_data: &crate::parser::thin_node::ClassData,
        has_extends: bool,
    ) -> IRNode {
        // Collect instance property initializers
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
                if self.is_static(&prop_data.modifiers) {
                    return None;
                }
                if is_private_identifier(self.arena, prop_data.name) {
                    return None;
                }
                if prop_data.initializer.is_none() {
                    return None;
                }
                Some(member_idx)
            })
            .collect();

        // Find constructor
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == syntax_kind_ext::CONSTRUCTOR {
                let Some(ctor_data) = self.arena.get_constructor(member_node) else {
                    continue;
                };

                if ctor_data.body.is_none() {
                    continue;
                }

                // Build constructor function
                let params = self.transform_parameters(&ctor_data.parameters);
                let body = self.transform_constructor_body(
                    ctor_data.body,
                    &ctor_data.parameters,
                    &instance_props,
                    has_extends,
                );

                return IRNode::func_decl(class_name, params, body);
            }
        }

        // Default constructor
        let body = self.build_default_constructor_body(&instance_props, has_extends);
        IRNode::func_decl(class_name, vec![], body)
    }

    /// Transform constructor body
    fn transform_constructor_body(
        &mut self,
        body_idx: NodeIndex,
        _params: &NodeList,
        instance_props: &[NodeIndex],
        has_extends: bool,
    ) -> Vec<IRNode> {
        let mut stmts = Vec::new();

        if has_extends {
            // For derived classes, emit super() transformation
            // var _this = _super.call(this, ...args) || this;
            self.use_this_capture = true;
            stmts.push(IRNode::var_decl(
                "_this",
                Some(IRNode::LogicalOr {
                    left: Box::new(IRNode::call(
                        IRNode::prop(IRNode::id("_super"), "call"),
                        vec![IRNode::this()], // TODO: pass super args
                    )),
                    right: Box::new(IRNode::this()),
                }),
            ));

            // Emit instance property initializers
            for &prop_idx in instance_props {
                if let Some(ir) = self.transform_property_initializer(prop_idx, true) {
                    stmts.push(ir);
                }
            }

            // Transform body statements
            stmts.extend(self.transform_block_contents(body_idx));

            // Add return _this
            stmts.push(IRNode::ret(Some(IRNode::id("_this"))));
        } else {
            // Non-derived class
            // Emit instance property initializers
            for &prop_idx in instance_props {
                if let Some(ir) = self.transform_property_initializer(prop_idx, false) {
                    stmts.push(ir);
                }
            }

            // Transform body statements
            stmts.extend(self.transform_block_contents(body_idx));
        }

        stmts
    }

    /// Build default constructor body
    fn build_default_constructor_body(
        &mut self,
        instance_props: &[NodeIndex],
        has_extends: bool,
    ) -> Vec<IRNode> {
        let mut stmts = Vec::new();

        if has_extends && instance_props.is_empty() {
            // return _super !== null && _super.apply(this, arguments) || this;
            stmts.push(IRNode::ret(Some(IRNode::LogicalOr {
                left: Box::new(IRNode::LogicalAnd {
                    left: Box::new(IRNode::binary(
                        IRNode::id("_super"),
                        "!==",
                        IRNode::NullLiteral,
                    )),
                    right: Box::new(IRNode::call(
                        IRNode::prop(IRNode::id("_super"), "apply"),
                        vec![IRNode::this(), IRNode::id("arguments")],
                    )),
                }),
                right: Box::new(IRNode::this()),
            })));
        } else if has_extends {
            // var _this = _super !== null && _super.apply(this, arguments) || this;
            stmts.push(IRNode::var_decl(
                "_this",
                Some(IRNode::LogicalOr {
                    left: Box::new(IRNode::LogicalAnd {
                        left: Box::new(IRNode::binary(
                            IRNode::id("_super"),
                            "!==",
                            IRNode::NullLiteral,
                        )),
                        right: Box::new(IRNode::call(
                            IRNode::prop(IRNode::id("_super"), "apply"),
                            vec![IRNode::this(), IRNode::id("arguments")],
                        )),
                    }),
                    right: Box::new(IRNode::this()),
                }),
            ));

            // Emit instance property initializers
            for &prop_idx in instance_props {
                if let Some(ir) = self.transform_property_initializer(prop_idx, true) {
                    stmts.push(ir);
                }
            }

            // return _this;
            stmts.push(IRNode::ret(Some(IRNode::id("_this"))));
        } else {
            // Non-derived: emit property initializers
            for &prop_idx in instance_props {
                if let Some(ir) = self.transform_property_initializer(prop_idx, false) {
                    stmts.push(ir);
                }
            }
        }

        stmts
    }

    /// Transform a property initializer
    fn transform_property_initializer(
        &self,
        prop_idx: NodeIndex,
        use_this_capture: bool,
    ) -> Option<IRNode> {
        let prop_node = self.arena.get(prop_idx)?;
        let prop_data = self.arena.get_property_decl(prop_node)?;

        let receiver = if use_this_capture {
            IRNode::this_captured()
        } else {
            IRNode::this()
        };

        let prop_name = self.get_identifier_text(prop_data.name);
        let target = IRNode::prop(receiver, &prop_name);
        let value = self.transform_expression(prop_data.initializer)?;

        Some(IRNode::expr_stmt(IRNode::assign(target, value)))
    }

    /// Transform prototype methods
    fn transform_methods(
        &mut self,
        class_name: &str,
        class_data: &crate::parser::thin_node::ClassData,
    ) -> Vec<IRNode> {
        let mut nodes = Vec::new();

        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                let Some(method_data) = self.arena.get_method_decl(member_node) else {
                    continue;
                };

                if self.is_static(&method_data.modifiers) {
                    continue;
                }

                if method_data.body.is_none() {
                    continue;
                }

                let method_name = self.get_method_name(method_data.name);
                let params = self.transform_parameters(&method_data.parameters);
                let body = self.transform_block_contents(method_data.body);

                let func = IRNode::func_expr(None, params, body);

                nodes.push(IRNode::PrototypeMethod {
                    class_name: class_name.to_string(),
                    method_name,
                    function: Box::new(func),
                });
            }
        }

        nodes
    }

    /// Transform static members
    fn transform_static_members(
        &mut self,
        class_name: &str,
        class_data: &crate::parser::thin_node::ClassData,
    ) -> Vec<IRNode> {
        let mut nodes = Vec::new();

        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                let Some(method_data) = self.arena.get_method_decl(member_node) else {
                    continue;
                };

                if !self.is_static(&method_data.modifiers) {
                    continue;
                }

                if method_data.body.is_none() {
                    continue;
                }

                let method_name = self.get_method_name(method_data.name);
                let params = self.transform_parameters(&method_data.parameters);
                let body = self.transform_block_contents(method_data.body);

                let func = IRNode::func_expr(None, params, body);

                nodes.push(IRNode::StaticMethod {
                    class_name: class_name.to_string(),
                    method_name,
                    function: Box::new(func),
                });
            } else if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                let Some(prop_data) = self.arena.get_property_decl(member_node) else {
                    continue;
                };

                if !self.is_static(&prop_data.modifiers) {
                    continue;
                }

                if prop_data.initializer.is_none() {
                    continue;
                }

                let prop_name = self.get_identifier_text(prop_data.name);
                if let Some(value) = self.transform_expression(prop_data.initializer) {
                    let target = IRNode::prop(IRNode::id(class_name), &prop_name);
                    nodes.push(IRNode::expr_stmt(IRNode::assign(target, value)));
                }
            }
        }

        nodes
    }

    /// Transform parameters to IR
    fn transform_parameters(&self, params: &NodeList) -> Vec<IRParam> {
        let mut result = Vec::new();

        for &param_idx in &params.nodes {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param_data) = self.arena.get_parameter(param_node) else {
                continue;
            };

            let name = self.get_identifier_text(param_data.name);
            if name.is_empty() {
                continue;
            }

            let mut param = if param_data.dot_dot_dot_token {
                IRParam::rest(&name)
            } else {
                IRParam::new(&name)
            };

            if !param_data.initializer.is_none() {
                if let Some(default) = self.transform_expression(param_data.initializer) {
                    param = param.with_default(default);
                }
            }

            result.push(param);
        }

        result
    }

    /// Transform block contents to IR
    fn transform_block_contents(&self, body_idx: NodeIndex) -> Vec<IRNode> {
        let Some(body_node) = self.arena.get(body_idx) else {
            return vec![];
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return vec![];
        };

        let mut stmts = Vec::new();
        for &stmt_idx in &block.statements.nodes {
            if let Some(ir) = self.transform_statement(stmt_idx) {
                stmts.push(ir);
            }
        }
        stmts
    }

    /// Transform a statement to IR
    fn transform_statement(&self, stmt_idx: NodeIndex) -> Option<IRNode> {
        let stmt_node = self.arena.get(stmt_idx)?;

        match stmt_node.kind {
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                let expr_stmt = self.arena.get_expression_statement(stmt_node)?;
                let expr = self.transform_expression(expr_stmt.expression)?;
                Some(IRNode::expr_stmt(expr))
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                let ret_stmt = self.arena.get_return_statement(stmt_node)?;
                let expr = if ret_stmt.expression.is_none() {
                    None
                } else {
                    self.transform_expression(ret_stmt.expression)
                };
                Some(IRNode::ret(expr))
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                // For now, emit as AST reference (printer handles this)
                Some(IRNode::ASTRef(stmt_idx))
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                let if_stmt = self.arena.get_if_statement(stmt_node)?;
                let condition = self.transform_expression(if_stmt.expression)?;
                let then_branch = self.transform_statement(if_stmt.then_statement)?;
                let else_branch = if if_stmt.else_statement.is_none() {
                    None
                } else {
                    self.transform_statement(if_stmt.else_statement)
                };
                Some(IRNode::IfStatement {
                    condition: Box::new(condition),
                    then_branch: Box::new(then_branch),
                    else_branch: else_branch.map(Box::new),
                })
            }
            k if k == syntax_kind_ext::BLOCK => {
                let stmts = self.transform_block_contents(stmt_idx);
                Some(IRNode::block(stmts))
            }
            _ => {
                // Fallback to AST reference
                Some(IRNode::ASTRef(stmt_idx))
            }
        }
    }

    /// Transform an expression to IR
    fn transform_expression(&self, expr_idx: NodeIndex) -> Option<IRNode> {
        if expr_idx.is_none() {
            return None;
        }

        let expr_node = self.arena.get(expr_idx)?;

        match expr_node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.arena.get_literal(expr_node)?;
                Some(IRNode::number(&lit.text))
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                let lit = self.arena.get_literal(expr_node)?;
                Some(IRNode::string(&lit.text))
            }
            k if k == SyntaxKind::Identifier as u16 => {
                let ident = self.arena.get_identifier(expr_node)?;
                Some(IRNode::id(&ident.escaped_text))
            }
            k if k == SyntaxKind::TrueKeyword as u16 => Some(IRNode::BooleanLiteral(true)),
            k if k == SyntaxKind::FalseKeyword as u16 => Some(IRNode::BooleanLiteral(false)),
            k if k == SyntaxKind::NullKeyword as u16 => Some(IRNode::NullLiteral),
            k if k == SyntaxKind::ThisKeyword as u16 => Some(if self.use_this_capture {
                IRNode::this_captured()
            } else {
                IRNode::this()
            }),
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let bin = self.arena.get_binary_expr(expr_node)?;
                let left = self.transform_expression(bin.left)?;
                let right = self.transform_expression(bin.right)?;
                let op = self.get_operator_string(bin.operator_token);
                Some(IRNode::binary(left, op, right))
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                let call = self.arena.get_call_expr(expr_node)?;
                let callee = self.transform_expression(call.expression)?;
                let mut args = Vec::new();
                if let Some(ref arg_list) = call.arguments {
                    for &arg_idx in &arg_list.nodes {
                        if let Some(arg) = self.transform_expression(arg_idx) {
                            args.push(arg);
                        }
                    }
                }
                Some(IRNode::call(callee, args))
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr(expr_node)?;
                let object = self.transform_expression(access.expression)?;
                let property = self.get_identifier_text(access.name_or_argument);
                Some(IRNode::prop(object, &property))
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr(expr_node)?;
                let object = self.transform_expression(access.expression)?;
                let index = self.transform_expression(access.name_or_argument)?;
                Some(IRNode::elem(object, index))
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let paren = self.arena.get_parenthesized(expr_node)?;
                let expr = self.transform_expression(paren.expression)?;
                Some(expr.paren())
            }
            _ => {
                // Fallback to AST reference
                Some(IRNode::ASTRef(expr_idx))
            }
        }
    }

    // Helper methods

    fn get_identifier_text(&self, idx: NodeIndex) -> String {
        let Some(node) = self.arena.get(idx) else {
            return String::new();
        };
        if let Some(ident) = self.arena.get_identifier(node) {
            return ident.escaped_text.clone();
        }
        String::new()
    }

    fn get_method_name(&self, name_idx: NodeIndex) -> IRMethodName {
        let Some(name_node) = self.arena.get(name_idx) else {
            return IRMethodName::Identifier(String::new());
        };

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            if let Some(computed) = self.arena.get_computed_property(name_node) {
                if let Some(expr) = self.transform_expression(computed.expression) {
                    return IRMethodName::Computed(Box::new(expr));
                }
            }
        } else if name_node.kind == SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return IRMethodName::Identifier(ident.escaped_text.clone());
            }
        } else if name_node.kind == SyntaxKind::StringLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(name_node) {
                return IRMethodName::StringLiteral(lit.text.clone());
            }
        } else if name_node.kind == SyntaxKind::NumericLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(name_node) {
                return IRMethodName::NumericLiteral(lit.text.clone());
            }
        }

        IRMethodName::Identifier(String::new())
    }

    fn get_operator_string(&self, op: u16) -> String {
        match op {
            k if k == SyntaxKind::PlusToken as u16 => "+",
            k if k == SyntaxKind::MinusToken as u16 => "-",
            k if k == SyntaxKind::AsteriskToken as u16 => "*",
            k if k == SyntaxKind::SlashToken as u16 => "/",
            k if k == SyntaxKind::PercentToken as u16 => "%",
            k if k == SyntaxKind::EqualsToken as u16 => "=",
            k if k == SyntaxKind::PlusEqualsToken as u16 => "+=",
            k if k == SyntaxKind::MinusEqualsToken as u16 => "-=",
            k if k == SyntaxKind::EqualsEqualsToken as u16 => "==",
            k if k == SyntaxKind::EqualsEqualsEqualsToken as u16 => "===",
            k if k == SyntaxKind::ExclamationEqualsToken as u16 => "!=",
            k if k == SyntaxKind::ExclamationEqualsEqualsToken as u16 => "!==",
            k if k == SyntaxKind::LessThanToken as u16 => "<",
            k if k == SyntaxKind::GreaterThanToken as u16 => ">",
            k if k == SyntaxKind::LessThanEqualsToken as u16 => "<=",
            k if k == SyntaxKind::GreaterThanEqualsToken as u16 => ">=",
            k if k == SyntaxKind::AmpersandAmpersandToken as u16 => "&&",
            k if k == SyntaxKind::BarBarToken as u16 => "||",
            _ => "?",
        }
        .to_string()
    }

    fn has_declare_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        let Some(mods) = modifiers else {
            return false;
        };
        for &mod_idx in &mods.nodes {
            let Some(mod_node) = self.arena.get(mod_idx) else {
                continue;
            };
            if mod_node.kind == SyntaxKind::DeclareKeyword as u16 {
                return true;
            }
        }
        false
    }

    fn is_static(&self, modifiers: &Option<NodeList>) -> bool {
        let Some(mods) = modifiers else {
            return false;
        };
        for &mod_idx in &mods.nodes {
            let Some(mod_node) = self.arena.get(mod_idx) else {
                continue;
            };
            if mod_node.kind == SyntaxKind::StaticKeyword as u16 {
                return true;
            }
        }
        false
    }

    fn get_extends_class(&self, heritage: &Option<NodeList>) -> Option<IRNode> {
        let heritage_list = heritage.as_ref()?;
        for &clause_idx in &heritage_list.nodes {
            let clause_node = self.arena.get(clause_idx)?;
            if clause_node.kind != syntax_kind_ext::HERITAGE_CLAUSE {
                continue;
            }
            let heritage_clause = self.arena.get_heritage_clause(clause_node)?;
            if heritage_clause.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            // Get the first type in the extends clause
            let first_type_idx = heritage_clause.types.nodes.first()?;
            let type_node = self.arena.get(*first_type_idx)?;
            if let Some(expr_type) = self.arena.get_expr_type_args(type_node) {
                return self.transform_expression(expr_type.expression);
            }
        }
        None
    }

    fn get_temp_var_name(&mut self) -> String {
        let name = format!("_{}", (b'a' + (self.temp_var_counter % 26) as u8) as char);
        self.temp_var_counter += 1;
        name
    }
}

/// ES5 Async Transformer - produces IR nodes for async/await lowering
pub struct ES5AsyncTransformer<'a> {
    arena: &'a ThinNodeArena,
    /// Label counter for generator state machine
    label_counter: u32,
    /// Whether we're using _this capture
    use_this_capture: bool,
    /// Current class name for private field access
    class_name: Option<String>,
}

impl<'a> ES5AsyncTransformer<'a> {
    pub fn new(arena: &'a ThinNodeArena) -> Self {
        Self {
            arena,
            label_counter: 0,
            use_this_capture: false,
            class_name: None,
        }
    }

    pub fn set_this_capture(&mut self, capture: bool) {
        self.use_this_capture = capture;
    }

    pub fn set_class_name(&mut self, name: &str) {
        self.class_name = Some(name.to_string());
    }

    /// Transform an async function body to IR
    pub fn transform_async_body(&mut self, body_idx: NodeIndex) -> IRNode {
        let this_arg = if self.use_this_capture {
            IRNode::this_captured()
        } else {
            IRNode::this()
        };

        let has_await = self.body_contains_await(body_idx);

        let generator_body = if has_await {
            self.transform_generator_body_with_await(body_idx)
        } else {
            self.transform_simple_generator_body(body_idx)
        };

        IRNode::AwaiterCall {
            this_arg: Box::new(this_arg),
            generator_body: Box::new(generator_body),
        }
    }

    /// Transform a simple async body (no await) to IR
    fn transform_simple_generator_body(&self, body_idx: NodeIndex) -> IRNode {
        let Some(body_node) = self.arena.get(body_idx) else {
            return IRNode::GeneratorBody {
                has_await: false,
                cases: vec![IRGeneratorCase {
                    label: 0,
                    statements: vec![IRNode::ret(Some(IRNode::GeneratorOp {
                        opcode: 2,
                        value: None,
                        comment: Some("return".to_string()),
                    }))],
                }],
            };
        };

        if body_node.kind == syntax_kind_ext::BLOCK {
            if let Some(block) = self.arena.get_block(body_node) {
                if block.statements.nodes.is_empty() {
                    return IRNode::GeneratorBody {
                        has_await: false,
                        cases: vec![IRGeneratorCase {
                            label: 0,
                            statements: vec![IRNode::ret(Some(IRNode::GeneratorOp {
                                opcode: 2,
                                value: None,
                                comment: Some("return".to_string()),
                            }))],
                        }],
                    };
                }

                // Check for single return statement
                if block.statements.nodes.len() == 1 {
                    let stmt_idx = block.statements.nodes[0];
                    if let Some(stmt_node) = self.arena.get(stmt_idx) {
                        if stmt_node.kind == syntax_kind_ext::RETURN_STATEMENT {
                            if let Some(ret) = self.arena.get_return_statement(stmt_node) {
                                let value = if ret.expression.is_none() {
                                    None
                                } else {
                                    self.transform_expression(ret.expression)
                                };
                                return IRNode::GeneratorBody {
                                    has_await: false,
                                    cases: vec![IRGeneratorCase {
                                        label: 0,
                                        statements: vec![IRNode::ret(Some(IRNode::GeneratorOp {
                                            opcode: 2,
                                            value: value.map(Box::new),
                                            comment: Some("return".to_string()),
                                        }))],
                                    }],
                                };
                            }
                        }
                    }
                }
            }
        }

        // Non-trivial body - emit statements
        let stmts = self.transform_async_statements(body_idx);
        let mut final_stmts = stmts;
        final_stmts.push(IRNode::ret(Some(IRNode::GeneratorOp {
            opcode: 2,
            value: None,
            comment: Some("return".to_string()),
        })));

        IRNode::GeneratorBody {
            has_await: false,
            cases: vec![IRGeneratorCase {
                label: 0,
                statements: final_stmts,
            }],
        }
    }

    /// Transform async body with await expressions
    fn transform_generator_body_with_await(&mut self, body_idx: NodeIndex) -> IRNode {
        self.label_counter = 0;

        let mut cases = Vec::new();
        let mut current_stmts = Vec::new();

        // Process body statements, creating new cases at await points
        let stmts = self.collect_async_statements(body_idx);

        for stmt in stmts {
            match stmt {
                AsyncStatement::Normal(ir) => {
                    current_stmts.push(ir);
                }
                AsyncStatement::Await { operand } => {
                    // Emit yield for await
                    current_stmts.push(IRNode::ret(Some(IRNode::GeneratorOp {
                        opcode: 4,
                        value: Some(Box::new(operand)),
                        comment: Some("yield".to_string()),
                    })));

                    // Save current case
                    cases.push(IRGeneratorCase {
                        label: self.label_counter,
                        statements: current_stmts,
                    });

                    // Start new case
                    self.label_counter += 1;
                    current_stmts = vec![IRNode::expr_stmt(IRNode::GeneratorSent)];
                }
                AsyncStatement::ReturnAwait { operand } => {
                    // Emit yield for await
                    current_stmts.push(IRNode::ret(Some(IRNode::GeneratorOp {
                        opcode: 4,
                        value: Some(Box::new(operand)),
                        comment: Some("yield".to_string()),
                    })));

                    // Save current case
                    cases.push(IRGeneratorCase {
                        label: self.label_counter,
                        statements: current_stmts,
                    });

                    // New case returns the sent value
                    self.label_counter += 1;
                    cases.push(IRGeneratorCase {
                        label: self.label_counter,
                        statements: vec![IRNode::ret(Some(IRNode::GeneratorOp {
                            opcode: 2,
                            value: Some(Box::new(IRNode::GeneratorSent)),
                            comment: Some("return".to_string()),
                        }))],
                    });

                    self.label_counter += 1;
                    current_stmts = Vec::new();
                }
                AsyncStatement::Return { value } => {
                    current_stmts.push(IRNode::ret(Some(IRNode::GeneratorOp {
                        opcode: 2,
                        value: value.map(Box::new),
                        comment: Some("return".to_string()),
                    })));
                }
            }
        }

        // Add final return if needed
        if !current_stmts.is_empty() {
            current_stmts.push(IRNode::ret(Some(IRNode::GeneratorOp {
                opcode: 2,
                value: None,
                comment: Some("return".to_string()),
            })));
            cases.push(IRGeneratorCase {
                label: self.label_counter,
                statements: current_stmts,
            });
        }

        IRNode::GeneratorBody {
            has_await: true,
            cases,
        }
    }

    /// Collect statements from an async body, identifying await points
    fn collect_async_statements(&self, body_idx: NodeIndex) -> Vec<AsyncStatement> {
        let mut result = Vec::new();

        let Some(body_node) = self.arena.get(body_idx) else {
            return result;
        };

        if body_node.kind != syntax_kind_ext::BLOCK {
            // Concise arrow body - treat as return expression
            if let Some(expr) = self.transform_expression(body_idx) {
                if self.is_await_expression(body_idx) {
                    result.push(AsyncStatement::ReturnAwait { operand: expr });
                } else {
                    result.push(AsyncStatement::Return { value: Some(expr) });
                }
            }
            return result;
        }

        let Some(block) = self.arena.get_block(body_node) else {
            return result;
        };

        for &stmt_idx in &block.statements.nodes {
            self.collect_statement(stmt_idx, &mut result);
        }

        result
    }

    fn collect_statement(&self, stmt_idx: NodeIndex, result: &mut Vec<AsyncStatement>) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node) else {
                    return;
                };

                if self.is_await_expression(expr_stmt.expression) {
                    if let Some(operand) = self.get_await_operand(expr_stmt.expression) {
                        result.push(AsyncStatement::Await { operand });
                    }
                } else if let Some(expr) = self.transform_expression(expr_stmt.expression) {
                    result.push(AsyncStatement::Normal(IRNode::expr_stmt(expr)));
                }
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                let Some(ret) = self.arena.get_return_statement(stmt_node) else {
                    return;
                };

                if ret.expression.is_none() {
                    result.push(AsyncStatement::Return { value: None });
                } else if self.is_await_expression(ret.expression) {
                    if let Some(operand) = self.get_await_operand(ret.expression) {
                        result.push(AsyncStatement::ReturnAwait { operand });
                    }
                } else if let Some(value) = self.transform_expression(ret.expression) {
                    result.push(AsyncStatement::Return { value: Some(value) });
                }
            }
            _ => {
                // Other statements are passed through
                result.push(AsyncStatement::Normal(IRNode::ASTRef(stmt_idx)));
            }
        }
    }

    fn transform_async_statements(&self, body_idx: NodeIndex) -> Vec<IRNode> {
        let Some(body_node) = self.arena.get(body_idx) else {
            return vec![];
        };

        if body_node.kind != syntax_kind_ext::BLOCK {
            // Concise arrow body
            if let Some(expr) = self.transform_expression(body_idx) {
                return vec![IRNode::ret(Some(IRNode::GeneratorOp {
                    opcode: 2,
                    value: Some(Box::new(expr)),
                    comment: Some("return".to_string()),
                }))];
            }
            return vec![];
        }

        let Some(block) = self.arena.get_block(body_node) else {
            return vec![];
        };

        let mut stmts = Vec::new();
        for &stmt_idx in &block.statements.nodes {
            if let Some(ir) = self.transform_statement(stmt_idx) {
                stmts.push(ir);
            }
        }
        stmts
    }

    fn transform_statement(&self, stmt_idx: NodeIndex) -> Option<IRNode> {
        let stmt_node = self.arena.get(stmt_idx)?;

        match stmt_node.kind {
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                let expr_stmt = self.arena.get_expression_statement(stmt_node)?;
                let expr = self.transform_expression(expr_stmt.expression)?;
                Some(IRNode::expr_stmt(expr))
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                let ret = self.arena.get_return_statement(stmt_node)?;
                let value = if ret.expression.is_none() {
                    None
                } else {
                    self.transform_expression(ret.expression)
                };
                Some(IRNode::ret(value))
            }
            _ => Some(IRNode::ASTRef(stmt_idx)),
        }
    }

    fn transform_expression(&self, expr_idx: NodeIndex) -> Option<IRNode> {
        if expr_idx.is_none() {
            return None;
        }

        let expr_node = self.arena.get(expr_idx)?;

        match expr_node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.arena.get_literal(expr_node)?;
                Some(IRNode::number(&lit.text))
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                let lit = self.arena.get_literal(expr_node)?;
                Some(IRNode::string(&lit.text))
            }
            k if k == SyntaxKind::Identifier as u16 => {
                let ident = self.arena.get_identifier(expr_node)?;
                Some(IRNode::id(&ident.escaped_text))
            }
            k if k == SyntaxKind::TrueKeyword as u16 => Some(IRNode::BooleanLiteral(true)),
            k if k == SyntaxKind::FalseKeyword as u16 => Some(IRNode::BooleanLiteral(false)),
            k if k == SyntaxKind::NullKeyword as u16 => Some(IRNode::NullLiteral),
            k if k == SyntaxKind::ThisKeyword as u16 => Some(if self.use_this_capture {
                IRNode::this_captured()
            } else {
                IRNode::this()
            }),
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                let call = self.arena.get_call_expr(expr_node)?;
                let callee = self.transform_expression(call.expression)?;
                let mut args = Vec::new();
                if let Some(ref arg_list) = call.arguments {
                    for &arg_idx in &arg_list.nodes {
                        if let Some(arg) = self.transform_expression(arg_idx) {
                            args.push(arg);
                        }
                    }
                }
                Some(IRNode::call(callee, args))
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr(expr_node)?;
                let object = self.transform_expression(access.expression)?;
                let property = self.get_identifier_text(access.name_or_argument);
                Some(IRNode::prop(object, &property))
            }
            _ => Some(IRNode::ASTRef(expr_idx)),
        }
    }

    fn get_identifier_text(&self, idx: NodeIndex) -> String {
        let Some(node) = self.arena.get(idx) else {
            return String::new();
        };
        if let Some(ident) = self.arena.get_identifier(node) {
            return ident.escaped_text.clone();
        }
        String::new()
    }

    /// Check if body contains any await expressions
    pub fn body_contains_await(&self, body_idx: NodeIndex) -> bool {
        self.contains_await_recursive(body_idx)
    }

    fn contains_await_recursive(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::AWAIT_EXPRESSION {
            return true;
        }

        // Don't recurse into nested functions
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || node.kind == syntax_kind_ext::ARROW_FUNCTION
        {
            return false;
        }

        // Check children based on node type
        match node.kind {
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = self.arena.get_block(node) {
                    for &stmt_idx in &block.statements.nodes {
                        if self.contains_await_recursive(stmt_idx) {
                            return true;
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.arena.get_expression_statement(node) {
                    return self.contains_await_recursive(expr_stmt.expression);
                }
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.arena.get_return_statement(node) {
                    return self.contains_await_recursive(ret.expression);
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(node) {
                    if self.contains_await_recursive(call.expression) {
                        return true;
                    }
                    if let Some(args) = &call.arguments {
                        for &arg_idx in &args.nodes {
                            if self.contains_await_recursive(arg_idx) {
                                return true;
                            }
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.arena.get_binary_expr(node) {
                    if self.contains_await_recursive(bin.left) {
                        return true;
                    }
                    if self.contains_await_recursive(bin.right) {
                        return true;
                    }
                }
            }
            _ => {}
        }

        false
    }

    fn is_await_expression(&self, idx: NodeIndex) -> bool {
        if let Some(node) = self.arena.get(idx) {
            return node.kind == syntax_kind_ext::AWAIT_EXPRESSION;
        }
        false
    }

    fn get_await_operand(&self, await_idx: NodeIndex) -> Option<IRNode> {
        let await_node = self.arena.get(await_idx)?;
        let unary_ex = self.arena.get_unary_expr_ex(await_node)?;
        self.transform_expression(unary_ex.expression)
    }
}

/// Represents a statement during async body analysis
enum AsyncStatement {
    Normal(IRNode),
    Await { operand: IRNode },
    ReturnAwait { operand: IRNode },
    Return { value: Option<IRNode> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_es5_class_transformer_basic() {
        // This would need actual AST nodes to test properly
        // For now, just verify the transformer compiles
        let arena = ThinNodeArena::new();
        let mut transformer = ES5ClassTransformer::new(&arena);
        assert!(transformer.transform_class(NodeIndex::NONE).is_none());
    }

    #[test]
    fn test_es5_async_transformer_basic() {
        let arena = ThinNodeArena::new();
        let transformer = ES5AsyncTransformer::new(&arena);
        assert!(!transformer.body_contains_await(NodeIndex::NONE));
    }
}
