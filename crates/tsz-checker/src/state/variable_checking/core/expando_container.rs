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
    pub(in crate::state_domain::variable_checking) fn is_expando_container_var_decl(
        &self,
        decl_idx: NodeIndex,
        name: &str,
    ) -> bool {
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
}
