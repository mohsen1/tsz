//! Module Dependency Graph
//!
//! This module provides a graph data structure for tracking module dependencies,
//! enabling:
//! - Circular dependency detection
//! - Topological sorting for build ordering
//! - Dependency analysis and tree-shaking support
//! - Change impact analysis

use crate::exports::ExportTracker;
use crate::imports::ImportTracker;
use crate::module_resolver::{ModuleResolver, ResolutionFailure, ResolvedModule};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

/// Unique identifier for a module in the graph
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModuleId(pub u32);

impl ModuleId {
    pub const NONE: ModuleId = ModuleId(u32::MAX);

    pub fn is_none(&self) -> bool {
        self.0 == u32::MAX
    }
}

/// Information about a module in the dependency graph
#[derive(Debug)]
pub struct ModuleInfo {
    /// Unique identifier
    pub id: ModuleId,
    /// Resolved file path
    pub path: PathBuf,
    /// Original specifier (for external modules)
    pub specifier: Option<String>,
    /// Whether this is an external module (from node_modules)
    pub is_external: bool,
    /// Import tracking for this module
    pub imports: ImportTracker,
    /// Export tracking for this module
    pub exports: ExportTracker,
    /// Modules this module imports from
    pub dependencies: FxHashSet<ModuleId>,
    /// Modules that import this module
    pub dependents: FxHashSet<ModuleId>,
    /// Whether this module has been fully processed
    pub is_processed: bool,
    /// Resolution errors encountered
    pub resolution_errors: Vec<ResolutionFailure>,
}

impl ModuleInfo {
    /// Create a new module info
    pub fn new(id: ModuleId, path: PathBuf) -> Self {
        Self {
            id,
            path,
            specifier: None,
            is_external: false,
            imports: ImportTracker::new(),
            exports: ExportTracker::new(),
            dependencies: FxHashSet::default(),
            dependents: FxHashSet::default(),
            is_processed: false,
            resolution_errors: Vec::new(),
        }
    }

    /// Mark as external module
    pub fn external(mut self, specifier: String) -> Self {
        self.is_external = true;
        self.specifier = Some(specifier);
        self
    }
}

/// A dependency edge in the module graph
#[derive(Debug, Clone)]
pub struct DependencyEdge {
    /// Source module (the importer)
    pub from: ModuleId,
    /// Target module (the importee)
    pub to: ModuleId,
    /// Import specifier used
    pub specifier: String,
    /// Whether this is a type-only import
    pub is_type_only: bool,
    /// Whether this is a dynamic import
    pub is_dynamic: bool,
    /// Whether this is a side-effect import
    pub is_side_effect: bool,
}

/// Circular dependency information
#[derive(Debug, Clone)]
pub struct CircularDependency {
    /// Modules forming the cycle
    pub cycle: Vec<ModuleId>,
    /// File paths for display
    pub paths: Vec<PathBuf>,
}

/// Module dependency graph
#[derive(Debug)]
pub struct ModuleGraph {
    /// All modules in the graph
    modules: FxHashMap<ModuleId, ModuleInfo>,
    /// Path to module ID mapping
    path_to_id: FxHashMap<PathBuf, ModuleId>,
    /// Next available module ID
    next_id: u32,
    /// All dependency edges
    edges: Vec<DependencyEdge>,
    /// Entry points
    entry_points: Vec<ModuleId>,
    /// Detected circular dependencies
    circular_dependencies: Vec<CircularDependency>,
    /// Module resolver (optional)
    resolver: Option<ModuleResolver>,
}

impl ModuleGraph {
    /// Create a new empty module graph
    pub fn new() -> Self {
        Self {
            modules: FxHashMap::default(),
            path_to_id: FxHashMap::default(),
            next_id: 0,
            edges: Vec::new(),
            entry_points: Vec::new(),
            circular_dependencies: Vec::new(),
            resolver: None,
        }
    }

    /// Create a module graph with a resolver
    pub fn with_resolver(resolver: ModuleResolver) -> Self {
        Self {
            modules: FxHashMap::default(),
            path_to_id: FxHashMap::default(),
            next_id: 0,
            edges: Vec::new(),
            entry_points: Vec::new(),
            circular_dependencies: Vec::new(),
            resolver: Some(resolver),
        }
    }

    /// Add or get a module by path
    pub fn add_module(&mut self, path: &Path) -> ModuleId {
        // Canonicalize path
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        if let Some(&id) = self.path_to_id.get(&canonical) {
            return id;
        }

        let id = ModuleId(self.next_id);
        self.next_id += 1;

        let info = ModuleInfo::new(id, canonical.clone());
        self.modules.insert(id, info);
        self.path_to_id.insert(canonical, id);

        id
    }

    /// Add an external module
    pub fn add_external_module(&mut self, specifier: &str, resolved: &ResolvedModule) -> ModuleId {
        let canonical = resolved
            .resolved_path
            .canonicalize()
            .unwrap_or_else(|_| resolved.resolved_path.clone());

        if let Some(&id) = self.path_to_id.get(&canonical) {
            return id;
        }

        let id = ModuleId(self.next_id);
        self.next_id += 1;

        let info = ModuleInfo::new(id, canonical.clone()).external(specifier.to_string());
        self.modules.insert(id, info);
        self.path_to_id.insert(canonical, id);

        id
    }

    /// Get a module by ID
    pub fn get_module(&self, id: ModuleId) -> Option<&ModuleInfo> {
        self.modules.get(&id)
    }

    /// Get a mutable module by ID
    pub fn get_module_mut(&mut self, id: ModuleId) -> Option<&mut ModuleInfo> {
        self.modules.get_mut(&id)
    }

    /// Get a module by path
    pub fn get_module_by_path(&self, path: &Path) -> Option<&ModuleInfo> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        self.path_to_id
            .get(&canonical)
            .and_then(|id| self.modules.get(id))
    }

    /// Get module ID by path
    pub fn get_module_id(&self, path: &Path) -> Option<ModuleId> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        self.path_to_id.get(&canonical).copied()
    }

    /// Add an entry point
    pub fn add_entry_point(&mut self, module_id: ModuleId) {
        if !self.entry_points.contains(&module_id) {
            self.entry_points.push(module_id);
        }
    }

    /// Add a dependency edge
    pub fn add_dependency(&mut self, edge: DependencyEdge) {
        // Update module dependencies
        if let Some(from_module) = self.modules.get_mut(&edge.from) {
            from_module.dependencies.insert(edge.to);
        }

        // Update module dependents
        if let Some(to_module) = self.modules.get_mut(&edge.to) {
            to_module.dependents.insert(edge.from);
        }

        self.edges.push(edge);
    }

    /// Add a dependency between two modules
    pub fn add_simple_dependency(&mut self, from: ModuleId, to: ModuleId, specifier: &str) {
        self.add_dependency(DependencyEdge {
            from,
            to,
            specifier: specifier.to_string(),
            is_type_only: false,
            is_dynamic: false,
            is_side_effect: false,
        });
    }

    /// Detect circular dependencies using Tarjan's algorithm
    pub fn detect_circular_dependencies(&mut self) -> &[CircularDependency] {
        self.circular_dependencies.clear();

        let mut index_counter = 0u32;
        let mut stack: Vec<ModuleId> = Vec::new();
        let mut on_stack: FxHashSet<ModuleId> = FxHashSet::default();
        let mut indices: FxHashMap<ModuleId, u32> = FxHashMap::default();
        let mut lowlinks: FxHashMap<ModuleId, u32> = FxHashMap::default();

        let module_ids: Vec<ModuleId> = self.modules.keys().copied().collect();

        for module_id in module_ids {
            if !indices.contains_key(&module_id) {
                self.strongconnect(
                    module_id,
                    &mut index_counter,
                    &mut stack,
                    &mut on_stack,
                    &mut indices,
                    &mut lowlinks,
                );
            }
        }

        &self.circular_dependencies
    }

    /// Tarjan's strongconnect helper
    fn strongconnect(
        &mut self,
        v: ModuleId,
        index_counter: &mut u32,
        stack: &mut Vec<ModuleId>,
        on_stack: &mut FxHashSet<ModuleId>,
        indices: &mut FxHashMap<ModuleId, u32>,
        lowlinks: &mut FxHashMap<ModuleId, u32>,
    ) {
        indices.insert(v, *index_counter);
        lowlinks.insert(v, *index_counter);
        *index_counter += 1;

        stack.push(v);
        on_stack.insert(v);

        // Get dependencies
        let deps: Vec<ModuleId> = self
            .modules
            .get(&v)
            .map(|m| m.dependencies.iter().copied().collect())
            .unwrap_or_default();

        for w in deps {
            if !indices.contains_key(&w) {
                self.strongconnect(w, index_counter, stack, on_stack, indices, lowlinks);
                let w_lowlink = *lowlinks.get(&w).unwrap();
                let v_lowlink = lowlinks.get_mut(&v).unwrap();
                *v_lowlink = (*v_lowlink).min(w_lowlink);
            } else if on_stack.contains(&w) {
                let w_index = *indices.get(&w).unwrap();
                let v_lowlink = lowlinks.get_mut(&v).unwrap();
                *v_lowlink = (*v_lowlink).min(w_index);
            }
        }

        // Root of SCC
        if lowlinks.get(&v) == indices.get(&v) {
            let mut scc = Vec::new();
            loop {
                let w = stack.pop().unwrap();
                on_stack.remove(&w);
                scc.push(w);
                if w == v {
                    break;
                }
            }

            // Only report cycles (SCC with more than one node or self-loop)
            if scc.len() > 1 {
                let paths: Vec<PathBuf> = scc
                    .iter()
                    .filter_map(|id| self.modules.get(id).map(|m| m.path.clone()))
                    .collect();

                self.circular_dependencies
                    .push(CircularDependency { cycle: scc, paths });
            } else if scc.len() == 1 {
                // Check for self-loop
                let id = scc[0];
                if let Some(m) = self.modules.get(&id) {
                    if m.dependencies.contains(&id) {
                        self.circular_dependencies.push(CircularDependency {
                            cycle: scc,
                            paths: vec![m.path.clone()],
                        });
                    }
                }
            }
        }
    }

    /// Get topological sort of modules (for build ordering)
    ///
    /// Returns modules in dependency order: modules with no dependencies come first,
    /// followed by modules that depend only on already-listed modules.
    /// This is the correct order for building/processing modules.
    pub fn topological_sort(&self) -> Result<Vec<ModuleId>, CircularDependencyError> {
        let mut result = Vec::new();
        let mut visited = FxHashSet::default();
        let mut temp_visited = FxHashSet::default();
        let mut cycle_path = Vec::new();

        for &id in self.modules.keys() {
            if !visited.contains(&id) {
                if !self.visit_topological(
                    id,
                    &mut visited,
                    &mut temp_visited,
                    &mut result,
                    &mut cycle_path,
                ) {
                    return Err(CircularDependencyError { cycle: cycle_path });
                }
            }
        }

        // Post-order DFS naturally produces dependencies before dependents,
        // which is the correct build order (no reverse needed)
        Ok(result)
    }

    /// DFS helper for topological sort
    fn visit_topological(
        &self,
        id: ModuleId,
        visited: &mut FxHashSet<ModuleId>,
        temp_visited: &mut FxHashSet<ModuleId>,
        result: &mut Vec<ModuleId>,
        cycle_path: &mut Vec<ModuleId>,
    ) -> bool {
        if temp_visited.contains(&id) {
            // Cycle detected
            cycle_path.push(id);
            return false;
        }

        if visited.contains(&id) {
            return true;
        }

        temp_visited.insert(id);

        if let Some(module) = self.modules.get(&id) {
            for &dep in &module.dependencies {
                if !self.visit_topological(dep, visited, temp_visited, result, cycle_path) {
                    cycle_path.push(id);
                    return false;
                }
            }
        }

        temp_visited.remove(&id);
        visited.insert(id);
        result.push(id);

        true
    }

    /// Get all modules that depend on a given module (transitive)
    pub fn get_dependents(&self, id: ModuleId) -> FxHashSet<ModuleId> {
        let mut result = FxHashSet::default();
        let mut queue = VecDeque::new();

        if let Some(module) = self.modules.get(&id) {
            for &dep in &module.dependents {
                queue.push_back(dep);
            }
        }

        while let Some(current) = queue.pop_front() {
            if result.insert(current) {
                if let Some(module) = self.modules.get(&current) {
                    for &dep in &module.dependents {
                        if !result.contains(&dep) {
                            queue.push_back(dep);
                        }
                    }
                }
            }
        }

        result
    }

    /// Get all dependencies of a module (transitive)
    pub fn get_dependencies(&self, id: ModuleId) -> FxHashSet<ModuleId> {
        let mut result = FxHashSet::default();
        let mut queue = VecDeque::new();

        if let Some(module) = self.modules.get(&id) {
            for &dep in &module.dependencies {
                queue.push_back(dep);
            }
        }

        while let Some(current) = queue.pop_front() {
            if result.insert(current) {
                if let Some(module) = self.modules.get(&current) {
                    for &dep in &module.dependencies {
                        if !result.contains(&dep) {
                            queue.push_back(dep);
                        }
                    }
                }
            }
        }

        result
    }

    /// Check if a module depends on another (directly or transitively)
    pub fn depends_on(&self, from: ModuleId, to: ModuleId) -> bool {
        self.get_dependencies(from).contains(&to)
    }

    /// Get statistics about the module graph
    pub fn stats(&self) -> ModuleGraphStats {
        let mut internal_modules = 0;
        let mut external_modules = 0;
        let mut total_dependencies = 0;
        let mut max_depth = 0;

        for module in self.modules.values() {
            if module.is_external {
                external_modules += 1;
            } else {
                internal_modules += 1;
            }
            total_dependencies += module.dependencies.len();
        }

        // Calculate max depth from entry points
        for &entry in &self.entry_points {
            let depth = self.calculate_depth(entry, &mut FxHashSet::default());
            max_depth = max_depth.max(depth);
        }

        ModuleGraphStats {
            total_modules: self.modules.len(),
            internal_modules,
            external_modules,
            total_edges: self.edges.len(),
            entry_points: self.entry_points.len(),
            circular_dependencies: self.circular_dependencies.len(),
            max_depth,
            average_dependencies: if self.modules.is_empty() {
                0.0
            } else {
                total_dependencies as f64 / self.modules.len() as f64
            },
        }
    }

    /// Calculate depth of module from entry points
    fn calculate_depth(&self, id: ModuleId, visited: &mut FxHashSet<ModuleId>) -> usize {
        if visited.contains(&id) {
            return 0;
        }
        visited.insert(id);

        let mut max_child_depth = 0;
        if let Some(module) = self.modules.get(&id) {
            for &dep in &module.dependencies {
                let child_depth = self.calculate_depth(dep, visited);
                max_child_depth = max_child_depth.max(child_depth);
            }
        }

        max_child_depth + 1
    }

    /// Get all modules in the graph
    pub fn modules(&self) -> impl Iterator<Item = &ModuleInfo> {
        self.modules.values()
    }

    /// Get number of modules
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    /// Check if graph is empty
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    /// Get all edges
    pub fn edges(&self) -> &[DependencyEdge] {
        &self.edges
    }

    /// Get entry points
    pub fn entry_points(&self) -> &[ModuleId] {
        &self.entry_points
    }

    /// Get circular dependencies
    pub fn circular_deps(&self) -> &[CircularDependency] {
        &self.circular_dependencies
    }

    /// Clear the graph
    pub fn clear(&mut self) {
        self.modules.clear();
        self.path_to_id.clear();
        self.next_id = 0;
        self.edges.clear();
        self.entry_points.clear();
        self.circular_dependencies.clear();
    }
}

impl Default for ModuleGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Error returned when topological sort fails due to circular dependency
#[derive(Debug)]
pub struct CircularDependencyError {
    pub cycle: Vec<ModuleId>,
}

impl std::fmt::Display for CircularDependencyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Circular dependency detected involving {} modules",
            self.cycle.len()
        )
    }
}

impl std::error::Error for CircularDependencyError {}

/// Statistics about the module graph
#[derive(Debug, Clone, Default)]
pub struct ModuleGraphStats {
    pub total_modules: usize,
    pub internal_modules: usize,
    pub external_modules: usize,
    pub total_edges: usize,
    pub entry_points: usize,
    pub circular_dependencies: usize,
    pub max_depth: usize,
    pub average_dependencies: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn create_test_path(name: &str) -> PathBuf {
        // Use a temp directory structure for testing
        PathBuf::from(format!("/tmp/test_modules/{}", name))
    }

    #[test]
    fn test_module_graph_basic() {
        let mut graph = ModuleGraph::new();

        let a = graph.add_module(&create_test_path("a.ts"));
        let b = graph.add_module(&create_test_path("b.ts"));

        assert_ne!(a, b);
        assert_eq!(graph.len(), 2);
    }

    #[test]
    fn test_module_graph_dedup() {
        let mut graph = ModuleGraph::new();

        let path = create_test_path("a.ts");
        let a1 = graph.add_module(&path);
        let a2 = graph.add_module(&path);

        assert_eq!(a1, a2);
        assert_eq!(graph.len(), 1);
    }

    #[test]
    fn test_add_dependency() {
        let mut graph = ModuleGraph::new();

        let a = graph.add_module(&create_test_path("a.ts"));
        let b = graph.add_module(&create_test_path("b.ts"));

        graph.add_simple_dependency(a, b, "./b");

        let module_a = graph.get_module(a).unwrap();
        assert!(module_a.dependencies.contains(&b));

        let module_b = graph.get_module(b).unwrap();
        assert!(module_b.dependents.contains(&a));
    }

    #[test]
    fn test_circular_dependency_detection() {
        let mut graph = ModuleGraph::new();

        let a = graph.add_module(&create_test_path("a.ts"));
        let b = graph.add_module(&create_test_path("b.ts"));
        let c = graph.add_module(&create_test_path("c.ts"));

        // Create cycle: a -> b -> c -> a
        graph.add_simple_dependency(a, b, "./b");
        graph.add_simple_dependency(b, c, "./c");
        graph.add_simple_dependency(c, a, "./a");

        let cycles = graph.detect_circular_dependencies();
        assert!(!cycles.is_empty());
    }

    #[test]
    fn test_topological_sort_no_cycles() {
        let mut graph = ModuleGraph::new();

        let a = graph.add_module(&create_test_path("a.ts"));
        let b = graph.add_module(&create_test_path("b.ts"));
        let c = graph.add_module(&create_test_path("c.ts"));

        // a -> b -> c (a depends on b, b depends on c)
        graph.add_simple_dependency(a, b, "./b");
        graph.add_simple_dependency(b, c, "./c");

        let sorted = graph.topological_sort().unwrap();

        // All modules should be present
        assert_eq!(sorted.len(), 3);
        assert!(sorted.contains(&a));
        assert!(sorted.contains(&b));
        assert!(sorted.contains(&c));

        // In a valid topological order, a module appears before its dependents.
        // Since a depends on b, and b depends on c:
        // - c should appear before b (c has no dependencies, b depends on c)
        // - b should appear before a (a depends on b)
        let pos_a = sorted.iter().position(|&x| x == a).unwrap();
        let pos_b = sorted.iter().position(|&x| x == b).unwrap();
        let pos_c = sorted.iter().position(|&x| x == c).unwrap();

        // Verify: dependencies appear before their dependents in topological order
        // c comes before b (since b depends on c)
        // b comes before a (since a depends on b)
        assert!(
            pos_c < pos_b,
            "c (pos {}) should come before b (pos {}) since b depends on c",
            pos_c,
            pos_b
        );
        assert!(
            pos_b < pos_a,
            "b (pos {}) should come before a (pos {}) since a depends on b",
            pos_b,
            pos_a
        );
    }

    #[test]
    fn test_topological_sort_with_cycle() {
        let mut graph = ModuleGraph::new();

        let a = graph.add_module(&create_test_path("a.ts"));
        let b = graph.add_module(&create_test_path("b.ts"));

        // Create cycle: a -> b -> a
        graph.add_simple_dependency(a, b, "./b");
        graph.add_simple_dependency(b, a, "./a");

        let result = graph.topological_sort();
        assert!(result.is_err());
    }

    #[test]
    fn test_get_dependents() {
        let mut graph = ModuleGraph::new();

        let a = graph.add_module(&create_test_path("a.ts"));
        let b = graph.add_module(&create_test_path("b.ts"));
        let c = graph.add_module(&create_test_path("c.ts"));

        // a -> b, c -> b (both depend on b)
        graph.add_simple_dependency(a, b, "./b");
        graph.add_simple_dependency(c, b, "./b");

        let dependents = graph.get_dependents(b);
        assert!(dependents.contains(&a));
        assert!(dependents.contains(&c));
        assert!(!dependents.contains(&b));
    }

    #[test]
    fn test_get_dependencies() {
        let mut graph = ModuleGraph::new();

        let a = graph.add_module(&create_test_path("a.ts"));
        let b = graph.add_module(&create_test_path("b.ts"));
        let c = graph.add_module(&create_test_path("c.ts"));

        // a -> b -> c
        graph.add_simple_dependency(a, b, "./b");
        graph.add_simple_dependency(b, c, "./c");

        let deps = graph.get_dependencies(a);
        assert!(deps.contains(&b));
        assert!(deps.contains(&c));
        assert!(!deps.contains(&a));
    }

    #[test]
    fn test_depends_on() {
        let mut graph = ModuleGraph::new();

        let a = graph.add_module(&create_test_path("a.ts"));
        let b = graph.add_module(&create_test_path("b.ts"));
        let c = graph.add_module(&create_test_path("c.ts"));

        graph.add_simple_dependency(a, b, "./b");
        graph.add_simple_dependency(b, c, "./c");

        assert!(graph.depends_on(a, b));
        assert!(graph.depends_on(a, c)); // transitive
        assert!(!graph.depends_on(c, a));
    }

    #[test]
    fn test_module_graph_stats() {
        let mut graph = ModuleGraph::new();

        let a = graph.add_module(&create_test_path("a.ts"));
        let b = graph.add_module(&create_test_path("b.ts"));

        graph.add_entry_point(a);
        graph.add_simple_dependency(a, b, "./b");

        let stats = graph.stats();
        assert_eq!(stats.total_modules, 2);
        assert_eq!(stats.internal_modules, 2);
        assert_eq!(stats.total_edges, 1);
        assert_eq!(stats.entry_points, 1);
    }
}
