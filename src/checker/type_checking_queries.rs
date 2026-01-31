//! Type Checking Queries Module
//!
//! This module contains modifier, member access, and query methods for CheckerState.
//! Split from type_checking.rs for maintainability.

use crate::binder::{SymbolId, symbol_flags};
use crate::checker::state::{CheckerState, MemberAccessLevel};
use crate::parser::NodeIndex;
use crate::parser::node::NodeAccess;
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use crate::solver::types::TypeParamInfo;
use crate::solver::{TypeId, TypePredicateTarget};

#[allow(dead_code)]
impl<'a> CheckerState<'a> {
    // =========================================================================
    // Section 27: Modifier and Member Access Utilities
    // =========================================================================

    /// Check if a node has the `declare` modifier.
    pub(crate) fn has_declare_modifier(&self, modifiers: &Option<crate::parser::NodeList>) -> bool {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx)
                    && mod_node.kind == SyntaxKind::DeclareKeyword as u16
                {
                    return true;
                }
            }
        }
        false
    }

    /// Check if a node has the `async` modifier.
    pub(crate) fn has_async_modifier(&self, modifiers: &Option<crate::parser::NodeList>) -> bool {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx)
                    && mod_node.kind == SyntaxKind::AsyncKeyword as u16
                {
                    return true;
                }
            }
        }
        false
    }

    /// Find the `async` modifier NodeIndex in a modifier list, if present.
    pub(crate) fn find_async_modifier(
        &self,
        modifiers: &Option<crate::parser::NodeList>,
    ) -> Option<NodeIndex> {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx)
                    && mod_node.kind == SyntaxKind::AsyncKeyword as u16
                {
                    return Some(mod_idx);
                }
            }
        }
        None
    }

    /// Check if a node has the `abstract` modifier.
    pub(crate) fn has_abstract_modifier(
        &self,
        modifiers: &Option<crate::parser::NodeList>,
    ) -> bool {
        self.has_modifier_kind(modifiers, SyntaxKind::AbstractKeyword)
    }

    /// Check if modifiers include the 'static' keyword.
    pub(crate) fn has_static_modifier(&self, modifiers: &Option<crate::parser::NodeList>) -> bool {
        self.has_modifier_kind(modifiers, SyntaxKind::StaticKeyword)
    }

    /// Check if modifiers include the 'private' keyword.
    pub(crate) fn has_private_modifier(&self, modifiers: &Option<crate::parser::NodeList>) -> bool {
        self.has_modifier_kind(modifiers, SyntaxKind::PrivateKeyword)
    }

    /// Check if modifiers include the 'protected' keyword.
    pub(crate) fn has_protected_modifier(
        &self,
        modifiers: &Option<crate::parser::NodeList>,
    ) -> bool {
        self.has_modifier_kind(modifiers, SyntaxKind::ProtectedKeyword)
    }

    /// Check if modifiers include the 'readonly' keyword.
    pub(crate) fn has_readonly_modifier(
        &self,
        modifiers: &Option<crate::parser::NodeList>,
    ) -> bool {
        self.has_modifier_kind(modifiers, SyntaxKind::ReadonlyKeyword)
    }

    /// Check if modifiers include a parameter property keyword.
    pub(crate) fn has_parameter_property_modifier(
        &self,
        modifiers: &Option<crate::parser::NodeList>,
    ) -> bool {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx)
                    && (mod_node.kind == SyntaxKind::PublicKeyword as u16
                        || mod_node.kind == SyntaxKind::PrivateKeyword as u16
                        || mod_node.kind == SyntaxKind::ProtectedKeyword as u16
                        || mod_node.kind == SyntaxKind::ReadonlyKeyword as u16)
                {
                    return true;
                }
            }
        }
        false
    }

    /// Check if a node is a private identifier.
    pub(crate) fn is_private_identifier_name(&self, name_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(name_idx) else {
            return false;
        };
        node.kind == SyntaxKind::PrivateIdentifier as u16
    }

    /// Check if a member requires nominal typing (private/protected/private identifier).
    pub(crate) fn member_requires_nominal(
        &self,
        modifiers: &Option<crate::parser::NodeList>,
        name_idx: NodeIndex,
    ) -> bool {
        self.has_private_modifier(modifiers)
            || self.has_protected_modifier(modifiers)
            || self.is_private_identifier_name(name_idx)
    }

    /// Get the access level from modifiers (private/protected).
    pub(crate) fn member_access_level_from_modifiers(
        &self,
        modifiers: &Option<crate::parser::NodeList>,
    ) -> Option<MemberAccessLevel> {
        if self.has_private_modifier(modifiers) {
            return Some(MemberAccessLevel::Private);
        }
        if self.has_protected_modifier(modifiers) {
            return Some(MemberAccessLevel::Protected);
        }
        None
    }

    /// Check if a member with the given name is static by looking up its symbol flags.
    /// Uses the binder's symbol information for efficient O(1) flag checks.
    pub(crate) fn is_static_member(&self, member_nodes: &[NodeIndex], name: &str) -> bool {
        for &member_idx in member_nodes {
            // Get symbol for this member
            if let Some(sym_id) = self.ctx.binder.get_node_symbol(member_idx)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            {
                // Check if name matches and symbol has STATIC flag
                if symbol.escaped_name == name && (symbol.flags & symbol_flags::STATIC != 0) {
                    return true;
                }
            }
        }
        false
    }

    /// Check if a member with the given name is an abstract property by looking up its symbol flags.
    /// Only checks properties (not methods) because accessing this.abstractMethod() in constructor is allowed.
    pub(crate) fn is_abstract_member(&self, member_nodes: &[NodeIndex], name: &str) -> bool {
        for &member_idx in member_nodes {
            // Get symbol for this member
            if let Some(sym_id) = self.ctx.binder.get_node_symbol(member_idx)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            {
                // Check if name matches and symbol has ABSTRACT flag (property only)
                if symbol.escaped_name == name
                    && (symbol.flags & symbol_flags::ABSTRACT != 0)
                    && (symbol.flags & symbol_flags::PROPERTY != 0)
                {
                    return true;
                }
            }
        }
        false
    }

    // =========================================================================
    // Section 28: Expression Analysis Utilities
    // =========================================================================

    /// Skip parenthesized expressions to get to the underlying expression.
    pub(crate) fn skip_parenthesized_expression(&self, mut expr_idx: NodeIndex) -> NodeIndex {
        while let Some(node) = self.ctx.arena.get(expr_idx) {
            if node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                break;
            }
            let Some(paren) = self.ctx.arena.get_parenthesized(node) else {
                break;
            };
            expr_idx = paren.expression;
        }
        expr_idx
    }

    /// Check if an expression is side-effect free.
    ///
    /// Returns true if the expression does not modify state or have observable effects.
    /// This matches TypeScript's hasSideEffects logic.
    ///
    /// Side-effect free expressions:
    /// - Literals (number, string, boolean, null, undefined, regex)
    /// - Identifiers (variable reads)
    /// - Type assertions and typeof
    /// - Function/class/arrow function expressions (defining, not calling)
    /// - Array/object literals (recursively checked)
    /// - Conditional expressions (recursively checked)
    /// - Binary expressions with non-assignment operators (recursively checked)
    /// - Unary expressions like !, +, -, ~, typeof (recursively checked)
    ///
    /// Has side effects (returns false):
    /// - Function calls, new expressions, await, yield
    /// - Assignments (=, +=, etc.)
    /// - Increment/decrement (++, --)
    /// - Property/element access (may trigger getters)
    /// - Tagged templates (function calls)
    /// - Delete expressions
    pub(crate) fn is_side_effect_free(&self, expr_idx: NodeIndex) -> bool {
        let expr_idx = self.skip_parenthesized_expression(expr_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };

        match node.kind {
            // Literals and identifiers are side-effect free
            k if k == SyntaxKind::Identifier as u16
                || k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::RegularExpressionLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::BigIntLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::UndefinedKeyword as u16 =>
            {
                true
            }
            // Tagged templates are function calls - they have side effects
            // k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
            // Plain template literals, function/class definitions, arrays/objects, typeof, non-null, JSX
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::CLASS_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::TYPE_OF_EXPRESSION
                || k == syntax_kind_ext::NON_NULL_EXPRESSION
                || k == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
                || k == syntax_kind_ext::JSX_ELEMENT =>
            {
                true
            }
            // Conditional: check branches (condition can be side-effect free)
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                let Some(cond) = self.ctx.arena.get_conditional_expr(node) else {
                    return false;
                };
                self.is_side_effect_free(cond.when_true)
                    && self.is_side_effect_free(cond.when_false)
            }
            // Binary: check both sides, unless it's an assignment
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let Some(bin) = self.ctx.arena.get_binary_expr(node) else {
                    return false;
                };
                if self.is_assignment_operator(bin.operator_token) {
                    return false;
                }
                self.is_side_effect_free(bin.left) && self.is_side_effect_free(bin.right)
            }
            // Unary: only !, +, -, ~, typeof are side-effect free
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION =>
            {
                let Some(unary) = self.ctx.arena.get_unary_expr(node) else {
                    return false;
                };
                matches!(
                    unary.operator,
                    k if k == SyntaxKind::ExclamationToken as u16
                        || k == SyntaxKind::PlusToken as u16
                        || k == SyntaxKind::MinusToken as u16
                        || k == SyntaxKind::TildeToken as u16
                        || k == SyntaxKind::TypeOfKeyword as u16
                )
            }
            // Property access, element access, calls, tagged templates, etc. have side effects
            _ => false,
        }
    }

    /// Check if a comma expression is an indirect call (e.g., `(0, obj.method)()`).
    /// This pattern is used to change the `this` binding for the call.
    pub(crate) fn is_indirect_call(
        &self,
        comma_idx: NodeIndex,
        left: NodeIndex,
        right: NodeIndex,
    ) -> bool {
        let parent = self
            .ctx
            .arena
            .get_extended(comma_idx)
            .map(|ext| ext.parent)
            .unwrap_or(NodeIndex::NONE);
        if parent.is_none() {
            return false;
        }
        let Some(parent_node) = self.ctx.arena.get(parent) else {
            return false;
        };
        if parent_node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            return false;
        }
        if !self.is_numeric_literal_zero(left) {
            return false;
        }

        let grand_parent = self
            .ctx
            .arena
            .get_extended(parent)
            .map(|ext| ext.parent)
            .unwrap_or(NodeIndex::NONE);
        if grand_parent.is_none() {
            return false;
        }
        let Some(grand_node) = self.ctx.arena.get(grand_parent) else {
            return false;
        };

        let is_indirect_target = if grand_node.kind == syntax_kind_ext::CALL_EXPRESSION {
            if let Some(call) = self.ctx.arena.get_call_expr(grand_node) {
                call.expression == parent
            } else {
                false
            }
        } else if grand_node.kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION {
            if let Some(tagged) = self.ctx.arena.get_tagged_template(grand_node) {
                tagged.tag == parent
            } else {
                false
            }
        } else {
            false
        };
        if !is_indirect_target {
            return false;
        }

        if self.is_access_expression(right) {
            return true;
        }
        let Some(right_node) = self.ctx.arena.get(right) else {
            return false;
        };
        if right_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(ident) = self.ctx.arena.get_identifier(right_node) else {
            return false;
        };
        ident.escaped_text == "eval"
    }

    // =========================================================================
    // Section 29: Expression Kind Detection Utilities
    // =========================================================================

    /// Check if a node is a `this` expression.
    pub(crate) fn is_this_expression(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        node.kind == SyntaxKind::ThisKeyword as u16
    }

    /// Check if a node is a `globalThis` identifier expression.
    pub(crate) fn is_global_this_expression(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(node) else {
            return false;
        };
        ident.escaped_text == "globalThis"
    }

    /// Check if a name is a known global value (e.g., console, Math, JSON).
    /// These are globals that should be available in most JavaScript environments.
    pub(crate) fn is_known_global_value_name(&self, name: &str) -> bool {
        matches!(
            name,
            "console"
                | "Math"
                | "JSON"
                | "Object"
                | "Array"
                | "String"
                | "Number"
                | "Boolean"
                | "Function"
                | "Date"
                | "RegExp"
                | "Error"
                | "Promise"
                | "Map"
                | "Set"
                | "WeakMap"
                | "WeakSet"
                | "WeakRef"
                | "Proxy"
                | "Reflect"
                | "globalThis"
                | "window"
                | "document"
                | "exports"
                | "module"
                | "require"
                | "__dirname"
                | "__filename"
                | "FinalizationRegistry"
                | "BigInt"
                | "ArrayBuffer"
                | "SharedArrayBuffer"
                | "DataView"
                | "Int8Array"
                | "Uint8Array"
                | "Uint8ClampedArray"
                | "Int16Array"
                | "Uint16Array"
                | "Int32Array"
                | "Uint32Array"
                | "Float32Array"
                | "Float64Array"
                | "BigInt64Array"
                | "BigUint64Array"
                | "Intl"
                | "Atomics"
                | "WebAssembly"
                | "Iterator"
                | "AsyncIterator"
                | "Generator"
                | "AsyncGenerator"
                | "URL"
                | "URLSearchParams"
                | "Headers"
                | "Request"
                | "Response"
                | "FormData"
                | "Blob"
                | "File"
                | "ReadableStream"
                | "WritableStream"
                | "TransformStream"
                | "TextEncoder"
                | "TextDecoder"
                | "AbortController"
                | "AbortSignal"
                | "fetch"
                | "setTimeout"
                | "setInterval"
                | "clearTimeout"
                | "clearInterval"
                | "queueMicrotask"
                | "structuredClone"
                | "atob"
                | "btoa"
                | "performance"
                | "crypto"
                | "navigator"
                | "location"
                | "history"
        )
    }

    /// Check if a name is a Node.js runtime global that is always available.
    /// These globals are injected by the Node.js runtime and don't require lib.d.ts.
    /// Note: console, globalThis, and process are NOT included here because they
    /// require proper lib definitions (lib.dom.d.ts, lib.es2020.d.ts, @types/node).
    pub(crate) fn is_nodejs_runtime_global(&self, name: &str) -> bool {
        matches!(
            name,
            "exports" | "module" | "require" | "__dirname" | "__filename"
        )
    }

    // =========================================================================
    // Section 30: Name Extraction Utilities
    // =========================================================================

    /// Get property name as string from a property name node (identifier, string literal, etc.)
    pub(crate) fn get_property_name(&self, name_idx: NodeIndex) -> Option<String> {
        let name_node = self.ctx.arena.get(name_idx)?;

        // Identifier
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            return Some(ident.escaped_text.clone());
        }

        if matches!(
            name_node.kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
        ) && let Some(lit) = self.ctx.arena.get_literal(name_node)
            && !lit.text.is_empty()
        {
            return Some(lit.text.clone());
        }

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.ctx.arena.get_computed_property(name_node)
        {
            if let Some(symbol_name) = self.get_symbol_property_name_from_expr(computed.expression)
            {
                return Some(symbol_name);
            }
            if let Some(expr_node) = self.ctx.arena.get(computed.expression)
                && matches!(
                    expr_node.kind,
                    k if k == SyntaxKind::StringLiteral as u16
                        || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                        || k == SyntaxKind::NumericLiteral as u16
                )
                && let Some(lit) = self.ctx.arena.get_literal(expr_node)
                && !lit.text.is_empty()
            {
                return Some(lit.text.clone());
            }
        }

        None
    }

    /// Get class name from a class declaration node.
    /// Returns "<anonymous>" for unnamed classes.
    pub(crate) fn get_class_name_from_decl(&self, class_idx: NodeIndex) -> String {
        let Some(node) = self.ctx.arena.get(class_idx) else {
            return "<anonymous>".to_string();
        };
        let Some(class) = self.ctx.arena.get_class(node) else {
            return "<anonymous>".to_string();
        };

        if !class.name.is_none()
            && let Some(name_node) = self.ctx.arena.get(class.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            return ident.escaped_text.clone();
        }

        "<anonymous>".to_string()
    }

    /// Get the name of a class member (property, method, or accessor).
    pub(crate) fn get_member_name(&self, member_idx: NodeIndex) -> Option<String> {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return None;
        };

        // Use helper to get name node, then get property name text
        let name_idx = self.get_member_name_node(node)?;
        self.get_property_name(name_idx)
    }

    /// Get the name of a function declaration.
    pub(crate) fn get_function_name_from_node(&self, stmt_idx: NodeIndex) -> Option<String> {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return None;
        };

        if let Some(func) = self.ctx.arena.get_function(node)
            && !func.name.is_none()
        {
            let Some(name_node) = self.ctx.arena.get(func.name) else {
                return None;
            };
            if let Some(id) = self.ctx.arena.get_identifier(name_node) {
                return Some(id.escaped_text.clone());
            }
        }

        None
    }

    /// Get the name of a parameter from its binding name node.
    /// Returns None for destructuring patterns.
    pub(crate) fn get_parameter_name(&self, name_idx: NodeIndex) -> Option<String> {
        let name_node = self.ctx.arena.get(name_idx)?;
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            return Some(ident.escaped_text.clone());
        }
        None
    }

    // =========================================================================
    // Section 31: Class Hierarchy Utilities
    // =========================================================================

    /// Get the base class node index from a class declaration.
    /// Returns None if the class doesn't extend anything.
    pub(crate) fn get_base_class_idx(&self, class_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(class_idx)?;
        let class = self.ctx.arena.get_class(node)?;
        let heritage_clauses = class.heritage_clauses.as_ref()?;

        for &clause_idx in &heritage_clauses.nodes {
            let clause_node = self.ctx.arena.get(clause_idx)?;
            let heritage = self.ctx.arena.get_heritage_clause(clause_node)?;
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let &type_idx = heritage.types.nodes.first()?;
            let type_node = self.ctx.arena.get(type_idx)?;
            let expr_idx =
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                    expr_type_args.expression
                } else {
                    type_idx
                };
            let base_sym_id = self.resolve_heritage_symbol(expr_idx)?;
            return self.get_class_declaration_from_symbol(base_sym_id);
        }

        None
    }

    /// Check if a derived class is derived from a base class.
    /// Traverses the inheritance chain to check if base_idx is an ancestor of derived_idx.
    pub(crate) fn is_class_derived_from(
        &self,
        derived_idx: NodeIndex,
        base_idx: NodeIndex,
    ) -> bool {
        use rustc_hash::FxHashSet;

        if derived_idx == base_idx {
            return true;
        }

        let mut visited: FxHashSet<NodeIndex> = FxHashSet::default();
        let mut current = derived_idx;

        while visited.insert(current) {
            let Some(parent) = self.get_base_class_idx(current) else {
                return false;
            };
            if parent == base_idx {
                return true;
            }
            current = parent;
        }

        false
    }

    // =========================================================================
    // Section 32: Context and Expression Utilities
    // =========================================================================

    /// Get the current `this` type from the type stack.
    /// Returns None if there's no current `this` type in scope.
    pub(crate) fn current_this_type(&self) -> Option<TypeId> {
        self.ctx.this_type_stack.last().copied()
    }

    /// Check if a node is a `super` expression.
    pub(crate) fn is_super_expression(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        node.kind == SyntaxKind::SuperKeyword as u16
    }

    /// Check if a call expression is a dynamic import (`import('...')`).
    pub(crate) fn is_dynamic_import(&self, call: &crate::parser::node::CallExprData) -> bool {
        let Some(node) = self.ctx.arena.get(call.expression) else {
            return false;
        };
        node.kind == SyntaxKind::ImportKeyword as u16
    }

    // =========================================================================
    // Section 33: Literal Extraction Utilities
    // =========================================================================

    /// Get a numeric literal index from a node.
    /// Returns None if the node is not a non-negative integer literal.
    pub(crate) fn get_literal_index_from_node(&self, idx: NodeIndex) -> Option<usize> {
        let Some(node) = self.ctx.arena.get(idx) else {
            return None;
        };

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.ctx.arena.get_parenthesized(node)
        {
            return self.get_literal_index_from_node(paren.expression);
        }

        if node.kind == SyntaxKind::NumericLiteral as u16
            && let Some(lit) = self.ctx.arena.get_literal(node)
            && let Some(value) = lit.value
            && value.is_finite()
            && value.fract() == 0.0
            && value >= 0.0
        {
            return Some(value as usize);
        }

        None
    }

    /// Get a string literal from a node.
    /// Returns None if the node is not a string literal or template literal.
    pub(crate) fn get_literal_string_from_node(&self, idx: NodeIndex) -> Option<String> {
        let Some(node) = self.ctx.arena.get(idx) else {
            return None;
        };

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.ctx.arena.get_parenthesized(node)
        {
            return self.get_literal_string_from_node(paren.expression);
        }

        if let Some(symbol_name) = self.get_symbol_property_name_from_expr(idx) {
            return Some(symbol_name);
        }

        if node.kind == SyntaxKind::StringLiteral as u16
            || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            return self.ctx.arena.get_literal(node).map(|lit| lit.text.clone());
        }

        None
    }

    /// Parse a numeric index from a string.
    /// Returns None if the string is not a valid non-negative integer.
    pub(crate) fn get_numeric_index_from_string(&self, value: &str) -> Option<usize> {
        let parsed: f64 = value.parse().ok()?;
        if !parsed.is_finite() || parsed.fract() != 0.0 || parsed < 0.0 {
            return None;
        }
        if parsed > (usize::MAX as f64) {
            return None;
        }
        Some(parsed as usize)
    }

    // =========================================================================
    // Section 34: Type Validation Utilities
    // =========================================================================

    /// Check if a type can be array-destructured.
    /// Returns true for arrays, tuples, strings, and types with [Symbol.iterator].
    pub(crate) fn is_array_destructurable_type(&self, type_id: TypeId) -> bool {
        use crate::solver::type_queries;

        // Handle primitive types
        if type_id == TypeId::STRING {
            return true;
        }

        // Use type_queries for Array and Tuple detection
        if type_queries::is_array_type(self.ctx.types, type_id) {
            return true;
        }
        if type_queries::is_tuple_type(self.ctx.types, type_id) {
            return true;
        }

        // Handle ReadonlyType by unwrapping and recursing
        let unwrapped = type_queries::unwrap_readonly(self.ctx.types, type_id);
        if unwrapped != type_id {
            return self.is_array_destructurable_type(unwrapped);
        }

        // Union types: all members must be destructurable
        if let Some(members) = type_queries::get_union_members(self.ctx.types, type_id) {
            return members
                .iter()
                .all(|&t| self.is_array_destructurable_type(t));
        }

        // Intersection types: at least one member must be array-like
        if let Some(members) = type_queries::get_intersection_members(self.ctx.types, type_id) {
            return members
                .iter()
                .any(|&t| self.is_array_destructurable_type(t));
        }

        // Object types might have an iterator - for now conservatively return false
        if type_queries::is_object_type(self.ctx.types, type_id) {
            return false;
        }

        // Literal types: check the base type (string literals are destructurable)
        if type_queries::is_string_literal(self.ctx.types, type_id) {
            return true;
        }

        // Other types are not array-destructurable
        false
    }

    // =========================================================================
    // Section 35: Symbol and Declaration Utilities
    // =========================================================================

    /// Get the class declaration node from a symbol.
    /// Returns None if the symbol doesn't represent a class.
    pub(crate) fn get_class_declaration_from_symbol(
        &self,
        sym_id: crate::binder::SymbolId,
    ) -> Option<NodeIndex> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.value_declaration.is_none() {
            let decl_idx = symbol.value_declaration;
            if let Some(node) = self.ctx.arena.get(decl_idx)
                && self.ctx.arena.get_class(node).is_some()
            {
                return Some(decl_idx);
            }
        }

        for &decl_idx in &symbol.declarations {
            if let Some(node) = self.ctx.arena.get(decl_idx)
                && self.ctx.arena.get_class(node).is_some()
            {
                return Some(decl_idx);
            }
        }

        None
    }

    // =========================================================================
    // Section 36: Type Query Utilities
    // =========================================================================

    /// Check if a type contains ERROR anywhere in its structure.
    /// Recursively checks all type components for error types.
    ///
    /// Uses the solver's visitor pattern which provides:
    /// - Cycle detection via FxHashSet
    /// - Max depth protection (20 levels)
    /// - Comprehensive type traversal including function parameters
    pub(crate) fn type_contains_error(&self, type_id: TypeId) -> bool {
        crate::solver::contains_error_type(self.ctx.types, type_id)
    }

    // =========================================================================
    // Section 37: Nullish Type Utilities
    // =========================================================================

    /// Split a type into its non-nullable part and its nullable cause.
    /// Returns (non_null_type, nullable_cause) where nullable_cause is the type that makes it nullable.
    pub(crate) fn split_nullish_type(
        &mut self,
        type_id: TypeId,
    ) -> (Option<TypeId>, Option<TypeId>) {
        crate::solver::split_nullish_type(&self.ctx.types, type_id)
    }

    /// Report an error for possibly nullish object access.
    /// Reports the appropriate error code based on the nullable cause type.
    /// When the node is directly a `null` or `undefined` keyword, emits TS18050.
    ///
    /// TS2531/2532/2533 are only emitted when strictNullChecks is enabled.
    /// TS18050 is always emitted for literal null/undefined keywords.
    pub(crate) fn report_possibly_nullish_object(&mut self, idx: NodeIndex, cause: TypeId) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };
        use crate::scanner::SyntaxKind;

        // Check if the node is directly a null or undefined keyword - emit TS18050
        // TS18050 is not gated by strictNullChecks
        if let Some(node) = self.ctx.arena.get(idx) {
            if node.kind == SyntaxKind::NullKeyword as u16 {
                let message =
                    format_message(diagnostic_messages::VALUE_CANNOT_BE_USED_HERE, &["null"]);
                self.error_at_node(idx, &message, diagnostic_codes::VALUE_CANNOT_BE_USED_HERE);
                return;
            }
            if node.kind == SyntaxKind::Identifier as u16 {
                if let Some(ident) = self.ctx.arena.get_identifier(node) {
                    if ident.escaped_text == "undefined" {
                        let message = format_message(
                            diagnostic_messages::VALUE_CANNOT_BE_USED_HERE,
                            &["undefined"],
                        );
                        self.error_at_node(
                            idx,
                            &message,
                            diagnostic_codes::VALUE_CANNOT_BE_USED_HERE,
                        );
                        return;
                    }
                }
            }
        }

        // TS2531/2532/2533 require strictNullChecks. When strictNullChecks is off,
        // null/undefined are assignable to all types and property access is allowed.
        if !self.ctx.compiler_options.strict_null_checks {
            return;
        }

        let (code, message) = if cause == TypeId::NULL {
            (
                diagnostic_codes::OBJECT_IS_POSSIBLY_NULL,
                "Object is possibly 'null'.",
            )
        } else if cause == TypeId::UNDEFINED {
            (
                diagnostic_codes::OBJECT_IS_POSSIBLY_UNDEFINED,
                "Object is possibly 'undefined'.",
            )
        } else {
            (
                diagnostic_codes::OBJECT_IS_POSSIBLY_NULL_OR_UNDEFINED,
                "Object is possibly 'null' or 'undefined'.",
            )
        };

        self.error_at_node(idx, message, code);
    }

    // =========================================================================
    // Section 38: Index Signature Utilities
    // =========================================================================

    /// Merge an incoming index signature into a target.
    /// If the signatures conflict, sets the target to ERROR.
    pub(crate) fn merge_index_signature(
        target: &mut Option<crate::solver::IndexSignature>,
        incoming: crate::solver::IndexSignature,
    ) {
        if let Some(existing) = target.as_mut() {
            if existing.value_type != incoming.value_type || existing.readonly != incoming.readonly
            {
                existing.value_type = TypeId::ERROR;
                existing.readonly = false;
            }
        } else {
            *target = Some(incoming);
        }
    }

    // =========================================================================
    // Section 39: Type Parameter Scope Utilities
    // =========================================================================

    /// Pop type parameters from scope, restoring previous values.
    /// Used to restore the type parameter scope after exiting a generic context.
    pub(crate) fn pop_type_parameters(&mut self, updates: Vec<(String, Option<TypeId>)>) {
        for (name, previous) in updates.into_iter().rev() {
            if let Some(prev_type) = previous {
                self.ctx.type_parameter_scope.insert(name, prev_type);
            } else {
                self.ctx.type_parameter_scope.remove(&name);
            }
        }
    }

    /// Collect all `infer` type parameter names from a type node.
    /// This is used to add inferred type parameters to the scope when checking conditional types.
    pub(crate) fn collect_infer_type_parameters(&self, type_idx: NodeIndex) -> Vec<String> {
        let mut params = Vec::new();
        self.collect_infer_type_parameters_inner(type_idx, &mut params);
        params
    }

    /// Inner implementation for collecting infer type parameters.
    /// Recursively walks the type node to find all infer type parameter names.
    fn collect_infer_type_parameters_inner(&self, type_idx: NodeIndex, params: &mut Vec<String>) {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::INFER_TYPE => {
                if let Some(infer) = self.ctx.arena.get_infer_type(node)
                    && let Some(param_node) = self.ctx.arena.get(infer.type_parameter)
                    && let Some(param) = self.ctx.arena.get_type_parameter(param_node)
                    && let Some(name_node) = self.ctx.arena.get(param.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                {
                    let name = ident.escaped_text.clone();
                    if !params.contains(&name) {
                        params.push(name);
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.ctx.arena.get_type_ref(node)
                    && let Some(ref args) = type_ref.type_arguments
                {
                    for &arg_idx in &args.nodes {
                        self.collect_infer_type_parameters_inner(arg_idx, params);
                    }
                }
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                    for &member_idx in &composite.types.nodes {
                        self.collect_infer_type_parameters_inner(member_idx, params);
                    }
                }
            }
            _ => {}
        }
    }

    // Section 40: Node and Name Utilities
    // ------------------------------------

    /// Get the text content of a node from the source file.
    pub(crate) fn node_text(&self, node_idx: NodeIndex) -> Option<String> {
        let (start, end) = self.get_node_span(node_idx)?;
        let source = self.ctx.arena.source_files.first()?.text.as_ref();
        let start = start as usize;
        let end = end as usize;
        if start >= end || end > source.len() {
            return None;
        }
        Some(source[start..end].to_string())
    }

    /// Get the name of a parameter for error messages.
    pub(crate) fn parameter_name_for_error(&self, name_idx: NodeIndex) -> String {
        if let Some(name_node) = self.ctx.arena.get(name_idx) {
            if name_node.kind == SyntaxKind::ThisKeyword as u16 {
                return "this".to_string();
            }
            if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                return ident.escaped_text.clone();
            }
            if let Some(lit) = self.ctx.arena.get_literal(name_node) {
                return lit.text.clone();
            }
        }

        self.node_text(name_idx)
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| "parameter".to_string())
    }

    /// Get the name of a property for error messages.
    pub(crate) fn property_name_for_error(&self, name_idx: NodeIndex) -> Option<String> {
        self.get_property_name(name_idx).or_else(|| {
            self.node_text(name_idx)
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty())
        })
    }

    /// Collect all nodes within an initializer expression that reference a given name.
    /// Used for TS2372: parameter cannot reference itself.
    ///
    /// Recursively walks the initializer AST and collects every identifier node
    /// that matches `name`. Stops recursion at scope boundaries (function expressions,
    /// arrow functions, class expressions) since those introduce new scopes where
    /// the identifier would not be a self-reference of the outer parameter.
    ///
    /// Returns a list of NodeIndex values, one for each self-referencing identifier.
    /// TSC emits a separate TS2372 error for each occurrence.
    pub(crate) fn collect_self_references(
        &self,
        init_idx: NodeIndex,
        name: &str,
    ) -> Vec<NodeIndex> {
        let mut refs = Vec::new();
        self.collect_self_references_recursive(init_idx, name, &mut refs);
        refs
    }

    /// Recursive helper for collect_self_references.
    fn collect_self_references_recursive(
        &self,
        node_idx: NodeIndex,
        name: &str,
        refs: &mut Vec<NodeIndex>,
    ) {
        if node_idx.is_none() {
            return;
        }
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        // If this node is an identifier matching the parameter name, record it
        if let Some(ident) = self.ctx.arena.get_identifier(node) {
            if ident.escaped_text == name {
                refs.push(node_idx);
            }
            return;
        }

        // Stop at scope boundaries: function expressions, arrow functions,
        // and class expressions introduce new scopes where the name would
        // refer to something different (not the outer parameter).
        match node.kind {
            syntax_kind_ext::FUNCTION_EXPRESSION
            | syntax_kind_ext::ARROW_FUNCTION
            | syntax_kind_ext::CLASS_EXPRESSION => {
                return;
            }
            _ => {}
        }

        // Recurse into all children of this node
        let children = self.ctx.arena.get_children(node_idx);
        for child_idx in children {
            self.collect_self_references_recursive(child_idx, name, refs);
        }
    }

    // Section 41: Function Implementation Checking
    // --------------------------------------------

    /// Infer the return type of a getter from its body.
    pub(crate) fn infer_getter_return_type(&mut self, body_idx: NodeIndex) -> TypeId {
        if body_idx.is_none() {
            return TypeId::VOID;
        }

        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return TypeId::VOID;
        };

        // If it's a block, look for return statements
        if body_node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = self.ctx.arena.get_block(body_node)
        {
            for &stmt_idx in &block.statements.nodes {
                if let Some(stmt_node) = self.ctx.arena.get(stmt_idx)
                    && stmt_node.kind == syntax_kind_ext::RETURN_STATEMENT
                    && let Some(ret) = self.ctx.arena.get_return_statement(stmt_node)
                    && !ret.expression.is_none()
                {
                    return self.get_type_of_node(ret.expression);
                }
            }
        }

        // No return statements with values found - return void (not any)
        // This prevents false positive TS7010 errors for getters without return statements
        TypeId::VOID
    }

    /// Check that all top-level function overload signatures have implementations.
    /// Reports errors 2389, 2391.
    pub(crate) fn check_function_implementations(&mut self, statements: &[NodeIndex]) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        let mut i = 0;
        while i < statements.len() {
            let stmt_idx = statements[i];
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                i += 1;
                continue;
            };

            if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                && let Some(func) = self.ctx.arena.get_function(node)
                && func.body.is_none()
            {
                let is_declared = self.has_declare_modifier(&func.modifiers);
                // Use func.is_async as the parser stores async as a flag, not a modifier
                let is_async = func.is_async;

                // TS1040: 'async' modifier cannot be used in an ambient context
                if is_declared && is_async {
                    self.error_at_node(
                        stmt_idx,
                        "'async' modifier cannot be used in an ambient context.",
                        diagnostic_codes::ASYNC_MODIFIER_IN_AMBIENT_CONTEXT,
                    );
                    i += 1;
                    continue;
                }

                if is_declared {
                    i += 1;
                    continue;
                }
                // Function overload signature - check for implementation
                let func_name = self.get_function_name_from_node(stmt_idx);
                if let Some(name) = func_name {
                    let (has_impl, impl_name) = self.find_function_impl(statements, i + 1, &name);
                    if !has_impl {
                        self.error_at_node(
                                    stmt_idx,
                                    "Function implementation is missing or not immediately following the declaration.",
                                    diagnostic_codes::FUNCTION_IMPLEMENTATION_MISSING
                                );
                    } else if let Some(actual_name) = impl_name
                        && actual_name != name
                    {
                        // Implementation has wrong name
                        self.error_at_node(
                            statements[i + 1],
                            &format!("Function implementation name must be '{}'.", name),
                            diagnostic_codes::FUNCTION_IMPLEMENTATION_NAME_MUST_BE,
                        );
                    }
                }
            }
            i += 1;
        }
    }

    // Section 42: Class Member Utilities
    // ------------------------------------

    /// Check if a class member is static.
    pub(crate) fn class_member_is_static(&self, member_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                .ctx
                .arena
                .get_property_decl(node)
                .map(|prop| self.has_static_modifier(&prop.modifiers))
                .unwrap_or(false),
            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                .ctx
                .arena
                .get_method_decl(node)
                .map(|method| self.has_static_modifier(&method.modifiers))
                .unwrap_or(false),
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => self
                .ctx
                .arena
                .get_accessor(node)
                .map(|accessor| self.has_static_modifier(&accessor.modifiers))
                .unwrap_or(false),
            _ => false,
        }
    }

    /// Get the declaring type for a private member.
    pub(crate) fn private_member_declaring_type(
        &mut self,
        sym_id: crate::binder::SymbolId,
    ) -> Option<TypeId> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;

        for &decl_idx in &symbol.declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if !matches!(
                node.kind,
                k if k == syntax_kind_ext::PROPERTY_DECLARATION
                    || k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR
            ) {
                continue;
            }

            let Some(ext) = self.ctx.arena.get_extended(decl_idx) else {
                continue;
            };
            if ext.parent.is_none() {
                continue;
            }
            let Some(parent_node) = self.ctx.arena.get(ext.parent) else {
                continue;
            };
            if parent_node.kind != syntax_kind_ext::CLASS_DECLARATION
                && parent_node.kind != syntax_kind_ext::CLASS_EXPRESSION
            {
                continue;
            }
            let Some(class) = self.ctx.arena.get_class(parent_node) else {
                continue;
            };
            let is_static = self.class_member_is_static(decl_idx);
            return Some(if is_static {
                self.get_class_constructor_type(ext.parent, class)
            } else {
                self.get_class_instance_type(ext.parent, class)
            });
        }

        None
    }

    /// Get the this type for a class member.
    pub(crate) fn class_member_this_type(&mut self, member_idx: NodeIndex) -> Option<TypeId> {
        let class_info = self.ctx.enclosing_class.as_ref()?;
        let class_idx = class_info.class_idx;
        let is_static = self.class_member_is_static(member_idx);

        if !is_static {
            // Use the current class type parameters in scope for instance `this`.
            if let Some(node) = self.ctx.arena.get(class_idx)
                && let Some(class) = self.ctx.arena.get_class(node)
            {
                return Some(self.get_class_instance_type(class_idx, class));
            }
        }

        if let Some(sym_id) = self.ctx.binder.get_node_symbol(class_idx) {
            if is_static {
                return Some(self.get_type_of_symbol(sym_id));
            }
            return self.class_instance_type_from_symbol(sym_id);
        }

        let node = self.ctx.arena.get(class_idx)?;
        let class = self.ctx.arena.get_class(node)?;
        Some(if is_static {
            self.get_class_constructor_type(class_idx, class)
        } else {
            self.get_class_instance_type(class_idx, class)
        })
    }

    // Section 43: Accessor Type Checking
    // -----------------------------------

    /// Check that accessor pairs (get/set) have compatible types.
    /// The getter return type must be assignable to the setter parameter type.
    pub(crate) fn check_accessor_type_compatibility(&mut self, members: &[NodeIndex]) {
        use crate::checker::types::diagnostics::diagnostic_codes;
        use std::collections::HashMap;

        // Collect getter return types and setter parameter types
        struct AccessorTypeInfo {
            getter: Option<(NodeIndex, TypeId, NodeIndex, bool, bool)>, // (accessor_idx, return_type, body_or_return_pos, is_abstract, is_declared)
            setter: Option<(NodeIndex, TypeId, bool, bool)>, // (accessor_idx, param_type, is_abstract, is_declared)
        }

        let mut accessors: HashMap<String, AccessorTypeInfo> = HashMap::new();

        for &member_idx in members {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::GET_ACCESSOR {
                if let Some(accessor) = self.ctx.arena.get_accessor(node)
                    && let Some(name) = self.get_property_name(accessor.name)
                {
                    // Check if this accessor is abstract or declared
                    let is_abstract = self.has_abstract_modifier(&accessor.modifiers);
                    let is_declared = self.has_declare_modifier(&accessor.modifiers);

                    // Get the return type - check explicit annotation first
                    let return_type = if !accessor.type_annotation.is_none() {
                        self.get_type_of_node(accessor.type_annotation)
                    } else {
                        // Infer from return statements in body
                        self.infer_getter_return_type(accessor.body)
                    };

                    // Find the position of the return statement for error reporting
                    let error_pos = self
                        .find_return_statement_pos(accessor.body)
                        .unwrap_or(member_idx);

                    let info = accessors.entry(name).or_insert_with(|| AccessorTypeInfo {
                        getter: None,
                        setter: None,
                    });
                    info.getter =
                        Some((member_idx, return_type, error_pos, is_abstract, is_declared));
                }
            } else if node.kind == syntax_kind_ext::SET_ACCESSOR
                && let Some(accessor) = self.ctx.arena.get_accessor(node)
                && let Some(name) = self.get_property_name(accessor.name)
            {
                // Check if this accessor is abstract or declared
                let is_abstract = self.has_abstract_modifier(&accessor.modifiers);
                let is_declared = self.has_declare_modifier(&accessor.modifiers);

                // Get the parameter type from the setter's first parameter
                let param_type = if let Some(&first_param_idx) = accessor.parameters.nodes.first() {
                    if let Some(param_node) = self.ctx.arena.get(first_param_idx) {
                        if let Some(param) = self.ctx.arena.get_parameter(param_node) {
                            if !param.type_annotation.is_none() {
                                self.get_type_of_node(param.type_annotation)
                            } else {
                                TypeId::ANY
                            }
                        } else {
                            TypeId::ANY
                        }
                    } else {
                        TypeId::ANY
                    }
                } else {
                    TypeId::ANY
                };

                let info = accessors.entry(name).or_insert_with(|| AccessorTypeInfo {
                    getter: None,
                    setter: None,
                });
                info.setter = Some((member_idx, param_type, is_abstract, is_declared));
            }
        }

        // Check type compatibility for each accessor pair
        for (_, info) in accessors {
            if let (
                Some((_getter_idx, getter_type, error_pos, getter_abstract, getter_declared)),
                Some((_setter_idx, setter_type, setter_abstract, setter_declared)),
            ) = (info.getter, info.setter)
            {
                // Skip if either accessor is abstract - abstract accessors don't need type compatibility checks
                if getter_abstract || setter_abstract {
                    continue;
                }

                // Skip if either accessor is declared - declared accessors don't need type compatibility checks
                if getter_declared || setter_declared {
                    continue;
                }

                // Skip if either type is ANY (no meaningful check)
                if getter_type == TypeId::ANY || setter_type == TypeId::ANY {
                    continue;
                }

                // Check if getter return type is assignable to setter param type
                if !self.is_assignable_to(getter_type, setter_type) {
                    // Get type strings for error message
                    let getter_type_str = self.format_type(getter_type);
                    let setter_type_str = self.format_type(setter_type);

                    self.error_at_node(
                        error_pos,
                        &format!(
                            "Type '{}' is not assignable to type '{}'.",
                            getter_type_str, setter_type_str
                        ),
                        diagnostic_codes::TYPE_NOT_ASSIGNABLE_TO_TYPE,
                    );
                }
            }
        }
    }

    /// Recursively check for TS7006 in nested function/arrow expressions within a node.
    /// This handles cases like `async function foo(a = x => x)` where the nested arrow function
    /// parameter `x` should trigger TS7006 if it lacks a type annotation.
    pub(crate) fn check_for_nested_function_ts7006(&mut self, node_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        // Check if this is a function or arrow expression
        let is_function = match node.kind {
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => true,
            k if k == syntax_kind_ext::ARROW_FUNCTION => true,
            _ => false,
        };

        if is_function {
            // Check all parameters of this function for TS7006
            if let Some(func) = self.ctx.arena.get_function(node) {
                for &param_idx in &func.parameters.nodes {
                    if let Some(param_node) = self.ctx.arena.get(param_idx)
                        && let Some(param) = self.ctx.arena.get_parameter(param_node)
                    {
                        // Nested functions in default values don't have contextual types
                        self.maybe_report_implicit_any_parameter(param, false);
                    }
                }
            }

            // Recursively check the function body for more nested functions
            if let Some(func) = self.ctx.arena.get_function(node)
                && !func.body.is_none()
            {
                self.check_for_nested_function_ts7006(func.body);
            }
        } else {
            // Recursively check child nodes for function expressions
            match node.kind {
                // Binary expressions - check both sides
                k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                    if let Some(bin_expr) = self.ctx.arena.get_binary_expr(node) {
                        self.check_for_nested_function_ts7006(bin_expr.left);
                        self.check_for_nested_function_ts7006(bin_expr.right);
                    }
                }
                // Conditional expressions - check condition, then/else branches
                k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                    if let Some(cond) = self.ctx.arena.get_conditional_expr(node) {
                        self.check_for_nested_function_ts7006(cond.condition);
                        self.check_for_nested_function_ts7006(cond.when_true);
                        if !cond.when_false.is_none() {
                            self.check_for_nested_function_ts7006(cond.when_false);
                        }
                    }
                }
                // Call expressions - check arguments
                k if k == syntax_kind_ext::CALL_EXPRESSION => {
                    if let Some(call) = self.ctx.arena.get_call_expr(node) {
                        self.check_for_nested_function_ts7006(call.expression);
                        if let Some(args) = &call.arguments {
                            for &arg in &args.nodes {
                                self.check_for_nested_function_ts7006(arg);
                            }
                        }
                    }
                }
                // Parenthesized expression - check contents
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                        self.check_for_nested_function_ts7006(paren.expression);
                    }
                }
                // Type assertion - check expression
                k if k == syntax_kind_ext::TYPE_ASSERTION => {
                    if let Some(assertion) = self.ctx.arena.get_type_assertion(node) {
                        self.check_for_nested_function_ts7006(assertion.expression);
                    }
                }
                // Spread element - check expression
                k if k == syntax_kind_ext::SPREAD_ELEMENT => {
                    if let Some(spread) = self.ctx.arena.get_spread(node) {
                        self.check_for_nested_function_ts7006(spread.expression);
                    }
                }
                _ => {
                    // For other node types, we don't recursively check
                    // This covers literals, identifiers, array/object literals, etc.
                }
            }
        }
    }

    // Section 45: Symbol Resolution Utilities
    // ----------------------------------------

    /// Resolve a library type by name from lib.d.ts and other library contexts.
    ///
    /// This function resolves types from library definition files like lib.d.ts,
    /// es2015.d.ts, etc., which provide built-in JavaScript types and DOM APIs.
    ///
    /// ## Library Contexts:
    /// - Searches through loaded library contexts (lib.d.ts, es2015.d.ts, etc.)
    /// - Each lib context has its own binder and arena
    /// - Types are "lowered" from lib arena to main arena
    ///
    /// ## Declaration Merging:
    /// - Interfaces can have multiple declarations that are merged
    /// - All declarations are lowered together to create merged type
    /// - Essential for types like `Array` which have multiple lib declarations
    ///
    /// ## Global Augmentations:
    /// - User's `declare global` blocks are merged with lib types
    /// - Allows extending built-in types like `Window`, `String`, etc.
    ///
    /// ## Examples:
    /// ```typescript
    /// // Built-in types from lib.d.ts
    /// let arr: Array<number>;  // resolve_lib_type_by_name("Array")
    /// let obj: Object;         // resolve_lib_type_by_name("Object")
    /// let prom: Promise<string>; // resolve_lib_type_by_name("Promise")
    ///
    /// // Global augmentation
    /// declare global {
    ///   interface Window {
    ///     myCustomProperty: string;
    ///   }
    /// }
    /// // lib Window type is merged with augmentation
    /// ```
    pub(crate) fn resolve_lib_type_by_name(&mut self, name: &str) -> Option<TypeId> {
        use crate::parser::node::NodeAccess;
        use crate::solver::{TypeLowering, types::is_compiler_managed_type};

        let mut lib_type_id: Option<TypeId> = None;

        // Clone lib_contexts to allow access within the resolver closure
        let lib_contexts = self.ctx.lib_contexts.clone();

        // Collect lowered types from ALL lib contexts that define this symbol.
        // This is critical for interface merging across lib files - e.g. Array is
        // defined in lib.es5.d.ts and augmented in lib.es2015.core.d.ts with
        // methods like find(), findIndex(), etc.
        let mut lib_types: Vec<TypeId> = Vec::new();

        for lib_ctx in &lib_contexts {
            // Look up the symbol in this lib file's file_locals
            if let Some(sym_id) = lib_ctx.binder.file_locals.get(name) {
                // Get the symbol's declaration(s)
                if let Some(symbol) = lib_ctx.binder.get_symbol(sym_id) {
                    // Create a resolver that looks up symbols in all lib binders
                    let arena_ref = lib_ctx.arena.as_ref();
                    let resolver = |node_idx: NodeIndex| -> Option<u32> {
                        // Get the identifier name from the node
                        let ident_name = arena_ref.get_identifier_text(node_idx)?;

                        // Skip built-in types that have special handling in TypeLowering
                        if is_compiler_managed_type(ident_name) {
                            return None;
                        }

                        // Look up the symbol in all lib contexts' file_locals
                        for ctx in &lib_contexts {
                            if let Some(found_sym) = ctx.binder.file_locals.get(ident_name) {
                                return Some(found_sym.0);
                            }
                        }
                        None
                    };

                    // Lower the type from the lib file's arena with the resolver
                    let lowering = TypeLowering::with_resolver(
                        lib_ctx.arena.as_ref(),
                        self.ctx.types,
                        &resolver,
                    );
                    // For interfaces, use all declarations (handles declaration merging)
                    if !symbol.declarations.is_empty() {
                        lib_types.push(lowering.lower_interface_declarations(&symbol.declarations));
                        continue;
                    }
                    // For type aliases and other single-declaration types
                    let decl_idx = symbol.value_declaration;
                    if decl_idx.0 != u32::MAX {
                        lib_types.push(lowering.lower_type(decl_idx));
                        // Type aliases don't merge across files, take the first one
                        break;
                    }
                }
            }
        }

        // Merge all found types from different lib files using intersection
        if lib_types.len() == 1 {
            lib_type_id = Some(lib_types[0]);
        } else if lib_types.len() > 1 {
            let mut merged = lib_types[0];
            for &ty in &lib_types[1..] {
                merged = self.ctx.types.intersection2(merged, ty);
            }
            lib_type_id = Some(merged);
        }

        // Check for global augmentations in the current file that should merge with this type
        if let Some(augmentation_decls) = self.ctx.binder.global_augmentations.get(name)
            && !augmentation_decls.is_empty()
        {
            // Create a resolver for the current file's binder
            let arena_ref = self.ctx.arena;
            let binder_ref = self.ctx.binder;
            let resolver = |node_idx: NodeIndex| -> Option<u32> {
                // Get the identifier name from the node
                let ident_name = arena_ref.get_identifier_text(node_idx)?;

                // Skip built-in types that have special handling in TypeLowering
                if is_compiler_managed_type(ident_name) {
                    return None;
                }

                // First check the current file's locals
                if let Some(found_sym) = binder_ref.file_locals.get(ident_name) {
                    return Some(found_sym.0);
                }

                // Then check all lib contexts
                for ctx in &lib_contexts {
                    if let Some(found_sym) = ctx.binder.file_locals.get(ident_name) {
                        return Some(found_sym.0);
                    }
                }
                None
            };

            // Lower the augmentation declarations from the current file's arena with the resolver
            let lowering = TypeLowering::with_resolver(self.ctx.arena, self.ctx.types, &resolver);
            let augmentation_type = lowering.lower_interface_declarations(augmentation_decls);

            // Merge lib type with augmentation using intersection
            if let Some(lib_type) = lib_type_id {
                return Some(self.ctx.types.intersection2(lib_type, augmentation_type));
            } else {
                // No lib type found, just return the augmentation
                return Some(augmentation_type);
            }
        }

        lib_type_id
    }

    /// Resolve a lib type by name and also return its type parameters.
    /// Used by `register_boxed_types` for generic types like Array<T> to extract
    /// the actual type parameters from the interface definition rather than
    /// synthesizing fresh ones.
    pub(crate) fn resolve_lib_type_with_params(
        &mut self,
        name: &str,
    ) -> (Option<TypeId>, Vec<TypeParamInfo>) {
        use crate::parser::node::NodeAccess;
        use crate::solver::{TypeLowering, types::is_compiler_managed_type};

        let lib_contexts = self.ctx.lib_contexts.clone();

        let mut lib_types: Vec<TypeId> = Vec::new();
        let mut first_params: Option<Vec<TypeParamInfo>> = None;

        for lib_ctx in &lib_contexts {
            if let Some(sym_id) = lib_ctx.binder.file_locals.get(name) {
                if let Some(symbol) = lib_ctx.binder.get_symbol(sym_id) {
                    let arena_ref = lib_ctx.arena.as_ref();
                    let resolver = |node_idx: NodeIndex| -> Option<u32> {
                        let ident_name = arena_ref.get_identifier_text(node_idx)?;
                        if is_compiler_managed_type(ident_name) {
                            return None;
                        }
                        for ctx in &lib_contexts {
                            if let Some(found_sym) = ctx.binder.file_locals.get(ident_name) {
                                return Some(found_sym.0);
                            }
                        }
                        None
                    };

                    let lowering = TypeLowering::with_resolver(
                        lib_ctx.arena.as_ref(),
                        self.ctx.types,
                        &resolver,
                    );

                    if !symbol.declarations.is_empty() {
                        let (ty, params) =
                            lowering.lower_interface_declarations_with_params(&symbol.declarations);
                        lib_types.push(ty);
                        // Take type params from the first definition (primary interface)
                        if first_params.is_none() && !params.is_empty() {
                            first_params = Some(params);
                        }
                        continue;
                    }
                    let decl_idx = symbol.value_declaration;
                    if decl_idx.0 != u32::MAX {
                        lib_types.push(lowering.lower_type(decl_idx));
                        break;
                    }
                }
            }
        }

        let mut lib_type_id = if lib_types.len() == 1 {
            Some(lib_types[0])
        } else if lib_types.len() > 1 {
            let mut merged = lib_types[0];
            for &ty in &lib_types[1..] {
                merged = self.ctx.types.intersection2(merged, ty);
            }
            Some(merged)
        } else {
            None
        };

        // Merge global augmentations (same as resolve_lib_type_by_name)
        if let Some(augmentation_decls) = self.ctx.binder.global_augmentations.get(name)
            && !augmentation_decls.is_empty()
        {
            let arena_ref = self.ctx.arena;
            let binder_ref = self.ctx.binder;
            let resolver = |node_idx: NodeIndex| -> Option<u32> {
                let ident_name = arena_ref.get_identifier_text(node_idx)?;
                if is_compiler_managed_type(ident_name) {
                    return None;
                }
                if let Some(found_sym) = binder_ref.file_locals.get(ident_name) {
                    return Some(found_sym.0);
                }
                for ctx in &lib_contexts {
                    if let Some(found_sym) = ctx.binder.file_locals.get(ident_name) {
                        return Some(found_sym.0);
                    }
                }
                None
            };

            let lowering = TypeLowering::with_resolver(self.ctx.arena, self.ctx.types, &resolver);
            let augmentation_type = lowering.lower_interface_declarations(augmentation_decls);

            lib_type_id = if let Some(lib_type) = lib_type_id {
                Some(self.ctx.types.intersection2(lib_type, augmentation_type))
            } else {
                Some(augmentation_type)
            };
        }

        (lib_type_id, first_params.unwrap_or_default())
    }

    /// Resolve an alias symbol to its target symbol.
    ///
    /// This function follows alias chains to find the ultimate target symbol.
    /// Aliases are created by:
    /// - ES6 imports: `import { foo } from 'bar'`
    /// - Import equals: `import foo = require('bar')`
    /// - Re-exports: `export { foo } from 'bar'`
    ///
    /// ## Alias Resolution:
    /// - Follows re-export chains recursively
    /// - Uses binder's resolve_import_symbol for ES6 imports
    /// - Falls back to module_exports lookup
    /// - Handles circular references with visited_aliases tracking
    ///
    /// ## Re-export Chains:
    /// ```typescript
    /// // a.ts exports { x } from 'b.ts'
    /// // b.ts exports { x } from 'c.ts'
    /// // c.ts exports { x }
    /// // resolve_alias_symbol('x' in a.ts) → 'x' in c.ts
    /// ```
    ///
    /// ## Returns:
    /// - `Some(SymbolId)` - The resolved target symbol
    /// - `None` - If circular reference detected or resolution failed
    pub(crate) fn resolve_alias_symbol(
        &self,
        sym_id: crate::binder::SymbolId,
        visited_aliases: &mut Vec<crate::binder::SymbolId>,
    ) -> Option<crate::binder::SymbolId> {
        // Prevent stack overflow from long alias chains
        const MAX_ALIAS_RESOLUTION_DEPTH: usize = 128;
        if visited_aliases.len() >= MAX_ALIAS_RESOLUTION_DEPTH {
            return None;
        }

        // Use get_symbol_with_libs to properly handle symbols from lib files
        let lib_binders = self.get_lib_binders();
        let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)?;
        // Defensive: Verify symbol is valid before accessing fields
        // This prevents crashes when symbol IDs reference non-existent symbols
        if symbol.flags & symbol_flags::ALIAS == 0 {
            return Some(sym_id);
        }
        if visited_aliases.contains(&sym_id) {
            return None;
        }
        visited_aliases.push(sym_id);

        // First, try using the binder's resolve_import_symbol which follows re-export chains
        // This handles both named re-exports (`export { foo } from 'bar'`) and wildcard
        // re-exports (`export * from 'bar'`), properly following chains like:
        // a.ts exports { x } from 'b.ts'
        // b.ts exports { x } from 'c.ts'
        // c.ts exports { x }
        if let Some(resolved_sym_id) = self.ctx.binder.resolve_import_symbol(sym_id) {
            // Prevent infinite loops in re-export chains
            if !visited_aliases.contains(&resolved_sym_id) {
                return self.resolve_alias_symbol(resolved_sym_id, visited_aliases);
            }
        }

        // Fallback to direct module_exports lookup for backward compatibility
        // Handle ES6 imports: import { X } from 'module' or import X from 'module'
        // The binder sets import_module and import_name for these
        if let Some(ref module_name) = symbol.import_module {
            let export_name = symbol
                .import_name
                .as_deref()
                .unwrap_or(&symbol.escaped_name);
            // Look up the exported symbol in module_exports
            if let Some(exports) = self.ctx.binder.module_exports.get(module_name)
                && let Some(target_sym_id) = exports.get(export_name)
            {
                // Recursively resolve if the target is also an alias
                return self.resolve_alias_symbol(target_sym_id, visited_aliases);
            }
            // For ES6 imports, if we can't find the export, return the alias symbol itself
            // This allows the type checker to use the symbol reference
            return Some(sym_id);
        }

        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let decl_node = self.ctx.arena.get(decl_idx)?;
        if decl_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            let import = self.ctx.arena.get_import_decl(decl_node)?;
            // Track resolution depth to prevent stack overflow
            let depth = visited_aliases.len();
            if depth >= 128 {
                return None; // Prevent stack overflow
            }
            if let Some(target) =
                self.resolve_qualified_symbol_inner(import.module_specifier, visited_aliases, depth)
            {
                return Some(target);
            }
            return self
                .resolve_require_call_symbol(import.module_specifier, Some(visited_aliases));
        }
        // For other alias symbols (not ES6 imports or import equals), return None
        // to indicate we couldn't resolve the alias
        None
    }

    /// Get the text representation of a heritage clause name.
    ///
    /// Heritage clauses appear in class declarations as `extends` and `implements` clauses.
    /// This function extracts the name text from various heritage clause node types.
    ///
    /// ## Heritage Clause Types:
    /// - Simple identifier: `extends Foo` → "Foo"
    /// - Qualified name: `extends ns.Foo` → "ns.Foo"
    /// - Property access: `extends ns.Foo` → "ns.Foo"
    /// - Keyword literals: `extends null`, `extends true` → "null", "true"
    ///
    /// ## Examples:
    /// ```typescript
    /// class Foo extends Bar {} // "Bar"
    /// class Foo extends ns.Bar {} // "ns.Bar"
    /// class Foo implements IFoo {} // "IFoo"
    /// ```
    pub(crate) fn heritage_name_text(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.clone());
        }

        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            return self.entity_name_text(idx);
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.ctx.arena.get_access_expr(node)?;
            let left = self.heritage_name_text(access.expression)?;
            let right = self
                .ctx
                .arena
                .get(access.name_or_argument)
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                .map(|ident| ident.escaped_text.clone())?;
            let mut combined = String::with_capacity(left.len() + 1 + right.len());
            combined.push_str(&left);
            combined.push('.');
            combined.push_str(&right);
            return Some(combined);
        }

        // Handle keyword literals in heritage clauses (e.g., extends null, extends true)
        match node.kind {
            k if k == SyntaxKind::NullKeyword as u16 => return Some("null".to_string()),
            k if k == SyntaxKind::TrueKeyword as u16 => return Some("true".to_string()),
            k if k == SyntaxKind::FalseKeyword as u16 => return Some("false".to_string()),
            k if k == SyntaxKind::UndefinedKeyword as u16 => return Some("undefined".to_string()),
            k if k == SyntaxKind::NumericLiteral as u16 => return Some("0".to_string()),
            k if k == SyntaxKind::StringLiteral as u16 => return Some("0".to_string()),
            _ => {}
        }

        None
    }

    // Section 46: Namespace Type Utilities
    // -------------------------------------

    /// Resolve a namespace value member by name.
    ///
    /// This function resolves value members of namespace/enum types.
    /// It handles both namespace exports and enum members.
    ///
    /// ## Namespace Members:
    /// - Resolves exported members of namespace types
    /// - Filters out type-only members (no value flag)
    /// - Returns the type of the member symbol
    ///
    /// ## Enum Members:
    /// - Resolves enum members by name
    /// - Returns the member's literal type
    ///
    /// ## Examples:
    /// ```typescript
    /// namespace Utils {
    ///   export function helper(): void {}
    ///   export type Helper = number;
    /// }
    /// const x = Utils.helper; // resolve_namespace_value_member(Utils, "helper")
    /// // x has type () => void
    ///
    /// enum Color {
    ///   Red,
    ///   Green,
    /// }
    /// const c = Color.Red; // resolve_namespace_value_member(Color, "Red")
    /// // c has type Color.Red
    /// ```
    pub(crate) fn resolve_namespace_value_member(
        &mut self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        use crate::solver::type_queries::{NamespaceMemberKind, classify_namespace_member};

        match classify_namespace_member(self.ctx.types, object_type) {
            // Handle Ref types (direct namespace/module references)
            NamespaceMemberKind::SymbolRef(sym_ref) => {
                let sym_id = sym_ref.0;
                let symbol = self.ctx.binder.get_symbol(SymbolId(sym_id))?;
                if symbol.flags & (symbol_flags::MODULE | symbol_flags::ENUM) == 0 {
                    return None;
                }

                // Check direct exports first
                if let Some(exports) = symbol.exports.as_ref()
                    && let Some(member_id) = exports.get(property_name)
                {
                    // Follow re-export chains to get the actual symbol
                    let resolved_member_id = if let Some(member_symbol) =
                        self.ctx.binder.get_symbol(member_id)
                        && member_symbol.flags & symbol_flags::ALIAS != 0
                    {
                        let mut visited_aliases = Vec::new();
                        self.resolve_alias_symbol(member_id, &mut visited_aliases)
                            .unwrap_or(member_id)
                    } else {
                        member_id
                    };

                    if let Some(member_symbol) = self.ctx.binder.get_symbol(resolved_member_id)
                        && member_symbol.flags & symbol_flags::VALUE == 0
                        && member_symbol.flags & symbol_flags::ALIAS == 0
                    {
                        return None;
                    }
                    return Some(self.get_type_of_symbol(resolved_member_id));
                }

                // Check for re-exports from other modules
                // This handles cases like: export { foo } from './bar'
                if let Some(ref module_specifier) = symbol.import_module {
                    let mut visited_aliases = Vec::new();
                    if let Some(reexported_sym) = self.resolve_reexported_member_symbol(
                        module_specifier,
                        property_name,
                        &mut visited_aliases,
                    ) {
                        if let Some(member_symbol) = self.ctx.binder.get_symbol(reexported_sym)
                            && member_symbol.flags & symbol_flags::VALUE == 0
                            && member_symbol.flags & symbol_flags::ALIAS == 0
                        {
                            return None;
                        }
                        return Some(self.get_type_of_symbol(reexported_sym));
                    }
                }

                if symbol.flags & symbol_flags::ENUM != 0
                    && let Some(member_type) =
                        self.enum_member_type_for_name(SymbolId(sym_id), property_name)
                {
                    return Some(member_type);
                }

                None
            }

            // Handle Callable types from merged class+namespace or function+namespace symbols
            // When a class/function merges with a namespace, the type is a Callable with
            // properties containing the namespace exports
            NamespaceMemberKind::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);

                // Check if the callable has the property as a member (from namespace merge)
                for prop in &shape.properties {
                    let prop_name = self.ctx.types.resolve_atom_ref(prop.name);
                    if prop_name.as_ref() == property_name {
                        return Some(prop.type_id);
                    }
                }

                None
            }

            NamespaceMemberKind::Other => None,
        }
    }

    /// Check if a namespace has a type-only member.
    ///
    /// This function determines if a specific property of a namespace
    /// is type-only (has TYPE flag but not VALUE flag).
    ///
    /// ## Type-Only Members:
    /// - Interface declarations: `export interface Foo {}`
    /// - Type alias declarations: `export type Bar = number;`
    /// - Class declarations (when used as types): `export class Baz {}`
    ///
    /// ## Value Members:
    /// - Function declarations: `export function foo() {}`
    /// - Variable declarations: `export const x = 1;`
    /// - Enum declarations: `export enum E {}`
    ///
    /// ## Examples:
    /// ```typescript
    /// namespace Types {
    ///   export interface Foo {} // type-only
    ///   export type Bar = number; // type-only
    ///   export function helper() {} // value member
    /// }
    /// // namespace_has_type_only_member(Types, "Foo") → true
    /// // namespace_has_type_only_member(Types, "helper") → false
    /// ```
    pub(crate) fn namespace_has_type_only_member(
        &self,
        object_type: TypeId,
        property_name: &str,
    ) -> bool {
        use crate::solver::type_queries::{NamespaceMemberKind, classify_namespace_member};

        match classify_namespace_member(self.ctx.types, object_type) {
            // Handle Ref types (direct namespace/module references)
            NamespaceMemberKind::SymbolRef(sym_ref) => {
                let sym_id = sym_ref.0;
                let symbol = match self.ctx.binder.get_symbol(SymbolId(sym_id)) {
                    Some(symbol) => symbol,
                    None => return false,
                };

                if symbol.flags & symbol_flags::MODULE == 0 {
                    return false;
                }

                let exports = match symbol.exports.as_ref() {
                    Some(exports) => exports,
                    None => return false,
                };

                let member_id = match exports.get(property_name) {
                    Some(member_id) => member_id,
                    None => return false,
                };

                // Follow alias chains to determine if the ultimate target is type-only
                let resolved_member_id = if let Some(member_symbol) =
                    self.ctx.binder.get_symbol(member_id)
                    && member_symbol.flags & symbol_flags::ALIAS != 0
                {
                    let mut visited_aliases = Vec::new();
                    self.resolve_alias_symbol(member_id, &mut visited_aliases)
                        .unwrap_or(member_id)
                } else {
                    member_id
                };

                let member_symbol = match self.ctx.binder.get_symbol(resolved_member_id) {
                    Some(member_symbol) => member_symbol,
                    None => return false,
                };

                let has_value =
                    (member_symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS)) != 0;
                let has_type = (member_symbol.flags & symbol_flags::TYPE) != 0;
                has_type && !has_value
            }

            // Handle Callable types from merged class+namespace or function+namespace symbols
            // For merged symbols, the namespace exports are stored as properties on the Callable
            NamespaceMemberKind::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);

                // Check if the property exists in the callable's properties
                for prop in &shape.properties {
                    let prop_name = self.ctx.types.resolve_atom_ref(prop.name);
                    if prop_name.as_ref() == property_name {
                        // Found the property - now check if it's type-only
                        // For merged symbols, properties from namespace exports should have value members
                        // We need to look at the type to determine if it's type-only
                        return self.is_type_only_type(prop.type_id);
                    }
                }

                false
            }

            NamespaceMemberKind::Other => false,
        }
    }

    /// Check if an alias symbol resolves to a type-only symbol.
    ///
    /// This function follows alias chains to determine if the ultimate
    /// target is type-only (has TYPE flag but not VALUE flag).
    ///
    /// ## Type-Only Imports:
    /// - `import type { Foo } from 'module'` - Foo is type-only
    /// - `import type { Bar } from './types'` - Bar is type-only
    ///
    /// ## Alias Resolution:
    /// - Follows re-export chains
    /// - Checks the ultimate target's flags
    /// - Respects `is_type_only` flag on alias symbols
    ///
    /// ## Examples:
    /// ```typescript
    /// // types.ts
    /// export interface Foo {}
    /// export const bar: number = 42;
    ///
    /// // main.ts
    /// import type { Foo } from './types'; // type-only import
    /// import { bar } from './types'; // value import
    ///
    /// // alias_resolves_to_type_only(Foo) → true
    /// // alias_resolves_to_type_only(bar) → false
    /// ```
    pub(crate) fn alias_resolves_to_type_only(&self, sym_id: SymbolId) -> bool {
        let symbol = match self.ctx.binder.get_symbol(sym_id) {
            Some(symbol) => symbol,
            None => return false,
        };

        if symbol.flags & symbol_flags::ALIAS == 0 {
            return false;
        }
        if symbol.is_type_only {
            return true;
        }

        let mut visited = Vec::new();
        let target = match self.resolve_alias_symbol(sym_id, &mut visited) {
            Some(target) => target,
            None => return false,
        };

        let target_symbol = match self.ctx.binder.get_symbol(target) {
            Some(target_symbol) => target_symbol,
            None => return false,
        };

        let has_value = (target_symbol.flags & symbol_flags::VALUE) != 0;
        let has_type = (target_symbol.flags & symbol_flags::TYPE) != 0;
        has_type && !has_value
    }

    /// Check if a type is type-only (has no runtime value).
    ///
    /// This is used for merged class+namespace symbols where namespace exports
    /// are stored as properties on the Callable type.
    fn is_type_only_type(&self, type_id: TypeId) -> bool {
        use crate::solver::type_queries;

        // Check if this is a Ref to a type-only symbol
        if let Some(sym_ref) = type_queries::get_ref_symbol(self.ctx.types, type_id) {
            if let Some(symbol) = self.ctx.binder.get_symbol(SymbolId(sym_ref.0)) {
                let has_value = (symbol.flags & symbol_flags::VALUE) != 0;
                let has_type = (symbol.flags & symbol_flags::TYPE) != 0;
                return has_type && !has_value;
            }
        }

        false
    }

    /// Get the namespace name if the type is a namespace/module type.
    ///
    /// This function checks if a type is a reference to a namespace or module
    /// and returns the namespace name if so.
    ///
    /// ## Returns:
    /// - `Some(name)` if the type is a namespace/module reference
    /// - `None` if the type is not a namespace/module
    ///
    /// ## Examples:
    /// ```typescript
    /// namespace NS { export const x = 1; }
    /// // get_namespace_name(typeof NS) → Some("NS")
    ///
    /// const obj = { x: 1 };
    /// // get_namespace_name(typeof obj) → None
    /// ```
    pub(crate) fn get_namespace_name(&self, type_id: TypeId) -> Option<String> {
        use crate::solver::type_queries;

        if let Some(sym_ref) = type_queries::get_ref_symbol(self.ctx.types, type_id) {
            if let Some(symbol) = self.ctx.binder.get_symbol(SymbolId(sym_ref.0)) {
                // Check if this is a namespace/module symbol
                if symbol.flags & (symbol_flags::MODULE | symbol_flags::NAMESPACE) != 0 {
                    return Some(symbol.escaped_name.clone());
                }
            }
        }

        None
    }

    /// Check if a symbol is type-only (from `import type`).
    ///
    /// This is used to allow type-only imports in type positions while
    /// preventing their use in value positions.
    ///
    /// ## Import Type Statement:
    /// - `import type { Foo } from 'module'` - Foo.is_type_only = true
    /// - Type-only imports can only be used in type annotations
    /// - Cannot be used as values (variables, function arguments, etc.)
    ///
    /// ## Examples:
    /// ```typescript
    /// import type { Foo } from './types'; // type-only import
    /// import { Bar } from './types'; // regular import
    ///
    /// const x: Foo = ...; // OK - Foo used in type position
    /// const y = Foo; // ERROR - Foo cannot be used as value
    ///
    /// const z: Bar = ...; // OK - Bar has both type and value
    /// const w = Bar; // OK - Bar can be used as value
    /// ```
    pub(crate) fn symbol_is_type_only(&self, sym_id: SymbolId, name_hint: Option<&str>) -> bool {
        self.lookup_symbol_with_name(sym_id, name_hint)
            .map(|(symbol, _arena)| symbol.is_type_only)
            .unwrap_or(false)
    }

    // Section 47: Node Predicate Utilities
    // ------------------------------------

    /// Check if a variable declaration is a catch clause variable.
    ///
    /// This function determines if a given variable declaration node is
    /// the variable declaration of a catch clause (try/catch statement).
    ///
    /// ## Catch Clause Variables:
    /// - Catch clause variables have special scoping rules
    /// - They are block-scoped to the catch block
    /// - They shadow variables with the same name in outer scopes
    /// - They cannot be accessed before declaration (TDZ applies)
    ///
    /// ## Examples:
    /// ```typescript
    /// try {
    ///   throw new Error("error");
    /// } catch (e) {
    ///   // e is a catch clause variable
    ///   console.log(e.message);
    /// }
    /// // is_catch_clause_variable_declaration(e_node) → true
    ///
    /// const x = 5;
    /// // is_catch_clause_variable_declaration(x_node) → false
    /// ```
    pub(crate) fn is_catch_clause_variable_declaration(&self, var_decl_idx: NodeIndex) -> bool {
        let Some(ext) = self.ctx.arena.get_extended(var_decl_idx) else {
            return false;
        };
        let parent_idx = ext.parent;
        if parent_idx.is_none() {
            return false;
        }
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        if parent_node.kind != syntax_kind_ext::CATCH_CLAUSE {
            return false;
        }
        let Some(catch) = self.ctx.arena.get_catch_clause(parent_node) else {
            return false;
        };
        catch.variable_declaration == var_decl_idx
    }

    // Section 48: Type Predicate Utilities
    // -------------------------------------

    /// Get the target of a type predicate from a parameter name node.
    ///
    /// Type predicates are used in function signatures to narrow types
    /// based on runtime checks. The target can be either `this` or an
    /// identifier parameter name.
    ///
    /// ## Type Predicate Targets:
    /// - **This**: `asserts this is T` - Used in methods to narrow the receiver type
    /// - **Identifier**: `argName is T` - Used to narrow a parameter's type
    ///
    /// ## Examples:
    /// ```typescript
    /// // This type predicate
    /// function assertIsString(this: unknown): asserts this is string {
    ///   if (typeof this === 'string') {
    ///     return; // this is narrowed to string
    ///   }
    ///   throw new Error('Not a string');
    /// }
    /// // type_predicate_target(thisKeywordNode) → TypePredicateTarget::This
    ///
    /// // Identifier type predicate
    /// function isString(val: unknown): val is string {
    ///   return typeof val === 'string';
    /// }
    /// // type_predicate_target(valIdentifierNode) → TypePredicateTarget::Identifier("val")
    /// ```
    pub(crate) fn type_predicate_target(
        &self,
        param_name: NodeIndex,
    ) -> Option<TypePredicateTarget> {
        let node = self.ctx.arena.get(param_name)?;
        if node.kind == SyntaxKind::ThisKeyword as u16 || node.kind == syntax_kind_ext::THIS_TYPE {
            return Some(TypePredicateTarget::This);
        }

        self.ctx.arena.get_identifier(node).map(|ident| {
            TypePredicateTarget::Identifier(self.ctx.types.intern_string(&ident.escaped_text))
        })
    }

    // Section 49: Constructor Accessibility Utilities
    // -----------------------------------------------

    /// Convert a constructor access level to its string representation.
    ///
    /// This function is used for error messages to display the accessibility
    /// level of a constructor (private, protected, or public).
    ///
    /// ## Constructor Accessibility:
    /// - **Private**: `private constructor()` - Only accessible within the class
    /// - **Protected**: `protected constructor()` - Accessible within class and subclasses
    /// - **Public**: `constructor()` or `public constructor()` - Accessible everywhere
    ///
    /// ## Examples:
    /// ```typescript
    /// class Singleton {
    ///   private constructor() {} // Only accessible within Singleton
    /// }
    /// // constructor_access_name(Some(Private)) → "private"
    ///
    /// class Base {
    ///   protected constructor() {} // Accessible in Base and subclasses
    /// }
    /// // constructor_access_name(Some(Protected)) → "protected"
    ///
    /// class Public {
    ///   constructor() {} // Public by default
    /// }
    /// // constructor_access_name(None) → "public"
    /// ```
    pub(crate) fn constructor_access_name(level: Option<MemberAccessLevel>) -> &'static str {
        match level {
            Some(MemberAccessLevel::Private) => "private",
            Some(MemberAccessLevel::Protected) => "protected",
            None => "public",
        }
    }

    /// Get the numeric rank of a constructor access level.
    ///
    /// This function assigns a numeric value to access levels for comparison:
    /// - Private (2) > Protected (1) > Public (0)
    ///
    /// Higher ranks indicate more restrictive access levels. This is used
    /// to determine if a constructor accessibility mismatch exists between
    /// source and target types.
    ///
    /// ## Rank Ordering:
    /// ```typescript
    /// Private (2)   - Most restrictive
    /// Protected (1) - Medium restrictiveness
    /// Public (0)    - Least restrictive
    /// ```
    ///
    /// ## Examples:
    /// ```typescript
    /// constructor_access_rank(Some(Private))    // → 2
    /// constructor_access_rank(Some(Protected)) // → 1
    /// constructor_access_rank(None)            // → 0 (Public)
    /// ```
    pub(crate) fn constructor_access_rank(level: Option<MemberAccessLevel>) -> u8 {
        match level {
            Some(MemberAccessLevel::Private) => 2,
            Some(MemberAccessLevel::Protected) => 1,
            None => 0,
        }
    }

    /// Get the excluded symbol flags for a given symbol.
    ///
    /// Each symbol type (function, class, interface, etc.) has specific
    /// flags that represent incompatible symbols that cannot share the same name.
    /// This function returns those exclusion flags.
    ///
    /// ## Symbol Exclusion Rules:
    /// - Functions exclude other functions with the same name
    /// - Classes exclude interfaces with the same name (unless merging)
    /// - Variables exclude other variables with the same name in the same scope
    ///
    /// ## Examples:
    /// ```typescript
    /// // Function exclusions
    /// function foo() {}
    /// function foo() {} // ERROR: Duplicate function declaration
    ///
    /// // Class/Interface merging (allowed)
    /// interface Foo {}
    /// class Foo {} // Allowed: interface and class can merge
    ///
    /// // Variable exclusions
    /// let x = 1;
    /// let x = 2; // ERROR: Duplicate variable declaration
    /// ```
    fn excluded_symbol_flags(flags: u32) -> u32 {
        if (flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0 {
            return symbol_flags::FUNCTION_SCOPED_VARIABLE_EXCLUDES;
        }
        if (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0 {
            return symbol_flags::BLOCK_SCOPED_VARIABLE_EXCLUDES;
        }
        if (flags & symbol_flags::FUNCTION) != 0 {
            return symbol_flags::FUNCTION_EXCLUDES;
        }
        if (flags & symbol_flags::CLASS) != 0 {
            return symbol_flags::CLASS_EXCLUDES;
        }
        if (flags & symbol_flags::INTERFACE) != 0 {
            return symbol_flags::INTERFACE_EXCLUDES;
        }
        if (flags & symbol_flags::TYPE_ALIAS) != 0 {
            return symbol_flags::TYPE_ALIAS_EXCLUDES;
        }
        if (flags & symbol_flags::REGULAR_ENUM) != 0 {
            return symbol_flags::REGULAR_ENUM_EXCLUDES;
        }
        if (flags & symbol_flags::CONST_ENUM) != 0 {
            return symbol_flags::CONST_ENUM_EXCLUDES;
        }
        // Check NAMESPACE_MODULE before VALUE_MODULE since namespaces have both flags
        // and NAMESPACE_MODULE_EXCLUDES (NONE) allows more merging than VALUE_MODULE_EXCLUDES
        if (flags & symbol_flags::NAMESPACE_MODULE) != 0 {
            return symbol_flags::NAMESPACE_MODULE_EXCLUDES;
        }
        if (flags & symbol_flags::VALUE_MODULE) != 0 {
            return symbol_flags::VALUE_MODULE_EXCLUDES;
        }
        if (flags & symbol_flags::GET_ACCESSOR) != 0 {
            return symbol_flags::GET_ACCESSOR_EXCLUDES;
        }
        if (flags & symbol_flags::SET_ACCESSOR) != 0 {
            return symbol_flags::SET_ACCESSOR_EXCLUDES;
        }
        if (flags & symbol_flags::METHOD) != 0 {
            return symbol_flags::METHOD_EXCLUDES;
        }
        symbol_flags::NONE
    }

    /// Check if two declarations conflict based on their symbol flags.
    ///
    /// This function determines whether two symbols with the given flags
    /// can coexist in the same scope without conflict.
    ///
    /// ## Conflict Rules:
    /// - **Static vs Instance**: Static and instance members with the same name don't conflict
    /// - **Exclusion Flags**: If either declaration excludes the other's flags, they conflict
    ///
    /// ## Examples:
    /// ```typescript
    /// class Example {
    ///   static x = 1;  // Static member
    ///   x = 2;         // Instance member - no conflict
    /// }
    ///
    /// class Conflict {
    ///   foo() {}      // Method
    ///   foo: number;  // Property - CONFLICT!
    /// }
    ///
    /// interface Merge {
    ///   foo(): void;
    /// }
    /// interface Merge {
    ///   bar(): void;  // No conflict - different members
    /// }
    /// ```
    pub(crate) fn declarations_conflict(flags_a: u32, flags_b: u32) -> bool {
        // Static and instance members with the same name don't conflict
        let a_is_static = (flags_a & symbol_flags::STATIC) != 0;
        let b_is_static = (flags_b & symbol_flags::STATIC) != 0;
        if a_is_static != b_is_static {
            return false;
        }

        let excludes_a = Self::excluded_symbol_flags(flags_a);
        let excludes_b = Self::excluded_symbol_flags(flags_b);
        (flags_a & excludes_b) != 0 || (flags_b & excludes_a) != 0
    }

    // Section 51: Literal Type Utilities
    // ----------------------------------

    /// Infer a literal type from an initializer expression.
    ///
    /// This function attempts to infer the most specific literal type from an
    /// expression, enabling const declarations to have literal types.
    ///
    /// **Literal Type Inference:**
    /// - **String literals**: `"hello"` → `"hello"` (string literal type)
    /// - **Numeric literals**: `42` → `42` (numeric literal type)
    /// - **Boolean literals**: `true` → `true`, `false` → `false`
    /// - **Null literal**: `null` → null type
    /// - **Unary expressions**: `-42` → `-42`, `+42` → `42`
    ///
    /// **Non-Literal Expressions:**
    /// - Complex expressions return None (not a literal)
    /// - Function calls, object literals, etc. return None
    ///
    /// **Const Declarations:**
    /// - `const x = "hello"` infers type `"hello"` (not `string`)
    /// - `let y = "hello"` infers type `string` (widened)
    /// - This function enables the const behavior
    ///
    /// ## Examples:
    /// ```typescript
    /// // String literal
    /// const greeting = "hello";  // Type: "hello"
    /// literal_type_from_initializer(greeting_node) → Some("hello")
    ///
    /// // Numeric literal
    /// const count = 42;  // Type: 42
    /// literal_type_from_initializer(count_node) → Some(42)
    ///
    /// // Negative number
    /// const temp = -42;  // Type: -42
    /// literal_type_from_initializer(temp_node) → Some(-42)
    ///
    /// // Boolean
    /// const flag = true;  // Type: true
    /// literal_type_from_initializer(flag_node) → Some(true)
    ///
    /// // Non-literal
    /// const arr = [1, 2, 3];  // Type: number[]
    /// literal_type_from_initializer(arr_node) → None
    /// ```
    pub(crate) fn literal_type_from_initializer(&self, idx: NodeIndex) -> Option<TypeId> {
        let Some(node) = self.ctx.arena.get(idx) else {
            return None;
        };

        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                let lit = self.ctx.arena.get_literal(node)?;
                Some(self.ctx.types.literal_string(&lit.text))
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.ctx.arena.get_literal(node)?;
                lit.value.map(|value| self.ctx.types.literal_number(value))
            }
            k if k == SyntaxKind::TrueKeyword as u16 => Some(self.ctx.types.literal_boolean(true)),
            k if k == SyntaxKind::FalseKeyword as u16 => {
                Some(self.ctx.types.literal_boolean(false))
            }
            k if k == SyntaxKind::NullKeyword as u16 => Some(TypeId::NULL),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.ctx.arena.get_unary_expr(node)?;
                let op = unary.operator;
                if op != SyntaxKind::MinusToken as u16 && op != SyntaxKind::PlusToken as u16 {
                    return None;
                }
                let operand = unary.operand;
                let Some(operand_node) = self.ctx.arena.get(operand) else {
                    return None;
                };
                if operand_node.kind != SyntaxKind::NumericLiteral as u16 {
                    return None;
                }
                let lit = self.ctx.arena.get_literal(operand_node)?;
                let value = lit.value?;
                let value = if op == SyntaxKind::MinusToken as u16 {
                    -value
                } else {
                    value
                };
                Some(self.ctx.types.literal_number(value))
            }
            _ => None,
        }
    }
}
