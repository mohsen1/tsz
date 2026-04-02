//! Comprehensive parser unit tests covering operator precedence, arrow functions,
//! type syntax, declarations, class syntax, statements, and error recovery.

use crate::parser::node::NodeArena;
use crate::parser::syntax_kind_ext;
use crate::parser::{NodeIndex, ParserState};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_scanner::SyntaxKind;

// =============================================================================
// Helpers
// =============================================================================

fn parse_source(source: &str) -> (ParserState, NodeIndex) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

fn assert_no_errors(parser: &ParserState, context: &str) {
    let diags = parser.get_diagnostics();
    assert!(
        diags.is_empty(),
        "{context}: expected no errors, got {}: {:?}",
        diags.len(),
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

fn assert_has_errors(parser: &ParserState, context: &str) {
    assert!(
        !parser.get_diagnostics().is_empty(),
        "{context}: expected errors but got none"
    );
}

fn get_first_statement(arena: &NodeArena, root: NodeIndex) -> NodeIndex {
    let sf = arena.get_source_file_at(root).expect("missing source file");
    assert!(
        !sf.statements.nodes.is_empty(),
        "expected at least one statement"
    );
    sf.statements.nodes[0]
}

fn get_statements(arena: &NodeArena, root: NodeIndex) -> Vec<NodeIndex> {
    let sf = arena.get_source_file_at(root).expect("missing source file");
    sf.statements.nodes.clone()
}

fn get_first_variable_declaration(arena: &NodeArena, root: NodeIndex) -> NodeIndex {
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt node");
    let var_stmt = arena.get_variable(stmt_node).expect("variable statement");
    let decl_list_idx = var_stmt.declarations.nodes[0];
    let decl_list_node = arena.get(decl_list_idx).expect("var decl list node");
    let decl_list = arena
        .get_variable(decl_list_node)
        .expect("variable declaration list");
    decl_list.declarations.nodes[0]
}

/// For `const x = <expr>;` or `let x = <expr>;`, extract the initializer expression.
/// Structure: `VARIABLE_STATEMENT` -> [`VARIABLE_DECLARATION_LIST`] -> [`VARIABLE_DECLARATION`, ...]
fn get_var_initializer(arena: &NodeArena, root: NodeIndex) -> NodeIndex {
    let decl_idx = get_first_variable_declaration(arena, root);
    let decl_node = arena.get(decl_idx).expect("var decl node");
    let decl = arena
        .get_variable_declaration(decl_node)
        .expect("var decl data");
    decl.initializer
}

fn get_var_type_annotation(arena: &NodeArena, root: NodeIndex) -> NodeIndex {
    let decl_idx = get_first_variable_declaration(arena, root);
    let decl_node = arena.get(decl_idx).expect("var decl node");
    let decl = arena
        .get_variable_declaration(decl_node)
        .expect("var decl data");
    decl.type_annotation
}

fn node_text<'a>(arena: &NodeArena, source: &'a str, idx: NodeIndex) -> &'a str {
    let node = arena.get(idx).expect("node");
    &source[node.pos as usize..node.end as usize]
}

/// For a binary expression node, get its data.
fn get_binary(arena: &NodeArena, idx: NodeIndex) -> (NodeIndex, u16, NodeIndex) {
    let node = arena.get(idx).expect("node");
    let bin = arena.get_binary_expr(node).expect("binary expr data");
    (bin.left, bin.operator_token, bin.right)
}

// =============================================================================
// 1. Operator Precedence Tests (15+ tests)
// =============================================================================

#[test]
fn precedence_multiplication_binds_tighter_than_addition() {
    // `1 + 2 * 3` should parse as `1 + (2 * 3)`
    let (parser, root) = parse_source("const x = 1 + 2 * 3;");
    assert_no_errors(&parser, "1 + 2 * 3");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let (left, op, right) = get_binary(arena, init);
    assert_eq!(op, SyntaxKind::PlusToken as u16, "top should be +");
    // left should be numeric literal 1
    let left_node = arena.get(left).expect("left node");
    assert_eq!(left_node.kind, SyntaxKind::NumericLiteral as u16);
    // right should be binary: 2 * 3
    let right_node = arena.get(right).expect("right node");
    assert_eq!(right_node.kind, syntax_kind_ext::BINARY_EXPRESSION);
    let (_, inner_op, _) = get_binary(arena, right);
    assert_eq!(
        inner_op,
        SyntaxKind::AsteriskToken as u16,
        "inner should be *"
    );
}

#[test]
fn precedence_nullish_coalescing_vs_logical_or() {
    // `a ?? b || c` — ?? and || mixing. The parser may or may not error here
    // (tsc treats it as a parse error). We verify it produces a valid AST.
    let (parser, root) = parse_source("const x = a ?? b || c;");
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).expect("source file");
    // Should produce at least one statement regardless of errors
    assert!(!sf.statements.nodes.is_empty(), "should parse something");
}

#[test]
fn precedence_logical_and_vs_logical_or() {
    // `a || b && c` should parse as `a || (b && c)` since && binds tighter
    let (parser, root) = parse_source("const x = a || b && c;");
    assert_no_errors(&parser, "a || b && c");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let (_, op, right) = get_binary(arena, init);
    assert_eq!(op, SyntaxKind::BarBarToken as u16, "top should be ||");
    let right_node = arena.get(right).expect("right node");
    assert_eq!(
        right_node.kind,
        syntax_kind_ext::BINARY_EXPRESSION,
        "RHS should be binary"
    );
    let (_, inner_op, _) = get_binary(arena, right);
    assert_eq!(
        inner_op,
        SyntaxKind::AmpersandAmpersandToken as u16,
        "inner should be &&"
    );
}

#[test]
fn precedence_ternary_nesting_right_associative() {
    // `a ? b : c ? d : e` should parse as `a ? b : (c ? d : e)`
    let (parser, root) = parse_source("const x = a ? b : c ? d : e;");
    assert_no_errors(&parser, "ternary nesting");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init node");
    assert_eq!(
        node.kind,
        syntax_kind_ext::CONDITIONAL_EXPRESSION,
        "top should be conditional"
    );
    let cond = arena.get_conditional_expr(node).expect("cond data");
    // when_false should itself be a conditional expression
    let false_node = arena.get(cond.when_false).expect("false branch");
    assert_eq!(
        false_node.kind,
        syntax_kind_ext::CONDITIONAL_EXPRESSION,
        "false branch should be nested conditional"
    );
}

#[test]
fn precedence_comma_operator_vs_argument_separator() {
    // In `f(a, b)`, comma separates arguments, not comma operator
    let (parser, root) = parse_source("f(a, b);");
    assert_no_errors(&parser, "f(a, b)");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt node");
    let expr_stmt = arena.get_expression_statement(stmt_node).expect("expr");
    let call_node = arena.get(expr_stmt.expression).expect("call node");
    let call = arena.get_call_expr(call_node).expect("call data");
    let args = call.arguments.as_ref().expect("arguments");
    assert_eq!(args.nodes.len(), 2, "should have 2 arguments, not comma op");
}

#[test]
fn precedence_comma_operator_in_expression() {
    // `const x = (1, 2, 3)` — the comma operator inside parens
    let (parser, root) = parse_source("const x = (1, 2, 3);");
    assert_no_errors(&parser, "comma operator");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    // Should be parenthesized
    let paren_node = arena.get(init).expect("node");
    assert_eq!(paren_node.kind, syntax_kind_ext::PARENTHESIZED_EXPRESSION);
    let paren = arena.get_parenthesized(paren_node).expect("paren data");
    // Inner should be comma binary expression
    let inner = arena.get(paren.expression).expect("inner");
    assert_eq!(inner.kind, syntax_kind_ext::BINARY_EXPRESSION);
    let (_, op, _) = get_binary(arena, paren.expression);
    assert_eq!(op, SyntaxKind::CommaToken as u16, "should be comma");
}

#[test]
fn precedence_assignment_right_associativity() {
    // `a = b = c` should parse as `a = (b = c)`
    let (parser, root) = parse_source("a = b = c;");
    assert_no_errors(&parser, "a = b = c");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let expr_stmt = arena
        .get_expression_statement(stmt_node)
        .expect("expr stmt");
    let (_, op, right) = get_binary(arena, expr_stmt.expression);
    assert_eq!(op, SyntaxKind::EqualsToken as u16, "top = assignment");
    let right_node = arena.get(right).expect("right");
    assert_eq!(
        right_node.kind,
        syntax_kind_ext::BINARY_EXPRESSION,
        "RHS should be nested assignment"
    );
    let (_, inner_op, _) = get_binary(arena, right);
    assert_eq!(
        inner_op,
        SyntaxKind::EqualsToken as u16,
        "inner = assignment"
    );
}

#[test]
fn precedence_exponentiation_right_associative() {
    // `2 ** 3 ** 4` should parse as `2 ** (3 ** 4)`
    let (parser, root) = parse_source("const x = 2 ** 3 ** 4;");
    assert_no_errors(&parser, "2 ** 3 ** 4");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let (left, op, right) = get_binary(arena, init);
    assert_eq!(
        op,
        SyntaxKind::AsteriskAsteriskToken as u16,
        "top should be **"
    );
    // left should be numeric literal 2
    let left_node = arena.get(left).expect("left");
    assert_eq!(left_node.kind, SyntaxKind::NumericLiteral as u16);
    // right should be binary: 3 ** 4
    let right_node = arena.get(right).expect("right");
    assert_eq!(right_node.kind, syntax_kind_ext::BINARY_EXPRESSION);
    let (_, inner_op, _) = get_binary(arena, right);
    assert_eq!(
        inner_op,
        SyntaxKind::AsteriskAsteriskToken as u16,
        "inner should be **"
    );
}

#[test]
fn precedence_optional_chaining_with_call() {
    // `a?.b()` should parse as call on optional property access
    let (parser, root) = parse_source("a?.b();");
    assert_no_errors(&parser, "a?.b()");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let expr_stmt = arena.get_expression_statement(stmt_node).expect("expr");
    let call_node = arena.get(expr_stmt.expression).expect("call node");
    assert_eq!(
        call_node.kind,
        syntax_kind_ext::CALL_EXPRESSION,
        "should be call expr"
    );
    let call = arena.get_call_expr(call_node).expect("call data");
    // The callee should be a property access with question dot
    let access_node = arena.get(call.expression).expect("access");
    assert_eq!(
        access_node.kind,
        syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
    );
    let access = arena.get_access_expr(access_node).expect("access data");
    assert!(access.question_dot_token, "should have ?. token");
}

#[test]
fn precedence_comparison_operators() {
    // `a < b === c > d` should parse as `(a < b) === (c > d)`
    let (parser, root) = parse_source("const x = a < b === c > d;");
    assert_no_errors(&parser, "comparison operators");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let (_, op, _) = get_binary(arena, init);
    assert_eq!(
        op,
        SyntaxKind::EqualsEqualsEqualsToken as u16,
        "top should be ==="
    );
}

#[test]
fn precedence_bitwise_and_vs_equality() {
    // `a === b & c`: & has lower precedence than ===, so it's `(a === b) & c`
    let (parser, root) = parse_source("const x = a === b & c;");
    assert_no_errors(&parser, "=== vs &");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let (_, op, _) = get_binary(arena, init);
    assert_eq!(
        op,
        SyntaxKind::AmpersandToken as u16,
        "top should be & (lower precedence)"
    );
}

#[test]
fn precedence_as_expression() {
    // `a as T` produces an AsExpression
    let (parser, root) = parse_source("const x = a as number;");
    assert_no_errors(&parser, "as expression");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::AS_EXPRESSION,
        "should be as expression"
    );
}

#[test]
fn precedence_satisfies_expression() {
    // `a satisfies T` produces a SatisfiesExpression
    let (parser, root) = parse_source("const x = a satisfies number;");
    assert_no_errors(&parser, "satisfies expression");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::SATISFIES_EXPRESSION,
        "should be satisfies expression"
    );
}

#[test]
fn precedence_non_null_assertion() {
    // `a!` produces a NonNullExpression
    let (parser, root) = parse_source("const x = a!;");
    assert_no_errors(&parser, "non-null assertion");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::NON_NULL_EXPRESSION,
        "should be non-null expr"
    );
}

#[test]
fn precedence_type_assertion_angle_bracket() {
    // `<number>a` produces a TypeAssertion
    let (parser, root) = parse_source("const x = <number>a;");
    assert_no_errors(&parser, "type assertion");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::TYPE_ASSERTION,
        "should be type assertion"
    );
}

#[test]
fn precedence_instanceof_and_in() {
    // `a instanceof B` and `a in b` should parse without errors
    let (parser, _) = parse_source("const x = a instanceof B; const y = 'a' in b;");
    assert_no_errors(&parser, "instanceof and in");
}

#[test]
fn precedence_ternary_with_assignment() {
    // `a ? b = 1 : c = 2` should parse correctly
    let (parser, root) = parse_source("a ? b = 1 : c = 2;");
    assert_no_errors(&parser, "ternary with assignment");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let expr_stmt = arena.get_expression_statement(stmt_node).expect("expr");
    let cond_node = arena.get(expr_stmt.expression).expect("cond");
    assert_eq!(
        cond_node.kind,
        syntax_kind_ext::CONDITIONAL_EXPRESSION,
        "should be conditional"
    );
}

#[test]
fn precedence_addition_left_associative() {
    // `1 + 2 + 3` should parse as `(1 + 2) + 3`
    let (parser, root) = parse_source("const x = 1 + 2 + 3;");
    assert_no_errors(&parser, "1 + 2 + 3");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let (left, op, right) = get_binary(arena, init);
    assert_eq!(op, SyntaxKind::PlusToken as u16, "top should be +");
    // right should be a numeric literal (3), left should be binary
    let right_node = arena.get(right).expect("right");
    assert_eq!(
        right_node.kind,
        SyntaxKind::NumericLiteral as u16,
        "RHS should be literal"
    );
    let left_node = arena.get(left).expect("left");
    assert_eq!(
        left_node.kind,
        syntax_kind_ext::BINARY_EXPRESSION,
        "LHS should be binary (left-assoc)"
    );
}

// =============================================================================
// 2. Arrow Function Edge Cases (10+ tests)
// =============================================================================

#[test]
fn arrow_single_param_no_parens() {
    // `x => x`
    let (parser, root) = parse_source("const f = x => x;");
    assert_no_errors(&parser, "x => x");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::ARROW_FUNCTION,
        "should be arrow"
    );
    let func = arena.get_function(node).expect("function data");
    assert_eq!(func.parameters.nodes.len(), 1, "should have 1 param");
    assert!(!func.is_async, "should not be async");
}

#[test]
fn arrow_multi_param() {
    // `(x, y) => x + y`
    let (parser, root) = parse_source("const f = (x, y) => x + y;");
    assert_no_errors(&parser, "(x, y) => x + y");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::ARROW_FUNCTION);
    let func = arena.get_function(node).expect("function data");
    assert_eq!(func.parameters.nodes.len(), 2, "should have 2 params");
}

#[test]
fn arrow_no_params() {
    // `() => 42`
    let (parser, root) = parse_source("const f = () => 42;");
    assert_no_errors(&parser, "() => 42");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::ARROW_FUNCTION);
    let func = arena.get_function(node).expect("function data");
    assert_eq!(func.parameters.nodes.len(), 0, "should have 0 params");
}

#[test]
fn arrow_object_literal_body_needs_parens() {
    // `() => ({})` — object literal body must be parenthesized
    let (parser, root) = parse_source("const f = () => ({});");
    assert_no_errors(&parser, "() => ({})");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::ARROW_FUNCTION);
    let func = arena.get_function(node).expect("function data");
    // body should be a parenthesized expression
    let body = arena.get(func.body).expect("body");
    assert_eq!(
        body.kind,
        syntax_kind_ext::PARENTHESIZED_EXPRESSION,
        "body should be parenthesized"
    );
}

#[test]
fn arrow_async() {
    // `async x => await x`
    let (parser, root) = parse_source("const f = async x => await x;");
    assert_no_errors(&parser, "async arrow");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::ARROW_FUNCTION);
    let func = arena.get_function(node).expect("function data");
    assert!(func.is_async, "should be async");
}

#[test]
fn arrow_async_multi_param() {
    // `async (a, b) => a + b`
    let (parser, root) = parse_source("const f = async (a, b) => a + b;");
    assert_no_errors(&parser, "async multi param arrow");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::ARROW_FUNCTION);
    let func = arena.get_function(node).expect("function data");
    assert!(func.is_async, "should be async");
    assert_eq!(func.parameters.nodes.len(), 2);
}

#[test]
fn arrow_with_block_body() {
    // `(x) => { return x; }`
    let (parser, root) = parse_source("const f = (x) => { return x; };");
    assert_no_errors(&parser, "arrow with block body");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::ARROW_FUNCTION);
    let func = arena.get_function(node).expect("function data");
    let body = arena.get(func.body).expect("body");
    assert_eq!(body.kind, syntax_kind_ext::BLOCK, "body should be block");
}

#[test]
fn arrow_with_type_annotation() {
    // `(x: number): string => x.toString()`
    let (parser, root) = parse_source("const f = (x: number): string => x.toString();");
    assert_no_errors(&parser, "arrow with type annotation");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::ARROW_FUNCTION);
    let func = arena.get_function(node).expect("function data");
    assert!(func.type_annotation.is_some(), "should have return type");
}

#[test]
fn arrow_in_ternary() {
    // `cond ? x => x : y => y` — arrows in ternary branches
    let (parser, root) = parse_source("const f = cond ? x => x : y => y;");
    assert_no_errors(&parser, "arrow in ternary");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::CONDITIONAL_EXPRESSION,
        "top should be conditional"
    );
    let cond = arena.get_conditional_expr(node).expect("cond data");
    let true_branch = arena.get(cond.when_true).expect("true");
    assert_eq!(
        true_branch.kind,
        syntax_kind_ext::ARROW_FUNCTION,
        "true branch should be arrow"
    );
    let false_branch = arena.get(cond.when_false).expect("false");
    assert_eq!(
        false_branch.kind,
        syntax_kind_ext::ARROW_FUNCTION,
        "false branch should be arrow"
    );
}

#[test]
fn arrow_generic_in_ts_file() {
    // `<T>(x: T) => x` — generic arrow in .ts file (not TSX)
    let (parser, root) = parse_source("const f = <T>(x: T) => x;");
    assert_no_errors(&parser, "generic arrow .ts");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::ARROW_FUNCTION,
        "should be arrow function"
    );
    let func = arena.get_function(node).expect("function data");
    assert!(
        func.type_parameters.is_some(),
        "should have type parameters"
    );
}

#[test]
fn arrow_generic_with_constraint() {
    // `<T extends string>(x: T) => x`
    let (parser, root) = parse_source("const f = <T extends string>(x: T) => x;");
    assert_no_errors(&parser, "generic arrow with constraint");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::ARROW_FUNCTION);
}

#[test]
fn arrow_nested() {
    // `(a) => (b) => a + b` — curried arrow
    let (parser, root) = parse_source("const f = (a) => (b) => a + b;");
    assert_no_errors(&parser, "nested arrow");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::ARROW_FUNCTION);
    let func = arena.get_function(node).expect("function data");
    let body = arena.get(func.body).expect("body");
    assert_eq!(
        body.kind,
        syntax_kind_ext::ARROW_FUNCTION,
        "body should be nested arrow"
    );
}

#[test]
fn js_optional_parameter_span_starts_at_question_token() {
    let source = "const f = (b, c?: string) => c;";
    let mut parser = ParserState::new("fileJs.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let init = get_var_initializer(arena, root);
    let arrow_node = arena.get(init).expect("arrow node");
    let arrow = arena.get_function(arrow_node).expect("arrow data");
    let param_idx = arrow.parameters.nodes[1];
    let param_node = arena.get(param_idx).expect("param node");

    assert_eq!(
        param_node.pos,
        source.find('?').expect("question token position") as u32,
        "JS optional parameter spans should anchor at '?' for JS-only diagnostics"
    );
}

// =============================================================================
// 3. Type Syntax Parsing (15+ tests)
// =============================================================================

#[test]
fn type_union() {
    // `type T = A | B | C`
    let (parser, root) = parse_source("type T = A | B | C;");
    assert_no_errors(&parser, "union type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::UNION_TYPE,
        "should be union type"
    );
    let composite = arena.get_composite_type(type_node).expect("composite");
    assert_eq!(composite.types.nodes.len(), 3, "should have 3 members");
}

#[test]
fn type_intersection() {
    // `type T = A & B & C`
    let (parser, root) = parse_source("type T = A & B & C;");
    assert_no_errors(&parser, "intersection type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::INTERSECTION_TYPE,
        "should be intersection type"
    );
    let composite = arena.get_composite_type(type_node).expect("composite");
    assert_eq!(composite.types.nodes.len(), 3, "should have 3 members");
}

#[test]
fn type_conditional() {
    // `type T = X extends Y ? A : B`
    let (parser, root) = parse_source("type T = X extends Y ? A : B;");
    assert_no_errors(&parser, "conditional type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::CONDITIONAL_TYPE,
        "should be conditional type"
    );
    let cond = arena.get_conditional_type(type_node).expect("cond type");
    assert!(cond.check_type.is_some(), "should have check type");
    assert!(cond.extends_type.is_some(), "should have extends type");
    assert!(cond.true_type.is_some(), "should have true type");
    assert!(cond.false_type.is_some(), "should have false type");
}

#[test]
fn type_conditional_nested() {
    // `type T = X extends A ? B extends C ? D : E : F`
    let (parser, root) = parse_source("type T = X extends A ? B extends C ? D : E : F;");
    assert_no_errors(&parser, "nested conditional type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(type_node.kind, syntax_kind_ext::CONDITIONAL_TYPE);
    let outer = arena.get_conditional_type(type_node).expect("outer cond");
    // true branch should be a nested conditional type
    let true_node = arena.get(outer.true_type).expect("true branch");
    assert_eq!(
        true_node.kind,
        syntax_kind_ext::CONDITIONAL_TYPE,
        "true branch should be nested conditional"
    );
}

#[test]
fn type_mapped() {
    // `type T = { [K in keyof O]: O[K] }`
    let (parser, root) = parse_source("type T = { [K in keyof O]: O[K] };");
    assert_no_errors(&parser, "mapped type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::MAPPED_TYPE,
        "should be mapped type"
    );
    let mapped = arena.get_mapped_type(type_node).expect("mapped data");
    assert!(mapped.type_parameter.is_some(), "should have type param");
    assert!(mapped.type_node.is_some(), "should have type node");
}

#[test]
fn type_template_literal() {
    // type T = `${string}px`
    let (parser, root) = parse_source("type T = `${string}px`;");
    assert_no_errors(&parser, "template literal type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::TEMPLATE_LITERAL_TYPE,
        "should be template literal type"
    );
}

#[test]
fn type_tuple_with_labels() {
    // `type T = [name: string, age: number]`
    let (parser, root) = parse_source("type T = [name: string, age: number];");
    assert_no_errors(&parser, "tuple with labels");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::TUPLE_TYPE,
        "should be tuple type"
    );
    let tuple = arena.get_tuple_type(type_node).expect("tuple data");
    assert_eq!(tuple.elements.nodes.len(), 2, "should have 2 elements");
    // Each element should be a NamedTupleMember
    let elem = arena.get(tuple.elements.nodes[0]).expect("elem0");
    assert_eq!(
        elem.kind,
        syntax_kind_ext::NAMED_TUPLE_MEMBER,
        "should be named tuple member"
    );
}

#[test]
fn type_tuple_optional_element() {
    // `type T = [string, number?]`
    let (parser, root) = parse_source("type T = [string, number?];");
    assert_no_errors(&parser, "tuple optional element");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(type_node.kind, syntax_kind_ext::TUPLE_TYPE);
    let tuple = arena.get_tuple_type(type_node).expect("tuple");
    assert_eq!(tuple.elements.nodes.len(), 2);
    // Second element should be an OptionalType
    let elem1 = arena.get(tuple.elements.nodes[1]).expect("elem1");
    assert_eq!(
        elem1.kind,
        syntax_kind_ext::OPTIONAL_TYPE,
        "should be optional type"
    );
}

#[test]
fn type_tuple_rest_element() {
    // `type T = [string, ...number[]]`
    let (parser, root) = parse_source("type T = [string, ...number[]];");
    assert_no_errors(&parser, "tuple rest element");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(type_node.kind, syntax_kind_ext::TUPLE_TYPE);
    let tuple = arena.get_tuple_type(type_node).expect("tuple");
    assert_eq!(tuple.elements.nodes.len(), 2);
    // Second element should be a RestType
    let elem1 = arena.get(tuple.elements.nodes[1]).expect("elem1");
    assert_eq!(
        elem1.kind,
        syntax_kind_ext::REST_TYPE,
        "should be rest type"
    );
}

#[test]
fn type_infer_in_conditional() {
    // `type T = X extends Array<infer U> ? U : never`
    let (parser, root) = parse_source("type T = X extends Array<infer U> ? U : never;");
    assert_no_errors(&parser, "infer in conditional");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(type_node.kind, syntax_kind_ext::CONDITIONAL_TYPE);
    // The extends type should be a TypeReference with type arguments containing infer
    let cond = arena.get_conditional_type(type_node).expect("cond");
    let extends_node = arena.get(cond.extends_type).expect("extends");
    assert_eq!(extends_node.kind, syntax_kind_ext::TYPE_REFERENCE);
    let type_ref = arena.get_type_ref(extends_node).expect("type ref");
    let args = type_ref.type_arguments.as_ref().expect("type args");
    assert_eq!(args.nodes.len(), 1);
    let infer_node = arena.get(args.nodes[0]).expect("infer");
    assert_eq!(
        infer_node.kind,
        syntax_kind_ext::INFER_TYPE,
        "should be infer type"
    );
}

#[test]
fn type_index_access() {
    // `type T = Foo["key"]`
    let (parser, root) = parse_source("type T = Foo[\"key\"];");
    assert_no_errors(&parser, "index access type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::INDEXED_ACCESS_TYPE,
        "should be indexed access type"
    );
}

#[test]
fn type_index_access_number() {
    // `type T = Arr[number]`
    let (parser, root) = parse_source("type T = Arr[number];");
    assert_no_errors(&parser, "index access type number");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(type_node.kind, syntax_kind_ext::INDEXED_ACCESS_TYPE);
}

#[test]
fn type_typeof() {
    // `type T = typeof x`
    let (parser, root) = parse_source("type T = typeof x;");
    assert_no_errors(&parser, "typeof type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::TYPE_QUERY,
        "should be type query"
    );
}

#[test]
fn type_keyof() {
    // `type T = keyof X`
    let (parser, root) = parse_source("type T = keyof X;");
    assert_no_errors(&parser, "keyof type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::TYPE_OPERATOR,
        "should be type operator"
    );
    let op = arena.get_type_operator(type_node).expect("type operator");
    assert_eq!(
        op.operator,
        SyntaxKind::KeyOfKeyword as u16,
        "should be keyof"
    );
}

#[test]
fn type_function_type() {
    // `type T = (x: number) => string`
    let (parser, root) = parse_source("type T = (x: number) => string;");
    assert_no_errors(&parser, "function type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::FUNCTION_TYPE,
        "should be function type"
    );
    let func_type = arena.get_function_type(type_node).expect("func type data");
    assert_eq!(func_type.parameters.nodes.len(), 1);
    assert!(
        func_type.type_annotation.is_some(),
        "should have return type"
    );
}

#[test]
fn type_constructor_type() {
    // `type T = new (x: number) => Foo`
    let (parser, root) = parse_source("type T = new (x: number) => Foo;");
    assert_no_errors(&parser, "constructor type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::CONSTRUCTOR_TYPE,
        "should be constructor type"
    );
}

#[test]
fn type_array() {
    // `type T = number[]`
    let (parser, root) = parse_source("type T = number[];");
    assert_no_errors(&parser, "array type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::ARRAY_TYPE,
        "should be array type"
    );
}

#[test]
fn type_parenthesized() {
    // `type T = (A | B) & C`
    let (parser, root) = parse_source("type T = (A | B) & C;");
    assert_no_errors(&parser, "parenthesized type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::INTERSECTION_TYPE,
        "top should be intersection"
    );
}

#[test]
fn type_readonly_array() {
    // `type T = readonly number[]`
    let (parser, root) = parse_source("type T = readonly number[];");
    assert_no_errors(&parser, "readonly array");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::TYPE_OPERATOR,
        "should be type operator (readonly)"
    );
    let op = arena.get_type_operator(type_node).expect("type op");
    assert_eq!(
        op.operator,
        SyntaxKind::ReadonlyKeyword as u16,
        "should be readonly"
    );
}

#[test]
fn type_this() {
    // `interface I { get(): this }`
    let (parser, root) = parse_source("interface I { get(): this; }");
    assert_no_errors(&parser, "this type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let iface = arena.get_interface(stmt_node).expect("interface");
    let member_node = arena.get(iface.members.nodes[0]).expect("member");
    let sig = arena.get_signature(member_node).expect("signature");
    let ret_node = arena.get(sig.type_annotation).expect("return type");
    assert_eq!(
        ret_node.kind,
        syntax_kind_ext::THIS_TYPE,
        "should be this type"
    );
}

#[test]
fn type_literal_string() {
    // `type T = "hello"`
    let (parser, root) = parse_source("type T = \"hello\";");
    assert_no_errors(&parser, "literal type string");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::LITERAL_TYPE,
        "should be literal type"
    );
}

// =============================================================================
// 4. Declaration Edge Cases (10+ tests)
// =============================================================================

#[test]
fn decl_export_default_function_anonymous() {
    // `export default function() {}` — wraps function in export declaration
    let (parser, root) = parse_source("export default function() {}");
    assert_no_errors(&parser, "export default function anonymous");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::EXPORT_DECLARATION,
        "should be export declaration wrapping the function"
    );
    let export = arena.get_export_decl(stmt_node).expect("export decl");
    assert!(export.is_default_export, "should be default export");
}

#[test]
fn decl_export_as_default() {
    // `export { x as default }`
    let (parser, root) = parse_source("const x = 1; export { x as default };");
    assert_no_errors(&parser, "export { x as default }");
    let arena = parser.get_arena();
    let stmts = get_statements(arena, root);
    assert_eq!(stmts.len(), 2);
    let export_node = arena.get(stmts[1]).expect("export");
    assert_eq!(export_node.kind, syntax_kind_ext::EXPORT_DECLARATION);
}

#[test]
fn decl_import_type() {
    // `import type { Foo } from 'bar'`
    let (parser, root) = parse_source("import type { Foo } from 'bar';");
    assert_no_errors(&parser, "import type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(stmt_node.kind, syntax_kind_ext::IMPORT_DECLARATION);
    let import = arena.get_import_decl(stmt_node).expect("import decl");
    let clause_node = arena.get(import.import_clause).expect("clause");
    let clause = arena.get_import_clause(clause_node).expect("import clause");
    assert!(clause.is_type_only, "should be type-only import");
}

#[test]
fn decl_declare_module_string_literal() {
    // `declare module 'foo' {}`
    let (parser, root) = parse_source("declare module 'foo' {}");
    assert_no_errors(&parser, "declare module");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::MODULE_DECLARATION,
        "should be module declaration"
    );
}

#[test]
fn decl_ambient_function() {
    // `declare function f(): void`
    let (parser, root) = parse_source("declare function f(): void;");
    assert_no_errors(&parser, "ambient function");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(stmt_node.kind, syntax_kind_ext::FUNCTION_DECLARATION);
    let func = arena.get_function(stmt_node).expect("function");
    assert!(func.body.is_none(), "ambient function should have no body");
}

#[test]
fn decl_enum_basic() {
    // `enum Color { Red, Green, Blue }`
    let (parser, root) = parse_source("enum Color { Red, Green, Blue }");
    assert_no_errors(&parser, "enum");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(stmt_node.kind, syntax_kind_ext::ENUM_DECLARATION);
    let enum_data = arena.get_enum(stmt_node).expect("enum");
    assert_eq!(enum_data.members.nodes.len(), 3, "should have 3 members");
}

#[test]
fn decl_enum_with_initializers() {
    // `enum Dir { Up = 1, Down = 2 }`
    let (parser, root) = parse_source("enum Dir { Up = 1, Down = 2 }");
    assert_no_errors(&parser, "enum with initializers");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let enum_data = arena.get_enum(stmt_node).expect("enum");
    let member_node = arena.get(enum_data.members.nodes[0]).expect("member");
    let member = arena.get_enum_member(member_node).expect("member data");
    assert!(member.initializer.is_some(), "should have initializer");
}

#[test]
fn decl_const_enum() {
    // `const enum Flags { A, B }`
    let (parser, root) = parse_source("const enum Flags { A, B }");
    assert_no_errors(&parser, "const enum");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(stmt_node.kind, syntax_kind_ext::ENUM_DECLARATION);
}

#[test]
fn decl_namespace() {
    // `namespace Foo { export const x = 1; }`
    let (parser, root) = parse_source("namespace Foo { export const x = 1; }");
    assert_no_errors(&parser, "namespace");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(stmt_node.kind, syntax_kind_ext::MODULE_DECLARATION);
}

#[test]
fn decl_export_equals() {
    // `export = x`
    let (parser, root) = parse_source("export = x;");
    assert_no_errors(&parser, "export equals");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::EXPORT_ASSIGNMENT,
        "should be export assignment"
    );
    let export = arena.get_export_assignment(stmt_node).expect("export");
    assert!(export.is_export_equals, "should be export =");
}

#[test]
fn decl_export_default_expression() {
    // `export default 42` — parsed as EXPORT_DECLARATION with default flag
    let (parser, root) = parse_source("export default 42;");
    assert_no_errors(&parser, "export default expression");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::EXPORT_DECLARATION,
        "should be export declaration"
    );
    let export = arena.get_export_decl(stmt_node).expect("export decl");
    assert!(export.is_default_export, "should be default export");
}

#[test]
fn decl_interface_with_extends() {
    // `interface Foo extends Bar, Baz { x: number; }`
    let (parser, root) = parse_source("interface Foo extends Bar, Baz { x: number; }");
    assert_no_errors(&parser, "interface extends");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(stmt_node.kind, syntax_kind_ext::INTERFACE_DECLARATION);
    let iface = arena.get_interface(stmt_node).expect("interface");
    assert!(iface.heritage_clauses.is_some(), "should have extends");
    assert_eq!(iface.members.nodes.len(), 1, "should have 1 member");
}

#[test]
fn decl_type_alias_generic() {
    // `type Box<T> = { value: T }`
    let (parser, root) = parse_source("type Box<T> = { value: T };");
    assert_no_errors(&parser, "generic type alias");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(stmt_node.kind, syntax_kind_ext::TYPE_ALIAS_DECLARATION);
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    assert!(
        alias.type_parameters.is_some(),
        "should have type parameters"
    );
}

// =============================================================================
// 5. Class Syntax (10+ tests)
// =============================================================================

#[test]
fn class_basic() {
    // `class Foo { x: number; }`
    let (parser, root) = parse_source("class Foo { x: number; }");
    assert_no_errors(&parser, "basic class");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(stmt_node.kind, syntax_kind_ext::CLASS_DECLARATION);
    let class = arena.get_class(stmt_node).expect("class");
    assert_eq!(class.members.nodes.len(), 1, "should have 1 member");
}

#[test]
fn class_private_field() {
    // `class Foo { #x: number; }`
    let (parser, root) = parse_source("class Foo { #x: number; }");
    assert_no_errors(&parser, "private field");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let member_node = arena.get(class.members.nodes[0]).expect("member");
    assert_eq!(
        member_node.kind,
        syntax_kind_ext::PROPERTY_DECLARATION,
        "should be property declaration"
    );
    let prop = arena.get_property_decl(member_node).expect("prop");
    let name_node = arena.get(prop.name).expect("name");
    assert_eq!(
        name_node.kind,
        SyntaxKind::PrivateIdentifier as u16,
        "should be private identifier"
    );
}

#[test]
fn class_static_block() {
    // `class Foo { static { console.log("init"); } }`
    let (parser, root) = parse_source("class Foo { static { console.log(\"init\"); } }");
    assert_no_errors(&parser, "static block");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    assert!(!class.members.nodes.is_empty(), "should have members");
    let member_node = arena.get(class.members.nodes[0]).expect("member");
    assert_eq!(
        member_node.kind,
        syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION,
        "should be static block"
    );
}

#[test]
fn class_abstract_method() {
    // `abstract class Foo { abstract bar(): void; }`
    let (parser, root) = parse_source("abstract class Foo { abstract bar(): void; }");
    assert_no_errors(&parser, "abstract method");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let member_node = arena.get(class.members.nodes[0]).expect("member");
    assert_eq!(
        member_node.kind,
        syntax_kind_ext::METHOD_DECLARATION,
        "should be method"
    );
    let method = arena.get_method_decl(member_node).expect("method");
    assert!(
        arena.has_modifier(&method.modifiers, SyntaxKind::AbstractKeyword),
        "should have abstract modifier"
    );
}

#[test]
fn class_parameter_property() {
    // `class Foo { constructor(public x: number) {} }`
    let (parser, root) = parse_source("class Foo { constructor(public x: number) {} }");
    assert_no_errors(&parser, "parameter property");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let ctor_node = arena.get(class.members.nodes[0]).expect("ctor");
    assert_eq!(
        ctor_node.kind,
        syntax_kind_ext::CONSTRUCTOR,
        "should be constructor"
    );
    let ctor = arena.get_constructor(ctor_node).expect("ctor data");
    let param_node = arena.get(ctor.parameters.nodes[0]).expect("param");
    let param = arena.get_parameter(param_node).expect("param data");
    assert!(
        arena.has_modifier(&param.modifiers, SyntaxKind::PublicKeyword),
        "should have public modifier"
    );
}

#[test]
fn class_decorator() {
    // `@dec class Foo {}`
    let (parser, root) = parse_source("declare var dec: any; @dec class Foo {}");
    assert_no_errors(&parser, "class decorator");
    let arena = parser.get_arena();
    let stmts = get_statements(arena, root);
    let class_node = arena.get(stmts[1]).expect("class node");
    assert_eq!(class_node.kind, syntax_kind_ext::CLASS_DECLARATION);
    let class = arena.get_class(class_node).expect("class");
    // Modifiers should include a decorator
    let mods = class.modifiers.as_ref().expect("modifiers");
    let has_decorator = mods.nodes.iter().any(|&idx| {
        arena
            .get(idx)
            .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
    });
    assert!(has_decorator, "should have decorator modifier");
}

#[test]
fn class_multiple_decorators() {
    // `@a @b class Foo {}`
    let (parser, root) = parse_source("declare var a: any; declare var b: any; @a @b class Foo {}");
    assert_no_errors(&parser, "multiple decorators");
    let arena = parser.get_arena();
    let stmts = get_statements(arena, root);
    let class_node = arena.get(stmts[2]).expect("class node");
    let class = arena.get_class(class_node).expect("class");
    let mods = class.modifiers.as_ref().expect("modifiers");
    let decorator_count = mods
        .nodes
        .iter()
        .filter(|&&idx| {
            arena
                .get(idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
        })
        .count();
    assert_eq!(decorator_count, 2, "should have 2 decorators");
}

#[test]
fn class_index_signature() {
    // `class Foo { [key: string]: number; }`
    let (parser, root) = parse_source("class Foo { [key: string]: number; }");
    assert_no_errors(&parser, "class index signature");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let member_node = arena.get(class.members.nodes[0]).expect("member");
    assert_eq!(
        member_node.kind,
        syntax_kind_ext::INDEX_SIGNATURE,
        "should be index signature"
    );
}

#[test]
fn class_computed_property() {
    // `class Foo { [Symbol.iterator]() {} }`
    let (parser, root) = parse_source("class Foo { [Symbol.iterator]() {} }");
    assert_no_errors(&parser, "computed property name");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let member_node = arena.get(class.members.nodes[0]).expect("member");
    assert_eq!(
        member_node.kind,
        syntax_kind_ext::METHOD_DECLARATION,
        "should be method"
    );
    let method = arena.get_method_decl(member_node).expect("method");
    let name_node = arena.get(method.name).expect("name");
    assert_eq!(
        name_node.kind,
        syntax_kind_ext::COMPUTED_PROPERTY_NAME,
        "name should be computed property"
    );
}

#[test]
fn class_getter_setter() {
    // `class Foo { get x() { return 1; } set x(v: number) {} }`
    let (parser, root) = parse_source("class Foo { get x() { return 1; } set x(v: number) {} }");
    assert_no_errors(&parser, "getter/setter");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    assert_eq!(class.members.nodes.len(), 2, "should have getter + setter");
    let getter = arena.get(class.members.nodes[0]).expect("getter");
    assert_eq!(
        getter.kind,
        syntax_kind_ext::GET_ACCESSOR,
        "first should be getter"
    );
    let setter = arena.get(class.members.nodes[1]).expect("setter");
    assert_eq!(
        setter.kind,
        syntax_kind_ext::SET_ACCESSOR,
        "second should be setter"
    );
}

#[test]
fn class_extends_implements() {
    // `class Foo extends Bar implements Baz {}`
    let (parser, root) = parse_source("class Foo extends Bar implements Baz {}");
    assert_no_errors(&parser, "extends implements");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let heritage = class.heritage_clauses.as_ref().expect("heritage clauses");
    assert_eq!(
        heritage.nodes.len(),
        2,
        "should have extends + implements clauses"
    );
}

#[test]
fn class_duplicate_extends_recovery_discards_duplicate_clause_types() {
    let (parser, root) = parse_source("class C extends A extends B {}");
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::EXTENDS_CLAUSE_ALREADY_SEEN),
        "expected TS1172 for duplicate extends clause, got {:?}",
        parser.get_diagnostics()
    );

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let heritage = class.heritage_clauses.as_ref().expect("heritage clauses");
    assert_eq!(
        heritage.nodes.len(),
        1,
        "duplicate extends recovery should keep only the first heritage clause"
    );

    let clause_node = arena.get(heritage.nodes[0]).expect("heritage node");
    let clause = arena.get_heritage(clause_node).expect("heritage data");
    assert_eq!(
        clause.types.nodes.len(),
        1,
        "duplicate extends recovery should keep only the first base type"
    );
}

#[test]
fn class_duplicate_implements_recovery_discards_duplicate_clause_types() {
    let (parser, root) = parse_source("class C implements A implements B {}");
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::IMPLEMENTS_CLAUSE_ALREADY_SEEN),
        "expected TS1175 for duplicate implements clause, got {:?}",
        parser.get_diagnostics()
    );

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let heritage = class.heritage_clauses.as_ref().expect("heritage clauses");
    assert_eq!(
        heritage.nodes.len(),
        1,
        "duplicate implements recovery should keep only the first heritage clause"
    );

    let clause_node = arena.get(heritage.nodes[0]).expect("heritage node");
    let clause = arena.get_heritage(clause_node).expect("heritage data");
    assert_eq!(
        clause.types.nodes.len(),
        1,
        "duplicate implements recovery should keep only the first implemented type"
    );
}

#[test]
fn class_extends_comma_recovery_keeps_single_base_type() {
    let source = "class C extends A, B {}";
    let (parser, root) = parse_source(source);
    let diags = parser.get_diagnostics();
    let ts1174 = diags
        .iter()
        .find(|diag| diag.code == diagnostic_codes::CLASSES_CAN_ONLY_EXTEND_A_SINGLE_CLASS)
        .expect("expected TS1174 for comma-separated extends");
    let b_pos = source.find('B').expect("B position") as u32;
    assert_eq!(
        ts1174.start, b_pos,
        "TS1174 should point at the extra base type, got {diags:?}"
    );

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let heritage = class.heritage_clauses.as_ref().expect("heritage clauses");
    assert_eq!(
        heritage.nodes.len(),
        1,
        "comma extends recovery should keep a single heritage clause"
    );

    let clause_node = arena.get(heritage.nodes[0]).expect("heritage node");
    let clause = arena.get_heritage(clause_node).expect("heritage data");
    assert_eq!(
        clause.types.nodes.len(),
        2,
        "comma extends recovery should preserve all base types for emit (matching tsc)"
    );
}

#[test]
fn class_out_of_order_extends_recovery_keeps_trailing_clause() {
    let (parser, root) = parse_source("class C implements A extends B {}");
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::EXTENDS_CLAUSE_MUST_PRECEDE_IMPLEMENTS_CLAUSE),
        "expected TS1173 for out-of-order extends clause, got {:?}",
        parser.get_diagnostics()
    );

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let heritage = class.heritage_clauses.as_ref().expect("heritage clauses");
    assert_eq!(
        heritage.nodes.len(),
        2,
        "out-of-order extends recovery should keep both heritage clauses"
    );
}

#[test]
fn class_extends_object_literal_recovery_keeps_body_and_uses_ts1005() {
    let source = "class C extends { foo: string; } { method() {} }";
    let (parser, root) = parse_source(source);
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::EXPECTED),
        "expected TS1005 from the object-literal separator recovery, got {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(&diagnostic_codes::LIST_CANNOT_BE_EMPTY),
        "should not treat the object literal as an empty extends list, got {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(
            &diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED
        ),
        "should not spill the heritage literal into class-member parsing, got {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "should not emit TS1109 for object literal bases, got {:?}",
        parser.get_diagnostics()
    );

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let heritage = class.heritage_clauses.as_ref().expect("heritage clauses");
    assert_eq!(
        heritage.nodes.len(),
        1,
        "should keep a single extends clause"
    );
    let clause_node = arena.get(heritage.nodes[0]).expect("heritage node");
    let clause = arena.get_heritage(clause_node).expect("heritage data");
    assert_eq!(
        clause.types.nodes.len(),
        1,
        "should keep one base expression"
    );
    let base_node = arena.get(clause.types.nodes[0]).expect("base");
    assert_eq!(
        base_node.kind,
        syntax_kind_ext::OBJECT_LITERAL_EXPRESSION,
        "extends base should recover as an object literal expression"
    );
    assert_eq!(
        class.members.nodes.len(),
        1,
        "class body should still parse"
    );
}

#[test]
fn class_extends_array_literal_expression_keeps_body() {
    let source = "class C extends [] { method() {} }";
    let (parser, root) = parse_source(source);
    assert_no_errors(&parser, "class extends array literal");

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let heritage = class.heritage_clauses.as_ref().expect("heritage clauses");
    assert_eq!(
        heritage.nodes.len(),
        1,
        "should keep a single extends clause"
    );
    let clause_node = arena.get(heritage.nodes[0]).expect("heritage node");
    let clause = arena.get_heritage(clause_node).expect("heritage data");
    assert_eq!(
        clause.types.nodes.len(),
        1,
        "should keep one base expression"
    );
    let base_node = arena.get(clause.types.nodes[0]).expect("base");
    assert_eq!(
        base_node.kind,
        syntax_kind_ext::ARRAY_LITERAL_EXPRESSION,
        "extends base should recover as an array literal expression"
    );
    assert_eq!(
        class.members.nodes.len(),
        1,
        "class body should still parse"
    );
}

#[test]
fn class_extends_void_emits_ts1109_and_preserves_body() {
    let (parser, root) = parse_source("class C extends void { method() {} }");
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "expected TS1109 for `extends void`, got {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(&diagnostic_codes::LIST_CANNOT_BE_EMPTY),
        "should not treat `void` as an empty extends list, got {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(
            &diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED
        ),
        "should not spill `void` into class-member parsing, got {:?}",
        parser.get_diagnostics()
    );

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    assert_eq!(
        class.members.nodes.len(),
        1,
        "class body should still parse"
    );
}

#[test]
fn class_empty_extends_list_still_reports_ts1097() {
    let (parser, _root) = parse_source("class C extends { }");
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::LIST_CANNOT_BE_EMPTY),
        "expected TS1097 for an empty extends list, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn interface_extends_array_literal_reports_interface_heritage_error() {
    let (parser, _root) = parse_source("interface I extends [] {}");
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(
            &diagnostic_codes::AN_INTERFACE_CAN_ONLY_EXTEND_AN_IDENTIFIER_QUALIFIED_NAME_WITH_OPTIONAL_TYPE_ARG
        ),
        "expected the interface-specific heritage diagnostic, got {:?}",
        parser.get_diagnostics()
    );
}

// =============================================================================
// 6. Statement Edge Cases (8+ tests)
// =============================================================================

#[test]
fn stmt_labeled() {
    // `label: for (;;) {}`
    let (parser, root) = parse_source("label: for (;;) {}");
    assert_no_errors(&parser, "labeled statement");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::LABELED_STATEMENT,
        "should be labeled"
    );
    let labeled = arena.get_labeled_statement(stmt_node).expect("labeled");
    let inner = arena.get(labeled.statement).expect("inner");
    assert_eq!(
        inner.kind,
        syntax_kind_ext::FOR_STATEMENT,
        "body should be for"
    );
}

#[test]
fn stmt_for_await_of() {
    // `for await (const x of iter) {}`
    let (parser, root) = parse_source("async function f() { for await (const x of iter) {} }");
    assert_no_errors(&parser, "for await of");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let func_node = arena.get(stmt_idx).expect("func");
    let func = arena.get_function(func_node).expect("func data");
    let body_node = arena.get(func.body).expect("body");
    let block = arena.get_block(body_node).expect("block");
    let for_node = arena.get(block.statements.nodes[0]).expect("for");
    assert_eq!(
        for_node.kind,
        syntax_kind_ext::FOR_OF_STATEMENT,
        "should be for-of"
    );
    let for_data = arena.get_for_in_of(for_node).expect("for data");
    assert!(for_data.await_modifier, "should have await modifier");
}

#[test]
fn stmt_switch_with_fallthrough() {
    // Switch with fallthrough
    let (parser, root) = parse_source("switch (x) { case 1: case 2: break; default: break; }");
    assert_no_errors(&parser, "switch with fallthrough");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(stmt_node.kind, syntax_kind_ext::SWITCH_STATEMENT);
}

#[test]
fn stmt_try_catch_finally() {
    // `try {} catch (e) {} finally {}`
    let (parser, root) = parse_source("try {} catch (e) {} finally {}");
    assert_no_errors(&parser, "try/catch/finally");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(stmt_node.kind, syntax_kind_ext::TRY_STATEMENT);
    let try_data = arena.get_try(stmt_node).expect("try data");
    assert!(try_data.try_block.is_some(), "should have try block");
    assert!(try_data.catch_clause.is_some(), "should have catch clause");
    assert!(
        try_data.finally_block.is_some(),
        "should have finally block"
    );
}

#[test]
fn stmt_try_finally_no_catch() {
    // `try {} finally {}`
    let (parser, root) = parse_source("try {} finally {}");
    assert_no_errors(&parser, "try/finally no catch");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let try_data = arena.get_try(stmt_node).expect("try data");
    assert!(try_data.catch_clause.is_none(), "should have no catch");
    assert!(try_data.finally_block.is_some(), "should have finally");
}

#[test]
fn stmt_catch_without_binding() {
    // `try {} catch {}`  (ES2019 optional catch binding)
    let (parser, root) = parse_source("try {} catch {}");
    assert_no_errors(&parser, "catch without binding");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let try_data = arena.get_try(stmt_node).expect("try data");
    let catch_node = arena.get(try_data.catch_clause).expect("catch");
    let catch = arena.get_catch_clause(catch_node).expect("catch data");
    assert!(
        catch.variable_declaration.is_none(),
        "should have no binding"
    );
}

#[test]
fn stmt_with() {
    // `with (obj) { x; }` (legacy)
    let (parser, root) = parse_source("with (obj) { x; }");
    assert_no_errors(&parser, "with statement");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::WITH_STATEMENT,
        "should be with statement"
    );
}

#[test]
fn stmt_empty() {
    // `;` (empty statement)
    let (parser, root) = parse_source(";");
    assert_no_errors(&parser, "empty statement");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::EMPTY_STATEMENT,
        "should be empty statement"
    );
}

#[test]
fn stmt_debugger() {
    // `debugger;`
    let (parser, root) = parse_source("debugger;");
    assert_no_errors(&parser, "debugger statement");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::DEBUGGER_STATEMENT,
        "should be debugger"
    );
}

#[test]
fn stmt_for_in() {
    // `for (const k in obj) {}`
    let (parser, root) = parse_source("for (const k in obj) {}");
    assert_no_errors(&parser, "for-in");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::FOR_IN_STATEMENT,
        "should be for-in"
    );
}

#[test]
fn stmt_for_of() {
    // `for (const x of arr) {}`
    let (parser, root) = parse_source("for (const x of arr) {}");
    assert_no_errors(&parser, "for-of");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::FOR_OF_STATEMENT,
        "should be for-of"
    );
}

#[test]
fn stmt_do_while() {
    // `do { x++; } while (x < 10);`
    let (parser, root) = parse_source("do { x++; } while (x < 10);");
    assert_no_errors(&parser, "do-while");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::DO_STATEMENT,
        "should be do-while"
    );
}

#[test]
fn stmt_break_continue_with_label() {
    // `outer: for (;;) { inner: for (;;) { break outer; continue inner; } }`
    let (parser, root) =
        parse_source("outer: for (;;) { inner: for (;;) { break outer; continue inner; } }");
    assert_no_errors(&parser, "break/continue with label");
    // The existence of no errors proves the parser handles labeled break/continue
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).expect("source file");
    assert!(!sf.statements.nodes.is_empty());
}

// =============================================================================
// 7. Error Recovery (5+ tests)
// =============================================================================

#[test]
fn error_recovery_missing_semicolon_asi() {
    // ASI should insert semicolons
    let (parser, root) = parse_source("const x = 1\nconst y = 2");
    assert_no_errors(&parser, "ASI");
    let arena = parser.get_arena();
    let stmts = get_statements(arena, root);
    assert_eq!(stmts.len(), 2, "should have 2 statements via ASI");
}

#[test]
fn error_recovery_missing_closing_brace() {
    // Missing closing brace should not panic
    let (parser, root) = parse_source("function f() { const x = 1;");
    assert_has_errors(&parser, "missing closing brace");
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).expect("source file");
    assert!(
        !sf.statements.nodes.is_empty(),
        "should still produce some statements"
    );
}

#[test]
fn error_recovery_unexpected_token() {
    // Unexpected token should not panic
    let (parser, _) = parse_source("const x = @@@;");
    assert_has_errors(&parser, "unexpected token");
}

#[test]
fn error_recovery_multiple_errors_continue_parsing() {
    // Multiple errors — parser should recover and continue
    let (parser, root) = parse_source("const x = ; const y = 2; const z = ;");
    let arena = parser.get_arena();
    let stmts = get_statements(arena, root);
    // Should at least parse `const y = 2` properly
    assert!(
        stmts.len() >= 2,
        "parser should recover and parse subsequent statements"
    );
}

#[test]
fn error_recovery_no_panic_on_empty_input() {
    // Empty input should not panic
    let (parser, root) = parse_source("");
    assert_no_errors(&parser, "empty input");
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).expect("source file");
    assert!(sf.statements.nodes.is_empty(), "should have no statements");
}

#[test]
fn error_recovery_no_panic_on_only_whitespace() {
    let (parser, root) = parse_source("   \n\n  \t  ");
    assert_no_errors(&parser, "whitespace only");
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).expect("source file");
    assert!(sf.statements.nodes.is_empty());
}

#[test]
fn error_recovery_deeply_nested_parens() {
    // Deeply nested parentheses should not overflow the stack
    let depth = 100;
    let mut source = String::new();
    source.push_str("const x = ");
    for _ in 0..depth {
        source.push('(');
    }
    source.push('1');
    for _ in 0..depth {
        source.push(')');
    }
    source.push(';');
    let (parser, _root) = parse_source(&source);
    // Should not panic; may or may not have errors depending on recursion limits
    let _ = parser.get_diagnostics();
}

// =============================================================================
// 8. Expression Miscellaneous Tests
// =============================================================================

#[test]
fn expr_new_expression() {
    // `new Foo(1, 2)`
    let (parser, root) = parse_source("const x = new Foo(1, 2);");
    assert_no_errors(&parser, "new expression");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::NEW_EXPRESSION,
        "should be new expression"
    );
    let call = arena.get_call_expr(node).expect("call data");
    let args = call.arguments.as_ref().expect("args");
    assert_eq!(args.nodes.len(), 2, "should have 2 arguments");
}

#[test]
fn expr_new_without_parens() {
    // `new Foo` (without arguments)
    let (parser, root) = parse_source("const x = new Foo;");
    assert_no_errors(&parser, "new without parens");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::NEW_EXPRESSION);
}

#[test]
fn expr_tagged_template() {
    // tag`hello ${x} world`
    let (parser, root) = parse_source("const x = tag`hello ${y} world`;");
    assert_no_errors(&parser, "tagged template");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION,
        "should be tagged template"
    );
}

#[test]
fn expr_spread_element() {
    // `[1, ...arr, 2]`
    let (parser, root) = parse_source("const x = [1, ...arr, 2];");
    assert_no_errors(&parser, "spread element");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::ARRAY_LITERAL_EXPRESSION,
        "should be array literal"
    );
    let lit_expr = arena.get_literal_expr(node).expect("array lit");
    assert_eq!(lit_expr.elements.nodes.len(), 3, "should have 3 elements");
    let spread = arena.get(lit_expr.elements.nodes[1]).expect("spread");
    assert_eq!(
        spread.kind,
        syntax_kind_ext::SPREAD_ELEMENT,
        "middle should be spread"
    );
}

#[test]
fn expr_object_literal() {
    // `{ a: 1, b, ...c, [d]: 2 }`
    let (parser, root) = parse_source("const x = { a: 1, b, ...c, [d]: 2 };");
    assert_no_errors(&parser, "object literal");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::OBJECT_LITERAL_EXPRESSION);
    let lit_expr = arena.get_literal_expr(node).expect("object lit");
    assert_eq!(
        lit_expr.elements.nodes.len(),
        4,
        "should have 4 properties/spread"
    );
    // Verify different property types
    let prop_a = arena.get(lit_expr.elements.nodes[0]).expect("a");
    assert_eq!(prop_a.kind, syntax_kind_ext::PROPERTY_ASSIGNMENT);
    let prop_b = arena.get(lit_expr.elements.nodes[1]).expect("b");
    assert_eq!(prop_b.kind, syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT);
    let prop_c = arena.get(lit_expr.elements.nodes[2]).expect("c");
    assert_eq!(prop_c.kind, syntax_kind_ext::SPREAD_ASSIGNMENT);
    let prop_d = arena.get(lit_expr.elements.nodes[3]).expect("d");
    assert_eq!(prop_d.kind, syntax_kind_ext::PROPERTY_ASSIGNMENT);
}

#[test]
fn expr_yield() {
    // `function* gen() { yield 1; yield* other(); }`
    let (parser, _) = parse_source("function* gen() { yield 1; yield* other(); }");
    assert_no_errors(&parser, "yield expression");
}

#[test]
fn expr_void_typeof_delete() {
    // `void 0; typeof x; delete obj.x;`
    // These are parsed as PREFIX_UNARY_EXPRESSION with the keyword as operator
    let (parser, root) = parse_source("void 0; typeof x; delete obj.x;");
    assert_no_errors(&parser, "void/typeof/delete");
    let arena = parser.get_arena();
    let stmts = get_statements(arena, root);
    assert_eq!(stmts.len(), 3);

    let s0 = arena.get(stmts[0]).expect("s0");
    let e0 = arena.get_expression_statement(s0).expect("es0");
    let void_node = arena.get(e0.expression).expect("void");
    assert_eq!(
        void_node.kind,
        syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
        "void should be prefix unary"
    );
    let void_unary = arena.get_unary_expr(void_node).expect("void unary");
    assert_eq!(void_unary.operator, SyntaxKind::VoidKeyword as u16);

    let s1 = arena.get(stmts[1]).expect("s1");
    let e1 = arena.get_expression_statement(s1).expect("es1");
    let typeof_node = arena.get(e1.expression).expect("typeof");
    assert_eq!(
        typeof_node.kind,
        syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
        "typeof should be prefix unary"
    );
    let typeof_unary = arena.get_unary_expr(typeof_node).expect("typeof unary");
    assert_eq!(typeof_unary.operator, SyntaxKind::TypeOfKeyword as u16);

    let s2 = arena.get(stmts[2]).expect("s2");
    let e2 = arena.get_expression_statement(s2).expect("es2");
    let delete_node = arena.get(e2.expression).expect("delete");
    assert_eq!(
        delete_node.kind,
        syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
        "delete should be prefix unary"
    );
    let delete_unary = arena.get_unary_expr(delete_node).expect("delete unary");
    assert_eq!(delete_unary.operator, SyntaxKind::DeleteKeyword as u16);
}

#[test]
fn expr_prefix_postfix_unary() {
    // `++x; x--;`
    let (parser, root) = parse_source("++x; x--;");
    assert_no_errors(&parser, "prefix/postfix unary");
    let arena = parser.get_arena();
    let stmts = get_statements(arena, root);
    let s0 = arena.get(stmts[0]).expect("s0");
    let e0 = arena.get_expression_statement(s0).expect("es0");
    let pre = arena.get(e0.expression).expect("prefix");
    assert_eq!(pre.kind, syntax_kind_ext::PREFIX_UNARY_EXPRESSION);

    let s1 = arena.get(stmts[1]).expect("s1");
    let e1 = arena.get_expression_statement(s1).expect("es1");
    let post = arena.get(e1.expression).expect("postfix");
    assert_eq!(post.kind, syntax_kind_ext::POSTFIX_UNARY_EXPRESSION);
}

#[test]
fn expr_prefix_update_delete_recovery_drops_outer_update() {
    let source = "++ delete foo.bar;";
    let (parser, root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let ts1109 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .expect("expected TS1109 for invalid prefix update operand");
    assert_eq!(
        ts1109.start,
        source.find("delete").expect("delete") as u32,
        "TS1109 should anchor at `delete`: {diagnostics:?}"
    );

    let arena = parser.get_arena();
    let stmt = arena.get(get_first_statement(arena, root)).expect("stmt");
    let expr_stmt = arena
        .get_expression_statement(stmt)
        .expect("expression statement");
    let expr = arena.get(expr_stmt.expression).expect("expression");
    assert_eq!(
        expr.kind,
        syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
        "recovered expression should still be unary"
    );
    let unary = arena.get_unary_expr(expr).expect("unary expression");
    assert_eq!(
        unary.operator,
        SyntaxKind::DeleteKeyword as u16,
        "outer prefix update should be dropped during recovery"
    );
}

#[test]
fn expr_prefix_update_repeated_operator_recovers_to_inner_update() {
    let source = "++\n++y;";
    let (parser, root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let ts1109 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .expect("expected TS1109 for repeated prefix update");
    assert_eq!(
        ts1109.start,
        source.find("\n++").expect("inner update") as u32 + 1,
        "TS1109 should anchor at the inner `++`: {diagnostics:?}"
    );

    let arena = parser.get_arena();
    let stmt = arena.get(get_first_statement(arena, root)).expect("stmt");
    let expr_stmt = arena
        .get_expression_statement(stmt)
        .expect("expression statement");
    let expr = arena.get(expr_stmt.expression).expect("expression");
    assert_eq!(
        expr.kind,
        syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
        "recovered expression should keep the inner prefix update"
    );
    let unary = arena.get_unary_expr(expr).expect("unary expression");
    assert_eq!(unary.operator, SyntaxKind::PlusPlusToken as u16);
    let operand = arena.get(unary.operand).expect("inner operand");
    assert_eq!(
        operand.kind,
        SyntaxKind::Identifier as u16,
        "inner update should still target the identifier"
    );
}

#[test]
fn expr_prefix_update_repeated_operator_same_line_anchors_inner_update() {
    let source = "++++y;";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let ts1109 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .expect("expected TS1109 for repeated prefix update");
    assert_eq!(
        ts1109.start, 2,
        "TS1109 should anchor at the second `++`: {diagnostics:?}"
    );
}

#[test]
fn object_spread_invalid_asterisk_recovers_to_operand_expression() {
    let source = "let o8 = { ...*o };";
    let (parser, root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let ts1109 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .expect("expected TS1109 for invalid spread operand");
    assert_eq!(
        ts1109.start,
        source.find('*').expect("asterisk") as u32,
        "TS1109 should anchor at the stray `*`: {diagnostics:?}"
    );

    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let object = arena.get(init).expect("object literal");
    let object_data = arena
        .get_literal_expr(object)
        .expect("object literal expression");
    assert_eq!(
        object_data.elements.nodes.len(),
        1,
        "expected one spread property"
    );

    let spread = arena
        .get(object_data.elements.nodes[0])
        .expect("spread assignment");
    assert_eq!(spread.kind, syntax_kind_ext::SPREAD_ASSIGNMENT);
    let spread_data = arena.get_spread(spread).expect("spread data");
    let operand = arena.get(spread_data.expression).expect("spread operand");
    assert_eq!(
        operand.kind,
        SyntaxKind::Identifier as u16,
        "recovery should keep the identifier operand after skipping the stray `*`"
    );
    assert_eq!(node_text(arena, source, spread_data.expression), "o");
}

#[test]
fn expr_prefix_update_repeated_operator_after_line_break_matches_sputnik_anchor() {
    let source = "var x=0, y=0;\nvar z=\nx\n++\n++\ny\n";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let ts1109 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .expect("expected TS1109 for repeated prefix update after line break");
    assert_eq!(
        ts1109.start,
        source.rfind("\n++\n").expect("second update line") as u32 + 1,
        "TS1109 should anchor at the second `++`: {diagnostics:?}"
    );
}

#[test]
fn expr_element_access() {
    // `a[0]`
    let (parser, root) = parse_source("const x = a[0];");
    assert_no_errors(&parser, "element access");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION,
        "should be element access"
    );
}

#[test]
fn expr_optional_element_access() {
    // `a?.[0]`
    let (parser, root) = parse_source("const x = a?.[0];");
    assert_no_errors(&parser, "optional element access");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION);
    let access = arena.get_access_expr(node).expect("access");
    assert!(access.question_dot_token, "should have ?.");
}

#[test]
fn expr_optional_call() {
    // `a?.(1)`
    let (parser, root) = parse_source("const x = a?.(1);");
    assert_no_errors(&parser, "optional call");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::CALL_EXPRESSION);
}

// =============================================================================
// 9. Destructuring Tests
// =============================================================================

#[test]
fn destructuring_object() {
    // `const { a, b: c, ...rest } = obj;`
    let (parser, root) = parse_source("const { a, b: c, ...rest } = obj;");
    assert_no_errors(&parser, "object destructuring");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let var = arena.get_variable(stmt_node).expect("var");
    let decl_list_node = arena.get(var.declarations.nodes[0]).expect("decl list");
    let decl_list = arena.get_variable(decl_list_node).expect("decl list data");
    let decl_node = arena.get(decl_list.declarations.nodes[0]).expect("decl");
    let decl = arena
        .get_variable_declaration(decl_node)
        .expect("decl data");
    let name_node = arena.get(decl.name).expect("name");
    assert_eq!(
        name_node.kind,
        syntax_kind_ext::OBJECT_BINDING_PATTERN,
        "should be object binding"
    );
}

#[test]
fn destructuring_array() {
    // `const [a, , b, ...rest] = arr;`
    let (parser, root) = parse_source("const [a, , b, ...rest] = arr;");
    assert_no_errors(&parser, "array destructuring");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let var = arena.get_variable(stmt_node).expect("var");
    let decl_list_node = arena.get(var.declarations.nodes[0]).expect("decl list");
    let decl_list = arena.get_variable(decl_list_node).expect("decl list data");
    let decl_node = arena.get(decl_list.declarations.nodes[0]).expect("decl");
    let decl = arena
        .get_variable_declaration(decl_node)
        .expect("decl data");
    let name_node = arena.get(decl.name).expect("name");
    assert_eq!(
        name_node.kind,
        syntax_kind_ext::ARRAY_BINDING_PATTERN,
        "should be array binding"
    );
}

#[test]
fn destructuring_nested() {
    // `const { a: { b } } = obj;`
    let (parser, _) = parse_source("const { a: { b } } = obj;");
    assert_no_errors(&parser, "nested destructuring");
}

#[test]
fn destructuring_with_defaults() {
    // `const { a = 1, b = 2 } = obj;`
    let (parser, _) = parse_source("const { a = 1, b = 2 } = obj;");
    assert_no_errors(&parser, "destructuring with defaults");
}

// =============================================================================
// 10. Additional Type Tests
// =============================================================================

#[test]
fn type_import() {
    // `type T = import('module').Foo`
    let (parser, root) = parse_source("type T = import('module').Foo;");
    assert_no_errors(&parser, "import type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("alias");
    let type_node = arena.get(alias.type_node).expect("type");
    assert!(type_node.kind != 0, "should have valid kind");
}

#[test]
fn type_reference_qualified_name_span_excludes_type_arguments() {
    let source = "type T = Foo.Bar<Baz>;";
    let (parser, root) = parse_source(source);
    assert_no_errors(&parser, "qualified type reference span");

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("alias");
    let type_node = arena.get(alias.type_node).expect("type");
    assert_eq!(type_node.kind, syntax_kind_ext::TYPE_REFERENCE);

    let type_ref = arena.get_type_ref(type_node).expect("type ref");
    assert_eq!(node_text(arena, source, type_ref.type_name), "Foo.Bar");
    assert_eq!(node_text(arena, source, alias.type_node), "Foo.Bar<Baz>");
}

#[test]
fn type_query_qualified_name_span_excludes_type_arguments() {
    let source = "type T = typeof ns.Foo<Bar>;";
    let (parser, root) = parse_source(source);
    assert_no_errors(&parser, "type query span");

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("alias");
    let type_node = arena.get(alias.type_node).expect("type");
    assert_eq!(type_node.kind, syntax_kind_ext::TYPE_QUERY);

    let type_query = arena.get_type_query(type_node).expect("type query");
    assert_eq!(node_text(arena, source, type_query.expr_name), "ns.Foo");
    assert_eq!(
        node_text(arena, source, alias.type_node),
        "typeof ns.Foo<Bar>"
    );
}

#[test]
fn import_type_qualified_name_span_excludes_type_arguments() {
    let source = "type T = import('m').Foo<Bar>;";
    let (parser, root) = parse_source(source);
    assert_no_errors(&parser, "import type span");

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("alias");
    let type_node = arena.get(alias.type_node).expect("type");
    assert_eq!(type_node.kind, syntax_kind_ext::TYPE_REFERENCE);

    let type_ref = arena.get_type_ref(type_node).expect("type ref");
    assert_eq!(
        node_text(arena, source, type_ref.type_name),
        "import('m').Foo"
    );
    assert_eq!(
        node_text(arena, source, alias.type_node),
        "import('m').Foo<Bar>"
    );
}

#[test]
fn intrinsic_type_keyword_recovery_stops_before_qualified_name() {
    let source = "var v: void.x;";
    let (parser, root) = parse_source(source);
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::EXPECTED),
        "expected TS1005 for malformed intrinsic qualified name, got {:?}",
        parser.get_diagnostics()
    );

    let arena = parser.get_arena();
    let type_annotation = get_var_type_annotation(arena, root);
    let type_node = arena.get(type_annotation).expect("type");
    assert_eq!(type_node.kind, SyntaxKind::VoidKeyword as u16);
    assert_eq!(node_text(arena, source, type_annotation), "void");
}

#[test]
fn unique_symbol_keeps_symbol_as_type_reference() {
    let source = "type T = unique symbol;";
    let (parser, root) = parse_source(source);
    assert_no_errors(&parser, "unique symbol");

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("alias");
    let type_node = arena.get(alias.type_node).expect("type");
    assert_eq!(type_node.kind, syntax_kind_ext::TYPE_OPERATOR);

    let type_op = arena.get_type_operator(type_node).expect("type operator");
    assert_eq!(type_op.operator, SyntaxKind::UniqueKeyword as u16);

    let inner_node = arena.get(type_op.type_node).expect("inner type");
    assert_eq!(inner_node.kind, syntax_kind_ext::TYPE_REFERENCE);
    let type_ref = arena.get_type_ref(inner_node).expect("type ref");
    assert_eq!(node_text(arena, source, type_ref.type_name), "symbol");
}

#[test]
fn super_type_arguments_report_parser_error_and_recover_to_call() {
    let source = "class Derived extends Base { method() { super<T>(0); } }";
    let (parser, root) = parse_source(source);
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::SUPER_MAY_NOT_USE_TYPE_ARGUMENTS),
        "expected TS2754 for super type arguments, got {:?}",
        parser.get_diagnostics()
    );

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let member_node = arena.get(class.members.nodes[0]).expect("member");
    let method = arena.get_method_decl(member_node).expect("method");
    let body_node = arena.get(method.body).expect("body");
    let block = arena.get_block(body_node).expect("block");
    let expr_stmt_node = arena
        .get(block.statements.nodes[0])
        .expect("expr stmt node");
    let expr_stmt = arena
        .get_expression_statement(expr_stmt_node)
        .expect("expr stmt");
    let call_node = arena.get(expr_stmt.expression).expect("call");
    assert_eq!(call_node.kind, syntax_kind_ext::CALL_EXPRESSION);

    let call = arena.get_call_expr(call_node).expect("call data");
    assert!(
        call.type_arguments.is_some(),
        "recovery should preserve type arguments on super calls for later checker recovery"
    );
    let callee_node = arena.get(call.expression).expect("callee");
    assert_eq!(callee_node.kind, SyntaxKind::SuperKeyword as u16);
}

#[test]
fn class_field_type_annotation_dot_reports_ts1442() {
    let source = "class C { a: this.foo; }";
    let (parser, _) = parse_source(source);
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();

    assert!(
        codes.contains(&diagnostic_codes::EXPECTED_FOR_PROPERTY_INITIALIZER),
        "expected TS1442 for class field type annotation followed by dot access, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn class_field_type_annotation_call_reports_ts1441() {
    let source = "class Base {} class C extends Base { a: super(); }";
    let (parser, _) = parse_source(source);
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();

    assert!(
        codes.contains(&diagnostic_codes::CANNOT_START_A_FUNCTION_CALL_IN_A_TYPE_ANNOTATION),
        "expected TS1441 for class field type annotation followed by call syntax, got {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(&diagnostic_codes::EXPECTED_FOR_PROPERTY_INITIALIZER),
        "did not expect TS1442 once call syntax is classified as TS1441, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn type_mapped_with_modifiers() {
    // `type T = { readonly [K in keyof T]-?: T[K] }`
    let (parser, root) = parse_source("type T = { readonly [K in keyof T]-?: T[K] };");
    assert_no_errors(&parser, "mapped type with modifiers");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("alias");
    let type_node = arena.get(alias.type_node).expect("type");
    assert_eq!(type_node.kind, syntax_kind_ext::MAPPED_TYPE);
    let mapped = arena.get_mapped_type(type_node).expect("mapped");
    assert!(mapped.readonly_token.is_some(), "should have readonly");
    assert!(mapped.question_token.is_some(), "should have question");
}

#[test]
fn type_type_literal() {
    // `type T = { x: string; y: number }`
    let (parser, root) = parse_source("type T = { x: string; y: number };");
    assert_no_errors(&parser, "type literal");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("alias");
    let type_node = arena.get(alias.type_node).expect("type");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::TYPE_LITERAL,
        "should be type literal"
    );
    let lit = arena
        .get_type_literal(type_node)
        .expect("type literal data");
    assert_eq!(lit.members.nodes.len(), 2, "should have 2 members");
}

#[test]
fn type_union_intersection_precedence() {
    // `A & B | C & D` should parse as `(A & B) | (C & D)` — intersection binds tighter
    let (parser, root) = parse_source("type T = A & B | C & D;");
    assert_no_errors(&parser, "union/intersection precedence");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("alias");
    let type_node = arena.get(alias.type_node).expect("type");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::UNION_TYPE,
        "top should be union (lower precedence)"
    );
    let composite = arena.get_composite_type(type_node).expect("composite");
    assert_eq!(
        composite.types.nodes.len(),
        2,
        "union should have 2 branches"
    );
    // Each branch should be an intersection
    let left = arena.get(composite.types.nodes[0]).expect("left");
    assert_eq!(
        left.kind,
        syntax_kind_ext::INTERSECTION_TYPE,
        "left should be intersection"
    );
    let right = arena.get(composite.types.nodes[1]).expect("right");
    assert_eq!(
        right.kind,
        syntax_kind_ext::INTERSECTION_TYPE,
        "right should be intersection"
    );
}

// =============================================================================
// 11. Template Literal Tests
// =============================================================================

#[test]
fn template_no_substitution() {
    // `const x = \`hello\``
    let (parser, root) = parse_source("const x = `hello`;");
    assert_no_errors(&parser, "no-sub template");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        SyntaxKind::NoSubstitutionTemplateLiteral as u16,
        "should be no-sub template"
    );
}

#[test]
fn template_with_substitution() {
    // `const x = \`hello ${name} world\``
    let (parser, root) = parse_source("const x = `hello ${name} world`;");
    assert_no_errors(&parser, "template with substitution");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::TEMPLATE_EXPRESSION,
        "should be template expression"
    );
}

// =============================================================================
// 12. Using / Await Using Declarations
// =============================================================================

#[test]
fn decl_using() {
    // `using x = getResource();`
    let (parser, root) = parse_source("using x = getResource();");
    assert_no_errors(&parser, "using declaration");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::VARIABLE_STATEMENT,
        "should be variable statement"
    );
}

#[test]
fn decl_await_using() {
    // `await using x = getResource();`
    let (parser, _root) = parse_source("async function f() { await using x = getResource(); }");
    assert_no_errors(&parser, "await using declaration");
}

// =============================================================================
// 13. Edge Cases for Specific AST Verification
// =============================================================================

#[test]
fn class_expression() {
    // `const C = class extends Base { constructor() { super(); } };`
    let (parser, root) =
        parse_source("const C = class extends Base { constructor() { super(); } };");
    assert_no_errors(&parser, "class expression");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::CLASS_EXPRESSION,
        "should be class expression"
    );
}

#[test]
fn function_expression() {
    // `const f = function foo(x: number) { return x; };`
    let (parser, root) = parse_source("const f = function foo(x: number) { return x; };");
    assert_no_errors(&parser, "function expression");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::FUNCTION_EXPRESSION,
        "should be function expression"
    );
    let func = arena.get_function(node).expect("func data");
    assert!(func.name.is_some(), "should have name 'foo'");
}

#[test]
fn generator_function() {
    // `function* gen() { yield 1; }`
    let (parser, root) = parse_source("function* gen() { yield 1; }");
    assert_no_errors(&parser, "generator function");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let func = arena.get_function(stmt_node).expect("function");
    assert!(func.asterisk_token, "should have asterisk (generator)");
}

#[test]
fn async_generator_function() {
    // `async function* gen() { yield 1; }`
    let (parser, root) = parse_source("async function* gen() { yield 1; }");
    assert_no_errors(&parser, "async generator");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let func = arena.get_function(stmt_node).expect("function");
    assert!(func.is_async, "should be async");
    assert!(func.asterisk_token, "should be generator");
}

#[test]
fn multiple_variable_declarations() {
    // `const a = 1, b = 2, c = 3;`
    let (parser, root) = parse_source("const a = 1, b = 2, c = 3;");
    assert_no_errors(&parser, "multiple variable declarations");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let var = arena.get_variable(stmt_node).expect("var");
    // var.declarations contains the VARIABLE_DECLARATION_LIST
    let decl_list_node = arena.get(var.declarations.nodes[0]).expect("decl list");
    let decl_list = arena.get_variable(decl_list_node).expect("decl list data");
    assert_eq!(
        decl_list.declarations.nodes.len(),
        3,
        "should have 3 declarations"
    );
}

#[test]
fn interface_call_and_construct_signatures() {
    // `interface I { (): void; new (): I; }`
    let (parser, root) = parse_source("interface I { (): void; new (): I; }");
    assert_no_errors(&parser, "call and construct signatures");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let iface = arena.get_interface(stmt_node).expect("interface");
    assert_eq!(iface.members.nodes.len(), 2, "should have 2 members");
    let m0 = arena.get(iface.members.nodes[0]).expect("m0");
    assert_eq!(
        m0.kind,
        syntax_kind_ext::CALL_SIGNATURE,
        "first should be call signature"
    );
    let m1 = arena.get(iface.members.nodes[1]).expect("m1");
    assert_eq!(
        m1.kind,
        syntax_kind_ext::CONSTRUCT_SIGNATURE,
        "second should be construct signature"
    );
}

#[test]
fn type_predicate_in_function() {
    // `function isString(x: any): x is string { return typeof x === "string"; }`
    let (parser, root) =
        parse_source("function isString(x: any): x is string { return typeof x === 'string'; }");
    assert_no_errors(&parser, "type predicate");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let func = arena.get_function(stmt_node).expect("function");
    let ret_type = arena.get(func.type_annotation).expect("return type");
    assert_eq!(
        ret_type.kind,
        syntax_kind_ext::TYPE_PREDICATE,
        "should be type predicate"
    );
}

#[test]
fn import_with_attributes() {
    // `import data from './data.json' with { type: 'json' };`
    let (parser, root) = parse_source("import data from './data.json' with { type: 'json' };");
    assert_no_errors(&parser, "import with attributes");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let import = arena.get_import_decl(stmt_node).expect("import");
    assert!(import.attributes.is_some(), "should have import attributes");
}

#[test]
fn export_star_from() {
    // `export * from './module';`
    let (parser, root) = parse_source("export * from './module';");
    assert_no_errors(&parser, "export star from");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(stmt_node.kind, syntax_kind_ext::EXPORT_DECLARATION);
    let export = arena.get_export_decl(stmt_node).expect("export decl");
    assert!(
        export.module_specifier.is_some(),
        "should have module specifier"
    );
}

#[test]
fn export_star_as_namespace() {
    // `export * as ns from './module';`
    let (parser, root) = parse_source("export * as ns from './module';");
    assert_no_errors(&parser, "export * as ns");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(stmt_node.kind, syntax_kind_ext::EXPORT_DECLARATION);
}
