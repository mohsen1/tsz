//! Diagnostics for tuple spread expansions that exceed tsc's representation limit.

use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use crate::types_domain::unique_symbol_arena::unwrap_parenthesized_type;
use rustc_hash::FxHashSet;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::{NodeIndex, node::NodeAccess, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

const MAX_REPRESENTABLE_TUPLE_LENGTH: usize = 10_000;
const MAX_AST_RECURSION_DEPTH: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TupleLengthEstimate {
    Known(usize),
    TooLarge,
    Unknown,
}

impl TupleLengthEstimate {
    const fn from_len(len: usize) -> Self {
        if len > MAX_REPRESENTABLE_TUPLE_LENGTH {
            Self::TooLarge
        } else {
            Self::Known(len)
        }
    }

    const fn add(self, other: Self) -> Self {
        match (self, other) {
            (Self::TooLarge, _) | (_, Self::TooLarge) => Self::TooLarge,
            (Self::Known(left), Self::Known(right)) => Self::from_len(left.saturating_add(right)),
            _ => Self::Unknown,
        }
    }

    fn is_too_large(self) -> bool {
        self == Self::TooLarge
    }
}

impl<'a> CheckerState<'a> {
    pub(crate) fn type_node_produces_too_large_tuple(&self, type_node: NodeIndex) -> bool {
        self.type_node_contains_tuple_spread(type_node, 0)
            && self
                .estimate_tuple_type_node_length(type_node, &mut FxHashSet::default(), 0)
                .is_too_large()
    }

    pub(crate) fn array_literal_produces_too_large_tuple(&self, expr: NodeIndex) -> bool {
        self.estimate_array_literal_length(expr, &mut FxHashSet::default(), 0)
            .is_too_large()
    }

    /// Returns true when the alias body has a tuple spread **and** its top-level node is
    /// not a conditional type (modulo parentheses).
    ///
    /// `TS2799` applies to unconditional exponential spread chains (e.g. `type T = [...A, ...B]`).
    /// Conditional-type aliases express termination via their false branch; if depth is exceeded
    /// for those, the correct diagnostic is `TS2589`, not `TS2799`.
    pub(crate) fn type_alias_is_unconditional_tuple_spread(&self, alias_sym: SymbolId) -> bool {
        self.type_alias_type_node(alias_sym)
            .is_some_and(|type_node| {
                let inner = unwrap_parenthesized_type(self.ctx.arena, type_node);
                self.ctx
                    .arena
                    .get(inner)
                    .is_none_or(|n| n.kind != syntax_kind_ext::CONDITIONAL_TYPE)
                    && self.type_node_contains_tuple_spread(type_node, 0)
            })
    }

    fn estimate_tuple_type_node_length(
        &self,
        type_node: NodeIndex,
        alias_stack: &mut FxHashSet<SymbolId>,
        depth: usize,
    ) -> TupleLengthEstimate {
        if type_node.is_none() || depth > MAX_AST_RECURSION_DEPTH {
            return TupleLengthEstimate::Unknown;
        }

        let Some(node) = self.ctx.arena.get(type_node) else {
            return TupleLengthEstimate::Unknown;
        };

        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            && let Some(wrapped) = self.ctx.arena.get_wrapped_type(node)
        {
            return self.estimate_tuple_type_node_length(wrapped.type_node, alias_stack, depth + 1);
        }

        if node.kind == syntax_kind_ext::TUPLE_TYPE {
            let Some(tuple) = self.ctx.arena.get_tuple_type(node) else {
                return TupleLengthEstimate::Unknown;
            };
            let mut total = TupleLengthEstimate::Known(0);
            for &element in &tuple.elements.nodes {
                total =
                    total.add(self.estimate_tuple_element_length(element, alias_stack, depth + 1));
                if total.is_too_large() {
                    return total;
                }
            }
            return total;
        }

        if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
                return TupleLengthEstimate::Unknown;
            };
            let TypeSymbolResolution::Type(sym_id) =
                self.resolve_qualified_symbol_in_type_position(type_ref.type_name)
            else {
                return TupleLengthEstimate::Unknown;
            };
            return self.estimate_type_alias_length(sym_id, alias_stack, depth + 1);
        }

        TupleLengthEstimate::Unknown
    }

    fn estimate_type_alias_length(
        &self,
        sym_id: SymbolId,
        alias_stack: &mut FxHashSet<SymbolId>,
        depth: usize,
    ) -> TupleLengthEstimate {
        if !alias_stack.insert(sym_id) {
            return TupleLengthEstimate::Unknown;
        }
        let result = self
            .type_alias_type_node(sym_id)
            .map(|type_node| self.estimate_tuple_type_node_length(type_node, alias_stack, depth))
            .unwrap_or(TupleLengthEstimate::Unknown);
        alias_stack.remove(&sym_id);
        result
    }

    fn estimate_tuple_element_length(
        &self,
        element: NodeIndex,
        alias_stack: &mut FxHashSet<SymbolId>,
        depth: usize,
    ) -> TupleLengthEstimate {
        let Some(node) = self.ctx.arena.get(element) else {
            return TupleLengthEstimate::Unknown;
        };
        if node.kind == syntax_kind_ext::REST_TYPE {
            return self
                .ctx
                .arena
                .get_wrapped_type(node)
                .map(|wrapped| {
                    self.estimate_tuple_type_node_length(wrapped.type_node, alias_stack, depth + 1)
                })
                .unwrap_or(TupleLengthEstimate::Unknown);
        }
        if node.kind == syntax_kind_ext::NAMED_TUPLE_MEMBER
            && let Some(member) = self.ctx.arena.get_named_tuple_member(node)
        {
            if member.dot_dot_dot_token {
                return self.estimate_tuple_type_node_length(
                    member.type_node,
                    alias_stack,
                    depth + 1,
                );
            }
            return TupleLengthEstimate::Known(1);
        }
        TupleLengthEstimate::Known(1)
    }

    fn estimate_array_literal_length(
        &self,
        expr: NodeIndex,
        value_stack: &mut FxHashSet<SymbolId>,
        depth: usize,
    ) -> TupleLengthEstimate {
        if expr.is_none() || depth > MAX_AST_RECURSION_DEPTH {
            return TupleLengthEstimate::Unknown;
        }

        let expr = self.ctx.arena.skip_parenthesized_and_assertions(expr);
        let Some(node) = self.ctx.arena.get(expr) else {
            return TupleLengthEstimate::Unknown;
        };

        if node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            let Some(array) = self.ctx.arena.get_literal_expr(node) else {
                return TupleLengthEstimate::Unknown;
            };
            let mut total = TupleLengthEstimate::Known(0);
            for &element in &array.elements.nodes {
                if element.is_none() {
                    total = total.add(TupleLengthEstimate::Known(1));
                    continue;
                }
                let element_len = self.estimate_array_element_length(element, value_stack, depth);
                total = total.add(element_len);
                if total.is_too_large() {
                    return total;
                }
            }
            return total;
        }

        if node.kind == SyntaxKind::Identifier as u16
            && let Some(sym_id) = self.ctx.binder.resolve_identifier(self.ctx.arena, expr)
        {
            return self.estimate_const_value_length(sym_id, value_stack, depth + 1);
        }

        TupleLengthEstimate::Unknown
    }

    fn estimate_array_element_length(
        &self,
        element: NodeIndex,
        value_stack: &mut FxHashSet<SymbolId>,
        depth: usize,
    ) -> TupleLengthEstimate {
        let Some(node) = self.ctx.arena.get(element) else {
            return TupleLengthEstimate::Unknown;
        };
        if node.kind == syntax_kind_ext::SPREAD_ELEMENT
            && let Some(spread) = self.ctx.arena.get_spread(node)
        {
            return self.estimate_array_literal_length(spread.expression, value_stack, depth + 1);
        }
        TupleLengthEstimate::Known(1)
    }

    fn estimate_const_value_length(
        &self,
        sym_id: SymbolId,
        value_stack: &mut FxHashSet<SymbolId>,
        depth: usize,
    ) -> TupleLengthEstimate {
        if !value_stack.insert(sym_id) {
            return TupleLengthEstimate::Unknown;
        }
        let result = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .map(|symbol| symbol.value_declaration)
            .filter(|decl_idx| decl_idx.is_some())
            .filter(|&decl_idx| self.ctx.arena.is_const_variable_declaration(decl_idx))
            .and_then(|decl_idx| self.ctx.arena.get(decl_idx))
            .and_then(|decl_node| self.ctx.arena.get_variable_declaration(decl_node))
            .and_then(|decl| decl.initializer.is_some().then_some(decl.initializer))
            .map(|initializer| {
                self.estimate_array_literal_length(initializer, value_stack, depth + 1)
            })
            .unwrap_or(TupleLengthEstimate::Unknown);
        value_stack.remove(&sym_id);
        result
    }

    fn type_alias_type_node(&self, sym_id: SymbolId) -> Option<NodeIndex> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(symbol_flags::TYPE_ALIAS) {
            return None;
        }
        symbol.declarations.iter().find_map(|&decl_idx| {
            let decl_node = self.ctx.arena.get(decl_idx)?;
            self.ctx
                .arena
                .get_type_alias(decl_node)
                .map(|alias| alias.type_node)
        })
    }

    /// `dot_dot_dot_token` on `NAMED_TUPLE_MEMBER` is a boolean field, not an
    /// AST child, so it is invisible to the generic `get_children` traversal
    /// and must be checked explicitly.
    fn tuple_element_is_spread(&self, element: NodeIndex) -> bool {
        self.ctx.arena.get(element).is_some_and(|node| {
            node.kind == syntax_kind_ext::REST_TYPE
                || (node.kind == syntax_kind_ext::NAMED_TUPLE_MEMBER
                    && self
                        .ctx
                        .arena
                        .get_named_tuple_member(node)
                        .is_some_and(|m| m.dot_dot_dot_token))
        })
    }

    fn type_node_contains_tuple_spread(&self, type_node: NodeIndex, depth: usize) -> bool {
        if type_node.is_none() || depth > MAX_AST_RECURSION_DEPTH {
            return false;
        }
        let Some(node) = self.ctx.arena.get(type_node) else {
            return false;
        };
        if self.tuple_element_is_spread(type_node) {
            return true;
        }
        if node.kind == syntax_kind_ext::TUPLE_TYPE
            && let Some(tuple) = self.ctx.arena.get_tuple_type(node)
            && tuple
                .elements
                .nodes
                .iter()
                .any(|&el| self.tuple_element_is_spread(el))
        {
            return true;
        }
        self.ctx
            .arena
            .get_children(type_node)
            .into_iter()
            .any(|child| self.type_node_contains_tuple_spread(child, depth + 1))
    }
}
