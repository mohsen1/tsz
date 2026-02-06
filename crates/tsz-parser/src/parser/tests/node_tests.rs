use super::*;
use std::mem::size_of;
use tsz_common::interner::Atom;

#[test]
fn test_node_size() {
    // This is the critical test - Node MUST be 16 bytes
    assert_eq!(size_of::<Node>(), 16, "Node must be exactly 16 bytes");

    // 4 nodes per cache line
    let nodes_per_cache_line = 64 / size_of::<Node>();
    assert_eq!(
        nodes_per_cache_line, 4,
        "Should fit 4 Nodes per 64-byte cache line"
    );
}

#[test]
fn test_node_arena_basic() {
    use tsz_scanner::SyntaxKind;

    let mut arena = NodeArena::new();

    // Add a token (no data)
    let token = arena.add_token(SyntaxKind::AsteriskToken as u16, 0, 5);
    assert_eq!(token.0, 0);

    // Add an identifier
    let ident = arena.add_identifier(
        SyntaxKind::Identifier as u16,
        10,
        15,
        IdentifierData {
            atom: Atom::NONE,
            escaped_text: "hello".to_string(),
            original_text: None,
            type_arguments: None,
        },
    );
    assert_eq!(ident.0, 1);

    // Verify we can retrieve them
    let node = arena.get(token).unwrap();
    assert_eq!(node.kind, SyntaxKind::AsteriskToken as u16);
    assert_eq!(node.pos, 0);
    assert_eq!(node.end, 5);
    assert!(!node.has_data());

    let node = arena.get(ident).unwrap();
    assert_eq!(node.kind, SyntaxKind::Identifier as u16);
    assert!(node.has_data());

    let data = arena.get_identifier(node).unwrap();
    assert_eq!(data.escaped_text, "hello");
}

#[test]
fn test_data_pool_sizes() {
    // Verify data pool element sizes are reasonable
    assert!(
        size_of::<IdentifierData>() <= 120,
        "IdentifierData too large"
    );
    assert!(size_of::<FunctionData>() <= 168, "FunctionData too large");
    assert!(size_of::<ClassData>() <= 200, "ClassData too large");
    assert!(
        size_of::<SourceFileData>() <= 200,
        "SourceFileData too large"
    );
}

#[test]
fn test_node_view() {
    use tsz_scanner::SyntaxKind;

    let mut arena = NodeArena::new();

    // Add an identifier
    let ident_idx = arena.add_identifier(
        SyntaxKind::Identifier as u16,
        10,
        15,
        IdentifierData {
            atom: Atom::NONE,
            escaped_text: "myVar".to_string(),
            original_text: None,
            type_arguments: None,
        },
    );

    // Create a view and access data through it
    let view = NodeView::new(&arena, ident_idx).unwrap();
    assert_eq!(view.kind(), SyntaxKind::Identifier as u16);
    assert_eq!(view.pos(), 10);
    assert_eq!(view.end(), 15);
    assert!(view.has_data());

    let ident = view.as_identifier().unwrap();
    assert_eq!(ident.escaped_text, "myVar");
}

#[test]
fn test_node_kind_utilities() {
    use super::super::syntax_kind_ext::*;
    use tsz_scanner::SyntaxKind;

    let ident = Node::new(SyntaxKind::Identifier as u16, 0, 5);
    assert!(ident.is_identifier());
    assert!(!ident.is_string_literal());

    let func = Node::new(FUNCTION_DECLARATION, 0, 100);
    assert!(func.is_function_declaration());
    assert!(func.is_function_like());
    assert!(func.is_declaration());

    let class = Node::new(CLASS_DECLARATION, 0, 200);
    assert!(class.is_class_declaration());
    assert!(class.is_declaration());

    let block = Node::new(BLOCK, 0, 50);
    assert!(block.is_statement());

    let type_ref = Node::new(TYPE_REFERENCE, 0, 10);
    assert!(type_ref.is_type_node());
}

#[test]
fn test_node_access_trait() {
    use tsz_scanner::SyntaxKind;

    let mut arena = NodeArena::new();

    // Add an identifier
    let ident_idx = arena.add_identifier(
        SyntaxKind::Identifier as u16,
        10,
        20,
        IdentifierData {
            atom: Atom::NONE,
            escaped_text: "testVar".to_string(),
            original_text: None,
            type_arguments: None,
        },
    );

    // Test NodeAccess trait methods
    assert!(arena.exists(ident_idx));
    assert!(!arena.exists(NodeIndex::NONE));

    assert_eq!(arena.kind(ident_idx), Some(SyntaxKind::Identifier as u16));
    assert_eq!(arena.pos_end(ident_idx), Some((10, 20)));
    assert_eq!(arena.get_identifier_text(ident_idx), Some("testVar"));

    // Test NodeInfo
    let info = arena.node_info(ident_idx).unwrap();
    assert_eq!(info.kind, SyntaxKind::Identifier as u16);
    assert_eq!(info.pos, 10);
    assert_eq!(info.end, 20);
}

#[test]
fn test_parent_mapping() {
    use crate::parser::syntax_kind_ext::BINARY_EXPRESSION;
    use tsz_scanner::SyntaxKind;

    let mut arena = NodeArena::new();

    // Create a simple expression tree: (a + b)
    // Binary expression with two identifier children
    let left_ident = arena.add_identifier(
        SyntaxKind::Identifier as u16,
        0,
        1,
        IdentifierData {
            atom: Atom::NONE,
            escaped_text: "a".to_string(),
            original_text: None,
            type_arguments: None,
        },
    );

    let right_ident = arena.add_identifier(
        SyntaxKind::Identifier as u16,
        4,
        5,
        IdentifierData {
            atom: Atom::NONE,
            escaped_text: "b".to_string(),
            original_text: None,
            type_arguments: None,
        },
    );

    let binary_expr = arena.add_binary_expr(
        BINARY_EXPRESSION,
        0,
        5,
        BinaryExprData {
            left: left_ident,
            operator_token: SyntaxKind::PlusToken as u16,
            right: right_ident,
        },
    );

    // Verify parent mapping
    let left_extended = arena.get_extended(left_ident).unwrap();
    assert_eq!(
        left_extended.parent, binary_expr,
        "Left identifier should have binary expression as parent"
    );

    let right_extended = arena.get_extended(right_ident).unwrap();
    assert_eq!(
        right_extended.parent, binary_expr,
        "Right identifier should have binary expression as parent"
    );

    // Verify binary expression has no parent (it's the root)
    let binary_extended = arena.get_extended(binary_expr).unwrap();
    assert!(
        binary_extended.parent.is_none(),
        "Binary expression should have no parent (it's the root)"
    );
}

#[test]
fn test_parent_mapping_nested() {
    use crate::parser::syntax_kind_ext::BINARY_EXPRESSION;
    use tsz_scanner::SyntaxKind;

    let mut arena = NodeArena::new();

    // Create nested expression: (a + b) * c
    // This creates a tree where:
    //   multiply
    //   ├─ add
    //   │  ├─ a
    //   │  └─ b
    //   └─ c

    let a = arena.add_identifier(
        SyntaxKind::Identifier as u16,
        0,
        1,
        IdentifierData {
            atom: Atom::NONE,
            escaped_text: "a".to_string(),
            original_text: None,
            type_arguments: None,
        },
    );
    let b = arena.add_identifier(
        SyntaxKind::Identifier as u16,
        4,
        5,
        IdentifierData {
            atom: Atom::NONE,
            escaped_text: "b".to_string(),
            original_text: None,
            type_arguments: None,
        },
    );
    let add = arena.add_binary_expr(
        BINARY_EXPRESSION,
        0,
        5,
        BinaryExprData {
            left: a,
            operator_token: SyntaxKind::PlusToken as u16,
            right: b,
        },
    );

    let c = arena.add_identifier(
        SyntaxKind::Identifier as u16,
        9,
        10,
        IdentifierData {
            atom: Atom::NONE,
            escaped_text: "c".to_string(),
            original_text: None,
            type_arguments: None,
        },
    );
    let multiply = arena.add_binary_expr(
        BINARY_EXPRESSION,
        0,
        10,
        BinaryExprData {
            left: add,
            operator_token: SyntaxKind::AsteriskToken as u16,
            right: c,
        },
    );

    // Verify parent chain: a -> add -> multiply
    assert_eq!(arena.get_extended(a).unwrap().parent, add);
    assert_eq!(arena.get_extended(b).unwrap().parent, add);
    assert_eq!(arena.get_extended(add).unwrap().parent, multiply);
    assert_eq!(arena.get_extended(c).unwrap().parent, multiply);
    assert!(arena.get_extended(multiply).unwrap().parent.is_none());
}

#[test]
fn test_parent_mapping_function() {
    use crate::parser::NodeList;
    use crate::parser::syntax_kind_ext::{BLOCK, FUNCTION_DECLARATION, RETURN_STATEMENT};
    use tsz_scanner::SyntaxKind;

    let mut arena = NodeArena::new();

    // Create a simple function: function foo() { return 42; }
    let name = arena.add_identifier(
        SyntaxKind::Identifier as u16,
        9,
        12,
        IdentifierData {
            atom: Atom::NONE,
            escaped_text: "foo".to_string(),
            original_text: None,
            type_arguments: None,
        },
    );

    let literal = arena.add_literal(
        SyntaxKind::NumericLiteral as u16,
        23,
        25,
        LiteralData {
            text: "42".to_string(),
            raw_text: None,
            value: Some(42.0),
        },
    );

    let return_stmt = arena.add_return(
        RETURN_STATEMENT,
        16,
        26,
        ReturnData {
            expression: literal,
        },
    );

    let block = arena.add_block(
        BLOCK,
        14,
        28,
        BlockData {
            statements: NodeList {
                nodes: vec![return_stmt],
                pos: 16,
                end: 26,
                has_trailing_comma: false,
            },
            multi_line: true,
        },
    );

    let func = arena.add_function(
        FUNCTION_DECLARATION,
        0,
        28,
        FunctionData {
            modifiers: None,
            is_async: false,
            asterisk_token: false,
            name,
            type_parameters: None,
            parameters: NodeList {
                nodes: vec![],
                pos: 13,
                end: 14,
                has_trailing_comma: false,
            },
            type_annotation: NodeIndex::NONE,
            body: block,
            equals_greater_than_token: false,
        },
    );

    // Verify parent chain
    assert_eq!(
        arena.get_extended(name).unwrap().parent,
        func,
        "Function name should have function as parent"
    );
    assert_eq!(
        arena.get_extended(block).unwrap().parent,
        func,
        "Function body should have function as parent"
    );
    assert_eq!(
        arena.get_extended(return_stmt).unwrap().parent,
        block,
        "Return statement should have block as parent"
    );
    assert_eq!(
        arena.get_extended(literal).unwrap().parent,
        return_stmt,
        "Literal should have return statement as parent"
    );
    assert!(
        arena.get_extended(func).unwrap().parent.is_none(),
        "Function should have no parent"
    );
}
