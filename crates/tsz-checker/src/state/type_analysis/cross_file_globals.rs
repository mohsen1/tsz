use crate::state::CheckerState;
use tsz_binder::{BinderState, symbol_flags};

impl<'a> CheckerState<'a> {
    pub(crate) fn resolve_cross_file_global_type_symbol(
        &self,
        name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        let normalized = name.strip_prefix("globalThis.").unwrap_or(name);
        let lib_binders = self.get_lib_binders();
        self.ctx
            .binder
            .file_locals
            .get(normalized)
            .or_else(|| {
                self.ctx
                    .binder
                    .get_global_type_with_libs(normalized, &lib_binders)
            })
            .or_else(|| {
                normalized
                    .rsplit('.')
                    .next()
                    .filter(|tail| *tail != normalized)
                    .and_then(|tail| {
                        self.ctx.binder.file_locals.get(tail).or_else(|| {
                            self.ctx
                                .binder
                                .get_global_type_with_libs(tail, &lib_binders)
                        })
                    })
            })
    }

    pub(crate) fn source_file_global_type_is_direct_lowerable(
        &self,
        delegate_binder: &BinderState,
        type_name: &str,
    ) -> bool {
        if delegate_binder.file_locals.get(type_name).is_some() {
            return false;
        }
        let lib_binders = self.get_lib_binders();
        self.ctx
            .binder
            .get_global_type_with_libs(type_name, &lib_binders)
            .or_else(|| {
                lib_binders
                    .iter()
                    .find_map(|lib| lib.file_locals.get(type_name))
            })
            .and_then(|sym_id| self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders))
            .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::TYPE))
    }
}
