#[test]
fn test_index_file_extracts_named_imports() {
    let source = r#"import { foo, bar } from './utils';"#;
    let (binder, parser) = parse_and_bind("app.ts", source);

    let mut index = SymbolIndex::new();
    index.index_file("app.ts", &binder, parser.get_arena(), source);

    let imports = index.get_imports("app.ts");
    assert!(
        imports.len() >= 2,
        "expected at least 2 imports from index_file, got {}",
        imports.len()
    );

    let foo_import = imports.iter().find(|i| i.local_name == "foo");
    assert!(foo_import.is_some(), "expected 'foo' import");
    let foo_import = foo_import.unwrap();
    assert_eq!(foo_import.source_module, "./utils");
    assert_eq!(foo_import.kind, ImportKind::Named);

    let bar_import = imports.iter().find(|i| i.local_name == "bar");
    assert!(bar_import.is_some(), "expected 'bar' import");

    // Reverse import graph should track the dependency
    let importers = index.get_importing_files("./utils");
    assert!(
        importers.contains(&"app.ts".to_string()),
        "expected app.ts in importers of ./utils"
    );
}

#[test]
fn test_index_file_extracts_default_import() {
    let source = r#"import React from 'react';"#;
    let (binder, parser) = parse_and_bind("app.ts", source);

    let mut index = SymbolIndex::new();
    index.index_file("app.ts", &binder, parser.get_arena(), source);

    let imports = index.get_imports("app.ts");
    let react_import = imports.iter().find(|i| i.local_name == "React");
    assert!(react_import.is_some(), "expected 'React' default import");
    let react_import = react_import.unwrap();
    assert_eq!(react_import.source_module, "react");
    assert_eq!(react_import.exported_name, "default");
    assert_eq!(react_import.kind, ImportKind::Default);
}

#[test]
fn test_index_file_extracts_namespace_import() {
    let source = r#"import * as utils from './utils';"#;
    let (binder, parser) = parse_and_bind("app.ts", source);

    let mut index = SymbolIndex::new();
    index.index_file("app.ts", &binder, parser.get_arena(), source);

    let imports = index.get_imports("app.ts");
    let ns_import = imports.iter().find(|i| i.local_name == "utils");
    assert!(ns_import.is_some(), "expected 'utils' namespace import");
    let ns_import = ns_import.unwrap();
    assert_eq!(ns_import.source_module, "./utils");
    assert_eq!(ns_import.exported_name, "*");
    assert_eq!(ns_import.kind, ImportKind::Namespace);
}

#[test]
fn test_index_file_extracts_side_effect_import() {
    let source = r#"import './polyfill';"#;
    let (binder, parser) = parse_and_bind("app.ts", source);

    let mut index = SymbolIndex::new();
    index.index_file("app.ts", &binder, parser.get_arena(), source);

    let imports = index.get_imports("app.ts");
    let side_effect = imports.iter().find(|i| i.source_module == "./polyfill");
    assert!(
        side_effect.is_some(),
        "expected side-effect import for './polyfill'"
    );
    assert_eq!(side_effect.unwrap().kind, ImportKind::SideEffect);

    let importers = index.get_importing_files("./polyfill");
    assert!(
        importers.contains(&"app.ts".to_string()),
        "expected app.ts in importers of ./polyfill"
    );
}

#[test]
fn test_index_file_has_file_after_import_indexing() {
    let source = r#"import { x } from './lib';"#;
    let (binder, parser) = parse_and_bind("consumer.ts", source);

    let mut index = SymbolIndex::new();
    index.index_file("consumer.ts", &binder, parser.get_arena(), source);

    assert!(
        index.has_file("consumer.ts"),
        "has_file should return true after index_file"
    );
}

#[test]
fn test_index_file_renamed_import() {
    let source = r#"import { foo as bar } from './mod';"#;
    let (binder, parser) = parse_and_bind("app.ts", source);

    let mut index = SymbolIndex::new();
    index.index_file("app.ts", &binder, parser.get_arena(), source);

    let imports = index.get_imports("app.ts");
    let renamed = imports.iter().find(|i| i.local_name == "bar");
    assert!(renamed.is_some(), "expected renamed import 'bar'");
    let renamed = renamed.unwrap();
    assert_eq!(renamed.exported_name, "foo");
    assert_eq!(renamed.source_module, "./mod");
    assert_eq!(renamed.kind, ImportKind::Named);
}

#[test]
fn test_index_file_remove_clears_auto_imports() {
    let source = r#"import { x } from './lib';"#;
    let (binder, parser) = parse_and_bind("app.ts", source);

    let mut index = SymbolIndex::new();
    index.index_file("app.ts", &binder, parser.get_arena(), source);

    assert!(!index.get_imports("app.ts").is_empty());
    assert!(!index.get_importing_files("./lib").is_empty());

    index.remove_file("app.ts");

    assert!(
        index.get_imports("app.ts").is_empty(),
        "imports should be cleared after remove_file"
    );
    assert!(
        index.get_importing_files("./lib").is_empty(),
        "importers should be cleared after remove_file"
    );
}

#[test]
fn test_index_file_populates_sub_to_bases_for_class() {
    // Regression: previously the second-pass heritage builder called
    // get_identifier_text on the ClassDeclaration node itself, which
    // always returned None — so sub_to_bases was silently empty and
    // upward heritage lookups returned nothing.
    let source = r#"
        class Base {}
        interface I {}
        class Sub extends Base implements I {}
    "#;
    let (binder, parser) = parse_and_bind("file.ts", source);

    let mut index = SymbolIndex::new();
    index.index_file("file.ts", &binder, parser.get_arena(), source);

    let bases = index.get_bases_for_class("Sub");
    assert!(
        bases.contains(&"Base".to_string()),
        "expected Sub -> Base, got {bases:?}"
    );
    assert!(
        bases.contains(&"I".to_string()),
        "expected Sub -> I (implements), got {bases:?}"
    );
}

#[test]
fn test_index_file_populates_sub_to_bases_for_interface() {
    let source = r#"
        interface Base {}
        interface Mixin {}
        interface Derived extends Base, Mixin {}
    "#;
    let (binder, parser) = parse_and_bind("file.ts", source);

    let mut index = SymbolIndex::new();
    index.index_file("file.ts", &binder, parser.get_arena(), source);

    let bases = index.get_bases_for_class("Derived");
    assert!(bases.contains(&"Base".to_string()), "got {bases:?}");
    assert!(bases.contains(&"Mixin".to_string()), "got {bases:?}");
}

#[test]
fn test_index_file_sub_to_bases_survives_large_class_body() {
    // Previously the second-pass used a 50-node forward-scan to find
    // HERITAGE_CLAUSE siblings — classes with big bodies between the
    // name and heritage clauses would fall outside the window. The fix
    // walks the declaration's own heritage_clauses list, so body size
    // is irrelevant.
    let mut body = String::new();
    for i in 0..60 {
        body.push_str(&format!("    m{i}() {{}}\n"));
    }
    let source = format!("class Base {{}}\nclass Sub extends Base {{\n{body}}}\n");
    let (binder, parser) = parse_and_bind("file.ts", &source);

    let mut index = SymbolIndex::new();
    index.index_file("file.ts", &binder, parser.get_arena(), &source);

    assert_eq!(
        index.get_bases_for_class("Sub"),
        vec!["Base".to_string()],
        "heritage lookup should work regardless of class body size"
    );
}

#[test]
fn test_remove_file_resets_file_owned_sub_to_bases() {
    // Regression: remove_file used to treat sub_to_bases values as file names.
    // The map is actually keyed by subclass/interface name (`Sub -> {Base}`),
    // so removing the file left stale upward heritage edges behind.
    let source = r#"class Base {} class Sub extends Base {}"#;
    let (binder, parser) = parse_and_bind("sub.ts", source);

    let mut index = SymbolIndex::new();
    index.index_file("sub.ts", &binder, parser.get_arena(), source);

    assert!(!index.get_files_with_heritage("Base").is_empty());
    assert_eq!(index.get_bases_for_class("Sub"), vec!["Base".to_string()]);

    index.remove_file("sub.ts");

    assert!(
        index.get_files_with_heritage("Base").is_empty(),
        "downward heritage lookup should be cleared after remove_file"
    );
    assert!(
        index.get_bases_for_class("Sub").is_empty(),
        "upward heritage lookup should be cleared after remove_file"
    );
}

#[test]
fn test_remove_file_preserves_other_file_sub_to_bases_for_same_name() {
    let source_a = r#"class BaseA {} class Sub extends BaseA {}"#;
    let source_b = r#"class BaseB {} class Sub extends BaseB {}"#;
    let (binder_a, parser_a) = parse_and_bind("a.ts", source_a);
    let (binder_b, parser_b) = parse_and_bind("b.ts", source_b);

    let mut index = SymbolIndex::new();
    index.index_file("a.ts", &binder_a, parser_a.get_arena(), source_a);
    index.index_file("b.ts", &binder_b, parser_b.get_arena(), source_b);

    let bases = index.get_bases_for_class("Sub");
    assert!(bases.contains(&"BaseA".to_string()), "got {bases:?}");
    assert!(bases.contains(&"BaseB".to_string()), "got {bases:?}");

    index.remove_file("a.ts");

    let bases = index.get_bases_for_class("Sub");
    assert!(
        !bases.contains(&"BaseA".to_string()),
        "removed file's heritage edge should be gone, got {bases:?}"
    );
    assert!(
        bases.contains(&"BaseB".to_string()),
        "remaining file's heritage edge should survive, got {bases:?}"
    );
}

#[test]
fn test_clear_resets_heritage_and_sub_to_bases() {
    // Regression: clear() used to leave heritage_clauses and sub_to_bases
    // populated, so a fully-rebuilt index would see stale class edges.
    let source = r#"class Base {} class Sub extends Base {}"#;
    let (binder, parser) = parse_and_bind("file.ts", source);

    let mut index = SymbolIndex::new();
    index.index_file("file.ts", &binder, parser.get_arena(), source);

    assert!(!index.get_files_with_heritage("Base").is_empty());
    assert!(!index.get_bases_for_class("Sub").is_empty());

    index.clear();

    assert!(
        index.get_files_with_heritage("Base").is_empty(),
        "heritage_clauses should be cleared"
    );
    assert!(
        index.get_bases_for_class("Sub").is_empty(),
        "sub_to_bases should be cleared"
    );
}
