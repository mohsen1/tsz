#[test]
fn stable_identity_survives_rebind_same_source() {
    // Parsing+binding the same source twice should produce identical DefId
    // structure in the DefinitionStore (same count, same kinds, same names).
    let source = concat!(
        "export class Foo<T> extends Array<T> {}\n",
        "export interface Bar { x: number; }\n",
        "export type Baz = string;\n",
        "export enum Color { Red, Green }\n",
    );

    let make_program = || {
        let files = vec![("test.ts".to_string(), source.to_string())];
        let results = parse_and_bind_parallel(files);
        merge_bind_results(results)
    };

    let p1 = make_program();
    let p2 = make_program();

    // Both runs should produce the same number of semantic_defs
    assert_eq!(
        p1.semantic_defs.len(),
        p2.semantic_defs.len(),
        "semantic_defs count should be identical across rebinds"
    );

    // Both runs should produce the same set of names with same kinds
    let names1: std::collections::BTreeMap<String, crate::binder::SemanticDefKind> = p1
        .semantic_defs
        .values()
        .map(|e| (e.name.clone(), e.kind))
        .collect();
    let names2: std::collections::BTreeMap<String, crate::binder::SemanticDefKind> = p2
        .semantic_defs
        .values()
        .map(|e| (e.name.clone(), e.kind))
        .collect();
    assert_eq!(
        names1, names2,
        "semantic_def names+kinds should be identical across rebinds"
    );

    // Both DefinitionStores should have the same number of entries
    let count1 = p1.definition_store.all_symbol_mappings().len();
    let count2 = p2.definition_store.all_symbol_mappings().len();
    assert_eq!(
        count1, count2,
        "DefinitionStore symbol mapping counts should be identical across rebinds"
    );
}

#[test]
fn stable_identity_cross_file_merge_preserves_all_defs() {
    // Cross-file declaration merging: interface + class across files
    // Both should get DefIds and the interface heritage should be resolved.
    let files = vec![
        (
            "types.ts".to_string(),
            "export interface Base { x: number; }".to_string(),
        ),
        (
            "impl.ts".to_string(),
            concat!(
                "export class Derived { y: string; }\n",
                "export type Alias = string;\n",
                "export enum Status { OK, ERR }\n",
            )
            .to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);
    let store = &program.definition_store;
    let interner = &program.type_interner;

    // All four declarations should have DefIds
    for name in &["Base", "Derived", "Alias", "Status"] {
        let atom = interner.intern_string(name);
        let defs = store.find_defs_by_name(atom);
        assert!(
            defs.is_some() && !defs.as_ref().unwrap().is_empty(),
            "{name} should have DefId(s) in cross-file merge"
        );
    }

    // Both files should have per-file semantic_defs
    let file0_names: std::collections::HashSet<_> = program.files[0]
        .semantic_defs
        .values()
        .map(|e| e.name.as_str())
        .collect();
    let file1_names: std::collections::HashSet<_> = program.files[1]
        .semantic_defs
        .values()
        .map(|e| e.name.as_str())
        .collect();
    assert!(file0_names.contains("Base"), "types.ts should own Base");
    assert!(
        file1_names.contains("Derived"),
        "impl.ts should own Derived"
    );
    assert!(file1_names.contains("Alias"), "impl.ts should own Alias");
    assert!(file1_names.contains("Status"), "impl.ts should own Status");
}

// =============================================================================
// is_declare flag through merge pipeline
// =============================================================================

#[test]
fn is_declare_flag_survives_merge_for_all_families() {
    // Verify that the binder's `is_declare` flag propagates through merge
    // into the merged `semantic_defs` and the shared `DefinitionStore`.
    // Only test families where `declare` is semantically meaningful and
    // captured as a modifier (class, enum, namespace).
    let files = vec![(
        "ambient.ts".to_string(),
        r"
declare class DeclaredClass {}
declare enum DeclaredEnum { A, B }
declare namespace DeclaredNS {}
"
        .to_string(),
    )];

    let bind_results = parse_and_bind_parallel(files);
    let program = merge_bind_results(bind_results);

    // Check that is_declare survived in the merged semantic_defs.
    let find_entry = |name: &str| -> Option<&tsz_binder::SemanticDefEntry> {
        program.semantic_defs.values().find(|e| e.name == name)
    };

    let dc = find_entry("DeclaredClass").expect("Missing DeclaredClass in merged semantic_defs");
    assert!(
        dc.is_declare,
        "DeclaredClass should have is_declare=true after merge"
    );

    let de = find_entry("DeclaredEnum").expect("Missing DeclaredEnum in merged semantic_defs");
    assert!(
        de.is_declare,
        "DeclaredEnum should have is_declare=true after merge"
    );

    let dn = find_entry("DeclaredNS").expect("Missing DeclaredNS in merged semantic_defs");
    assert!(
        dn.is_declare,
        "DeclaredNS should have is_declare=true after merge"
    );

    // Also verify the DefinitionStore has is_declare set correctly.
    let store = &program.definition_store;
    let interner = &program.type_interner;

    let check_store_declare = |name: &str| {
        let atom = interner.intern_string(name);
        let defs = store.find_defs_by_name(atom).unwrap_or_default();
        assert!(!defs.is_empty(), "{name} should have DefId in store");
        for &def_id in &defs {
            let info = store.get(def_id).expect("DefId should have DefinitionInfo");
            assert!(
                info.is_declare,
                "{name} DefinitionInfo should have is_declare=true"
            );
        }
    };

    check_store_declare("DeclaredClass");
    check_store_declare("DeclaredEnum");
    check_store_declare("DeclaredNS");
}

#[test]
fn non_ambient_declarations_have_is_declare_false_after_merge() {
    // Verify that non-ambient declarations have is_declare=false after merge.
    let files = vec![(
        "regular.ts".to_string(),
        r"
export class RegClass {}
export interface RegIface {}
export type RegAlias = number;
export enum RegEnum { X }
export namespace RegNS {}
"
        .to_string(),
    )];

    let bind_results = parse_and_bind_parallel(files);
    let program = merge_bind_results(bind_results);

    for entry in program.semantic_defs.values() {
        assert!(
            !entry.is_declare,
            "{} should have is_declare=false for non-ambient declaration",
            entry.name
        );
    }
}

#[test]
fn semantic_def_identity_stable_across_remerge() {
    // Verify that merging the same files twice produces identical
    // semantic_defs structure (kind, name, arity, flags). This is a
    // fundamental invariant for incremental compilation.
    let files = vec![
        (
            "types.ts".to_string(),
            r"
export class MyClass<T> extends Object {}
export interface MyInterface<A, B> { x: number }
export type MyAlias<X> = X | null;
"
            .to_string(),
        ),
        (
            "values.ts".to_string(),
            r"
export enum MyEnum { Red, Green, Blue }
export namespace MyNS { export type Inner = number }
declare class AmbientClass {}
"
            .to_string(),
        ),
    ];

    // First merge
    let results1 = parse_and_bind_parallel(files.clone());
    let program1 = merge_bind_results(results1);

    // Second merge (fresh parse + bind + merge)
    let results2 = parse_and_bind_parallel(files);
    let program2 = merge_bind_results(results2);

    // Same number of semantic_defs
    assert_eq!(
        program1.semantic_defs.len(),
        program2.semantic_defs.len(),
        "Remerge should produce the same number of semantic_defs"
    );

    // Each entry in program1 should have a match in program2 with same metadata
    for entry1 in program1.semantic_defs.values() {
        let entry2 = program2
            .semantic_defs
            .values()
            .find(|e| e.name == entry1.name)
            .unwrap_or_else(|| panic!("Missing {} after remerge", entry1.name));

        assert_eq!(entry1.kind, entry2.kind, "{}: kind mismatch", entry1.name);
        assert_eq!(
            entry1.type_param_count, entry2.type_param_count,
            "{}: type_param_count mismatch",
            entry1.name
        );
        assert_eq!(
            entry1.is_exported, entry2.is_exported,
            "{}: is_exported mismatch",
            entry1.name
        );
        assert_eq!(
            entry1.is_declare, entry2.is_declare,
            "{}: is_declare mismatch",
            entry1.name
        );
        assert_eq!(
            entry1.is_abstract, entry2.is_abstract,
            "{}: is_abstract mismatch",
            entry1.name
        );
        assert_eq!(
            entry1.extends_names, entry2.extends_names,
            "{}: extends_names mismatch",
            entry1.name
        );
    }

    // DefinitionStore should have the same number of definitions
    let stats1 = program1.definition_store.statistics();
    let stats2 = program2.definition_store.statistics();
    assert_eq!(
        stats1.total_definitions, stats2.total_definitions,
        "DefinitionStore should have same size after remerge"
    );
}

// =============================================================================
// Stable identity tests: solver-owned DefinitionStore::from_semantic_defs
// =============================================================================

/// Verify that `DefinitionStore::from_semantic_defs` (solver factory) produces
/// the same DefId structure as `create_definition_store_from_binder` (core helper).
#[test]
fn solver_from_semantic_defs_matches_core_helper() {
    use tsz_solver::def::{DefKind, DefinitionStore};

    let source = r#"
        export class Animal<T> {}
        export interface Serializable { toJSON(): string; }
        export type ID = string | number;
        export enum Color { Red, Green, Blue }
        export namespace Utils { export function helper(): void {} }
        export function identity<T>(x: T): T { return x; }
        export const VERSION = "1.0";
    "#;

    let parsed = crate::parallel::parse_file_single("test.ts".to_string(), source.to_string());
    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(&parsed.arena, parsed.source_file);

    let interner = tsz_solver::TypeInterner::new();

    // Path A: core helper (delegates to solver factory internally)
    let store_a = crate::parallel::create_definition_store_from_binder(&binder, &interner);

    // Path B: solver factory directly
    let store_b =
        DefinitionStore::from_semantic_defs(&binder.semantic_defs, |s| interner.intern_string(s));

    let stats_a = store_a.statistics();
    let stats_b = store_b.statistics();

    assert_eq!(
        stats_a.total_definitions, stats_b.total_definitions,
        "Both paths should produce the same number of definitions"
    );

    // Verify each declaration family is present in both stores
    let families = [
        ("Animal", DefKind::Class),
        ("Serializable", DefKind::Interface),
        ("ID", DefKind::TypeAlias),
        ("Color", DefKind::Enum),
        ("Utils", DefKind::Namespace),
        ("identity", DefKind::Function),
        ("VERSION", DefKind::Variable),
    ];

    for (name, expected_kind) in &families {
        let name_atom = interner.intern_string(name);
        let def_a = store_a
            .find_defs_by_name(name_atom)
            .and_then(|d: Vec<tsz_solver::def::DefId>| d.first().copied());
        let def_b = store_b
            .find_defs_by_name(name_atom)
            .and_then(|d: Vec<tsz_solver::def::DefId>| d.first().copied());

        assert!(def_a.is_some(), "{name} should exist in store_a");
        assert!(def_b.is_some(), "{name} should exist in store_b");

        let info_a = store_a.get(def_a.unwrap()).unwrap();
        let info_b = store_b.get(def_b.unwrap()).unwrap();
        assert_eq!(
            info_a.kind, *expected_kind,
            "{name} kind mismatch in store_a"
        );
        assert_eq!(
            info_b.kind, *expected_kind,
            "{name} kind mismatch in store_b"
        );
        assert_eq!(
            info_a.type_params.len(),
            info_b.type_params.len(),
            "{name} type_param count mismatch"
        );
    }
}

/// Verify stable identity for all declaration families across merge/rebind.
/// Bind two files, merge, then verify all top-level declarations from both
/// files have stable `DefId`s in the merged `DefinitionStore`.
#[test]
fn stable_identity_survives_multi_file_merge() {
    use tsz_solver::def::DefKind;

    let file_a = r#"
        export class Base<T> { value: T; }
        export interface Printable { print(): void; }
        export type StringOrNumber = string | number;
        export enum Direction { North, South, East, West }
    "#;

    let file_b = r#"
        export class Child extends Base<string> {}
        export interface Loggable { log(): void; }
        export type ID = number;
        export enum Status { Active, Inactive }
    "#;

    let sources = vec![
        ("a.ts".to_string(), file_a.to_string()),
        ("b.ts".to_string(), file_b.to_string()),
    ];

    let program = merge_bind_results(parse_and_bind_parallel(sources));

    let interner = &program.type_interner;
    let store = &program.definition_store;

    // File A declarations
    let check = |name: &str, kind: DefKind| {
        let atom = interner.intern_string(name);
        let def = store
            .find_defs_by_name(atom)
            .and_then(|d: Vec<tsz_solver::def::DefId>| d.first().copied());
        assert!(def.is_some(), "{name} should have stable DefId after merge");
        let info = store.get(def.unwrap()).unwrap();
        assert_eq!(info.kind, kind, "{name} should be {kind:?} after merge");
        def.unwrap()
    };

    let base_def = check("Base", DefKind::Class);
    check("Printable", DefKind::Interface);
    check("StringOrNumber", DefKind::TypeAlias);
    check("Direction", DefKind::Enum);

    // File B declarations
    let child_def = check("Child", DefKind::Class);
    check("Loggable", DefKind::Interface);
    check("ID", DefKind::TypeAlias);
    check("Status", DefKind::Enum);

    // Class companion constructors should exist
    let base_ctor = store.get_constructor_def(base_def);
    assert!(
        base_ctor.is_some(),
        "Base class should have ClassConstructor companion"
    );
    let base_ctor_info = store.get(base_ctor.unwrap()).unwrap();
    assert_eq!(base_ctor_info.kind, DefKind::ClassConstructor);

    let child_ctor = store.get_constructor_def(child_def);
    assert!(
        child_ctor.is_some(),
        "Child class should have ClassConstructor companion"
    );

    // Verify type param arity
    let base_info = store.get(base_def).unwrap();
    assert_eq!(
        base_info.type_params.len(),
        1,
        "Base<T> should have 1 type param"
    );

    // Direction enum should have 4 members
    let dir_atom = interner.intern_string("Direction");
    let dir_def = store
        .find_defs_by_name(dir_atom)
        .and_then(|d: Vec<tsz_solver::def::DefId>| d.first().copied())
        .unwrap();
    let dir_info = store.get(dir_def).unwrap();
    assert_eq!(
        dir_info.enum_members.len(),
        4,
        "Direction should have 4 members"
    );
}

/// Verify heritage resolution survives merge: `extends` and `implements`
/// are wired at the DefId level during pre-population.
#[test]
fn heritage_resolution_survives_merge() {
    use tsz_solver::def::DefKind;

    let source = r#"
        export interface Readable { read(): string; }
        export interface Writable { write(data: string): void; }
        export class Stream implements Readable, Writable {
            read() { return ""; }
            write(data: string) {}
        }
        export class FileStream extends Stream {
            path: string;
        }
    "#;

    let sources = vec![("io.ts".to_string(), source.to_string())];
    let program = merge_bind_results(parse_and_bind_parallel(sources));

    let interner = &program.type_interner;
    let store = &program.definition_store;

    let find = |name: &str| -> tsz_solver::def::DefId {
        let atom = interner.intern_string(name);
        store
            .find_defs_by_name(atom)
            .and_then(|d: Vec<tsz_solver::def::DefId>| {
                d.iter().copied().find(|&id| {
                    store.get(id).is_some_and(|info| {
                        matches!(info.kind, DefKind::Class | DefKind::Interface)
                    })
                })
            })
            .unwrap_or_else(|| panic!("{name} should have a DefId"))
    };

    let readable_def = find("Readable");
    let _writable_def = find("Writable");
    let stream_def = find("Stream");
    let file_stream_def = find("FileStream");

    // Stream implements Readable and Writable
    let stream_info = store.get(stream_def).unwrap();
    assert!(
        !stream_info.implements.is_empty(),
        "Stream should have implements entries from heritage resolution"
    );

    // FileStream extends Stream
    let fs_info = store.get(file_stream_def).unwrap();
    assert_eq!(
        fs_info.extends,
        Some(stream_def),
        "FileStream.extends should point to Stream's DefId"
    );

    // Verify Readable is one of the implements targets
    assert!(
        stream_info.implements.contains(&readable_def),
        "Stream.implements should contain Readable's DefId"
    );
}

/// Verify that namespace-member export wiring survives merge.
/// Declarations inside namespaces should be wired as exports of their parent.
#[test]
fn namespace_export_wiring_survives_merge() {
    use tsz_solver::def::DefKind;

    let source = r#"
        export namespace Geo {
            export interface Point { x: number; y: number; }
            export type Distance = number;
            export class Vector { magnitude: number; }
        }
    "#;

    let sources = vec![("geo.ts".to_string(), source.to_string())];
    let program = merge_bind_results(parse_and_bind_parallel(sources));

    let interner = &program.type_interner;
    let store = &program.definition_store;

    // Find the namespace
    let geo_atom = interner.intern_string("Geo");
    let geo_def = store
        .find_defs_by_name(geo_atom)
        .and_then(|d: Vec<tsz_solver::def::DefId>| d.first().copied())
        .expect("Geo namespace should have a DefId");
    let geo_info = store.get(geo_def).unwrap();
    assert_eq!(geo_info.kind, DefKind::Namespace);

    // Namespace should have exports wired
    assert!(
        !geo_info.exports.is_empty(),
        "Geo namespace should have exports from namespace-member wiring"
    );

    // Check that Point, Distance, and Vector are among the exports
    let export_names: Vec<_> = geo_info.exports.iter().map(|(name, _)| *name).collect();
    let point_atom = interner.intern_string("Point");
    let distance_atom = interner.intern_string("Distance");
    let vector_atom = interner.intern_string("Vector");

    assert!(
        export_names.contains(&point_atom),
        "Geo should export Point"
    );
    assert!(
        export_names.contains(&distance_atom),
        "Geo should export Distance"
    );
    assert!(
        export_names.contains(&vector_atom),
        "Geo should export Vector"
    );
}

/// Regression test: cross-file interface merging must not lose local members.
///
/// When interface C is declared in two script files (non-module), the checker
/// for the second file must include members from BOTH declarations. Previously,
/// `delegate_cross_arena_symbol_resolution` would delegate the entire type
/// computation to the first file's checker, losing the second file's members
/// and heritage clauses.
#[test]
fn cross_file_interface_merge_preserves_local_members_and_heritage() {
    // File 0: interface I and C extends I
    // File 1: interface D and C extends D, plus usage
    let files = vec![
        (
            "file0.ts".to_string(),
            r#"
interface I { foo(): string; }
interface C extends I {
    a(): number;
}
"#
            .to_string(),
        ),
        (
            "file1.ts".to_string(),
            r#"
interface D { bar(): number; }
interface C extends D {
    b(): Date;
}
var c: C;
var a: string = c.foo();
var b: number = c.bar();
var d: number = c.a();
var e: Date = c.b();
"#
            .to_string(),
        ),
    ];

    let program = compile_files(files);
    let checker_options = crate::checker::context::CheckerOptions {
        no_lib: true,
        ..Default::default()
    };

    // Use a binder with program-level declaration_arenas (filtered for non-local)
    // to match the CLI path which uses create_binder_from_bound_file_with_augmentations.
    let file1_bound = program
        .files
        .iter()
        .find(|f| f.file_name == "file1.ts")
        .expect("expected file1.ts");
    let file1_idx = program
        .files
        .iter()
        .position(|f| f.file_name == "file1.ts")
        .unwrap();

    let declaration_arenas: crate::binder::state::DeclarationArenaMap = program
        .declaration_arenas
        .iter()
        .filter_map(|(&(sym_id, decl_idx), arenas)| {
            let has_non_local = arenas
                .iter()
                .any(|arena| !std::sync::Arc::ptr_eq(arena, &file1_bound.arena));
            has_non_local.then(|| ((sym_id, decl_idx), arenas.clone()))
        })
        .collect();

    let mut file_locals = crate::binder::SymbolTable::new();
    if file1_idx < program.file_locals.len() {
        for (name, &sym_id) in program.file_locals[file1_idx].iter() {
            file_locals.set(name.clone(), sym_id);
        }
    }
    for (name, &sym_id) in program.globals.iter() {
        if !file_locals.has(name) {
            file_locals.set(name.clone(), sym_id);
        }
    }

    let binder = crate::binder::BinderState::from_bound_state_with_scopes_and_augmentations(
        crate::binder::BinderOptions::default(),
        program.symbols.clone(),
        file_locals,
        file1_bound.node_symbols.clone(),
        crate::binder::state::BinderStateScopeInputs {
            scopes: file1_bound.scopes.clone(),
            node_scope_ids: file1_bound.node_scope_ids.clone(),
            global_augmentations: file1_bound.global_augmentations.clone(),
            module_augmentations: file1_bound.module_augmentations.clone(),
            augmentation_target_modules: file1_bound.augmentation_target_modules.clone(),
            module_exports: program.module_exports.clone(),
            module_declaration_exports_publicly: file1_bound
                .module_declaration_exports_publicly
                .clone(),
            reexports: program.reexports.clone(),
            wildcard_reexports: program.wildcard_reexports.clone(),
            wildcard_reexports_type_only: program.wildcard_reexports_type_only.clone(),
            symbol_arenas: file1_bound.symbol_arenas.clone(),
            declaration_arenas,
            cross_file_node_symbols: program.cross_file_node_symbols.clone(),
            shorthand_ambient_modules: program.shorthand_ambient_modules.clone(),
            modules_with_export_equals: Default::default(),
            flow_nodes: file1_bound.flow_nodes.clone(),
            node_flow: file1_bound.node_flow.clone(),
            switch_clause_to_switch: file1_bound.switch_clause_to_switch.clone(),
            expando_properties: file1_bound.expando_properties.clone(),
            alias_partners: program.alias_partners.clone(),
        },
    );

    let query_cache = tsz_solver::QueryCache::new(&program.type_interner);
    let mut checker = crate::checker::state::CheckerState::with_options(
        &file1_bound.arena,
        &binder,
        &query_cache,
        file1_bound.file_name.clone(),
        &checker_options,
    );
    checker.check_source_file(file1_bound.source_file);

    // Should NOT have any TS2339 "Property does not exist" errors.
    // All properties (foo from I, a from C in file0, bar from D, b from C in file1)
    // should be found on the merged interface C.
    let ts2339_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .collect();
    assert!(
        ts2339_errors.is_empty(),
        "Expected no TS2339 errors for merged interface C, but got: {:?}",
        ts2339_errors
            .iter()
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );
}

