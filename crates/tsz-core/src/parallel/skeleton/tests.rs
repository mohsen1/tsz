//! Skeleton extraction, reduction, diff, and legacy-validation tests.
//!
//! Tests for `extract_skeleton`, `reduce_skeletons`, `SkeletonIndex` lookup /
//! build helpers, `diff_skeletons`, and the `validate_*_against_legacy` parity
//! assertions. Moved out of `skeleton/mod.rs` so the production module stays
//! under the §19 2000-line cap; `use super::*` continues to import everything
//! re-exported by `mod.rs` (which itself re-exports `diff::*`).

use super::*;

type AugDecl = (String, u32, u32);
type ModuleAugEntry = (String, Vec<AugDecl>);

/// Helper to make a minimal skeleton for testing diffs.
fn make_skeleton(name: &str, fingerprint: u64) -> FileSkeleton {
    FileSkeleton {
        file_name: name.to_string(),
        is_external_module: true,
        symbols: vec![],
        global_augmentations: vec![],
        module_augmentations: vec![],
        augmentation_targets: vec![],
        reexports: vec![],
        wildcard_reexports: vec![],
        expando_properties: vec![],
        declared_modules: vec![],
        shorthand_ambient_modules: vec![],
        module_export_specifiers: vec![],
        module_exports_entries: vec![],
        import_sources: vec![],
        file_features: Default::default(),
        fingerprint,
    }
}

#[test]
fn diff_identical_skeletons_is_empty() {
    let skels = vec![make_skeleton("a.ts", 100), make_skeleton("b.ts", 200)];
    let diff = diff_skeletons(&skels, &skels);
    assert!(diff.is_empty());
    assert_eq!(diff.affected_count(), 0);
    assert!(!diff.topology_changed);
}

#[test]
fn diff_detects_changed_file() {
    let prev = vec![make_skeleton("a.ts", 100), make_skeleton("b.ts", 200)];
    let curr = vec![
        make_skeleton("a.ts", 100),
        make_skeleton("b.ts", 999), // changed
    ];
    let diff = diff_skeletons(&prev, &curr);
    assert_eq!(diff.changed, vec!["b.ts"]);
    assert!(diff.added.is_empty());
    assert!(diff.removed.is_empty());
    assert_eq!(diff.affected_count(), 1);
}

#[test]
fn diff_detects_added_file() {
    let prev = vec![make_skeleton("a.ts", 100)];
    let curr = vec![make_skeleton("a.ts", 100), make_skeleton("new.ts", 300)];
    let diff = diff_skeletons(&prev, &curr);
    assert!(diff.changed.is_empty());
    assert_eq!(diff.added, vec!["new.ts"]);
    assert!(diff.removed.is_empty());
}

#[test]
fn diff_detects_removed_file() {
    let prev = vec![make_skeleton("a.ts", 100), make_skeleton("old.ts", 200)];
    let curr = vec![make_skeleton("a.ts", 100)];
    let diff = diff_skeletons(&prev, &curr);
    assert!(diff.changed.is_empty());
    assert!(diff.added.is_empty());
    assert_eq!(diff.removed, vec!["old.ts"]);
}

#[test]
fn diff_detects_combined_changes() {
    let prev = vec![
        make_skeleton("keep.ts", 100),
        make_skeleton("change.ts", 200),
        make_skeleton("remove.ts", 300),
    ];
    let curr = vec![
        make_skeleton("keep.ts", 100),
        make_skeleton("change.ts", 999),
        make_skeleton("add.ts", 400),
    ];
    let diff = diff_skeletons(&prev, &curr);
    assert_eq!(diff.changed, vec!["change.ts"]);
    assert_eq!(diff.added, vec!["add.ts"]);
    assert_eq!(diff.removed, vec!["remove.ts"]);
    assert_eq!(diff.affected_count(), 3);
    assert!(diff.topology_changed);
}

#[test]
fn heritage_fingerprint_affects_skeleton_fingerprint() {
    // Two skeletons identical except for heritage fingerprint on a symbol
    // should produce different fingerprints.
    let sym_no_heritage = SkeletonSymbol {
        name: "Foo".to_string(),
        flags: 0,
        is_exported: true,
        declaration_count: 1,
        has_exports: false,
        has_members: false,
        is_lib_origin: false,
        is_import_alias: false,
        import_module: None,
        heritage_fingerprint: 0,
        heritage_count: 0,
    };
    let sym_with_heritage = SkeletonSymbol {
        heritage_fingerprint: 42,
        heritage_count: 1,
        ..sym_no_heritage.clone()
    };

    let mut skel1 = FileSkeleton {
        file_name: "test.ts".to_string(),
        is_external_module: true,
        symbols: vec![sym_no_heritage],
        global_augmentations: vec![],
        module_augmentations: vec![],
        augmentation_targets: vec![],
        reexports: vec![],
        wildcard_reexports: vec![],
        expando_properties: vec![],
        declared_modules: vec![],
        shorthand_ambient_modules: vec![],
        module_export_specifiers: vec![],
        module_exports_entries: vec![],
        import_sources: vec![],
        file_features: Default::default(),
        fingerprint: 0,
    };
    skel1.fingerprint = skel1.compute_fingerprint();

    let mut skel2 = FileSkeleton {
        file_name: "test.ts".to_string(),
        is_external_module: true,
        symbols: vec![sym_with_heritage],
        global_augmentations: vec![],
        module_augmentations: vec![],
        augmentation_targets: vec![],
        reexports: vec![],
        wildcard_reexports: vec![],
        expando_properties: vec![],
        declared_modules: vec![],
        shorthand_ambient_modules: vec![],
        module_export_specifiers: vec![],
        module_exports_entries: vec![],
        import_sources: vec![],
        file_features: Default::default(),
        fingerprint: 0,
    };
    skel2.fingerprint = skel2.compute_fingerprint();

    assert_ne!(
        skel1.fingerprint, skel2.fingerprint,
        "Heritage fingerprint should change the skeleton fingerprint"
    );
}

#[test]
fn heritage_fingerprint_included_in_skeleton_symbol_hash() {
    use std::hash::{Hash, Hasher};

    let sym1 = SkeletonSymbol {
        name: "Foo".to_string(),
        flags: 0,
        is_exported: false,
        declaration_count: 1,
        has_exports: false,
        has_members: false,
        is_lib_origin: false,
        is_import_alias: false,
        import_module: None,
        heritage_fingerprint: 0,
        heritage_count: 0,
    };
    let sym2 = SkeletonSymbol {
        heritage_fingerprint: 99,
        heritage_count: 2,
        ..sym1.clone()
    };

    let hash_of = |sym: &SkeletonSymbol| {
        let mut h = rustc_hash::FxHasher::default();
        sym.hash(&mut h);
        h.finish()
    };

    assert_ne!(
        hash_of(&sym1),
        hash_of(&sym2),
        "Different heritage fingerprints should produce different hashes"
    );
}

#[test]
fn diff_detects_heritage_change() {
    // Heritage name change should be detected as a changed file.
    let sym1 = SkeletonSymbol {
        name: "Foo".to_string(),
        flags: 0,
        is_exported: true,
        declaration_count: 1,
        has_exports: false,
        has_members: false,
        is_lib_origin: false,
        is_import_alias: false,
        import_module: None,
        heritage_fingerprint: 10,
        heritage_count: 1,
    };
    let sym2 = SkeletonSymbol {
        heritage_fingerprint: 20,
        heritage_count: 1,
        ..sym1.clone()
    };

    let mut prev = FileSkeleton {
        file_name: "a.ts".to_string(),
        is_external_module: true,
        symbols: vec![sym1],
        global_augmentations: vec![],
        module_augmentations: vec![],
        augmentation_targets: vec![],
        reexports: vec![],
        wildcard_reexports: vec![],
        expando_properties: vec![],
        declared_modules: vec![],
        shorthand_ambient_modules: vec![],
        module_export_specifiers: vec![],
        module_exports_entries: vec![],
        import_sources: vec![],
        file_features: Default::default(),
        fingerprint: 0,
    };
    prev.fingerprint = prev.compute_fingerprint();

    let mut curr = FileSkeleton {
        file_name: "a.ts".to_string(),
        is_external_module: true,
        symbols: vec![sym2],
        global_augmentations: vec![],
        module_augmentations: vec![],
        augmentation_targets: vec![],
        reexports: vec![],
        wildcard_reexports: vec![],
        expando_properties: vec![],
        declared_modules: vec![],
        shorthand_ambient_modules: vec![],
        module_export_specifiers: vec![],
        module_exports_entries: vec![],
        import_sources: vec![],
        file_features: Default::default(),
        fingerprint: 0,
    };
    curr.fingerprint = curr.compute_fingerprint();

    let diff = diff_skeletons(&[prev], &[curr]);
    assert_eq!(
        diff.changed,
        vec!["a.ts"],
        "Heritage name change should be detected"
    );
}

// -------------------------------------------------------------------------
// Phase 2 step 1: ambient module resolution served from SkeletonIndex alone.
//
// The CLI driver's `module_resolver.lookup` `is_ambient_module` closure used to
// read `MergedProgram.declared_modules` and `MergedProgram.shorthand_ambient_modules`
// directly. The migrated path routes through `SkeletonIndex::is_ambient_module`,
// which means the consumer can answer the lookup without retaining any arena
// / binder state from the legacy merge. These tests prove that.
// -------------------------------------------------------------------------

/// Helper: build a `SkeletonIndex` with the given ambient module declarations.
/// Constructs a skeleton WITHOUT any `MergedProgram` involvement, demonstrating
/// that `is_ambient_module` is fully arena-free.
fn skeleton_index_with_ambient_modules(declared: &[&str], shorthand: &[&str]) -> SkeletonIndex {
    let skel = FileSkeleton {
        file_name: "ambient.d.ts".to_string(),
        is_external_module: false,
        symbols: vec![],
        global_augmentations: vec![],
        module_augmentations: vec![],
        augmentation_targets: vec![],
        reexports: vec![],
        wildcard_reexports: vec![],
        expando_properties: vec![],
        declared_modules: declared.iter().map(|s| (*s).to_string()).collect(),
        shorthand_ambient_modules: shorthand.iter().map(|s| (*s).to_string()).collect(),
        module_export_specifiers: vec![],
        module_exports_entries: vec![],
        import_sources: vec![],
        file_features: Default::default(),
        fingerprint: 0,
    };
    reduce_skeletons(&[skel])
}

#[test]
fn is_ambient_module_matches_declared_modules() {
    let idx = skeleton_index_with_ambient_modules(&["my-lib", "react"], &[]);
    assert!(idx.is_ambient_module("my-lib"));
    assert!(idx.is_ambient_module("react"));
    assert!(!idx.is_ambient_module("not-declared"));
}

#[test]
fn is_ambient_module_matches_shorthand_modules() {
    let idx = skeleton_index_with_ambient_modules(&[], &["*.json", "shorthand-only"]);
    assert!(idx.is_ambient_module("*.json"));
    assert!(idx.is_ambient_module("shorthand-only"));
    assert!(!idx.is_ambient_module("react"));
}

#[test]
fn is_ambient_module_unions_both_sets() {
    // Mixed: some names from declared_modules, some from shorthand.
    let idx = skeleton_index_with_ambient_modules(&["with-body"], &["bodyless"]);
    assert!(
        idx.is_ambient_module("with-body"),
        "declared_modules entry should be detected"
    );
    assert!(
        idx.is_ambient_module("bodyless"),
        "shorthand_ambient_modules entry should be detected"
    );
    assert!(!idx.is_ambient_module("neither"));
}

#[test]
fn is_ambient_module_returns_false_on_empty_index() {
    let idx = skeleton_index_with_ambient_modules(&[], &[]);
    assert!(!idx.is_ambient_module("anything"));
}

#[test]
fn is_ambient_module_uses_exact_match_no_normalization() {
    // The legacy MergedProgram.declared_modules stores names without quotes
    // (the binder strips quotes before insertion), so the skeleton's contains
    // check must be exact-match in the same encoding. Quoted strings should
    // NOT match unquoted entries (and vice versa).
    let idx = skeleton_index_with_ambient_modules(&["my-lib"], &[]);
    assert!(idx.is_ambient_module("my-lib"));
    assert!(
        !idx.is_ambient_module("\"my-lib\""),
        "raw quoted string must not match unquoted declared name (parity with legacy semantics)"
    );
}

#[test]
fn is_ambient_module_consumer_works_after_legacy_fields_emptied() {
    // Phase 5 scenario: the consumer must still produce the correct answer
    // when the legacy MergedProgram fields are evicted/empty. We model this
    // by constructing `SkeletonIndex` directly (no MergedProgram involvement)
    // and verifying the consumer-shaped closure (mirroring the CLI driver's
    // `is_ambient_module` closure) returns the right answer.
    let idx = skeleton_index_with_ambient_modules(&["my-lib"], &["*.css"]);

    // Mirror the CLI driver's consumer closure (post-migration shape):
    //   |spec| skeleton.is_ambient_module(spec)
    let consumer = |spec: &str| -> bool { idx.is_ambient_module(spec) };

    assert!(
        consumer("my-lib"),
        "declared module must be visible to consumer"
    );
    assert!(
        consumer("*.css"),
        "shorthand ambient must be visible to consumer"
    );
    assert!(!consumer("not-ambient"));
}

#[test]
fn is_ambient_module_aggregates_across_files() {
    // The reducer unions declared_modules and shorthand_ambient_modules from
    // every input skeleton. The consumer must see the cross-file union.
    let skel_a = FileSkeleton {
        file_name: "a.d.ts".to_string(),
        is_external_module: false,
        symbols: vec![],
        global_augmentations: vec![],
        module_augmentations: vec![],
        augmentation_targets: vec![],
        reexports: vec![],
        wildcard_reexports: vec![],
        expando_properties: vec![],
        declared_modules: vec!["from-a".to_string()],
        shorthand_ambient_modules: vec![],
        module_export_specifiers: vec![],
        module_exports_entries: vec![],
        import_sources: vec![],
        file_features: Default::default(),
        fingerprint: 0,
    };
    let skel_b = FileSkeleton {
        file_name: "b.d.ts".to_string(),
        is_external_module: false,
        symbols: vec![],
        global_augmentations: vec![],
        module_augmentations: vec![],
        augmentation_targets: vec![],
        reexports: vec![],
        wildcard_reexports: vec![],
        expando_properties: vec![],
        declared_modules: vec![],
        shorthand_ambient_modules: vec!["from-b".to_string()],
        module_export_specifiers: vec![],
        module_exports_entries: vec![],
        import_sources: vec![],
        file_features: Default::default(),
        fingerprint: 0,
    };
    let idx = reduce_skeletons(&[skel_a, skel_b]);
    assert!(idx.is_ambient_module("from-a"));
    assert!(idx.is_ambient_module("from-b"));
    assert!(!idx.is_ambient_module("from-neither"));
}

// -------------------------------------------------------------------------
// Phase 2 step 2 / step 3: module-augmentations and augmentation-targets
// indexes served from SkeletonIndex.
//
// The checker's `global_module_augmentations_index` was previously built
// by iterating every binder's `module_augmentations` map. Phase 2 step 2
// moves the build to `SkeletonIndex::module_augmentations_for(...)` /
// `build_module_augmentations_index(...)`.
//
// The checker's `global_augmentation_targets_index` was previously built
// by iterating every binder's `augmentation_target_modules` map. Phase 2
// step 3 moves the build to `SkeletonIndex::augmentation_targets_for(...)` /
// `build_augmentation_targets_index(...)`.
//
// Both let the checker rebuild the merged index from skeleton data alone
// — required for Phase 5 (arena eviction).
// -------------------------------------------------------------------------

/// Helper: build a skeleton with the given module-augmentation entries.
fn skeleton_with_module_augmentations(file_name: &str, augs: Vec<ModuleAugEntry>) -> FileSkeleton {
    let module_augmentations: Vec<SkeletonAugmentation> = augs
        .into_iter()
        .map(|(target, decls)| SkeletonAugmentation {
            target,
            declaration_count: decls.len() as u32,
            declarations: decls
                .into_iter()
                .map(|(name, pos, end)| SkeletonAugmentationDecl {
                    name,
                    location: StableLocation::with_unassigned_file(pos, end),
                })
                .collect(),
        })
        .collect();
    let mut skel = FileSkeleton {
        file_name: file_name.to_string(),
        is_external_module: true,
        symbols: vec![],
        global_augmentations: vec![],
        module_augmentations,
        augmentation_targets: vec![],
        reexports: vec![],
        wildcard_reexports: vec![],
        expando_properties: vec![],
        declared_modules: vec![],
        shorthand_ambient_modules: vec![],
        module_export_specifiers: vec![],
        module_exports_entries: vec![],
        import_sources: vec![],
        file_features: Default::default(),
        fingerprint: 0,
    };
    skel.fingerprint = skel.compute_fingerprint();
    skel
}

/// Helper: build a skeleton with the given augmentation-target entries.
/// Each tuple is `(symbol_id, module_spec, pos, end)`.
fn skeleton_with_augmentation_targets(
    file_name: &str,
    targets: Vec<(u32, String, u32, u32)>,
) -> FileSkeleton {
    let augmentation_targets: Vec<SkeletonAugmentationTarget> = targets
        .into_iter()
        .map(
            |(sym_id, module_spec, pos, end)| SkeletonAugmentationTarget {
                symbol_id: tsz_binder::SymbolId(sym_id),
                module_spec,
                stable_location: StableLocation::with_unassigned_file(pos, end),
            },
        )
        .collect();
    let mut skel = FileSkeleton {
        file_name: file_name.to_string(),
        is_external_module: true,
        symbols: vec![],
        global_augmentations: vec![],
        module_augmentations: vec![],
        augmentation_targets,
        reexports: vec![],
        wildcard_reexports: vec![],
        expando_properties: vec![],
        declared_modules: vec![],
        shorthand_ambient_modules: vec![],
        module_export_specifiers: vec![],
        module_exports_entries: vec![],
        import_sources: vec![],
        file_features: Default::default(),
        fingerprint: 0,
    };
    skel.fingerprint = skel.compute_fingerprint();
    skel
}

#[test]
fn skeleton_reduction_capacities_count_distinct_map_keys() {
    let mut skel_a = skeleton_with_module_augmentations(
        "a.ts",
        vec![("./shared".to_string(), vec![("Foo".to_string(), 10, 20)])],
    );
    skel_a.global_augmentations = vec![SkeletonAugmentation {
        target: "global".to_string(),
        declaration_count: 1,
        declarations: vec![],
    }];
    skel_a.augmentation_targets = vec![SkeletonAugmentationTarget {
        symbol_id: tsz_binder::SymbolId(1),
        module_spec: "./shared".to_string(),
        stable_location: StableLocation::with_unassigned_file(10, 20),
    }];

    let mut skel_b = skeleton_with_module_augmentations(
        "b.ts",
        vec![
            ("./shared".to_string(), vec![("Bar".to_string(), 30, 40)]),
            ("./other".to_string(), vec![("Baz".to_string(), 50, 60)]),
        ],
    );
    skel_b.global_augmentations = vec![SkeletonAugmentation {
        target: "global".to_string(),
        declaration_count: 1,
        declarations: vec![],
    }];
    skel_b.augmentation_targets = vec![
        SkeletonAugmentationTarget {
            symbol_id: tsz_binder::SymbolId(2),
            module_spec: "./shared".to_string(),
            stable_location: StableLocation::with_unassigned_file(30, 40),
        },
        SkeletonAugmentationTarget {
            symbol_id: tsz_binder::SymbolId(3),
            module_spec: "./other".to_string(),
            stable_location: StableLocation::with_unassigned_file(50, 60),
        },
    ];

    let capacities = SkeletonReductionCapacities::from_skeletons(&[skel_a, skel_b]);

    assert_eq!(capacities.global_augmentations, 1);
    assert_eq!(capacities.module_augmentations, 2);
    assert_eq!(capacities.augmentation_targets, 2);
}

#[test]
fn module_augmentations_for_returns_per_file_entries() {
    let skel_a = skeleton_with_module_augmentations(
        "a.ts",
        vec![("./shared".to_string(), vec![("Foo".to_string(), 10, 20)])],
    );
    let skel_b = skeleton_with_module_augmentations(
        "b.ts",
        vec![("./shared".to_string(), vec![("Bar".to_string(), 30, 40)])],
    );
    let idx = reduce_skeletons(&[skel_a, skel_b]);

    let entries = idx.module_augmentations_for("./shared");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0, 0);
    assert_eq!(entries[0].1.declarations[0].name, "Foo");
    assert_eq!(entries[1].0, 1);
    assert_eq!(entries[1].1.declarations[0].name, "Bar");
}

#[test]
fn module_augmentations_for_returns_empty_for_unknown_spec() {
    let idx = reduce_skeletons(&[]);
    assert!(idx.module_augmentations_for("./nope").is_empty());
}

#[test]
fn module_augmentations_stamps_file_idx_into_locations() {
    // The reducer must stamp each declaration's StableLocation with the
    // owning file index so post-Phase-5 consumers can route through
    // `node_at_stable_location` without a separate file_idx arg.
    let skel_a = skeleton_with_module_augmentations(
        "a.ts",
        vec![("./m".to_string(), vec![("X".to_string(), 5, 12)])],
    );
    let skel_b = skeleton_with_module_augmentations(
        "b.ts",
        vec![("./m".to_string(), vec![("Y".to_string(), 100, 200)])],
    );
    let idx = reduce_skeletons(&[skel_a, skel_b]);
    let entries = idx.module_augmentations_for("./m");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].1.declarations[0].location.file_idx, 0);
    assert_eq!(entries[1].1.declarations[0].location.file_idx, 1);
}

#[test]
fn module_augmentations_consumer_works_after_legacy_program_emptied() {
    // Phase 5 invariant: the checker-side merged map must be reproducible
    // from `SkeletonIndex` alone, even if the legacy `MergedProgram`'s
    // per-binder `module_augmentations` field has been emptied.
    //
    // We model the post-eviction state by building the index using only
    // the skeleton (and a per-file arena placeholder) — no MergedProgram
    // and no per-binder loop. The expected (spec, file_idx, name) set
    // recovered from the skeleton must match what the legacy loop would
    // have produced for the same inputs.
    let skel_a = skeleton_with_module_augmentations(
        "a.ts",
        vec![(
            "./shared".to_string(),
            vec![("Alpha".to_string(), 10, 20), ("Beta".to_string(), 25, 35)],
        )],
    );
    let skel_b = skeleton_with_module_augmentations(
        "b.ts",
        vec![("./shared".to_string(), vec![("Gamma".to_string(), 0, 5)])],
    );
    let skeletons = vec![skel_a, skel_b];
    let idx = reduce_skeletons(&skeletons);

    // Recover the legacy `(spec, file_idx, name)` triples directly from
    // the skeleton accessor — no MergedProgram involvement.
    let mut recovered: Vec<(String, usize, String)> = Vec::new();
    for (spec, entries) in &idx.module_augmentations_by_spec {
        for (file_idx, aug) in entries {
            for decl in &aug.declarations {
                recovered.push((spec.clone(), *file_idx, decl.name.clone()));
            }
        }
    }
    recovered.sort();

    let mut expected: Vec<(String, usize, String)> = vec![
        ("./shared".to_string(), 0, "Alpha".to_string()),
        ("./shared".to_string(), 0, "Beta".to_string()),
        ("./shared".to_string(), 1, "Gamma".to_string()),
    ];
    expected.sort();

    assert_eq!(
        recovered, expected,
        "Skeleton-only recovery must reproduce legacy per-binder topology"
    );
}

#[test]
fn build_module_augmentations_index_matches_legacy_topology() {
    // Cross-check `build_module_augmentations_index` against the
    // skeleton's per-spec data: every (spec, file_idx, name) triple
    // recorded in the skeleton must surface in the rebuilt map.
    let skel_a = skeleton_with_module_augmentations(
        "a.ts",
        vec![(
            "./mod".to_string(),
            vec![("First".to_string(), 1, 2), ("Second".to_string(), 3, 4)],
        )],
    );
    let skel_b = skeleton_with_module_augmentations(
        "b.ts",
        vec![("./mod".to_string(), vec![("Third".to_string(), 5, 6)])],
    );
    let idx = reduce_skeletons(&[skel_a, skel_b]);

    // Empty arenas: the helper should still produce the right (spec,
    // file_idx, name) topology with NodeIndex::NONE for unresolved nodes.
    let arenas: Vec<std::sync::Arc<tsz_parser::parser::node::NodeArena>> = Vec::new();
    let map = idx.build_module_augmentations_index(&arenas);

    let mut got: Vec<(String, usize, String)> = Vec::new();
    for (spec, entries) in &map {
        for (file_idx, aug) in entries {
            got.push((spec.clone(), *file_idx, aug.name.clone()));
        }
    }
    got.sort();
    let mut want: Vec<(String, usize, String)> = vec![
        ("./mod".to_string(), 0, "First".to_string()),
        ("./mod".to_string(), 0, "Second".to_string()),
        ("./mod".to_string(), 1, "Third".to_string()),
    ];
    want.sort();
    assert_eq!(got, want);
}

#[test]
fn augmentation_targets_for_returns_per_file_entries() {
    let skel_a =
        skeleton_with_augmentation_targets("a.ts", vec![(7, "./shared".to_string(), 10, 20)]);
    let skel_b =
        skeleton_with_augmentation_targets("b.ts", vec![(11, "./shared".to_string(), 30, 40)]);
    let idx = reduce_skeletons(&[skel_a, skel_b]);

    let entries = idx.augmentation_targets_for("./shared");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0, 0);
    assert_eq!(entries[0].1.symbol_id, tsz_binder::SymbolId(7));
    assert_eq!(entries[1].0, 1);
    assert_eq!(entries[1].1.symbol_id, tsz_binder::SymbolId(11));
}

#[test]
fn augmentation_targets_for_returns_empty_for_unknown_spec() {
    let idx = reduce_skeletons(&[]);
    assert!(idx.augmentation_targets_for("./nope").is_empty());
}

#[test]
fn augmentation_targets_stamps_file_idx_into_locations() {
    // The reducer must stamp each entry's StableLocation with the owning
    // file index so post-Phase-5 consumers can route through
    // `node_at_stable_location` without a separate file_idx arg.
    let skel_a = skeleton_with_augmentation_targets("a.ts", vec![(3, "./m".to_string(), 5, 12)]);
    let skel_b = skeleton_with_augmentation_targets("b.ts", vec![(4, "./m".to_string(), 100, 200)]);
    let idx = reduce_skeletons(&[skel_a, skel_b]);
    let entries = idx.augmentation_targets_for("./m");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].1.stable_location.file_idx, 0);
    assert_eq!(entries[1].1.stable_location.file_idx, 1);
    // Per-file pos/end preserved.
    assert_eq!(entries[0].1.stable_location.pos, 5);
    assert_eq!(entries[0].1.stable_location.end, 12);
    assert_eq!(entries[1].1.stable_location.pos, 100);
    assert_eq!(entries[1].1.stable_location.end, 200);
}

#[test]
fn augmentation_targets_consumer_works_after_legacy_program_emptied() {
    // Phase 5 invariant: the checker-side merged map must be reproducible
    // from `SkeletonIndex` alone, even if the legacy `MergedProgram`'s
    // per-binder `augmentation_target_modules` field has been emptied.
    //
    // We model the post-eviction state by building the index using only
    // the skeleton — no MergedProgram and no per-binder loop. The expected
    // (spec, sym_id, file_idx) set recovered from the skeleton must match
    // what the legacy loop would have produced for the same inputs.
    let skel_a = skeleton_with_augmentation_targets(
        "a.ts",
        vec![
            (1, "./shared".to_string(), 10, 20),
            (2, "./shared".to_string(), 25, 35),
        ],
    );
    let skel_b =
        skeleton_with_augmentation_targets("b.ts", vec![(3, "./shared".to_string(), 0, 5)]);
    let skeletons = vec![skel_a, skel_b];
    let idx = reduce_skeletons(&skeletons);

    // Recover the legacy `(spec, sym_id, file_idx)` triples directly from
    // the skeleton accessor — no MergedProgram involvement.
    let mut recovered: Vec<(String, u32, usize)> = Vec::new();
    for (spec, entries) in &idx.augmentation_targets_by_spec {
        for (file_idx, target) in entries {
            recovered.push((spec.clone(), target.symbol_id.0, *file_idx));
        }
    }
    recovered.sort();

    let mut expected: Vec<(String, u32, usize)> = vec![
        ("./shared".to_string(), 1, 0),
        ("./shared".to_string(), 2, 0),
        ("./shared".to_string(), 3, 1),
    ];
    expected.sort();

    assert_eq!(
        recovered, expected,
        "Skeleton-only recovery must reproduce legacy per-binder topology"
    );
}

#[test]
fn build_augmentation_targets_index_matches_legacy_topology() {
    // Cross-check `build_augmentation_targets_index` against the
    // skeleton's per-spec data: every (spec, sym_id, file_idx) triple
    // recorded in the skeleton must surface in the rebuilt map in the
    // legacy `Vec<(SymbolId, file_idx)>` shape.
    let skel_a = skeleton_with_augmentation_targets(
        "a.ts",
        vec![
            (5, "./mod".to_string(), 1, 2),
            (6, "./mod".to_string(), 3, 4),
        ],
    );
    let skel_b = skeleton_with_augmentation_targets("b.ts", vec![(7, "./mod".to_string(), 5, 6)]);
    let idx = reduce_skeletons(&[skel_a, skel_b]);

    let map = idx.build_augmentation_targets_index();

    let mut got: Vec<(String, u32, usize)> = Vec::new();
    for (spec, entries) in &map {
        for (sym_id, file_idx) in entries {
            got.push((spec.clone(), sym_id.0, *file_idx));
        }
    }
    got.sort();
    let mut want: Vec<(String, u32, usize)> = vec![
        ("./mod".to_string(), 5, 0),
        ("./mod".to_string(), 6, 0),
        ("./mod".to_string(), 7, 1),
    ];
    want.sort();
    assert_eq!(got, want);
}

// -------------------------------------------------------------------------
// Phase 2 step 4: module-binder index served from SkeletonIndex.
//
// The checker's `global_module_binder_index` was previously built by
// iterating every binder's `module_exports` map. Phase 2 step 4 moves the
// build to `SkeletonIndex::module_binders_for(...)` /
// `build_module_binder_index(...)`, letting the checker rebuild the
// merged index from skeleton data alone — required for Phase 5 (arena
// eviction).
// -------------------------------------------------------------------------

/// Helper: build a skeleton with the given module-export specifier list.
/// `specs` carries the raw (possibly quoted) module specifier keys, exactly
/// as the binder records them in `module_exports.keys()`.
fn skeleton_with_module_export_specifiers(file_name: &str, specs: Vec<String>) -> FileSkeleton {
    let mut sorted = specs;
    sorted.sort();
    let mut skel = FileSkeleton {
        file_name: file_name.to_string(),
        is_external_module: true,
        symbols: vec![],
        global_augmentations: vec![],
        module_augmentations: vec![],
        augmentation_targets: vec![],
        reexports: vec![],
        wildcard_reexports: vec![],
        expando_properties: vec![],
        declared_modules: vec![],
        shorthand_ambient_modules: vec![],
        module_export_specifiers: sorted,
        module_exports_entries: vec![],
        import_sources: vec![],
        file_features: Default::default(),
        fingerprint: 0,
    };
    skel.fingerprint = skel.compute_fingerprint();
    skel
}

#[test]
fn module_binders_for_returns_per_file_indices() {
    let skel_a = skeleton_with_module_export_specifiers("a.ts", vec!["my-lib".to_string()]);
    let skel_b = skeleton_with_module_export_specifiers("b.ts", vec!["my-lib".to_string()]);
    let skel_c = skeleton_with_module_export_specifiers("c.ts", vec!["other".to_string()]);
    let idx = reduce_skeletons(&[skel_a, skel_b, skel_c]);

    let mut got = idx.module_binders_for("my-lib").to_vec();
    got.sort();
    assert_eq!(got, vec![0, 1]);

    let other = idx.module_binders_for("other").to_vec();
    assert_eq!(other, vec![2]);
}

#[test]
fn module_binders_for_returns_empty_for_unknown_spec() {
    let idx = reduce_skeletons(&[]);
    assert!(idx.module_binders_for("./nope").is_empty());
}

#[test]
fn module_binders_records_normalized_variant_when_quoted() {
    // The legacy per-binder loop pushes file_idx for both the raw spec
    // (e.g. `"\"my-lib\""`) and its de-quoted form (`"my-lib"`).
    let skel = skeleton_with_module_export_specifiers("a.ts", vec!["\"my-lib\"".to_string()]);
    let idx = reduce_skeletons(&[skel]);

    let raw = idx.module_binders_for("\"my-lib\"").to_vec();
    assert_eq!(raw, vec![0]);

    let normalized = idx.module_binders_for("my-lib").to_vec();
    assert_eq!(normalized, vec![0]);
}

#[test]
fn module_binders_consumer_works_after_legacy_program_emptied() {
    // Phase 5 invariant: the checker-side merged map must be reproducible
    // from `SkeletonIndex` alone, even if the legacy `MergedProgram`'s
    // per-binder `module_exports` field has been emptied.
    //
    // We model the post-eviction state by building the index using only
    // the skeleton — no MergedProgram and no per-binder loop. The expected
    // (spec, file_idx) set recovered from the skeleton must match what the
    // legacy loop would have produced for the same inputs.
    let skel_a = skeleton_with_module_export_specifiers(
        "a.ts",
        vec!["\"shared\"".to_string(), "other".to_string()],
    );
    let skel_b = skeleton_with_module_export_specifiers("b.ts", vec!["\"shared\"".to_string()]);
    let skeletons = vec![skel_a, skel_b];
    let idx = reduce_skeletons(&skeletons);

    // Recover the legacy `(spec, file_idx)` pairs directly from the
    // skeleton accessor — no MergedProgram involvement.
    let mut recovered: Vec<(String, usize)> = Vec::new();
    for (spec, files) in &idx.module_binder_index_by_spec {
        for f in files {
            recovered.push((spec.clone(), *f));
        }
    }
    recovered.sort();

    let mut expected: Vec<(String, usize)> = vec![
        // Raw quoted entries:
        ("\"shared\"".to_string(), 0),
        ("\"shared\"".to_string(), 1),
        // Normalized entries (de-quoted):
        ("shared".to_string(), 0),
        ("shared".to_string(), 1),
        // Unquoted spec only contributes once:
        ("other".to_string(), 0),
    ];
    expected.sort();

    assert_eq!(
        recovered, expected,
        "Skeleton-only recovery must reproduce legacy per-binder topology"
    );
}

#[test]
fn build_module_binder_index_matches_legacy_topology() {
    // Cross-check `build_module_binder_index` against the skeleton's
    // per-spec data: every (spec, file_idx) pair recorded in the skeleton
    // must surface in the rebuilt map, including the de-quoted normalized
    // variant.
    let skel_a = skeleton_with_module_export_specifiers("a.ts", vec!["\"my-lib\"".to_string()]);
    let skel_b = skeleton_with_module_export_specifiers("b.ts", vec!["\"other\"".to_string()]);
    let idx = reduce_skeletons(&[skel_a, skel_b]);

    let map = idx.build_module_binder_index();

    let mut got: Vec<(String, usize)> = Vec::new();
    for (spec, files) in &map {
        for f in files {
            got.push((spec.clone(), *f));
        }
    }
    got.sort();
    let mut want: Vec<(String, usize)> = vec![
        ("\"my-lib\"".to_string(), 0),
        ("my-lib".to_string(), 0),
        ("\"other\"".to_string(), 1),
        ("other".to_string(), 1),
    ];
    want.sort();
    assert_eq!(got, want);
}

// -------------------------------------------------------------------------
// Phase 2 step 6: module-exports index served from SkeletonIndex.
//
// The checker's `global_module_exports_index` was previously built by
// iterating every binder's `module_exports[spec].iter()` map. Phase 2 step
// 6 moves the build to `SkeletonIndex::module_exports_for(...)` /
// `build_module_exports_index(merged_module_exports)`, letting the checker
// rebuild the merged index from skeleton data plus the post-merge
// `program.module_exports` map alone — required for Phase 5 (arena
// eviction). SymbolIds are resolved at projection time from the post-merge
// map (which holds globally-remapped IDs), avoiding the pre-merge local-
// SymbolId trap that regressed PR #1145.
// -------------------------------------------------------------------------

/// Helper: build a skeleton with the given `(spec, [export_name])` entries.
fn skeleton_with_module_exports_entries(
    file_name: &str,
    entries: Vec<(String, Vec<String>)>,
) -> FileSkeleton {
    let mut sorted_entries = entries;
    sorted_entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (_, names) in &mut sorted_entries {
        names.sort();
    }
    let module_export_specifiers: Vec<String> = sorted_entries
        .iter()
        .map(|(spec, _)| spec.clone())
        .collect();
    let mut skel = FileSkeleton {
        file_name: file_name.to_string(),
        is_external_module: true,
        symbols: vec![],
        global_augmentations: vec![],
        module_augmentations: vec![],
        augmentation_targets: vec![],
        reexports: vec![],
        wildcard_reexports: vec![],
        expando_properties: vec![],
        declared_modules: vec![],
        shorthand_ambient_modules: vec![],
        module_export_specifiers,
        module_exports_entries: sorted_entries,
        import_sources: vec![],
        file_features: Default::default(),
        fingerprint: 0,
    };
    skel.fingerprint = skel.compute_fingerprint();
    skel
}

#[test]
fn module_exports_for_returns_per_file_indices_per_export() {
    let skel_a = skeleton_with_module_exports_entries(
        "a.ts",
        vec![(
            "my-lib".to_string(),
            vec!["foo".to_string(), "bar".to_string()],
        )],
    );
    let skel_b = skeleton_with_module_exports_entries(
        "b.ts",
        vec![("my-lib".to_string(), vec!["foo".to_string()])],
    );
    let skel_c = skeleton_with_module_exports_entries(
        "c.ts",
        vec![("other".to_string(), vec!["baz".to_string()])],
    );
    let idx = reduce_skeletons(&[skel_a, skel_b, skel_c]);

    let by_name = idx.module_exports_for("my-lib").unwrap();
    let mut foo = by_name["foo"].clone();
    foo.sort();
    assert_eq!(foo, vec![0, 1]);
    let bar = by_name["bar"].clone();
    assert_eq!(bar, vec![0]);

    let other = idx.module_exports_for("other").unwrap();
    assert_eq!(other["baz"], vec![2]);

    assert!(idx.module_exports_for("nope").is_none());
}

#[test]
fn module_exports_records_normalized_variant_when_quoted() {
    let skel = skeleton_with_module_exports_entries(
        "a.ts",
        vec![("\"my-lib\"".to_string(), vec!["foo".to_string()])],
    );
    let idx = reduce_skeletons(&[skel]);

    let raw = idx.module_exports_for("\"my-lib\"").unwrap();
    assert_eq!(raw["foo"], vec![0]);

    let normalized = idx.module_exports_for("my-lib").unwrap();
    assert_eq!(normalized["foo"], vec![0]);
}

#[test]
fn build_module_exports_index_resolves_sym_ids_from_merged_map() {
    // Two files declare overlapping exports under the same spec.
    let skel_a = skeleton_with_module_exports_entries(
        "a.ts",
        vec![("\"my-lib\"".to_string(), vec!["foo".to_string()])],
    );
    let skel_b = skeleton_with_module_exports_entries(
        "b.ts",
        vec![(
            "\"my-lib\"".to_string(),
            vec!["foo".to_string(), "bar".to_string()],
        )],
    );
    let idx = reduce_skeletons(&[skel_a, skel_b]);

    // Build a fake merged map: post-merge SymbolIds for the (spec,
    // export_name) pairs. The merge typically picks one SymbolId per
    // (spec, export_name) — the projection pairs every recorded
    // file_idx with that single id.
    let mut merged: FxHashMap<String, SymbolTable> = FxHashMap::default();
    let mut shared = SymbolTable::new();
    shared.set("foo".to_string(), SymbolId(101));
    shared.set("bar".to_string(), SymbolId(102));
    merged.insert("\"my-lib\"".to_string(), shared);

    let projected = idx.build_module_exports_index(&merged);

    // Raw spec key resolves to (file_idx, sym_id) entries for each export.
    let raw = &projected["\"my-lib\""];
    let mut foo_entries = raw["foo"].clone();
    foo_entries.sort_by_key(|(f, _)| *f);
    assert_eq!(
        foo_entries,
        vec![(0, SymbolId(101)), (1, SymbolId(101))],
        "foo should appear under both files with the merged sym_id"
    );
    let bar_entries = raw["bar"].clone();
    assert_eq!(
        bar_entries,
        vec![(1, SymbolId(102))],
        "bar should appear only under file 1 with the merged sym_id"
    );

    // The de-quoted normalized variant also resolves (the projection
    // falls back to the raw merged-map key when the normalized one is
    // missing, mirroring the legacy lookup).
    let normalized = &projected["my-lib"];
    let mut foo_norm = normalized["foo"].clone();
    foo_norm.sort_by_key(|(f, _)| *f);
    assert_eq!(foo_norm, vec![(0, SymbolId(101)), (1, SymbolId(101))]);
}

#[test]
fn build_module_exports_index_drops_entries_missing_from_merged_map() {
    // The skeleton recorded an export name that did not survive the
    // merge (e.g., its pre-merge SymbolId was not in `id_remap`). The
    // projection must drop it — same as the legacy `remap_symbol_table`
    // filter.
    let skel = skeleton_with_module_exports_entries(
        "a.ts",
        vec![(
            "my-lib".to_string(),
            vec!["foo".to_string(), "ghost".to_string()],
        )],
    );
    let idx = reduce_skeletons(&[skel]);

    let mut merged: FxHashMap<String, SymbolTable> = FxHashMap::default();
    let mut tbl = SymbolTable::new();
    tbl.set("foo".to_string(), SymbolId(7));
    // "ghost" intentionally absent from the merged map.
    merged.insert("my-lib".to_string(), tbl);

    let projected = idx.build_module_exports_index(&merged);

    let by_name = &projected["my-lib"];
    assert_eq!(by_name["foo"], vec![(0, SymbolId(7))]);
    assert!(
        !by_name.contains_key("ghost"),
        "exports missing from the merged map must be dropped"
    );
}

#[test]
fn build_module_exports_index_drops_specs_missing_from_merged_map() {
    // The skeleton recorded a spec key that was entirely dropped during
    // the merge. The projection must skip it.
    let skel = skeleton_with_module_exports_entries(
        "a.ts",
        vec![("dead-spec".to_string(), vec!["foo".to_string()])],
    );
    let idx = reduce_skeletons(&[skel]);

    let merged: FxHashMap<String, SymbolTable> = FxHashMap::default();
    let projected = idx.build_module_exports_index(&merged);
    assert!(projected.is_empty());
}

#[test]
fn module_exports_consumer_works_after_legacy_program_emptied() {
    // Phase 5 invariant: the checker-side merged map must be reproducible
    // from `SkeletonIndex` + `program.module_exports` alone, even if every
    // per-binder `module_exports` field has been emptied (which is the
    // post-eviction state).
    //
    // We model this by:
    //   1) Building `SkeletonIndex` from skeleton-only inputs (no
    //      MergedProgram, no per-binder loop).
    //   2) Supplying a `merged_module_exports` map that mirrors what
    //      `MergedProgram.module_exports` would carry after merging the
    //      same files.
    //   3) Asserting the projection produces the same `(spec, export_name,
    //      file_idx, sym_id)` set the legacy per-binder loop would have
    //      computed.
    let skel_a = skeleton_with_module_exports_entries(
        "a.ts",
        vec![(
            "\"shared\"".to_string(),
            vec!["foo".to_string(), "bar".to_string()],
        )],
    );
    let skel_b = skeleton_with_module_exports_entries(
        "b.ts",
        vec![("\"shared\"".to_string(), vec!["foo".to_string()])],
    );
    let idx = reduce_skeletons(&[skel_a, skel_b]);

    let mut merged: FxHashMap<String, SymbolTable> = FxHashMap::default();
    let mut shared = SymbolTable::new();
    shared.set("foo".to_string(), SymbolId(50));
    shared.set("bar".to_string(), SymbolId(51));
    merged.insert("\"shared\"".to_string(), shared);

    let projected = idx.build_module_exports_index(&merged);

    // Recover the legacy `(spec, export_name, file_idx, sym_id)` tuples.
    let mut recovered: Vec<(String, String, usize, SymbolId)> = Vec::new();
    for (spec, by_name) in &projected {
        for (name, entries) in by_name {
            for &(file_idx, sym_id) in entries {
                recovered.push((spec.clone(), name.clone(), file_idx, sym_id));
            }
        }
    }
    recovered.sort();

    let mut expected: Vec<(String, String, usize, SymbolId)> = vec![
        // Raw spec key entries:
        ("\"shared\"".to_string(), "foo".to_string(), 0, SymbolId(50)),
        ("\"shared\"".to_string(), "foo".to_string(), 1, SymbolId(50)),
        ("\"shared\"".to_string(), "bar".to_string(), 0, SymbolId(51)),
        // Normalized (de-quoted) entries:
        ("shared".to_string(), "foo".to_string(), 0, SymbolId(50)),
        ("shared".to_string(), "foo".to_string(), 1, SymbolId(50)),
        ("shared".to_string(), "bar".to_string(), 0, SymbolId(51)),
    ];
    expected.sort();

    assert_eq!(
        recovered, expected,
        "Skeleton + merged-map recovery must reproduce legacy per-binder topology"
    );
}
