//! Admission tests for the explicit-default generic-alias shortcut.

use super::cross_file_direct_alias_chain_tests::with_program_state_with_libs;
use std::sync::Arc;
use tsz_solver::TypeId;

fn shortcut_parameter_count(target_source: &str, requester_source: &str) -> Option<usize> {
    with_program_state_with_libs(
        &[
            ("target.ts", target_source),
            ("requester.ts", requester_source),
        ],
        "requester.ts",
        "target.ts",
        &[],
        |state, _, _| {
            let import_symbol = state.ctx.binder.file_locals.get("Imported")?;
            state
                .ctx
                .register_symbol_file_target(import_symbol, state.ctx.current_file_idx);
            state
                .try_resolve_cross_arena_named_alias_without_child(import_symbol)
                .map(|(_, params)| params.len())
        },
    )
}

#[test]
fn explicit_default_generic_type_alias_is_admitted() {
    assert_eq!(
        shortcut_parameter_count(
            "type Parcel<Content> = { content: Content }; export default Parcel;",
            "import type Imported from './target'; type Use = Imported<string>;",
        ),
        Some(1),
        "an explicit type-only default export of a generic alias should use direct lowering",
    );
}

#[test]
fn explicit_default_generic_alias_primes_nested_default_targets_cold() {
    with_program_state_with_libs(
        &[
            (
                "arrival.ts",
                "type Arrival<Payload> = { success: true; value: Payload }; export default Arrival;",
            ),
            (
                "rejection.ts",
                "type Rejection = { success: false; message: string }; export default Rejection;",
            ),
            (
                "resolution.ts",
                "import type FailedOutcome from './rejection'; import type SuccessfulOutcome from './arrival'; type Resolution<Item> = SuccessfulOutcome<Item> | FailedOutcome; export default Resolution;",
            ),
            (
                "requester.ts",
                "import type Outcome from './resolution'; type Use = Outcome<unknown>;",
            ),
        ],
        "requester.ts",
        "resolution.ts",
        &[],
        |state, _, _| {
            let import_symbol = state
                .ctx
                .binder
                .file_locals
                .get("Outcome")
                .expect("requester default import");
            let (_, params) = state
                .try_resolve_cross_arena_named_alias_without_child(import_symbol)
                .expect("nested default alias should lower directly");
            assert_eq!(params.len(), 1);

            for (file_idx, name, expected_params) in
                [(0, "Arrival", Some(1)), (1, "Rejection", None)]
            {
                let binder = state
                    .ctx
                    .get_binder_for_file(file_idx)
                    .expect("target binder");
                let sym_id = binder.file_locals.get(name).expect("target alias");
                let symbol = binder.get_symbol(sym_id).expect("target symbol");
                let def_id = state
                    .ctx
                    .def_id_for_declaration_in_file(sym_id, file_idx, &symbol.escaped_name)
                    .expect("declaration-file definition");
                assert!(
                    state.ctx.definition_store.get_body(def_id).is_some(),
                    "{name} must be materialized under its declaration-file definition",
                );
                assert_eq!(
                    state
                        .ctx
                        .get_def_type_params(def_id)
                        .map(|params| params.len()),
                    expected_params,
                );
            }
        },
    );
}

#[test]
fn nested_builtin_application_uses_published_alias_parameter_identity() {
    with_program_state_with_libs(
        &[
            (
                "parcel.ts",
                "type Parcel<Content> = { content: Content }; export default Parcel;",
            ),
            (
                "wrapper.ts",
                "import type Package from './parcel'; type Wrapper<Value> = { parcel: Package<Value>; values: Array<Value> }; export default Wrapper;",
            ),
            (
                "requester.ts",
                "import type Wrapped from './wrapper'; type Use = Wrapped<string>;",
            ),
        ],
        "requester.ts",
        "wrapper.ts",
        &["es5.d.ts"],
        |state, wrapper_binder, wrapper_idx| {
            let wrapper_sym = wrapper_binder
                .file_locals
                .get("Wrapper")
                .expect("wrapper alias");
            let (body, params) = state
                .direct_source_file_type_alias_result(wrapper_sym, Some(wrapper_idx), true)
                .expect("wrapper alias should lower directly");
            let values_name = state.ctx.types.intern_string("values");
            let instantiated = crate::query_boundaries::generic_instantiation::instantiate_generic(
                state.ctx.types,
                body,
                &params,
                &[TypeId::STRING],
            );
            let values = crate::query_boundaries::common::raw_property_type(
                state.ctx.types.as_type_database(),
                instantiated,
                values_name,
            )
            .expect("instantiated values property");
            let element = crate::query_boundaries::common::array_element_type(
                state.ctx.types.as_type_database(),
                values,
            )
            .expect("Array<Value> element");
            assert_eq!(
                element,
                TypeId::STRING,
                "the published alias body and parameter list must share identity",
            );

            let wrapper_symbol = wrapper_binder
                .get_symbol(wrapper_sym)
                .expect("wrapper symbol");
            let wrapper_def = state
                .ctx
                .def_id_for_declaration_in_file(
                    wrapper_sym,
                    wrapper_idx,
                    &wrapper_symbol.escaped_name,
                )
                .expect("owner-qualified wrapper definition");
            let application = state.ctx.types.factory().application(
                state.ctx.types.factory().lazy(wrapper_def),
                vec![TypeId::STRING],
            );
            let evaluated = state.evaluate_type_with_env(application);
            let evaluated_values = crate::query_boundaries::common::raw_property_type(
                state.ctx.types.as_type_database(),
                evaluated,
                values_name,
            )
            .expect("evaluated values property");
            assert_eq!(
                crate::query_boundaries::common::array_element_type(
                    state.ctx.types.as_type_database(),
                    evaluated_values,
                ),
                Some(TypeId::STRING),
                "the shared evaluator must instantiate before materializing nested applications",
            );
        },
    );
}

#[test]
fn owner_qualified_default_alias_bodies_survive_raw_id_readiness_collisions() {
    for (
        leaf_name,
        fallback_name,
        outer_name,
        leaf_param_name,
        outer_param_name,
        nested_import_name,
        fallback_import_name,
        requester_import_name,
    ) in [
        (
            "Receipt",
            "Refusal",
            "Envelope",
            "Payload",
            "Entry",
            "AcceptedReceipt",
            "RejectedReceipt",
            "RequestedEnvelope",
        ),
        (
            "Cargo",
            "Detour",
            "Manifest",
            "Freight",
            "Unit",
            "LoadedCargo",
            "DivertedCargo",
            "RequestedManifest",
        ),
    ] {
        let leaf_source = format!(
            "type LocalPaddingA = never; \
             type LocalPaddingB = never; \
             type {leaf_name}<{leaf_param_name}> = \
             {{ accepted: true; value: {leaf_param_name} }}; \
             export default {leaf_name};"
        );
        let fallback_source = format!(
            "type {fallback_name} = {{ accepted: false; reason: string }}; export default {fallback_name};"
        );
        let outer_source = format!(
            "import type {fallback_import_name} from './fallback'; \
             import type {nested_import_name} from './leaf'; \
             type {outer_name}<{outer_param_name}> = \
             {nested_import_name}<{outer_param_name}> | {fallback_import_name}; \
             export default {outer_name};"
        );
        let requester_source = format!(
            "import type {requester_import_name} from './outer'; \
             type Use = {requester_import_name}<unknown>;"
        );
        let files = [
            ("leaf.ts", leaf_source.as_str()),
            ("fallback.ts", fallback_source.as_str()),
            ("outer.ts", outer_source.as_str()),
            ("requester.ts", requester_source.as_str()),
        ];

        with_program_state_with_libs(
            &files,
            "requester.ts",
            "outer.ts",
            &[],
            |state, outer_binder, outer_idx| {
                let requester_import = state
                    .ctx
                    .binder
                    .file_locals
                    .get(requester_import_name)
                    .expect("requester default import");
                state
                    .ctx
                    .register_symbol_file_target(requester_import, state.ctx.current_file_idx);
                state
                    .try_resolve_cross_arena_named_alias_without_child(requester_import)
                    .expect("outer default alias should lower and prime its nested target");

                let leaf_idx = 0;
                let leaf_binder = state
                    .ctx
                    .get_binder_for_file(leaf_idx)
                    .expect("leaf binder");
                let leaf_sym = leaf_binder.file_locals.get(leaf_name).expect("leaf alias");
                let outer_sym = outer_binder
                    .file_locals
                    .get(outer_name)
                    .expect("outer alias");
                assert_eq!(
                    leaf_sym, outer_sym,
                    "fixture must exercise equal raw ids for differently named aliases"
                );

                let leaf_def = state
                    .ctx
                    .def_id_for_declaration_in_file(leaf_sym, leaf_idx, leaf_name)
                    .expect("owner-qualified leaf definition");
                let outer_def = state
                    .ctx
                    .def_id_for_declaration_in_file(outer_sym, outer_idx, outer_name)
                    .expect("owner-qualified outer definition");
                assert_ne!(
                    leaf_def, outer_def,
                    "equal raw ids from different binders need distinct definitions"
                );

                let leaf_body = state
                    .ctx
                    .definition_store
                    .get_body(leaf_def)
                    .expect("primed leaf body");
                let leaf_params = state
                    .ctx
                    .get_def_type_params(leaf_def)
                    .expect("primed leaf parameters");
                let outer_body = state
                    .ctx
                    .definition_store
                    .get_body(outer_def)
                    .expect("primed outer body");
                let outer_params = state
                    .ctx
                    .get_def_type_params(outer_def)
                    .expect("primed outer parameters");
                assert_ne!(
                    leaf_body, outer_body,
                    "the colliding definitions need observably different bodies"
                );
                let value_name = state.ctx.types.intern_string("value");
                let instantiated =
                    crate::query_boundaries::generic_instantiation::instantiate_generic(
                        state.ctx.types,
                        leaf_body,
                        &leaf_params,
                        &[TypeId::STRING],
                    );
                assert_eq!(
                    crate::query_boundaries::common::raw_property_type(
                        state.ctx.types.as_type_database(),
                        instantiated,
                        value_name,
                    ),
                    Some(TypeId::STRING),
                    "the stored leaf body must reference the exact stored parameter identity"
                );

                // Model the owner-equals-current hole from relation readiness:
                // a stale raw-id overlay names the outer declaration even though
                // the exact leaf DefId still records the leaf file as its owner.
                state.ctx.set_current_file_idx(leaf_idx);
                state.ctx.register_symbol_file_target(leaf_sym, outer_idx);
                assert_eq!(
                    state.resolve_and_insert_def_type(leaf_def),
                    Some(leaf_body),
                    "exact-def readiness must not demote the leaf to the outer raw symbol"
                );
                assert_eq!(
                    state.ctx.definition_store.get_body(leaf_def),
                    Some(leaf_body)
                );
                assert_eq!(
                    state.ctx.get_def_type_params(leaf_def),
                    Some(leaf_params.clone())
                );
                assert_eq!(
                    state.ctx.definition_store.get_body(outer_def),
                    Some(outer_body)
                );
                assert_eq!(
                    state.ctx.get_def_type_params(outer_def),
                    Some(outer_params.clone())
                );

                // Exercise the parallel application-readiness route with the
                // collision reversed. Before the owner-qualified body guard,
                // this resolved the leaf through `outer_sym` and could publish
                // that body and parameter list over `outer_def`.
                state.ctx.set_current_file_idx(outer_idx);
                state.ctx.register_symbol_file_target(outer_sym, leaf_idx);
                let outer_lazy = state.ctx.types.factory().lazy(outer_def);
                let mut visited = rustc_hash::FxHashSet::default();
                assert!(
                    state.ensure_application_symbols_resolved_inner(outer_lazy, &mut visited),
                    "the exact outer body should make readiness complete"
                );
                assert_eq!(
                    state.ctx.definition_store.get_body(leaf_def),
                    Some(leaf_body)
                );
                assert_eq!(state.ctx.get_def_type_params(leaf_def), Some(leaf_params));
                assert_eq!(
                    state.ctx.definition_store.get_body(outer_def),
                    Some(outer_body)
                );
                assert_eq!(state.ctx.get_def_type_params(outer_def), Some(outer_params));
            },
        );
    }
}

#[test]
fn explicit_default_generic_alias_reuses_requester_cache_under_raw_id_collision() {
    with_program_state_with_libs(
        &[
            (
                "target.ts",
                "type Parcel<Content> = { content: Content }; export default Parcel;",
            ),
            (
                "requester.ts",
                "import type Imported from './target'; type Use = Imported<string>;",
            ),
        ],
        "requester.ts",
        "target.ts",
        &[],
        |state, target_binder, _| {
            state.ctx.share_owner_symbol_type_results = true;
            let requester_idx = state.ctx.current_file_idx;
            let import_symbol = state
                .ctx
                .binder
                .file_locals
                .get("Imported")
                .expect("default import symbol");
            let target_symbol = target_binder
                .file_locals
                .get("Parcel")
                .expect("target alias symbol");
            assert_eq!(
                import_symbol, target_symbol,
                "fixture must exercise equal raw ids from different binders",
            );
            state
                .ctx
                .register_symbol_file_target(import_symbol, requester_idx);

            let first = state
                .try_resolve_cross_arena_named_alias_without_child(import_symbol)
                .expect("first default import resolution");
            let second = state
                .try_resolve_cross_arena_named_alias_without_child(import_symbol)
                .expect("warm default import resolution");

            assert_eq!(first.0, second.0, "warm resolution should reuse the body");
            assert_eq!(first.1.len(), 1);
            assert_eq!(second.1.len(), 1);
            assert!(
                state
                    .ctx
                    .cached_cross_file_symbol_type(import_symbol, requester_idx as u32)
                    .is_some(),
                "the import alias cache must be owned by the requester file",
            );
            assert_eq!(
                state.ctx.resolve_symbol_file_index(import_symbol),
                Some(requester_idx),
                "a colliding target lookup must restore the requester import owner",
            );
        },
    );
}

#[test]
fn explicit_default_alias_restores_different_requester_symbol_with_target_raw_id() {
    with_program_state_with_libs(
        &[
            (
                "target.ts",
                "type Padding = never; type Parcel<Content> = { content: Content }; export default Parcel;",
            ),
            ("other.ts", "export type Occupant = { occupied: true };"),
            (
                "requester.ts",
                "import type Imported from './target'; import type { Occupant } from './other'; type Use = Imported<string>;",
            ),
        ],
        "requester.ts",
        "target.ts",
        &[],
        |state, target_binder, _| {
            let requester_idx = state.ctx.current_file_idx;
            let import_symbol = state
                .ctx
                .binder
                .file_locals
                .get("Imported")
                .expect("default import symbol");
            let occupant_symbol = state
                .ctx
                .binder
                .file_locals
                .get("Occupant")
                .expect("second requester import");
            let target_symbol = target_binder
                .file_locals
                .get("Parcel")
                .expect("target alias symbol");
            assert_ne!(
                import_symbol, target_symbol,
                "the resolved import must have a different raw id from its target",
            );
            assert_eq!(
                occupant_symbol, target_symbol,
                "a different requester symbol must occupy the target's raw id",
            );
            state
                .ctx
                .register_symbol_file_target(occupant_symbol, requester_idx);

            let (_, params) = state
                .try_resolve_cross_arena_named_alias_without_child(import_symbol)
                .expect("default import resolution");

            assert_eq!(params.len(), 1);
            assert_eq!(
                state.ctx.resolve_symbol_file_index(occupant_symbol),
                Some(requester_idx),
                "temporary target ownership must restore the colliding requester symbol",
            );
        },
    );
}

#[test]
fn outer_delegation_ignores_target_owned_cache_for_local_default_import() {
    with_program_state_with_libs(
        &[
            (
                "target.ts",
                "type Padding = never; type Parcel<Content> = { content: Content }; export default Parcel;",
            ),
            (
                "requester.ts",
                "import type Imported from './target'; type Use = Imported<string>;",
            ),
        ],
        "requester.ts",
        "target.ts",
        &[],
        |state, target_binder, target_idx| {
            state.ctx.share_owner_symbol_type_results = true;
            let requester_idx = state.ctx.current_file_idx;
            let import_symbol = state
                .ctx
                .binder
                .file_locals
                .get("Imported")
                .expect("default import symbol");
            let target_symbol = target_binder
                .file_locals
                .get("Parcel")
                .expect("target alias symbol");
            let mut global_index = rustc_hash::FxHashMap::default();
            global_index.insert(import_symbol, target_idx);
            state
                .ctx
                .set_global_symbol_file_index(Arc::new(global_index));
            state.ctx.cache_cross_file_symbol_type(
                import_symbol,
                target_idx as u32,
                TypeId::NUMBER,
                Vec::new(),
            );
            state.ctx.cache_cross_file_symbol_type(
                target_symbol,
                target_idx as u32,
                TypeId::STRING,
                Vec::new(),
            );
            let (resolved, params) = state
                .delegate_cross_arena_symbol_resolution(import_symbol)
                .expect("local default import should resolve through the shortcut");

            assert_ne!(
                resolved,
                TypeId::NUMBER,
                "a target-owned raw-id collision cache must not satisfy the requester alias",
            );
            assert_ne!(
                resolved,
                TypeId::STRING,
                "the direct alias body must not reuse an opaque canonical-target cache entry",
            );
            assert_eq!(params.len(), 1);
            assert_eq!(
                state
                    .ctx
                    .cached_cross_file_symbol_type(import_symbol, requester_idx as u32)
                    .map(|(type_id, params)| (type_id, params.len())),
                Some((resolved, 1)),
                "the outer delegation cache must use the requester file",
            );
        },
    );
}

#[test]
fn nested_default_interface_keeps_existing_target_resolution() {
    with_program_state_with_libs(
        &[
            (
                "container.ts",
                "export default interface Container<Value> { value: Value }",
            ),
            (
                "wrapper.ts",
                "import type Container from './container'; export type Wrapped<Value> = Container<Value>;",
            ),
            (
                "requester.ts",
                "import type { Wrapped } from './wrapper'; type Use = Wrapped<string>;",
            ),
        ],
        "requester.ts",
        "wrapper.ts",
        &[],
        |state, wrapper_binder, wrapper_idx| {
            let imported = wrapper_binder
                .file_locals
                .get("Container")
                .expect("default interface import");
            let target = state
                .source_file_import_alias_target_for_lowering(wrapper_idx, wrapper_binder, imported)
                .expect("the legacy default-interface target should remain resolvable");
            assert_ne!(
                target.file_idx,
                Some(wrapper_idx),
                "the fallback must retain the default export's cross-file target",
            );
        },
    );
}

#[test]
fn non_generic_default_type_alias_stays_on_fallback() {
    assert_eq!(
        shortcut_parameter_count(
            "type Parcel = { content: unknown }; export default Parcel;",
            "import type Imported from './target'; type Use = Imported;",
        ),
        None,
        "the new shortcut is limited to generic default aliases",
    );
}

#[test]
fn default_generic_alias_with_type_query_stays_on_fallback() {
    assert_eq!(
        shortcut_parameter_count(
            "declare const marker: unique symbol; type Parcel<Content> = { content: Content; marker: typeof marker }; export default Parcel;",
            "import type Imported from './target'; type Use = Imported<string>;",
        ),
        None,
        "aliases rejected by the direct-lowering proof must keep the child-checker fallback",
    );
}

#[test]
fn export_equals_default_import_stays_on_fallback() {
    assert_eq!(
        shortcut_parameter_count(
            "declare class Service { value: number } export = Service;",
            "import Imported from './target'; declare const value: Imported;",
        ),
        None,
        "an `export =` provider is not an explicit type-only default export",
    );
}

#[test]
fn default_exported_class_stays_on_fallback() {
    assert_eq!(
        shortcut_parameter_count(
            "export default class Container<T> { value!: T }",
            "import Imported from './target'; declare const value: Imported<string>;",
        ),
        None,
        "class defaults must keep constructor/instance resolution",
    );
}
