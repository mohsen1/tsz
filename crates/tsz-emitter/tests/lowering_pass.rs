use super::*;
use tsz_parser::parser::ParserState;
use tsz_parser::parser::node::NodeArena;

fn parse(source: &str) -> (NodeArena, NodeIndex) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser.arena, root)
}

#[test]
fn test_lowering_pass_es6_no_transforms() {
    let (arena, root) = parse("class Foo {}");
    let ctx = EmitContext::default();
    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    // ES6 target should not add transforms for classes
    assert!(transforms.is_empty());
}

#[test]
fn test_lowering_pass_es5_class() {
    let (arena, root) = parse("class Foo { constructor(x) { this.x = x; } }");
    let ctx = EmitContext {
        target_es5: true,
        ..EmitContext::default()
    };

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    // ES5 target should add ES5Class transform
    // The actual class node index depends on parser implementation
    // This test validates the architecture, not specific indices
    assert!(!transforms.is_empty(), "Expected ES5 class transform");
}

#[test]
fn test_lowering_pass_commonjs_export() {
    let (arena, root) = parse("export class Foo {}");
    let mut ctx = EmitContext::default();
    ctx.options.module = crate::emitter::ModuleKind::CommonJS;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    // CommonJS module should add export transform
    assert!(!transforms.is_empty(), "Expected CommonJS export transform");
}

#[test]
fn test_lowering_pass_commonjs_export_vars() {
    let (arena, root) = parse("export const a = 1, b = 2;");
    let mut ctx = EmitContext::default();
    ctx.options.module = crate::emitter::ModuleKind::CommonJS;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    assert!(
        !transforms.is_empty(),
        "Expected CommonJS export transform for variables"
    );
}

#[test]
fn test_lowering_pass_commonjs_export_name_indices() {
    let (arena, root) = parse("export const x = 1;");
    let mut ctx = EmitContext::default();
    ctx.options.module = crate::emitter::ModuleKind::CommonJS;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let root_node = arena.get(root).expect("expected source file node");
    let source = arena
        .get_source_file(root_node)
        .expect("expected source file data");
    let stmt_idx = *source.statements.nodes.first().expect("expected statement");
    let stmt_node = arena.get(stmt_idx).expect("expected statement node");
    let var_stmt_idx = if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
        let export_decl = arena
            .get_export_decl(stmt_node)
            .expect("expected export declaration");
        export_decl.export_clause
    } else {
        stmt_idx
    };
    assert!(!var_stmt_idx.is_none(), "expected variable statement node");

    let directive = transforms
        .get(var_stmt_idx)
        .expect("expected CommonJS export directive");
    match directive {
        TransformDirective::CommonJSExport { names, .. } => {
            assert_eq!(names.len(), 1, "Expected single exported name");
            let ident = arena
                .identifiers
                .get(names[0] as usize)
                .expect("expected exported identifier");
            assert_eq!(ident.escaped_text, "x");
        }
        _ => panic!("Expected CommonJSExport directive"),
    }
}

#[test]
fn test_lowering_pass_commonjs_non_export_function_no_transforms() {
    let (arena, root) = parse("function foo() {}");
    let mut ctx = EmitContext::default();
    ctx.options.module = crate::emitter::ModuleKind::CommonJS;

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    assert!(
        transforms.is_empty(),
        "Non-exported functions should not add CommonJS transforms"
    );
}

#[test]
fn test_lowering_pass_nested_arrow_in_class() {
    let (arena, root) = parse("class C { m() { const f = () => this; } }");
    let ctx = EmitContext {
        target_es5: true,
        ..EmitContext::default()
    };

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    assert!(
        transforms.len() >= 2,
        "Expected transforms for class and nested arrow function"
    );
}

#[test]
fn test_malformed_arrow_recovery_not_lowered_to_es5_function() {
    let source = "var v = (a): => {\n\n};";
    let (arena, root) = parse(source);
    let ctx = EmitContext {
        target_es5: true,
        ..EmitContext::default()
    };

    let lowering = LoweringPass::new(&arena, &ctx);
    let transforms = lowering.run(root);

    let root_node = arena.get(root).expect("expected source file node");
    let source_file = arena
        .get_source_file(root_node)
        .expect("expected source file data");
    let stmt_idx = *source_file
        .statements
        .nodes
        .first()
        .expect("expected statement");
    let stmt_node = arena
        .get(stmt_idx)
        .expect("expected variable statement node");
    let var_stmt = arena
        .get_variable(stmt_node)
        .expect("expected variable statement data");
    let decl_list_idx = *var_stmt
        .declarations
        .nodes
        .first()
        .expect("expected declaration list");
    let decl_list_node = arena
        .get(decl_list_idx)
        .expect("expected declaration list node");
    let decl_list = arena
        .get_variable(decl_list_node)
        .expect("expected declaration list data");
    let decl_idx = *decl_list
        .declarations
        .nodes
        .first()
        .expect("expected variable declaration");
    let decl_node = arena.get(decl_idx).expect("expected declaration node");
    let decl = arena
        .get_variable_declaration(decl_node)
        .expect("expected variable declaration data");
    let arrow_idx = decl.initializer;

    assert!(
        !arrow_idx.is_none(),
        "expected malformed arrow function initializer"
    );
    assert!(
        transforms.get(arrow_idx).is_none(),
        "malformed recovery arrow must not be lowered"
    );
}
