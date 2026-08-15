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

    /// Whether the CURRENT file's own declaration of `name` is the
    /// canonical (earliest-processed) global `var` declaration among every
    /// file that declares `name`. tsc's cross-file `var` merge establishes
    /// the PRIMARY declared type from whichever file's declaration binds
    /// FIRST in program order — the same rule the cross-file half of
    /// `TS2403` uses (`crates/tsz-checker/src/state/variable_checking/core.rs`,
    /// "Only check against files with lower indices"). Once an earlier file
    /// declares `name` as a plain (non-block-scoped, non-bare) `var`, that
    /// earlier declaration's type governs property-lookup resolution
    /// everywhere `name` is used — including inside a LATER file's own
    /// expando-container declaration, even though the #17443 exemption
    /// otherwise keeps a JS file's expando container authoritative for its
    /// own writes (oracle-verified via `typescript@7.0.2`,
    /// `TypeScript/tests/cases/conformance/salsa/jsContainerMergeTsDeclaration.ts`,
    /// #17544). A file with no earlier conflicting declaration (or running
    /// without the multi-file indices) is trivially canonical.
    pub(crate) fn current_file_expando_container_is_canonical(&self, name: &str) -> bool {
        let Some(entries) = self
            .ctx
            .global_file_locals_index
            .as_ref()
            .and_then(|idx| idx.get(name))
        else {
            return true;
        };
        let Some(all_arenas) = self.ctx.all_arenas.as_ref() else {
            return true;
        };
        let Some(all_binders) = self.ctx.all_binders.as_ref() else {
            return true;
        };
        let current_file_idx = self.ctx.current_file_idx;
        for &(file_idx, other_sym_id) in entries.iter() {
            if file_idx >= current_file_idx {
                continue;
            }
            let Some(other_binder) = all_binders.get(file_idx) else {
                continue;
            };
            if other_binder.is_external_module {
                continue;
            }
            let Some(other_arena) = all_arenas.get(file_idx) else {
                continue;
            };
            let Some(other_sym) = other_binder.get_symbol(other_sym_id) else {
                continue;
            };
            for &other_decl in &other_sym.declarations {
                if !other_decl.is_some() {
                    continue;
                }
                let Some(other_node) = other_arena.get(other_decl) else {
                    continue;
                };
                if other_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                    continue;
                }
                let decl_name_matches = other_arena
                    .get_variable_declaration(other_node)
                    .and_then(|vd| other_arena.get(vd.name))
                    .and_then(|name_node| other_arena.get_identifier(name_node))
                    .map(|id| other_arena.resolve_identifier_text(id))
                    .is_some_and(|n| n == name);
                if !decl_name_matches {
                    continue;
                }
                use tsz_parser::parser::node_flags;
                let is_block_scoped = other_arena
                    .get_extended(other_decl)
                    .and_then(|ext| other_arena.get(ext.parent))
                    .filter(|parent| parent.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST)
                    .is_some_and(|parent| node_flags::is_block_scoped(parent.flags as u32));
                if is_block_scoped {
                    continue;
                }
                let is_bare = other_arena
                    .get_variable_declaration(other_node)
                    .is_some_and(|d| d.type_annotation.is_none() && d.initializer.is_none());
                if is_bare {
                    continue;
                }
                // An earlier file declares a genuine, merge-eligible `var`
                // of the same name: it is canonical, not the current file.
                return false;
            }
        }
        true
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
