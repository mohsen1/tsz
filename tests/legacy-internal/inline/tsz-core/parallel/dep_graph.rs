//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-core/src/parallel/dep_graph.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 0efef099f0d49c0a10383788ee9be6a94a4ae8ef30aebb8afd7a39031c60522e 383 empty_graph
    #[test]
    fn empty_graph() {
        let graph = DepGraph::build_simple(&[]);
        assert_eq!(graph.node_count, 0);
        assert_eq!(graph.edge_count, 0);
        let result = graph.topological_order();
        assert!(result.order.is_empty());
        assert!(result.is_acyclic);
    }
// TSZ_INLINE_TEST_END 0efef099f0d49c0a10383788ee9be6a94a4ae8ef30aebb8afd7a39031c60522e

// TSZ_INLINE_TEST_BEGIN 9abc3e384f4aae5e0bdc5790debe9bca9f076e4ddf13d6e6fa4f4960a30a6443 393 single_file_no_deps
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
// TSZ_INLINE_TEST_END 9abc3e384f4aae5e0bdc5790debe9bca9f076e4ddf13d6e6fa4f4960a30a6443

// TSZ_INLINE_TEST_BEGIN a761dee71a1f23e80d599f8f8d53c9e3ba8898320c33cf39dcf67964b8de3ac2 404 linear_chain
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
// TSZ_INLINE_TEST_END a761dee71a1f23e80d599f8f8d53c9e3ba8898320c33cf39dcf67964b8de3ac2

// TSZ_INLINE_TEST_BEGIN f257304cc618cb85d3085c9a609139dfff4ffbef2fcda343d294bad8f04a92e1 428 diamond_dependency
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
// TSZ_INLINE_TEST_END f257304cc618cb85d3085c9a609139dfff4ffbef2fcda343d294bad8f04a92e1

// TSZ_INLINE_TEST_BEGIN 70867e4de3b0217eff9e13533619a3d9cfc32dc7c7fe6ac24c3f4edf79729d6d 453 simple_cycle
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
// TSZ_INLINE_TEST_END 70867e4de3b0217eff9e13533619a3d9cfc32dc7c7fe6ac24c3f4edf79729d6d

// TSZ_INLINE_TEST_BEGIN 8a131d5158b9db6ca2c26cbbbc15d41811b459c73cd8a75180f2e803390be304 469 cycle_with_tail
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
// TSZ_INLINE_TEST_END 8a131d5158b9db6ca2c26cbbbc15d41811b459c73cd8a75180f2e803390be304

// TSZ_INLINE_TEST_BEGIN 2d4baa7829a9265253700bc1dd31cb5a62eb29cb3b6e83304aa6f510d6fc3a35 487 unresolved_specifiers_tracked
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
// TSZ_INLINE_TEST_END 2d4baa7829a9265253700bc1dd31cb5a62eb29cb3b6e83304aa6f510d6fc3a35

// TSZ_INLINE_TEST_BEGIN 77f5adb9d1f9ad6b5736420b868b33671d7cd1dd004438b7d0ff7b08abc5ac20 514 external_deps_silently_skipped
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
// TSZ_INLINE_TEST_END 77f5adb9d1f9ad6b5736420b868b33671d7cd1dd004438b7d0ff7b08abc5ac20

// TSZ_INLINE_TEST_BEGIN 8219d2e0e190e7993cce18f130ee78c3f311a4f50a634f202b2fbc09d2f39544 526 relative_import_resolution
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
// TSZ_INLINE_TEST_END 8219d2e0e190e7993cce18f130ee78c3f311a4f50a634f202b2fbc09d2f39544

// TSZ_INLINE_TEST_BEGIN 55398cfec21ef2723a5b7146f8d88f01a50bae091a39e14070ae32073d16db56 537 self_import_ignored
    #[test]
    fn self_import_ignored() {
        let skeletons = vec![make_skeleton("a.ts", &["a.ts"])];
        let graph = DepGraph::build_simple(&skeletons);
        assert_eq!(graph.edge_count, 0, "self-imports should not create edges");
        let result = graph.topological_order();
        assert!(result.is_acyclic);
    }
// TSZ_INLINE_TEST_END 55398cfec21ef2723a5b7146f8d88f01a50bae091a39e14070ae32073d16db56

// TSZ_INLINE_TEST_BEGIN 964bf6d3919d4f4742a2afd00cd399a2810b835c166703b774212a785fa16136 546 dependents_and_dependencies
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
// TSZ_INLINE_TEST_END 964bf6d3919d4f4742a2afd00cd399a2810b835c166703b774212a785fa16136

// TSZ_INLINE_TEST_BEGIN f622235eb72b24bbb9b087b3b125f779ab20815e13fc75c47358d1b127732d6f 557 roots_are_leaf_nodes
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
// TSZ_INLINE_TEST_END f622235eb72b24bbb9b087b3b125f779ab20815e13fc75c47358d1b127732d6f

// TSZ_INLINE_TEST_BEGIN 334c64bfb55629bba69779b61a89f732b68f9cfcc29040994450c30cf4012a40 571 custom_resolver
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
// TSZ_INLINE_TEST_END 334c64bfb55629bba69779b61a89f732b68f9cfcc29040994450c30cf4012a40

// TSZ_INLINE_TEST_BEGIN c7b83169f7c15e6f56908e347494f9c1f06373a02a2afd1cbadfc4df75c8140b 588 multiple_independent_components
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
// TSZ_INLINE_TEST_END c7b83169f7c15e6f56908e347494f9c1f06373a02a2afd1cbadfc4df75c8140b
