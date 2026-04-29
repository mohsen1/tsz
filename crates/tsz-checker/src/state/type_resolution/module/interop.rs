use crate::context::ResolutionModeOverride;
use crate::state::CheckerState;
use tsz_common::common::ModuleKind;

impl<'a> CheckerState<'a> {
    /// Check if the target module is a pure ESM module (from a package with
    /// `"type": "module"` or using `.mjs`/`.mts` extension).
    pub(crate) fn module_is_esm(&self, module_specifier: &str) -> bool {
        let Some(target_idx) = self.ctx.resolve_import_target(module_specifier) else {
            return false;
        };
        let arena = self.ctx.get_arena_for_file(target_idx as u32);
        let Some(source_file) = arena.source_files.first() else {
            return false;
        };
        let file_name = source_file.file_name.as_str();

        if file_name.ends_with(".mjs") || file_name.ends_with(".mts") {
            return true;
        }
        if file_name.ends_with(".cjs") || file_name.ends_with(".cts") {
            return false;
        }

        self.ctx
            .file_is_esm_map
            .as_ref()
            .and_then(|map| map.get(file_name))
            .copied()
            .unwrap_or(false)
    }

    /// In Node20/NodeNext require-style consumers, a target ESM file can
    /// expose a CommonJS-facing binding via `export type { X as "module.exports" }`.
    /// Callers use this to treat `"module.exports"` like a default/export-equals
    /// binding for diagnostics and type-only classification.
    pub(crate) fn module_uses_module_exports_interop(
        &self,
        module_specifier: &str,
        resolution_mode: Option<ResolutionModeOverride>,
    ) -> bool {
        if !matches!(
            self.ctx.compiler_options.module,
            ModuleKind::Node20 | ModuleKind::NodeNext
        ) {
            return false;
        }

        if resolution_mode != Some(ResolutionModeOverride::Require) {
            return false;
        }

        let Some(target_idx) = self.ctx.resolve_import_target_from_file_with_mode(
            self.ctx.current_file_idx,
            module_specifier,
            Some(ResolutionModeOverride::Require),
        ) else {
            return false;
        };

        let arena = self.ctx.get_arena_for_file(target_idx as u32);
        let Some(source_file) = arena.source_files.first() else {
            return false;
        };
        let file_name = source_file.file_name.as_str();
        let target_is_esm = if file_name.ends_with(".mjs") || file_name.ends_with(".mts") {
            true
        } else if file_name.ends_with(".cjs") || file_name.ends_with(".cts") {
            false
        } else {
            self.lookup_file_is_esm(file_name).unwrap_or(false)
        };

        let mut visited = rustc_hash::FxHashSet::default();
        let has_module_exports = self
            .resolve_export_in_file(target_idx, "module.exports", &mut visited)
            .is_some()
            || self
                .resolve_effective_module_exports_with_mode(
                    module_specifier,
                    Some(ResolutionModeOverride::Require),
                )
                .is_some_and(|exports| exports.has("module.exports"));

        target_is_esm && has_module_exports
    }

    pub(crate) fn current_file_uses_module_exports_require_interop(
        &self,
        module_specifier: &str,
    ) -> bool {
        matches!(
            self.ctx.compiler_options.module,
            ModuleKind::Node20 | ModuleKind::NodeNext
        ) && matches!(
            self.current_file_emit_resolution_mode(),
            ResolutionModeOverride::Require
        ) && self.module_is_esm(module_specifier)
            && self
                .resolve_effective_module_exports(module_specifier)
                .is_some_and(|exports| exports.has("module.exports"))
    }
}
