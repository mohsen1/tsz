//! Interface merge compatibility, declaration name matching, property name
//! utilities, and node containment checks.
//!
//! Extracted from `type_checking_declarations.rs` for maintainability.

use crate::state::{CheckerState, ComputedKey, MAX_TREE_WALK_ITERATIONS, PropertyKey};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Compare interface type parameters across declarations for declaration-merge compatibility.
    ///
    /// - Parameter names must match and appear in the same order.
    /// - Parameter constraints must be mutually assignable when both are present.
    /// - Missing constraints are compatible with any constraint (e.g. `T` vs `T extends number`).
    pub(crate) fn interface_type_parameters_are_merge_compatible(
        &mut self,
        first: NodeIndex,
        second: NodeIndex,
    ) -> bool {
        let Some(first_profile) = self.interface_type_parameter_profile(first) else {
            return false;
        };
        let Some(second_profile) = self.interface_type_parameter_profile(second) else {
            return false;
        };

        if first_profile.len() != second_profile.len() {
            return false;
        }

        for i in 0..first_profile.len() {
            let (first_name, first_constraint) = &first_profile[i];
            let (second_name, second_constraint) = &second_profile[i];

            if first_name != second_name {
                return false;
            }

            if let (Some(first_constraint), Some(second_constraint)) =
                (first_constraint, second_constraint)
                && (!self.is_assignable_to(*first_constraint, *second_constraint)
                    || !self.is_assignable_to(*second_constraint, *first_constraint))
            {
                return false;
            }
        }

        true
    }

    /// Collect interface type parameter names and constraint type ids.
    fn interface_type_parameter_profile(
        &mut self,
        decl_idx: NodeIndex,
    ) -> Option<Vec<(String, Option<TypeId>)>> {
        let node = self.ctx.arena.get(decl_idx)?;
        let interface = self.ctx.arena.get_interface(node)?;
        let list = match interface.type_parameters.as_ref() {
            Some(list) => list,
            None => return Some(Vec::new()),
        };

        let mut profile = Vec::with_capacity(list.nodes.len());
        for &param_idx in &list.nodes {
            let param_node = self.ctx.arena.get(param_idx)?;
            let type_param = self.ctx.arena.get_type_parameter(param_node)?;
            let param_name_node = self.ctx.arena.get(type_param.name)?;
            let param_name = self.ctx.arena.get_identifier(param_name_node)?;

            let constraint = if type_param.constraint != NodeIndex::NONE {
                Some(self.get_type_from_type_node(type_param.constraint))
            } else {
                None
            };

            profile.push((
                self.ctx
                    .arena
                    .resolve_identifier_text(param_name)
                    .to_string(),
                constraint,
            ));
        }

        Some(profile)
    }

    /// Verify that a declaration node actually has a name matching the expected symbol name.
    /// This is used to filter out false matches when lib declarations' `NodeIndex` values
    /// overlap with user arena indices and point to unrelated user nodes.
    pub(crate) fn declaration_name_matches(
        &self,
        decl_idx: NodeIndex,
        expected_name: &str,
    ) -> bool {
        let Some(name_node_idx) = self.get_declaration_name_node(decl_idx) else {
            // For declarations without extractable names (methods, properties, constructors, etc.),
            // fall back to checking the node's identifier directly
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                return false;
            };
            match node.kind {
                syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.ctx.arena.get_method_decl(node)
                        && let Some(name_node) = self.ctx.arena.get(method.name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        return self.ctx.arena.resolve_identifier_text(ident) == expected_name;
                    }
                    return false;
                }
                syntax_kind_ext::PROPERTY_DECLARATION => {
                    if let Some(prop) = self.ctx.arena.get_property_decl(node)
                        && let Some(name_node) = self.ctx.arena.get(prop.name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        return self.ctx.arena.resolve_identifier_text(ident) == expected_name;
                    }
                    return false;
                }
                syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                    if let Some(accessor) = self.ctx.arena.get_accessor(node)
                        && let Some(name_node) = self.ctx.arena.get(accessor.name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        return self.ctx.arena.resolve_identifier_text(ident) == expected_name;
                    }
                    return false;
                }
                syntax_kind_ext::MODULE_DECLARATION => {
                    if let Some(module) = self.ctx.arena.get_module(node)
                        && let Some(name_node) = self.ctx.arena.get(module.name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        return self.ctx.arena.resolve_identifier_text(ident) == expected_name;
                    }
                    return false;
                }
                _ => return false,
            }
        };
        // Check the name node is an identifier with the expected name
        if let Some(ident) = self.ctx.arena.get_identifier_at(name_node_idx) {
            return self.ctx.arena.resolve_identifier_text(ident) == expected_name;
        }
        false
    }

    /// Convert a floating-point number to a numeric index.
    ///
    /// Returns Some(index) if the value is a valid non-negative integer, None otherwise.
    pub(crate) fn get_numeric_index_from_number(&self, value: f64) -> Option<usize> {
        if !value.is_finite() || value.fract() != 0.0 || value < 0.0 {
            return None;
        }
        if value > (usize::MAX as f64) {
            return None;
        }
        Some(value as usize)
    }

    // 21. Property Name Utilities

    /// Get the display string for a property key.
    ///
    /// Converts a `PropertyKey` enum into its string representation
    /// for use in error messages and diagnostics.
    pub(crate) fn get_property_name_from_key(&self, key: &PropertyKey) -> String {
        match key {
            PropertyKey::Ident(s) => s.clone(),
            PropertyKey::Computed(ComputedKey::Ident(s)) => {
                format!("[{s}]")
            }
            PropertyKey::Computed(ComputedKey::String(s)) => {
                format!("[\"{s}\"]")
            }
            PropertyKey::Computed(ComputedKey::Number(n)) => {
                format!("[{n}]")
            }
            PropertyKey::Private(s) => format!("#{s}"),
        }
    }

    /// Get the Symbol property name from an expression.
    ///
    /// Extracts the name from a `Symbol()` expression, e.g., Symbol("foo") -> "Symbol.foo".
    pub(crate) fn get_symbol_property_name_from_expr(&self, expr_idx: NodeIndex) -> Option<String> {
        use tsz_scanner::SyntaxKind;

        let node = self.ctx.arena.get(expr_idx)?;

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            let paren = self.ctx.arena.get_parenthesized(node)?;
            return self.get_symbol_property_name_from_expr(paren.expression);
        }

        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return None;
        }

        let access = self.ctx.arena.get_access_expr(node)?;
        let base_node = self.ctx.arena.get(access.expression)?;
        let base_ident = self.ctx.arena.get_identifier(base_node)?;
        if base_ident.escaped_text != "Symbol" {
            return None;
        }

        let name_node = self.ctx.arena.get(access.name_or_argument)?;
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            return Some(format!("[Symbol.{}]", ident.escaped_text));
        }

        if matches!(
            name_node.kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        ) && let Some(lit) = self.ctx.arena.get_literal(name_node)
            && !lit.text.is_empty()
        {
            return Some(format!("[Symbol.{}]", lit.text));
        }

        None
    }

    // 22. Node Containment

    /// Check if a node is within another node in the AST tree.
    ///
    /// Traverses up the parent chain to check if `node_idx` is a descendant
    /// of `root_idx`. Used for scope checking and containment analysis.
    pub(crate) fn is_node_within(&self, node_idx: NodeIndex, root_idx: NodeIndex) -> bool {
        if node_idx == root_idx {
            return true;
        }
        let mut current = node_idx;
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return false;
            }
            let ext = match self.ctx.arena.get_extended(current) {
                Some(ext) => ext,
                None => return false,
            };
            if ext.parent.is_none() {
                return false;
            }
            if ext.parent == root_idx {
                return true;
            }
            current = ext.parent;
        }
    }
}
