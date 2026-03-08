use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::{NodeIndex, ParserState, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::{TypeId, TypeInterner};

fn compile_and_collect_diagnostics(source: &str) -> Vec<(u32, String)> {
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

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn build_program(source: &str) -> (ParserState, BinderState, TypeInterner, NodeIndex) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    (parser, binder, types, root)
}

fn find_first_node_by_kind(parser: &ParserState, kind: u16) -> NodeIndex {
    for idx in 0..parser.get_arena().len() {
        let node_idx = NodeIndex(idx as u32);
        if parser
            .get_arena()
            .get(node_idx)
            .is_some_and(|node| node.kind == kind)
        {
            return node_idx;
        }
    }
    panic!("missing node kind {kind}");
}

#[test]
fn recursive_mapped_alias_return_annotation_self_assignment_does_not_overflow() {
    let diagnostics = compile_and_collect_diagnostics(
        r#"
type ReadonlyDeep<T> =
  T extends object ? { readonly [K in keyof T]: ReadonlyDeep<T[K]> } : T;

function asReadonly<T>(value: T): ReadonlyDeep<T> {
  return value as ReadonlyDeep<T>;
}
"#,
    );

    let relevant: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect();

    assert!(
        relevant.is_empty(),
        "Expected no non-lib diagnostics for recursive alias return self-assignment, got: {relevant:?}"
    );
}

#[test]
fn spread_element_type_uses_spread_expression_type() {
    let source = r#"
const parts = [1, 2, 3];
const values = [...parts];
"#;

    let (parser, binder, types, root) = build_program(source);
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.check_source_file(root);

    let spread_idx = find_first_node_by_kind(&parser, syntax_kind_ext::SPREAD_ELEMENT);
    let spread_type = checker.get_type_of_node(spread_idx);

    let parts_ident = find_first_node_by_kind(&parser, SyntaxKind::Identifier as u16);
    let parts_type = checker.get_type_of_node(parts_ident);

    assert_eq!(
        checker.format_type(spread_type),
        checker.format_type(parts_type),
        "expected spread element to reuse the spread expression type"
    );
}

#[test]
fn regex_literal_resolves_to_regexp_type_for_property_access() {
    let diagnostics = compile_and_collect_diagnostics(
        r#"
interface RegExp {
  source: string;
}

const patternSource = /x/.source;
"#,
    );

    let relevant: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect();

    assert!(
        relevant.is_empty(),
        "Expected regex literal property access to succeed without non-lib diagnostics, got: {relevant:?}"
    );
}

#[test]
fn named_exports_type_check_as_void_instead_of_error() {
    let source = r#"
const value = 1;
export { value };
"#;

    let (parser, binder, types, root) = build_program(source);
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.check_source_file(root);

    let named_exports_idx = find_first_node_by_kind(&parser, syntax_kind_ext::NAMED_EXPORTS);

    assert_eq!(
        checker.get_type_of_node(named_exports_idx),
        TypeId::VOID,
        "expected named exports to behave like a statement node in type queries"
    );
}

#[test]
fn recursive_iterator_chain_return_context_does_not_overflow() {
    let diagnostics = compile_and_collect_diagnostics(
        r#"
type IterableLike<T> = {
  next(): T | undefined;
};

type IteratorChain<T> = IterableLike<T> & {
  map<U>(transform: (value: T) => U): IteratorChain<U>;
};

declare const emptyValues: <T>() => IterableLike<T>;

class ArrayIteratorChain<T> implements IteratorChain<T> {
  constructor(private readonly values: IterableLike<T>) {}

  next(): T | undefined {
    return this.values.next();
  }

  map<U>(_transform: (value: T) => U): IteratorChain<U> {
    const nextValues: IterableLike<U> = emptyValues<U>();
    return createIteratorChain(nextValues);
  }
}

const createIteratorChain = <T>(input: IterableLike<T>): IteratorChain<T> =>
  new ArrayIteratorChain(input);
"#,
    );

    let relevant: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect();

    assert!(
        relevant.is_empty(),
        "Expected no non-lib diagnostics for recursive iterator chain return context, got: {relevant:?}"
    );
}

#[test]
fn build_type_environment_defers_plain_class_and_function_symbols() {
    let source = r#"
class Box<T> {
  constructor(readonly value: T) {}
}

function makeBox<T>(value: T): Box<T> {
  return new Box(value);
}

const value = makeBox(1).value;
"#;

    let (parser, binder, types, root) = build_program(source);
    let class_idx = find_first_node_by_kind(&parser, syntax_kind_ext::CLASS_DECLARATION);
    let function_idx = find_first_node_by_kind(&parser, syntax_kind_ext::FUNCTION_DECLARATION);
    let class_sym = binder
        .node_symbols
        .get(&class_idx.0)
        .copied()
        .expect("expected class symbol");
    let function_sym = binder
        .node_symbols
        .get(&function_idx.0)
        .copied()
        .expect("expected function symbol");

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    let _ = checker.build_type_environment();

    assert!(
        !checker.ctx.symbol_types.contains_key(&class_sym),
        "expected build_type_environment to defer plain class symbols"
    );
    assert!(
        !checker.ctx.symbol_types.contains_key(&function_sym),
        "expected build_type_environment to defer plain function symbols"
    );

    checker.check_source_file(root);

    let relevant: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .filter(|(code, _)| *code != 2318)
        .collect();

    assert!(
        relevant.is_empty(),
        "Expected deferred class/function environment build to preserve checking, got: {relevant:?}"
    );
}
