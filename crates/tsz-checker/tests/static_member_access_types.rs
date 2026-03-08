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

#[test]
fn auto_accessor_assignments_keep_concrete_types() {
    let source = r#"
class C {
    accessor x;
    accessor y;
    accessor z;
    accessor 0;

    constructor(seed: number) {
        this['x'] = [seed];
        this['y'] = { seed };
        this['z'] = `${seed}`;
        this[0] = [seed];
    }
}
const instance = new C(1);
const xValue = instance.x;
const yValue = instance.y;
const zValue = instance.z;
const zeroValue = instance[0];

class StaticC {
    static accessor x;
    static {
        this.x = 1;
    }
    static accessor y = this.x;
    static accessor z;
    static {
        this.z = this.y;
    }
}
const staticX = StaticC.x;
const staticY = StaticC.y;
const staticZ = StaticC.z;
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

    let x_value = checker.get_type_of_node(variable_declaration_initializer_at(&parser, root, 2));
    let y_value = checker.get_type_of_node(variable_declaration_initializer_at(&parser, root, 3));
    let z_value = checker.get_type_of_node(variable_declaration_initializer_at(&parser, root, 4));
    let zero_value =
        checker.get_type_of_node(variable_declaration_initializer_at(&parser, root, 5));
    let static_x = checker.get_type_of_node(variable_declaration_initializer_at(&parser, root, 7));
    let static_y = checker.get_type_of_node(variable_declaration_initializer_at(&parser, root, 8));
    let static_z = checker.get_type_of_node(variable_declaration_initializer_at(&parser, root, 9));

    assert_eq!(
        checker.format_type(x_value),
        "number[]",
        "expected instance auto-accessor array type"
    );
    assert!(
        checker.format_type(y_value).contains("seed: number"),
        "expected instance auto-accessor object type, got: {}",
        checker.format_type(y_value)
    );
    assert_eq!(
        checker.format_type(z_value),
        "string",
        "expected instance auto-accessor string type"
    );
    assert_eq!(
        checker.format_type(zero_value),
        "number[]",
        "expected numeric auto-accessor array type"
    );
    assert_eq!(
        checker.format_type(static_x),
        "number",
        "expected static auto-accessor type from static block"
    );
    assert_eq!(
        checker.format_type(static_y),
        "number",
        "expected static auto-accessor initializer type"
    );
    assert_eq!(
        checker.format_type(static_z),
        "number",
        "expected static auto-accessor type from later static block"
    );
}
