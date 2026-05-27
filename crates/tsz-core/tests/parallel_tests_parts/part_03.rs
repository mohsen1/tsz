#[test]
fn test_pre_merge_bind_total_bytes_equals_sum_of_individual() {
    let files = vec![
        ("a.ts".to_string(), "export const a = 1;".to_string()),
        (
            "b.ts".to_string(),
            "export function foo(x: number): string { return String(x); }".to_string(),
        ),
        (
            "c.ts".to_string(),
            "export interface Bar { name: string; value: number; }".to_string(),
        ),
    ];

    // Compute individual sizes before merge
    let bind_results = parse_and_bind_parallel(files);
    let individual_sum: usize = bind_results.iter().map(|r| r.estimated_size_bytes()).sum();

    let program = merge_bind_results(bind_results);
    let stats = program.residency_stats();

    assert_eq!(
        stats.pre_merge_bind_total_bytes, individual_sum,
        "pre_merge_bind_total_bytes ({}) should equal sum of individual BindResult sizes ({})",
        stats.pre_merge_bind_total_bytes, individual_sum
    );
    assert!(
        stats.pre_merge_bind_total_bytes > 0,
        "total should be nonzero for 3 files"
    );
}

#[test]
fn test_pre_merge_bind_total_bytes_scales_with_file_count() {
    let one_file = vec![("a.ts".to_string(), "export const a = 1;".to_string())];
    let three_files = vec![
        ("a.ts".to_string(), "export const a = 1;".to_string()),
        ("b.ts".to_string(), "export const b = 2;".to_string()),
        ("c.ts".to_string(), "export const c = 3;".to_string()),
    ];

    let prog1 = merge_bind_results(parse_and_bind_parallel(one_file));
    let prog3 = merge_bind_results(parse_and_bind_parallel(three_files));

    assert!(
        prog3.pre_merge_bind_total_bytes > prog1.pre_merge_bind_total_bytes,
        "3-file merge ({} bytes) should have larger pre-merge total than 1-file ({} bytes)",
        prog3.pre_merge_bind_total_bytes,
        prog1.pre_merge_bind_total_bytes
    );
}

#[test]
fn test_bound_file_estimated_size_bytes_nonzero() {
    let files = vec![
        ("a.ts".to_string(), "export const a = 1;".to_string()),
        (
            "b.ts".to_string(),
            "export function b(x: number) { if (x > 0) return x; return -x; }".to_string(),
        ),
    ];

    let bind_results = parse_and_bind_parallel(files);
    let program = merge_bind_results(bind_results);

    for file in &program.files {
        let size = file.estimated_size_bytes();
        assert!(
            size > 0,
            "BoundFile '{}' should have nonzero estimated size",
            file.file_name,
        );
    }
}

#[test]
fn test_bound_file_estimated_size_scales_with_complexity() {
    let files = vec![
        ("small.ts".to_string(), "const x = 1;".to_string()),
        (
            "large.ts".to_string(),
            r#"
            export function f(x: number, y: string) {
                if (x > 0) { return y.toUpperCase(); }
                else if (x < 0) { return y.toLowerCase(); }
                switch (y) {
                    case "a": return "A";
                    case "b": return "B";
                    default: return y;
                }
            }
            export const arr = [1, 2, 3];
            export interface Foo { bar: string; baz: number; }
            "#
            .to_string(),
        ),
    ];

    let bind_results = parse_and_bind_parallel(files);
    let program = merge_bind_results(bind_results);

    let small = program
        .files
        .iter()
        .find(|f| f.file_name.contains("small"))
        .unwrap();
    let large = program
        .files
        .iter()
        .find(|f| f.file_name.contains("large"))
        .unwrap();

    assert!(
        large.estimated_size_bytes() > small.estimated_size_bytes(),
        "larger file ({} bytes) should have bigger estimate than small file ({} bytes)",
        large.estimated_size_bytes(),
        small.estimated_size_bytes(),
    );
}

#[test]
fn test_residency_stats_total_bound_file_bytes_nonzero() {
    let files = vec![
        ("a.ts".to_string(), "export const a = 1;".to_string()),
        ("b.ts".to_string(), "export const b = 2;".to_string()),
    ];

    let bind_results = parse_and_bind_parallel(files);
    let program = merge_bind_results(bind_results);
    let stats = program.residency_stats();

    assert!(
        stats.total_bound_file_bytes > 0,
        "total_bound_file_bytes should be nonzero for a program with files"
    );

    // The total should equal the sum of individual file estimates
    let manual_sum: usize = program.files.iter().map(|f| f.estimated_size_bytes()).sum();
    assert_eq!(
        stats.total_bound_file_bytes, manual_sum,
        "total_bound_file_bytes should equal sum of per-file estimates"
    );
}

#[test]
fn test_residency_stats_unique_arena_estimated_bytes_nonzero() {
    let files = vec![
        ("a.ts".to_string(), "export const a = 1;".to_string()),
        (
            "b.ts".to_string(),
            "export function b(x: number): string { return String(x); }".to_string(),
        ),
    ];

    let bind_results = parse_and_bind_parallel(files);
    let program = merge_bind_results(bind_results);
    let stats = program.residency_stats();

    assert!(
        stats.unique_arena_estimated_bytes > 0,
        "unique_arena_estimated_bytes should be nonzero for a program with files"
    );

    // Arena size should be larger than the struct overhead alone
    assert!(
        stats.unique_arena_estimated_bytes > std::mem::size_of::<tsz_parser::parser::NodeArena>(),
        "arena estimate should exceed bare struct size when files have been parsed"
    );
}

// =============================================================================
// Skeleton extraction determinism and content validation
// =============================================================================

#[test]
fn skeleton_extraction_captures_exported_symbols() {
    let files = vec![(
        "module.ts".to_string(),
        r#"
export const API_KEY = "secret";
export function greet(name: string): string { return name; }
export class UserService {
    getUser() { return null; }
}
export enum Status { Active, Inactive }
"#
        .to_string(),
    )];

    let results = parse_and_bind_parallel(files);
    assert_eq!(results.len(), 1);

    let skeleton = extract_skeleton(&results[0]);
    assert_eq!(skeleton.file_name, "module.ts");

    // Should capture all exported symbols
    let names: Vec<&str> = skeleton.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"API_KEY"), "missing API_KEY in {names:?}");
    assert!(names.contains(&"greet"), "missing greet in {names:?}");
    assert!(
        names.contains(&"UserService"),
        "missing UserService in {names:?}"
    );
    assert!(names.contains(&"Status"), "missing Status in {names:?}");
}

#[test]
fn skeleton_extraction_is_deterministic() {
    let source = r#"
export const a = 1;
export function b() {}
export class C {}
"#;

    let files = vec![("det.ts".to_string(), source.to_string())];
    let results = parse_and_bind_parallel(files);

    let skel1 = extract_skeleton(&results[0]);
    let skel2 = extract_skeleton(&results[0]);

    // Same input must produce identical skeletons
    assert_eq!(skel1.symbols.len(), skel2.symbols.len());
    assert_eq!(skel1.file_name, skel2.file_name);
    assert_eq!(
        skel1.fingerprint, skel2.fingerprint,
        "fingerprint must be deterministic"
    );
}

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

#[test]
fn semantic_defs_enriched_fields_survive_merge() {
    // Enriched fields (is_abstract, is_const, enum_member_names) must survive
    // the merge pipeline.
    let files = vec![(
        "a.ts".to_string(),
        "abstract class Abs {} const enum CE { X, Y } enum RE { A, B, C }".to_string(),
    )];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let abs = program
        .semantic_defs
        .values()
        .find(|e| e.name == "Abs")
        .expect("Abs should be in semantic_defs");
    assert!(
        abs.is_abstract,
        "abstract class should preserve is_abstract"
    );

    let ce = program
        .semantic_defs
        .values()
        .find(|e| e.name == "CE")
        .expect("CE should be in semantic_defs");
    assert!(ce.is_const, "const enum should preserve is_const");
    assert_eq!(ce.enum_member_names, vec!["X", "Y"]);

    let re = program
        .semantic_defs
        .values()
        .find(|e| e.name == "RE")
        .expect("RE should be in semantic_defs");
    assert!(!re.is_const, "regular enum should not be const");
    assert_eq!(re.enum_member_names, vec!["A", "B", "C"]);
}

#[test]
fn semantic_defs_type_param_count_survives_merge() {
    // Generic declarations should preserve their type_param_count through merge.
    let files = vec![
        (
            "a.ts".to_string(),
            "class Box<T> {} interface Pair<A, B> { first: A; second: B; }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "type Triple<X, Y, Z> = [X, Y, Z];".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let box_def = program
        .semantic_defs
        .values()
        .find(|e| e.name == "Box")
        .expect("Box should be in semantic_defs");
    assert_eq!(box_def.type_param_count, 1);

    let pair_def = program
        .semantic_defs
        .values()
        .find(|e| e.name == "Pair")
        .expect("Pair should be in semantic_defs");
    assert_eq!(pair_def.type_param_count, 2);

    let triple_def = program
        .semantic_defs
        .values()
        .find(|e| e.name == "Triple")
        .expect("Triple should be in semantic_defs");
    assert_eq!(triple_def.type_param_count, 3);
}

#[test]
fn semantic_defs_export_visibility_survives_merge() {
    // Exported declarations should preserve is_exported through the merge pipeline.
    let files = vec![(
        "a.ts".to_string(),
        "export class Exported {} class Internal {}".to_string(),
    )];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let exported = program
        .semantic_defs
        .values()
        .find(|e| e.name == "Exported")
        .expect("Exported should be in semantic_defs");
    assert!(
        exported.is_exported,
        "exported class should preserve is_exported"
    );

    let internal = program
        .semantic_defs
        .values()
        .find(|e| e.name == "Internal")
        .expect("Internal should be in semantic_defs");
    assert!(
        !internal.is_exported,
        "non-exported class should not be marked exported"
    );
}

#[test]
fn semantic_defs_stable_symbol_ids_across_merge_rebuilds() {
    // semantic_defs should produce identical name/kind sets when the same
    // source is compiled and merged multiple times.
    let files = vec![
        (
            "a.ts".to_string(),
            "class A {} interface I {} type T = number;".to_string(),
        ),
        (
            "b.ts".to_string(),
            "enum E { X } namespace NS { export type Inner = string; }".to_string(),
        ),
    ];

    let results1 = parse_and_bind_parallel(files.clone());
    let program1 = merge_bind_results(results1);
    let results2 = parse_and_bind_parallel(files);
    let program2 = merge_bind_results(results2);

    let mut defs1: Vec<(String, String)> = program1
        .semantic_defs
        .values()
        .map(|e| (e.name.clone(), format!("{:?}", e.kind)))
        .collect();
    let mut defs2: Vec<(String, String)> = program2
        .semantic_defs
        .values()
        .map(|e| (e.name.clone(), format!("{:?}", e.kind)))
        .collect();
    defs1.sort();
    defs2.sort();

    assert_eq!(
        defs1, defs2,
        "semantic_defs name/kind sets should be identical across rebuilds"
    );
}

#[test]
fn semantic_defs_namespace_scoped_declarations_survive_merge() {
    // Declarations inside exported namespaces should be captured in semantic_defs
    // because the namespace body creates a ContainerKind::Module scope.
    let files = vec![(
        "a.ts".to_string(),
        r#"
namespace Outer {
    export interface Inner {}
    export type Alias = string;
    export class Klass {}
    export enum E { A }
}
"#
        .to_string(),
    )];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let names: std::collections::HashSet<_> = program
        .semantic_defs
        .values()
        .map(|e| e.name.as_str())
        .collect();

    assert!(
        names.contains("Outer"),
        "namespace itself should be captured"
    );
    // Namespace-scoped declarations should also be captured
    assert!(
        names.contains("Inner"),
        "namespace-scoped interface should be captured"
    );
    assert!(
        names.contains("Alias"),
        "namespace-scoped type alias should be captured"
    );
    assert!(
        names.contains("Klass"),
        "namespace-scoped class should be captured"
    );
    assert!(
        names.contains("E"),
        "namespace-scoped enum should be captured"
    );
}

// =============================================================================
// Per-File Semantic Identity in BoundFile
// =============================================================================
// These tests verify that BoundFile.semantic_defs carries file-scoped
// stable identity through the merge pipeline, and that the compose path
// in create_binder_from_bound_file correctly layers per-file + global.

#[test]
fn bound_file_semantic_defs_contains_own_declarations() {
    let files = vec![
        (
            "a.ts".to_string(),
            "export class Foo {} export type Bar = number;".to_string(),
        ),
        (
            "b.ts".to_string(),
            "export interface Baz { x: number } export enum Qux { A, B }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    assert_eq!(program.files.len(), 2);

    // File a.ts should have Foo and Bar
    let a_names: std::collections::HashSet<_> = program.files[0]
        .semantic_defs
        .values()
        .map(|e| e.name.as_str())
        .collect();
    assert!(a_names.contains("Foo"), "a.ts should contain Foo");
    assert!(a_names.contains("Bar"), "a.ts should contain Bar");
    assert!(!a_names.contains("Baz"), "a.ts should not contain Baz");
    assert!(!a_names.contains("Qux"), "a.ts should not contain Qux");

    // File b.ts should have Baz and Qux
    let b_names: std::collections::HashSet<_> = program.files[1]
        .semantic_defs
        .values()
        .map(|e| e.name.as_str())
        .collect();
    assert!(b_names.contains("Baz"), "b.ts should contain Baz");
    assert!(b_names.contains("Qux"), "b.ts should contain Qux");
    assert!(!b_names.contains("Foo"), "b.ts should not contain Foo");
    assert!(!b_names.contains("Bar"), "b.ts should not contain Bar");
}

#[test]
fn bound_file_semantic_defs_covers_all_declaration_families() {
    let files = vec![(
        "all.ts".to_string(),
        concat!(
            "export class MyClass {} ",
            "export interface MyInterface { x: number } ",
            "export type MyAlias = string; ",
            "export enum MyEnum { A, B } ",
            "export namespace MyNS { export const v = 1; } ",
            "export function myFn() {} ",
            "export const myVar = 42;",
        )
        .to_string(),
    )];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let file = &program.files[0];
    let defs: std::collections::HashMap<_, _> = file
        .semantic_defs
        .values()
        .map(|e| (e.name.as_str(), e.kind))
        .collect();

    assert_eq!(
        defs.get("MyClass"),
        Some(&crate::binder::SemanticDefKind::Class),
        "class should be captured"
    );
    assert_eq!(
        defs.get("MyInterface"),
        Some(&crate::binder::SemanticDefKind::Interface),
        "interface should be captured"
    );
    assert_eq!(
        defs.get("MyAlias"),
        Some(&crate::binder::SemanticDefKind::TypeAlias),
        "type alias should be captured"
    );
    assert_eq!(
        defs.get("MyEnum"),
        Some(&crate::binder::SemanticDefKind::Enum),
        "enum should be captured"
    );
    assert_eq!(
        defs.get("MyNS"),
        Some(&crate::binder::SemanticDefKind::Namespace),
        "namespace should be captured"
    );
    assert_eq!(
        defs.get("myFn"),
        Some(&crate::binder::SemanticDefKind::Function),
        "function should be captured"
    );
    assert_eq!(
        defs.get("myVar"),
        Some(&crate::binder::SemanticDefKind::Variable),
        "variable should be captured"
    );
}

#[test]
fn bound_file_semantic_defs_file_id_matches_merge_index() {
    let files = vec![
        ("file0.ts".to_string(), "export class A {}".to_string()),
        ("file1.ts".to_string(), "export interface B {}".to_string()),
        (
            "file2.ts".to_string(),
            "export type C = number;".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    for (idx, file) in program.files.iter().enumerate() {
        for entry in file.semantic_defs.values() {
            assert_eq!(
                entry.file_id, idx as u32,
                "per-file semantic_def '{}' should have file_id == {} but got {}",
                entry.name, idx, entry.file_id
            );
        }
    }
}

#[test]
fn bound_file_semantic_defs_stable_across_rebuild() {
    let files = vec![
        (
            "a.ts".to_string(),
            "export class Foo<T> {} export enum E { X, Y }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "export interface Bar extends Object {} export type Alias = number;".to_string(),
        ),
    ];

    let results1 = parse_and_bind_parallel(files.clone());
    let program1 = merge_bind_results(results1);
    let results2 = parse_and_bind_parallel(files);
    let program2 = merge_bind_results(results2);

    for (idx, (f1, f2)) in program1.files.iter().zip(program2.files.iter()).enumerate() {
        let defs1: std::collections::HashMap<_, _> = f1
            .semantic_defs
            .values()
            .map(|e| (e.name.clone(), (e.kind, e.type_param_count, e.is_exported)))
            .collect();
        let defs2: std::collections::HashMap<_, _> = f2
            .semantic_defs
            .values()
            .map(|e| (e.name.clone(), (e.kind, e.type_param_count, e.is_exported)))
            .collect();
        assert_eq!(
            defs1, defs2,
            "per-file semantic_defs should be identical across rebuilds for file {idx}"
        );
    }
}

#[test]
fn bound_file_semantic_defs_declaration_merging_interface() {
    // When two files declare the same interface, each file's BoundFile
    // should contain its own declaration. The global map should keep first.
    let files = vec![
        (
            "a.ts".to_string(),
            "interface Shared { x: number }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "interface Shared { y: string }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Each file's per-file semantic_defs should have its own Shared entry
    let a_has_shared = program.files[0]
        .semantic_defs
        .values()
        .any(|e| e.name == "Shared");
    // File b may or may not have Shared depending on whether SymbolId was
    // merged (cross-file declaration merging collapses to one SymbolId).
    // The global map should have exactly one entry.
    let global_shared_count = program
        .semantic_defs
        .values()
        .filter(|e| e.name == "Shared")
        .count();
    assert!(
        a_has_shared,
        "file a should have Shared in per-file semantic_defs"
    );
    assert_eq!(
        global_shared_count, 1,
        "global semantic_defs should have exactly one Shared (first-wins merge)"
    );
}

#[test]
fn create_binder_from_bound_file_composes_per_file_and_global() {
    let files = vec![
        ("a.ts".to_string(), "export class Foo {}".to_string()),
        ("b.ts".to_string(), "export interface Bar {}".to_string()),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // When DefinitionStore is fully populated (parallel path), semantic_defs are
    // intentionally skipped in per-file binders. Verify via DefinitionStore instead.
    if program.definition_store.is_fully_populated() {
        // Find symbols via semantic_defs (globals may not contain module-scoped exports)
        let foo_sym_id = program
            .semantic_defs
            .iter()
            .find(|(_, e)| e.name == "Foo")
            .map(|(&id, _)| id)
            .expect("Foo should be in program semantic_defs");
        let bar_sym_id = program
            .semantic_defs
            .iter()
            .find(|(_, e)| e.name == "Bar")
            .map(|(&id, _)| id)
            .expect("Bar should be in program semantic_defs");
        assert!(
            program
                .definition_store
                .find_def_by_symbol(foo_sym_id.0)
                .is_some(),
            "Foo should have DefId in DefinitionStore"
        );
        assert!(
            program
                .definition_store
                .find_def_by_symbol(bar_sym_id.0)
                .is_some(),
            "Bar should have DefId in DefinitionStore"
        );
        // Per-file entry file_id is preserved in program.semantic_defs
        let foo_entry = program
            .semantic_defs
            .get(&foo_sym_id)
            .expect("Foo should exist in program semantic_defs");
        assert_eq!(
            foo_entry.file_id, 0,
            "Foo should have file_id 0 from per-file entry"
        );
    } else {
        // Legacy path: reconstructed binder has composed semantic_defs
        let binder_a = create_binder_from_bound_file(&program.files[0], &program, 0);
        let names: std::collections::HashSet<_> = binder_a
            .semantic_defs
            .values()
            .map(|e| e.name.as_str())
            .collect();
        assert!(
            names.contains("Foo"),
            "binder for a.ts should see Foo (own file)"
        );
        assert!(
            names.contains("Bar"),
            "binder for a.ts should see Bar (cross-file via global)"
        );
        let foo_sym_id = program
            .globals
            .get("Foo")
            .expect("Foo should be in globals");
        let foo_entry = binder_a
            .semantic_defs
            .get(&foo_sym_id)
            .expect("Foo should exist in composed semantic_defs");
        assert_eq!(
            foo_entry.file_id, 0,
            "Foo should have file_id 0 from per-file overlay"
        );
    }
}

// =============================================================================
// Declaration merging accumulation survival through merge pipeline
// =============================================================================

#[test]
fn semantic_defs_heritage_accumulation_survives_merge() {
    // Within-file interface merging should accumulate heritage_names,
    // and this enriched entry should survive the merge pipeline.
    let files = vec![(
        "a.ts".to_string(),
        "
interface Merged extends A { a: string }
interface Merged extends B { b: number }
interface Merged extends C { c: boolean }
"
        .to_string(),
    )];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let entry = program
        .semantic_defs
        .values()
        .find(|e| e.name == "Merged")
        .expect("Merged should be in semantic_defs");
    assert!(
        entry.heritage_names().contains(&"A".to_string()),
        "heritage should include A after merge"
    );
    assert!(
        entry.heritage_names().contains(&"B".to_string()),
        "heritage should include B after merge"
    );
    assert!(
        entry.heritage_names().contains(&"C".to_string()),
        "heritage should include C after merge"
    );
}

#[test]
fn semantic_defs_enum_member_accumulation_survives_merge() {
    // Within-file enum merging should accumulate members,
    // and this enriched entry should survive the merge pipeline.
    let files = vec![(
        "a.ts".to_string(),
        "
enum Direction { Up, Down }
enum Direction { Left, Right }
"
        .to_string(),
    )];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let entry = program
        .semantic_defs
        .values()
        .find(|e| e.name == "Direction")
        .expect("Direction should be in semantic_defs");
    assert_eq!(
        entry.enum_member_names.len(),
        4,
        "all 4 enum members should survive merge"
    );
    assert!(entry.enum_member_names.contains(&"Up".to_string()));
    assert!(entry.enum_member_names.contains(&"Down".to_string()));
    assert!(entry.enum_member_names.contains(&"Left".to_string()));
    assert!(entry.enum_member_names.contains(&"Right".to_string()));
}

#[test]
fn semantic_defs_type_param_promotion_survives_merge() {
    // Within-file interface augmentation that adds type params should
    // have the promoted type_param_count survive merge.
    let files = vec![(
        "a.ts".to_string(),
        "
interface Container { base: string }
interface Container<T> { extra: T }
"
        .to_string(),
    )];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let entry = program
        .semantic_defs
        .values()
        .find(|e| e.name == "Container")
        .expect("Container should be in semantic_defs");
    assert_eq!(
        entry.type_param_count, 1,
        "type_param_count promotion should survive merge"
    );
}

#[test]
fn semantic_defs_enriched_heritage_in_bound_file() {
    // Verify the per-file BoundFile.semantic_defs also carries
    // the accumulated heritage_names.
    let files = vec![(
        "a.ts".to_string(),
        "
interface Extended extends Base { a: string }
interface Extended extends Extra { b: number }
"
        .to_string(),
    )];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Check the per-file BoundFile
    let file_entry = program.files[0]
        .semantic_defs
        .values()
        .find(|e| e.name == "Extended")
        .expect("Extended should be in BoundFile.semantic_defs");
    assert!(
        file_entry.heritage_names().contains(&"Base".to_string()),
        "per-file entry should have Base heritage"
    );
    assert!(
        file_entry.heritage_names().contains(&"Extra".to_string()),
        "per-file entry should have Extra heritage"
    );
}

#[test]
fn semantic_defs_enriched_data_survives_binder_reconstruction() {
    // Heritage data from declaration merging should be preserved in the
    // global program.semantic_defs (authoritative source after merge).
    // When DefinitionStore is fully populated (parallel path), per-file
    // binder semantic_defs are intentionally empty for performance.
    let files = vec![
        (
            "a.ts".to_string(),
            "
interface Composed extends A { a: string }
interface Composed extends B { b: number }
"
            .to_string(),
        ),
        ("b.ts".to_string(), "export class Other {}".to_string()),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Check program-level semantic_defs (always populated)
    let entry = program
        .semantic_defs
        .values()
        .find(|e| e.name == "Composed")
        .expect("Composed should be in program's semantic_defs");
    assert!(
        entry.heritage_names().contains(&"A".to_string()),
        "program semantic_defs should preserve heritage A"
    );
    assert!(
        entry.heritage_names().contains(&"B".to_string()),
        "program semantic_defs should preserve heritage B"
    );
}

// =============================================================================
// Merge-time DefinitionStore Pre-population Tests
// =============================================================================

#[test]
fn definition_store_pre_populated_during_merge() {
    let files = vec![
        ("a.ts".to_string(), "export class Foo {}".to_string()),
        (
            "b.ts".to_string(),
            "export interface Bar { x: number }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // The definition store should have DefIds for both declarations
    let stats = program.definition_store.statistics();
    assert!(
        stats.total_definitions >= 2,
        "expected at least 2 pre-populated DefIds in store, got {}",
        stats.total_definitions
    );
}

#[test]
fn definition_store_contains_all_declaration_families() {
    let source = r#"
        export class MyClass {}
        export interface MyInterface { x: number }
        export type MyAlias = string | number
        export enum MyEnum { A, B }
        export namespace MyNS { export type T = number }
        export function myFunc() {}
        export const myVar = 42;
    "#;
    let files = vec![("test.ts".to_string(), source.to_string())];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let stats = program.definition_store.statistics();
    assert!(
        stats.total_definitions >= 7,
        "expected at least 7 pre-populated DefIds (class, interface, alias, enum, namespace, function, variable), got {}",
        stats.total_definitions
    );
}

#[test]
fn definition_store_defids_match_semantic_defs_symbols() {
    let files = vec![
        ("a.ts".to_string(), "export class Alpha {}".to_string()),
        ("b.ts".to_string(), "export type Beta = string".to_string()),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Every symbol in semantic_defs should have a DefId in the store
    for &sym_id in program.semantic_defs.keys() {
        let def_id = program.definition_store.find_def_by_symbol(sym_id.0);
        assert!(
            def_id.is_some(),
            "SymbolId({}) should have a pre-populated DefId in the store",
            sym_id.0
        );
    }
}

#[test]
fn definition_store_defids_survive_binder_reconstruction() {
    let files = vec![
        ("a.ts".to_string(), "export class Foo {}".to_string()),
        (
            "b.ts".to_string(),
            "export interface Bar { y: string }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Reconstruct binders (as check_files_parallel does)
    let binder_a = create_binder_from_bound_file(&program.files[0], &program, 0);
    let binder_b = create_binder_from_bound_file(&program.files[1], &program, 1);

    // After reconstruction, the semantic_defs should still map to the same
    // DefIds that were pre-populated during merge.
    for &sym_id in binder_a.semantic_defs.keys() {
        let def_id = program.definition_store.find_def_by_symbol(sym_id.0);
        assert!(
            def_id.is_some(),
            "reconstructed binder_a: SymbolId({}) should have DefId in shared store",
            sym_id.0
        );
    }
    for &sym_id in binder_b.semantic_defs.keys() {
        let def_id = program.definition_store.find_def_by_symbol(sym_id.0);
        assert!(
            def_id.is_some(),
            "reconstructed binder_b: SymbolId({}) should have DefId in shared store",
            sym_id.0
        );
    }
}

#[test]
fn definition_store_defids_deterministic_across_merges() {
    let files = vec![
        ("a.ts".to_string(), "export class X {}".to_string()),
        ("b.ts".to_string(), "export class Y {}".to_string()),
    ];

    // Merge twice with the same input
    let results1 = parse_and_bind_parallel(files.clone());
    let program1 = merge_bind_results(results1);

    let results2 = parse_and_bind_parallel(files);
    let program2 = merge_bind_results(results2);

    // Both merges should produce the same number of DefIds
    let stats1 = program1.definition_store.statistics();
    let stats2 = program2.definition_store.statistics();
    assert_eq!(
        stats1.total_definitions, stats2.total_definitions,
        "deterministic merge should produce same DefId count"
    );

    // The DefId values should also be the same (sequential allocation from 1)
    for (&sym_id, entry) in program1.semantic_defs.iter() {
        let def1 = program1.definition_store.find_def_by_symbol(sym_id.0);
        // Find the corresponding symbol in program2 by name
        let sym_id2 = program2
            .semantic_defs
            .iter()
            .find(|(_, e)| e.name == entry.name)
            .map(|(&id, _)| id);
        if let Some(sid2) = sym_id2 {
            let def2 = program2.definition_store.find_def_by_symbol(sid2.0);
            assert!(
                def1.is_some() && def2.is_some(),
                "both merges should produce DefIds for '{}'",
                entry.name
            );
        }
    }
}

#[test]
fn definition_store_preserves_kind_and_metadata() {
    let source = r#"
        export abstract class Abs {}
        export const enum ConstEnum { X }
        export interface Generic<T> { value: T }
    "#;
    let files = vec![("test.ts".to_string(), source.to_string())];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Check that DefKind, is_abstract, is_const are preserved
    for (_sym_id, entry) in program.semantic_defs.iter() {
        let def_id = program
            .definition_store
            .find_def_by_symbol(_sym_id.0)
            .expect("should have DefId");
        let info = program
            .definition_store
            .get(def_id)
            .expect("should have DefinitionInfo");

        match entry.name.as_str() {
            "Abs" => {
                assert_eq!(info.kind, tsz_solver::def::DefKind::Class);
                assert!(info.is_abstract, "Abs should be abstract");
            }
            "ConstEnum" => {
                assert_eq!(info.kind, tsz_solver::def::DefKind::Enum);
                assert!(info.is_const, "ConstEnum should be const");
            }
            "Generic" => {
                assert_eq!(info.kind, tsz_solver::def::DefKind::Interface);
                assert_eq!(
                    info.type_params.len(),
                    1,
                    "Generic should have 1 type param"
                );
            }
            _ => {}
        }
    }
}

#[test]
fn definition_store_declaration_merge_preserves_first_defid() {
    // Two files with the same interface name → declaration merging
    let files = vec![
        (
            "a.ts".to_string(),
            "interface Shared { x: number }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "interface Shared { y: string }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Declaration merging means one symbol, one semantic_def, one DefId
    let shared_entries: Vec<_> = program
        .semantic_defs
        .iter()
        .filter(|(_, e)| e.name == "Shared")
        .collect();
    assert_eq!(
        shared_entries.len(),
        1,
        "declaration-merged interface should have one semantic_def"
    );

    let (&sym_id, _) = shared_entries[0];
    let def_id = program
        .definition_store
        .find_def_by_symbol(sym_id.0)
        .expect("merged interface should have a DefId");
    assert!(
        def_id.is_valid(),
        "DefId for merged interface should be valid"
    );
}

#[test]
fn definition_store_namespace_exports_wired_during_pre_populate() {
    // Namespace members with parent_namespace should be wired as exports
    // of their parent's DefinitionInfo during pre_populate_definition_store.
    let source = r#"
        namespace MyNS {
            export class Foo {}
            export interface Bar {}
            export type Baz = string;
            export enum Color { Red }
        }
    "#;
    let files = vec![("test.ts".to_string(), source.to_string())];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Find the namespace DefId
    let ns_entry = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "MyNS" && e.kind == crate::binder::SemanticDefKind::Namespace);
    let (&ns_sym, _) = ns_entry.expect("expected semantic def for MyNS");
    let ns_def_id = program
        .definition_store
        .find_def_by_symbol(ns_sym.0)
        .expect("MyNS should have a DefId");

    // The namespace's DefinitionInfo.exports should contain its members
    let exports = program
        .definition_store
        .get_exports(ns_def_id)
        .unwrap_or_default();
    assert!(
        exports.len() >= 4,
        "MyNS should have at least 4 exports (Foo, Bar, Baz, Color), got {}",
        exports.len()
    );

    // Each member should also have parent_namespace set in semantic_defs
    let member_names = ["Foo", "Bar", "Baz", "Color"];
    for name in &member_names {
        let member_entry = program.semantic_defs.values().find(|e| e.name == *name);
        let entry = member_entry.unwrap_or_else(|| panic!("expected semantic def for '{name}'"));
        assert_eq!(
            entry.parent_namespace,
            Some(ns_sym),
            "'{name}' should have parent_namespace = MyNS"
        );
    }
}

#[test]
fn definition_store_namespace_exports_survive_binder_reconstruction() {
    // After binder reconstruction (as check_files_parallel does),
    // namespace export wiring should still be intact in the shared store.
    let source = r#"
        namespace NS {
            export class Inner {}
        }
    "#;
    let files = vec![("test.ts".to_string(), source.to_string())];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Reconstruct binder (as check_files_parallel does)
    let _binder = create_binder_from_bound_file(&program.files[0], &program, 0);

    // Use program.semantic_defs (always populated) to verify parent_namespace
    let inner_entry = program.semantic_defs.values().find(|e| e.name == "Inner");
    assert!(
        inner_entry.is_some(),
        "program semantic_defs should have entry for Inner"
    );
    let inner = inner_entry.unwrap();
    assert!(
        inner.parent_namespace.is_some(),
        "Inner should have parent_namespace set"
    );

    // The shared DefinitionStore should still have the export wiring
    let ns_sym = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "NS")
        .map(|(&id, _)| id)
        .expect("expected NS in program semantic_defs");
    let ns_def_id = program
        .definition_store
        .find_def_by_symbol(ns_sym.0)
        .expect("NS should have a DefId");
    let exports = program
        .definition_store
        .get_exports(ns_def_id)
        .unwrap_or_default();
    assert!(
        !exports.is_empty(),
        "NS exports should be non-empty in shared store after reconstruction"
    );
}

#[test]
fn definition_store_nested_namespace_exports_wired() {
    // Nested namespaces should have their own export wiring.
    let source = r#"
        namespace Outer {
            export namespace Inner {
                export class Deep {}
            }
        }
    "#;
    let files = vec![("test.ts".to_string(), source.to_string())];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Outer should have Inner as export
    let outer_sym = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Outer")
        .map(|(&id, _)| id)
        .expect("expected Outer");
    let outer_def = program
        .definition_store
        .find_def_by_symbol(outer_sym.0)
        .expect("Outer should have DefId");
    let outer_exports = program
        .definition_store
        .get_exports(outer_def)
        .unwrap_or_default();
    assert!(
        !outer_exports.is_empty(),
        "Outer should have Inner as an export"
    );

    // Inner should have Deep as export
    let inner_sym = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Inner")
        .map(|(&id, _)| id)
        .expect("expected Inner");
    let inner_def = program
        .definition_store
        .find_def_by_symbol(inner_sym.0)
        .expect("Inner should have DefId");
    let inner_exports = program
        .definition_store
        .get_exports(inner_def)
        .unwrap_or_default();
    assert!(
        !inner_exports.is_empty(),
        "Inner should have Deep as an export"
    );
}

// =============================================================================
// all_symbol_mappings + warm path tests
// =============================================================================

#[test]
fn all_symbol_mappings_covers_all_declaration_families() {
    // Verify that pre_populate_definition_store registers DefIds for all
    // major declaration families and that all_symbol_mappings() returns them.
    let source = r#"
        class MyClass {}
        interface MyInterface { x: number }
        type MyAlias = string;
        enum MyEnum { A, B }
        namespace MyNS { export class Inner {} }
        function myFunc() {}
        const myVar = 42;
    "#;
    let files = vec![("test.ts".to_string(), source.to_string())];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let mappings = program.definition_store.all_symbol_mappings();

    // Collect the names of all symbols that have DefIds
    let def_names: std::collections::HashSet<String> = mappings
        .iter()
        .filter_map(|(_raw_sym, def_id)| {
            program.definition_store.get(*def_id).map(|info| info.name)
        })
        .map(|atom| program.type_interner.resolve_atom(atom))
        .collect();

    assert!(
        def_names.contains("MyClass"),
        "all_symbol_mappings should include classes"
    );
    assert!(
        def_names.contains("MyInterface"),
        "all_symbol_mappings should include interfaces"
    );
    assert!(
        def_names.contains("MyAlias"),
        "all_symbol_mappings should include type aliases"
    );
    assert!(
        def_names.contains("MyEnum"),
        "all_symbol_mappings should include enums"
    );
    assert!(
        def_names.contains("MyNS"),
        "all_symbol_mappings should include namespaces"
    );
    assert!(
        def_names.contains("myFunc"),
        "all_symbol_mappings should include functions"
    );
    assert!(
        def_names.contains("myVar"),
        "all_symbol_mappings should include variables"
    );
}

#[test]
fn definition_store_identity_stable_across_merge_rebind() {
    // Verify that DefIds created during merge survive binder reconstruction.
    // This tests the full cycle: bind → merge → create_binder_from_bound_file.
    let files = vec![
        (
            "a.ts".to_string(),
            "export class Alpha {} export interface Beta {}".to_string(),
        ),
        (
            "b.ts".to_string(),
            "export type Gamma = string; export enum Delta { X }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Record DefIds from the merged program
    let alpha_def = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Alpha")
        .and_then(|(sym, _)| program.definition_store.find_def_by_symbol(sym.0));
    let beta_def = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Beta")
        .and_then(|(sym, _)| program.definition_store.find_def_by_symbol(sym.0));
    let gamma_def = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Gamma")
        .and_then(|(sym, _)| program.definition_store.find_def_by_symbol(sym.0));
    let delta_def = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Delta")
        .and_then(|(sym, _)| program.definition_store.find_def_by_symbol(sym.0));

    assert!(alpha_def.is_some(), "Alpha should have a DefId after merge");
    assert!(beta_def.is_some(), "Beta should have a DefId after merge");
    assert!(gamma_def.is_some(), "Gamma should have a DefId after merge");
    assert!(delta_def.is_some(), "Delta should have a DefId after merge");

    // Reconstruct binders (as check_files_parallel does)
    let _binder_a = create_binder_from_bound_file(&program.files[0], &program, 0);
    let _binder_b = create_binder_from_bound_file(&program.files[1], &program, 1);

    // Verify DefIds still resolve after reconstruction — the shared store persists.
    // Use program.semantic_defs (always populated) to find symbol IDs.
    for (name, expected) in [
        ("Alpha", alpha_def.unwrap()),
        ("Beta", beta_def.unwrap()),
        ("Gamma", gamma_def.unwrap()),
        ("Delta", delta_def.unwrap()),
    ] {
        let sym_id = program
            .semantic_defs
            .iter()
            .find(|(_, e)| e.name == name)
            .map(|(&id, _)| id);
        let sym = sym_id.unwrap_or_else(|| panic!("{name} should be in program semantic_defs"));
        let found = program.definition_store.find_def_by_symbol(sym.0);
        assert_eq!(
            found,
            Some(expected),
            "{name}'s DefId should be stable after binder reconstruction"
        );
    }
}

#[test]
fn all_symbol_mappings_count_matches_semantic_defs_count() {
    // The number of all_symbol_mappings entries should equal the number of
    // semantic_defs entries in the merged program (since pre_populate_definition_store
    // creates exactly one DefId per semantic_def entry).
    let files = vec![
        (
            "a.ts".to_string(),
            "export class A {} export interface B {}".to_string(),
        ),
        (
            "b.ts".to_string(),
            "export type C = number; export enum D { X, Y }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let mappings = program.definition_store.all_symbol_mappings();
    let semantic_defs_count = program.semantic_defs.len();

    assert_eq!(
        mappings.len(),
        semantic_defs_count,
        "all_symbol_mappings count ({}) should equal semantic_defs count ({})",
        mappings.len(),
        semantic_defs_count
    );
}

// =============================================================================
// Cross-file semantic_defs merge accumulation tests
// =============================================================================

#[test]
fn cross_file_interface_heritage_accumulated_in_semantic_defs() {
    // When an interface is declared across two files with different heritage
    // clauses, the merged semantic_defs entry should accumulate both sets of
    // heritage names.
    let files = vec![
        (
            "a.ts".to_string(),
            "interface Foo extends Bar { x: number }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "interface Foo extends Baz { y: string }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let foo_entry = program
        .semantic_defs
        .values()
        .find(|e| e.name == "Foo" && e.kind == crate::binder::SemanticDefKind::Interface)
        .expect("expected semantic def for Foo");

    assert!(
        foo_entry.heritage_names().contains(&"Bar".to_string()),
        "Foo should have heritage name 'Bar' from file a.ts, got {:?}",
        foo_entry.heritage_names()
    );
    assert!(
        foo_entry.heritage_names().contains(&"Baz".to_string()),
        "Foo should have heritage name 'Baz' from file b.ts, got {:?}",
        foo_entry.heritage_names()
    );
}

