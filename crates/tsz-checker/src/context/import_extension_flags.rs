//! TS2877 helpers — the `resolvedUsingTsExtension` flag map.
//!
//! Extracted from `context/core.rs` to keep the core file under the per-file
//! LOC ceiling. The accessors here are consulted by the import-extension
//! emission gate in `declarations/import/declaration.rs`.

use std::sync::Arc;

use super::CheckerContext;
use super::aliases::ResolvedModuleTsExtensionMap;
use crate::module_resolution::module_specifier_candidates;

impl CheckerContext<'_> {
    /// Set the per-`(source_file_idx, specifier)` `resolvedUsingTsExtension`
    /// flag map. See [`ResolvedModuleTsExtensionMap`].
    pub fn set_resolved_module_ts_extension_flags(
        &mut self,
        flags: Arc<ResolvedModuleTsExtensionMap>,
    ) {
        self.resolved_module_ts_extension_flags = Some(flags);
    }

    /// Returns true when the resolver consumed a TypeScript source extension
    /// from the specifier via a literal package.json `exports`/`imports` key.
    /// Used by the TS2877 gate to suppress the import-extension warning when
    /// the package author opted into the `.ts` mapping (e.g. `"./*.ts": ...`).
    ///
    /// Returns false when the flag is unknown — callers should treat unknown
    /// as "extension preserved through wildcard substitution", which is the
    /// situation TS2877 warns about.
    pub fn import_resolved_using_ts_extension(&self, specifier: &str) -> bool {
        let Some(flags) = self.resolved_module_ts_extension_flags.as_ref() else {
            return false;
        };
        for candidate in module_specifier_candidates(specifier) {
            if let Some(&flag) = flags.get(&(self.current_file_idx, candidate)) {
                return flag;
            }
        }
        false
    }
}
