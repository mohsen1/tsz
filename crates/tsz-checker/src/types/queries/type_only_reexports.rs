use crate::state::CheckerState;

impl<'a> CheckerState<'a> {
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
        super::with_type_only_query_path(visited, key, |visited| {
            self.name_exists_in_resolved_module_exports(target_file_idx, export_name, visited)
        })
    }

    fn name_exists_in_resolved_module_exports(
        &self,
        target_file_idx: usize,
        export_name: &str,
        visited: &mut rustc_hash::FxHashSet<(usize, String)>,
    ) -> bool {
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

        if let Some(entries) = self
            .ctx
            .wildcard_reexports_for_file(target_binder, &target_file_name)
        {
            for (source_module, _) in entries {
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
