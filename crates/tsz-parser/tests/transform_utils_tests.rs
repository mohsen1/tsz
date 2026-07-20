//! Tests for transform utility helpers (`contains_this_reference`, `contains_arguments_reference`).
use crate::parser::node::NodeAccess;
use crate::parser::test_fixture::parse_source;
use crate::parser::{NodeIndex, ParserState};
use crate::syntax::transform_utils::arrow_captures_lexical_this;
use crate::syntax::transform_utils::contains_arguments_reference;
use crate::syntax::transform_utils::contains_lexical_arrow_function;
use crate::syntax::transform_utils::contains_new_target_reference;
use crate::syntax::transform_utils::contains_this_reference;
use crate::syntax::transform_utils::nearest_enclosing_lexical_this_class;

fn class_member_initializer(source: &str, member_index: usize) -> (ParserState, NodeIndex) {
    let (parser, root) = parse_source(source);
    let sf = parser.get_arena().get_source_file_at(root).unwrap();
    let class_idx = sf.statements.nodes[0];
    let class_node = parser.get_arena().get(class_idx).unwrap();
    let class_data = parser.get_arena().get_class(class_node).unwrap();
    let member_idx = class_data.members.nodes[member_index];
    let member_node = parser.get_arena().get(member_idx).unwrap();
    let initializer = parser
        .get_arena()
        .get_property_decl(member_node)
        .unwrap()
        .initializer;
    (parser, initializer)
}

#[test]
fn contains_this_reference_detects_this_in_function_body() {
    let (parser, root) = parse_source("function f() { return this; }");
    let sf = parser.get_arena().get_source_file_at(root).unwrap();
    let statement = sf.statements.nodes[0];
    let statement_node = parser.get_arena().get(statement).unwrap();
    let func = parser.get_arena().get_function(statement_node).unwrap();
    let body = func.body;

    assert!(contains_this_reference(parser.get_arena(), body));
}

#[test]
fn contains_this_reference_ignores_literal_tree() {
    let (parser, root) = parse_source("function noThis() { return 42; }");
    let sf = parser.get_arena().get_source_file_at(root).unwrap();
    let statement = sf.statements.nodes[0];
    let statement_node = parser.get_arena().get(statement).unwrap();
    let func = parser.get_arena().get_function(statement_node).unwrap();
    let body = func.body;

    assert!(!contains_this_reference(parser.get_arena(), body));
}

#[test]
fn contains_arguments_reference_detects_arguments_in_function_body() {
    let (parser, root) = parse_source("function f() { return arguments; }");
    let sf = parser.get_arena().get_source_file_at(root).unwrap();
    let statement = sf.statements.nodes[0];
    let statement_node = parser.get_arena().get(statement).unwrap();
    let func = parser.get_arena().get_function(statement_node).unwrap();
    let body = func.body;

    assert!(contains_arguments_reference(parser.get_arena(), body));
}

#[test]
fn contains_arguments_reference_ignores_missing_reference() {
    let (parser, root) = parse_source("function noArgs() { return 42; }");
    let sf = parser.get_arena().get_source_file_at(root).unwrap();
    let statement = sf.statements.nodes[0];
    let statement_node = parser.get_arena().get(statement).unwrap();
    let func = parser.get_arena().get_function(statement_node).unwrap();
    let body = func.body;

    assert!(!contains_arguments_reference(parser.get_arena(), body));
}

#[test]
fn contains_new_target_reference_detects_current_function_body() {
    let (parser, root) = parse_source("function f() { return new.target; }");
    let sf = parser.get_arena().get_source_file_at(root).unwrap();
    let statement = sf.statements.nodes[0];
    let statement_node = parser.get_arena().get(statement).unwrap();
    let func = parser.get_arena().get_function(statement_node).unwrap();

    assert!(contains_new_target_reference(parser.get_arena(), func.body));
}

#[test]
fn contains_new_target_reference_follows_arrows_but_not_nested_regular_functions() {
    let (parser, root) = parse_source(
        "function f() { const g = () => new.target; function h() { return new.target; } }",
    );
    let sf = parser.get_arena().get_source_file_at(root).unwrap();
    let statement = sf.statements.nodes[0];
    let statement_node = parser.get_arena().get(statement).unwrap();
    let func = parser.get_arena().get_function(statement_node).unwrap();

    assert!(contains_new_target_reference(parser.get_arena(), func.body));

    let (parser, root) = parse_source("function f() { function h() { return new.target; } }");
    let sf = parser.get_arena().get_source_file_at(root).unwrap();
    let statement = sf.statements.nodes[0];
    let statement_node = parser.get_arena().get(statement).unwrap();
    let func = parser.get_arena().get_function(statement_node).unwrap();

    assert!(!contains_new_target_reference(
        parser.get_arena(),
        func.body
    ));
}

#[test]
fn contains_arguments_reference_ignores_object_literal_property_names() {
    let (parser, root) = parse_source("function f() { foo({ x, arguments: [] }); return 0; }");
    let sf = parser.get_arena().get_source_file_at(root).unwrap();
    let statement = sf.statements.nodes[0];
    let statement_node = parser.get_arena().get(statement).unwrap();
    let func = parser.get_arena().get_function(statement_node).unwrap();
    let body = func.body;

    assert!(!contains_arguments_reference(parser.get_arena(), body));
}

#[test]
fn contains_this_reference_ignores_nested_class_instance_scope() {
    let (parser, initializer) = class_member_initializer(
        "class C { static bar = class Inner { value = this; method() { return this; } } }",
        0,
    );

    assert!(!contains_this_reference(parser.get_arena(), initializer));
}

#[test]
fn contains_this_reference_detects_nested_class_computed_property_names() {
    let (parser, initializer) = class_member_initializer(
        "class C { static c = 'foo'; static bar = class Inner { static [this.c] = 123; [this.c] = 123; } }",
        1,
    );

    assert!(contains_this_reference(parser.get_arena(), initializer));
}

#[test]
fn contains_this_reference_detects_nested_class_heritage_clauses() {
    let (parser, initializer) = class_member_initializer(
        "class C { static Base = class {}; static bar = class Inner extends this.Base {} }",
        1,
    );

    assert!(contains_this_reference(parser.get_arena(), initializer));
}

#[test]
fn contains_lexical_arrow_function_follows_expression_wrappers() {
    for source in [
        "class C { field = [() => this]; }",
        "class C { field = { handler: () => this }; }",
        "class C { field = true ? (() => this) : null; }",
        "class C { field = consume(() => this); }",
        "class C { field = (0, () => this); }",
    ] {
        let (parser, initializer) = class_member_initializer(source, 0);
        assert!(
            contains_lexical_arrow_function(parser.get_arena(), initializer),
            "wrapper should be transparent: {source}",
        );
    }
}

#[test]
fn contains_lexical_arrow_function_stops_at_regular_this_boundaries() {
    for source in [
        "class C { field = function () { return () => this; }; }",
        "class C { field = { method() { return () => this; } }; }",
        "class C { field = class Inner { method = () => this }; }",
    ] {
        let (parser, initializer) = class_member_initializer(source, 0);
        assert!(
            !contains_lexical_arrow_function(parser.get_arena(), initializer),
            "regular `this` owner should be opaque: {source}",
        );
    }
}

#[test]
fn contains_lexical_arrow_function_follows_computed_method_and_accessor_names() {
    for source in [
        "class C { field = { [consume(() => this)]() {} }; }",
        "class C { field = { get [consume(() => this)]() { return 1; } }; }",
        "class C { field = { set [consume(() => this)](value) {} }; }",
    ] {
        let (parser, initializer) = class_member_initializer(source, 0);
        assert!(
            contains_lexical_arrow_function(parser.get_arena(), initializer),
            "computed member name should retain the enclosing lexical scope: {source}",
        );
    }
}

#[test]
fn contains_lexical_arrow_function_follows_nested_class_headers() {
    for source in [
        "class C { field = class Inner extends factory(() => this) {}; }",
        "class C { field = class Inner { [consume(() => this)]() {} }; }",
    ] {
        let (parser, initializer) = class_member_initializer(source, 0);
        assert!(
            contains_lexical_arrow_function(parser.get_arena(), initializer),
            "nested class heritage and computed names run in the outer lexical scope: {source}",
        );
    }
}

#[test]
fn contains_lexical_arrow_function_follows_class_element_and_parameter_decorators() {
    for source in [
        "class C { field = class Inner { @decorate(<U extends string>() => this.value) method() {} }; }",
        "class C { field = class Inner { @decorate(() => this) property = 1; }; }",
        "class C { field = class Inner { @decorate(() => this) get property() { return 1; } }; }",
        "class C { field = class Inner { method(@decorate(() => this) value: unknown) {} }; }",
        "class C { field = class Inner { constructor(@decorate(() => this) value: unknown) {} }; }",
    ] {
        let (parser, initializer) = class_member_initializer(source, 0);
        assert!(
            contains_lexical_arrow_function(parser.get_arena(), initializer),
            "decorator expression should retain the enclosing lexical scope: {source}",
        );
    }
}

#[test]
fn decorator_arrow_lexical_this_owner_skips_the_nested_class() {
    let (parser, inner_class) = class_member_initializer(
        "class Outer<U> { value!: U; field = class Inner { @decorate(() => this.value) method() {} }; }",
        1,
    );
    let arena = parser.get_arena();
    let arrow = first_arrow_function(&parser, inner_class);
    let arrow_data = arena
        .get(arrow)
        .and_then(|node| arena.get_function(node))
        .expect("decorator arrow");
    let this_expression = arena
        .get(arrow_data.body)
        .and_then(|node| arena.get_access_expr(node))
        .map(|access| access.expression)
        .expect("this.value body");
    let field = arena
        .get_extended(inner_class)
        .expect("inner class parent")
        .parent;
    let outer_class = arena.get_extended(field).expect("field parent").parent;

    assert_eq!(
        nearest_enclosing_lexical_this_class(arena, this_expression),
        Some(outer_class),
    );
}

#[test]
fn contains_lexical_arrow_function_keeps_nested_class_bodies_and_fields_opaque() {
    for source in [
        "class C { field = class Inner { method() { return () => this; } }; }",
        "class C { field = class Inner { @decorate(plain) method() { return () => this; } }; }",
        "class C { field = class Inner { value = () => this; }; }",
        "class C { field = class Inner { get value() { return () => this; } }; }",
    ] {
        let (parser, initializer) = class_member_initializer(source, 0);
        assert!(
            !contains_lexical_arrow_function(parser.get_arena(), initializer),
            "nested class body should own its arrows: {source}",
        );
    }
}

/// Find the first arrow-function node reachable from `root` (breadth-first).
fn first_arrow_function(parser: &ParserState, root: NodeIndex) -> NodeIndex {
    use crate::parser::syntax_kind_ext::ARROW_FUNCTION;
    let arena = parser.get_arena();
    let mut queue = vec![root];
    while let Some(idx) = queue.pop() {
        if arena
            .get(idx)
            .is_some_and(|node| node.kind == ARROW_FUNCTION)
        {
            return idx;
        }
        queue.extend(arena.get_children(idx));
    }
    panic!("no arrow function found");
}

fn arrow_captures(source: &str) -> bool {
    let (parser, root) = parse_source(source);
    let arrow = first_arrow_function(&parser, root);
    arrow_captures_lexical_this(parser.get_arena(), arrow)
}

#[test]
fn arrow_captures_lexical_this_for_super_method_call() {
    // A `super.m()` call lowers to `_super.prototype.m.call(_this)`, so the
    // arrow captures `this`. Two name choices prove the rule is structural.
    assert!(arrow_captures(
        "class B extends A { m() { var f = () => super.greet(); } }"
    ));
    assert!(arrow_captures(
        "class Dog extends Animal { bark() { var go = () => super.speak(); } }"
    ));
}

#[test]
fn arrow_captures_lexical_this_for_super_element_call() {
    assert!(arrow_captures(
        r#"class B extends A { m() { var f = () => super["greet"](); } }"#
    ));
}

#[test]
fn arrow_does_not_capture_for_super_property_access_only() {
    // Bare `super.x` access lowers to `_super.prototype.x` (no `this`), so an
    // arrow that only reads a super property must not capture `this`.
    assert!(!arrow_captures(
        "class B extends A { m() { var f = () => super.value; } }"
    ));
    assert!(!arrow_captures(
        "class Circle extends Shape { measure() { var g = () => super.area; } }"
    ));
}

#[test]
fn arrow_captures_lexical_this_for_literal_this() {
    assert!(arrow_captures(
        "class B extends A { m() { var f = () => this.value; } }"
    ));
}

#[test]
fn arrow_does_not_capture_without_this_or_super_call() {
    assert!(!arrow_captures(
        "class B extends A { m() { var f = (a, b) => a + b; } }"
    ));
}

#[test]
fn arrow_does_not_capture_for_chained_super_property_call() {
    // `super.a.b()` is a normal call on `super.a` (which lowers to
    // `_super.prototype.a`), not a super call, so it must not capture `this`.
    assert!(!arrow_captures(
        "class B extends A { m() { var f = () => super.a.b(); } }"
    ));
}
