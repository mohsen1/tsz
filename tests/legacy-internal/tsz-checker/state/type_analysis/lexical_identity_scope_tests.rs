//! Selective declaration identity must follow lexical syntax ownership, not
//! transient entries in the checker's re-entrant type-parameter scratch scope.

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, ParserState, syntax_kind_ext};
use tsz_solver::construction::TypeInterner;
use tsz_solver::{TypeParamInfo, TypeParamOrigin};

#[test]
fn same_name_scratch_entry_does_not_stamp_top_level_declaration() {
    let source = "class Target<Slot> { value!: Slot }";
    let mut parser = ParserState::new("scratch-scope.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let target_idx = arena
        .get_source_file_at(root)
        .expect("source file")
        .statements
        .nodes[0];
    let target_params = arena
        .get_class(arena.get(target_idx).expect("class node"))
        .and_then(|class| class.type_parameters.clone())
        .expect("class type parameters");

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(arena, root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "scratch-scope.ts".to_string(),
        CheckerOptions::default(),
    );

    let name = types.intern_string("Slot");
    let scratch = types.fresh_type_param(TypeParamInfo::simple(name));
    checker
        .ctx
        .type_parameter_scope
        .insert("Slot".to_string(), scratch);

    let (params, updates) = checker.push_type_parameters(&Some(target_params));
    assert_eq!(params.len(), 1);
    assert_eq!(
        params[0].origin,
        TypeParamOrigin::User,
        "an unrelated scratch entry is not a lexical owner and must not opt the top-level binder into exact identity",
    );
    assert_ne!(checker.ctx.type_parameter_scope["Slot"], scratch);

    checker.pop_type_parameters(updates);
    assert_eq!(checker.ctx.type_parameter_scope["Slot"], scratch);
}

#[test]
fn same_name_lexical_owner_stamps_nested_declaration() {
    let source = "class Outer<Slot> { method<Slot>(value: Slot): Slot { return value } }";
    let mut parser = ParserState::new("lexical-scope.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let outer_idx = arena
        .get_source_file_at(root)
        .expect("source file")
        .statements
        .nodes[0];
    let outer = arena
        .get_class(arena.get(outer_idx).expect("class node"))
        .expect("class data");
    let outer_params = outer
        .type_parameters
        .clone()
        .expect("outer type parameters");
    let method_idx = outer.members.nodes[0];
    let inner_params = arena
        .get_method_decl(arena.get(method_idx).expect("method node"))
        .and_then(|method| method.type_parameters.clone())
        .expect("method type parameters");

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(arena, root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "lexical-scope.ts".to_string(),
        CheckerOptions::default(),
    );

    let (_, outer_updates) = checker.push_type_parameters(&Some(outer_params));
    let (inner, inner_updates) = checker.push_type_parameters(&Some(inner_params));
    assert_eq!(inner.len(), 1);
    assert!(matches!(
        inner[0].origin,
        TypeParamOrigin::DeclScoped { .. }
    ));

    checker.pop_type_parameters(inner_updates);
    checker.pop_type_parameters(outer_updates);
}

#[test]
fn shadowed_declaration_id_is_stable_across_nested_closure_reentry() {
    let source = r#"
class Container<Token> {
  static wrap<Token>(value: Token) {
    const callback = () => value
    return callback()
  }
}
"#;
    let mut parser = ParserState::new("closure-reentry.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let class_idx = arena
        .get_source_file_at(root)
        .expect("source file")
        .statements
        .nodes[0];
    let class = arena
        .get_class(arena.get(class_idx).expect("class node"))
        .expect("class data");
    let class_params = class
        .type_parameters
        .clone()
        .expect("class type parameters");
    let method_idx = class.members.nodes[0];
    let method_params = arena
        .get_method_decl(arena.get(method_idx).expect("method node"))
        .and_then(|method| method.type_parameters.clone())
        .expect("method type parameters");
    let arrow_idx = arena
        .nodes
        .iter()
        .position(|node| node.kind == syntax_kind_ext::ARROW_FUNCTION)
        .map(|idx| NodeIndex(idx as u32))
        .expect("nested arrow");

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(arena, root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "closure-reentry.ts".to_string(),
        CheckerOptions::default(),
    );

    let (_, class_updates) = checker.push_type_parameters(&Some(class_params));
    let (_, method_updates) = checker.push_type_parameters(&Some(method_params));
    let direct = checker.ctx.type_parameter_scope["Token"];
    checker.pop_type_parameters(method_updates);
    checker.pop_type_parameters(class_updates);

    let reentry_updates = checker.push_enclosing_type_parameters(arrow_idx);
    let reentered = checker.ctx.type_parameter_scope["Token"];
    checker.pop_type_parameters(reentry_updates);

    let repeated_updates = checker.push_enclosing_type_parameters(arrow_idx);
    let repeated = checker.ctx.type_parameter_scope["Token"];
    checker.pop_type_parameters(repeated_updates);

    assert_eq!(
        direct, reentered,
        "canonical method push and nested closure re-entry must reuse one declaration TypeId"
    );
    assert_eq!(
        direct, repeated,
        "repeated nested closure re-entry must keep the declaration TypeId stable"
    );
}
