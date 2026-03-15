//! Circular initializer/return-site helpers for variable checking.

use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    pub(super) fn consume_circular_return_sites_for_initializer(
        &mut self,
        sym_id: SymbolId,
        init_idx: NodeIndex,
    ) -> Vec<NodeIndex> {
        self.ctx
            .pending_circular_return_sites
            .remove(&sym_id)
            .unwrap_or_default()
            .into_iter()
            .filter(|&site_idx| {
                site_idx == init_idx || self.is_descendant_of_node(site_idx, init_idx)
            })
            .collect()
    }

    pub(super) fn suppress_circular_initializer_relation_diagnostics(
        &mut self,
        diag_start: usize,
        emitted_before: &rustc_hash::FxHashSet<(u32, u32)>,
        init_idx: NodeIndex,
    ) {
        let Some(init_node) = self.ctx.arena.get(init_idx) else {
            return;
        };
        let init_start = init_node.pos;
        let init_end = init_node.end;
        let kept_new_diags: Vec<_> = self.ctx.diagnostics[diag_start..]
            .iter()
            .filter(|diag| {
                let in_initializer = diag.start >= init_start && diag.start <= init_end;
                let is_downstream_relation_noise =
                    matches!(diag.code, 2322 | 2345 | 2769) && in_initializer;
                !is_downstream_relation_noise
            })
            .cloned()
            .collect();

        self.ctx.diagnostics.truncate(diag_start);
        self.ctx.diagnostics.extend(kept_new_diags.iter().cloned());
        self.ctx.emitted_diagnostics = emitted_before.clone();
        for diag in &kept_new_diags {
            self.ctx
                .emitted_diagnostics
                .insert(self.ctx.diagnostic_dedup_key(diag));
        }
    }

    pub(super) fn emit_circular_return_site_diagnostic(
        &mut self,
        site_idx: NodeIndex,
        var_name: Option<&str>,
        var_name_idx: NodeIndex,
        init_idx: NodeIndex,
    ) {
        use crate::diagnostics::diagnostic_codes;

        if site_idx == init_idx {
            if let Some(name) = var_name {
                self.error_at_node_msg(
                    var_name_idx,
                    diagnostic_codes::IMPLICITLY_HAS_RETURN_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_RETURN_TYPE_ANNOTATION,
                    &[name],
                );
            } else {
                self.error_at_node_msg(
                    site_idx,
                    diagnostic_codes::FUNCTION_IMPLICITLY_HAS_RETURN_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_RETURN_TYPE_A,
                    &[],
                );
            }
            return;
        }

        if let Some(ext) = self.ctx.arena.get_extended(site_idx) {
            let parent_idx = ext.parent;
            if let Some(parent_node) = self.ctx.arena.get(parent_idx) {
                match parent_node.kind {
                    syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                        if let Some(prop) = self.ctx.arena.get_property_assignment(parent_node)
                            && prop.initializer == site_idx
                            && let Some(name) = self.property_name_for_error(prop.name)
                        {
                            self.error_at_node_msg(
                                prop.name,
                                diagnostic_codes::IMPLICITLY_HAS_RETURN_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_RETURN_TYPE_ANNOTATION,
                                &[&name],
                            );
                            return;
                        }
                    }
                    syntax_kind_ext::METHOD_DECLARATION
                    | syntax_kind_ext::GET_ACCESSOR
                    | syntax_kind_ext::SET_ACCESSOR => {
                        if let Some(name_idx) = match parent_node.kind {
                            syntax_kind_ext::METHOD_DECLARATION => {
                                self.ctx.arena.get_method_decl(parent_node).map(|m| m.name)
                            }
                            _ => self.ctx.arena.get_accessor(parent_node).map(|a| a.name),
                        } && let Some(name) = self.property_name_for_error(name_idx)
                        {
                            self.error_at_node_msg(
                                name_idx,
                                diagnostic_codes::IMPLICITLY_HAS_RETURN_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_RETURN_TYPE_ANNOTATION,
                                &[&name],
                            );
                            return;
                        }
                    }
                    _ => {}
                }
            }
        }

        if let Some(site_node) = self.ctx.arena.get(site_idx)
            && let Some(func) = self.ctx.arena.get_function(site_node)
            && func.name.is_some()
            && let Some(name) = self.get_function_name_from_node(site_idx)
        {
            self.error_at_node_msg(
                func.name,
                diagnostic_codes::IMPLICITLY_HAS_RETURN_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_RETURN_TYPE_ANNOTATION,
                &[&name],
            );
            return;
        }

        self.error_at_node_msg(
            site_idx,
            diagnostic_codes::FUNCTION_IMPLICITLY_HAS_RETURN_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_RETURN_TYPE_A,
            &[],
        );
    }

    /// Check if the initializer has any self-references to `sym_id` that are NOT
    /// inside deferred contexts (getter/setter bodies, function/arrow bodies,
    /// method bodies, class bodies).
    ///
    /// Getter/setter bodies are lazily evaluated — a self-reference inside them
    /// (e.g., `const a = { get self() { return a; } }`) does not constitute
    /// a TS7022-worthy circularity because the getter runs after initialization.
    /// Similarly, function/method/class bodies are deferred.
    ///
    /// Returns `true` if there exists at least one self-reference OUTSIDE all
    /// deferred boundaries (i.e., the circularity is real and TS7022 should fire).
    pub(super) fn initializer_has_non_deferred_self_reference(
        &self,
        node_idx: NodeIndex,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        // Check if this node is an identifier referencing the target symbol
        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let ref_sym = self
                .ctx
                .binder
                .get_node_symbol(node_idx)
                .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, node_idx));
            return ref_sym == Some(sym_id);
        }

        // Stop at deferred boundaries — self-references inside these are benign
        if matches!(
            node.kind,
            syntax_kind_ext::FUNCTION_EXPRESSION
                | syntax_kind_ext::ARROW_FUNCTION
                | syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::METHOD_DECLARATION
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::CLASS_EXPRESSION
        ) {
            return false;
        }

        // Recurse into children
        for child_idx in self.ctx.arena.get_children(node_idx) {
            if self.initializer_has_non_deferred_self_reference(child_idx, sym_id) {
                return true;
            }
        }

        false
    }
}
