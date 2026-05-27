//! Project-level LSP tests.

use super::*;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tsz_common::position::LineMap;

struct TempWorkspace {
    path: PathBuf,
}

impl TempWorkspace {
    fn new(name: &str) -> Self {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("tsz-{name}-{suffix}"));
        std::fs::create_dir_all(&path).expect("create temp workspace");
        Self { path }
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent directory");
    }
    std::fs::write(path, content).expect("write test file");
}

fn slash_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn apply_text_edits(source: &str, line_map: &LineMap, edits: &[TextEdit]) -> String {
    let mut result = source.to_string();
    let mut edits_with_offsets: Vec<(usize, usize, &TextEdit)> = edits
        .iter()
        .map(|edit| {
            let start = line_map
                .position_to_offset(edit.range.start, source)
                .unwrap_or(0) as usize;
            let end = line_map
                .position_to_offset(edit.range.end, source)
                .unwrap_or(0) as usize;
            (start, end, edit)
        })
        .collect();

    edits_with_offsets.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    for (start, end, edit) in edits_with_offsets {
        result.replace_range(start..end, &edit.new_text);
    }
    result
}

fn range_for_substring(source: &str, line_map: &LineMap, needle: &str) -> Range {
    let start = source.find(needle).expect("substring not found") as u32;
    let end = start + needle.len() as u32;
    let start_pos = line_map.offset_to_position(start, source);
    let end_pos = line_map.offset_to_position(end, source);
    Range::new(start_pos, end_pos)
}

#[path = "project_tests/cache_and_auto_imports.rs"]
mod cache_and_auto_imports;
#[path = "project_tests/discovery_and_references.rs"]
mod discovery_and_references;
#[path = "project_tests/feature_wrappers.rs"]
mod feature_wrappers;
#[path = "project_tests/incremental_updates.rs"]
mod incremental_updates;
#[path = "project_tests/rename_and_navigation.rs"]
mod rename_and_navigation;
#[path = "project_tests/request_cache.rs"]
mod request_cache;
#[path = "project_tests/storage_and_residency.rs"]
mod storage_and_residency;
