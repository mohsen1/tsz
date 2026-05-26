//! Large wildcard-barrel analysis for CLI check scheduling.

use rustc_hash::FxHashMap;
use tsz::parallel::BoundFile;

pub(super) const LARGE_WILDCARD_BARREL_EXPORTS: usize = 32;

pub(super) struct WildcardBarrelAnalysisInput<'a> {
    pub(super) files: &'a [BoundFile],
    pub(super) wildcard_reexports: &'a FxHashMap<String, Vec<String>>,
    pub(super) work_items: &'a [usize],
    pub(super) large_export_threshold: usize,
}

pub(super) fn has_large_wildcard_barrel(input: WildcardBarrelAnalysisInput<'_>) -> bool {
    input.work_items.iter().any(|&file_idx| {
        work_item_file_name(input.files, file_idx)
            .and_then(|file_name| wildcard_export_count(input.wildcard_reexports, file_name))
            .is_some_and(|export_count| export_count >= input.large_export_threshold)
    })
}

fn work_item_file_name(files: &[BoundFile], file_idx: usize) -> Option<&str> {
    files.get(file_idx).map(|file| file.file_name.as_str())
}

fn wildcard_export_count(
    wildcard_reexports: &FxHashMap<String, Vec<String>>,
    file_name: &str,
) -> Option<usize> {
    wildcard_reexports.get(file_name).map(Vec::len)
}
