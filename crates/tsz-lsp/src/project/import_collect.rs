//! Auto-import candidate collection helpers.
//!
//! The public collection entrypoints remain in `imports`; this module carries
//! request-local state and duplicate suppression so the top-level loops stay
//! orchestration-shaped.

use rustc_hash::{FxHashMap, FxHashSet};
use std::mem;

use crate::code_actions::{ImportCandidate, ImportCandidateKind};

use super::imports::BareSpecifierSourceCache;
use super::{Project, ProjectFile};

pub(super) type ImportCandidateKey = (String, String, String, bool);

pub(super) struct ImportCandidateSink<'a> {
    output: &'a mut Vec<ImportCandidate>,
    seen: &'a mut FxHashSet<ImportCandidateKey>,
}

impl<'a> ImportCandidateSink<'a> {
    pub(super) const fn new(
        output: &'a mut Vec<ImportCandidate>,
        seen: &'a mut FxHashSet<ImportCandidateKey>,
    ) -> Self {
        Self { output, seen }
    }

    pub(super) const fn len(&self) -> usize {
        self.output.len()
    }

    pub(super) fn push(&mut self, candidate: ImportCandidate) {
        if self.seen.insert((
            candidate.module_specifier.clone(),
            candidate.local_name.clone(),
            import_candidate_kind_key(&candidate.kind),
            candidate.is_type_only,
        )) {
            self.output.push(candidate);
        }
    }
}

pub(super) struct AutoImportCandidateContext<'a> {
    from_file: &'a ProjectFile,
    allowed_packages: Option<FxHashSet<String>>,
    existing_imported_packages: FxHashSet<String>,
    source_cache: BareSpecifierSourceCache,
    module_specifiers_cache: FxHashMap<String, Vec<String>>,
    excluded_file_set: FxHashSet<String>,
}

impl<'a> AutoImportCandidateContext<'a> {
    pub(super) fn new(project: &Project, from_file: &'a ProjectFile, all_files: &[String]) -> Self {
        let excluded_file_set = if project.auto_import_file_exclude_matchers.is_empty() {
            FxHashSet::default()
        } else {
            all_files
                .iter()
                .filter(|file_name| project.auto_import_path_is_excluded(file_name))
                .cloned()
                .collect()
        };

        Self {
            from_file,
            allowed_packages: project.allowed_dependency_package_names(from_file.file_name()),
            existing_imported_packages: Project::imported_package_names(from_file),
            source_cache: BareSpecifierSourceCache::default(),
            module_specifiers_cache: FxHashMap::default(),
            excluded_file_set,
        }
    }

    pub(super) fn request_file_name(&self) -> &str {
        self.from_file.file_name()
    }

    pub(super) fn module_specifiers_cache_entries(&self) -> usize {
        self.module_specifiers_cache.len()
    }

    pub(super) fn module_specifiers_cache_estimated_size_bytes(&self) -> usize {
        module_specifiers_cache_estimated_size_bytes(&self.module_specifiers_cache)
    }

    pub(super) fn is_regular_file_excluded(&self, file_name: &str) -> bool {
        self.excluded_file_set.contains(file_name)
    }

    pub(super) fn is_ambient_module_candidate_excluded(
        &mut self,
        project: &Project,
        module_specifier: &str,
    ) -> bool {
        let Self {
            from_file,
            allowed_packages,
            existing_imported_packages,
            source_cache,
            ..
        } = self;

        project.is_ambient_module_candidate_excluded(
            module_specifier,
            from_file.source_text(),
            allowed_packages.as_ref(),
            existing_imported_packages,
            source_cache,
        )
    }

    pub(super) fn has_module_specifiers_for(&mut self, project: &Project, file_name: &str) -> bool {
        let from_file_name = self.from_file.file_name().to_string();
        !self
            .module_specifiers_cache
            .entry(file_name.to_string())
            .or_insert_with(|| {
                project.auto_import_module_specifiers_from_files(&from_file_name, file_name)
            })
            .is_empty()
    }

    pub(super) fn first_allowed_module_specifier(
        &mut self,
        project: &Project,
        file_name: &str,
    ) -> Option<String> {
        let Self {
            from_file,
            allowed_packages,
            existing_imported_packages,
            source_cache,
            module_specifiers_cache,
            ..
        } = self;

        let module_specifiers = module_specifiers_cache
            .entry(file_name.to_string())
            .or_insert_with(|| {
                project.auto_import_module_specifiers_from_files(from_file.file_name(), file_name)
            });

        module_specifiers
            .iter()
            .find(|module_specifier| {
                !project.is_auto_import_candidate_excluded(
                    file_name,
                    module_specifier,
                    from_file.source_text(),
                    allowed_packages.as_ref(),
                    existing_imported_packages,
                    source_cache,
                )
            })
            .cloned()
    }

    /// All non-excluded module specifiers that reach `file_name`, in ranked
    /// order. Unlike `first_allowed_module_specifier`, this keeps every
    /// distinct specifier form (e.g. a package.json `imports` subpath *and*
    /// the plain relative path to the same target) — tsc's `getNewImportFixes`
    /// (`codefixes/importFixes.ts`) maps a fix over every entry `getModuleSpecifiers`
    /// returns for a candidate declaration, not just the best one, so a missing-import
    /// quickfix must offer one action per specifier. Auto-import completions use the
    /// single-best specifier instead (`getModuleSpecifierForBestExportInfo` in
    /// `completions.ts`), so callers on that path should keep using
    /// `first_allowed_module_specifier`.
    pub(super) fn allowed_module_specifiers(
        &mut self,
        project: &Project,
        file_name: &str,
    ) -> Vec<String> {
        let Self {
            from_file,
            allowed_packages,
            existing_imported_packages,
            source_cache,
            module_specifiers_cache,
            ..
        } = self;

        let module_specifiers = module_specifiers_cache
            .entry(file_name.to_string())
            .or_insert_with(|| {
                project.auto_import_module_specifiers_from_files(from_file.file_name(), file_name)
            });

        module_specifiers
            .iter()
            .filter(|module_specifier| {
                !project.is_auto_import_candidate_excluded(
                    file_name,
                    module_specifier,
                    from_file.source_text(),
                    allowed_packages.as_ref(),
                    existing_imported_packages,
                    source_cache,
                )
            })
            .cloned()
            .collect()
    }
}

#[derive(Clone, Copy)]
pub(super) struct ImportCandidateCollectionMode {
    pub(super) include_namespace_default: bool,
    /// When true, emit one `ImportCandidate` per allowed module specifier for
    /// a matching export instead of only the best one. Set for the
    /// missing-import code-fix path (matches tsc's `getNewImportFixes`);
    /// left `false` for completions, which show a single best specifier per
    /// symbol.
    pub(super) emit_all_specifiers: bool,
}

fn import_candidate_kind_key(kind: &ImportCandidateKind) -> String {
    match kind {
        ImportCandidateKind::Named { export_name } => format!("named:{export_name}"),
        ImportCandidateKind::Default => "default".to_string(),
        ImportCandidateKind::Namespace => "namespace".to_string(),
    }
}

fn module_specifiers_cache_estimated_size_bytes(cache: &FxHashMap<String, Vec<String>>) -> usize {
    let entries_size = cache.capacity().saturating_mul(
        mem::size_of::<String>()
            .saturating_add(mem::size_of::<Vec<String>>())
            .saturating_add(8),
    );
    let key_size = cache.keys().map(String::len).sum::<usize>();
    let value_size = cache
        .values()
        .map(|specifiers| {
            specifiers
                .capacity()
                .saturating_mul(mem::size_of::<String>())
                .saturating_add(specifiers.iter().map(String::len).sum::<usize>())
        })
        .sum::<usize>();
    entries_size
        .saturating_add(key_size)
        .saturating_add(value_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_specifiers_cache_statistics_report_entries_and_size() {
        let mut cache = FxHashMap::default();
        assert_eq!(module_specifiers_cache_estimated_size_bytes(&cache), 0);

        cache.insert(
            "/workspace/src/source.ts".to_string(),
            vec!["./source".to_string(), "pkg/source".to_string()],
        );

        assert_eq!(cache.len(), 1);
        assert!(module_specifiers_cache_estimated_size_bytes(&cache) > 0);
    }
}
