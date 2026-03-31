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
