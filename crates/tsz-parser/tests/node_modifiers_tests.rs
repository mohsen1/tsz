//! Unit tests for `crates/tsz-parser/src/parser/node_modifiers.rs`.
//!
//! These helpers (`has_modifier`, `has_modifier_ref`, `find_modifier`,
//! `is_static`, `is_declare`, `is_declare_ref`, `get_visibility_from_modifiers`)
//! are the single source of truth for modifier-list queries used by the
//! binder, checker, emitter, and lowering crates. They previously had only
//! incidental coverage from unrelated declaration tests; this module covers
//! their direct contracts (`None`/empty handling, keyword detection,
//! visibility extraction, and the `_ref` parity variants).
use crate::parser::base::NodeList;
use crate::parser::node::NodeArena;
use crate::parser::syntax_kind_ext;
use crate::parser::test_fixture::parse_source;
use crate::parser::{NodeIndex, ParserState};
use tsz_common::Visibility;
use tsz_scanner::SyntaxKind;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn assert_no_errors(parser: &ParserState, ctx: &str) {
    let diags = parser.get_diagnostics();
    assert!(
        diags.is_empty(),
        "{ctx}: expected no errors, got {}: {:?}",
        diags.len(),
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

fn first_statement(arena: &NodeArena, root: NodeIndex) -> NodeIndex {
    let sf = arena.get_source_file_at(root).expect("source file");
    assert!(!sf.statements.nodes.is_empty(), "no statements");
    sf.statements.nodes[0]
}

// ---------------------------------------------------------------------------
// has_modifier / has_modifier_ref — `None` and empty lists
// ---------------------------------------------------------------------------

#[test]
fn has_modifier_returns_false_when_modifiers_is_none() {
    let arena = NodeArena::new();
    let none: Option<NodeList> = None;
    assert!(!arena.has_modifier(&none, SyntaxKind::ExportKeyword));
    assert!(!arena.has_modifier(&none, SyntaxKind::DeclareKeyword));
    assert!(!arena.has_modifier(&none, SyntaxKind::StaticKeyword));
}

#[test]
fn has_modifier_returns_false_when_modifier_list_is_empty() {
    let arena = NodeArena::new();
    let empty = Some(NodeList::new());
    assert!(!arena.has_modifier(&empty, SyntaxKind::ExportKeyword));
    assert!(!arena.has_modifier(&empty, SyntaxKind::PublicKeyword));
}

#[test]
fn has_modifier_ref_handles_none_and_empty() {
    let arena = NodeArena::new();
    let empty_list = NodeList::new();
    assert!(!arena.has_modifier_ref(None, SyntaxKind::ExportKeyword));
    assert!(!arena.has_modifier_ref(Some(&empty_list), SyntaxKind::ExportKeyword));
}

// ---------------------------------------------------------------------------
// find_modifier — `None` and empty lists
// ---------------------------------------------------------------------------

#[test]
fn find_modifier_returns_none_when_modifiers_is_none() {
    let arena = NodeArena::new();
    let none: Option<NodeList> = None;
    assert_eq!(arena.find_modifier(&none, SyntaxKind::ExportKeyword), None);
}

#[test]
fn find_modifier_returns_none_for_empty_list() {
    let arena = NodeArena::new();
    let empty = Some(NodeList::new());
    assert_eq!(arena.find_modifier(&empty, SyntaxKind::ExportKeyword), None);
}

// ---------------------------------------------------------------------------
// is_static / is_declare — defaults on `None`
// ---------------------------------------------------------------------------

#[test]
fn is_static_returns_false_when_modifiers_is_none() {
    let arena = NodeArena::new();
    let none: Option<NodeList> = None;
    assert!(!arena.is_static(&none));
}

#[test]
fn is_declare_returns_false_when_modifiers_is_none() {
    let arena = NodeArena::new();
    let none: Option<NodeList> = None;
    assert!(!arena.is_declare(&none));
}

#[test]
fn is_declare_ref_returns_false_for_none_and_empty() {
    let arena = NodeArena::new();
    let empty = NodeList::new();
    assert!(!arena.is_declare_ref(None));
    assert!(!arena.is_declare_ref(Some(&empty)));
}

// ---------------------------------------------------------------------------
// get_visibility_from_modifiers — defaults
// ---------------------------------------------------------------------------

#[test]
fn get_visibility_defaults_to_public_when_modifiers_is_none() {
    let arena = NodeArena::new();
    let none: Option<NodeList> = None;
    assert_eq!(
        arena.get_visibility_from_modifiers(&none),
        Visibility::Public
    );
}

#[test]
fn get_visibility_defaults_to_public_for_empty_list() {
    let arena = NodeArena::new();
    let empty = Some(NodeList::new());
    assert_eq!(
        arena.get_visibility_from_modifiers(&empty),
        Visibility::Public
    );
}

// ---------------------------------------------------------------------------
// has_modifier — declare modifier on `declare var`
// ---------------------------------------------------------------------------

#[test]
fn has_modifier_detects_declare_on_variable_statement() {
    let (parser, root) = parse_source("declare var x: number;");
    assert_no_errors(&parser, "declare var");
    let arena = parser.get_arena();
    let stmt_idx = first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let var_stmt = arena.get_variable(stmt_node).expect("variable");
    assert!(arena.has_modifier(&var_stmt.modifiers, SyntaxKind::DeclareKeyword));
    // No other modifiers.
    assert!(!arena.has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword));
    assert!(!arena.has_modifier(&var_stmt.modifiers, SyntaxKind::ConstKeyword));
}

#[test]
fn is_declare_ref_returns_true_for_declare_modifier_list() {
    let (parser, root) = parse_source("declare var x: number;");
    assert_no_errors(&parser, "declare var");
    let arena = parser.get_arena();
    let stmt_idx = first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let var_stmt = arena.get_variable(stmt_node).expect("variable");
    let mods_ref = var_stmt.modifiers.as_ref();
    assert!(arena.is_declare_ref(mods_ref));
    assert!(arena.is_declare(&var_stmt.modifiers));
}

// ---------------------------------------------------------------------------
// has_modifier — multiple keywords on `export declare function`
// ---------------------------------------------------------------------------

#[test]
fn has_modifier_detects_async_and_static_on_method() {
    // Both keywords appear on a single class method modifier list.
    let (parser, root) = parse_source("class C { static async m() {} }");
    assert_no_errors(&parser, "static async method");
    let arena = parser.get_arena();
    let stmt_idx = first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let member_node = arena.get(class.members.nodes[0]).expect("method");
    let method = arena.get_method_decl(member_node).expect("method data");
    assert!(arena.has_modifier(&method.modifiers, SyntaxKind::StaticKeyword));
    assert!(arena.has_modifier(&method.modifiers, SyntaxKind::AsyncKeyword));
    assert!(!arena.has_modifier(&method.modifiers, SyntaxKind::AbstractKeyword));
}

// ---------------------------------------------------------------------------
// is_static — class members
// ---------------------------------------------------------------------------

#[test]
fn is_static_returns_true_for_static_method() {
    let (parser, root) = parse_source("class C { static m(): void {} }");
    assert_no_errors(&parser, "static method");
    let arena = parser.get_arena();
    let stmt_idx = first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let member_node = arena.get(class.members.nodes[0]).expect("method node");
    let method = arena.get_method_decl(member_node).expect("method data");
    assert!(arena.is_static(&method.modifiers));
    assert!(arena.has_modifier(&method.modifiers, SyntaxKind::StaticKeyword));
}

#[test]
fn is_static_returns_false_for_instance_method() {
    let (parser, root) = parse_source("class C { m(): void {} }");
    assert_no_errors(&parser, "instance method");
    let arena = parser.get_arena();
    let stmt_idx = first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let member_node = arena.get(class.members.nodes[0]).expect("method node");
    let method = arena.get_method_decl(member_node).expect("method data");
    assert!(!arena.is_static(&method.modifiers));
}

#[test]
fn is_static_returns_true_for_static_property() {
    let (parser, root) = parse_source("class C { static count: number = 0; }");
    assert_no_errors(&parser, "static property");
    let arena = parser.get_arena();
    let stmt_idx = first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let member_node = arena.get(class.members.nodes[0]).expect("prop node");
    let prop = arena.get_property_decl(member_node).expect("prop data");
    assert!(arena.is_static(&prop.modifiers));
}

// ---------------------------------------------------------------------------
// has_modifier — async modifier on class method
// ---------------------------------------------------------------------------

#[test]
fn has_modifier_detects_async_on_class_method() {
    // For top-level `async function`, the keyword is consumed before the
    // modifier list and surfaces only as `FunctionData::is_async`. On class
    // methods, however, `async` *is* part of the modifier list, so the helper
    // should report it.
    let (parser, root) = parse_source("class C { async m() {} }");
    assert_no_errors(&parser, "async method");
    let arena = parser.get_arena();
    let stmt_idx = first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let member_node = arena.get(class.members.nodes[0]).expect("method");
    let method = arena.get_method_decl(member_node).expect("method data");
    assert!(arena.has_modifier(&method.modifiers, SyntaxKind::AsyncKeyword));
    assert!(!arena.has_modifier(&method.modifiers, SyntaxKind::DeclareKeyword));
    assert!(!arena.is_declare(&method.modifiers));
}

// ---------------------------------------------------------------------------
// has_modifier — abstract class / abstract method
// ---------------------------------------------------------------------------

#[test]
fn has_modifier_detects_abstract_on_class_and_method() {
    let (parser, root) = parse_source("abstract class A { abstract m(): void; }");
    assert_no_errors(&parser, "abstract class");
    let arena = parser.get_arena();
    let stmt_idx = first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    assert!(arena.has_modifier(&class.modifiers, SyntaxKind::AbstractKeyword));

    let member_node = arena.get(class.members.nodes[0]).expect("method");
    let method = arena.get_method_decl(member_node).expect("method data");
    assert!(arena.has_modifier(&method.modifiers, SyntaxKind::AbstractKeyword));
}

// ---------------------------------------------------------------------------
// find_modifier — returns NodeIndex of matching modifier
// ---------------------------------------------------------------------------

#[test]
fn find_modifier_returns_index_of_matching_modifier() {
    // Use a class method which carries multiple modifiers in one list without
    // the export-declaration wrapper that `export declare` produces at the
    // module level.
    let (parser, root) = parse_source("class C { public static readonly m() {} }");
    assert_no_errors(&parser, "public static readonly method");
    let arena = parser.get_arena();
    let stmt_idx = first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let member_node = arena.get(class.members.nodes[0]).expect("method");
    let method = arena.get_method_decl(member_node).expect("method data");
    let public_idx = arena
        .find_modifier(&method.modifiers, SyntaxKind::PublicKeyword)
        .expect("public modifier present");
    let static_idx = arena
        .find_modifier(&method.modifiers, SyntaxKind::StaticKeyword)
        .expect("static modifier present");
    let readonly_idx = arena
        .find_modifier(&method.modifiers, SyntaxKind::ReadonlyKeyword)
        .expect("readonly modifier present");
    assert_ne!(public_idx, static_idx);
    assert_ne!(static_idx, readonly_idx);
    // Each returned index resolves to a node of the matching kind.
    assert_eq!(
        arena.get(public_idx).expect("public node").kind,
        SyntaxKind::PublicKeyword as u16
    );
    assert_eq!(
        arena.get(static_idx).expect("static node").kind,
        SyntaxKind::StaticKeyword as u16
    );
    assert_eq!(
        arena.get(readonly_idx).expect("readonly node").kind,
        SyntaxKind::ReadonlyKeyword as u16
    );
    // No async modifier here.
    assert_eq!(
        arena.find_modifier(&method.modifiers, SyntaxKind::AsyncKeyword),
        None
    );
}

// ---------------------------------------------------------------------------
// get_visibility_from_modifiers — Public / Private / Protected
// ---------------------------------------------------------------------------

#[test]
fn get_visibility_returns_public_for_public_keyword() {
    let (parser, root) = parse_source("class C { public x = 0; }");
    assert_no_errors(&parser, "public x");
    let arena = parser.get_arena();
    let stmt_idx = first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let member_node = arena.get(class.members.nodes[0]).expect("prop node");
    let prop = arena.get_property_decl(member_node).expect("prop data");
    // `public` keyword is recorded but the helper still returns Public.
    assert_eq!(
        arena.get_visibility_from_modifiers(&prop.modifiers),
        Visibility::Public
    );
}

#[test]
fn get_visibility_returns_private_for_private_keyword() {
    let (parser, root) = parse_source("class C { private x = 0; }");
    assert_no_errors(&parser, "private x");
    let arena = parser.get_arena();
    let stmt_idx = first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let member_node = arena.get(class.members.nodes[0]).expect("prop node");
    let prop = arena.get_property_decl(member_node).expect("prop data");
    assert_eq!(
        arena.get_visibility_from_modifiers(&prop.modifiers),
        Visibility::Private
    );
}

#[test]
fn get_visibility_returns_protected_for_protected_keyword() {
    let (parser, root) = parse_source("class C { protected x = 0; }");
    assert_no_errors(&parser, "protected x");
    let arena = parser.get_arena();
    let stmt_idx = first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let member_node = arena.get(class.members.nodes[0]).expect("prop node");
    let prop = arena.get_property_decl(member_node).expect("prop data");
    assert_eq!(
        arena.get_visibility_from_modifiers(&prop.modifiers),
        Visibility::Protected
    );
}

#[test]
fn get_visibility_returns_public_for_unrelated_modifiers() {
    // `static readonly` has no visibility keyword — should default to Public.
    let (parser, root) = parse_source("class C { static readonly x = 0; }");
    assert_no_errors(&parser, "static readonly x");
    let arena = parser.get_arena();
    let stmt_idx = first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let member_node = arena.get(class.members.nodes[0]).expect("prop node");
    let prop = arena.get_property_decl(member_node).expect("prop data");
    assert_eq!(
        arena.get_visibility_from_modifiers(&prop.modifiers),
        Visibility::Public
    );
}

#[test]
fn get_visibility_returns_private_when_private_combined_with_static() {
    let (parser, root) = parse_source("class C { private static x = 0; }");
    assert_no_errors(&parser, "private static x");
    let arena = parser.get_arena();
    let stmt_idx = first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let member_node = arena.get(class.members.nodes[0]).expect("prop node");
    let prop = arena.get_property_decl(member_node).expect("prop data");
    assert_eq!(
        arena.get_visibility_from_modifiers(&prop.modifiers),
        Visibility::Private
    );
    // And the static helper sees the static keyword too.
    assert!(arena.is_static(&prop.modifiers));
}

#[test]
fn get_visibility_returns_protected_for_protected_method() {
    let (parser, root) = parse_source("class C { protected m(): void {} }");
    assert_no_errors(&parser, "protected method");
    let arena = parser.get_arena();
    let stmt_idx = first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let member_node = arena.get(class.members.nodes[0]).expect("method");
    let method = arena.get_method_decl(member_node).expect("method data");
    assert_eq!(
        arena.get_visibility_from_modifiers(&method.modifiers),
        Visibility::Protected
    );
}

// ---------------------------------------------------------------------------
// Constructor / parameter properties
// ---------------------------------------------------------------------------

#[test]
fn parameter_property_visibility_is_extracted_from_parameter_modifiers() {
    let (parser, root) = parse_source(
        "class C { constructor(public p: number, private q: string, protected r: boolean) {} }",
    );
    assert_no_errors(&parser, "parameter properties");
    let arena = parser.get_arena();
    let stmt_idx = first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let ctor_node = arena.get(class.members.nodes[0]).expect("ctor");
    assert_eq!(ctor_node.kind, syntax_kind_ext::CONSTRUCTOR);
    let ctor = arena.get_constructor(ctor_node).expect("ctor data");

    let p0 = arena.get(ctor.parameters.nodes[0]).expect("p0");
    let p1 = arena.get(ctor.parameters.nodes[1]).expect("p1");
    let p2 = arena.get(ctor.parameters.nodes[2]).expect("p2");
    let pp0 = arena.get_parameter(p0).expect("pp0");
    let pp1 = arena.get_parameter(p1).expect("pp1");
    let pp2 = arena.get_parameter(p2).expect("pp2");

    assert_eq!(
        arena.get_visibility_from_modifiers(&pp0.modifiers),
        Visibility::Public
    );
    assert_eq!(
        arena.get_visibility_from_modifiers(&pp1.modifiers),
        Visibility::Private
    );
    assert_eq!(
        arena.get_visibility_from_modifiers(&pp2.modifiers),
        Visibility::Protected
    );
}

// ---------------------------------------------------------------------------
// has_modifier_ref vs has_modifier — parity
// ---------------------------------------------------------------------------

#[test]
fn has_modifier_ref_matches_has_modifier_for_owned_lists() {
    let (parser, root) = parse_source("abstract class K {}");
    assert_no_errors(&parser, "abstract class");
    let arena = parser.get_arena();
    let stmt_idx = first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let mods_ref = class.modifiers.as_ref();
    for kw in [
        SyntaxKind::ExportKeyword,
        SyntaxKind::DeclareKeyword,
        SyntaxKind::AbstractKeyword,
        SyntaxKind::AsyncKeyword,
    ] {
        assert_eq!(
            arena.has_modifier_ref(mods_ref, kw),
            arena.has_modifier(&class.modifiers, kw),
            "mismatch for {kw:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Readonly modifier on properties — covered by has_modifier
// ---------------------------------------------------------------------------

#[test]
fn has_modifier_detects_readonly_on_property() {
    let (parser, root) = parse_source("class C { readonly value = 1; }");
    assert_no_errors(&parser, "readonly property");
    let arena = parser.get_arena();
    let stmt_idx = first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let member_node = arena.get(class.members.nodes[0]).expect("prop node");
    let prop = arena.get_property_decl(member_node).expect("prop data");
    assert!(arena.has_modifier(&prop.modifiers, SyntaxKind::ReadonlyKeyword));
    assert!(!arena.is_static(&prop.modifiers));
}

// ---------------------------------------------------------------------------
// Override keyword — modern TS feature
// ---------------------------------------------------------------------------

#[test]
fn has_modifier_detects_override_on_method() {
    let (parser, root) =
        parse_source("class A { m(): void {} } class B extends A { override m(): void {} }");
    assert_no_errors(&parser, "override method");
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).expect("sf");
    let class_b_node = arena.get(sf.statements.nodes[1]).expect("B");
    let class_b = arena.get_class(class_b_node).expect("class B");
    let m_node = arena.get(class_b.members.nodes[0]).expect("method");
    let method = arena.get_method_decl(m_node).expect("method data");
    assert!(arena.has_modifier(&method.modifiers, SyntaxKind::OverrideKeyword));
}
