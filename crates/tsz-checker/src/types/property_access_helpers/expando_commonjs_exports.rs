//! CommonJS export member resolution for expando reads.
//!
//! Resolves `module.exports` / `exports.foo` static member names and detects
//! reads of CommonJS export bases that precede their assignment, feeding the
//! expando read-before-assignment checks in `CheckerState`.
//! Extracted verbatim from `expando.rs` to keep that shard under the size limit.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    pub(in crate::types_domain) fn current_file_commonjs_export_member_name(
        &self,
        idx: NodeIndex,
    ) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        match node.kind {
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                if !self.is_current_file_commonjs_export_base_for_expando(access.expression) {
                    return None;
                }
                self.ctx
                    .arena
                    .get_identifier_at(access.name_or_argument)
                    .map(|ident| ident.escaped_text.to_string())
            }
            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                if !self.is_current_file_commonjs_export_base_for_expando(access.expression) {
                    return None;
                }
                self.commonjs_static_member_name_for_expando(access.name_or_argument)
            }
            _ => None,
        }
    }

    /// A property/element access whose receiver chain is rooted at this file's
    /// CommonJS exports object with at least one intermediate member — i.e.
    /// `exports.X`, `module.exports.X`, `exports.X.Y`, and deeper. tsc 7.0.2
    /// treats such a member as a CLOSED value, never an expando host: a further
    /// write or read `<member>.Z` is a plain property access against the
    /// member's own type (TS2339 when absent) for every RHS shape (object,
    /// function, or class). The bare exports object itself (`exports` /
    /// `module.exports`) is NOT a member — direct writes `exports.X = ...` stay
    /// valid — so it returns `false`.
    pub(in crate::types_domain) fn is_current_file_commonjs_export_member_access(
        &self,
        idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return false;
        };
        self.is_current_file_commonjs_export_base_syntax(access.expression)
            || self.is_current_file_commonjs_export_member_access(access.expression)
    }

    pub(super) fn is_current_file_commonjs_export_base_for_expando(&self, idx: NodeIndex) -> bool {
        if self
            .ctx
            .js_export_surface_cache
            .get(&self.ctx.current_file_idx)
            .and_then(|surface| surface.direct_export_type)
            .is_some_and(|direct_export_type| {
                !crate::query_boundaries::js_exports::commonjs_direct_export_supports_named_props(
                    self.ctx.types,
                    direct_export_type,
                )
            })
        {
            return false;
        }

        self.is_current_file_commonjs_export_base_syntax(idx)
    }

    pub(crate) fn is_current_file_commonjs_export_base_syntax(&self, idx: NodeIndex) -> bool {
        if self.current_source_file_has_esm_syntax() {
            return false;
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            return self.is_unshadowed_commonjs_exports_identifier(idx);
        }

        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return false;
        };
        self.is_unshadowed_commonjs_module_identifier(access.expression)
            && self
                .ctx
                .arena
                .get_identifier_at(access.name_or_argument)
                .is_some_and(|ident| ident.escaped_text == "exports")
    }

    fn commonjs_static_member_name_for_expando(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                self.ctx.arena.get_literal(node).map(|lit| lit.text.clone())
            }
            _ => None,
        }
    }

    pub(super) fn commonjs_export_read_before_assignment(
        &self,
        property_access_idx: NodeIndex,
        property_name: &str,
    ) -> bool {
        let Some(read_node) = self.ctx.arena.get(property_access_idx) else {
            return false;
        };
        let read_pos = read_node.pos;
        let Some(source_file) = self.ctx.arena.source_files.first() else {
            return false;
        };

        let mut assigned_before = false;
        let mut assigned_after = false;
        for &stmt_idx in &source_file.statements.nodes {
            self.collect_commonjs_export_assignment_order(
                stmt_idx,
                property_name,
                read_pos,
                &mut assigned_before,
                &mut assigned_after,
            );
            if assigned_before && assigned_after {
                break;
            }
        }

        assigned_after && !assigned_before
    }

    fn collect_commonjs_export_assignment_order(
        &self,
        idx: NodeIndex,
        property_name: &str,
        read_pos: u32,
        assigned_before: &mut bool,
        assigned_after: &mut bool,
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
            && let Some(name) = self.commonjs_export_assignment_name(binary.left)
            && name == property_name
        {
            if node.pos < read_pos {
                *assigned_before = true;
            } else if node.pos > read_pos {
                *assigned_after = true;
            }
        }

        for child_idx in self.ctx.arena.get_children(idx) {
            self.collect_commonjs_export_assignment_order(
                child_idx,
                property_name,
                read_pos,
                assigned_before,
                assigned_after,
            );
        }
    }

    fn commonjs_export_assignment_name(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        match node.kind {
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                if !self.is_current_file_commonjs_export_base_for_expando(access.expression) {
                    return None;
                }
                self.ctx
                    .arena
                    .get_identifier_at(access.name_or_argument)
                    .map(|ident| ident.escaped_text.to_string())
            }
            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                if !self.is_current_file_commonjs_export_base_for_expando(access.expression) {
                    return None;
                }
                self.commonjs_static_member_name_for_expando(access.name_or_argument)
            }
            _ => None,
        }
    }
}
