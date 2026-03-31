use std::path::Path;
use std::sync::Arc;

use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_binder::state::LibContext as BinderLibContext;
use tsz_checker::context::CheckerOptions;
use tsz_checker::context::LibContext;
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

fn load_lib_files_for_test() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let search_roots: Vec<&Path> = {
        let mut roots = vec![manifest_dir];
        let mut parent = manifest_dir.parent();
        while let Some(dir) = parent {
            roots.push(dir);
            parent = dir.parent();
        }
        roots
    };
    let candidates = [
        (
            "lib.es5.d.ts",
            ["crates/tsz-core/src/lib-assets-stripped/es5.d.ts"],
        ),
        (
            "lib.es2015.d.ts",
            ["crates/tsz-core/src/lib-assets-stripped/es2015.d.ts"],
        ),
        (
            "lib.es2015.symbol.d.ts",
            ["crates/tsz-core/src/lib-assets-stripped/es2015.symbol.d.ts"],
        ),
    ];

    let mut lib_files = Vec::new();
    for (file_name, suffixes) in candidates {
        let maybe_path = search_roots
            .iter()
            .flat_map(|root| suffixes.iter().map(move |suffix| root.join(suffix)))
            .find(|path| path.exists());
        if let Some(path) = maybe_path
            && let Ok(content) = std::fs::read_to_string(&path)
        {
            lib_files.push(Arc::new(LibFile::from_source(
                file_name.to_string(),
                content,
            )));
        }
    }
    lib_files
}

fn check_entry_with_libs(
    files: &[(&str, &str)],
    entry_file: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let lib_files = load_lib_files_for_test();
    let raw_lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| BinderLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    let checker_lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();

    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in files {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        if !raw_lib_contexts.is_empty() {
            binder.merge_lib_contexts_into_binder(&raw_lib_contexts);
        }
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
    if !checker_lib_contexts.is_empty() {
        checker.ctx.set_lib_contexts(checker_lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }
    checker.check_source_file(roots[entry_idx]);

    checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
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

#[test]
fn checked_js_const_merged_with_ambient_class_reports_ts2739_not_ts2451() {
    let diagnostics = check_entry_with_libs(
        &[
            (
                "a.d.ts",
                r#"
declare class A {
    static d: number;
}
"#,
            ),
            ("b.js", r#"const A = {};"#),
        ],
        "b.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            no_lib: true,
            ..Default::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2451),
        "Did not expect TS2451 for checked-JS const merged with ambient class. Actual diagnostics: {diagnostics:#?}"
    );

    let ts2739: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2739)
        .collect();
    assert_eq!(
        ts2739.len(),
        1,
        "Expected exactly one TS2739 for the merged JS/class constructor-side value. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2739[0]
            .1
            .contains("Type '{}' is missing the following properties from type 'typeof A'")
            && ts2739[0].1.contains("prototype")
            && ts2739[0].1.contains("d"),
        "Expected the checked-JS const initializer to be checked against the ambient class value shape. Actual diagnostics: {diagnostics:#?}"
    );
}
