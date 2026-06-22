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

impl Project {
    /// Mark a file as open in the editor.
    ///
    /// Open files are never evicted. Call this when the LSP receives
    /// `textDocument/didOpen`. Also promotes the file to "focused" so
    /// fuzzy ranking in workspace-symbol search prefers nearby files.
    pub fn mark_file_open(&mut self, file_name: &str) {
        self.open_files.insert(file_name.to_string());
        self.focused_file = Some(file_name.to_string());
    }

    /// Mark a file as closed in the editor.
    ///
    /// Closed files become eligible for eviction. Call this when the LSP
    /// receives `textDocument/didClose`. If the closed file was focused,
    /// the focus is cleared.
    pub fn mark_file_closed(&mut self, file_name: &str) {
        self.open_files.remove(file_name);
        if self.focused_file.as_deref() == Some(file_name) {
            self.focused_file = None;
        }
    }

    /// Record the file the editor is currently focused on.
    ///
    /// Call this from any LSP request that carries a `textDocument.uri`
    /// (`didChange`, `hover`, `definition`, `completion`, ...). The hint
    /// is used as a tie-breaker for fuzzy-ranked workspace-symbol search.
    pub fn set_focused_file(&mut self, file_name: &str) {
        self.focused_file = Some(file_name.to_string());
    }

    /// Get the currently focused file, if any.
    pub fn focused_file(&self) -> Option<&str> {
        self.focused_file.as_deref()
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
    /// Candidates come from the shared [`Project::eviction_candidates`] ranking
    /// (coldest-and-largest first: `idle_seconds * estimated_bytes`, with `.d.ts`
    /// declaration files deprioritized since they are typically shared
    /// dependencies). Files open in the editor are filtered out and never
    /// evicted.
    pub fn evict_under_pressure(&mut self, target_bytes: usize) -> EvictionResult {
        // Rank cold, large files first via the shared residency ranking,
        // excluding files currently open in the editor (never evicted).
        let ranked: Vec<(String, usize)> = self
            .eviction_candidates(None)
            .into_iter()
            .filter(|info| !self.open_files.contains(&info.file_name))
            .map(|info| (info.file_name, info.estimated_bytes))
            .collect();
        self.evict_ranked(ranked, target_bytes)
    }

    /// Evict cold, safely-droppable files when the project exceeds its
    /// configured memory budget ([`Project::set_memory_budget`]).
    ///
    /// No-op when no budget is configured (the default), so eviction is
    /// strictly opt-in. Only files that are **all** of:
    ///
    /// 1. not open in the editor,
    /// 2. imported by no other file (zero dependents), and
    /// 3. byte-identical to their on-disk contents,
    ///
    /// are evicted. The zero-dependents requirement guarantees that no
    /// remaining file's analysis depends on an evicted file, so diagnostics are
    /// unchanged; the disk-identity requirement guarantees an evicted file can
    /// be rehydrated losslessly via [`Project::ensure_file_loaded`] when a later
    /// request targets it directly.
    ///
    /// Files are dropped in coldest-and-largest-first order (the LRU ranking
    /// produced by [`Project::eviction_candidates`]) until the footprint drops
    /// to or below the budget, or no further file is safe to evict.
    pub fn evict_if_over_budget(&mut self) -> EvictionResult {
        let total = self.total_estimated_bytes();
        // Disabled (no budget) or already within budget: skip ranking entirely.
        let within_budget = EvictionResult {
            evicted: Vec::new(),
            bytes_freed: 0,
            bytes_remaining: total,
        };
        let Some(budget) = self.memory_budget_bytes else {
            return within_budget;
        };
        if total <= budget {
            return within_budget;
        }
        let ranked: Vec<(String, usize)> = self
            .eviction_candidates(None)
            .into_iter()
            .filter(|info| self.is_safely_evictable(&info.file_name))
            .map(|info| (info.file_name, info.estimated_bytes))
            .collect();
        self.evict_ranked(ranked, budget)
    }

    /// Whether a file can be dropped under memory pressure without affecting
    /// any other file's analysis or losing unsaved editor state.
    ///
    /// See [`Project::evict_if_over_budget`] for the three conditions.
    fn is_safely_evictable(&self, file_name: &str) -> bool {
        if self.open_files.contains(file_name) {
            return false;
        }
        let has_dependents = self
            .dependency_graph
            .get_dependents(file_name)
            .is_some_and(|deps| !deps.is_empty());
        if has_dependents {
            return false;
        }
        self.in_memory_matches_disk(file_name)
    }

    /// Reload a previously-evicted file from disk if it is currently missing.
    ///
    /// Returns `true` when the file is present after the call — either it was
    /// already loaded, or it was successfully rehydrated from disk. The LSP
    /// request path calls this so a request targeting an evicted file
    /// transparently reloads it. Files with no on-disk backing (e.g. untitled
    /// buffers) cannot be rehydrated and return `false`.
    pub fn ensure_file_loaded(&mut self, file_name: &str) -> bool {
        if self.files.contains_key(file_name) {
            return true;
        }
        match std::fs::read_to_string(file_name) {
            Ok(content) => {
                self.set_file(file_name.to_string(), content);
                true
            }
            Err(_) => false,
        }
    }

    /// Remove ranked files (best candidate first) until the total estimated
    /// footprint drops to or below `target_bytes`.
    fn evict_ranked(
        &mut self,
        ranked: Vec<(String, usize)>,
        target_bytes: usize,
    ) -> EvictionResult {
        let mut total = self.total_estimated_bytes();
        let mut evicted = Vec::new();
        let mut bytes_freed: usize = 0;

        if total <= target_bytes {
            return EvictionResult {
                evicted,
                bytes_freed,
                bytes_remaining: total,
            };
        }

        for (file_name, estimated_bytes) in ranked {
            if total <= target_bytes {
                break;
            }

            if self.remove_file(&file_name).is_some() {
                total = total.saturating_sub(estimated_bytes);
                bytes_freed = bytes_freed.saturating_add(estimated_bytes);
                tracing::info!(
                    evicted_file = %file_name,
                    freed_bytes = estimated_bytes,
                    remaining_total = total,
                    target = target_bytes,
                    "eviction: removed file under memory pressure"
                );
                evicted.push(EvictedFile {
                    file_name,
                    estimated_bytes,
                });
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
            project.set_file((*name).to_string(), format!("const x_{name} = 1;"));
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

    // ── Opt-in, disk-backed memory-budget eviction ──────────────────────

    /// Create a fresh, unique on-disk directory for a single test and return
    /// its path. Cleaned up best-effort by [`TempDir::drop`].
    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let mut dir = std::env::temp_dir();
            dir.push(format!(
                "tsz_lsp_evict_{tag}_{}_{nanos}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).expect("create temp dir");
            Self(dir)
        }

        /// Write `content` to `name` inside the temp dir, returning the absolute
        /// path string used as the project's file name.
        fn write(&self, name: &str, content: &str) -> String {
            let path = self.0.join(name);
            std::fs::write(&path, content).expect("write temp file");
            path.to_string_lossy().to_string()
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn evict_if_over_budget_is_noop_without_budget() {
        // Default: no budget configured -> eviction never runs.
        let mut project = make_project_with_files(&["a.ts", "b.ts", "c.ts"]);
        assert_eq!(project.memory_budget_bytes(), None);

        let result = project.evict_if_over_budget();

        assert!(result.evicted.is_empty());
        assert_eq!(project.file_count(), 3);
    }

    #[test]
    fn evict_if_over_budget_drops_clean_disk_backed_files() {
        let tmp = TempDir::new("clean");
        let a = tmp.write("a.ts", "export const a = 1;\n");
        let b = tmp.write("b.ts", "export const b = 2;\n");

        let mut project = Project::new();
        project.set_file(a.clone(), "export const a = 1;\n".to_string());
        project.set_file(b, "export const b = 2;\n".to_string());
        assert_eq!(project.file_count(), 2);

        // Budget below the current footprint forces eviction. Neither file is
        // open or imported, and both match disk, so both are safe to drop.
        project.set_memory_budget(Some(0));
        let result = project.evict_if_over_budget();

        assert!(!result.evicted.is_empty());
        assert_eq!(project.file_count(), 0);

        // An evicted, on-disk file rehydrates transparently.
        assert!(project.ensure_file_loaded(&a));
        assert_eq!(project.file_count(), 1);
        assert!(project.file(&a).is_some());
    }

    #[test]
    fn evict_if_over_budget_keeps_files_with_unsaved_changes() {
        let tmp = TempDir::new("dirty");
        // On-disk content differs from the in-memory (edited-but-unsaved) buffer.
        let a = tmp.write("a.ts", "export const a = 1;\n");

        let mut project = Project::new();
        project.set_file(a, "export const a = 999; // unsaved edit\n".to_string());

        project.set_memory_budget(Some(0));
        let result = project.evict_if_over_budget();

        // Dropping it would lose the unsaved edit, so it must be retained.
        assert!(result.evicted.is_empty());
        assert_eq!(project.file_count(), 1);
    }

    #[test]
    fn evict_if_over_budget_keeps_files_with_dependents() {
        let tmp = TempDir::new("deps");
        let lib = tmp.write("lib.ts", "export const v = 1;\n");
        let app = tmp.write("app.ts", "export const v = 1;\n");

        let mut project = Project::new();
        project.set_file(lib.clone(), "export const v = 1;\n".to_string());
        project.set_file(app.clone(), "export const v = 1;\n".to_string());
        // `app` imports `lib`, so `lib` has a dependent and must not be evicted
        // (its removal would break `app`'s analysis); `app` itself is a leaf.
        project.dependency_graph.add_dependency(&app, &lib);

        project.set_memory_budget(Some(0));
        let result = project.evict_if_over_budget();

        assert!(project.file(&lib).is_some(), "imported file must survive");
        assert!(
            result.evicted.iter().all(|e| e.file_name != lib),
            "imported file must not be evicted"
        );
        assert!(project.file(&app).is_none(), "leaf file should be evicted");
    }

    #[test]
    fn evict_if_over_budget_protects_open_files() {
        let tmp = TempDir::new("open");
        let a = tmp.write("a.ts", "export const a = 1;\n");

        let mut project = Project::new();
        project.set_file(a.clone(), "export const a = 1;\n".to_string());
        project.mark_file_open(&a);

        project.set_memory_budget(Some(0));
        let result = project.evict_if_over_budget();

        assert!(result.evicted.is_empty());
        assert!(project.file(&a).is_some());
    }

    #[test]
    fn ensure_file_loaded_returns_false_for_missing_file() {
        let mut project = Project::new();
        assert!(!project.ensure_file_loaded("/no/such/path/does-not-exist.ts"));
        assert_eq!(project.file_count(), 0);
    }
}
