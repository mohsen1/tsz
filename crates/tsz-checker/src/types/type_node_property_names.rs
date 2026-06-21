//! Property-name helpers for `TypeNodeChecker`.
//!
//! Computed-property-name policy (well-known `[Symbol.<name>]` keys and
//! `__unique_<id>` binding-identity keys) is owned by
//! [`super::computed_names`]; this module supplies the lowering layer's
//! symbol resolution and the mutation hooks (well-known-name registration)
//! around that shared policy.

use super::computed_names;
use super::type_node::TypeNodeChecker;
use crate::symbols_domain::name_text::expression_name_text_in_arena;
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::SymbolRef;

impl<'a, 'ctx> TypeNodeChecker<'a, 'ctx> {
    /// Get property name from a property name node.
    fn get_property_name(&self, name_idx: NodeIndex) -> Option<String> {
        crate::types_domain::queries::core::get_literal_property_name(self.ctx.arena, name_idx)
    }

    fn register_well_known_symbol_ref_mapping(&mut self, name: &str, symbol_ref: SymbolRef) {
        if !name.starts_with("[Symbol.") {
            return;
        }

        let name_key = name.to_string();

        if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
            env.register_well_known_symbol_name(name_key.clone(), symbol_ref);
        }
        if let Ok(mut env) = self.ctx.type_environment.try_borrow_mut() {
            env.register_well_known_symbol_name(name_key, symbol_ref);
        }
    }

    fn register_well_known_symbol_name_mapping(&mut self, name: &str, sym_id: SymbolId) {
        self.register_well_known_symbol_ref_mapping(name, SymbolRef(sym_id.0));
    }

    /// Resolve a property name, including computed names backed by unique symbols.
    pub(super) fn get_property_name_resolved(&mut self, name_idx: NodeIndex) -> Option<String> {
        let name_node = self.ctx.arena.get(name_idx)?;

        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return self.get_property_name(name_idx);
        }

        let computed = self.ctx.arena.get_computed_property(name_node)?;

        if let Some(name) = self.get_property_name(name_idx)
            && (name.starts_with("[Symbol.") || name.starts_with("__unique_"))
        {
            if name.starts_with("[Symbol.")
                && let Some(sym_id) = self.resolve_computed_property_symbol(computed.expression)
            {
                self.register_well_known_symbol_name_mapping(&name, sym_id);
            } else if name.starts_with("[Symbol.")
                && let Some((declared_name, symbol_ref)) =
                    computed_names::declared_unique_symbol_member_ref_for_expr(
                        self.ctx,
                        |idx| self.resolve_computed_name_value_symbol(idx),
                        computed.expression,
                    )
                && declared_name == name
            {
                return Some(format!("__unique_{}", symbol_ref.0));
            }
            return Some(name);
        }

        // A computed name `[s]` whose identifier resolves to a user binding with
        // unique-symbol identity (`const s: unique symbol` / a verified global
        // `Symbol(...)`/`Symbol.for(...)` const) keys the member under the
        // canonical `__unique_<id>` binding-identity atom. The strong
        // `CheckerState` lowering path (`computed_identifier_unique_symbol_property_ref`)
        // already does this; without the same leg here, an *inline* object type
        // literal `{ [s]: T }` reached through the `TypeNodeChecker` path (e.g. as
        // the object operand of an indexed-access type `{ [s]: T }[typeof s]`, or
        // nested inside a union/intersection) silently dropped the symbol member,
        // collapsing the shape to `{}` and yielding false TS2536/TS2339. Resolve
        // the value symbol scope-aware (matching the strong path) and emit the
        // binding-identity key so both lowering paths agree on the member atom.
        if let Some(sym_id) = self.resolve_computed_name_value_symbol(computed.expression)
            && let Some(sym_ref) = computed_names::unique_symbol_property_ref(self.ctx, sym_id)
        {
            return Some(format!("__unique_{}", sym_ref.0));
        }

        if let Some(atom) = self.computed_property_expression_name_atom(computed.expression) {
            let name = self.ctx.types.resolve_atom(atom);
            if name.starts_with("[Symbol.")
                && let Some(sym_id) = self.resolve_computed_property_symbol(computed.expression)
            {
                self.register_well_known_symbol_name_mapping(&name, sym_id);
            } else if name.starts_with("[Symbol.")
                && let Some((declared_name, symbol_ref)) =
                    computed_names::declared_unique_symbol_member_ref_for_expr(
                        self.ctx,
                        |idx| self.resolve_computed_name_value_symbol(idx),
                        computed.expression,
                    )
                && declared_name == name
            {
                return Some(format!("__unique_{}", symbol_ref.0));
            }
            return Some(name);
        }

        self.get_property_name(name_idx)
    }

    pub(super) fn is_symbol_property_name(&mut self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }
        let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
            return false;
        };

        if self
            .get_property_name_resolved(name_idx)
            .is_some_and(|name| name.starts_with("[Symbol."))
        {
            return true;
        }

        self.computed_property_expression_is_symbol_named(computed.expression)
    }

    pub(super) fn computed_property_expression_name_atom(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<Atom> {
        computed_names::computed_property_name_atom(
            self.ctx,
            |idx| self.resolve_computed_property_symbol(idx),
            expr_idx,
        )
    }

    pub(super) fn computed_property_expression_is_symbol_named(&self, expr_idx: NodeIndex) -> bool {
        computed_names::computed_property_is_symbol_named(
            self.ctx,
            |idx| self.resolve_computed_property_symbol(idx),
            expr_idx,
        )
    }

    /// Resolve the binding a computed-name expression refers to, mirroring the
    /// strong `CheckerState` path's `resolve_computed_name_expression_symbol`:
    /// scope-aware `resolve_identifier` with a `file_locals` fallback (no
    /// VALUE-flag filtering, no eager import-alias following — the canonical key
    /// helpers in `computed_names` own alias hops). Used only for the
    /// unique-symbol binding-identity key leg, so both lowering paths agree on
    /// the same symbol and therefore the same `__unique_<id>` member atom.
    fn resolve_computed_name_value_symbol(&self, expr_idx: NodeIndex) -> Option<SymbolId> {
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            let paren = self.ctx.arena.get_parenthesized(node)?;
            return self.resolve_computed_name_value_symbol(paren.expression);
        }
        if let Some(ident) = self.ctx.arena.get_identifier(node) {
            return self
                .ctx
                .binder
                .resolve_identifier(self.ctx.arena, expr_idx)
                .or_else(|| self.ctx.binder.file_locals.get(&ident.escaped_text));
        }
        self.resolve_computed_property_symbol(expr_idx)
    }

    fn resolve_computed_property_symbol(&self, expr_idx: NodeIndex) -> Option<SymbolId> {
        let node = self.ctx.arena.get(expr_idx)?;

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            let paren = self.ctx.arena.get_parenthesized(node)?;
            return self.resolve_computed_property_symbol(paren.expression);
        }

        if node.kind == SyntaxKind::Identifier as u16 {
            let sym_id = self
                .resolve_value_symbol_with_libs(expr_idx)
                .map(SymbolId)?;
            return Some(computed_names::follow_import_aliases(self.ctx, sym_id));
        }

        let qualified = self.expression_name_text(expr_idx)?;
        self.resolve_entity_name_text_symbol(&qualified)
    }

    fn expression_name_text(&self, idx: NodeIndex) -> Option<String> {
        expression_name_text_in_arena(self.ctx.arena, idx)
    }
}
