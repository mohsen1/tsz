use crate::state::CheckerState;

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
}
