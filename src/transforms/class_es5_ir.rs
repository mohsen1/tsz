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
//! ## Derived Classes with super()
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
//! - Constructors with super() calls
//! - Instance and static methods
//! - Instance and static properties
//! - Getters and setters (combined into Object.defineProperty)
//! - Private fields (WeakMap pattern)
//! - Parameter properties (public/private/protected/readonly)
//! - Async methods (__awaiter wrapper)
//! - Computed property names
//! - Static blocks
//!
//! The `AstToIr` converter handles most JavaScript constructs. For complex or edge cases
//! not yet supported, it falls back to `IRNode::ASTRef` which copies source text directly.

use crate::parser::node::NodeArena;
use crate::parser::syntax_kind_ext;
use crate::parser::{NodeIndex, NodeList};
use crate::scanner::SyntaxKind;
use crate::syntax::transform_utils::contains_this_reference;
use crate::syntax::transform_utils::is_private_identifier;
use crate::transform_context::TransformContext;
use crate::transforms::async_es5_ir::AsyncES5Transformer;
use crate::transforms::ir::*;
use crate::transforms::private_fields_es5::{
    PrivateAccessorInfo, PrivateFieldInfo, collect_private_accessors, collect_private_fields,
};
use std::cell::Cell;
use std::collections::HashMap;

/// Context for ES5 class transformation
pub struct ES5ClassTransformer<'a> {
    arena: &'a NodeArena,
    class_name: String,
    has_extends: bool,
    private_fields: Vec<PrivateFieldInfo>,
    private_accessors: Vec<PrivateAccessorInfo>,
    /// Transform directives from LoweringPass
    transforms: Option<TransformContext>,
}

impl<'a> ES5ClassTransformer<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            class_name: String::new(),
            has_extends: false,
            private_fields: Vec::new(),
            private_accessors: Vec::new(),
            transforms: None,
        }
    }

    /// Set transform directives from LoweringPass
    pub fn set_transforms(&mut self, transforms: TransformContext) {
        self.transforms = Some(transforms);
    }

    /// Convert an AST statement to IR (avoids ASTRef when possible)
    fn convert_statement(&self, idx: NodeIndex) -> IRNode {
        let mut converter = AstToIr::new(self.arena);
        if let Some(ref transforms) = self.transforms {
            converter = converter.with_transforms(transforms.clone());
        }
        converter.convert_statement(idx)
    }

    /// Convert an AST expression to IR (avoids ASTRef when possible)
    fn convert_expression(&self, idx: NodeIndex) -> IRNode {
        let mut converter = AstToIr::new(self.arena);
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
                    name: alias.clone(),
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

        self.class_name = class_name.clone();

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
        self.emit_static_members_ir(&mut body, class_idx);

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
                    weakmap_inits.push(format!("{} = new WeakMap()", get_var));
                }
                if let Some(ref set_var) = acc.set_var_name {
                    weakmap_inits.push(format!("{} = new WeakMap()", set_var));
                }
            }
        }

        Some(IRNode::ES5ClassIIFE {
            name: self.class_name.clone(),
            base_class: base_class.map(Box::new),
            body,
            weakmap_decls,
            weakmap_inits,
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
                // Skip private fields (they use WeakMap pattern)
                if is_private_identifier(self.arena, prop_data.name) {
                    return None;
                }
                // Include if has initializer
                if !prop_data.initializer.is_none() {
                    Some(member_idx)
                } else {
                    None
                }
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
        let has_private_fields = self.private_fields.iter().any(|f| !f.is_static);

        if let Some(ctor) = constructor_data {
            // Extract parameters
            params = self.extract_parameters(&ctor.parameters);

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

        Some(IRNode::FunctionDecl {
            name: self.class_name.clone(),
            parameters: params,
            body: ctor_body,
        })
    }

    /// Emit derived class constructor body with super() transformation
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
        for (i, &stmt_idx) in block.statements.nodes.iter().enumerate() {
            if i >= super_stmt_position && super_stmt_idx.is_some() {
                break;
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
        if super_stmt_idx.is_some() {
            for (i, &stmt_idx) in block.statements.nodes.iter().enumerate() {
                if i <= super_stmt_position {
                    continue;
                }
                // Transform this to _this in these statements
                body.push(self.convert_statement(stmt_idx));
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
        // Emit private field initializations first
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
            for &stmt_idx in &block.statements.nodes {
                body.push(self.convert_statement(stmt_idx));
            }
        }
    }

    /// Check if a statement is a super() call
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
        {
            if let Some(ref call_args) = call.arguments {
                for &arg_idx in &call_args.nodes {
                    args.push(self.convert_expression(arg_idx));
                }
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

            if has_parameter_property_modifier(self.arena, &param.modifiers) {
                if let Some(param_name) = get_identifier_text(self.arena, param.name) {
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
    }

    /// Emit private field initializations using WeakMap.set()
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

    /// Emit private accessor initializations using WeakMap.set()
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
        } else if name_node.kind == SyntaxKind::NumericLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(name_node) {
                return Some(PropertyNameIR::NumericLiteral(lit.text.clone()));
            }
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

    /// Check if a static method body contains arrow functions with class_alias,
    /// and return the alias if found
    fn get_class_alias_for_static_method(&self, body_idx: NodeIndex) -> Option<String> {
        if let Some(ref transforms) = self.transforms {
            // Get all arrow function nodes in the method body
            let arrow_indices = self.collect_arrow_functions_in_block(body_idx);
            // Check if any arrow function has a class_alias directive
            for &arrow_idx in &arrow_indices {
                if let Some(dir) = transforms.get(arrow_idx) {
                    if let crate::transform_context::TransformDirective::ES5ArrowFunction {
                        class_alias,
                        ..
                    } = dir
                    {
                        if let Some(alias) = class_alias {
                            return Some(alias.to_string());
                        }
                    }
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

    /// Recursively collect arrow function indices starting from a node
    fn collect_arrow_functions_in_node(&self, idx: NodeIndex, arrows: &mut Vec<NodeIndex>) {
        use crate::parser::syntax_kind_ext;

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
        let mut emitted_accessors: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        // Second pass: emit methods and accessors in source order
        for &member_idx in &class_data.members.nodes {
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
                    .map(|body_node| (body_node.pos as u32, body_node.end as u32));

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
                    self.convert_block_body(method_data.body)
                };

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
                            property_name: accessor_name.clone(),
                            descriptor: IRPropertyDescriptor {
                                get: get_fn.map(Box::new),
                                set: set_fn.map(Box::new),
                                enumerable: false,
                                configurable: true,
                            },
                        });

                        emitted_accessors.insert(accessor_name);
                    }
                }
            }
        }
    }

    /// Build a getter function IR from an accessor node
    fn build_getter_function_ir(&self, accessor_idx: NodeIndex) -> Option<IRNode> {
        let accessor_node = self.arena.get(accessor_idx)?;
        let accessor_data = self.arena.get_accessor(accessor_node)?;

        Some(IRNode::FunctionExpr {
            name: None,
            parameters: vec![],
            body: if accessor_data.body.is_none() {
                vec![]
            } else {
                self.convert_block_body(accessor_data.body)
            },
            is_expression_body: false,
            body_source_range: None,
        })
    }

    /// Build a setter function IR from an accessor node
    fn build_setter_function_ir(&self, accessor_idx: NodeIndex) -> Option<IRNode> {
        let accessor_node = self.arena.get(accessor_idx)?;
        let accessor_data = self.arena.get_accessor(accessor_node)?;

        let params = self.extract_parameters(&accessor_data.parameters);

        Some(IRNode::FunctionExpr {
            name: None,
            parameters: params,
            body: if accessor_data.body.is_none() {
                vec![]
            } else {
                self.convert_block_body(accessor_data.body)
            },
            is_expression_body: false,
            body_source_range: None,
        })
    }

    /// Emit static members as IR
    fn emit_static_members_ir(&self, body: &mut Vec<IRNode>, class_idx: NodeIndex) {
        let Some(class_node) = self.arena.get(class_idx) else {
            return;
        };
        let Some(class_data) = self.arena.get_class(class_node) else {
            return;
        };

        // First pass: collect static accessors by name to combine getter/setter pairs
        let static_accessor_map = collect_accessor_pairs(self.arena, &class_data.members, true);

        // Track which static accessor names we've emitted
        let mut emitted_static_accessors: std::collections::HashSet<String> =
            std::collections::HashSet::new();

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

                // ClassName.methodName = function () { body };
                body.push(IRNode::StaticMethod {
                    class_name: self.class_name.clone(),
                    method_name,
                    function: Box::new(IRNode::FunctionExpr {
                        name: None,
                        parameters: params,
                        body: method_body,
                        is_expression_body: false,
                        body_source_range: None,
                    }),
                });
            } else if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                let Some(prop_data) = self.arena.get_property_decl(member_node) else {
                    continue;
                };

                // Only static properties
                if !has_static_modifier(self.arena, &prop_data.modifiers) {
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
                // Static block: emit contents as a Sequence node
                // The static block node itself has a block structure
                if let Some(block_data) = self.arena.get_block(member_node) {
                    let statements: Vec<IRNode> = block_data
                        .statements
                        .nodes
                        .iter()
                        .map(|&stmt_idx| self.convert_statement(stmt_idx))
                        .collect();

                    if !statements.is_empty() {
                        body.push(IRNode::Sequence(statements));
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
                            property_name: accessor_name.clone(),
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
        } else if name_node.kind == SyntaxKind::NumericLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(name_node) {
                return IRMethodName::NumericLiteral(lit.text.clone());
            }
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
) -> HashMap<String, (Option<NodeIndex>, Option<NodeIndex>)> {
    let mut accessor_map: HashMap<String, (Option<NodeIndex>, Option<NodeIndex>)> = HashMap::new();

    for &member_idx in &members.nodes {
        let Some(member_node) = arena.get(member_idx) else {
            continue;
        };

        if member_node.kind == syntax_kind_ext::GET_ACCESSOR
            || member_node.kind == syntax_kind_ext::SET_ACCESSOR
        {
            if let Some(accessor_data) = arena.get_accessor(member_node) {
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
// AST to IR Conversion
// =============================================================================

/// Convert an AST node to IR, avoiding ASTRef when possible
pub struct AstToIr<'a> {
    arena: &'a NodeArena,
    /// Track if we're inside an arrow function that captures `this`
    this_captured: Cell<bool>,
    /// Transform directives from LoweringPass
    transforms: Option<TransformContext>,
    /// Current class alias to use for `this` substitution in static methods
    current_class_alias: Cell<Option<String>>,
}

impl<'a> AstToIr<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            this_captured: Cell::new(false),
            transforms: None,
            current_class_alias: Cell::new(None),
        }
    }

    /// Set transform directives from LoweringPass
    pub fn with_transforms(mut self, transforms: TransformContext) -> Self {
        self.transforms = Some(transforms);
        self
    }

    /// Set the current class alias for `this` substitution
    pub fn with_class_alias(self, alias: Option<String>) -> Self {
        self.current_class_alias.set(alias);
        self
    }

    /// Convert a statement to IR
    pub fn convert_statement(&self, idx: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(idx) else {
            return IRNode::ASTRef(idx);
        };

        match node.kind {
            k if k == syntax_kind_ext::BLOCK => self.convert_block(idx),
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                self.convert_expression_statement(idx)
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => self.convert_return_statement(idx),
            k if k == syntax_kind_ext::IF_STATEMENT => self.convert_if_statement(idx),
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => self.convert_variable_statement(idx),
            k if k == syntax_kind_ext::THROW_STATEMENT => self.convert_throw_statement(idx),
            k if k == syntax_kind_ext::TRY_STATEMENT => self.convert_try_statement(idx),
            k if k == syntax_kind_ext::FOR_STATEMENT => self.convert_for_statement(idx),
            k if k == syntax_kind_ext::WHILE_STATEMENT => self.convert_while_statement(idx),
            k if k == syntax_kind_ext::DO_STATEMENT => self.convert_do_while_statement(idx),
            k if k == syntax_kind_ext::SWITCH_STATEMENT => self.convert_switch_statement(idx),
            k if k == syntax_kind_ext::BREAK_STATEMENT => self.convert_break_statement(idx),
            k if k == syntax_kind_ext::CONTINUE_STATEMENT => self.convert_continue_statement(idx),
            k if k == syntax_kind_ext::LABELED_STATEMENT => self.convert_labeled_statement(idx),
            k if k == syntax_kind_ext::EMPTY_STATEMENT => IRNode::EmptyStatement,
            k if k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT =>
            {
                self.convert_for_in_of_statement(idx)
            }
            _ => IRNode::ASTRef(idx), // Fallback for unsupported statements
        }
    }

    /// Convert an expression to IR
    pub fn convert_expression(&self, idx: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(idx) else {
            return IRNode::ASTRef(idx);
        };

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => self.convert_identifier(idx),
            k if k == SyntaxKind::NumericLiteral as u16 => self.convert_numeric_literal(idx),
            k if k == SyntaxKind::StringLiteral as u16 => self.convert_string_literal(idx),
            k if k == SyntaxKind::TrueKeyword as u16 => IRNode::BooleanLiteral(true),
            k if k == SyntaxKind::FalseKeyword as u16 => IRNode::BooleanLiteral(false),
            k if k == SyntaxKind::NullKeyword as u16 => IRNode::NullLiteral,
            k if k == SyntaxKind::UndefinedKeyword as u16 => IRNode::Undefined,
            k if k == SyntaxKind::ThisKeyword as u16 => {
                // If we have a class_alias set (static method context), use it instead of `this`
                if let Some(alias) = self.current_class_alias.take() {
                    self.current_class_alias.set(Some(alias.clone()));
                    IRNode::Identifier(alias)
                } else {
                    IRNode::This {
                        captured: self.this_captured.get(),
                    }
                }
            }
            k if k == SyntaxKind::SuperKeyword as u16 => IRNode::Super,
            k if k == syntax_kind_ext::CALL_EXPRESSION => self.convert_call_expression(idx),
            k if k == syntax_kind_ext::NEW_EXPRESSION => self.convert_new_expression(idx),
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                self.convert_property_access(idx)
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                self.convert_element_access(idx)
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => self.convert_binary_expression(idx),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => self.convert_prefix_unary(idx),
            k if k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION => self.convert_postfix_unary(idx),
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => self.convert_parenthesized(idx),
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => self.convert_conditional(idx),
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => self.convert_array_literal(idx),
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.convert_object_literal(idx)
            }
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => self.convert_function_expression(idx),
            k if k == syntax_kind_ext::ARROW_FUNCTION => self.convert_arrow_function(idx),
            k if k == syntax_kind_ext::SPREAD_ELEMENT => self.convert_spread_element(idx),
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                self.convert_template_literal(idx)
            }
            k if k == syntax_kind_ext::AWAIT_EXPRESSION => self.convert_await_expression(idx),
            k if k == syntax_kind_ext::TYPE_ASSERTION || k == syntax_kind_ext::AS_EXPRESSION => {
                // Type assertions are stripped in ES5
                self.convert_type_assertion(idx)
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => self.convert_non_null(idx),
            _ => IRNode::ASTRef(idx), // Fallback
        }
    }

    fn convert_block(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(block) = self.arena.get_block(node) {
            let stmts: Vec<IRNode> = block
                .statements
                .nodes
                .iter()
                .map(|&s| self.convert_statement(s))
                .collect();
            IRNode::Block(stmts)
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_expression_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(expr_stmt) = self.arena.get_expression_statement(node) {
            IRNode::ExpressionStatement(Box::new(self.convert_expression(expr_stmt.expression)))
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_return_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(ret) = self.arena.get_return_statement(node) {
            let expr = if ret.expression.is_none() {
                None
            } else {
                Some(Box::new(self.convert_expression(ret.expression)))
            };
            IRNode::ReturnStatement(expr)
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_if_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(if_stmt) = self.arena.get_if_statement(node) {
            let else_branch = if if_stmt.else_statement.is_none() {
                None
            } else {
                Some(Box::new(self.convert_statement(if_stmt.else_statement)))
            };
            IRNode::IfStatement {
                condition: Box::new(self.convert_expression(if_stmt.expression)),
                then_branch: Box::new(self.convert_statement(if_stmt.then_statement)),
                else_branch,
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_variable_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // VariableStatement uses VariableData which has declarations directly
        if let Some(var_data) = self.arena.get_variable(node) {
            // Collect all declaration indices, handling the case where
            // VariableData.declarations may contain VARIABLE_DECLARATION_LIST nodes
            let mut decl_indices = Vec::new();
            for &decl_idx in &var_data.declarations.nodes {
                if let Some(decl_node) = self.arena.get(decl_idx) {
                    use crate::parser::syntax_kind_ext;
                    // Check if this is a VARIABLE_DECLARATION_LIST (intermediate node)
                    if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                        // Get the VariableData for this list and collect its declarations
                        if let Some(list_var_data) = self.arena.get_variable(decl_node) {
                            for &actual_decl_idx in &list_var_data.declarations.nodes {
                                decl_indices.push(actual_decl_idx);
                            }
                        }
                    } else {
                        // Direct VARIABLE_DECLARATION node
                        decl_indices.push(decl_idx);
                    }
                }
            }

            let decls: Vec<IRNode> = decl_indices
                .iter()
                .filter_map(|&d| self.convert_variable_declaration(d))
                .collect();

            if decls.is_empty() {
                // If all declarations were filtered out (e.g., due to parsing issues),
                // fallback to source text
                return IRNode::ASTRef(idx);
            }
            if decls.len() == 1 {
                return decls.into_iter().next().unwrap();
            }
            return IRNode::VarDeclList(decls);
        }
        IRNode::ASTRef(idx)
    }

    fn convert_variable_declaration(&self, idx: NodeIndex) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        let var_decl = self.arena.get_variable_declaration(node)?;

        // Try to get identifier text, but handle binding patterns and other cases
        let name = if let Some(name) = get_identifier_text(self.arena, var_decl.name) {
            name
        } else if let Some(name_node) = self.arena.get(var_decl.name) {
            // Fallback: try to get text from source span if available
            // For binding patterns, return None and let caller handle via ASTRef
            if name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            {
                return None; // Handled via ASTRef
            }
            // Try getting identifier via IdentifierData
            if let Some(id_data) = self.arena.get_identifier(name_node) {
                id_data.escaped_text.clone()
            } else {
                return None;
            }
        } else {
            return None;
        };

        let initializer = if var_decl.initializer.is_none() {
            None
        } else {
            Some(Box::new(self.convert_expression(var_decl.initializer)))
        };
        Some(IRNode::VarDecl { name, initializer })
    }

    fn convert_throw_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // Throw uses ReturnData (same as return statement)
        if let Some(return_data) = self.arena.get_return_statement(node) {
            IRNode::ThrowStatement(Box::new(self.convert_expression(return_data.expression)))
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_try_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(try_data) = self.arena.get_try(node) {
            let try_block = Box::new(self.convert_statement(try_data.try_block));

            let catch_clause = if try_data.catch_clause.is_none() {
                None
            } else if let Some(catch_node) = self.arena.get(try_data.catch_clause)
                && let Some(catch) = self.arena.get_catch_clause(catch_node)
            {
                let param = if catch.variable_declaration.is_none() {
                    None
                } else {
                    get_identifier_text(self.arena, catch.variable_declaration)
                };
                let catch_block = self.arena.get(catch.block);
                let body = if let Some(block_node) = catch_block
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
                Some(IRCatchClause { param, body })
            } else {
                None
            };

            let finally_block = if try_data.finally_block.is_none() {
                None
            } else {
                Some(Box::new(self.convert_statement(try_data.finally_block)))
            };

            IRNode::TryStatement {
                try_block,
                catch_clause,
                finally_block,
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_for_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // For uses LoopData (same as while/do-while)
        if let Some(loop_data) = self.arena.get_loop(node) {
            let initializer = if loop_data.initializer.is_none() {
                None
            } else {
                Some(Box::new(self.convert_expression(loop_data.initializer)))
            };
            let condition = if loop_data.condition.is_none() {
                None
            } else {
                Some(Box::new(self.convert_expression(loop_data.condition)))
            };
            let incrementor = if loop_data.incrementor.is_none() {
                None
            } else {
                Some(Box::new(self.convert_expression(loop_data.incrementor)))
            };
            IRNode::ForStatement {
                initializer,
                condition,
                incrementor,
                body: Box::new(self.convert_statement(loop_data.statement)),
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_while_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // While uses LoopData (same as for/do-while)
        if let Some(loop_data) = self.arena.get_loop(node) {
            IRNode::WhileStatement {
                condition: Box::new(self.convert_expression(loop_data.condition)),
                body: Box::new(self.convert_statement(loop_data.statement)),
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_do_while_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // DoWhile uses LoopData (same as while/for loops)
        if let Some(loop_data) = self.arena.get_loop(node) {
            IRNode::DoWhileStatement {
                body: Box::new(self.convert_statement(loop_data.statement)),
                condition: Box::new(self.convert_expression(loop_data.condition)),
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_switch_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(switch_data) = self.arena.get_switch(node) {
            // Case block uses BlockData where statements contains the case clauses
            let cases = if let Some(case_block_node) = self.arena.get(switch_data.case_block)
                && let Some(block_data) = self.arena.get_block(case_block_node)
            {
                block_data
                    .statements
                    .nodes
                    .iter()
                    .map(|&c| self.convert_switch_case(c))
                    .collect()
            } else {
                vec![]
            };
            IRNode::SwitchStatement {
                expression: Box::new(self.convert_expression(switch_data.expression)),
                cases,
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_switch_case(&self, idx: NodeIndex) -> IRSwitchCase {
        let node = self.arena.get(idx).unwrap();
        // get_case_clause works for both CASE_CLAUSE and DEFAULT_CLAUSE
        // For DEFAULT_CLAUSE, expression is NONE
        if let Some(case_clause) = self.arena.get_case_clause(node) {
            let test = if case_clause.expression.is_none() {
                None // Default clause
            } else {
                Some(self.convert_expression(case_clause.expression))
            };
            IRSwitchCase {
                test,
                statements: case_clause
                    .statements
                    .nodes
                    .iter()
                    .map(|&s| self.convert_statement(s))
                    .collect(),
            }
        } else {
            IRSwitchCase {
                test: None,
                statements: vec![],
            }
        }
    }

    fn convert_break_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(jump_data) = self.arena.get_jump_data(node) {
            let label = if jump_data.label.is_none() {
                None
            } else {
                get_identifier_text(self.arena, jump_data.label)
            };
            IRNode::BreakStatement(label)
        } else {
            IRNode::BreakStatement(None)
        }
    }

    fn convert_continue_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(jump_data) = self.arena.get_jump_data(node) {
            let label = if jump_data.label.is_none() {
                None
            } else {
                get_identifier_text(self.arena, jump_data.label)
            };
            IRNode::ContinueStatement(label)
        } else {
            IRNode::ContinueStatement(None)
        }
    }

    fn convert_labeled_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(labeled) = self.arena.get_labeled_statement(node) {
            if let Some(label) = get_identifier_text(self.arena, labeled.label) {
                return IRNode::LabeledStatement {
                    label,
                    statement: Box::new(self.convert_statement(labeled.statement)),
                };
            }
        }
        IRNode::ASTRef(idx)
    }

    fn convert_for_in_of_statement(&self, idx: NodeIndex) -> IRNode {
        // For-in/for-of need ES5 transformation - use ASTRef for now
        // A complete implementation would convert to a regular for loop
        IRNode::ASTRef(idx)
    }

    fn convert_identifier(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(ident) = self.arena.get_identifier(node) {
            IRNode::Identifier(ident.escaped_text.clone())
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_numeric_literal(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(lit) = self.arena.get_literal(node) {
            IRNode::NumericLiteral(lit.text.clone())
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_string_literal(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(lit) = self.arena.get_literal(node) {
            IRNode::StringLiteral(lit.text.clone())
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_call_expression(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(call) = self.arena.get_call_expr(node) {
            let callee = self.convert_expression(call.expression);
            let args = if let Some(ref args) = call.arguments {
                args.nodes
                    .iter()
                    .map(|&a| self.convert_expression(a))
                    .collect()
            } else {
                vec![]
            };
            IRNode::CallExpr {
                callee: Box::new(callee),
                arguments: args,
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_new_expression(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // NewExpression uses CallExprData (same as CallExpression)
        if let Some(call_data) = self.arena.get_call_expr(node) {
            let callee = self.convert_expression(call_data.expression);
            let args = if let Some(ref args) = call_data.arguments {
                args.nodes
                    .iter()
                    .map(|&a| self.convert_expression(a))
                    .collect()
            } else {
                vec![]
            };
            IRNode::NewExpr {
                callee: Box::new(callee),
                arguments: args,
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_property_access(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // PropertyAccessExpression uses AccessExprData
        if let Some(access) = self.arena.get_access_expr(node) {
            let object = self.convert_expression(access.expression);
            if let Some(name) = get_identifier_text(self.arena, access.name_or_argument) {
                return IRNode::PropertyAccess {
                    object: Box::new(object),
                    property: name,
                };
            }
        }
        IRNode::ASTRef(idx)
    }

    fn convert_element_access(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // ElementAccessExpression uses AccessExprData
        if let Some(access) = self.arena.get_access_expr(node) {
            let object = self.convert_expression(access.expression);
            let index = self.convert_expression(access.name_or_argument);
            IRNode::ElementAccess {
                object: Box::new(object),
                index: Box::new(index),
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_binary_expression(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(bin) = self.arena.get_binary_expr(node) {
            let left = self.convert_expression(bin.left);
            let right = self.convert_expression(bin.right);
            let op = self.get_binary_operator(bin.operator_token);

            // Handle logical operators specially
            if op == "||" {
                return IRNode::LogicalOr {
                    left: Box::new(left),
                    right: Box::new(right),
                };
            }
            if op == "&&" {
                return IRNode::LogicalAnd {
                    left: Box::new(left),
                    right: Box::new(right),
                };
            }

            IRNode::BinaryExpr {
                left: Box::new(left),
                operator: op,
                right: Box::new(right),
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn get_binary_operator(&self, token: u16) -> String {
        match token {
            k if k == SyntaxKind::PlusToken as u16 => "+".to_string(),
            k if k == SyntaxKind::MinusToken as u16 => "-".to_string(),
            k if k == SyntaxKind::AsteriskToken as u16 => "*".to_string(),
            k if k == SyntaxKind::SlashToken as u16 => "/".to_string(),
            k if k == SyntaxKind::PercentToken as u16 => "%".to_string(),
            k if k == SyntaxKind::EqualsToken as u16 => "=".to_string(),
            k if k == SyntaxKind::PlusEqualsToken as u16 => "+=".to_string(),
            k if k == SyntaxKind::MinusEqualsToken as u16 => "-=".to_string(),
            k if k == SyntaxKind::AsteriskEqualsToken as u16 => "*=".to_string(),
            k if k == SyntaxKind::SlashEqualsToken as u16 => "/=".to_string(),
            k if k == SyntaxKind::EqualsEqualsToken as u16 => "==".to_string(),
            k if k == SyntaxKind::EqualsEqualsEqualsToken as u16 => "===".to_string(),
            k if k == SyntaxKind::ExclamationEqualsToken as u16 => "!=".to_string(),
            k if k == SyntaxKind::ExclamationEqualsEqualsToken as u16 => "!==".to_string(),
            k if k == SyntaxKind::LessThanToken as u16 => "<".to_string(),
            k if k == SyntaxKind::LessThanEqualsToken as u16 => "<=".to_string(),
            k if k == SyntaxKind::GreaterThanToken as u16 => ">".to_string(),
            k if k == SyntaxKind::GreaterThanEqualsToken as u16 => ">=".to_string(),
            k if k == SyntaxKind::AmpersandAmpersandToken as u16 => "&&".to_string(),
            k if k == SyntaxKind::BarBarToken as u16 => "||".to_string(),
            k if k == SyntaxKind::AmpersandToken as u16 => "&".to_string(),
            k if k == SyntaxKind::BarToken as u16 => "|".to_string(),
            k if k == SyntaxKind::CaretToken as u16 => "^".to_string(),
            k if k == SyntaxKind::LessThanLessThanToken as u16 => "<<".to_string(),
            k if k == SyntaxKind::GreaterThanGreaterThanToken as u16 => ">>".to_string(),
            k if k == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16 => {
                ">>>".to_string()
            }
            k if k == SyntaxKind::InKeyword as u16 => "in".to_string(),
            k if k == SyntaxKind::InstanceOfKeyword as u16 => "instanceof".to_string(),
            k if k == SyntaxKind::CommaToken as u16 => ",".to_string(),
            _ => "?".to_string(),
        }
    }

    fn convert_prefix_unary(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // PrefixUnaryExpression uses UnaryExprData
        if let Some(unary) = self.arena.get_unary_expr(node) {
            let operand = self.convert_expression(unary.operand);
            let op = self.get_prefix_operator(unary.operator);
            IRNode::PrefixUnaryExpr {
                operator: op,
                operand: Box::new(operand),
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn get_prefix_operator(&self, token: u16) -> String {
        match token {
            k if k == SyntaxKind::PlusPlusToken as u16 => "++".to_string(),
            k if k == SyntaxKind::MinusMinusToken as u16 => "--".to_string(),
            k if k == SyntaxKind::ExclamationToken as u16 => "!".to_string(),
            k if k == SyntaxKind::TildeToken as u16 => "~".to_string(),
            k if k == SyntaxKind::PlusToken as u16 => "+".to_string(),
            k if k == SyntaxKind::MinusToken as u16 => "-".to_string(),
            k if k == SyntaxKind::TypeOfKeyword as u16 => "typeof ".to_string(),
            k if k == SyntaxKind::VoidKeyword as u16 => "void ".to_string(),
            k if k == SyntaxKind::DeleteKeyword as u16 => "delete ".to_string(),
            _ => "".to_string(),
        }
    }

    fn convert_postfix_unary(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // PostfixUnaryExpression uses UnaryExprData
        if let Some(unary) = self.arena.get_unary_expr(node) {
            let operand = self.convert_expression(unary.operand);
            let op = match unary.operator {
                k if k == SyntaxKind::PlusPlusToken as u16 => "++".to_string(),
                k if k == SyntaxKind::MinusMinusToken as u16 => "--".to_string(),
                _ => "".to_string(),
            };
            IRNode::PostfixUnaryExpr {
                operand: Box::new(operand),
                operator: op,
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_parenthesized(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(paren) = self.arena.get_parenthesized(node) {
            IRNode::Parenthesized(Box::new(self.convert_expression(paren.expression)))
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_conditional(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // ConditionalExpression uses ConditionalExprData
        if let Some(cond) = self.arena.get_conditional_expr(node) {
            IRNode::ConditionalExpr {
                condition: Box::new(self.convert_expression(cond.condition)),
                when_true: Box::new(self.convert_expression(cond.when_true)),
                when_false: Box::new(self.convert_expression(cond.when_false)),
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_array_literal(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // Array and Object literals use LiteralExprData
        if let Some(arr) = self.arena.get_literal_expr(node) {
            let elements: Vec<IRNode> = arr
                .elements
                .nodes
                .iter()
                .map(|&e| self.convert_expression(e))
                .collect();
            IRNode::ArrayLiteral(elements)
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_object_literal(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // Array and Object literals use LiteralExprData (elements = properties)
        if let Some(obj) = self.arena.get_literal_expr(node) {
            let props: Vec<IRProperty> = obj
                .elements
                .nodes
                .iter()
                .filter_map(|&p| self.convert_object_property(p))
                .collect();
            IRNode::ObjectLiteral(props)
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_object_property(&self, idx: NodeIndex) -> Option<IRProperty> {
        let node = self.arena.get(idx)?;

        if let Some(prop_assign) = self.arena.get_property_assignment(node) {
            let key = self.get_property_key(prop_assign.name)?;
            let value = self.convert_expression(prop_assign.initializer);
            Some(IRProperty {
                key,
                value,
                kind: IRPropertyKind::Init,
            })
        } else if let Some(shorthand) = self.arena.get_shorthand_property(node) {
            let name = get_identifier_text(self.arena, shorthand.name)?;
            Some(IRProperty {
                key: IRPropertyKey::Identifier(name.clone()),
                value: IRNode::Identifier(name),
                kind: IRPropertyKind::Init,
            })
        } else {
            None
        }
    }

    fn get_property_key(&self, idx: NodeIndex) -> Option<IRPropertyKey> {
        let node = self.arena.get(idx)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            let name = get_identifier_text(self.arena, idx)?;
            Some(IRPropertyKey::Identifier(name))
        } else if node.kind == SyntaxKind::StringLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(node) {
                Some(IRPropertyKey::StringLiteral(lit.text.clone()))
            } else {
                None
            }
        } else if node.kind == SyntaxKind::NumericLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(node) {
                Some(IRPropertyKey::NumericLiteral(lit.text.clone()))
            } else {
                None
            }
        } else if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            if let Some(computed) = self.arena.get_computed_property(node) {
                Some(IRPropertyKey::Computed(Box::new(
                    self.convert_expression(computed.expression),
                )))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn convert_function_expression(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // FunctionExpression uses FunctionData
        if let Some(func) = self.arena.get_function(node) {
            let name = if func.name.is_none() {
                None
            } else {
                get_identifier_text(self.arena, func.name)
            };
            let params = self.convert_parameters(&func.parameters);
            // Capture body source range for single-line detection
            let body_source_range = if !func.body.is_none() {
                self.arena
                    .get(func.body)
                    .map(|body_node| (body_node.pos as u32, body_node.end as u32))
            } else {
                None
            };
            let body = if func.body.is_none() {
                vec![]
            } else if let Some(body_node) = self.arena.get(func.body)
                && let Some(block) = self.arena.get_block(body_node)
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
            IRNode::FunctionExpr {
                name,
                parameters: params,
                body,
                is_expression_body: false,
                body_source_range,
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_arrow_function(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();

        // ArrowFunction uses FunctionData (has equals_greater_than_token set)
        if let Some(arrow) = self.arena.get_function(node) {
            // First check if there's a directive from LoweringPass
            let (captures_this, class_alias) = if let Some(ref transforms) = self.transforms {
                if let Some(crate::transform_context::TransformDirective::ES5ArrowFunction {
                    captures_this,
                    class_alias,
                    ..
                }) = transforms.get(idx)
                {
                    (*captures_this, class_alias.as_ref().map(|s| s.to_string()))
                } else {
                    // No directive, fall back to local analysis
                    (contains_this_reference(self.arena, idx), None)
                }
            } else {
                // No transforms available, fall back to local analysis
                (contains_this_reference(self.arena, idx), None)
            };

            // Save previous state and set captured flag if needed
            let prev_captured = self.this_captured.get();
            let prev_alias = self.current_class_alias.take();

            if captures_this {
                self.this_captured.set(true);
            }
            // Set the class_alias so `this` references in the body get converted
            self.current_class_alias.set(class_alias.clone());

            let params = self.convert_parameters(&arrow.parameters);
            let (body, is_expression_body) = if let Some(body_node) = self.arena.get(arrow.body) {
                if let Some(block) = self.arena.get_block(body_node) {
                    let stmts: Vec<IRNode> = block
                        .statements
                        .nodes
                        .iter()
                        .map(|&s| self.convert_statement(s))
                        .collect();
                    (stmts, false)
                } else {
                    // Expression body
                    let expr = self.convert_expression(arrow.body);
                    (vec![IRNode::ReturnStatement(Some(Box::new(expr)))], true)
                }
            } else {
                (vec![], false)
            };

            // Restore previous state
            self.this_captured.set(prev_captured);
            self.current_class_alias.set(prev_alias);

            // Arrow functions become regular functions in ES5
            let func_expr = IRNode::FunctionExpr {
                name: None,
                parameters: params,
                body,
                is_expression_body,
                body_source_range: None,
            };

            // Handle this capture:
            // - If class_alias is Some (static method), no IIFE wrapper needed - the alias is provided by outer scope
            // - If captures_this but no class_alias (regular method), wrap in IIFE
            if captures_this && class_alias.is_none() {
                IRNode::CallExpr {
                    callee: Box::new(IRNode::FunctionExpr {
                        name: None,
                        parameters: vec![IRParam {
                            name: "_this".to_string(),
                            rest: false,
                            default_value: None,
                        }],
                        body: vec![IRNode::ReturnStatement(Some(Box::new(func_expr)))],
                        is_expression_body: false,
                        body_source_range: None,
                    }),
                    arguments: vec![IRNode::This {
                        captured: prev_captured,
                    }],
                }
            } else {
                // Either doesn't capture this, or has class_alias (static method)
                func_expr
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_parameters(&self, params: &NodeList) -> Vec<IRParam> {
        params
            .nodes
            .iter()
            .filter_map(|&p| {
                let node = self.arena.get(p)?;
                let param = self.arena.get_parameter(node)?;
                let name = get_identifier_text(self.arena, param.name)?;
                let rest = param.dot_dot_dot_token;
                // Convert default value if present
                let default_value = if !param.initializer.is_none() {
                    Some(Box::new(self.convert_expression(param.initializer)))
                } else {
                    None
                };
                Some(IRParam {
                    name,
                    rest,
                    default_value,
                })
            })
            .collect()
    }

    fn convert_spread_element(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // SpreadElement uses SpreadData
        if let Some(spread) = self.arena.get_spread(node) {
            IRNode::SpreadElement(Box::new(self.convert_expression(spread.expression)))
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_template_literal(&self, idx: NodeIndex) -> IRNode {
        // Template literals need string concatenation in ES5
        // For now, use ASTRef as a fallback
        IRNode::ASTRef(idx)
    }

    fn convert_await_expression(&self, idx: NodeIndex) -> IRNode {
        // Await expressions are handled by the async transform
        IRNode::ASTRef(idx)
    }

    fn convert_type_assertion(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // Both TYPE_ASSERTION and AS_EXPRESSION use TypeAssertionData
        if let Some(assertion) = self.arena.get_type_assertion(node) {
            self.convert_expression(assertion.expression)
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_non_null(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // NON_NULL_EXPRESSION uses UnaryExpressionData
        if let Some(unary) = self.arena.get_unary_expr_ex(node) {
            self.convert_expression(unary.expression)
        } else {
            IRNode::ASTRef(idx)
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ParserState;
    use crate::transforms::ir_printer::IRPrinter;

    fn transform_class(source: &str) -> Option<String> {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let root_node = parser.arena.get(root)?;
        let source_file = parser.arena.get_source_file(root_node)?;

        // Find the class declaration
        for &stmt_idx in &source_file.statements.nodes {
            if let Some(node) = parser.arena.get(stmt_idx)
                && node.kind == syntax_kind_ext::CLASS_DECLARATION
            {
                let mut transformer = ES5ClassTransformer::new(&parser.arena);
                if let Some(ir) = transformer.transform_class_to_ir(stmt_idx) {
                    let mut printer = IRPrinter::with_arena(&parser.arena);
                    printer.set_source_text(source);
                    return Some(printer.emit(&ir).to_string());
                }
            }
        }

        None
    }

    #[test]
    fn test_simple_class() {
        let source = r#"class Point {
            x: number;
            y: number;
            constructor(x: number, y: number) {
                this.x = x;
                this.y = y;
            }
        }"#;

        let output = transform_class(source);
        assert!(output.is_some());
        let output = output.unwrap();

        assert!(output.contains("var Point = /** @class */ (function ()"));
        assert!(output.contains("function Point(x, y)"));
        assert!(output.contains("return Point;"));
    }

    #[test]
    fn test_class_with_extends() {
        let source = r#"class Dog extends Animal {
            constructor(name: string) {
                super(name);
            }
        }"#;

        let output = transform_class(source);
        assert!(output.is_some(), "Transform should produce output");
        let output = output.unwrap();

        assert!(
            output.contains("(function (_super)"),
            "Should have _super parameter: {}",
            output
        );
        assert!(
            output.contains("__extends(Dog, _super)"),
            "Should have extends helper: {}",
            output
        );
        assert!(
            output.contains("_super.call(this"),
            "Should have super.call pattern: {}",
            output
        );
    }

    #[test]
    fn test_class_with_method() {
        let source = r#"class Greeter {
            greet() {
                console.log("Hello");
            }
        }"#;

        let output = transform_class(source);
        assert!(output.is_some());
        let output = output.unwrap();

        assert!(output.contains("Greeter.prototype.greet = function ()"));
    }

    #[test]
    fn test_class_with_static_method() {
        let source = r#"class Counter {
            static count() {
                return 0;
            }
        }"#;

        let output = transform_class(source);
        assert!(output.is_some());
        let output = output.unwrap();

        assert!(output.contains("Counter.count = function ()"));
    }

    #[test]
    fn test_class_with_private_field() {
        let source = r#"class Container {
            #value = 42;
        }"#;

        let output = transform_class(source);
        assert!(output.is_some());
        let output = output.unwrap();

        assert!(output.contains("var _Container_value"));
        assert!(output.contains("_Container_value.set(this, void 0)"));
        assert!(output.contains("_Container_value = new WeakMap()"));
    }

    #[test]
    fn test_class_with_parameter_property() {
        let source = r#"class Point {
            constructor(public x: number, public y: number) {}
        }"#;

        let output = transform_class(source);
        assert!(output.is_some());
        let output = output.unwrap();

        assert!(output.contains("this.x = x"));
        assert!(output.contains("this.y = y"));
    }

    #[test]
    fn test_derived_class_default_constructor() {
        let source = r#"class Child extends Parent {
        }"#;

        let output = transform_class(source);
        assert!(output.is_some());
        let output = output.unwrap();

        assert!(output.contains("__extends(Child, _super)"));
        assert!(
            output.contains("_super !== null && _super.apply(this, arguments) || this")
                || output.contains("_super.apply(this, arguments)")
        );
    }

    #[test]
    fn test_class_with_instance_property() {
        let source = r#"class Counter {
            count = 0;
        }"#;

        let output = transform_class(source);
        assert!(output.is_some());
        let output = output.unwrap();

        assert!(output.contains("this.count ="));
    }

    #[test]
    fn test_declare_class_ignored() {
        let source = r#"declare class Foo {
            bar(): void;
        }"#;

        let output = transform_class(source);
        assert!(output.is_none());
    }

    #[test]
    fn test_accessor_pair_combined() {
        let source = r#"class Person {
            _name: string = "";
            get name() { return this._name; }
            set name(value: string) { this._name = value; }
        }"#;

        let output = transform_class(source);
        assert!(output.is_some());
        let output = output.unwrap();

        // Should have single Object.defineProperty call with both get and set
        assert!(output.contains("Object.defineProperty"));
        assert!(output.contains("get:"));
        assert!(output.contains("set:"));
        assert!(output.contains("enumerable: false"));
        assert!(output.contains("configurable: true"));
    }

    #[test]
    fn test_static_accessor_combined() {
        let source = r#"class Config {
            static _instance: Config | null = null;
            static get instance() { return Config._instance; }
            static set instance(value: Config) { Config._instance = value; }
        }"#;

        let output = transform_class(source);
        assert!(output.is_some());
        let output = output.unwrap();

        // Should have Object.defineProperty on class directly (not prototype)
        assert!(output.contains("Object.defineProperty(Config,"));
        assert!(output.contains("get:"));
        assert!(output.contains("set:"));
    }

    #[test]
    fn test_async_method() {
        let source = r#"class Fetcher {
            async fetch() {
                return await Promise.resolve(42);
            }
        }"#;

        let output = transform_class(source);
        assert!(output.is_some());
        let output = output.unwrap();

        // Async method should have __awaiter wrapper
        assert!(output.contains("__awaiter"));
    }

    #[test]
    fn test_static_async_method() {
        let source = r#"class API {
            static async request() {
                return await fetch("/api");
            }
        }"#;

        let output = transform_class(source);
        assert!(output.is_some());
        let output = output.unwrap();

        // Static async method should have __awaiter wrapper
        assert!(output.contains("API.request = function ()"));
        assert!(output.contains("__awaiter"));
    }

    #[test]
    fn test_computed_method_name() {
        let source = r#"class Container {
            [Symbol.iterator]() {
                return this;
            }
        }"#;

        let output = transform_class(source);
        assert!(output.is_some());
        let output = output.unwrap();

        // Computed method name should use bracket notation
        assert!(output.contains("Container.prototype[Symbol.iterator]"));
    }

    #[test]
    fn test_getter_only() {
        let source = r#"class ReadOnly {
            get value() { return 42; }
        }"#;

        let output = transform_class(source);
        assert!(output.is_some());
        let output = output.unwrap();

        // Should have DefineProperty with only get
        assert!(output.contains("Object.defineProperty"));
        assert!(output.contains("get:"));
        // Should still have enumerable and configurable
        assert!(output.contains("enumerable: false"));
        assert!(output.contains("configurable: true"));
    }

    #[test]
    fn test_setter_only() {
        let source = r#"class WriteOnly {
            set value(v: number) { console.log(v); }
        }"#;

        let output = transform_class(source);
        assert!(output.is_some());
        let output = output.unwrap();

        // Should have DefineProperty with only set
        assert!(output.contains("Object.defineProperty"));
        assert!(output.contains("set:"));
    }

    #[test]
    fn test_static_block() {
        let source = r#"class Initializer {
            static value: number;
            static {
                Initializer.value = 42;
            }
        }"#;

        let output = transform_class(source);
        assert!(output.is_some());
        let output = output.unwrap();

        // Static block content should be emitted
        assert!(output.contains("Initializer.value = 42"));
    }

    #[test]
    fn test_string_method_name() {
        let source = r#"class StringMethods {
            "my-method"() {
                return 1;
            }
        }"#;

        let output = transform_class(source);
        assert!(output.is_some());
        let output = output.unwrap();

        // String literal method name should use bracket notation
        assert!(output.contains("StringMethods.prototype[\"my-method\"]"));
    }

    #[test]
    fn test_numeric_method_name() {
        let source = r#"class NumericMethods {
            42() {
                return "answer";
            }
        }"#;

        let output = transform_class(source);
        assert!(output.is_some());
        let output = output.unwrap();

        // Numeric literal method name should use bracket notation
        assert!(output.contains("NumericMethods.prototype[42]"));
    }
}
