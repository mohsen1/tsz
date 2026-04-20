#[test]
fn test_eviction_candidates_filters_by_min_idle() {
    use web_time::Duration;

    let mut project = Project::new();
    project.set_file("/a.ts".to_string(), "const a = 1;".to_string());
    project.set_file("/b.ts".to_string(), "const b = 2;".to_string());

    // Touch file a so it's recently accessed
    project.touch_file("/a.ts");

    // With a very high min_idle threshold, recently touched files should be filtered out.
    // Both files were just created/touched, so a 1-hour threshold filters all of them.
    let candidates = project.eviction_candidates(Some(Duration::from_secs(3600)));
    assert!(
        candidates.is_empty(),
        "recently accessed files should not be eviction candidates with high min_idle"
    );

    // With zero threshold, all files should be candidates
    let candidates = project.eviction_candidates(Some(Duration::ZERO));
    assert_eq!(
        candidates.len(),
        2,
        "all files should be candidates with zero min_idle"
    );
}

#[test]
fn test_eviction_candidates_include_residency_info() {
    let mut project = Project::new();
    project.set_file("/a.ts".to_string(), "const a = 1;".to_string());
    let candidates = project.eviction_candidates(None);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].file_name, "/a.ts");
    assert!(
        candidates[0].estimated_bytes > 0,
        "estimated_bytes should be positive"
    );
}

#[test]
fn test_touch_file_updates_last_accessed() {
    let mut project = Project::new();
    project.set_file("/a.ts".to_string(), "const a = 1;".to_string());

    let before = project.files["/a.ts"].last_accessed();
    // Small sleep to ensure timestamp difference
    std::thread::sleep(std::time::Duration::from_millis(5));
    project.touch_file("/a.ts");
    let after = project.files["/a.ts"].last_accessed();

    assert!(
        after > before,
        "touch should update last_accessed timestamp"
    );
}

#[test]
fn test_eviction_candidates_deprioritizes_dts_files() {
    let mut project = Project::new();
    // Create a .d.ts file and a .ts file of similar size
    project.set_file(
        "/types.d.ts".to_string(),
        "declare const x: number;".to_string(),
    );
    project.set_file(
        "/app.ts".to_string(),
        "declare const y: string;".to_string(),
    );

    let candidates = project.eviction_candidates(None);
    assert_eq!(candidates.len(), 2);

    // The .ts file should rank higher (better eviction candidate) than .d.ts
    // because .d.ts files are deprioritized with a 4x penalty
    let ts_idx = candidates
        .iter()
        .position(|c| c.file_name == "/app.ts")
        .unwrap();
    let dts_idx = candidates
        .iter()
        .position(|c| c.file_name == "/types.d.ts")
        .unwrap();
    assert!(
        ts_idx < dts_idx,
        "regular .ts file should rank as better eviction candidate than .d.ts"
    );
}

// =============================================================================
// Binder-based dependency graph wiring
// =============================================================================

#[test]
fn test_set_file_populates_dependency_graph_from_binder() {
    // Verifies that `set_file` uses binder's `file_import_sources` to populate
    // the dependency graph automatically, without a separate AST walk.
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "export const x = 1;".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import { x } from \"./a\";\nexport const y = x + 1;".to_string(),
    );

    // The dependency graph should automatically have b.ts -> "./a"
    let b_deps = project.dependency_graph.get_dependencies("b.ts");
    assert!(
        b_deps.is_some(),
        "b.ts should have dependencies in the graph"
    );
    assert!(
        b_deps.unwrap().contains("./a"),
        "b.ts should depend on './a', got: {b_deps:?}",
    );

    // Reverse: "./a" should have b.ts as a dependent
    let a_dependents = project.dependency_graph.get_dependents("./a");
    assert!(
        a_dependents.is_some(),
        "'./a' should have dependents in the graph"
    );
    assert!(
        a_dependents.unwrap().contains("b.ts"),
        "'./a' dependents should include 'b.ts', got: {a_dependents:?}",
    );
}

#[test]
fn test_dependency_graph_tracks_reexports() {
    // Verifies that `export ... from` specifiers are captured.
    let mut project = Project::new();

    project.set_file(
        "barrel.ts".to_string(),
        "export { foo } from \"./impl\";\nexport * from \"./types\";".to_string(),
    );

    let deps = project.dependency_graph.get_dependencies("barrel.ts");
    assert!(deps.is_some(), "barrel.ts should have dependencies");
    let deps = deps.unwrap();
    assert!(
        deps.contains("./impl"),
        "barrel.ts should depend on './impl', got: {deps:?}",
    );
    assert!(
        deps.contains("./types"),
        "barrel.ts should depend on './types', got: {deps:?}",
    );
}

#[test]
fn test_dependency_graph_updates_on_file_change() {
    // Verifies that re-setting a file updates the dependency graph edges.
    let mut project = Project::new();

    project.set_file(
        "c.ts".to_string(),
        "import { a } from \"./old-dep\";".to_string(),
    );

    // Initial state: c.ts depends on ./old-dep
    let deps = project.dependency_graph.get_dependencies("c.ts").unwrap();
    assert!(deps.contains("./old-dep"));

    // Change c.ts to import from a different module
    project.set_file(
        "c.ts".to_string(),
        "import { b } from \"./new-dep\";".to_string(),
    );

    // After update: c.ts should depend on ./new-dep, not ./old-dep
    let deps = project.dependency_graph.get_dependencies("c.ts").unwrap();
    assert!(
        deps.contains("./new-dep"),
        "c.ts should now depend on './new-dep', got: {deps:?}",
    );
    assert!(
        !deps.contains("./old-dep"),
        "c.ts should no longer depend on './old-dep', got: {deps:?}",
    );
}

#[test]
fn test_dependency_graph_side_effect_imports() {
    // Side-effect imports (import "module") should also be tracked.
    let mut project = Project::new();

    project.set_file(
        "app.ts".to_string(),
        "import \"./polyfill\";\nimport { foo } from \"./lib\";".to_string(),
    );

    let deps = project.dependency_graph.get_dependencies("app.ts").unwrap();
    assert!(
        deps.contains("./polyfill"),
        "side-effect import should be in dependency graph, got: {deps:?}",
    );
    assert!(
        deps.contains("./lib"),
        "named import should be in dependency graph, got: {deps:?}",
    );
}
