//! Same-scope `this.prop = …` write tracking for property-read resolution.
//!
//! Split from `expando.rs` to keep it under the repo's 2000-line ceiling.
//! Covers `this`/`this`-alias detection, the eligibility gate (real class
//! instance receivers only), and the scan for a prior same-scope write.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::types_domain) fn prior_js_this_property_assignment_type(
        &mut self,
        property_access_idx: NodeIndex,
        property_name: &str,
    ) -> Option<TypeId> {
        let scope_root = self.find_enclosing_function_or_source_file(property_access_idx);
        let read_pos = self.ctx.arena.get(property_access_idx)?.pos;
        let mut best_match: Option<(u32, TypeId)> = None;
        self.collect_prior_js_this_property_assignment_type(
            scope_root,
            scope_root,
            property_name,
            read_pos,
            &mut best_match,
        );
        best_match.map(|(_, ty)| ty)
    }

    pub(in crate::types_domain) fn js_object_expr_is_this_or_alias(&self, idx: NodeIndex) -> bool {
        self.this_alias_root_node(idx).is_some()
    }

    /// Resolves `idx` (a bare `this`, or an identifier aliasing one via
    /// `const self = this;`) to the underlying `this`-keyword node.
    fn this_alias_root_node(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind == SyntaxKind::ThisKeyword as u16 {
            return Some(idx);
        }
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self.resolve_identifier_symbol(idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl_node = self.ctx.arena.get(symbol.value_declaration)?;
        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        let init_node = self.ctx.arena.get(var_decl.initializer)?;
        (init_node.kind == SyntaxKind::ThisKeyword as u16).then_some(var_decl.initializer)
    }

    /// Whether a `this.<prop>` (or aliased-`this`) receiver genuinely binds to
    /// a real class instance — the only shape tsc infers members for from a
    /// same-scope prior `this.prop = …` write. `typeof globalThis` and a
    /// post-TS7 `@constructor` function's implicit-`any` `this` are excluded
    /// (oracle-verified against `typescript@7.0.2`): both keep re-reporting
    /// their own missing-member/implicit-any diagnostic instead.
    pub(in crate::types_domain) fn this_property_assignment_receiver_is_class_instance(
        &mut self,
        object_expr_idx: NodeIndex,
    ) -> bool {
        let Some(this_idx) = self.this_alias_root_node(object_expr_idx) else {
            return false;
        };
        !self.is_this_in_nested_function_without_own_this_binding(this_idx)
            && self
                .nearest_enclosing_class_for_this_binding(this_idx)
                .is_some()
    }

    fn collect_prior_js_this_property_assignment_type(
        &mut self,
        idx: NodeIndex,
        scope_root: NodeIndex,
        property_name: &str,
        read_pos: u32,
        best_match: &mut Option<(u32, TypeId)>,
    ) {
        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };

        if idx != scope_root
            && (self.is_scope_owner_kind(node.kind)
                || node.kind == syntax_kind_ext::CLASS_DECLARATION)
        {
            return;
        }

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
            && binary.operator_token == SyntaxKind::EqualsToken as u16
            && node.pos < read_pos
            && self
                .js_this_assignment_target_name(binary.left)
                .is_some_and(|name| name == property_name)
        {
            let rhs_idx = self.ctx.arena.skip_parenthesized(binary.right);
            let rhs_type = self.get_type_of_node(rhs_idx);
            if rhs_type != TypeId::ANY
                && rhs_type != TypeId::ERROR
                && best_match.is_none_or(|(best_pos, _)| node.pos >= best_pos)
            {
                *best_match = Some((node.pos, rhs_type));
            }
        }

        for child_idx in self.ctx.arena.get_children(idx) {
            self.collect_prior_js_this_property_assignment_type(
                child_idx,
                scope_root,
                property_name,
                read_pos,
                best_match,
            );
        }
    }

    fn js_this_assignment_target_name(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        match node.kind {
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                let object_node = self.ctx.arena.get(access.expression)?;
                if object_node.kind != SyntaxKind::ThisKeyword as u16 {
                    return None;
                }
                self.ctx
                    .arena
                    .get_identifier_at(access.name_or_argument)
                    .map(|ident| ident.escaped_text.to_string())
            }
            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                let object_node = self.ctx.arena.get(access.expression)?;
                if object_node.kind != SyntaxKind::ThisKeyword as u16 {
                    return None;
                }
                self.current_file_commonjs_static_member_name(access.name_or_argument)
            }
            _ => None,
        }
    }
}
