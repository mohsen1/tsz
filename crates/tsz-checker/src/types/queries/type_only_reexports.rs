use crate::state::CheckerState;
use tsz_binder::symbol_flags;

impl<'a> CheckerState<'a> {
    pub(super) fn is_export_type_only_syntax_in_file(
        &self,
        source_file_idx: usize,
        module_specifier: &str,
        export_name: &str,
        visited: &mut rustc_hash::FxHashSet<(usize, String)>,
    ) -> bool {
        let Some(target_file_idx) = self
            .ctx
            .resolve_import_target_from_file(source_file_idx, module_specifier)
        else {
            return false;
        };

        let key = (target_file_idx, export_name.to_string());
        if !visited.insert(key) {
            return false;
        }

        let Some(target_binder) = self.ctx.get_binder_for_file(target_file_idx) else {
            return false;
        };

        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let Some(target_file_name) = target_arena
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
        else {
            return false;
        };

        if let Some(exports_table) = self
            .ctx
            .module_exports_for_module(target_binder, &target_file_name)
            && let Some(sym_id) = exports_table.get(export_name)
        {
            let sym_opt = target_binder
                .get_symbol(sym_id)
                .or_else(|| self.ctx.binder.get_symbol(sym_id));
            if let Some(sym) = sym_opt {
                if sym.is_type_only {
                    let has_value_flags = sym.has_any_flags(symbol_flags::ALIAS)
                        && sym.has_any_flags(symbol_flags::VALUE);
                    let has_value_partner =
                        self.ctx.alias_partners_contains(self.ctx.binder, sym_id);
                    if !has_value_flags && !has_value_partner {
                        return true;
                    }
                }

                if sym.has_any_flags(symbol_flags::ALIAS)
                    && let Some(ref import_module) = sym.import_module
                {
                    let import_name = sym.import_name.as_deref().unwrap_or(&sym.escaped_name);
                    if self.is_export_type_only_syntax_in_file(
                        target_file_idx,
                        import_module,
                        import_name,
                        visited,
                    ) {
                        return true;
                    }
                }
                return false;
            }
        }

        if let Some(file_reexports) = self
            .ctx
            .reexports_for_file(target_binder, &target_file_name)
            && let Some((source_module, original_name)) = file_reexports.get(export_name)
        {
            let name_to_lookup = original_name.as_deref().unwrap_or(export_name);
            return self.is_export_type_only_syntax_in_file(
                target_file_idx,
                source_module,
                name_to_lookup,
                visited,
            );
        }

        if let Some(source_modules) = self
            .ctx
            .wildcard_reexports_for_file(target_binder, &target_file_name)
        {
            let source_type_only_flags = self
                .ctx
                .wildcard_reexports_type_only_for_file(target_binder, &target_file_name);

            for (i, source_module) in source_modules.iter().enumerate() {
                let source_is_type_only = source_type_only_flags
                    .and_then(|flags| flags.get(i).map(|(_, is_to)| *is_to))
                    .unwrap_or(false);
                if source_is_type_only
                    && self.name_exists_in_module_exports(
                        target_file_idx,
                        source_module,
                        export_name,
                        visited,
                    )
                {
                    return true;
                }
            }
        }

        false
    }

    pub(super) fn name_exists_in_module_exports(
        &self,
        source_file_idx: usize,
        module_specifier: &str,
        export_name: &str,
        visited: &mut rustc_hash::FxHashSet<(usize, String)>,
    ) -> bool {
        let Some(target_file_idx) = self
            .ctx
            .resolve_import_target_from_file(source_file_idx, module_specifier)
        else {
            return false;
        };

        let key = (target_file_idx, format!("exists:{export_name}"));
        if !visited.insert(key) {
            return false;
        }

        let Some(target_binder) = self.ctx.get_binder_for_file(target_file_idx) else {
            return false;
        };

        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let Some(target_file_name) = target_arena
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
        else {
            return false;
        };

        if let Some(exports_table) = self
            .ctx
            .module_exports_for_module(target_binder, &target_file_name)
            && exports_table.get(export_name).is_some()
        {
            return true;
        }

        if let Some(file_reexports) = self
            .ctx
            .reexports_for_file(target_binder, &target_file_name)
            && file_reexports.get(export_name).is_some()
        {
            return true;
        }

        if let Some(source_modules) = self
            .ctx
            .wildcard_reexports_for_file(target_binder, &target_file_name)
        {
            for source_module in source_modules.iter() {
                if self.name_exists_in_module_exports(
                    target_file_idx,
                    source_module,
                    export_name,
                    visited,
                ) {
                    return true;
                }
            }
        }

        false
    }
}
