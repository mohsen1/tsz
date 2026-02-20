//! Modifier, member access, and query methods for `CheckerState`.

use crate::state::{CheckerState, MemberAccessLevel};
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// Result of tsc's `getSyntacticTruthySemantics` — purely syntactic truthiness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyntacticTruthiness {
    AlwaysTruthy,
    AlwaysFalsy,
    Sometimes,
}

impl SyntacticTruthiness {
    /// Combine two branches (e.g., both arms of a conditional expression).
    fn combine(self, other: Self) -> Self {
        if self == other { self } else { Self::Sometimes }
    }
}

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Section 27: Modifier and Member Access Utilities
    // =========================================================================

    /// Check if the current file is a JavaScript file (.js, .jsx, .mjs, .cjs).
    /// Used for `TS8xxx` JS grammar checks.
    pub(crate) fn is_js_file(&self) -> bool {
        self.ctx.file_name.ends_with(".js")
            || self.ctx.file_name.ends_with(".jsx")
            || self.ctx.file_name.ends_with(".mjs")
            || self.ctx.file_name.ends_with(".cjs")
    }

    /// Check if a node has the `declare` modifier.
    pub(crate) fn has_declare_modifier(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> bool {
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

    /// Find the `declare` modifier `NodeIndex` in a modifier list, if present.
    /// Used to point error messages at the specific modifier.
    pub(crate) fn get_declare_modifier(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> Option<NodeIndex> {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx)
                    && mod_node.kind == SyntaxKind::DeclareKeyword as u16
                {
                    return Some(mod_idx);
                }
            }
        }
        None
    }

    /// Check if a node has the `async` modifier.
    pub(crate) fn has_async_modifier(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> bool {
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

    /// Find the `async` modifier `NodeIndex` in a modifier list, if present.
    pub(crate) fn find_async_modifier(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
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
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> bool {
        self.has_modifier_kind(modifiers, SyntaxKind::AbstractKeyword)
    }

    /// Check if modifiers include the 'static' keyword.
    pub(crate) fn has_static_modifier(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> bool {
        self.has_modifier_kind(modifiers, SyntaxKind::StaticKeyword)
    }

    /// Check if modifiers include the 'accessor' keyword (auto-accessor).
    pub(crate) fn has_accessor_modifier(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> bool {
        self.has_modifier_kind(modifiers, SyntaxKind::AccessorKeyword)
    }

    /// Check if modifiers include the 'override' keyword.
    pub(crate) fn has_override_modifier(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> bool {
        self.has_modifier_kind(modifiers, SyntaxKind::OverrideKeyword)
    }

    /// Check if modifiers include the 'private' keyword.
    pub(crate) fn has_private_modifier(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> bool {
        self.has_modifier_kind(modifiers, SyntaxKind::PrivateKeyword)
    }

    /// Check if modifiers include the 'protected' keyword.
    pub(crate) fn has_protected_modifier(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> bool {
        self.has_modifier_kind(modifiers, SyntaxKind::ProtectedKeyword)
    }

    /// Check if modifiers include the 'readonly' keyword.
    pub(crate) fn has_readonly_modifier(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> bool {
        self.has_modifier_kind(modifiers, SyntaxKind::ReadonlyKeyword)
    }

    /// Check if modifiers include a parameter property keyword.
    pub(crate) fn has_parameter_property_modifier(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
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
        modifiers: &Option<tsz_parser::parser::NodeList>,
        name_idx: NodeIndex,
    ) -> bool {
        self.has_private_modifier(modifiers)
            || self.has_protected_modifier(modifiers)
            || self.is_private_identifier_name(name_idx)
    }

    /// Get the visibility from modifiers list.
    /// Returns Private, Protected, or Public (default).
    pub(crate) fn get_visibility_from_modifiers(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> tsz_solver::Visibility {
        use tsz_solver::Visibility;

        if self.has_private_modifier(modifiers) {
            Visibility::Private
        } else if self.has_protected_modifier(modifiers) {
            Visibility::Protected
        } else {
            Visibility::Public
        }
    }

    /// Get the access level from modifiers (private/protected).
    pub(crate) fn member_access_level_from_modifiers(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
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
    /// Only checks properties (not methods) because accessing `this.abstractMethod()` in constructor is allowed.
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
            .map_or(NodeIndex::NONE, |ext| ext.parent);
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
            .map_or(NodeIndex::NONE, |ext| ext.parent);
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
        {
            // Canonicalize numeric property names (e.g. "1.", "1.0" -> "1")
            if name_node.kind == SyntaxKind::NumericLiteral as u16
                && let Some(canonical) = tsz_solver::utils::canonicalize_numeric_name(&lit.text)
            {
                return Some(canonical);
            }
            return Some(lit.text.clone());
        }

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.ctx.arena.get_computed_property(name_node)
        {
            if let Some(symbol_name) = self.get_symbol_property_name_from_expr(computed.expression)
            {
                return Some(symbol_name);
            }
            if let Some(expr_node) = self.ctx.arena.get(computed.expression) {
                // NOTE: Do NOT return the identifier text for computed property names
                // like `[e]`. The identifier `e` must go through the computed property
                // checking path so its expression is type-checked (emitting TS2304 if
                // undeclared). Returning the identifier text here would skip that check
                // and produce the wrong property name (the variable name instead of its
                // value). Only statically-known values (string/number literals, unique
                // symbols) can be resolved here.

                if matches!(
                    expr_node.kind,
                    k if k == SyntaxKind::StringLiteral as u16
                        || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                        || k == SyntaxKind::NumericLiteral as u16
                ) && let Some(lit) = self.ctx.arena.get_literal(expr_node)
                {
                    if expr_node.kind == SyntaxKind::NumericLiteral as u16
                        && let Some(canonical) =
                            tsz_solver::utils::canonicalize_numeric_name(&lit.text)
                    {
                        return Some(canonical);
                    }
                    return Some(lit.text.clone());
                }
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
        let node = self.ctx.arena.get(member_idx)?;

        // Use helper to get name node, then get property name text
        let name_idx = self.get_member_name_node(node)?;
        self.get_property_name(name_idx)
    }

    /// Get the name of a function declaration.
    pub(crate) fn get_function_name_from_node(&self, stmt_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(stmt_idx)?;

        if let Some(func) = self.ctx.arena.get_function(node)
            && !func.name.is_none()
        {
            let name_node = self.ctx.arena.get(func.name)?;
            if let Some(id) = self.ctx.arena.get_identifier(name_node) {
                return Some(id.escaped_text.clone());
            }
        }

        None
    }

    /// Get the name of a parameter from its binding name node.
    /// Returns None for destructuring patterns.
    pub(crate) fn get_parameter_name(&self, name_idx: NodeIndex) -> Option<String> {
        let ident = self.ctx.arena.get_identifier_at(name_idx)?;
        Some(ident.escaped_text.clone())
    }

    // =========================================================================
    // Section 31: Class Hierarchy Utilities
    // =========================================================================

    /// Get the base class node index from a class declaration.
    /// Returns None if the class doesn't extend anything.
    pub(crate) fn get_base_class_idx(&self, class_idx: NodeIndex) -> Option<NodeIndex> {
        let class = self.ctx.arena.get_class_at(class_idx)?;
        let heritage_clauses = class.heritage_clauses.as_ref()?;

        for &clause_idx in &heritage_clauses.nodes {
            let heritage = self.ctx.arena.get_heritage_clause_at(clause_idx)?;
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let &type_idx = heritage.types.nodes.first()?;
            let expr_idx =
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args_at(type_idx) {
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
    /// Traverses the inheritance chain to check if `base_idx` is an ancestor of `derived_idx`.
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
    pub(crate) fn is_dynamic_import(&self, call: &tsz_parser::parser::node::CallExprData) -> bool {
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
        let node = self.ctx.arena.get(idx)?;

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
        let node = self.ctx.arena.get(idx)?;

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
        // TypeScript only treats canonical unsigned integer strings as numeric indexes.
        // Examples: "0", "1", "42"
        // Non-canonical forms like "0.0", "01", "+1" are string property names.
        if value.is_empty() {
            return None;
        }
        if value != "0" && value.starts_with('0') {
            return None;
        }
        if !value.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        value.parse::<usize>().ok()
    }

    // =========================================================================
    // Section 35: Symbol and Declaration Utilities
    // =========================================================================

    /// Get the class declaration node from a symbol.
    /// Returns None if the symbol doesn't represent a class.
    pub(crate) fn get_class_declaration_from_symbol(
        &self,
        sym_id: tsz_binder::SymbolId,
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
    /// - Cycle detection via `FxHashSet`
    /// - Max depth protection (20 levels)
    /// - Comprehensive type traversal including function parameters
    pub(crate) fn type_contains_error(&self, type_id: TypeId) -> bool {
        tsz_solver::contains_error_type(self.ctx.types, type_id)
    }

    // =========================================================================
    // Section 37: Nullish Type Utilities
    // =========================================================================

    /// Split a type into its non-nullable part and its nullable cause.
    /// Returns (`non_null_type`, `nullable_cause`) where `nullable_cause` is the type that makes it nullable.
    pub(crate) fn split_nullish_type(
        &mut self,
        type_id: TypeId,
    ) -> (Option<TypeId>, Option<TypeId>) {
        tsz_solver::split_nullish_type(self.ctx.types.as_type_database(), type_id)
    }

    /// Report an error for nullish object access.
    /// Emits TS18050 when the value IS definitively null/undefined,
    /// or TS2531/2532/2533 when the value is POSSIBLY null/undefined.
    ///
    /// # Arguments
    /// * `idx` - The node index of the expression being accessed
    /// * `cause` - The nullish type (null, undefined, or null|undefined)
    /// * `is_definitely_nullish` - If true, the entire type is nullish (emit TS18050).
    ///   If false, the type includes nullish but also non-nullish parts (emit TS2531/2532/2533).
    pub(crate) fn report_nullish_object(
        &mut self,
        idx: NodeIndex,
        cause: TypeId,
        is_definitely_nullish: bool,
    ) {
        use crate::diagnostics::diagnostic_codes;

        // Check if the expression is a literal null/undefined keyword (not a variable)
        // TS18050 is only for `null.foo` and `undefined.bar`, not `x.foo` where x: null
        // TS18050 is emitted even without strictNullChecks, so check first
        let is_literal_nullish = if let Some(node) = self.ctx.arena.get(idx) {
            use tsz_scanner::SyntaxKind;
            node.kind == SyntaxKind::NullKeyword as u16
                || (node.kind == SyntaxKind::Identifier as u16
                    && self
                        .ctx
                        .arena
                        .get_identifier(node)
                        .is_some_and(|ident| ident.escaped_text == "undefined"))
        } else {
            false
        };

        // When the expression IS a literal null/undefined keyword (e.g., null.foo or undefined.bar),
        // emit TS18050 "The value 'X' cannot be used here." (even without strictNullChecks)
        if is_definitely_nullish && is_literal_nullish {
            let value_name = if cause == TypeId::NULL {
                "null"
            } else if cause == TypeId::UNDEFINED {
                "undefined"
            } else {
                "null | undefined"
            };
            self.error_at_node(
                idx,
                &format!("The value '{value_name}' cannot be used here."),
                diagnostic_codes::THE_VALUE_CANNOT_BE_USED_HERE,
            );
            return;
        }

        // Without strictNullChecks, null/undefined are in every type's domain,
        // so TS18047/TS18048/TS18049 are never emitted (matches tsc behavior).
        // Note: TS18050 for literal null/undefined is handled above.
        if !self.ctx.compiler_options.strict_null_checks {
            return;
        }

        // Try to get the name if the expression is an identifier
        // Use specific error codes (TS18047/18048/18049) when name is available
        let name = self
            .ctx
            .arena
            .get(idx)
            .and_then(|node| self.ctx.arena.get_identifier(node))
            .map(|ident| ident.escaped_text.clone());

        let (code, message) = if let Some(ref name) = name {
            // Use specific error codes with the variable name
            if cause == TypeId::NULL {
                (
                    diagnostic_codes::IS_POSSIBLY_NULL,
                    format!("'{name}' is possibly 'null'."),
                )
            } else if cause == TypeId::UNDEFINED {
                (
                    diagnostic_codes::IS_POSSIBLY_UNDEFINED,
                    format!("'{name}' is possibly 'undefined'."),
                )
            } else {
                (
                    diagnostic_codes::IS_POSSIBLY_NULL_OR_UNDEFINED,
                    format!("'{name}' is possibly 'null' or 'undefined'."),
                )
            }
        } else {
            // Fall back to generic error codes
            if cause == TypeId::NULL {
                (
                    diagnostic_codes::OBJECT_IS_POSSIBLY_NULL,
                    "Object is possibly 'null'.".to_string(),
                )
            } else if cause == TypeId::UNDEFINED {
                (
                    diagnostic_codes::OBJECT_IS_POSSIBLY_UNDEFINED,
                    "Object is possibly 'undefined'.".to_string(),
                )
            } else {
                (
                    diagnostic_codes::OBJECT_IS_POSSIBLY_NULL_OR_UNDEFINED,
                    "Object is possibly 'null' or 'undefined'.".to_string(),
                )
            }
        };

        self.error_at_node(idx, &message, code);
    }

    /// Report an error for possibly nullish object access (legacy wrapper).
    /// Use `report_nullish_object` directly for new code.
    pub(crate) fn report_possibly_nullish_object(&mut self, idx: NodeIndex, cause: TypeId) {
        // Legacy behavior: check if this is a literal null/undefined keyword
        // If so, it's definitely nullish; otherwise, it's possibly nullish
        use tsz_scanner::SyntaxKind;

        let is_definitely_nullish = if let Some(node) = self.ctx.arena.get(idx) {
            node.kind == SyntaxKind::NullKeyword as u16
                || (node.kind == SyntaxKind::Identifier as u16
                    && self
                        .ctx
                        .arena
                        .get_identifier(node)
                        .is_some_and(|ident| ident.escaped_text == "undefined"))
        } else {
            false
        };

        self.report_nullish_object(idx, cause, is_definitely_nullish);
    }

    // =========================================================================
    // Truthiness Checks
    // =========================================================================

    /// TS2872: "This kind of expression is always truthy."
    ///
    /// Emitted when a truthy-checked expression is syntactically always truthy.
    /// Matches tsc's `getSyntacticTruthySemantics` — purely syntactic, never type-based.
    /// TS2872: Check if expression is syntactically always truthy.
    /// Used for left side of `||` and `??` operators.
    pub(crate) fn check_always_truthy(&mut self, node_idx: NodeIndex, _type_id: TypeId) {
        if self.get_syntactic_truthy_semantics(node_idx) == SyntacticTruthiness::AlwaysTruthy {
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node(
                node_idx,
                "This kind of expression is always truthy.",
                diagnostic_codes::THIS_KIND_OF_EXPRESSION_IS_ALWAYS_TRUTHY,
            );
        }
    }

    /// TS2872/TS2873: Check if condition is syntactically always truthy or falsy.
    /// Used for if-conditions and `!` operands.
    pub(crate) fn check_truthy_or_falsy(&mut self, node_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;
        match self.get_syntactic_truthy_semantics(node_idx) {
            SyntacticTruthiness::AlwaysTruthy => {
                self.error_at_node(
                    node_idx,
                    "This kind of expression is always truthy.",
                    diagnostic_codes::THIS_KIND_OF_EXPRESSION_IS_ALWAYS_TRUTHY,
                );
            }
            SyntacticTruthiness::AlwaysFalsy => {
                self.error_at_node(
                    node_idx,
                    "This kind of expression is always falsy.",
                    diagnostic_codes::THIS_KIND_OF_EXPRESSION_IS_ALWAYS_FALSY,
                );
            }
            SyntacticTruthiness::Sometimes => {}
        }
    }

    /// Matches tsc's `getSyntacticTruthySemantics`.
    ///
    /// Returns whether an expression is syntactically always truthy, always falsy,
    /// or sometimes either. This is a purely syntactic check — it examines node kinds
    /// and literal values, never the resolved type.
    fn get_syntactic_truthy_semantics(&self, node_idx: NodeIndex) -> SyntacticTruthiness {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return SyntacticTruthiness::Sometimes;
        };

        // Skip parenthesized expressions (tsc's skipOuterExpressions)
        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                return self.get_syntactic_truthy_semantics(paren.expression);
            }
            return SyntacticTruthiness::Sometimes;
        }

        if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION {
            if let Some(unary) = self.ctx.arena.get_unary_expr_ex(node) {
                return self.get_syntactic_truthy_semantics(unary.expression);
            }
            return SyntacticTruthiness::Sometimes;
        }

        if node.kind == syntax_kind_ext::TYPE_ASSERTION
            || node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION
        {
            if let Some(assertion) = self.ctx.arena.get_type_assertion(node) {
                return self.get_syntactic_truthy_semantics(assertion.expression);
            }
            return SyntacticTruthiness::Sometimes;
        }

        match node.kind {
            // Numeric literals: 0 and 1 are "sometimes" (allows while(0)/while(1)),
            // all others are always truthy
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.ctx.arena.get_literal(node)
                    && (lit.text == "0" || lit.text == "1")
                {
                    return SyntacticTruthiness::Sometimes;
                }
                SyntacticTruthiness::AlwaysTruthy
            }
            // These expression kinds are always truthy (they produce objects/functions)
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == SyntaxKind::BigIntLiteral as u16
                || k == syntax_kind_ext::CLASS_EXPRESSION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == SyntaxKind::RegularExpressionLiteral as u16 =>
            {
                SyntacticTruthiness::AlwaysTruthy
            }
            // void and null are always falsy
            // Note: Our parser represents `void expr` as PREFIX_UNARY_EXPRESSION
            // with VoidKeyword operator, not as a separate VOID_EXPRESSION node.
            k if k == syntax_kind_ext::VOID_EXPRESSION || k == SyntaxKind::NullKeyword as u16 => {
                SyntacticTruthiness::AlwaysFalsy
            }
            // Handle void as a prefix unary (our parser emits void as PREFIX_UNARY_EXPRESSION)
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.ctx.arena.get_unary_expr(node)
                    && unary.operator == SyntaxKind::VoidKeyword as u16
                {
                    return SyntacticTruthiness::AlwaysFalsy;
                }
                SyntacticTruthiness::Sometimes
            }
            // String/template literals: truthy if non-empty, falsy if empty
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                if let Some(lit) = self.ctx.arena.get_literal(node) {
                    if lit.text.is_empty() {
                        SyntacticTruthiness::AlwaysFalsy
                    } else {
                        SyntacticTruthiness::AlwaysTruthy
                    }
                } else {
                    SyntacticTruthiness::Sometimes
                }
            }
            // Conditional expressions: combine both branches
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.ctx.arena.get_conditional_expr(node) {
                    let when_true = self.get_syntactic_truthy_semantics(cond.when_true);
                    let when_false = self.get_syntactic_truthy_semantics(cond.when_false);
                    when_true.combine(when_false)
                } else {
                    SyntacticTruthiness::Sometimes
                }
            }
            // Everything else (identifiers, call expressions, etc.) is "sometimes"
            _ => SyntacticTruthiness::Sometimes,
        }
    }

    // =========================================================================
    // Section 38: Index Signature Utilities
    // =========================================================================

    /// Merge an incoming index signature into a target.
    /// If the signatures conflict, sets the target to ERROR.
    pub(crate) fn merge_index_signature(
        target: &mut Option<tsz_solver::IndexSignature>,
        incoming: tsz_solver::IndexSignature,
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
}
