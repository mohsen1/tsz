//! Interface merge compatibility, declaration name matching, property name
//! utilities, and node containment checks.

use crate::state::{CheckerState, ComputedKey, MAX_TREE_WALK_ITERATIONS, PropertyKey};
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

/// Per-position record for a declaration's type parameter list.
///
/// The interner guarantees `Atom`-equality iff string-equality, so a single
/// `name` atom serves both cross-declaration name comparison and the
/// positional canonicalization scope used to align self-referential
/// constraints across declarations.
#[derive(Clone, Debug)]
struct TypeParamProfileEntry {
    name: Atom,
    constraint: Option<TypeId>,
    default: Option<TypeId>,
}

impl<'a> CheckerState<'a> {
    /// Compare interface type parameters across declarations for declaration-merge compatibility.
    ///
    /// - Parameter names must match and appear in the same order.
    /// - Parameter constraints must be identical (TypeId equality) when both are present.
    ///   TSC uses `isTypeIdenticalTo`, not assignability — this matters for `any` constraints.
    /// - Missing constraints are compatible with any constraint (e.g. `T` vs `T extends number`).
    /// - Self-referential constraints (e.g. `T extends Foo<T>`) are canonicalized in a
    ///   shared positional type-parameter scope before comparison, so `<T extends Foo<T>>`
    ///   declared in declaration A and `<T extends Foo<T>>` declared in declaration B compare
    ///   equal even though each declaration's `T` resolves to a distinct underlying `TypeId`.
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

        // Names must match positionally before any constraint canonicalization is
        // meaningful: a `<T>` vs `<S>` pair is structurally distinct under tsc's
        // declaration-merge rule regardless of the constraint content.
        for (a, b) in first_profile.iter().zip(second_profile.iter()) {
            if a.name != b.name {
                return false;
            }
        }

        let scope = Self::profile_param_scope(&first_profile);
        self.constraints_and_defaults_match_in_scope(&first_profile, &second_profile, &scope)
    }

    /// Check if ALL interface declarations in a merge group have compatible
    /// type parameters.
    ///
    /// tsc considers the merge group as a whole: a type parameter position has a
    /// default if ANY declaration in the group provides one.  Shorter declarations
    /// are compatible with longer ones when the extra positions all have defaults
    /// somewhere in the group.  Overlapping positions must agree on names (and on
    /// constraints/defaults when both declarations provide them).
    pub(crate) fn interface_type_parameters_are_group_merge_compatible(
        &mut self,
        decls: &[NodeIndex],
    ) -> bool {
        let profiles: Vec<Vec<TypeParamProfileEntry>> = decls
            .iter()
            .filter_map(|&d| self.interface_type_parameter_profile(d))
            .collect();
        if profiles.len() < 2 {
            return true;
        }

        let max_len = profiles.iter().map(|p| p.len()).max().unwrap_or(0);

        for pos in 0..max_len {
            let all_have_pos = profiles.iter().all(|p| p.len() > pos);
            if !all_have_pos {
                let has_default = profiles
                    .iter()
                    .any(|p| p.get(pos).is_some_and(|entry| entry.default.is_some()));
                if !has_default {
                    return false;
                }
            }
        }

        // Build the positional canonicalization scope from the longest profile so
        // every name reachable from any declaration in the group has an anchor.
        // Names agree at overlapping positions (rechecked pairwise below), and
        // since the scope is keyed by name+position, picking any profile of the
        // maximum length produces the same scope content — the comparison is
        // symmetric regardless of which longest-tied profile is chosen.
        let longest = profiles
            .iter()
            .max_by_key(|p| p.len())
            .expect("profiles non-empty");
        let scope = Self::profile_param_scope(longest);

        for i in 0..profiles.len() {
            for j in (i + 1)..profiles.len() {
                for (entry_i, entry_j) in profiles[i].iter().zip(profiles[j].iter()) {
                    if entry_i.name != entry_j.name {
                        return false;
                    }
                }
                if !self.constraints_and_defaults_match_in_scope(&profiles[i], &profiles[j], &scope)
                {
                    return false;
                }
            }
        }

        true
    }

    /// Check if class+interface merged declarations have compatible type
    /// parameters. Unlike the interface-only check, this allows different
    /// arity when the declaration with more params has defaults for the
    /// extras (e.g., React Component pattern: class<P,S> + interface<P,S,SS=any>).
    /// Only the overlapping parameters are checked for name+constraint identity.
    pub(crate) fn class_interface_type_parameters_are_merge_compatible(
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

        // Check the overlapping portion only
        let min_len = first_profile.len().min(second_profile.len());

        // Names must match in overlapping positions before canonicalization.
        for i in 0..min_len {
            if first_profile[i].name != second_profile[i].name {
                return false;
            }
        }

        // Build the canonicalization scope from the longer profile so positions
        // present only on one side (e.g. defaulted extras on the interface side
        // of a class+interface merge) still have an anchor for the shorter
        // side's constraints to reference symmetrically.
        let longest = if first_profile.len() >= second_profile.len() {
            &first_profile
        } else {
            &second_profile
        };
        let scope = Self::profile_param_scope(longest);

        let overlap_first = &first_profile[..min_len];
        let overlap_second = &second_profile[..min_len];
        self.constraints_and_defaults_match_in_scope(overlap_first, overlap_second, &scope)
    }

    /// Collect type parameter names and constraint type ids from an interface
    /// or class declaration.
    fn interface_type_parameter_profile(
        &mut self,
        decl_idx: NodeIndex,
    ) -> Option<Vec<TypeParamProfileEntry>> {
        let node = self.ctx.arena.get(decl_idx)?;
        // Handle both interface and class declarations
        let list = if let Some(interface) = self.ctx.arena.get_interface(node) {
            match interface.type_parameters.as_ref() {
                Some(list) => list.clone(),
                None => return Some(Vec::new()),
            }
        } else {
            let class = self.ctx.arena.get_class(node)?;
            match class.type_parameters.as_ref() {
                Some(list) => list.clone(),
                None => return Some(Vec::new()),
            }
        };
        let list = &list;

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

            let default = if type_param.default != NodeIndex::NONE {
                Some(self.get_type_from_type_node(type_param.default))
            } else {
                None
            };

            let name_atom = self
                .ctx
                .types
                .intern_string(self.ctx.arena.resolve_identifier_text(param_name));

            profile.push(TypeParamProfileEntry {
                name: name_atom,
                constraint,
                default,
            });
        }

        Some(profile)
    }

    /// Collect the parameter-name scope used to canonicalize constraints and
    /// defaults across declarations in a merge group.
    ///
    /// Position 0 of the returned vector is the "innermost" parameter; the
    /// canonicalizer maps each name to its `BoundParameter(index)` De Bruijn
    /// index when it encounters the corresponding `TypeParameter(name)` in a
    /// canonicalized expression. Two declarations whose constraints reference
    /// positionally-equivalent parameters canonicalize to the same form.
    fn profile_param_scope(profile: &[TypeParamProfileEntry]) -> Vec<Atom> {
        profile.iter().map(|entry| entry.name).collect()
    }

    /// Apply the overlap-positional check that drives all three callers:
    /// for each shared position with both-sides constraints, both-sides
    /// defaults, the structural identity check must agree in `scope`.
    fn constraints_and_defaults_match_in_scope(
        &self,
        first: &[TypeParamProfileEntry],
        second: &[TypeParamProfileEntry],
        scope: &[Atom],
    ) -> bool {
        for (a, b) in first.iter().zip(second.iter()) {
            // TSC uses isTypeIdenticalTo for constraint comparison (not assignability).
            // Only mismatch when BOTH have constraints and they differ. Missing
            // constraints stay compatible with any constraint (`T` vs `T extends number`).
            if let (Some(ac), Some(bc)) = (a.constraint, b.constraint)
                && !self.types_identical_in_param_scope(ac, bc, scope)
            {
                return false;
            }

            // Defaults follow the same both-sides-or-skip policy.
            if let (Some(ad), Some(bd)) = (a.default, b.default)
                && !self.types_identical_in_param_scope(ad, bd, scope)
            {
                return false;
            }
        }
        true
    }

    /// Structural identity for two `TypeId`s under a shared outer
    /// type-parameter scope. Returns `true` when the two types canonicalize to
    /// the same form once positionally-equivalent parameter references have
    /// been collapsed to De Bruijn indices — the rule that makes `<T extends
    /// Foo<T>>` declared twice compare equal even though each declaration's
    /// own `T` is a distinct underlying `TypeId`.
    fn types_identical_in_param_scope(&self, a: TypeId, b: TypeId, scope: &[Atom]) -> bool {
        crate::query_boundaries::assignability::are_types_structurally_identical_in_param_scope(
            self.ctx.types,
            &self.ctx,
            a,
            b,
            scope,
        )
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
            PropertyKey::Private(s) => {
                // The scanner stores private identifiers with the `#` prefix already
                if s.starts_with('#') {
                    s.clone()
                } else {
                    format!("#{s}")
                }
            }
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
