#[test]
fn test_check_nested_functions() {
    let files = vec![(
        "a.ts".to_string(),
        "function outer() { function inner() { return 1; } return inner(); }".to_string(),
    )];

    let program = compile_files(files);
    let result = check_functions_parallel(&program);

    // Should find both outer and inner functions
    let total_functions: usize = result
        .file_results
        .iter()
        .map(|r| r.function_results.len())
        .sum();
    assert!(
        total_functions >= 2,
        "Should find both outer and inner functions"
    );
}

#[test]
fn test_check_exported_functions() {
    let files = vec![
        (
            "a.ts".to_string(),
            "export function add(x: number, y: number) { return x + y; }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "export function subtract(x: number, y: number) { return x - y; }".to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_functions_parallel(&program);

    // Should find the exported functions
    let total_functions: usize = result
        .file_results
        .iter()
        .map(|r| r.function_results.len())
        .sum();

    assert_eq!(total_functions, 2);
}

#[test]
fn test_parallel_type_interner_concurrent_access() {
    use std::sync::Arc;
    use std::thread;

    // Test that the new lock-free TypeInterner supports concurrent access
    let interner = Arc::new(TypeInterner::new());

    let mut handles = vec![];

    // Spawn multiple threads that all intern types concurrently
    for i in 0..10 {
        let interner_clone = Arc::clone(&interner);
        let handle = thread::spawn(move || {
            // Each thread interns various types
            for j in 0..100 {
                let _ = interner_clone.literal_number(j as f64);
                let _ = interner_clone.literal_string(&format!("str_{i}_{j}"));
                let _ = interner_clone.union(vec![
                    interner_clone.literal_number((j % 10) as f64),
                    interner_clone.literal_number(((j + 1) % 10) as f64),
                ]);
            }
        });
        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify the interner has the expected number of types
    // (exact count depends on deduplication, but should be reasonable)
    let len = interner.len();
    assert!(len > 100, "Expected at least 100 types, got {len}");
    assert!(len < 2000, "Expected fewer than 2000 types, got {len}");
}

#[test]
fn test_parallel_type_checking_with_shared_interner() {
    // Test that multiple files can be type-checked in parallel
    // while sharing a single TypeInterner for type deduplication
    let files = vec![
        (
            "math.ts".to_string(),
            r#"
                function add(a: number, b: number): number { return a + b; }
                function subtract(a: number, b: number): number { return a - b; }
                function multiply(a: number, b: number): number { return a * b; }
            "#
            .to_string(),
        ),
        (
            "strings.ts".to_string(),
            r#"
                function concat(a: string, b: string): string { return a + b; }
                function upper(s: string): string { return s.toUpperCase(); }
                function lower(s: string): string { return s.toLowerCase(); }
            "#
            .to_string(),
        ),
        (
            "arrays.ts".to_string(),
            r#"
                function first<T>(arr: T[]): T | undefined { return arr[0]; }
                function last<T>(arr: T[]): T | undefined { return arr[arr.length - 1]; }
                function isEmpty<T>(arr: T[]): boolean { return arr.length === 0; }
            "#
            .to_string(),
        ),
        (
            "objects.ts".to_string(),
            r#"
                function keys(obj: object): string[] { return Object.keys(obj); }
                function values(obj: object): unknown[] { return Object.values(obj); }
                function entries(obj: object): [string, unknown][] { return Object.entries(obj); }
            "#
            .to_string(),
        ),
    ];

    let program = compile_files(files);
    assert_eq!(program.files.len(), 4);

    // Check all files in parallel
    let (_result, stats) = check_functions_with_stats(&program);

    assert_eq!(stats.file_count, 4);
    // Each file has 3 functions
    assert!(
        stats.function_count >= 12,
        "Expected at least 12 functions, got {}",
        stats.function_count
    );

    // The shared TypeInterner should have deduplicated common types
    // (number, string, boolean, etc. are shared across all files)
    let interner_len = program.type_interner.len();
    assert!(
        interner_len > TypeId::FIRST_USER as usize,
        "TypeInterner should have user-defined types"
    );
}

#[test]
fn test_parallel_binding_produces_consistent_symbols() {
    // Test that parallel binding produces consistent results
    // by binding the same files multiple times
    let files = vec![
        (
            "a.ts".to_string(),
            "export const x: number = 1;".to_string(),
        ),
        (
            "b.ts".to_string(),
            "export const y: string = 'hello';".to_string(),
        ),
        (
            "c.ts".to_string(),
            "export function add(a: number, b: number) { return a + b; }".to_string(),
        ),
    ];

    // Bind multiple times
    let results1 = parse_and_bind_parallel(files.clone());
    let results2 = parse_and_bind_parallel(files);

    // Results should be structurally identical
    assert_eq!(results1.len(), results2.len());

    for (r1, r2) in results1.iter().zip(results2.iter()) {
        assert_eq!(r1.file_name, r2.file_name);
        assert_eq!(r1.arena.len(), r2.arena.len());
        assert_eq!(r1.symbols.len(), r2.symbols.len());

        // Same symbols should be present
        for (name, _) in r1.file_locals.iter() {
            assert!(
                r2.file_locals.has(name),
                "Symbol {name} should be present in both results"
            );
        }
    }
}

// =============================================================================
// Phase 1 DefId-First Stable Identity Tests (Parallel Pipeline)
// =============================================================================

#[test]
fn semantic_defs_survive_single_file_bind() {
    let result = parse_and_bind_single(
        "test.ts".to_string(),
        "class A {} interface B {} type C = number; enum D { X } namespace E {}".to_string(),
    );
    assert_eq!(
        result.semantic_defs.len(),
        5,
        "expected 5 semantic defs, got {}",
        result.semantic_defs.len()
    );
}

#[test]
fn semantic_defs_survive_merge_with_remapped_symbol_ids() {
    let files = vec![
        ("a.ts".to_string(), "export class Foo {}".to_string()),
        (
            "b.ts".to_string(),
            "export interface Bar { x: number }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Both Foo and Bar should be in the merged semantic_defs
    let names: std::collections::HashSet<_> = program
        .semantic_defs
        .values()
        .map(|e| e.name.as_str())
        .collect();
    assert!(
        names.contains("Foo"),
        "Foo should be in merged semantic_defs"
    );
    assert!(
        names.contains("Bar"),
        "Bar should be in merged semantic_defs"
    );
}

#[test]
fn semantic_defs_file_id_is_correct_after_merge() {
    let files = vec![
        ("file0.ts".to_string(), "export class Alpha {}".to_string()),
        (
            "file1.ts".to_string(),
            "export type Beta = string".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    for entry in program.semantic_defs.values() {
        match entry.name.as_str() {
            "Alpha" => assert_eq!(entry.file_id, 0, "Alpha should be in file 0"),
            "Beta" => assert_eq!(entry.file_id, 1, "Beta should be in file 1"),
            _ => {}
        }
    }
}

#[test]
fn semantic_defs_stable_across_repeated_merge() {
    let files = vec![(
        "a.ts".to_string(),
        "export class C {} export interface I {} export type T = number; export enum E { X }"
            .to_string(),
    )];

    let results1 = parse_and_bind_parallel(files.clone());
    let program1 = merge_bind_results(results1);
    let results2 = parse_and_bind_parallel(files);
    let program2 = merge_bind_results(results2);

    assert_eq!(program1.semantic_defs.len(), program2.semantic_defs.len());

    // Same names and kinds should appear
    let defs1: std::collections::HashMap<_, _> = program1
        .semantic_defs
        .values()
        .map(|e| (e.name.clone(), e.kind))
        .collect();
    let defs2: std::collections::HashMap<_, _> = program2
        .semantic_defs
        .values()
        .map(|e| (e.name.clone(), e.kind))
        .collect();
    assert_eq!(
        defs1, defs2,
        "semantic defs should be identical across rebuilds"
    );
}

// =============================================================================
// Skeleton integration into MergedProgram
// =============================================================================

#[test]
fn skeleton_index_populated_after_merge() {
    let files = vec![
        ("a.ts".to_string(), "let x = 1;".to_string()),
        ("b.ts".to_string(), "let y = 2;".to_string()),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    assert!(
        program.skeleton_index.is_some(),
        "skeleton_index should be populated after merge"
    );
    let idx = program.skeleton_index.as_ref().unwrap();
    assert_eq!(idx.file_count, 2);
}

