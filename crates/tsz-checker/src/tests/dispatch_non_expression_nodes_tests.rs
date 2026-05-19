use crate::context::CheckerOptions;
use crate::dispatch::ExpressionDispatcher;
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::{NodeIndex, ParserState, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

fn checker_for(source: &str) -> (tsz_parser::parser::NodeArena, BinderState, TypeInterner) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let source_file = parser.parse_source_file();
    let arena = parser.into_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(&arena, source_file);

    (arena, binder, TypeInterner::new())
}

fn first_node_of_kind(arena: &tsz_parser::parser::NodeArena, kind: u16) -> NodeIndex {
    arena
        .nodes
        .iter()
        .position(|node| node.kind == kind)
        .map(|idx| NodeIndex(idx as u32))
        .unwrap_or_else(|| panic!("expected parsed node kind {kind}"))
}

#[test]
fn dispatch_handles_syntax_only_nodes_without_error_type() {
    let source = r#"
import type { Foo as TypeFoo } from "./dep";
import * as values from "./dep";
export { TypeFoo as ExportedFoo };

type ImportedFoo = import("./dep").Foo;

interface Api {
    readonly value: number;
    method(value: number): number;
}

class Holder {
    #secret = 1;

    method(param: number = 1) {
        return this.#secret + param + values.value;
    }
}
"#;
    let (arena, binder, types) = checker_for(source);
    let mut checker = CheckerState::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.ctx.set_lib_contexts(Vec::new());

    let non_poisoning_kinds = [
        syntax_kind_ext::BLOCK,
        syntax_kind_ext::IMPORT_DECLARATION,
        syntax_kind_ext::IMPORT_CLAUSE,
        syntax_kind_ext::IMPORT_SPECIFIER,
        syntax_kind_ext::NAMED_IMPORTS,
        syntax_kind_ext::NAMESPACE_IMPORT,
        syntax_kind_ext::NAMED_EXPORTS,
        syntax_kind_ext::EXPORT_DECLARATION,
        syntax_kind_ext::PARAMETER,
        SyntaxKind::ImportKeyword as u16,
        SyntaxKind::PrivateIdentifier as u16,
    ];

    for kind in non_poisoning_kinds {
        let node = first_node_of_kind(&arena, kind);
        let actual = ExpressionDispatcher::new(&mut checker).dispatch_type_computation(node);
        assert_ne!(
            actual,
            TypeId::ERROR,
            "dispatcher should not poison syntax-only node kind {kind} with TypeId::ERROR"
        );
    }

    let method_signature = first_node_of_kind(&arena, syntax_kind_ext::METHOD_SIGNATURE);
    let method_type =
        ExpressionDispatcher::new(&mut checker).dispatch_type_computation(method_signature);
    assert_ne!(
        method_type,
        TypeId::ERROR,
        "method signatures reached through broad walks should still resolve to a member type"
    );
    assert_ne!(
        method_type,
        TypeId::VOID,
        "method signatures should not be swallowed by syntax-only node handling"
    );

    let property_signature = first_node_of_kind(&arena, syntax_kind_ext::PROPERTY_SIGNATURE);
    let property_type =
        ExpressionDispatcher::new(&mut checker).dispatch_type_computation(property_signature);
    assert_ne!(
        property_type,
        TypeId::ERROR,
        "property signatures reached through broad walks should resolve to their declared type"
    );
    assert_ne!(
        property_type,
        TypeId::VOID,
        "property signatures should not be treated as syntax-only nodes"
    );
}
