//! AST child collection methods for `NodeArena`.
//!
//! This module provides the `collect_*_children` family of methods used by the
//! `NodeAccess::get_children` implementation to enumerate the child nodes of
//! any AST node. Each collector handles a category of syntax kinds (names,
//! expressions, statements, declarations, imports/exports, types, members,
//! patterns, JSX, signatures, source files).

use super::base::{NodeIndex, NodeList};
use super::node::{Node, NodeArena};
use super::syntax_kind_ext::{
    ARRAY_BINDING_PATTERN, ARRAY_LITERAL_EXPRESSION, ARRAY_TYPE, ARROW_FUNCTION, AS_EXPRESSION,
    AWAIT_EXPRESSION, BINARY_EXPRESSION, BINDING_ELEMENT, BLOCK, BREAK_STATEMENT, CALL_EXPRESSION,
    CALL_SIGNATURE, CASE_BLOCK, CASE_CLAUSE, CATCH_CLAUSE, CLASS_DECLARATION, CLASS_EXPRESSION,
    CLASS_STATIC_BLOCK_DECLARATION, COMPUTED_PROPERTY_NAME, CONDITIONAL_EXPRESSION,
    CONDITIONAL_TYPE, CONSTRUCT_SIGNATURE, CONSTRUCTOR, CONSTRUCTOR_TYPE, CONTINUE_STATEMENT,
    DECORATOR, DEFAULT_CLAUSE, DO_STATEMENT, ELEMENT_ACCESS_EXPRESSION, ENUM_DECLARATION,
    ENUM_MEMBER, EXPORT_ASSIGNMENT, EXPORT_DECLARATION, EXPORT_SPECIFIER, EXPRESSION_STATEMENT,
    EXPRESSION_WITH_TYPE_ARGUMENTS, FOR_IN_STATEMENT, FOR_OF_STATEMENT, FOR_STATEMENT,
    FUNCTION_DECLARATION, FUNCTION_EXPRESSION, FUNCTION_TYPE, GET_ACCESSOR, HERITAGE_CLAUSE,
    IF_STATEMENT, IMPORT_CLAUSE, IMPORT_DECLARATION, IMPORT_EQUALS_DECLARATION, IMPORT_SPECIFIER,
    INDEX_SIGNATURE, INDEXED_ACCESS_TYPE, INFER_TYPE, INTERFACE_DECLARATION, INTERSECTION_TYPE,
    JSX_ATTRIBUTE, JSX_ATTRIBUTES, JSX_CLOSING_ELEMENT, JSX_ELEMENT, JSX_EXPRESSION, JSX_FRAGMENT,
    JSX_NAMESPACED_NAME, JSX_OPENING_ELEMENT, JSX_SELF_CLOSING_ELEMENT, JSX_SPREAD_ATTRIBUTE,
    LABELED_STATEMENT, LITERAL_TYPE, MAPPED_TYPE, METHOD_DECLARATION, METHOD_SIGNATURE,
    MODULE_BLOCK, MODULE_DECLARATION, NAMED_EXPORTS, NAMED_IMPORTS, NAMED_TUPLE_MEMBER,
    NAMESPACE_EXPORT, NAMESPACE_IMPORT, NEW_EXPRESSION, NON_NULL_EXPRESSION,
    OBJECT_BINDING_PATTERN, OBJECT_LITERAL_EXPRESSION, OPTIONAL_TYPE, PARAMETER,
    PARENTHESIZED_EXPRESSION, PARENTHESIZED_TYPE, POSTFIX_UNARY_EXPRESSION,
    PREFIX_UNARY_EXPRESSION, PROPERTY_ACCESS_EXPRESSION, PROPERTY_ASSIGNMENT, PROPERTY_DECLARATION,
    PROPERTY_SIGNATURE, QUALIFIED_NAME, REST_TYPE, RETURN_STATEMENT, SATISFIES_EXPRESSION,
    SET_ACCESSOR, SHORTHAND_PROPERTY_ASSIGNMENT, SOURCE_FILE, SPREAD_ASSIGNMENT, SPREAD_ELEMENT,
    SWITCH_STATEMENT, TAGGED_TEMPLATE_EXPRESSION, TEMPLATE_EXPRESSION, TEMPLATE_LITERAL_TYPE,
    TEMPLATE_SPAN, THROW_STATEMENT, TRY_STATEMENT, TUPLE_TYPE, TYPE_ALIAS_DECLARATION,
    TYPE_ASSERTION, TYPE_LITERAL, TYPE_OPERATOR, TYPE_PARAMETER, TYPE_PREDICATE, TYPE_QUERY,
    TYPE_REFERENCE, UNION_TYPE, VARIABLE_DECLARATION, VARIABLE_DECLARATION_LIST,
    VARIABLE_STATEMENT, WHILE_STATEMENT, WITH_STATEMENT, YIELD_EXPRESSION,
};

impl NodeArena {
    #[inline]
    pub(crate) fn add_opt_child(children: &mut Vec<NodeIndex>, idx: NodeIndex) {
        if idx.is_some() {
            children.push(idx);
        }
    }

    #[inline]
    pub(crate) fn add_list(children: &mut Vec<NodeIndex>, list: &NodeList) {
        children.extend(list.nodes.iter().copied());
    }

    #[inline]
    pub(crate) fn add_opt_list(children: &mut Vec<NodeIndex>, list: Option<&NodeList>) {
        if let Some(l) = list {
            children.extend(l.nodes.iter().copied());
        }
    }

    pub(crate) fn collect_name_children(&self, node: &Node, children: &mut Vec<NodeIndex>) -> bool {
        match node.kind {
            QUALIFIED_NAME => {
                if let Some(data) = self.get_qualified_name(node) {
                    children.push(data.left);
                    children.push(data.right);
                    return true;
                }
            }
            COMPUTED_PROPERTY_NAME => {
                if let Some(data) = self.get_computed_property(node) {
                    children.push(data.expression);
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    pub(crate) fn collect_expression_children(
        &self,
        node: &Node,
        children: &mut Vec<NodeIndex>,
    ) -> bool {
        match node.kind {
            BINARY_EXPRESSION => {
                if let Some(data) = self.get_binary_expr(node) {
                    children.push(data.left);
                    children.push(data.right);
                    return true;
                }
            }
            PREFIX_UNARY_EXPRESSION | POSTFIX_UNARY_EXPRESSION => {
                if let Some(data) = self.get_unary_expr(node) {
                    children.push(data.operand);
                    return true;
                }
            }
            CALL_EXPRESSION | NEW_EXPRESSION => {
                if let Some(data) = self.get_call_expr(node) {
                    children.push(data.expression);
                    Self::add_opt_list(children, data.type_arguments.as_ref());
                    Self::add_opt_list(children, data.arguments.as_ref());
                    return true;
                }
            }
            TAGGED_TEMPLATE_EXPRESSION => {
                if let Some(data) = self.get_tagged_template(node) {
                    children.push(data.tag);
                    Self::add_opt_list(children, data.type_arguments.as_ref());
                    children.push(data.template);
                    return true;
                }
            }
            TEMPLATE_EXPRESSION => {
                if let Some(data) = self.get_template_expr(node) {
                    children.push(data.head);
                    Self::add_list(children, &data.template_spans);
                    return true;
                }
            }
            TEMPLATE_SPAN => {
                if let Some(data) = self.get_template_span(node) {
                    children.push(data.expression);
                    children.push(data.literal);
                    return true;
                }
            }
            PROPERTY_ACCESS_EXPRESSION | ELEMENT_ACCESS_EXPRESSION => {
                if let Some(data) = self.get_access_expr(node) {
                    children.push(data.expression);
                    children.push(data.name_or_argument);
                    return true;
                }
            }
            CONDITIONAL_EXPRESSION => {
                if let Some(data) = self.get_conditional_expr(node) {
                    children.push(data.condition);
                    children.push(data.when_true);
                    children.push(data.when_false);
                    return true;
                }
            }
            ARROW_FUNCTION | FUNCTION_EXPRESSION => {
                if let Some(data) = self.get_function(node) {
                    Self::add_opt_list(children, data.modifiers.as_ref());
                    Self::add_opt_list(children, data.type_parameters.as_ref());
                    Self::add_list(children, &data.parameters);
                    Self::add_opt_child(children, data.type_annotation);
                    children.push(data.body);
                    return true;
                }
            }
            ARRAY_LITERAL_EXPRESSION | OBJECT_LITERAL_EXPRESSION => {
                if let Some(data) = self.get_literal_expr(node) {
                    Self::add_list(children, &data.elements);
                    return true;
                }
            }
            PARENTHESIZED_EXPRESSION => {
                if let Some(data) = self.get_parenthesized(node) {
                    children.push(data.expression);
                    return true;
                }
            }
            YIELD_EXPRESSION | AWAIT_EXPRESSION | NON_NULL_EXPRESSION => {
                if let Some(data) = self.get_unary_expr_ex(node) {
                    children.push(data.expression);
                    return true;
                }
            }
            SPREAD_ASSIGNMENT | SPREAD_ELEMENT => {
                if let Some(data) = self.get_spread(node) {
                    children.push(data.expression);
                    return true;
                }
            }
            AS_EXPRESSION | SATISFIES_EXPRESSION => {
                if let Some(data) = self.get_type_assertion(node) {
                    children.push(data.expression);
                    children.push(data.type_node);
                    return true;
                }
            }
            TYPE_ASSERTION => {
                if let Some(data) = self.get_type_assertion(node) {
                    children.push(data.type_node);
                    children.push(data.expression);
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    pub(crate) fn collect_statement_children(
        &self,
        node: &Node,
        children: &mut Vec<NodeIndex>,
    ) -> bool {
        match node.kind {
            VARIABLE_STATEMENT => {
                if let Some(data) = self.get_variable(node) {
                    Self::add_opt_list(children, data.modifiers.as_ref());
                    Self::add_list(children, &data.declarations);
                    return true;
                }
            }
            VARIABLE_DECLARATION_LIST => {
                if let Some(data) = self.get_variable(node) {
                    Self::add_list(children, &data.declarations);
                    return true;
                }
            }
            VARIABLE_DECLARATION => {
                if let Some(data) = self.get_variable_declaration(node) {
                    children.push(data.name);
                    Self::add_opt_child(children, data.type_annotation);
                    Self::add_opt_child(children, data.initializer);
                    return true;
                }
            }
            EXPRESSION_STATEMENT => {
                if let Some(data) = self.get_expression_statement(node) {
                    children.push(data.expression);
                    return true;
                }
            }
            IF_STATEMENT => {
                if let Some(data) = self.get_if_statement(node) {
                    children.push(data.expression);
                    children.push(data.then_statement);
                    Self::add_opt_child(children, data.else_statement);
                    return true;
                }
            }
            WHILE_STATEMENT | DO_STATEMENT | FOR_STATEMENT => {
                if let Some(data) = self.get_loop(node) {
                    Self::add_opt_child(children, data.initializer);
                    Self::add_opt_child(children, data.condition);
                    Self::add_opt_child(children, data.incrementor);
                    children.push(data.statement);
                    return true;
                }
            }
            FOR_IN_STATEMENT | FOR_OF_STATEMENT => {
                if let Some(data) = self.get_for_in_of(node) {
                    children.push(data.initializer);
                    children.push(data.expression);
                    children.push(data.statement);
                    return true;
                }
            }
            SWITCH_STATEMENT => {
                if let Some(data) = self.get_switch(node) {
                    children.push(data.expression);
                    children.push(data.case_block);
                    return true;
                }
            }
            CASE_BLOCK | BLOCK | CLASS_STATIC_BLOCK_DECLARATION => {
                if let Some(data) = self.get_block(node) {
                    Self::add_list(children, &data.statements);
                    return true;
                }
            }
            CASE_CLAUSE | DEFAULT_CLAUSE => {
                if let Some(data) = self.get_case_clause(node) {
                    Self::add_opt_child(children, data.expression);
                    Self::add_list(children, &data.statements);
                    return true;
                }
            }
            RETURN_STATEMENT => {
                if let Some(data) = self.get_return_statement(node) {
                    Self::add_opt_child(children, data.expression);
                    return true;
                }
            }
            THROW_STATEMENT => {
                if let Some(data) = self.get_return_statement(node) {
                    children.push(data.expression);
                    return true;
                }
            }
            TRY_STATEMENT => {
                if let Some(data) = self.get_try(node) {
                    children.push(data.try_block);
                    Self::add_opt_child(children, data.catch_clause);
                    Self::add_opt_child(children, data.finally_block);
                    return true;
                }
            }
            CATCH_CLAUSE => {
                if let Some(data) = self.get_catch_clause(node) {
                    Self::add_opt_child(children, data.variable_declaration);
                    children.push(data.block);
                    return true;
                }
            }
            LABELED_STATEMENT => {
                if let Some(data) = self.get_labeled_statement(node) {
                    children.push(data.label);
                    children.push(data.statement);
                    return true;
                }
            }
            BREAK_STATEMENT | CONTINUE_STATEMENT => {
                if let Some(data) = self.get_jump_data(node) {
                    Self::add_opt_child(children, data.label);
                    return true;
                }
            }
            WITH_STATEMENT => {
                if let Some(data) = self.get_with_statement(node) {
                    children.push(data.expression);
                    children.push(data.then_statement);
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    pub(crate) fn collect_declaration_children(
        &self,
        node: &Node,
        children: &mut Vec<NodeIndex>,
    ) -> bool {
        match node.kind {
            FUNCTION_DECLARATION => {
                if let Some(data) = self.get_function(node) {
                    Self::add_opt_list(children, data.modifiers.as_ref());
                    Self::add_opt_child(children, data.name);
                    Self::add_opt_list(children, data.type_parameters.as_ref());
                    Self::add_list(children, &data.parameters);
                    Self::add_opt_child(children, data.type_annotation);
                    children.push(data.body);
                    return true;
                }
            }
            CLASS_DECLARATION | CLASS_EXPRESSION => {
                if let Some(data) = self.get_class(node) {
                    Self::add_opt_list(children, data.modifiers.as_ref());
                    Self::add_opt_child(children, data.name);
                    Self::add_opt_list(children, data.type_parameters.as_ref());
                    Self::add_opt_list(children, data.heritage_clauses.as_ref());
                    Self::add_list(children, &data.members);
                    return true;
                }
            }
            INTERFACE_DECLARATION => {
                if let Some(data) = self.get_interface(node) {
                    Self::add_opt_list(children, data.modifiers.as_ref());
                    Self::add_opt_child(children, data.name);
                    Self::add_opt_list(children, data.type_parameters.as_ref());
                    Self::add_opt_list(children, data.heritage_clauses.as_ref());
                    Self::add_list(children, &data.members);
                    return true;
                }
            }
            TYPE_ALIAS_DECLARATION => {
                if let Some(data) = self.get_type_alias(node) {
                    Self::add_opt_list(children, data.modifiers.as_ref());
                    Self::add_opt_child(children, data.name);
                    Self::add_opt_list(children, data.type_parameters.as_ref());
                    children.push(data.type_node);
                    return true;
                }
            }
            ENUM_DECLARATION => {
                if let Some(data) = self.get_enum(node) {
                    Self::add_opt_list(children, data.modifiers.as_ref());
                    Self::add_opt_child(children, data.name);
                    Self::add_list(children, &data.members);
                    return true;
                }
            }
            ENUM_MEMBER => {
                if let Some(data) = self.get_enum_member(node) {
                    Self::add_opt_child(children, data.name);
                    Self::add_opt_child(children, data.initializer);
                    return true;
                }
            }
            MODULE_DECLARATION => {
                if let Some(data) = self.get_module(node) {
                    Self::add_opt_list(children, data.modifiers.as_ref());
                    Self::add_opt_child(children, data.name);
                    Self::add_opt_child(children, data.body);
                    return true;
                }
            }
            MODULE_BLOCK => {
                if let Some(data) = self.get_module_block(node) {
                    Self::add_opt_list(children, data.statements.as_ref());
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    pub(crate) fn collect_import_export_children(
        &self,
        node: &Node,
        children: &mut Vec<NodeIndex>,
    ) -> bool {
        match node.kind {
            IMPORT_DECLARATION | IMPORT_EQUALS_DECLARATION => {
                if let Some(data) = self.get_import_decl(node) {
                    Self::add_opt_list(children, data.modifiers.as_ref());
                    Self::add_opt_child(children, data.import_clause);
                    children.push(data.module_specifier);
                    Self::add_opt_child(children, data.attributes);
                    return true;
                }
            }
            IMPORT_CLAUSE => {
                if let Some(data) = self.get_import_clause(node) {
                    Self::add_opt_child(children, data.name);
                    Self::add_opt_child(children, data.named_bindings);
                    return true;
                }
            }
            NAMESPACE_IMPORT | NAMESPACE_EXPORT => {
                if let Some(data) = self.get_named_imports(node) {
                    children.push(data.name);
                    return true;
                }
            }
            NAMED_IMPORTS | NAMED_EXPORTS => {
                if let Some(data) = self.get_named_imports(node) {
                    Self::add_list(children, &data.elements);
                    return true;
                }
            }
            IMPORT_SPECIFIER | EXPORT_SPECIFIER => {
                if let Some(data) = self.get_specifier(node) {
                    Self::add_opt_child(children, data.property_name);
                    children.push(data.name);
                    return true;
                }
            }
            EXPORT_DECLARATION => {
                if let Some(data) = self.get_export_decl(node) {
                    Self::add_opt_list(children, data.modifiers.as_ref());
                    Self::add_opt_child(children, data.export_clause);
                    Self::add_opt_child(children, data.module_specifier);
                    Self::add_opt_child(children, data.attributes);
                    return true;
                }
            }
            EXPORT_ASSIGNMENT => {
                if let Some(data) = self.get_export_assignment(node) {
                    Self::add_opt_list(children, data.modifiers.as_ref());
                    children.push(data.expression);
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    pub(crate) fn collect_type_children(&self, node: &Node, children: &mut Vec<NodeIndex>) -> bool {
        match node.kind {
            TYPE_REFERENCE => {
                if let Some(data) = self.get_type_ref(node) {
                    children.push(data.type_name);
                    Self::add_opt_list(children, data.type_arguments.as_ref());
                    return true;
                }
            }
            FUNCTION_TYPE | CONSTRUCTOR_TYPE => {
                if let Some(data) = self.get_function_type(node) {
                    Self::add_opt_list(children, data.type_parameters.as_ref());
                    Self::add_list(children, &data.parameters);
                    children.push(data.type_annotation);
                    return true;
                }
            }
            TYPE_QUERY => {
                if let Some(data) = self.get_type_query(node) {
                    children.push(data.expr_name);
                    Self::add_opt_list(children, data.type_arguments.as_ref());
                    return true;
                }
            }
            TYPE_LITERAL => {
                if let Some(data) = self.get_type_literal(node) {
                    Self::add_list(children, &data.members);
                    return true;
                }
            }
            ARRAY_TYPE => {
                if let Some(data) = self.get_array_type(node) {
                    children.push(data.element_type);
                    return true;
                }
            }
            TUPLE_TYPE => {
                if let Some(data) = self.get_tuple_type(node) {
                    Self::add_list(children, &data.elements);
                    return true;
                }
            }
            OPTIONAL_TYPE | REST_TYPE | PARENTHESIZED_TYPE => {
                if let Some(data) = self.get_wrapped_type(node) {
                    children.push(data.type_node);
                    return true;
                }
            }
            UNION_TYPE | INTERSECTION_TYPE => {
                if let Some(data) = self.get_composite_type(node) {
                    Self::add_list(children, &data.types);
                    return true;
                }
            }
            CONDITIONAL_TYPE => {
                if let Some(data) = self.get_conditional_type(node) {
                    children.push(data.check_type);
                    children.push(data.extends_type);
                    children.push(data.true_type);
                    children.push(data.false_type);
                    return true;
                }
            }
            INFER_TYPE => {
                if let Some(data) = self.get_infer_type(node) {
                    children.push(data.type_parameter);
                    return true;
                }
            }
            TYPE_OPERATOR => {
                if let Some(data) = self.get_type_operator(node) {
                    children.push(data.type_node);
                    return true;
                }
            }
            INDEXED_ACCESS_TYPE => {
                if let Some(data) = self.get_indexed_access_type(node) {
                    children.push(data.object_type);
                    children.push(data.index_type);
                    return true;
                }
            }
            MAPPED_TYPE => {
                if let Some(data) = self.get_mapped_type(node) {
                    Self::add_opt_child(children, data.type_parameter);
                    Self::add_opt_child(children, data.name_type);
                    Self::add_opt_child(children, data.type_node);
                    Self::add_opt_list(children, data.members.as_ref());
                    return true;
                }
            }
            LITERAL_TYPE => {
                if let Some(data) = self.get_literal_type(node) {
                    Self::add_opt_child(children, data.literal);
                    return true;
                }
            }
            TEMPLATE_LITERAL_TYPE => {
                if let Some(data) = self.get_template_literal_type(node) {
                    children.push(data.head);
                    Self::add_list(children, &data.template_spans);
                    return true;
                }
            }
            NAMED_TUPLE_MEMBER => {
                if let Some(data) = self.get_named_tuple_member(node) {
                    children.push(data.name);
                    children.push(data.type_node);
                    return true;
                }
            }
            TYPE_PREDICATE => {
                if let Some(data) = self.get_type_predicate(node) {
                    children.push(data.parameter_name);
                    Self::add_opt_child(children, data.type_node);
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    pub(crate) fn collect_member_children(
        &self,
        node: &Node,
        children: &mut Vec<NodeIndex>,
    ) -> bool {
        match node.kind {
            PROPERTY_DECLARATION => {
                if let Some(data) = self.get_property_decl(node) {
                    Self::add_opt_list(children, data.modifiers.as_ref());
                    Self::add_opt_child(children, data.name);
                    Self::add_opt_child(children, data.type_annotation);
                    Self::add_opt_child(children, data.initializer);
                    return true;
                }
            }
            METHOD_DECLARATION => {
                if let Some(data) = self.get_method_decl(node) {
                    Self::add_opt_list(children, data.modifiers.as_ref());
                    Self::add_opt_child(children, data.name);
                    Self::add_opt_list(children, data.type_parameters.as_ref());
                    Self::add_list(children, &data.parameters);
                    Self::add_opt_child(children, data.type_annotation);
                    children.push(data.body);
                    return true;
                }
            }
            CONSTRUCTOR => {
                if let Some(data) = self.get_constructor(node) {
                    Self::add_opt_list(children, data.modifiers.as_ref());
                    Self::add_opt_list(children, data.type_parameters.as_ref());
                    Self::add_list(children, &data.parameters);
                    children.push(data.body);
                    return true;
                }
            }
            GET_ACCESSOR | SET_ACCESSOR => {
                if let Some(data) = self.get_accessor(node) {
                    Self::add_opt_list(children, data.modifiers.as_ref());
                    Self::add_opt_child(children, data.name);
                    Self::add_opt_list(children, data.type_parameters.as_ref());
                    Self::add_list(children, &data.parameters);
                    Self::add_opt_child(children, data.type_annotation);
                    children.push(data.body);
                    return true;
                }
            }
            PARAMETER => {
                if let Some(data) = self.get_parameter(node) {
                    Self::add_opt_list(children, data.modifiers.as_ref());
                    Self::add_opt_child(children, data.name);
                    Self::add_opt_child(children, data.type_annotation);
                    Self::add_opt_child(children, data.initializer);
                    return true;
                }
            }
            TYPE_PARAMETER => {
                if let Some(data) = self.get_type_parameter(node) {
                    Self::add_opt_list(children, data.modifiers.as_ref());
                    children.push(data.name);
                    Self::add_opt_child(children, data.constraint);
                    Self::add_opt_child(children, data.default);
                    return true;
                }
            }
            DECORATOR => {
                if let Some(data) = self.get_decorator(node) {
                    children.push(data.expression);
                    return true;
                }
            }
            HERITAGE_CLAUSE => {
                if let Some(data) = self.get_heritage_clause(node) {
                    Self::add_list(children, &data.types);
                    return true;
                }
            }
            EXPRESSION_WITH_TYPE_ARGUMENTS => {
                if let Some(data) = self.get_expr_type_args(node) {
                    children.push(data.expression);
                    Self::add_opt_list(children, data.type_arguments.as_ref());
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    pub(crate) fn collect_pattern_children(
        &self,
        node: &Node,
        children: &mut Vec<NodeIndex>,
    ) -> bool {
        match node.kind {
            OBJECT_BINDING_PATTERN | ARRAY_BINDING_PATTERN => {
                if let Some(data) = self.get_binding_pattern(node) {
                    Self::add_list(children, &data.elements);
                    return true;
                }
            }
            BINDING_ELEMENT => {
                if let Some(data) = self.get_binding_element(node) {
                    Self::add_opt_child(children, data.property_name);
                    children.push(data.name);
                    Self::add_opt_child(children, data.initializer);
                    return true;
                }
            }
            PROPERTY_ASSIGNMENT => {
                if let Some(data) = self.get_property_assignment(node) {
                    Self::add_opt_list(children, data.modifiers.as_ref());
                    Self::add_opt_child(children, data.name);
                    children.push(data.initializer);
                    return true;
                }
            }
            SHORTHAND_PROPERTY_ASSIGNMENT => {
                if let Some(data) = self.get_shorthand_property(node) {
                    Self::add_opt_list(children, data.modifiers.as_ref());
                    children.push(data.name);
                    Self::add_opt_child(children, data.object_assignment_initializer);
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    pub(crate) fn collect_jsx_children(&self, node: &Node, children: &mut Vec<NodeIndex>) -> bool {
        match node.kind {
            JSX_ELEMENT => {
                if let Some(data) = self.get_jsx_element(node) {
                    children.push(data.opening_element);
                    Self::add_list(children, &data.children);
                    Self::add_opt_child(children, data.closing_element);
                    return true;
                }
            }
            JSX_SELF_CLOSING_ELEMENT | JSX_OPENING_ELEMENT => {
                if let Some(data) = self.get_jsx_opening(node) {
                    children.push(data.tag_name);
                    Self::add_opt_list(children, data.type_arguments.as_ref());
                    Self::add_opt_child(children, data.attributes);
                    return true;
                }
            }
            JSX_CLOSING_ELEMENT => {
                if let Some(data) = self.get_jsx_closing(node) {
                    children.push(data.tag_name);
                    return true;
                }
            }
            JSX_FRAGMENT => {
                if let Some(data) = self.get_jsx_fragment(node) {
                    children.push(data.opening_fragment);
                    Self::add_list(children, &data.children);
                    children.push(data.closing_fragment);
                    return true;
                }
            }
            JSX_ATTRIBUTES => {
                if let Some(data) = self.get_jsx_attributes(node) {
                    Self::add_list(children, &data.properties);
                    return true;
                }
            }
            JSX_ATTRIBUTE => {
                if let Some(data) = self.get_jsx_attribute(node) {
                    children.push(data.name);
                    Self::add_opt_child(children, data.initializer);
                    return true;
                }
            }
            JSX_SPREAD_ATTRIBUTE => {
                if let Some(data) = self.get_jsx_spread_attribute(node) {
                    children.push(data.expression);
                    return true;
                }
            }
            JSX_EXPRESSION => {
                if let Some(data) = self.get_jsx_expression(node) {
                    Self::add_opt_child(children, data.expression);
                    return true;
                }
            }
            JSX_NAMESPACED_NAME => {
                if let Some(data) = self.get_jsx_namespaced_name(node) {
                    children.push(data.namespace);
                    children.push(data.name);
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    pub(crate) fn collect_signature_children(
        &self,
        node: &Node,
        children: &mut Vec<NodeIndex>,
    ) -> bool {
        match node.kind {
            CALL_SIGNATURE | CONSTRUCT_SIGNATURE => {
                if let Some(data) = self.get_signature(node) {
                    Self::add_opt_list(children, data.type_parameters.as_ref());
                    Self::add_opt_list(children, data.parameters.as_ref());
                    Self::add_opt_child(children, data.type_annotation);
                    return true;
                }
            }
            INDEX_SIGNATURE => {
                if let Some(data) = self.get_index_signature(node) {
                    Self::add_opt_list(children, data.modifiers.as_ref());
                    Self::add_list(children, &data.parameters);
                    Self::add_opt_child(children, data.type_annotation);
                    return true;
                }
            }
            PROPERTY_SIGNATURE => {
                if let Some(data) = self.get_signature(node) {
                    Self::add_opt_list(children, data.modifiers.as_ref());
                    Self::add_opt_child(children, data.name);
                    Self::add_opt_child(children, data.type_annotation);
                    return true;
                }
            }
            METHOD_SIGNATURE => {
                if let Some(data) = self.get_signature(node) {
                    Self::add_opt_list(children, data.modifiers.as_ref());
                    Self::add_opt_child(children, data.name);
                    Self::add_opt_list(children, data.type_parameters.as_ref());
                    Self::add_opt_list(children, data.parameters.as_ref());
                    Self::add_opt_child(children, data.type_annotation);
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    pub(crate) fn collect_source_children(
        &self,
        node: &Node,
        children: &mut Vec<NodeIndex>,
    ) -> bool {
        if node.kind == SOURCE_FILE
            && let Some(data) = self.get_source_file(node)
        {
            Self::add_list(children, &data.statements);
            children.push(data.end_of_file_token);
            return true;
        }
        false
    }
}
