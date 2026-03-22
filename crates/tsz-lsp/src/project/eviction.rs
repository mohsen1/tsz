//! Memory pressure eviction for LSP projects.
//!
//! When the project grows large (many files loaded), memory pressure can
//! degrade responsiveness. This module provides:
//!
//! - [`Project::mark_file_open`] / [`Project::mark_file_closed`]: tracks which
//!   files are actively open in the editor (never evicted).
//! - [`Project::evict_under_pressure`]: removes files until total estimated bytes
//!   drops below a target budget, respecting open-file protection.
//!
//! # Eviction Strategy
//!
//! Files are ranked for eviction using this priority (highest priority = evicted first):
//!
//! 1. Files that are **open in the editor** are never evicted.
//! 2. Files with **zero dependents** (no other file imports them) are evicted
//!    before files that are imported.
//! 3. Declaration files (`*.d.ts`) are deprioritized (divided score by 4).
//! 4. Among files with equal priority, **larger files** are evicted first
//!    to reclaim the most memory per eviction.

use super::Project;

/// Result of an eviction pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvictionResult {
    /// Files that were evicted.
    pub evicted: Vec<EvictedFile>,
    /// Total bytes freed by eviction.
    pub bytes_freed: usize,
    /// Estimated total bytes remaining after eviction.
    pub bytes_remaining: usize,
}

/// Record of a single evicted file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvictedFile {
    /// File name that was removed.
    pub file_name: String,
    /// Estimated bytes freed by removing this file.
    pub estimated_bytes: usize,
}

/// Internal ranking entry for eviction decisions.
struct EvictionEntry {
    file_name: String,
    estimated_bytes: usize,
    has_dependents: bool,
    is_declaration: bool,
}

impl EvictionEntry {
    /// Composite eviction score (higher = evict first).
    const fn score(&self) -> u64 {
        let size = self.estimated_bytes as u64;
        // Files with dependents get a 8x penalty (lower score).
        let dep_factor = if self.has_dependents { size / 8 } else { size };
        // Declaration files get an additional 4x penalty.
        if self.is_declaration {
            dep_factor / 4
        } else {
            dep_factor
        }
    }
}

impl Project {
    /// Mark a file as open in the editor.
    ///
    /// Open files are never evicted. Call this when the LSP receives
    /// `textDocument/didOpen`.
    pub fn mark_file_open(&mut self, file_name: &str) {
        self.open_files.insert(file_name.to_string());
    }

    /// Mark a file as closed in the editor.
    ///
    /// Closed files become eligible for eviction. Call this when the LSP
    /// receives `textDocument/didClose`.
    pub fn mark_file_closed(&mut self, file_name: &str) {
        self.open_files.remove(file_name);
    }

    /// Whether a file is currently open in the editor.
    pub fn is_file_open(&self, file_name: &str) -> bool {
        self.open_files.contains(file_name)
    }

    /// Number of files currently open in the editor.
    #[cfg(test)]
    pub fn open_file_count(&self) -> usize {
        self.open_files.len()
    }

    /// Evict files until the total estimated memory drops below `target_bytes`.
    ///
    /// Returns an [`EvictionResult`] describing what was evicted. Files that
    /// are open in the editor are never evicted.
    ///
    /// If the total is already below the target, no files are evicted and the
    /// result will have an empty `evicted` list.
    ///
    /// # Ranking
    ///
    /// Files are ranked by a composite score that considers:
    /// - **Estimated size** (larger = evicted first)
    /// - **Dependents** (files that no one imports are evicted first)
    /// - **Declaration files** (`.d.ts` are deprioritized, kept longer)
    /// - **Open status** (open files are never evicted)
    pub fn evict_under_pressure(&mut self, target_bytes: usize) -> EvictionResult {
        let mut total = self.total_estimated_bytes();

        if total <= target_bytes {
            return EvictionResult {
                evicted: Vec::new(),
                bytes_freed: 0,
                bytes_remaining: total,
            };
        }

        // Build ranked eviction list, excluding open files.
        let mut entries: Vec<EvictionEntry> = self
            .files
            .iter()
            .filter(|(name, _)| !self.open_files.contains(name.as_str()))
            .map(|(name, file)| {
                let has_dependents = self
                    .dependency_graph
                    .get_dependents(name)
                    .is_some_and(|deps| !deps.is_empty());
                EvictionEntry {
                    file_name: name.clone(),
                    estimated_bytes: file.estimated_size_bytes(),
                    has_dependents,
                    is_declaration: name.ends_with(".d.ts"),
                }
            })
            .collect();

        // Sort by score descending (highest score = evict first).
        entries.sort_by_key(|b| std::cmp::Reverse(b.score()));

        let mut evicted = Vec::new();
        let mut bytes_freed: usize = 0;

        for entry in entries {
            if total <= target_bytes {
                break;
            }

            let file_bytes = entry.estimated_bytes;
            let file_name = entry.file_name;

            if self.remove_file(&file_name).is_some() {
                total = total.saturating_sub(file_bytes);
                bytes_freed = bytes_freed.saturating_add(file_bytes);
                evicted.push(EvictedFile {
                    file_name,
                    estimated_bytes: file_bytes,
                });

                tracing::info!(
                    evicted_file = %evicted.last().unwrap().file_name,
                    freed_bytes = file_bytes,
                    remaining_total = total,
                    target = target_bytes,
                    "eviction: removed file under memory pressure"
                );
            }
        }

        EvictionResult {
            evicted,
            bytes_freed,
            bytes_remaining: total,
        }
    }

    /// Sum of `estimated_size_bytes()` across all files.
    ///
    /// This is a convenience wrapper that avoids computing the full
    /// [`ProjectResidencyStats`] when only the total is needed.
    pub(crate) fn total_estimated_bytes(&self) -> usize {
        self.files.values().map(|f| f.estimated_size_bytes()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_project_with_files(names: &[&str]) -> Project {
        let mut project = Project::new();
        for name in names {
            project.set_file(name.to_string(), format!("const x_{name} = 1;"));
        }
        project
    }

    #[test]
    fn open_close_tracking() {
        let mut project = make_project_with_files(&["a.ts", "b.ts"]);
        assert!(!project.is_file_open("a.ts"));

        project.mark_file_open("a.ts");
        assert!(project.is_file_open("a.ts"));
        assert!(!project.is_file_open("b.ts"));
        assert_eq!(project.open_file_count(), 1);

        project.mark_file_closed("a.ts");
        assert!(!project.is_file_open("a.ts"));
        assert_eq!(project.open_file_count(), 0);
    }

    #[test]
    fn evict_under_pressure_no_eviction_when_under_target() {
        let mut project = make_project_with_files(&["a.ts"]);
        let result = project.evict_under_pressure(usize::MAX);
        assert!(result.evicted.is_empty());
        assert_eq!(result.bytes_freed, 0);
    }

    #[test]
    fn evict_under_pressure_removes_files() {
        let mut project = make_project_with_files(&["a.ts", "b.ts", "c.ts"]);
        assert_eq!(project.file_count(), 3);

        let result = project.evict_under_pressure(0);
        assert_eq!(result.evicted.len(), 3);
        assert_eq!(project.file_count(), 0);
        assert!(result.bytes_freed > 0);
        assert_eq!(result.bytes_remaining, 0);
    }

    #[test]
    fn evict_under_pressure_skips_open_files() {
        let mut project = make_project_with_files(&["a.ts", "b.ts", "c.ts"]);
        project.mark_file_open("b.ts");

        let result = project.evict_under_pressure(0);

        // b.ts should survive (it's open).
        assert_eq!(project.file_count(), 1);
        assert!(project.files.contains_key("b.ts"));

        // Only a.ts and c.ts should have been evicted.
        assert_eq!(result.evicted.len(), 2);
        assert!(result.evicted.iter().all(|e| e.file_name != "b.ts"));
    }

    #[test]
    fn evict_partial_when_target_reached() {
        let mut project = make_project_with_files(&["a.ts", "b.ts", "c.ts", "d.ts"]);

        let total = project.total_estimated_bytes();
        let target = total / 2;

        let result = project.evict_under_pressure(target);

        assert!(!result.evicted.is_empty());
        assert!(project.file_count() > 0);
        assert!(result.bytes_remaining <= target);
    }

    #[test]
    fn eviction_result_bytes_accounting() {
        let mut project = make_project_with_files(&["a.ts", "b.ts"]);
        let total_before = project.total_estimated_bytes();

        let result = project.evict_under_pressure(0);

        assert_eq!(result.bytes_freed, total_before);
        assert_eq!(result.bytes_remaining, 0);
    }

    #[test]
    fn declaration_files_evicted_after_source_files() {
        let mut project = Project::new();
        // Add source and declaration files with similar content.
        project.set_file(
            "lib.d.ts".to_string(),
            "declare const x: number;".to_string(),
        );
        project.set_file("app.ts".to_string(), "const x: number = 42;".to_string());

        let total = project.total_estimated_bytes();
        // Set target so only one file is evicted.
        let target = total / 2;

        let result = project.evict_under_pressure(target);

        // app.ts (source) should be evicted before lib.d.ts (declaration).
        assert_eq!(result.evicted.len(), 1);
        assert_eq!(result.evicted[0].file_name, "app.ts");
        assert!(project.files.contains_key("lib.d.ts"));
    }

    #[test]
    fn multiple_open_files_all_protected() {
        let mut project = make_project_with_files(&["a.ts", "b.ts", "c.ts"]);
        project.mark_file_open("a.ts");
        project.mark_file_open("b.ts");
        project.mark_file_open("c.ts");

        let result = project.evict_under_pressure(0);

        // All files are open, so none should be evicted.
        assert!(result.evicted.is_empty());
        assert_eq!(project.file_count(), 3);
    }
}
