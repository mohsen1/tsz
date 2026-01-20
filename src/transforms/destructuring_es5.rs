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

use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::parser::thin_node::ThinNodeArena;
use crate::scanner::SyntaxKind;
use crate::transforms::ir::*;

/// ES5 Destructuring Transformer - produces IR nodes for destructuring lowering
pub struct ES5DestructuringTransformer<'a> {
    arena: &'a ThinNodeArena,
    /// Counter for temporary variable names (_a, _b, etc.)
    temp_var_counter: u32,
}

impl<'a> ES5DestructuringTransformer<'a> {
    pub fn new(arena: &'a ThinNodeArena) -> Self {
        Self {
            arena,
            temp_var_counter: 0,
        }
    }

    /// Reset temporary variable counter
    pub fn reset(&mut self) {
        self.temp_var_counter = 0;
    }

    /// Get next temporary variable name (_a, _b, _c, ...)
    fn next_temp_var(&mut self) -> String {
        let name = format!("_{}", (b'a' + (self.temp_var_counter % 26) as u8) as char);
        self.temp_var_counter += 1;
        name
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
    /// Returns an expression that performs the destructuring and returns the RHS value
    pub fn transform_destructuring_assignment(
        &mut self,
        pattern_idx: NodeIndex,
        value_idx: NodeIndex,
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

        // Return the temp (so the assignment expression has the correct value)
        exprs.push(IRNode::id(&temp_var));

        if exprs.len() == 1 {
            exprs.pop().unwrap()
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
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.emit_array_destructuring_expr(source, pattern_idx, result);
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
            if element_node.kind == syntax_kind_ext::BINDING_ELEMENT {
                if let Some(binding_elem) = self.arena.get_binding_element(element_node) {
                    let element_source = format!("{}[{}]", source, index);

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
                    if let Some(name_node) = self.arena.get(binding_elem.name) {
                        if name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                            || name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        {
                            // Nested pattern - create temp and recurse
                            let nested_temp = self.next_temp_var();
                            let access =
                                IRNode::elem(IRNode::id(source), IRNode::number(index.to_string()));
                            result.push(IRNode::var_decl(&nested_temp, Some(access)));
                            self.emit_destructuring_pattern(
                                &nested_temp,
                                binding_elem.name,
                                result,
                            );
                            continue;
                        }
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

            if element_node.kind == syntax_kind_ext::BINDING_ELEMENT {
                if let Some(binding_elem) = self.arena.get_binding_element(element_node) {
                    // Check for rest pattern
                    if binding_elem.dot_dot_dot_token {
                        // Rest: ...others -> others = __rest(source, ["a", "b", ...])
                        let name = self.get_identifier_text(binding_elem.name);
                        if !name.is_empty() {
                            let excluded_array: Vec<IRNode> =
                                rest_excluded.iter().map(|s| IRNode::string(s)).collect();
                            let rest_call = IRNode::call(
                                IRNode::id("__rest"),
                                vec![IRNode::id(source), IRNode::ArrayLiteral(excluded_array)],
                            );
                            result.push(IRNode::var_decl(&name, Some(rest_call)));
                        }
                        continue;
                    }

                    // Get property name
                    let prop_name = if binding_elem.property_name.is_none() {
                        self.get_identifier_text(binding_elem.name)
                    } else {
                        self.get_property_name_text(binding_elem.property_name)
                    };

                    // Track for rest pattern
                    if !prop_name.is_empty() {
                        rest_excluded.push(prop_name.clone());
                    }

                    // Check for nested pattern
                    if let Some(name_node) = self.arena.get(binding_elem.name) {
                        if name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                            || name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        {
                            // Nested pattern - create temp and recurse
                            let nested_temp = self.next_temp_var();
                            let access = IRNode::prop(IRNode::id(source), &prop_name);
                            result.push(IRNode::var_decl(&nested_temp, Some(access)));
                            self.emit_destructuring_pattern(
                                &nested_temp,
                                binding_elem.name,
                                result,
                            );
                            continue;
                        }
                    }

                    // Get binding name (might differ from property name with renaming)
                    let binding_name = self.get_identifier_text(binding_elem.name);
                    if binding_name.is_empty() {
                        continue;
                    }

                    let access = IRNode::prop(IRNode::id(source), &prop_name);

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

        let Some(literal) = self.arena.get_literal_expr(pattern_node) else {
            return;
        };

        for (index, &element_idx) in literal.elements.nodes.iter().enumerate() {
            if element_idx.is_none() {
                continue;
            }

            let Some(element_node) = self.arena.get(element_idx) else {
                continue;
            };

            // Handle spread element
            if element_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                if let Some(spread) = self.arena.get_unary_expr_ex(element_node) {
                    if let Some(target) = self.transform_expression(spread.expression) {
                        let slice_call = IRNode::call(
                            IRNode::prop(IRNode::id(source), "slice"),
                            vec![IRNode::number(index.to_string())],
                        );
                        result.push(IRNode::assign(target, slice_call));
                    }
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

        let Some(literal) = self.arena.get_literal_expr(pattern_node) else {
            return;
        };

        let mut rest_excluded: Vec<String> = Vec::new();

        for &element_idx in &literal.elements.nodes {
            let Some(element_node) = self.arena.get(element_idx) else {
                continue;
            };

            match element_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    if let Some(prop) = self.arena.get_property_assignment(element_node) {
                        let prop_name = self.get_property_name_text(prop.name);
                        rest_excluded.push(prop_name.clone());

                        // Check for nested destructuring
                        if let Some(init_node) = self.arena.get(prop.initializer) {
                            if init_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                || init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                            {
                                let nested_temp = self.next_temp_var();
                                let access = IRNode::prop(IRNode::id(source), &prop_name);
                                result.push(IRNode::assign(IRNode::id(&nested_temp), access));
                                self.emit_destructuring_assignments(
                                    &nested_temp,
                                    prop.initializer,
                                    result,
                                );
                                continue;
                            }
                        }

                        if let Some(target) = self.transform_expression(prop.initializer) {
                            let access = IRNode::prop(IRNode::id(source), &prop_name);
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
                    if let Some(spread) = self.arena.get_unary_expr_ex(element_node) {
                        if let Some(target) = self.transform_expression(spread.expression) {
                            let excluded_array: Vec<IRNode> =
                                rest_excluded.iter().map(|s| IRNode::string(s)).collect();
                            let rest_call = IRNode::call(
                                IRNode::id("__rest"),
                                vec![IRNode::id(source), IRNode::ArrayLiteral(excluded_array)],
                            );
                            result.push(IRNode::assign(target, rest_call));
                        }
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
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
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
            _ => Some(IRNode::ASTRef(idx)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thin_parser::ThinParserState;
    use crate::transforms::ir_printer::IRPrinter;

    fn transform_destructuring(source: &str) -> String {
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let Some(root_node) = parser.arena.get(root) else {
            return String::new();
        };
        let Some(source_file) = parser.arena.get_source_file(root_node) else {
            return String::new();
        };

        // Get first statement (variable statement)
        let Some(&stmt_idx) = source_file.statements.nodes.first() else {
            return String::new();
        };
        let Some(stmt_node) = parser.arena.get(stmt_idx) else {
            return String::new();
        };

        if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
            return String::new();
        }

        let Some(var_data) = parser.arena.get_variable(stmt_node) else {
            return String::new();
        };

        // Get first declaration
        let Some(&decl_idx) = var_data.declarations.nodes.first() else {
            return String::new();
        };
        let Some(decl_node) = parser.arena.get(decl_idx) else {
            return String::new();
        };
        let Some(decl) = parser.arena.get_variable_declaration(decl_node) else {
            return String::new();
        };

        let mut transformer = ES5DestructuringTransformer::new(&parser.arena);
        let nodes = transformer.transform_destructuring_declaration(decl.name, decl.initializer);

        // Print the IR nodes
        let mut output = String::new();
        for (i, node) in nodes.iter().enumerate() {
            if i > 0 {
                output.push_str("\n");
            }
            let mut printer = IRPrinter::with_arena(&parser.arena);
            printer.emit(&node);
            output.push_str(printer.get_output());
            output.push(';');
        }

        output
    }

    #[test]
    fn test_transformer_creation() {
        let arena = ThinNodeArena::new();
        let transformer = ES5DestructuringTransformer::new(&arena);
        assert!(!transformer.is_destructuring_pattern(NodeIndex::NONE));
        assert!(!transformer.is_destructuring_assignment(NodeIndex::NONE));
    }

    #[test]
    fn test_temp_var_generation() {
        let arena = ThinNodeArena::new();
        let mut transformer = ES5DestructuringTransformer::new(&arena);

        // Test temp var name generation
        assert_eq!(transformer.next_temp_var(), "_a");
        assert_eq!(transformer.next_temp_var(), "_b");
        assert_eq!(transformer.next_temp_var(), "_c");

        // Reset and verify it starts over
        transformer.reset();
        assert_eq!(transformer.next_temp_var(), "_a");
    }

    #[test]
    fn test_ir_node_generation() {
        use crate::transforms::ir::IRNode;

        // Test that we can build the expected IR structure
        let temp_var = "_a";
        let ir = IRNode::var_decl(temp_var, Some(IRNode::id("arr")));

        let mut printer = IRPrinter::new();
        printer.emit(&ir);
        let output = printer.get_output();

        assert!(output.contains("var _a"));
        assert!(output.contains("arr"));
    }
}
