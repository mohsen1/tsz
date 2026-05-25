use super::*;

#[test]
fn test_heritage_names_captured_for_class_extends() {
    let source = "class Foo extends Bar {}";
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Foo")
        .expect("expected semantic_def for Foo");
    assert_eq!(entry.extends_names, vec!["Bar"]);
    assert!(entry.implements_names.is_empty());
}

#[test]
fn test_heritage_names_captured_for_class_implements() {
    let source = "class Foo implements Iface1, Iface2 {}";
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Foo")
        .expect("expected semantic_def for Foo");
    assert!(entry.extends_names.is_empty());
    assert_eq!(entry.implements_names, vec!["Iface1", "Iface2"]);
}

#[test]
fn test_heritage_names_captured_for_class_extends_and_implements() {
    let source = "class Foo extends Base implements I1, I2 {}";
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Foo")
        .expect("expected semantic_def for Foo");
    assert_eq!(entry.extends_names, vec!["Base"]);
    assert_eq!(entry.implements_names, vec!["I1", "I2"]);
    // Combined heritage_names() accessor should include all
    assert_eq!(entry.heritage_names(), vec!["Base", "I1", "I2"]);
}

#[test]
fn test_heritage_names_captured_for_interface_extends() {
    let source = "interface Foo extends Bar, Baz {}";
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Foo")
        .expect("expected semantic_def for Foo");
    // Interfaces use `extends`, not `implements`
    assert_eq!(entry.extends_names, vec!["Bar", "Baz"]);
    assert!(entry.implements_names.is_empty());
}

#[test]
fn test_heritage_names_empty_for_no_heritage() {
    let source = "class Plain {} interface Empty {}";
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let plain = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Plain")
        .expect("expected semantic_def for Plain");
    assert!(plain.extends_names.is_empty());
    assert!(plain.implements_names.is_empty());

    let empty = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Empty")
        .expect("expected semantic_def for Empty");
    assert!(empty.extends_names.is_empty());
    assert!(empty.implements_names.is_empty());
}

#[test]
fn test_heritage_names_property_access_expression() {
    let source = "class Foo extends ns.Base {}";
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Foo")
        .expect("expected semantic_def for Foo");
    assert_eq!(entry.extends_names, vec!["ns.Base"]);
    assert!(entry.implements_names.is_empty());
}

// =========================================================================
// Stable identity flag tests
// =========================================================================

#[test]
fn semantic_defs_captures_is_abstract_for_abstract_classes() {
    let binder = bind_source(
        "
abstract class Base {}
class Concrete {}
",
    );
    let base_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Base")
        .expect("expected semantic_def for Base");
    assert!(
        base_entry.is_abstract,
        "abstract class Base should have is_abstract=true"
    );
    assert_eq!(base_entry.kind, super::SemanticDefKind::Class);

    let concrete_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Concrete")
        .expect("expected semantic_def for Concrete");
    assert!(
        !concrete_entry.is_abstract,
        "non-abstract class Concrete should have is_abstract=false"
    );
}

#[test]
fn semantic_defs_captures_is_const_for_const_enums() {
    let binder = bind_source(
        "
const enum ConstDir { Up, Down }
enum RegularDir { Left, Right }
",
    );
    let const_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "ConstDir")
        .expect("expected semantic_def for ConstDir");
    assert!(
        const_entry.is_const,
        "const enum ConstDir should have is_const=true"
    );
    assert_eq!(const_entry.kind, super::SemanticDefKind::Enum);

    let regular_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "RegularDir")
        .expect("expected semantic_def for RegularDir");
    assert!(
        !regular_entry.is_const,
        "regular enum RegularDir should have is_const=false"
    );
}

#[test]
fn semantic_defs_captures_is_exported() {
    let binder = bind_source(
        "
export class ExportedClass {}
class PrivateClass {}
export interface ExportedIface {}
interface PrivateIface {}
export type ExportedAlias = number;
type PrivateAlias = string;
export enum ExportedEnum { A }
enum PrivateEnum { B }
",
    );
    for (name, expected_exported) in [
        ("ExportedClass", true),
        ("PrivateClass", false),
        ("ExportedIface", true),
        ("PrivateIface", false),
        ("ExportedAlias", true),
        ("PrivateAlias", false),
        ("ExportedEnum", true),
        ("PrivateEnum", false),
    ] {
        let entry = binder
            .semantic_defs
            .values()
            .find(|e| e.name == name)
            .unwrap_or_else(|| panic!("expected semantic_def for {name}"));
        assert_eq!(
            entry.is_exported, expected_exported,
            "{name}: is_exported should be {expected_exported}"
        );
    }
}

#[test]
fn semantic_defs_identity_flags_survive_lib_merge() {
    // Identity flags (is_abstract, is_const, is_exported) must be
    // preserved when lib semantic_defs are propagated through merge.
    let lib_source = r"
abstract class AbstractError {}
const enum LibDirection { Up, Down }
export interface ExportedArray<T> {}
";
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

    // After merge, identity flags should be preserved on remapped entries
    let abstract_entry = main_binder
        .semantic_defs
        .values()
        .find(|e| e.name == "AbstractError")
        .expect("expected AbstractError in merged semantic_defs");
    assert!(
        abstract_entry.is_abstract,
        "is_abstract should survive lib merge"
    );

    let const_entry = main_binder
        .semantic_defs
        .values()
        .find(|e| e.name == "LibDirection")
        .expect("expected LibDirection in merged semantic_defs");
    assert!(const_entry.is_const, "is_const should survive lib merge");

    let exported_entry = main_binder
        .semantic_defs
        .values()
        .find(|e| e.name == "ExportedArray")
        .expect("expected ExportedArray in merged semantic_defs");
    assert!(
        exported_entry.is_exported,
        "is_exported should survive lib merge"
    );
}

#[test]
fn semantic_defs_all_top_level_families_have_stable_identity() {
    // Verify that all top-level declaration families produce semantic
    // defs with complete identity data, and that nested declarations
    // are not incorrectly captured.
    let binder = bind_source(
        "
export abstract class MyClass<T> extends Base {}
export interface MyInterface<T, U> {}
export type MyAlias<T> = T | null;
export const enum MyConstEnum { A, B, C }
export enum MyEnum { X, Y }
export namespace MyNamespace {}
export function myFunction<T>(x: T): T { return x; }
export const myVariable = 42;
function nested() {
    class InnerClass {}
    interface InnerIface {}
}
",
    );
    // All top-level declarations should be captured
    let names_and_kinds: Vec<(&str, super::SemanticDefKind)> = vec![
        ("MyClass", super::SemanticDefKind::Class),
        ("MyInterface", super::SemanticDefKind::Interface),
        ("MyAlias", super::SemanticDefKind::TypeAlias),
        ("MyConstEnum", super::SemanticDefKind::Enum),
        ("MyEnum", super::SemanticDefKind::Enum),
        ("MyNamespace", super::SemanticDefKind::Namespace),
        ("myFunction", super::SemanticDefKind::Function),
        ("myVariable", super::SemanticDefKind::Variable),
    ];

    for (name, expected_kind) in &names_and_kinds {
        let entry = binder
            .semantic_defs
            .values()
            .find(|e| e.name == *name)
            .unwrap_or_else(|| panic!("expected semantic_def for top-level '{name}'"));
        assert_eq!(entry.kind, *expected_kind, "{name}: wrong kind");
        assert!(entry.is_exported, "{name}: should be exported");
    }

    // Specific flag checks
    let class_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyClass")
        .unwrap();
    assert!(class_entry.is_abstract, "MyClass should be abstract");
    assert_eq!(
        class_entry.type_param_count, 1,
        "MyClass should have 1 type param"
    );

    let const_enum_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyConstEnum")
        .unwrap();
    assert!(const_enum_entry.is_const, "MyConstEnum should be const");
    assert_eq!(const_enum_entry.enum_member_names.len(), 3);

    let regular_enum_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "MyEnum")
        .unwrap();
    assert!(!regular_enum_entry.is_const, "MyEnum should not be const");

    // Nested declarations should NOT be in semantic_defs (they're in function scope)
    assert!(
        !binder
            .semantic_defs
            .values()
            .any(|e| e.name == "InnerClass"),
        "nested InnerClass should not be in semantic_defs"
    );
    assert!(
        !binder
            .semantic_defs
            .values()
            .any(|e| e.name == "InnerIface"),
        "nested InnerIface should not be in semantic_defs"
    );
}

// =============================================================================
// Declaration merging accumulation tests
// =============================================================================

#[test]
fn semantic_defs_declaration_merging_accumulates_heritage_names() {
    // When an interface is declared multiple times with different heritage
    // clauses, heritage_names should accumulate from all declarations.
    let binder = bind_source(
        "
interface Merged extends A { a: string }
interface Merged extends B { b: number }
interface Merged extends C { c: boolean }
",
    );
    let sym_id = binder.file_locals.get("Merged").expect("expected Merged");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for Merged");
    assert_eq!(entry.kind, super::SemanticDefKind::Interface);
    // Heritage names from all three declarations should be accumulated.
    // Interface heritage uses extends, not implements
    assert!(
        entry.extends_names.contains(&"A".to_string()),
        "extends should include A, got: {:?}",
        entry.extends_names
    );
    assert!(
        entry.extends_names.contains(&"B".to_string()),
        "extends should include B, got: {:?}",
        entry.extends_names
    );
    assert!(
        entry.extends_names.contains(&"C".to_string()),
        "extends should include C, got: {:?}",
        entry.extends_names
    );
    // Core identity (name, kind, span) should still be from the first declaration.
    let first_decl = binder.symbols.get(sym_id).unwrap().declarations[0];
    assert_eq!(entry.span_start, first_decl.0);
}

#[test]
fn semantic_defs_declaration_merging_deduplicates_heritage() {
    // If the same heritage name appears in multiple declarations,
    // it should appear only once.
    let binder = bind_source(
        "
interface Dup extends Base { a: string }
interface Dup extends Base { b: number }
",
    );
    let sym_id = binder.file_locals.get("Dup").expect("expected Dup");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for Dup");
    let base_count = entry.extends_names.iter().filter(|h| *h == "Base").count();
    assert_eq!(
        base_count, 1,
        "Base should appear exactly once in extends_names"
    );
}

#[test]
fn semantic_defs_declaration_merging_promotes_type_param_count() {
    // If the first interface declaration has no type params but a later
    // one does (augmentation adding generics), the count should be promoted.
    let binder = bind_source(
        "
interface Augmented { base: string }
interface Augmented<T> { extra: T }
",
    );
    let sym_id = binder
        .file_locals
        .get("Augmented")
        .expect("expected Augmented");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for Augmented");
    assert_eq!(
        entry.type_param_count, 1,
        "type_param_count should be promoted from later declaration"
    );
}

#[test]
fn semantic_defs_declaration_merging_promotes_export_visibility() {
    // If the first declaration is not exported but a later one is,
    // the entry should be marked as exported.
    let binder = bind_source(
        "
interface Internal { a: string }
export interface Internal { b: number }
",
    );
    let sym_id = binder
        .file_locals
        .get("Internal")
        .expect("expected Internal");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for Internal");
    assert!(
        entry.is_exported,
        "export visibility should be promoted from later declaration"
    );
}

#[test]
fn semantic_defs_enum_merging_accumulates_members() {
    // When an enum is declared in multiple blocks, members should accumulate.
    let binder = bind_source(
        "
enum Direction { Up, Down }
enum Direction { Left, Right }
",
    );
    let sym_id = binder
        .file_locals
        .get("Direction")
        .expect("expected Direction");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for Direction");
    assert_eq!(entry.kind, super::SemanticDefKind::Enum);
    // All four members should be accumulated.
    assert!(
        entry.enum_member_names.contains(&"Up".to_string()),
        "should contain Up"
    );
    assert!(
        entry.enum_member_names.contains(&"Down".to_string()),
        "should contain Down"
    );
    assert!(
        entry.enum_member_names.contains(&"Left".to_string()),
        "should contain Left"
    );
    assert!(
        entry.enum_member_names.contains(&"Right".to_string()),
        "should contain Right"
    );
}

#[test]
fn semantic_defs_declare_global_captures_declarations() {
    // Declarations inside `declare global {}` blocks are in Module scope
    // and should be captured as semantic defs.
    let binder = bind_source(
        r#"
export {};
declare global {
    interface Window {
        customProp: string;
    }
    type GlobalAlias = string;
    function globalFn(): void;
    const GLOBAL_CONST: number;
}
"#,
    );
    let has_window = binder
        .semantic_defs
        .values()
        .any(|e| e.name == "Window" && e.kind == super::SemanticDefKind::Interface);
    let has_alias = binder
        .semantic_defs
        .values()
        .any(|e| e.name == "GlobalAlias" && e.kind == super::SemanticDefKind::TypeAlias);
    assert!(
        has_window,
        "declare global interface Window should be in semantic_defs"
    );
    assert!(
        has_alias,
        "declare global type alias should be in semantic_defs"
    );
}

#[test]
fn semantic_defs_class_with_extends_and_implements() {
    // Verify heritage_names captures both extends and implements.
    let binder = bind_source(
        "
interface Serializable {}
interface Printable {}
class Base {}
class Derived extends Base implements Serializable, Printable {}
",
    );
    let sym_id = binder.file_locals.get("Derived").expect("expected Derived");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for Derived");
    assert_eq!(entry.kind, super::SemanticDefKind::Class);
    assert!(
        entry.extends_names.contains(&"Base".to_string()),
        "extends should include Base, got: {:?}",
        entry.extends_names
    );
    assert!(
        entry.implements_names.contains(&"Serializable".to_string()),
        "implements should include Serializable, got: {:?}",
        entry.implements_names
    );
    assert!(
        entry.implements_names.contains(&"Printable".to_string()),
        "implements should include Printable, got: {:?}",
        entry.implements_names
    );
}

#[test]
fn semantic_defs_merging_preserves_first_kind_and_span() {
    // Even when later declarations add heritage/type params, the
    // original kind and span_start must be preserved.
    let binder = bind_source(
        "
interface Stable { a: string }
interface Stable extends Extra { b: number }
",
    );
    let sym_id = binder.file_locals.get("Stable").expect("expected Stable");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for Stable");
    assert_eq!(entry.kind, super::SemanticDefKind::Interface);
    // Span should be from the FIRST declaration
    let first_decl = binder.symbols.get(sym_id).unwrap().declarations[0];
    assert_eq!(
        entry.span_start, first_decl.0,
        "span_start must be from first declaration"
    );
    // But heritage should include Extra from the second declaration
    assert!(
        entry.extends_names.contains(&"Extra".to_string()),
        "extends should include Extra from later declaration"
    );
}

#[test]
fn semantic_defs_namespace_then_interface_merge_promotes_to_interface() {
    // When a Namespace declaration is followed by an Interface declaration
    // with the same name, the merged kind must promote to Interface so that
    // type-position references render as `B`, not `typeof B`.
    let binder = bind_source(
        "
namespace B { export const x = 1; }
interface B { name: string; }
",
    );
    let sym_id = binder.file_locals.get("B").expect("expected B");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for B");
    assert_eq!(entry.kind, super::SemanticDefKind::Interface);
}

#[test]
fn semantic_defs_namespace_then_class_merge_promotes_to_class() {
    let binder = bind_source(
        "
namespace C { export const helper = 1; }
class C { method() {} }
",
    );
    let sym_id = binder.file_locals.get("C").expect("expected C");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for C");
    assert_eq!(entry.kind, super::SemanticDefKind::Class);
}

#[test]
fn semantic_defs_namespace_then_type_alias_merge_promotes_to_type_alias() {
    let binder = bind_source(
        "
namespace T { export const v = 1; }
type T = string;
",
    );
    let sym_id = binder.file_locals.get("T").expect("expected T");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for T");
    assert_eq!(entry.kind, super::SemanticDefKind::TypeAlias);
}

#[test]
fn semantic_defs_function_then_namespace_merge_keeps_function() {
    // Function-namespace declaration merging is the namespace-on-callable
    // pattern; the value side is the function. The kind-promotion rule
    // only fires for type-side declarations (Interface/Class/TypeAlias/Enum)
    // merging into a Namespace, so a Function entry must remain unchanged
    // when a Namespace declaration merges in.
    let binder = bind_source(
        "
function f() {}
namespace f { export const helper = 1; }
",
    );
    let sym_id = binder.file_locals.get("f").expect("expected f");
    let entry = binder
        .semantic_defs
        .get(&sym_id)
        .expect("expected semantic def for f");
    assert_eq!(entry.kind, super::SemanticDefKind::Function);
}

// =============================================================================
// BinderFileSummary tests
// =============================================================================

#[test]
fn file_skeleton_captures_exported_defs() {
    let source = r#"
export class Animal {}
export interface Movable { move(): void; }
export type ID = string;
class Internal {}
"#;
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.file_idx = 42;
    binder.bind_source_file(parser.get_arena(), root);

    let skeleton = binder.file_summary();

    assert_eq!(skeleton.file_idx, 42);
    assert!(
        skeleton.is_external_module,
        "has exports so should be external module"
    );

    let names: Vec<&str> = skeleton
        .exported_defs
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert!(
        names.contains(&"Animal"),
        "expected Animal in exported_defs, got: {names:?}"
    );
    assert!(
        names.contains(&"Movable"),
        "expected Movable in exported_defs, got: {names:?}"
    );
    assert!(
        names.contains(&"ID"),
        "expected ID in exported_defs, got: {names:?}"
    );
    assert!(
        !names.contains(&"Internal"),
        "Internal should not be in exported_defs"
    );
}

#[test]
fn file_skeleton_captures_import_sources() {
    let source = r#"
import { foo } from "./utils";
import * as React from "react";
export { bar } from "./helpers";
"#;
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let skeleton = binder.file_summary();

    assert!(skeleton.import_sources.contains(&"./utils".to_string()));
    assert!(skeleton.import_sources.contains(&"react".to_string()));
    assert!(skeleton.import_sources.contains(&"./helpers".to_string()));
}

#[test]
fn file_skeleton_captures_heritage_deps() {
    let source = r#"
export class Dog extends Animal implements Movable {}
export interface FastAnimal extends Animal {}
"#;
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let skeleton = binder.file_summary();

    assert!(
        skeleton.heritage_deps.contains(&"Animal".to_string()),
        "expected Animal in heritage_deps, got: {:?}",
        skeleton.heritage_deps
    );
    assert!(
        skeleton.heritage_deps.contains(&"Movable".to_string()),
        "expected Movable in heritage_deps, got: {:?}",
        skeleton.heritage_deps
    );
}

#[test]
fn file_skeleton_dependency_specifiers_deduplicates() {
    let source = r#"
import { a } from "./shared";
import { b } from "./shared";
export { c } from "./other";
"#;
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let skeleton = binder.file_summary();
    let specs = skeleton.dependency_specifiers();

    // Should deduplicate "./shared"
    let shared_count = specs.iter().filter(|&&s| s == "./shared").count();
    assert_eq!(
        shared_count, 1,
        "dependency_specifiers should deduplicate, got: {specs:?}"
    );
    assert!(specs.contains(&"./other"));
}

#[test]
fn file_skeleton_has_exports_and_heritage_helpers() {
    let source = r#"
const x = 1;
"#;
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let skeleton = binder.file_summary();

    assert!(
        !skeleton.has_exports(),
        "script file should have no exports"
    );
    assert!(
        !skeleton.has_heritage_deps(),
        "simple script should have no heritage deps"
    );
}

#[test]
fn file_skeleton_api_fingerprint_changes_on_export_change() {
    // Two files with different exports should have different fingerprints
    let source_a = r#"export class Foo {}"#;
    let source_b = r#"export class Bar {}"#;

    let mut parser_a = ParserState::new("a.ts".to_string(), source_a.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = ParserState::new("b.ts".to_string(), source_b.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    let fp_a = binder_a.file_summary().api_fingerprint();
    let fp_b = binder_b.file_summary().api_fingerprint();

    assert_ne!(
        fp_a, fp_b,
        "different exports should produce different fingerprints"
    );
}

#[test]
fn file_skeleton_api_fingerprint_stable_across_calls() {
    let source = r#"export interface I { x: number; }"#;
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let fp1 = binder.file_summary().api_fingerprint();
    let fp2 = binder.file_summary().api_fingerprint();
    assert_eq!(fp1, fp2, "fingerprint should be deterministic");
}

#[test]
fn file_skeleton_exported_defs_sorted_deterministically() {
    let source = r#"
export class Zebra {}
export class Alpha {}
export class Middle {}
"#;
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let skeleton = binder.file_summary();
    let names: Vec<&str> = skeleton
        .exported_defs
        .iter()
        .map(|e| e.name.as_str())
        .collect();

    assert_eq!(
        names,
        vec!["Alpha", "Middle", "Zebra"],
        "exported_defs should be sorted by name"
    );
}

#[test]
fn file_skeleton_generic_type_param_count() {
    let source = r#"
export interface Map<K, V> {}
export type Pair<A, B> = [A, B];
export class List<T> {}
"#;
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let skeleton = binder.file_summary();

    for exp in &skeleton.exported_defs {
        match exp.name.as_str() {
            "Map" => assert_eq!(exp.type_param_count, 2, "Map should have 2 type params"),
            "Pair" => assert_eq!(exp.type_param_count, 2, "Pair should have 2 type params"),
            "List" => assert_eq!(exp.type_param_count, 1, "List should have 1 type param"),
            other => panic!("unexpected export: {other}"),
        }
    }
}

#[test]
fn semantic_defs_parent_namespace_is_none_for_top_level() {
    // Top-level (source-file scope) declarations should have parent_namespace = None.
    let binder = bind_source(
        "
class Foo {}
interface Bar {}
type Baz = string;
enum Color { Red }
",
    );
    for entry in binder.semantic_defs.values() {
        assert!(
            entry.parent_namespace.is_none(),
            "top-level '{}' should have parent_namespace = None, got {:?}",
            entry.name,
            entry.parent_namespace
        );
    }
}

#[test]
fn semantic_defs_parent_namespace_set_for_namespace_members() {
    // Declarations inside a namespace body should have parent_namespace
    // pointing to the namespace's SymbolId.
    let binder = bind_source(
        "
namespace NS {
    export interface Inner {}
    export type Alias = string;
    export class Klass {}
    export enum E { A }
}
",
    );
    let ns_sym = binder
        .file_locals
        .get("NS")
        .expect("expected NS in file_locals");

    // The namespace itself should have no parent_namespace
    let ns_entry = binder
        .semantic_defs
        .get(&ns_sym)
        .expect("expected semantic def for NS");
    assert!(
        ns_entry.parent_namespace.is_none(),
        "namespace NS itself should have no parent_namespace"
    );

    // Each member should reference NS as its parent_namespace
    let member_names = ["Inner", "Alias", "Klass", "E"];
    for name in &member_names {
        let entry = binder
            .semantic_defs
            .values()
            .find(|e| e.name == *name)
            .unwrap_or_else(|| panic!("expected semantic def for '{name}'"));
        assert_eq!(
            entry.parent_namespace,
            Some(ns_sym),
            "'{name}' should have parent_namespace = NS (SymbolId {:?}), got {:?}",
            ns_sym,
            entry.parent_namespace
        );
    }
}

#[test]
fn semantic_defs_nested_namespace_parent_chain() {
    // Declarations in nested namespaces should reference the
    // immediate containing namespace, not the outermost one.
    let binder = bind_source(
        "
namespace Outer {
    export namespace Inner {
        export class Deep {}
    }
}
",
    );
    let outer_sym = binder
        .file_locals
        .get("Outer")
        .expect("expected Outer in file_locals");

    // Inner should reference Outer as parent
    let inner_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Inner")
        .expect("expected semantic def for Inner");
    assert_eq!(
        inner_entry.parent_namespace,
        Some(outer_sym),
        "Inner should have Outer as parent_namespace"
    );

    // Deep should reference Inner as parent (not Outer)
    let inner_sym = binder
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Inner")
        .map(|(&id, _)| id)
        .expect("expected to find Inner's SymbolId");
    let deep_entry = binder
        .semantic_defs
        .values()
        .find(|e| e.name == "Deep")
        .expect("expected semantic def for Deep");
    assert_eq!(
        deep_entry.parent_namespace,
        Some(inner_sym),
        "Deep should have Inner as parent_namespace, not Outer"
    );
}

#[test]
fn merge_cross_file_accumulates_heritage() {
    // SemanticDefEntry::merge_cross_file should accumulate heritage_names
    // from a second entry without duplicating existing ones.
    let mut first = super::SemanticDefEntry {
        kind: super::SemanticDefKind::Interface,
        name: "Foo".to_string(),
        file_id: 0,
        span_start: 0,
        type_param_count: 0,
        type_param_names: Vec::new(),
        is_exported: false,
        enum_member_names: Vec::new(),
        is_const: false,
        is_abstract: false,
        extends_names: vec!["Bar".to_string()],
        implements_names: Vec::new(),
        parent_namespace: None,
        is_global_augmentation: false,
        is_declare: false,
    };
    let second = super::SemanticDefEntry {
        kind: super::SemanticDefKind::Interface,
        name: "Foo".to_string(),
        file_id: 1,
        span_start: 100,
        type_param_count: 2,
        type_param_names: vec!["T".to_string(), "U".to_string()],
        is_exported: true,
        enum_member_names: Vec::new(),
        is_const: false,
        is_abstract: false,
        extends_names: vec!["Bar".to_string(), "Baz".to_string()],
        implements_names: Vec::new(),
        parent_namespace: None,
        is_global_augmentation: false,
        is_declare: false,
    };

    first.merge_cross_file(&second);

    // Heritage: Bar (already present, not duplicated) + Baz (new)
    assert_eq!(first.extends_names, vec!["Bar", "Baz"]);
    // Type param arity and names updated from 0 to 2
    assert_eq!(first.type_param_count, 2);
    assert_eq!(first.type_param_names, vec!["T", "U"]);
    // Export promoted
    assert!(first.is_exported);
    // Core identity preserved from first
    assert_eq!(first.file_id, 0);
    assert_eq!(first.span_start, 0);
}

#[test]
fn merge_cross_file_accumulates_enum_members() {
    let mut first = super::SemanticDefEntry {
        kind: super::SemanticDefKind::Enum,
        name: "Color".to_string(),
        file_id: 0,
        span_start: 0,
        type_param_count: 0,
        type_param_names: Vec::new(),
        is_exported: false,
        enum_member_names: vec!["Red".to_string(), "Green".to_string()],
        is_const: false,
        is_abstract: false,
        extends_names: Vec::new(),
        implements_names: Vec::new(),
        parent_namespace: None,
        is_global_augmentation: false,
        is_declare: false,
    };
    let second = super::SemanticDefEntry {
        kind: super::SemanticDefKind::Enum,
        name: "Color".to_string(),
        file_id: 1,
        span_start: 50,
        type_param_count: 0,
        type_param_names: Vec::new(),
        is_exported: false,
        enum_member_names: vec!["Green".to_string(), "Blue".to_string()],
        is_const: true,
        is_abstract: false,
        extends_names: Vec::new(),
        implements_names: Vec::new(),
        parent_namespace: None,
        is_global_augmentation: false,
        is_declare: false,
    };

    first.merge_cross_file(&second);

    assert_eq!(
        first.enum_member_names,
        vec!["Red", "Green", "Blue"],
        "Should accumulate unique enum members"
    );
    assert!(first.is_const, "const flag should be promoted");
}

#[test]
fn merge_cross_file_does_not_downgrade_type_param_count() {
    // If the first declaration has type params, a later declaration without
    // them should not reset the count.
    let mut first = super::SemanticDefEntry {
        kind: super::SemanticDefKind::Interface,
        name: "Foo".to_string(),
        file_id: 0,
        span_start: 0,
        type_param_count: 3,
        type_param_names: vec!["A".to_string(), "B".to_string(), "C".to_string()],
        is_exported: true,
        enum_member_names: Vec::new(),
        is_const: false,
        is_abstract: false,
        extends_names: Vec::new(),
        implements_names: Vec::new(),
        parent_namespace: None,
        is_global_augmentation: false,
        is_declare: false,
    };
    let second = super::SemanticDefEntry {
        kind: super::SemanticDefKind::Interface,
        name: "Foo".to_string(),
        file_id: 1,
        span_start: 50,
        type_param_count: 0,
        type_param_names: Vec::new(),
        is_exported: false,
        enum_member_names: Vec::new(),
        is_const: false,
        is_abstract: false,
        extends_names: vec!["Extra".to_string()],
        implements_names: Vec::new(),
        parent_namespace: None,
        is_global_augmentation: false,
        is_declare: false,
    };

    first.merge_cross_file(&second);

    assert_eq!(
        first.type_param_count, 3,
        "type_param_count should not decrease"
    );
    assert!(first.is_exported, "export flag should not be lost");
    assert_eq!(first.extends_names, vec!["Extra"]);
}

// =============================================================================
// Stable Identity Tests: All Declaration Families
// =============================================================================
