use super::*;
use crate::transforms::ir_printer::IRPrinter;
use tsz_parser::parser::ParserState;

/// Helper info for namespace extraction
struct NamespaceInfo {
    ns_idx: NodeIndex,
    is_exported: bool,
}

/// Helper to find the namespace node (unwraps EXPORT_DECLARATION if needed)
fn find_namespace_info(parser: &ParserState, stmt_idx: NodeIndex) -> Option<NamespaceInfo> {
    let stmt_node = parser.arena.get(stmt_idx)?;

    // If it's an export declaration, get the inner namespace
    if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
        if let Some(export_data) = parser.arena.get_export_decl(stmt_node) {
            let inner_node = parser.arena.get(export_data.export_clause)?;
            if inner_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                return Some(NamespaceInfo {
                    ns_idx: export_data.export_clause,
                    is_exported: true,
                });
            }
        }
        return None;
    }

    // Otherwise, if it's a namespace directly
    if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION {
        return Some(NamespaceInfo {
            ns_idx: stmt_idx,
            is_exported: false,
        });
    }

    None
}

/// Helper to parse and transform a namespace, returning the IR node
fn transform_namespace(source: &str) -> Option<IRNode> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    if let Some(root_node) = parser.arena.get(root)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&stmt_idx) = source_file.statements.nodes.first()
    {
        let info = find_namespace_info(&parser, stmt_idx)?;
        let transformer = NamespaceES5Transformer::new(&parser.arena);
        if info.is_exported {
            return transformer.transform_exported_namespace(info.ns_idx);
        } else {
            return transformer.transform_namespace(info.ns_idx);
        }
    }
    None
}

/// Helper to parse, transform and emit a namespace to string
fn transform_and_emit(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    if let Some(root_node) = parser.arena.get(root)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&stmt_idx) = source_file.statements.nodes.first()
    {
        if let Some(info) = find_namespace_info(&parser, stmt_idx) {
            let transformer = NamespaceES5Transformer::new(&parser.arena);
            let ir = if info.is_exported {
                transformer.transform_exported_namespace(info.ns_idx)
            } else {
                transformer.transform_namespace(info.ns_idx)
            };
            if let Some(ir) = ir {
                return IRPrinter::emit_to_string(&ir);
            }
        }
    }
    String::new()
}

/// Helper to parse, transform and emit with CommonJS mode
fn transform_and_emit_commonjs(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    if let Some(root_node) = parser.arena.get(root)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&stmt_idx) = source_file.statements.nodes.first()
    {
        if let Some(info) = find_namespace_info(&parser, stmt_idx) {
            let transformer = NamespaceES5Transformer::with_commonjs(&parser.arena, true);
            let ir = if info.is_exported {
                transformer.transform_exported_namespace(info.ns_idx)
            } else {
                transformer.transform_namespace(info.ns_idx)
            };
            if let Some(ir) = ir {
                return IRPrinter::emit_to_string(&ir);
            }
        }
    }
    String::new()
}

// =========================================================================
// Basic namespace tests
// =========================================================================

#[test]
fn test_namespace_es5_empty_namespace_skipped() {
    let ir = transform_namespace("namespace M { }");
    assert!(ir.is_none(), "Empty namespace should produce no IR");
}

#[test]
fn test_namespace_es5_simple_namespace() {
    let ir = transform_namespace("namespace M { export var x = 1; }");
    assert!(ir.is_some(), "Should produce IR for namespace with content");

    if let Some(IRNode::NamespaceIIFE {
        name,
        name_parts,
        is_exported,
        attach_to_exports,
        ..
    }) = ir
    {
        assert_eq!(name, "M");
        assert_eq!(name_parts, vec!["M"]);
        assert!(!is_exported);
        assert!(!attach_to_exports);
    } else {
        panic!("Expected NamespaceIIFE IR node");
    }
}

#[test]
fn test_namespace_es5_simple_namespace_output() {
    let output = transform_and_emit("namespace M { export var x = 1; }");
    assert!(output.contains("var M;"), "Should declare var M");
    assert!(output.contains("(function (M)"), "Should have IIFE");
    assert!(
        output.contains("(M || (M = {}))"),
        "Should have M || (M = {{}})"
    );
}

#[test]
fn test_namespace_es5_enum_emit_lowered() {
    let output = transform_and_emit("namespace A { export enum Color { Red, Blue } }");
    assert!(
        output.contains("Color[Color[\"Red\"] = 0] = \"Red\""),
        "Should lower enum member assignments inside namespace"
    );
    assert!(
        output.contains("Color = A.Color || (A.Color = {})"),
        "Should inline enum namespace binding in IIFE argument to match tsc"
    );
}

#[test]
fn test_namespace_es5_exported_empty_namespace_skipped() {
    let ir = transform_namespace("export namespace M { }");
    assert!(
        ir.is_none(),
        "Empty exported namespace should produce no IR"
    );
}

#[test]
fn test_namespace_es5_exported_namespace() {
    let ir = transform_namespace("export namespace M { export var x = 1; }");
    assert!(
        ir.is_some(),
        "Should produce IR for exported namespace with content"
    );

    if let Some(IRNode::NamespaceIIFE {
        name,
        is_exported,
        attach_to_exports,
        ..
    }) = ir
    {
        assert_eq!(name, "M");
        assert!(is_exported);
        assert!(!attach_to_exports); // Not in CommonJS mode
    } else {
        panic!("Expected NamespaceIIFE IR node");
    }
}

// =========================================================================
// Qualified namespace name tests (A.B.C)
// =========================================================================

#[test]
fn test_namespace_es5_qualified_name_two_parts() {
    let ir = transform_namespace("namespace A.B { export var x = 1; }");
    assert!(ir.is_some(), "Should produce IR for qualified namespace");

    if let Some(IRNode::NamespaceIIFE {
        name, name_parts, ..
    }) = ir
    {
        assert_eq!(name, "A");
        assert_eq!(name_parts, vec!["A", "B"]);
    } else {
        panic!("Expected NamespaceIIFE IR node");
    }
}

#[test]
fn test_namespace_es5_qualified_name_three_parts() {
    let ir = transform_namespace("namespace A.B.C { export var x = 1; }");
    assert!(ir.is_some(), "Should produce IR for qualified namespace");

    if let Some(IRNode::NamespaceIIFE {
        name, name_parts, ..
    }) = ir
    {
        assert_eq!(name, "A");
        assert_eq!(name_parts, vec!["A", "B", "C"]);
    } else {
        panic!("Expected NamespaceIIFE IR node");
    }
}

#[test]
fn test_namespace_es5_qualified_name_output() {
    let output = transform_and_emit("namespace A.B.C { export var x = 1; }");
    // Should have var declarations for each level
    assert!(output.contains("var A;"), "Should declare var A");
    assert!(
        output.contains("var B;"),
        "Should declare var B inside A's IIFE"
    );
    assert!(
        output.contains("var C;"),
        "Should declare var C inside B's IIFE"
    );
    // Should have nested IIFEs
    assert!(
        output.contains("(function (A)"),
        "Should have outer IIFE for A"
    );
    assert!(
        output.contains("(function (B)"),
        "Should have middle IIFE for B"
    );
    assert!(
        output.contains("(function (C)"),
        "Should have inner IIFE for C"
    );
    // Should have proper argument patterns
    assert!(
        output.contains("A || (A = {})"),
        "Should have A || (A = {{}})"
    );
    assert!(
        output.contains("B = A.B || (A.B = {})"),
        "Should have B = A.B || (A.B = {{}})"
    );
    assert!(
        output.contains("C = B.C || (B.C = {})"),
        "Should have C = B.C || (B.C = {{}})"
    );
}

// =========================================================================
// CommonJS mode tests
// =========================================================================

#[test]
fn test_namespace_es5_commonjs_exported() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "export namespace M { export var x = 1; }".to_string(),
    );
    let root = parser.parse_source_file();

    let ir = if let Some(root_node) = parser.arena.get(root)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&stmt_idx) = source_file.statements.nodes.first()
    {
        if let Some(info) = find_namespace_info(&parser, stmt_idx) {
            let transformer = NamespaceES5Transformer::with_commonjs(&parser.arena, true);
            if info.is_exported {
                transformer.transform_exported_namespace(info.ns_idx)
            } else {
                transformer.transform_namespace(info.ns_idx)
            }
        } else {
            None
        }
    } else {
        None
    };

    assert!(ir.is_some(), "Should produce IR for exported namespace");
    if let Some(IRNode::NamespaceIIFE {
        is_exported,
        attach_to_exports,
        ..
    }) = ir
    {
        assert!(is_exported, "Namespace should be marked as exported");
        assert!(
            attach_to_exports,
            "Should attach to exports in CommonJS mode"
        );
    } else {
        panic!("Expected NamespaceIIFE IR node");
    }
}

#[test]
fn test_namespace_es5_commonjs_exported_output() {
    let output = transform_and_emit_commonjs("export namespace M { export var x = 1; }");
    // In CommonJS mode, exported namespaces attach to exports
    // The pattern is: M = exports.M || (exports.M = {})
    assert!(
        output.contains("exports.M"),
        "Should reference exports.M in CommonJS mode. Got: {}",
        output
    );
}

#[test]
fn test_namespace_es5_commonjs_non_exported() {
    let output = transform_and_emit_commonjs("namespace M { export var x = 1; }");
    // Non-exported namespace in CommonJS mode should not attach to exports
    assert!(
        !output.contains("exports.M"),
        "Non-exported namespace should not reference exports. Got: {}",
        output
    );
}

// =========================================================================
// Declare namespace tests (should be skipped)
// =========================================================================

#[test]
fn test_namespace_es5_declare_namespace_skipped() {
    let ir = transform_namespace("declare namespace M { }");
    assert!(ir.is_none(), "Declare namespaces should be skipped");
}

// =========================================================================
// Namespace with members tests
// =========================================================================

#[test]
fn test_namespace_es5_with_function() {
    let ir = transform_namespace("namespace M { export function foo() { } }");
    assert!(ir.is_some());

    if let Some(IRNode::NamespaceIIFE { body, .. }) = ir {
        assert!(!body.is_empty(), "Body should have function");
        // Check for namespace export
        let has_export = body.iter().any(|node| {
                matches!(
                    node,
                    IRNode::Sequence(nodes) if nodes.iter().any(|n| matches!(n, IRNode::NamespaceExport { namespace, name, .. } if namespace == "M" && name == "foo"))
                )
            });
        assert!(has_export, "Should have namespace export for foo");
    } else {
        panic!("Expected NamespaceIIFE IR node");
    }
}

#[test]
fn test_namespace_es5_with_class() {
    let ir = transform_namespace("namespace M { export class Foo { } }");
    assert!(ir.is_some());

    if let Some(IRNode::NamespaceIIFE { body, .. }) = ir {
        assert!(!body.is_empty(), "Body should have class");
    } else {
        panic!("Expected NamespaceIIFE IR node");
    }
}

#[test]
fn test_namespace_es5_with_variable() {
    let ir = transform_namespace("namespace M { export const x = 1; }");
    assert!(ir.is_some());

    if let Some(IRNode::NamespaceIIFE { body, .. }) = ir {
        assert!(!body.is_empty(), "Body should have variable");
    } else {
        panic!("Expected NamespaceIIFE IR node");
    }
}

#[test]
fn test_namespace_es5_interface_only_skipped() {
    // Namespace with only interfaces is non-instantiated, should be skipped
    let ir = transform_namespace("namespace M { interface Foo { } }");
    assert!(ir.is_none(), "Interface-only namespace should be skipped");
}

#[test]
fn test_namespace_es5_type_alias_only_skipped() {
    // Namespace with only type aliases is non-instantiated, should be skipped
    let ir = transform_namespace("namespace M { type Foo = string; }");
    assert!(ir.is_none(), "Type-alias-only namespace should be skipped");
}

// =========================================================================
// Nested namespace tests
// =========================================================================

#[test]
fn test_namespace_es5_nested_namespace() {
    let ir = transform_namespace("namespace A { namespace B { export var x = 1; } }");
    assert!(ir.is_some());

    if let Some(IRNode::NamespaceIIFE {
        name,
        name_parts,
        body,
        ..
    }) = ir
    {
        assert_eq!(name, "A");
        assert_eq!(name_parts, vec!["A"]);
        // Should have nested namespace in body
        let has_nested = body
            .iter()
            .any(|node| matches!(node, IRNode::NamespaceIIFE { name, .. } if name == "B"));
        assert!(has_nested, "Should have nested namespace B");
    } else {
        panic!("Expected NamespaceIIFE IR node");
    }
}

#[test]
fn test_namespace_es5_nested_empty_namespace_skipped() {
    // Namespace A contains only an empty nested namespace B, which gets skipped.
    // Since A then has no runtime content, A should also be skipped.
    let ir = transform_namespace("namespace A { namespace B { } }");
    assert!(
        ir.is_none(),
        "Namespace with only empty nested namespace should be skipped"
    );
}

#[test]
fn test_namespace_es5_nested_exported_namespace() {
    let ir = transform_namespace("namespace A { export namespace B { export var x = 1; } }");
    assert!(ir.is_some());

    if let Some(IRNode::NamespaceIIFE { body, .. }) = ir {
        // Check nested namespace is exported
        let has_exported_nested = body.iter().any(|node| {
            matches!(
                node,
                IRNode::NamespaceIIFE {
                    name,
                    is_exported: true,
                    ..
                } if name == "B"
            )
        });
        assert!(
            has_exported_nested,
            "Should have exported nested namespace B"
        );
    } else {
        panic!("Expected NamespaceIIFE IR node");
    }
}

// =========================================================================
// Edge case tests
// =========================================================================

#[test]
fn test_namespace_es5_empty_namespace_no_output() {
    let output = transform_and_emit("namespace A { }");
    assert!(
        output.is_empty() || output.trim().is_empty(),
        "Empty namespace should produce no output"
    );
}

#[test]
fn test_namespace_es5_multiple_exports() {
    let ir = transform_namespace("namespace M { export const a = 1; export const b = 2; }");
    assert!(ir.is_some());

    if let Some(IRNode::NamespaceIIFE { body, .. }) = ir {
        assert_eq!(body.len(), 2, "Should have two exports");
    } else {
        panic!("Expected NamespaceIIFE IR node");
    }
}

#[test]
fn test_namespace_es5_transformer_set_commonjs() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "export namespace M { export var x = 1; }".to_string(),
    );
    let root = parser.parse_source_file();

    if let Some(root_node) = parser.arena.get(root)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&ns_idx) = source_file.statements.nodes.first()
    {
        let mut transformer = NamespaceES5Transformer::new(&parser.arena);

        // Initially not CommonJS
        let ir1 = transformer.transform_namespace(ns_idx);
        if let Some(IRNode::NamespaceIIFE {
            attach_to_exports, ..
        }) = ir1
        {
            assert!(!attach_to_exports);
        }

        // Set CommonJS mode
        transformer.set_commonjs(true);
        let ir2 = transformer.transform_namespace(ns_idx);
        if let Some(IRNode::NamespaceIIFE {
            attach_to_exports, ..
        }) = ir2
        {
            assert!(attach_to_exports);
        }
    }
}

// =========================================================================
// Comment preservation tests
// =========================================================================

/// Helper that sets source text for comment extraction
fn transform_and_emit_with_comments(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    if let Some(root_node) = parser.arena.get(root)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&stmt_idx) = source_file.statements.nodes.first()
    {
        if let Some(info) = find_namespace_info(&parser, stmt_idx) {
            let mut transformer = NamespaceES5Transformer::new(&parser.arena);
            transformer.set_source_text(source);
            let ir = if info.is_exported {
                transformer.transform_exported_namespace(info.ns_idx)
            } else {
                transformer.transform_namespace(info.ns_idx)
            };
            if let Some(ir) = ir {
                return IRPrinter::emit_to_string(&ir);
            }
        }
    }
    String::new()
}

#[test]
fn test_namespace_leading_comment_preserved() {
    let source = r#"namespace M {
    // this is a leading comment
    export function foo() { return 1; }
}"#;
    let output = transform_and_emit_with_comments(source);
    assert!(
        output.contains("// this is a leading comment"),
        "Leading comment should be preserved. Got: {}",
        output
    );
}

#[test]
fn test_namespace_trailing_comment_preserved() {
    let source = r#"namespace M {
    export function foo() { return 1; } //trailing comment
}"#;
    let output = transform_and_emit_with_comments(source);
    assert!(
        output.contains("//trailing comment"),
        "Trailing comment should be preserved. Got: {}",
        output
    );
}

#[test]
fn test_namespace_exported_function_trailing_comment_stays_on_function() {
    let source = r#"namespace M {
    export function foo() { return 1; } //trailing comment
}"#;
    let output = transform_and_emit_with_comments(source);
    assert!(
        output.contains("} //trailing comment\n    M.foo = foo;"),
        "Trailing comment should stay on function declaration. Got: {}",
        output
    );
}

#[test]
fn test_namespace_trailing_comment_variable() {
    // Simpler case: variable with trailing comment
    let source = "namespace M { export var x = 1; //comment\n}";
    let output = transform_and_emit_with_comments(source);
    assert!(
        output.contains("//comment"),
        "Trailing comment on variable should be preserved. Got: {}",
        output
    );
}

#[test]
fn test_trailing_comment_extraction_direct() {
    // Directly test that comment ranges are found
    let source = "namespace M { export var x = 1; //comment\n}";
    let ranges = tsz_common::comments::get_comment_ranges(source);
    assert!(
        !ranges.is_empty(),
        "Should find at least one comment range in: {}",
        source
    );
    let comment_text = ranges[0].get_text(source);
    assert_eq!(comment_text, "//comment", "Comment text should match");
}

#[test]
fn test_trailing_comment_ir_structure() {
    // Verify the IR body contains TrailingComment nodes
    let source = "namespace M { export var x = 1; //comment\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    if let Some(root_node) = parser.arena.get(root)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&stmt_idx) = source_file.statements.nodes.first()
    {
        if let Some(info) = find_namespace_info(&parser, stmt_idx) {
            let mut transformer = NamespaceES5Transformer::new(&parser.arena);
            transformer.set_source_text(source);
            let ir = transformer.transform_namespace(info.ns_idx);
            if let Some(IRNode::NamespaceIIFE { body, .. }) = &ir {
                let has_trailing = body.iter().any(|n| matches!(n, IRNode::TrailingComment(_)));
                assert!(
                    has_trailing,
                    "Body should contain TrailingComment node. Body: {:?}",
                    body
                );
            } else {
                panic!("Expected NamespaceIIFE, got: {:?}", ir);
            }
        }
    }
}

#[test]
fn test_namespace_comment_after_erased_interface() {
    // Comment between an erased interface and a value declaration
    // should be preserved. The interface is erased during emit, but
    // the comment in its trailing trivia must survive.
    let source = r#"namespace A {
    export interface Point {
        x: number;
        y: number;
    }

    // valid since Point is exported
    export var Origin: Point = { x: 0, y: 0 };
}"#;
    let output = transform_and_emit_with_comments(source);
    assert!(
        output.contains("// valid since Point is exported"),
        "Comment after erased interface should be preserved. Got:\n{}",
        output
    );
}

#[test]
fn test_namespace_inline_block_comment_preserved() {
    let source = r#"namespace M {
    /* block comment */
    export var x = 1;
}"#;
    let output = transform_and_emit_with_comments(source);
    assert!(
        output.contains("/* block comment */"),
        "Block comment should be preserved. Got: {}",
        output
    );
}
