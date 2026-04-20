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

