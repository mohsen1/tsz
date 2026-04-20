#[test]
fn skeleton_index_single_file() {
    let files = vec![("test.ts".to_string(), "let x = 42;".to_string())];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let idx = program.skeleton_index.as_ref().unwrap();
    assert_eq!(idx.file_count, 1);
    assert!(
        idx.merge_candidates.is_empty(),
        "single file should have no merge candidates"
    );
    assert!(
        idx.total_symbol_count > 0,
        "should have at least one symbol"
    );
}

#[test]
fn skeleton_index_captures_declared_modules() {
    let files = vec![(
        "ambient.d.ts".to_string(),
        r#"declare module "my-module" { export function hello(): void; }"#.to_string(),
    )];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let idx = program.skeleton_index.as_ref().unwrap();
    assert!(
        idx.declared_modules.contains("my-module"),
        "skeleton index should capture declared module names"
    );
}

#[test]
fn skeleton_index_captures_merge_candidates() {
    // Two script files (not modules) with the same interface name should produce
    // a merge candidate.
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

    let idx = program.skeleton_index.as_ref().unwrap();
    let shared = idx.merge_candidates.iter().find(|c| c.name == "Shared");
    assert!(
        shared.is_some(),
        "interface 'Shared' should appear as a merge candidate"
    );
    let shared = shared.unwrap();
    assert_eq!(shared.source_files.len(), 2);
    assert!(
        shared.is_valid_merge,
        "interface + interface should be a valid merge"
    );
}

#[test]
fn skeleton_index_stable_across_rebuilds() {
    let files = vec![
        ("a.ts".to_string(), "let x = 1;".to_string()),
        ("b.ts".to_string(), "let y = 2;".to_string()),
    ];

    let results1 = parse_and_bind_parallel(files.clone());
    let program1 = merge_bind_results(results1);
    let results2 = parse_and_bind_parallel(files);
    let program2 = merge_bind_results(results2);

    let idx1 = program1.skeleton_index.as_ref().unwrap();
    let idx2 = program2.skeleton_index.as_ref().unwrap();

    assert_eq!(idx1.file_count, idx2.file_count);
    assert_eq!(idx1.total_symbol_count, idx2.total_symbol_count);
    assert_eq!(idx1.merge_candidates.len(), idx2.merge_candidates.len());
    assert_eq!(idx1.total_reexport_count, idx2.total_reexport_count);
}

#[test]
fn skeleton_index_reexport_counts() {
    let files = vec![
        ("a.ts".to_string(), "export const foo = 1;".to_string()),
        ("b.ts".to_string(), "export { foo } from './a';".to_string()),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let idx = program.skeleton_index.as_ref().unwrap();
    // b.ts has a named re-export
    assert!(
        idx.total_reexport_count > 0 || idx.total_wildcard_reexport_count > 0,
        "should track re-export edges in skeleton index"
    );
}

#[test]
fn skeleton_index_external_modules_excluded_from_global_merge() {
    // External modules (files with import/export) should not contribute to
    // global merge candidates. Only script files do.
    let files = vec![
        (
            "mod_a.ts".to_string(),
            "export interface Dup { x: number; }".to_string(),
        ),
        (
            "mod_b.ts".to_string(),
            "export interface Dup { y: string; }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let idx = program.skeleton_index.as_ref().unwrap();
    let dup = idx.merge_candidates.iter().find(|c| c.name == "Dup");
    assert!(
        dup.is_none(),
        "external module symbols should not appear as merge candidates"
    );
}

#[test]
fn skeleton_index_captures_module_export_specifiers() {
    // declare module "x" { ... } populates module_exports in the binder.
    // The skeleton should capture those keys in module_export_specifiers.
    let files = vec![(
        "ambient.d.ts".to_string(),
        r#"declare module "my-lib" { export function greet(): string; }"#.to_string(),
    )];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let idx = program.skeleton_index.as_ref().unwrap();
    assert!(
        idx.module_export_specifiers.contains("my-lib")
            || idx.module_export_specifiers.contains("\"my-lib\""),
        "skeleton index should capture module export specifiers, got: {:?}",
        idx.module_export_specifiers
    );
}

#[test]
fn skeleton_build_declared_modules_matches_binder() {
    // Verify that SkeletonIndex::build_declared_module_sets produces the same
    // result as the binder-scanning loop in set_all_binders for declared modules.
    let files = vec![
        (
            "ambient.d.ts".to_string(),
            r#"declare module "fs" { export function readFile(): void; }"#.to_string(),
        ),
        (
            "wildcard.d.ts".to_string(),
            r#"declare module "*.css" { const content: string; export default content; }"#
                .to_string(),
        ),
        (
            "shorthand.d.ts".to_string(),
            r#"declare module "my-shorthand";"#.to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let idx = program.skeleton_index.as_ref().unwrap();
    let (exact, patterns) = idx.build_declared_module_sets();

    // "fs" should be in exact (from module_exports key or declared_modules)
    assert!(
        exact.contains("fs"),
        "exact set should contain 'fs', got: {exact:?}"
    );

    // "my-shorthand" from shorthand ambient module
    assert!(
        exact.contains("my-shorthand"),
        "exact set should contain 'my-shorthand', got: {exact:?}"
    );

    // "*.css" should be in patterns
    assert!(
        patterns.contains(&"*.css".to_string()),
        "patterns should contain '*.css', got: {patterns:?}"
    );
}

#[test]
fn skeleton_build_declared_modules_deduplicates_patterns() {
    // Two files both declaring the same wildcard module should produce
    // only one entry in patterns.
    let files = vec![
        (
            "a.d.ts".to_string(),
            r#"declare module "*.svg" { const url: string; export default url; }"#.to_string(),
        ),
        (
            "b.d.ts".to_string(),
            r#"declare module "*.svg" { }"#.to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let idx = program.skeleton_index.as_ref().unwrap();
    let (_exact, patterns) = idx.build_declared_module_sets();

    let svg_count = patterns.iter().filter(|p| *p == "*.svg").count();
    assert_eq!(
        svg_count, 1,
        "duplicate wildcard patterns should be deduplicated, got {svg_count} occurrences"
    );
}

#[test]
fn skeleton_validate_against_merged_declared_modules() {
    // Ambient module declarations should match between skeleton and legacy merge.
    let files = vec![
        (
            "ambient.d.ts".to_string(),
            r#"declare module "my-lib" { export function greet(): string; }"#.to_string(),
        ),
        (
            "ambient2.d.ts".to_string(),
            r#"declare module "my-other-lib" { export const version: number; }"#.to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // If we got here without panic, the debug validation in merge_bind_results passed.
    let idx = program.skeleton_index.as_ref().unwrap();
    assert!(
        idx.declared_modules.contains("\"my-lib\"") || idx.declared_modules.contains("my-lib"),
        "skeleton should contain declared module 'my-lib', got: {:?}",
        idx.declared_modules
    );
    assert_eq!(
        idx.declared_modules, program.declared_modules,
        "skeleton and legacy declared_modules must match"
    );
}

