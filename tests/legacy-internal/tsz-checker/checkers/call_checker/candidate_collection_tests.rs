use super::*;
use crate::context::CheckerOptions;
use crate::module_resolution::build_module_resolution_maps;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

fn first_argument_for_call(checker: &CheckerState<'_>, callee_name: &str) -> NodeIndex {
    checker
        .ctx
        .arena
        .nodes
        .iter()
        .find_map(|node| {
            if node.kind != syntax_kind_ext::CALL_EXPRESSION {
                return None;
            }
            let call = checker.ctx.arena.get_call_expr(node)?;
            let callee_node = checker.ctx.arena.get(call.expression)?;
            let callee_ident = checker.ctx.arena.get_identifier(callee_node)?;
            if callee_ident.escaped_text != callee_name {
                return None;
            }
            call.arguments.as_ref()?.nodes.first().copied()
        })
        .expect("expected call argument")
}

#[test]
fn generic_call_source_markers_fast_fail_parameter_identifiers() {
    let source = r#"
declare function fromParam<T>(value: T): T;
declare function fromPayload<U>(value: U): U;
declare function fromTyped<V>(value: V): V;
declare function fromAssertion<W>(value: W): W;
declare function fromRedeclared<X>(value: X): X;
declare function fromDestructured<Y>(value: Y): Y;

function run<T, U>(input: T, payload: U) {
    const typed: { a: 1 } = { a: 1 };
    fromParam(input);
    fromPayload(payload);
    fromTyped(typed);
    fromAssertion(input as T);
}

function redeclared(input: unknown) {
    var input: { a: 1 };
    fromRedeclared(input);
}

function destructured({ item }: { item: { a: 1 } }) {
    fromDestructured(item);
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena().clone();
    let mut binder = BinderState::new();
    binder.bind_source_file(&arena, root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.check_source_file(root);

    let param_arg = first_argument_for_call(&checker, "fromParam");
    let renamed_param_arg = first_argument_for_call(&checker, "fromPayload");
    let typed_arg = first_argument_for_call(&checker, "fromTyped");
    let asserted_arg = first_argument_for_call(&checker, "fromAssertion");
    let redeclared_arg = first_argument_for_call(&checker, "fromRedeclared");
    let destructured_arg = first_argument_for_call(&checker, "fromDestructured");
    let param_sym = checker
        .resolve_identifier_symbol(param_arg)
        .expect("parameter argument should resolve");
    let typed_sym = checker
        .resolve_identifier_symbol(typed_arg)
        .expect("typed local argument should resolve");
    let redeclared_sym = checker
        .resolve_identifier_symbol(redeclared_arg)
        .expect("redeclared argument should resolve");

    assert!(
        checker.local_symbol_value_declaration_is_plain_parameter(param_sym),
        "plain local parameters should take the fast-fail path"
    );
    assert!(
        !checker.local_symbol_value_declaration_is_plain_parameter(typed_sym),
        "typed locals are not parameter fast-fail candidates"
    );
    assert!(
        !checker.local_symbol_value_declaration_is_plain_parameter(redeclared_sym),
        "parameter symbols with typed variable declarations must stay on the full scan"
    );

    assert_eq!(
        checker.call_arg_source_type_annotation_markers(&[param_arg], 1),
        vec![false],
        "parameter identifiers are not variable-declaration type annotation sources"
    );
    assert_eq!(
        checker.call_arg_source_type_annotation_markers(&[renamed_param_arg], 1),
        vec![false],
        "renamed parameter identifiers should take the same fast-fail path"
    );
    assert_eq!(
        checker.call_arg_source_type_annotation_markers(&[typed_arg], 1),
        vec![true],
        "typed local variable identifiers must still mark generic inference sources"
    );
    assert_eq!(
        checker.call_arg_source_type_annotation_markers(&[asserted_arg], 1),
        vec![true],
        "explicit type assertions must still mark generic inference sources"
    );
    assert_eq!(
        checker.call_arg_source_type_annotation_markers(&[redeclared_arg], 1),
        vec![true],
        "parameter symbols that also have typed variable declarations should keep old marker behavior"
    );
    assert_eq!(
        checker.call_arg_source_type_annotation_markers(&[destructured_arg], 1),
        vec![false],
        "destructured parameter bindings are not variable-declaration type annotation sources"
    );
}

#[test]
fn generic_call_source_markers_keep_cross_file_typed_identifier_sources() {
    let files = [
        (
            "consumer.ts",
            r#"
declare function fromTyped<T>(value: T): T;

function run(local: unknown) {
    fromTyped(typedGlobal);
}
"#,
        ),
        (
            "shared.ts",
            r#"
declare const typedGlobal: { a: 1 };
"#,
        ),
    ];

    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();
    for (file_idx, (name, source)) in files.iter().enumerate() {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.set_file_idx(file_idx as u32);
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);
    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[0].as_ref(),
        all_binders[0].as_ref(),
        &types,
        file_names[0].clone(),
        CheckerOptions::default(),
    );
    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(0);
    checker.ctx.set_lib_contexts(Vec::new());
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(roots[0]);

    let typed_arg = first_argument_for_call(&checker, "fromTyped");
    let typed_sym = checker
        .resolve_identifier_symbol(typed_arg)
        .expect("cross-file typed argument should resolve");
    assert!(
        !checker.local_symbol_value_declaration_is_plain_parameter(typed_sym),
        "cross-file typed variables must not collide with local parameter fast-fail"
    );
    assert_eq!(
        checker.call_arg_source_type_annotation_markers(&[typed_arg], 1),
        vec![true],
        "cross-file typed variable identifiers must keep old marker behavior"
    );
}
