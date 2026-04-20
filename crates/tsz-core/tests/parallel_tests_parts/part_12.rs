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

