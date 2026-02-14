#[cfg(test)]
mod tests {
    use crate::dependency_graph::DependencyGraph;

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
