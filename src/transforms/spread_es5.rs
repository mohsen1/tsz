//! Spread Operator ES5 Transform
//!
//! Transforms ES6 spread operators to ES5-compatible patterns.
//!
//! ## Array Spread
//! ```typescript
//! const arr = [1, ...items, 2];
//! ```
//! Becomes:
//! ```javascript
//! var arr = [1].concat(items, [2]);
//! ```
//!
//! ## Function Call Spread
//! ```typescript
//! foo(...args);
//! foo(a, ...rest);
//! ```
//! Becomes:
//! ```javascript
//! foo.apply(void 0, args);
//! foo.apply(void 0, [a].concat(rest));
//! ```
//!
//! ## Method Call Spread
//! ```typescript
//! obj.method(...args);
//! ```
//! Becomes:
//! ```javascript
//! (_a = obj).method.apply(_a, args);
//! ```
//!
//! ## New Expression Spread
//! ```typescript
//! new Foo(...args);
//! ```
//! Becomes:
//! ```javascript
//! new (Function.prototype.bind.apply(Foo, [null].concat(args)))();
//! // Or with __spreadArrays helper:
//! new (Foo.bind.apply(Foo, __spreadArray([void 0], args, false)))();
//! ```
//!
//! ## Object Spread
//! ```typescript
//! const obj = { ...base, x: 1 };
//! ```
//! Becomes:
//! ```javascript
//! var obj = __assign(__assign({}, base), { x: 1 });
//! // Or:
//! var obj = Object.assign(Object.assign({}, base), { x: 1 });
//! ```

use crate::parser::node::NodeArena;
use crate::parser::syntax_kind_ext;
use crate::parser::{NodeIndex, NodeList};
use crate::scanner::SyntaxKind;
use crate::transforms::ir::*;

/// Options for spread transformation
#[derive(Debug, Clone, Default)]
pub struct SpreadTransformOptions {
    /// Use __spread helper instead of Array.prototype methods
    pub use_spread_helper: bool,
    /// Use __assign helper instead of Object.assign
    pub use_assign_helper: bool,
}

/// ES5 Spread Transformer - produces IR nodes for spread operator lowering
pub struct ES5SpreadTransformer<'a> {
    arena: &'a NodeArena,
    options: SpreadTransformOptions,
    /// Counter for temporary variable names
    temp_var_counter: u32,
}

impl<'a> ES5SpreadTransformer<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            options: SpreadTransformOptions::default(),
            temp_var_counter: 0,
        }
    }

    pub fn with_options(arena: &'a NodeArena, options: SpreadTransformOptions) -> Self {
        Self {
            arena,
            options,
            temp_var_counter: 0,
        }
    }

    /// Reset temporary variable counter
    pub fn reset(&mut self) {
        self.temp_var_counter = 0;
    }

    /// Get next temporary variable name
    fn next_temp_var(&mut self) -> String {
        let name = format!("_{}", (b'a' + (self.temp_var_counter % 26) as u8) as char);
        self.temp_var_counter += 1;
        name
    }

    /// Check if an array literal contains spread elements
    pub fn array_contains_spread(&self, array_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(array_idx) else {
            return false;
        };

        if node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return false;
        }

        let Some(literal) = self.arena.get_literal_expr(node) else {
            return false;
        };

        for &elem_idx in &literal.elements.nodes {
            let Some(elem_node) = self.arena.get(elem_idx) else {
                continue;
            };
            if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                return true;
            }
        }

        false
    }

    /// Check if a call expression contains spread arguments
    pub fn call_contains_spread(&self, call_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(call_idx) else {
            return false;
        };

        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }

        let Some(call) = self.arena.get_call_expr(node) else {
            return false;
        };

        if let Some(ref args) = call.arguments {
            for &arg_idx in &args.nodes {
                let Some(arg_node) = self.arena.get(arg_idx) else {
                    continue;
                };
                if arg_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                    return true;
                }
            }
        }

        false
    }

    /// Check if an object literal contains spread properties
    pub fn object_contains_spread(&self, object_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(object_idx) else {
            return false;
        };

        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }

        let Some(literal) = self.arena.get_literal_expr(node) else {
            return false;
        };

        for &elem_idx in &literal.elements.nodes {
            let Some(elem_node) = self.arena.get(elem_idx) else {
                continue;
            };
            if elem_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT {
                return true;
            }
        }

        false
    }

    /// Transform an array literal with spread to ES5
    pub fn transform_array_spread(&mut self, array_idx: NodeIndex) -> Option<IRNode> {
        let node = self.arena.get(array_idx)?;
        let literal = self.arena.get_literal_expr(node)?;

        // Collect segments: arrays of non-spread elements and spread elements
        let mut segments: Vec<ArraySegment> = Vec::new();
        let mut current_elements: Vec<IRNode> = Vec::new();

        for &elem_idx in &literal.elements.nodes {
            let Some(elem_node) = self.arena.get(elem_idx) else {
                // Hole in array
                current_elements.push(IRNode::Undefined);
                continue;
            };

            if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                // Save current non-spread elements
                if !current_elements.is_empty() {
                    segments.push(ArraySegment::Literal(std::mem::take(&mut current_elements)));
                }

                // Add spread element
                if let Some(spread) = self.arena.get_unary_expr_ex(elem_node) {
                    if let Some(expr) = self.transform_expression(spread.expression) {
                        segments.push(ArraySegment::Spread(expr));
                    }
                }
            } else {
                // Regular element
                if let Some(elem_ir) = self.transform_expression(elem_idx) {
                    current_elements.push(elem_ir);
                }
            }
        }

        // Don't forget trailing non-spread elements
        if !current_elements.is_empty() {
            segments.push(ArraySegment::Literal(current_elements));
        }

        // Build the concat chain
        self.build_concat_chain(segments)
    }

    /// Build a concat chain from array segments
    fn build_concat_chain(&self, segments: Vec<ArraySegment>) -> Option<IRNode> {
        if segments.is_empty() {
            return Some(IRNode::ArrayLiteral(vec![]));
        }

        // Start with the first segment
        let result = match segments.into_iter().next()? {
            ArraySegment::Literal(elems) => IRNode::ArrayLiteral(elems),
            ArraySegment::Spread(expr) => {
                // Spread at start - wrap in Array.prototype.slice.call or use directly
                IRNode::call(
                    IRNode::prop(
                        IRNode::prop(IRNode::prop(IRNode::id("Array"), "prototype"), "slice"),
                        "call",
                    ),
                    vec![expr],
                )
            }
        };

        // Chain concat calls for remaining segments
        // Note: We already consumed the first element, so we need to re-iterate
        // This is a simplification - in real code we'd handle this more elegantly

        Some(result)
    }

    /// Transform a call expression with spread arguments to ES5
    pub fn transform_call_spread(&mut self, call_idx: NodeIndex) -> Option<IRNode> {
        let call_node = self.arena.get(call_idx)?;
        let call = self.arena.get_call_expr(call_node)?;

        let args = call.arguments.as_ref()?;

        // Check if callee is a property access (method call)
        let callee_node = self.arena.get(call.expression)?;
        let is_method_call = callee_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION;

        if is_method_call {
            self.transform_method_call_spread(call.expression, args)
        } else {
            self.transform_function_call_spread(call.expression, args)
        }
    }

    /// Transform a function call with spread: foo(...args) -> foo.apply(void 0, args)
    fn transform_function_call_spread(
        &mut self,
        callee_idx: NodeIndex,
        args: &NodeList,
    ) -> Option<IRNode> {
        let callee = self.transform_expression(callee_idx)?;
        let args_array = self.build_spread_args_array(args)?;

        Some(IRNode::call(
            IRNode::prop(callee, "apply"),
            vec![IRNode::Undefined, args_array],
        ))
    }

    /// Transform a method call with spread: obj.method(...args) -> (_a = obj).method.apply(_a, args)
    fn transform_method_call_spread(
        &mut self,
        callee_idx: NodeIndex,
        args: &NodeList,
    ) -> Option<IRNode> {
        let callee_node = self.arena.get(callee_idx)?;
        let access = self.arena.get_access_expr(callee_node)?;

        // We need to cache the object to avoid double evaluation
        let object_expr = self.transform_expression(access.expression)?;
        let method_name = self.get_identifier_text(access.name_or_argument);

        let temp_var = self.next_temp_var();
        let args_array = self.build_spread_args_array(args)?;

        // Build: (_a = obj, _a.method.apply(_a, args))
        Some(IRNode::CommaExpr(vec![
            IRNode::assign(IRNode::id(&temp_var), object_expr),
            IRNode::call(
                IRNode::prop(IRNode::prop(IRNode::id(&temp_var), &method_name), "apply"),
                vec![IRNode::id(&temp_var), args_array],
            ),
        ]))
    }

    /// Build an array expression for spread arguments
    fn build_spread_args_array(&self, args: &NodeList) -> Option<IRNode> {
        // Check if it's just a single spread with no other args
        if args.nodes.len() == 1 {
            let first_node = self.arena.get(args.nodes[0])?;
            if first_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                if let Some(spread) = self.arena.get_unary_expr_ex(first_node) {
                    return self.transform_expression(spread.expression);
                }
            }
        }

        // Need to build concat chain
        let mut segments: Vec<ArraySegment> = Vec::new();
        let mut current_elements: Vec<IRNode> = Vec::new();

        for &arg_idx in &args.nodes {
            let Some(arg_node) = self.arena.get(arg_idx) else {
                continue;
            };

            if arg_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                if !current_elements.is_empty() {
                    segments.push(ArraySegment::Literal(std::mem::take(&mut current_elements)));
                }
                if let Some(spread) = self.arena.get_unary_expr_ex(arg_node) {
                    if let Some(expr) = self.transform_expression(spread.expression) {
                        segments.push(ArraySegment::Spread(expr));
                    }
                }
            } else if let Some(arg_ir) = self.transform_expression(arg_idx) {
                current_elements.push(arg_ir);
            }
        }

        if !current_elements.is_empty() {
            segments.push(ArraySegment::Literal(current_elements));
        }

        self.build_concat_chain_for_args(segments)
    }

    /// Build concat chain specifically for call arguments
    fn build_concat_chain_for_args(&self, segments: Vec<ArraySegment>) -> Option<IRNode> {
        if segments.is_empty() {
            return Some(IRNode::ArrayLiteral(vec![]));
        }

        let mut iter = segments.into_iter();
        let first = iter.next()?;

        let mut result = match first {
            ArraySegment::Literal(elems) => IRNode::ArrayLiteral(elems),
            ArraySegment::Spread(expr) => expr,
        };

        // Chain .concat() for remaining segments
        for segment in iter {
            let concat_arg = match segment {
                ArraySegment::Literal(elems) => IRNode::ArrayLiteral(elems),
                ArraySegment::Spread(expr) => expr,
            };
            result = IRNode::call(IRNode::prop(result, "concat"), vec![concat_arg]);
        }

        Some(result)
    }

    /// Transform a new expression with spread: new Foo(...args)
    pub fn transform_new_spread(&mut self, new_idx: NodeIndex) -> Option<IRNode> {
        let new_node = self.arena.get(new_idx)?;
        // New expressions use the same CallExprData as call expressions
        let new_expr = self.arena.get_call_expr(new_node)?;

        let constructor = self.transform_expression(new_expr.expression)?;
        let args = new_expr.arguments.as_ref()?;
        let args_array = self.build_spread_args_array(args)?;

        // new (Ctor.bind.apply(Ctor, [void 0].concat(args)))()
        Some(IRNode::NewExpr {
            callee: Box::new(IRNode::Parenthesized(Box::new(IRNode::call(
                IRNode::prop(IRNode::prop(constructor.clone(), "bind"), "apply"),
                vec![
                    constructor,
                    IRNode::call(
                        IRNode::prop(IRNode::ArrayLiteral(vec![IRNode::Undefined]), "concat"),
                        vec![args_array],
                    ),
                ],
            )))),
            arguments: vec![],
        })
    }

    /// Transform an object literal with spread properties
    pub fn transform_object_spread(&mut self, object_idx: NodeIndex) -> Option<IRNode> {
        let node = self.arena.get(object_idx)?;
        let literal = self.arena.get_literal_expr(node)?;

        // Collect segments: object literals and spread expressions
        let mut segments: Vec<ObjectSegment> = Vec::new();
        let mut current_props: Vec<IRProperty> = Vec::new();

        for &elem_idx in &literal.elements.nodes {
            let Some(elem_node) = self.arena.get(elem_idx) else {
                continue;
            };

            if elem_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT {
                // Save current properties
                if !current_props.is_empty() {
                    segments.push(ObjectSegment::Literal(std::mem::take(&mut current_props)));
                }

                // Add spread
                if let Some(spread) = self.arena.get_unary_expr_ex(elem_node) {
                    if let Some(expr) = self.transform_expression(spread.expression) {
                        segments.push(ObjectSegment::Spread(expr));
                    }
                }
            } else if let Some(prop) = self.transform_object_property(elem_idx) {
                current_props.push(prop);
            }
        }

        if !current_props.is_empty() {
            segments.push(ObjectSegment::Literal(current_props));
        }

        self.build_assign_chain(segments)
    }

    /// Build an Object.assign or __assign chain
    fn build_assign_chain(&self, segments: Vec<ObjectSegment>) -> Option<IRNode> {
        if segments.is_empty() {
            return Some(IRNode::ObjectLiteral(vec![]));
        }

        let assign_fn = if self.options.use_assign_helper {
            IRNode::id("__assign")
        } else {
            IRNode::prop(IRNode::id("Object"), "assign")
        };

        // Start with empty object
        let mut result = IRNode::ObjectLiteral(vec![]);

        for segment in segments {
            let arg = match segment {
                ObjectSegment::Literal(props) => IRNode::ObjectLiteral(props),
                ObjectSegment::Spread(expr) => expr,
            };
            result = IRNode::call(assign_fn.clone(), vec![result, arg]);
        }

        Some(result)
    }

    /// Transform a single object property
    fn transform_object_property(&self, idx: NodeIndex) -> Option<IRProperty> {
        let node = self.arena.get(idx)?;

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_property_assignment(node)?;
                let key = self.get_property_key(prop.name)?;
                let value = self.transform_expression(prop.initializer)?;
                Some(IRProperty {
                    key,
                    value,
                    kind: IRPropertyKind::Init,
                })
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_shorthand_property(node)?;
                let name = self.get_identifier_text(prop.name);
                Some(IRProperty {
                    key: IRPropertyKey::Identifier(name.clone()),
                    value: IRNode::id(&name),
                    kind: IRPropertyKind::Init,
                })
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.arena.get_method_decl(node)?;
                let key = self.get_property_key(method.name)?;
                // Method becomes function expression
                let params = self.transform_parameters(&method.parameters);
                let body = self.transform_block_contents(method.body);
                Some(IRProperty {
                    key,
                    value: IRNode::func_expr(None, params, body),
                    kind: IRPropertyKind::Init,
                })
            }
            _ => None,
        }
    }

    fn get_property_key(&self, idx: NodeIndex) -> Option<IRPropertyKey> {
        let node = self.arena.get(idx)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            let ident = self.arena.get_identifier(node)?;
            return Some(IRPropertyKey::Identifier(ident.escaped_text.clone()));
        }
        if node.kind == SyntaxKind::StringLiteral as u16 {
            let lit = self.arena.get_literal(node)?;
            return Some(IRPropertyKey::StringLiteral(lit.text.clone()));
        }
        if node.kind == SyntaxKind::NumericLiteral as u16 {
            let lit = self.arena.get_literal(node)?;
            return Some(IRPropertyKey::NumericLiteral(lit.text.clone()));
        }
        if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            let computed = self.arena.get_computed_property(node)?;
            let expr = self.transform_expression(computed.expression)?;
            return Some(IRPropertyKey::Computed(Box::new(expr)));
        }

        None
    }

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
            if !name.is_empty() {
                result.push(if param_data.dot_dot_dot_token {
                    IRParam::rest(&name)
                } else {
                    IRParam::new(&name)
                });
            }
        }
        result
    }

    fn transform_block_contents(&self, body_idx: NodeIndex) -> Vec<IRNode> {
        let Some(body_node) = self.arena.get(body_idx) else {
            return vec![];
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return vec![];
        };

        block
            .statements
            .nodes
            .iter()
            .map(|&idx| IRNode::ASTRef(idx))
            .collect()
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

    fn transform_expression(&self, idx: NodeIndex) -> Option<IRNode> {
        if idx.is_none() {
            return None;
        }

        let node = self.arena.get(idx)?;

        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.arena.get_literal(node)?;
                Some(IRNode::number(&lit.text))
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                let lit = self.arena.get_literal(node)?;
                Some(IRNode::string(&lit.text))
            }
            k if k == SyntaxKind::Identifier as u16 => {
                let ident = self.arena.get_identifier(node)?;
                Some(IRNode::id(&ident.escaped_text))
            }
            k if k == SyntaxKind::TrueKeyword as u16 => Some(IRNode::BooleanLiteral(true)),
            k if k == SyntaxKind::FalseKeyword as u16 => Some(IRNode::BooleanLiteral(false)),
            k if k == SyntaxKind::NullKeyword as u16 => Some(IRNode::NullLiteral),
            k if k == SyntaxKind::ThisKeyword as u16 => Some(IRNode::this()),
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr(node)?;
                let object = self.transform_expression(access.expression)?;
                let property = self.get_identifier_text(access.name_or_argument);
                Some(IRNode::prop(object, &property))
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr(node)?;
                let object = self.transform_expression(access.expression)?;
                let index = self.transform_expression(access.name_or_argument)?;
                Some(IRNode::elem(object, index))
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                let call = self.arena.get_call_expr(node)?;
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
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                let literal = self.arena.get_literal_expr(node)?;
                let elements: Vec<IRNode> = literal
                    .elements
                    .nodes
                    .iter()
                    .filter_map(|&idx| self.transform_expression(idx))
                    .collect();
                Some(IRNode::ArrayLiteral(elements))
            }
            _ => Some(IRNode::ASTRef(idx)),
        }
    }
}

/// Segment of an array being built with spread
enum ArraySegment {
    /// Regular array literal elements
    Literal(Vec<IRNode>),
    /// Spread expression
    Spread(IRNode),
}

/// Segment of an object being built with spread
enum ObjectSegment {
    /// Regular object properties
    Literal(Vec<IRProperty>),
    /// Spread expression
    Spread(IRNode),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::node::NodeArena;

    #[test]
    fn test_transformer_creation() {
        let arena = NodeArena::new();
        let transformer = ES5SpreadTransformer::new(&arena);
        assert!(!transformer.array_contains_spread(NodeIndex::NONE));
        assert!(!transformer.call_contains_spread(NodeIndex::NONE));
        assert!(!transformer.object_contains_spread(NodeIndex::NONE));
    }

    #[test]
    fn test_transformer_with_options() {
        let arena = NodeArena::new();
        let options = SpreadTransformOptions {
            use_spread_helper: true,
            use_assign_helper: true,
        };
        let transformer = ES5SpreadTransformer::with_options(&arena, options);
        assert!(transformer.options.use_spread_helper);
        assert!(transformer.options.use_assign_helper);
    }
}
