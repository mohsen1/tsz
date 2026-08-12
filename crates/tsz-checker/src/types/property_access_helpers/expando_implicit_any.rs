//! TS7008 for a JS expando/special-declaration member whose only same-file
//! assignments are implicit-any placeholders (`void 0`, bare `undefined`, or
//! `null`).
//!
//! Extracted from `expando.rs` to keep that shard under the line-count limit.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    /// `x.y = void 0` / `x.y = undefined` / `x.y = null` DECLARES the member
    /// in `tsc` (verified against the pinned `typescript@7.0.2` oracle: the
    /// member is visible to later reads with no `TS2339`), but contributes no
    /// concrete type. When every same-file assignment to the member has one
    /// of these implicit-any shapes, `tsc` reports `TS7008` once, at the
    /// first such assignment — unconditionally in checked JS, independent of
    /// `noImplicitAny`. A CommonJS export member
    /// (`module.exports.x`/`exports.x`) is excluded: that family already has
    /// its own dedicated implicit-any diagnostic
    /// (`maybe_report_commonjs_export_implicit_any_assignment`, `TS7005`,
    /// gated on `noImplicitAny`), and reporting here too would duplicate it
    /// under a different code.
    pub(crate) fn maybe_report_js_expando_implicit_any_assignment(
        &mut self,
        target_idx: NodeIndex,
        right_idx: NodeIndex,
        expr_idx: NodeIndex,
    ) {
        if !self.is_js_file() || !self.ctx.compiler_options.check_js {
            return;
        }

        let target_idx = self.ctx.arena.skip_parenthesized(target_idx);
        let Some(target_node) = self.ctx.arena.get(target_idx) else {
            return;
        };
        if target_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return;
        }
        if !Self::is_implicit_any_expando_rhs_in_arena(self.ctx.arena, right_idx) {
            return;
        }
        let Some(access) = self.ctx.arena.get_access_expr(target_node) else {
            return;
        };
        let Some(property_name) = self
            .ctx
            .arena
            .get_identifier_at(access.name_or_argument)
            .map(|ident| ident.escaped_text.to_string())
        else {
            return;
        };
        if self
            .current_file_commonjs_export_member_name(target_idx)
            .is_some()
        {
            return;
        }
        let Some(root_name) =
            Self::property_access_chain_in_arena(self.ctx.arena, access.expression)
        else {
            return;
        };

        let object_type = self.get_type_of_node(access.expression);
        if !self.is_js_expando_object_assignment(
            target_idx,
            access.expression,
            object_type,
            &property_name,
        ) {
            return;
        }

        let Some((first_pos, all_implicit_any)) =
            self.expando_property_implicit_any_survey(&root_name, &property_name)
        else {
            return;
        };
        if !all_implicit_any {
            return;
        }
        // Report once, at the first assignment — matching tsc. Later
        // assignments to the same member skip silently.
        if self
            .ctx
            .arena
            .get(expr_idx)
            .is_none_or(|node| node.pos != first_pos)
        {
            return;
        }

        self.error_at_node_msg(
            target_idx,
            crate::diagnostics::diagnostic_codes::MEMBER_IMPLICITLY_HAS_AN_TYPE,
            &[&property_name, "any"],
        );
    }

    fn is_implicit_any_expando_rhs_in_arena(
        arena: &tsz_parser::parser::NodeArena,
        idx: NodeIndex,
    ) -> bool {
        if Self::is_void_zero_or_undefined_rhs_in_arena(arena, idx) {
            return true;
        }
        arena
            .get(idx)
            .is_some_and(|node| node.kind == SyntaxKind::NullKeyword as u16)
    }

    /// Scans this file's statements for every `root_name.property_name =
    /// <rhs>` assignment, mirroring the reach of
    /// `collect_expando_property_assignment_type` (file/block scope only, no
    /// nested-function assignments). Returns the source position of the
    /// first assignment together with whether every assignment found has an
    /// implicit-any RHS shape. `None` when there is no same-file assignment
    /// to the property.
    fn expando_property_implicit_any_survey(
        &self,
        root_name: &str,
        property_name: &str,
    ) -> Option<(u32, bool)> {
        let expected_key = format!("{root_name}.{property_name}");
        let source_file = self
            .ctx
            .arena
            .source_files
            .get(self.ctx.current_file_idx)
            .or_else(|| self.ctx.arena.source_files.first())?;
        let mut first_pos: Option<u32> = None;
        let mut all_implicit_any = true;
        for &stmt_idx in &source_file.statements.nodes {
            self.collect_expando_property_implicit_any_survey(
                stmt_idx,
                &expected_key,
                &mut first_pos,
                &mut all_implicit_any,
            );
        }
        first_pos.map(|pos| (pos, all_implicit_any))
    }

    fn collect_expando_property_implicit_any_survey(
        &self,
        idx: NodeIndex,
        expected_key: &str,
        first_pos: &mut Option<u32>,
        all_implicit_any: &mut bool,
    ) {
        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };
        if self.is_scope_owner_kind(node.kind) || node.kind == syntax_kind_ext::CLASS_DECLARATION {
            return;
        }
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
            && binary.operator_token == SyntaxKind::EqualsToken as u16
            && Self::expando_assignment_access_key_in_arena(self.ctx.arena, binary.left)
                .is_some_and(|key| key == expected_key)
        {
            if first_pos.is_none() {
                *first_pos = Some(node.pos);
            }
            if !Self::is_implicit_any_expando_rhs_in_arena(self.ctx.arena, binary.right) {
                *all_implicit_any = false;
            }
        }
        for child_idx in self.ctx.arena.get_children(idx) {
            self.collect_expando_property_implicit_any_survey(
                child_idx,
                expected_key,
                first_pos,
                all_implicit_any,
            );
        }
    }
}
