//! Conditional type, `infer` constraint, and `intrinsic` keyword validation helpers.

use crate::state::CheckerState;
use crate::types_domain::unique_symbol_arena::{
    is_unique_symbol_type_annotation_unwrapped, unwrap_parenthesized_type,
};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// TS2838: Check that all `infer X` declarations with the same name in a
    /// conditional type's extends clause have identical constraints.
    ///
    /// For example, `T extends { a: infer U extends string, b: infer U extends number }`
    /// should emit TS2838 because `U` has constraints `string` and `number`.
    pub(crate) fn check_infer_constraint_consistency(&mut self, extends_type: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;
        use std::collections::HashMap;

        let infer_decls = self.collect_infer_type_params_with_constraints(extends_type);
        if infer_decls.len() < 2 {
            return;
        }

        // Group by name: collect all (constraint, type_param_node) for each infer name
        let mut groups: HashMap<String, Vec<[NodeIndex; 2]>> = HashMap::new();
        for (name, constraint, tp_node) in &infer_decls {
            groups
                .entry(name.clone())
                .or_default()
                .push([*constraint, *tp_node]);
        }

        // For each duplicate group, check constraint consistency.
        // Only declarations with EXPLICIT constraints participate — unconstrained
        // `infer U` declarations inherit from the constrained ones (TSC behavior).
        for (name, entries) in &groups {
            if entries.len() < 2 {
                continue;
            }

            // Collect only entries that have an explicit constraint
            let constrained: Vec<(TypeId, NodeIndex)> = entries
                .iter()
                .filter(|pair| pair[0] != NodeIndex::NONE)
                .map(|pair| (self.get_type_from_type_node(pair[0]), pair[1]))
                .collect();

            // Need at least 2 explicitly constrained declarations to have a conflict
            if constrained.len() < 2 {
                continue;
            }

            // Check if all explicit constraints are identical
            let first_type = constrained[0].0;
            let all_identical = constrained
                .iter()
                .all(|(type_id, _)| *type_id == first_type);

            if !all_identical {
                // Emit TS2838 at each explicitly constrained declaration site
                for (_, tp_node) in &constrained {
                    self.error_at_node_msg(
                        *tp_node,
                        diagnostic_codes::ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_CONSTRAINTS,
                        &[name],
                    );
                }
            }
        }
    }

    pub(crate) fn check_unique_symbol_in_conditional_extends(&mut self, extends_type: NodeIndex) {
        if is_unique_symbol_type_annotation_unwrapped(self.ctx.arena, extends_type) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            let type_idx = unwrap_parenthesized_type(self.ctx.arena, extends_type);
            self.error_at_node(
                type_idx,
                diagnostic_messages::UNIQUE_SYMBOL_TYPES_ARE_NOT_ALLOWED_HERE,
                diagnostic_codes::UNIQUE_SYMBOL_TYPES_ARE_NOT_ALLOWED_HERE,
            );
        }
    }

    /// Check if a node is a bare `intrinsic` type reference (no type args, no qualification,
    /// not inside parentheses).
    ///
    /// Returns true when `node_idx` points to a `TYPE_REFERENCE` whose `type_name` is a simple
    /// `IDENTIFIER` with text `"intrinsic"` and no type arguments.
    ///
    /// TSC treats `intrinsic` as a keyword only when it appears directly as the type alias body
    /// (e.g., `type Uppercase<S extends string> = intrinsic`). When parenthesized like
    /// `type TE1 = (intrinsic)`, TSC treats it as a regular identifier reference (TS2304).
    /// Since our parser doesn't create a `PARENTHESIZED_TYPE` wrapper node, we detect
    /// parenthesization by checking if the character before the type reference in the source
    /// is `(`.
    pub(crate) fn is_bare_intrinsic_type_ref(&self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return false;
        };
        // Must have no type arguments
        if type_ref.type_arguments.is_some() {
            return false;
        }
        // type_name must be a simple IDENTIFIER (not QUALIFIED_NAME)
        let Some(name_node) = self.ctx.arena.get(type_ref.type_name) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        if ident.escaped_text != "intrinsic" {
            return false;
        }
        // Check that it's not parenthesized: look at the source text before the
        // type reference's position. If the nearest non-whitespace character is `(`,
        // the reference is parenthesized and should NOT be treated as the keyword.
        if let Some(sf) = self.ctx.arena.source_files.first() {
            let pos = node.pos as usize;
            if pos > 0 {
                let before = &sf.text[..pos];
                let last_non_ws = before
                    .bytes()
                    .rev()
                    .find(|&b| b != b' ' && b != b'\t' && b != b'\n' && b != b'\r');
                if last_non_ws == Some(b'(') {
                    return false;
                }
            }
        }
        true
    }

    /// Whether `name_idx` is `intrinsic` in a full type-alias `TypeReference` body.
    /// In that position tsc treats `intrinsic` as a keyword and reports TS2795
    /// (or accepts it for the four built-in string mapping aliases) — name
    /// resolution must not also fire TS2304.
    pub(crate) fn is_intrinsic_keyword_in_type_alias_body(&self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        if ident.escaped_text != "intrinsic" {
            return false;
        }
        let Some(name_ext) = self.ctx.arena.get_extended(name_idx) else {
            return false;
        };
        let type_ref_idx = name_ext.parent;
        if type_ref_idx.is_none() {
            return false;
        }
        if !self.is_bare_intrinsic_type_ref(type_ref_idx) {
            return false;
        }
        let Some(type_ref_ext) = self.ctx.arena.get_extended(type_ref_idx) else {
            return false;
        };
        let Some(parent_node) = self.ctx.arena.get(type_ref_ext.parent) else {
            return false;
        };
        parent_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
    }

    /// Check if a type node subtree references any resolving type.
    /// Used to detect TS2577 "Return type annotation circularly references itself".
    pub(crate) fn type_node_contains_circular_reference(&self, type_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return false;
        };
        match node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.ctx.arena.get_type_ref(node) {
                    if let Some(sym_id) = self
                        .resolve_type_symbol_for_lowering(type_ref.type_name)
                        .map(tsz_binder::SymbolId)
                    {
                        // Only flag direct self-references (the type reference
                        // directly names a type alias currently being resolved).
                        // Transitive references through other type aliases (e.g.,
                        // `type Op<I,O> = (x: Thing<I>) => Thing<O>` where Thing
                        // references Op) are valid mutual recursion and should NOT
                        // trigger TS2577. tsc only emits TS2577 for the case
                        // where a function's return type annotation directly names
                        // the same type being defined (e.g., `type F = () => F`).
                        if self.ctx.symbol_resolution_set.contains(&sym_id) {
                            return true;
                        }
                    }
                    // Also check type arguments
                    if let Some(args) = type_ref.type_arguments.as_ref() {
                        for &arg_idx in &args.nodes {
                            if self.type_node_contains_circular_reference(arg_idx) {
                                return true;
                            }
                        }
                    }
                }
                false
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                if let Some(tuple) = self.ctx.arena.get_tuple_type(node) {
                    for &elem_idx in &tuple.elements.nodes {
                        if self.type_node_contains_circular_reference(elem_idx) {
                            return true;
                        }
                    }
                }
                false
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(arr) = self.ctx.arena.get_array_type(node) {
                    return self.type_node_contains_circular_reference(arr.element_type);
                }
                false
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                    for &member_idx in &composite.types.nodes {
                        if self.type_node_contains_circular_reference(member_idx) {
                            return true;
                        }
                    }
                }
                false
            }
            k if k == syntax_kind_ext::REST_TYPE
                || k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::PARENTHESIZED_TYPE =>
            {
                if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
                    return self.type_node_contains_circular_reference(wrapped.type_node);
                }
                false
            }
            _ => false,
        }
    }
}
