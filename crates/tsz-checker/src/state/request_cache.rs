//! Request-aware type cache safety checks for `CheckerState`.
//!
//! These methods determine whether a node's type can be safely cached and
//! reused under a non-empty `TypingRequest`. Only node kinds whose type
//! computation is independent of mutable ambient context (enclosing class,
//! `this` type, destructuring target, etc.) are eligible.

use super::state::CheckerState;
use crate::context::{RequestCacheKey, TypingRequest};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) const fn request_cache_is_audited_access_kind(kind: u16) -> bool {
        kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
    }

    pub(super) fn request_cache_key_for_node(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> Option<RequestCacheKey> {
        let key = RequestCacheKey::from_request(request)?;
        let Some(node) = self.ctx.arena.get(idx) else {
            self.ctx.request_cache_counters.contextual_cache_bypasses += 1;
            return None;
        };

        let audited = match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                self.is_request_cache_safe_property_access(idx)
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                self.is_request_cache_safe_element_access(idx)
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.is_request_cache_safe_object_literal(idx)
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.is_request_cache_safe_array_literal(idx)
            }
            _ => false,
        };

        if !audited {
            self.ctx.request_cache_counters.contextual_cache_bypasses += 1;
            return None;
        }

        Some(key)
    }

    pub(super) fn request_cache_lookup(
        &mut self,
        idx: NodeIndex,
        kind: u16,
        key: RequestCacheKey,
    ) -> Option<TypeId> {
        if Self::request_cache_is_audited_access_kind(kind) {
            self.ctx
                .request_cache_counters
                .property_access_request_cache_lookups += 1;
        }
        if let Some(&cached) = self.ctx.request_node_types.get(&(idx.0, key)) {
            self.ctx.request_cache_counters.request_cache_hits += 1;
            if Self::request_cache_is_audited_access_kind(kind) {
                self.ctx
                    .request_cache_counters
                    .property_access_request_cache_hits += 1;
            }
            return Some(cached);
        }
        self.ctx.request_cache_counters.request_cache_misses += 1;
        None
    }

    pub(super) fn cache_request_type(&mut self, idx: NodeIndex, key: RequestCacheKey, ty: TypeId) {
        self.ctx.request_node_types.insert((idx.0, key), ty);
    }

    fn is_request_cache_safe_property_access(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return false;
        };
        if self.ctx.enclosing_class.is_some() || self.is_this_expression(access.expression) {
            return false;
        }
        if self.is_super_expression(access.expression) {
            return false;
        }
        if self
            .ctx
            .arena
            .get(access.name_or_argument)
            .is_some_and(|name| name.kind == tsz_scanner::SyntaxKind::PrivateIdentifier as u16)
        {
            return false;
        }
        self.is_request_cache_safe_expression_tree(access.expression)
    }

    fn is_request_cache_safe_element_access(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return false;
        };
        if self.ctx.enclosing_class.is_some() || self.is_this_expression(access.expression) {
            return false;
        }
        if self.is_super_expression(access.expression) {
            return false;
        }
        self.is_request_cache_safe_expression_tree(access.expression)
            && self.is_request_cache_safe_expression_tree(access.name_or_argument)
    }

    fn is_request_cache_safe_object_literal(&self, idx: NodeIndex) -> bool {
        if self.ctx.in_destructuring_target
            || self.ctx.preserve_literal_types
            || self.current_this_type().is_some()
        {
            return false;
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(obj) = self.ctx.arena.get_literal_expr(node) else {
            return false;
        };
        for &prop_idx in &obj.elements.nodes {
            let Some(prop_node) = self.ctx.arena.get(prop_idx) else {
                return false;
            };
            match prop_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.ctx.arena.get_property_assignment(prop_node) else {
                        return false;
                    };
                    if self
                        .ctx
                        .arena
                        .get(prop.name)
                        .is_some_and(|name| name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
                    {
                        return false;
                    }
                    if !self.is_request_cache_safe_expression_tree(prop.initializer) {
                        return false;
                    }
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {}
                k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                    let Some(spread) = self.ctx.arena.get_spread(prop_node) else {
                        return false;
                    };
                    if !self.is_request_cache_safe_expression_tree(spread.expression) {
                        return false;
                    }
                }
                _ => return false,
            }
        }
        true
    }

    fn is_request_cache_safe_array_literal(&self, idx: NodeIndex) -> bool {
        if self.ctx.in_destructuring_target || self.ctx.preserve_literal_types {
            return false;
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(array) = self.ctx.arena.get_literal_expr(node) else {
            return false;
        };
        for &elem_idx in &array.elements.nodes {
            if elem_idx.is_none() {
                continue;
            }
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                return false;
            };
            if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                let Some(spread) = self.ctx.arena.get_spread(elem_node) else {
                    return false;
                };
                if !self.is_request_cache_safe_expression_tree(spread.expression) {
                    return false;
                }
                continue;
            }
            if !self.is_request_cache_safe_expression_tree(elem_idx) {
                return false;
            }
        }
        true
    }

    fn is_request_cache_safe_expression_tree(&self, idx: NodeIndex) -> bool {
        if idx.is_none() {
            return true;
        }
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        match node.kind {
            k if k == tsz_scanner::SyntaxKind::Identifier as u16
                || k == tsz_scanner::SyntaxKind::NumericLiteral as u16
                || k == tsz_scanner::SyntaxKind::StringLiteral as u16
                || k == tsz_scanner::SyntaxKind::TrueKeyword as u16
                || k == tsz_scanner::SyntaxKind::FalseKeyword as u16
                || k == tsz_scanner::SyntaxKind::NullKeyword as u16 =>
            {
                true
            }
            k if k == tsz_scanner::SyntaxKind::ThisKeyword as u16 => false,
            k if k == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16 => true,
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                self.ctx.arena.get_parenthesized(node).is_some_and(|paren| {
                    self.is_request_cache_safe_expression_tree(paren.expression)
                })
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => self
                .ctx
                .arena
                .get_unary_expr(node)
                .is_some_and(|expr| self.is_request_cache_safe_expression_tree(expr.operand)),
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION
                || k == syntax_kind_ext::NON_NULL_EXPRESSION =>
            {
                self.ctx
                    .arena
                    .get_type_assertion(node)
                    .is_some_and(|expr| self.is_request_cache_safe_expression_tree(expr.expression))
                    || self.ctx.arena.get_unary_expr_ex(node).is_some_and(|expr| {
                        self.is_request_cache_safe_expression_tree(expr.expression)
                    })
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                self.is_request_cache_safe_property_access(idx)
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                self.is_request_cache_safe_element_access(idx)
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.is_request_cache_safe_object_literal(idx)
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.is_request_cache_safe_array_literal(idx)
            }
            _ => false,
        }
    }
}
