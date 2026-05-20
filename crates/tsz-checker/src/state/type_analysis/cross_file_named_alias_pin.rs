//! Named-import-alias pinning for cross-file symbol lookup.
//!
//! Per-file binders mint raw `SymbolId`s starting from zero, so the same
//! integer id can name a local `import { x } from "./m"` alias in one file
//! and an unrelated export in `./m`. When the consuming file looks up that
//! id via `get_symbol_globally` / `get_cross_file_symbol`, the
//! `cross_file_symbol_targets` overlay routes the request to the source
//! file's binder. If we look there blindly we may pick up *whatever decl*
//! happens to share the raw id rather than the alias itself, and downstream
//! type computation collapses (e.g. `import { instance }` collapses to
//! `typeof instance` and drops the imported class's heritage).
//!
//! The alias's actual target is resolved through the import chain in
//! `compute_type_of_symbol_type_alias_variable_alias`, so the alias symbol
//! must stay anchored to the current binder when one is present.
use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};

impl<'a> CheckerState<'a> {
    /// Local named import alias (`ALIAS` + `import_module`) at `sym_id`, if any.
    pub(crate) fn local_named_import_alias(&self, sym_id: SymbolId) -> Option<&tsz_binder::Symbol> {
        let local = self.ctx.binder.get_symbol(sym_id)?;
        (local.has_any_flags(symbol_flags::ALIAS) && local.import_module.is_some()).then_some(local)
    }
}
