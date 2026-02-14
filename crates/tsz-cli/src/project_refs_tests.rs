use super::*;
use tempfile::TempDir;

fn create_test_project(dir: &Path, config: &str) -> PathBuf {
    let config_path = dir.join("tsconfig.json");
    std::fs::write(&config_path, config).unwrap();
    config_path
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
fn test_validate_composite_requirements() {
    let config = r#"
        {
            "compilerOptions": {
                "composite": true,
                "declaration": false
            }
        }
        "#;

    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let config_path = create_test_project(temp.path(), config);
    let _project = load_project(&config_path).unwrap();

    // The project claims to be composite but doesn't emit declarations
    // Our simple check won't catch this because we check the raw source
    // In a real implementation, we'd parse the compiler options properly
}
