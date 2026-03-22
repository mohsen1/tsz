//! Memory pressure eviction for LSP projects.
//!
//! When the project grows large (many files loaded), memory pressure can
//! degrade responsiveness. This module provides:
//!
//! - [`EvictionCandidate`]: a ranked entry describing a file eligible for eviction.
//! - [`Project::eviction_candidates`]: returns files ranked by eviction priority.
//! - [`Project::evict_under_pressure`]: removes files until total estimated bytes
//!   drops below a target budget.
//! - [`Project::mark_file_open`] / [`Project::mark_file_closed`]: tracks which
//!   files are actively open in the editor (never evicted).
//!
//! # Eviction Strategy
//!
//! Files are ranked for eviction using this priority (highest priority = evicted first):
//!
//! 1. Files with **zero dependents** (no other file imports them) are evicted
//!    before files that are imported.
//! 2. Among files with equal dependent status, **larger files** are evicted first
//!    to reclaim the most memory per eviction.
//! 3. Files that are **open in the editor** are never evicted.

use super::Project;

/// A file eligible for eviction, with metadata for ranking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvictionCandidate {
    /// File name (key in `Project::files`).
    pub file_name: String,
    /// Estimated heap footprint in bytes.
    pub estimated_bytes: usize,
    /// Number of files that directly import this file.
    pub dependent_count: usize,
    /// Whether this file is open in the editor.
    pub is_open: bool,
}

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

    /// Return files ranked for eviction.
    ///
    /// Files open in the editor are included in the list (with `is_open = true`)
    /// but are sorted to the end and will be skipped by [`evict_under_pressure`].
    ///
    /// The returned list is sorted by eviction priority (highest priority first):
    /// 1. Closed files before open files.
    /// 2. Files with zero dependents before files with dependents.
    /// 3. Larger files before smaller files.
    pub fn eviction_candidates(&self) -> Vec<EvictionCandidate> {
        let mut candidates: Vec<EvictionCandidate> = self
            .files
            .iter()
            .map(|(name, file)| {
                let dependent_count = self
                    .dependency_graph
                    .get_dependents(name)
                    .map_or(0, |deps| deps.len());
                EvictionCandidate {
                    file_name: name.clone(),
                    estimated_bytes: file.estimated_size_bytes(),
                    dependent_count,
                    is_open: self.open_files.contains(name),
                }
            })
            .collect();

        // Sort: open files last, then zero-dependents first, then largest first.
        candidates.sort_by(|a, b| {
            a.is_open
                .cmp(&b.is_open)
                .then_with(|| {
                    let a_has_deps = a.dependent_count > 0;
                    let b_has_deps = b.dependent_count > 0;
                    a_has_deps.cmp(&b_has_deps)
                })
                .then_with(|| b.estimated_bytes.cmp(&a.estimated_bytes))
        });

        candidates
    }

    /// Evict files until the total estimated memory drops below `target_bytes`.
    ///
    /// Returns an [`EvictionResult`] describing what was evicted. Files that
    /// are open in the editor are never evicted.
    ///
    /// If the total is already below the target, no files are evicted and the
    /// result will have an empty `evicted` list.
    pub fn evict_under_pressure(&mut self, target_bytes: usize) -> EvictionResult {
        let mut total = self.total_estimated_bytes();

        if total <= target_bytes {
            return EvictionResult {
                evicted: Vec::new(),
                bytes_freed: 0,
                bytes_remaining: total,
            };
        }

        // Collect candidates (sorted by eviction priority).
        let candidates = self.eviction_candidates();

        let mut evicted = Vec::new();
        let mut bytes_freed: usize = 0;

        for candidate in candidates {
            if total <= target_bytes {
                break;
            }
            if candidate.is_open {
                // Never evict open files; since they're sorted last, we can break.
                break;
            }

            let file_bytes = candidate.estimated_bytes;
            let file_name = candidate.file_name;

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
    fn total_estimated_bytes(&self) -> usize {
        self.files
            .values()
            .map(|f| f.estimated_size_bytes())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_project_with_files(names: &[&str]) -> Project {
        let mut project = Project::new();
        for name in names {
            // Small source text; estimated_size_bytes will still be > 0
            // because of struct overhead and binder state.
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
    fn eviction_candidates_returns_all_files() {
        let project = make_project_with_files(&["a.ts", "b.ts", "c.ts"]);
        let candidates = project.eviction_candidates();
        assert_eq!(candidates.len(), 3);
        // All should be closed (is_open = false)
        assert!(candidates.iter().all(|c| !c.is_open));
    }

    #[test]
    fn open_files_sorted_last() {
        let mut project = make_project_with_files(&["a.ts", "b.ts", "c.ts"]);
        project.mark_file_open("b.ts");
        let candidates = project.eviction_candidates();

        // b.ts should be last (it's open)
        assert_eq!(candidates.last().unwrap().file_name, "b.ts");
        assert!(candidates.last().unwrap().is_open);
        // Others should not be open
        assert!(candidates.iter().take(2).all(|c| !c.is_open));
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

        // Set target to 0 to force evicting everything.
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

        // Force evict everything possible.
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

        // Find total and set target to roughly half.
        let total = project.total_estimated_bytes();
        let target = total / 2;

        let result = project.evict_under_pressure(target);

        // Should have evicted some but not all files.
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
}
