//! AST Traversal and Node Access
//!
//! Provides TypeScript-compatible AST traversal and node type guards.

use wasm_bindgen::prelude::*;

use tsz::parser::syntax_kind_ext;
use tsz::parser::{NodeArena, NodeIndex};
use tsz::scanner::SyntaxKind;

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

/// Macro to generate single-kind wasm_bindgen predicate functions.
macro_rules! define_kind_predicates {
    ($($(#[doc = $doc:expr])* $js_name:literal, $rust_name:ident => $kind:expr);* $(;)?) => {
        $(
            $(#[doc = $doc])*
            #[wasm_bindgen(js_name = $js_name)]
            pub fn $rust_name(kind: u16) -> bool {
                kind == $kind
            }
        )*
    };
}

define_kind_predicates! {
    /// Check if node is an identifier
    "isIdentifier", is_identifier => IDENTIFIER;
    /// Check if node is a string literal
    "isStringLiteral", is_string_literal => STRING_LITERAL;
    /// Check if node is a numeric literal
    "isNumericLiteral", is_numeric_literal => NUMERIC_LITERAL;
    /// Check if node is a function declaration
    "isFunctionDeclaration", is_function_declaration => syntax_kind_ext::FUNCTION_DECLARATION;
    /// Check if node is a function expression
    "isFunctionExpression", is_function_expression => syntax_kind_ext::FUNCTION_EXPRESSION;
    /// Check if node is an arrow function
    "isArrowFunction", is_arrow_function => syntax_kind_ext::ARROW_FUNCTION;
    /// Check if node is a class declaration
    "isClassDeclaration", is_class_declaration => syntax_kind_ext::CLASS_DECLARATION;
    /// Check if node is a class expression
    "isClassExpression", is_class_expression => syntax_kind_ext::CLASS_EXPRESSION;
    /// Check if node is an interface declaration
    "isInterfaceDeclaration", is_interface_declaration => syntax_kind_ext::INTERFACE_DECLARATION;
    /// Check if node is a type alias declaration
    "isTypeAliasDeclaration", is_type_alias_declaration => syntax_kind_ext::TYPE_ALIAS_DECLARATION;
    /// Check if node is an enum declaration
    "isEnumDeclaration", is_enum_declaration => syntax_kind_ext::ENUM_DECLARATION;
    /// Check if node is a module/namespace declaration
    "isModuleDeclaration", is_module_declaration => syntax_kind_ext::MODULE_DECLARATION;
    /// Check if node is a variable statement
    "isVariableStatement", is_variable_statement => syntax_kind_ext::VARIABLE_STATEMENT;
    /// Check if node is a variable declaration
    "isVariableDeclaration", is_variable_declaration => syntax_kind_ext::VARIABLE_DECLARATION;
    /// Check if node is a parameter
    "isParameter", is_parameter => syntax_kind_ext::PARAMETER;
    /// Check if node is a property declaration
    "isPropertyDeclaration", is_property_declaration => syntax_kind_ext::PROPERTY_DECLARATION;
    /// Check if node is a method declaration
    "isMethodDeclaration", is_method_declaration => syntax_kind_ext::METHOD_DECLARATION;
    /// Check if node is a constructor
    "isConstructorDeclaration", is_constructor_declaration => syntax_kind_ext::CONSTRUCTOR;
    /// Check if node is a call expression
    "isCallExpression", is_call_expression => syntax_kind_ext::CALL_EXPRESSION;
    /// Check if node is a new expression
    "isNewExpression", is_new_expression => syntax_kind_ext::NEW_EXPRESSION;
    /// Check if node is a property access expression
    "isPropertyAccessExpression", is_property_access_expression => syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION;
    /// Check if node is an element access expression
    "isElementAccessExpression", is_element_access_expression => syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION;
    /// Check if node is a binary expression
    "isBinaryExpression", is_binary_expression => syntax_kind_ext::BINARY_EXPRESSION;
    /// Check if node is a block
    "isBlock", is_block => syntax_kind_ext::BLOCK;
    /// Check if node is an if statement
    "isIfStatement", is_if_statement => syntax_kind_ext::IF_STATEMENT;
    /// Check if node is a for statement
    "isForStatement", is_for_statement => syntax_kind_ext::FOR_STATEMENT;
    /// Check if node is a while statement
    "isWhileStatement", is_while_statement => syntax_kind_ext::WHILE_STATEMENT;
    /// Check if node is a return statement
    "isReturnStatement", is_return_statement => syntax_kind_ext::RETURN_STATEMENT;
    /// Check if node is an expression statement
    "isExpressionStatement", is_expression_statement => syntax_kind_ext::EXPRESSION_STATEMENT;
    /// Check if node is an import declaration
    "isImportDeclaration", is_import_declaration => syntax_kind_ext::IMPORT_DECLARATION;
    /// Check if node is an export declaration
    "isExportDeclaration", is_export_declaration => syntax_kind_ext::EXPORT_DECLARATION;
    /// Check if node is a type reference
    "isTypeReferenceNode", is_type_reference_node => syntax_kind_ext::TYPE_REFERENCE;
    /// Check if node is an array type
    "isArrayTypeNode", is_array_type_node => syntax_kind_ext::ARRAY_TYPE;
    /// Check if node is a union type
    "isUnionTypeNode", is_union_type_node => syntax_kind_ext::UNION_TYPE;
    /// Check if node is an intersection type
    "isIntersectionTypeNode", is_intersection_type_node => syntax_kind_ext::INTERSECTION_TYPE;
}

/// Check if node is any class-like
#[wasm_bindgen(js_name = isClassLike)]
pub fn is_class_like(kind: u16) -> bool {
    kind == syntax_kind_ext::CLASS_DECLARATION || kind == syntax_kind_ext::CLASS_EXPRESSION
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
