use crate::context::{CheckerContext, CheckerOptions};
use crate::module_resolution::build_module_resolution_maps;
use crate::query_boundaries::common::TypeInterner;
use crate::query_boundaries::enum_analysis::{is_nominal_enum_type, object_shape_for_type};
use crate::query_boundaries::property_access::type_has_property;
use crate::state::CheckerState;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::{NodeArena, ParserState};
use tsz_solver::TypeId;

fn parse_and_bind(file_name: &str, source: &str) -> (Arc<NodeArena>, Arc<BinderState>) {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    (Arc::new(parser.into_arena()), Arc::new(binder))
}

#[test]
fn augmentation_only_enum_runtime_values_are_concrete_in_the_consumer_arena() {
    let fixtures = [
        ("/project/home.ts", "export {};"),
        (
            "/project/augmentation.d.ts",
            r#"
import "./home";
declare module "./home" {
    export enum AddedEnum { Member }
    export const enum AddedConstEnum { Member }
}
"#,
        ),
        (
            "/project/consumer.ts",
            r#"
import * as home from "./home";
const member: home.AddedEnum = home.AddedEnum.Member;
const enumObject = home.AddedEnum;
home.AddedEnum.Member;
home.AddedConstEnum.Member;
"#,
        ),
    ];

    let mut arenas = Vec::new();
    let mut binders = Vec::new();
    for (file_name, source) in fixtures {
        let (arena, binder) = parse_and_bind(file_name, source);
        arenas.push(arena);
        binders.push(binder);
    }
    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let file_names: Vec<_> = fixtures
        .iter()
        .map(|(file_name, _)| (*file_name).to_string())
        .collect();
    let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);
    let types = TypeInterner::new();
    let mut checker = CheckerState {
        ctx: CheckerContext::new(
            all_arenas[2].as_ref(),
            all_binders[2].as_ref(),
            &types,
            file_names[2].clone(),
            CheckerOptions::default(),
        ),
    };
    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(2);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);

    let mut runtime_types = Vec::new();
    for name in ["AddedEnum", "AddedConstEnum"] {
        let runtime_type = checker
            .module_augmentation_runtime_export_type("./home", name)
            .unwrap_or_else(|| panic!("missing exact runtime value for {name}"));
        assert_ne!(
            runtime_type,
            TypeId::ANY,
            "an owner-arena enum declaration must not publish a bailout `any`"
        );
        assert!(
            type_has_property(&types, runtime_type, types.intern_string("Member"),),
            "an exact enum runtime object must expose its declared member"
        );
        runtime_types.push(runtime_type);
    }

    let qualified_enum = all_arenas[2]
        .nodes
        .iter()
        .enumerate()
        .find_map(|(raw_idx, node)| {
            let idx = tsz_parser::parser::NodeIndex(raw_idx as u32);
            (node.kind == tsz_parser::parser::syntax_kind_ext::QUALIFIED_NAME
                && checker.entity_name_text(idx).as_deref() == Some("home.AddedEnum"))
            .then_some(idx)
        })
        .expect("qualified enum type reference");
    let enum_type_reference = all_arenas[2]
        .get_extended(qualified_enum)
        .map(|extended| extended.parent)
        .filter(|&parent| {
            all_arenas[2].get(parent).is_some_and(|node| {
                node.kind == tsz_parser::parser::syntax_kind_ext::TYPE_REFERENCE
            })
        })
        .expect("enum type reference");
    let (enum_augmentation, enum_owner) = checker
        .exact_module_augmentation_declaration_owner("./home", "AddedEnum")
        .expect("exact enum augmentation owner");
    assert_eq!(enum_owner, 1);
    let enum_symbol = all_binders[enum_owner]
        .get_node_symbol(enum_augmentation.node)
        .expect("enum augmentation symbol");
    let previous_owner = checker.ctx.local_symbol_file_target_override(enum_symbol);
    checker.ctx.register_symbol_file_target(enum_symbol, 0);
    let enum_type = checker.get_type_from_type_reference(enum_type_reference);
    assert_eq!(
        checker.ctx.resolve_dynamic_symbol_file_index(enum_symbol),
        Some(0),
        "the exact owner override must be scoped to one type reference"
    );
    checker
        .ctx
        .restore_local_symbol_file_target_override(enum_symbol, previous_owner);
    assert_ne!(
        enum_type,
        TypeId::ANY,
        "the type side of an augmentation-only enum must be concrete"
    );
    assert_ne!(
        enum_type, runtime_types[0],
        "an enum's member type and runtime namespace object are distinct declaration spaces"
    );
    assert!(
        is_nominal_enum_type(&types, enum_type),
        "the qualified enum type must retain nominal enum identity, got {enum_type:?}"
    );
    assert!(
        object_shape_for_type(&types, runtime_types[0]).is_some(),
        "the runtime enum value must be a namespace object, got {:?}",
        runtime_types[0]
    );

    let enum_value_access = all_arenas[2]
        .nodes
        .iter()
        .enumerate()
        .find_map(|(raw_idx, node)| {
            let idx = tsz_parser::parser::NodeIndex(raw_idx as u32);
            let is_outer_receiver = all_arenas[2]
                .get_extended(idx)
                .and_then(|extended| all_arenas[2].get(extended.parent))
                .is_some_and(|parent| {
                    all_arenas[2]
                        .get_access_expr(parent)
                        .is_some_and(|access| access.expression == idx)
                });
            let access = all_arenas[2].get_access_expr(node)?;
            let expression_name = all_arenas[2]
                .get_identifier_at(access.expression)
                .map(|identifier| identifier.escaped_text.as_str());
            let property_name = all_arenas[2]
                .get_identifier_at(access.name_or_argument)
                .map(|identifier| identifier.escaped_text.as_str());
            (node.kind == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && !is_outer_receiver
                && expression_name == Some("home")
                && property_name == Some("AddedEnum"))
            .then_some(idx)
        })
        .expect("standalone enum runtime access");
    let enum_value_type = checker.get_type_of_node(enum_value_access);
    assert_eq!(
        enum_value_type, runtime_types[0],
        "namespace property access must use the exact enum runtime object"
    );
    assert!(
        !checker.is_assignable_to(enum_value_type, enum_type),
        "the enum namespace object must not be assignable to its member type"
    );
}
