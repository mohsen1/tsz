//! Declaration-kind check shared by the `TS2403` cross-file merge sites in
//! the parent module, plus JS expando-container detection used by the
//! cross-file-global value-type preference sites (#17443).
//!
//! Split out of the parent module to satisfy the source-file line cap.

use super::*;
use tsz_binder::SymbolId;

impl<'a> CheckerState<'a> {
    /// Whether `kind` is one of the declaration kinds that merge instead of
    /// conflicting for `TS2403` purposes: namespace/module, enum, class,
    /// interface, function. A `var`/`let`/`const` initialized with a
    /// function/arrow/class expression does **not** get this exemption even
    /// once its name picks up a JS expando member assignment (`x.prop =
    /// ...`) — verified against `typescript@7.0.2`:
    /// `TypeScript/tests/cases/conformance/salsa/jsContainerMergeTsDeclaration.ts`
    /// (`a.js`'s `var x = function foo() {}; x.a = function bar() {}` vs
    /// `b.ts`'s `var x = function () { return 1; }();`) still reports
    /// `TS2403` alongside `TS2339`.
    pub(in crate::state_domain::variable_checking) const fn is_mergeable_decl_kind(
        &self,
        kind: u16,
    ) -> bool {
        matches!(
            kind,
            syntax_kind_ext::MODULE_DECLARATION
                | syntax_kind_ext::ENUM_DECLARATION
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::INTERFACE_DECLARATION
                | syntax_kind_ext::FUNCTION_DECLARATION
        )
    }

    /// Whether `decl_idx` is a `var`/`let`/`const` initialized with a
    /// function/arrow/class expression whose name has picked up JS expando
    /// member assignments (`x.prop = ...`) anywhere in the project. Used to
    /// keep a JS file's own expando container authoritative for that file's
    /// writes against a conflicting cross-file global (#17443) — this is
    /// unrelated to (and does NOT exempt) the `TS2403` cross-file
    /// redeclaration-type-identity check above.
    pub(crate) fn is_expando_container_var_decl(&self, decl_idx: NodeIndex, name: &str) -> bool {
        self.is_expando_container_var_decl_in_arena(self.ctx.arena, decl_idx, name)
    }

    /// Whether the CURRENT file declares `name` (bound to a genuinely
    /// current-file symbol) as its own JS expando container variable. Used by
    /// the cross-file-global value-type preference sites: a JS file that owns
    /// its own expando container must resolve `name` to that container, not
    /// defer to a `.ts`/`.d.ts` sibling's conflicting global declaration
    /// (#17443).
    pub(crate) fn current_file_declares_expando_container_variable(&self, name: &str) -> bool {
        let Some(sym_id) = self.ctx.binder.file_locals.get(name) else {
            return false;
        };
        self.current_file_owns_expando_container_declaration(sym_id)
    }

    /// Whether the CURRENT file's binder owns a declaration of `sym_id` that is
    /// a JS expando container variable (function/arrow/class-expression
    /// initializer with `name.prop = …` members). Keyed on `SymbolId` for the
    /// cross-arena delegation guard, which must keep a JS file's own expando
    /// container local rather than routing `sym_id` to a conflicting sibling's
    /// arena (#17443). The `get_node_symbol` round-trip keeps this arena-safe
    /// against raw `SymbolId`/`NodeIndex` reuse across files.
    pub(crate) fn current_file_owns_expando_container_declaration(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let name = symbol.escaped_name.as_str();
        symbol.all_declarations().into_iter().any(|decl_idx| {
            self.ctx.binder.get_node_symbol(decl_idx) == Some(sym_id)
                && self.is_expando_container_var_decl(decl_idx, name)
        })
    }

    /// Arena-parameterized core of [`is_expando_container_var_decl`], usable
    /// for a declaration that lives in a *different* file's arena. The
    /// project-wide expando-property index is looked up on `self` — it is
    /// not arena-scoped — while the declaration's own syntax is read from
    /// the caller-supplied `arena`.
    pub(in crate::state_domain::variable_checking) fn is_expando_container_var_decl_in_arena(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        decl_idx: NodeIndex,
        name: &str,
    ) -> bool {
        let Some(node) = arena.get(decl_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return false;
        }
        let Some(var_decl) = arena.get_variable_declaration(node) else {
            return false;
        };
        if var_decl.initializer.is_none() {
            return false;
        }
        let Some(init_node) = arena.get(var_decl.initializer) else {
            return false;
        };
        if !matches!(
            init_node.kind,
            syntax_kind_ext::FUNCTION_EXPRESSION
                | syntax_kind_ext::ARROW_FUNCTION
                | syntax_kind_ext::CLASS_EXPRESSION
        ) {
            return false;
        }
        !self.collect_expando_properties_for_root(name).is_empty()
    }
}
