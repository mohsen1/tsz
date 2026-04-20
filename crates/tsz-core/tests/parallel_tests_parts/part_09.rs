#[test]
fn skeleton_index_fingerprint_deterministic() {
    let files = vec![
        ("a.ts".to_string(), "let x = 1;".to_string()),
        ("b.ts".to_string(), "let y = 2;".to_string()),
    ];
    let results1 = parse_and_bind_parallel(files.clone());
    let results2 = parse_and_bind_parallel(files);

    let skels1: Vec<_> = results1.iter().map(extract_skeleton).collect();
    let skels2: Vec<_> = results2.iter().map(extract_skeleton).collect();

    let idx1 = reduce_skeletons(&skels1);
    let idx2 = reduce_skeletons(&skels2);

    assert_eq!(
        idx1.fingerprint, idx2.fingerprint,
        "identical projects should produce identical aggregate fingerprints"
    );
    assert_ne!(
        idx1.fingerprint, 0,
        "aggregate fingerprint should not be zero"
    );
}

#[test]
fn skeleton_index_fingerprint_changes_on_file_add() {
    let files_v1 = vec![("a.ts".to_string(), "let x = 1;".to_string())];
    let files_v2 = vec![
        ("a.ts".to_string(), "let x = 1;".to_string()),
        ("b.ts".to_string(), "let y = 2;".to_string()),
    ];

    let results_v1 = parse_and_bind_parallel(files_v1);
    let results_v2 = parse_and_bind_parallel(files_v2);

    let skels_v1: Vec<_> = results_v1.iter().map(extract_skeleton).collect();
    let skels_v2: Vec<_> = results_v2.iter().map(extract_skeleton).collect();

    let idx_v1 = reduce_skeletons(&skels_v1);
    let idx_v2 = reduce_skeletons(&skels_v2);

    assert_ne!(
        idx_v1.fingerprint, idx_v2.fingerprint,
        "adding a file should change the aggregate fingerprint"
    );
}

#[test]
fn skeleton_index_fingerprint_changes_on_symbol_change() {
    let files_v1 = vec![
        ("a.ts".to_string(), "let x = 1;".to_string()),
        ("b.ts".to_string(), "let y = 2;".to_string()),
    ];
    let files_v2 = vec![
        ("a.ts".to_string(), "let x = 1; let z = 3;".to_string()),
        ("b.ts".to_string(), "let y = 2;".to_string()),
    ];

    let results_v1 = parse_and_bind_parallel(files_v1);
    let results_v2 = parse_and_bind_parallel(files_v2);

    let skels_v1: Vec<_> = results_v1.iter().map(extract_skeleton).collect();
    let skels_v2: Vec<_> = results_v2.iter().map(extract_skeleton).collect();

    let idx_v1 = reduce_skeletons(&skels_v1);
    let idx_v2 = reduce_skeletons(&skels_v2);

    assert_ne!(
        idx_v1.fingerprint, idx_v2.fingerprint,
        "adding a symbol to one file should change the aggregate fingerprint"
    );
}

#[test]
fn skeleton_index_fingerprint_stable_on_body_change() {
    // Changing function bodies should not affect the aggregate fingerprint
    // since skeletons only capture top-level symbol topology.
    let files_v1 = vec![(
        "a.ts".to_string(),
        "function foo() { return 1; }".to_string(),
    )];
    let files_v2 = vec![(
        "a.ts".to_string(),
        "function foo() { return 999; }".to_string(),
    )];

    let results_v1 = parse_and_bind_parallel(files_v1);
    let results_v2 = parse_and_bind_parallel(files_v2);

    let skels_v1: Vec<_> = results_v1.iter().map(extract_skeleton).collect();
    let skels_v2: Vec<_> = results_v2.iter().map(extract_skeleton).collect();

    let idx_v1 = reduce_skeletons(&skels_v1);
    let idx_v2 = reduce_skeletons(&skels_v2);

    assert_eq!(
        idx_v1.fingerprint, idx_v2.fingerprint,
        "changing function bodies should not change the aggregate fingerprint"
    );
}

#[test]
fn skeleton_index_fingerprint_changes_on_merge_topology() {
    // Two script files declaring the same global name creates a merge candidate.
    // Changing one file to not declare that name should change the fingerprint.
    let files_v1 = vec![
        ("a.ts".to_string(), "let x = 1;".to_string()),
        ("b.ts".to_string(), "let x = 2;".to_string()),
    ];
    let files_v2 = vec![
        ("a.ts".to_string(), "let x = 1;".to_string()),
        ("b.ts".to_string(), "let y = 2;".to_string()),
    ];

    let results_v1 = parse_and_bind_parallel(files_v1);
    let results_v2 = parse_and_bind_parallel(files_v2);

    let skels_v1: Vec<_> = results_v1.iter().map(extract_skeleton).collect();
    let skels_v2: Vec<_> = results_v2.iter().map(extract_skeleton).collect();

    let idx_v1 = reduce_skeletons(&skels_v1);
    let idx_v2 = reduce_skeletons(&skels_v2);

    // v1 has a merge candidate for `x`, v2 does not.
    assert!(
        idx_v1.merge_candidates.iter().any(|mc| mc.name == "x"),
        "v1 should have merge candidate for x"
    );
    assert!(
        !idx_v2.merge_candidates.iter().any(|mc| mc.name == "x"),
        "v2 should not have merge candidate for x"
    );
    assert_ne!(
        idx_v1.fingerprint, idx_v2.fingerprint,
        "different merge topology should produce different aggregate fingerprints"
    );
}

#[test]
fn test_merge_deterministic_symbol_order() {
    // Merging the same set of files multiple times must produce identical
    // global symbol arenas and declaration orderings.  This exercises the
    // sorted id_remap iteration introduced for deterministic merge output.
    let files = vec![
        (
            "a.ts".to_string(),
            "export interface Shared { a: number; }\nexport function helper(): void {}".to_string(),
        ),
        (
            "b.ts".to_string(),
            "export interface Shared { b: string; }\nexport const VAL = 42;".to_string(),
        ),
        (
            "c.ts".to_string(),
            "export namespace NS { export function inner(): void {} }\nexport type Alias = string;"
                .to_string(),
        ),
    ];

    // Run the full bind + merge pipeline several times.
    let mut prev_symbol_names: Option<Vec<String>> = None;
    let mut prev_globals_names: Option<Vec<String>> = None;
    let mut prev_decl_counts: Option<Vec<usize>> = None;

    for _run in 0..5 {
        let bind_results = parse_and_bind_parallel(files.clone());
        let merged = merge_bind_results(bind_results);

        // Collect ordered lists of global symbol names and declaration counts.
        let mut symbol_names: Vec<String> = Vec::new();
        let mut decl_counts: Vec<usize> = Vec::new();
        for i in 0..merged.symbols.len() {
            let id = SymbolId(i as u32);
            if let Some(sym) = merged.symbols.get(id) {
                symbol_names.push(sym.escaped_name.clone());
                decl_counts.push(sym.declarations.len());
            }
        }

        let mut globals_names: Vec<String> =
            merged.globals.iter().map(|(n, _)| n.clone()).collect();
        globals_names.sort();

        if let Some(ref prev) = prev_symbol_names {
            assert_eq!(
                symbol_names, *prev,
                "global symbol arena ordering must be deterministic across runs"
            );
        }
        if let Some(ref prev) = prev_globals_names {
            assert_eq!(
                globals_names, *prev,
                "globals table content must be deterministic across runs"
            );
        }
        if let Some(ref prev) = prev_decl_counts {
            assert_eq!(
                decl_counts, *prev,
                "declaration counts per symbol must be deterministic across runs"
            );
        }

        prev_symbol_names = Some(symbol_names);
        prev_globals_names = Some(globals_names);
        prev_decl_counts = Some(decl_counts);
    }
}

#[test]
fn test_merge_deterministic_global_namespace() {
    // Cross-file global namespace merging must produce deterministic export
    // tables regardless of FxHashMap iteration order.  We use `declare
    // namespace` (not `export namespace`) so symbols land in globals, not
    // per-file module_exports.
    let files = vec![
        (
            "x.d.ts".to_string(),
            "declare namespace Deep { function fa(): void; }".to_string(),
        ),
        (
            "y.d.ts".to_string(),
            "declare namespace Deep { function fb(): void; }".to_string(),
        ),
    ];

    let mut prev_deep_exports: Option<Vec<String>> = None;
    let mut prev_symbol_names: Option<Vec<String>> = None;

    for _run in 0..5 {
        let bind_results = parse_and_bind_parallel(files.clone());
        let merged = merge_bind_results(bind_results);

        // Collect ordered list of global symbol names.
        let mut symbol_names: Vec<String> = Vec::new();
        for i in 0..merged.symbols.len() {
            let id = SymbolId(i as u32);
            if let Some(sym) = merged.symbols.get(id) {
                symbol_names.push(sym.escaped_name.clone());
            }
        }

        // Find the "Deep" symbol in globals.
        let deep_id = merged
            .globals
            .get("Deep")
            .expect("Deep namespace must be in globals");

        let deep_sym = merged.symbols.get(deep_id).expect("Deep symbol must exist");

        let deep_exports: Vec<String> = deep_sym
            .exports
            .as_ref()
            .map(|e| {
                let mut names: Vec<String> = e.iter().map(|(n, _)| n.clone()).collect();
                names.sort();
                names
            })
            .unwrap_or_default();

        // Deep should have both fa and fb from cross-file merge.
        assert!(
            deep_exports.contains(&"fa".to_string()),
            "Deep exports: {deep_exports:?} — must contain fa"
        );
        assert!(
            deep_exports.contains(&"fb".to_string()),
            "Deep exports: {deep_exports:?} — must contain fb"
        );

        if let Some(ref prev) = prev_symbol_names {
            assert_eq!(
                symbol_names, *prev,
                "global symbol arena ordering must be deterministic"
            );
        }
        if let Some(ref prev) = prev_deep_exports {
            assert_eq!(
                deep_exports, *prev,
                "Deep namespace exports must be deterministic"
            );
        }
        prev_symbol_names = Some(symbol_names);
        prev_deep_exports = Some(deep_exports);
    }
}

#[test]
fn test_skeleton_index_estimated_size_bytes_is_nonzero() {
    let files = vec![
        ("a.ts".to_string(), "export const a = 1;".to_string()),
        ("b.ts".to_string(), "export const b = 2;".to_string()),
        (
            "c.ts".to_string(),
            "export * from './a'; export { b } from './b';".to_string(),
        ),
    ];

    let bind_results = parse_and_bind_parallel(files);
    let program = merge_bind_results(bind_results);

    let stats = program.residency_stats();
    assert!(stats.has_skeleton_index);
    assert!(
        stats.skeleton_estimated_size_bytes > 0,
        "skeleton index should report nonzero estimated size, got 0"
    );
    // The estimate should at least cover the base struct size
    assert!(
        stats.skeleton_estimated_size_bytes >= std::mem::size_of::<SkeletonIndex>(),
        "skeleton size estimate ({}) should be >= struct size ({})",
        stats.skeleton_estimated_size_bytes,
        std::mem::size_of::<SkeletonIndex>()
    );
}

#[test]
fn test_skeleton_index_estimated_size_grows_with_content() {
    // Small project
    let small_files = vec![("a.ts".to_string(), "export const a = 1;".to_string())];
    let small_results = parse_and_bind_parallel(small_files);
    let small_program = merge_bind_results(small_results);
    let small_size = small_program
        .skeleton_index
        .as_ref()
        .unwrap()
        .estimated_size_bytes();

    // Larger project with more symbols and cross-file relationships
    let large_files = vec![
        (
            "a.ts".to_string(),
            "export const a1 = 1; export const a2 = 2; export const a3 = 3;".to_string(),
        ),
        (
            "b.ts".to_string(),
            "export const b1 = 1; export const b2 = 2; export const b3 = 3;".to_string(),
        ),
        (
            "c.ts".to_string(),
            "export * from './a'; export * from './b';".to_string(),
        ),
        (
            "d.ts".to_string(),
            "export { a1, a2 } from './a'; export { b1 } from './b';".to_string(),
        ),
    ];
    let large_results = parse_and_bind_parallel(large_files);
    let large_program = merge_bind_results(large_results);
    let large_size = large_program
        .skeleton_index
        .as_ref()
        .unwrap()
        .estimated_size_bytes();

    assert!(
        large_size > small_size,
        "larger project skeleton ({large_size} bytes) should be bigger than small ({small_size} bytes)"
    );
}

#[test]
fn test_bind_result_estimated_size_bytes_is_nonzero() {
    let result = parse_and_bind_single("a.ts".to_string(), "export const a = 1;".to_string());
    let size = result.estimated_size_bytes();
    assert!(
        size > 0,
        "estimated_size_bytes should be nonzero for any bind result"
    );
    // Must be at least the struct size itself
    assert!(
        size >= std::mem::size_of::<BindResult>(),
        "estimated size ({}) should be >= struct size ({})",
        size,
        std::mem::size_of::<BindResult>()
    );
}

