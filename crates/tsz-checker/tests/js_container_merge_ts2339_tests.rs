use std::sync::Arc;

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_entry(
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
    checker.check_source_file(roots[entry_idx]);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn merged_checked_js_global_uses_non_js_type_for_ts2339() {
    let diagnostics = check_entry(
        &[
            (
                "a.js",
                r#"
var x = function foo() {
}
x.a = function bar() {
}
"#,
            ),
            (
                "b.ts",
                r#"
var x = function () {
    return 1;
}();
"#,
            ),
        ],
        "a.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            no_lib: true,
            ..Default::default()
        },
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();

    assert_eq!(
        ts2339.len(),
        1,
        "Expected exactly one TS2339 for the merged JS/TS global. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2339[0]
            .1
            .contains("Property 'a' does not exist on type 'number'."),
        "Expected the checked-JS write error to use the merged TS declaration type. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn checked_js_define_property_call_still_reports_missing_class_member() {
    let diagnostics = check_entry(
        &[
            (
                "helper.d.ts",
                r#"
type PropertyKey = string | number | symbol;
interface ThisType<T> {}
interface PropertyDescriptor {
    configurable?: boolean;
    enumerable?: boolean;
    value?: any;
    writable?: boolean;
    get?(): any;
    set?(v: any): void;
}
declare const helper: {
    defineProperty<T>(o: T, p: PropertyKey, attributes: PropertyDescriptor & ThisType<any>): T;
};
"#,
            ),
            (
                "a.js",
                r#"
class C {
    constructor() {
        helper.defineProperty(this, "_prop", { value: {} });
        helper.defineProperty(this._prop, "num", { value: 12 });
    }
}
"#,
            ),
        ],
        "a.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            no_lib: true,
            ..Default::default()
        },
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();

    assert_eq!(
        ts2339.len(),
        1,
        "Expected exactly one TS2339 for the second defineProperty target access. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2339[0]
            .1
            .contains("Property '_prop' does not exist on type 'C'."),
        "Expected the checked-JS defineProperty call to keep reporting the missing class member. Actual diagnostics: {diagnostics:#?}"
    );
}
