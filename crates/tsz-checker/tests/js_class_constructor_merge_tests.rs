use std::sync::Arc;

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_common::common::ScriptTarget;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

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
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        no_lib: true,
        strict: true,
        target: ScriptTarget::ES2015,
        ..Default::default()
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
            checker.check_source_file(roots[file_idx]);

            (
                file_name.clone(),
                checker.ctx.diagnostics.iter().map(|d| d.code).collect(),
            )
        })
        .collect()
}

#[test]
fn checked_js_constructor_var_merges_with_class_without_false_duplicates_or_new_errors() {
    let diagnostics = check_project(&[
        (
            "file1.js",
            r#"
var SomeClass = function () {
    this.otherProp = 0;
};

new SomeClass();
"#,
        ),
        (
            "file2.js",
            r#"
class SomeClass { }
SomeClass.prop = 0;
"#,
        ),
    ]);

    let mut offenders = Vec::new();
    for (file_name, codes) in &diagnostics {
        for &code in codes {
            if code == 2300 || code == 7009 {
                offenders.push((file_name.clone(), code));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "Expected no TS2300/TS7009 for checked-JS constructor/class merge, got: {diagnostics:#?}"
    );
}

/// Regression test: when a constructor function in file1.js merges with a class
/// in file2.js, accessing static properties on the class (e.g. `SomeClass.prop = 0`)
/// should NOT produce TS18046 ("'SomeClass' is of type 'unknown'").
///
/// Root cause: `compute_class_symbol_type` only searched the current file's arena
/// for the class declaration. When the CLASS declaration was in a different file's
/// arena, the function returned TypeId::UNKNOWN, triggering false TS18046 errors
/// on any property access or constructor call on the class.
#[test]
fn cross_file_class_merge_no_false_ts18046() {
    let diagnostics = check_project(&[
        (
            "file1.js",
            r#"
var SomeClass = function () {
    this.otherProp = 0;
};

new SomeClass();
"#,
        ),
        (
            "file2.js",
            r#"
class SomeClass { }
SomeClass.prop = 0;
"#,
        ),
    ]);

    let mut ts18046_errors = Vec::new();
    for (file_name, codes) in &diagnostics {
        for &code in codes {
            if code == 18046 {
                ts18046_errors.push(file_name.clone());
            }
        }
    }

    assert!(
        ts18046_errors.is_empty(),
        "Expected no TS18046 for cross-file class/constructor merge, but got TS18046 in: {ts18046_errors:?}\nAll diagnostics: {diagnostics:#?}"
    );
}
