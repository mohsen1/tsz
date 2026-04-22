//! CommonJS/JS module assignment helpers.
//!
//! Handles special assignment patterns in JavaScript files:
//! - `module.exports = X` and `exports = X` declarations
//! - `exports.X = value` property declarations
//! - JS container export declarations
//! - Checked-JS constructor property declarations
//! - JS namespace enum rebind assignments

use crate::state::CheckerState;
use crate::symbols_domain::name_text::property_access_chain_text_in_arena;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    pub(crate) fn maybe_report_commonjs_export_implicit_any_assignment(
        &mut self,
        target_idx: NodeIndex,
        right_idx: NodeIndex,
    ) {
        if !self.is_js_file()
            || !self.ctx.compiler_options.check_js
            || !self.ctx.no_implicit_any()
            || self.ctx.has_real_syntax_errors
        {
            return;
        }

        let Some(name_idx) = self.commonjs_export_property_name_node(target_idx) else {
            return;
        };

        let direct_null = self.is_null_literal(right_idx);
        let chained_nullish = self.is_assignment_expression(right_idx)
            && self.assignment_chain_terminal_is_null_or_undefined(right_idx);
        if !direct_null && !chained_nullish {
            return;
        }

        let Some(name) = self
            .ctx
            .arena
            .get_identifier_at(name_idx)
            .map(|ident| ident.escaped_text.clone())
        else {
            return;
        };

        self.error_at_node_msg(
            self.ctx.arena.skip_parenthesized(target_idx),
            crate::diagnostics::diagnostic_codes::VARIABLE_IMPLICITLY_HAS_AN_TYPE,
            &[&name, "any"],
        );
    }

    fn commonjs_export_property_name_node(&self, target_idx: NodeIndex) -> Option<NodeIndex> {
        let target_idx = self.ctx.arena.skip_parenthesized(target_idx);
        let target_node = self.ctx.arena.get(target_idx)?;
        if target_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }

        let access = self.ctx.arena.get_access_expr(target_node)?;
        self.is_commonjs_module_exports_assignment(access.expression)
            .then_some(access.name_or_argument)
    }

    fn is_assignment_expression(&self, idx: NodeIndex) -> bool {
        self.ctx
            .arena
            .get(idx)
            .filter(|node| node.kind == syntax_kind_ext::BINARY_EXPRESSION)
            .and_then(|node| self.ctx.arena.get_binary_expr(node))
            .is_some_and(|binary| binary.operator_token == SyntaxKind::EqualsToken as u16)
    }

    fn assignment_chain_terminal_is_null_or_undefined(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        for _ in 0..512 {
            current = self.ctx.arena.skip_parenthesized_and_assertions(current);
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(binary) = self.ctx.arena.get_binary_expr(node)
                && binary.operator_token == SyntaxKind::EqualsToken as u16
            {
                current = binary.right;
                continue;
            }

            return self.is_null_literal(current) || self.is_undefined_identifier(current);
        }

        false
    }

    fn is_null_literal(&self, idx: NodeIndex) -> bool {
        let idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        self.ctx
            .arena
            .get(idx)
            .is_some_and(|node| node.kind == SyntaxKind::NullKeyword as u16)
    }

    fn is_undefined_identifier(&self, idx: NodeIndex) -> bool {
        let idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        self.ctx
            .arena
            .get(idx)
            .and_then(|node| self.ctx.arena.get_identifier(node))
            .is_some_and(|ident| ident.escaped_text == "undefined")
    }

    /// In JS files, `module.exports = X` and `exports = X` are declarations, not assignments.
    /// tsc does not check assignability for these — the type flows from the RHS.
    /// Without this suppression, tsz would emit false TS2322/TS2741 errors when the
    /// module's augmented export type (with later `.D = ...` property assignments)
    /// is used as the assignment target type.
    pub(crate) fn is_commonjs_module_exports_assignment(&self, target_idx: NodeIndex) -> bool {
        if !self.is_js_file() {
            return false;
        }

        let target_idx = self.ctx.arena.skip_parenthesized(target_idx);
        let Some(target_node) = self.ctx.arena.get(target_idx) else {
            return false;
        };

        // Check for `exports` identifier (unbound)
        if target_node.kind == SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.ctx.arena.get_identifier(target_node)
                && ident.escaped_text == "exports"
            {
                return true;
            }
            return false;
        }

        // Check for `module.exports` property access
        if target_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(target_node)
        {
            let is_module = self
                .ctx
                .arena
                .get_identifier_at(access.expression)
                .is_some_and(|ident| ident.escaped_text == "module");
            let is_exports = self
                .ctx
                .arena
                .get_identifier_at(access.name_or_argument)
                .is_some_and(|ident| ident.escaped_text == "exports");
            return is_module && is_exports;
        }

        false
    }

    /// In JS files, `exports.X = value` and `module.exports.X = value` are
    /// declaration-like assignments (tsc's `AssignmentDeclarationKind.ExportsProperty`).
    /// The type of the export property is inferred from the union of all assigned
    /// values, so individual assignments should not be checked for assignability
    /// against the inferred type. Without this, `exports.apply = undefined` followed
    /// by `exports.apply = function() {}` would emit false TS2322.
    pub(crate) fn is_commonjs_exports_property_declaration(&self, target_idx: NodeIndex) -> bool {
        if !self.is_js_file() {
            return false;
        }

        let target_idx = self.ctx.arena.skip_parenthesized(target_idx);
        let Some(target_node) = self.ctx.arena.get(target_idx) else {
            return false;
        };

        if target_node.kind != tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.ctx.arena.get_access_expr(target_node) else {
            return false;
        };

        // Check if the base is `exports` or `module.exports` (depth-1 only)
        self.is_exports_rooted_access(access.expression)
    }

    /// In JS files, assignments like `exports.n = {}` or `module.exports.n = {}`
    /// where `n` is subsequently augmented with property assignments (e.g., `exports.n.K = ...`)
    /// are JS container declarations. The type of the container is built up from all
    /// property assignments, so the initial value assignment should not be checked
    /// against the augmented type. tsc treats these as declarations, not assignments.
    pub(crate) fn is_js_container_export_declaration(&self, target_idx: NodeIndex) -> bool {
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        if !self.is_js_file() {
            return false;
        }

        let target_idx = self.ctx.arena.skip_parenthesized(target_idx);
        let Some(target_node) = self.ctx.arena.get(target_idx) else {
            return false;
        };

        // Must be a property access expression (e.g., `exports.n` or `module.exports.n`)
        if target_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        // Check if the base of the property access chain is rooted at `exports` or `module.exports`
        let Some(access) = self.ctx.arena.get_access_expr(target_node) else {
            return false;
        };

        let is_exports_rooted = self.is_exports_rooted_access(access.expression);
        if !is_exports_rooted {
            return false;
        }

        // Helper: check if a symbol has namespace-like members (is a JS container)
        let symbol_is_container = |symbol: &tsz_binder::Symbol| -> bool {
            if (symbol.flags
                & (symbol_flags::NAMESPACE_MODULE
                    | symbol_flags::VALUE_MODULE
                    | symbol_flags::MODULE))
                != 0
            {
                return true;
            }
            if symbol.members.as_ref().is_some_and(|m| !m.is_empty()) {
                return true;
            }
            if symbol.exports.as_ref().is_some_and(|e| !e.is_empty()) {
                return true;
            }
            false
        };

        // Check if the target symbol has namespace-like members (is a JS container)
        if let Some(sym_id) = self.resolve_qualified_symbol(target_idx)
            && let Some(symbol) = self
                .get_cross_file_symbol(sym_id)
                .or_else(|| self.ctx.binder.get_symbol(sym_id))
            && symbol_is_container(symbol)
        {
            return true;
        }

        // Fallback: when qualified symbol resolution fails (e.g., for `module.exports.b`
        // where `module` doesn't have a standard binder symbol), look up the property name
        // directly in the file's export table. This handles CJS patterns where
        // `module.exports.b = function() {}` followed by `module.exports.b.cat = "cat"`
        // creates an augmented export that the binder tracks in module_exports.
        let prop_name = self
            .ctx
            .arena
            .get_identifier_at(access.name_or_argument)
            .map(|ident| ident.escaped_text.as_str());
        if let Some(prop_name) = prop_name
            && let Some(file_exports) = self
                .ctx
                .module_exports_for_module(self.ctx.binder, &self.ctx.file_name)
            && let Some(export_sym_id) = file_exports.get(prop_name)
            && let Some(symbol) = self.ctx.binder.get_symbol(export_sym_id)
            && symbol_is_container(symbol)
        {
            return true;
        }

        false
    }

    pub(crate) fn is_checked_js_constructor_property_declaration(
        &self,
        target_idx: NodeIndex,
        right_idx: NodeIndex,
    ) -> bool {
        if !self.is_js_file() || !self.ctx.compiler_options.check_js {
            return false;
        }

        let target_idx = self.ctx.arena.skip_parenthesized(target_idx);
        let Some(target_node) = self.ctx.arena.get(target_idx) else {
            return false;
        };
        if target_node.kind != tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.ctx.arena.get_access_expr(target_node) else {
            return false;
        };
        let Some(prop_name) = self
            .ctx
            .arena
            .get_identifier_at(access.name_or_argument)
            .map(|ident| ident.escaped_text.to_string())
        else {
            return false;
        };
        let Some(prop_name_initial) = prop_name.chars().next() else {
            return false;
        };
        if !prop_name_initial.is_ascii_uppercase() {
            return false;
        };

        // Check if the RHS is a function or class expression, possibly wrapped
        // in a logical OR/nullish coalescing expression (e.g., `X.Y = X.Y || function() {}`).
        // tsc treats these "lazy constructor initialization" patterns as declarations
        // and does not check assignability.
        if !Self::rhs_contains_function_or_class_expression(self.ctx.arena, right_idx) {
            return false;
        }

        // Primary path: when qualified-name resolution finds a member symbol and any
        // of its declarations is a checked-JS constructor assignment.
        if let Some(checked_ctor_sym_id) = self.resolve_qualified_symbol(target_idx) {
            if let Some(symbol) = self
                .get_cross_file_symbol(checked_ctor_sym_id)
                .or_else(|| self.ctx.binder.get_symbol(checked_ctor_sym_id))
                && symbol.declarations.iter().copied().any(|decl_idx| {
                    self.declaration_is_checked_js_constructor_value_declaration(
                        checked_ctor_sym_id,
                        decl_idx,
                    )
                })
            {
                return true;
            }

            return false;
        }

        // Fallback path for JS expando containers where qualified resolution misses
        // the member symbol but binder metadata still records the assignment
        // (`var X = {}; X.Y = function(){}` style). This should behave like a
        // constructor declaration write in TS.
        fn root_identifier(
            arena: &tsz_parser::parser::node::NodeArena,
            idx: NodeIndex,
        ) -> Option<NodeIndex> {
            let mut current = idx;
            loop {
                let node = arena.get(current)?;
                match node.kind {
                    k if k == SyntaxKind::Identifier as u16 => return Some(current),
                    k if k == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                        let access = arena.get_access_expr(node)?;
                        current = access.expression;
                    }
                    _ => return None,
                }
            }
        }

        let Some(object_key) =
            property_access_chain_text_in_arena(self.ctx.arena, access.expression)
        else {
            return false;
        };
        let mut has_expando_property = self
            .collect_expando_properties_for_root(&object_key)
            .contains(&prop_name);
        if !has_expando_property && let Some((_, last_segment)) = object_key.rsplit_once('.') {
            has_expando_property = self
                .collect_expando_properties_for_root(last_segment)
                .contains(&prop_name);
        }
        if !has_expando_property {
            return false;
        }

        let Some(root_ident_idx) = root_identifier(self.ctx.arena, access.expression) else {
            return false;
        };
        let Some(root_sym_id) = self
            .resolve_identifier_symbol(root_ident_idx)
            .or_else(|| self.resolve_qualified_symbol(root_ident_idx))
        else {
            return false;
        };
        let Some(root_symbol) = self
            .get_cross_file_symbol(root_sym_id)
            .or_else(|| self.ctx.binder.get_symbol(root_sym_id))
        else {
            return false;
        };

        if root_symbol.has_any_flags(symbol_flags::ALIAS) && root_symbol.import_module.is_some() {
            return false;
        }
        if root_symbol.has_any_flags(symbol_flags::CLASS) {
            return false;
        }

        true
    }

    /// Check if an expression contains a `FunctionExpression` or `ClassExpression`,
    /// looking through parentheses, type assertions, and logical OR (`||`) /
    /// nullish coalescing (`??`) expressions.
    ///
    /// This handles patterns like `X.Y = X.Y || function() {}` where the
    /// function expression is wrapped in a binary logical expression.
    fn rhs_contains_function_or_class_expression(
        arena: &tsz_parser::parser::node::NodeArena,
        idx: NodeIndex,
    ) -> bool {
        let idx = arena.skip_parenthesized_and_assertions(idx);
        let Some(node) = arena.get(idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || node.kind == syntax_kind_ext::CLASS_EXPRESSION
        {
            return true;
        }
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = arena.get_binary_expr(node)
        {
            let op = bin.operator_token;
            if op == SyntaxKind::BarBarToken as u16
                || op == SyntaxKind::QuestionQuestionToken as u16
            {
                return Self::rhs_contains_function_or_class_expression(arena, bin.left)
                    || Self::rhs_contains_function_or_class_expression(arena, bin.right);
            }
        }
        false
    }

    /// Check if an expression is rooted at `exports` or `module.exports`.
    /// Walks up the property access chain to the root.
    pub(crate) fn is_exports_rooted_access(&self, expr_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let idx = self.ctx.arena.skip_parenthesized(expr_idx);
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        // Direct `exports` identifier
        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(node)
                .is_some_and(|ident| ident.escaped_text == "exports");
        }

        // Property access: check for `module.exports` or recurse
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(node)
        {
            // Check for `module.exports`
            let is_module = self
                .ctx
                .arena
                .get_identifier_at(access.expression)
                .is_some_and(|ident| ident.escaped_text == "module");
            let is_exports = self
                .ctx
                .arena
                .get_identifier_at(access.name_or_argument)
                .is_some_and(|ident| ident.escaped_text == "exports");
            if is_module && is_exports {
                return true;
            }

            // Recurse for deeper chains like `exports.n.K`
            return self.is_exports_rooted_access(access.expression);
        }

        false
    }

    pub(crate) fn is_js_namespace_enum_rebind_assignment_target(
        &self,
        target_idx: NodeIndex,
    ) -> bool {
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        if !self.is_js_file() {
            return false;
        }

        let target_idx = self.ctx.arena.skip_parenthesized(target_idx);
        let Some(target_node) = self.ctx.arena.get(target_idx) else {
            return false;
        };
        if target_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        if let Some(member_sym_id) = self.resolve_qualified_symbol(target_idx)
            && let Some(member_symbol) = self
                .get_cross_file_symbol(member_sym_id)
                .or_else(|| self.ctx.binder.get_symbol(member_sym_id))
            && member_symbol.has_any_flags(symbol_flags::ENUM)
        {
            let parent_sym_id = member_symbol.parent;
            if let Some(parent_symbol) = self
                .get_cross_file_symbol(parent_sym_id)
                .or_else(|| self.ctx.binder.get_symbol(parent_sym_id))
                && parent_symbol.has_any_flags(
                    symbol_flags::MODULE | symbol_flags::NAMESPACE | symbol_flags::NAMESPACE_MODULE,
                )
                && !parent_symbol.has_any_flags(symbol_flags::ENUM)
            {
                return true;
            }
        }

        let Some(access) = self.ctx.arena.get_access_expr(target_node) else {
            return false;
        };

        if let Some(enum_sym_id) = self.resolve_qualified_symbol(access.expression)
            && let Some(enum_symbol) = self
                .get_cross_file_symbol(enum_sym_id)
                .or_else(|| self.ctx.binder.get_symbol(enum_sym_id))
            && enum_symbol.has_any_flags(symbol_flags::ENUM)
            && !enum_symbol.has_any_flags(symbol_flags::ENUM_MEMBER)
        {
            // If the property being assigned is a declared member of this enum,
            // this is an enum member assignment (should get TS2540), not a rebind.
            if self.is_declared_enum_member_property(
                enum_sym_id,
                &self
                    .ctx
                    .arena
                    .get_identifier_at(access.name_or_argument)
                    .map(|ident| ident.escaped_text.clone())
                    .unwrap_or_default(),
            ) {
                return false;
            }
            let parent_sym_id = enum_symbol.parent;
            if let Some(parent_symbol) = self
                .get_cross_file_symbol(parent_sym_id)
                .or_else(|| self.ctx.binder.get_symbol(parent_sym_id))
                && parent_symbol.has_any_flags(
                    symbol_flags::MODULE | symbol_flags::NAMESPACE | symbol_flags::NAMESPACE_MODULE,
                )
                && !parent_symbol.has_any_flags(symbol_flags::ENUM)
            {
                return true;
            }
        }

        let Some(prop_ident) = self.ctx.arena.get_identifier_at(access.name_or_argument) else {
            return false;
        };

        let Some(base_sym_id) = self
            .resolve_identifier_symbol(access.expression)
            .or_else(|| self.resolve_qualified_symbol(access.expression))
        else {
            return false;
        };
        let Some(base_symbol) = self
            .get_cross_file_symbol(base_sym_id)
            .or_else(|| self.ctx.binder.get_symbol(base_sym_id))
        else {
            return false;
        };
        if !base_symbol.has_any_flags(
            symbol_flags::MODULE | symbol_flags::NAMESPACE | symbol_flags::NAMESPACE_MODULE,
        ) {
            return false;
        }

        let Some(exports) = base_symbol.exports.as_ref() else {
            return false;
        };
        let Some(member_sym_id) = exports.get(prop_ident.escaped_text.as_str()) else {
            return false;
        };
        let Some(member_symbol) = self
            .get_cross_file_symbol(member_sym_id)
            .or_else(|| self.ctx.binder.get_symbol(member_sym_id))
        else {
            return false;
        };

        member_symbol.has_any_flags(symbol_flags::ENUM)
    }

    pub(crate) fn is_js_namespace_enum_expando_member_assignment(
        &self,
        target_idx: NodeIndex,
    ) -> bool {
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        if !self.is_js_file() || !self.ctx.compiler_options.check_js {
            return false;
        }

        let target_idx = self.ctx.arena.skip_parenthesized(target_idx);
        let Some(target_node) = self.ctx.arena.get(target_idx) else {
            return false;
        };
        if target_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.ctx.arena.get_access_expr(target_node) else {
            return false;
        };
        let Some(prop_name) = self
            .ctx
            .arena
            .get_identifier_at(access.name_or_argument)
            .map(|ident| ident.escaped_text.clone())
        else {
            return false;
        };

        let enum_sym_id = self
            .resolve_qualified_symbol(access.expression)
            .or_else(|| {
                let root_node = self.ctx.arena.get(access.expression)?;
                if root_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                    return None;
                }
                let root_access = self.ctx.arena.get_access_expr(root_node)?;
                let namespace_sym_id = self
                    .resolve_identifier_symbol(root_access.expression)
                    .or_else(|| self.resolve_qualified_symbol(root_access.expression))?;
                let namespace_symbol = self
                    .get_cross_file_symbol(namespace_sym_id)
                    .or_else(|| self.ctx.binder.get_symbol(namespace_sym_id))?;
                let member_name = self
                    .ctx
                    .arena
                    .get_identifier_at(root_access.name_or_argument)?
                    .escaped_text
                    .as_str();
                namespace_symbol.exports.as_ref()?.get(member_name)
            });
        let Some(enum_sym_id) = enum_sym_id else {
            return false;
        };
        let Some(enum_symbol) = self
            .get_cross_file_symbol(enum_sym_id)
            .or_else(|| self.ctx.binder.get_symbol(enum_sym_id))
        else {
            return false;
        };
        if !enum_symbol.has_any_flags(symbol_flags::ENUM)
            || enum_symbol.has_any_flags(symbol_flags::ENUM_MEMBER)
        {
            return false;
        }

        let Some(parent_symbol) = self
            .get_cross_file_symbol(enum_symbol.parent)
            .or_else(|| self.ctx.binder.get_symbol(enum_symbol.parent))
        else {
            return false;
        };
        if !parent_symbol.has_any_flags(
            symbol_flags::MODULE | symbol_flags::NAMESPACE | symbol_flags::NAMESPACE_MODULE,
        ) || parent_symbol.has_any_flags(symbol_flags::ENUM)
        {
            return false;
        }

        let Some(object_key) =
            property_access_chain_text_in_arena(self.ctx.arena, access.expression)
        else {
            return false;
        };

        if self.is_js_namespace_enum_rebind_assignment_target(access.expression) {
            // If the property being assigned is already a declared enum member,
            // this is NOT an expando — it's an assignment to a readonly enum member.
            // Let the normal readonly check (TS2540) handle it instead of suppressing.
            if self.is_declared_enum_member_property(enum_sym_id, &prop_name) {
                return false;
            }
            return true;
        }

        let has_root_prop = self
            .collect_expando_properties_for_root(&object_key)
            .contains(&prop_name);
        let has_last_segment_prop = object_key
            .rsplit_once('.')
            .is_some_and(|(_, last_segment)| {
                self.collect_expando_properties_for_root(last_segment)
                    .contains(&prop_name)
            });
        has_root_prop || has_last_segment_prop
    }

    /// Check if `prop_name` is a declared member of the enum identified by `enum_sym_id`.
    ///
    /// Used to distinguish genuine expando property assignments from assignments to
    /// existing (readonly) enum members. For example, given:
    /// ```ts
    /// declare namespace lf { export enum Order { ASC, DESC } }
    /// ```
    /// `lf.Order.DESC = 0` targets the declared member `DESC`, not a new expando property.
    fn is_declared_enum_member_property(
        &self,
        enum_sym_id: tsz_binder::SymbolId,
        prop_name: &str,
    ) -> bool {
        use tsz_binder::symbol_flags;

        let enum_symbol = self
            .get_cross_file_symbol(enum_sym_id)
            .or_else(|| self.ctx.binder.get_symbol(enum_sym_id));
        let Some(enum_symbol) = enum_symbol else {
            return false;
        };
        let Some(exports) = enum_symbol.exports.as_ref() else {
            return false;
        };
        let Some(member_sym_id) = exports.get(prop_name) else {
            return false;
        };
        let member_symbol = self
            .get_cross_file_symbol(member_sym_id)
            .or_else(|| self.ctx.binder.get_symbol(member_sym_id));
        member_symbol.is_some_and(|sym| sym.has_any_flags(symbol_flags::ENUM_MEMBER))
    }

    pub(crate) fn error_top_level_js_this_computed_element_assignment(
        &mut self,
        target_idx: NodeIndex,
    ) -> bool {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        if !self.is_js_file() || !self.ctx.no_implicit_any() {
            return false;
        }

        let target_idx = self.ctx.arena.skip_parenthesized(target_idx);
        let Some(target_node) = self.ctx.arena.get(target_idx) else {
            return false;
        };
        if target_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(target_node) else {
            return false;
        };
        if self
            .ctx
            .arena
            .get(access.expression)
            .is_none_or(|node| node.kind != SyntaxKind::ThisKeyword as u16)
        {
            return false;
        }
        if self.ctx.enclosing_class.is_some()
            || self
                .find_enclosing_non_arrow_function(access.expression)
                .is_some()
        {
            return false;
        }
        if self
            .get_literal_string_from_node(access.name_or_argument)
            .is_some()
        {
            return false;
        }

        let index_type = self.get_type_of_node(access.name_or_argument);
        let index_type = self.format_type_diagnostic(index_type);
        self.error_at_node(
            target_idx,
            &format_message(
                diagnostic_messages::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN,
                &[&index_type, "typeof globalThis"],
            ),
            diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN,
        );
        true
    }

    pub(crate) fn error_invalid_commonjs_export_property_assignment(
        &mut self,
        target_idx: NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        if !self.is_js_file() {
            return false;
        }

        let target_idx = self.ctx.arena.skip_parenthesized(target_idx);
        let Some(target_node) = self.ctx.arena.get(target_idx) else {
            return false;
        };
        if target_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && target_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(target_node) else {
            return false;
        };
        if !self.is_current_file_commonjs_export_base(access.expression) {
            return false;
        }

        let surface = self.resolve_js_export_surface(self.ctx.current_file_idx);
        let direct_export_type = surface
            .direct_export_type
            .unwrap_or_else(|| self.get_type_of_node(access.expression));
        if crate::query_boundaries::js_exports::commonjs_direct_export_supports_named_props(
            self.ctx.types,
            direct_export_type,
        ) {
            return false;
        }

        let prop_name = match target_node.kind {
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => self
                .ctx
                .arena
                .get_identifier_at(access.name_or_argument)
                .map(|ident| ident.escaped_text.to_string()),
            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                self.current_file_commonjs_static_member_name(access.name_or_argument)
            }
            _ => None,
        };
        let Some(prop_name) = prop_name else {
            return false;
        };

        self.error_property_not_exist_at(&prop_name, direct_export_type, access.name_or_argument);
        true
    }
}
