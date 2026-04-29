use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::module_resolution::build_module_resolution_maps;
use tsz_checker::state::CheckerState;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn compile_named_files(
    files: &[(&str, &str)],
    entry_file: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
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
        .expect("entry file should exist");
    let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);

    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &types,
        file_names[entry_idx].clone(),
        options,
    );

    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    checker.ctx.set_lib_contexts(Vec::new());
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(roots[entry_idx]);

    checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn has_error(diagnostics: &[(u32, String)], code: u32) -> bool {
    diagnostics.iter().any(|(c, _)| *c == code)
}

fn diagnostic_message(diagnostics: &[(u32, String)], code: u32) -> Option<&str> {
    diagnostics
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, message)| message.as_str())
}

#[test]
fn checked_js_parameter_does_not_report_ts7006_without_no_implicit_any() {
    let diagnostics = compile_named_files(
        &[(
            "index.js",
            r#"
function f(x) {
  return x;
}
            "#,
        )],
        "index.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            no_implicit_any: false,
            strict: false,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 7006),
        "Did not expect TS7006 in checked JS when noImplicitAny is disabled. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn checked_js_default_type_import_reports_ts18042() {
    let diagnostics = compile_named_files(
        &[
            (
                "dep.d.ts",
                r#"
export default interface TruffleContract {
  foo: number;
}
                "#,
            ),
            (
                "caller.js",
                r#"
import TruffleContract from "./dep";
console.log(typeof TruffleContract);
                "#,
            ),
        ],
        "caller.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ES2015,
            module: ModuleKind::ES2020,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 18042),
        "Expected TS18042 for default import of a type-only default export in checked JS. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn checked_js_elided_default_namespace_import_reports_duplicate_and_value_errors() {
    let files = [
        (
            "node_modules/@truffle/contract/index.d.ts",
            r#"
declare module "@truffle/contract" {
    interface ContractObject {
        foo: number;
    }
    namespace TruffleContract {
        export type Contract = ContractObject;
    }
    export default TruffleContract;
}
                "#,
        ),
        (
            "caller.js",
            r#"
import TruffleContract from "@truffle/contract";
console.log(typeof TruffleContract, TruffleContract);
                "#,
        ),
    ];

    let declaration_diagnostics = compile_named_files(
        &files,
        "node_modules/@truffle/contract/index.d.ts",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ES2015,
            module: ModuleKind::ES2020,
            ..CheckerOptions::default()
        },
    );
    let declaration_codes: Vec<u32> = declaration_diagnostics
        .iter()
        .map(|(code, _)| *code)
        .collect();
    assert!(
        declaration_codes.contains(&2300),
        "Expected TS2300 for default export colliding with namespace. Actual diagnostics: {declaration_diagnostics:#?}"
    );

    let caller_diagnostics = compile_named_files(
        &files,
        "caller.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ES2015,
            module: ModuleKind::ES2020,
            ..CheckerOptions::default()
        },
    );

    let caller_codes: Vec<u32> = caller_diagnostics.iter().map(|(code, _)| *code).collect();
    assert!(
        caller_codes.iter().filter(|&&code| code == 2708).count() >= 2,
        "Expected TS2708 for both value uses of the elided namespace import. Actual diagnostics: {caller_diagnostics:#?}"
    );
}

#[test]
fn checked_js_type_import_and_type_export_report_ts18042_ts18043() {
    let diagnostics = compile_named_files(
        &[
            (
                "mod.d.ts",
                r#"
export interface WriteFileOptions {}
export function writeFile(path: string, data: any, options: WriteFileOptions, callback: (err: Error) => void): void;
                "#,
            ),
            (
                "index.js",
                r#"
import { writeFile, WriteFileOptions, WriteFileOptions as OtherName } from "./mod";

/** @typedef {{ x: any }} JSDocType */

export { JSDocType };
export { JSDocType as ThisIsFine };
export { WriteFileOptions };
                "#,
            ),
        ],
        "index.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ES2015,
            module: ModuleKind::ES2020,
            ..CheckerOptions::default()
        },
    );

    let ts18042_count = diagnostics
        .iter()
        .filter(|(code, _)| *code == 18042)
        .count();
    let ts18043_count = diagnostics
        .iter()
        .filter(|(code, _)| *code == 18043)
        .count();

    assert_eq!(
        ts18042_count, 2,
        "Expected two TS18042 diagnostics for type-only imports. Actual diagnostics: {diagnostics:#?}"
    );
    assert_eq!(
        ts18043_count, 3,
        "Expected three TS18043 diagnostics for type-only exports in JS. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn checked_js_jsdoc_namespace_import_reports_ts18042() {
    let diagnostics = compile_named_files(
        &[
            (
                "file.js",
                r#"
/**
 * @namespace myTypes
 * @global
 * @type {Object<string,*>}
 */
const myTypes = {};

/** @typedef {string|RegExp|Array<string|RegExp>} myTypes.typeA */
/**
 * @typedef myTypes.typeB
 * @property {myTypes.typeA} prop1
 * @property {string} prop2
 */
/** @typedef {myTypes.typeB|Function} myTypes.typeC */

export { myTypes };
                "#,
            ),
            (
                "file2.js",
                r#"
import { myTypes } from "./file.js";
export { myTypes };
                "#,
            ),
        ],
        "file2.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 18042),
        "Expected TS18042 for importing JSDoc namespace alias in checked JS. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn checked_js_declaration_emit_private_name_from_module_reports_ts9006() {
    let diagnostics = compile_named_files(
        &[
            (
                "some-mod.d.ts",
                r#"
interface Item {
  x: string;
}
declare function getItems(): Item[];
export = getItems;
                "#,
            ),
            (
                "index.js",
                r#"
const items = require("./some-mod")();
module.exports = items;
                "#,
            ),
        ],
        "index.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            emit_declarations: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 9006),
        "Expected TS9006 for declaration emit requiring a private type name from another module. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostic_message(&diagnostics, 9006)
            .is_some_and(|message| message.contains("Item") && message.contains("\"some-mod\"")),
        "Expected TS9006 message to include private name and module. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn esm_file_with_module_exports_does_not_emit_ts9006() {
    let diagnostics = compile_named_files(
        &[
            (
                "cls.js",
                r#"
export class Foo {}
                "#,
            ),
            (
                "bin.js",
                r#"
import * as ns from "./cls";
module.exports = ns;
                "#,
            ),
        ],
        "bin.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            emit_declarations: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 9006),
        "ESM file with module.exports should NOT emit TS9006. Actual diagnostics: {diagnostics:#?}"
    );
}

/// Regression for `jsDeclarationsTypeReassignmentFromDeclaration.ts`: when a
/// JSDoc `@type {typeof import("/some-mod")}` references an unresolvable
/// module specifier (here an absolute path `/some-mod` that tsc rejects),
/// tsc emits only TS2307. The follow-on TS9006 about `Item` being a private
/// name from `"some-mod"` would be misleading because the module never
/// resolved to begin with — `Item` cannot become "private" from a module
/// the program cannot find.
#[test]
fn checked_js_jsdoc_type_with_unresolvable_module_does_not_emit_ts9006() {
    let diagnostics = compile_named_files(
        &[
            (
                "/some-mod.d.ts",
                r#"
interface Item {
    x: string;
}
declare const items: Item[];
export = items;
                "#,
            ),
            (
                "index.js",
                r#"
/** @type {typeof import("/some-mod")} */
const items = [];
module.exports = items;
                "#,
            ),
        ],
        "index.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            emit_declarations: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 9006),
        "TS9006 must not be emitted when the JSDoc `typeof import(...)` module specifier is unresolvable (TS2307 already covers the failure). Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn checked_js_optional_nested_jsdoc_param_flows_into_destructured_binding() {
    let diagnostics = compile_named_files(
        &[(
            "index.js",
            r#"
/**
 * @param {object} opts
 * @param {string} [opts.x]
 */
function f({ x }) {
  /** @type {string} */
  const mustBeString = x;
  return mustBeString;
}
            "#,
        )],
        "index.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: false,
            no_implicit_any: false,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 7031),
        "Expected no TS7031 for destructured binding with JSDoc param docs. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 because optional [opts.x] should flow as string | undefined into x. Actual diagnostics: {diagnostics:#?}"
    );
}
