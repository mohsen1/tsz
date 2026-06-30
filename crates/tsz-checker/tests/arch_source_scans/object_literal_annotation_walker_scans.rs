use std::fs;
use std::path::Path;

#[test]
fn object_literal_annotation_walk_uses_named_visit_state() {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/types/computation/object_literal/conditional_mapped_annotation.rs");
    let source =
        fs::read_to_string(&source_path).expect("read object literal annotation walker source");

    assert!(
        source.contains("struct ObjectLiteralAnnotationWalkState"),
        "object literal annotation predicates should own visit state in a named walker state"
    );
    assert!(
        source.contains("fn enter_type_node(") && source.contains("fn enter_alias("),
        "walker state should name both type-node and alias-cycle entry operations"
    );
    assert!(
        !source.contains("let mut visited_type_nodes")
            && !source.contains("let mut visited_symbols")
            && !source.contains("visited_type_nodes: &mut")
            && !source.contains("visited_symbols: &mut"),
        "annotation walker helpers should not thread raw visit sets through every helper"
    );
}
