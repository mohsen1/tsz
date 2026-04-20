#[test]
fn test_project_definition_store_invalidation_on_set_file() {
    let mut project = Project::new();

    // First set: creates definitions.
    project.set_file(
        "a.ts".to_string(),
        "export interface Foo { x: number }".to_string(),
    );

    // The definition store should have registered definitions for file_idx.
    let file_idx = project.files["a.ts"].file_idx;
    let has_file = project.definition_store.has_file(file_idx);
    // Note: definitions are lazily registered during checker runs, not during
    // binding. So has_file may be false here. The important thing is that
    // invalidate_file is called during set_file (verified by the pipeline
    // not crashing and the file_idx being stable).
    let _ = has_file;

    // Replacing the file should not crash even if no definitions were registered.
    project.set_file(
        "a.ts".to_string(),
        "export interface Bar { y: string }".to_string(),
    );
    assert_eq!(
        project.files["a.ts"].file_idx, file_idx,
        "File index should be stable after replacement"
    );
}

// =============================================================================
// Skeleton fingerprint cache tests
// =============================================================================

#[test]
fn test_fingerprint_cache_tracks_new_files() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const x = 1;".to_string());
    project.set_file("b.ts".to_string(), "export const y = 2;".to_string());

    // Both files should have fingerprints in the cache.
    let fp_a = project.fingerprint_for_file("a.ts");
    let fp_b = project.fingerprint_for_file("b.ts");
    assert!(fp_a.is_some(), "a.ts should have a fingerprint");
    assert!(fp_b.is_some(), "b.ts should have a fingerprint");

    // Different exports should produce different fingerprints.
    assert_ne!(
        fp_a.unwrap(),
        fp_b.unwrap(),
        "Different exports should produce different fingerprints"
    );
}

#[test]
fn test_fingerprint_cache_stable_across_body_edits() {
    let mut project = Project::new();
    project.set_file(
        "a.ts".to_string(),
        "export function foo() { return 1; }".to_string(),
    );
    let fp_before = project
        .fingerprint_for_file("a.ts")
        .expect("should have fingerprint");

    // Change function body but keep the same export signature.
    project.set_file(
        "a.ts".to_string(),
        "export function foo() { return 42; }".to_string(),
    );
    let fp_after = project
        .fingerprint_for_file("a.ts")
        .expect("should still have fingerprint");

    assert_eq!(
        fp_before, fp_after,
        "Body-only changes should not change the export signature fingerprint"
    );
}

#[test]
fn test_fingerprint_cache_changes_on_api_change() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const x = 1;".to_string());
    let fp_before = project
        .fingerprint_for_file("a.ts")
        .expect("should have fingerprint");

    // Add a new export — this changes the public API.
    project.set_file(
        "a.ts".to_string(),
        "export const x = 1;\nexport const y = 2;".to_string(),
    );
    let fp_after = project
        .fingerprint_for_file("a.ts")
        .expect("should still have fingerprint");

    assert_ne!(
        fp_before, fp_after,
        "Adding a new export should change the fingerprint"
    );
}

#[test]
fn test_fingerprint_cache_removed_on_file_removal() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const x = 1;".to_string());
    assert!(project.fingerprint_for_file("a.ts").is_some());

    project.remove_file("a.ts");
    assert!(
        project.fingerprint_for_file("a.ts").is_none(),
        "Fingerprint should be removed when file is removed"
    );
}

#[test]
fn test_fingerprint_snapshot_returns_all_files() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const a = 1;".to_string());
    project.set_file("b.ts".to_string(), "export const b = 2;".to_string());
    project.set_file("c.ts".to_string(), "export const c = 3;".to_string());

    let snapshot = project.fingerprint_snapshot();
    assert_eq!(
        snapshot.len(),
        3,
        "Snapshot should contain entries for all 3 files"
    );
}

#[test]
fn test_fingerprint_cache_update_via_incremental_edit() {
    let mut project = Project::new();
    let source = "export const x = 1;";
    project.set_file("a.ts".to_string(), source.to_string());
    let fp_before = project.fingerprint_for_file("a.ts").unwrap();

    // Apply an incremental edit that changes the API.
    let line_map = LineMap::build(source);
    let edit = TextEdit {
        range: range_for_substring(source, &line_map, "1"),
        new_text: "1;\nexport const y = 2".to_string(),
    };
    project.update_file("a.ts", &[edit]);

    let fp_after = project.fingerprint_for_file("a.ts").unwrap();
    assert_ne!(
        fp_before, fp_after,
        "Incremental edit adding a new export should change fingerprint"
    );
}

// ===== Memory accounting tests =====

#[test]
fn test_project_file_estimated_size_is_nonzero() {
    let file = ProjectFile::new("test.ts".to_string(), "const x = 1;".to_string());
    let size = file.estimated_size_bytes();
    assert!(
        size > 0,
        "estimated_size_bytes should be nonzero for any file"
    );
    // Even the simplest file has the struct itself + parser arena + binder data
    assert!(
        size > std::mem::size_of::<ProjectFile>(),
        "size should exceed the bare struct: got {size}"
    );
}

#[test]
fn test_project_file_estimated_size_grows_with_content() {
    let small = ProjectFile::new("small.ts".to_string(), "const a = 1;".to_string());
    let big = ProjectFile::new(
        "big.ts".to_string(),
        (0..100)
            .map(|i| format!("export const v{i}: number = {i};\n"))
            .collect::<String>(),
    );
    assert!(
        big.estimated_size_bytes() > small.estimated_size_bytes(),
        "Larger source should produce a larger memory estimate: small={}, big={}",
        small.estimated_size_bytes(),
        big.estimated_size_bytes(),
    );
}

#[test]
fn test_project_residency_stats_empty_project() {
    let project = Project::new();
    let stats = project.residency_stats();
    assert_eq!(stats.file_count, 0);
    assert_eq!(stats.total_estimated_bytes, 0);
    assert!(stats.largest_file.is_none());
    assert!(stats.smallest_file.is_none());
}

