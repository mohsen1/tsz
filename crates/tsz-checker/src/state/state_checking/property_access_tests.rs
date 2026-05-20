use crate::test_utils::check_source_diagnostics;
use crate::{
    context::CheckerOptions, query_boundaries::type_construction::TypeInterner, state::CheckerState,
};
use tsz_binder::BinderState;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::{NodeIndex, ParserState, syntax_kind_ext};

/// Mapped type template with name collision: `MyReadonly`<P> where P is a
/// user type parameter with the same name as the mapped key param.
/// Name-based substitution must be bypassed to avoid incorrectly
/// replacing the outer P with the key literal.
#[test]
fn mapped_type_name_collision_readonly_of_type_param() {
    let diags = check_source_diagnostics(
        "interface Foo { foo(): void }
type MyPartial<T> = { [P in keyof T]?: T[P] };
type MyReadonly<T> = { readonly [P in keyof T]: T[P] };
class A<P extends MyPartial<Foo>> {
    constructor(public props: MyReadonly<P>) {}
    doSomething() {
        this.props.foo && this.props.foo()
    }
}",
    );
    let relevant: Vec<_> = diags.iter().filter(|d| d.code != 2318).collect();
    assert!(
        relevant.is_empty(),
        "expected only TS2318 (if any), got: {:?}",
        relevant
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Property access on a type parameter with a mapped-type constraint
/// should resolve through the constraint.
#[test]
fn type_param_property_access_with_mapped_constraint() {
    let diags = check_source_diagnostics(
        "interface Foo { foo(): void }
type MyPartial<T> = { [P in keyof T]?: T[P] };
function f<P extends MyPartial<Foo>>(p: P) {
    p.foo;
}",
    );
    let relevant: Vec<_> = diags.iter().filter(|d| d.code != 2318).collect();
    assert!(
        relevant.is_empty(),
        "expected only TS2318 (if any), got: {:?}",
        relevant
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

fn build_checker(source: &str) -> (ParserState, NodeIndex, BinderState, TypeInterner) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    (parser, root, binder, types)
}

fn find_node_by_text_and_kind(
    arena: &NodeArena,
    source: &str,
    kind: u16,
    text: &str,
) -> Option<NodeIndex> {
    (0..arena.len()).find_map(|i| {
        let idx = NodeIndex(i as u32);
        let node = arena.get(idx)?;
        (node.kind == kind && &source[node.pos as usize..node.end as usize] == text).then_some(idx)
    })
}

#[test]
fn mapped_type_application_property_resolution_preserves_optional_method_type() {
    let source = "interface Foo { foo(): void }
type MyPartial<T> = { [P in keyof T]?: T[P] };
type MyReadonly<T> = { readonly [P in keyof T]: T[P] };
class A<P extends MyPartial<Foo>> {
    constructor(public props: MyReadonly<P>) {}
    doSomething() {
        this.props.foo && this.props.foo()
    }
}";

    let (parser, root, binder, types) = build_checker(source);
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);

    let call = find_node_by_text_and_kind(
        parser.get_arena(),
        source,
        syntax_kind_ext::CALL_EXPRESSION,
        "this.props.foo()",
    )
    .expect("call expression");
    let callee_access = parser
        .get_arena()
        .get(call)
        .and_then(|node| parser.get_arena().get_call_expr(node))
        .map(|call| call.expression)
        .expect("call callee");
    let object_access = parser
        .get_arena()
        .get(callee_access)
        .and_then(|node| parser.get_arena().get_access_expr(node))
        .map(|access| access.expression)
        .expect("callee object access");

    let object_ty = checker.get_type_of_node(object_access);
    let raw_lookup = checker.resolve_property_access_with_env(object_ty, "foo");
    let tsz_solver::operations::property::PropertyAccessResult::Success { type_id, .. } =
        raw_lookup
    else {
        panic!("expected successful property lookup on MyReadonly<P>, got {raw_lookup:?}");
    };

    let formatted = checker.format_type(type_id);
    assert!(
        formatted.contains("=> void") && formatted.contains("undefined"),
        "expected MyReadonly<P>.foo to preserve optional method type, got {formatted}",
    );
}

#[test]
fn lazy_interface_property_lookup_preserves_inherited_overloads() {
    let source = r#"
interface Base {
    select<T extends string>(value: T): T;
    select<T extends number>(value: T): T;
}
interface Derived extends Base {}
declare const d: Derived;
const s = d.select<string>("x");
const n = d.select<number>(1);
const sCheck: string = s;
const nCheck: number = n;
"#;

    let diags = check_source_diagnostics(source);
    let relevant: Vec<_> = diags.iter().filter(|d| d.code != 2318).collect();
    assert!(
        relevant.is_empty(),
        "expected inherited overloads to resolve without semantic diagnostics, got: {:?}",
        relevant
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );

    let (parser, root, binder, types) = build_checker(source);
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);

    let derived_sym = checker
        .ctx
        .binder
        .file_locals
        .get("Derived")
        .expect("Derived symbol");
    let derived_type = checker.type_reference_symbol_type(derived_sym);
    let result = checker.resolve_property_access_with_env(derived_type, "select");
    let tsz_solver::operations::property::PropertyAccessResult::Success { type_id, .. } = result
    else {
        panic!("expected Derived.select property, got {result:?}");
    };
    let overloads =
        tsz_solver::type_queries::data::get_overload_call_signatures(checker.ctx.types, type_id)
            .expect("select should remain overloaded");
    assert_eq!(overloads.len(), 2);
}

#[test]
fn mapped_enum_discriminant_application_exposes_member_property() {
    let source = r#"
enum ABC { A, B }

type Gen<T extends ABC> = { v: T } & (
  { v: ABC.A, a: string } |
  { v: ABC.B, b: string }
);

type Gen2<T extends ABC> = {
  [Property in keyof Gen<T>]: string;
};

type ProbeGen = Gen<ABC.A>;
type Probe = Gen2<ABC.A>;
"#;

    let (parser, root, binder, types) = build_checker(source);
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);

    let probe_sym = checker
        .ctx
        .binder
        .file_locals
        .get("Probe")
        .expect("Probe symbol");
    let probe_gen_sym = checker
        .ctx
        .binder
        .file_locals
        .get("ProbeGen")
        .expect("ProbeGen symbol");
    let probe_gen_type = checker.type_reference_symbol_type(probe_gen_sym);
    let probe_type = checker.type_reference_symbol_type(probe_sym);
    let gen_a_result = checker.resolve_property_access_with_env(probe_gen_type, "a");
    let a_result = checker.resolve_property_access_with_env(probe_type, "a");

    assert!(
        matches!(
            gen_a_result,
            tsz_solver::operations::property::PropertyAccessResult::Success { .. }
        ),
        "expected ProbeGen.a to resolve, got {gen_a_result:?} for type {}",
        checker.format_type(probe_gen_type),
    );

    assert!(
        matches!(
            a_result,
            tsz_solver::operations::property::PropertyAccessResult::Success { .. }
        ),
        "expected Probe.a to resolve, got {a_result:?} for type {}",
        checker.format_type(probe_type),
    );
}
