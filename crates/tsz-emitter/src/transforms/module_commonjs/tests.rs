use super::collect_export_names;
use tsz_parser::ParserState;

/// When a module has two `export namespace N {}` blocks (merged declarations),
/// `collect_export_names` must return `N` only once, matching tsc's behavior
/// for the `exports.N = void 0` initialization line.
#[test]
fn collect_export_names_deduplicates_merged_namespaces() {
    let source =
        "export namespace N { export const a = 1; }\nexport namespace N { export const b = 2; }\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let sf_node = parser.arena.get(root).unwrap();
    let stmts = parser.arena.get_source_file(sf_node).unwrap();
    let names = collect_export_names(&parser.arena, &stmts.statements.nodes);

    let n_count = names.iter().filter(|n| n.as_str() == "N").count();
    assert_eq!(
        n_count, 1,
        "Merged namespace declarations should produce exactly one export name, got: {names:?}"
    );
}

/// When exports are unique, deduplication should not remove anything.
#[test]
fn collect_export_names_preserves_unique_names() {
    let source = "export const a = 1;\nexport const b = 2;\nexport function c() {}\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let sf_node = parser.arena.get(root).unwrap();
    let stmts = parser.arena.get_source_file(sf_node).unwrap();
    let names = collect_export_names(&parser.arena, &stmts.statements.nodes);

    assert_eq!(
        names.len(),
        3,
        "All unique names should be preserved: {names:?}"
    );
    assert!(names.contains(&"a".to_string()));
    assert!(names.contains(&"b".to_string()));
    assert!(names.contains(&"c".to_string()));
}
