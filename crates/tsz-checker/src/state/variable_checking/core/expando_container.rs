//! Detection of a JS "expando container" variable — a `var`/`let`/`const`
//! initialized with a function/arrow/class expression whose name later
//! picks up a JS expando member assignment (`x.prop = ...`).
//!
//! Split out of the parent module to satisfy the source-file line cap.

use super::*;

impl<'a> CheckerState<'a> {
    /// Whether `decl_idx` is a `var`/`let`/`const` initialized with a
    /// function/arrow/class expression whose name has picked up JS expando
    /// member assignments (`x.prop = ...`) anywhere in the project. tsc
    /// treats such a variable as a function-like container for
    /// declaration-merge purposes — the same exemption a bare
    /// `FUNCTION_DECLARATION` already gets from TS2403 — even though
    /// syntactically it is an ordinary `VariableDeclaration`. Verified
    /// against `typescript@7.0.2`: a bare `var x = function(){}` with no
    /// expando still conflicts by TS2403 with an incompatible cross-file
    /// `var x`; only once an `x.prop = ...` expando assignment exists does
    /// the redeclaration check stop comparing types
    /// (`TypeScript/tests/cases/conformance/salsa/jsContainerMergeTsDeclaration.ts`).
    pub(crate) fn is_expando_container_var_decl(&self, decl_idx: NodeIndex, name: &str) -> bool {
        self.is_expando_container_var_decl_in_arena(self.ctx.arena, decl_idx, name)
    }

    /// Arena-parameterized core of [`is_expando_container_var_decl`], usable
    /// for a declaration that lives in a *different* file's arena (the
    /// cross-arena delegation path further down this function). The
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

    /// Whether `decl_idx`'s node kind is one of the declaration kinds that
    /// merge instead of conflicting for `TS2403` purposes (namespace/module,
    /// enum, class, interface, function), OR `decl_idx` is an expando
    /// container var (see [`is_expando_container_var_decl`]). `name` is the
    /// declaration's own name, used only for the expando-container check.
    pub(in crate::state_domain::variable_checking) fn is_mergeable_or_expando_container_decl(
        &self,
        decl_idx: NodeIndex,
        name: Option<&str>,
    ) -> bool {
        let kind_is_mergeable = self.ctx.arena.get(decl_idx).is_some_and(|decl_node| {
            matches!(
                decl_node.kind,
                syntax_kind_ext::MODULE_DECLARATION
                    | syntax_kind_ext::ENUM_DECLARATION
                    | syntax_kind_ext::CLASS_DECLARATION
                    | syntax_kind_ext::INTERFACE_DECLARATION
                    | syntax_kind_ext::FUNCTION_DECLARATION
            )
        });
        kind_is_mergeable
            || name.is_some_and(|name| self.is_expando_container_var_decl(decl_idx, name))
    }

    /// Whether the file currently being checked declares `sym_id` as its own JS
    /// "expando container" variable (see [`is_expando_container_var_decl`]).
    ///
    /// A script-global `var x` in one file merges into a single symbol with a
    /// same-named `var x` in another file. That merged symbol has one canonical
    /// owner (`resolve_symbol_file_index`), which may point at the *other*
    /// file — whose plain, non-expando (e.g. `number`) declaration then wins
    /// every cross-file-global-preference heuristic and wrongly types this
    /// file's own `x.prop = ...` write against the foreign type. `tsc` instead
    /// keeps each expando container's own file authoritative for that file's
    /// writes. This predicate lets the current file reclaim that ownership.
    ///
    /// Arena-safe: only a declaration the *current* binder maps back to this
    /// exact `SymbolId` (`get_node_symbol` round-trip) is considered, so a
    /// foreign declaration sharing the raw `SymbolId`, or a cross-arena
    /// `NodeIndex`, is never read against this file's arena.
    ///
    /// [`is_expando_container_var_decl`]:
    /// CheckerState::is_expando_container_var_decl
    pub(crate) fn current_file_owns_expando_container_variable(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        self.current_file_expando_container_decl(sym_id).is_some()
    }

    /// The current file's own JS expando-container declaration of `sym_id`, if
    /// any — the counterpart of [`current_file_owns_expando_container_variable`]
    /// that also yields the owning `NodeIndex`.
    ///
    /// Callers needing the declaration's *type* (base-type selection) use this
    /// so they resolve `x` through the current file's declaration rather than
    /// the merged symbol's canonical `value_declaration`, which is the
    /// first-bound file's — foreign when the sibling `.ts`/`.js` was bound
    /// first. Arena-safe via the same `get_node_symbol` round-trip.
    ///
    /// [`current_file_owns_expando_container_variable`]:
    /// CheckerState::current_file_owns_expando_container_variable
    pub(crate) fn current_file_expando_container_decl(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<NodeIndex> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        // `is_expando_container_var_decl` reborrows `self`, so snapshot the
        // declarations and name before the loop to release the symbol borrow.
        let name = symbol.escaped_name.clone();
        let decls: Vec<NodeIndex> = symbol
            .declarations
            .iter()
            .copied()
            .chain(std::iter::once(symbol.value_declaration))
            .collect();
        decls.into_iter().find(|&decl| {
            !decl.is_none()
                && self.ctx.binder.get_node_symbol(decl) == Some(sym_id)
                && self.is_expando_container_var_decl(decl, &name)
        })
    }
}
