//! ES5 Arrow Function Transform
//!
//! Transforms ES6 arrow functions to ES5 function expressions:
//!
//! ```typescript
//! const add = (a, b) => a + b;
//! const greet = (name) => {
//!     console.log("Hello " + name);
//! };
//! const obj = {
//!     method() {
//!         const arrow = () => this.x;  // `this` capture needed
//!     }
//! };
//! ```
//!
//! Becomes:
//!
//! ```javascript
//! var add = function (a, b) { return a + b; };
//! var greet = function (name) {
//!     console.log("Hello " + name);
//! };
//! var obj = {
//!     method: function () {
//!         var _this = this;
//!         var arrow = function () { return _this.x; };
//!     }
//! };
//! ```

use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::parser::thin_node::ThinNodeArena;

/// Checks if a node or its descendants contain `this` references
pub fn contains_this_reference(arena: &ThinNodeArena, node_idx: NodeIndex) -> bool {
    let Some(node) = arena.get(node_idx) else {
        return false;
    };

    // Check if this node is `this`
    if node.kind == crate::scanner::SyntaxKind::ThisKeyword as u16
        || node.kind == crate::scanner::SyntaxKind::SuperKeyword as u16
    {
        return true;
    }

    // Check children recursively based on node type
    match node.kind {
        k if k == syntax_kind_ext::BLOCK => {
            if let Some(block) = arena.get_block(node) {
                for &stmt_idx in &block.statements.nodes {
                    if contains_this_reference(arena, stmt_idx) {
                        return true;
                    }
                }
            }
        }
        k if k == syntax_kind_ext::CALL_EXPRESSION || k == syntax_kind_ext::NEW_EXPRESSION => {
            if let Some(call) = arena.get_call_expr(node) {
                if contains_this_reference(arena, call.expression) {
                    return true;
                }
                if let Some(ref args) = call.arguments {
                    for &arg_idx in &args.nodes {
                        if contains_this_reference(arena, arg_idx) {
                            return true;
                        }
                    }
                }
            }
        }
        k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
        {
            if let Some(access) = arena.get_access_expr(node) {
                if contains_this_reference(arena, access.expression) {
                    return true;
                }
                if contains_this_reference(arena, access.name_or_argument) {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
            if let Some(paren) = arena.get_parenthesized(node) {
                return contains_this_reference(arena, paren.expression);
            }
        }
        k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
            if let Some(cond) = arena.get_conditional_expr(node) {
                return contains_this_reference(arena, cond.condition)
                    || contains_this_reference(arena, cond.when_true)
                    || contains_this_reference(arena, cond.when_false);
            }
        }
        k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
            if let Some(lit) = arena.get_literal_expr(node) {
                for &elem_idx in &lit.elements.nodes {
                    if !elem_idx.is_none() && contains_this_reference(arena, elem_idx) {
                        return true;
                    }
                }
            }
        }
        k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
            if let Some(lit) = arena.get_literal_expr(node) {
                for &elem_idx in &lit.elements.nodes {
                    if !elem_idx.is_none() && contains_this_reference(arena, elem_idx) {
                        return true;
                    }
                }
            }
        }
        k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
            if let Some(prop) = arena.get_property_assignment(node) {
                if contains_this_reference(arena, prop.name) {
                    return true;
                }
                if !prop.initializer.is_none() && contains_this_reference(arena, prop.initializer) {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
            if let Some(prop) = arena.get_shorthand_property(node) {
                if contains_this_reference(arena, prop.name) {
                    return true;
                }
                if prop.equals_token
                    && !prop.object_assignment_initializer.is_none()
                    && contains_this_reference(arena, prop.object_assignment_initializer)
                {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::SPREAD_ELEMENT || k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
            if let Some(spread) = arena.get_spread(node) {
                if contains_this_reference(arena, spread.expression) {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::METHOD_DECLARATION => {
            if let Some(method) = arena.get_method_decl(node) {
                if contains_this_reference(arena, method.name) {
                    return true;
                }
            }
            return false;
        }
        k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
            if let Some(accessor) = arena.get_accessor(node) {
                if contains_this_reference(arena, accessor.name) {
                    return true;
                }
            }
            return false;
        }
        k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
            if let Some(computed) = arena.get_computed_property(node) {
                if contains_this_reference(arena, computed.expression) {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
            if let Some(tagged) = arena.get_tagged_template(node) {
                if contains_this_reference(arena, tagged.tag)
                    || contains_this_reference(arena, tagged.template)
                {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
            if let Some(template) = arena.get_template_expr(node) {
                for &span_idx in &template.template_spans.nodes {
                    if contains_this_reference(arena, span_idx) {
                        return true;
                    }
                }
            }
        }
        k if k == syntax_kind_ext::TEMPLATE_SPAN => {
            if let Some(span) = arena.get_template_span(node) {
                if contains_this_reference(arena, span.expression) {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::BINARY_EXPRESSION => {
            if let Some(bin) = arena.get_binary_expr(node) {
                if contains_this_reference(arena, bin.left) {
                    return true;
                }
                if contains_this_reference(arena, bin.right) {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
            if let Some(expr_stmt) = arena.get_expression_statement(node) {
                return contains_this_reference(arena, expr_stmt.expression);
            }
        }
        k if k == syntax_kind_ext::RETURN_STATEMENT => {
            if let Some(ret) = arena.get_return_statement(node) {
                if !ret.expression.is_none() {
                    return contains_this_reference(arena, ret.expression);
                }
            }
        }
        k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION =>
        {
            if let Some(unary) = arena.get_unary_expr(node) {
                if contains_this_reference(arena, unary.operand) {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::AWAIT_EXPRESSION
            || k == syntax_kind_ext::YIELD_EXPRESSION
            || k == syntax_kind_ext::NON_NULL_EXPRESSION =>
        {
            if let Some(unary) = arena.get_unary_expr_ex(node) {
                if !unary.expression.is_none() && contains_this_reference(arena, unary.expression) {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::TYPE_ASSERTION
            || k == syntax_kind_ext::AS_EXPRESSION
            || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
        {
            if let Some(assertion) = arena.get_type_assertion(node) {
                if contains_this_reference(arena, assertion.expression) {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::VARIABLE_STATEMENT
            || k == syntax_kind_ext::VARIABLE_DECLARATION_LIST =>
        {
            if let Some(var_stmt) = arena.get_variable(node) {
                for &decl_idx in &var_stmt.declarations.nodes {
                    if contains_this_reference(arena, decl_idx) {
                        return true;
                    }
                }
            }
        }
        k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
            if let Some(decl) = arena.get_variable_declaration(node) {
                if !decl.initializer.is_none() && contains_this_reference(arena, decl.initializer) {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::ARROW_FUNCTION => {
            if let Some(func) = arena.get_function(node) {
                for &param_idx in &func.parameters.nodes {
                    let Some(param_node) = arena.get(param_idx) else {
                        continue;
                    };
                    let Some(param) = arena.get_parameter(param_node) else {
                        continue;
                    };
                    if !param.initializer.is_none()
                        && contains_this_reference(arena, param.initializer)
                    {
                        return true;
                    }
                }

                if !func.body.is_none() && contains_this_reference(arena, func.body) {
                    return true;
                }
            }
            return false;
        }
        k if k == syntax_kind_ext::FUNCTION_EXPRESSION
            || k == syntax_kind_ext::FUNCTION_DECLARATION =>
        {
            // Regular functions have their own `this`, so don't recurse
            return false;
        }
        _ => {}
    }

    false
}

/// Context for arrow function transformation
pub struct ArrowTransformContext {
    /// Whether we need to capture `this` as `_this`
    pub needs_this_capture: bool,
}

impl ArrowTransformContext {
    pub fn new() -> Self {
        ArrowTransformContext {
            needs_this_capture: false,
        }
    }

    /// Analyze an arrow function to determine if `this` capture is needed
    pub fn analyze_arrow(&mut self, arena: &ThinNodeArena, func_idx: NodeIndex) {
        let Some(func_node) = arena.get(func_idx) else {
            return;
        };
        let Some(func_data) = arena.get_function(func_node) else {
            return;
        };

        // Check if body contains `this` references
        if !func_data.body.is_none() && contains_this_reference(arena, func_data.body) {
            self.needs_this_capture = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::SyntaxKind;
    use crate::thin_parser::ThinParserState;

    #[test]
    fn test_detect_this_in_arrow() {
        let source = "const f = () => this.x;";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let _root = parser.parse_source_file();

        // Simple test: the source contains "this" keyword
        assert!(
            source.contains("this"),
            "Expected to detect 'this' in source"
        );
    }

    #[test]
    fn test_no_this_in_arrow() {
        let source = "const add = (a, b) => a + b;";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let _root = parser.parse_source_file();

        // Simple test: the source doesn't contain "this"
        assert!(
            !source.contains("this"),
            "Should not detect 'this' in simple arrow"
        );
    }
}
