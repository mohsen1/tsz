//! Auto-import suggestion pipeline — collection, filtering, and rendering.
//!
//! This module is split into responsibility-scoped submodules:
//!
//! - [`collection`]: per-file AST traversal to find matching export declarations
//! - [`filtering`]: exclusion predicates for paths, specifiers, and bare package names
//! - [`position`]: cursor/identifier resolution and import-target navigation
//!
//! Module specifier resolution (computing which path string to use in an import
//! statement) lives in the sibling `module_specifiers` submodule.

mod collection;
mod filtering;
mod position;

use rustc_hash::FxHashSet;

use crate::code_actions::ImportCandidate;
use crate::diagnostics::LspDiagnostic;
use tsz_common::position::{Location, Position};

// Brought into scope for `use super::*` in the test module.
#[cfg(test)]
use crate::code_actions::ImportCandidateKind;
#[cfg(test)]
use tsz_common::position::Range;

use super::import_collect::{
    AutoImportCandidateContext, ImportCandidateCollectionMode, ImportCandidateKey,
    ImportCandidateSink,
};
use super::{ImportKind, Project, ProjectFile};

// Re-export for `import_collect` which references `super::imports::BareSpecifierSourceCache`.
pub(super) use filtering::BareSpecifierSourceCache;

impl Project {
    pub(crate) fn definition_from_import(
        &self,
        file: &ProjectFile,
        position: Position,
    ) -> Option<Vec<Location>> {
        let target = self.import_target_at_position(file, position)?;
        let resolved = self.resolve_module_specifier(file.file_name(), &target.module_specifier)?;
        let target_file = self.files.get(&resolved)?;

        match target.kind {
            ImportKind::Namespace => {
                let location = target_file.node_location(target_file.root())?;
                Some(vec![location])
            }
            ImportKind::Default => {
                let locations = target_file.export_locations("default");
                if locations.is_empty() {
                    None
                } else {
                    Some(locations)
                }
            }
            ImportKind::Named(name) => {
                let locations = target_file.export_locations(&name);
                if locations.is_empty() {
                    None
                } else {
                    Some(locations)
                }
            }
        }
    }

    pub(crate) fn import_candidates_for_diagnostics(
        &self,
        file: &ProjectFile,
        diagnostics: &[LspDiagnostic],
    ) -> Vec<ImportCandidate> {
        let mut candidates = Vec::new();
        let mut seen = FxHashSet::default();

        for diag in diagnostics {
            let diag_code = diag.code.unwrap_or_default();
            if diag_code != tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME
                && diag_code != tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAMESPACE
            {
                continue;
            }

            let Some(missing_name) = self.identifier_at_range(file, diag.range) else {
                continue;
            };

            self.collect_import_candidates_for_name_with_mode(
                file,
                &missing_name,
                diag_code == tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAMESPACE,
                &mut candidates,
                &mut seen,
            );
        }

        candidates
    }

    fn collect_import_candidates_for_name_with_mode(
        &self,
        from_file: &ProjectFile,
        missing_name: &str,
        is_namespace_missing: bool,
        output: &mut Vec<ImportCandidate>,
        seen: &mut FxHashSet<ImportCandidateKey>,
    ) {
        if !self.auto_imports_allowed_for_file(from_file.file_name()) {
            return;
        }
        let all_files: Vec<String> = self.files.keys().cloned().collect();
        let wildcard_reexport_files: Vec<String> = all_files
            .iter()
            .filter(|file_name| self.file_has_wildcard_reexport(file_name))
            .cloned()
            .collect();

        let files_to_check = self.files_to_check_for_symbol(
            missing_name,
            from_file.file_name(),
            &all_files,
            &wildcard_reexport_files,
        );

        let mut context = AutoImportCandidateContext::new(self, from_file, &all_files);
        let mut sink = ImportCandidateSink::new(output, seen);
        let mode = ImportCandidateCollectionMode {
            include_namespace_default: is_namespace_missing,
        };

        if !self.collect_import_candidates_for_symbol_from_files(
            files_to_check,
            missing_name,
            mode,
            &mut context,
            &mut sink,
        ) {
            let fallback_files = all_files
                .into_iter()
                .filter(|file_name| file_name != from_file.file_name())
                .collect();
            let _ = self.collect_import_candidates_for_symbol_from_files(
                fallback_files,
                missing_name,
                mode,
                &mut context,
                &mut sink,
            );
        }
        tracing::trace!(
            module_specifiers_cache_entries = context.module_specifiers_cache_entries(),
            module_specifiers_cache_estimated_size_bytes =
                context.module_specifiers_cache_estimated_size_bytes(),
            "auto-import module specifier cache"
        );
    }

    /// Collect import candidates for symbols matching a prefix.
    ///
    /// This is used for auto-completion when the user has typed a partial
    /// identifier (e.g., "use" should match "useEffect", "useState", etc.).
    pub(crate) fn collect_import_candidates_for_prefix(
        &self,
        from_file: &ProjectFile,
        prefix: &str,
        existing: &FxHashSet<String>,
        output: &mut Vec<ImportCandidate>,
        seen: &mut FxHashSet<ImportCandidateKey>,
    ) {
        if !self.auto_imports_allowed_for_file(from_file.file_name()) {
            return;
        }
        let all_files: Vec<String> = self.files.keys().cloned().collect();
        let wildcard_reexport_files: Vec<String> = all_files
            .iter()
            .filter(|file_name| self.file_has_wildcard_reexport(file_name))
            .cloned()
            .collect();

        let mut supplemental_symbol_set = FxHashSet::default();
        let mut context = AutoImportCandidateContext::new(self, from_file, &all_files);
        let mut sink = ImportCandidateSink::new(output, seen);
        let mode = ImportCandidateCollectionMode {
            include_namespace_default: false,
        };

        // Get all symbols that match the prefix using the sorted symbol index
        let mut matching_symbols = self.symbol_index.get_symbols_with_prefix(prefix);
        if !prefix.is_empty() {
            let mut known_symbols: FxHashSet<String> = matching_symbols.iter().cloned().collect();
            let mut supplemental_symbols = Vec::new();
            for file_name in &all_files {
                for export_name in self.reexported_names_with_prefix(file_name, prefix) {
                    if known_symbols.insert(export_name.clone()) {
                        supplemental_symbol_set.insert(export_name.clone());
                        supplemental_symbols.push(export_name);
                    }
                }
            }
            supplemental_symbols.sort();
            matching_symbols.extend(supplemental_symbols);
        }

        for symbol_name in matching_symbols {
            // Skip if the symbol already exists in the current file (local definition or imported)
            if existing.contains(&symbol_name) {
                continue;
            }

            let mut files_to_check = if supplemental_symbol_set.contains(&symbol_name) {
                all_files.clone()
            } else {
                self.files_to_check_for_symbol(
                    &symbol_name,
                    from_file.file_name(),
                    &all_files,
                    &wildcard_reexport_files,
                )
            };
            if !files_to_check.is_empty()
                && files_to_check
                    .iter()
                    .all(|file_name| file_name == from_file.file_name())
            {
                files_to_check = all_files.clone();
            }

            let _ = self.collect_import_candidates_for_symbol_from_files(
                files_to_check,
                &symbol_name,
                mode,
                &mut context,
                &mut sink,
            );
        }
        tracing::trace!(
            module_specifiers_cache_entries = context.module_specifiers_cache_entries(),
            module_specifiers_cache_estimated_size_bytes =
                context.module_specifiers_cache_estimated_size_bytes(),
            "auto-import prefix module specifier cache"
        );
    }
}

#[cfg(test)]
mod tests;
