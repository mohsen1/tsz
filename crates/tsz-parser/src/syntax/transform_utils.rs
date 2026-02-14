//! Transform utilities for syntax analysis.
//!
//! Common functions used by ES5 transformations.

use crate::parser::{NodeArena, NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

/// Check if an AST node contains a reference to `this` or `super`.
pub fn contains_this_reference(arena: &NodeArena, node_idx: NodeIndex) -> bool {
    let Some(node) = arena.get(node_idx) else {
        return false;
    };

    if node.kind == SyntaxKind::Identifier as u16
        && let Some(identifier) = arena.get_identifier(node)
        && identifier.escaped_text == "this"
    {
        return true;
    }

    // Check if this node is `this` or `super`
    if node.kind == SyntaxKind::ThisKeyword as u16 || node.kind == SyntaxKind::SuperKeyword as u16 {
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
            if let Some(spread) = arena.get_spread(node)
                && contains_this_reference(arena, spread.expression)
            {
                return true;
            }
        }
        k if k == syntax_kind_ext::METHOD_DECLARATION => {
            if let Some(method) = arena.get_method_decl(node)
                && contains_this_reference(arena, method.name)
            {
                return true;
            }
            return false;
        }
        k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
            if let Some(accessor) = arena.get_accessor(node)
                && contains_this_reference(arena, accessor.name)
            {
                return true;
            }
            return false;
        }
        k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
            if let Some(computed) = arena.get_computed_property(node)
                && contains_this_reference(arena, computed.expression)
            {
                return true;
            }
        }
        k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
            if let Some(tagged) = arena.get_tagged_template(node)
                && (contains_this_reference(arena, tagged.tag)
                    || contains_this_reference(arena, tagged.template))
            {
                return true;
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
            if let Some(span) = arena.get_template_span(node)
                && contains_this_reference(arena, span.expression)
            {
                return true;
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
        k if k == syntax_kind_ext::IF_STATEMENT => {
            if let Some(if_stmt) = arena.get_if_statement(node) {
                if contains_this_reference(arena, if_stmt.expression) {
                    return true;
                }
                if contains_this_reference(arena, if_stmt.then_statement) {
                    return true;
                }
                if !if_stmt.else_statement.is_none()
                    && contains_this_reference(arena, if_stmt.else_statement)
                {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::FOR_STATEMENT
            || k == syntax_kind_ext::WHILE_STATEMENT
            || k == syntax_kind_ext::DO_STATEMENT =>
        {
            if let Some(loop_data) = arena.get_loop(node) {
                if !loop_data.initializer.is_none()
                    && contains_this_reference(arena, loop_data.initializer)
                {
                    return true;
                }
                if !loop_data.condition.is_none()
                    && contains_this_reference(arena, loop_data.condition)
                {
                    return true;
                }
                if !loop_data.incrementor.is_none()
                    && contains_this_reference(arena, loop_data.incrementor)
                {
                    return true;
                }
                if contains_this_reference(arena, loop_data.statement) {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::FOR_IN_STATEMENT || k == syntax_kind_ext::FOR_OF_STATEMENT => {
            if let Some(for_in_of) = arena.get_for_in_of(node) {
                if contains_this_reference(arena, for_in_of.initializer) {
                    return true;
                }
                if contains_this_reference(arena, for_in_of.expression) {
                    return true;
                }
                if contains_this_reference(arena, for_in_of.statement) {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::RETURN_STATEMENT => {
            if let Some(ret) = arena.get_return_statement(node)
                && !ret.expression.is_none()
            {
                return contains_this_reference(arena, ret.expression);
            }
        }
        k if k == syntax_kind_ext::THROW_STATEMENT => {
            if let Some(thr) = arena.get_return_statement(node)
                && !thr.expression.is_none()
                && contains_this_reference(arena, thr.expression)
            {
                return true;
            }
        }
        k if k == syntax_kind_ext::SWITCH_STATEMENT => {
            if let Some(switch) = arena.get_switch(node) {
                if contains_this_reference(arena, switch.expression) {
                    return true;
                }
                if contains_this_reference(arena, switch.case_block) {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
            if let Some(clause) = arena.get_case_clause(node) {
                if !clause.expression.is_none() && contains_this_reference(arena, clause.expression)
                {
                    return true;
                }
                for &stmt in &clause.statements.nodes {
                    if contains_this_reference(arena, stmt) {
                        return true;
                    }
                }
            }
        }
        k if k == syntax_kind_ext::TRY_STATEMENT => {
            if let Some(try_stmt) = arena.get_try(node) {
                if contains_this_reference(arena, try_stmt.try_block) {
                    return true;
                }
                if !try_stmt.catch_clause.is_none()
                    && contains_this_reference(arena, try_stmt.catch_clause)
                {
                    return true;
                }
                if !try_stmt.finally_block.is_none()
                    && contains_this_reference(arena, try_stmt.finally_block)
                {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::CATCH_CLAUSE => {
            if let Some(catch) = arena.get_catch_clause(node) {
                if !catch.variable_declaration.is_none()
                    && contains_this_reference(arena, catch.variable_declaration)
                {
                    return true;
                }
                if contains_this_reference(arena, catch.block) {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION =>
        {
            if let Some(unary) = arena.get_unary_expr(node)
                && contains_this_reference(arena, unary.operand)
            {
                return true;
            }
        }
        k if k == syntax_kind_ext::AWAIT_EXPRESSION
            || k == syntax_kind_ext::YIELD_EXPRESSION
            || k == syntax_kind_ext::NON_NULL_EXPRESSION =>
        {
            if let Some(unary) = arena.get_unary_expr_ex(node)
                && !unary.expression.is_none()
                && contains_this_reference(arena, unary.expression)
            {
                return true;
            }
        }
        k if k == syntax_kind_ext::TYPE_ASSERTION
            || k == syntax_kind_ext::AS_EXPRESSION
            || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
        {
            if let Some(assertion) = arena.get_type_assertion(node)
                && contains_this_reference(arena, assertion.expression)
            {
                return true;
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
            if let Some(decl) = arena.get_variable_declaration(node)
                && !decl.initializer.is_none()
                && contains_this_reference(arena, decl.initializer)
            {
                return true;
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

/// Check if a node contains a reference to `arguments`.
///
/// This is used to determine if an arrow function needs to capture the parent
/// function's `arguments` object for ES5 downleveling.
///
/// Important: Regular functions have their own `arguments`, so we don't recurse
/// into them. Only arrow functions inherit the parent's `arguments`.
pub fn contains_arguments_reference(arena: &NodeArena, node_idx: NodeIndex) -> bool {
    let Some(node) = arena.get(node_idx) else {
        return false;
    };

    // Check if this node is `arguments` identifier
    if node.kind == SyntaxKind::Identifier as u16 {
        // Check the identifier text to see if it's "arguments"
        if let Some(identifier) = arena.get_identifier(node) {
            if identifier.escaped_text == "arguments" {
                return true;
            }
        }
    }

    // Check children recursively based on node type
    match node.kind {
        k if k == syntax_kind_ext::BLOCK => {
            if let Some(block) = arena.get_block(node) {
                for &stmt_idx in &block.statements.nodes {
                    if contains_arguments_reference(arena, stmt_idx) {
                        return true;
                    }
                }
            }
        }
        k if k == syntax_kind_ext::CALL_EXPRESSION || k == syntax_kind_ext::NEW_EXPRESSION => {
            if let Some(call) = arena.get_call_expr(node) {
                if contains_arguments_reference(arena, call.expression) {
                    return true;
                }
                if let Some(ref args) = call.arguments {
                    for &arg_idx in &args.nodes {
                        if contains_arguments_reference(arena, arg_idx) {
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
                if contains_arguments_reference(arena, access.expression) {
                    return true;
                }
                if contains_arguments_reference(arena, access.name_or_argument) {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
            if let Some(paren) = arena.get_parenthesized(node) {
                return contains_arguments_reference(arena, paren.expression);
            }
        }
        k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
            if let Some(cond) = arena.get_conditional_expr(node) {
                return contains_arguments_reference(arena, cond.condition)
                    || contains_arguments_reference(arena, cond.when_true)
                    || contains_arguments_reference(arena, cond.when_false);
            }
        }
        k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
            if let Some(lit) = arena.get_literal_expr(node) {
                for &elem_idx in &lit.elements.nodes {
                    if !elem_idx.is_none() && contains_arguments_reference(arena, elem_idx) {
                        return true;
                    }
                }
            }
        }
        k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
            if let Some(lit) = arena.get_literal_expr(node) {
                for &elem_idx in &lit.elements.nodes {
                    if !elem_idx.is_none() && contains_arguments_reference(arena, elem_idx) {
                        return true;
                    }
                }
            }
        }
        k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
            if let Some(prop) = arena.get_property_assignment(node) {
                if contains_arguments_reference(arena, prop.name) {
                    return true;
                }
                if !prop.initializer.is_none()
                    && contains_arguments_reference(arena, prop.initializer)
                {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
            if let Some(prop) = arena.get_shorthand_property(node) {
                if contains_arguments_reference(arena, prop.name) {
                    return true;
                }
                if prop.equals_token
                    && !prop.object_assignment_initializer.is_none()
                    && contains_arguments_reference(arena, prop.object_assignment_initializer)
                {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::SPREAD_ELEMENT || k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
            if let Some(spread) = arena.get_spread(node)
                && contains_arguments_reference(arena, spread.expression)
            {
                return true;
            }
        }
        k if k == syntax_kind_ext::METHOD_DECLARATION => {
            if let Some(method) = arena.get_method_decl(node)
                && contains_arguments_reference(arena, method.name)
            {
                return true;
            }
            return false;
        }
        k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
            if let Some(accessor) = arena.get_accessor(node)
                && contains_arguments_reference(arena, accessor.name)
            {
                return true;
            }
            return false;
        }
        k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
            if let Some(computed) = arena.get_computed_property(node)
                && contains_arguments_reference(arena, computed.expression)
            {
                return true;
            }
        }
        k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
            if let Some(tagged) = arena.get_tagged_template(node)
                && (contains_arguments_reference(arena, tagged.tag)
                    || contains_arguments_reference(arena, tagged.template))
            {
                return true;
            }
        }
        k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
            if let Some(template) = arena.get_template_expr(node) {
                for &span_idx in &template.template_spans.nodes {
                    if contains_arguments_reference(arena, span_idx) {
                        return true;
                    }
                }
            }
        }
        k if k == syntax_kind_ext::TEMPLATE_SPAN => {
            if let Some(span) = arena.get_template_span(node)
                && contains_arguments_reference(arena, span.expression)
            {
                return true;
            }
        }
        k if k == syntax_kind_ext::BINARY_EXPRESSION => {
            if let Some(bin) = arena.get_binary_expr(node) {
                if contains_arguments_reference(arena, bin.left) {
                    return true;
                }
                if contains_arguments_reference(arena, bin.right) {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
            if let Some(expr_stmt) = arena.get_expression_statement(node) {
                return contains_arguments_reference(arena, expr_stmt.expression);
            }
        }
        k if k == syntax_kind_ext::IF_STATEMENT => {
            if let Some(if_stmt) = arena.get_if_statement(node) {
                if contains_arguments_reference(arena, if_stmt.expression) {
                    return true;
                }
                if contains_arguments_reference(arena, if_stmt.then_statement) {
                    return true;
                }
                if !if_stmt.else_statement.is_none()
                    && contains_arguments_reference(arena, if_stmt.else_statement)
                {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::FOR_STATEMENT
            || k == syntax_kind_ext::WHILE_STATEMENT
            || k == syntax_kind_ext::DO_STATEMENT =>
        {
            if let Some(loop_data) = arena.get_loop(node) {
                if !loop_data.initializer.is_none()
                    && contains_arguments_reference(arena, loop_data.initializer)
                {
                    return true;
                }
                if !loop_data.condition.is_none()
                    && contains_arguments_reference(arena, loop_data.condition)
                {
                    return true;
                }
                if !loop_data.incrementor.is_none()
                    && contains_arguments_reference(arena, loop_data.incrementor)
                {
                    return true;
                }
                if contains_arguments_reference(arena, loop_data.statement) {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::FOR_IN_STATEMENT || k == syntax_kind_ext::FOR_OF_STATEMENT => {
            if let Some(for_in_of) = arena.get_for_in_of(node) {
                if contains_arguments_reference(arena, for_in_of.initializer) {
                    return true;
                }
                if contains_arguments_reference(arena, for_in_of.expression) {
                    return true;
                }
                if contains_arguments_reference(arena, for_in_of.statement) {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::RETURN_STATEMENT => {
            if let Some(ret) = arena.get_return_statement(node)
                && !ret.expression.is_none()
            {
                return contains_arguments_reference(arena, ret.expression);
            }
        }
        k if k == syntax_kind_ext::THROW_STATEMENT => {
            if let Some(thr) = arena.get_return_statement(node)
                && !thr.expression.is_none()
                && contains_arguments_reference(arena, thr.expression)
            {
                return true;
            }
        }
        k if k == syntax_kind_ext::SWITCH_STATEMENT => {
            if let Some(switch) = arena.get_switch(node) {
                if contains_arguments_reference(arena, switch.expression) {
                    return true;
                }
                if contains_arguments_reference(arena, switch.case_block) {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
            if let Some(clause) = arena.get_case_clause(node) {
                if !clause.expression.is_none()
                    && contains_arguments_reference(arena, clause.expression)
                {
                    return true;
                }
                for &stmt in &clause.statements.nodes {
                    if contains_arguments_reference(arena, stmt) {
                        return true;
                    }
                }
            }
        }
        k if k == syntax_kind_ext::TRY_STATEMENT => {
            if let Some(try_stmt) = arena.get_try(node) {
                if contains_arguments_reference(arena, try_stmt.try_block) {
                    return true;
                }
                if !try_stmt.catch_clause.is_none()
                    && contains_arguments_reference(arena, try_stmt.catch_clause)
                {
                    return true;
                }
                if !try_stmt.finally_block.is_none()
                    && contains_arguments_reference(arena, try_stmt.finally_block)
                {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::CATCH_CLAUSE => {
            if let Some(catch) = arena.get_catch_clause(node) {
                if !catch.variable_declaration.is_none()
                    && contains_arguments_reference(arena, catch.variable_declaration)
                {
                    return true;
                }
                if contains_arguments_reference(arena, catch.block) {
                    return true;
                }
            }
        }
        k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION =>
        {
            if let Some(unary) = arena.get_unary_expr(node)
                && contains_arguments_reference(arena, unary.operand)
            {
                return true;
            }
        }
        k if k == syntax_kind_ext::AWAIT_EXPRESSION
            || k == syntax_kind_ext::YIELD_EXPRESSION
            || k == syntax_kind_ext::NON_NULL_EXPRESSION =>
        {
            if let Some(unary) = arena.get_unary_expr_ex(node)
                && !unary.expression.is_none()
                && contains_arguments_reference(arena, unary.expression)
            {
                return true;
            }
        }
        k if k == syntax_kind_ext::TYPE_ASSERTION
            || k == syntax_kind_ext::AS_EXPRESSION
            || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
        {
            if let Some(assertion) = arena.get_type_assertion(node)
                && contains_arguments_reference(arena, assertion.expression)
            {
                return true;
            }
        }
        k if k == syntax_kind_ext::VARIABLE_STATEMENT
            || k == syntax_kind_ext::VARIABLE_DECLARATION_LIST =>
        {
            if let Some(var_stmt) = arena.get_variable(node) {
                for &decl_idx in &var_stmt.declarations.nodes {
                    if contains_arguments_reference(arena, decl_idx) {
                        return true;
                    }
                }
            }
        }
        k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
            if let Some(decl) = arena.get_variable_declaration(node)
                && !decl.initializer.is_none()
                && contains_arguments_reference(arena, decl.initializer)
            {
                return true;
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
                        && contains_arguments_reference(arena, param.initializer)
                    {
                        return true;
                    }
                }

                if !func.body.is_none() && contains_arguments_reference(arena, func.body) {
                    return true;
                }
            }
            return false;
        }
        k if k == syntax_kind_ext::FUNCTION_EXPRESSION
            || k == syntax_kind_ext::FUNCTION_DECLARATION =>
        {
            // Regular functions have their own `arguments`, so don't recurse
            return false;
        }
        _ => {}
    }

    false
}

/// Check if a node is a private identifier (#field)
pub fn is_private_identifier(arena: &NodeArena, name_idx: NodeIndex) -> bool {
    let Some(node) = arena.get(name_idx) else {
        return false;
    };
    node.kind == SyntaxKind::PrivateIdentifier as u16
}

#[cfg(test)]
mod tests {
    use crate::parser::{NodeIndex, ParserState};
    use crate::syntax::transform_utils::contains_arguments_reference;
    use crate::syntax::transform_utils::contains_this_reference;

    fn parse_arena(source: &str) -> (ParserState, NodeIndex) {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        (parser, root)
    }

    #[test]
    fn contains_this_reference_detects_this_in_function_body() {
        let (parser, root) = parse_arena("function f() { return this; }");
        let sf = parser.get_arena().get_source_file_at(root).unwrap();
        let statement = sf.statements.nodes[0];
        let statement_node = parser.get_arena().get(statement).unwrap();
        let func = parser.get_arena().get_function(statement_node).unwrap();
        let body = func.body;

        assert!(contains_this_reference(parser.get_arena(), body));
    }

    #[test]
    fn contains_this_reference_ignores_literal_tree() {
        let (parser, root) = parse_arena("function noThis() { return 42; }");
        let sf = parser.get_arena().get_source_file_at(root).unwrap();
        let statement = sf.statements.nodes[0];
        let statement_node = parser.get_arena().get(statement).unwrap();
        let func = parser.get_arena().get_function(statement_node).unwrap();
        let body = func.body;

        assert!(!contains_this_reference(parser.get_arena(), body));
    }

    #[test]
    fn contains_arguments_reference_detects_arguments_in_function_body() {
        let (parser, root) = parse_arena("function f() { return arguments; }");
        let sf = parser.get_arena().get_source_file_at(root).unwrap();
        let statement = sf.statements.nodes[0];
        let statement_node = parser.get_arena().get(statement).unwrap();
        let func = parser.get_arena().get_function(statement_node).unwrap();
        let body = func.body;

        assert!(contains_arguments_reference(parser.get_arena(), body));
    }

    #[test]
    fn contains_arguments_reference_ignores_missing_reference() {
        let (parser, root) = parse_arena("function noArgs() { return 42; }");
        let sf = parser.get_arena().get_source_file_at(root).unwrap();
        let statement = sf.statements.nodes[0];
        let statement_node = parser.get_arena().get(statement).unwrap();
        let func = parser.get_arena().get_function(statement_node).unwrap();
        let body = func.body;

        assert!(!contains_arguments_reference(parser.get_arena(), body));
    }
}
