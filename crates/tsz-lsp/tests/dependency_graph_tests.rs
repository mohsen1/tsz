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
