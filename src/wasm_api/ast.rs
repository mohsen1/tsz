//! AST Traversal and Node Access
//!
//! Provides TypeScript-compatible AST traversal and node type guards.

use wasm_bindgen::prelude::*;

use crate::parser::syntax_kind_ext;
use crate::parser::{NodeArena, NodeIndex};
use crate::scanner::SyntaxKind;

// Token kind constants (from SyntaxKind enum)
const IDENTIFIER: u16 = SyntaxKind::Identifier as u16;
const STRING_LITERAL: u16 = SyntaxKind::StringLiteral as u16;
const NUMERIC_LITERAL: u16 = SyntaxKind::NumericLiteral as u16;
const BIGINT_LITERAL: u16 = SyntaxKind::BigIntLiteral as u16;
const REGEX_LITERAL: u16 = SyntaxKind::RegularExpressionLiteral as u16;
const THIS_KEYWORD: u16 = SyntaxKind::ThisKeyword as u16;
const SUPER_KEYWORD: u16 = SyntaxKind::SuperKeyword as u16;
const NULL_KEYWORD: u16 = SyntaxKind::NullKeyword as u16;
const TRUE_KEYWORD: u16 = SyntaxKind::TrueKeyword as u16;
const FALSE_KEYWORD: u16 = SyntaxKind::FalseKeyword as u16;

/// Get children of a node based on its kind
///
/// Returns a vector of child NodeIndex values
pub fn get_node_children(arena: &NodeArena, node_idx: NodeIndex) -> Vec<NodeIndex> {
    let Some(node) = arena.get(node_idx) else {
        return Vec::new();
    };

    let mut children = Vec::new();

    match node.kind {
        // --- Source File ---
        k if k == syntax_kind_ext::SOURCE_FILE => {
            if let Some(sf) = arena.get_source_file(node) {
                children.extend(sf.statements.nodes.iter().copied());
            }
        }

        // --- Block Statements ---
        k if k == syntax_kind_ext::BLOCK
            || k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
            || k == syntax_kind_ext::CASE_BLOCK =>
        {
            if let Some(block) = arena.get_block(node) {
                children.extend(block.statements.nodes.iter().copied());
            }
        }

        k if k == syntax_kind_ext::MODULE_BLOCK => {
            if let Some(mod_block) = arena.get_module_block(node) {
                if let Some(ref stmts) = mod_block.statements {
                    children.extend(stmts.nodes.iter().copied());
                }
            }
        }

        // --- Function-like ---
        k if k == syntax_kind_ext::FUNCTION_DECLARATION
            || k == syntax_kind_ext::FUNCTION_EXPRESSION
            || k == syntax_kind_ext::ARROW_FUNCTION =>
        {
            if let Some(func) = arena.get_function(node) {
                if let Some(ref modifiers) = func.modifiers {
                    children.extend(modifiers.nodes.iter().copied());
                }
                if func.name.is_some() {
                    children.push(func.name);
                }
                if let Some(ref type_params) = func.type_parameters {
                    children.extend(type_params.nodes.iter().copied());
                }
                children.extend(func.parameters.nodes.iter().copied());
                if func.type_annotation.is_some() {
                    children.push(func.type_annotation);
                }
                if func.body.is_some() {
                    children.push(func.body);
                }
            }
        }

        k if k == syntax_kind_ext::METHOD_DECLARATION => {
            if let Some(method) = arena.get_method_decl(node) {
                if let Some(ref modifiers) = method.modifiers {
                    children.extend(modifiers.nodes.iter().copied());
                }
                if method.name.is_some() {
                    children.push(method.name);
                }
                if let Some(ref type_params) = method.type_parameters {
                    children.extend(type_params.nodes.iter().copied());
                }
                children.extend(method.parameters.nodes.iter().copied());
                if method.type_annotation.is_some() {
                    children.push(method.type_annotation);
                }
                if method.body.is_some() {
                    children.push(method.body);
                }
            }
        }

        k if k == syntax_kind_ext::CONSTRUCTOR => {
            if let Some(ctor) = arena.get_constructor(node) {
                if let Some(ref modifiers) = ctor.modifiers {
                    children.extend(modifiers.nodes.iter().copied());
                }
                if let Some(ref type_params) = ctor.type_parameters {
                    children.extend(type_params.nodes.iter().copied());
                }
                children.extend(ctor.parameters.nodes.iter().copied());
                if ctor.body.is_some() {
                    children.push(ctor.body);
                }
            }
        }

        // --- Class ---
        k if k == syntax_kind_ext::CLASS_DECLARATION || k == syntax_kind_ext::CLASS_EXPRESSION => {
            if let Some(class) = arena.get_class(node) {
                if let Some(ref modifiers) = class.modifiers {
                    children.extend(modifiers.nodes.iter().copied());
                }
                if class.name.is_some() {
                    children.push(class.name);
                }
                if let Some(ref type_params) = class.type_parameters {
                    children.extend(type_params.nodes.iter().copied());
                }
                if let Some(ref heritage) = class.heritage_clauses {
                    children.extend(heritage.nodes.iter().copied());
                }
                children.extend(class.members.nodes.iter().copied());
            }
        }

        // --- Interface ---
        k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
            if let Some(iface) = arena.get_interface(node) {
                if let Some(ref modifiers) = iface.modifiers {
                    children.extend(modifiers.nodes.iter().copied());
                }
                if iface.name.is_some() {
                    children.push(iface.name);
                }
                if let Some(ref type_params) = iface.type_parameters {
                    children.extend(type_params.nodes.iter().copied());
                }
                if let Some(ref heritage) = iface.heritage_clauses {
                    children.extend(heritage.nodes.iter().copied());
                }
                children.extend(iface.members.nodes.iter().copied());
            }
        }

        // --- Type Alias ---
        k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
            if let Some(alias) = arena.get_type_alias(node) {
                if let Some(ref modifiers) = alias.modifiers {
                    children.extend(modifiers.nodes.iter().copied());
                }
                if alias.name.is_some() {
                    children.push(alias.name);
                }
                if let Some(ref type_params) = alias.type_parameters {
                    children.extend(type_params.nodes.iter().copied());
                }
                if alias.type_node.is_some() {
                    children.push(alias.type_node);
                }
            }
        }

        // --- Variable Statement ---
        k if k == syntax_kind_ext::VARIABLE_STATEMENT
            || k == syntax_kind_ext::VARIABLE_DECLARATION_LIST =>
        {
            if let Some(var) = arena.get_variable(node) {
                children.extend(var.declarations.nodes.iter().copied());
            }
        }

        k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
            if let Some(decl) = arena.get_variable_declaration(node) {
                if decl.name.is_some() {
                    children.push(decl.name);
                }
                if decl.type_annotation.is_some() {
                    children.push(decl.type_annotation);
                }
                if decl.initializer.is_some() {
                    children.push(decl.initializer);
                }
            }
        }

        // --- Parameters ---
        k if k == syntax_kind_ext::PARAMETER => {
            if let Some(param) = arena.get_parameter(node) {
                if let Some(ref modifiers) = param.modifiers {
                    children.extend(modifiers.nodes.iter().copied());
                }
                if param.name.is_some() {
                    children.push(param.name);
                }
                if param.type_annotation.is_some() {
                    children.push(param.type_annotation);
                }
                if param.initializer.is_some() {
                    children.push(param.initializer);
                }
            }
        }

        // --- Property ---
        k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
            if let Some(prop) = arena.get_property_decl(node) {
                if let Some(ref modifiers) = prop.modifiers {
                    children.extend(modifiers.nodes.iter().copied());
                }
                if prop.name.is_some() {
                    children.push(prop.name);
                }
                if prop.type_annotation.is_some() {
                    children.push(prop.type_annotation);
                }
                if prop.initializer.is_some() {
                    children.push(prop.initializer);
                }
            }
        }

        // --- Expressions ---
        k if k == syntax_kind_ext::BINARY_EXPRESSION => {
            if let Some(bin) = arena.get_binary_expr(node) {
                if bin.left.is_some() {
                    children.push(bin.left);
                }
                if bin.right.is_some() {
                    children.push(bin.right);
                }
            }
        }

        k if k == syntax_kind_ext::CALL_EXPRESSION || k == syntax_kind_ext::NEW_EXPRESSION => {
            if let Some(call) = arena.get_call_expr(node) {
                if call.expression.is_some() {
                    children.push(call.expression);
                }
                if let Some(ref type_args) = call.type_arguments {
                    children.extend(type_args.nodes.iter().copied());
                }
                if let Some(ref args) = call.arguments {
                    children.extend(args.nodes.iter().copied());
                }
            }
        }

        k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
        {
            if let Some(access) = arena.get_access_expr(node) {
                if access.expression.is_some() {
                    children.push(access.expression);
                }
                if access.name_or_argument.is_some() {
                    children.push(access.name_or_argument);
                }
            }
        }

        k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
            if let Some(cond) = arena.get_conditional_expr(node) {
                if cond.condition.is_some() {
                    children.push(cond.condition);
                }
                if cond.when_true.is_some() {
                    children.push(cond.when_true);
                }
                if cond.when_false.is_some() {
                    children.push(cond.when_false);
                }
            }
        }

        // --- Statements ---
        k if k == syntax_kind_ext::IF_STATEMENT => {
            if let Some(if_stmt) = arena.get_if_statement(node) {
                if if_stmt.expression.is_some() {
                    children.push(if_stmt.expression);
                }
                if if_stmt.then_statement.is_some() {
                    children.push(if_stmt.then_statement);
                }
                if if_stmt.else_statement.is_some() {
                    children.push(if_stmt.else_statement);
                }
            }
        }

        k if k == syntax_kind_ext::FOR_STATEMENT
            || k == syntax_kind_ext::WHILE_STATEMENT
            || k == syntax_kind_ext::DO_STATEMENT =>
        {
            if let Some(loop_data) = arena.get_loop(node) {
                if loop_data.initializer.is_some() {
                    children.push(loop_data.initializer);
                }
                if loop_data.condition.is_some() {
                    children.push(loop_data.condition);
                }
                if loop_data.incrementor.is_some() {
                    children.push(loop_data.incrementor);
                }
                if loop_data.statement.is_some() {
                    children.push(loop_data.statement);
                }
            }
        }

        k if k == syntax_kind_ext::FOR_IN_STATEMENT || k == syntax_kind_ext::FOR_OF_STATEMENT => {
            if let Some(for_in) = arena.get_for_in_of(node) {
                if for_in.initializer.is_some() {
                    children.push(for_in.initializer);
                }
                if for_in.expression.is_some() {
                    children.push(for_in.expression);
                }
                if for_in.statement.is_some() {
                    children.push(for_in.statement);
                }
            }
        }

        k if k == syntax_kind_ext::RETURN_STATEMENT || k == syntax_kind_ext::THROW_STATEMENT => {
            if let Some(ret) = arena.get_return_statement(node) {
                if ret.expression.is_some() {
                    children.push(ret.expression);
                }
            }
        }

        k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
            if let Some(expr_stmt) = arena.get_expression_statement(node) {
                if expr_stmt.expression.is_some() {
                    children.push(expr_stmt.expression);
                }
            }
        }

        // --- Imports/Exports ---
        k if k == syntax_kind_ext::IMPORT_DECLARATION => {
            if let Some(import) = arena.get_import_decl(node) {
                if import.import_clause.is_some() {
                    children.push(import.import_clause);
                }
                if import.module_specifier.is_some() {
                    children.push(import.module_specifier);
                }
            }
        }

        k if k == syntax_kind_ext::EXPORT_DECLARATION => {
            if let Some(export) = arena.get_export_decl(node) {
                if export.export_clause.is_some() {
                    children.push(export.export_clause);
                }
                if export.module_specifier.is_some() {
                    children.push(export.module_specifier);
                }
            }
        }

        // --- Type Nodes ---
        k if k == syntax_kind_ext::TYPE_REFERENCE => {
            if let Some(type_ref) = arena.get_type_ref(node) {
                if type_ref.type_name.is_some() {
                    children.push(type_ref.type_name);
                }
                if let Some(ref type_args) = type_ref.type_arguments {
                    children.extend(type_args.nodes.iter().copied());
                }
            }
        }

        // --- Qualified Names ---
        k if k == syntax_kind_ext::QUALIFIED_NAME => {
            if let Some(qn) = arena.get_qualified_name(node) {
                if qn.left.is_some() {
                    children.push(qn.left);
                }
                if qn.right.is_some() {
                    children.push(qn.right);
                }
            }
        }

        // Default: no children (tokens, leaves, etc.)
        _ => {}
    }

    children
}

// === Node Type Guards ===
// These functions check if a node is of a specific kind

/// Check if node is an identifier
#[wasm_bindgen(js_name = isIdentifier)]
pub fn is_identifier(kind: u16) -> bool {
    kind == IDENTIFIER
}

/// Check if node is a string literal
#[wasm_bindgen(js_name = isStringLiteral)]
pub fn is_string_literal(kind: u16) -> bool {
    kind == STRING_LITERAL
}

/// Check if node is a numeric literal
#[wasm_bindgen(js_name = isNumericLiteral)]
pub fn is_numeric_literal(kind: u16) -> bool {
    kind == NUMERIC_LITERAL
}

/// Check if node is a function declaration
#[wasm_bindgen(js_name = isFunctionDeclaration)]
pub fn is_function_declaration(kind: u16) -> bool {
    kind == syntax_kind_ext::FUNCTION_DECLARATION
}

/// Check if node is a function expression
#[wasm_bindgen(js_name = isFunctionExpression)]
pub fn is_function_expression(kind: u16) -> bool {
    kind == syntax_kind_ext::FUNCTION_EXPRESSION
}

/// Check if node is an arrow function
#[wasm_bindgen(js_name = isArrowFunction)]
pub fn is_arrow_function(kind: u16) -> bool {
    kind == syntax_kind_ext::ARROW_FUNCTION
}

/// Check if node is any function-like
#[wasm_bindgen(js_name = isFunctionLike)]
pub fn is_function_like(kind: u16) -> bool {
    kind == syntax_kind_ext::FUNCTION_DECLARATION
        || kind == syntax_kind_ext::FUNCTION_EXPRESSION
        || kind == syntax_kind_ext::ARROW_FUNCTION
        || kind == syntax_kind_ext::METHOD_DECLARATION
        || kind == syntax_kind_ext::CONSTRUCTOR
        || kind == syntax_kind_ext::GET_ACCESSOR
        || kind == syntax_kind_ext::SET_ACCESSOR
}

/// Check if node is a class declaration
#[wasm_bindgen(js_name = isClassDeclaration)]
pub fn is_class_declaration(kind: u16) -> bool {
    kind == syntax_kind_ext::CLASS_DECLARATION
}

/// Check if node is a class expression
#[wasm_bindgen(js_name = isClassExpression)]
pub fn is_class_expression(kind: u16) -> bool {
    kind == syntax_kind_ext::CLASS_EXPRESSION
}

/// Check if node is any class-like
#[wasm_bindgen(js_name = isClassLike)]
pub fn is_class_like(kind: u16) -> bool {
    kind == syntax_kind_ext::CLASS_DECLARATION || kind == syntax_kind_ext::CLASS_EXPRESSION
}

/// Check if node is an interface declaration
#[wasm_bindgen(js_name = isInterfaceDeclaration)]
pub fn is_interface_declaration(kind: u16) -> bool {
    kind == syntax_kind_ext::INTERFACE_DECLARATION
}

/// Check if node is a type alias declaration
#[wasm_bindgen(js_name = isTypeAliasDeclaration)]
pub fn is_type_alias_declaration(kind: u16) -> bool {
    kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
}

/// Check if node is an enum declaration
#[wasm_bindgen(js_name = isEnumDeclaration)]
pub fn is_enum_declaration(kind: u16) -> bool {
    kind == syntax_kind_ext::ENUM_DECLARATION
}

/// Check if node is a module/namespace declaration
#[wasm_bindgen(js_name = isModuleDeclaration)]
pub fn is_module_declaration(kind: u16) -> bool {
    kind == syntax_kind_ext::MODULE_DECLARATION
}

/// Check if node is a variable statement
#[wasm_bindgen(js_name = isVariableStatement)]
pub fn is_variable_statement(kind: u16) -> bool {
    kind == syntax_kind_ext::VARIABLE_STATEMENT
}

/// Check if node is a variable declaration
#[wasm_bindgen(js_name = isVariableDeclaration)]
pub fn is_variable_declaration(kind: u16) -> bool {
    kind == syntax_kind_ext::VARIABLE_DECLARATION
}

/// Check if node is a parameter
#[wasm_bindgen(js_name = isParameter)]
pub fn is_parameter(kind: u16) -> bool {
    kind == syntax_kind_ext::PARAMETER
}

/// Check if node is a property declaration
#[wasm_bindgen(js_name = isPropertyDeclaration)]
pub fn is_property_declaration(kind: u16) -> bool {
    kind == syntax_kind_ext::PROPERTY_DECLARATION
}

/// Check if node is a method declaration
#[wasm_bindgen(js_name = isMethodDeclaration)]
pub fn is_method_declaration(kind: u16) -> bool {
    kind == syntax_kind_ext::METHOD_DECLARATION
}

/// Check if node is a constructor
#[wasm_bindgen(js_name = isConstructorDeclaration)]
pub fn is_constructor_declaration(kind: u16) -> bool {
    kind == syntax_kind_ext::CONSTRUCTOR
}

/// Check if node is a call expression
#[wasm_bindgen(js_name = isCallExpression)]
pub fn is_call_expression(kind: u16) -> bool {
    kind == syntax_kind_ext::CALL_EXPRESSION
}

/// Check if node is a new expression
#[wasm_bindgen(js_name = isNewExpression)]
pub fn is_new_expression(kind: u16) -> bool {
    kind == syntax_kind_ext::NEW_EXPRESSION
}

/// Check if node is a property access expression
#[wasm_bindgen(js_name = isPropertyAccessExpression)]
pub fn is_property_access_expression(kind: u16) -> bool {
    kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
}

/// Check if node is an element access expression
#[wasm_bindgen(js_name = isElementAccessExpression)]
pub fn is_element_access_expression(kind: u16) -> bool {
    kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
}

/// Check if node is a binary expression
#[wasm_bindgen(js_name = isBinaryExpression)]
pub fn is_binary_expression(kind: u16) -> bool {
    kind == syntax_kind_ext::BINARY_EXPRESSION
}

/// Check if node is a block
#[wasm_bindgen(js_name = isBlock)]
pub fn is_block(kind: u16) -> bool {
    kind == syntax_kind_ext::BLOCK
}

/// Check if node is an if statement
#[wasm_bindgen(js_name = isIfStatement)]
pub fn is_if_statement(kind: u16) -> bool {
    kind == syntax_kind_ext::IF_STATEMENT
}

/// Check if node is a for statement
#[wasm_bindgen(js_name = isForStatement)]
pub fn is_for_statement(kind: u16) -> bool {
    kind == syntax_kind_ext::FOR_STATEMENT
}

/// Check if node is a while statement
#[wasm_bindgen(js_name = isWhileStatement)]
pub fn is_while_statement(kind: u16) -> bool {
    kind == syntax_kind_ext::WHILE_STATEMENT
}

/// Check if node is a return statement
#[wasm_bindgen(js_name = isReturnStatement)]
pub fn is_return_statement(kind: u16) -> bool {
    kind == syntax_kind_ext::RETURN_STATEMENT
}

/// Check if node is an expression statement
#[wasm_bindgen(js_name = isExpressionStatement)]
pub fn is_expression_statement(kind: u16) -> bool {
    kind == syntax_kind_ext::EXPRESSION_STATEMENT
}

/// Check if node is an import declaration
#[wasm_bindgen(js_name = isImportDeclaration)]
pub fn is_import_declaration(kind: u16) -> bool {
    kind == syntax_kind_ext::IMPORT_DECLARATION
}

/// Check if node is an export declaration
#[wasm_bindgen(js_name = isExportDeclaration)]
pub fn is_export_declaration(kind: u16) -> bool {
    kind == syntax_kind_ext::EXPORT_DECLARATION
}

/// Check if node is a type reference
#[wasm_bindgen(js_name = isTypeReferenceNode)]
pub fn is_type_reference_node(kind: u16) -> bool {
    kind == syntax_kind_ext::TYPE_REFERENCE
}

/// Check if node is an array type
#[wasm_bindgen(js_name = isArrayTypeNode)]
pub fn is_array_type_node(kind: u16) -> bool {
    kind == syntax_kind_ext::ARRAY_TYPE
}

/// Check if node is a union type
#[wasm_bindgen(js_name = isUnionTypeNode)]
pub fn is_union_type_node(kind: u16) -> bool {
    kind == syntax_kind_ext::UNION_TYPE
}

/// Check if node is an intersection type
#[wasm_bindgen(js_name = isIntersectionTypeNode)]
pub fn is_intersection_type_node(kind: u16) -> bool {
    kind == syntax_kind_ext::INTERSECTION_TYPE
}

/// Check if node is a statement
#[wasm_bindgen(js_name = isStatement)]
pub fn is_statement(kind: u16) -> bool {
    kind == syntax_kind_ext::VARIABLE_STATEMENT
        || kind == syntax_kind_ext::EXPRESSION_STATEMENT
        || kind == syntax_kind_ext::IF_STATEMENT
        || kind == syntax_kind_ext::DO_STATEMENT
        || kind == syntax_kind_ext::WHILE_STATEMENT
        || kind == syntax_kind_ext::FOR_STATEMENT
        || kind == syntax_kind_ext::FOR_IN_STATEMENT
        || kind == syntax_kind_ext::FOR_OF_STATEMENT
        || kind == syntax_kind_ext::CONTINUE_STATEMENT
        || kind == syntax_kind_ext::BREAK_STATEMENT
        || kind == syntax_kind_ext::RETURN_STATEMENT
        || kind == syntax_kind_ext::WITH_STATEMENT
        || kind == syntax_kind_ext::SWITCH_STATEMENT
        || kind == syntax_kind_ext::LABELED_STATEMENT
        || kind == syntax_kind_ext::THROW_STATEMENT
        || kind == syntax_kind_ext::TRY_STATEMENT
        || kind == syntax_kind_ext::DEBUGGER_STATEMENT
        || kind == syntax_kind_ext::BLOCK
        || kind == syntax_kind_ext::EMPTY_STATEMENT
}

/// Check if node is a declaration
#[wasm_bindgen(js_name = isDeclaration)]
pub fn is_declaration(kind: u16) -> bool {
    kind == syntax_kind_ext::VARIABLE_DECLARATION
        || kind == syntax_kind_ext::FUNCTION_DECLARATION
        || kind == syntax_kind_ext::CLASS_DECLARATION
        || kind == syntax_kind_ext::INTERFACE_DECLARATION
        || kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
        || kind == syntax_kind_ext::ENUM_DECLARATION
        || kind == syntax_kind_ext::MODULE_DECLARATION
        || kind == syntax_kind_ext::IMPORT_DECLARATION
        || kind == syntax_kind_ext::EXPORT_DECLARATION
        || kind == syntax_kind_ext::PARAMETER
        || kind == syntax_kind_ext::TYPE_PARAMETER
        || kind == syntax_kind_ext::PROPERTY_DECLARATION
        || kind == syntax_kind_ext::METHOD_DECLARATION
        || kind == syntax_kind_ext::CONSTRUCTOR
}

/// Check if node is an expression
#[wasm_bindgen(js_name = isExpression)]
pub fn is_expression(kind: u16) -> bool {
    kind == IDENTIFIER
        || kind == STRING_LITERAL
        || kind == NUMERIC_LITERAL
        || kind == BIGINT_LITERAL
        || kind == REGEX_LITERAL
        || kind == THIS_KEYWORD
        || kind == SUPER_KEYWORD
        || kind == NULL_KEYWORD
        || kind == TRUE_KEYWORD
        || kind == FALSE_KEYWORD
        || kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
        || kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        || kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        || kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        || kind == syntax_kind_ext::CALL_EXPRESSION
        || kind == syntax_kind_ext::NEW_EXPRESSION
        || kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
        || kind == syntax_kind_ext::TYPE_ASSERTION
        || kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
        || kind == syntax_kind_ext::FUNCTION_EXPRESSION
        || kind == syntax_kind_ext::ARROW_FUNCTION
        || kind == syntax_kind_ext::DELETE_EXPRESSION
        || kind == syntax_kind_ext::TYPE_OF_EXPRESSION
        || kind == syntax_kind_ext::VOID_EXPRESSION
        || kind == syntax_kind_ext::AWAIT_EXPRESSION
        || kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
        || kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
        || kind == syntax_kind_ext::BINARY_EXPRESSION
        || kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
        || kind == syntax_kind_ext::TEMPLATE_EXPRESSION
        || kind == syntax_kind_ext::YIELD_EXPRESSION
        || kind == syntax_kind_ext::SPREAD_ELEMENT
        || kind == syntax_kind_ext::CLASS_EXPRESSION
        || kind == syntax_kind_ext::AS_EXPRESSION
        || kind == syntax_kind_ext::SATISFIES_EXPRESSION
        || kind == syntax_kind_ext::NON_NULL_EXPRESSION
}
