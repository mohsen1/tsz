//! Modifier, member access, and query methods for `CheckerState`.

use crate::query_boundaries::common::contains_type_parameters;
use crate::state::{CheckerState, MemberAccessLevel};
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// Extract a property name from a non-computed property name node.
///
/// Handles identifiers, string literals, no-substitution template literals,
/// numeric literals (canonicalized via `canonicalize_numeric_name`), and
/// signed numeric literals (`+1`, `-1`) matching TSC's `isSignedNumericLiteral`.
/// Does NOT handle computed property names — callers must handle those separately
/// when symbol resolution or special formatting is needed.
pub(crate) fn get_literal_property_name(arena: &NodeArena, name_idx: NodeIndex) -> Option<String> {
    let name_node = arena.get(name_idx)?;

    // Identifier
    if let Some(ident) = arena.get_identifier(name_node) {
        return Some(ident.escaped_text.clone());
    }

    // String literal, no-substitution template literal, or numeric literal
    if matches!(
        name_node.kind,
        k if k == SyntaxKind::StringLiteral as u16
            || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            || k == SyntaxKind::NumericLiteral as u16
    ) && let Some(lit) = arena.get_literal(name_node)
    {
        // Canonicalize numeric property names (e.g. "1.", "1.0" -> "1")
        if name_node.kind == SyntaxKind::NumericLiteral as u16
            && let Some(canonical) = tsz_solver::utils::canonicalize_numeric_name(&lit.text)
        {
            return Some(canonical);
        }
        return Some(lit.text.clone());
    }

    // Signed numeric literal: prefix +/- with numeric literal operand.
    // TSC's isSignedNumericLiteral handles `[+1]` → "1" and `[-1]` → "-1".
    if name_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
        && let Some(unary) = arena.get_unary_expr(name_node)
        && (unary.operator == SyntaxKind::PlusToken as u16
            || unary.operator == SyntaxKind::MinusToken as u16)
        && let Some(operand_node) = arena.get(unary.operand)
        && operand_node.kind == SyntaxKind::NumericLiteral as u16
        && let Some(lit) = arena.get_literal(operand_node)
    {
        let num_text = tsz_solver::utils::canonicalize_numeric_name(&lit.text)
            .unwrap_or_else(|| lit.text.clone());
        if unary.operator == SyntaxKind::MinusToken as u16 {
            return Some(format!("-{num_text}"));
        }
        return Some(num_text);
    }

    None
}

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Section 27: Modifier and Member Access Utilities
    // =========================================================================

    /// Check if the current file is a JavaScript file (.js, .jsx, .mjs, .cjs).
    /// Delegates to `CheckerContext::is_js_file()`.
    pub(crate) fn is_js_file(&self) -> bool {
        self.ctx.is_js_file()
    }

    /// Check if a node has the `declare` modifier.
    pub(crate) fn has_declare_modifier(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> bool {
        self.ctx
            .arena
            .has_modifier(modifiers, SyntaxKind::DeclareKeyword)
    }

    /// Find the `declare` modifier `NodeIndex` in a modifier list, if present.
    /// Used to point error messages at the specific modifier.
    pub(crate) fn get_declare_modifier(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> Option<NodeIndex> {
        self.ctx
            .arena
            .find_modifier(modifiers, SyntaxKind::DeclareKeyword)
    }

    /// Check if a node has the `async` modifier.
    pub(crate) fn has_async_modifier(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> bool {
        self.ctx
            .arena
            .has_modifier(modifiers, SyntaxKind::AsyncKeyword)
    }

    /// Find the `async` modifier `NodeIndex` in a modifier list, if present.
    pub(crate) fn find_async_modifier(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> Option<NodeIndex> {
        self.ctx
            .arena
            .find_modifier(modifiers, SyntaxKind::AsyncKeyword)
    }

    pub(crate) fn find_override_modifier(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> Option<NodeIndex> {
        self.ctx
            .arena
            .find_modifier(modifiers, SyntaxKind::OverrideKeyword)
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
        self.ctx
            .arena
            .has_modifier(modifiers, SyntaxKind::PublicKeyword)
            || self
                .ctx
                .arena
                .has_modifier(modifiers, SyntaxKind::PrivateKeyword)
            || self
                .ctx
                .arena
                .has_modifier(modifiers, SyntaxKind::ProtectedKeyword)
            || self
                .ctx
                .arena
                .has_modifier(modifiers, SyntaxKind::ReadonlyKeyword)
            || self
                .ctx
                .arena
                .has_modifier(modifiers, SyntaxKind::OverrideKeyword)
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
    /// Delegates to [`NodeArena::get_visibility_from_modifiers`].
    pub(crate) fn get_visibility_from_modifiers(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> tsz_solver::Visibility {
        self.ctx.arena.get_visibility_from_modifiers(modifiers)
    }

    /// Get the effective member visibility, treating ECMAScript private identifiers
    /// as non-public members even though they don't use `private` modifiers.
    pub(crate) fn get_member_visibility(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
        name_idx: NodeIndex,
    ) -> tsz_solver::Visibility {
        if self.is_private_identifier_name(name_idx) {
            tsz_solver::Visibility::Private
        } else {
            self.get_visibility_from_modifiers(modifiers)
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

    /// Check whether the enclosing class has a non-static (instance) member with
    /// the given name. Used for TS2663 "Did you mean the instance member 'this.X'?"
    pub(crate) fn is_instance_member(&self, member_nodes: &[NodeIndex], name: &str) -> bool {
        for &member_idx in member_nodes {
            if let Some(sym_id) = self.ctx.binder.get_node_symbol(member_idx)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && symbol.escaped_name == name
                && (symbol.flags & symbol_flags::STATIC == 0)
            {
                return true;
            }
        }
        false
    }

    /// Find an abstract instance property/accessor in the current class chain and
    /// return the declaring class name for TS2715 reporting.
    pub(crate) fn find_abstract_property_declaring_class(
        &self,
        class_idx: NodeIndex,
        name: &str,
    ) -> Option<String> {
        let mut current = class_idx;
        let mut visited = 0usize;

        while visited < 64 {
            visited += 1;

            let class_node = self.ctx.arena.get(current)?;
            let class_data = self.ctx.arena.get_class(class_node)?;

            for &member_idx in &class_data.members.nodes {
                let Some(member_name) = self.get_member_name(member_idx) else {
                    continue;
                };
                if member_name != name {
                    continue;
                }

                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let is_property_like = matches!(
                    member_node.kind,
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION
                        || k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR
                );
                if !is_property_like {
                    continue;
                }

                if !self.member_is_abstract(member_idx) {
                    return None;
                }

                let class_name = if class_data.name.is_some() {
                    self.ctx
                        .arena
                        .get(class_data.name)
                        .and_then(|node| self.ctx.arena.get_identifier(node))
                        .map(|ident| ident.escaped_text.clone())
                        .unwrap_or_default()
                } else {
                    String::new()
                };

                return Some(class_name);
            }

            let Some(base_idx) = self.get_base_class_idx(current) else {
                break;
            };
            current = base_idx;
        }

        None
    }

    /// Returns true when `idx` is inside an instance property initializer of the
    /// enclosing class. Field initializers run in the same abstract-property
    /// access context as constructors for TS2715.
    pub(crate) fn is_in_instance_property_initializer(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        let mut visited = 0usize;

        while visited < 64 {
            visited += 1;
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return false;
            }

            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };

            if parent_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                return self
                    .ctx
                    .arena
                    .get_property_decl(parent_node)
                    .is_some_and(|prop| {
                        prop.initializer.is_some() && !self.has_static_modifier(&prop.modifiers)
                    });
            }

            if matches!(
                parent_node.kind,
                k if k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR
                    || k == syntax_kind_ext::CONSTRUCTOR
                    || k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION
            ) {
                return false;
            }

            current = parent;
        }

        false
    }

    // =========================================================================
    // Section 28: Expression Analysis Utilities
    // =========================================================================

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
        let expr_idx = self.ctx.arena.skip_parenthesized(expr_idx);
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

    /// Check if a node is inside a bare block statement (a block that is NOT a
    /// function/method/accessor body).  Walks up the parent chain (max 6 levels)
    /// to find the nearest `Block` node and returns `true` if that block's parent
    /// is **not** a function-like declaration.
    ///
    /// Used by the TS2695 suppression logic: comma expressions inside bare blocks
    /// with parse errors are likely malformed destructuring patterns
    /// (e.g., `{ a, b } = fn()`), and the diagnostic should be suppressed.
    pub(crate) fn is_inside_bare_block(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        // Function-like parent kinds whose body-block should NOT be considered
        // a "bare block".
        const fn is_function_like(kind: u16) -> bool {
            kind == syntax_kind_ext::FUNCTION_DECLARATION
                || kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || kind == syntax_kind_ext::ARROW_FUNCTION
                || kind == syntax_kind_ext::METHOD_DECLARATION
                || kind == syntax_kind_ext::CONSTRUCTOR
                || kind == syntax_kind_ext::GET_ACCESSOR
                || kind == syntax_kind_ext::SET_ACCESSOR
                || kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
        }

        let mut current = idx;
        for _ in 0..6 {
            let ext = self.ctx.arena.get_extended(current);
            let parent = ext.map_or(NodeIndex::NONE, |e| e.parent);
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };
            if parent_node.kind == syntax_kind_ext::BLOCK {
                // Found a Block ancestor.  Check its parent.
                let block_parent = self
                    .ctx
                    .arena
                    .get_extended(parent)
                    .map_or(NodeIndex::NONE, |e| e.parent);
                if block_parent.is_none() {
                    // Block at the top level (no parent) — treat as bare.
                    return true;
                }
                let Some(bp_node) = self.ctx.arena.get(block_parent) else {
                    return true;
                };
                // If the block's parent is a function-like node, it's a function
                // body — not a bare block.
                return !is_function_like(bp_node.kind);
            }
            current = parent;
        }
        false
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

    /// Check if a `this` keyword node resolves to `typeof globalThis` —
    /// either at module/script top-level or inside a global-capturing arrow.
    pub(crate) fn is_this_resolving_to_global(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind != SyntaxKind::ThisKeyword as u16 {
            return false;
        }
        // `this` at the top level (no enclosing non-arrow function, no enclosing class)
        // resolves to `typeof globalThis`.
        if self.ctx.enclosing_class.is_none()
            && self.find_enclosing_non_arrow_function(idx).is_none()
        {
            // Double-check the AST: `enclosing_class` may be None during lazy
            // evaluation of class field initializers (e.g., `other = this.prop`),
            // or `this` may be inside a namespace block (where `this` is the
            // namespace's runtime context, not `globalThis`).
            if self.is_inside_class_or_namespace(idx) {
                return false;
            }
            return true;
        }
        // `this` in a top-level arrow function that captures globalThis.
        if self.is_this_in_global_capturing_arrow(idx) {
            return true;
        }
        false
    }

    /// Check if a node is inside a class body or namespace by walking AST parents.
    fn is_inside_class_or_namespace(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let mut current = idx;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return false;
            }
            if let Some(parent_node) = self.ctx.arena.get(parent_idx)
                && (parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
                    || parent_node.kind == syntax_kind_ext::MODULE_DECLARATION)
            {
                return true;
            }
            current = parent_idx;
        }
        false
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

    /// Check if a node is an ambient global object alias that should resolve through
    /// the global property table like `globalThis`.
    ///
    /// This intentionally accepts `self` and `window` only when they resolve to an
    /// ambient/global symbol rather than a same-file local binding.
    pub(crate) fn is_global_this_like_expression(&self, idx: NodeIndex) -> bool {
        if self.is_global_this_expression(idx) {
            return true;
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(node) else {
            return false;
        };
        if !matches!(ident.escaped_text.as_str(), "self" | "window") {
            return false;
        }

        let Some(global_sym_id) = self.resolve_global_value_symbol(&ident.escaped_text) else {
            return false;
        };

        if let Some(sym_id) = self.resolve_identifier_symbol(idx)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            let mut same_file_decl = symbol
                .declarations
                .iter()
                .copied()
                .any(|decl_idx| self.ctx.arena.get(decl_idx).is_some());

            if !same_file_decl
                && symbol.value_declaration != NodeIndex::NONE
                && self.ctx.arena.get(symbol.value_declaration).is_some()
            {
                same_file_decl = true;
            }

            if same_file_decl {
                return false;
            }
        }

        self.ctx.binder.get_symbol(global_sym_id).is_some()
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
    ///
    /// Also handles computed property names with literal or symbol expressions.
    pub(crate) fn get_property_name(&self, name_idx: NodeIndex) -> Option<String> {
        // Try non-computed property name first
        if let Some(name) = get_literal_property_name(self.ctx.arena, name_idx) {
            return Some(name);
        }

        // Handle computed property names
        let name_node = self.ctx.arena.get(name_idx)?;
        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.ctx.arena.get_computed_property(name_node)
        {
            if let Some(symbol_name) = self.get_symbol_property_name_from_expr(computed.expression)
            {
                return Some(symbol_name);
            }
            // Skip identifiers in computed expressions — they are variable references
            // (e.g. `[an]` where `const an = 0`), not literal property names. Callers
            // that need type-based resolution (e.g. object literal type computation)
            // should fall back to evaluating the expression's type.
            let expr_node = self.ctx.arena.get(computed.expression)?;
            if self.ctx.arena.get_identifier(expr_node).is_some() {
                return None;
            }
            return get_literal_property_name(self.ctx.arena, computed.expression);
        }

        None
    }

    /// Like `get_property_name` but additionally resolves computed property names
    /// by evaluating the expression's type when the syntax alone cannot determine
    /// the name. This handles cases like `const k = 'foo' as const; class C { [k]() {} }`
    /// and `const k = 'foo'; class C { [k]() {} }` (tsc infers the literal type from
    /// the const initializer).
    pub(crate) fn get_property_name_resolved(&mut self, name_idx: NodeIndex) -> Option<String> {
        let name_node = self.ctx.arena.get(name_idx)?;
        // For computed property names with identifier expressions (e.g., `[k]` where
        // `const k = 'foo'`), skip `get_property_name` which would incorrectly return
        // the identifier text ("k") instead of the resolved value ("foo").
        // Instead, evaluate the expression type to resolve the actual property name.
        let is_computed_identifier = name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && self
                .ctx
                .arena
                .get_computed_property(name_node)
                .and_then(|computed| self.ctx.arena.get(computed.expression))
                .is_some_and(|expr_node| self.ctx.arena.get_identifier(expr_node).is_some());

        if !is_computed_identifier && let Some(name) = self.get_property_name(name_idx) {
            // When the syntactic resolver returns a `[Symbol.xxx]` name but the
            // property access expression resolves to ERROR (e.g. `Symbol.nonsense`
            // where `nonsense` doesn't exist on SymbolConstructor), discard the
            // name. This prevents creating a phantom named property in the object
            // type, which would cause false TS2322 errors on assignment.
            if name.starts_with("[Symbol.")
                && let Some(computed) = self.ctx.arena.get_computed_property(name_node)
                && self.get_type_of_node(computed.expression) == TypeId::ERROR
            {
                return None;
            }
            return Some(name);
        }

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            let computed = self.ctx.arena.get_computed_property(name_node)?;
            // Preserve literal types so that `const k = 'foo'` (no `as const`)
            // still resolves to the literal `"foo"` rather than widening to `string`.
            let prev = self.ctx.preserve_literal_types;
            self.ctx.preserve_literal_types = true;
            // Set checking_computed_property_name to suppress TS1212 emission from
            // get_type_of_identifier when resolving reserved words like `public`.
            // The proper TS1212/TS1213 diagnostic is emitted by check_computed_property_name.
            let prev_checking = self.ctx.checking_computed_property_name.take();
            self.ctx.checking_computed_property_name = Some(name_idx);
            let prop_name_type = self.get_type_of_node(computed.expression);
            self.ctx.checking_computed_property_name = prev_checking;
            self.ctx.preserve_literal_types = prev;

            let evaluated_prop_name_type = self.evaluate_type_with_env(prop_name_type);
            let resolved_for_property_access =
                self.resolve_type_for_property_access(evaluated_prop_name_type);
            let resolved_prop_name_type = self.resolve_lazy_type(resolved_for_property_access);
            let application_prop_name_type =
                self.evaluate_application_type(resolved_prop_name_type);
            let assignability_prop_name_type = self.evaluate_type_for_assignability(prop_name_type);

            // Fallback: when the computed expression is an identifier referencing a
            // variable initialized with or annotated as `Symbol.xxx`, resolve to the
            // canonical `[Symbol.xxx]` property name.  This handles patterns like:
            //   const observable: typeof Symbol.obs = Symbol.obs;
            //   class C { [observable]() { ... } }
            // where type-based resolution yields plain `symbol` instead of a unique symbol.
            if prop_name_type == TypeId::SYMBOL
                && let Some(well_known) =
                    self.resolve_computed_symbol_property_name(computed.expression)
            {
                return Some(well_known);
            }
            // When the computed property type resolves to a unique symbol (e.g.
            // `typeof Symbol.obs`), map it to the canonical `[Symbol.xxx]` format
            // that type literals and interfaces use.  Without this, class members
            // like `[observable]()` (where `const observable = Symbol.obs`) would
            // be stored as `__unique_N` while the target type uses `[Symbol.obs]`,
            // causing false TS2345/TS2322 structural mismatches.
            for candidate in [
                prop_name_type,
                evaluated_prop_name_type,
                resolved_prop_name_type,
                application_prop_name_type,
                assignability_prop_name_type,
            ] {
                if let Some(sym_ref) =
                    tsz_solver::visitor::unique_symbol_ref(self.ctx.types, candidate)
                {
                    let sym_id = tsz_binder::SymbolId(sym_ref.0);
                    if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                        let sym_name = symbol.escaped_name.clone();
                        if symbol.parent.is_some()
                            && let Some(parent_sym) = self.ctx.binder.get_symbol(symbol.parent)
                            && parent_sym.escaped_name == "Symbol"
                        {
                            return Some(format!("[Symbol.{sym_name}]"));
                        }
                    }
                }
            }

            for candidate in [
                prop_name_type,
                evaluated_prop_name_type,
                resolved_prop_name_type,
                application_prop_name_type,
                assignability_prop_name_type,
            ] {
                if let Some(atom) =
                    crate::query_boundaries::type_computation::access::literal_property_name(
                        self.ctx.types,
                        candidate,
                    )
                {
                    tracing::trace!(
                        name_idx = name_idx.0,
                        expr_idx = computed.expression.0,
                        prop_name_type = prop_name_type.0,
                        prop_name_type_str = %self.format_type(prop_name_type),
                        evaluated_prop_name_type = evaluated_prop_name_type.0,
                        evaluated_prop_name_type_str = %self.format_type(evaluated_prop_name_type),
                        resolved_prop_name_type = resolved_prop_name_type.0,
                        resolved_prop_name_type_str = %self.format_type(resolved_prop_name_type),
                        application_prop_name_type = application_prop_name_type.0,
                        application_prop_name_type_str = %self.format_type(application_prop_name_type),
                        assignability_prop_name_type = assignability_prop_name_type.0,
                        assignability_prop_name_type_str = %self.format_type(assignability_prop_name_type),
                        chosen_candidate = candidate.0,
                        chosen_name = %self.ctx.types.resolve_atom(atom),
                        "get_property_name_resolved: computed property resolved"
                    );
                    return Some(self.ctx.types.resolve_atom(atom));
                }
            }
            tracing::trace!(
                name_idx = name_idx.0,
                expr_idx = computed.expression.0,
                prop_name_type = prop_name_type.0,
                prop_name_type_str = %self.format_type(prop_name_type),
                evaluated_prop_name_type = evaluated_prop_name_type.0,
                evaluated_prop_name_type_str = %self.format_type(evaluated_prop_name_type),
                resolved_prop_name_type = resolved_prop_name_type.0,
                resolved_prop_name_type_str = %self.format_type(resolved_prop_name_type),
                application_prop_name_type = application_prop_name_type.0,
                application_prop_name_type_str = %self.format_type(application_prop_name_type),
                assignability_prop_name_type = assignability_prop_name_type.0,
                assignability_prop_name_type_str = %self.format_type(assignability_prop_name_type),
                "get_property_name_resolved: computed property unresolved"
            );
            None
        } else {
            None
        }
    }

    /// For an identifier expression, trace back to the variable's declaration
    /// and check if the initializer or type annotation references `Symbol.xxx`.
    /// If so, return the canonical `[Symbol.xxx]` property name.
    ///
    /// This handles computed property names like `[observable]` where
    /// `const observable: typeof Symbol.obs = Symbol.obs`.  The declared type
    /// resolves to plain `symbol`, but the structural property key must match
    /// the `[Symbol.obs]` format used by type literals.
    fn resolve_computed_symbol_property_name(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_node = self.ctx.arena.get(expr_idx)?;
        let ident = self.ctx.arena.get_identifier(expr_node)?;

        // Look up the identifier in the binder to find its declaration
        let sym_id = self.ctx.binder.file_locals.get(&ident.escaped_text)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl = symbol.value_declaration;
        let decl_node = self.ctx.arena.get(decl)?;
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;

        // Check initializer first: `= Symbol.obs`
        if var_decl.initializer.is_some()
            && let Some(name) = self.get_symbol_property_name_from_expr(var_decl.initializer)
        {
            return Some(name);
        }

        // Check type annotation: `typeof Symbol.obs`
        // The type annotation is a TYPE_QUERY node whose expr_name is `Symbol.obs`.
        if var_decl.type_annotation.is_some() {
            let ann_node = self.ctx.arena.get(var_decl.type_annotation)?;
            if ann_node.kind == syntax_kind_ext::TYPE_QUERY {
                let type_query = self.ctx.arena.get_type_query(ann_node)?;
                if let Some(name) = self.get_symbol_property_name_from_expr(type_query.expr_name) {
                    return Some(name);
                }
            }
        }

        None
    }

    pub(crate) fn get_bound_class_name_from_decl(&self, class_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(class_idx)?;
        let class = self.ctx.arena.get_class(node)?;

        if class.name.is_some()
            && let Some(name_node) = self.ctx.arena.get(class.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            return Some(ident.escaped_text.clone());
        }

        let parent_idx = self
            .ctx
            .arena
            .get_extended(class_idx)
            .map(|ext| ext.parent)?;
        let parent_node = self.ctx.arena.get(parent_idx)?;
        if parent_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let var_decl = self.ctx.arena.get_variable_declaration(parent_node)?;
        let name_ident = self.ctx.arena.get_identifier_at(var_decl.name)?;
        Some(name_ident.escaped_text.clone())
    }

    /// Get class name from a class declaration node.
    /// Returns "<anonymous>" for unnamed classes.
    pub(crate) fn get_class_name_from_decl(&self, class_idx: NodeIndex) -> String {
        self.get_bound_class_name_from_decl(class_idx)
            .unwrap_or_else(|| "<anonymous>".to_string())
    }

    pub(crate) fn get_class_decl_for_display_type(
        &self,
        type_id: TypeId,
    ) -> Option<(NodeIndex, bool)> {
        if let Some(class_idx) = self.get_class_decl_from_type(type_id) {
            return Some((class_idx, false));
        }

        if let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, type_id)
            && let Some(sym_id) = self.ctx.def_to_symbol_id_with_fallback(def_id)
            && let Some(class_idx) = self.get_class_declaration_from_symbol(sym_id)
        {
            return Some((class_idx, true));
        }

        if let Some((&class_idx, _)) = self
            .ctx
            .class_constructor_type_cache
            .iter()
            .find(|entry| *entry.1 == type_id)
        {
            return Some((class_idx, true));
        }

        // Check symbol_types: if a class symbol's resolved type matches this type_id,
        // the type represents that class's constructor. This handles inferred return types
        // like `getClass() { return C; }` where the return type is a fresh TypeId that
        // differs from the cached constructor type.
        for (&sym_id, &sym_type) in self.ctx.symbol_types.iter() {
            if sym_type == type_id
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && symbol.flags & tsz_binder::symbol_flags::CLASS != 0
                && let Some(class_idx) = self.get_class_declaration_from_symbol(sym_id)
            {
                return Some((class_idx, true));
            }
        }

        let sigs = crate::query_boundaries::common::construct_signatures_for_type(
            self.ctx.types,
            type_id,
        )?;
        for sig in &sigs {
            if let Some(class_idx) = self.get_class_decl_from_type(sig.return_type) {
                return Some((class_idx, true));
            }
            if let Some(def_id) =
                crate::query_boundaries::common::lazy_def_id(self.ctx.types, sig.return_type)
                && let Some(sym_id) = self.ctx.def_to_symbol_id_with_fallback(def_id)
                && let Some(class_idx) = self.get_class_declaration_from_symbol(sym_id)
            {
                return Some((class_idx, true));
            }
        }

        None
    }

    pub(crate) fn get_class_display_name_from_type(&self, type_id: TypeId) -> Option<String> {
        let (class_idx, is_constructor) = self.get_class_decl_for_display_type(type_id)?;
        let class_name = self.get_class_name_from_decl(class_idx);
        if is_constructor {
            Some(format!("typeof {class_name}"))
        } else {
            Some(class_name)
        }
    }

    /// Get class name with type parameters from a class declaration node.
    /// E.g., for `class D<T>`, returns `"D<T>"` instead of just `"D"`.
    /// Returns "<anonymous>" for unnamed classes.
    pub(crate) fn get_class_name_with_type_params_from_decl(&self, class_idx: NodeIndex) -> String {
        let Some(node) = self.ctx.arena.get(class_idx) else {
            return "<anonymous>".to_string();
        };
        let Some(class) = self.ctx.arena.get_class(node) else {
            return "<anonymous>".to_string();
        };

        if class.name.is_some()
            && let Some(name_node) = self.ctx.arena.get(class.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            let mut name = ident.escaped_text.clone();
            self.append_type_param_names(&mut name, &class.type_parameters);
            return name;
        }

        self.get_bound_class_name_from_decl(class_idx)
            .unwrap_or_else(|| "<anonymous>".to_string())
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
            && func.name.is_some()
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

    fn is_import_defer_expression(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return false;
        };
        let Some(base_node) = self.ctx.arena.get(access.expression) else {
            return false;
        };
        if base_node.kind != SyntaxKind::ImportKeyword as u16 {
            return false;
        }
        self.ctx
            .arena
            .get_identifier_at(access.name_or_argument)
            .is_some_and(|ident| ident.escaped_text == "defer")
    }

    /// Check if a call expression is a dynamic import (`import('...')` or `import.defer('...')`).
    pub(crate) fn is_dynamic_import(&self, call: &tsz_parser::parser::node::CallExprData) -> bool {
        let Some(node) = self.ctx.arena.get(call.expression) else {
            return false;
        };
        node.kind == SyntaxKind::ImportKeyword as u16
            || self.is_import_defer_expression(call.expression)
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
        fn class_decl_from_decl_idx(
            checker: &CheckerState<'_>,
            decl_idx: NodeIndex,
        ) -> Option<NodeIndex> {
            let node = checker.ctx.arena.get(decl_idx)?;
            if checker.ctx.arena.get_class(node).is_some() {
                return Some(decl_idx);
            }
            if checker.ctx.arena.get_identifier(node).is_some() {
                let parent_idx = checker.ctx.arena.get_extended(decl_idx)?.parent;
                if parent_idx.is_none() {
                    return None;
                }
                let parent_node = checker.ctx.arena.get(parent_idx)?;
                if checker.ctx.arena.get_class(parent_node).is_some() {
                    return Some(parent_idx);
                }
            }
            None
        }

        if let Some(cached) = self
            .ctx
            .class_symbol_to_decl_cache
            .borrow()
            .get(&sym_id)
            .copied()
        {
            return cached;
        }

        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let resolved = if symbol.value_declaration.is_some() {
            let decl_idx = symbol.value_declaration;
            class_decl_from_decl_idx(self, decl_idx).or_else(|| {
                symbol
                    .declarations
                    .iter()
                    .find_map(|&decl_idx| class_decl_from_decl_idx(self, decl_idx))
            })
        } else {
            symbol
                .declarations
                .iter()
                .find_map(|&decl_idx| class_decl_from_decl_idx(self, decl_idx))
        };

        self.ctx
            .class_symbol_to_decl_cache
            .borrow_mut()
            .insert(sym_id, resolved);

        resolved
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

    /// Returns whether a type references type parameters.
    /// Cached because this query is hot on optional-chain/property access paths.
    pub(crate) fn contains_type_parameters_cached(&mut self, type_id: TypeId) -> bool {
        if let Some(&cached) = self
            .ctx
            .narrowing_cache
            .contains_type_parameters_cache
            .borrow()
            .get(&type_id)
        {
            return cached;
        }

        let contains = contains_type_parameters(self.ctx.types, type_id);
        self.ctx
            .narrowing_cache
            .contains_type_parameters_cache
            .borrow_mut()
            .insert(type_id, contains);
        contains
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
        if let Some(&cached) = self
            .ctx
            .narrowing_cache
            .split_nullish_cache
            .borrow()
            .get(&type_id)
        {
            return cached;
        }

        let split = tsz_solver::split_nullish_type(self.ctx.types.as_type_database(), type_id);
        self.ctx
            .narrowing_cache
            .split_nullish_cache
            .borrow_mut()
            .insert(type_id, split);
        split
    }

    /// Check if a node is a literal `null` keyword or an identifier named `undefined`.
    /// Used to distinguish `null.foo` / `undefined.bar` from `x.foo` where `x: null`.
    pub(crate) fn is_literal_null_or_undefined_node(&self, idx: NodeIndex) -> bool {
        use tsz_scanner::SyntaxKind;
        if let Some(node) = self.ctx.arena.get(idx) {
            node.kind == SyntaxKind::NullKeyword as u16
                || (node.kind == SyntaxKind::Identifier as u16
                    && self
                        .ctx
                        .arena
                        .get_identifier(node)
                        .is_some_and(|ident| ident.escaped_text == "undefined"))
        } else {
            false
        }
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
        let is_literal_nullish = self.is_literal_null_or_undefined_node(idx);

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
        let is_definitely_nullish = self.is_literal_null_or_undefined_node(idx);
        self.report_nullish_object(idx, cause, is_definitely_nullish);
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

    /// Find the class that declares a private member (e.g., `#prop`) by walking
    /// the class hierarchy of the given type. Returns the declaring class name.
    ///
    /// tsc reports TS18013 with the class that **declares** the private member,
    /// not the class of the object being accessed. For example, if `#prop` is
    /// declared in `Base` and `x: Derived`, the error should say "outside class 'Base'".
    pub(crate) fn get_declaring_class_name_for_private_member(
        &self,
        object_type: TypeId,
        member_name: &str,
    ) -> Option<String> {
        let class_idx = self.get_class_decl_for_display_type(object_type)?.0;
        let mut current = class_idx;
        let mut visited = rustc_hash::FxHashSet::default();

        while visited.insert(current) {
            if self.class_directly_declares_member(current, member_name) {
                return Some(self.get_class_name_from_decl(current));
            }
            match self.get_base_class_idx(current) {
                Some(base) => current = base,
                None => break,
            }
        }

        // Fallback: use the object type's own class name
        Some(self.get_class_name_from_decl(class_idx))
    }

    pub(crate) fn get_private_identifier_declaring_class_name(
        &mut self,
        object_type: TypeId,
        object_expr: NodeIndex,
        member_name: &str,
    ) -> String {
        self.get_declaring_class_name_for_private_member(object_type, member_name)
            .or_else(|| self.get_class_name_from_expression(object_expr))
            .or_else(|| {
                let fallback = self.format_type_diagnostic(object_type);
                fallback
                    .strip_prefix("typeof ")
                    .map(str::trim)
                    .filter(|name| {
                        !name.is_empty()
                            && !name.contains('{')
                            && !name.contains("=>")
                            && *name != "any"
                            && *name != "unknown"
                            && *name != "object"
                    })
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "the class".to_string())
    }

    /// Check if a class directly declares a member with the given name.
    fn class_directly_declares_member(&self, class_idx: NodeIndex, name: &str) -> bool {
        let Some(node) = self.ctx.arena.get(class_idx) else {
            return false;
        };
        let Some(class) = self.ctx.arena.get_class(node) else {
            return false;
        };
        for &member_idx in &class.members.nodes {
            if let Some(prop_name) = self.get_property_name_of_member(member_idx)
                && prop_name == name
            {
                return true;
            }
        }
        false
    }

    /// Get the property name of a class member (property, method, accessor).
    fn get_property_name_of_member(&self, member_idx: NodeIndex) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;
        let member_node = self.ctx.arena.get(member_idx)?;
        let name_idx = match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                self.ctx.arena.get_property_decl(member_node)?.name
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                self.ctx.arena.get_method_decl(member_node)?.name
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                self.ctx.arena.get_accessor(member_node)?.name
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                self.ctx.arena.get_accessor(member_node)?.name
            }
            _ => return None,
        };
        self.get_property_name(name_idx)
    }
}
