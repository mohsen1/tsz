//! NodeArena access methods, NodeView, and NodeAccess trait.
//!
//! This module contains all node access/query methods, the NodeView ergonomic wrapper,
//! Node kind utility methods, and the NodeAccess trait.

use super::base::{NodeIndex, NodeList};
use super::node::*;

impl NodeArena {
    /// Get a thin node by index
    #[inline]
    pub fn get(&self, index: NodeIndex) -> Option<&Node> {
        if index.is_none() {
            None
        } else {
            self.nodes.get(index.0 as usize)
        }
    }

    /// Get a mutable thin node by index
    #[inline]
    pub fn get_mut(&mut self, index: NodeIndex) -> Option<&mut Node> {
        if index.is_none() {
            None
        } else {
            self.nodes.get_mut(index.0 as usize)
        }
    }

    /// Get extended info for a node
    #[inline]
    pub fn get_extended(&self, index: NodeIndex) -> Option<&ExtendedNodeInfo> {
        if index.is_none() {
            None
        } else {
            self.extended_info.get(index.0 as usize)
        }
    }

    /// Get mutable extended info for a node
    #[inline]
    pub fn get_extended_mut(&mut self, index: NodeIndex) -> Option<&mut ExtendedNodeInfo> {
        if index.is_none() {
            None
        } else {
            self.extended_info.get_mut(index.0 as usize)
        }
    }

    /// Get identifier data for a node.
    /// Returns None if node is not an identifier or has no data.
    #[inline]
    pub fn get_identifier(&self, node: &Node) -> Option<&IdentifierData> {
        use tsz_scanner::SyntaxKind;
        if node.has_data()
            && (node.kind == SyntaxKind::Identifier as u16
                || node.kind == SyntaxKind::PrivateIdentifier as u16)
        {
            self.identifiers.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get literal data for a node.
    /// Returns None if node is not a literal or has no data.
    #[inline]
    pub fn get_literal(&self, node: &Node) -> Option<&LiteralData> {
        use tsz_scanner::SyntaxKind;
        if node.has_data()
            && matches!(node.kind,
                k if k == SyntaxKind::StringLiteral as u16 ||
                     k == SyntaxKind::NumericLiteral as u16 ||
                     k == SyntaxKind::BigIntLiteral as u16 ||
                     k == SyntaxKind::RegularExpressionLiteral as u16 ||
                     k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 ||
                     k == SyntaxKind::TemplateHead as u16 ||
                     k == SyntaxKind::TemplateMiddle as u16 ||
                     k == SyntaxKind::TemplateTail as u16
            )
        {
            self.literals.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get binary expression data.
    /// Returns None if node is not a binary expression or has no data.
    #[inline]
    pub fn get_binary_expr(&self, node: &Node) -> Option<&BinaryExprData> {
        use super::syntax_kind_ext::BINARY_EXPRESSION;
        if node.has_data() && node.kind == BINARY_EXPRESSION {
            self.binary_exprs.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get call expression data.
    /// Returns None if node is not a call/new expression or has no data.
    #[inline]
    pub fn get_call_expr(&self, node: &Node) -> Option<&CallExprData> {
        use super::syntax_kind_ext::{CALL_EXPRESSION, NEW_EXPRESSION};
        if node.has_data() && (node.kind == CALL_EXPRESSION || node.kind == NEW_EXPRESSION) {
            self.call_exprs.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get access expression data (property access or element access).
    /// Returns None if node is not an access expression or has no data.
    #[inline]
    pub fn get_access_expr(&self, node: &Node) -> Option<&AccessExprData> {
        use super::syntax_kind_ext::{ELEMENT_ACCESS_EXPRESSION, PROPERTY_ACCESS_EXPRESSION};
        if node.has_data()
            && (node.kind == PROPERTY_ACCESS_EXPRESSION || node.kind == ELEMENT_ACCESS_EXPRESSION)
        {
            self.access_exprs.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get conditional expression data (ternary: a ? b : c).
    /// Returns None if node is not a conditional expression or has no data.
    #[inline]
    pub fn get_conditional_expr(&self, node: &Node) -> Option<&ConditionalExprData> {
        use super::syntax_kind_ext::CONDITIONAL_EXPRESSION;
        if node.has_data() && node.kind == CONDITIONAL_EXPRESSION {
            self.conditional_exprs.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get qualified name data (A.B syntax).
    /// Returns None if node is not a qualified name or has no data.
    #[inline]
    pub fn get_qualified_name(&self, node: &Node) -> Option<&QualifiedNameData> {
        use super::syntax_kind_ext::QUALIFIED_NAME;
        if node.has_data() && node.kind == QUALIFIED_NAME {
            self.qualified_names.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get literal expression data (array or object literal).
    /// Returns None if node is not a literal expression or has no data.
    #[inline]
    pub fn get_literal_expr(&self, node: &Node) -> Option<&LiteralExprData> {
        use super::syntax_kind_ext::{ARRAY_LITERAL_EXPRESSION, OBJECT_LITERAL_EXPRESSION};
        if node.has_data()
            && (node.kind == ARRAY_LITERAL_EXPRESSION || node.kind == OBJECT_LITERAL_EXPRESSION)
        {
            self.literal_exprs.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get property assignment data.
    /// Returns None if node is not a property assignment or has no data.
    #[inline]
    pub fn get_property_assignment(&self, node: &Node) -> Option<&PropertyAssignmentData> {
        use super::syntax_kind_ext::PROPERTY_ASSIGNMENT;
        if node.has_data() && node.kind == PROPERTY_ASSIGNMENT {
            self.property_assignments.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get type assertion data (as/satisfies/type assertion).
    /// Returns None if node is not a type assertion or has no data.
    #[inline]
    pub fn get_type_assertion(&self, node: &Node) -> Option<&TypeAssertionData> {
        use super::syntax_kind_ext::{AS_EXPRESSION, SATISFIES_EXPRESSION, TYPE_ASSERTION};
        if node.has_data()
            && (node.kind == TYPE_ASSERTION
                || node.kind == AS_EXPRESSION
                || node.kind == SATISFIES_EXPRESSION)
        {
            self.type_assertions.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get unary expression data (prefix or postfix).
    /// Returns None if node is not a unary expression or has no data.
    #[inline]
    pub fn get_unary_expr(&self, node: &Node) -> Option<&UnaryExprData> {
        use super::syntax_kind_ext::{POSTFIX_UNARY_EXPRESSION, PREFIX_UNARY_EXPRESSION};
        if node.has_data()
            && (node.kind == PREFIX_UNARY_EXPRESSION || node.kind == POSTFIX_UNARY_EXPRESSION)
        {
            self.unary_exprs.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get extended unary expression data (await/yield/non-null/spread).
    /// Returns None if node is not an await/yield/non-null/spread expression or has no data.
    #[inline]
    pub fn get_unary_expr_ex(&self, node: &Node) -> Option<&UnaryExprDataEx> {
        use super::syntax_kind_ext::{
            AWAIT_EXPRESSION, NON_NULL_EXPRESSION, SPREAD_ELEMENT, YIELD_EXPRESSION,
        };
        if node.has_data()
            && (node.kind == AWAIT_EXPRESSION
                || node.kind == YIELD_EXPRESSION
                || node.kind == NON_NULL_EXPRESSION
                || node.kind == SPREAD_ELEMENT)
        {
            self.unary_exprs_ex.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get function data.
    /// Returns None if node is not a function-like node or has no data.
    #[inline]
    pub fn get_function(&self, node: &Node) -> Option<&FunctionData> {
        use super::syntax_kind_ext::*;
        if node.has_data()
            && matches!(
                node.kind,
                FUNCTION_DECLARATION | FUNCTION_EXPRESSION | ARROW_FUNCTION
            )
        {
            self.functions.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get class data.
    /// Returns None if node is not a class declaration/expression or has no data.
    #[inline]
    pub fn get_class(&self, node: &Node) -> Option<&ClassData> {
        use super::syntax_kind_ext::{CLASS_DECLARATION, CLASS_EXPRESSION};
        if node.has_data() && (node.kind == CLASS_DECLARATION || node.kind == CLASS_EXPRESSION) {
            self.classes.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get block data.
    /// Returns None if node is not a block or has no data.
    #[inline]
    pub fn get_block(&self, node: &Node) -> Option<&BlockData> {
        use super::syntax_kind_ext::{BLOCK, CASE_BLOCK, CLASS_STATIC_BLOCK_DECLARATION};
        if node.has_data()
            && (node.kind == BLOCK
                || node.kind == CLASS_STATIC_BLOCK_DECLARATION
                || node.kind == CASE_BLOCK)
        {
            self.blocks.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get source file data.
    /// Returns None if node is not a source file or has no data.
    #[inline]
    pub fn get_source_file(&self, node: &Node) -> Option<&SourceFileData> {
        use super::syntax_kind_ext::SOURCE_FILE;
        if node.has_data() && node.kind == SOURCE_FILE {
            self.source_files.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get variable data (VariableStatement or VariableDeclarationList).
    #[inline]
    pub fn get_variable(&self, node: &Node) -> Option<&VariableData> {
        use super::syntax_kind_ext::{VARIABLE_DECLARATION_LIST, VARIABLE_STATEMENT};
        if node.has_data()
            && (node.kind == VARIABLE_STATEMENT || node.kind == VARIABLE_DECLARATION_LIST)
        {
            self.variables.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get variable declaration data.
    #[inline]
    pub fn get_variable_declaration(&self, node: &Node) -> Option<&VariableDeclarationData> {
        use super::syntax_kind_ext::VARIABLE_DECLARATION;
        if node.has_data() && node.kind == VARIABLE_DECLARATION {
            self.variable_declarations.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get interface data.
    #[inline]
    pub fn get_interface(&self, node: &Node) -> Option<&InterfaceData> {
        use super::syntax_kind_ext::INTERFACE_DECLARATION;
        if node.has_data() && node.kind == INTERFACE_DECLARATION {
            self.interfaces.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get type alias data.
    #[inline]
    pub fn get_type_alias(&self, node: &Node) -> Option<&TypeAliasData> {
        use super::syntax_kind_ext::TYPE_ALIAS_DECLARATION;
        if node.has_data() && node.kind == TYPE_ALIAS_DECLARATION {
            self.type_aliases.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get enum data.
    #[inline]
    pub fn get_enum(&self, node: &Node) -> Option<&EnumData> {
        use super::syntax_kind_ext::ENUM_DECLARATION;
        if node.has_data() && node.kind == ENUM_DECLARATION {
            self.enums.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get enum member data.
    #[inline]
    pub fn get_enum_member(&self, node: &Node) -> Option<&EnumMemberData> {
        use super::syntax_kind_ext::ENUM_MEMBER;
        if node.has_data() && node.kind == ENUM_MEMBER {
            self.enum_members.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get module data.
    #[inline]
    pub fn get_module(&self, node: &Node) -> Option<&ModuleData> {
        use super::syntax_kind_ext::MODULE_DECLARATION;
        if node.has_data() && node.kind == MODULE_DECLARATION {
            self.modules.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get module block data.
    #[inline]
    pub fn get_module_block(&self, node: &Node) -> Option<&ModuleBlockData> {
        use super::syntax_kind_ext::MODULE_BLOCK;
        if node.has_data() && node.kind == MODULE_BLOCK {
            self.module_blocks.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get if statement data.
    #[inline]
    pub fn get_if_statement(&self, node: &Node) -> Option<&IfStatementData> {
        use super::syntax_kind_ext::IF_STATEMENT;
        if node.has_data() && node.kind == IF_STATEMENT {
            self.if_statements.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get loop data (while, for, do-while).
    #[inline]
    pub fn get_loop(&self, node: &Node) -> Option<&LoopData> {
        use super::syntax_kind_ext::{DO_STATEMENT, FOR_STATEMENT, WHILE_STATEMENT};
        if node.has_data()
            && (node.kind == WHILE_STATEMENT
                || node.kind == DO_STATEMENT
                || node.kind == FOR_STATEMENT)
        {
            self.loops.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get for-in/for-of data.
    #[inline]
    pub fn get_for_in_of(&self, node: &Node) -> Option<&ForInOfData> {
        use super::syntax_kind_ext::{FOR_IN_STATEMENT, FOR_OF_STATEMENT};
        if node.has_data() && (node.kind == FOR_IN_STATEMENT || node.kind == FOR_OF_STATEMENT) {
            self.for_in_of.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get switch data.
    #[inline]
    pub fn get_switch(&self, node: &Node) -> Option<&SwitchData> {
        use super::syntax_kind_ext::SWITCH_STATEMENT;
        if node.has_data() && node.kind == SWITCH_STATEMENT {
            self.switch_data.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get case clause data.
    #[inline]
    pub fn get_case_clause(&self, node: &Node) -> Option<&CaseClauseData> {
        use super::syntax_kind_ext::{CASE_CLAUSE, DEFAULT_CLAUSE};
        if node.has_data() && (node.kind == CASE_CLAUSE || node.kind == DEFAULT_CLAUSE) {
            self.case_clauses.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get try data.
    #[inline]
    pub fn get_try(&self, node: &Node) -> Option<&TryData> {
        use super::syntax_kind_ext::TRY_STATEMENT;
        if node.has_data() && node.kind == TRY_STATEMENT {
            self.try_data.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get catch clause data.
    #[inline]
    pub fn get_catch_clause(&self, node: &Node) -> Option<&CatchClauseData> {
        use super::syntax_kind_ext::CATCH_CLAUSE;
        if node.has_data() && node.kind == CATCH_CLAUSE {
            self.catch_clauses.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get labeled statement data.
    #[inline]
    pub fn get_labeled_statement(&self, node: &Node) -> Option<&LabeledData> {
        use super::syntax_kind_ext::LABELED_STATEMENT;
        if node.has_data() && node.kind == LABELED_STATEMENT {
            self.labeled_data.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get jump data (break/continue statements).
    #[inline]
    pub fn get_jump_data(&self, node: &Node) -> Option<&JumpData> {
        use super::syntax_kind_ext::{BREAK_STATEMENT, CONTINUE_STATEMENT};
        if node.has_data() && (node.kind == BREAK_STATEMENT || node.kind == CONTINUE_STATEMENT) {
            self.jump_data.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get with statement data (stored in if statement pool).
    #[inline]
    pub fn get_with_statement(&self, node: &Node) -> Option<&IfStatementData> {
        use super::syntax_kind_ext::WITH_STATEMENT;
        if node.has_data() && node.kind == WITH_STATEMENT {
            self.if_statements.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get import declaration data (handles both IMPORT_DECLARATION and IMPORT_EQUALS_DECLARATION).
    #[inline]
    pub fn get_import_decl(&self, node: &Node) -> Option<&ImportDeclData> {
        use super::syntax_kind_ext::{IMPORT_DECLARATION, IMPORT_EQUALS_DECLARATION};
        if node.has_data()
            && (node.kind == IMPORT_DECLARATION || node.kind == IMPORT_EQUALS_DECLARATION)
        {
            self.import_decls.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get import clause data.
    #[inline]
    pub fn get_import_clause(&self, node: &Node) -> Option<&ImportClauseData> {
        use super::syntax_kind_ext::IMPORT_CLAUSE;
        if node.has_data() && node.kind == IMPORT_CLAUSE {
            self.import_clauses.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get named imports/exports data.
    /// Works for NAMED_IMPORTS, NAMESPACE_IMPORT, and NAMED_EXPORTS (they share the same data structure).
    #[inline]
    pub fn get_named_imports(&self, node: &Node) -> Option<&NamedImportsData> {
        use super::syntax_kind_ext::{NAMED_EXPORTS, NAMED_IMPORTS, NAMESPACE_IMPORT};
        if node.has_data()
            && (node.kind == NAMED_IMPORTS
                || node.kind == NAMED_EXPORTS
                || node.kind == NAMESPACE_IMPORT)
        {
            self.named_imports.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get import/export specifier data.
    #[inline]
    pub fn get_specifier(&self, node: &Node) -> Option<&SpecifierData> {
        use super::syntax_kind_ext::{EXPORT_SPECIFIER, IMPORT_SPECIFIER};
        if node.has_data() && (node.kind == IMPORT_SPECIFIER || node.kind == EXPORT_SPECIFIER) {
            self.specifiers.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get export declaration data.
    #[inline]
    pub fn get_export_decl(&self, node: &Node) -> Option<&ExportDeclData> {
        use super::syntax_kind_ext::EXPORT_DECLARATION;
        if node.has_data() && node.kind == EXPORT_DECLARATION {
            self.export_decls.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get export assignment data (export = expr).
    #[inline]
    pub fn get_export_assignment(&self, node: &Node) -> Option<&ExportAssignmentData> {
        use super::syntax_kind_ext::EXPORT_ASSIGNMENT;
        if node.has_data() && node.kind == EXPORT_ASSIGNMENT {
            self.export_assignments.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get parameter data.
    #[inline]
    pub fn get_parameter(&self, node: &Node) -> Option<&ParameterData> {
        use super::syntax_kind_ext::PARAMETER;
        if node.has_data() && node.kind == PARAMETER {
            self.parameters.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get property declaration data.
    #[inline]
    pub fn get_property_decl(&self, node: &Node) -> Option<&PropertyDeclData> {
        use super::syntax_kind_ext::PROPERTY_DECLARATION;
        if node.has_data() && node.kind == PROPERTY_DECLARATION {
            self.property_decls.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get method declaration data.
    #[inline]
    pub fn get_method_decl(&self, node: &Node) -> Option<&MethodDeclData> {
        use super::syntax_kind_ext::METHOD_DECLARATION;
        if node.has_data() && node.kind == METHOD_DECLARATION {
            self.method_decls.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get constructor data.
    #[inline]
    pub fn get_constructor(&self, node: &Node) -> Option<&ConstructorData> {
        use super::syntax_kind_ext::CONSTRUCTOR;
        if node.has_data() && node.kind == CONSTRUCTOR {
            self.constructors.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get accessor data (get/set accessor).
    #[inline]
    pub fn get_accessor(&self, node: &Node) -> Option<&AccessorData> {
        use super::syntax_kind_ext::{GET_ACCESSOR, SET_ACCESSOR};
        if node.has_data() && (node.kind == GET_ACCESSOR || node.kind == SET_ACCESSOR) {
            self.accessors.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get decorator data.
    #[inline]
    pub fn get_decorator(&self, node: &Node) -> Option<&DecoratorData> {
        use super::syntax_kind_ext::DECORATOR;
        if node.has_data() && node.kind == DECORATOR {
            self.decorators.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get type reference data.
    #[inline]
    pub fn get_type_ref(&self, node: &Node) -> Option<&TypeRefData> {
        use super::syntax_kind_ext::TYPE_REFERENCE;
        if node.has_data() && node.kind == TYPE_REFERENCE {
            self.type_refs.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get expression statement data (returns the expression node index).
    #[inline]
    pub fn get_expression_statement(&self, node: &Node) -> Option<&ExprStatementData> {
        use super::syntax_kind_ext::EXPRESSION_STATEMENT;
        if node.has_data() && node.kind == EXPRESSION_STATEMENT {
            self.expr_statements.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get return statement data (returns the expression node index).
    #[inline]
    pub fn get_return_statement(&self, node: &Node) -> Option<&ReturnData> {
        use super::syntax_kind_ext::{RETURN_STATEMENT, THROW_STATEMENT};
        if node.has_data() && (node.kind == RETURN_STATEMENT || node.kind == THROW_STATEMENT) {
            self.return_data.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get JSX element data.
    #[inline]
    pub fn get_jsx_element(&self, node: &Node) -> Option<&JsxElementData> {
        use super::syntax_kind_ext::JSX_ELEMENT;
        if node.has_data() && node.kind == JSX_ELEMENT {
            self.jsx_elements.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get JSX opening/self-closing element data.
    #[inline]
    pub fn get_jsx_opening(&self, node: &Node) -> Option<&JsxOpeningData> {
        use super::syntax_kind_ext::{JSX_OPENING_ELEMENT, JSX_SELF_CLOSING_ELEMENT};
        if node.has_data()
            && (node.kind == JSX_OPENING_ELEMENT || node.kind == JSX_SELF_CLOSING_ELEMENT)
        {
            self.jsx_opening.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get JSX closing element data.
    #[inline]
    pub fn get_jsx_closing(&self, node: &Node) -> Option<&JsxClosingData> {
        use super::syntax_kind_ext::JSX_CLOSING_ELEMENT;
        if node.has_data() && node.kind == JSX_CLOSING_ELEMENT {
            self.jsx_closing.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get JSX fragment data.
    #[inline]
    pub fn get_jsx_fragment(&self, node: &Node) -> Option<&JsxFragmentData> {
        use super::syntax_kind_ext::JSX_FRAGMENT;
        if node.has_data() && node.kind == JSX_FRAGMENT {
            self.jsx_fragments.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get JSX attributes data.
    #[inline]
    pub fn get_jsx_attributes(&self, node: &Node) -> Option<&JsxAttributesData> {
        use super::syntax_kind_ext::JSX_ATTRIBUTES;
        if node.has_data() && node.kind == JSX_ATTRIBUTES {
            self.jsx_attributes.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get JSX attribute data.
    #[inline]
    pub fn get_jsx_attribute(&self, node: &Node) -> Option<&JsxAttributeData> {
        use super::syntax_kind_ext::JSX_ATTRIBUTE;
        if node.has_data() && node.kind == JSX_ATTRIBUTE {
            self.jsx_attribute.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get JSX spread attribute data.
    #[inline]
    pub fn get_jsx_spread_attribute(&self, node: &Node) -> Option<&JsxSpreadAttributeData> {
        use super::syntax_kind_ext::JSX_SPREAD_ATTRIBUTE;
        if node.has_data() && node.kind == JSX_SPREAD_ATTRIBUTE {
            self.jsx_spread_attributes.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get JSX expression data.
    #[inline]
    pub fn get_jsx_expression(&self, node: &Node) -> Option<&JsxExpressionData> {
        use super::syntax_kind_ext::JSX_EXPRESSION;
        if node.has_data() && node.kind == JSX_EXPRESSION {
            self.jsx_expressions.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get JSX text data.
    #[inline]
    pub fn get_jsx_text(&self, node: &Node) -> Option<&JsxTextData> {
        use tsz_scanner::SyntaxKind;
        if node.has_data() && node.kind == SyntaxKind::JsxText as u16 {
            self.jsx_text.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get JSX namespaced name data.
    #[inline]
    pub fn get_jsx_namespaced_name(&self, node: &Node) -> Option<&JsxNamespacedNameData> {
        use super::syntax_kind_ext::JSX_NAMESPACED_NAME;
        if node.has_data() && node.kind == JSX_NAMESPACED_NAME {
            self.jsx_namespaced_names.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get signature data (call, construct, method, property signatures).
    #[inline]
    pub fn get_signature(&self, node: &Node) -> Option<&SignatureData> {
        use super::syntax_kind_ext::{
            CALL_SIGNATURE, CONSTRUCT_SIGNATURE, METHOD_SIGNATURE, PROPERTY_SIGNATURE,
        };
        if node.has_data()
            && (node.kind == CALL_SIGNATURE
                || node.kind == CONSTRUCT_SIGNATURE
                || node.kind == METHOD_SIGNATURE
                || node.kind == PROPERTY_SIGNATURE)
        {
            self.signatures.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get index signature data.
    #[inline]
    pub fn get_index_signature(&self, node: &Node) -> Option<&IndexSignatureData> {
        use super::syntax_kind_ext::INDEX_SIGNATURE;
        if node.has_data() && node.kind == INDEX_SIGNATURE {
            self.index_signatures.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get heritage clause data.
    #[inline]
    pub fn get_heritage_clause(&self, node: &Node) -> Option<&HeritageData> {
        use super::syntax_kind_ext::HERITAGE_CLAUSE;
        if node.has_data() && node.kind == HERITAGE_CLAUSE {
            self.heritage_clauses.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get composite type data (union or intersection).
    #[inline]
    pub fn get_composite_type(&self, node: &Node) -> Option<&CompositeTypeData> {
        use super::syntax_kind_ext::{INTERSECTION_TYPE, UNION_TYPE};
        if node.has_data() && (node.kind == UNION_TYPE || node.kind == INTERSECTION_TYPE) {
            self.composite_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get array type data.
    #[inline]
    pub fn get_array_type(&self, node: &Node) -> Option<&ArrayTypeData> {
        use super::syntax_kind_ext::ARRAY_TYPE;
        if node.has_data() && node.kind == ARRAY_TYPE {
            self.array_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get tuple type data.
    #[inline]
    pub fn get_tuple_type(&self, node: &Node) -> Option<&TupleTypeData> {
        use super::syntax_kind_ext::TUPLE_TYPE;
        if node.has_data() && node.kind == TUPLE_TYPE {
            self.tuple_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get function type data.
    #[inline]
    pub fn get_function_type(&self, node: &Node) -> Option<&FunctionTypeData> {
        use super::syntax_kind_ext::{CONSTRUCTOR_TYPE, FUNCTION_TYPE};
        if node.has_data() && (node.kind == FUNCTION_TYPE || node.kind == CONSTRUCTOR_TYPE) {
            self.function_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get type literal data.
    #[inline]
    pub fn get_type_literal(&self, node: &Node) -> Option<&TypeLiteralData> {
        use super::syntax_kind_ext::TYPE_LITERAL;
        if node.has_data() && node.kind == TYPE_LITERAL {
            self.type_literals.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get conditional type data.
    #[inline]
    pub fn get_conditional_type(&self, node: &Node) -> Option<&ConditionalTypeData> {
        use super::syntax_kind_ext::CONDITIONAL_TYPE;
        if node.has_data() && node.kind == CONDITIONAL_TYPE {
            self.conditional_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get mapped type data.
    #[inline]
    pub fn get_mapped_type(&self, node: &Node) -> Option<&MappedTypeData> {
        use super::syntax_kind_ext::MAPPED_TYPE;
        if node.has_data() && node.kind == MAPPED_TYPE {
            self.mapped_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get indexed access type data.
    #[inline]
    pub fn get_indexed_access_type(&self, node: &Node) -> Option<&IndexedAccessTypeData> {
        use super::syntax_kind_ext::INDEXED_ACCESS_TYPE;
        if node.has_data() && node.kind == INDEXED_ACCESS_TYPE {
            self.indexed_access_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get literal type data.
    #[inline]
    pub fn get_literal_type(&self, node: &Node) -> Option<&LiteralTypeData> {
        use super::syntax_kind_ext::LITERAL_TYPE;
        if node.has_data() && node.kind == LITERAL_TYPE {
            self.literal_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get wrapped type data (parenthesized, optional, rest types).
    #[inline]
    pub fn get_wrapped_type(&self, node: &Node) -> Option<&WrappedTypeData> {
        use super::syntax_kind_ext::{OPTIONAL_TYPE, PARENTHESIZED_TYPE, REST_TYPE};
        if node.has_data()
            && (node.kind == PARENTHESIZED_TYPE
                || node.kind == OPTIONAL_TYPE
                || node.kind == REST_TYPE)
        {
            self.wrapped_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get heritage clause data.
    #[inline]
    pub fn get_heritage(&self, node: &Node) -> Option<&HeritageData> {
        use super::syntax_kind_ext::HERITAGE_CLAUSE;
        if node.has_data() && node.kind == HERITAGE_CLAUSE {
            self.heritage_clauses.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get expression with type arguments data (e.g., `extends Base<T>`).
    #[inline]
    pub fn get_expr_type_args(&self, node: &Node) -> Option<&ExprWithTypeArgsData> {
        use super::syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS;
        if node.has_data() && node.kind == EXPRESSION_WITH_TYPE_ARGUMENTS {
            self.expr_with_type_args.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get type query data (typeof in type position).
    #[inline]
    pub fn get_type_query(&self, node: &Node) -> Option<&TypeQueryData> {
        use super::syntax_kind_ext::TYPE_QUERY;
        if node.has_data() && node.kind == TYPE_QUERY {
            self.type_queries.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get type operator data (keyof, unique, readonly).
    #[inline]
    pub fn get_type_operator(&self, node: &Node) -> Option<&TypeOperatorData> {
        use super::syntax_kind_ext::TYPE_OPERATOR;
        if node.has_data() && node.kind == TYPE_OPERATOR {
            self.type_operators.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get infer type data.
    #[inline]
    pub fn get_infer_type(&self, node: &Node) -> Option<&InferTypeData> {
        use super::syntax_kind_ext::INFER_TYPE;
        if node.has_data() && node.kind == INFER_TYPE {
            self.infer_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get template literal type data.
    #[inline]
    pub fn get_template_literal_type(&self, node: &Node) -> Option<&TemplateLiteralTypeData> {
        use super::syntax_kind_ext::TEMPLATE_LITERAL_TYPE;
        if node.has_data() && node.kind == TEMPLATE_LITERAL_TYPE {
            self.template_literal_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get named tuple member data.
    #[inline]
    pub fn get_named_tuple_member(&self, node: &Node) -> Option<&NamedTupleMemberData> {
        use super::syntax_kind_ext::NAMED_TUPLE_MEMBER;
        if node.has_data() && node.kind == NAMED_TUPLE_MEMBER {
            self.named_tuple_members.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get type predicate data.
    #[inline]
    pub fn get_type_predicate(&self, node: &Node) -> Option<&TypePredicateData> {
        use super::syntax_kind_ext::TYPE_PREDICATE;
        if node.has_data() && node.kind == TYPE_PREDICATE {
            self.type_predicates.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get type parameter data.
    #[inline]
    pub fn get_type_parameter(&self, node: &Node) -> Option<&TypeParameterData> {
        use super::syntax_kind_ext::TYPE_PARAMETER;
        if node.has_data() && node.kind == TYPE_PARAMETER {
            self.type_parameters.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get parenthesized expression data.
    /// Returns None if node is not a parenthesized expression or has no data.
    #[inline]
    pub fn get_parenthesized(&self, node: &Node) -> Option<&ParenthesizedData> {
        use super::syntax_kind_ext::PARENTHESIZED_EXPRESSION;
        if node.has_data() && node.kind == PARENTHESIZED_EXPRESSION {
            self.parenthesized.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get template expression data.
    #[inline]
    pub fn get_template_expr(&self, node: &Node) -> Option<&TemplateExprData> {
        use super::syntax_kind_ext::TEMPLATE_EXPRESSION;
        if node.has_data() && node.kind == TEMPLATE_EXPRESSION {
            self.template_exprs.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get template span data.
    #[inline]
    pub fn get_template_span(&self, node: &Node) -> Option<&TemplateSpanData> {
        use super::syntax_kind_ext::TEMPLATE_SPAN;
        if node.has_data() && node.kind == TEMPLATE_SPAN {
            self.template_spans.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get tagged template expression data.
    #[inline]
    pub fn get_tagged_template(&self, node: &Node) -> Option<&TaggedTemplateData> {
        use super::syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION;
        if node.has_data() && node.kind == TAGGED_TEMPLATE_EXPRESSION {
            self.tagged_templates.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get spread element/assignment data.
    #[inline]
    pub fn get_spread(&self, node: &Node) -> Option<&SpreadData> {
        use super::syntax_kind_ext::{SPREAD_ASSIGNMENT, SPREAD_ELEMENT};
        if node.has_data() && (node.kind == SPREAD_ELEMENT || node.kind == SPREAD_ASSIGNMENT) {
            self.spread_data.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get shorthand property assignment data.
    #[inline]
    pub fn get_shorthand_property(&self, node: &Node) -> Option<&ShorthandPropertyData> {
        use super::syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT;
        if node.has_data() && node.kind == SHORTHAND_PROPERTY_ASSIGNMENT {
            self.shorthand_properties.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get binding pattern data (ObjectBindingPattern or ArrayBindingPattern).
    #[inline]
    pub fn get_binding_pattern(&self, node: &Node) -> Option<&BindingPatternData> {
        use super::syntax_kind_ext::{ARRAY_BINDING_PATTERN, OBJECT_BINDING_PATTERN};
        if node.has_data()
            && (node.kind == OBJECT_BINDING_PATTERN || node.kind == ARRAY_BINDING_PATTERN)
        {
            self.binding_patterns.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get binding element data.
    #[inline]
    pub fn get_binding_element(&self, node: &Node) -> Option<&BindingElementData> {
        use super::syntax_kind_ext::BINDING_ELEMENT;
        if node.has_data() && node.kind == BINDING_ELEMENT {
            self.binding_elements.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get computed property name data
    #[inline]
    pub fn get_computed_property(&self, node: &Node) -> Option<&ComputedPropertyData> {
        use super::syntax_kind_ext::COMPUTED_PROPERTY_NAME;
        if node.has_data() && node.kind == COMPUTED_PROPERTY_NAME {
            self.computed_properties.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Number of nodes in the arena
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if arena is empty
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

// =============================================================================
// Node View - Ergonomic wrapper for reading Nodes
// =============================================================================

/// A view into a node that provides convenient access to both the Node
/// header and its type-specific data. This avoids the need to pass the arena
/// around when working with node data.
#[derive(Clone, Copy)]
pub struct NodeView<'a> {
    pub node: &'a Node,
    pub arena: &'a NodeArena,
    pub index: NodeIndex,
}

impl<'a> NodeView<'a> {
    /// Create a new NodeView
    #[inline]
    pub fn new(arena: &'a NodeArena, index: NodeIndex) -> Option<NodeView<'a>> {
        arena.get(index).map(|node| NodeView { node, arena, index })
    }

    /// Get the SyntaxKind
    #[inline]
    pub fn kind(&self) -> u16 {
        self.node.kind
    }

    /// Get the start position
    #[inline]
    pub fn pos(&self) -> u32 {
        self.node.pos
    }

    /// Get the end position
    #[inline]
    pub fn end(&self) -> u32 {
        self.node.end
    }

    /// Get the flags
    #[inline]
    pub fn flags(&self) -> u16 {
        self.node.flags
    }

    /// Check if this node has associated data
    #[inline]
    pub fn has_data(&self) -> bool {
        self.node.has_data()
    }

    /// Get extended node info (parent, id, modifier/transform flags)
    #[inline]
    pub fn extended(&self) -> Option<&'a ExtendedNodeInfo> {
        self.arena.get_extended(self.index)
    }

    /// Get parent node index
    #[inline]
    pub fn parent(&self) -> NodeIndex {
        self.extended().map_or(NodeIndex::NONE, |e| e.parent)
    }

    /// Get node id
    #[inline]
    pub fn id(&self) -> u32 {
        self.extended().map_or(0, |e| e.id)
    }

    /// Get a child node as a NodeView
    #[inline]
    pub fn child(&self, index: NodeIndex) -> Option<NodeView<'a>> {
        NodeView::new(self.arena, index)
    }

    // Typed data accessors - return Option<&T> based on node kind

    /// Get identifier data (for Identifier, PrivateIdentifier nodes)
    #[inline]
    pub fn as_identifier(&self) -> Option<&'a IdentifierData> {
        self.arena.get_identifier(self.node)
    }

    /// Get literal data (for StringLiteral, NumericLiteral, etc.)
    #[inline]
    pub fn as_literal(&self) -> Option<&'a LiteralData> {
        self.arena.get_literal(self.node)
    }

    /// Get binary expression data
    #[inline]
    pub fn as_binary_expr(&self) -> Option<&'a BinaryExprData> {
        self.arena.get_binary_expr(self.node)
    }

    /// Get call expression data
    #[inline]
    pub fn as_call_expr(&self) -> Option<&'a CallExprData> {
        self.arena.get_call_expr(self.node)
    }

    /// Get function data
    #[inline]
    pub fn as_function(&self) -> Option<&'a FunctionData> {
        self.arena.get_function(self.node)
    }

    /// Get class data
    #[inline]
    pub fn as_class(&self) -> Option<&'a ClassData> {
        self.arena.get_class(self.node)
    }

    /// Get block data
    #[inline]
    pub fn as_block(&self) -> Option<&'a BlockData> {
        self.arena.get_block(self.node)
    }

    /// Get source file data
    #[inline]
    pub fn as_source_file(&self) -> Option<&'a SourceFileData> {
        self.arena.get_source_file(self.node)
    }
}

// =============================================================================
// Node Kind Utilities
// =============================================================================

impl Node {
    /// Check if this is an identifier node
    #[inline]
    pub fn is_identifier(&self) -> bool {
        use tsz_scanner::SyntaxKind;
        self.kind == SyntaxKind::Identifier as u16
    }

    /// Check if this is a string literal
    #[inline]
    pub fn is_string_literal(&self) -> bool {
        use tsz_scanner::SyntaxKind;
        self.kind == SyntaxKind::StringLiteral as u16
    }

    /// Check if this is a numeric literal
    #[inline]
    pub fn is_numeric_literal(&self) -> bool {
        use tsz_scanner::SyntaxKind;
        self.kind == SyntaxKind::NumericLiteral as u16
    }

    /// Check if this is a function declaration
    #[inline]
    pub fn is_function_declaration(&self) -> bool {
        use super::syntax_kind_ext::FUNCTION_DECLARATION;
        self.kind == FUNCTION_DECLARATION
    }

    /// Check if this is a class declaration
    #[inline]
    pub fn is_class_declaration(&self) -> bool {
        use super::syntax_kind_ext::CLASS_DECLARATION;
        self.kind == CLASS_DECLARATION
    }

    /// Check if this is any kind of function-like node
    #[inline]
    pub fn is_function_like(&self) -> bool {
        use super::syntax_kind_ext::*;
        matches!(
            self.kind,
            FUNCTION_DECLARATION
                | FUNCTION_EXPRESSION
                | ARROW_FUNCTION
                | METHOD_DECLARATION
                | CONSTRUCTOR
                | GET_ACCESSOR
                | SET_ACCESSOR
        )
    }

    /// Check if this is a statement
    #[inline]
    pub fn is_statement(&self) -> bool {
        use super::syntax_kind_ext::*;
        (BLOCK..=DEBUGGER_STATEMENT).contains(&self.kind) || self.kind == VARIABLE_STATEMENT
    }

    /// Check if this is a declaration
    #[inline]
    pub fn is_declaration(&self) -> bool {
        use super::syntax_kind_ext::*;
        (VARIABLE_DECLARATION..=EXPORT_SPECIFIER).contains(&self.kind)
    }

    /// Check if this is a type node
    #[inline]
    pub fn is_type_node(&self) -> bool {
        use super::syntax_kind_ext::*;
        (TYPE_PREDICATE..=IMPORT_TYPE).contains(&self.kind)
    }
}

// =============================================================================
// Node Access Trait - Unified Interface for Arena Types
// =============================================================================

/// Common node information that both arena types can provide.
/// This struct contains the essential fields needed by most consumers.
#[derive(Clone, Debug)]
pub struct NodeInfo {
    pub kind: u16,
    pub flags: u32,
    pub modifier_flags: u32,
    pub pos: u32,
    pub end: u32,
    pub parent: NodeIndex,
    pub id: u32,
}

impl NodeInfo {
    /// Create from a Node and its extended info
    pub fn from_thin(node: &Node, ext: &ExtendedNodeInfo) -> NodeInfo {
        NodeInfo {
            kind: node.kind,
            flags: node.flags as u32,
            modifier_flags: ext.modifier_flags,
            pos: node.pos,
            end: node.end,
            parent: ext.parent,
            id: ext.id,
        }
    }
}

/// Trait for unified access to AST nodes across different arena implementations.
/// This allows consumers (binder, checker, emitter) to work with either
/// different arena implementations without code changes.
pub trait NodeAccess {
    /// Get basic node information by index
    fn node_info(&self, index: NodeIndex) -> Option<NodeInfo>;

    /// Get the syntax kind of a node
    fn kind(&self, index: NodeIndex) -> Option<u16>;

    /// Get the source position range
    fn pos_end(&self, index: NodeIndex) -> Option<(u32, u32)>;

    /// Check if a node exists
    fn exists(&self, index: NodeIndex) -> bool {
        !index.is_none() && self.kind(index).is_some()
    }

    /// Get identifier text (if this is an identifier node)
    fn get_identifier_text(&self, index: NodeIndex) -> Option<&str>;

    /// Get literal value text (if this is a literal node)
    fn get_literal_text(&self, index: NodeIndex) -> Option<&str>;

    /// Get children of a node (for traversal)
    fn get_children(&self, index: NodeIndex) -> Vec<NodeIndex>;
}

/// Implementation of NodeAccess for NodeArena
impl NodeAccess for NodeArena {
    fn node_info(&self, index: NodeIndex) -> Option<NodeInfo> {
        if index.is_none() {
            return None;
        }
        let node = self.nodes.get(index.0 as usize)?;
        let ext = self.extended_info.get(index.0 as usize)?;
        Some(NodeInfo::from_thin(node, ext))
    }

    fn kind(&self, index: NodeIndex) -> Option<u16> {
        if index.is_none() {
            return None;
        }
        self.nodes.get(index.0 as usize).map(|n| n.kind)
    }

    fn pos_end(&self, index: NodeIndex) -> Option<(u32, u32)> {
        if index.is_none() {
            return None;
        }
        self.nodes.get(index.0 as usize).map(|n| (n.pos, n.end))
    }

    fn get_identifier_text(&self, index: NodeIndex) -> Option<&str> {
        let node = self.get(index)?;
        let data = self.get_identifier(node)?;
        // Use atom for O(1) lookup if available, otherwise fall back to escaped_text
        Some(self.resolve_identifier_text(data))
    }

    fn get_literal_text(&self, index: NodeIndex) -> Option<&str> {
        let node = self.get(index)?;
        let data = self.get_literal(node)?;
        Some(&data.text)
    }

    fn get_children(&self, index: NodeIndex) -> Vec<NodeIndex> {
        if index.is_none() {
            return Vec::new();
        }

        let node = match self.nodes.get(index.0 as usize) {
            Some(n) => n,
            None => return Vec::new(),
        };

        // Helper to add optional NodeIndex (ignoring NONE)
        let add_opt = |children: &mut Vec<NodeIndex>, idx: NodeIndex| {
            if idx.is_some() {
                children.push(idx);
            }
        };

        // Helper to add NodeList (expanding to individual nodes)
        let add_list = |children: &mut Vec<NodeIndex>, list: &NodeList| {
            children.extend(list.nodes.iter().copied());
        };

        // Helper to add optional NodeList
        let add_opt_list = |children: &mut Vec<NodeIndex>, list: &Option<NodeList>| {
            if let Some(l) = list {
                children.extend(l.nodes.iter().copied());
            }
        };

        use super::syntax_kind_ext::*;

        let mut children = Vec::new();

        // Match on node kind and retrieve data from appropriate pool
        match node.kind {
            // Names
            QUALIFIED_NAME => {
                if let Some(data) = self.get_qualified_name(node) {
                    children.push(data.left);
                    children.push(data.right);
                }
            }
            COMPUTED_PROPERTY_NAME => {
                if let Some(data) = self.get_computed_property(node) {
                    children.push(data.expression);
                }
            }

            // Expressions
            BINARY_EXPRESSION => {
                if let Some(data) = self.get_binary_expr(node) {
                    children.push(data.left);
                    children.push(data.right);
                }
            }
            PREFIX_UNARY_EXPRESSION | POSTFIX_UNARY_EXPRESSION => {
                if let Some(data) = self.get_unary_expr(node) {
                    children.push(data.operand);
                }
            }
            CALL_EXPRESSION | NEW_EXPRESSION => {
                if let Some(data) = self.get_call_expr(node) {
                    children.push(data.expression);
                    add_opt_list(&mut children, &data.type_arguments);
                    add_opt_list(&mut children, &data.arguments);
                }
            }
            TAGGED_TEMPLATE_EXPRESSION => {
                if let Some(data) = self.get_tagged_template(node) {
                    children.push(data.tag);
                    add_opt_list(&mut children, &data.type_arguments);
                    children.push(data.template);
                }
            }
            TEMPLATE_EXPRESSION => {
                if let Some(data) = self.get_template_expr(node) {
                    children.push(data.head);
                    add_list(&mut children, &data.template_spans);
                }
            }
            TEMPLATE_SPAN => {
                if let Some(data) = self.get_template_span(node) {
                    children.push(data.expression);
                    children.push(data.literal);
                }
            }
            PROPERTY_ACCESS_EXPRESSION | ELEMENT_ACCESS_EXPRESSION => {
                if let Some(data) = self.get_access_expr(node) {
                    children.push(data.expression);
                    children.push(data.name_or_argument);
                }
            }
            CONDITIONAL_EXPRESSION => {
                if let Some(data) = self.get_conditional_expr(node) {
                    children.push(data.condition);
                    children.push(data.when_true);
                    children.push(data.when_false);
                }
            }
            ARROW_FUNCTION | FUNCTION_EXPRESSION => {
                if let Some(data) = self.get_function(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt_list(&mut children, &data.type_parameters);
                    add_list(&mut children, &data.parameters);
                    add_opt(&mut children, data.type_annotation);
                    children.push(data.body);
                }
            }
            ARRAY_LITERAL_EXPRESSION => {
                if let Some(data) = self.get_literal_expr(node) {
                    add_list(&mut children, &data.elements);
                }
            }
            OBJECT_LITERAL_EXPRESSION => {
                if let Some(data) = self.get_literal_expr(node) {
                    add_list(&mut children, &data.elements);
                }
            }
            PARENTHESIZED_EXPRESSION => {
                if let Some(data) = self.get_parenthesized(node) {
                    children.push(data.expression);
                }
            }
            YIELD_EXPRESSION => {
                if let Some(data) = self.get_unary_expr_ex(node) {
                    add_opt(&mut children, data.expression);
                }
            }
            AWAIT_EXPRESSION => {
                if let Some(data) = self.get_unary_expr_ex(node) {
                    children.push(data.expression);
                }
            }
            SPREAD_ELEMENT => {
                if let Some(data) = self.get_spread(node) {
                    children.push(data.expression);
                }
            }
            AS_EXPRESSION | SATISFIES_EXPRESSION => {
                if let Some(data) = self.get_type_assertion(node) {
                    children.push(data.expression);
                    children.push(data.type_node);
                }
            }
            TYPE_ASSERTION => {
                if let Some(data) = self.get_type_assertion(node) {
                    children.push(data.type_node);
                    children.push(data.expression);
                }
            }
            NON_NULL_EXPRESSION => {
                if let Some(data) = self.get_unary_expr_ex(node) {
                    children.push(data.expression);
                }
            }

            // Statements
            VARIABLE_STATEMENT => {
                if let Some(data) = self.get_variable(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    // Variable statements contain their declarations directly
                    add_list(&mut children, &data.declarations);
                }
            }
            VARIABLE_DECLARATION_LIST => {
                if let Some(data) = self.get_variable(node) {
                    add_list(&mut children, &data.declarations);
                }
            }
            VARIABLE_DECLARATION => {
                if let Some(data) = self.get_variable_declaration(node) {
                    children.push(data.name);
                    add_opt(&mut children, data.type_annotation);
                    add_opt(&mut children, data.initializer);
                }
            }
            EXPRESSION_STATEMENT => {
                if let Some(data) = self.get_expression_statement(node) {
                    children.push(data.expression);
                }
            }
            IF_STATEMENT => {
                if let Some(data) = self.get_if_statement(node) {
                    children.push(data.expression);
                    children.push(data.then_statement);
                    add_opt(&mut children, data.else_statement);
                }
            }
            WHILE_STATEMENT | DO_STATEMENT | FOR_STATEMENT => {
                if let Some(data) = self.get_loop(node) {
                    add_opt(&mut children, data.initializer);
                    add_opt(&mut children, data.condition);
                    add_opt(&mut children, data.incrementor);
                    children.push(data.statement);
                }
            }
            FOR_IN_STATEMENT | FOR_OF_STATEMENT => {
                if let Some(data) = self.get_for_in_of(node) {
                    children.push(data.initializer);
                    children.push(data.expression);
                    children.push(data.statement);
                }
            }
            SWITCH_STATEMENT => {
                if let Some(data) = self.get_switch(node) {
                    children.push(data.expression);
                    children.push(data.case_block);
                }
            }
            CASE_BLOCK => {
                if let Some(data) = self.get_block(node) {
                    add_list(&mut children, &data.statements);
                }
            }
            CASE_CLAUSE | DEFAULT_CLAUSE => {
                if let Some(data) = self.get_case_clause(node) {
                    add_opt(&mut children, data.expression);
                    add_list(&mut children, &data.statements);
                }
            }
            RETURN_STATEMENT => {
                if let Some(data) = self.get_return_statement(node) {
                    add_opt(&mut children, data.expression);
                }
            }
            THROW_STATEMENT => {
                if let Some(data) = self.get_return_statement(node) {
                    children.push(data.expression);
                }
            }
            TRY_STATEMENT => {
                if let Some(data) = self.get_try(node) {
                    children.push(data.try_block);
                    add_opt(&mut children, data.catch_clause);
                    add_opt(&mut children, data.finally_block);
                }
            }
            CATCH_CLAUSE => {
                if let Some(data) = self.get_catch_clause(node) {
                    add_opt(&mut children, data.variable_declaration);
                    children.push(data.block);
                }
            }
            LABELED_STATEMENT => {
                if let Some(data) = self.get_labeled_statement(node) {
                    children.push(data.label);
                    children.push(data.statement);
                }
            }
            BREAK_STATEMENT | CONTINUE_STATEMENT => {
                if let Some(data) = self.get_jump_data(node) {
                    add_opt(&mut children, data.label);
                }
            }
            WITH_STATEMENT => {
                if let Some(data) = self.get_with_statement(node) {
                    children.push(data.expression);
                    children.push(data.then_statement);
                }
            }
            BLOCK | CLASS_STATIC_BLOCK_DECLARATION => {
                if let Some(data) = self.get_block(node) {
                    add_list(&mut children, &data.statements);
                }
            }

            // Declarations
            FUNCTION_DECLARATION => {
                if let Some(data) = self.get_function(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    add_opt_list(&mut children, &data.type_parameters);
                    add_list(&mut children, &data.parameters);
                    add_opt(&mut children, data.type_annotation);
                    children.push(data.body);
                }
            }
            CLASS_DECLARATION | CLASS_EXPRESSION => {
                if let Some(data) = self.get_class(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    add_opt_list(&mut children, &data.type_parameters);
                    add_opt_list(&mut children, &data.heritage_clauses);
                    add_list(&mut children, &data.members);
                }
            }
            INTERFACE_DECLARATION => {
                if let Some(data) = self.get_interface(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    add_opt_list(&mut children, &data.type_parameters);
                    add_opt_list(&mut children, &data.heritage_clauses);
                    add_list(&mut children, &data.members);
                }
            }
            TYPE_ALIAS_DECLARATION => {
                if let Some(data) = self.get_type_alias(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    add_opt_list(&mut children, &data.type_parameters);
                    children.push(data.type_node);
                }
            }
            ENUM_DECLARATION => {
                if let Some(data) = self.get_enum(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    add_list(&mut children, &data.members);
                }
            }
            ENUM_MEMBER => {
                if let Some(data) = self.get_enum_member(node) {
                    add_opt(&mut children, data.name);
                    add_opt(&mut children, data.initializer);
                }
            }
            MODULE_DECLARATION => {
                if let Some(data) = self.get_module(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    add_opt(&mut children, data.body);
                }
            }
            MODULE_BLOCK => {
                if let Some(data) = self.get_module_block(node) {
                    add_opt_list(&mut children, &data.statements);
                }
            }

            // Import/Export
            IMPORT_DECLARATION | IMPORT_EQUALS_DECLARATION => {
                if let Some(data) = self.get_import_decl(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.import_clause);
                    children.push(data.module_specifier);
                    add_opt(&mut children, data.attributes);
                }
            }
            IMPORT_CLAUSE => {
                if let Some(data) = self.get_import_clause(node) {
                    add_opt(&mut children, data.name);
                    add_opt(&mut children, data.named_bindings);
                }
            }
            NAMESPACE_IMPORT | NAMESPACE_EXPORT => {
                if let Some(data) = self.get_named_imports(node) {
                    children.push(data.name);
                }
            }
            NAMED_IMPORTS | NAMED_EXPORTS => {
                if let Some(data) = self.get_named_imports(node) {
                    add_list(&mut children, &data.elements);
                }
            }
            IMPORT_SPECIFIER | EXPORT_SPECIFIER => {
                if let Some(data) = self.get_specifier(node) {
                    add_opt(&mut children, data.property_name);
                    children.push(data.name);
                }
            }
            EXPORT_DECLARATION => {
                if let Some(data) = self.get_export_decl(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.export_clause);
                    add_opt(&mut children, data.module_specifier);
                    add_opt(&mut children, data.attributes);
                }
            }
            EXPORT_ASSIGNMENT => {
                if let Some(data) = self.get_export_assignment(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    children.push(data.expression);
                }
            }

            // Type nodes
            TYPE_REFERENCE => {
                if let Some(data) = self.get_type_ref(node) {
                    children.push(data.type_name);
                    add_opt_list(&mut children, &data.type_arguments);
                }
            }
            FUNCTION_TYPE | CONSTRUCTOR_TYPE => {
                if let Some(data) = self.get_function_type(node) {
                    add_opt_list(&mut children, &data.type_parameters);
                    add_list(&mut children, &data.parameters);
                    children.push(data.type_annotation);
                }
            }
            TYPE_QUERY => {
                if let Some(data) = self.get_type_query(node) {
                    children.push(data.expr_name);
                    add_opt_list(&mut children, &data.type_arguments);
                }
            }
            TYPE_LITERAL => {
                if let Some(data) = self.get_type_literal(node) {
                    add_list(&mut children, &data.members);
                }
            }
            ARRAY_TYPE => {
                if let Some(data) = self.get_array_type(node) {
                    children.push(data.element_type);
                }
            }
            TUPLE_TYPE => {
                if let Some(data) = self.get_tuple_type(node) {
                    add_list(&mut children, &data.elements);
                }
            }
            OPTIONAL_TYPE | REST_TYPE | PARENTHESIZED_TYPE => {
                if let Some(data) = self.get_wrapped_type(node) {
                    children.push(data.type_node);
                }
            }
            UNION_TYPE | INTERSECTION_TYPE => {
                if let Some(data) = self.get_composite_type(node) {
                    add_list(&mut children, &data.types);
                }
            }
            CONDITIONAL_TYPE => {
                if let Some(data) = self.get_conditional_type(node) {
                    children.push(data.check_type);
                    children.push(data.extends_type);
                    children.push(data.true_type);
                    children.push(data.false_type);
                }
            }
            INFER_TYPE => {
                if let Some(data) = self.get_infer_type(node) {
                    children.push(data.type_parameter);
                }
            }
            TYPE_OPERATOR => {
                if let Some(data) = self.get_type_operator(node) {
                    children.push(data.type_node);
                }
            }
            INDEXED_ACCESS_TYPE => {
                if let Some(data) = self.get_indexed_access_type(node) {
                    children.push(data.object_type);
                    children.push(data.index_type);
                }
            }
            MAPPED_TYPE => {
                if let Some(data) = self.get_mapped_type(node) {
                    add_opt(&mut children, data.type_parameter);
                    add_opt(&mut children, data.name_type);
                    add_opt(&mut children, data.type_node);
                    add_opt_list(&mut children, &data.members);
                }
            }
            LITERAL_TYPE => {
                if let Some(data) = self.get_literal_type(node) {
                    add_opt(&mut children, data.literal);
                }
            }
            TEMPLATE_LITERAL_TYPE => {
                if let Some(data) = self.get_template_literal_type(node) {
                    children.push(data.head);
                    add_list(&mut children, &data.template_spans);
                }
            }
            NAMED_TUPLE_MEMBER => {
                if let Some(data) = self.get_named_tuple_member(node) {
                    children.push(data.name);
                    children.push(data.type_node);
                }
            }
            TYPE_PREDICATE => {
                if let Some(data) = self.get_type_predicate(node) {
                    children.push(data.parameter_name);
                    add_opt(&mut children, data.type_node);
                }
            }

            // Class members
            PROPERTY_DECLARATION => {
                if let Some(data) = self.get_property_decl(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    add_opt(&mut children, data.type_annotation);
                    add_opt(&mut children, data.initializer);
                }
            }
            METHOD_DECLARATION => {
                if let Some(data) = self.get_method_decl(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    add_opt_list(&mut children, &data.type_parameters);
                    add_list(&mut children, &data.parameters);
                    add_opt(&mut children, data.type_annotation);
                    children.push(data.body);
                }
            }
            CONSTRUCTOR => {
                if let Some(data) = self.get_constructor(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt_list(&mut children, &data.type_parameters);
                    add_list(&mut children, &data.parameters);
                    children.push(data.body);
                }
            }
            GET_ACCESSOR | SET_ACCESSOR => {
                if let Some(data) = self.get_accessor(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    add_opt_list(&mut children, &data.type_parameters);
                    add_list(&mut children, &data.parameters);
                    add_opt(&mut children, data.type_annotation);
                    children.push(data.body);
                }
            }
            PARAMETER => {
                if let Some(data) = self.get_parameter(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    add_opt(&mut children, data.type_annotation);
                    add_opt(&mut children, data.initializer);
                }
            }
            TYPE_PARAMETER => {
                if let Some(data) = self.get_type_parameter(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    children.push(data.name);
                    add_opt(&mut children, data.constraint);
                    add_opt(&mut children, data.default);
                }
            }
            DECORATOR => {
                if let Some(data) = self.get_decorator(node) {
                    children.push(data.expression);
                }
            }
            HERITAGE_CLAUSE => {
                if let Some(data) = self.get_heritage_clause(node) {
                    add_list(&mut children, &data.types);
                }
            }
            EXPRESSION_WITH_TYPE_ARGUMENTS => {
                if let Some(data) = self.get_expr_type_args(node) {
                    children.push(data.expression);
                    add_opt_list(&mut children, &data.type_arguments);
                }
            }

            // Binding patterns
            OBJECT_BINDING_PATTERN | ARRAY_BINDING_PATTERN => {
                if let Some(data) = self.get_binding_pattern(node) {
                    add_list(&mut children, &data.elements);
                }
            }
            BINDING_ELEMENT => {
                if let Some(data) = self.get_binding_element(node) {
                    add_opt(&mut children, data.property_name);
                    children.push(data.name);
                    add_opt(&mut children, data.initializer);
                }
            }

            // Object literal members
            PROPERTY_ASSIGNMENT => {
                if let Some(data) = self.get_property_assignment(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    children.push(data.initializer);
                }
            }
            SHORTHAND_PROPERTY_ASSIGNMENT => {
                if let Some(data) = self.get_shorthand_property(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    children.push(data.name);
                    add_opt(&mut children, data.object_assignment_initializer);
                }
            }
            SPREAD_ASSIGNMENT => {
                if let Some(data) = self.get_spread(node) {
                    children.push(data.expression);
                }
            }

            // JSX nodes
            JSX_ELEMENT => {
                if let Some(data) = self.get_jsx_element(node) {
                    children.push(data.opening_element);
                    add_list(&mut children, &data.children);
                    add_opt(&mut children, data.closing_element);
                }
            }
            JSX_SELF_CLOSING_ELEMENT | JSX_OPENING_ELEMENT => {
                if let Some(data) = self.get_jsx_opening(node) {
                    children.push(data.tag_name);
                    add_opt_list(&mut children, &data.type_arguments);
                    add_opt(&mut children, data.attributes);
                }
            }
            JSX_CLOSING_ELEMENT => {
                if let Some(data) = self.get_jsx_closing(node) {
                    children.push(data.tag_name);
                }
            }
            JSX_FRAGMENT => {
                if let Some(data) = self.get_jsx_fragment(node) {
                    children.push(data.opening_fragment);
                    add_list(&mut children, &data.children);
                    children.push(data.closing_fragment);
                }
            }
            JSX_OPENING_FRAGMENT | JSX_CLOSING_FRAGMENT => {
                // No children
            }
            JSX_ATTRIBUTES => {
                if let Some(data) = self.get_jsx_attributes(node) {
                    add_list(&mut children, &data.properties);
                }
            }
            JSX_ATTRIBUTE => {
                if let Some(data) = self.get_jsx_attribute(node) {
                    children.push(data.name);
                    add_opt(&mut children, data.initializer);
                }
            }
            JSX_SPREAD_ATTRIBUTE => {
                if let Some(data) = self.get_jsx_spread_attribute(node) {
                    children.push(data.expression);
                }
            }
            JSX_EXPRESSION => {
                if let Some(data) = self.get_jsx_expression(node) {
                    add_opt(&mut children, data.expression);
                }
            }
            JSX_NAMESPACED_NAME => {
                if let Some(data) = self.get_jsx_namespaced_name(node) {
                    children.push(data.namespace);
                    children.push(data.name);
                }
            }

            // Signatures
            CALL_SIGNATURE | CONSTRUCT_SIGNATURE => {
                if let Some(data) = self.get_signature(node) {
                    add_opt_list(&mut children, &data.type_parameters);
                    add_opt_list(&mut children, &data.parameters);
                    add_opt(&mut children, data.type_annotation);
                }
            }
            INDEX_SIGNATURE => {
                if let Some(data) = self.get_index_signature(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_list(&mut children, &data.parameters);
                    add_opt(&mut children, data.type_annotation);
                }
            }
            PROPERTY_SIGNATURE => {
                if let Some(data) = self.get_signature(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    add_opt(&mut children, data.type_annotation);
                    // Note: SignatureData doesn't have initializer, property signatures in thin nodes
                    // use the same structure as method signatures
                }
            }
            METHOD_SIGNATURE => {
                if let Some(data) = self.get_signature(node) {
                    add_opt_list(&mut children, &data.modifiers);
                    add_opt(&mut children, data.name);
                    add_opt_list(&mut children, &data.type_parameters);
                    add_opt_list(&mut children, &data.parameters);
                    add_opt(&mut children, data.type_annotation);
                }
            }

            // Source file
            SOURCE_FILE => {
                if let Some(data) = self.get_source_file(node) {
                    add_list(&mut children, &data.statements);
                    children.push(data.end_of_file_token);
                }
            }

            // Nodes with no children (tokens, identifiers, literals)
            _ => {
                // Tokens, identifiers, literals, etc. have no children
            }
        }

        children
    }
}
