use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::{NodeIndex, ParserState};
use tsz_solver::TypeInterner;

fn variable_declaration_initializer_at(
    parser: &ParserState,
    root: NodeIndex,
    stmt_index: usize,
) -> NodeIndex {
    parser
        .get_arena()
        .get(root)
        .and_then(|node| parser.get_arena().get_source_file(node))
        .and_then(|source_file| {
            parser
                .get_arena()
                .get(source_file.statements.nodes[stmt_index])
        })
        .and_then(|node| parser.get_arena().get_variable(node))
        .and_then(|stmt| parser.get_arena().get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.get_arena().get_variable(node))
        .and_then(|decl_list| parser.get_arena().get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.get_arena().get_variable_declaration(node))
        .map(|decl| decl.initializer)
        .expect("missing variable declaration")
}

#[test]
fn class_member_access_keeps_concrete_types() {
    let source = r#"
class C {
    p1: number;
    p2(b: number) {
        return this.p1 + b;
    }
    get p3() {
        return this.p2(this.p1);
    }
    static s1: number;
    static s2(b: number) {
        return C.s1 + b;
    }
    static get s3() {
        return C.s2(C.s1);
    }
}
const instance = new C();
const instanceMethod = instance.p2;
const instanceGetter = instance.p3;
const methodValue = C.s2;
const getterValue = C.s3;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.check_source_file(root);

    let instance_method_init = variable_declaration_initializer_at(&parser, root, 2);
    let instance_getter_init = variable_declaration_initializer_at(&parser, root, 3);
    let method_init = variable_declaration_initializer_at(&parser, root, 4);
    let getter_init = variable_declaration_initializer_at(&parser, root, 5);

    let instance_method_type = checker.get_type_of_node(instance_method_init);
    let instance_getter_type = checker.get_type_of_node(instance_getter_init);
    let method_type = checker.get_type_of_node(method_init);
    let getter_type = checker.get_type_of_node(getter_init);

    assert_eq!(
        checker.format_type(instance_method_type),
        "(b: number) => number",
        "expected instance method property access type"
    );
    assert_eq!(
        checker.format_type(instance_getter_type),
        "number",
        "expected instance getter property access type"
    );
    assert_eq!(
        checker.format_type(method_type),
        "(b: number) => number",
        "expected static method property access type"
    );
    assert_eq!(
        checker.format_type(getter_type),
        "number",
        "expected static getter property access type"
    );
}
