use super::*;
use crate::{ParserState, syntax_kind_ext};
use std::mem::size_of;
use tsz_common::interner::Atom;
use tsz_scanner::SyntaxKind;

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
    let node = arena.get(token).expect("node should exist in arena");
    assert_eq!(node.kind, SyntaxKind::AsteriskToken as u16);
    assert_eq!(node.pos, 0);
    assert_eq!(node.end, 5);
    assert!(!node.has_data());

    let node = arena.get(ident).expect("node should exist in arena");
    assert_eq!(node.kind, SyntaxKind::Identifier as u16);
    assert!(node.has_data());

    let data = arena
        .get_identifier(node)
        .expect("identifier data should exist");
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
    let view = NodeView::new(&arena, ident_idx).expect("NodeView creation should succeed");
    assert_eq!(view.kind(), SyntaxKind::Identifier as u16);
    assert_eq!(view.pos(), 10);
    assert_eq!(view.end(), 15);
    assert!(view.has_data());

    let ident = view.as_identifier().expect("view should be identifier");
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
    let info = arena.node_info(ident_idx).expect("node info should exist");
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
    let left_extended = arena
        .get_extended(left_ident)
        .expect("extended data should exist");
    assert_eq!(
        left_extended.parent, binary_expr,
        "Left identifier should have binary expression as parent"
    );

    let right_extended = arena
        .get_extended(right_ident)
        .expect("extended data should exist");
    assert_eq!(
        right_extended.parent, binary_expr,
        "Right identifier should have binary expression as parent"
    );

    // Verify binary expression has no parent (it's the root)
    let binary_extended = arena
        .get_extended(binary_expr)
        .expect("extended data should exist");
    assert!(
        binary_extended.parent.is_none(),
        "Binary expression should have no parent (it's the root)"
    );
}

#[test]
fn test_reserved_function_name_does_not_swallow_following_interface_declaration() {
    let source = "function function() { }\ninterface void { }\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let source_file = arena
        .get_source_file_at(root)
        .expect("source file should exist");
    assert_eq!(
        source_file.statements.nodes.len(),
        2,
        "expected function + interface statements after recovery"
    );

    let function_stmt = arena
        .get(source_file.statements.nodes[0])
        .expect("function statement should exist");
    assert_eq!(
        function_stmt.kind,
        syntax_kind_ext::FUNCTION_DECLARATION,
        "first statement should stay a function declaration"
    );

    let interface_stmt = arena
        .get(source_file.statements.nodes[1])
        .expect("interface statement should exist");
    assert_eq!(
        interface_stmt.kind,
        syntax_kind_ext::INTERFACE_DECLARATION,
        "second statement should stay an interface declaration"
    );

    let iface = arena
        .get_interface(interface_stmt)
        .expect("interface data should exist");
    let name = arena
        .get_identifier_at(iface.name)
        .expect("interface name should be preserved");
    assert_eq!(name.escaped_text, "void");
    assert_eq!(arena.kind(iface.name), Some(SyntaxKind::Identifier as u16));
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
    assert_eq!(
        arena
            .get_extended(a)
            .expect("extended data should exist")
            .parent,
        add
    );
    assert_eq!(
        arena
            .get_extended(b)
            .expect("extended data should exist")
            .parent,
        add
    );
    assert_eq!(
        arena
            .get_extended(add)
            .expect("extended data should exist")
            .parent,
        multiply
    );
    assert_eq!(
        arena
            .get_extended(c)
            .expect("extended data should exist")
            .parent,
        multiply
    );
    assert!(
        arena
            .get_extended(multiply)
            .expect("extended data should exist")
            .parent
            .is_none()
    );
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
            has_invalid_escape: false,
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
        arena
            .get_extended(name)
            .expect("extended data should exist")
            .parent,
        func,
        "Function name should have function as parent"
    );
    assert_eq!(
        arena
            .get_extended(block)
            .expect("extended data should exist")
            .parent,
        func,
        "Function body should have function as parent"
    );
    assert_eq!(
        arena
            .get_extended(return_stmt)
            .expect("extended data should exist")
            .parent,
        block,
        "Return statement should have block as parent"
    );
    assert_eq!(
        arena
            .get_extended(literal)
            .expect("extended data should exist")
            .parent,
        return_stmt,
        "Literal should have return statement as parent"
    );
    assert!(
        arena
            .get_extended(func)
            .expect("extended data should exist")
            .parent
            .is_none(),
        "Function should have no parent"
    );
}

// ============================================================================
// NodeArena::estimated_size_bytes tests
// ============================================================================

#[test]
fn estimated_size_bytes_default_arena_is_nonzero() {
    let arena = NodeArena::default();
    let size = arena.estimated_size_bytes();
    assert!(
        size > 0,
        "estimated_size_bytes should be nonzero even for a default arena (struct overhead)"
    );
    // At minimum it should account for size_of::<NodeArena>()
    assert!(
        size >= size_of::<NodeArena>(),
        "estimated_size_bytes ({}) should be >= size_of::<NodeArena>() ({})",
        size,
        size_of::<NodeArena>(),
    );
}

#[test]
fn estimated_size_bytes_grows_with_nodes() {
    let mut arena = NodeArena::new();
    let baseline = arena.estimated_size_bytes();

    // Add several tokens to grow the nodes Vec
    for i in 0..100 {
        arena.add_token(SyntaxKind::AsteriskToken as u16, i * 2, i * 2 + 1);
    }
    let after_tokens = arena.estimated_size_bytes();
    assert!(
        after_tokens > baseline,
        "estimated_size_bytes should grow after adding nodes: {after_tokens} vs {baseline}",
    );
}

#[test]
fn estimated_size_bytes_grows_with_identifiers() {
    let mut arena = NodeArena::new();
    let baseline = arena.estimated_size_bytes();

    // Add identifiers with string data
    for i in 0..50 {
        let name = format!("identifier_{i}_with_some_length");
        arena.add_identifier(
            SyntaxKind::Identifier as u16,
            0,
            10,
            IdentifierData {
                atom: Atom::NONE,
                escaped_text: name,
                original_text: None,
                type_arguments: None,
            },
        );
    }
    let after_ids = arena.estimated_size_bytes();
    assert!(
        after_ids > baseline,
        "estimated_size_bytes should grow after adding identifiers: {after_ids} vs {baseline}",
    );
    // The growth should account for string heap data (each identifier ~30 chars)
    // 50 identifiers * ~30 bytes = ~1500 bytes of string data minimum
    let growth = after_ids - baseline;
    assert!(
        growth >= 1000,
        "growth ({growth}) should account for heap string data in identifiers",
    );
}

#[test]
fn estimated_size_bytes_grows_with_literals() {
    let mut arena = NodeArena::new();
    let baseline = arena.estimated_size_bytes();

    // Add string literals
    for i in 0..30 {
        let text = format!("literal string value number {i}");
        arena.add_literal(
            SyntaxKind::StringLiteral as u16,
            0,
            50,
            LiteralData {
                text,
                raw_text: None,
                value: None,
                has_invalid_escape: false,
            },
        );
    }
    let after_lits = arena.estimated_size_bytes();
    assert!(
        after_lits > baseline,
        "estimated_size_bytes should grow after adding literals: {after_lits} vs {baseline}",
    );
}

#[test]
fn estimated_size_bytes_larger_arena_beats_smaller() {
    // Parse a small source
    let small_arena = {
        let mut state = ParserState::new("test.ts".into(), "let x = 1;".into());
        state.parse_source_file();
        state.into_arena()
    };

    // Parse a larger source
    let large_src = r#"
        interface Foo { a: string; b: number; c: boolean; }
        function bar(x: Foo): string { return x.a; }
        class Baz implements Foo {
            a = "";
            b = 0;
            c = false;
            method(): void {}
        }
        const arr: number[] = [1, 2, 3];
        type Union = string | number | boolean;
        enum Direction { Up, Down, Left, Right }
    "#;
    let large_arena = {
        let mut state = ParserState::new("test.ts".into(), large_src.into());
        state.parse_source_file();
        state.into_arena()
    };

    let small_size = small_arena.estimated_size_bytes();
    let large_size = large_arena.estimated_size_bytes();
    assert!(
        large_size > small_size,
        "larger source should produce larger estimated_size_bytes: large={large_size} vs small={small_size}",
    );
}
