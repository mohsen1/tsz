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
//! ## Current Limitations (Migration Notes)
//!
//! This transformer produces the class IIFE structure but uses `IRNode::ASTRef` for:
//! - Method bodies
//! - Constructor body statements
//! - Property initializers
//! - Getter/setter bodies
//!
//! `ASTRef` nodes copy raw source text via `IRPrinter`, which means:
//! 1. Source text must be available during printing
//! 2. **No ES5 transformations are applied** to method body contents
//!
//! For a complete migration to replace `class_es5.rs`, this transformer would need to:
//! - Fully transform all expressions/statements into IR nodes (no ASTRef)
//! - Handle arrow function → regular function with `_this` capture
//! - Handle `super` property access and method calls
//! - Handle template literals → string concatenation
//! - Handle destructuring in parameters
//! - Handle for-of/for-in → ES5 loops
//! - Handle all other ES6+ → ES5 transformations
//!
//! The legacy `class_es5.rs` handles all these cases with its own expression emission.
//! Until this transformer is complete, `class_es5.rs` remains the primary implementation.

use crate::parser::node::NodeArena;
use crate::parser::syntax_kind_ext;
use crate::parser::{NodeIndex, NodeList};
use crate::scanner::SyntaxKind;
use crate::transforms::ir::*;
use crate::transforms::private_fields_es5::{
    PrivateAccessorInfo, PrivateFieldInfo, collect_private_accessors, collect_private_fields,
    is_private_identifier,
};
use std::collections::HashMap;

/// Context for ES5 class transformation
pub struct ES5ClassTransformer<'a> {
    arena: &'a NodeArena,
    class_name: String,
    has_extends: bool,
    private_fields: Vec<PrivateFieldInfo>,
    private_accessors: Vec<PrivateAccessorInfo>,
}

impl<'a> ES5ClassTransformer<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            class_name: String::new(),
            has_extends: false,
            private_fields: Vec::new(),
            private_accessors: Vec::new(),
        }
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
            body.push(IRNode::ASTRef(stmt_idx));
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
                body.push(IRNode::ASTRef(stmt_idx));
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
                body.push(IRNode::ASTRef(stmt_idx));
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
                    args.push(IRNode::ASTRef(arg_idx));
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
                    value: Box::new(IRNode::ASTRef(field.initializer)),
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
                        body: vec![IRNode::ASTRef(getter_body)],
                        is_expression_body: false,
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
                        body: vec![IRNode::ASTRef(setter_body)],
                        is_expression_body: false,
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
            IRNode::ASTRef(prop_data.initializer),
        )))
    }

    /// Build property access node based on property name type
    fn build_property_access(&self, receiver: IRNode, name: PropertyNameIR) -> IRNode {
        match name {
            PropertyNameIR::Identifier(n) => IRNode::prop(receiver, n),
            PropertyNameIR::StringLiteral(s) => IRNode::elem(receiver, IRNode::string(s)),
            PropertyNameIR::NumericLiteral(n) => IRNode::elem(receiver, IRNode::number(n)),
            PropertyNameIR::Computed(expr_idx) => IRNode::elem(receiver, IRNode::ASTRef(expr_idx)),
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
            let ir_param = if is_rest {
                IRParam::rest(name)
            } else {
                IRParam::new(name)
            };

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
                // Return the expression as an AST reference
                return Some(IRNode::ASTRef(expr_data.expression));
            }
        }

        None
    }

    /// Emit prototype methods as IR
    fn emit_methods_ir(&self, body: &mut Vec<IRNode>, class_idx: NodeIndex) {
        let Some(class_node) = self.arena.get(class_idx) else {
            return;
        };
        let Some(class_data) = self.arena.get_class(class_node) else {
            return;
        };

        // First pass: collect accessors by name to combine getter/setter pairs
        let mut accessor_map: HashMap<String, (Option<NodeIndex>, Option<NodeIndex>)> =
            HashMap::new();

        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                || member_node.kind == syntax_kind_ext::SET_ACCESSOR
            {
                if let Some(accessor_data) = self.arena.get_accessor(member_node) {
                    // Skip static (handled in emit_static_members_ir)
                    if has_static_modifier(self.arena, &accessor_data.modifiers) {
                        continue;
                    }
                    // Skip abstract
                    if has_abstract_modifier(self.arena, &accessor_data.modifiers) {
                        continue;
                    }
                    // Skip private
                    if is_private_identifier(self.arena, accessor_data.name) {
                        continue;
                    }

                    let name =
                        get_identifier_text(self.arena, accessor_data.name).unwrap_or_default();
                    let entry = accessor_map.entry(name).or_insert((None, None));

                    if member_node.kind == syntax_kind_ext::GET_ACCESSOR {
                        entry.0 = Some(member_idx);
                    } else {
                        entry.1 = Some(member_idx);
                    }
                }
            }
        }

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

                let method_body = if is_async {
                    // Async method: wrap body in __awaiter call
                    vec![IRNode::AwaiterCall {
                        this_arg: Box::new(IRNode::this()),
                        generator_body: Box::new(IRNode::ASTRef(method_data.body)),
                    }]
                } else {
                    vec![IRNode::ASTRef(method_data.body)]
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
                vec![IRNode::ASTRef(accessor_data.body)]
            },
            is_expression_body: false,
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
                vec![IRNode::ASTRef(accessor_data.body)]
            },
            is_expression_body: false,
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
        let mut static_accessor_map: HashMap<String, (Option<NodeIndex>, Option<NodeIndex>)> =
            HashMap::new();

        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                || member_node.kind == syntax_kind_ext::SET_ACCESSOR
            {
                if let Some(accessor_data) = self.arena.get_accessor(member_node)
                    && has_static_modifier(self.arena, &accessor_data.modifiers)
                {
                    // Skip abstract
                    if has_abstract_modifier(self.arena, &accessor_data.modifiers) {
                        continue;
                    }
                    // Skip private
                    if is_private_identifier(self.arena, accessor_data.name) {
                        continue;
                    }

                    let name =
                        get_identifier_text(self.arena, accessor_data.name).unwrap_or_default();
                    let entry = static_accessor_map.entry(name).or_insert((None, None));

                    if member_node.kind == syntax_kind_ext::GET_ACCESSOR {
                        entry.0 = Some(member_idx);
                    } else {
                        entry.1 = Some(member_idx);
                    }
                }
            }
        }

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
                    // Async method: wrap body in __awaiter call
                    vec![IRNode::AwaiterCall {
                        this_arg: Box::new(IRNode::this()),
                        generator_body: Box::new(IRNode::ASTRef(method_data.body)),
                    }]
                } else {
                    vec![IRNode::ASTRef(method_data.body)]
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
                        PropertyNameIR::Computed(expr_idx) => {
                            IRNode::elem(IRNode::id(&self.class_name), IRNode::ASTRef(*expr_idx))
                        }
                    };

                    // ClassName.prop = value;
                    body.push(IRNode::expr_stmt(IRNode::assign(
                        target,
                        IRNode::ASTRef(prop_data.initializer),
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
                        .map(|&stmt_idx| IRNode::ASTRef(stmt_idx))
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
                return IRMethodName::Computed(Box::new(IRNode::ASTRef(computed.expression)));
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
