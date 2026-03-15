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

#[test]
fn test_self_import() {
    let mut graph = DependencyGraph::new();
    // a.ts imports itself
    graph.add_dependency("a.ts", "a.ts");

    assert!(graph.get_dependencies("a.ts").unwrap().contains("a.ts"));
    assert!(graph.get_dependents("a.ts").unwrap().contains("a.ts"));

    // When a.ts changes, a.ts is affected (self-referential)
    let affected = graph.get_affected_files("a.ts");
    assert_eq!(affected.len(), 1);
    assert!(affected.contains(&"a.ts".to_string()));
}

#[test]
fn test_large_star_topology() {
    let mut graph = DependencyGraph::new();
    // Many files all import "hub.ts"
    for i in 0..20 {
        let file = format!("file{i}.ts");
        graph.add_dependency(&file, "hub.ts");
    }

    // When hub.ts changes, all 20 files are affected
    let affected = graph.get_affected_files("hub.ts");
    assert_eq!(affected.len(), 20);
    for i in 0..20 {
        let file = format!("file{i}.ts");
        assert!(
            affected.contains(&file),
            "file{i}.ts should be affected when hub.ts changes"
        );
    }

    // hub.ts itself has no dependencies
    assert!(graph.get_dependencies("hub.ts").is_none());
}

#[test]
fn test_multiple_transitive_levels() {
    let mut graph = DependencyGraph::new();
    // Chain: a -> b -> c -> d -> e
    graph.add_dependency("a.ts", "b.ts");
    graph.add_dependency("b.ts", "c.ts");
    graph.add_dependency("c.ts", "d.ts");
    graph.add_dependency("d.ts", "e.ts");

    // When e.ts changes, a, b, c, d are all affected
    let affected = graph.get_affected_files("e.ts");
    assert_eq!(affected.len(), 4);
    assert!(affected.contains(&"a.ts".to_string()));
    assert!(affected.contains(&"b.ts".to_string()));
    assert!(affected.contains(&"c.ts".to_string()));
    assert!(affected.contains(&"d.ts".to_string()));

    // When c.ts changes, only a and b are affected
    let affected = graph.get_affected_files("c.ts");
    assert_eq!(affected.len(), 2);
    assert!(affected.contains(&"a.ts".to_string()));
    assert!(affected.contains(&"b.ts".to_string()));
}

#[test]
fn test_reexport_chain() {
    let mut graph = DependencyGraph::new();
    // index.ts re-exports from utils.ts which re-exports from helpers.ts
    // app.ts imports from index.ts
    graph.add_dependency("index.ts", "utils.ts");
    graph.add_dependency("utils.ts", "helpers.ts");
    graph.add_dependency("app.ts", "index.ts");

    // When helpers.ts changes, utils.ts, index.ts, and app.ts are affected
    let affected = graph.get_affected_files("helpers.ts");
    assert_eq!(affected.len(), 3);
    assert!(affected.contains(&"utils.ts".to_string()));
    assert!(affected.contains(&"index.ts".to_string()));
    assert!(affected.contains(&"app.ts".to_string()));
}

#[test]
fn test_file_with_no_dependents() {
    let mut graph = DependencyGraph::new();
    // leaf.ts imports base.ts, but nothing imports leaf.ts
    graph.add_dependency("leaf.ts", "base.ts");

    // leaf.ts has no dependents
    assert!(
        graph.get_dependents("leaf.ts").is_none(),
        "leaf.ts should have no dependents"
    );

    // When leaf.ts changes, nothing is affected
    let affected = graph.get_affected_files("leaf.ts");
    assert!(
        affected.is_empty(),
        "No files should be affected when a leaf changes"
    );
}

#[test]
fn test_file_with_no_dependencies() {
    let mut graph = DependencyGraph::new();
    // other.ts imports standalone.ts, but standalone.ts imports nothing
    graph.add_dependency("other.ts", "standalone.ts");

    // standalone.ts has no dependencies (imports nothing)
    assert!(
        graph.get_dependencies("standalone.ts").is_none(),
        "standalone.ts should have no dependencies"
    );

    // But standalone.ts has dependents
    assert!(
        graph.get_dependents("standalone.ts").is_some(),
        "standalone.ts should have dependents"
    );
    let standalone_deps = graph.get_dependents("standalone.ts").unwrap();
    assert!(
        standalone_deps.contains("other.ts"),
        "standalone.ts should have other.ts as dependent"
    );
}

#[test]
fn test_remove_middle_of_chain() {
    let mut graph = DependencyGraph::new();
    // Chain: a -> b -> c
    graph.add_dependency("a.ts", "b.ts");
    graph.add_dependency("b.ts", "c.ts");

    // Initially, changing c.ts affects both a.ts and b.ts
    let affected = graph.get_affected_files("c.ts");
    assert_eq!(affected.len(), 2);

    // Remove b.ts from the graph
    graph.remove_file("b.ts");

    // Now changing c.ts should not affect anything
    // (b.ts was the only dependent of c.ts, and it was removed)
    let affected = graph.get_affected_files("c.ts");
    assert!(
        affected.is_empty(),
        "After removing b.ts, c.ts should have no affected files, got {:?}",
        affected
    );

    // a.ts still exists but its dependency on b.ts was cleared
    // (a.ts's dependency set still exists but b.ts was removed from it)
}

#[test]
fn test_file_count() {
    let mut graph = DependencyGraph::new();
    assert_eq!(graph.file_count(), 0, "Empty graph should have 0 files");

    graph.add_dependency("a.ts", "b.ts");
    assert_eq!(
        graph.file_count(),
        2,
        "Graph with a->b should track 2 files"
    );

    graph.add_dependency("a.ts", "c.ts");
    assert_eq!(
        graph.file_count(),
        3,
        "Graph with a->b, a->c should track 3 files"
    );

    graph.add_dependency("b.ts", "c.ts");
    assert_eq!(
        graph.file_count(),
        3,
        "Adding b->c should not change count (all files already tracked)"
    );
}

#[test]
fn test_clear() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "b.ts");
    graph.add_dependency("b.ts", "c.ts");

    assert!(graph.file_count() > 0);

    graph.clear();

    assert_eq!(graph.file_count(), 0, "Cleared graph should have 0 files");
    assert!(graph.get_dependencies("a.ts").is_none());
    assert!(graph.get_dependents("b.ts").is_none());
}

#[test]
fn test_contains_file() {
    let mut graph = DependencyGraph::new();
    assert!(
        !graph.contains_file("a.ts"),
        "Empty graph should not contain any file"
    );

    graph.add_dependency("a.ts", "b.ts");
    assert!(graph.contains_file("a.ts"), "a.ts should be in the graph");
    assert!(graph.contains_file("b.ts"), "b.ts should be in the graph");
    assert!(
        !graph.contains_file("c.ts"),
        "c.ts should not be in the graph"
    );
}

#[test]
fn test_update_file_to_same_imports() {
    let mut graph = DependencyGraph::new();
    graph.update_file("a.ts", &["b.ts".to_string(), "c.ts".to_string()]);

    // Update with the same imports - should be a no-op effectively
    graph.update_file("a.ts", &["b.ts".to_string(), "c.ts".to_string()]);

    assert!(graph.get_dependencies("a.ts").unwrap().contains("b.ts"));
    assert!(graph.get_dependencies("a.ts").unwrap().contains("c.ts"));
    assert!(graph.get_dependents("b.ts").unwrap().contains("a.ts"));
    assert!(graph.get_dependents("c.ts").unwrap().contains("a.ts"));
}

#[test]
fn test_add_duplicate_dependency() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "b.ts");
    graph.add_dependency("a.ts", "b.ts");

    // Should still be just one entry in the set
    assert_eq!(graph.get_dependencies("a.ts").unwrap().len(), 1);
    assert_eq!(graph.get_dependents("b.ts").unwrap().len(), 1);
}

#[test]
fn test_remove_nonexistent_file() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "b.ts");

    // Removing a file that doesn't exist should not panic
    graph.remove_file("z.ts");

    // Original edges should still be intact
    assert!(graph.get_dependencies("a.ts").unwrap().contains("b.ts"));
    assert!(graph.get_dependents("b.ts").unwrap().contains("a.ts"));
}

#[test]
fn test_complex_bidirectional_edges() {
    let mut graph = DependencyGraph::new();
    // Mutual import: a imports b, b imports a
    graph.add_dependency("a.ts", "b.ts");
    graph.add_dependency("b.ts", "a.ts");

    assert!(graph.get_dependencies("a.ts").unwrap().contains("b.ts"));
    assert!(graph.get_dependencies("b.ts").unwrap().contains("a.ts"));
    assert!(graph.get_dependents("a.ts").unwrap().contains("b.ts"));
    assert!(graph.get_dependents("b.ts").unwrap().contains("a.ts"));

    // Changing a.ts affects b.ts and a.ts (cycle)
    let affected = graph.get_affected_files("a.ts");
    assert_eq!(affected.len(), 2);
    assert!(affected.contains(&"a.ts".to_string()));
    assert!(affected.contains(&"b.ts".to_string()));
}

#[test]
fn test_get_affected_files_no_dependents() {
    let graph = DependencyGraph::new();
    let affected = graph.get_affected_files("nonexistent.ts");
    assert!(
        affected.is_empty(),
        "File with no dependents in empty graph should have empty affected set"
    );
}

#[test]
fn test_update_file_adds_to_empty_graph() {
    let mut graph = DependencyGraph::new();
    assert_eq!(graph.file_count(), 0);

    graph.update_file("main.ts", &["utils.ts".to_string(), "types.ts".to_string()]);

    assert_eq!(graph.file_count(), 3);
    assert!(graph.contains_file("main.ts"));
    assert!(graph.contains_file("utils.ts"));
    assert!(graph.contains_file("types.ts"));
}

#[test]
fn test_remove_file_cleans_up_dependents() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "shared.ts");
    graph.add_dependency("b.ts", "shared.ts");

    // shared.ts has two dependents
    assert_eq!(graph.get_dependents("shared.ts").unwrap().len(), 2);

    // Remove a.ts
    graph.remove_file("a.ts");

    // shared.ts should now have only one dependent (b.ts)
    assert_eq!(graph.get_dependents("shared.ts").unwrap().len(), 1);
    assert!(graph.get_dependents("shared.ts").unwrap().contains("b.ts"));
}

#[test]
fn test_update_file_partial_overlap() {
    let mut graph = DependencyGraph::new();
    graph.update_file("app.ts", &["a.ts".to_string(), "b.ts".to_string()]);

    // Update with partial overlap: keep b, drop a, add c
    graph.update_file("app.ts", &["b.ts".to_string(), "c.ts".to_string()]);

    let deps = graph.get_dependencies("app.ts").unwrap();
    assert!(!deps.contains("a.ts"), "a.ts should be removed");
    assert!(deps.contains("b.ts"), "b.ts should be kept");
    assert!(deps.contains("c.ts"), "c.ts should be added");

    // a.ts should no longer have app.ts as dependent
    assert!(graph.get_dependents("a.ts").is_none());
    // b.ts should still have app.ts as dependent
    assert!(graph.get_dependents("b.ts").unwrap().contains("app.ts"));
    // c.ts should have app.ts as dependent
    assert!(graph.get_dependents("c.ts").unwrap().contains("app.ts"));
}

// =========================================================================
// Additional dependency graph tests for broader coverage
// =========================================================================

#[test]
fn test_two_disconnected_components() {
    let mut graph = DependencyGraph::new();
    // Component 1: a -> b
    graph.add_dependency("a.ts", "b.ts");
    // Component 2: c -> d
    graph.add_dependency("c.ts", "d.ts");

    // Changing b.ts should only affect a.ts
    let affected = graph.get_affected_files("b.ts");
    assert_eq!(affected.len(), 1);
    assert!(affected.contains(&"a.ts".to_string()));
    assert!(!affected.contains(&"c.ts".to_string()));

    // Changing d.ts should only affect c.ts
    let affected = graph.get_affected_files("d.ts");
    assert_eq!(affected.len(), 1);
    assert!(affected.contains(&"c.ts".to_string()));
    assert!(!affected.contains(&"a.ts".to_string()));
}

#[test]
fn test_wide_fan_out_from_single_file() {
    let mut graph = DependencyGraph::new();
    // consumer.ts imports 15 different files
    for i in 0..15 {
        let dep = format!("dep{i}.ts");
        graph.add_dependency("consumer.ts", &dep);
    }

    assert_eq!(graph.get_dependencies("consumer.ts").unwrap().len(), 15);

    // Changing any dep should only affect consumer.ts
    let affected = graph.get_affected_files("dep7.ts");
    assert_eq!(affected.len(), 1);
    assert!(affected.contains(&"consumer.ts".to_string()));
}

#[test]
fn test_clear_then_rebuild() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "b.ts");
    graph.add_dependency("b.ts", "c.ts");
    graph.clear();

    assert_eq!(graph.file_count(), 0);

    // Rebuild the graph with new files
    graph.add_dependency("x.ts", "y.ts");
    assert_eq!(graph.file_count(), 2);
    assert!(graph.contains_file("x.ts"));
    assert!(!graph.contains_file("a.ts"));
}

#[test]
fn test_contains_file_after_removal() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "b.ts");
    assert!(graph.contains_file("a.ts"));

    graph.remove_file("a.ts");
    assert!(
        !graph.contains_file("a.ts"),
        "Removed file should not be contained"
    );
}

#[test]
fn test_file_count_after_update_to_empty() {
    let mut graph = DependencyGraph::new();
    graph.update_file("a.ts", &["b.ts".to_string()]);
    assert_eq!(graph.file_count(), 2);

    // Clear a.ts's imports
    graph.update_file("a.ts", &[]);

    // Both maps should be empty now
    assert_eq!(graph.file_count(), 0);
}

#[test]
fn test_remove_file_with_multiple_dependents_and_deps() {
    let mut graph = DependencyGraph::new();
    // mid.ts imports base.ts, and both a.ts and b.ts import mid.ts
    graph.add_dependency("mid.ts", "base.ts");
    graph.add_dependency("a.ts", "mid.ts");
    graph.add_dependency("b.ts", "mid.ts");

    // Remove mid.ts
    graph.remove_file("mid.ts");

    // mid.ts should be gone
    assert!(!graph.contains_file("mid.ts"));

    // base.ts should have no dependents
    assert!(graph.get_dependents("base.ts").is_none());

    // a.ts and b.ts still exist but their dep on mid.ts is cleared
    if let Some(deps) = graph.get_dependencies("a.ts") {
        assert!(!deps.contains("mid.ts"));
    }
}

#[test]
fn test_complex_cycle_with_branch() {
    let mut graph = DependencyGraph::new();
    // Cycle: a -> b -> c -> a
    // Branch: d -> b (d depends on b which is in the cycle)
    graph.add_dependency("a.ts", "b.ts");
    graph.add_dependency("b.ts", "c.ts");
    graph.add_dependency("c.ts", "a.ts");
    graph.add_dependency("d.ts", "b.ts");

    // When b.ts changes, a.ts is affected (imports b), c.ts (via cycle), d.ts (imports b)
    let affected = graph.get_affected_files("b.ts");
    assert!(affected.contains(&"a.ts".to_string()));
    assert!(affected.contains(&"d.ts".to_string()));
    // The cycle means c.ts -> a.ts -> (needs b.ts, which is the changed file)
    // But since we traverse dependents, b.ts dependents are {a, d}
    // a.ts dependents are {c}, c.ts dependents are {b} - but b is already visited
    assert!(
        affected.len() >= 2,
        "At least a.ts and d.ts should be affected"
    );
}

#[test]
fn test_update_file_completely_new_deps() {
    let mut graph = DependencyGraph::new();
    graph.update_file("main.ts", &["a.ts".to_string(), "b.ts".to_string()]);

    // Completely replace with new deps
    graph.update_file(
        "main.ts",
        &["x.ts".to_string(), "y.ts".to_string(), "z.ts".to_string()],
    );

    let deps = graph.get_dependencies("main.ts").unwrap();
    assert_eq!(deps.len(), 3);
    assert!(deps.contains("x.ts"));
    assert!(deps.contains("y.ts"));
    assert!(deps.contains("z.ts"));
    assert!(!deps.contains("a.ts"));
    assert!(!deps.contains("b.ts"));

    // Old deps should have no dependents
    assert!(graph.get_dependents("a.ts").is_none());
    assert!(graph.get_dependents("b.ts").is_none());
}

#[test]
fn test_multiple_files_import_same_target() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "shared.ts");
    graph.add_dependency("b.ts", "shared.ts");
    graph.add_dependency("c.ts", "shared.ts");

    let dependents = graph.get_dependents("shared.ts").unwrap();
    assert_eq!(dependents.len(), 3);
    assert!(dependents.contains("a.ts"));
    assert!(dependents.contains("b.ts"));
    assert!(dependents.contains("c.ts"));
}

#[test]
fn test_affected_files_excludes_changed_file_unless_cycle() {
    let mut graph = DependencyGraph::new();
    // Simple chain: a -> b -> c (no cycle)
    graph.add_dependency("a.ts", "b.ts");
    graph.add_dependency("b.ts", "c.ts");

    // When c.ts changes, c.ts itself should NOT be in affected
    let affected = graph.get_affected_files("c.ts");
    assert!(!affected.contains(&"c.ts".to_string()));
    assert!(affected.contains(&"a.ts".to_string()));
    assert!(affected.contains(&"b.ts".to_string()));
}

#[test]
fn test_long_chain_affected_count() {
    let mut graph = DependencyGraph::new();
    // Chain of 10: f0 -> f1 -> f2 -> ... -> f9
    for i in 0..9 {
        let from = format!("f{}.ts", i);
        let to = format!("f{}.ts", i + 1);
        graph.add_dependency(&from, &to);
    }

    // When f9 changes, f0 through f8 are affected (9 files)
    let affected = graph.get_affected_files("f9.ts");
    assert_eq!(affected.len(), 9);
    for i in 0..9 {
        let file = format!("f{i}.ts");
        assert!(affected.contains(&file));
    }
}

#[test]
fn test_file_count_with_self_import() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "a.ts");

    // Self-import should count as 1 file, not 2
    assert_eq!(
        graph.file_count(),
        1,
        "Self-importing file should be counted once"
    );
}

// =========================================================================
// Additional tests to reach 50
// =========================================================================

#[test]
fn test_remove_file_then_readd() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "b.ts");
    graph.remove_file("a.ts");

    // Re-add the same dependency
    graph.add_dependency("a.ts", "b.ts");
    assert!(graph.get_dependencies("a.ts").unwrap().contains("b.ts"));
    assert!(graph.get_dependents("b.ts").unwrap().contains("a.ts"));
}

#[test]
fn test_update_file_single_dep_to_many() {
    let mut graph = DependencyGraph::new();
    graph.update_file("app.ts", &["a.ts".to_string()]);
    assert_eq!(graph.get_dependencies("app.ts").unwrap().len(), 1);

    graph.update_file(
        "app.ts",
        &[
            "a.ts".to_string(),
            "b.ts".to_string(),
            "c.ts".to_string(),
            "d.ts".to_string(),
        ],
    );
    assert_eq!(graph.get_dependencies("app.ts").unwrap().len(), 4);
}

#[test]
fn test_three_node_cycle_all_affected_from_any() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "b.ts");
    graph.add_dependency("b.ts", "c.ts");
    graph.add_dependency("c.ts", "a.ts");

    // From any node in the cycle, all 3 should be affected
    for start in &["a.ts", "b.ts", "c.ts"] {
        let affected = graph.get_affected_files(start);
        assert_eq!(
            affected.len(),
            3,
            "All nodes in cycle should be affected when {} changes",
            start
        );
    }
}

#[test]
fn test_parallel_chains_no_cross_contamination() {
    let mut graph = DependencyGraph::new();
    // Chain 1: a1 -> b1 -> c1
    graph.add_dependency("a1.ts", "b1.ts");
    graph.add_dependency("b1.ts", "c1.ts");
    // Chain 2: a2 -> b2 -> c2
    graph.add_dependency("a2.ts", "b2.ts");
    graph.add_dependency("b2.ts", "c2.ts");

    let affected = graph.get_affected_files("c1.ts");
    assert!(affected.contains(&"a1.ts".to_string()));
    assert!(affected.contains(&"b1.ts".to_string()));
    assert!(!affected.contains(&"a2.ts".to_string()));
    assert!(!affected.contains(&"b2.ts".to_string()));
}

#[test]
fn test_remove_last_dependent_cleans_up() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "shared.ts");

    // Remove the only dependent
    graph.remove_file("a.ts");

    // shared.ts should have no dependents
    assert!(
        graph.get_dependents("shared.ts").is_none(),
        "shared.ts should have no dependents after removing its only dependent"
    );
}

#[test]
fn test_update_file_removes_stale_dependents_precisely() {
    let mut graph = DependencyGraph::new();
    // a.ts and b.ts both import shared.ts
    graph.add_dependency("a.ts", "shared.ts");
    graph.add_dependency("b.ts", "shared.ts");

    // Now a.ts stops importing shared.ts
    graph.update_file("a.ts", &[]);

    // shared.ts should still have b.ts as dependent
    let dependents = graph.get_dependents("shared.ts").unwrap();
    assert_eq!(dependents.len(), 1);
    assert!(dependents.contains("b.ts"));
}

#[test]
fn test_diamond_with_extra_leaf() {
    let mut graph = DependencyGraph::new();
    // Diamond: a -> b, a -> c, b -> d, c -> d
    // Plus extra leaf: e -> d
    graph.add_dependency("a.ts", "b.ts");
    graph.add_dependency("a.ts", "c.ts");
    graph.add_dependency("b.ts", "d.ts");
    graph.add_dependency("c.ts", "d.ts");
    graph.add_dependency("e.ts", "d.ts");

    let affected = graph.get_affected_files("d.ts");
    assert_eq!(affected.len(), 4); // a, b, c, e
    assert!(affected.contains(&"e.ts".to_string()));
    assert!(affected.contains(&"a.ts".to_string()));
}

#[test]
fn test_contains_file_both_sides() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("importer.ts", "imported.ts");

    // Both the importer and imported file should be "contained"
    assert!(graph.contains_file("importer.ts"));
    assert!(graph.contains_file("imported.ts"));
}

#[test]
fn test_clear_followed_by_contains() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "b.ts");
    graph.clear();

    assert!(!graph.contains_file("a.ts"));
    assert!(!graph.contains_file("b.ts"));
    assert_eq!(graph.file_count(), 0);
}

#[test]
fn test_get_dependencies_returns_none_for_unknown_file() {
    let graph = DependencyGraph::new();
    assert!(
        graph.get_dependencies("nonexistent.ts").is_none(),
        "Unknown file should return None for dependencies"
    );
}

#[test]
fn test_get_dependents_returns_none_for_unknown_file() {
    let graph = DependencyGraph::new();
    assert!(
        graph.get_dependents("nonexistent.ts").is_none(),
        "Unknown file should return None for dependents"
    );
}

#[test]
fn test_update_file_with_duplicate_imports() {
    let mut graph = DependencyGraph::new();
    // Provide duplicate imports in the list
    graph.update_file(
        "a.ts",
        &["b.ts".to_string(), "b.ts".to_string(), "c.ts".to_string()],
    );

    // Should deduplicate: a.ts depends on b.ts and c.ts
    let deps = graph.get_dependencies("a.ts").unwrap();
    assert!(deps.contains("b.ts"));
    assert!(deps.contains("c.ts"));
    // b.ts should have exactly 1 dependent (a.ts), not 2
    assert_eq!(graph.get_dependents("b.ts").unwrap().len(), 1);
}

// =========================================================================
// Additional tests to reach 65+
// =========================================================================

#[test]
fn test_add_dependency_creates_both_maps() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("src/app.ts", "src/lib.ts");

    assert!(graph.get_dependencies("src/app.ts").is_some());
    assert!(graph.get_dependents("src/lib.ts").is_some());
    // The imported file should NOT appear in dependencies map as a key (it imports nothing)
    assert!(graph.get_dependencies("src/lib.ts").is_none());
    // The importer should NOT appear in dependents map as a key (nothing imports it)
    assert!(graph.get_dependents("src/app.ts").is_none());
}

#[test]
fn test_remove_file_preserves_unrelated_edges() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "shared.ts");
    graph.add_dependency("b.ts", "shared.ts");
    graph.add_dependency("c.ts", "other.ts");

    graph.remove_file("a.ts");

    // b -> shared and c -> other should be intact
    assert!(
        graph
            .get_dependencies("b.ts")
            .unwrap()
            .contains("shared.ts")
    );
    assert!(graph.get_dependents("shared.ts").unwrap().contains("b.ts"));
    assert!(graph.get_dependencies("c.ts").unwrap().contains("other.ts"));
    assert!(graph.get_dependents("other.ts").unwrap().contains("c.ts"));
}

#[test]
fn test_update_file_idempotent_multiple_times() {
    let mut graph = DependencyGraph::new();
    let imports = vec!["x.ts".to_string(), "y.ts".to_string()];

    for _ in 0..5 {
        graph.update_file("main.ts", &imports);
    }

    assert_eq!(graph.get_dependencies("main.ts").unwrap().len(), 2);
    assert_eq!(graph.get_dependents("x.ts").unwrap().len(), 1);
    assert_eq!(graph.get_dependents("y.ts").unwrap().len(), 1);
}

#[test]
fn test_affected_files_with_two_separate_cycles() {
    let mut graph = DependencyGraph::new();
    // Cycle 1: a -> b -> a
    graph.add_dependency("a.ts", "b.ts");
    graph.add_dependency("b.ts", "a.ts");
    // Cycle 2: c -> d -> c
    graph.add_dependency("c.ts", "d.ts");
    graph.add_dependency("d.ts", "c.ts");

    let affected = graph.get_affected_files("a.ts");
    assert!(affected.contains(&"b.ts".to_string()));
    assert!(affected.contains(&"a.ts".to_string()));
    assert!(!affected.contains(&"c.ts".to_string()));
    assert!(!affected.contains(&"d.ts".to_string()));
}

#[test]
fn test_file_count_after_remove_all() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "b.ts");
    graph.add_dependency("b.ts", "c.ts");

    graph.remove_file("a.ts");
    graph.remove_file("b.ts");
    graph.remove_file("c.ts");

    assert_eq!(graph.file_count(), 0);
}

#[test]
fn test_update_file_grow_shrink_grow() {
    let mut graph = DependencyGraph::new();

    // Start with 3 deps
    graph.update_file(
        "app.ts",
        &["a.ts".to_string(), "b.ts".to_string(), "c.ts".to_string()],
    );
    assert_eq!(graph.get_dependencies("app.ts").unwrap().len(), 3);

    // Shrink to 1 dep
    graph.update_file("app.ts", &["a.ts".to_string()]);
    assert_eq!(graph.get_dependencies("app.ts").unwrap().len(), 1);

    // Grow to 4 deps
    graph.update_file(
        "app.ts",
        &[
            "a.ts".to_string(),
            "b.ts".to_string(),
            "c.ts".to_string(),
            "d.ts".to_string(),
        ],
    );
    assert_eq!(graph.get_dependencies("app.ts").unwrap().len(), 4);
}

#[test]
fn test_affected_files_returns_vec_not_set() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "root.ts");
    graph.add_dependency("b.ts", "root.ts");

    let affected = graph.get_affected_files("root.ts");
    // Should have exactly 2 unique entries
    let mut sorted = affected.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        affected.len(),
        sorted.len(),
        "No duplicates in affected files"
    );
}

#[test]
fn test_deep_tree_topology() {
    let mut graph = DependencyGraph::new();
    //       root
    //      /    \
    //    l1a    l1b
    //   / \    / \
    // l2a l2b l2c l2d
    graph.add_dependency("l1a.ts", "root.ts");
    graph.add_dependency("l1b.ts", "root.ts");
    graph.add_dependency("l2a.ts", "l1a.ts");
    graph.add_dependency("l2b.ts", "l1a.ts");
    graph.add_dependency("l2c.ts", "l1b.ts");
    graph.add_dependency("l2d.ts", "l1b.ts");

    let affected = graph.get_affected_files("root.ts");
    assert_eq!(affected.len(), 6);
    for name in &["l1a.ts", "l1b.ts", "l2a.ts", "l2b.ts", "l2c.ts", "l2d.ts"] {
        assert!(
            affected.contains(&name.to_string()),
            "{name} should be affected"
        );
    }
}

#[test]
fn test_contains_file_after_update_to_empty() {
    let mut graph = DependencyGraph::new();
    graph.update_file("a.ts", &["b.ts".to_string()]);
    assert!(graph.contains_file("a.ts"));
    assert!(graph.contains_file("b.ts"));

    graph.update_file("a.ts", &[]);
    // After clearing a.ts's deps, neither should remain tracked
    assert!(!graph.contains_file("a.ts"));
    assert!(!graph.contains_file("b.ts"));
}

#[test]
fn test_update_file_swap_dependency() {
    let mut graph = DependencyGraph::new();
    // a imports b
    graph.update_file("a.ts", &["b.ts".to_string()]);
    // Now a imports c instead
    graph.update_file("a.ts", &["c.ts".to_string()]);

    assert!(!graph.get_dependencies("a.ts").unwrap().contains("b.ts"));
    assert!(graph.get_dependencies("a.ts").unwrap().contains("c.ts"));
    assert!(graph.get_dependents("b.ts").is_none());
    assert!(graph.get_dependents("c.ts").unwrap().contains("a.ts"));
}

#[test]
fn test_multiple_files_same_dependency_remove_one() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "lib.ts");
    graph.add_dependency("b.ts", "lib.ts");
    graph.add_dependency("c.ts", "lib.ts");

    graph.remove_file("b.ts");

    let dependents = graph.get_dependents("lib.ts").unwrap();
    assert_eq!(dependents.len(), 2);
    assert!(dependents.contains("a.ts"));
    assert!(dependents.contains("c.ts"));
    assert!(!dependents.contains("b.ts"));
}

#[test]
fn test_clear_then_affected_files() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "b.ts");
    graph.clear();

    let affected = graph.get_affected_files("b.ts");
    assert!(
        affected.is_empty(),
        "Cleared graph should produce no affected files"
    );
}

#[test]
fn test_paths_with_slashes() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("src/components/App.tsx", "src/utils/helpers.ts");
    graph.add_dependency("src/utils/helpers.ts", "src/types/index.ts");

    let affected = graph.get_affected_files("src/types/index.ts");
    assert_eq!(affected.len(), 2);
    assert!(affected.contains(&"src/utils/helpers.ts".to_string()));
    assert!(affected.contains(&"src/components/App.tsx".to_string()));
}

#[test]
fn test_add_dependency_after_clear() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "b.ts");
    graph.clear();
    graph.add_dependency("c.ts", "d.ts");

    assert!(!graph.contains_file("a.ts"));
    assert!(!graph.contains_file("b.ts"));
    assert!(graph.contains_file("c.ts"));
    assert!(graph.contains_file("d.ts"));
    assert_eq!(graph.file_count(), 2);
}

#[test]
fn test_affected_files_large_fan_in() {
    let mut graph = DependencyGraph::new();
    // 50 files all import hub.ts
    for i in 0..50 {
        graph.add_dependency(&format!("f{i}.ts"), "hub.ts");
    }

    let affected = graph.get_affected_files("hub.ts");
    assert_eq!(affected.len(), 50);
}

#[test]
fn test_remove_file_that_is_only_dependent() {
    let mut graph = DependencyGraph::new();
    // b.ts depends on a.ts; a.ts depends on nothing
    graph.add_dependency("b.ts", "a.ts");

    graph.remove_file("b.ts");
    assert!(!graph.contains_file("b.ts"));
    // a.ts should have no dependents left
    assert!(graph.get_dependents("a.ts").is_none());
}

// =========================================================================
// Additional tests to reach 80+ (batch 4)
// =========================================================================

#[test]
fn test_add_dependency_multiple_targets_from_same_source() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("app.ts", "util.ts");
    graph.add_dependency("app.ts", "types.ts");
    graph.add_dependency("app.ts", "config.ts");

    let deps = graph.get_dependencies("app.ts").unwrap();
    assert_eq!(deps.len(), 3);
    assert!(deps.contains("util.ts"));
    assert!(deps.contains("types.ts"));
    assert!(deps.contains("config.ts"));
}

#[test]
fn test_affected_files_single_node_no_deps() {
    let mut graph = DependencyGraph::new();
    graph.update_file("standalone.ts", &[]);
    // standalone.ts has no deps, no dependents
    let affected = graph.get_affected_files("standalone.ts");
    assert!(affected.is_empty());
}

#[test]
fn test_update_file_to_self_import() {
    let mut graph = DependencyGraph::new();
    graph.update_file("a.ts", &["a.ts".to_string()]);

    assert!(graph.get_dependencies("a.ts").unwrap().contains("a.ts"));
    assert!(graph.get_dependents("a.ts").unwrap().contains("a.ts"));
}

#[test]
fn test_remove_file_with_self_import() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "a.ts");
    graph.remove_file("a.ts");

    assert!(!graph.contains_file("a.ts"));
    assert!(graph.get_dependencies("a.ts").is_none());
    assert!(graph.get_dependents("a.ts").is_none());
}

#[test]
fn test_diamond_affected_files_no_duplicates() {
    let mut graph = DependencyGraph::new();
    // Diamond: a -> b, a -> c, b -> d, c -> d
    graph.add_dependency("a.ts", "b.ts");
    graph.add_dependency("a.ts", "c.ts");
    graph.add_dependency("b.ts", "d.ts");
    graph.add_dependency("c.ts", "d.ts");

    let affected = graph.get_affected_files("d.ts");
    let mut sorted = affected.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        affected.len(),
        sorted.len(),
        "Diamond should not produce duplicate affected files"
    );
}

#[test]
fn test_update_file_replace_single_dep() {
    let mut graph = DependencyGraph::new();
    graph.update_file("app.ts", &["old.ts".to_string()]);
    graph.update_file("app.ts", &["new.ts".to_string()]);

    assert!(!graph.get_dependencies("app.ts").unwrap().contains("old.ts"));
    assert!(graph.get_dependencies("app.ts").unwrap().contains("new.ts"));
    assert!(graph.get_dependents("old.ts").is_none());
    assert!(graph.get_dependents("new.ts").unwrap().contains("app.ts"));
}

#[test]
fn test_complex_three_layer_hierarchy() {
    let mut graph = DependencyGraph::new();
    // Layer 0: app.ts
    // Layer 1: service.ts, controller.ts
    // Layer 2: repo.ts
    graph.add_dependency("app.ts", "service.ts");
    graph.add_dependency("app.ts", "controller.ts");
    graph.add_dependency("service.ts", "repo.ts");
    graph.add_dependency("controller.ts", "repo.ts");

    // When repo.ts changes, all 3 are affected
    let affected = graph.get_affected_files("repo.ts");
    assert_eq!(affected.len(), 3);
    assert!(affected.contains(&"service.ts".to_string()));
    assert!(affected.contains(&"controller.ts".to_string()));
    assert!(affected.contains(&"app.ts".to_string()));

    // When service.ts changes, only app.ts is affected
    let affected = graph.get_affected_files("service.ts");
    assert_eq!(affected.len(), 1);
    assert!(affected.contains(&"app.ts".to_string()));
}

#[test]
fn test_file_count_with_multiple_shared_deps() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "shared.ts");
    graph.add_dependency("b.ts", "shared.ts");
    graph.add_dependency("c.ts", "shared.ts");

    // 4 unique files: a, b, c, shared
    assert_eq!(graph.file_count(), 4);
}

#[test]
fn test_clear_and_rebuild_complex() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "b.ts");
    graph.add_dependency("b.ts", "c.ts");
    graph.add_dependency("c.ts", "d.ts");

    graph.clear();
    assert_eq!(graph.file_count(), 0);

    // Rebuild with different topology
    graph.add_dependency("x.ts", "y.ts");
    graph.add_dependency("y.ts", "z.ts");

    assert_eq!(graph.file_count(), 3);
    assert!(!graph.contains_file("a.ts"));
    assert!(graph.contains_file("x.ts"));

    let affected = graph.get_affected_files("z.ts");
    assert_eq!(affected.len(), 2);
    assert!(affected.contains(&"x.ts".to_string()));
    assert!(affected.contains(&"y.ts".to_string()));
}

#[test]
fn test_remove_all_deps_of_shared() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "shared.ts");
    graph.add_dependency("b.ts", "shared.ts");
    graph.add_dependency("c.ts", "shared.ts");

    graph.remove_file("a.ts");
    graph.remove_file("b.ts");
    graph.remove_file("c.ts");

    // shared.ts should have no dependents
    assert!(graph.get_dependents("shared.ts").is_none());
}

#[test]
fn test_affected_files_with_bridge_node() {
    let mut graph = DependencyGraph::new();
    // a -> bridge -> c, d
    // b -> bridge
    graph.add_dependency("a.ts", "bridge.ts");
    graph.add_dependency("b.ts", "bridge.ts");
    graph.add_dependency("bridge.ts", "c.ts");
    graph.add_dependency("bridge.ts", "d.ts");

    // When c.ts changes, bridge is affected, then a and b
    let affected = graph.get_affected_files("c.ts");
    assert!(affected.contains(&"bridge.ts".to_string()));
    assert!(affected.contains(&"a.ts".to_string()));
    assert!(affected.contains(&"b.ts".to_string()));
    assert_eq!(affected.len(), 3);
}

#[test]
fn test_update_file_preserves_other_files_deps() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "shared.ts");
    graph.add_dependency("b.ts", "shared.ts");

    // Update only a.ts
    graph.update_file("a.ts", &["other.ts".to_string()]);

    // b.ts deps should be untouched
    assert!(
        graph
            .get_dependencies("b.ts")
            .unwrap()
            .contains("shared.ts")
    );
    assert!(graph.get_dependents("shared.ts").unwrap().contains("b.ts"));
}

#[test]
fn test_get_dependencies_after_add_remove_add() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "b.ts");
    graph.remove_file("a.ts");
    graph.add_dependency("a.ts", "c.ts");

    let deps = graph.get_dependencies("a.ts").unwrap();
    assert!(deps.contains("c.ts"));
    assert!(!deps.contains("b.ts"));
}

#[test]
fn test_four_node_cycle_affected_from_any() {
    let mut graph = DependencyGraph::new();
    // a -> b -> c -> d -> a
    graph.add_dependency("a.ts", "b.ts");
    graph.add_dependency("b.ts", "c.ts");
    graph.add_dependency("c.ts", "d.ts");
    graph.add_dependency("d.ts", "a.ts");

    for start in &["a.ts", "b.ts", "c.ts", "d.ts"] {
        let affected = graph.get_affected_files(start);
        assert_eq!(
            affected.len(),
            4,
            "All 4 nodes should be affected when {} changes",
            start
        );
    }
}

#[test]
fn test_file_extensions_variety() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("app.tsx", "util.ts");
    graph.add_dependency("util.ts", "types.d.ts");

    assert!(graph.contains_file("app.tsx"));
    assert!(graph.contains_file("util.ts"));
    assert!(graph.contains_file("types.d.ts"));

    let affected = graph.get_affected_files("types.d.ts");
    assert_eq!(affected.len(), 2);
    assert!(affected.contains(&"util.ts".to_string()));
    assert!(affected.contains(&"app.tsx".to_string()));
}

#[test]
fn test_update_file_empty_then_add_deps() {
    let mut graph = DependencyGraph::new();
    graph.update_file("a.ts", &[]);
    assert_eq!(graph.file_count(), 0);

    graph.update_file("a.ts", &["b.ts".to_string(), "c.ts".to_string()]);
    assert_eq!(graph.file_count(), 3);
    assert!(graph.get_dependencies("a.ts").unwrap().contains("b.ts"));
    assert!(graph.get_dependencies("a.ts").unwrap().contains("c.ts"));
}
