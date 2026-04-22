//! `NodeArena` typed data accessors and semantic utility methods.
//!
//! This module contains the core `get_*` accessor methods for retrieving typed
//! node data from the arena's side pools, plus semantic utility methods like
//! `skip_parenthesized`, `is_namespace_instantiated`, and `is_in_ambient_context`.
//!
//! `NodeView`, `NodeInfo`, and `NodeAccess` are in `node_view.rs`.
use super::base::NodeIndex;
use super::node::{
    AccessExprData, AccessorData, ArrayTypeData, BinaryExprData, BindingElementData,
    BindingPatternData, BlockData, CallExprData, CaseClauseData, CatchClauseData, ClassData,
    CompositeTypeData, ComputedPropertyData, ConditionalExprData, ConditionalTypeData,
    ConstructorData, DecoratorData, EnumData, EnumMemberData, ExportAssignmentData, ExportDeclData,
    ExprStatementData, ExprWithTypeArgsData, ExtendedNodeInfo, ForInOfData, FunctionData,
    FunctionTypeData, HeritageData, IdentifierData, IfStatementData, ImportAttributeData,
    ImportAttributesData, ImportClauseData, ImportDeclData, IndexSignatureData,
    IndexedAccessTypeData, InferTypeData, InterfaceData, JsxAttributeData, JsxAttributesData,
    JsxClosingData, JsxElementData, JsxExpressionData, JsxFragmentData, JsxNamespacedNameData,
    JsxOpeningData, JsxSpreadAttributeData, JsxTextData, JumpData, LabeledData, LiteralData,
    LiteralExprData, LiteralTypeData, LoopData, MappedTypeData, MethodDeclData, ModuleBlockData,
    ModuleData, NamedImportsData, NamedTupleMemberData, Node, NodeArena, ParameterData,
    ParenthesizedData, PropertyAssignmentData, PropertyDeclData, QualifiedNameData, ReturnData,
    ShorthandPropertyData, SignatureData, SourceFileData, SpecifierData, SpreadData, SwitchData,
    TaggedTemplateData, TemplateExprData, TemplateLiteralTypeData, TemplateSpanData, TryData,
    TupleTypeData, TypeAliasData, TypeAssertionData, TypeLiteralData, TypeOperatorData,
    TypeParameterData, TypePredicateData, TypeQueryData, TypeRefData, UnaryExprData,
    UnaryExprDataEx, VariableData, VariableDeclarationData, WrappedTypeData,
};
use super::syntax_kind_ext::{
    ARRAY_BINDING_PATTERN, ARROW_FUNCTION, AS_EXPRESSION, BINARY_EXPRESSION, BLOCK,
    CLASS_DECLARATION, CLASS_EXPRESSION, CONSTRUCTOR, DEBUGGER_STATEMENT, ENUM_DECLARATION,
    EXPORT_ASSIGNMENT, EXPORT_DECLARATION, EXPORT_SPECIFIER, FUNCTION_DECLARATION,
    FUNCTION_EXPRESSION, GET_ACCESSOR, IMPORT_DECLARATION, IMPORT_EQUALS_DECLARATION, IMPORT_TYPE,
    INDEX_SIGNATURE, INTERFACE_DECLARATION, METHOD_DECLARATION, METHOD_SIGNATURE, MODULE_BLOCK,
    MODULE_DECLARATION, NAMED_EXPORTS, NAMESPACE_EXPORT_DECLARATION, NON_NULL_EXPRESSION,
    OBJECT_BINDING_PATTERN, PARAMETER, PARENTHESIZED_EXPRESSION, PROPERTY_DECLARATION,
    PROPERTY_SIGNATURE, SATISFIES_EXPRESSION, SET_ACCESSOR, TYPE_ALIAS_DECLARATION, TYPE_ASSERTION,
    TYPE_PREDICATE, VARIABLE_DECLARATION, VARIABLE_DECLARATION_LIST, VARIABLE_STATEMENT,
};

impl NodeArena {
    /// Get a thin node by index
    #[inline]
    #[must_use]
    pub fn get(&self, index: NodeIndex) -> Option<&Node> {
        if index.is_none() {
            None
        } else {
            self.nodes.get(index.0 as usize)
        }
    }

    /// Get a mutable thin node by index
    #[inline]
    #[must_use]
    pub fn get_mut(&mut self, index: NodeIndex) -> Option<&mut Node> {
        if index.is_none() {
            None
        } else {
            self.nodes.get_mut(index.0 as usize)
        }
    }

    /// Get the source start position of a node by index. Returns `None` if
    /// the index is `NodeIndex::NONE` or out of bounds. Inherent helper for
    /// the common `arena.get(idx).map(|n| n.pos)` pattern.
    #[inline]
    #[must_use]
    pub fn pos_at(&self, index: NodeIndex) -> Option<u32> {
        self.get(index).map(|n| n.pos)
    }

    /// Get the source end position of a node by index. Returns `None` if
    /// the index is `NodeIndex::NONE` or out of bounds. Inherent helper for
    /// the common `arena.get(idx).map(|n| n.end)` pattern.
    #[inline]
    #[must_use]
    pub fn end_at(&self, index: NodeIndex) -> Option<u32> {
        self.get(index).map(|n| n.end)
    }

    /// Get the `(pos, end)` source range of a node by index. Returns `None`
    /// if the index is `NodeIndex::NONE` or out of bounds. Inherent helper
    /// for the common `arena.get(idx).map(|n| (n.pos, n.end))` pattern used
    /// by emitter source-range plumbing and diagnostics.
    #[inline]
    #[must_use]
    pub fn pos_end_at(&self, index: NodeIndex) -> Option<(u32, u32)> {
        self.get(index).map(|n| (n.pos, n.end))
    }

    /// Get extended info for a node
    #[inline]
    #[must_use]
    pub fn get_extended(&self, index: NodeIndex) -> Option<&ExtendedNodeInfo> {
        if index.is_none() {
            None
        } else {
            self.extended_info.get(index.0 as usize)
        }
    }

    /// Get mutable extended info for a node
    #[inline]
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn get_call_expr(&self, node: &Node) -> Option<&CallExprData> {
        use super::syntax_kind_ext::{CALL_EXPRESSION, NEW_EXPRESSION};
        if node.has_data() && (node.kind == CALL_EXPRESSION || node.kind == NEW_EXPRESSION) {
            self.call_exprs.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Check if a function-like node is immediately invoked (IIFE pattern).
    ///
    /// Detects patterns like `(function() {})()`, `(() => expr)()`,
    /// `((fn))()` (arbitrary paren nesting), and `new (function() {})()`.
    #[must_use]
    pub fn is_immediately_invoked(&self, func_idx: NodeIndex) -> bool {
        use super::syntax_kind_ext::{CALL_EXPRESSION, NEW_EXPRESSION, PARENTHESIZED_EXPRESSION};

        let mut current = func_idx;
        // Guard against pathological nesting depth
        for _ in 0..100 {
            let Some(ext) = self.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.get(ext.parent) else {
                return false;
            };
            if parent_node.kind == PARENTHESIZED_EXPRESSION {
                current = ext.parent;
                continue;
            }
            if (parent_node.kind == CALL_EXPRESSION || parent_node.kind == NEW_EXPRESSION)
                && let Some(call) = self.get_call_expr(parent_node)
                && call.expression == current
            {
                return true;
            }
            return false;
        }
        false
    }

    /// Skip through parenthesized expressions to the underlying expression.
    ///
    /// Unwraps any number of `(expr)` wrappers.
    /// Uses a bounded loop (max 100 iterations) to guard against pathological input.
    #[must_use]
    pub fn skip_parenthesized(&self, mut idx: NodeIndex) -> NodeIndex {
        for _ in 0..100 {
            let Some(node) = self.get(idx) else {
                return idx;
            };
            if node.kind == PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.get_parenthesized(node)
            {
                idx = paren.expression;
                continue;
            }
            return idx;
        }
        idx
    }

    /// Skip through parenthesized, non-null assertion, and comma-expression wrappers.
    ///
    /// Unwraps `(expr)`, `expr!`, and comma expressions (`(a, b)`).
    /// Uses a bounded loop (max 100 iterations) to guard against pathological input.
    #[must_use]
    pub fn skip_parenthesized_and_assertions_and_comma(&self, mut idx: NodeIndex) -> NodeIndex {
        for _ in 0..100 {
            let Some(node) = self.get(idx) else {
                return idx;
            };
            if node.kind == PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.get_parenthesized(node)
            {
                idx = paren.expression;
                continue;
            }
            if node.kind == NON_NULL_EXPRESSION
                && let Some(unary) = self.get_unary_expr_ex(node)
            {
                idx = unary.expression;
                continue;
            }
            if node.kind == BINARY_EXPRESSION
                && let Some(binary) = self.get_binary_expr(node)
                && binary.operator_token == tsz_scanner::SyntaxKind::CommaToken as u16
            {
                idx = binary.right;
                continue;
            }

            return idx;
        }
        idx
    }

    /// Skip through parenthesized, non-null assertion, and type assertion expressions.
    ///
    /// Unwraps `(expr)`, `expr!`, `expr as T`, `<T>expr`, and `expr satisfies T` wrappers.
    /// Uses a bounded loop (max 100 iterations) to guard against pathological input.
    #[must_use]
    pub fn skip_parenthesized_and_assertions(&self, mut idx: NodeIndex) -> NodeIndex {
        for _ in 0..100 {
            let Some(node) = self.get(idx) else {
                return idx;
            };
            if node.kind == PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.get_parenthesized(node)
            {
                idx = paren.expression;
                continue;
            }
            if node.kind == NON_NULL_EXPRESSION
                && let Some(unary) = self.get_unary_expr_ex(node)
            {
                idx = unary.expression;
                continue;
            }
            if (node.kind == TYPE_ASSERTION
                || node.kind == AS_EXPRESSION
                || node.kind == SATISFIES_EXPRESSION)
                && let Some(assertion) = self.get_type_assertion(node)
            {
                idx = assertion.expression;
                continue;
            }
            return idx;
        }
        idx
    }

    /// Check whether a namespace/module declaration is instantiated (has runtime value declarations).
    ///
    /// Returns `true` if the namespace contains value declarations (variables, functions,
    /// classes, enums, expression statements, export assignments), or is a
    /// `NAMESPACE_EXPORT_DECLARATION` (`export as namespace X`), which always produces a
    /// runtime global.
    ///
    /// Recursively walks dotted namespaces (`namespace Foo.Bar`) and `EXPORT_DECLARATION`
    /// wrappers to find the innermost `MODULE_BLOCK`, then checks each statement.
    #[must_use]
    pub fn is_namespace_instantiated(&self, namespace_idx: NodeIndex) -> bool {
        let Some(node) = self.get(namespace_idx) else {
            return false;
        };

        // `export as namespace X` always creates a global runtime value.
        if node.kind == NAMESPACE_EXPORT_DECLARATION {
            return true;
        }

        if node.kind != MODULE_DECLARATION {
            return false;
        }
        let Some(module_decl) = self.get_module(node) else {
            return false;
        };
        self.module_body_has_runtime_members(module_decl.body)
    }

    /// Check whether a module body contains runtime value declarations.
    ///
    /// Helper for [`is_namespace_instantiated`]. Handles dotted namespaces
    /// (body is another `MODULE_DECLARATION`) and `MODULE_BLOCK` bodies.
    fn module_body_has_runtime_members(&self, body_idx: NodeIndex) -> bool {
        if body_idx.is_none() {
            return false;
        }
        let Some(body_node) = self.get(body_idx) else {
            return false;
        };

        // Dotted namespace: `namespace Foo.Bar { ... }` — recurse into inner module
        if body_node.kind == MODULE_DECLARATION {
            return self.is_namespace_instantiated(body_idx);
        }

        if body_node.kind != MODULE_BLOCK {
            return false;
        }

        let Some(module_block) = self.get_module_block(body_node) else {
            return false;
        };
        let Some(statements) = &module_block.statements else {
            return false;
        };

        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.get(stmt_idx) else {
                continue;
            };
            if self.is_runtime_module_statement(stmt_node, stmt_idx) {
                return true;
            }
        }

        false
    }

    /// Check if a statement inside a module block is a runtime value declaration.
    ///
    /// Uses tsc's inverse logic: a module is uninstantiated if it contains ONLY
    /// type-level declarations (interfaces, type aliases, non-exported imports).
    /// Any other statement (try, if, for, expression, variable, etc.) makes the
    /// module instantiated.
    fn is_runtime_module_statement(&self, node: &Node, node_idx: NodeIndex) -> bool {
        match node.kind {
            // Type-only declarations — never instantiate a module
            k if k == INTERFACE_DECLARATION || k == TYPE_ALIAS_DECLARATION => false,

            // Import declarations — non-instantiated (they don't produce runtime code
            // in the namespace itself, even if exported)
            k if k == IMPORT_DECLARATION || k == IMPORT_EQUALS_DECLARATION => false,

            // Export declarations — check what's being exported
            k if k == EXPORT_DECLARATION => {
                if let Some(export_decl) = self.get_export_decl(node)
                    && let Some(clause) = self.get(export_decl.export_clause)
                {
                    match clause.kind {
                        k if k == VARIABLE_STATEMENT
                            || k == FUNCTION_DECLARATION
                            || k == CLASS_DECLARATION
                            || k == ENUM_DECLARATION =>
                        {
                            true
                        }
                        k if k == MODULE_DECLARATION => {
                            self.is_namespace_instantiated(export_decl.export_clause)
                        }
                        // Named exports (`export { name }`) make a namespace instantiated.
                        // tsc resolves each specifier to check if it has a value meaning,
                        // but at the parser level we conservatively treat all named exports
                        // as potentially instantiating (matches tsc's practical behavior
                        // for import-alias re-export patterns).
                        k if k == NAMED_EXPORTS => true,
                        _ => false,
                    }
                } else {
                    false
                }
            }

            // Nested namespace — recurse
            k if k == MODULE_DECLARATION => self.is_namespace_instantiated(node_idx),

            // Everything else (variables, functions, classes, enums, try/catch, if,
            // for, while, switch, expression statements, etc.) is runtime code
            _ => true,
        }
    }

    /// Get the modifier list for a declaration node, if it has one.
    ///
    /// Returns `Some(&NodeList)` for any declaration kind that carries modifiers
    /// (function, class, variable statement, enum, interface, type alias, module,
    /// method, property, constructor, accessor, parameter, import, export, etc.).
    /// Returns `None` for non-declaration nodes or nodes without modifier data.
    #[must_use]
    pub fn get_declaration_modifiers(&self, node: &Node) -> Option<&super::base::NodeList> {
        match node.kind {
            k if k == FUNCTION_DECLARATION || k == FUNCTION_EXPRESSION || k == ARROW_FUNCTION => {
                self.get_function(node).and_then(|d| d.modifiers.as_ref())
            }
            k if k == CLASS_DECLARATION || k == CLASS_EXPRESSION => {
                self.get_class(node).and_then(|d| d.modifiers.as_ref())
            }
            VARIABLE_STATEMENT => self.get_variable(node).and_then(|d| d.modifiers.as_ref()),
            ENUM_DECLARATION => self.get_enum(node).and_then(|d| d.modifiers.as_ref()),
            INTERFACE_DECLARATION => self.get_interface(node).and_then(|d| d.modifiers.as_ref()),
            TYPE_ALIAS_DECLARATION => self.get_type_alias(node).and_then(|d| d.modifiers.as_ref()),
            MODULE_DECLARATION => self.get_module(node).and_then(|d| d.modifiers.as_ref()),
            IMPORT_DECLARATION => self
                .get_import_decl(node)
                .and_then(|d| d.modifiers.as_ref()),
            EXPORT_DECLARATION => self
                .get_export_decl(node)
                .and_then(|d| d.modifiers.as_ref()),
            EXPORT_ASSIGNMENT => self
                .get_export_assignment(node)
                .and_then(|d| d.modifiers.as_ref()),
            k if k == METHOD_DECLARATION || k == METHOD_SIGNATURE => self
                .get_method_decl(node)
                .and_then(|d| d.modifiers.as_ref()),
            k if k == PROPERTY_DECLARATION || k == PROPERTY_SIGNATURE => self
                .get_property_decl(node)
                .and_then(|d| d.modifiers.as_ref()),
            CONSTRUCTOR => self
                .get_constructor(node)
                .and_then(|d| d.modifiers.as_ref()),
            k if k == GET_ACCESSOR || k == SET_ACCESSOR => {
                self.get_accessor(node).and_then(|d| d.modifiers.as_ref())
            }
            PARAMETER => self.get_parameter(node).and_then(|d| d.modifiers.as_ref()),
            INDEX_SIGNATURE => self
                .get_index_signature(node)
                .and_then(|d| d.modifiers.as_ref()),
            _ => None,
        }
    }

    /// Check whether a node is in an ambient context.
    ///
    /// A node is in an ambient context if it or any ancestor:
    /// - Has the `AMBIENT` node flag (set by parser for `.d.ts` files),
    /// - Has a `declare` keyword modifier, or
    /// - Is an interface or type alias declaration (implicitly ambient).
    ///
    /// This does **not** check the file extension (`.d.ts`); callers that need
    /// that check should do it separately since it requires filename context
    /// that `NodeArena` doesn't have.
    #[must_use]
    pub fn is_in_ambient_context(&self, idx: NodeIndex) -> bool {
        use super::flags::node_flags;

        let mut current = idx;
        for _ in 0..100 {
            let Some(node) = self.get(current) else {
                return false;
            };

            // Check the AMBIENT node flag (set by parser/binder)
            if (node.flags as u32) & node_flags::AMBIENT != 0 {
                return true;
            }

            // Interfaces and type aliases are implicitly ambient
            if node.kind == INTERFACE_DECLARATION || node.kind == TYPE_ALIAS_DECLARATION {
                return true;
            }

            // Check for `declare` keyword modifier on this node
            if let Some(mods) = self.get_declaration_modifiers(node) {
                for &mod_idx in &mods.nodes {
                    if let Some(mod_node) = self.get(mod_idx)
                        && mod_node.kind == tsz_scanner::SyntaxKind::DeclareKeyword as u16
                    {
                        return true;
                    }
                }
            }

            // Walk to parent
            if let Some(ext) = self.get_extended(current) {
                if ext.parent.is_none() {
                    return false;
                }
                current = ext.parent;
            } else {
                return false;
            }
        }
        false
    }

    /// Returns the combined `node_flags` for a `VARIABLE_DECLARATION` node,
    /// merging the node's own flags with its parent `VARIABLE_DECLARATION_LIST`
    /// flags. This is needed because the parser may place `LET`/`CONST`/`USING`
    /// flags on either the declaration or the list.
    ///
    /// Returns `0` if the node doesn't exist.
    #[must_use]
    pub fn get_variable_declaration_flags(&self, node_idx: NodeIndex) -> u32 {
        let Some(node) = self.get(node_idx) else {
            return 0;
        };
        let mut flags = node.flags as u32;
        use super::flags::node_flags;
        if (flags & (node_flags::LET | node_flags::CONST | node_flags::USING)) == 0
            && let Some(ext) = self.get_extended(node_idx)
            && let Some(parent) = self.get(ext.parent)
            && parent.kind == VARIABLE_DECLARATION_LIST
        {
            flags |= parent.flags as u32;
        }
        flags
    }

    /// Returns `true` if a `VARIABLE_DECLARATION` node is declared with `const`.
    ///
    /// Handles the fact that the `CONST` flag may live on the node itself or
    /// on its parent `VARIABLE_DECLARATION_LIST`.
    #[must_use]
    pub fn is_const_variable_declaration(&self, node_idx: NodeIndex) -> bool {
        use super::flags::node_flags;
        (self.get_variable_declaration_flags(node_idx) & node_flags::CONST) != 0
    }

    /// Get access expression data (property access or element access).
    /// Returns None if node is not an access expression or has no data.
    #[inline]
    #[must_use]
    pub fn get_access_expr(&self, node: &Node) -> Option<&AccessExprData> {
        use super::syntax_kind_ext::{
            ELEMENT_ACCESS_EXPRESSION, META_PROPERTY, PROPERTY_ACCESS_EXPRESSION,
        };
        if node.has_data()
            && (node.kind == PROPERTY_ACCESS_EXPRESSION
                || node.kind == ELEMENT_ACCESS_EXPRESSION
                || node.kind == META_PROPERTY)
        {
            self.access_exprs.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get conditional expression data (ternary: a ? b : c).
    /// Returns None if node is not a conditional expression or has no data.
    #[inline]
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn get_function(&self, node: &Node) -> Option<&FunctionData> {
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn get_source_file(&self, node: &Node) -> Option<&SourceFileData> {
        use super::syntax_kind_ext::SOURCE_FILE;
        if node.has_data() && node.kind == SOURCE_FILE {
            self.source_files.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get variable data (`VariableStatement` or `VariableDeclarationList`).
    #[inline]
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn get_with_statement(&self, node: &Node) -> Option<&IfStatementData> {
        use super::syntax_kind_ext::WITH_STATEMENT;
        if node.has_data() && node.kind == WITH_STATEMENT {
            self.if_statements.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get import declaration data (handles both `IMPORT_DECLARATION` and `IMPORT_EQUALS_DECLARATION`).
    #[inline]
    #[must_use]
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
    #[must_use]
    pub fn get_import_clause(&self, node: &Node) -> Option<&ImportClauseData> {
        use super::syntax_kind_ext::IMPORT_CLAUSE;
        if node.has_data() && node.kind == IMPORT_CLAUSE {
            self.import_clauses.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get named imports/exports data.
    /// Works for `NAMED_IMPORTS`, `NAMESPACE_IMPORT`, and `NAMED_EXPORTS` (they share the same data structure).
    #[inline]
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn get_export_decl(&self, node: &Node) -> Option<&ExportDeclData> {
        use super::syntax_kind_ext::{EXPORT_DECLARATION, NAMESPACE_EXPORT_DECLARATION};
        if node.has_data()
            && (node.kind == EXPORT_DECLARATION || node.kind == NAMESPACE_EXPORT_DECLARATION)
        {
            self.export_decls.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get export assignment data (export = expr).
    #[inline]
    #[must_use]
    pub fn get_export_assignment(&self, node: &Node) -> Option<&ExportAssignmentData> {
        use super::syntax_kind_ext::EXPORT_ASSIGNMENT;
        if node.has_data() && node.kind == EXPORT_ASSIGNMENT {
            self.export_assignments.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get import attributes data (`with { ... }` or `assert { ... }`).
    #[inline]
    #[must_use]
    pub fn get_import_attributes_data(&self, node: &Node) -> Option<&ImportAttributesData> {
        use super::syntax_kind_ext::IMPORT_ATTRIBUTES;
        if node.has_data() && node.kind == IMPORT_ATTRIBUTES {
            self.import_attributes.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get single import attribute data (name: value pair).
    #[inline]
    #[must_use]
    pub fn get_import_attribute_data(&self, node: &Node) -> Option<&ImportAttributeData> {
        use super::syntax_kind_ext::IMPORT_ATTRIBUTE;
        if node.has_data() && node.kind == IMPORT_ATTRIBUTE {
            self.import_attribute.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get parameter data.
    #[inline]
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn get_template_expr(&self, node: &Node) -> Option<&TemplateExprData> {
        use super::syntax_kind_ext::TEMPLATE_EXPRESSION;
        if node.has_data() && node.kind == TEMPLATE_EXPRESSION {
            self.template_exprs.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get template span data. Accepts both `TEMPLATE_SPAN` (expression-level)
    /// and `TEMPLATE_LITERAL_TYPE_SPAN` (type-level) since both store data in
    /// the same `template_spans` array.
    #[inline]
    #[must_use]
    pub fn get_template_span(&self, node: &Node) -> Option<&TemplateSpanData> {
        use super::syntax_kind_ext::{TEMPLATE_LITERAL_TYPE_SPAN, TEMPLATE_SPAN};
        if node.has_data()
            && (node.kind == TEMPLATE_SPAN || node.kind == TEMPLATE_LITERAL_TYPE_SPAN)
        {
            self.template_spans.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get tagged template expression data.
    #[inline]
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn get_shorthand_property(&self, node: &Node) -> Option<&ShorthandPropertyData> {
        use super::syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT;
        if node.has_data() && node.kind == SHORTHAND_PROPERTY_ASSIGNMENT {
            self.shorthand_properties.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get binding pattern data (`ObjectBindingPattern` or `ArrayBindingPattern`).
    #[inline]
    #[must_use]
    pub fn get_binding_pattern(&self, node: &Node) -> Option<&BindingPatternData> {
        if node.has_data() && node.is_binding_pattern() {
            self.binding_patterns.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get binding element data.
    #[inline]
    #[must_use]
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
    #[must_use]
    pub fn get_computed_property(&self, node: &Node) -> Option<&ComputedPropertyData> {
        use super::syntax_kind_ext::COMPUTED_PROPERTY_NAME;
        if node.has_data() && node.kind == COMPUTED_PROPERTY_NAME {
            self.computed_properties.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Number of nodes in the arena
    #[must_use]
    pub const fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if arena is empty
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

// =============================================================================
// Index-based convenience accessors: get(index) + get_TYPE(node) in one call
// =============================================================================

/// Generate `get_*_at(index: NodeIndex) -> Option<&T>` convenience methods
/// that combine `arena.get(index)` with a typed getter in a single call.
macro_rules! define_at_accessors {
    ($($at_name:ident => $getter:ident -> $ret:ty);* $(;)?) => {
        impl NodeArena {
            $(
                #[inline]
#[must_use]
                pub fn $at_name(&self, index: NodeIndex) -> Option<&$ret> {
                    self.$getter(self.get(index)?)
                }
            )*
        }
    };
}

define_at_accessors! {
    get_identifier_at => get_identifier -> IdentifierData;
    get_literal_at => get_literal -> LiteralData;
    get_binary_expr_at => get_binary_expr -> BinaryExprData;
    get_call_expr_at => get_call_expr -> CallExprData;
    get_access_expr_at => get_access_expr -> AccessExprData;
    get_conditional_expr_at => get_conditional_expr -> ConditionalExprData;
    get_qualified_name_at => get_qualified_name -> QualifiedNameData;
    get_literal_expr_at => get_literal_expr -> LiteralExprData;
    get_property_assignment_at => get_property_assignment -> PropertyAssignmentData;
    get_type_assertion_at => get_type_assertion -> TypeAssertionData;
    get_unary_expr_at => get_unary_expr -> UnaryExprData;
    get_unary_expr_ex_at => get_unary_expr_ex -> UnaryExprDataEx;
    get_function_at => get_function -> FunctionData;
    get_class_at => get_class -> ClassData;
    get_block_at => get_block -> BlockData;
    get_source_file_at => get_source_file -> SourceFileData;
    get_variable_at => get_variable -> VariableData;
    get_variable_declaration_at => get_variable_declaration -> VariableDeclarationData;
    get_interface_at => get_interface -> InterfaceData;
    get_type_alias_at => get_type_alias -> TypeAliasData;
    get_enum_at => get_enum -> EnumData;
    get_enum_member_at => get_enum_member -> EnumMemberData;
    get_module_at => get_module -> ModuleData;
    get_module_block_at => get_module_block -> ModuleBlockData;
    get_if_statement_at => get_if_statement -> IfStatementData;
    get_loop_at => get_loop -> LoopData;
    get_for_in_of_at => get_for_in_of -> ForInOfData;
    get_switch_at => get_switch -> SwitchData;
    get_case_clause_at => get_case_clause -> CaseClauseData;
    get_try_at => get_try -> TryData;
    get_catch_clause_at => get_catch_clause -> CatchClauseData;
    get_labeled_statement_at => get_labeled_statement -> LabeledData;
    get_jump_data_at => get_jump_data -> JumpData;
    get_with_statement_at => get_with_statement -> IfStatementData;
    get_import_decl_at => get_import_decl -> ImportDeclData;
    get_import_clause_at => get_import_clause -> ImportClauseData;
    get_named_imports_at => get_named_imports -> NamedImportsData;
    get_specifier_at => get_specifier -> SpecifierData;
    get_export_decl_at => get_export_decl -> ExportDeclData;
    get_export_assignment_at => get_export_assignment -> ExportAssignmentData;
    get_import_attributes_data_at => get_import_attributes_data -> ImportAttributesData;
    get_import_attribute_data_at => get_import_attribute_data -> ImportAttributeData;
    get_parameter_at => get_parameter -> ParameterData;
    get_property_decl_at => get_property_decl -> PropertyDeclData;
    get_method_decl_at => get_method_decl -> MethodDeclData;
    get_constructor_at => get_constructor -> ConstructorData;
    get_accessor_at => get_accessor -> AccessorData;
    get_decorator_at => get_decorator -> DecoratorData;
    get_type_ref_at => get_type_ref -> TypeRefData;
    get_expression_statement_at => get_expression_statement -> ExprStatementData;
    get_return_statement_at => get_return_statement -> ReturnData;
    get_jsx_element_at => get_jsx_element -> JsxElementData;
    get_jsx_opening_at => get_jsx_opening -> JsxOpeningData;
    get_jsx_closing_at => get_jsx_closing -> JsxClosingData;
    get_jsx_fragment_at => get_jsx_fragment -> JsxFragmentData;
    get_jsx_attributes_at => get_jsx_attributes -> JsxAttributesData;
    get_jsx_attribute_at => get_jsx_attribute -> JsxAttributeData;
    get_jsx_spread_attribute_at => get_jsx_spread_attribute -> JsxSpreadAttributeData;
    get_jsx_expression_at => get_jsx_expression -> JsxExpressionData;
    get_jsx_text_at => get_jsx_text -> JsxTextData;
    get_jsx_namespaced_name_at => get_jsx_namespaced_name -> JsxNamespacedNameData;
    get_signature_at => get_signature -> SignatureData;
    get_index_signature_at => get_index_signature -> IndexSignatureData;
    get_heritage_clause_at => get_heritage_clause -> HeritageData;
    get_composite_type_at => get_composite_type -> CompositeTypeData;
    get_array_type_at => get_array_type -> ArrayTypeData;
    get_tuple_type_at => get_tuple_type -> TupleTypeData;
    get_function_type_at => get_function_type -> FunctionTypeData;
    get_type_literal_at => get_type_literal -> TypeLiteralData;
    get_conditional_type_at => get_conditional_type -> ConditionalTypeData;
    get_mapped_type_at => get_mapped_type -> MappedTypeData;
    get_indexed_access_type_at => get_indexed_access_type -> IndexedAccessTypeData;
    get_literal_type_at => get_literal_type -> LiteralTypeData;
    get_wrapped_type_at => get_wrapped_type -> WrappedTypeData;
    get_expr_type_args_at => get_expr_type_args -> ExprWithTypeArgsData;
    get_type_query_at => get_type_query -> TypeQueryData;
    get_type_operator_at => get_type_operator -> TypeOperatorData;
    get_infer_type_at => get_infer_type -> InferTypeData;
    get_template_literal_type_at => get_template_literal_type -> TemplateLiteralTypeData;
    get_named_tuple_member_at => get_named_tuple_member -> NamedTupleMemberData;
    get_type_predicate_at => get_type_predicate -> TypePredicateData;
    get_type_parameter_at => get_type_parameter -> TypeParameterData;
    get_parenthesized_at => get_parenthesized -> ParenthesizedData;
    get_template_expr_at => get_template_expr -> TemplateExprData;
    get_template_span_at => get_template_span -> TemplateSpanData;
    get_tagged_template_at => get_tagged_template -> TaggedTemplateData;
    get_spread_at => get_spread -> SpreadData;
    get_shorthand_property_at => get_shorthand_property -> ShorthandPropertyData;
    get_binding_pattern_at => get_binding_pattern -> BindingPatternData;
    get_binding_element_at => get_binding_element -> BindingElementData;
    get_computed_property_at => get_computed_property -> ComputedPropertyData
}

// NodeView, NodeInfo, and NodeAccess are in node_view.rs

// =============================================================================
// Node Kind Utilities
// =============================================================================

impl Node {
    /// Check if this is an identifier node
    #[inline]
    #[must_use]
    pub const fn is_identifier(&self) -> bool {
        use tsz_scanner::SyntaxKind;
        self.kind == SyntaxKind::Identifier as u16
    }

    /// Check if this is a string literal
    #[inline]
    #[must_use]
    pub const fn is_string_literal(&self) -> bool {
        use tsz_scanner::SyntaxKind;
        self.kind == SyntaxKind::StringLiteral as u16
    }

    /// Check if this is a numeric literal
    #[inline]
    #[must_use]
    pub const fn is_numeric_literal(&self) -> bool {
        use tsz_scanner::SyntaxKind;
        self.kind == SyntaxKind::NumericLiteral as u16
    }

    /// Check if this is a function declaration
    #[inline]
    #[must_use]
    pub const fn is_function_declaration(&self) -> bool {
        use super::syntax_kind_ext::FUNCTION_DECLARATION;
        self.kind == FUNCTION_DECLARATION
    }

    /// Check if this is a class declaration
    #[inline]
    #[must_use]
    pub const fn is_class_declaration(&self) -> bool {
        use super::syntax_kind_ext::CLASS_DECLARATION;
        self.kind == CLASS_DECLARATION
    }

    /// Check if this is any kind of function-like node
    #[inline]
    #[must_use]
    pub const fn is_function_like(&self) -> bool {
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

    /// Check if this is a binding pattern (array or object destructuring)
    #[inline]
    #[must_use]
    pub const fn is_binding_pattern(&self) -> bool {
        self.kind == OBJECT_BINDING_PATTERN || self.kind == ARRAY_BINDING_PATTERN
    }

    /// Check if this is a statement
    #[inline]
    #[must_use]
    pub fn is_statement(&self) -> bool {
        (BLOCK..=DEBUGGER_STATEMENT).contains(&self.kind) || self.kind == VARIABLE_STATEMENT
    }

    /// Check if this is a declaration
    #[inline]
    #[must_use]
    pub fn is_declaration(&self) -> bool {
        (VARIABLE_DECLARATION..=EXPORT_SPECIFIER).contains(&self.kind)
    }

    /// Check if this is a type node
    #[inline]
    #[must_use]
    pub fn is_type_node(&self) -> bool {
        (TYPE_PREDICATE..=IMPORT_TYPE).contains(&self.kind)
    }
}

// Child collection methods are in node_children.rs
// (collect_name_children, collect_expression_children, collect_statement_children,
//  collect_declaration_children, collect_import_export_children, collect_type_children,
//  collect_member_children, collect_pattern_children, collect_jsx_children,
//  collect_signature_children, collect_source_children, and helper functions
//  add_opt_child, add_list, add_opt_list)

// NodeAccess trait and NodeInfo are in node_view.rs
