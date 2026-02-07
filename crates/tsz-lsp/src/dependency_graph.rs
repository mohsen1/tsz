//! Reverse dependency graph for efficient incremental cache invalidation.
//!
//! This module tracks file dependencies bidirectionally to enable efficient
//! invalidation of type caches when a file changes. When file A imports file B,
//! we track both:
//! - `dependencies`: A -> {B} (what A imports)
//! - `dependents`: B -> {A} (what imports B)
//!
//! On file change, we use `dependents` to find the transitive closure of all
//! files that need their caches invalidated.

use rustc_hash::{FxHashMap, FxHashSet};

/// Bidirectional dependency graph for tracking file imports.
///
/// Maintains both forward (`dependencies`) and reverse (`dependents`) mappings
/// for O(1) lookups in either direction.
#[derive(Default, Debug)]
pub struct DependencyGraph {
    /// Forward dependencies: file -> files it imports
    dependencies: FxHashMap<String, FxHashSet<String>>,
    /// Reverse dependencies: file -> files that import it
    dependents: FxHashMap<String, FxHashSet<String>>,
}

impl DependencyGraph {
    /// Create a new empty dependency graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a single dependency edge: `file` imports `imported_file`.
    pub fn add_dependency(&mut self, file: &str, imported_file: &str) {
        // Add to forward dependencies
        self.dependencies
            .entry(file.to_string())
            .or_default()
            .insert(imported_file.to_string());

        // Add to reverse dependencies
        self.dependents
            .entry(imported_file.to_string())
            .or_default()
            .insert(file.to_string());
    }

    /// Get all files that transitively depend on the given file.
    ///
    /// This performs a breadth-first traversal of the reverse dependency graph
    /// to find all files that directly or indirectly import the changed file.
    /// The returned set does NOT include the original file itself.
    ///
    /// Handles cycles correctly by tracking visited nodes.
    pub fn get_affected_files(&self, file: &str) -> Vec<String> {
        let mut affected = FxHashSet::default();
        let mut stack = vec![file.to_string()];

        while let Some(current) = stack.pop() {
            if let Some(deps) = self.dependents.get(&current) {
                for dep in deps {
                    // Only process each file once to handle cycles
                    if affected.insert(dep.clone()) {
                        stack.push(dep.clone());
                    }
                }
            }
        }

        affected.into_iter().collect()
    }

    /// Update dependencies for a file with a new set of imports.
    ///
    /// This atomically removes old dependency edges and adds new ones.
    /// Handles the case where some imports are unchanged efficiently.
    pub fn update_file(&mut self, file: &str, imports: &[String]) {
        // 1. Remove old edges from 'dependents'
        if let Some(old_imports) = self.dependencies.get(file) {
            for imported in old_imports.iter() {
                if let Some(rev) = self.dependents.get_mut(imported) {
                    rev.remove(file);
                    // Clean up empty sets to avoid memory leaks
                    if rev.is_empty() {
                        self.dependents.remove(imported);
                    }
                }
            }
        }

        // 2. Update forward dependencies
        if imports.is_empty() {
            self.dependencies.remove(file);
        } else {
            let new_set: FxHashSet<String> = imports.iter().cloned().collect();
            self.dependencies.insert(file.to_string(), new_set);

            // 3. Add new edges to 'dependents'
            for imported in imports {
                self.dependents
                    .entry(imported.clone())
                    .or_default()
                    .insert(file.to_string());
            }
        }
    }

    /// Remove a file completely from the dependency graph.
    ///
    /// Removes:
    /// - All outgoing dependency edges (what this file imports)
    /// - All incoming dependency edges (files that import this file)
    /// - The file's own entry in both maps
    pub fn remove_file(&mut self, file: &str) {
        // Remove outgoing edges: this file's imports
        if let Some(old_imports) = self.dependencies.remove(file) {
            for imported in old_imports {
                if let Some(rev) = self.dependents.get_mut(&imported) {
                    rev.remove(file);
                    if rev.is_empty() {
                        self.dependents.remove(&imported);
                    }
                }
            }
        }

        // Remove incoming edges: files that import this file
        if let Some(old_dependents) = self.dependents.remove(file) {
            for dependent in old_dependents {
                if let Some(deps) = self.dependencies.get_mut(&dependent) {
                    deps.remove(file);
                    // Note: We don't remove the dependent's entry even if empty,
                    // because the file still exists, just with fewer imports
                }
            }
        }
    }

    /// Get the direct dependencies of a file (files it imports).
    pub fn get_dependencies(&self, file: &str) -> Option<&FxHashSet<String>> {
        self.dependencies.get(file)
    }

    /// Get the direct dependents of a file (files that import it).
    pub fn get_dependents(&self, file: &str) -> Option<&FxHashSet<String>> {
        self.dependents.get(file)
    }

    /// Check if the graph contains any information about a file.
    pub fn contains_file(&self, file: &str) -> bool {
        self.dependencies.contains_key(file) || self.dependents.contains_key(file)
    }

    /// Get the total number of files tracked in the graph.
    pub fn file_count(&self) -> usize {
        let mut files = FxHashSet::default();
        files.extend(self.dependencies.keys().cloned());
        files.extend(self.dependents.keys().cloned());
        files.len()
    }

    /// Clear all data from the graph.
    pub fn clear(&mut self) {
        self.dependencies.clear();
        self.dependents.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_dependency() {
        let mut graph = DependencyGraph::new();
        graph.add_dependency("a.ts", "b.ts");

        assert!(graph.get_dependencies("a.ts").unwrap().contains("b.ts"));
        assert!(graph.get_dependents("b.ts").unwrap().contains("a.ts"));
    }

    #[test]
    fn test_get_affected_files_simple() {
        let mut graph = DependencyGraph::new();
        // a.ts imports b.ts
        graph.add_dependency("a.ts", "b.ts");

        // When b.ts changes, a.ts is affected
        let affected = graph.get_affected_files("b.ts");
        assert_eq!(affected.len(), 1);
        assert!(affected.contains(&"a.ts".to_string()));
    }

    #[test]
    fn test_get_affected_files_transitive() {
        let mut graph = DependencyGraph::new();
        // a.ts imports b.ts, b.ts imports c.ts
        graph.add_dependency("a.ts", "b.ts");
        graph.add_dependency("b.ts", "c.ts");

        // When c.ts changes, both a.ts and b.ts are affected
        let affected = graph.get_affected_files("c.ts");
        assert_eq!(affected.len(), 2);
        assert!(affected.contains(&"a.ts".to_string()));
        assert!(affected.contains(&"b.ts".to_string()));
    }

    #[test]
    fn test_get_affected_files_with_cycle() {
        let mut graph = DependencyGraph::new();
        // Circular dependency: a -> b -> c -> a
        // "a -> b" means "a imports b", so dependents[b] = {a}
        graph.add_dependency("a.ts", "b.ts");
        graph.add_dependency("b.ts", "c.ts");
        graph.add_dependency("c.ts", "a.ts");

        // When a.ts changes:
        // - c.ts imports a.ts, so c.ts is affected
        // - b.ts imports c.ts, so b.ts is affected
        // - a.ts imports b.ts, so a.ts is affected (cycle completes)
        // All files in the cycle are affected
        let affected = graph.get_affected_files("a.ts");
        assert_eq!(affected.len(), 3);
        assert!(affected.contains(&"a.ts".to_string()));
        assert!(affected.contains(&"b.ts".to_string()));
        assert!(affected.contains(&"c.ts".to_string()));
    }

    #[test]
    fn test_update_file() {
        let mut graph = DependencyGraph::new();

        // Initial: a.ts imports b.ts and c.ts
        graph.update_file("a.ts", &["b.ts".to_string(), "c.ts".to_string()]);
        assert!(graph.get_dependencies("a.ts").unwrap().contains("b.ts"));
        assert!(graph.get_dependencies("a.ts").unwrap().contains("c.ts"));

        // Update: a.ts now only imports d.ts
        graph.update_file("a.ts", &["d.ts".to_string()]);
        assert!(!graph.get_dependencies("a.ts").unwrap().contains("b.ts"));
        assert!(!graph.get_dependencies("a.ts").unwrap().contains("c.ts"));
        assert!(graph.get_dependencies("a.ts").unwrap().contains("d.ts"));

        // b.ts and c.ts should no longer have a.ts as dependent
        assert!(graph.get_dependents("b.ts").is_none());
        assert!(graph.get_dependents("c.ts").is_none());

        // d.ts should have a.ts as dependent
        assert!(graph.get_dependents("d.ts").unwrap().contains("a.ts"));
    }

    #[test]
    fn test_remove_file() {
        let mut graph = DependencyGraph::new();

        // Setup: a.ts imports b.ts, c.ts imports a.ts
        graph.add_dependency("a.ts", "b.ts");
        graph.add_dependency("c.ts", "a.ts");

        // Remove a.ts
        graph.remove_file("a.ts");

        // a.ts should be completely gone
        assert!(graph.get_dependencies("a.ts").is_none());
        assert!(graph.get_dependents("a.ts").is_none());

        // b.ts should no longer have a.ts as dependent
        assert!(graph.get_dependents("b.ts").is_none());

        // c.ts should still exist but have empty dependency on removed file
        // (the file c.ts still exists, just importing something that doesn't)
    }

    #[test]
    fn test_empty_imports() {
        let mut graph = DependencyGraph::new();

        // a.ts has imports
        graph.update_file("a.ts", &["b.ts".to_string()]);
        assert!(graph.get_dependencies("a.ts").is_some());

        // Clear all imports
        graph.update_file("a.ts", &[]);
        assert!(graph.get_dependencies("a.ts").is_none());
        assert!(graph.get_dependents("b.ts").is_none());
    }

    #[test]
    fn test_diamond_dependency() {
        let mut graph = DependencyGraph::new();
        // Diamond: a imports b and c, both b and c import d
        graph.add_dependency("a.ts", "b.ts");
        graph.add_dependency("a.ts", "c.ts");
        graph.add_dependency("b.ts", "d.ts");
        graph.add_dependency("c.ts", "d.ts");

        // When d.ts changes, a, b, and c are all affected
        let affected = graph.get_affected_files("d.ts");
        assert_eq!(affected.len(), 3);
        assert!(affected.contains(&"a.ts".to_string()));
        assert!(affected.contains(&"b.ts".to_string()));
        assert!(affected.contains(&"c.ts".to_string()));
    }
}
