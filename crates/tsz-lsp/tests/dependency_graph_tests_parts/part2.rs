#[test]
fn test_update_file_no_change_still_valid() {
    let mut graph = DependencyGraph::new();
    let deps = vec!["b.ts".to_string()];
    graph.update_file("a.ts", &deps);
    graph.update_file("a.ts", &deps);

    assert_eq!(graph.file_count(), 2);
    assert!(graph.get_dependencies("a.ts").unwrap().contains("b.ts"));
    assert!(graph.get_dependents("b.ts").unwrap().contains("a.ts"));
}

#[test]
fn test_complex_web_topology() {
    let mut graph = DependencyGraph::new();
    // a -> b, c
    // b -> d
    // c -> d
    // d -> e
    graph.add_dependency("a.ts", "b.ts");
    graph.add_dependency("a.ts", "c.ts");
    graph.add_dependency("b.ts", "d.ts");
    graph.add_dependency("c.ts", "d.ts");
    graph.add_dependency("d.ts", "e.ts");

    let affected = graph.get_affected_files("e.ts");
    assert_eq!(affected.len(), 4);
    assert!(affected.contains(&"a.ts".to_string()));
    assert!(affected.contains(&"b.ts".to_string()));
    assert!(affected.contains(&"c.ts".to_string()));
    assert!(affected.contains(&"d.ts".to_string()));
}

#[test]
fn test_add_remove_add_same_edge() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "b.ts");
    graph.remove_file("a.ts");
    graph.add_dependency("a.ts", "b.ts");

    assert!(graph.contains_file("a.ts"));
    assert!(graph.get_dependencies("a.ts").unwrap().contains("b.ts"));
}

#[test]
fn test_get_dependents_after_all_removed() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "b.ts");
    graph.remove_file("a.ts");

    // b.ts had a dependent (a.ts) which was removed
    let dependents = graph.get_dependents("b.ts");
    // Either None or empty set
    if let Some(deps) = dependents {
        assert!(deps.is_empty());
    }
}

#[test]
fn test_update_file_with_overlapping_deps() {
    let mut graph = DependencyGraph::new();
    graph.update_file("a.ts", &["b.ts".to_string(), "c.ts".to_string()]);
    // Update with overlap: keep c, drop b, add d
    graph.update_file("a.ts", &["c.ts".to_string(), "d.ts".to_string()]);

    let deps = graph.get_dependencies("a.ts").unwrap();
    assert!(!deps.contains("b.ts"));
    assert!(deps.contains("c.ts"));
    assert!(deps.contains("d.ts"));
}

#[test]
fn test_affected_files_isolated_node() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "b.ts");
    // c.ts is unrelated
    graph.add_dependency("c.ts", "d.ts");

    let affected = graph.get_affected_files("b.ts");
    assert_eq!(affected.len(), 1);
    assert!(affected.contains(&"a.ts".to_string()));
    assert!(!affected.contains(&"c.ts".to_string()));
}

#[test]
fn test_file_count_after_clear() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "b.ts");
    graph.add_dependency("b.ts", "c.ts");
    graph.clear();
    assert_eq!(graph.file_count(), 0);
}

#[test]
fn test_unicode_file_paths() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("src/\u{00fc}ber.ts", "lib/\u{00e4}pp.ts");
    assert!(graph.contains_file("src/\u{00fc}ber.ts"));
    assert!(graph.contains_file("lib/\u{00e4}pp.ts"));
    let affected = graph.get_affected_files("lib/\u{00e4}pp.ts");
    assert_eq!(affected.len(), 1);
}

#[test]
fn test_deeply_nested_paths() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("src/a/b/c/d/e.ts", "src/a/b/c/d/f.ts");
    assert!(
        graph
            .get_dependencies("src/a/b/c/d/e.ts")
            .unwrap()
            .contains("src/a/b/c/d/f.ts")
    );
}

#[test]
fn test_remove_nonexistent_file_no_panic() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency("a.ts", "b.ts");
    graph.remove_file("zzz.ts"); // should not panic
    assert_eq!(graph.file_count(), 2);
}
