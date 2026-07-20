use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_common::common::ScriptTarget;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

fn check_project(files: &[(&str, &str)]) -> Vec<(String, Vec<u32>)> {
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

    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let mut resolved_module_paths = FxHashMap::default();
    let mut resolved_modules = FxHashSet::default();
    for source_idx in 0..file_names.len() {
        for (target_idx, target_name) in file_names.iter().enumerate() {
            if source_idx == target_idx {
                continue;
            }
            let specifier = format!("./{target_name}");
            resolved_module_paths.insert((source_idx, specifier.clone()), target_idx);
            resolved_modules.insert(specifier);
        }
    }
    let resolved_module_paths = Arc::new(resolved_module_paths);
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        no_lib: true,
        strict: true,
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    };
    let types = TypeInterner::new();

    file_names
        .iter()
        .enumerate()
        .map(|(file_idx, file_name)| {
            let mut checker = CheckerState::new(
                all_arenas[file_idx].as_ref(),
                all_binders[file_idx].as_ref(),
                &types,
                file_name.clone(),
                options.clone(),
            );
            checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
            checker.ctx.set_all_binders(Arc::clone(&all_binders));
            checker.ctx.set_current_file_idx(file_idx);
            checker.ctx.set_lib_contexts(Vec::new());
            checker
                .ctx
                .set_resolved_module_paths(Arc::clone(&resolved_module_paths));
            checker.ctx.set_resolved_modules(resolved_modules.clone());
            checker.check_source_file(roots[file_idx]);
            (
                file_name.clone(),
                checker
                    .ctx
                    .diagnostics
                    .iter()
                    .filter(|diagnostic| diagnostic.code != 2318)
                    .map(|diagnostic| diagnostic.code)
                    .collect(),
            )
        })
        .collect()
}

const PAIR_JS: &str = r#"
/**
 * @template T, U
 * @param {T} left
 * @param {U} right
 * @returns {[T, U]}
 */
export function pair(left, right) { return [left, right]; }
"#;

#[test]
fn implicit_call_keeps_sibling_jsdoc_type_parameters_distinct() {
    let source = format!(
        r#"{PAIR_JS}
const value = pair(1, "x");
/** @type {{number}} */ const first = value[0];
/** @type {{string}} */ const second = value[1];
/** @type {{[number, string]}} */ const exact = value;
"#
    );
    let diagnostics = check_project(&[("jsdoc_multi.js", &source)]);
    assert_eq!(diagnostics, vec![("jsdoc_multi.js".to_string(), vec![])]);
}

#[test]
fn explicit_type_arguments_keep_sibling_jsdoc_type_parameters_distinct() {
    let diagnostics = check_project(&[
        ("jsdoc_multi.js", PAIR_JS),
        (
            "jsdoc_multi_use.ts",
            r#"
import { pair } from "./jsdoc_multi.js";
export const value = pair<number, string>(1, "x");
const exact: [number, string] = value;
"#,
        ),
    ]);
    assert_eq!(
        diagnostics,
        vec![
            ("jsdoc_multi.js".to_string(), vec![]),
            ("jsdoc_multi_use.ts".to_string(), vec![]),
        ]
    );
}

#[test]
fn sibling_jsdoc_type_parameters_remain_distinct_in_assignment_and_return_relations() {
    let diagnostics = check_project(&[(
        "jsdoc_sibling_assign.js",
        r#"
/**
 * @template T, U
 * @param {T} left
 * @param {U} right
 * @returns {T}
 */
export function wrong(left, right) {
    left = right;
    return right;
}
"#,
    )]);
    assert_eq!(
        diagnostics,
        vec![("jsdoc_sibling_assign.js".to_string(), vec![2322, 2322])]
    );
}
