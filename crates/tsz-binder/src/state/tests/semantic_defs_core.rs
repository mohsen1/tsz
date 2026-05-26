use super::*;

#[test]
fn semantic_defs_captures_top_level_class() {
    let binder = bind_source("class Foo {}");
    let sym_id = binder.file_locals.get("Foo").expect("expected Foo");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for class Foo");
    assert_eq!(entry.kind, super::SemanticDefKind::Class);
    assert_eq!(entry.name, "Foo");
}

#[test]
fn semantic_defs_captures_top_level_interface() {
    let binder = bind_source("interface Bar { x: number }");
    let sym_id = binder.file_locals.get("Bar").expect("expected Bar");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for interface Bar");
    assert_eq!(entry.kind, super::SemanticDefKind::Interface);
    assert_eq!(entry.name, "Bar");
}

#[test]
fn semantic_defs_captures_top_level_type_alias() {
    let binder = bind_source("type Baz = string | number");
    let sym_id = binder.file_locals.get("Baz").expect("expected Baz");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for type alias Baz");
    assert_eq!(entry.kind, super::SemanticDefKind::TypeAlias);
    assert_eq!(entry.name, "Baz");
}

#[test]
fn semantic_defs_captures_top_level_enum() {
    let binder = bind_source("enum Color { Red, Green, Blue }");
    let sym_id = binder.file_locals.get("Color").expect("expected Color");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for enum Color");
    assert_eq!(entry.kind, super::SemanticDefKind::Enum);
    assert_eq!(entry.name, "Color");
}

#[test]
fn semantic_defs_captures_top_level_namespace() {
    let binder = bind_source("namespace NS { export type T = number }");
    let sym_id = binder.file_locals.get("NS").expect("expected NS");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for namespace NS");
    assert_eq!(entry.kind, super::SemanticDefKind::Namespace);
    assert_eq!(entry.name, "NS");
}

#[test]
fn semantic_defs_excludes_nested_declarations() {
    let binder = bind_source(
        "
function outer() {
    class Inner {}
    type Local = string;
}
",
    );
    // Inner and Local should NOT be in semantic_defs because they're inside a function body
    for entry in binder.semantic_defs.values() {
        assert_ne!(entry.name, "Inner", "nested class should not be captured");
        assert_ne!(
            entry.name, "Local",
            "nested type alias should not be captured"
        );
    }
}

#[test]
fn semantic_defs_stable_across_rebuild() {
    // Binding the same source twice should produce identical semantic_defs
    let source = "
class A {}
interface B { x: number }
type C = string;
enum D { X }
namespace E { export const v = 1 }
function F() {}
const G = 42;
";
    let binder1 = bind_source(source);
    let binder2 = bind_source(source);

    // Same number of semantic defs
    assert_eq!(binder1.semantic_defs.len(), binder2.semantic_defs.len());

    // Same names and kinds
    for (sym_id, entry1) in binder1.semantic_defs.iter() {
        let entry2 = binder2
            .semantic_defs
            .get(sym_id)
            .expect("same SymbolId should exist in second binding");
        assert_eq!(
            entry1.kind, entry2.kind,
            "kind mismatch for {}",
            entry1.name
        );
        assert_eq!(entry1.name, entry2.name, "name mismatch");
        assert_eq!(
            entry1.span_start, entry2.span_start,
            "span_start mismatch for {}",
            entry1.name
        );
    }
}

#[test]
fn semantic_defs_declaration_merging_keeps_first_identity() {
    // When a symbol is declared multiple times (interface merging),
    // the first declaration's span should be preserved.
    let binder = bind_source(
        "
interface Merged { a: string }
interface Merged { b: number }
",
    );
    let sym_id = binder.file_locals.get("Merged").expect("expected Merged");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for Merged");
    assert_eq!(entry.kind, super::SemanticDefKind::Interface);
    assert_eq!(entry.name, "Merged");
    // Span should be from the FIRST declaration
    let first_decl = binder.symbols.get(sym_id).unwrap().declarations[0];
    assert_eq!(entry.span_start, first_decl.0);
}

#[test]
fn symbols_capture_stable_declaration_spans() {
    let source = "
interface Merged { a: string }
interface Merged { b: number }
const value = 1;
";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena().clone();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let merged_sym_id = binder.file_locals.get("Merged").expect("expected Merged");
    let merged = binder
        .symbols
        .get(merged_sym_id)
        .expect("expected symbol for Merged");
    let first_decl = merged.declarations[0];
    let expected_first_span = arena.get(first_decl).map(|node| (node.pos, node.end));
    assert_eq!(merged.first_declaration_span, expected_first_span);
    assert_eq!(merged.value_declaration_span, None);

    let value_sym_id = binder.file_locals.get("value").expect("expected value");
    let value = binder
        .symbols
        .get(value_sym_id)
        .expect("expected symbol for value");
    let expected_value_span = arena
        .get(value.value_declaration)
        .map(|node| (node.pos, node.end));
    assert_eq!(value.first_declaration_span, expected_value_span);
    assert_eq!(value.value_declaration_span, expected_value_span);
}

#[test]
fn semantic_defs_covers_all_seven_declaration_kinds() {
    let binder = bind_source(
        "
class MyClass {}
interface MyInterface {}
type MyType = number;
enum MyEnum { A }
namespace MyNS {}
function myFunc() {}
const myVar = 1;
",
    );
    assert_eq!(
        binder.semantic_defs.len(),
        7,
        "expected exactly 7 semantic defs, got {:?}",
        binder
            .semantic_defs
            .values()
            .map(|e| &e.name)
            .collect::<Vec<_>>()
    );
    let kinds: std::collections::HashSet<_> =
        binder.semantic_defs.values().map(|e| e.kind).collect();
    assert!(kinds.contains(&super::SemanticDefKind::Class));
    assert!(kinds.contains(&super::SemanticDefKind::Interface));
    assert!(kinds.contains(&super::SemanticDefKind::TypeAlias));
    assert!(kinds.contains(&super::SemanticDefKind::Enum));
    assert!(kinds.contains(&super::SemanticDefKind::Namespace));
    assert!(kinds.contains(&super::SemanticDefKind::Function));
    assert!(kinds.contains(&super::SemanticDefKind::Variable));
}

#[test]
fn semantic_defs_captures_top_level_function() {
    let binder = bind_source("function greet(name: string): string { return name; }");
    let sym_id = binder.file_locals.get("greet").expect("expected greet");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for function greet");
    assert_eq!(entry.kind, super::SemanticDefKind::Function);
    assert_eq!(entry.name, "greet");
}

#[test]
fn semantic_defs_captures_top_level_variable_const() {
    let binder = bind_source("const MAX_SIZE = 100;");
    let sym_id = binder
        .file_locals
        .get("MAX_SIZE")
        .expect("expected MAX_SIZE");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for variable MAX_SIZE");
    assert_eq!(entry.kind, super::SemanticDefKind::Variable);
    assert_eq!(entry.name, "MAX_SIZE");
}

#[test]
fn semantic_defs_captures_top_level_variable_let() {
    let binder = bind_source("let counter = 0;");
    let sym_id = binder.file_locals.get("counter").expect("expected counter");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for variable counter");
    assert_eq!(entry.kind, super::SemanticDefKind::Variable);
    assert_eq!(entry.name, "counter");
}

#[test]
fn semantic_defs_captures_top_level_variable_var() {
    let binder = bind_source("var legacy = true;");
    let sym_id = binder.file_locals.get("legacy").expect("expected legacy");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for variable legacy");
    assert_eq!(entry.kind, super::SemanticDefKind::Variable);
    assert_eq!(entry.name, "legacy");
}

#[test]
fn semantic_defs_captures_destructured_top_level_variables() {
    let binder = bind_source("const { a, b } = { a: 1, b: 2 };");
    let sym_a = binder.file_locals.get("a").expect("expected a");
    let entry_a = binder
        .semantic_defs
        .get(&sym_a)
        .expect("expected semantic def for variable a");
    assert_eq!(entry_a.kind, super::SemanticDefKind::Variable);
    assert_eq!(entry_a.name, "a");

    let sym_b = binder.file_locals.get("b").expect("expected b");
    let entry_b = binder
        .semantic_defs
        .get(&sym_b)
        .expect("expected semantic def for variable b");
    assert_eq!(entry_b.kind, super::SemanticDefKind::Variable);
    assert_eq!(entry_b.name, "b");
}

#[test]
fn semantic_defs_excludes_nested_functions_and_variables() {
    let binder = bind_source(
        "
function outer() {
    function inner() {}
    const localVar = 1;
}
",
    );
    // outer should be captured, but inner and localVar should not
    let has_outer = binder.semantic_defs.values().any(|e| e.name == "outer");
    assert!(has_outer, "top-level function 'outer' should be captured");
    let has_inner = binder.semantic_defs.values().any(|e| e.name == "inner");
    assert!(!has_inner, "nested function 'inner' should not be captured");
    let has_local_var = binder.semantic_defs.values().any(|e| e.name == "localVar");
    assert!(
        !has_local_var,
        "nested variable 'localVar' should not be captured"
    );
}

#[test]
fn semantic_defs_file_id_matches_symbol_decl_file_idx() {
    // SemanticDefEntry.file_id must match the symbol's decl_file_idx.
    // pre_populate_def_ids_from_binder relies on this instead of
    // looking up the symbol table, so a mismatch would break DefId
    // registration in the DefinitionStore's composite key index.
    let binder = bind_source(
        "
class A {}
interface B {}
type C = number;
enum D { X }
namespace E {}
",
    );
    for (&sym_id, entry) in binder.semantic_defs.iter() {
        let symbol = binder
            .symbols
            .get(sym_id)
            .unwrap_or_else(|| panic!("symbol {} not found for {}", sym_id.0, entry.name));
        assert_eq!(
            entry.file_id, symbol.decl_file_idx,
            "file_id mismatch for {} (sym_id {}): entry.file_id={}, symbol.decl_file_idx={}",
            entry.name, sym_id.0, entry.file_id, symbol.decl_file_idx
        );
    }
}

#[test]
fn merge_lib_contexts_propagates_semantic_defs_with_remapped_ids() {
    // After merge_lib_contexts_into_binder, the main binder's semantic_defs
    // should contain entries for lib symbols under the new (remapped) SymbolIds.
    // This ensures pre_populate_def_ids_from_binder covers lib symbols, so the
    // checker doesn't fall through to get_or_create_def_id's repair path.

    // Create a "lib" binder with top-level declarations.
    let lib_source = r"
interface Array<T> {}
interface String {}
type Partial<T> = { [P in keyof T]?: T[P] };
class Error {}
enum Direction { Up, Down }
";
    let mut lib_parser = ParserState::new("lib.d.ts".to_string(), lib_source.to_string());
    let lib_root = lib_parser.parse_source_file();
    let mut lib_binder = BinderState::new();
    lib_binder.bind_source_file(lib_parser.get_arena(), lib_root);

    // Verify lib binder has semantic_defs for all 5 declarations.
    assert!(
        lib_binder.semantic_defs.len() >= 5,
        "lib binder should have at least 5 semantic_defs, got {}",
        lib_binder.semantic_defs.len()
    );

    let lib_ctx = super::LibContext {
        arena: std::sync::Arc::new(lib_parser.get_arena().clone()),
        binder: std::sync::Arc::new(lib_binder),
    };

    // Create the main user binder and merge.
    let user_source = "let x: number = 1;";
    let mut user_parser = ParserState::new("test.ts".to_string(), user_source.to_string());
    let user_root = user_parser.parse_source_file();
    let mut main_binder = BinderState::new();
    main_binder.bind_source_file(user_parser.get_arena(), user_root);

    let pre_merge_count = main_binder.semantic_defs.len();
    main_binder.merge_lib_contexts_into_binder(&[lib_ctx]);

    // After merge, semantic_defs should have grown with lib entries.
    let post_merge_count = main_binder.semantic_defs.len();
    assert!(
        post_merge_count > pre_merge_count,
        "merge should propagate lib semantic_defs: before={pre_merge_count}, after={post_merge_count}"
    );

    // Each lib semantic_def should use a remapped SymbolId that exists in the
    // main binder's symbol arena (not the lib binder's original IDs).
    for (&sym_id, entry) in main_binder.semantic_defs.iter() {
        assert!(
            main_binder.symbols.get(sym_id).is_some(),
            "semantic_def for '{}' (SymbolId {}) should reference a symbol in the main arena",
            entry.name,
            sym_id.0,
        );
    }

    // The expected lib type names should be findable via file_locals → semantic_defs.
    for expected_name in &["Array", "String", "Partial", "Error", "Direction"] {
        let sym_id = main_binder
            .file_locals
            .get(expected_name)
            .unwrap_or_else(|| panic!("expected '{expected_name}' in file_locals after merge"));
        assert!(
            main_binder.semantic_defs.contains_key(&sym_id),
            "expected semantic_def for '{expected_name}' (SymbolId {}) after lib merge",
            sym_id.0,
        );
    }
}

#[test]
fn merge_lib_contexts_does_not_overwrite_user_semantic_defs() {
    // If the user declares a type with the same name as a lib type,
    // the user's semantic_def should take precedence.

    let lib_source = "interface Error {}";
    let mut lib_parser = ParserState::new("lib.d.ts".to_string(), lib_source.to_string());
    let lib_root = lib_parser.parse_source_file();
    let mut lib_binder = BinderState::new();
    lib_binder.bind_source_file(lib_parser.get_arena(), lib_root);

    let lib_ctx = super::LibContext {
        arena: std::sync::Arc::new(lib_parser.get_arena().clone()),
        binder: std::sync::Arc::new(lib_binder),
    };

    // User declares their own Error class.
    let user_source = "class Error { message: string; }";
    let mut user_parser = ParserState::new("test.ts".to_string(), user_source.to_string());
    let user_root = user_parser.parse_source_file();
    let mut main_binder = BinderState::new();
    main_binder.bind_source_file(user_parser.get_arena(), user_root);

    // User's Error should be a Class, not an Interface.
    let user_sym_id = main_binder.file_locals.get("Error").unwrap();
    let user_entry = &main_binder.semantic_defs[&user_sym_id];
    assert_eq!(user_entry.kind, super::SemanticDefKind::Class);

    main_binder.merge_lib_contexts_into_binder(&[lib_ctx]);

    // After merge, the semantic_def for Error should still be the user's Class.
    // The lib's Interface version should NOT overwrite it.
    // Note: can_merge_symbols allows Class+Interface merging, so the file_locals
    // entry reuses the user's SymbolId with merged flags. The semantic_def
    // should still be the user's original entry.
    let merged_sym_id = main_binder.file_locals.get("Error").unwrap();
    let merged_entry = &main_binder.semantic_defs[&merged_sym_id];
    assert_eq!(
        merged_entry.kind,
        super::SemanticDefKind::Class,
        "user's semantic_def should not be overwritten by lib merge"
    );
}

#[test]
fn type_only_export_clone_preserves_lib_provenance() {
    let lib_source = "interface LibGlobal {}";
    let mut lib_parser = ParserState::new("lib.d.ts".to_string(), lib_source.to_string());
    let lib_root = lib_parser.parse_source_file();
    let mut lib_binder = BinderState::new();
    lib_binder.bind_source_file(lib_parser.get_arena(), lib_root);

    let lib_ctx = super::LibContext {
        arena: Arc::new(lib_parser.get_arena().clone()),
        binder: Arc::new(lib_binder),
    };

    let user_source = "export type { LibGlobal };";
    let mut user_parser = ParserState::new("test.ts".to_string(), user_source.to_string());
    let user_root = user_parser.parse_source_file();
    let mut main_binder = BinderState::new();
    main_binder.merge_lib_contexts_into_binder(&[lib_ctx]);
    main_binder.bind_source_file(user_parser.get_arena(), user_root);

    let export_sym_id = main_binder
        .file_locals
        .get("LibGlobal")
        .expect("expected type-only export clone in file_locals");
    let export_symbol = main_binder
        .symbols
        .get(export_sym_id)
        .expect("expected type-only export clone symbol");

    assert!(
        export_symbol.is_type_only,
        "type-only export clone should remain type-only"
    );
    assert!(
        main_binder.lib_symbol_ids.contains(&export_sym_id),
        "type-only export clone of a lib global should preserve lib provenance"
    );
    assert!(
        main_binder.symbol_arenas.contains_key(&export_sym_id),
        "type-only export clone should preserve declaration arena provenance"
    );
}

#[test]
fn semantic_defs_captures_generic_interface() {
    // Generic interfaces should be captured with the same identity
    // regardless of type parameter count. The binder only records
    // kind + name + span; type params are resolved later by the checker.
    let binder = bind_source("interface Container<T, U> { value: T; key: U }");
    let sym_id = binder
        .file_locals
        .get("Container")
        .expect("expected Container");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for generic interface Container");
    assert_eq!(entry.kind, super::SemanticDefKind::Interface);
    assert_eq!(entry.name, "Container");
}

#[test]
fn semantic_defs_captures_namespace_but_not_its_children() {
    // A namespace itself gets a semantic def, but types declared
    // inside it also get captured because they're in a Module scope
    // (which is allowed by record_semantic_def's is_top_level check).
    let binder = bind_source(
        "
namespace Outer {
    export interface Inner {}
    export type Alias = string;
    export class Klass {}
    export enum E { A }
}
",
    );
    // The namespace itself should be captured
    let ns_sym = binder
        .file_locals
        .get("Outer")
        .expect("expected Outer namespace");
    let ns_entry = binder
        .semantic_defs
        .get(&ns_sym)
        .expect("expected semantic def for Outer");
    assert_eq!(ns_entry.kind, super::SemanticDefKind::Namespace);
}

#[test]
fn semantic_defs_captures_module_scoped_declarations() {
    // Declarations inside `declare module "foo" {}` should be captured
    // because the module body creates a ContainerKind::Module scope.
    let binder = bind_source(
        r#"
declare module "mylib" {
    export interface Config {}
    export type Mode = "dark" | "light";
    export class Client {}
}
"#,
    );
    // Module-scoped declarations should appear in semantic_defs
    // (they use the module's scope which is ContainerKind::Module).
    let has_interface = binder
        .semantic_defs
        .values()
        .any(|e| e.name == "Config" && e.kind == super::SemanticDefKind::Interface);
    let has_alias = binder
        .semantic_defs
        .values()
        .any(|e| e.name == "Mode" && e.kind == super::SemanticDefKind::TypeAlias);
    let has_class = binder
        .semantic_defs
        .values()
        .any(|e| e.name == "Client" && e.kind == super::SemanticDefKind::Class);
    assert!(
        has_interface,
        "module-scoped interface Config should be in semantic_defs"
    );
    assert!(
        has_alias,
        "module-scoped type alias Mode should be in semantic_defs"
    );
    assert!(
        has_class,
        "module-scoped class Client should be in semantic_defs"
    );
}

#[test]
fn semantic_defs_generic_class_with_constraints() {
    // Generic classes with constrained type params should still
    // be captured. Only kind/name/span matters at bind time.
    let binder =
        bind_source("class Registry<K extends string, V extends object> { entries: Map<K, V> }");
    let sym_id = binder
        .file_locals
        .get("Registry")
        .expect("expected Registry");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for generic class Registry");
    assert_eq!(entry.kind, super::SemanticDefKind::Class);
    assert_eq!(entry.name, "Registry");
}

#[test]
fn merge_lib_contexts_propagates_generic_lib_interfaces() {
    // Generic lib interfaces like Array<T> must be propagated through
    // lib merge so pre_populate_def_ids_from_binder covers them.
    let lib_source = r"
interface Array<T> {
    length: number;
    push(...items: T[]): number;
}
interface ReadonlyArray<T> {
    readonly length: number;
}
type Partial<T> = { [P in keyof T]?: T[P] };
type Required<T> = { [P in keyof T]-?: T[P] };
";
    let mut lib_parser = ParserState::new("lib.es5.d.ts".to_string(), lib_source.to_string());
    let lib_root = lib_parser.parse_source_file();
    let mut lib_binder = BinderState::new();
    lib_binder.bind_source_file(lib_parser.get_arena(), lib_root);

    let lib_ctx = super::LibContext {
        arena: std::sync::Arc::new(lib_parser.get_arena().clone()),
        binder: std::sync::Arc::new(lib_binder),
    };

    let user_source = "let arr: Array<number> = [1, 2, 3];";
    let mut user_parser = ParserState::new("test.ts".to_string(), user_source.to_string());
    let user_root = user_parser.parse_source_file();
    let mut main_binder = BinderState::new();
    main_binder.bind_source_file(user_parser.get_arena(), user_root);
    main_binder.merge_lib_contexts_into_binder(&[lib_ctx]);

    // All generic lib types should be findable via file_locals → semantic_defs
    for name in &["Array", "ReadonlyArray", "Partial", "Required"] {
        let sym_id = main_binder
            .file_locals
            .get(name)
            .unwrap_or_else(|| panic!("expected '{name}' in file_locals after lib merge"));
        assert!(
            main_binder.semantic_defs.contains_key(&sym_id),
            "expected semantic_def for generic lib type '{name}' (SymbolId {})",
            sym_id.0,
        );
    }
}

// =============================================================================
// file_import_sources tests
// =============================================================================

#[test]
fn file_import_sources_static_imports() {
    let source = r#"
import { foo } from "./utils";
import bar from "react";
import "./side-effect";
"#;
    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_import_sources.contains(&"./utils".to_string()),
        "expected './utils' in file_import_sources, got: {:?}",
        binder.file_import_sources
    );
    assert!(
        binder.file_import_sources.contains(&"react".to_string()),
        "expected 'react' in file_import_sources, got: {:?}",
        binder.file_import_sources
    );
    assert!(
        binder
            .file_import_sources
            .contains(&"./side-effect".to_string()),
        "expected './side-effect' in file_import_sources, got: {:?}",
        binder.file_import_sources
    );
}

#[test]
fn file_import_sources_export_from() {
    let source = r#"
export { x } from "./module-a";
export * from "./module-b";
export type { T } from "./types";
"#;
    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder
            .file_import_sources
            .contains(&"./module-a".to_string()),
        "expected './module-a' in file_import_sources, got: {:?}",
        binder.file_import_sources
    );
    assert!(
        binder
            .file_import_sources
            .contains(&"./module-b".to_string()),
        "expected './module-b' in file_import_sources, got: {:?}",
        binder.file_import_sources
    );
    assert!(
        binder.file_import_sources.contains(&"./types".to_string()),
        "expected './types' in file_import_sources, got: {:?}",
        binder.file_import_sources
    );
}

#[test]
fn file_import_sources_import_equals_require() {
    let source = r#"
import ts = require("typescript");
"#;
    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder
            .file_import_sources
            .contains(&"typescript".to_string()),
        "expected 'typescript' in file_import_sources, got: {:?}",
        binder.file_import_sources
    );
}

#[test]
fn file_import_sources_reset_clears() {
    let source = r#"import { a } from "./a";"#;
    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(!binder.file_import_sources.is_empty());

    binder.reset();
    assert!(
        binder.file_import_sources.is_empty(),
        "reset should clear file_import_sources"
    );
}

#[test]
fn file_import_sources_no_dynamic_imports() {
    // Dynamic imports (import() calls) should NOT appear in file_import_sources
    let source = r#"
const m = import("./dynamic");
const r = require("./required");
"#;
    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_import_sources.is_empty(),
        "dynamic imports should not be in file_import_sources, got: {:?}",
        binder.file_import_sources
    );
}

#[test]
fn expando_const_variable_function_expression() {
    let (binder, _parser) = parse_and_bind(
        r"
const Y = function Y() {}
Y.test = 42;
",
    );

    assert!(
        binder
            .expando_properties
            .get("Y")
            .is_some_and(|props| props.contains("test")),
        "should track Y.test as expando property, got: {:?}",
        binder.expando_properties
    );
}

#[test]
fn expando_typed_variable_with_arrow_function() {
    let (binder, _parser) = parse_and_bind(
        r"
const foo: Foo = () => {};
foo.prop = true;
",
    );

    assert!(
        binder
            .expando_properties
            .get("foo")
            .is_some_and(|props| props.contains("prop")),
        "should track foo.prop as expando property even with type annotation, got: {:?}",
        binder.expando_properties
    );
}

#[test]
fn semantic_defs_captures_type_param_count_for_generics() {
    let binder = bind_source(
        "
class MyClass<T, U> {}
interface MyInterface<A, B, C> {}
type MyType<X> = X[];
function myFunc<T>(): T { return undefined as any; }
enum MyEnum { A }
const myVar = 1;
namespace MyNS {}
",
    );

    // Generic class: 2 type params
    let class_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyClass")
        .expect("MyClass");
    assert_eq!(
        class_entry.type_param_count, 2,
        "MyClass should have 2 type params"
    );

    // Generic interface: 3 type params
    let iface_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyInterface")
        .expect("MyInterface");
    assert_eq!(
        iface_entry.type_param_count, 3,
        "MyInterface should have 3 type params"
    );

    // Generic type alias: 1 type param
    let alias_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyType")
        .expect("MyType");
    assert_eq!(
        alias_entry.type_param_count, 1,
        "MyType should have 1 type param"
    );

    // Generic function: 1 type param
    let func_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "myFunc")
        .expect("myFunc");
    assert_eq!(
        func_entry.type_param_count, 1,
        "myFunc should have 1 type param"
    );

    // Non-generic declarations: 0 type params
    let enum_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyEnum")
        .expect("MyEnum");
    assert_eq!(
        enum_entry.type_param_count, 0,
        "MyEnum should have 0 type params"
    );

    let var_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "myVar")
        .expect("myVar");
    assert_eq!(
        var_entry.type_param_count, 0,
        "myVar should have 0 type params"
    );

    let ns_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyNS")
        .expect("MyNS");
    assert_eq!(
        ns_entry.type_param_count, 0,
        "MyNS should have 0 type params"
    );
}

#[test]
fn semantic_defs_captures_type_param_names_for_generics() {
    let binder = bind_source(
        "
class MyClass<T, U> {}
interface MyInterface<A, B, C> {}
type MyType<X> = X[];
function myFunc<R>(): R { return undefined as any; }
enum MyEnum { A }
const myVar = 1;
namespace MyNS {}
",
    );

    let class_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyClass")
        .expect("MyClass");
    assert_eq!(
        class_entry.type_param_names,
        vec!["T", "U"],
        "MyClass should capture type param names T, U"
    );

    let iface_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyInterface")
        .expect("MyInterface");
    assert_eq!(
        iface_entry.type_param_names,
        vec!["A", "B", "C"],
        "MyInterface should capture type param names A, B, C"
    );

    let alias_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyType")
        .expect("MyType");
    assert_eq!(
        alias_entry.type_param_names,
        vec!["X"],
        "MyType should capture type param name X"
    );

    let func_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "myFunc")
        .expect("myFunc");
    assert_eq!(
        func_entry.type_param_names,
        vec!["R"],
        "myFunc should capture type param name R"
    );

    // Non-generic declarations: empty names
    let enum_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyEnum")
        .expect("MyEnum");
    assert!(enum_entry.type_param_names.is_empty());

    let var_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "myVar")
        .expect("myVar");
    assert!(var_entry.type_param_names.is_empty());

    let ns_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyNS")
        .expect("MyNS");
    assert!(ns_entry.type_param_names.is_empty());
}

#[test]
fn semantic_defs_type_param_count_zero_for_non_generic() {
    let binder = bind_source("class Plain {} interface Simple {} type Alias = string;");

    for entry in binder.semantic_defs.values() {
        assert_eq!(
            entry.type_param_count, 0,
            "{} should have 0 type params",
            entry.name
        );
    }
}

// ===== is_exported field tests =====

#[test]
fn semantic_defs_captures_export_visibility() {
    let binder = bind_source(
        "
export class ExportedClass {}
class LocalClass {}
export interface ExportedIface {}
interface LocalIface {}
export type ExportedAlias = string;
type LocalAlias = number;
export enum ExportedEnum { A }
enum LocalEnum { B }
export function exportedFn() {}
function localFn() {}
export const exportedVar = 1;
const localVar = 2;
",
    );

    let exported_names = [
        "ExportedClass",
        "ExportedIface",
        "ExportedAlias",
        "ExportedEnum",
        "exportedFn",
        "exportedVar",
    ];
    let local_names = [
        "LocalClass",
        "LocalIface",
        "LocalAlias",
        "LocalEnum",
        "localFn",
        "localVar",
    ];

    for name in &exported_names {
        let sym_id = binder
            .file_locals
            .get(name)
            .unwrap_or_else(|| panic!("expected {name} in file_locals"));
        let entry = binder
            .semantic_defs
            .get(&sym_id)
            .unwrap_or_else(|| panic!("expected semantic_def for {name}"));
        assert!(entry.is_exported, "{name} should be marked as exported");
    }

    for name in &local_names {
        let sym_id = binder
            .file_locals
            .get(name)
            .unwrap_or_else(|| panic!("expected {name} in file_locals"));
        let entry = binder
            .semantic_defs
            .get(&sym_id)
            .unwrap_or_else(|| panic!("expected semantic_def for {name}"));
        assert!(
            !entry.is_exported,
            "{name} should NOT be marked as exported"
        );
    }
}

#[test]
fn semantic_defs_exported_namespace_nested_members() {
    // Declarations inside a namespace body (Module scope) should also
    // have their is_exported field set correctly.
    let binder = bind_source(
        "
namespace Outer {
    export class Inner {}
    class Private {}
    export type PubAlias = string;
}
",
    );

    // Outer namespace should be captured
    let outer_id = binder.file_locals.get("Outer").expect("expected Outer");
    let outer_entry = binder
        .semantic_defs
        .get(&outer_id)
        .expect("expected semantic_def for Outer");
    assert_eq!(outer_entry.kind, super::SemanticDefKind::Namespace);
    assert!(!outer_entry.is_exported, "Outer has no export modifier");

    // Inner and PubAlias are in the namespace's Module scope, so they should
    // be captured in semantic_defs if they are top-level within that Module.
    let has_inner = binder
        .semantic_defs
        .values()
        .any(|e| e.name == "Inner" && e.is_exported);
    assert!(
        has_inner,
        "exported Inner class inside namespace should be captured with is_exported=true"
    );

    let has_private = binder
        .semantic_defs
        .values()
        .any(|e| e.name == "Private" && !e.is_exported);
    assert!(
        has_private,
        "non-exported Private class inside namespace should be captured with is_exported=false"
    );

    let has_pub_alias = binder
        .semantic_defs
        .values()
        .any(|e| e.name == "PubAlias" && e.is_exported);
    assert!(
        has_pub_alias,
        "exported PubAlias inside namespace should be captured with is_exported=true"
    );
}

// ===== Merge/rebind identity stability tests =====

#[test]
fn semantic_defs_stable_identity_across_rebind() {
    // Binding the same source twice must produce entries with identical
    // kind, name, span_start, type_param_count, and is_exported.
    // This ensures stable identity survives rebind (e.g., after file edit).
    let source = "
export class Foo<T> {}
interface Bar { x: number }
export type Baz<A, B> = A | B;
enum Color { Red }
export namespace NS { export type Inner = string; }
export function greet(name: string): void {}
const LOCAL = 42;
";
    let binder1 = bind_source(source);
    let binder2 = bind_source(source);

    assert_eq!(binder1.semantic_defs.len(), binder2.semantic_defs.len());

    for (sym_id, entry1) in binder1.semantic_defs.iter() {
        let entry2 = binder2
            .semantic_defs
            .get(sym_id)
            .expect("same SymbolId should exist in second binding");
        assert_eq!(
            entry1.kind, entry2.kind,
            "kind mismatch for {}",
            entry1.name
        );
        assert_eq!(entry1.name, entry2.name, "name mismatch");
        assert_eq!(
            entry1.span_start, entry2.span_start,
            "span mismatch for {}",
            entry1.name
        );
        assert_eq!(
            entry1.type_param_count, entry2.type_param_count,
            "tp_count mismatch for {}",
            entry1.name
        );
        assert_eq!(
            entry1.is_exported, entry2.is_exported,
            "is_exported mismatch for {}",
            entry1.name
        );
    }
}

#[test]
fn semantic_defs_lib_merge_preserves_is_exported() {
    // Lib symbols merged into the main binder should retain their is_exported flag.
    let lib_source = "export interface LibIface {} interface InternalIface {}";
    let mut lib_parser = ParserState::new("lib.d.ts".to_string(), lib_source.to_string());
    let lib_root = lib_parser.parse_source_file();
    let mut lib_binder = BinderState::new();
    lib_binder.bind_source_file(lib_parser.get_arena(), lib_root);

    let lib_ctx = super::LibContext {
        arena: std::sync::Arc::new(lib_parser.get_arena().clone()),
        binder: std::sync::Arc::new(lib_binder),
    };

    let user_source = "let x = 1;";
    let mut user_parser = ParserState::new("test.ts".to_string(), user_source.to_string());
    let user_root = user_parser.parse_source_file();
    let mut main_binder = BinderState::new();
    main_binder.bind_source_file(user_parser.get_arena(), user_root);

    main_binder.merge_lib_contexts_into_binder(&[lib_ctx]);

    // Find the merged lib entries and check is_exported
    let lib_iface = main_binder
        .semantic_defs
        .values()
        .find(|e| e.name == "LibIface")
        .expect("LibIface should be in semantic_defs after merge");
    assert!(
        lib_iface.is_exported,
        "LibIface should preserve is_exported=true through merge"
    );

    let internal_iface = main_binder
        .semantic_defs
        .values()
        .find(|e| e.name == "InternalIface")
        .expect("InternalIface should be in semantic_defs after merge");
    assert!(
        !internal_iface.is_exported,
        "InternalIface should preserve is_exported=false through merge"
    );
}

#[test]
fn semantic_defs_declaration_merging_across_files_keeps_first_identity() {
    // When the same interface is declared in two lib files, the first
    // declaration's identity (kind, span, is_exported) should be preserved.
    let lib1_source = "export interface Shared { a: string }";
    let mut lib1_parser = ParserState::new("lib1.d.ts".to_string(), lib1_source.to_string());
    let lib1_root = lib1_parser.parse_source_file();
    let mut lib1_binder = BinderState::new();
    lib1_binder.bind_source_file(lib1_parser.get_arena(), lib1_root);

    let lib2_source = "interface Shared { b: number }";
    let mut lib2_parser = ParserState::new("lib2.d.ts".to_string(), lib2_source.to_string());
    let lib2_root = lib2_parser.parse_source_file();
    let mut lib2_binder = BinderState::new();
    lib2_binder.bind_source_file(lib2_parser.get_arena(), lib2_root);

    let lib_ctx1 = super::LibContext {
        arena: std::sync::Arc::new(lib1_parser.get_arena().clone()),
        binder: std::sync::Arc::new(lib1_binder),
    };
    let lib_ctx2 = super::LibContext {
        arena: std::sync::Arc::new(lib2_parser.get_arena().clone()),
        binder: std::sync::Arc::new(lib2_binder),
    };

    let user_source = "let x = 1;";
    let mut user_parser = ParserState::new("test.ts".to_string(), user_source.to_string());
    let user_root = user_parser.parse_source_file();
    let mut main_binder = BinderState::new();
    main_binder.bind_source_file(user_parser.get_arena(), user_root);

    main_binder.merge_lib_contexts_into_binder(&[lib_ctx1, lib_ctx2]);

    let shared_entry = main_binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Shared")
        .expect("Shared should be in semantic_defs after merge");

    // First declaration wins: lib1 declares `export interface Shared`,
    // so is_exported should be true.
    assert_eq!(shared_entry.kind, super::SemanticDefKind::Interface);
    assert!(
        shared_entry.is_exported,
        "declaration merging should keep first identity (exported from lib1)"
    );
}

#[test]
fn semantic_defs_enriched_fields_stable_across_rebind() {
    // Enriched fields (enum_member_names, is_const, is_abstract) must be
    // identical across two bindings of the same source. This ensures the
    // binder-owned data that feeds solver DefinitionInfo pre-population
    // is deterministic.
    let source = "
export const enum Direction { Up, Down, Left, Right }
enum Plain { A, B }
abstract class AbstractBase<T> {}
class Concrete {}
";
    let binder1 = bind_source(source);
    let binder2 = bind_source(source);

    for (sym_id, e1) in binder1.semantic_defs.iter() {
        let e2 = binder2
            .semantic_defs
            .get(sym_id)
            .unwrap_or_else(|| panic!("{} missing in second binding", e1.name));
        assert_eq!(
            e1.enum_member_names, e2.enum_member_names,
            "enum_member_names mismatch for {}",
            e1.name
        );
        assert_eq!(
            e1.is_const, e2.is_const,
            "is_const mismatch for {}",
            e1.name
        );
        assert_eq!(
            e1.is_abstract, e2.is_abstract,
            "is_abstract mismatch for {}",
            e1.name
        );
    }

    // Verify specific enrichments
    let direction = binder1
        .semantic_defs
        .values()
        .find(|e| e.name == "Direction")
        .expect("Direction should be captured");
    assert!(direction.is_const, "const enum should have is_const=true");
    assert_eq!(
        direction.enum_member_names,
        vec!["Up", "Down", "Left", "Right"]
    );

    let plain = binder1
        .semantic_defs
        .values()
        .find(|e| e.name == "Plain")
        .expect("Plain should be captured");
    assert!(!plain.is_const, "non-const enum should have is_const=false");
    assert_eq!(plain.enum_member_names, vec!["A", "B"]);

    let abstract_base = binder1
        .semantic_defs
        .values()
        .find(|e| e.name == "AbstractBase")
        .expect("AbstractBase should be captured");
    assert!(
        abstract_base.is_abstract,
        "abstract class should have is_abstract=true"
    );

    let concrete = binder1
        .semantic_defs
        .values()
        .find(|e| e.name == "Concrete")
        .expect("Concrete should be captured");
    assert!(
        !concrete.is_abstract,
        "non-abstract class should have is_abstract=false"
    );
}

#[test]
fn semantic_defs_namespace_scoped_declarations_captured() {
    // Declarations inside namespace bodies use ContainerKind::Module scope,
    // which is included in the top-level check. This verifies that classes,
    // interfaces, type aliases, enums, and functions inside namespaces get
    // semantic_defs entries.
    let source = "
namespace Outer {
    export class InnerClass {}
    export interface InnerIface { x: number }
    export type InnerAlias = string;
    export enum InnerEnum { A }
    export function innerFn(): void {}
}
";
    let binder = bind_source(source);

    // The namespace itself should be captured
    let outer = binder.semantic_defs.values().find(|e| e.name == "Outer");
    assert!(
        outer.is_some(),
        "Outer namespace should be in semantic_defs"
    );

    // Declarations inside the namespace should also be captured
    for name in &[
        "InnerClass",
        "InnerIface",
        "InnerAlias",
        "InnerEnum",
        "innerFn",
    ] {
        let found = binder.semantic_defs.values().any(|e| e.name == *name);
        assert!(
            found,
            "namespace-scoped declaration '{name}' should be in semantic_defs"
        );
    }
}

#[test]
fn semantic_defs_enriched_fields_survive_lib_merge() {
    // Enriched fields (is_abstract, is_const, enum_member_names) captured at
    // bind time must survive the lib merge path so the checker's DefId
    // pre-population gets complete identity information.
    let lib_source = r"
abstract class AbstractBase { abstract foo(): void; }
const enum Direction { Up, Down, Left, Right }
enum Color { Red, Green, Blue }
";
    let mut lib_parser = ParserState::new("lib.d.ts".to_string(), lib_source.to_string());
    let lib_root = lib_parser.parse_source_file();
    let mut lib_binder = BinderState::new();
    lib_binder.bind_source_file(lib_parser.get_arena(), lib_root);

    // Verify fields are captured in the lib binder
    let abs = lib_binder
        .semantic_defs
        .values()
        .find(|e| e.name == "AbstractBase")
        .expect("AbstractBase should be in lib semantic_defs");
    assert!(
        abs.is_abstract,
        "abstract class should preserve is_abstract in lib binder"
    );

    let dir = lib_binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Direction")
        .expect("Direction should be in lib semantic_defs");
    assert!(
        dir.is_const,
        "const enum should preserve is_const in lib binder"
    );
    assert_eq!(dir.enum_member_names, vec!["Up", "Down", "Left", "Right"]);

    // Merge into a user binder
    let lib_ctx = super::LibContext {
        arena: std::sync::Arc::new(lib_parser.get_arena().clone()),
        binder: std::sync::Arc::new(lib_binder),
    };
    let user_source = "let x = 1;";
    let mut user_parser = ParserState::new("test.ts".to_string(), user_source.to_string());
    let user_root = user_parser.parse_source_file();
    let mut main_binder = BinderState::new();
    main_binder.bind_source_file(user_parser.get_arena(), user_root);
    main_binder.merge_lib_contexts_into_binder(&[lib_ctx]);

    // Verify enriched fields survived the merge
    let abs_merged = main_binder
        .semantic_defs
        .values()
        .find(|e| e.name == "AbstractBase")
        .expect("AbstractBase should survive lib merge");
    assert!(
        abs_merged.is_abstract,
        "is_abstract should survive lib merge"
    );

    let dir_merged = main_binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Direction")
        .expect("Direction should survive lib merge");
    assert!(dir_merged.is_const, "is_const should survive lib merge");
    assert_eq!(
        dir_merged.enum_member_names,
        vec!["Up", "Down", "Left", "Right"],
        "enum_member_names should survive lib merge"
    );

    let color_merged = main_binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Color")
        .expect("Color should survive lib merge");
    assert!(
        !color_merged.is_const,
        "regular enum should not be const after merge"
    );
    assert_eq!(
        color_merged.enum_member_names,
        vec!["Red", "Green", "Blue"],
        "enum_member_names should survive lib merge for regular enum"
    );
}

#[test]
fn semantic_defs_all_families_have_correct_kinds() {
    // Verify that all seven declaration families produce the correct
    // SemanticDefKind after binding.
    let source = r"
class MyClass {}
interface MyIface {}
type MyAlias = string;
enum MyEnum { A }
namespace MyNS {}
function myFunc() {}
const myVar = 1;
";
    let binder = bind_source(source);

    let expected: Vec<(&str, super::SemanticDefKind)> = vec![
        ("MyClass", super::SemanticDefKind::Class),
        ("MyIface", super::SemanticDefKind::Interface),
        ("MyAlias", super::SemanticDefKind::TypeAlias),
        ("MyEnum", super::SemanticDefKind::Enum),
        ("MyNS", super::SemanticDefKind::Namespace),
        ("myFunc", super::SemanticDefKind::Function),
        ("myVar", super::SemanticDefKind::Variable),
    ];

    for (name, expected_kind) in expected {
        let entry = binder
            .semantic_defs
            .values()
            .find(|e| e.name == name)
            .unwrap_or_else(|| panic!("expected semantic_def for '{name}'"));
        assert_eq!(
            entry.kind, expected_kind,
            "wrong kind for '{}': expected {:?}, got {:?}",
            name, expected_kind, entry.kind
        );
    }
}
