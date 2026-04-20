#[test]
fn skeleton_estimated_size_is_nonzero_for_nonempty_file() {
    let files = vec![(
        "sized.ts".to_string(),
        "export const x = 1; export const y = 2;".to_string(),
    )];
    let results = parse_and_bind_parallel(files);
    let skeleton = extract_skeleton(&results[0]);

    assert!(
        skeleton.estimated_size_bytes() > 0,
        "skeleton size should be nonzero for a file with exports"
    );
}

#[test]
fn reduce_skeletons_produces_nonzero_index_for_multiple_files() {
    let files = vec![
        ("a.ts".to_string(), "export const x = 1;".to_string()),
        ("b.ts".to_string(), "export const y = 2;".to_string()),
    ];

    let results = parse_and_bind_parallel(files);
    let skeletons: Vec<_> = results.iter().map(extract_skeleton).collect();
    let index = reduce_skeletons(&skeletons);

    assert_eq!(index.file_count, 2, "should track both files");
    assert!(
        index.total_symbol_count >= 2,
        "should track symbols from both files, got {}",
        index.total_symbol_count
    );
    assert!(
        index.estimated_size_bytes() > 0,
        "index size should be nonzero"
    );
}

#[test]
fn bind_result_estimated_size_bytes_is_positive() {
    let files = vec![(
        "accounting.ts".to_string(),
        "let x = 1; function f() { return x + 1; }".to_string(),
    )];
    let results = parse_and_bind_parallel(files);

    let size = results[0].estimated_size_bytes();
    assert!(
        size > 0,
        "BindResult estimated_size_bytes should be positive for non-empty file"
    );
}

// =============================================================================
// End-to-end skeleton pipeline: parse → bind → extract → reduce → diff
// =============================================================================

#[test]
fn skeleton_pipeline_end_to_end_no_change() {
    let files = vec![
        (
            "api.ts".to_string(),
            "export function hello() {}".to_string(),
        ),
        (
            "utils.ts".to_string(),
            "export const PI = 3.14;".to_string(),
        ),
    ];

    // First build
    let results1 = parse_and_bind_parallel(files.clone());
    let skels1: Vec<_> = results1.iter().map(extract_skeleton).collect();
    let idx1 = reduce_skeletons(&skels1);

    // Second build (identical source)
    let results2 = parse_and_bind_parallel(files);
    let skels2: Vec<_> = results2.iter().map(extract_skeleton).collect();
    let idx2 = reduce_skeletons(&skels2);

    // Fingerprints must match
    assert_eq!(
        idx1.fingerprint, idx2.fingerprint,
        "identical source → identical index fingerprint"
    );

    // Diff should be empty
    let diff = diff_skeletons(&skels1, &skels2);
    assert!(diff.is_empty(), "no changes expected, got {diff:?}");
    assert!(!diff.topology_changed);
}

#[test]
fn skeleton_pipeline_detects_added_export() {
    let files_v1 = vec![("mod.ts".to_string(), "export const x = 1;".to_string())];
    let files_v2 = vec![(
        "mod.ts".to_string(),
        "export const x = 1; export const y = 2;".to_string(),
    )];

    let results1 = parse_and_bind_parallel(files_v1);
    let skels1: Vec<_> = results1.iter().map(extract_skeleton).collect();

    let results2 = parse_and_bind_parallel(files_v2);
    let skels2: Vec<_> = results2.iter().map(extract_skeleton).collect();

    // Adding an export should change the fingerprint
    assert_ne!(
        skels1[0].fingerprint, skels2[0].fingerprint,
        "adding export should change file fingerprint"
    );

    let diff = diff_skeletons(&skels1, &skels2);
    assert_eq!(diff.changed, vec!["mod.ts"]);
    assert!(diff.topology_changed);
}

#[test]
fn skeleton_pipeline_body_change_no_topology_change() {
    let files_v1 = vec![(
        "fn.ts".to_string(),
        "export function f() { return 1; }".to_string(),
    )];
    let files_v2 = vec![(
        "fn.ts".to_string(),
        "export function f() { return 999; }".to_string(),
    )];

    let results1 = parse_and_bind_parallel(files_v1);
    let skels1: Vec<_> = results1.iter().map(extract_skeleton).collect();

    let results2 = parse_and_bind_parallel(files_v2);
    let skels2: Vec<_> = results2.iter().map(extract_skeleton).collect();

    // Body-only change should NOT change skeleton fingerprint
    // (skeleton captures merge-relevant topology, not function bodies)
    assert_eq!(
        skels1[0].fingerprint, skels2[0].fingerprint,
        "body-only change should not affect skeleton fingerprint"
    );

    let diff = diff_skeletons(&skels1, &skels2);
    assert!(diff.is_empty(), "body change should not show in diff");
}

#[test]
fn residency_budget_integration_with_merged_program() {
    let files = vec![
        ("a.ts".to_string(), "export const a = 1;".to_string()),
        ("b.ts".to_string(), "export const b = 2;".to_string()),
    ];

    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);
    let stats = program.residency_stats();

    // Budget with generous thresholds → low pressure for 2 tiny files
    let budget = ResidencyBudget {
        low_watermark_bytes: 1024 * 1024,      // 1MB
        high_watermark_bytes: 2 * 1024 * 1024, // 2MB
    };
    assert_eq!(
        budget.assess(&stats),
        MemoryPressure::Low,
        "2 tiny files should be low pressure"
    );

    // Skeleton should be present and offer eviction savings
    assert!(stats.has_skeleton_index);
    if stats.pre_merge_bind_total_bytes > 0 {
        assert!(
            ResidencyBudget::eviction_savings(&stats) > 0,
            "should offer eviction savings when skeleton present"
        );
    }
}

// =============================================================================
// Stable semantic identity through merge/rebind pipeline
// =============================================================================

#[test]
fn semantic_defs_all_five_families_survive_multifile_merge() {
    // All five top-level declaration families (class, interface, type alias,
    // enum, namespace) must appear in MergedProgram.semantic_defs after merge.
    let files = vec![
        (
            "a.ts".to_string(),
            "class MyClass {} interface MyIface { x: number }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "type MyAlias = string; enum MyEnum { A, B }".to_string(),
        ),
        (
            "c.ts".to_string(),
            "namespace MyNS { export type T = number; }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let names: std::collections::HashSet<_> = program
        .semantic_defs
        .values()
        .map(|e| e.name.as_str())
        .collect();

    assert!(
        names.contains("MyClass"),
        "class should be in semantic_defs"
    );
    assert!(
        names.contains("MyIface"),
        "interface should be in semantic_defs"
    );
    assert!(
        names.contains("MyAlias"),
        "type alias should be in semantic_defs"
    );
    assert!(names.contains("MyEnum"), "enum should be in semantic_defs");
    assert!(
        names.contains("MyNS"),
        "namespace should be in semantic_defs"
    );
}

#[test]
fn semantic_defs_declaration_merging_across_files_preserves_first_identity() {
    // When two script files declare the same interface (non-module, so they
    // get merged cross-file), the merged semantic_defs should have exactly one
    // entry and it should come from the first file.
    let files = vec![
        (
            "a.ts".to_string(),
            "interface Shared { x: number; }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "interface Shared { y: string; }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let shared_entries: Vec<_> = program
        .semantic_defs
        .values()
        .filter(|e| e.name == "Shared")
        .collect();
    assert_eq!(
        shared_entries.len(),
        1,
        "declaration merging should produce exactly one semantic_def entry"
    );
    assert_eq!(
        shared_entries[0].file_id, 0,
        "first file's identity should be kept"
    );
}

#[test]
fn semantic_defs_survive_per_file_binder_reconstruction() {
    // After merge, all semantic_defs should be available — either in the per-file
    // binder (when DefinitionStore is not fully populated) or in the shared
    // DefinitionStore (when fully populated, which is the parallel path).
    let files = vec![
        (
            "a.ts".to_string(),
            "class Alpha {} type AType = number;".to_string(),
        ),
        (
            "b.ts".to_string(),
            "interface Beta {} enum BEnum { X }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let global_names: std::collections::HashSet<String> = program
        .semantic_defs
        .values()
        .map(|e| e.name.clone())
        .collect();

    // When the store is fully populated (parallel path), semantic_defs are
    // intentionally skipped in the per-file binder for performance. Verify the
    // global merged map has all expected entries instead.
    if program.definition_store.is_fully_populated() {
        // All global names should be resolvable in the DefinitionStore
        for name in &global_names {
            let sym_id = program
                .semantic_defs
                .iter()
                .find(|(_, e)| e.name == *name)
                .map(|(&id, _)| id)
                .unwrap_or_else(|| panic!("global semantic_defs should contain '{name}'"));
            assert!(
                program
                    .definition_store
                    .find_def_by_symbol(sym_id.0)
                    .is_some(),
                "DefinitionStore should have DefId for '{name}'"
            );
        }
    } else {
        // Legacy path: per-file binder should have all global semantic_defs
        for file_idx in 0..program.files.len() {
            let binder =
                create_binder_from_bound_file(&program.files[file_idx], &program, file_idx);
            let binder_names: std::collections::HashSet<String> = binder
                .semantic_defs
                .values()
                .map(|e| e.name.clone())
                .collect();
            for name in &global_names {
                assert!(
                    binder_names.contains(name),
                    "per-file binder for file {file_idx} missing semantic_def for '{name}'"
                );
            }
        }
    }
}

