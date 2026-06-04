#[test]
fn test_resolution_map_prefers_source_over_declaration_independent_of_order() {
    // Driver-side map (`build_module_resolution_maps`, used by the CLI and
    // server). The extensionless stem `./types` must map to the source file in
    // BOTH file orderings — a fixture-order-only fix would pass one and fail
    // the other.
    for (files, source_idx) in [
        (
            vec![
                "/proj/main.ts".to_string(),
                "/proj/types.d.ts".to_string(),
                "/proj/types.ts".to_string(),
            ],
            2usize,
        ),
        (
            vec![
                "/proj/main.ts".to_string(),
                "/proj/types.ts".to_string(),
                "/proj/types.d.ts".to_string(),
            ],
            1usize,
        ),
    ] {
        let (paths, _modules) = build_module_resolution_maps(&files);
        assert_eq!(
            paths.get(&(0, "./types".to_string())),
            Some(&source_idx),
            "./types must map to the source sibling for files {files:?}",
        );
        // Explicit spellings stay unambiguous and addressable.
        assert!(paths.contains_key(&(0, "./types.d.ts".to_string())));
        assert!(paths.contains_key(&(0, "./types.ts".to_string())));
    }
}

#[test]
fn test_resolve_from_source_prefers_source_over_declaration() {
    // The `TargetIndex`-based resolver mirrors the same priority rule.
    let files = vec![
        "/proj/src/main.ts".to_string(),
        "/proj/src/api.d.ts".to_string(),
        "/proj/src/api.ts".to_string(),
    ];
    let index = build_target_index(&files);
    let spec = normalize_import_specifier("./api").unwrap();
    assert_eq!(
        resolve_from_source("/proj/src/main.ts", &spec, &index),
        Some(2),
        "./api must resolve to source api.ts, not api.d.ts",
    );
    // Explicit declaration spelling still selects the declaration file.
    let dts_spec = normalize_import_specifier("./api.d.ts").unwrap();
    assert_eq!(
        resolve_from_source("/proj/src/main.ts", &dts_spec, &index),
        Some(1),
        "./api.d.ts must still resolve to the declaration file",
    );
}
