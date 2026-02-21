//! Destructuring ES5 Transform
//!
//! Transforms ES6 destructuring patterns to ES5-compatible assignments.
//!
//! ## Array Destructuring
//! ```typescript
//! const [a, b, c] = arr;
//! ```
//! Becomes:
//! ```javascript
//! var _a = arr, a = _a[0], b = _a[1], c = _a[2];
//! ```
//!
//! ## Object Destructuring
//! ```typescript
//! const { x, y: renamed, z = 10 } = obj;
//! ```
//! Becomes:
//! ```javascript
//! var _a = obj, x = _a.x, renamed = _a.y, z = _a.z !== void 0 ? _a.z : 10;
//! ```
//!
//! ## Nested Destructuring
//! ```typescript
//! const { a: { b } } = obj;
//! ```
//! Becomes:
//! ```javascript
//! var _a = obj, _b = _a.a, b = _b.b;
//! ```
//!
//! ## Rest Patterns
//! ```typescript
//! const [first, ...rest] = arr;
//! const { a, ...others } = obj;
//! ```
//! Becomes:
//! ```javascript
//! var _a = arr, first = _a[0], rest = _a.slice(1);
//! var _a = obj, a = _a.a, others = __rest(_a, ["a"]);
//! ```

use crate::transforms::ir::IRNode;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// ES5 Destructuring Transformer - produces IR nodes for destructuring lowering
pub struct ES5DestructuringTransformer<'a> {
    arena: &'a NodeArena,
    /// Counter for temporary variable names (_a, _b, etc.)
    temp_var_counter: u32,
    /// Whether `this` references should lower to `_this`.
    capture_this: bool,
}

impl<'a> ES5DestructuringTransformer<'a> {
    pub const fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            temp_var_counter: 0,
            capture_this: false,
        }
    }

    /// Reset temporary variable counter
    pub const fn reset(&mut self) {
        self.temp_var_counter = 0;
    }

    /// Set the temp variable counter start value.
    pub const fn with_temp_counter(mut self, start: u32) -> Self {
        self.temp_var_counter = start;
        self
    }

    /// Set whether `this` references should lower to `_this`.
    pub const fn with_this_captured(mut self, capture_this: bool) -> Self {
        self.capture_this = capture_this;
        self
    }

    /// Get next temporary variable name (_a, _b, _c, ...)
    fn next_temp_var(&mut self) -> String {
        let name = format!("_{}", (b'a' + (self.temp_var_counter % 26) as u8) as char);
        self.temp_var_counter += 1;
        name
    }

    /// Get the current temp variable counter value.
    pub const fn temp_var_counter(&self) -> u32 {
        self.temp_var_counter
    }

    /// Transform a destructuring variable declaration to IR
    /// Returns a list of var declarations
    pub fn transform_destructuring_declaration(
        &mut self,
        pattern_idx: NodeIndex,
        initializer_idx: NodeIndex,
    ) -> Vec<IRNode> {
        let mut result = Vec::new();

        // Create temp var for the initializer
        let temp_var = self.next_temp_var();

        // First declaration: _a = initializer
        if let Some(init_expr) = self.transform_expression(initializer_idx) {
            result.push(IRNode::var_decl(&temp_var, Some(init_expr)));
        } else {
            result.push(IRNode::var_decl(&temp_var, Some(IRNode::Undefined)));
        }

        // Generate destructuring assignments
        self.emit_destructuring_pattern(&temp_var, pattern_idx, &mut result);

        result
    }

    /// Transform a destructuring assignment expression to IR
    /// Returns an expression that performs the destructuring.
    pub fn transform_destructuring_assignment(
        &mut self,
        pattern_idx: NodeIndex,
        value_idx: NodeIndex,
        keep_result: bool,
    ) -> IRNode {
        let temp_var = self.next_temp_var();

        // Build: (_temp = value, target1 = _temp.prop1, target2 = _temp.prop2, ..., _temp)
        let mut exprs = Vec::new();

        // _temp = value
        if let Some(value_expr) = self.transform_expression(value_idx) {
            exprs.push(IRNode::assign(IRNode::id(&temp_var), value_expr));
        }

        // Generate destructuring assignments as expressions
        self.emit_destructuring_assignments(&temp_var, pattern_idx, &mut exprs);

        if keep_result {
            // Preserve expression value when needed by returning the RHS temp.
            exprs.push(IRNode::id(&temp_var));
        }

        if exprs.len() == 1 {
            exprs
                .pop()
                .expect("exprs has exactly 1 element, checked above")
        } else {
            IRNode::CommaExpr(exprs)
        }
    }

    /// Emit destructuring pattern as variable declarations
    fn emit_destructuring_pattern(
        &mut self,
        source: &str,
        pattern_idx: NodeIndex,
        result: &mut Vec<IRNode>,
    ) {
        let Some(node) = self.arena.get(pattern_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                self.emit_array_destructuring(source, pattern_idx, result);
            }
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                self.emit_object_destructuring(source, pattern_idx, result);
            }
            _ => {}
        }
    }

    /// Emit destructuring as expressions (for assignment expressions)
    fn emit_destructuring_assignments(
        &mut self,
        source: &str,
        pattern_idx: NodeIndex,
        result: &mut Vec<IRNode>,
    ) {
        let Some(node) = self.arena.get(pattern_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                self.emit_array_destructuring_expr(source, pattern_idx, result);
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.emit_array_destructuring_expr(source, pattern_idx, result);
            }
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                self.emit_object_destructuring_expr(source, pattern_idx, result);
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.emit_object_destructuring_expr(source, pattern_idx, result);
            }
            _ => {}
        }
    }

    /// Emit array destructuring pattern as variable declarations
    fn emit_array_destructuring(
        &mut self,
        source: &str,
        pattern_idx: NodeIndex,
        result: &mut Vec<IRNode>,
    ) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };

        let Some(binding) = self.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        for (index, &element_idx) in binding.elements.nodes.iter().enumerate() {
            let Some(element_node) = self.arena.get(element_idx) else {
                continue;
            };

            // Skip omitted elements (holes in array pattern)
            if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                continue;
            }

            // Handle binding element
            if element_node.kind == syntax_kind_ext::BINDING_ELEMENT
                && let Some(binding_elem) = self.arena.get_binding_element(element_node)
            {
                // Check for rest pattern
                if binding_elem.dot_dot_dot_token {
                    // Rest: ...rest -> rest = source.slice(index)
                    let name = self.get_identifier_text(binding_elem.name);
                    if !name.is_empty() {
                        let slice_call = IRNode::call(
                            IRNode::prop(IRNode::id(source), "slice"),
                            vec![IRNode::number(index.to_string())],
                        );
                        result.push(IRNode::var_decl(&name, Some(slice_call)));
                    }
                    continue;
                }

                // Check for nested pattern
                if let Some(name_node) = self.arena.get(binding_elem.name)
                    && (name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                        || name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN)
                {
                    // Nested pattern - create temp and recurse
                    let nested_temp = self.next_temp_var();
                    let access =
                        IRNode::elem(IRNode::id(source), IRNode::number(index.to_string()));
                    result.push(IRNode::var_decl(&nested_temp, Some(access)));
                    self.emit_destructuring_pattern(&nested_temp, binding_elem.name, result);
                    continue;
                }

                // Simple binding
                let name = self.get_identifier_text(binding_elem.name);
                if !name.is_empty() {
                    let access =
                        IRNode::elem(IRNode::id(source), IRNode::number(index.to_string()));

                    // Handle default value
                    let value = if binding_elem.initializer.is_none() {
                        access
                    } else if let Some(default_val) =
                        self.transform_expression(binding_elem.initializer)
                    {
                        // name = source[i] !== void 0 ? source[i] : default
                        IRNode::ConditionalExpr {
                            condition: Box::new(IRNode::binary(
                                access.clone(),
                                "!==",
                                IRNode::Undefined,
                            )),
                            when_true: Box::new(access),
                            when_false: Box::new(default_val),
                        }
                    } else {
                        access
                    };

                    result.push(IRNode::var_decl(&name, Some(value)));
                }
            }
        }
    }

    /// Emit object destructuring pattern as variable declarations
    fn emit_object_destructuring(
        &mut self,
        source: &str,
        pattern_idx: NodeIndex,
        result: &mut Vec<IRNode>,
    ) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };

        let Some(binding) = self.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        let mut rest_excluded: Vec<String> = Vec::new();

        for &element_idx in &binding.elements.nodes {
            let Some(element_node) = self.arena.get(element_idx) else {
                continue;
            };

            if element_node.kind == syntax_kind_ext::BINDING_ELEMENT
                && let Some(binding_elem) = self.arena.get_binding_element(element_node)
            {
                // Check for rest pattern
                if binding_elem.dot_dot_dot_token {
                    // Rest: ...others -> others = __rest(source, ["a", "b", ...])
                    let name = self.get_identifier_text(binding_elem.name);
                    if !name.is_empty() {
                        let excluded_array: Vec<IRNode> =
                            rest_excluded.iter().map(IRNode::string).collect();
                        let rest_call = IRNode::call(
                            IRNode::id("__rest"),
                            vec![IRNode::id(source), IRNode::ArrayLiteral(excluded_array)],
                        );
                        result.push(IRNode::var_decl(&name, Some(rest_call)));
                    }
                    continue;
                }

                // Check if property name is computed
                let (is_computed, computed_temp) = if binding_elem.property_name.is_some() {
                    if let Some(prop_name_node) = self.arena.get(binding_elem.property_name)
                        && prop_name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                        && let Some(computed) = self.arena.get_computed_property(prop_name_node)
                    {
                        // Save computed key to a temporary variable
                        let temp_var = self.next_temp_var();
                        if let Some(key_expr) = self.transform_expression(computed.expression) {
                            result.push(IRNode::var_decl(&temp_var, Some(key_expr)));
                            (true, Some(temp_var))
                        } else {
                            (false, None)
                        }
                    } else {
                        (false, None)
                    }
                } else {
                    (false, None)
                };

                // Get property name (for non-computed or for rest tracking)
                let prop_name = if binding_elem.property_name.is_none() {
                    self.get_identifier_text(binding_elem.name)
                } else if !is_computed {
                    self.get_property_name_text(binding_elem.property_name)
                } else {
                    // For computed properties, we don't have a static name
                    String::new()
                };

                // Track for rest pattern (only for non-computed properties)
                if !prop_name.is_empty() {
                    rest_excluded.push(prop_name.clone());
                }

                // Check for nested pattern
                if let Some(name_node) = self.arena.get(binding_elem.name)
                    && (name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                        || name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN)
                {
                    // Nested pattern - create temp and recurse
                    let nested_temp = self.next_temp_var();
                    let access = if is_computed {
                        if let Some(computed_temp) = computed_temp.as_ref() {
                            IRNode::elem(IRNode::id(source), IRNode::id(computed_temp))
                        } else {
                            IRNode::prop(IRNode::id(source), &prop_name)
                        }
                    } else {
                        IRNode::prop(IRNode::id(source), &prop_name)
                    };
                    result.push(IRNode::var_decl(&nested_temp, Some(access)));
                    self.emit_destructuring_pattern(&nested_temp, binding_elem.name, result);
                    continue;
                }

                // Get binding name (might differ from property name with renaming)
                let binding_name = self.get_identifier_text(binding_elem.name);
                if binding_name.is_empty() {
                    continue;
                }

                // Create property access (computed or regular)
                let access = if is_computed {
                    if let Some(computed_temp) = computed_temp.as_ref() {
                        IRNode::elem(IRNode::id(source), IRNode::id(computed_temp))
                    } else {
                        IRNode::prop(IRNode::id(source), &prop_name)
                    }
                } else {
                    IRNode::prop(IRNode::id(source), &prop_name)
                };

                // Handle default value
                let value = if binding_elem.initializer.is_none() {
                    access
                } else if let Some(default_val) =
                    self.transform_expression(binding_elem.initializer)
                {
                    // name = source.prop !== void 0 ? source.prop : default
                    IRNode::ConditionalExpr {
                        condition: Box::new(IRNode::binary(
                            access.clone(),
                            "!==",
                            IRNode::Undefined,
                        )),
                        when_true: Box::new(access),
                        when_false: Box::new(default_val),
                    }
                } else {
                    access
                };

                result.push(IRNode::var_decl(&binding_name, Some(value)));
            }
        }
    }

    /// Emit array destructuring in expression context
    fn emit_array_destructuring_expr(
        &mut self,
        source: &str,
        pattern_idx: NodeIndex,
        result: &mut Vec<IRNode>,
    ) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };

        let elements = if let Some(binding) = self.arena.get_binding_pattern(pattern_node) {
            Some(&binding.elements.nodes)
        } else if let Some(literal) = self.arena.get_literal_expr(pattern_node) {
            Some(&literal.elements.nodes)
        } else {
            return;
        };

        let Some(elements) = elements else {
            return;
        };

        for (index, &element_idx) in elements.iter().enumerate() {
            if element_idx.is_none() {
                continue;
            }

            let Some(element_node) = self.arena.get(element_idx) else {
                continue;
            };

            // Handle spread element
            if element_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                if let Some(spread) = self.arena.get_unary_expr_ex(element_node)
                    && let Some(target) = self.transform_expression(spread.expression)
                {
                    let slice_call = IRNode::call(
                        IRNode::prop(IRNode::id(source), "slice"),
                        vec![IRNode::number(index.to_string())],
                    );
                    result.push(IRNode::assign(target, slice_call));
                }
                continue;
            }

            // Check for nested destructuring
            if element_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || element_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            {
                let nested_temp = self.next_temp_var();
                let access = IRNode::elem(IRNode::id(source), IRNode::number(index.to_string()));
                result.push(IRNode::assign(IRNode::id(&nested_temp), access));
                self.emit_destructuring_assignments(&nested_temp, element_idx, result);
                continue;
            }

            // Simple element
            if let Some(target) = self.transform_expression(element_idx) {
                let access = IRNode::elem(IRNode::id(source), IRNode::number(index.to_string()));
                result.push(IRNode::assign(target, access));
            }
        }
    }

    /// Emit object destructuring in expression context
    fn emit_object_destructuring_expr(
        &mut self,
        source: &str,
        pattern_idx: NodeIndex,
        result: &mut Vec<IRNode>,
    ) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };

        let elements = if let Some(binding) = self.arena.get_binding_pattern(pattern_node) {
            Some(&binding.elements.nodes)
        } else if let Some(literal) = self.arena.get_literal_expr(pattern_node) {
            Some(&literal.elements.nodes)
        } else {
            return;
        };

        let Some(elements) = elements else {
            return;
        };

        let mut rest_excluded: Vec<String> = Vec::new();

        for &element_idx in elements {
            let Some(element_node) = self.arena.get(element_idx) else {
                continue;
            };

            match element_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    if let Some(prop) = self.arena.get_property_assignment(element_node) {
                        // Check if property name is computed
                        let (is_computed, computed_temp) = if let Some(prop_name_node) =
                            self.arena.get(prop.name)
                            && prop_name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                            && let Some(computed) = self.arena.get_computed_property(prop_name_node)
                        {
                            // Save computed key to a temporary variable
                            let temp_var = self.next_temp_var();
                            if let Some(key_expr) = self.transform_expression(computed.expression) {
                                result.push(IRNode::assign(IRNode::id(&temp_var), key_expr));
                                (true, Some(temp_var))
                            } else {
                                (false, None)
                            }
                        } else {
                            (false, None)
                        };

                        let prop_name = if !is_computed {
                            let name = self.get_property_name_text(prop.name);
                            rest_excluded.push(name.clone());
                            name
                        } else {
                            String::new()
                        };

                        // Check for nested destructuring
                        if let Some(init_node) = self.arena.get(prop.initializer)
                            && (init_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                                || init_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                || init_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                || init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
                        {
                            let nested_temp = self.next_temp_var();
                            let access = if is_computed {
                                if let Some(computed_temp) = computed_temp.as_ref() {
                                    IRNode::elem(IRNode::id(source), IRNode::id(computed_temp))
                                } else {
                                    IRNode::prop(IRNode::id(source), &prop_name)
                                }
                            } else if let Some(prop_name_node) = self.arena.get(prop.name)
                                && prop_name_node.kind == SyntaxKind::StringLiteral as u16
                                && let Some(str_lit) = self.arena.get_literal(prop_name_node)
                            {
                                IRNode::elem(IRNode::id(source), IRNode::string(&str_lit.text))
                            } else {
                                IRNode::prop(IRNode::id(source), &prop_name)
                            };
                            result.push(IRNode::assign(IRNode::id(&nested_temp), access));
                            self.emit_destructuring_assignments(
                                &nested_temp,
                                prop.initializer,
                                result,
                            );
                            continue;
                        }

                        if let Some(target) = self.transform_expression(prop.initializer) {
                            let access = if is_computed {
                                if let Some(computed_temp) = computed_temp.as_ref() {
                                    IRNode::elem(IRNode::id(source), IRNode::id(computed_temp))
                                } else {
                                    IRNode::prop(IRNode::id(source), &prop_name)
                                }
                            } else if let Some(prop_name_node) = self.arena.get(prop.name)
                                && prop_name_node.kind == SyntaxKind::StringLiteral as u16
                                && let Some(str_lit) = self.arena.get_literal(prop_name_node)
                            {
                                IRNode::elem(IRNode::id(source), IRNode::string(&str_lit.text))
                            } else {
                                IRNode::prop(IRNode::id(source), &prop_name)
                            };
                            result.push(IRNode::assign(target, access));
                        }
                    }
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    if let Some(prop) = self.arena.get_shorthand_property(element_node) {
                        let name = self.get_identifier_text(prop.name);
                        rest_excluded.push(name.clone());

                        let access = IRNode::prop(IRNode::id(source), &name);
                        result.push(IRNode::assign(IRNode::id(&name), access));
                    }
                }
                k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                    if let Some(spread) = self.arena.get_unary_expr_ex(element_node)
                        && let Some(target) = self.transform_expression(spread.expression)
                    {
                        let excluded_array: Vec<IRNode> =
                            rest_excluded.iter().map(IRNode::string).collect();
                        let rest_call = IRNode::call(
                            IRNode::id("__rest"),
                            vec![IRNode::id(source), IRNode::ArrayLiteral(excluded_array)],
                        );
                        result.push(IRNode::assign(target, rest_call));
                    }
                }
                _ => {}
            }
        }
    }

    /// Check if a pattern contains any destructuring
    pub fn is_destructuring_pattern(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        matches!(
            node.kind,
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN
                || k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        )
    }

    /// Check if an expression is a destructuring assignment
    pub fn is_destructuring_assignment(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }

        let Some(bin) = self.arena.get_binary_expr(node) else {
            return false;
        };

        if bin.operator_token != SyntaxKind::EqualsToken as u16 {
            return false;
        }

        // Check if LHS is array or object literal (destructuring pattern)
        let Some(left_node) = self.arena.get(bin.left) else {
            return false;
        };

        matches!(
            left_node.kind,
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN
                || k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        )
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

    fn get_property_name_text(&self, idx: NodeIndex) -> String {
        let Some(node) = self.arena.get(idx) else {
            return String::new();
        };

        if let Some(ident) = self.arena.get_identifier(node) {
            return ident.escaped_text.clone();
        }
        if let Some(lit) = self.arena.get_literal(node) {
            return lit.text.clone();
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
            k if k == SyntaxKind::ThisKeyword as u16 => {
                if self.capture_this {
                    Some(IRNode::id("_this"))
                } else {
                    Some(IRNode::this())
                }
            }
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
            _ => Some(IRNode::ASTRef(idx)),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/destructuring_es5.rs"]
mod tests;
