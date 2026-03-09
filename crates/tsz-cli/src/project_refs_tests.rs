use super::*;
use tempfile::TempDir;

fn create_test_project(dir: &Path, config: &str) -> PathBuf {
    let config_path = dir.join("tsconfig.json");
    std::fs::write(&config_path, config).unwrap();
    config_path
}

fn project_name(graph: &ProjectReferenceGraph, id: ProjectId) -> String {
    graph
        .get_project(id)
        .unwrap()
        .config_path
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string()
}

fn component_names(graph: &ProjectReferenceGraph, component: &[ProjectId]) -> Vec<String> {
    component
        .iter()
        .map(|&project_id| project_name(graph, project_id))
        .collect()
}

#[test]
fn test_parse_project_reference() {
    let json = r#"{ "path": "./packages/core" }"#;
    let reference: ProjectReference =
        serde_json::from_str(json).expect("JSON parsing should succeed in test");
    assert_eq!(reference.path, "./packages/core");
    assert!(!reference.prepend);
}

#[test]
fn test_parse_project_reference_with_prepend() {
    let json = r#"{ "path": "./packages/core", "prepend": true }"#;
    let reference: ProjectReference =
        serde_json::from_str(json).expect("JSON parsing should succeed in test");
    assert_eq!(reference.path, "./packages/core");
    assert!(reference.prepend);
}

#[test]
fn test_parse_tsconfig_with_references() {
    let config = r#"
        {
            "compilerOptions": {
                "target": "ES2020",
                "composite": true,
                "declaration": true
            },
            "references": [
                { "path": "./packages/core" },
                { "path": "./packages/utils", "prepend": true }
            ]
        }
        "#;

    let parsed = parse_tsconfig_with_references(config).unwrap();
    assert!(parsed.references.is_some());
    let refs = parsed.references.unwrap();
    assert_eq!(refs.len(), 2);
    assert_eq!(refs[0].path, "./packages/core");
    assert_eq!(refs[1].path, "./packages/utils");
    assert!(refs[1].prepend);
}

#[test]
fn test_empty_references() {
    let config = r#"
        {
            "compilerOptions": {
                "target": "ES2020"
            }
        }
        "#;

    let parsed = parse_tsconfig_with_references(config).unwrap();
    assert!(parsed.references.is_none());
}

#[test]
fn test_build_order_simple() {
    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let root = temp.path();

    // Create project A (no dependencies)
    let proj_a = root.join("project-a");
    std::fs::create_dir_all(&proj_a).expect("directory creation should succeed in test");
    create_test_project(
        &proj_a,
        r#"{
            "compilerOptions": { "composite": true, "declaration": true }
        }"#,
    );

    // Create project B (depends on A)
    let proj_b = root.join("project-b");
    std::fs::create_dir_all(&proj_b).expect("directory creation should succeed in test");
    create_test_project(
        &proj_b,
        r#"{
            "compilerOptions": { "composite": true, "declaration": true },
            "references": [{ "path": "../project-a" }]
        }"#,
    );

    // Create root project (depends on B)
    let root_config = create_test_project(
        root,
        r#"{
            "references": [{ "path": "./project-b" }]
        }"#,
    );

    let graph = ProjectReferenceGraph::load(&root_config).unwrap();
    assert_eq!(graph.project_count(), 3);

    let order = graph.build_order().unwrap();
    assert_eq!(order.len(), 3);

    // A should come before B, B should come before root
    let a_idx = order.iter().position(|&id| {
        graph
            .get_project(id)
            .unwrap()
            .config_path
            .parent()
            .unwrap()
            .ends_with("project-a")
    });
    let b_idx = order.iter().position(|&id| {
        graph
            .get_project(id)
            .unwrap()
            .config_path
            .parent()
            .unwrap()
            .ends_with("project-b")
    });

    if let (Some(a), Some(b)) = (a_idx, b_idx) {
        assert!(a < b, "project-a should be built before project-b");
    }
}

#[test]
fn test_detect_cycles() {
    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let root = temp.path();

    // Create project A (depends on B)
    let proj_a = root.join("project-a");
    std::fs::create_dir_all(&proj_a).expect("directory creation should succeed in test");
    create_test_project(
        &proj_a,
        r#"{
            "compilerOptions": { "composite": true, "declaration": true },
            "references": [{ "path": "../project-b" }]
        }"#,
    );

    // Create project B (depends on A - cycle!)
    let proj_b = root.join("project-b");
    std::fs::create_dir_all(&proj_b).expect("directory creation should succeed in test");
    create_test_project(
        &proj_b,
        r#"{
            "compilerOptions": { "composite": true, "declaration": true },
            "references": [{ "path": "../project-a" }]
        }"#,
    );

    let config_a = proj_a.join("tsconfig.json");
    let graph = ProjectReferenceGraph::load(&config_a).unwrap();

    let cycles = graph.detect_cycles();
    assert!(!cycles.is_empty(), "Should detect circular reference");

    // build_order should fail
    assert!(graph.build_order().is_err());
}

#[test]
fn test_transitive_dependencies() {
    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let root = temp.path();

    // A -> B -> C
    let proj_c = root.join("project-c");
    std::fs::create_dir_all(&proj_c).expect("directory creation should succeed in test");
    create_test_project(
        &proj_c,
        r#"{
            "compilerOptions": { "composite": true, "declaration": true }
        }"#,
    );

    let proj_b = root.join("project-b");
    std::fs::create_dir_all(&proj_b).expect("directory creation should succeed in test");
    create_test_project(
        &proj_b,
        r#"{
            "compilerOptions": { "composite": true, "declaration": true },
            "references": [{ "path": "../project-c" }]
        }"#,
    );

    let proj_a = root.join("project-a");
    std::fs::create_dir_all(&proj_a).expect("directory creation should succeed in test");
    create_test_project(
        &proj_a,
        r#"{
            "compilerOptions": { "composite": true, "declaration": true },
            "references": [{ "path": "../project-b" }]
        }"#,
    );

    let config_a = proj_a.join("tsconfig.json");
    let graph = ProjectReferenceGraph::load(&config_a).unwrap();

    let a_id = graph
        .get_project_id(&std::fs::canonicalize(&config_a).unwrap())
        .unwrap();
    let deps = graph.transitive_dependencies(a_id);

    // A should transitively depend on both B and C
    assert_eq!(deps.len(), 2);
}

#[test]
fn test_validate_composite_from_compiler_options() {
    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let config_path = create_test_project(
        temp.path(),
        r#"{
            "compilerOptions": { "composite": true, "declaration": true }
        }"#,
    );
    let project = load_project(&config_path).unwrap();
    assert!(
        project.is_composite,
        "Project should be detected as composite"
    );
}

#[test]
fn test_non_composite_project() {
    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let config_path = create_test_project(
        temp.path(),
        r#"{
            "compilerOptions": { "target": "ES2020" }
        }"#,
    );
    let project = load_project(&config_path).unwrap();
    assert!(
        !project.is_composite,
        "Project without composite should not be composite"
    );
}

#[test]
fn test_no_emit_project() {
    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let config_path = create_test_project(
        temp.path(),
        r#"{
            "compilerOptions": { "noEmit": true }
        }"#,
    );
    let project = load_project(&config_path).unwrap();
    assert!(
        project.no_emit,
        "Project with noEmit should have no_emit=true"
    );
}

#[test]
fn test_validate_ts6306_referenced_project_not_composite() {
    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let root = temp.path();

    // Create a non-composite project (no composite flag)
    let proj_lib = root.join("lib");
    std::fs::create_dir_all(&proj_lib).expect("directory creation should succeed in test");
    create_test_project(
        &proj_lib,
        r#"{
            "compilerOptions": { "target": "ES2020" }
        }"#,
    );

    // Create root that references the non-composite project
    let root_config = create_test_project(
        root,
        r#"{
            "compilerOptions": { "composite": true, "declaration": true },
            "references": [{ "path": "./lib" }]
        }"#,
    );

    let graph = ProjectReferenceGraph::load(&root_config).unwrap();
    let diagnostics = graph.validate();

    assert!(
        diagnostics.iter().any(|d| d.code == 6306),
        "Should emit TS6306 for non-composite referenced project, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn test_validate_ts6310_referenced_project_no_emit() {
    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let root = temp.path();

    // Create a composite project that disables emit
    let proj_lib = root.join("lib");
    std::fs::create_dir_all(&proj_lib).expect("directory creation should succeed in test");
    create_test_project(
        &proj_lib,
        r#"{
            "compilerOptions": { "composite": true, "declaration": true, "noEmit": true }
        }"#,
    );

    // Create root that references it
    let root_config = create_test_project(
        root,
        r#"{
            "compilerOptions": { "composite": true, "declaration": true },
            "references": [{ "path": "./lib" }]
        }"#,
    );

    let graph = ProjectReferenceGraph::load(&root_config).unwrap();
    let diagnostics = graph.validate();

    assert!(
        diagnostics.iter().any(|d| d.code == 6310),
        "Should emit TS6310 for referenced project with noEmit, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn test_validate_no_errors_for_valid_references() {
    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let root = temp.path();

    // Create a valid composite project
    let proj_lib = root.join("lib");
    std::fs::create_dir_all(&proj_lib).expect("directory creation should succeed in test");
    create_test_project(
        &proj_lib,
        r#"{
            "compilerOptions": { "composite": true, "declaration": true }
        }"#,
    );

    // Create root that references it
    let root_config = create_test_project(
        root,
        r#"{
            "references": [{ "path": "./lib" }]
        }"#,
    );

    let graph = ProjectReferenceGraph::load(&root_config).unwrap();
    let diagnostics = graph.validate();

    assert!(
        diagnostics.is_empty(),
        "Should have no validation errors for valid references, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn test_validate_circular_reference_ts6202() {
    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let root = temp.path();

    let proj_a = root.join("project-a");
    std::fs::create_dir_all(&proj_a).expect("directory creation should succeed in test");
    create_test_project(
        &proj_a,
        r#"{
            "compilerOptions": { "composite": true, "declaration": true },
            "references": [{ "path": "../project-b" }]
        }"#,
    );

    let proj_b = root.join("project-b");
    std::fs::create_dir_all(&proj_b).expect("directory creation should succeed in test");
    create_test_project(
        &proj_b,
        r#"{
            "compilerOptions": { "composite": true, "declaration": true },
            "references": [{ "path": "../project-a" }]
        }"#,
    );

    let config_a = proj_a.join("tsconfig.json");
    let graph = ProjectReferenceGraph::load(&config_a).unwrap();
    let diagnostics = graph.validate();

    assert!(
        diagnostics.iter().any(|d| d.code == 6202),
        "Should emit TS6202 for circular reference, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn test_build_order_keeps_sibling_dependencies_deterministic() {
    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let root = temp.path();

    let project_a = root.join("project-a");
    std::fs::create_dir_all(&project_a).expect("directory creation should succeed in test");
    create_test_project(
        &project_a,
        r#"{
            "compilerOptions": { "composite": true, "declaration": true }
        }"#,
    );

    let project_b = root.join("project-b");
    std::fs::create_dir_all(&project_b).expect("directory creation should succeed in test");
    create_test_project(
        &project_b,
        r#"{
            "compilerOptions": { "composite": true, "declaration": true },
            "references": [{ "path": "../project-a" }]
        }"#,
    );

    let project_c = root.join("project-c");
    std::fs::create_dir_all(&project_c).expect("directory creation should succeed in test");
    create_test_project(
        &project_c,
        r#"{
            "compilerOptions": { "composite": true, "declaration": true },
            "references": [{ "path": "../project-a" }]
        }"#,
    );

    let root_config = create_test_project(
        root,
        r#"{
            "references": [
                { "path": "./project-c" },
                { "path": "./project-b" }
            ]
        }"#,
    );

    let graph = ProjectReferenceGraph::load(&root_config).unwrap();
    let order = graph.build_order().unwrap();
    let ordered_names: Vec<String> = order
        .iter()
        .map(|&id| {
            graph
                .get_project(id)
                .unwrap()
                .config_path
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
        .collect();

    assert_eq!(
        ordered_names,
        vec![
            "project-a".to_string(),
            "project-b".to_string(),
            "project-c".to_string(),
            temp.path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string(),
        ],
        "build order should remain deterministic for sibling projects that share a dependency"
    );
}

#[test]
fn test_condensation_groups_cycle_members_deterministically() {
    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let root = temp.path();

    let project_a = root.join("project-a");
    std::fs::create_dir_all(&project_a).expect("directory creation should succeed in test");
    let config_a = create_test_project(
        &project_a,
        r#"{
            "compilerOptions": { "composite": true, "declaration": true },
            "references": [{ "path": "../project-b" }]
        }"#,
    );

    let project_b = root.join("project-b");
    std::fs::create_dir_all(&project_b).expect("directory creation should succeed in test");
    let config_b = create_test_project(
        &project_b,
        r#"{
            "compilerOptions": { "composite": true, "declaration": true },
            "references": [{ "path": "../project-a" }]
        }"#,
    );

    let project_c = root.join("project-c");
    std::fs::create_dir_all(&project_c).expect("directory creation should succeed in test");
    create_test_project(
        &project_c,
        r#"{
            "compilerOptions": { "composite": true, "declaration": true },
            "references": [{ "path": "../project-a" }]
        }"#,
    );

    let root_config = create_test_project(
        root,
        r#"{
            "references": [{ "path": "./project-c" }]
        }"#,
    );

    let graph = ProjectReferenceGraph::load(&root_config).unwrap();
    let condensation = graph.condensation_graph();

    let a_id = graph
        .get_project_id(&std::fs::canonicalize(&config_a).unwrap())
        .unwrap();
    let b_id = graph
        .get_project_id(&std::fs::canonicalize(&config_b).unwrap())
        .unwrap();
    let cycle_component_id = condensation.component_for_project(a_id).unwrap();
    assert_eq!(
        condensation.component_for_project(b_id),
        Some(cycle_component_id),
        "project-a and project-b should collapse into the same SCC"
    );

    assert_eq!(
        component_names(&graph, condensation.component_members(cycle_component_id)),
        vec!["project-a".to_string(), "project-b".to_string()],
        "cycle members should be stable and path-sorted inside the SCC"
    );

    let cycles = graph.detect_cycles();
    assert_eq!(cycles.len(), 1, "expected exactly one cycle in the graph");
    assert_eq!(
        component_names(&graph, &cycles[0]),
        vec!["project-a".to_string(), "project-b".to_string()],
        "cycle reporting should remain deterministic after SCC condensation"
    );
}

#[test]
fn test_affected_projects_uses_condensation_graph_for_cycles() {
    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let root = temp.path();

    let project_a = root.join("project-a");
    std::fs::create_dir_all(&project_a).expect("directory creation should succeed in test");
    let config_a = create_test_project(
        &project_a,
        r#"{
            "compilerOptions": { "composite": true, "declaration": true },
            "references": [{ "path": "../project-b" }]
        }"#,
    );

    let project_b = root.join("project-b");
    std::fs::create_dir_all(&project_b).expect("directory creation should succeed in test");
    create_test_project(
        &project_b,
        r#"{
            "compilerOptions": { "composite": true, "declaration": true },
            "references": [{ "path": "../project-a" }]
        }"#,
    );

    let project_c = root.join("project-c");
    std::fs::create_dir_all(&project_c).expect("directory creation should succeed in test");
    let config_c = create_test_project(
        &project_c,
        r#"{
            "compilerOptions": { "composite": true, "declaration": true },
            "references": [{ "path": "../project-a" }]
        }"#,
    );

    let root_config = create_test_project(
        root,
        r#"{
            "references": [{ "path": "./project-c" }]
        }"#,
    );

    let graph = ProjectReferenceGraph::load(&root_config).unwrap();
    let a_id = graph
        .get_project_id(&std::fs::canonicalize(&config_a).unwrap())
        .unwrap();
    let b_id = graph
        .get_project_id(&std::fs::canonicalize(project_b.join("tsconfig.json")).unwrap())
        .unwrap();
    let c_id = graph
        .get_project_id(&std::fs::canonicalize(&config_c).unwrap())
        .unwrap();
    let root_id = graph
        .get_project_id(&std::fs::canonicalize(&root_config).unwrap())
        .unwrap();

    let affected = graph.affected_projects(a_id);
    assert_eq!(
        affected.len(),
        3,
        "peer cycle members and dependents should be included"
    );
    assert!(
        affected.contains(&b_id),
        "changing project-a should affect project-b in the same SCC"
    );
    assert!(
        affected.contains(&c_id),
        "changing project-a should affect direct dependent project-c"
    );
    assert!(
        affected.contains(&root_id),
        "changing project-a should affect the root project through project-c"
    );
}

#[test]
fn test_transitive_dependencies_exclude_self_inside_cycle() {
    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let root = temp.path();

    let project_a = root.join("project-a");
    std::fs::create_dir_all(&project_a).expect("directory creation should succeed in test");
    let config_a = create_test_project(
        &project_a,
        r#"{
            "compilerOptions": { "composite": true, "declaration": true },
            "references": [{ "path": "../project-b" }]
        }"#,
    );

    let project_b = root.join("project-b");
    std::fs::create_dir_all(&project_b).expect("directory creation should succeed in test");
    let config_b = create_test_project(
        &project_b,
        r#"{
            "compilerOptions": { "composite": true, "declaration": true },
            "references": [{ "path": "../project-a" }]
        }"#,
    );

    let root_config = create_test_project(
        root,
        r#"{
            "references": [{ "path": "./project-a" }]
        }"#,
    );

    let graph = ProjectReferenceGraph::load(&root_config).unwrap();
    let a_id = graph
        .get_project_id(&std::fs::canonicalize(&config_a).unwrap())
        .unwrap();
    let b_id = graph
        .get_project_id(&std::fs::canonicalize(&config_b).unwrap())
        .unwrap();

    let dependencies = graph.transitive_dependencies(a_id);
    assert!(
        dependencies.contains(&b_id),
        "project-a should still report project-b as a dependency inside the SCC"
    );
    assert!(
        !dependencies.contains(&a_id),
        "project-a should not report itself as a transitive dependency"
    );
}
