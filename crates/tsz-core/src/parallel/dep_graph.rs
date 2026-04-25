//! Dependency graph construction from skeleton `import_sources`.
//!
//! Given a set of `FileSkeleton`s with resolved file names, this module builds
//! a directed dependency graph and produces a topological ordering suitable for
//! sequential or batched type-checking.
//!
//! ## Algorithm
//!
//! Uses Kahn's algorithm for topological sort. Cycles are detected and reported
//! as strongly connected components; files in cycles are appended in stable
//! (input) order after all acyclic files.
//!
//! ## Usage
//!
//! ```ignore
//! let skeletons: Vec<FileSkeleton> = /* from extract_skeleton */;
//! let graph = DepGraph::build(&skeletons, |specifier, from_file| {
//!     resolve_specifier_to_filename(specifier, from_file)
//! });
//! let order: Vec<usize> = graph.topological_order();
//! ```

use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;

use super::skeleton::FileSkeleton;

/// A dependency graph over file skeletons.
///
/// Nodes are file indices (into the original skeleton slice). Edges represent
/// "file A imports file B" relationships derived from `import_sources`.
#[derive(Debug, Clone)]
pub struct DepGraph {
    /// Number of nodes (files).
    pub node_count: usize,
    /// Adjacency list: `edges[i]` is the set of file indices that file `i` imports.
    edges: Vec<FxHashSet<usize>>,
    /// Reverse adjacency: `reverse_edges[i]` is the set of files that import file `i`.
    reverse_edges: Vec<FxHashSet<usize>>,
    /// In-degree for each node (number of files this file imports that are in the graph).
    in_degrees: Vec<usize>,
    /// Files that have no dependencies (leaf nodes / entry points).
    roots: Vec<usize>,
    /// Number of edges (import relationships) in the graph.
    pub edge_count: usize,
    /// Specifiers that could not be resolved to any file in the skeleton set.
    pub unresolved_specifiers: Vec<UnresolvedSpecifier>,
}

/// An import specifier that could not be resolved to a file in the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedSpecifier {
    /// Index of the file containing the import.
    pub from_file: usize,
    /// The raw module specifier string.
    pub specifier: String,
}

/// Result of topological sorting.
#[derive(Debug, Clone)]
pub struct TopoResult {
    /// Files in topological order (dependencies before dependents).
    /// Files in cycles are appended at the end in stable (input) order.
    pub order: Vec<usize>,
    /// Files that participate in dependency cycles.
    /// Each inner `Vec` is one strongly connected component (SCC) with >1 member.
    pub cycles: Vec<Vec<usize>>,
    /// Whether the graph is a DAG (no cycles).
    pub is_acyclic: bool,
}

impl DepGraph {
    /// Build a dependency graph from file skeletons.
    ///
    /// The `resolve` callback maps `(specifier, from_file_name) -> Option<file_name>`.
    /// It should return `None` for external/unresolvable specifiers (e.g., `"react"`).
    /// Only specifiers that resolve to a file name present in the skeleton set
    /// produce graph edges.
    pub fn build<F>(skeletons: &[FileSkeleton], resolve: F) -> Self
    where
        F: Fn(&str, &str) -> Option<String>,
    {
        let n = skeletons.len();
        let mut edges = vec![FxHashSet::default(); n];
        let mut reverse_edges = vec![FxHashSet::default(); n];
        let mut edge_count = 0usize;
        let mut unresolved_specifiers = Vec::new();

        // Build name -> index map for O(1) lookup.
        let name_to_idx: FxHashMap<&str, usize> = skeletons
            .iter()
            .enumerate()
            .map(|(i, s)| (s.file_name.as_str(), i))
            .collect();

        for (i, skeleton) in skeletons.iter().enumerate() {
            for specifier in &skeleton.import_sources {
                if let Some(resolved_name) = resolve(specifier, &skeleton.file_name) {
                    if let Some(&target_idx) = name_to_idx.get(resolved_name.as_str()) {
                        if target_idx != i && edges[i].insert(target_idx) {
                            reverse_edges[target_idx].insert(i);
                            edge_count += 1;
                        }
                    } else {
                        unresolved_specifiers.push(UnresolvedSpecifier {
                            from_file: i,
                            specifier: specifier.clone(),
                        });
                    }
                }
                // resolve returned None -> external dependency, skip silently
            }
        }

        // Compute in-degrees (number of imports from within the graph).
        let in_degrees: Vec<usize> = (0..n).map(|i| edges[i].len()).collect();
        let roots: Vec<usize> = (0..n).filter(|&i| in_degrees[i] == 0).collect();

        DepGraph {
            node_count: n,
            edges,
            reverse_edges,
            in_degrees,
            roots,
            edge_count,
            unresolved_specifiers,
        }
    }

    /// Build a dependency graph with a simple name-matching resolver.
    ///
    /// This uses direct string matching: an import specifier resolves to a file
    /// if any skeleton's `file_name` ends with the specifier (after stripping
    /// leading `./` or `../`). This is a coarse heuristic suitable for testing
    /// and simple single-directory projects.
    ///
    /// For production use, prefer `build()` with a proper module resolver.
    pub fn build_simple(skeletons: &[FileSkeleton]) -> Self {
        // Build suffix map for fast lookup.
        let name_map: FxHashMap<&str, usize> = skeletons
            .iter()
            .enumerate()
            .map(|(i, s)| (s.file_name.as_str(), i))
            .collect();

        Self::build(skeletons, |specifier, _from| {
            // Try direct match first.
            if name_map.contains_key(specifier) {
                return Some(specifier.to_string());
            }
            // Strip relative prefix and try common extensions.
            let stripped = specifier
                .strip_prefix("./")
                .or_else(|| specifier.strip_prefix("../"))
                .unwrap_or(specifier);
            for ext in &["", ".ts", ".tsx", ".d.ts", ".js", ".jsx"] {
                let candidate = format!("{stripped}{ext}");
                if name_map.contains_key(candidate.as_str()) {
                    return Some(candidate);
                }
            }
            None
        })
    }

    /// Produce a topological ordering of files.
    ///
    /// Uses Kahn's algorithm. Files with no dependencies come first (they can
    /// be checked independently). Files in cycles are detected and appended
    /// at the end in stable (original) order.
    ///
    /// The resulting order is suitable for sequential type-checking: processing
    /// files in this order guarantees that a file's dependencies have been
    /// checked before the file itself (except for cycles).
    pub fn topological_order(&self) -> TopoResult {
        let n = self.node_count;
        let mut in_deg: Vec<usize> = self.in_degrees.clone();
        let mut queue: VecDeque<usize> = VecDeque::new();
        let mut order: Vec<usize> = Vec::with_capacity(n);

        // Seed with roots (files that import nothing in-graph).
        // Note: we use in_deg based on edges[i].len() which counts outgoing
        // imports. For topological sort we actually need "how many files must
        // be processed before me", which is reverse_edges[i].len() ... BUT
        // the standard definition is: process dependencies first. So the
        // in-degree for Kahn's should be "how many of my dependencies are
        // unprocessed" = edges[i].len(). Wait, no -- Kahn's algorithm uses
        // in-degree in the *dependency direction*: if A depends on B, the edge
        // is A->B, and we want B before A. So in-degree for Kahn's is the
        // number of *incoming* edges in the dependency graph, which is
        // reverse_edges[i].len() ... Hmm, let me reconsider.
        //
        // Actually: edges[i] = set of files that file i imports (i depends on them).
        // For topological sort (dependencies first), we want to process nodes
        // with no *incoming* dependency edges first. A node j has an incoming
        // dependency edge from i if i is in edges[j]... no, if j is in edges[i],
        // meaning i imports j. So the relevant in-degree for Kahn's is:
        // "how many files import me" -- but that's wrong too.
        //
        // Let's be precise: the DAG edge direction for "process dependencies
        // first" is: edge from B to A means "B must come before A" (A depends
        // on B). In our data, edges[A] contains B (A imports B). So the
        // topological edge is B -> A. The in-degree for Kahn's algorithm on
        // this DAG is: for each node A, count of edges[A] (its dependencies).
        //
        // Files with edges[i].len() == 0 are roots (no dependencies).

        for (i, deg) in in_deg.iter().enumerate() {
            if *deg == 0 {
                queue.push_back(i);
            }
        }

        while let Some(node) = queue.pop_front() {
            order.push(node);
            // For each file that depends on `node` (imports `node`),
            // decrement its effective in-degree.
            for &dependent in &self.reverse_edges[node] {
                in_deg[dependent] = in_deg[dependent].saturating_sub(1);
                if in_deg[dependent] == 0 {
                    queue.push_back(dependent);
                }
            }
        }

        // Detect cycles: any node not yet in `order` is part of a cycle.
        let in_order: FxHashSet<usize> = order.iter().copied().collect();
        let cycle_nodes: Vec<usize> = (0..n).filter(|i| !in_order.contains(i)).collect();
        let is_acyclic = cycle_nodes.is_empty();

        // Find SCCs among cycle nodes using iterative Tarjan's.
        let cycles = if cycle_nodes.is_empty() {
            Vec::new()
        } else {
            self.find_sccs(&cycle_nodes)
        };

        // Append cycle nodes in stable (input) order.
        order.extend(cycle_nodes);

        TopoResult {
            order,
            cycles,
            is_acyclic,
        }
    }

    /// Returns the direct dependencies of file `idx` (files it imports).
    pub fn dependencies(&self, idx: usize) -> &FxHashSet<usize> {
        &self.edges[idx]
    }

    /// Returns the direct dependents of file `idx` (files that import it).
    pub fn dependents(&self, idx: usize) -> &FxHashSet<usize> {
        &self.reverse_edges[idx]
    }

    /// Returns file indices that have no in-graph dependencies.
    pub fn roots(&self) -> &[usize] {
        &self.roots
    }

    /// Find strongly connected components among a subset of nodes.
    ///
    /// Uses an iterative variant of Tarjan's algorithm restricted to `nodes`.
    fn find_sccs(&self, nodes: &[usize]) -> Vec<Vec<usize>> {
        let node_set: FxHashSet<usize> = nodes.iter().copied().collect();
        let mut index_counter: usize = 0;
        let mut stack: Vec<usize> = Vec::new();
        let mut on_stack: FxHashSet<usize> = FxHashSet::default();
        let mut indices: FxHashMap<usize, usize> = FxHashMap::default();
        let mut lowlinks: FxHashMap<usize, usize> = FxHashMap::default();
        let mut sccs: Vec<Vec<usize>> = Vec::new();

        for &node in nodes {
            if indices.contains_key(&node) {
                continue;
            }

            // Iterative Tarjan's using an explicit call stack.
            let mut call_stack: Vec<(usize, bool, Vec<usize>)> = Vec::new();
            indices.insert(node, index_counter);
            lowlinks.insert(node, index_counter);
            index_counter += 1;
            stack.push(node);
            on_stack.insert(node);

            let neighbors: Vec<usize> = self.edges[node]
                .iter()
                .filter(|n| node_set.contains(n))
                .copied()
                .collect();
            call_stack.push((node, false, neighbors));

            while let Some((v, _, remaining)) = call_stack.last_mut() {
                let v = *v;
                if let Some(w) = remaining.pop() {
                    use std::collections::hash_map::Entry;
                    if let Entry::Vacant(entry) = indices.entry(w) {
                        entry.insert(index_counter);
                        lowlinks.insert(w, index_counter);
                        index_counter += 1;
                        stack.push(w);
                        on_stack.insert(w);
                        let w_neighbors: Vec<usize> = self.edges[w]
                            .iter()
                            .filter(|n| node_set.contains(n))
                            .copied()
                            .collect();
                        call_stack.push((w, false, w_neighbors));
                    } else if on_stack.contains(&w) {
                        let w_idx = indices[&w];
                        let v_low = lowlinks.get_mut(&v).unwrap();
                        if w_idx < *v_low {
                            *v_low = w_idx;
                        }
                    }
                } else {
                    // All neighbors processed; check if v is root of SCC.
                    let v_low = lowlinks[&v];
                    let v_idx = indices[&v];
                    if v_low == v_idx {
                        let mut scc = Vec::new();
                        while let Some(w) = stack.pop() {
                            on_stack.remove(&w);
                            scc.push(w);
                            if w == v {
                                break;
                            }
                        }
                        if scc.len() > 1 {
                            scc.sort_unstable();
                            sccs.push(scc);
                        }
                    }

                    // Pop current frame and propagate lowlink to parent.
                    call_stack.pop();
                    if let Some((parent, _, _)) = call_stack.last() {
                        let parent = *parent;
                        let v_low = lowlinks[&v];
                        let p_low = lowlinks.get_mut(&parent).unwrap();
                        if v_low < *p_low {
                            *p_low = v_low;
                        }
                    }
                }
            }
        }

        sccs.sort_by_key(|scc| scc[0]);
        sccs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binder::FileFeatures;

    /// Helper to make a minimal skeleton for testing.
    fn make_skeleton(name: &str, imports: &[&str]) -> FileSkeleton {
        FileSkeleton {
            file_name: name.to_string(),
            is_external_module: true,
            symbols: vec![],
            global_augmentations: vec![],
            module_augmentations: vec![],
            augmentation_targets: vec![],
            reexports: vec![],
            wildcard_reexports: vec![],
            expando_properties: vec![],
            declared_modules: vec![],
            shorthand_ambient_modules: vec![],
            module_export_specifiers: vec![],
            import_sources: imports.iter().map(|s| s.to_string()).collect(),
            file_features: FileFeatures::default(),
            fingerprint: 0,
        }
    }

    #[test]
    fn empty_graph() {
        let graph = DepGraph::build_simple(&[]);
        assert_eq!(graph.node_count, 0);
        assert_eq!(graph.edge_count, 0);
        let result = graph.topological_order();
        assert!(result.order.is_empty());
        assert!(result.is_acyclic);
    }

    #[test]
    fn single_file_no_deps() {
        let skeletons = vec![make_skeleton("a.ts", &[])];
        let graph = DepGraph::build_simple(&skeletons);
        assert_eq!(graph.node_count, 1);
        assert_eq!(graph.edge_count, 0);
        let result = graph.topological_order();
        assert_eq!(result.order, vec![0]);
        assert!(result.is_acyclic);
    }

    #[test]
    fn linear_chain() {
        // a.ts -> b.ts -> c.ts
        let skeletons = vec![
            make_skeleton("a.ts", &["b.ts"]),
            make_skeleton("b.ts", &["c.ts"]),
            make_skeleton("c.ts", &[]),
        ];
        let graph = DepGraph::build_simple(&skeletons);
        assert_eq!(graph.node_count, 3);
        assert_eq!(graph.edge_count, 2);
        let result = graph.topological_order();
        assert!(result.is_acyclic);
        // c must come before b, b before a
        let pos: FxHashMap<usize, usize> = result
            .order
            .iter()
            .enumerate()
            .map(|(pos, &idx)| (idx, pos))
            .collect();
        assert!(pos[&2] < pos[&1], "c.ts must come before b.ts");
        assert!(pos[&1] < pos[&0], "b.ts must come before a.ts");
    }

    #[test]
    fn diamond_dependency() {
        // a.ts -> b.ts, a.ts -> c.ts, b.ts -> d.ts, c.ts -> d.ts
        let skeletons = vec![
            make_skeleton("a.ts", &["b.ts", "c.ts"]),
            make_skeleton("b.ts", &["d.ts"]),
            make_skeleton("c.ts", &["d.ts"]),
            make_skeleton("d.ts", &[]),
        ];
        let graph = DepGraph::build_simple(&skeletons);
        assert_eq!(graph.edge_count, 4);
        let result = graph.topological_order();
        assert!(result.is_acyclic);
        let pos: FxHashMap<usize, usize> = result
            .order
            .iter()
            .enumerate()
            .map(|(pos, &idx)| (idx, pos))
            .collect();
        assert!(pos[&3] < pos[&1], "d.ts before b.ts");
        assert!(pos[&3] < pos[&2], "d.ts before c.ts");
        assert!(pos[&1] < pos[&0], "b.ts before a.ts");
        assert!(pos[&2] < pos[&0], "c.ts before a.ts");
    }

    #[test]
    fn simple_cycle() {
        // a.ts -> b.ts -> a.ts
        let skeletons = vec![
            make_skeleton("a.ts", &["b.ts"]),
            make_skeleton("b.ts", &["a.ts"]),
        ];
        let graph = DepGraph::build_simple(&skeletons);
        let result = graph.topological_order();
        assert!(!result.is_acyclic);
        assert_eq!(result.cycles.len(), 1);
        assert_eq!(result.cycles[0], vec![0, 1]);
        // Both files should still appear in order
        assert_eq!(result.order.len(), 2);
    }

    #[test]
    fn cycle_with_tail() {
        // a.ts -> b.ts -> c.ts -> b.ts (cycle: b,c), a depends on cycle
        let skeletons = vec![
            make_skeleton("a.ts", &["b.ts"]),
            make_skeleton("b.ts", &["c.ts"]),
            make_skeleton("c.ts", &["b.ts"]),
        ];
        let graph = DepGraph::build_simple(&skeletons);
        let result = graph.topological_order();
        assert!(!result.is_acyclic);
        assert_eq!(result.cycles.len(), 1);
        assert_eq!(result.cycles[0], vec![1, 2]);
        // a.ts depends on the cycle but is not part of it
        // All 3 files should be in the order
        assert_eq!(result.order.len(), 3);
    }

    #[test]
    fn unresolved_specifiers_tracked() {
        // Use a custom resolver that returns Some for known specifiers
        // but resolves to a name not in the skeleton set, which triggers
        // unresolved tracking. External deps (resolver returns None) are
        // silently ignored since they're outside the project.
        let skeletons = vec![
            make_skeleton("a.ts", &["./utils", "missing-local"]),
            make_skeleton("utils.ts", &[]),
        ];
        let graph = DepGraph::build(&skeletons, |specifier, _from| match specifier {
            "./utils" => Some("utils.ts".to_string()),
            "missing-local" => Some("nonexistent.ts".to_string()), // resolves but not in set
            _ => None,
        });
        assert_eq!(graph.edge_count, 1);
        let unresolved: Vec<&str> = graph
            .unresolved_specifiers
            .iter()
            .map(|u| u.specifier.as_str())
            .collect();
        assert!(
            unresolved.contains(&"missing-local"),
            "expected 'missing-local' in unresolved, got: {unresolved:?}"
        );
    }

    #[test]
    fn external_deps_silently_skipped() {
        // External deps (resolver returns None) should not appear as unresolved.
        let skeletons = vec![make_skeleton("a.ts", &["react", "lodash"])];
        let graph = DepGraph::build_simple(&skeletons);
        assert_eq!(graph.edge_count, 0);
        assert!(
            graph.unresolved_specifiers.is_empty(),
            "external deps should not be tracked as unresolved"
        );
    }

    #[test]
    fn relative_import_resolution() {
        let skeletons = vec![
            make_skeleton("src/app.ts", &["./utils"]),
            make_skeleton("utils.ts", &[]),
        ];
        let graph = DepGraph::build_simple(&skeletons);
        // build_simple strips "./" and tries extensions -- "utils.ts" should match
        assert_eq!(graph.edge_count, 1);
    }

    #[test]
    fn self_import_ignored() {
        let skeletons = vec![make_skeleton("a.ts", &["a.ts"])];
        let graph = DepGraph::build_simple(&skeletons);
        assert_eq!(graph.edge_count, 0, "self-imports should not create edges");
        let result = graph.topological_order();
        assert!(result.is_acyclic);
    }

    #[test]
    fn dependents_and_dependencies() {
        // a.ts -> b.ts
        let skeletons = vec![make_skeleton("a.ts", &["b.ts"]), make_skeleton("b.ts", &[])];
        let graph = DepGraph::build_simple(&skeletons);
        assert!(graph.dependencies(0).contains(&1));
        assert!(graph.dependencies(1).is_empty());
        assert!(graph.dependents(1).contains(&0));
        assert!(graph.dependents(0).is_empty());
    }

    #[test]
    fn roots_are_leaf_nodes() {
        let skeletons = vec![
            make_skeleton("a.ts", &["b.ts"]),
            make_skeleton("b.ts", &["c.ts"]),
            make_skeleton("c.ts", &[]),
            make_skeleton("d.ts", &[]),
        ];
        let graph = DepGraph::build_simple(&skeletons);
        let mut roots = graph.roots().to_vec();
        roots.sort();
        assert_eq!(roots, vec![2, 3], "c.ts and d.ts have no deps");
    }

    #[test]
    fn custom_resolver() {
        let skeletons = vec![
            make_skeleton("/src/app.ts", &["@lib/utils"]),
            make_skeleton("/src/lib/utils.ts", &[]),
        ];
        let graph = DepGraph::build(&skeletons, |specifier, _from| {
            if specifier == "@lib/utils" {
                Some("/src/lib/utils.ts".to_string())
            } else {
                None
            }
        });
        assert_eq!(graph.edge_count, 1);
        assert!(graph.dependencies(0).contains(&1));
    }

    #[test]
    fn multiple_independent_components() {
        // Two disconnected subgraphs: {a->b} and {c->d}
        let skeletons = vec![
            make_skeleton("a.ts", &["b.ts"]),
            make_skeleton("b.ts", &[]),
            make_skeleton("c.ts", &["d.ts"]),
            make_skeleton("d.ts", &[]),
        ];
        let graph = DepGraph::build_simple(&skeletons);
        let result = graph.topological_order();
        assert!(result.is_acyclic);
        assert_eq!(result.order.len(), 4);
        let pos: FxHashMap<usize, usize> = result
            .order
            .iter()
            .enumerate()
            .map(|(pos, &idx)| (idx, pos))
            .collect();
        assert!(pos[&1] < pos[&0], "b before a");
        assert!(pos[&3] < pos[&2], "d before c");
    }
}
