#[test]
fn test_project_residency_stats_single_file() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "const x = 1;".to_string());

    let stats = project.residency_stats();
    assert_eq!(stats.file_count, 1);
    assert!(stats.total_estimated_bytes > 0);
    let (name, size) = stats.largest_file.as_ref().unwrap();
    assert_eq!(name, "a.ts");
    assert_eq!(*size, stats.total_estimated_bytes);
    // largest == smallest for a single file
    assert_eq!(stats.largest_file, stats.smallest_file);
}

#[test]
fn test_project_residency_stats_multi_file() {
    let mut project = Project::new();
    project.set_file("small.ts".to_string(), "const a = 1;".to_string());
    project.set_file(
        "big.ts".to_string(),
        (0..50)
            .map(|i| format!("export const v{i}: number = {i};\n"))
            .collect::<String>(),
    );

    let stats = project.residency_stats();
    assert_eq!(stats.file_count, 2);

    let (largest_name, largest_size) = stats.largest_file.as_ref().unwrap();
    let (smallest_name, smallest_size) = stats.smallest_file.as_ref().unwrap();
    assert_eq!(largest_name, "big.ts");
    assert_eq!(smallest_name, "small.ts");
    assert!(largest_size > smallest_size);
    assert_eq!(stats.total_estimated_bytes, largest_size + smallest_size);
}

#[test]
fn test_project_file_estimated_size_query() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "const x = 1;".to_string());

    let size = project.file_estimated_size("a.ts");
    assert!(size.is_some());
    assert!(size.unwrap() > 0);

    assert!(project.file_estimated_size("nonexistent.ts").is_none());
}

#[test]
fn test_project_residency_stats_after_remove() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "const a = 1;".to_string());
    project.set_file("b.ts".to_string(), "const b = 2;".to_string());

    let before = project.residency_stats();
    assert_eq!(before.file_count, 2);

    project.remove_file("a.ts");

    let after = project.residency_stats();
    assert_eq!(after.file_count, 1);
    assert!(after.total_estimated_bytes < before.total_estimated_bytes);
}

#[test]
fn test_project_residency_stats_includes_type_interner() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "const x: number = 1;".to_string());

    let stats = project.residency_stats();
    assert!(
        stats.type_interner_estimated_bytes > 0,
        "type_interner_estimated_bytes should be nonzero for any project with files"
    );
    // The interner size should be at least the struct overhead
    assert!(
        stats.type_interner_estimated_bytes >= std::mem::size_of::<tsz_solver::TypeInterner>(),
        "interner estimate ({}) should be >= struct size ({})",
        stats.type_interner_estimated_bytes,
        std::mem::size_of::<tsz_solver::TypeInterner>(),
    );
}

#[test]
fn test_project_residency_stats_type_interner_nonzero_even_empty_project() {
    let project = Project::new();
    let stats = project.residency_stats();
    // Even an empty project has a TypeInterner with intrinsics pre-registered
    assert!(
        stats.type_interner_estimated_bytes > 0,
        "type_interner_estimated_bytes should be nonzero even for empty project (intrinsics)"
    );
}

#[test]
fn test_project_residency_stats_includes_definition_store() {
    let mut project = Project::new();
    project.set_file(
        "a.ts".to_string(),
        "interface Foo { x: number; }\nclass Bar {}".to_string(),
    );

    let stats = project.residency_stats();
    assert!(
        stats.definition_store_estimated_bytes > 0,
        "definition_store_estimated_bytes should be nonzero for a project with definitions"
    );
}

#[test]
fn test_project_residency_stats_definition_store_nonzero_even_empty_project() {
    let project = Project::new();
    let stats = project.residency_stats();
    // Even an empty store has struct overhead (DashMaps, atomics, etc.)
    assert!(
        stats.definition_store_estimated_bytes > 0,
        "definition_store_estimated_bytes should be nonzero even for empty project"
    );
}

#[test]
fn test_eviction_candidates_empty_project() {
    let project = Project::new();
    let candidates = project.eviction_candidates(None);
    assert!(
        candidates.is_empty(),
        "empty project should have no eviction candidates"
    );
}

#[test]
fn test_eviction_candidates_returns_all_files_without_min_idle() {
    let mut project = Project::new();
    project.set_file("/a.ts".to_string(), "const a = 1;".to_string());
    project.set_file("/b.ts".to_string(), "const b = 2;".to_string());
    let candidates = project.eviction_candidates(None);
    assert_eq!(candidates.len(), 2, "should return all files");
}

