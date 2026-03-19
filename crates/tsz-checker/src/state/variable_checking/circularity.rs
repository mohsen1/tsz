//! Circular initializer/return-site helpers for variable checking.

use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    fn identifier_is_non_value_name_position(&self, node_idx: NodeIndex) -> bool {
        let Some(ext) = self.ctx.arena.get_extended(node_idx) else {
            return false;
        };
        let Some(parent_node) = self.ctx.arena.get(ext.parent) else {
            return false;
        };

        match parent_node.kind {
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => self
                .ctx
                .arena
                .get_access_expr(parent_node)
                .is_some_and(|access| access.name_or_argument == node_idx),
            syntax_kind_ext::QUALIFIED_NAME => self
                .ctx
                .arena
                .get_qualified_name(parent_node)
                .is_some_and(|qualified| qualified.right == node_idx),
            syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                .ctx
                .arena
                .get_property_assignment(parent_node)
                .is_some_and(|prop| prop.name == node_idx),
            syntax_kind_ext::METHOD_DECLARATION => self
                .ctx
                .arena
                .get_method_decl(parent_node)
                .is_some_and(|method| method.name == node_idx),
            syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => self
                .ctx
                .arena
                .get_accessor(parent_node)
                .is_some_and(|accessor| accessor.name == node_idx),
            _ => false,
        }
    }

    pub(super) fn take_pending_circular_return_sites(
        &mut self,
        sym_id: SymbolId,
    ) -> Vec<NodeIndex> {
        self.ctx
            .pending_circular_return_sites
            .remove(&sym_id)
            .unwrap_or_default()
    }

    pub(super) fn consume_circular_return_sites_for_initializer(
        &mut self,
        sym_id: SymbolId,
        init_idx: NodeIndex,
    ) -> Vec<NodeIndex> {
        self.take_pending_circular_return_sites(sym_id)
            .into_iter()
            .filter(|&site_idx| {
                site_idx == init_idx || self.is_descendant_of_node(site_idx, init_idx)
            })
            .collect()
    }

    pub(super) fn retain_immediate_initializer_circular_return_sites(
        &self,
        init_idx: NodeIndex,
        sites: Vec<NodeIndex>,
    ) -> Vec<NodeIndex> {
        sites
            .into_iter()
            .filter(|&site_idx| {
                self.circular_return_site_requires_initializer_inference(site_idx, init_idx)
            })
            .collect()
    }

    pub(super) fn suppress_circular_initializer_relation_diagnostics(
        &mut self,
        snap: &crate::context::speculation::DiagnosticSnapshot,
        init_idx: NodeIndex,
    ) {
        let Some(init_node) = self.ctx.arena.get(init_idx) else {
            return;
        };
        let init_start = init_node.pos;
        let init_end = init_node.end;
        self.ctx.rollback_diagnostics_filtered(snap, |diag| {
            let in_initializer = diag.start >= init_start && diag.start <= init_end;
            let is_downstream_relation_noise =
                matches!(diag.code, 2322 | 2345 | 2769) && in_initializer;
            !is_downstream_relation_noise
        });
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

    fn circular_return_site_requires_initializer_inference(
        &self,
        site_idx: NodeIndex,
        init_idx: NodeIndex,
    ) -> bool {
        if site_idx == init_idx {
            return true;
        }

        let mut current = site_idx;
        loop {
            if current == init_idx {
                return false;
            }

            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return false;
            }

            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            if matches!(
                parent_node.kind,
                syntax_kind_ext::CALL_EXPRESSION
                    | syntax_kind_ext::NEW_EXPRESSION
                    | syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
            ) {
                return true;
            }

            current = parent_idx;
        }
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
            if self.identifier_is_non_value_name_position(node_idx) {
                return false;
            }
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

    pub(super) fn initializer_has_non_deferred_self_reference_by_name(
        &self,
        node_idx: NodeIndex,
        name: &str,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        if let Some(ident) = self.ctx.arena.get_identifier(node) {
            if self.identifier_is_non_value_name_position(node_idx) {
                return false;
            }
            return ident.escaped_text == name;
        }

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

        for child_idx in self.ctx.arena.get_children(node_idx) {
            if self.initializer_has_non_deferred_self_reference_by_name(child_idx, name) {
                return true;
            }
        }

        false
    }
}
