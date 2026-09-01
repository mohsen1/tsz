//! Cross-file class-instance publication invariants.
//!
//! Direct class-instance delegation returns the owner file's instance type to a
//! requesting checker. Declaration-file owners must also publish that result
//! into the requester so later `Lazy(DefId)` resolution can reuse the delegated
//! instance instead of reconstructing the child checker.

use std::sync::Arc;

use crate::context::CheckerOptions;
use crate::diagnostics::diagnostic_codes;
use crate::module_resolution::build_module_resolution_maps;
use crate::state::CheckerState;
use crate::test_utils::check_all_multi_file_with_global_index;
use tsz_binder::BinderState;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;
use tsz_solver::relations::subtype::TypeResolver;
use tsz_solver::{TypeId, TypeParamInfo};

fn with_two_file_state<F>(files: &[(&str, &str)], entry_file: &str, f: F)
where
    F: FnOnce(&mut CheckerState<'_>, usize),
{
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in files {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let entry_idx = file_names
        .iter()
        .position(|name| name == entry_file)
        .expect("entry file must exist");
    let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);
    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);

    let mut symbol_file_index = rustc_hash::FxHashMap::default();
    for (file_idx, binder) in all_binders.iter().enumerate() {
        for symbol in binder.symbols.iter() {
            symbol_file_index.entry(symbol.id).or_insert(file_idx);
        }
    }

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &types,
        file_names[entry_idx].clone(),
        CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            ..CheckerOptions::default()
        },
    );
    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    checker.ctx.set_lib_contexts(Vec::new());
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);
    checker
        .ctx
        .set_global_symbol_file_index(Arc::new(symbol_file_index));
    checker.ctx.share_owner_symbol_type_results = true;
    checker.check_source_file(roots[entry_idx]);

    f(&mut checker, entry_idx);
}

#[test]
fn delegated_cross_file_class_instance_is_published_to_requester_env() {
    with_two_file_state(
        &[
            (
                "model.d.ts",
                r#"
export declare class BoxThing {
    readonly value: number;
}
"#,
            ),
            (
                "entry.ts",
                r#"
import { BoxThing } from "./model";
let item: BoxThing | undefined;
"#,
            ),
        ],
        "entry.ts",
        |checker, entry_idx| {
            let sym_id = checker
                .resolve_cross_file_export_from_file("./model", "BoxThing", Some(entry_idx))
                .expect("exported class symbol should resolve");
            let owner_file = checker
                .ctx
                .resolve_symbol_file_index(sym_id)
                .expect("exported class should have an owner file");
            assert_ne!(owner_file, checker.ctx.current_file_idx);

            let (instance_type, params) = checker
                .class_instance_type_with_params_from_symbol(sym_id)
                .expect("cross-file class instance should delegate");
            assert!(!instance_type.is_any_unknown_or_error());
            assert!(
                params.is_empty(),
                "non-generic class should not publish params"
            );

            let def_id = checker
                .ctx
                .get_or_create_def_id_for_symbol_name(sym_id, "BoxThing");
            let resolved = {
                let env = checker.ctx.type_env.borrow();
                TypeResolver::resolve_lazy(&*env, def_id, checker.ctx.types)
            };
            assert_eq!(
                resolved,
                Some(instance_type),
                "requester env should resolve the class DefId to the delegated instance"
            );
            assert_eq!(
                checker.ctx.symbol_instance_types.get(&sym_id),
                Some(instance_type),
                "requester should memoize the delegated class instance by symbol"
            );
        },
    );
}

#[test]
fn delegated_source_class_instance_skips_requester_env_publication() {
    with_two_file_state(
        &[
            (
                "model.ts",
                r#"
export class BoxThing {
    readonly value: number = 1;
}
"#,
            ),
            (
                "entry.ts",
                r#"
import { BoxThing } from "./model";
let item: BoxThing | undefined;
"#,
            ),
        ],
        "entry.ts",
        |checker, entry_idx| {
            let sym_id = checker
                .resolve_cross_file_export_from_file("./model", "BoxThing", Some(entry_idx))
                .expect("exported class symbol should resolve");
            let env_before = checker
                .ctx
                .type_env
                .borrow()
                .snapshot_class_instance_types();
            let symbol_cache_before = checker.ctx.symbol_instance_types.get(&sym_id);

            let (instance_type, params) = checker
                .class_instance_type_with_params_from_symbol(sym_id)
                .expect("cross-file class instance should delegate");
            assert!(!instance_type.is_any_unknown_or_error());
            assert!(params.is_empty());
            assert_eq!(
                checker.ctx.symbol_instance_types.get(&sym_id),
                symbol_cache_before,
                "source class delegation must not publish the requester symbol instance"
            );

            let env_after = checker
                .ctx
                .type_env
                .borrow()
                .snapshot_class_instance_types();
            assert_eq!(
                env_after, env_before,
                "source class delegation must not mutate the requester class-instance env"
            );
        },
    );
}

#[test]
fn delegated_source_class_instance_does_not_poison_commonjs_namespace_static_side() {
    let diagnostics = check_all_multi_file_with_global_index(
        &[
            (
                "extendingClassFromAliasAndUsageInIndexer_backbone.ts",
                r#"
export class Model {
    public someData: string;
}
"#,
            ),
            (
                "extendingClassFromAliasAndUsageInIndexer_moduleA.ts",
                r#"
import Backbone = require("./extendingClassFromAliasAndUsageInIndexer_backbone");
export class VisualizationModel extends Backbone.Model {}
"#,
            ),
            (
                "extendingClassFromAliasAndUsageInIndexer_moduleB.ts",
                r#"
import Backbone = require("./extendingClassFromAliasAndUsageInIndexer_backbone");
export class VisualizationModel extends Backbone.Model {}
"#,
            ),
            (
                "extendingClassFromAliasAndUsageInIndexer_main.ts",
                r#"
import Backbone = require("./extendingClassFromAliasAndUsageInIndexer_backbone");
import moduleA = require("./extendingClassFromAliasAndUsageInIndexer_moduleA");
import moduleB = require("./extendingClassFromAliasAndUsageInIndexer_moduleB");

interface IHasVisualizationModel {
    VisualizationModel: typeof Backbone.Model;
}

var moduleATyped: IHasVisualizationModel = moduleA;
var moduleMap: { [key: string]: IHasVisualizationModel } = {
    "first": moduleA,
    "second": moduleB,
};
var moduleName: string;
var visModel = new moduleMap[moduleName].VisualizationModel();
"#,
            ),
        ],
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            ..CheckerOptions::default()
        },
    );

    let assignability_errors: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        assignability_errors.is_empty(),
        "source class instance publication must not make a CommonJS module namespace expose the instance side: {assignability_errors:?}"
    );
}

#[test]
fn cross_file_class_instance_with_params_uses_published_requester_cache_first() {
    with_two_file_state(
        &[
            (
                "model.ts",
                r#"
export class BoxThing<Value> {
    readonly value!: Value;
}
"#,
            ),
            (
                "entry.ts",
                r#"
import { BoxThing } from "./model";
let item: BoxThing<number> | undefined;
"#,
            ),
        ],
        "entry.ts",
        |checker, entry_idx| {
            let sym_id = checker
                .resolve_cross_file_export_from_file("./model", "BoxThing", Some(entry_idx))
                .expect("exported generic class symbol should resolve");
            let def_id = checker
                .ctx
                .get_or_create_def_id_for_symbol_name(sym_id, "BoxThing");
            let param = TypeParamInfo::simple(checker.ctx.types.intern_string("Value"));
            checker.ctx.insert_def_type_params(def_id, vec![param]);
            checker
                .ctx
                .symbol_instance_types
                .insert(sym_id, TypeId::STRING);

            let (instance_type, params) = checker
                .class_instance_type_with_params_from_symbol(sym_id)
                .expect("published requester class instance should be reused");

            assert_eq!(
                instance_type,
                TypeId::STRING,
                "requester symbol-instance cache should win before cross-file delegation"
            );
            assert_eq!(params, vec![param]);
        },
    );
}

#[test]
fn in_progress_cross_file_class_instance_returns_lazy_placeholder() {
    with_two_file_state(
        &[
            (
                "model.ts",
                r#"
export class BoxThing {
    make(): BoxThing {
        return new BoxThing();
    }
}
"#,
            ),
            (
                "entry.ts",
                r#"
import { BoxThing } from "./model";
let item: BoxThing | undefined;
"#,
            ),
        ],
        "entry.ts",
        |checker, entry_idx| {
            let sym_id = checker
                .resolve_cross_file_export_from_file("./model", "BoxThing", Some(entry_idx))
                .expect("exported class symbol should resolve");
            let def_id = checker
                .ctx
                .get_or_create_def_id_for_symbol_name(sym_id, "BoxThing");
            checker.ctx.symbol_instance_types.remove(&sym_id);
            checker.ctx.class_instance_resolution_set.insert(sym_id);

            let (instance_type, params) = checker
                .class_instance_type_with_params_from_symbol(sym_id)
                .expect("in-progress class should return a lazy placeholder");

            assert!(params.is_empty());
            assert_eq!(
                crate::query_boundaries::common::lazy_def_id(checker.ctx.types, instance_type),
                Some(def_id),
                "in-progress cross-file class read should not delegate recursively"
            );
        },
    );
}

#[test]
fn delegated_cross_file_class_instance_preserves_active_target_guard_after_collision_cleanup() {
    with_two_file_state(
        &[
            (
                "model.ts",
                r#"
export class BoxThing {
    make(): BoxThing {
        return new BoxThing();
    }
}
"#,
            ),
            (
                "entry.ts",
                r#"
import { BoxThing } from "./model";
class LocalThing {}
let item: BoxThing | undefined;
"#,
            ),
        ],
        "entry.ts",
        |checker, entry_idx| {
            let sym_id = checker
                .resolve_cross_file_export_from_file("./model", "BoxThing", Some(entry_idx))
                .expect("exported class symbol should resolve");
            let def_id = checker
                .ctx
                .get_or_create_def_id_for_symbol_name(sym_id, "BoxThing");
            checker.ctx.symbol_instance_types.remove(&sym_id);
            checker.ctx.class_instance_resolution_set.insert(sym_id);

            let (instance_type, params) = checker
                .delegate_cross_arena_class_instance_type(sym_id)
                .expect("cross-file delegation should preserve active target guard");

            assert!(params.is_empty());
            assert_eq!(
                crate::query_boundaries::common::lazy_def_id(checker.ctx.types, instance_type),
                Some(def_id),
                "delegation cleanup should not drop the active class guard for the target symbol"
            );
        },
    );
}
